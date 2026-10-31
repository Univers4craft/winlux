//! Mode jeu : quand un jeu ou une application 3D tourne, Winlux lui donne le
//! processeur et la mémoire, puis remet tout en place à sa fermeture.
//!
//! - Le runtime inscrit chaque programme qui importe Direct3D, OpenGL, Vulkan
//!   ou XInput (`cpu::is_heavy`) dans `/run/winlux/gamemode/<pid>`.
//! - Le service `winlux --gamemode daemon` (root, lancé par l'init) surveille
//!   ces inscriptions toutes les deux secondes. Tant qu'un jeu vit :
//!   gouverneur « performance » (sur secteur et seulement s'il existe),
//!   priorité du jeu relevée, tâches de fond (sauvegarde, mises à jour, apt)
//!   en priorité minimale processeur et disque et mises en pause par l'init,
//!   mémoire compactée, swappiness abaissée, veille et extinction d'écran
//!   bloquées.
//! - Chaque action n'est faite que si la machine la permet ; l'état
//!   (`/run/winlux/gamemode.txt`) dit lesquelles sont possibles et
//!   lesquelles sont actives, et les Paramètres n'affichent que celles-là.

use crate::cpu::{self, Profile, Sys};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

const REG_DIR: &str = "run/winlux/gamemode";
const STATE: &str = "run/winlux/gamemode.txt";
/// Présent tant que le mode jeu est actif : l'init suspend alors ses boucles
/// de sauvegarde et de mises à jour automatiques.
const ACTIVE_FLAG: &str = "run/winlux/gamemode.active";
const INHIBIT: &str = "run/winlux/power-inhibit";
const CONFIG: &str = "etc/winlux/cpu.conf";
const SWAPPINESS: &str = "proc/sys/vm/swappiness";
const COMPACT: &str = "proc/sys/vm/compact_memory";
/// Swappiness pendant le jeu : la mémoire du jeu reste en RAM.
const GAME_SWAPPINESS: u32 = 10;
/// Priorité donnée au jeu (si elle était normale).
const GAME_NICE: i32 = -5;
const BACKGROUND_NICE: i32 = 19;
const ES_SYSTEM_DISPLAY: &str = "0x3";

/// Tâches de fond Winlux et système (nom `comm`, 15 caractères au plus).
/// Leurs processus enfants (tar, zstd, rsync…) suivent.
const BACKGROUND: &[&str] = &[
    "winlux-backup",
    "winlux-update",
    "winlux-sysupdat",
    "winlux-update-a",
    "apt",
    "apt-get",
    "dpkg",
    "unattended-upgr",
    "packagekitd",
    "fc-cache",
    "updatedb",
    "mandb",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Off,
}

impl Mode {
    pub fn load(root: &Path) -> Self {
        let text = fs::read_to_string(root.join(CONFIG)).unwrap_or_default();
        match cpu::config_value(&text, "GAME_MODE") {
            Some("off") => Mode::Off,
            _ => Mode::Auto,
        }
    }
    fn key(self) -> &'static str {
        match self {
            Mode::Auto => "auto",
            Mode::Off => "off",
        }
    }
}

// --- Inscription par le runtime --------------------------------------------

/// Champ 22 de `/proc/PID/stat` : identifie le processus même si son numéro
/// est réutilisé plus tard.
fn start_time(root: &Path, pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(root.join(format!("proc/{pid}/stat"))).ok()?;
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(19)?.parse().ok()
}

fn comm(root: &Path, pid: u32) -> String {
    fs::read_to_string(root.join(format!("proc/{pid}/comm")))
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn parent(root: &Path, pid: u32) -> Option<u32> {
    let stat = fs::read_to_string(root.join(format!("proc/{pid}/stat"))).ok()?;
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(1)?.parse().ok()
}

/// Inscrit le programme courant comme jeu (appelé au lancement d'un `.exe`
/// lourd). Sans service ou en mode « désactivé », ne fait rien.
pub fn register_current(exe_name: &str) {
    let root = Path::new("/");
    if Mode::load(root) == Mode::Off || !root.join(REG_DIR).is_dir() {
        return;
    }
    let pid = std::process::id();
    let Some(start) = start_time(root, pid) else {
        return;
    };
    let name: String = exe_name
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    let _ = fs::write(
        root.join(REG_DIR).join(pid.to_string()),
        format!("{start} {name}\n"),
    );
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub pid: u32,
    pub name: String,
}

/// Jeux inscrits et encore vivants ; les inscriptions périmées sont effacées.
/// Une inscription ne compte que si elle appartient au même utilisateur que
/// le processus qu'elle désigne.
pub fn live_games(root: &Path) -> Vec<Game> {
    let mut games = Vec::new();
    let Ok(dir) = fs::read_dir(root.join(REG_DIR)) else {
        return games;
    };
    for entry in dir.flatten() {
        let path = entry.path();
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            let _ = fs::remove_file(&path);
            continue;
        };
        let text = fs::read_to_string(&path).unwrap_or_default();
        let (start, name) = text.trim().split_once(' ').unwrap_or((text.trim(), ""));
        let owner_ok = match (
            fs::metadata(&path),
            fs::metadata(root.join(format!("proc/{pid}"))),
        ) {
            (Ok(file), Ok(proc_dir)) => file.uid() == proc_dir.uid(),
            _ => false,
        };
        let alive = start
            .parse::<u64>()
            .ok()
            .is_some_and(|s| start_time(root, pid) == Some(s));
        if alive && owner_ok {
            games.push(Game {
                pid,
                name: name.to_string(),
            });
        } else {
            let _ = fs::remove_file(&path);
        }
    }
    games.sort_by_key(|g| g.pid);
    games
}

/// Processus de fond à ralentir : un nom de la liste, ou un descendant.
pub fn background_pids(root: &Path, exclude: &[u32]) -> Vec<u32> {
    let mut comms = HashMap::new();
    let mut parents = HashMap::new();
    if let Ok(dir) = fs::read_dir(root.join("proc")) {
        for entry in dir.flatten() {
            if let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<u32>().ok())
            {
                comms.insert(pid, comm(root, pid));
                if let Some(ppid) = parent(root, pid) {
                    parents.insert(pid, ppid);
                }
            }
        }
    }
    let listed = |pid: u32| {
        comms
            .get(&pid)
            .is_some_and(|c| BACKGROUND.contains(&c.as_str()))
    };
    let mut out: Vec<u32> = comms
        .keys()
        .copied()
        .filter(|pid| !exclude.contains(pid))
        .filter(|&pid| {
            let mut cur = pid;
            for _ in 0..8 {
                if listed(cur) {
                    return true;
                }
                match parents.get(&cur) {
                    Some(&p) if p > 1 => cur = p,
                    _ => return false,
                }
            }
            false
        })
        .collect();
    out.sort_unstable();
    out
}

// --- Capacités -------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Caps {
    /// Le gouverneur « performance » existe et peut être choisi.
    pub governor: bool,
    /// Sur secteur (ou machine sans batterie) : le gouverneur peut servir.
    pub on_ac: bool,
    pub memory: bool,
    /// Le service peut relever la priorité d'un jeu (root).
    pub priority: bool,
}

pub fn detect(root: &Path, cpu_caps: &cpu::Caps, root_user: bool) -> Caps {
    let writable = |rel: &str| {
        fs::OpenOptions::new()
            .write(true)
            .open(root.join(rel))
            .is_ok()
    };
    Caps {
        governor: cpu_caps.plan(Profile::Performance).is_some(),
        on_ac: crate::power::read_status_at(root).ac_online,
        memory: writable(SWAPPINESS) || writable(COMPACT),
        priority: root_user,
    }
}

// --- Actions (abstraites pour les tests) -----------------------------------

pub trait Os {
    fn nice(&self, tid: u32) -> Option<i32>;
    fn set_nice(&mut self, tid: u32, nice: i32) -> bool;
    fn ioprio(&self, pid: u32) -> Option<i32>;
    fn set_ioprio(&mut self, pid: u32, value: i32) -> bool;
}

pub struct Linux;

const IOPRIO_WHO_PROCESS: libc::c_long = 1;
const IOPRIO_IDLE: i32 = 3 << 13;

impl Os for Linux {
    fn nice(&self, tid: u32) -> Option<i32> {
        unsafe {
            *libc::__errno_location() = 0;
            let v = libc::getpriority(libc::PRIO_PROCESS, tid as libc::id_t);
            (*libc::__errno_location() == 0).then_some(v)
        }
    }
    fn set_nice(&mut self, tid: u32, nice: i32) -> bool {
        cpu::set_nice(tid as i32, nice)
    }
    fn ioprio(&self, pid: u32) -> Option<i32> {
        let v = unsafe {
            libc::syscall(
                libc::SYS_ioprio_get,
                IOPRIO_WHO_PROCESS,
                pid as libc::c_long,
            )
        };
        (v >= 0).then_some(v as i32)
    }
    fn set_ioprio(&mut self, pid: u32, value: i32) -> bool {
        unsafe {
            libc::syscall(
                libc::SYS_ioprio_set,
                IOPRIO_WHO_PROCESS,
                pid as libc::c_long,
                value as libc::c_long,
            ) == 0
        }
    }
}

fn threads(root: &Path, pid: u32) -> Vec<u32> {
    let mut out: Vec<u32> = fs::read_dir(root.join(format!("proc/{pid}/task")))
        .map(|d| {
            d.flatten()
                .filter_map(|e| e.file_name().to_str()?.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    if out.is_empty() {
        out.push(pid);
    }
    out
}

/// Ce que le mode jeu a changé, pour tout remettre en place.
#[derive(Debug, Default)]
pub struct Session {
    pub games: Vec<Game>,
    profile: Option<Profile>,
    governor: bool,
    swappiness: Option<String>,
    memory: bool,
    /// (pid, tid, start_time du processus, nice d'origine)
    boosted: Vec<(u32, u32, Option<u64>, i32)>,
    /// (pid, start_time, nice d'origine par thread, ioprio d'origine)
    slowed: Vec<(u32, Option<u64>, Vec<(u32, i32)>, Option<i32>)>,
}

impl Session {
    pub fn actions(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.governor {
            out.push("governor");
        }
        if !self.boosted.is_empty() {
            out.push("priority");
        }
        if !self.slowed.is_empty() {
            out.push("background");
        }
        if self.memory {
            out.push("memory");
        }
        out
    }
}

fn write_rel(root: &Path, rel: &str, value: &str) -> bool {
    fs::write(root.join(rel), value).is_ok()
}

/// Début du mode jeu.
pub fn start(root: &Path, caps: &Caps, cpu_caps: &cpu::Caps) -> Session {
    let mut s = Session::default();
    let _ = fs::write(root.join(ACTIVE_FLAG), "1\n");
    let _ = fs::create_dir_all(root.join(INHIBIT));
    let _ = fs::write(
        root.join(INHIBIT)
            .join(format!("{}-gamemode", std::process::id())),
        ES_SYSTEM_DISPLAY,
    );
    if caps.governor && caps.on_ac && cpu_caps.current_profile() != Some(Profile::Performance) {
        let sys = Sys::at(root);
        if cpu::apply_profile(&sys, cpu_caps, Profile::Performance).is_ok() {
            s.profile = cpu_caps.current_profile();
            s.governor = true;
        }
    }
    if caps.memory {
        if let Ok(old) = fs::read_to_string(root.join(SWAPPINESS)) {
            let old = old.trim().to_string();
            if old.parse::<u32>().is_ok_and(|v| v > GAME_SWAPPINESS)
                && write_rel(root, SWAPPINESS, &GAME_SWAPPINESS.to_string())
            {
                s.swappiness = Some(old);
                s.memory = true;
            }
        }
        // Regroupe la mémoire libre : les grosses allocations du jeu passent.
        if write_rel(root, COMPACT, "1") {
            s.memory = true;
        }
    }
    s
}

/// Appelé à chaque tour tant que le mode jeu est actif : nouveaux jeux et
/// nouvelles tâches de fond.
pub fn refresh(root: &Path, os: &mut dyn Os, caps: &Caps, s: &mut Session, games: Vec<Game>) {
    let game_pids: Vec<u32> = games.iter().map(|g| g.pid).collect();
    if caps.priority {
        for game in &games {
            let start = start_time(root, game.pid);
            for tid in threads(root, game.pid) {
                if s.boosted.iter().any(|(_, t, _, _)| *t == tid) {
                    continue;
                }
                // Une priorité choisie par l'utilisateur est respectée.
                if os.nice(tid) == Some(0) && os.set_nice(tid, GAME_NICE) {
                    s.boosted.push((game.pid, tid, start, 0));
                }
            }
        }
    }
    for pid in background_pids(root, &game_pids) {
        if s.slowed.iter().any(|(p, _, _, _)| *p == pid) {
            continue;
        }
        let mut old = Vec::new();
        for tid in threads(root, pid) {
            if let Some(n) = os.nice(tid) {
                if n < BACKGROUND_NICE && os.set_nice(tid, BACKGROUND_NICE) {
                    old.push((tid, n));
                }
            }
        }
        let io = os.ioprio(pid);
        let io_old = (io != Some(IOPRIO_IDLE) && os.set_ioprio(pid, IOPRIO_IDLE))
            .then_some(io)
            .flatten();
        if !old.is_empty() || io_old.is_some() {
            s.slowed.push((pid, start_time(root, pid), old, io_old));
        }
    }
    s.games = games;
}

/// Fin du mode jeu : tout revient comme avant (pour les processus encore là).
pub fn stop(root: &Path, os: &mut dyn Os, cpu_caps: &cpu::Caps, s: Session) {
    if s.governor {
        let sys = Sys::at(root);
        let back = s.profile.filter(|p| *p != Profile::Performance);
        // Profil d'origine inconnu (personnalisé) : celui des Paramètres.
        let wanted = back.or_else(|| {
            let text = fs::read_to_string(root.join(CONFIG)).unwrap_or_default();
            cpu::config_value(&text, "CPU_PROFILE").and_then(Profile::parse)
        });
        if let Some(profile) = wanted.filter(|p| *p != Profile::Performance) {
            let _ = cpu::apply_profile(&sys, cpu_caps, profile);
        }
    }
    if let Some(old) = &s.swappiness {
        write_rel(root, SWAPPINESS, old);
    }
    for (pid, tid, start, nice) in s.boosted {
        if start_time(root, pid) == start && os.nice(tid) == Some(GAME_NICE) {
            os.set_nice(tid, nice);
        }
    }
    for (pid, start, old, io) in s.slowed {
        if start_time(root, pid) != start {
            continue;
        }
        for (tid, nice) in old {
            if os.nice(tid) == Some(BACKGROUND_NICE) {
                os.set_nice(tid, nice);
            }
        }
        if let Some(io) = io {
            os.set_ioprio(pid, io);
        }
    }
    let _ = fs::remove_file(root.join(ACTIVE_FLAG));
    let _ = fs::remove_file(
        root.join(INHIBIT)
            .join(format!("{}-gamemode", std::process::id())),
    );
}

/// État lu par `winlux-control` (ajouté à `cpu.txt`) et les Paramètres.
pub fn status_text(mode: Mode, caps: &Caps, session: Option<&Session>) -> String {
    let mut out = format!("game_mode={}\n", mode.key());
    out.push_str(&format!(
        "cap_game_governor={}\ncap_game_memory={}\ncap_game_priority={}\n",
        u8::from(caps.governor),
        u8::from(caps.memory),
        u8::from(caps.priority)
    ));
    match session {
        Some(s) => {
            let names: Vec<&str> = s
                .games
                .iter()
                .map(|g| g.name.as_str())
                .filter(|n| !n.is_empty())
                .collect();
            let clean: String = names
                .first()
                .copied()
                .unwrap_or("")
                .chars()
                .filter(|c| !c.is_control() && *c != '=')
                .collect();
            out.push_str(&format!(
                "game_active=1\ngame_name={clean}\ngame_count={}\ngame_actions={}\n",
                s.games.len(),
                s.actions().join(",")
            ));
        }
        None => out.push_str("game_active=0\n"),
    }
    out
}

fn write_state(root: &Path, text: &str) {
    let path = root.join(STATE);
    let tmp: PathBuf = path.with_extension("tmp");
    if fs::write(&tmp, text).is_ok() {
        let _ = fs::rename(tmp, path);
    }
}

fn already_running(root: &Path) -> bool {
    let pid_file = root.join("run/winlux/gamemode.pid");
    if let Ok(pid) = fs::read_to_string(&pid_file) {
        let pid = pid.trim();
        if pid != std::process::id().to_string() && root.join("proc").join(pid).exists() {
            return true;
        }
    }
    let _ = fs::write(pid_file, format!("{}\n", std::process::id()));
    false
}

pub fn daemon(root: &Path) -> i32 {
    let reg = root.join(REG_DIR);
    let _ = fs::create_dir_all(&reg);
    // Tout programme peut s'inscrire ; seul son propriétaire peut l'effacer.
    let _ = fs::set_permissions(&reg, std::os::unix::fs::PermissionsExt::from_mode(0o1777));
    if already_running(root) {
        return 0;
    }
    let mut os = Linux;
    let root_user = unsafe { libc::geteuid() } == 0;
    let mut session: Option<Session> = None;
    let mut last = String::new();
    loop {
        let mode = Mode::load(root);
        let cpu_caps = cpu::detect(&Sys::at(root));
        let caps = detect(root, &cpu_caps, root_user);
        let games = if mode == Mode::Auto {
            live_games(root)
        } else {
            Vec::new()
        };
        if games.is_empty() {
            if let Some(s) = session.take() {
                stop(root, &mut os, &cpu_caps, s);
                eprintln!("mode jeu : arr\u{ea}t\u{e9}");
            }
        } else {
            let s = session.get_or_insert_with(|| {
                eprintln!("mode jeu : {} d\u{e9}marr\u{e9}", games[0].name);
                start(root, &caps, &cpu_caps)
            });
            refresh(root, &mut os, &caps, s, games);
        }
        let text = status_text(mode, &caps, session.as_ref());
        if text != last {
            write_state(root, &text);
            last = text;
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

/// `winlux --gamemode daemon|status [RACINE]`.
pub fn cli(args: &[String]) -> i32 {
    let root = Path::new(args.get(1).map_or("/", String::as_str));
    match args.first().map(String::as_str) {
        Some("daemon") => daemon(root),
        Some("status") => {
            match fs::read_to_string(root.join(STATE)) {
                // Le réglage vient d'être changé : il prime sur l'état écrit
                // par le service, qui le reprendra au tour suivant.
                Ok(text) => {
                    println!("game_mode={}", Mode::load(root).key());
                    for line in text.lines().filter(|l| !l.starts_with("game_mode=")) {
                        println!("{line}");
                    }
                }
                Err(_) => {
                    let cpu_caps = cpu::detect(&Sys::at(root));
                    let caps = detect(root, &cpu_caps, unsafe { libc::geteuid() } == 0);
                    print!("{}", status_text(Mode::load(root), &caps, None));
                }
            }
            0
        }
        _ => {
            eprintln!("usage: winlux --gamemode daemon|status [RACINE]");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        nice: HashMap<u32, i32>,
        io: HashMap<u32, i32>,
    }
    impl Os for Fake {
        fn nice(&self, tid: u32) -> Option<i32> {
            Some(*self.nice.get(&tid).unwrap_or(&0))
        }
        fn set_nice(&mut self, tid: u32, nice: i32) -> bool {
            self.nice.insert(tid, nice);
            true
        }
        fn ioprio(&self, pid: u32) -> Option<i32> {
            Some(*self.io.get(&pid).unwrap_or(&0))
        }
        fn set_ioprio(&mut self, pid: u32, value: i32) -> bool {
            self.io.insert(pid, value);
            true
        }
    }

    fn tree(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("winlux-game-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(REG_DIR)).unwrap();
        fs::create_dir_all(root.join("proc/sys/vm")).unwrap();
        fs::create_dir_all(root.join("etc/winlux")).unwrap();
        root
    }

    fn process(root: &Path, pid: u32, ppid: u32, name: &str, start: u64, tids: &[u32]) {
        let dir = root.join(format!("proc/{pid}"));
        fs::create_dir_all(dir.join("task")).unwrap();
        fs::write(dir.join("comm"), format!("{name}\n")).unwrap();
        // pid (comm) state ppid ... champ 22 = start_time
        let mut fields = vec!["S".to_string(), ppid.to_string()];
        fields.extend((0..17).map(|_| "0".to_string()));
        fields.push(start.to_string());
        fs::write(
            dir.join("stat"),
            format!("{pid} ({name}) {}\n", fields.join(" ")),
        )
        .unwrap();
        for t in tids {
            fs::create_dir_all(dir.join(format!("task/{t}"))).unwrap();
        }
    }

    #[test]
    fn registrations_follow_process_life() {
        let root = tree("reg");
        process(&root, 40, 1, "winlux", 777, &[40]);
        fs::write(root.join(REG_DIR).join("40"), "777 jeu.exe\n").unwrap();
        fs::write(root.join(REG_DIR).join("41"), "5 parti.exe\n").unwrap();
        fs::write(root.join(REG_DIR).join("junk"), "x").unwrap();
        assert_eq!(
            live_games(&root),
            vec![Game {
                pid: 40,
                name: "jeu.exe".into()
            }]
        );
        assert!(!root.join(REG_DIR).join("41").exists());
        assert!(!root.join(REG_DIR).join("junk").exists());
        // Numéro réutilisé par un autre processus : l'inscription tombe.
        process(&root, 40, 1, "bash", 900, &[40]);
        assert!(live_games(&root).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn background_includes_children_not_games() {
        let root = tree("bg");
        process(&root, 10, 1, "winlux-backup", 1, &[10]);
        process(&root, 11, 10, "zstd", 2, &[11, 12]);
        process(&root, 20, 1, "zstd", 3, &[20]);
        process(&root, 30, 1, "apt-get", 4, &[30]);
        assert_eq!(background_pids(&root, &[30]), vec![10, 11]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn start_refresh_stop_restores_everything() {
        let root = tree("cycle");
        fs::write(root.join(SWAPPINESS), "60\n").unwrap();
        fs::write(root.join(COMPACT), "").unwrap();
        process(&root, 40, 1, "winlux", 777, &[40, 41]);
        process(&root, 10, 1, "winlux-update", 1, &[10]);
        let mut os = Fake::default();
        os.nice.insert(41, 3); // priorité choisie par l'utilisateur
        os.nice.insert(10, 5);
        let caps = Caps {
            governor: false,
            on_ac: true,
            memory: true,
            priority: true,
        };
        let cpu_caps = cpu::Caps::default();
        let mut s = start(&root, &caps, &cpu_caps);
        assert!(root.join(ACTIVE_FLAG).exists());
        assert_eq!(fs::read_to_string(root.join(SWAPPINESS)).unwrap(), "10");
        let games = vec![Game {
            pid: 40,
            name: "jeu.exe".into(),
        }];
        refresh(&root, &mut os, &caps, &mut s, games);
        assert_eq!(os.nice[&40], GAME_NICE);
        assert_eq!(os.nice[&41], 3);
        assert_eq!(os.nice[&10], BACKGROUND_NICE);
        assert_eq!(os.io[&10], IOPRIO_IDLE);
        assert_eq!(s.actions(), vec!["priority", "background", "memory"]);
        let text = status_text(Mode::Auto, &caps, Some(&s));
        assert!(text.contains("game_active=1\ngame_name=jeu.exe\n"));
        assert!(text.contains("cap_game_governor=0\n"));
        stop(&root, &mut os, &cpu_caps, s);
        assert_eq!(os.nice[&40], 0);
        assert_eq!(os.nice[&10], 5);
        assert_eq!(os.io[&10], 0);
        assert_eq!(fs::read_to_string(root.join(SWAPPINESS)).unwrap(), "60");
        assert!(!root.join(ACTIVE_FLAG).exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn mode_off_and_status() {
        let root = tree("mode");
        assert_eq!(Mode::load(&root), Mode::Auto);
        fs::write(root.join(CONFIG), "CPU_PROFILE=balanced\nGAME_MODE=off\n").unwrap();
        assert_eq!(Mode::load(&root), Mode::Off);
        let text = status_text(Mode::Off, &Caps::default(), None);
        assert_eq!(
            text,
            "game_mode=off\ncap_game_governor=0\ncap_game_memory=0\ncap_game_priority=0\ngame_active=0\n"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
