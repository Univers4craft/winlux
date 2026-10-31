//! Planificateur de tâches Winlux : modèle des tâches, service de
//! déclenchement et commandes `winlux --tasks …`.
//!
//! Une tâche est un fichier texte `clé=valeur` dans `/etc/winlux/tasks`
//! (`<id>.task`, en Latin-1 pour que le bureau Win32 le lise et l'écrive
//! directement). Le service `winlux --tasks daemon`, lancé par l'init Winlux,
//! surveille l'heure et les événements système et exécute les actions.
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

fn task_file(id: &str) -> PathBuf {
    admin::path(TASKS_DIR).join(format!("{id}.task"))
}

pub fn load_all() -> Vec<Task> {
    let mut tasks = Vec::new();
    let Ok(entries) = std::fs::read_dir(admin::path(TASKS_DIR)) else {
        return tasks;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "task") {
            let id = p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            if let Some(text) = admin::read_text(&p) {
                tasks.push(Task::parse(&id, &text));
            }
        }
    }
    tasks.sort_by(|a, b| {
        a.full_path()
            .to_lowercase()
            .cmp(&b.full_path().to_lowercase())
    });
    tasks
}

pub fn load(id: &str) -> Option<Task> {
    admin::read_text(&task_file(id)).map(|t| Task::parse(id, &t))
}

pub fn save(task: &Task) -> std::io::Result<()> {
    admin::write_text(&task_file(&task.id), &task.to_text())
}

pub fn delete(id: &str) -> bool {
    let ok = std::fs::remove_file(task_file(id)).is_ok();
    let _ = std::fs::remove_file(admin::path(STATE_DIR).join(format!("{id}.last")));
    ok
}

// ---------- État et historique ----------

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LastRun {
    pub time: i64,
    pub result: i32,
}

pub fn last_run(id: &str) -> Option<LastRun> {
    let text = admin::read_text(&admin::path(STATE_DIR).join(format!("{id}.last")))?;
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
    let pid: i32 = admin::read_text(&running_file(id))?.trim().parse().ok()?;
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
            task.id,
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

/// Raison pour laquelle une tâche ne doit pas démarrer maintenant.
pub fn blocked_reason(task: &Task) -> Option<&'static str> {
    if task.ac_only && !admin::on_ac_power() {
        return Some("battery");
    }
    if task.network && !admin::network_available() {
        return Some("network");
    }
    if running_pid(&task.id).is_some() {
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
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                history(task, "error", &format!("{} : {e}", action.path));
                code = -1;
                continue;
            }
        };
        let marker = running_file(&task.id);
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
        &admin::path(STATE_DIR).join(format!("{}.last", task.id)),
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
        if !b.insert(task.id.clone()) {
            history(&task, "skip", "running");
            return;
        }
    }
    let busy = Arc::clone(busy);
    std::thread::spawn(move || {
        run_task(&task, &reason);
        crate::dlls::lock(&busy).remove(&task.id);
    });
}

fn write_next(tasks: &[Task], now: i64, bases: &HashMap<String, i64>) {
    let mut out = String::new();
    for t in tasks {
        if let Some(n) = t.next_run(now, bases.get(&t.id).copied()) {
            out += &format!("{}\t{}\t{}\n", t.id, n, admin::format_time(n));
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
    let mut tasks = load_all();
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
        tasks = load_all();
        for t in due_between(&tasks, last, now) {
            spawn_run(t, "time".into(), &busy);
        }
        for t in &tasks {
            for tr in &t.triggers {
                if let Trigger::Interval { minutes } = tr {
                    let base = *bases.entry(t.id.clone()).or_insert(now);
                    if t.enabled && now >= base + *minutes as i64 * 60 {
                        bases.insert(t.id.clone(), now);
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
            Some(mut t) => {
                t.enabled = arg(0) == "enable";
                i32::from(save(&t).is_err())
            }
            None => 1,
        },
        "delete" => i32::from(!delete(arg(1))),
        "fire" => {
            let what = arg(1);
            let tasks = load_all();
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
        assert_eq!(load_all(), vec![t.clone()]);
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
