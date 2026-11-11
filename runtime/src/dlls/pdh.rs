//! pdh.dll : compteurs de performance (#26). Requêtes, compteurs et valeurs
//! formatées, lus dans /proc : processeur (total et par cœur), mémoire
//! (disponible, engagée), processus et temps de fonctionnement du système.
//! Un compteur inconnu est refusé avec PDH_CSTATUS_NO_OBJECT ou
//! PDH_CSTATUS_NO_COUNTER, comme un objet absent sous Windows.

#![allow(non_snake_case)]

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
const PDH_FMT_NOSCALE: u32 = 0x1000;
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
    /// Chemin donné à PdhAddCounter, valeur dwUserData, facteur d'échelle
    /// (PdhSetCounterScaleFactor, puissance de dix).
    path: String,
    user: usize,
    scale: i32,
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

fn add(query: usize, path: Option<String>, user: usize, out: *mut usize) -> u32 {
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
            path,
            user,
            scale: 0,
        },
    );
    // SAFETY: vérifié non nul ci-dessus.
    unsafe { *out = h };
    OK
}

unsafe extern "win64" fn add_counter_w(
    query: usize,
    path: *const u16,
    user: usize,
    out: *mut usize,
) -> u32 {
    add(query, wide(path), user, out)
}

unsafe extern "win64" fn add_counter_a(
    query: usize,
    path: *const u8,
    user: usize,
    out: *mut usize,
) -> u32 {
    add(query, ansi(path), user, out)
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
    if format & PDH_FMT_NOSCALE == 0 {
        v *= 10f64.powi(c.scale);
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

// ---------------------------------------------------------------------
// Parcours des objets, chemins et index (lot E de #1410, #1614)
// ---------------------------------------------------------------------

const PDH_MORE_DATA: u32 = 0x8000_07D2;
const PDH_CSTATUS_NO_MACHINE: u32 = 0x8000_07D0;
const PDH_DIALOG_CANCELLED: u32 = 0x8000_07D9;
const PDH_INSUFFICIENT_BUFFER: u32 = 0xC000_0BC2;

/// Objets servis, avec leurs compteurs (noms anglais de Windows).
const OBJECTS: [(&str, &[&str]); 4] = [
    ("Processor", &["% Processor Time"]),
    (
        "Processor Information",
        &["% Processor Time", "% Processor Utility"],
    ),
    (
        "Memory",
        &[
            "Available Bytes",
            "Available KBytes",
            "Available MBytes",
            "Committed Bytes",
            "% Committed Bytes In Use",
        ],
    ),
    ("System", &["Processes", "System Up Time"]),
];

/// Index de compteurs bien connus du registre Perflib (009\Counter).
const INDEXES: [(u32, &str); 5] = [
    (2, "System"),
    (4, "Memory"),
    (6, "% Processor Time"),
    (230, "Process"),
    (238, "Processor"),
];

fn cores() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// Instances d'un objet : _Total et chaque cœur pour Processor.
fn instances(object: &str) -> Vec<String> {
    match object.to_ascii_lowercase().as_str() {
        "processor" => std::iter::once("_Total".to_string())
            .chain((0..cores()).map(|i| i.to_string()))
            .collect(),
        "processor information" => std::iter::once("_Total".to_string())
            .chain((0..cores()).map(|i| format!("0,{i}")))
            .collect(),
        _ => Vec::new(),
    }
}

fn multi_sz(items: &[String]) -> Vec<u16> {
    let mut v: Vec<u16> = Vec::new();
    for i in items {
        v.extend(i.encode_utf16());
        v.push(0);
    }
    v.push(0);
    v
}

/// Copie d'une liste MULTI_SZ dans un tampon de `*size` caractères.
unsafe fn put_multi(data: &[u16], buf: *mut u16, size: *mut u32) -> u32 {
    if size.is_null() {
        return PDH_INVALID_ARGUMENT;
    }
    if buf.is_null() || (*size as usize) < data.len() {
        *size = data.len() as u32;
        return PDH_MORE_DATA;
    }
    std::ptr::copy_nonoverlapping(data.as_ptr(), buf, data.len());
    *size = data.len() as u32;
    OK
}

fn local_machine(name: &str) -> bool {
    let n = name.trim_start_matches('\\');
    n.is_empty()
        || n == "."
        || n.eq_ignore_ascii_case("localhost")
        || n.eq_ignore_ascii_case(&crate::dlls::kernel32::get_hostname_string())
}

unsafe extern "win64" fn PdhBrowseCountersW(_config: usize) -> u32 {
    PDH_DIALOG_CANCELLED
}

unsafe extern "win64" fn PdhConnectMachineW(machine: *const u16) -> u32 {
    if wide(machine).is_none_or(|m| local_machine(&m)) {
        OK
    } else {
        PDH_CSTATUS_NO_MACHINE
    }
}

unsafe extern "win64" fn PdhEnumObjectsW(
    _source: usize,
    machine: *const u16,
    buf: *mut u16,
    size: *mut u32,
    _detail: u32,
    _refresh: i32,
) -> u32 {
    if !wide(machine).is_none_or(|m| local_machine(&m)) {
        return PDH_CSTATUS_NO_MACHINE;
    }
    let names: Vec<String> = OBJECTS.iter().map(|(o, _)| o.to_string()).collect();
    put_multi(&multi_sz(&names), buf, size)
}

unsafe extern "win64" fn PdhEnumObjectItemsW(
    _source: usize,
    machine: *const u16,
    object: *const u16,
    counters: *mut u16,
    counters_size: *mut u32,
    inst: *mut u16,
    inst_size: *mut u32,
    _detail: u32,
    _flags: u32,
) -> u32 {
    if !wide(machine).is_none_or(|m| local_machine(&m)) {
        return PDH_CSTATUS_NO_MACHINE;
    }
    let Some(object) = wide(object) else {
        return PDH_INVALID_ARGUMENT;
    };
    let Some((name, list)) = OBJECTS
        .iter()
        .find(|(o, _)| o.eq_ignore_ascii_case(&object))
    else {
        return PDH_CSTATUS_NO_OBJECT;
    };
    if counters_size.is_null() || inst_size.is_null() {
        return PDH_INVALID_ARGUMENT;
    }
    let c = multi_sz(&list.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    let i = instances(name);
    let i = if i.is_empty() {
        Vec::new()
    } else {
        multi_sz(&i)
    };
    let r1 = put_multi(&c, counters, counters_size);
    let r2 = if i.is_empty() {
        *inst_size = 0;
        OK
    } else {
        put_multi(&i, inst, inst_size)
    };
    if r1 != OK {
        r1
    } else {
        r2
    }
}

/// Éléments d'un chemin : (machine, objet, instance, parent, index, compteur).
type Elements = (String, String, Option<String>, Option<String>, u32, String);

fn split_path(path: &str) -> Option<Elements> {
    let (machine, rest) = match path.strip_prefix("\\\\") {
        Some(r) => {
            let i = r.find('\\')?;
            (format!("\\\\{}", &r[..i]), &r[i..])
        }
        None => (
            format!(
                "\\\\{}",
                crate::dlls::kernel32::get_hostname_string().to_ascii_uppercase()
            ),
            path,
        ),
    };
    let rest = rest.strip_prefix('\\')?;
    let (object, counter) = rest.rsplit_once('\\')?;
    let (object, instance) = match object.split_once('(') {
        Some((o, i)) => (o, Some(i.strip_suffix(')')?)),
        None => (object, None),
    };
    let (parent, name, index) = match instance {
        Some(i) => {
            let (p, n, x) = split_instance(i);
            (p, Some(n), x)
        }
        None => (None, None, 0),
    };
    Some((machine, object.into(), name, parent, index, counter.into()))
}

/// « parent/nom#index » → (parent, nom, index).
fn split_instance(text: &str) -> (Option<String>, String, u32) {
    let (parent, rest) = match text.split_once('/') {
        Some((p, r)) => (Some(p.to_string()), r),
        None => (None, text),
    };
    let (name, index) = match rest.rsplit_once('#') {
        Some((n, i)) if i.parse::<u32>().is_ok() => (n, i.parse().unwrap_or(0)),
        _ => (rest, 0),
    };
    (parent, name.to_string(), index)
}

fn make_path(e: &Elements) -> String {
    let (machine, object, instance, parent, index, counter) = e;
    let mut p = String::new();
    if !machine.is_empty() {
        if !machine.starts_with("\\\\") {
            p.push_str("\\\\");
        }
        p.push_str(machine);
    }
    p.push('\\');
    p.push_str(object);
    if let Some(i) = instance {
        p.push('(');
        if let Some(parent) = parent {
            p.push_str(parent);
            p.push('/');
        }
        p.push_str(i);
        if *index > 0 {
            p.push_str(&format!("#{index}"));
        }
        p.push(')');
    }
    p.push('\\');
    p.push_str(counter);
    p
}

/// PdhExpandCounterPathW : l'instance « * » prend chaque instance de
/// l'objet ; un chemin sans joker se rend tel quel s'il est servi.
unsafe extern "win64" fn PdhExpandCounterPathW(
    path: *const u16,
    buf: *mut u16,
    size: *mut u32,
) -> u32 {
    let Some(path) = wide(path) else {
        return PDH_INVALID_ARGUMENT;
    };
    let Some(e) = split_path(&path) else {
        return PDH_CSTATUS_NO_OBJECT;
    };
    let paths: Vec<String> = if e.2.as_deref() == Some("*") {
        instances(&e.1)
            .into_iter()
            .map(|i| {
                let mut x = e.clone();
                x.2 = Some(i);
                make_path(&x)
            })
            .filter(|p| parse(p).is_ok())
            .collect()
    } else if parse(&path).is_ok() {
        vec![path]
    } else {
        Vec::new()
    };
    if paths.is_empty() {
        return PDH_CSTATUS_NO_OBJECT;
    }
    put_multi(&multi_sz(&paths), buf, size)
}

/// Lecture d'un pointeur de chaîne dans une structure de l'appelant.
unsafe fn field(base: *const u8, at: usize) -> Option<String> {
    wide(crate::wow32::read_ptr(base as usize + at) as *const u16)
}

/// PdhMakeCounterPathW : PDH_COUNTER_PATH_ELEMENTS_W (szMachineName,
/// szObjectName, szInstanceName, szParentInstance, dwInstanceIndex,
/// szCounterName) → chemin complet.
unsafe extern "win64" fn PdhMakeCounterPathW(
    elements: *const u8,
    buf: *mut u16,
    size: *mut u32,
    _flags: u32,
) -> u32 {
    if elements.is_null() || size.is_null() {
        return PDH_INVALID_ARGUMENT;
    }
    let ps = crate::wow32::pointer_size();
    let (Some(object), Some(counter)) = (field(elements, ps), field(elements, 5 * ps)) else {
        return PDH_INVALID_ARGUMENT;
    };
    let e: Elements = (
        field(elements, 0).unwrap_or_default(),
        object,
        field(elements, 2 * ps),
        field(elements, 3 * ps),
        (elements.add(4 * ps) as *const u32).read_unaligned(),
        counter,
    );
    let mut w: Vec<u16> = make_path(&e).encode_utf16().collect();
    w.push(0);
    if buf.is_null() || (*size as usize) < w.len() {
        *size = w.len() as u32;
        return PDH_MORE_DATA;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    *size = w.len() as u32;
    OK
}

/// Structure de pointeurs de chaînes suivie des chaînes elles-mêmes, dans
/// un tampon de `*size` octets fourni par l'appelant.
struct Packer {
    head: usize,
    strings: Vec<(usize, Vec<u16>)>,
    values: Vec<(usize, Vec<u8>)>,
}
impl Packer {
    fn new(head: usize) -> Self {
        Packer {
            head,
            strings: Vec::new(),
            values: Vec::new(),
        }
    }
    fn text(&mut self, at: usize, s: Option<&str>) {
        if let Some(s) = s {
            self.strings
                .push((at, s.encode_utf16().chain(std::iter::once(0)).collect()));
        }
    }
    fn value(&mut self, at: usize, v: &[u8]) {
        self.values.push((at, v.to_vec()));
    }
    fn size(&self) -> usize {
        self.head + self.strings.iter().map(|(_, s)| s.len() * 2).sum::<usize>()
    }
    unsafe fn write(&self, buf: *mut u8) {
        std::ptr::write_bytes(buf, 0, self.head);
        for (at, v) in &self.values {
            std::ptr::copy_nonoverlapping(v.as_ptr(), buf.add(*at), v.len());
        }
        let mut text = buf.add(self.head);
        for (at, s) in &self.strings {
            crate::wow32::write_ptr(buf as usize + at, text as usize);
            std::ptr::copy_nonoverlapping(s.as_ptr() as *const u8, text, s.len() * 2);
            text = text.add(s.len() * 2);
        }
    }
    /// Remplit `buf` si `*size` suffit, sinon rend la taille voulue.
    unsafe fn deliver(&self, buf: *mut u8, size: *mut u32) -> u32 {
        let need = self.size() as u32;
        if buf.is_null() || *size < need {
            *size = need;
            return if buf.is_null() || *size == 0 {
                PDH_MORE_DATA
            } else {
                PDH_INSUFFICIENT_BUFFER
            };
        }
        self.write(buf);
        *size = need;
        OK
    }
}

unsafe extern "win64" fn PdhParseCounterPathW(
    path: *const u16,
    buf: *mut u8,
    size: *mut u32,
    _flags: u32,
) -> u32 {
    let (Some(path), false) = (wide(path), size.is_null()) else {
        return PDH_INVALID_ARGUMENT;
    };
    let Some(e) = split_path(&path) else {
        return PDH_INVALID_ARGUMENT;
    };
    let ps = crate::wow32::pointer_size();
    let mut p = Packer::new(6 * ps);
    p.text(0, Some(&e.0));
    p.text(ps, Some(&e.1));
    p.text(2 * ps, e.2.as_deref());
    p.text(3 * ps, e.3.as_deref());
    p.value(4 * ps, &e.4.to_le_bytes());
    p.text(5 * ps, Some(&e.5));
    p.deliver(buf, size)
}

unsafe extern "win64" fn PdhParseInstanceNameW(
    instance: *const u16,
    name: *mut u16,
    name_len: *mut u32,
    parent: *mut u16,
    parent_len: *mut u32,
    index: *mut u32,
) -> u32 {
    let Some(text) = wide(instance) else {
        return PDH_INVALID_ARGUMENT;
    };
    if name_len.is_null() || parent_len.is_null() || index.is_null() {
        return PDH_INVALID_ARGUMENT;
    }
    let (p, n, i) = split_instance(&text);
    let n: Vec<u16> = n.encode_utf16().chain(std::iter::once(0)).collect();
    let p: Vec<u16> = p
        .unwrap_or_default()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    *index = i;
    let fits = |buf: *mut u16, len: u32, v: &[u16]| !buf.is_null() && len as usize >= v.len();
    let ok = fits(name, *name_len, &n) && fits(parent, *parent_len, &p);
    *name_len = n.len() as u32;
    *parent_len = p.len() as u32;
    if !ok {
        return PDH_MORE_DATA;
    }
    std::ptr::copy_nonoverlapping(n.as_ptr(), name, n.len());
    std::ptr::copy_nonoverlapping(p.as_ptr(), parent, p.len());
    OK
}

unsafe extern "win64" fn PdhLookupPerfIndexByNameW(
    machine: *const u16,
    name: *const u16,
    index: *mut u32,
) -> u32 {
    if !wide(machine).is_none_or(|m| local_machine(&m)) {
        return PDH_CSTATUS_NO_MACHINE;
    }
    let (Some(name), false) = (wide(name), index.is_null()) else {
        return PDH_INVALID_ARGUMENT;
    };
    match INDEXES.iter().find(|(_, n)| n.eq_ignore_ascii_case(&name)) {
        Some((i, _)) => {
            *index = *i;
            OK
        }
        None => PDH_INVALID_ARGUMENT,
    }
}

unsafe extern "win64" fn PdhLookupPerfNameByIndexW(
    machine: *const u16,
    index: u32,
    buf: *mut u16,
    size: *mut u32,
) -> u32 {
    if !wide(machine).is_none_or(|m| local_machine(&m)) {
        return PDH_CSTATUS_NO_MACHINE;
    }
    if size.is_null() {
        return PDH_INVALID_ARGUMENT;
    }
    let Some((_, name)) = INDEXES.iter().find(|(i, _)| *i == index) else {
        return PDH_INVALID_ARGUMENT;
    };
    let w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    if buf.is_null() || (*size as usize) < w.len() {
        *size = w.len() as u32;
        return PDH_MORE_DATA;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    *size = w.len() as u32;
    OK
}

unsafe extern "win64" fn PdhSetCounterScaleFactor(counter: usize, factor: i32) -> u32 {
    let mut s = lock(state());
    let Some(c) = s.counters.get_mut(&counter) else {
        return PDH_INVALID_HANDLE;
    };
    if !(-7..=7).contains(&factor) {
        return PDH_INVALID_ARGUMENT;
    }
    c.scale = factor;
    OK
}

/// PdhGetCounterInfoW : PDH_COUNTER_INFO_W (dwLength, dwType, CVersion,
/// CStatus, lScale, lDefaultScale, dwUserData, dwQueryUserData,
/// szFullPath, éléments du chemin, szExplainText, DataBuffer).
unsafe extern "win64" fn PdhGetCounterInfoW(
    counter: usize,
    _explain: u8,
    size: *mut u32,
    buf: *mut u8,
) -> u32 {
    if size.is_null() {
        return PDH_INVALID_ARGUMENT;
    }
    let s = lock(state());
    let Some(c) = s.counters.get(&counter) else {
        return PDH_INVALID_HANDLE;
    };
    let Some(e) = split_path(&c.path) else {
        return PDH_INVALID_DATA;
    };
    let ps = crate::wow32::pointer_size();
    let user = 24;
    let full = user + 2 * ps;
    let elems = full + ps;
    let explain = elems + 6 * ps;
    let head = (explain + ps + 4).next_multiple_of(ps);
    let mut p = Packer::new(head);
    let kind = if matches!(c.kind, Kind::Cpu(_)) {
        TYPE_RATE
    } else {
        TYPE_RAW
    };
    p.value(4, &kind.to_le_bytes());
    p.value(8, &0x0501u32.to_le_bytes()); // CVersion
    p.value(16, &c.scale.to_le_bytes());
    p.value(user, &c.user.to_le_bytes()[..ps]);
    p.text(full, Some(&c.path));
    p.text(elems, Some(&e.0));
    p.text(elems + ps, Some(&e.1));
    p.text(elems + 2 * ps, e.2.as_deref());
    p.text(elems + 3 * ps, e.3.as_deref());
    p.value(elems + 4 * ps, &e.4.to_le_bytes());
    p.text(elems + 5 * ps, Some(&e.5));
    let need = p.size() as u32;
    p.value(0, &need.to_le_bytes());
    p.deliver(buf, size)
}

/// PdhGetFormattedCounterArrayW : un élément par compteur (son instance,
/// ou l'objet sans instance), PDH_FMT_COUNTERVALUE_ITEM_W (szName puis la
/// valeur à l'octet 8).
unsafe extern "win64" fn PdhGetFormattedCounterArrayW(
    counter: usize,
    format: u32,
    size: *mut u32,
    count: *mut u32,
    buf: *mut u8,
) -> u32 {
    if size.is_null() || count.is_null() {
        return PDH_INVALID_ARGUMENT;
    }
    let name = {
        let s = lock(state());
        let Some(c) = s.counters.get(&counter) else {
            return PDH_INVALID_HANDLE;
        };
        split_path(&c.path).and_then(|e| e.2).unwrap_or_default()
    };
    let mut value = [0u8; 16];
    let r = get_formatted_counter_value(counter, format, std::ptr::null_mut(), value.as_mut_ptr());
    if r != OK {
        return r;
    }
    let mut p = Packer::new(24);
    p.text(0, Some(&name));
    p.value(8, &value);
    let need = p.size() as u32;
    *count = 1;
    if buf.is_null() || *size < need {
        *size = need;
        return PDH_MORE_DATA;
    }
    p.write(buf);
    *size = need;
    OK
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
        "PdhBrowseCountersW" => fn PdhBrowseCountersW, "PdhConnectMachineW" => fn PdhConnectMachineW,
        "PdhEnumObjectsW" => fn PdhEnumObjectsW, "PdhEnumObjectItemsW" => fn PdhEnumObjectItemsW,
        "PdhExpandCounterPathW" => fn PdhExpandCounterPathW, "PdhMakeCounterPathW" => fn PdhMakeCounterPathW,
        "PdhParseCounterPathW" => fn PdhParseCounterPathW, "PdhParseInstanceNameW" => fn PdhParseInstanceNameW,
        "PdhLookupPerfIndexByNameW" => fn PdhLookupPerfIndexByNameW,
        "PdhLookupPerfNameByIndexW" => fn PdhLookupPerfNameByIndexW,
        "PdhSetCounterScaleFactor" => fn PdhSetCounterScaleFactor,
        "PdhGetCounterInfoW" => fn PdhGetCounterInfoW,
        "PdhGetFormattedCounterArrayW" => fn PdhGetFormattedCounterArrayW,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chemins_objets_et_index() {
        unsafe {
            let mut size = 0u32;
            assert_eq!(
                PdhEnumObjectsW(0, std::ptr::null(), std::ptr::null_mut(), &mut size, 0, 0),
                PDH_MORE_DATA
            );
            let mut buf = vec![0u16; size as usize];
            assert_eq!(
                PdhEnumObjectsW(0, std::ptr::null(), buf.as_mut_ptr(), &mut size, 0, 0),
                OK
            );
            assert!(String::from_utf16_lossy(&buf).contains("Memory"));

            let path: Vec<u16> = "\\\\PC\\Processor(_Total)\\% Processor Time\0"
                .encode_utf16()
                .collect();
            let mut bytes = 0u32;
            assert_eq!(
                PdhParseCounterPathW(path.as_ptr(), std::ptr::null_mut(), &mut bytes, 0),
                PDH_MORE_DATA
            );
            let mut el = vec![0u8; bytes as usize];
            assert_eq!(
                PdhParseCounterPathW(path.as_ptr(), el.as_mut_ptr(), &mut bytes, 0),
                OK
            );
            assert_eq!(field(el.as_ptr(), 0).as_deref(), Some("\\\\PC"));
            assert_eq!(field(el.as_ptr(), 16).as_deref(), Some("_Total"));
            assert_eq!(field(el.as_ptr(), 40).as_deref(), Some("% Processor Time"));
            let mut out = vec![0u16; 128];
            let mut n = 128u32;
            assert_eq!(
                PdhMakeCounterPathW(el.as_ptr(), out.as_mut_ptr(), &mut n, 0),
                OK
            );
            assert_eq!(
                wide(out.as_ptr()).unwrap(),
                "\\\\PC\\Processor(_Total)\\% Processor Time"
            );

            let inst: Vec<u16> = "svchost/thread#2\0".encode_utf16().collect();
            let (mut nl, mut pl, mut idx) = (16u32, 16u32, 0u32);
            let (mut nb, mut pb) = ([0u16; 16], [0u16; 16]);
            assert_eq!(
                PdhParseInstanceNameW(
                    inst.as_ptr(),
                    nb.as_mut_ptr(),
                    &mut nl,
                    pb.as_mut_ptr(),
                    &mut pl,
                    &mut idx
                ),
                OK
            );
            assert_eq!(
                (
                    wide(nb.as_ptr()).unwrap().as_str(),
                    wide(pb.as_ptr()).unwrap().as_str(),
                    idx
                ),
                ("thread", "svchost", 2)
            );

            let wild: Vec<u16> = "\\Processor(*)\\% Processor Time\0"
                .encode_utf16()
                .collect();
            let mut size = 0u32;
            assert_eq!(
                PdhExpandCounterPathW(wild.as_ptr(), std::ptr::null_mut(), &mut size),
                PDH_MORE_DATA
            );
            let mut list = vec![0u16; size as usize];
            assert_eq!(
                PdhExpandCounterPathW(wild.as_ptr(), list.as_mut_ptr(), &mut size),
                OK
            );
            assert_eq!(list.iter().filter(|&&c| c == 0).count(), cores() + 2);

            let name: Vec<u16> = "Processor\0".encode_utf16().collect();
            let mut i = 0u32;
            assert_eq!(
                PdhLookupPerfIndexByNameW(std::ptr::null(), name.as_ptr(), &mut i),
                OK
            );
            assert_eq!(i, 238);

            let mut q = 0usize;
            assert_eq!(open_query(0, 0, &mut q), OK);
            let p: Vec<u16> = "\\Memory\\Available Bytes\0".encode_utf16().collect();
            let mut c = 0usize;
            assert_eq!(add_counter_w(q, p.as_ptr(), 42, &mut c), OK);
            let mut sz = 0u32;
            assert_eq!(
                PdhGetCounterInfoW(c, 0, &mut sz, std::ptr::null_mut()),
                PDH_MORE_DATA
            );
            let mut info = vec![0u8; sz as usize];
            assert_eq!(PdhGetCounterInfoW(c, 0, &mut sz, info.as_mut_ptr()), OK);
            assert_eq!(u32::from_le_bytes(info[0..4].try_into().unwrap()), sz);
            assert_eq!(usize::from_le_bytes(info[24..32].try_into().unwrap()), 42);
            assert_eq!(
                field(info.as_ptr(), 40).as_deref(),
                Some("\\Memory\\Available Bytes")
            );
            assert_eq!(PdhSetCounterScaleFactor(c, 9), PDH_INVALID_ARGUMENT);
            assert_eq!(PdhSetCounterScaleFactor(c, -3), OK);
            assert_eq!(close_query(q), OK);
        }
    }

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
