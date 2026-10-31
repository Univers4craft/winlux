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
static JOURNAL_FILE: OnceLock<Mutex<Option<std::fs::File>>> = OnceLock::new();

fn journal_file() -> &'static Mutex<Option<std::fs::File>> {
    JOURNAL_FILE.get_or_init(|| {
        let dir = "/run/winlux/journal";
        let _ = std::fs::create_dir_all(dir);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("{dir}/winlux.log"));
        Mutex::new(file.ok())
    })
}

fn config() -> &'static Config {
    CONFIG.get_or_init(|| {
        let raw = std::env::var("WINLUX_LOG").unwrap_or_default();
        let raw = raw.trim();

        let journal = std::env::var_os("WINLUX_JOURNAL").is_none_or(|v| v != "0");

        if std::env::args_os().any(|a| a == "--verbose") || raw.eq_ignore_ascii_case("debug") {
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

fn fmt_is_json(s: &str) -> bool {
    s.eq_ignore_ascii_case("json")
}

pub fn enabled(level: Level) -> bool {
    level <= config().level
}

fn timestamp() -> String {
    use std::time::SystemTime;
    let d = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn write_to_journal(level: Level, target: &str, message: &str) {
    if let Ok(mut guard) = journal_file().lock() {
        if let Some(ref mut f) = *guard {
            let _ = writeln!(
                f,
                "{} [{}] {}: {}",
                timestamp(),
                level.as_str(),
                target,
                message
            );
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
