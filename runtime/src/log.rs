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
    fn as_str(self) -> &'static str {
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
}

static CONFIG: OnceLock<Config> = OnceLock::new();
static LOG_FILE: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();

const MAX_LOG_SIZE: u64 = 10 * 1024 * 1024;

fn config() -> &'static Config {
    CONFIG.get_or_init(|| {
        let raw = std::env::var("WINLUX_LOG").unwrap_or_default();
        let raw = raw.trim();

        if std::env::args_os().any(|a| a == "--verbose") || raw.eq_ignore_ascii_case("debug") {
            return Config {
                level: Level::Debug,
                format: Format::Text,
            };
        }

        if raw.eq_ignore_ascii_case("trace") || std::env::var_os("WINLUX_TRACE").is_some() {
            return Config {
                level: Level::Trace,
                format: Format::Text,
            };
        }

        if raw.eq_ignore_ascii_case("json") {
            return Config {
                level: Level::Debug,
                format: Format::Json,
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

        Config { level, format }
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
    // UTC timestamp : secondes depuis l'epoch, converti en ISO 8601 simplifié
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    // Calcul simplifié de la date à partir du nombre de jours depuis 1970
    let (y, mo, day) = days_to_ymd(days);
    format!("{y:04}-{mo:02}-{day:02}T{h:02}:{m:02}:{s:02}.{millis:03}Z")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Algorithme de conversion jours-depuis-epoch vers Y/M/D
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
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn emit(level: Level, target: &str, message: &std::fmt::Arguments<'_>) {
    if !enabled(level) {
        return;
    }
    let cfg = config();
    let stderr = std::io::stderr();
    let mut w = stderr.lock();
    match cfg.format {
        Format::Text => {
            let _ = writeln!(w, "winlux[{}/{}]: {}", level.as_str(), target, message);
        }
        Format::Json => {
            let msg = message.to_string();
            let _ = write!(
                w,
                "{{\"level\":\"{}\",\"target\":\"{}\",\"msg\":",
                level.as_str(),
                target
            );
            write_json_string(&mut w, &msg);
            let _ = writeln!(w, "}}");
        }
    }
    drop(w);

    // Écriture dans le fichier de log si configuré
    if let Some(file_mutex) = log_file() {
        if let Ok(mut f) = file_mutex.lock() {
            let ts = timestamp();
            let _ = writeln!(f, "{} [{}] {}: {}", ts, level.as_str(), target, message);
        }
    }
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
