use std::io::Write;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Level {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warn => "warn",
            Level::Info => "info",
            Level::Debug => "debug",
            Level::Trace => "trace",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Text,
    Json,
}

struct Config {
    level: Level,
    format: Format,
    journal: bool,
}

static CONFIG: OnceLock<Config> = OnceLock::new();
static LOG_FILE: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
static JOURNAL_FILE: OnceLock<Mutex<Option<std::fs::File>>> = OnceLock::new();

const MAX_LOG_SIZE: u64 = 10 * 1024 * 1024;

/// Taille au-delà de laquelle le journal du compte passe en `.old` (#1904) :
/// il vit dans /run, donc en mémoire vive, et une trace de plusieurs Go
/// finissait par la remplir.
const MAX_JOURNAL_SIZE: u64 = 32 * 1024 * 1024;

/// Écritures entre deux vérifications de la taille du journal.
const JOURNAL_CHECK_EVERY: u32 = 512;

static JOURNAL_WRITES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn open_journal(path: &std::path::Path) -> Option<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .ok()
}

fn journal_file() -> &'static Mutex<Option<std::fs::File>> {
    JOURNAL_FILE.get_or_init(|| {
        // Journal propre au compte (#633) : aucun autre compte ne peut
        // l'effacer ni y glisser de fausses lignes.
        Mutex::new(crate::journal::file("winlux.log").and_then(|p| open_journal(&p)))
    })
}

/// Rotation du journal partagé par tous les processus du compte (#1904).
/// Seul le processus dont le fichier est encore `winlux.log` le renomme en
/// `winlux.log.old` ; un processus qui tient déjà un fichier renommé par un
/// autre rouvre simplement le nouveau `winlux.log`. Au plus deux fichiers
/// d'environ MAX_JOURNAL_SIZE restent donc en mémoire.
fn rotate_journal(slot: &mut Option<std::fs::File>, path: &std::path::Path, limit: u64) {
    use std::os::unix::fs::MetadataExt;
    let Some(file) = slot.as_ref() else {
        return;
    };
    let Ok(ours) = file.metadata() else {
        return;
    };
    if ours.len() < limit {
        return;
    }
    let current = std::fs::symlink_metadata(path).ok();
    if current
        .as_ref()
        .is_some_and(|m| m.dev() == ours.dev() && m.ino() == ours.ino())
    {
        let mut old = path.as_os_str().to_owned();
        old.push(".old");
        let _ = std::fs::rename(path, old);
    }
    if let Some(fresh) = open_journal(path) {
        *slot = Some(fresh);
    }
}

fn config() -> &'static Config {
    CONFIG.get_or_init(|| {
        let raw = std::env::var("WINLUX_LOG").unwrap_or_default();
        let raw = raw.trim();

        let journal = std::env::var_os("WINLUX_JOURNAL").is_none_or(|v| v != "0");

        // Seules les options placées avant le programme comptent : un
        // « --verbose » destiné au .exe (claude.exe --verbose) ne doit pas
        // noyer sa sortie sous le journal de Winlux.
        let own_verbose = std::env::args_os()
            .skip(1)
            .take_while(|a| a.to_string_lossy().starts_with('-'))
            .any(|a| a == "--verbose");
        if own_verbose || raw.eq_ignore_ascii_case("debug") {
            return Config {
                level: Level::Debug,
                format: Format::Text,
                journal,
            };
        }

        if raw.eq_ignore_ascii_case("trace") || std::env::var_os("WINLUX_TRACE").is_some() {
            return Config {
                level: Level::Trace,
                format: Format::Text,
                journal,
            };
        }

        if raw.eq_ignore_ascii_case("json") {
            return Config {
                level: Level::Debug,
                format: Format::Json,
                journal,
            };
        }

        let (level, format) = match raw.split_once(',') {
            Some((lvl, fmt)) => (lvl.trim(), fmt.trim()),
            None => (raw, ""),
        };

        let level = match level.to_ascii_lowercase().as_str() {
            "error" => Level::Error,
            "warn" => Level::Warn,
            "info" => Level::Info,
            "debug" | "verbose" => Level::Debug,
            "trace" => Level::Trace,
            _ => Level::Warn,
        };

        let format = if fmt_is_json(format) {
            Format::Json
        } else {
            Format::Text
        };

        Config {
            level,
            format,
            journal,
        }
    })
}

fn log_file() -> &'static Option<Mutex<std::fs::File>> {
    LOG_FILE.get_or_init(|| {
        let raw = std::env::var("WINLUX_LOG_FILE").ok()?;
        let path = if raw.eq_ignore_ascii_case("auto") || raw == "1" {
            crate::paths::data_dir()?.join("winlux.log")
        } else {
            std::path::PathBuf::from(raw)
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Rotation : si le fichier dépasse 10 Mo, on le renomme en .old
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() > MAX_LOG_SIZE {
                let old = path.with_extension("log.old");
                let _ = std::fs::rename(&path, old);
            }
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()?;
        Some(Mutex::new(file))
    })
}

fn fmt_is_json(s: &str) -> bool {
    s.eq_ignore_ascii_case("json")
}

pub fn enabled(level: Level) -> bool {
    level <= config().level
}

fn timestamp() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    let millis = d.subsec_millis();
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    let (y, mo, day) = days_to_ymd(days);
    format!("{y:04}-{mo:02}-{day:02}T{h:02}:{m:02}:{s:02}.{millis:03}Z")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut y = 1970;
    loop {
        let year_days = if is_leap(y) { 366 } else { 365 };
        if days < year_days {
            break;
        }
        days -= year_days;
        y += 1;
    }
    let leap = is_leap(y);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut mo = 0;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        mo += 1;
    }
    (y, mo + 1, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

/// Programme à l'origine des lignes du journal, « calc.exe[1234] » ou
/// « winlux[1234] » pour les services : l'Observateur d'événements s'en sert
/// comme source et pour ranger les événements des applications .exe.
fn journal_origin() -> &'static str {
    static ORIGIN: OnceLock<String> = OnceLock::new();
    ORIGIN.get_or_init(|| {
        let name = std::env::args_os()
            .nth(1)
            .filter(|a| !a.to_string_lossy().starts_with("--"))
            .and_then(|a| {
                std::path::Path::new(&a)
                    .file_name()
                    .map(|n| n.to_string_lossy().replace([' ', '[', ']'], "_"))
            })
            .unwrap_or_else(|| "winlux".into());
        format!("{name}[{}]", std::process::id())
    })
}

/// Étape notée dans le journal quel que soit le niveau choisi (lancement et
/// fin d'un programme) : `winlux-report` en tire la chronologie d'un plantage.
pub fn journal_event(target: &str, message: &str) {
    if cfg!(test) || !config().journal {
        return;
    }
    write_to_journal(Level::Info, target, message);
}

fn write_to_journal(level: Level, target: &str, message: &str) {
    if let Ok(mut guard) = journal_file().lock() {
        let n = JOURNAL_WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if n.is_multiple_of(JOURNAL_CHECK_EVERY) {
            if let Some(path) = crate::journal::file("winlux.log") {
                rotate_journal(&mut guard, &path, MAX_JOURNAL_SIZE);
            }
        }
        if let Some(ref mut f) = *guard {
            let _ = writeln!(
                f,
                "{} [{}] {} {}: {}",
                timestamp(),
                level.as_str(),
                journal_origin(),
                target,
                message
            );
        }
    }
}

/// Copie dans WINLUX_LOG_FILE (et le journal) d'une ligne déjà écrite sur
/// la sortie d'erreur. Les enfants GPU de Chromium sont lancés sans sortie
/// d'erreur : sans cette copie, la cause de leur arrêt est perdue (#1565).
pub fn copy_to_file(level: Level, target: &str, message: &str) {
    if config().journal {
        write_to_journal(level, target, message);
    }
    if let Some(file_mutex) = log_file() {
        if let Ok(mut f) = file_mutex.lock() {
            let ts = timestamp();
            let _ = writeln!(f, "{}", file_line(&ts, level, target, message));
        }
    }
}

pub fn emit(level: Level, target: &str, message: &std::fmt::Arguments<'_>) {
    if !enabled(level) {
        return;
    }
    let cfg = config();
    let msg_str = message.to_string();

    if cfg.journal {
        write_to_journal(level, target, &msg_str);
    }

    let stderr = std::io::stderr();
    let mut w = stderr.lock();
    match cfg.format {
        Format::Text => {
            let _ = writeln!(w, "winlux[{}/{}]: {}", level.as_str(), target, msg_str);
        }
        Format::Json => {
            let _ = write!(
                w,
                "{{\"level\":\"{}\",\"target\":\"{}\",\"msg\":",
                level.as_str(),
                target
            );
            write_json_string(&mut w, &msg_str);
            let _ = writeln!(w, "}}");
        }
    }
    drop(w);

    // Écriture dans le fichier de log si configuré
    if let Some(file_mutex) = log_file() {
        if let Ok(mut f) = file_mutex.lock() {
            let ts = timestamp();
            let _ = writeln!(f, "{}", file_line(&ts, level, target, message));
        }
    }
}

/// Ligne de WINLUX_LOG_FILE. Programme et pid : tous les processus d'une
/// appli (enfants GPU de Chromium, lancés sans sortie d'erreur) écrivent
/// dans le même fichier et doivent rester distincts (#1313).
fn file_line(ts: &str, level: Level, target: &str, message: impl std::fmt::Display) -> String {
    format!(
        "{ts} [{}] {} {target}: {message}",
        level.as_str(),
        journal_origin()
    )
}

fn write_json_string(w: &mut impl Write, s: &str) {
    let _ = w.write_all(b"\"");
    for c in s.bytes() {
        match c {
            b'"' => {
                let _ = w.write_all(b"\\\"");
            }
            b'\\' => {
                let _ = w.write_all(b"\\\\");
            }
            b'\n' => {
                let _ = w.write_all(b"\\n");
            }
            b'\r' => {
                let _ = w.write_all(b"\\r");
            }
            b'\t' => {
                let _ = w.write_all(b"\\t");
            }
            0x00..=0x1f => {
                let _ = write!(w, "\\u{c:04x}");
            }
            _ => {
                let _ = w.write_all(&[c]);
            }
        }
    }
    let _ = w.write_all(b"\"");
}

#[macro_export]
macro_rules! log_error {
    ($target:expr, $($arg:tt)*) => {
        $crate::log::emit($crate::log::Level::Error, $target, &format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_warn {
    ($target:expr, $($arg:tt)*) => {
        if $crate::log::enabled($crate::log::Level::Warn) {
            $crate::log::emit($crate::log::Level::Warn, $target, &format_args!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_info {
    ($target:expr, $($arg:tt)*) => {
        if $crate::log::enabled($crate::log::Level::Info) {
            $crate::log::emit($crate::log::Level::Info, $target, &format_args!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_debug {
    ($target:expr, $($arg:tt)*) => {
        if $crate::log::enabled($crate::log::Level::Debug) {
            $crate::log::emit($crate::log::Level::Debug, $target, &format_args!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        if $crate::log::enabled($crate::log::Level::Trace) {
            $crate::log::emit($crate::log::Level::Trace, "runtime", &format_args!($($arg)*));
        }
    };
}

#[cfg(test)]
mod file_line_tests {
    use super::*;

    #[test]
    fn journal_fichier_porte_le_programme_et_le_pid() {
        let line = file_line(
            "2026-11-09T10:00:00.000Z",
            Level::Trace,
            "runtime",
            "chargement",
        );
        let pid = format!("[{}]", std::process::id());
        assert!(
            line.starts_with("2026-11-09T10:00:00.000Z [trace] "),
            "{line}"
        );
        assert!(line.contains(&pid), "{line}");
        assert!(line.ends_with(" runtime: chargement"), "{line}");
    }

    /// #1904 : au-delà de la limite, le journal passe en `.old` ; un second
    /// processus qui tient l'ancien fichier rouvre le nouveau sans rien
    /// renommer de plus.
    #[test]
    fn journal_tourne_au_dela_de_la_limite() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("winlux-journal-1904-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("winlux.log");
        let mut a = open_journal(&path);
        let mut b = open_journal(&path);
        a.as_mut().unwrap().write_all(&[b'x'; 200]).unwrap();
        rotate_journal(&mut a, &path, 100);
        let old = dir.join("winlux.log.old");
        assert_eq!(std::fs::metadata(&old).unwrap().len(), 200);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
        // b tient toujours l'ancien fichier : il rouvre, sans écraser .old.
        rotate_journal(&mut b, &path, 100);
        assert_eq!(std::fs::metadata(&old).unwrap().len(), 200);
        b.as_mut().unwrap().write_all(b"ligne\n").unwrap();
        a.as_mut().unwrap().write_all(b"ligne\n").unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 12);
        // Sous la limite : rien ne bouge.
        rotate_journal(&mut a, &path, 100);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 12);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
