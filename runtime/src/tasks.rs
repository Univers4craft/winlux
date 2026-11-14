//! Planificateur de tâches Winlux : modèle des tâches, service de
//! déclenchement et commandes `winlux --tasks …`.
//!
//! Une tâche est un fichier texte `clé=valeur` (`<id>.task`, en Latin-1 pour
//! que le bureau Win32 le lise et l'écrive directement), rangé :
//! - dans `/etc/winlux/tasks` pour les tâches du système, écrites par root ;
//!   leurs actions tournent en root ;
//! - dans `~/.local/share/winlux/tasks` pour celles qu'un compte enregistre
//!   lui-même (AddTask d'Acrobat, #1824) ; leurs actions tournent sous ce
//!   compte, jamais en root. `/etc/winlux/tasks` reste fermé aux comptes.
//!
//! Le service `winlux --tasks daemon`, lancé par l'init Winlux, surveille
//! l'heure et les événements système et exécute les actions.
//!
//! Fichiers produits pour le bureau :
//! - `/var/lib/winlux/tasks/history.log` : historique d'exécution, une ligne
//!   `époque\theure\tid\tnom\ttype\tdétail` par événement ;
//! - `/var/lib/winlux/tasks/<id>.last` : dernière exécution et son résultat ;
//! - `/run/winlux/tasks/next.txt` : prochaine exécution de chaque tâche ;
//! - `/run/winlux/tasks/running/<id>` : PID d'une tâche en cours.

use crate::admin;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

pub const TASKS_DIR: &str = "etc/winlux/tasks";
/// Tâches d'un compte, relatif à son dossier personnel (#1824).
pub const USER_TASKS_REL: &str = ".local/share/winlux/tasks";
pub const STATE_DIR: &str = "var/lib/winlux/tasks";
pub const RUN_DIR: &str = "run/winlux/tasks";
const HISTORY_MAX: u64 = 256 * 1024;

/// Événements système qu'un déclencheur « sur événement » peut attendre.
#[allow(dead_code)]
pub const EVENTS: [&str; 5] = [
    "network-up",
    "network-down",
    "power-ac",
    "power-battery",
    "usb-added",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// Une seule fois, à une date et une heure locales.
    Once {
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
    },
    /// Tous les `every` jours à heure fixe.
    Daily { hour: u32, minute: u32, every: u32 },
    /// Certains jours de la semaine (bit 0 = dimanche … bit 6 = samedi).
    Weekly { hour: u32, minute: u32, days: u8 },
    /// Toutes les `minutes` minutes.
    Interval { minutes: u32 },
    /// Au démarrage du système.
    Boot,
    /// À l'ouverture de session.
    Logon,
    /// Sur un événement système (voir [`EVENTS`]).
    Event(String),
}

fn hhmm(s: &str) -> Option<(u32, u32)> {
    let (h, m) = s.split_once(':')?;
    let (h, m) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

impl Trigger {
    pub fn parse(s: &str) -> Option<Trigger> {
        let mut words = s.split_whitespace();
        let kind = words.next()?;
        Some(match kind {
            "once" => {
                let date = words.next()?;
                let mut d = date.split('-');
                let year = d.next()?.parse().ok()?;
                let month: u32 = d.next()?.parse().ok()?;
                let day: u32 = d.next()?.parse().ok()?;
                let (hour, minute) = hhmm(words.next()?)?;
                if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
                    return None;
                }
                Trigger::Once {
                    year,
                    month,
                    day,
                    hour,
                    minute,
                }
            }
            "daily" => {
                let (hour, minute) = hhmm(words.next()?)?;
                let every = words.next().and_then(|w| w.parse().ok()).unwrap_or(1);
                Trigger::Daily {
                    hour,
                    minute,
                    every: every.max(1),
                }
            }
            "weekly" => {
                let (hour, minute) = hhmm(words.next()?)?;
                let mut days = 0u8;
                for d in words.next()?.split(',') {
                    let d: u8 = d.parse().ok()?;
                    if d > 6 {
                        return None;
                    }
                    days |= 1 << d;
                }
                if days == 0 {
                    return None;
                }
                Trigger::Weekly { hour, minute, days }
            }
            "interval" => Trigger::Interval {
                minutes: words.next()?.parse::<u32>().ok()?.max(1),
            },
            "boot" => Trigger::Boot,
            "logon" => Trigger::Logon,
            "event" => Trigger::Event(words.next()?.to_string()),
            _ => return None,
        })
    }

    pub fn to_line(&self) -> String {
        match self {
            Trigger::Once {
                year,
                month,
                day,
                hour,
                minute,
            } => format!("once {year:04}-{month:02}-{day:02} {hour:02}:{minute:02}"),
            Trigger::Daily {
                hour,
                minute,
                every,
            } => {
                if *every > 1 {
                    format!("daily {hour:02}:{minute:02} {every}")
                } else {
                    format!("daily {hour:02}:{minute:02}")
                }
            }
            Trigger::Weekly { hour, minute, days } => {
                let list: Vec<String> = (0..7)
                    .filter(|d| days & (1 << d) != 0)
                    .map(|d| d.to_string())
                    .collect();
                format!("weekly {hour:02}:{minute:02} {}", list.join(","))
            }
            Trigger::Interval { minutes } => format!("interval {minutes}"),
            Trigger::Boot => "boot".into(),
            Trigger::Logon => "logon".into(),
            Trigger::Event(e) => format!("event {e}"),
        }
    }

    /// Vrai si ce déclencheur horaire tombe sur la minute commençant à `m`.
    fn matches_minute(&self, m: i64) -> bool {
        let l = admin::local(m);
        match *self {
            Trigger::Once {
                year,
                month,
                day,
                hour,
                minute,
            } => (l.year, l.month, l.day, l.hour, l.minute) == (year, month, day, hour, minute),
            Trigger::Daily {
                hour,
                minute,
                every,
            } => {
                let days = (m + l.offset).div_euclid(86_400);
                l.hour == hour && l.minute == minute && days % every as i64 == 0
            }
            Trigger::Weekly { hour, minute, days } => {
                l.hour == hour && l.minute == minute && days & (1 << l.weekday) != 0
            }
            _ => false,
        }
    }

    /// Prochaine échéance strictement après `now` pour un déclencheur horaire.
    pub fn next_after(&self, now: i64) -> Option<i64> {
        match *self {
            Trigger::Once {
                year,
                month,
                day,
                hour,
                minute,
            } => {
                let t = admin::epoch_of(year, month, day as i64, hour, minute);
                (t > now).then_some(t)
            }
            Trigger::Daily { hour, minute, .. } | Trigger::Weekly { hour, minute, .. } => {
                let l = admin::local(now);
                let span = match *self {
                    Trigger::Daily { every, .. } => every as i64 + 1,
                    _ => 8,
                };
                (0..=span)
                    .map(|d| admin::epoch_of(l.year, l.month, l.day as i64 + d, hour, minute))
                    .find(|&t| t > now && self.matches_minute(t))
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Action {
    pub path: String,
    pub arguments: String,
    pub workdir: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    /// Nom du fichier sans `.task`.
    pub id: String,
    pub name: String,
    /// Dossier du planificateur (`\`, `\Winlux`…).
    pub folder: String,
    pub description: String,
    pub author: String,
    pub enabled: bool,
    pub triggers: Vec<Trigger>,
    pub actions: Vec<Action>,
    /// Ne démarrer que sur secteur.
    pub ac_only: bool,
    /// Ne démarrer que si le réseau est disponible.
    pub network: bool,
    /// Durée maximale en minutes (0 = illimitée).
    pub time_limit: u32,
    /// Compte propriétaire d'une tâche personnelle, sous lequel ses actions
    /// tournent ; `None` pour une tâche du système. Déduit de l'endroit où
    /// le fichier est rangé, jamais lu dans le fichier (#1824).
    pub owner: Option<u32>,
}

impl Default for Task {
    fn default() -> Task {
        Task {
            id: String::new(),
            name: String::new(),
            folder: "\\".into(),
            description: String::new(),
            author: String::new(),
            enabled: true,
            triggers: Vec::new(),
            actions: Vec::new(),
            ac_only: false,
            network: false,
            time_limit: 4320,
            owner: None,
        }
    }
}

fn clean(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c == '\n' || c == '\r' || c == '\t' {
                ' '
            } else {
                c
            }
        })
        .collect()
}

impl Task {
    pub fn parse(id: &str, text: &str) -> Task {
        let mut t = Task {
            id: id.to_string(),
            name: id.to_string(),
            ..Task::default()
        };
        // Format historique du bureau : une seule action en trois clés.
        let mut single = Action::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim_end_matches('\r');
            match key.trim() {
                "name" if !value.is_empty() => t.name = value.into(),
                "folder" if !value.is_empty() => t.folder = value.into(),
                "description" => t.description = value.into(),
                "author" => t.author = value.into(),
                "enabled" => t.enabled = value != "0",
                "ac_only" => t.ac_only = value == "1",
                "network" => t.network = value == "1",
                "time_limit" => t.time_limit = value.parse().unwrap_or(t.time_limit),
                "trigger" => t.triggers.extend(Trigger::parse(value)),
                "action" => {
                    let mut parts = value.split('\t');
                    let path = parts.next().unwrap_or_default().to_string();
                    if !path.is_empty() {
                        t.actions.push(Action {
                            path,
                            arguments: parts.next().unwrap_or_default().into(),
                            workdir: parts.next().unwrap_or_default().into(),
                        });
                    }
                }
                "command" => single.path = value.into(),
                "arguments" => single.arguments = value.into(),
                "workdir" => single.workdir = value.into(),
                _ => {}
            }
        }
        if !single.path.is_empty() {
            t.actions.insert(0, single);
        }
        t
    }

    pub fn to_text(&self) -> String {
        let mut s = String::new();
        s += &format!("name={}\n", clean(&self.name));
        s += &format!("folder={}\n", clean(&self.folder));
        s += &format!("description={}\n", clean(&self.description));
        s += &format!("author={}\n", clean(&self.author));
        s += &format!("enabled={}\n", self.enabled as u8);
        for tr in &self.triggers {
            s += &format!("trigger={}\n", tr.to_line());
        }
        for a in &self.actions {
            s += &format!(
                "action={}\t{}\t{}\n",
                clean(&a.path),
                clean(&a.arguments),
                clean(&a.workdir)
            );
        }
        s += &format!("ac_only={}\n", self.ac_only as u8);
        s += &format!("network={}\n", self.network as u8);
        s += &format!("time_limit={}\n", self.time_limit);
        s
    }

    /// Clé de l'état (dernière exécution, instance en cours, historique) :
    /// l'id pour une tâche du système, `u<uid>-<id>` pour celle d'un compte.
    pub fn key(&self) -> String {
        match self.owner {
            Some(uid) => account_key(uid, &self.id),
            None => self.id.clone(),
        }
    }

    /// Chemin complet façon Windows : `\Dossier\Nom`.
    pub fn full_path(&self) -> String {
        let folder = self.folder.trim_end_matches('\\');
        format!("{folder}\\{}", self.name)
    }

    /// Prochaine exécution horaire (les intervalles dépendent de l'état).
    pub fn next_run(&self, now: i64, interval_base: Option<i64>) -> Option<i64> {
        if !self.enabled {
            return None;
        }
        self.triggers
            .iter()
            .filter_map(|t| match t {
                Trigger::Interval { minutes } => {
                    let base = interval_base.unwrap_or(now);
                    Some(base + *minutes as i64 * 60)
                }
                other => other.next_after(now),
            })
            .min()
    }
}

/// Identifiant de fichier stable pour un chemin de tâche.
pub fn id_for(folder: &str, name: &str) -> String {
    let full = format!("{}\\{}", folder.trim_matches('\\'), name);
    let id: String = full
        .trim_matches('\\')
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if id.is_empty() {
        "tache".into()
    } else {
        id
    }
}

fn euid() -> u32 {
    // SAFETY : geteuid n'échoue jamais.
    unsafe { libc::geteuid() }
}

/// Seul root écrit dans la base du système ; un autre compte enregistre ses
/// tâches dans son dossier personnel (#1824).
fn writes_system() -> bool {
    euid() == 0
}

/// Dossier personnel d'un compte pour les tâches : sous la racine de test
/// quand `WINLUX_ADMIN_ROOT` est posée, pour ne jamais toucher au vrai.
fn own_home() -> PathBuf {
    if std::env::var_os("WINLUX_ADMIN_ROOT").is_some_and(|r| !r.is_empty()) {
        admin::path(&format!("home/{}", euid()))
    } else {
        crate::paths::home()
    }
}

/// Dossier où le compte courant enregistre ses tâches et ses dossiers.
pub fn store_dir() -> PathBuf {
    if writes_system() {
        admin::path(TASKS_DIR)
    } else {
        own_home().join(USER_TASKS_REL)
    }
}

fn own_file(id: &str) -> Option<PathBuf> {
    (!writes_system()).then(|| own_home().join(USER_TASKS_REL).join(format!("{id}.task")))
}

/// Fichier d'une tâche : celle du compte d'abord, sinon celle du système.
fn task_file(id: &str) -> PathBuf {
    if let Some(p) = own_file(id).filter(|p| p.exists()) {
        return p;
    }
    admin::path(TASKS_DIR).join(format!("{id}.task"))
}

/// Tâches `.task` d'un dossier, avec leur propriétaire.
fn load_dir(dir: &std::path::Path, owner: Option<u32>) -> Vec<Task> {
    let mut tasks = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return tasks;
    };
    for e in entries.flatten() {
        let p = e.path();
        if !p.extension().is_some_and(|x| x == "task") {
            continue;
        }
        // Pour une tâche de compte : un vrai fichier, à ce compte (pas un
        // lien vers le fichier d'un autre).
        if let Some(uid) = owner {
            use std::os::unix::fs::MetadataExt;
            match std::fs::symlink_metadata(&p) {
                Ok(m) if m.is_file() && m.uid() == uid => {}
                _ => continue,
            }
        }
        let id = p
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Some(text) = admin::read_text(&p) {
            let mut t = Task::parse(&id, &text);
            t.owner = owner;
            tasks.push(t);
        }
    }
    tasks
}

fn sort_tasks(tasks: &mut [Task]) {
    tasks.sort_by(|a, b| {
        a.full_path()
            .to_lowercase()
            .cmp(&b.full_path().to_lowercase())
    });
}

/// Tâches visibles du compte courant : celles du système et les siennes
/// (une tâche personnelle masque une tâche système de même id).
pub fn load_all() -> Vec<Task> {
    let mut tasks = Vec::new();
    if !writes_system() {
        tasks = load_dir(&own_home().join(USER_TASKS_REL), Some(euid()));
    }
    for t in load_dir(&admin::path(TASKS_DIR), None) {
        if !tasks.iter().any(|o| o.id == t.id) {
            tasks.push(t);
        }
    }
    sort_tasks(&mut tasks);
    tasks
}

/// Comptes ordinaires (uid ≥ 1000) et leur dossier personnel, d'après
/// `/etc/passwd`.
fn accounts() -> Vec<(u32, PathBuf)> {
    let text = admin::read_text(&admin::path("etc/passwd")).unwrap_or_default();
    accounts_in(&text)
}

fn accounts_in(passwd: &str) -> Vec<(u32, PathBuf)> {
    passwd
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(':').collect();
            let uid: u32 = f.get(2)?.parse().ok()?;
            let home = f.get(5)?;
            (uid >= 1000 && uid != 65534 && home.starts_with('/')).then(|| (uid, admin::path(home)))
        })
        .collect()
}

/// Tâches du service : celles du système, puis celles de chaque compte.
/// Leur état est rangé sous [`Task::key`], pour que deux comptes ne se
/// mêlent pas.
fn load_everyone() -> Vec<Task> {
    let mut tasks = load_dir(&admin::path(TASKS_DIR), None);
    for (uid, home) in accounts() {
        let dir = home.join(USER_TASKS_REL);
        // Le dossier lui-même doit appartenir au compte.
        use std::os::unix::fs::MetadataExt;
        if !std::fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir() && m.uid() == uid) {
            continue;
        }
        tasks.extend(load_dir(&dir, Some(uid)));
    }
    sort_tasks(&mut tasks);
    tasks
}

/// Clé d'état d'une tâche de compte : `u<uid>-<id>`.
fn account_key(uid: u32, id: &str) -> String {
    format!("u{uid}-{id}")
}

/// Clé d'état (dernière exécution, instance en cours) d'une tâche vue par
/// le compte courant.
fn state_key(id: &str) -> String {
    match own_file(id) {
        Some(p) if p.exists() => account_key(euid(), id),
        _ => id.to_string(),
    }
}

pub fn load(id: &str) -> Option<Task> {
    let p = task_file(id);
    let mut t = admin::read_text(&p).map(|t| Task::parse(id, &t))?;
    if own_file(id).is_some_and(|o| o == p) {
        t.owner = Some(euid());
    }
    Some(t)
}

pub fn save(task: &Task) -> std::io::Result<()> {
    save_text(&task.id, &task.to_text())
}

/// Identifiant utilisable comme nom de fichier `.task` : celui que produit
/// [`id_for`] (lettres, chiffres, `-`, `.`, `_`), sans `.` en tête.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('.')
        && id.len() <= 200
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
}

/// Écrit le texte d'une tâche dans le magasin du compte courant : celui du
/// système pour root, le dossier personnel sinon (#1824, #2230).
pub fn save_text(id: &str, text: &str) -> std::io::Result<()> {
    if !valid_id(id) {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    let dir = store_dir();
    if !writes_system() {
        // Dossier privé du compte : personne d'autre n'y lit ses tâches.
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)?;
    }
    admin::write_text(&dir.join(format!("{id}.task")), text)
}

/// Une tâche vue par le Planificateur du bureau.
pub struct Entry {
    pub id: String,
    /// Clé de son état (instance en cours, dernière exécution, prochaine).
    pub key: String,
    pub file: PathBuf,
    /// Le compte courant peut la modifier : la sienne, ou toutes pour root.
    pub mine: bool,
}

/// Tâches visibles du compte courant, pour le Planificateur du bureau
/// (#2230) : il lit ainsi les deux magasins sans connaître leurs chemins.
pub fn entries() -> Vec<Entry> {
    load_all()
        .into_iter()
        .map(|t| Entry {
            file: task_file(&t.id),
            key: t.key(),
            mine: writes_system() || t.owner.is_some(),
            id: t.id,
        })
        .collect()
}

pub fn delete(id: &str) -> bool {
    let key = state_key(id);
    let ok = std::fs::remove_file(task_file(id)).is_ok();
    let _ = std::fs::remove_file(admin::path(STATE_DIR).join(format!("{key}.last")));
    ok
}

// ---------- État et historique ----------

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LastRun {
    pub time: i64,
    pub result: i32,
}

pub fn last_run(id: &str) -> Option<LastRun> {
    let key = state_key(id);
    let text = admin::read_text(&admin::path(STATE_DIR).join(format!("{key}.last")))?;
    Some(LastRun {
        time: admin::field(&text, "last_run")?.parse().ok()?,
        result: admin::field(&text, "last_result")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
    })
}

fn running_file(id: &str) -> PathBuf {
    admin::path(RUN_DIR).join("running").join(id)
}

/// PID de l'instance en cours, si elle vit encore.
pub fn running_pid(id: &str) -> Option<i32> {
    running_pid_of(&state_key(id))
}

fn running_pid_of(key: &str) -> Option<i32> {
    let pid: i32 = admin::read_text(&running_file(key))?.trim().parse().ok()?;
    // SAFETY: kill(pid, 0) ne fait que tester l'existence du processus.
    (pid > 0 && unsafe { libc::kill(pid, 0) } == 0).then_some(pid)
}

pub fn history(task: &Task, kind: &str, detail: &str) {
    let now = admin::now();
    admin::append_line(
        &admin::path(STATE_DIR).join("history.log"),
        &format!(
            "{now}\t{}\t{}\t{}\t{kind}\t{}",
            admin::format_time(now),
            task.key(),
            clean(&task.name),
            clean(detail)
        ),
        HISTORY_MAX,
    );
}

// ---------- Exécution ----------

/// Commande Linux réalisant une action (programme Windows ou Linux).
pub fn command_for(action: &Action) -> Option<Command> {
    let raw = action.path.trim().trim_matches('"');
    if raw.is_empty() {
        return None;
    }
    let windows_like = raw.contains('\\') || raw.as_bytes().get(1) == Some(&b':');
    let path = if windows_like {
        crate::paths::to_linux(raw)
    } else {
        PathBuf::from(raw)
    };
    let args = if action.arguments.trim().is_empty() {
        Vec::new()
    } else {
        crate::dlls::kernel32::split_windows_command_line(&action.arguments)
    };
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let mut cmd = match ext.as_str() {
        "exe" | "com" | "scr" => {
            let mut c = Command::new(std::env::current_exe().unwrap_or_else(|_| "winlux".into()));
            c.arg(&path);
            c
        }
        "sh" => {
            let mut c = Command::new("sh");
            c.arg(&path);
            c
        }
        "py" => {
            let mut c = Command::new("python3");
            c.arg(&path);
            c
        }
        _ => Command::new(&path),
    };
    cmd.args(args).stdin(std::process::Stdio::null());
    let dir = action.workdir.trim();
    if !dir.is_empty() {
        let dir = if dir.contains('\\') || dir.as_bytes().get(1) == Some(&b':') {
            crate::paths::to_linux(dir)
        } else {
            PathBuf::from(dir)
        };
        if dir.is_dir() {
            cmd.current_dir(dir);
        }
    }
    Some(cmd)
}

/// Compte Linux : nom, groupe principal, dossier personnel, groupes.
struct Account {
    name: String,
    gid: u32,
    home: String,
    groups: Vec<libc::gid_t>,
}

fn account(uid: u32) -> Option<Account> {
    let mut pw: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buf = vec![0 as libc::c_char; 16 * 1024];
    let mut res: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY : tampons à nous, getpwuid_r est réentrant.
    let rc = unsafe { libc::getpwuid_r(uid, &mut pw, buf.as_mut_ptr(), buf.len(), &mut res) };
    if rc != 0 || res.is_null() || pw.pw_name.is_null() {
        return None;
    }
    // SAFETY : chaînes terminées par zéro dans `buf`.
    let (name, home) = unsafe {
        (
            std::ffi::CStr::from_ptr(pw.pw_name).to_owned(),
            std::ffi::CStr::from_ptr(pw.pw_dir)
                .to_string_lossy()
                .into_owned(),
        )
    };
    let mut groups: Vec<libc::gid_t> = vec![0; 256];
    let mut n = groups.len() as libc::c_int;
    // SAFETY : `groups` contient `n` cases.
    if unsafe { libc::getgrouplist(name.as_ptr(), pw.pw_gid, groups.as_mut_ptr(), &mut n) } < 0 {
        groups = vec![pw.pw_gid];
    } else {
        groups.truncate(n.max(0) as usize);
    }
    Some(Account {
        name: name.to_string_lossy().into_owned(),
        gid: pw.pw_gid,
        home,
        groups,
    })
}

/// Fait tourner la commande sous le compte `uid` : groupes, groupe puis
/// utilisateur, dans le processus enfant avant exec ; environnement du
/// compte. Sans compte connu, la tâche ne démarre pas (#1824).
fn as_account(cmd: &mut Command, uid: u32) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let acc = account(uid).ok_or_else(|| format!("compte {uid} inconnu"))?;
    let path = std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into());
    let lang = std::env::var_os("LANG");
    cmd.env_clear()
        .env("PATH", path)
        .env("HOME", &acc.home)
        .env("USER", &acc.name)
        .env("LOGNAME", &acc.name);
    if let Some(l) = lang {
        cmd.env("LANG", l);
    }
    let runtime = format!("/run/user/{uid}");
    if std::path::Path::new(&runtime).is_dir() {
        cmd.env("XDG_RUNTIME_DIR", runtime);
    }
    let (gid, groups) = (acc.gid, acc.groups);
    // Dossier de travail : Rust fait son chdir avant les closures pre_exec,
    // donc encore en root. On part de « / » et on entre dans le dossier de
    // la tâche après setuid, avec les droits du compte (revue de #1864).
    let workdir = cmd
        .get_current_dir()
        .map(|d| std::ffi::CString::new(d.as_os_str().as_encoded_bytes().to_vec()))
        .transpose()
        .map_err(|_| "dossier de travail illisible".to_string())?;
    cmd.current_dir("/");
    // SAFETY : seuls des appels système sûrs après fork (setgroups,
    // setgid, setuid, chdir), sur des données préparées avant.
    unsafe {
        cmd.pre_exec(move || {
            if libc::setgroups(groups.len(), groups.as_ptr()) != 0
                || libc::setgid(gid) != 0
                || libc::setuid(uid) != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            if let Some(dir) = &workdir {
                if libc::chdir(dir.as_ptr()) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    Ok(())
}

/// Raison pour laquelle une tâche ne doit pas démarrer maintenant.
pub fn blocked_reason(task: &Task) -> Option<&'static str> {
    if task.ac_only && !admin::on_ac_power() {
        return Some("battery");
    }
    if task.network && !admin::network_available() {
        return Some("network");
    }
    if running_pid_of(&task.key()).is_some() {
        return Some("running");
    }
    None
}

/// Exécute toutes les actions d'une tâche et attend leur fin. Renvoie le
/// code de la dernière action (ou -1 si rien n'a pu démarrer).
pub fn run_task(task: &Task, reason: &str) -> i32 {
    if let Some(why) = blocked_reason(task) {
        history(task, "skip", why);
        return -1;
    }
    if task.actions.is_empty() {
        history(task, "error", "aucune action");
        return -1;
    }
    history(task, "start", reason);
    let mut code = 0;
    for action in &task.actions {
        let Some(mut cmd) = command_for(action) else {
            history(task, "error", "action vide");
            code = -1;
            continue;
        };
        // Tâche d'un compte : jamais en root, toujours sous ce compte.
        if let Some(uid) = task.owner {
            if euid() == 0 {
                if let Err(e) = as_account(&mut cmd, uid) {
                    history(task, "error", &e);
                    code = -1;
                    continue;
                }
            } else if euid() != uid {
                history(task, "error", "tâche d'un autre compte");
                code = -1;
                continue;
            }
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                history(task, "error", &format!("{} : {e}", action.path));
                code = -1;
                continue;
            }
        };
        let marker = running_file(&task.key());
        let _ = admin::write_text(&marker, &child.id().to_string());
        let deadline = (task.time_limit > 0).then(|| admin::now() + task.time_limit as i64 * 60);
        code = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.code().unwrap_or(-1),
                Ok(None) => {}
                Err(_) => break -1,
            }
            if deadline.is_some_and(|d| admin::now() >= d) {
                let _ = child.kill();
                let _ = child.wait();
                history(task, "timeout", &task.time_limit.to_string());
                break -1;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        };
        let _ = std::fs::remove_file(&marker);
    }
    let now = admin::now();
    let _ = admin::write_text(
        &admin::path(STATE_DIR).join(format!("{}.last", task.key())),
        &format!(
            "last_run={now}\nlast_result={code}\nlast_run_text={}\n",
            admin::format_time(now)
        ),
    );
    history(task, "end", &code.to_string());
    code
}

/// Arrête l'instance en cours d'une tâche.
pub fn end_task(id: &str) -> bool {
    match running_pid(id) {
        // SAFETY: envoi d'un signal à un PID lu depuis notre propre marqueur.
        Some(pid) => unsafe { libc::kill(pid, libc::SIGTERM) == 0 },
        None => false,
    }
}

// ---------- Service ----------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SystemState {
    network: bool,
    ac: bool,
    usb: usize,
}

fn system_state() -> SystemState {
    let usb = std::fs::read_dir(admin::path("sys/bus/usb/devices"))
        .map(|d| {
            d.flatten()
                .filter(|e| !e.file_name().to_string_lossy().contains(':'))
                .count()
        })
        .unwrap_or(0);
    SystemState {
        network: admin::network_available(),
        ac: admin::on_ac_power(),
        usb,
    }
}

/// Événements survenus entre deux relevés de l'état système.
fn events_between(old: SystemState, new: SystemState) -> Vec<&'static str> {
    let mut ev = Vec::new();
    if new.network != old.network {
        ev.push(if new.network {
            "network-up"
        } else {
            "network-down"
        });
    }
    if new.ac != old.ac {
        ev.push(if new.ac { "power-ac" } else { "power-battery" });
    }
    if new.usb > old.usb {
        ev.push("usb-added");
    }
    ev
}

/// Tâches dont un déclencheur horaire tombe dans `(from, to]`. Au plus cinq
/// minutes sont rattrapées après une mise en veille.
pub fn due_between(tasks: &[Task], from: i64, to: i64) -> Vec<Task> {
    let mut minutes = Vec::new();
    let mut m = to.div_euclid(60) * 60;
    while m > from && minutes.len() < 5 {
        minutes.push(m);
        m -= 60;
    }
    tasks
        .iter()
        .filter(|t| t.enabled)
        .filter(|t| {
            t.triggers
                .iter()
                .any(|tr| minutes.iter().any(|&m| tr.matches_minute(m)))
        })
        .cloned()
        .collect()
}

fn matching(tasks: &[Task], pred: impl Fn(&Trigger) -> bool) -> Vec<Task> {
    tasks
        .iter()
        .filter(|t| t.enabled && t.triggers.iter().any(&pred))
        .cloned()
        .collect()
}

fn spawn_run(task: Task, reason: String, busy: &Arc<Mutex<HashSet<String>>>) {
    {
        let mut b = crate::dlls::lock(busy);
        if !b.insert(task.key()) {
            history(&task, "skip", "running");
            return;
        }
    }
    let busy = Arc::clone(busy);
    std::thread::spawn(move || {
        run_task(&task, &reason);
        crate::dlls::lock(&busy).remove(&task.key());
    });
}

fn write_next(tasks: &[Task], now: i64, bases: &HashMap<String, i64>) {
    let mut out = String::new();
    for t in tasks {
        if let Some(n) = t.next_run(now, bases.get(&t.key()).copied()) {
            out += &format!("{}\t{}\t{}\n", t.key(), n, admin::format_time(n));
        }
    }
    let _ = admin::write_text(&admin::path(RUN_DIR).join("next.txt"), &out);
}

pub fn daemon() -> ! {
    let busy: Arc<Mutex<HashSet<String>>> = Arc::default();
    let mut bases: HashMap<String, i64> = HashMap::new();
    let start = admin::now();
    let _ = std::fs::create_dir_all(admin::path(RUN_DIR).join("running"));
    let _ = admin::write_text(
        &admin::path(RUN_DIR).join("daemon.pid"),
        &std::process::id().to_string(),
    );
    let booted = admin::path(RUN_DIR).join("booted");
    let mut tasks = load_everyone();
    if !booted.exists() {
        let _ = admin::write_text(&booted, &start.to_string());
        for t in matching(&tasks, |tr| *tr == Trigger::Boot) {
            spawn_run(t, "boot".into(), &busy);
        }
    }
    let mut state = system_state();
    let mut last = start;
    let mut tick = 0u64;
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        tick += 1;
        let now = admin::now();
        tasks = load_everyone();
        for t in due_between(&tasks, last, now) {
            spawn_run(t, "time".into(), &busy);
        }
        for t in &tasks {
            for tr in &t.triggers {
                if let Trigger::Interval { minutes } = tr {
                    let base = *bases.entry(t.key()).or_insert(now);
                    if t.enabled && now >= base + *minutes as i64 * 60 {
                        bases.insert(t.key(), now);
                        spawn_run(t.clone(), "interval".into(), &busy);
                    }
                }
            }
        }
        if tick.is_multiple_of(2) {
            let new = system_state();
            for ev in events_between(state, new) {
                for t in matching(&tasks, |tr| matches!(tr, Trigger::Event(e) if e == ev)) {
                    spawn_run(t, format!("event:{ev}"), &busy);
                }
            }
            state = new;
        }
        if now / 60 != last / 60 || tick % 10 == 1 {
            write_next(&tasks, now, &bases);
        }
        last = now;
    }
}

fn usage() -> i32 {
    eprintln!(
        "usage : winlux --tasks daemon | list | run ID | end ID | enable ID | disable ID | delete ID | fire logon|boot|EVENEMENT"
    );
    2
}

/// Point d'entrée de `winlux --tasks …`.
pub fn cli(args: &[String]) -> i32 {
    let arg = |i: usize| args.get(i).map(String::as_str).unwrap_or("");
    match arg(0) {
        "daemon" => daemon(),
        "list" => {
            for t in load_all() {
                let state = if running_pid(&t.id).is_some() {
                    "en cours"
                } else if t.enabled {
                    "prête"
                } else {
                    "désactivée"
                };
                let triggers: Vec<String> = t.triggers.iter().map(Trigger::to_line).collect();
                println!(
                    "{}\t{}\t{}\t{}",
                    t.id,
                    t.full_path(),
                    state,
                    triggers.join("; ")
                );
            }
            0
        }
        "run" => match load(arg(1)) {
            Some(t) => {
                run_task(&t, "manual");
                0
            }
            None => {
                eprintln!("tâche introuvable : {}", arg(1));
                1
            }
        },
        "end" => i32::from(!end_task(arg(1))),
        "enable" | "disable" => match load(arg(1)) {
            // Un compte ordinaire ne recopie pas une tâche du système chez
            // lui : elle la masquerait pour lui seul (#2230).
            Some(t) if !writes_system() && t.owner.is_none() => {
                eprintln!("tâche du système : seul root peut la modifier : {}", arg(1));
                1
            }
            Some(mut t) => {
                t.enabled = arg(0) == "enable";
                i32::from(save(&t).is_err())
            }
            None => 1,
        },
        "delete" => i32::from(!delete(arg(1))),
        "fire" => {
            let what = arg(1);
            let tasks = if writes_system() {
                load_everyone()
            } else {
                load_all()
            };
            let selected = matching(&tasks, |tr| match what {
                "logon" => *tr == Trigger::Logon,
                "boot" => *tr == Trigger::Boot,
                ev => matches!(tr, Trigger::Event(e) if e == ev),
            });
            let handles: Vec<_> = selected
                .into_iter()
                .map(|t| {
                    let reason = what.to_string();
                    std::thread::spawn(move || run_task(&t, &reason))
                })
                .collect();
            for h in handles {
                let _ = h.join();
            }
            0
        }
        _ => usage(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::test_root::Root;

    #[test]
    fn triggers_round_trip() {
        for line in [
            "once 2026-11-02 09:05",
            "daily 08:30",
            "daily 08:30 3",
            "weekly 18:00 1,3,5",
            "interval 15",
            "boot",
            "logon",
            "event network-up",
        ] {
            let t = Trigger::parse(line).unwrap_or_else(|| panic!("{line}"));
            assert_eq!(t.to_line(), line);
        }
        assert!(Trigger::parse("daily 25:00").is_none());
        assert!(Trigger::parse("weekly 10:00 7").is_none());
        assert!(Trigger::parse("monthly 1").is_none());
    }

    #[test]
    fn task_text_round_trip_and_legacy_single_action() {
        let t = Task {
            id: "Sauvegarde".into(),
            name: "Sauvegarde nocturne".into(),
            description: "Copie des documents".into(),
            triggers: vec![Trigger::parse("daily 02:00").unwrap(), Trigger::Boot],
            actions: vec![Action {
                path: "C:\\Outils\\backup.exe".into(),
                arguments: "/all \"Mes documents\"".into(),
                workdir: "C:\\Outils".into(),
            }],
            ac_only: true,
            network: true,
            ..Task::default()
        };
        assert_eq!(Task::parse("Sauvegarde", &t.to_text()), t);
        let legacy = Task::parse("x", "command=/bin/true\narguments=-v\nenabled=0\n");
        assert!(!legacy.enabled);
        assert_eq!(legacy.actions[0].path, "/bin/true");
        assert_eq!(legacy.actions[0].arguments, "-v");
    }

    #[test]
    fn schedule_matches_and_next_run() {
        let at = admin::epoch_of(2026, 11, 2, 9, 5); // lundi
        let once = Trigger::parse("once 2026-11-02 09:05").unwrap();
        assert!(once.matches_minute(at));
        assert!(!once.matches_minute(at + 60));
        assert_eq!(once.next_after(at - 3600), Some(at));
        assert_eq!(once.next_after(at), None);
        let weekly = Trigger::parse("weekly 09:05 1").unwrap();
        assert!(weekly.matches_minute(at));
        assert_eq!(
            weekly.next_after(at),
            Some(admin::epoch_of(2026, 11, 9, 9, 5))
        );
        let daily = Trigger::parse("daily 07:00").unwrap();
        assert_eq!(
            daily.next_after(at),
            Some(admin::epoch_of(2026, 11, 3, 7, 0))
        );
        let task = Task {
            triggers: vec![daily, Trigger::Interval { minutes: 30 }],
            ..Task::default()
        };
        assert_eq!(task.next_run(at, Some(at)), Some(at + 1800));
        let due = due_between(
            std::slice::from_ref(&Task {
                triggers: vec![once],
                ..Task::default()
            }),
            at - 1,
            at + 1,
        );
        assert_eq!(due.len(), 1);
    }

    #[test]
    fn system_events_are_detected_from_state_changes() {
        let a = SystemState {
            network: false,
            ac: true,
            usb: 2,
        };
        let b = SystemState {
            network: true,
            ac: false,
            usb: 3,
        };
        assert_eq!(
            events_between(a, b),
            vec!["network-up", "power-battery", "usb-added"]
        );
        assert!(events_between(b, b).is_empty());
    }

    #[test]
    fn comptes_ordinaires_du_passwd() {
        let passwd = "root:x:0:0:root:/root:/bin/sh\n\
                      nobody:x:65534:65534::/nonexistent:/bin/false\n\
                      damien:x:1000:1000:Damien:/home/damien:/bin/bash\n\
                      cassé:x:abc:1::/home/x:/bin/sh\n";
        let a = accounts_in(passwd);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].0, 1000);
        assert!(a[0].1.ends_with("home/damien"));
    }

    #[test]
    fn tache_de_compte_rangee_chez_lui_et_lue_par_le_service() {
        use std::os::unix::fs::PermissionsExt;
        let r = Root::new("tasks-compte");
        let t = Task {
            id: id_for("\\Adobe", "Mise à jour"),
            name: "Mise à jour".into(),
            folder: "\\Adobe".into(),
            actions: vec![Action {
                path: "/bin/true".into(),
                ..Action::default()
            }],
            ..Task::default()
        };
        save(&t).unwrap();
        let me = euid();
        let file = store_dir().join(format!("{}.task", t.id));
        assert!(file.is_file());
        if me != 0 {
            // Jamais dans la base du système ; dossier privé du compte.
            assert!(!r
                .dir
                .join(TASKS_DIR)
                .join(format!("{}.task", t.id))
                .exists());
            let mode = std::fs::metadata(store_dir()).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0);
            assert_eq!(load(&t.id).unwrap().owner, Some(me));
            assert_eq!(load(&t.id).unwrap().key(), format!("u{me}-{}", t.id));
        } else {
            assert_eq!(load(&t.id).unwrap().owner, None);
        }
        // Le service lit les tâches d'un dossier de compte avec leur
        // propriétaire, et ignore celles qui ne lui appartiennent pas.
        let dir = r.dir.join("perso");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.task"), "name=A\naction=/bin/true\n").unwrap();
        let own = load_dir(&dir, Some(me));
        assert_eq!(own.len(), 1);
        assert_eq!(own[0].owner, Some(me));
        assert_eq!(own[0].key(), format!("u{me}-a"));
        assert!(load_dir(&dir, Some(me + 1)).is_empty());
        // Une tâche système garde son id comme clé.
        assert_eq!(load_dir(&dir, None)[0].key(), "a");
        assert!(delete(&t.id));
    }

    #[test]
    fn run_records_history_state_and_conditions() {
        let r = Root::new("tasks");
        let out = r.dir.join("ran");
        let mut t = Task {
            id: id_for("\\", "Écrire"),
            name: "Écrire".into(),
            triggers: vec![Trigger::Logon],
            actions: vec![Action {
                path: "/bin/sh".into(),
                arguments: format!("-c \"echo ok > {}; exit 3\"", out.display()),
                workdir: String::new(),
            }],
            ..Task::default()
        };
        assert_eq!(t.id, "_crire");
        save(&t).unwrap();
        // Sous un compte ordinaire (CI), la tâche va dans sa base à lui.
        let saved = Task {
            owner: (euid() != 0).then(euid),
            ..t.clone()
        };
        assert_eq!(load_all(), vec![saved]);
        assert_eq!(cli(&["fire".into(), "logon".into()]), 0);
        assert_eq!(std::fs::read_to_string(&out).unwrap().trim(), "ok");
        assert_eq!(last_run(&t.id).unwrap().result, 3);
        // Sans réseau, la condition empêche le démarrage.
        t.network = true;
        assert_eq!(run_task(&t, "manual"), -1);
        let log = admin::read_text(&r.dir.join(STATE_DIR).join("history.log")).unwrap();
        let kinds: Vec<&str> = log.lines().map(|l| l.split('\t').nth(4).unwrap()).collect();
        assert_eq!(kinds, vec!["start", "end", "skip"]);
        assert!(log.contains("\tÉcrire\t"));
        assert!(delete(&t.id));
        assert!(load_all().is_empty());
    }
}
