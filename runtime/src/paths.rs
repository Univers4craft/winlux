//! Correspondance entre chemins Windows et chemins Linux.
//!
//! Lecteurs :
//! - `C:\` correspond au dossier personnel (modifiable avec WINLUX_DRIVE_C) ;
//! - `Z:\` correspond à la racine `/` du système Linux ;
//! - les autres lettres viennent du registre des lecteurs (`mounts.rs`) ou de
//!   `WINLUX_DRIVE_<lettre>`. Une lettre inconnue ne mène nulle part (comme
//!   sous Windows) au lieu de retomber sur C:.
//!
//! Profil utilisateur : `C:\Users\<nom>` est le dossier personnel Linux, et
//! ses dossiers connus suivent les dossiers XDG de l'utilisateur
//! (`C:\Users\<nom>\Documents` → `~/Documents`, `…\Desktop` → `~/Bureau` sur
//! un système français, etc.). `C:\Users\Public` suit le dossier public XDG.
//! Dans l'autre sens, un fichier de `~/Bureau` est présenté aux applis comme
//! `C:\Users\<nom>\Desktop\…`.
//!
//! Règles Win32 appliquées aux chemins (sauf préfixe `\\?\`, laissé tel quel) :
//! `/` et `\` sont équivalents, les séparateurs répétés et `.` disparaissent,
//! `..` est résolu sur le texte (pas en suivant les liens), les points et
//! espaces en fin de composant sont retirés, `NUL`, `CONIN$` et `CONOUT$`
//! désignent des périphériques.
//!
//! Noms impossibles sous Windows : un fichier Linux nommé `réunion 10:30` ou
//! `notes.` ne peut pas s'écrire tel quel côté Windows. Comme Cygwin, chaque
//! caractère interdit (`" * : < > ? \ |`, caractères de contrôle, point ou
//! espace final) est présenté dans la zone privée Unicode (U+F000 + code) et
//! retraduit à l'aller : l'appli ouvre, renomme et supprime ces fichiers.
//!
//! Windows ignore la casse des noms de fichiers : si le chemin exact
//! n'existe pas, on cherche chaque composant sans tenir compte de la casse
//! (correspondance exacte d'abord, puis la première par ordre alphabétique,
//! pour que le résultat ne dépende pas de l'ordre du disque).

use std::path::{Component, Path, PathBuf};

/// Destination des lecteurs et partages inconnus : procfs refuse toute
/// création, l'appli reçoit donc « chemin introuvable » comme sous Windows.
const ABSENT: &str = "/proc/winlux-absent";

pub fn data_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("XDG_DATA_HOME") {
        return Some(PathBuf::from(d).join("winlux"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/winlux"))
}

pub fn drive_c() -> PathBuf {
    std::env::var_os("WINLUX_DRIVE_C")
        .filter(|c| !c.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(home)
}

/// Dossier personnel Linux (`C:\Users\<nom>`) ; `WINLUX_HOME` le remplace
/// (tests, profils itinérants).
pub fn home() -> PathBuf {
    std::env::var_os("WINLUX_HOME")
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var_os("HOME"))
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .or_else(|| passwd_entry().map(|(_, dir)| dir))
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Nom de l'utilisateur tel que le voient les applis Windows.
pub fn user_name() -> String {
    for var in ["USER", "LOGNAME"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                return v;
            }
        }
    }
    passwd_entry()
        .map(|(name, _)| name)
        .unwrap_or_else(|| "user".into())
}

fn passwd_entry() -> Option<(String, PathBuf)> {
    // SAFETY: getpwuid_r écrit dans `pw` et `buf`, deux tampons locaux.
    unsafe {
        let mut pw: libc::passwd = std::mem::zeroed();
        let mut out: *mut libc::passwd = std::ptr::null_mut();
        let mut buf = vec![0 as libc::c_char; 4096];
        if libc::getpwuid_r(
            libc::geteuid(),
            &mut pw,
            buf.as_mut_ptr(),
            buf.len(),
            &mut out,
        ) != 0
            || out.is_null()
        {
            return None;
        }
        let name = std::ffi::CStr::from_ptr(pw.pw_name)
            .to_string_lossy()
            .into_owned();
        let dir = std::ffi::CStr::from_ptr(pw.pw_dir)
            .to_string_lossy()
            .into_owned();
        Some((name, PathBuf::from(dir)))
    }
}

/// `C:\Users\<nom>`.
pub fn profile_windows() -> String {
    format!("C:\\Users\\{}", win_name(&user_name()))
}

// ---------- Dossiers connus (XDG) ----------

/// Dossiers connus du profil Windows : (nom Windows, clé XDG, dossier par défaut).
const KNOWN: [(&str, &str, &str); 6] = [
    ("Desktop", "XDG_DESKTOP_DIR", "Desktop"),
    ("Documents", "XDG_DOCUMENTS_DIR", "Documents"),
    ("Downloads", "XDG_DOWNLOAD_DIR", "Downloads"),
    ("Music", "XDG_MUSIC_DIR", "Music"),
    ("Pictures", "XDG_PICTURES_DIR", "Pictures"),
    ("Videos", "XDG_VIDEOS_DIR", "Videos"),
];

/// Anciens noms (jonctions de compatibilité de Windows).
const KNOWN_ALIASES: [(&str, &str); 4] = [
    ("My Documents", "Documents"),
    ("My Music", "Music"),
    ("My Pictures", "Pictures"),
    ("My Videos", "Videos"),
];

fn user_dirs_text() -> String {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|c| !c.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"));
    std::fs::read_to_string(config.join("user-dirs.dirs")).unwrap_or_default()
}

/// Valeur d'une clé de `~/.config/user-dirs.dirs` (`$HOME` développé).
fn xdg_user_dir(key: &str) -> Option<PathBuf> {
    parse_user_dirs(&user_dirs_text(), key, &home())
}

/// Tous les dossiers connus : (nom Windows, dossier Linux), en une lecture.
fn known_folders() -> Vec<(&'static str, PathBuf)> {
    let text = user_dirs_text();
    let home = home();
    KNOWN
        .iter()
        .map(|(name, key, default)| {
            (
                *name,
                parse_user_dirs(&text, key, &home).unwrap_or_else(|| home.join(default)),
            )
        })
        .collect()
}

fn parse_user_dirs(text: &str, key: &str, home: &Path) -> Option<PathBuf> {
    let line = text.lines().map(str::trim).find(|l| {
        l.strip_prefix(key)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    })?;
    let value = line.split_once('=')?.1.trim().trim_matches('"');
    let path = if let Some(rest) = value.strip_prefix("$HOME") {
        home.join(rest.trim_start_matches('/'))
    } else if value.starts_with('/') {
        PathBuf::from(value)
    } else {
        return None;
    };
    // La spécification XDG : un dossier égal au dossier personnel est désactivé.
    (path != home).then_some(path)
}

/// Dossier Linux d'un dossier connu (`"Documents"` → `~/Documents`).
pub fn known_folder(windows_name: &str) -> Option<PathBuf> {
    let name = KNOWN_ALIASES
        .iter()
        .find(|(alias, _)| alias.eq_ignore_ascii_case(windows_name))
        .map_or(windows_name, |(_, real)| real);
    let (_, key, default) = KNOWN
        .iter()
        .find(|(n, _, _)| n.eq_ignore_ascii_case(name))?;
    Some(xdg_user_dir(key).unwrap_or_else(|| home().join(default)))
}

/// Dossier public partagé (`C:\Users\Public`).
pub fn public_folder() -> PathBuf {
    xdg_user_dir("XDG_PUBLICSHARE_DIR").unwrap_or_else(|| home().join("Public"))
}

/// Chemin Windows d'un dossier du profil (`"AppData\\Roaming"`, `"Documents"`…).
pub fn profile_path(rel: &str) -> String {
    if rel.is_empty() {
        profile_windows()
    } else {
        format!("{}\\{}", profile_windows(), rel)
    }
}

// ---------- Noms de fichiers ----------

/// Caractère qu'un nom de fichier Windows ne peut pas contenir.
fn forbidden(c: char) -> bool {
    matches!(
        c,
        '\u{1}'..='\u{1f}' | '"' | '*' | ':' | '<' | '>' | '?' | '\\' | '|'
    )
}

/// Nom Linux → nom présentable à une appli Windows (sans perte).
pub fn win_name(linux: &str) -> String {
    if linux == "." || linux == ".." {
        return linux.to_string();
    }
    let chars: Vec<char> = linux.chars().collect();
    // Les points et espaces finaux sont retirés par Windows : on les protège.
    let keep = chars
        .iter()
        .rposition(|&c| c != '.' && c != ' ')
        .map_or(0, |i| i + 1);
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            if forbidden(c) || i >= keep {
                char::from_u32(0xF000 + c as u32).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

/// Nom vu par une appli Windows → nom Linux (inverse de `win_name`).
pub fn linux_name(win: &str) -> String {
    win.chars()
        .map(|c| {
            let n = c as u32;
            if (0xF001..=0xF07F).contains(&n) {
                let orig = char::from_u32(n - 0xF000).unwrap_or(c);
                if forbidden(orig) || orig == '.' || orig == ' ' {
                    return orig;
                }
            }
            c
        })
        .collect()
}

// ---------- Analyse d'un chemin Windows ----------

#[derive(Clone, Debug, PartialEq)]
enum Root {
    /// `C:\…`
    Drive(char),
    /// `C:…` (relatif au dossier courant de ce lecteur).
    DriveRelative(char),
    /// `\…` (racine du lecteur courant).
    CurrentDrive,
    /// `\\serveur\partage\…`
    Unc(String, String),
    /// Périphérique (`NUL`, `CONOUT$`…).
    Device(&'static str),
    /// Relatif au dossier courant.
    Relative,
}

#[derive(Clone, Debug, PartialEq)]
struct WinPath {
    root: Root,
    /// Composants au format Windows (pas encore retraduits en noms Linux).
    comps: Vec<String>,
    /// Préfixe `\\?\` : aucune normalisation.
    verbatim: bool,
    trailing_sep: bool,
}

fn device(name: &str) -> Option<&'static str> {
    let n = name.trim_end_matches([' ', '.', ':']);
    [
        ("NUL", "/dev/null"),
        ("CONIN$", "/dev/stdin"),
        ("CONOUT$", "/dev/stdout"),
    ]
    .iter()
    .find(|(dev, _)| dev.eq_ignore_ascii_case(n))
    .map(|(_, linux)| *linux)
}

fn split_comps(rest: &str, verbatim: bool, keep_leading_dotdot: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let parts: Box<dyn Iterator<Item = &str>> = if verbatim {
        Box::new(rest.split('\\'))
    } else {
        Box::new(rest.split(['\\', '/']))
    };
    for part in parts {
        if part.is_empty() {
            continue;
        }
        if verbatim {
            out.push(part.to_string());
            continue;
        }
        match part {
            "." => {}
            ".." => {
                if out.last().is_some_and(|l| l != "..") {
                    out.pop();
                } else if keep_leading_dotdot {
                    out.push("..".into());
                }
            }
            p => {
                let t = p.trim_end_matches(['.', ' ']);
                if !t.is_empty() {
                    out.push(t.to_string());
                }
            }
        }
    }
    out
}

fn parse(w: &str) -> WinPath {
    let w = w.trim_end_matches('\0');
    let trailing_sep = w.ends_with(['\\', '/']);
    let mk = |root, comps, verbatim| WinPath {
        root,
        comps,
        verbatim,
        trailing_sep,
    };
    // \\?\ et \??\ : chemin brut (chemins longs), pas de normalisation.
    for pre in ["\\\\?\\", "\\??\\"] {
        if let Some(r) = w.strip_prefix(pre) {
            if r.len() >= 4 && r[..4].eq_ignore_ascii_case("UNC\\") {
                return unc(&r[4..], true, trailing_sep);
            }
            return match drive_prefix(r) {
                Some(l) => mk(Root::Drive(l), split_comps(&r[2..], true, false), true),
                None => mk(Root::Relative, split_comps(r, true, true), true),
            };
        }
    }
    // \\.\ : espace des périphériques.
    if let Some(r) = w.strip_prefix("\\\\.\\").or_else(|| w.strip_prefix("//./")) {
        if let Some(d) = device(r) {
            return mk(Root::Device(d), Vec::new(), false);
        }
        if let Some(l) = drive_prefix(r) {
            return mk(Root::Drive(l), split_comps(&r[2..], false, false), false);
        }
        // `\\.\pipe\nom` et autres : comportement historique (relatif).
        return mk(Root::Relative, split_comps(r, false, true), false);
    }
    if w.starts_with("\\\\") || w.starts_with("//") {
        return unc(&w[2..], false, trailing_sep);
    }
    if let Some(l) = drive_prefix(w) {
        let rest = &w[2..];
        let comps = split_comps(rest, false, false);
        if let Some(d) = comps.last().and_then(|c| device(c)) {
            return mk(Root::Device(d), Vec::new(), false);
        }
        return if rest.starts_with(['\\', '/']) {
            mk(Root::Drive(l), comps, false)
        } else {
            mk(
                Root::DriveRelative(l),
                split_comps(rest, false, true),
                false,
            )
        };
    }
    // « /usr/bin/sh » : chemin Linux absolu (runtime, scripts, variables
    // d'environnement), donc Z:. Seul « \\x » désigne la racine du lecteur
    // courant, sinon /tmp deviendrait C:\tmp quand le dossier courant est
    // dans le profil.
    if w.starts_with('/') {
        return mk(Root::Drive('Z'), split_comps(w, false, false), false);
    }
    if w.starts_with('\\') {
        return mk(Root::CurrentDrive, split_comps(w, false, false), false);
    }
    let comps = split_comps(w, false, true);
    if let Some(d) = comps.last().and_then(|c| device(c)) {
        return mk(Root::Device(d), Vec::new(), false);
    }
    mk(Root::Relative, comps, false)
}

fn drive_prefix(s: &str) -> Option<char> {
    let b = s.as_bytes();
    (b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic())
        .then(|| b[0].to_ascii_uppercase() as char)
}

fn unc(r: &str, verbatim: bool, trailing_sep: bool) -> WinPath {
    let mut parts = split_comps(r, verbatim, false).into_iter();
    let server = parts.next().unwrap_or_default();
    let share = parts.next().unwrap_or_default();
    WinPath {
        root: Root::Unc(server, share),
        comps: parts.collect(),
        verbatim,
        trailing_sep,
    }
}

/// Ajoute des composants relatifs (pouvant commencer par `..`) à une base.
fn join_comps(base: &mut Vec<String>, rel: &[String]) {
    for c in rel {
        if c == ".." {
            base.pop();
        } else {
            base.push(c.clone());
        }
    }
}

// ---------- Windows → Linux ----------

/// Chemin Windows (absolu ou relatif) → chemin Linux.
pub fn to_linux(w: &str) -> PathBuf {
    guard(to_linux_inner(w, true))
}

/// Dossiers personnels : l'application doit en avoir reçu l'autorisation.
fn guard(p: PathBuf) -> PathBuf {
    if crate::permissions::path_allowed(&p) {
        p
    } else {
        crate::permissions::denied_path(&p)
    }
}

/// Comme `to_linux`, sans traverser les archives ouvertes : le chemin reste
/// `.../photos.zip/dossier` (sert à retrouver l'archive elle-même).
pub fn to_linux_raw(w: &str) -> PathBuf {
    guard(to_linux_inner(w, false))
}

/// Racine Linux d'une lettre de lecteur.
fn drive_root(letter: char) -> PathBuf {
    match letter {
        'C' => drive_c(),
        'Z' => PathBuf::from("/"),
        l => std::env::var_os(format!("WINLUX_DRIVE_{l}"))
            .map(PathBuf::from)
            .or_else(|| crate::mounts::root_of(l))
            .unwrap_or_else(|| Path::new(ABSENT).join(l.to_string())),
    }
}

/// Lettre du lecteur du dossier courant.
fn current_drive() -> char {
    std::env::current_dir()
        .ok()
        .and_then(|d| to_windows(&d).chars().next())
        .unwrap_or('C')
}

/// `C:` + composants → base Linux et composants restants, en appliquant le
/// profil utilisateur (`Users\<nom>`, dossiers connus, `Users\Public`).
fn resolve_drive(letter: char, comps: &[String]) -> (PathBuf, usize) {
    if letter == 'C' && comps.len() >= 2 && comps[0].eq_ignore_ascii_case("Users") {
        let who = linux_name(&comps[1]);
        if who.eq_ignore_ascii_case(&user_name()) {
            if let Some(k) = comps.get(2).and_then(|c| known_folder(c)) {
                return (k, 3);
            }
            return (home(), 2);
        }
        if who.eq_ignore_ascii_case("Public") {
            return (public_folder(), 2);
        }
    }
    (drive_root(letter), 0)
}

/// Partage `\\serveur\partage` : lecteur réseau monté ou `\\localhost\c$`.
fn resolve_unc(server: &str, share: &str, comps: &[String]) -> (PathBuf, usize, Vec<String>) {
    let local = ["localhost", "127.0.0.1", "::1", "."]
        .iter()
        .any(|h| h.eq_ignore_ascii_case(server))
        || hostname().is_some_and(|h| h.eq_ignore_ascii_case(server));
    if local {
        if let Some(l) = share.strip_suffix('$').and_then(drive_prefix_letter) {
            let (base, used) = resolve_drive(l, comps);
            return (base, used, comps.to_vec());
        }
    }
    let mut segs: Vec<String> = vec![share.to_string()];
    segs.extend(comps.iter().cloned());
    for m in crate::mounts::list() {
        if m.kind != "network" {
            continue;
        }
        let Some((host, path)) = source_host_path(&m.source) else {
            continue;
        };
        if !host.eq_ignore_ascii_case(server) {
            continue;
        }
        let msegs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if msegs.is_empty() || msegs.len() > segs.len() {
            continue;
        }
        if msegs
            .iter()
            .zip(&segs)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            let root = crate::mounts::root_of(m.letter).unwrap_or_else(|| m.mountpoint.clone());
            return (root, msegs.len(), segs);
        }
    }
    (Path::new(ABSENT).join("unc").join(server), 0, segs)
}

fn drive_prefix_letter(s: &str) -> Option<char> {
    let mut it = s.chars();
    let c = it.next()?;
    (it.next().is_none() && c.is_ascii_alphabetic()).then(|| c.to_ascii_uppercase())
}

/// `smb://user@hote:445/partage/sous` → (`hote`, `partage/sous`).
fn source_host_path(source: &str) -> Option<(&str, &str)> {
    let rest = source.split_once("://")?.1;
    let rest = rest.rsplit_once('@').map_or(rest, |(_, r)| r);
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority.split(':').next()?;
    Some((host, path))
}

fn hostname() -> Option<String> {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
}

fn to_linux_inner(w: &str, archives: bool) -> PathBuf {
    if w.trim_end_matches('\0').is_empty() {
        return PathBuf::new();
    }
    let wp = parse(w);
    let (base, segs, used): (Option<PathBuf>, Vec<String>, usize) = match &wp.root {
        Root::Device(d) => return PathBuf::from(d),
        Root::Drive(l) => {
            let (b, used) = resolve_drive(*l, &wp.comps);
            (Some(b), wp.comps.clone(), used)
        }
        Root::CurrentDrive => {
            let l = current_drive();
            let (b, used) = resolve_drive(l, &wp.comps);
            (Some(b), wp.comps.clone(), used)
        }
        Root::DriveRelative(l) => {
            if *l == current_drive() {
                (None, wp.comps.clone(), 0)
            } else {
                let (b, used) = resolve_drive(*l, &wp.comps);
                (Some(b), wp.comps.clone(), used)
            }
        }
        Root::Unc(server, share) => {
            let (b, used, segs) = resolve_unc(server, share, &wp.comps);
            (Some(b), segs, used)
        }
        Root::Relative => (None, wp.comps.clone(), 0),
    };
    let mut joined = base.unwrap_or_default();
    for c in &segs[used.min(segs.len())..] {
        if c == ".." {
            joined.push("..");
        } else {
            joined.push(linux_name(c));
        }
    }
    if joined.as_os_str().is_empty() {
        joined = PathBuf::from(".");
    }
    if joined.symlink_metadata().is_ok() {
        return joined;
    }
    // Chemin à l'intérieur d'une archive ouverte (navigation façon dossier).
    if archives {
        if let Some(v) = crate::archive::resolve(&joined) {
            return v;
        }
    }
    fix_case(&joined).unwrap_or(joined)
}

/// Retrouve chaque composant sans tenir compte de la casse.
fn fix_case(p: &Path) -> Option<PathBuf> {
    let mut cur = if p.is_absolute() {
        PathBuf::from("/")
    } else {
        PathBuf::from(".")
    };
    let comps: Vec<_> = p.components().collect();
    for (i, comp) in comps.iter().enumerate() {
        let name = match comp {
            Component::Normal(n) => n.to_string_lossy().into_owned(),
            Component::RootDir => continue,
            other => {
                cur.push(other.as_os_str());
                continue;
            }
        };
        let exact = cur.join(&name);
        if exact.symlink_metadata().is_ok() {
            cur = exact;
            continue;
        }
        let lower = name.to_lowercase();
        let mut found: Vec<std::ffi::OsString> = std::fs::read_dir(&cur)
            .ok()?
            .flatten()
            .map(|e| e.file_name())
            .filter(|n| n.to_string_lossy().to_lowercase() == lower)
            .collect();
        found.sort();
        match found.into_iter().next() {
            Some(n) => cur.push(n),
            // Dernier composant absent (création de fichier) : on garde le nom tel quel.
            None if i == comps.len() - 1 => cur = exact,
            None => return None,
        }
    }
    Some(if p.is_absolute() {
        cur
    } else {
        cur.strip_prefix(".")
            .map(|r| r.to_path_buf())
            .unwrap_or(cur)
    })
}

// ---------- Linux → Windows ----------

/// Chemin absolu sans `.`/`..`, sans suivre les liens symboliques.
fn lexical_absolute(p: &Path) -> PathBuf {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(p)
    };
    let mut out = PathBuf::from("/");
    for c in abs.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(n) => out.push(n),
            _ => {}
        }
    }
    out
}

/// `p` relatif à `base` (ou à sa forme canonique, si `base` passe par un lien).
fn strip_base(p: &Path, base: &Path) -> Option<PathBuf> {
    if let Ok(r) = p.strip_prefix(base) {
        return Some(r.to_path_buf());
    }
    let canon = std::fs::canonicalize(base).ok()?;
    p.strip_prefix(canon).ok().map(Path::to_path_buf)
}

fn win_join(prefix: &str, rest: &Path) -> String {
    let mut s = prefix.trim_end_matches('\\').to_string();
    for c in rest.components() {
        if let Component::Normal(n) = c {
            s.push('\\');
            s.push_str(&win_name(&n.to_string_lossy()));
        }
    }
    if s.len() == 2 && s.ends_with(':') {
        s.push('\\');
    }
    s
}

fn try_to_windows(abs: &Path) -> Option<String> {
    for m in crate::mounts::list() {
        if let Ok(r) = abs.strip_prefix(&m.mountpoint) {
            return Some(win_join(&format!("{}:", m.letter), r));
        }
    }
    let home = home();
    if home != Path::new("/") {
        for (name, dir) in known_folders() {
            if let Some(r) = strip_base(abs, &dir) {
                return Some(win_join(&profile_path(name), &r));
            }
        }
        if let Some(r) = strip_base(abs, &home) {
            if r.starts_with("AppData") {
                return Some(win_join(&profile_windows(), &r));
            }
        }
    }
    if let Some(r) = strip_base(abs, &drive_c()) {
        return Some(win_join("C:", &r));
    }
    if home != Path::new("/") {
        if let Some(r) = strip_base(abs, &home) {
            return Some(win_join(&profile_windows(), &r));
        }
    }
    None
}

/// Chemin Linux → chemin Windows (`C:\...`, `C:\Users\<nom>\...` ou `Z:\...`).
///
/// Les liens symboliques ne sont pas suivis : une appli qui a ouvert
/// `C:\Users\moi\Documents\lien\x.txt` retrouve ce même chemin.
pub fn to_windows(p: &Path) -> String {
    let abs = lexical_absolute(p);
    if let Some(s) = try_to_windows(&abs) {
        return s;
    }
    if let Ok(canon) = std::fs::canonicalize(&abs) {
        if let Some(s) = try_to_windows(&canon) {
            return s;
        }
    }
    win_join("Z:", abs.strip_prefix("/").unwrap_or(&abs))
}

/// Chemin complet Windows, calculé sur le texte comme `GetFullPathName` :
/// rien n'est lu sur le disque et les liens ne sont pas suivis.
pub fn full_windows(name: &str) -> String {
    let wp = parse(name);
    if wp.verbatim {
        return name.trim_end_matches('\0').to_string();
    }
    let cwd = || {
        std::env::current_dir()
            .map(|d| parse(&to_windows(&d)))
            .unwrap_or_else(|_| parse("C:\\"))
    };
    let (prefix, comps) = match wp.root {
        Root::Device(_) => {
            let n = name.trim_end_matches('\0');
            let last = n.rsplit(['\\', '/']).next().unwrap_or(n);
            return format!("\\\\.\\{}", last.trim_end_matches([' ', '.', ':']));
        }
        Root::Drive(l) => (format!("{l}:"), wp.comps),
        Root::Unc(s, sh) => (format!("\\\\{s}\\{sh}"), wp.comps),
        Root::CurrentDrive => {
            let l = match cwd().root {
                Root::Drive(l) => l,
                _ => 'C',
            };
            (format!("{l}:"), wp.comps)
        }
        Root::DriveRelative(l) => {
            let c = cwd();
            if c.root == Root::Drive(l) {
                let mut base = c.comps;
                join_comps(&mut base, &wp.comps);
                (format!("{l}:"), base)
            } else {
                let mut base = Vec::new();
                join_comps(&mut base, &wp.comps);
                (format!("{l}:"), base)
            }
        }
        Root::Relative => {
            let c = cwd();
            let prefix = match &c.root {
                Root::Drive(l) => format!("{l}:"),
                Root::Unc(s, sh) => format!("\\\\{s}\\{sh}"),
                _ => "C:".into(),
            };
            let mut base = c.comps;
            join_comps(&mut base, &wp.comps);
            (prefix, base)
        }
    };
    let mut s = prefix;
    for c in &comps {
        s.push('\\');
        s.push_str(c);
    }
    if comps.is_empty() || wp.trailing_sep {
        s.push('\\');
    }
    s
}

/// Cible d'un lien symbolique créé par une appli : un chemin relatif reste
/// relatif (le lien suit son dossier), un chemin absolu est traduit.
pub fn link_target(w: &str) -> PathBuf {
    let wp = parse(w);
    if wp.root != Root::Relative || wp.verbatim {
        return to_linux_raw(w);
    }
    let mut out = PathBuf::new();
    for c in &wp.comps {
        if c == ".." {
            out.push("..");
        } else {
            out.push(linux_name(c));
        }
    }
    out
}

// ---------- Environnement Windows ----------

/// Variables d'environnement que toute appli Windows trouve, calculées à
/// partir du compte Linux. Une variable déjà définie côté Linux l'emporte.
pub fn windows_env() -> Vec<(String, String)> {
    let profile = profile_windows();
    let host = hostname()
        .map(|h| h.to_uppercase())
        .unwrap_or_else(|| "WINLUX".into());
    vec![
        ("ALLUSERSPROFILE".into(), "C:\\ProgramData".into()),
        ("APPDATA".into(), format!("{profile}\\AppData\\Roaming")),
        (
            "CommonProgramFiles".into(),
            "C:\\Program Files\\Common Files".into(),
        ),
        (
            "CommonProgramFiles(x86)".into(),
            "C:\\Program Files (x86)\\Common Files".into(),
        ),
        ("COMPUTERNAME".into(), host.clone()),
        ("HOMEDRIVE".into(), "C:".into()),
        ("HOMEPATH".into(), profile[2..].to_string()),
        ("LOCALAPPDATA".into(), format!("{profile}\\AppData\\Local")),
        ("PROCESSOR_ARCHITECTURE".into(), "AMD64".into()),
        ("ProgramData".into(), "C:\\ProgramData".into()),
        ("ProgramFiles".into(), "C:\\Program Files".into()),
        ("ProgramFiles(x86)".into(), "C:\\Program Files (x86)".into()),
        ("ProgramW6432".into(), "C:\\Program Files".into()),
        ("PUBLIC".into(), "C:\\Users\\Public".into()),
        ("SystemDrive".into(), "C:".into()),
        ("SystemRoot".into(), "C:\\Windows".into()),
        ("USERDOMAIN".into(), host),
        ("USERNAME".into(), user_name()),
        ("USERPROFILE".into(), profile),
        ("windir".into(), "C:\\Windows".into()),
    ]
}

/// Ajoute à l'environnement du processus les variables Windows absentes
/// (au lancement d'un .exe). L'appli peut ensuite les modifier ou les
/// supprimer comme sous Windows, et ses enfants en héritent.
pub fn install_windows_env() {
    let present: Vec<String> = std::env::vars_os()
        .map(|(k, _)| k.to_string_lossy().to_ascii_uppercase())
        .collect();
    for (k, v) in windows_env() {
        if !present.contains(&k.to_ascii_uppercase()) {
            std::env::set_var(k, v);
        }
    }
}

#[cfg(test)]
mod tests;
