//! pdh.dll : compteurs de performance (#26). Requêtes, compteurs et valeurs
//! formatées, lus dans /proc : processeur (total et par cœur), mémoire
//! (disponible, engagée), processus et temps de fonctionnement du système.
//! Un compteur inconnu est refusé avec PDH_CSTATUS_NO_OBJECT ou
//! PDH_CSTATUS_NO_COUNTER, comme un objet absent sous Windows.

use super::lock;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const OK: u32 = 0;
const PDH_CSTATUS_NO_OBJECT: u32 = 0xC000_0BB8;
const PDH_CSTATUS_NO_COUNTER: u32 = 0xC000_0BB9;
const PDH_INVALID_HANDLE: u32 = 0xC000_0BBC;
const PDH_INVALID_ARGUMENT: u32 = 0xC000_0BBD;
const PDH_INVALID_DATA: u32 = 0xC000_0BC6;
const PDH_NO_DATA: u32 = 0x8000_07D5;
const PDH_FMT_LONG: u32 = 0x100;
const PDH_FMT_DOUBLE: u32 = 0x200;
const PDH_FMT_LARGE: u32 = 0x400;
const PDH_FMT_NOCAP100: u32 = 0x8000;
/// PERF_COUNTER_RAWCOUNT et PERF_100NSEC_TIMER_INV, renvoyés dans lpdwType.
const TYPE_RAW: u32 = 0x0001_0000;
const TYPE_RATE: u32 = 0x2541_0500;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    /// % Processor Time : None = _Total, Some(n) = cœur n.
    Cpu(Option<usize>),
    AvailableBytes(u64),
    CommittedBytes,
    CommittedPercent,
    Processes,
    Uptime,
}

struct Counter {
    query: usize,
    kind: Kind,
    /// Deux derniers relevés (occupé, total) pour les taux.
    samples: Vec<(u64, u64)>,
    value: Option<f64>,
}

#[derive(Default)]
struct State {
    next: usize,
    queries: Vec<usize>,
    counters: HashMap<usize, Counter>,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(State {
            next: 0x5D00,
            ..State::default()
        })
    })
}

/// Chemin « \\machine\Objet(instance)\Compteur » → compteur connu.
fn parse(path: &str) -> Result<Kind, u32> {
    let p = path.trim();
    let p = match p.strip_prefix("\\\\") {
        Some(rest) => rest
            .find('\\')
            .map(|i| &rest[i..])
            .ok_or(PDH_CSTATUS_NO_OBJECT)?,
        None => p,
    };
    let p = p.strip_prefix('\\').ok_or(PDH_CSTATUS_NO_OBJECT)?;
    let (object, counter) = p.rsplit_once('\\').ok_or(PDH_CSTATUS_NO_OBJECT)?;
    let (object, instance) = match object.split_once('(') {
        Some((o, rest)) => (o, Some(rest.trim_end_matches(')'))),
        None => (object, None),
    };
    let object = object.to_ascii_lowercase();
    let counter = counter.to_ascii_lowercase();
    match object.as_str() {
        "processor" | "processor information" => {
            let core = match instance {
                None | Some("_Total") | Some("_total") | Some("0,_Total") => None,
                Some(n) => Some(
                    n.rsplit(',')
                        .next()
                        .and_then(|n| n.parse().ok())
                        .ok_or(PDH_CSTATUS_NO_OBJECT)?,
                ),
            };
            match counter.as_str() {
                "% processor time" | "% processor utility" => Ok(Kind::Cpu(core)),
                _ => Err(PDH_CSTATUS_NO_COUNTER),
            }
        }
        "memory" => match counter.as_str() {
            "available bytes" => Ok(Kind::AvailableBytes(1)),
            "available kbytes" => Ok(Kind::AvailableBytes(1024)),
            "available mbytes" => Ok(Kind::AvailableBytes(1024 * 1024)),
            "committed bytes" => Ok(Kind::CommittedBytes),
            "% committed bytes in use" => Ok(Kind::CommittedPercent),
            _ => Err(PDH_CSTATUS_NO_COUNTER),
        },
        "system" => match counter.as_str() {
            "processes" => Ok(Kind::Processes),
            "system up time" => Ok(Kind::Uptime),
            _ => Err(PDH_CSTATUS_NO_COUNTER),
        },
        _ => Err(PDH_CSTATUS_NO_OBJECT),
    }
}

/// (occupé, total) en jiffies pour la ligne « cpu » ou « cpuN » de /proc/stat.
fn cpu_times(stat: &str, core: Option<usize>) -> Option<(u64, u64)> {
    let label = core.map_or_else(|| "cpu".to_string(), |n| format!("cpu{n}"));
    let line = stat
        .lines()
        .find(|l| l.split_whitespace().next() == Some(label.as_str()))?;
    let v: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|x| x.parse().ok())
        .collect();
    if v.len() < 4 {
        return None;
    }
    let total: u64 = v.iter().take(8).sum();
    let idle = v[3] + v.get(4).copied().unwrap_or(0);
    Some((total - idle, total))
}

fn meminfo(key: &str) -> Option<u64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = text
        .lines()
        .find(|l| l.starts_with(key) && l[key.len()..].starts_with(':'))?;
    line.split_whitespace()
        .nth(1)?
        .parse::<u64>()
        .ok()
        .map(|kb| kb * 1024)
}

/// Relève le compteur ; renvoie la valeur si elle est calculable.
fn sample(c: &mut Counter) -> Option<f64> {
    match c.kind {
        Kind::Cpu(core) => {
            let stat = std::fs::read_to_string("/proc/stat").ok()?;
            let now = cpu_times(&stat, core)?;
            c.samples.push(now);
            if c.samples.len() > 2 {
                c.samples.remove(0);
            }
            match c.samples[..] {
                [(b0, t0), (b1, t1)] if t1 > t0 => {
                    Some((b1.saturating_sub(b0)) as f64 * 100.0 / (t1 - t0) as f64)
                }
                [_, _] => Some(0.0),
                _ => None,
            }
        }
        Kind::AvailableBytes(unit) => meminfo("MemAvailable").map(|b| (b / unit) as f64),
        Kind::CommittedBytes => meminfo("Committed_AS").map(|b| b as f64),
        Kind::CommittedPercent => {
            let (used, limit) = (meminfo("Committed_AS")?, meminfo("CommitLimit")?);
            Some(used as f64 * 100.0 / limit.max(1) as f64)
        }
        Kind::Processes => std::fs::read_dir("/proc").ok().map(|d| {
            d.flatten()
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .bytes()
                        .all(|b| b.is_ascii_digit())
                })
                .count() as f64
        }),
        Kind::Uptime => std::fs::read_to_string("/proc/uptime")
            .ok()?
            .split_whitespace()
            .next()?
            .parse()
            .ok(),
    }
}

unsafe fn wide(p: *const u16) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let mut n = 0;
    while *p.add(n) != 0 {
        n += 1;
    }
    Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)))
}

unsafe fn ansi(p: *const u8) -> Option<String> {
    if p.is_null() {
        return None;
    }
    Some(
        std::ffi::CStr::from_ptr(p.cast())
            .to_string_lossy()
            .into_owned(),
    )
}

unsafe extern "win64" fn open_query(_source: usize, _user: usize, out: *mut usize) -> u32 {
    if out.is_null() {
        return PDH_INVALID_ARGUMENT;
    }
    let mut s = lock(state());
    s.next += 4;
    let h = s.next;
    s.queries.push(h);
    *out = h;
    OK
}

fn add(query: usize, path: Option<String>, out: *mut usize) -> u32 {
    let Some(path) = path else {
        return PDH_INVALID_ARGUMENT;
    };
    if out.is_null() {
        return PDH_INVALID_ARGUMENT;
    }
    let mut s = lock(state());
    if !s.queries.contains(&query) {
        return PDH_INVALID_HANDLE;
    }
    let kind = match parse(&path) {
        Ok(k) => k,
        Err(e) => return e,
    };
    s.next += 4;
    let h = s.next;
    s.counters.insert(
        h,
        Counter {
            query,
            kind,
            samples: Vec::new(),
            value: None,
        },
    );
    // SAFETY: vérifié non nul ci-dessus.
    unsafe { *out = h };
    OK
}

unsafe extern "win64" fn add_counter_w(
    query: usize,
    path: *const u16,
    _user: usize,
    out: *mut usize,
) -> u32 {
    add(query, wide(path), out)
}

unsafe extern "win64" fn add_counter_a(
    query: usize,
    path: *const u8,
    _user: usize,
    out: *mut usize,
) -> u32 {
    add(query, ansi(path), out)
}

unsafe extern "win64" fn collect_query_data(query: usize) -> u32 {
    let mut s = lock(state());
    if !s.queries.contains(&query) {
        return PDH_INVALID_HANDLE;
    }
    let mut any = false;
    for c in s.counters.values_mut().filter(|c| c.query == query) {
        c.value = sample(c);
        any = true;
    }
    if any {
        OK
    } else {
        PDH_NO_DATA
    }
}

/// PDH_FMT_COUNTERVALUE { CStatus: u32, (4 octets), union { long, double, i64 … } }.
unsafe extern "win64" fn get_formatted_counter_value(
    counter: usize,
    format: u32,
    kind: *mut u32,
    out: *mut u8,
) -> u32 {
    if out.is_null() {
        return PDH_INVALID_ARGUMENT;
    }
    let s = lock(state());
    let Some(c) = s.counters.get(&counter) else {
        return PDH_INVALID_HANDLE;
    };
    if !kind.is_null() {
        *kind = if matches!(c.kind, Kind::Cpu(_)) {
            TYPE_RATE
        } else {
            TYPE_RAW
        };
    }
    let Some(mut v) = c.value else {
        std::ptr::write_unaligned(out as *mut u32, PDH_INVALID_DATA);
        return PDH_INVALID_DATA;
    };
    if matches!(c.kind, Kind::Cpu(_)) && format & PDH_FMT_NOCAP100 == 0 {
        v = v.min(100.0);
    }
    std::ptr::write_unaligned(out as *mut u32, OK);
    let value = out.add(8);
    match format & (PDH_FMT_LONG | PDH_FMT_DOUBLE | PDH_FMT_LARGE) {
        PDH_FMT_LONG => std::ptr::write_unaligned(value as *mut i32, v.round() as i32),
        PDH_FMT_LARGE => std::ptr::write_unaligned(value as *mut i64, v.round() as i64),
        PDH_FMT_DOUBLE => std::ptr::write_unaligned(value as *mut f64, v),
        _ => return PDH_INVALID_ARGUMENT,
    }
    OK
}

unsafe extern "win64" fn remove_counter(counter: usize) -> u32 {
    if lock(state()).counters.remove(&counter).is_some() {
        OK
    } else {
        PDH_INVALID_HANDLE
    }
}

unsafe extern "win64" fn close_query(query: usize) -> u32 {
    let mut s = lock(state());
    let Some(i) = s.queries.iter().position(|&q| q == query) else {
        return PDH_INVALID_HANDLE;
    };
    s.queries.remove(i);
    s.counters.retain(|_, c| c.query != query);
    OK
}

unsafe extern "win64" fn validate_path_w(path: *const u16) -> u32 {
    wide(path).map_or(PDH_INVALID_ARGUMENT, |p| parse(&p).err().unwrap_or(OK))
}

unsafe extern "win64" fn validate_path_a(path: *const u8) -> u32 {
    ansi(path).map_or(PDH_INVALID_ARGUMENT, |p| parse(&p).err().unwrap_or(OK))
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "PdhOpenQueryW" => fn open_query, "PdhOpenQueryA" => fn open_query, "PdhOpenQuery" => fn open_query,
        "PdhAddCounterW" => fn add_counter_w, "PdhAddCounterA" => fn add_counter_a,
        "PdhAddEnglishCounterW" => fn add_counter_w, "PdhAddEnglishCounterA" => fn add_counter_a,
        "PdhCollectQueryData" => fn collect_query_data,
        "PdhGetFormattedCounterValue" => fn get_formatted_counter_value,
        "PdhRemoveCounter" => fn remove_counter, "PdhCloseQuery" => fn close_query,
        "PdhValidatePathW" => fn validate_path_w, "PdhValidatePathA" => fn validate_path_a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chemins_reconnus() {
        assert_eq!(
            parse("\\Processor(_Total)\\% Processor Time"),
            Ok(Kind::Cpu(None))
        );
        assert_eq!(
            parse("\\\\PC\\Processor(3)\\% Processor Time"),
            Ok(Kind::Cpu(Some(3)))
        );
        assert_eq!(
            parse("\\Processor Information(0,2)\\% Processor Utility"),
            Ok(Kind::Cpu(Some(2)))
        );
        assert_eq!(
            parse("\\Memory\\Available MBytes"),
            Ok(Kind::AvailableBytes(1024 * 1024))
        );
        assert_eq!(parse("\\Memory\\Pages/sec"), Err(PDH_CSTATUS_NO_COUNTER));
        assert_eq!(
            parse("\\GPU Engine(*)\\Utilization Percentage"),
            Err(PDH_CSTATUS_NO_OBJECT)
        );
    }

    #[test]
    fn processeur_par_difference() {
        let stat = "cpu  100 0 100 800 0 0 0 0 0 0\ncpu0 50 0 50 400 0 0 0 0 0 0\n";
        assert_eq!(cpu_times(stat, None), Some((200, 1000)));
        assert_eq!(cpu_times(stat, Some(0)), Some((100, 500)));
        assert_eq!(cpu_times(stat, Some(1)), None);
    }

    #[test]
    fn requete_complete() {
        unsafe {
            let mut q = 0usize;
            assert_eq!(open_query(0, 0, &mut q), OK);
            let path: Vec<u16> = "\\Memory\\Available KBytes\0".encode_utf16().collect();
            let mut c = 0usize;
            assert_eq!(add_counter_w(q, path.as_ptr(), 0, &mut c), OK);
            let mut value = [0u8; 16];
            assert_eq!(
                get_formatted_counter_value(
                    c,
                    PDH_FMT_LARGE,
                    std::ptr::null_mut(),
                    value.as_mut_ptr()
                ),
                PDH_INVALID_DATA
            );
            assert_eq!(collect_query_data(q), OK);
            assert_eq!(
                get_formatted_counter_value(
                    c,
                    PDH_FMT_LARGE,
                    std::ptr::null_mut(),
                    value.as_mut_ptr()
                ),
                OK
            );
            assert!(i64::from_le_bytes(value[8..16].try_into().unwrap()) > 0);
            let bad: Vec<u16> = "\\Nope\\X\0".encode_utf16().collect();
            assert_eq!(
                add_counter_w(q, bad.as_ptr(), 0, &mut c),
                PDH_CSTATUS_NO_OBJECT
            );
            assert_eq!(close_query(q), OK);
            assert_eq!(collect_query_data(q), PDH_INVALID_HANDLE);
        }
    }
}
