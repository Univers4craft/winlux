//! Planificateur de tâches 2.0 côté programmes Windows : `ITaskService`
//! (CLSID_TaskScheduler) et ses objets COM, créés par `CoCreateInstance`.
//!
//! Les tâches enregistrées par un `.exe` sont les mêmes fichiers que ceux du
//! planificateur Winlux (`/etc/winlux/tasks`) : elles apparaissent dans
//! l'application « Planificateur de tâches » et le service les déclenche.
//!
//! Correspondance des déclencheurs : temps (une fois, ou répété avec
//! `Repetition.Interval`), quotidien, hebdomadaire, démarrage, ouverture de
//! session et enregistrement (exécution immédiate). Les déclencheurs sur
//! événement conservent leur requête mais ne se déclenchent pas, le journal
//! d'événements Windows n'existant pas ; mensuel et inactivité sont ignorés.

use crate::admin;
use crate::dlls::lock;
use crate::dlls::ole32::Guid;
use crate::tasks::{self, Action, Task, Trigger};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_ACCESSDENIED: i32 = 0x8007_0005u32 as i32;
const ERROR_FILE_NOT_FOUND: i32 = 0x8007_0002u32 as i32;
const ERROR_ALREADY_EXISTS: i32 = 0x8007_00B7u32 as i32;
const ERROR_ONLY_IF_CONNECTED: i32 = 0x8007_04E3u32 as i32;
const SCHED_S_TASK_HAS_NOT_RUN: i32 = 0x0004_1303;
const SCHED_E_MALFORMEDXML: i32 = 0x8004_1318u32 as i32;
const VARIANT_TRUE: i16 = -1;

const TASK_CREATE: i32 = 2;
const TASK_UPDATE: i32 = 4;
const TASK_DISABLE: i32 = 8;

const TRIGGER_EVENT: i32 = 0;
const TRIGGER_TIME: i32 = 1;
const TRIGGER_DAILY: i32 = 2;
const TRIGGER_WEEKLY: i32 = 3;
const TRIGGER_REGISTRATION: i32 = 7;
const TRIGGER_BOOT: i32 = 8;
const TRIGGER_LOGON: i32 = 9;

const fn g(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Guid {
    Guid {
        data1: d1,
        data2: d2,
        data3: d3,
        data4: d4,
    }
}

pub const CLSID_TASK_SCHEDULER: Guid = g(
    0x0F87_369F,
    0xA4E5,
    0x4CFC,
    [0xBD, 0x3E, 0x73, 0xE6, 0x15, 0x45, 0x72, 0xDD],
);
const IID_IUNKNOWN: Guid = g(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IDISPATCH: Guid = g(0x0002_0400, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_ITASKSERVICE: Guid = g(
    0x2FAB_A4C7,
    0x4DA9,
    0x4013,
    [0x96, 0x97, 0x20, 0xCC, 0x3F, 0xD4, 0x0F, 0x85],
);
const IID_ITASKFOLDER: Guid = g(
    0x8CFA_C062,
    0xA080,
    0x4C15,
    [0x9A, 0x88, 0xAA, 0x7C, 0x2A, 0xF8, 0x0D, 0xFC],
);
const IID_ITASKFOLDERCOLLECTION: Guid = g(
    0x7918_4A66,
    0x8664,
    0x423F,
    [0x97, 0xF1, 0x63, 0x73, 0x56, 0xA5, 0xD8, 0x12],
);
const IID_IREGISTEREDTASK: Guid = g(
    0x9C86_F320,
    0xDEE3,
    0x4DD1,
    [0xB9, 0x72, 0xA3, 0x03, 0xF2, 0x6B, 0x06, 0x1E],
);
const IID_IREGISTEREDTASKCOLLECTION: Guid = g(
    0x8662_7EB4,
    0x42A7,
    0x41E4,
    [0xA4, 0xD9, 0xAC, 0x33, 0xA7, 0x2F, 0x2D, 0x52],
);
const IID_ITASKDEFINITION: Guid = g(
    0xF5BC_8FC5,
    0x536D,
    0x4F77,
    [0xB8, 0x52, 0xFB, 0xC1, 0x35, 0x6F, 0xDE, 0xB6],
);
const IID_IREGISTRATIONINFO: Guid = g(
    0x416D_8B73,
    0xCB41,
    0x4EA1,
    [0x80, 0x5C, 0x9B, 0xE9, 0xA5, 0xAC, 0x4A, 0x74],
);
const IID_ITRIGGERCOLLECTION: Guid = g(
    0x85DF_5081,
    0x1B24,
    0x4F32,
    [0x87, 0x8A, 0xD9, 0xD1, 0x4D, 0xF4, 0xCB, 0x77],
);
const IID_ITRIGGER: Guid = g(
    0x0994_1815,
    0xEA89,
    0x4B5B,
    [0x89, 0xE0, 0x2A, 0x77, 0x38, 0x01, 0xFA, 0xC3],
);
const IID_ITIMETRIGGER: Guid = g(
    0xB457_47E0,
    0xEBA7,
    0x4276,
    [0x9F, 0x29, 0x85, 0xC5, 0xBB, 0x30, 0x00, 0x06],
);
const IID_IDAILYTRIGGER: Guid = g(
    0x126C_5CD8,
    0xB288,
    0x41D5,
    [0x8D, 0xBF, 0xE4, 0x91, 0x44, 0x6A, 0xDC, 0x5C],
);
const IID_IWEEKLYTRIGGER: Guid = g(
    0x5038_FC98,
    0x82FF,
    0x436D,
    [0x87, 0x28, 0xA5, 0x12, 0xA5, 0x7C, 0x9D, 0xC1],
);
const IID_IBOOTTRIGGER: Guid = g(
    0x2A9C_35DA,
    0xD357,
    0x41F4,
    [0xBB, 0xC1, 0x20, 0x7A, 0xC1, 0xB1, 0xF3, 0xCB],
);
const IID_ILOGONTRIGGER: Guid = g(
    0x72DA_DE38,
    0xFAE4,
    0x4B3E,
    [0xBA, 0xF4, 0x5D, 0x00, 0x9A, 0xF0, 0x2B, 0x1C],
);
const IID_IEVENTTRIGGER: Guid = g(
    0xD45B_0167,
    0x9653,
    0x4EEF,
    [0xB9, 0x4F, 0x07, 0x32, 0xCA, 0x7A, 0xF2, 0x51],
);
const IID_IREGISTRATIONTRIGGER: Guid = g(
    0x4C8F_EC3A,
    0xC218,
    0x4E0C,
    [0xB2, 0x3D, 0x62, 0x90, 0x24, 0xDB, 0x91, 0xA2],
);
const IID_IREPETITIONPATTERN: Guid = g(
    0x7FB9_ACF1,
    0x26BE,
    0x400E,
    [0x85, 0xB5, 0x29, 0x4B, 0x9C, 0x75, 0xDF, 0xD6],
);
const IID_ITASKSETTINGS: Guid = g(
    0x8FD4_711D,
    0x2D02,
    0x4C8C,
    [0x87, 0xE3, 0xEF, 0xF6, 0x99, 0xDE, 0x12, 0x7E],
);
const IID_IPRINCIPAL: Guid = g(
    0xD98D_51E5,
    0xC9B4,
    0x496A,
    [0xA9, 0xC1, 0x18, 0x98, 0x02, 0x61, 0xCF, 0x0F],
);
const IID_IACTIONCOLLECTION: Guid = g(
    0x0282_0E19,
    0x7B98,
    0x4ED2,
    [0xB2, 0xE8, 0xFD, 0xCC, 0xCE, 0xFF, 0x61, 0x9B],
);
const IID_IACTION: Guid = g(
    0xBAE5_4997,
    0x48B1,
    0x4CBE,
    [0x99, 0x65, 0xD6, 0xBE, 0x26, 0x3E, 0xBE, 0xA4],
);
const IID_IEXECACTION: Guid = g(
    0x4C3D_624D,
    0xFD6B,
    0x49A3,
    [0xB9, 0xB7, 0x09, 0xCB, 0x3C, 0xD3, 0xF0, 0x47],
);
const IID_IRUNNINGTASK: Guid = g(
    0x6537_58FB,
    0x7B9A,
    0x4F1E,
    [0xA4, 0x71, 0xBE, 0xEB, 0x8E, 0x9B, 0x83, 0x4E],
);
const IID_IRUNNINGTASKCOLLECTION: Guid = g(
    0x6A67_614B,
    0x6828,
    0x4FEC,
    [0xAA, 0x54, 0x6D, 0x52, 0xE8, 0xF1, 0xF2, 0xDB],
);

// ---------- Données des définitions ----------

#[derive(Clone, Default)]
struct RegData {
    description: String,
    author: String,
    version: String,
    date: String,
    documentation: String,
    uri: String,
    source: String,
}

#[derive(Clone)]
struct TrigData {
    kind: i32,
    id: String,
    start: String,
    end: String,
    enabled: bool,
    days_interval: i16,
    days_of_week: i16,
    weeks_interval: i16,
    rep_interval: String,
    rep_duration: String,
    rep_stop: bool,
    time_limit: String,
    delay: String,
    subscription: String,
    user: String,
    random_delay: String,
}

impl TrigData {
    fn new(kind: i32) -> TrigData {
        TrigData {
            kind,
            id: String::new(),
            start: String::new(),
            end: String::new(),
            enabled: true,
            days_interval: 1,
            days_of_week: 0,
            weeks_interval: 1,
            rep_interval: String::new(),
            rep_duration: String::new(),
            rep_stop: false,
            time_limit: String::new(),
            delay: String::new(),
            subscription: String::new(),
            user: String::new(),
            random_delay: String::new(),
        }
    }
}

#[derive(Clone, Default)]
struct ActData {
    id: String,
    path: String,
    args: String,
    workdir: String,
}

#[derive(Clone)]
struct SetData {
    enabled: bool,
    disallow_battery: bool,
    stop_on_battery: bool,
    network: bool,
    time_limit: String,
    hidden: bool,
    allow_demand: bool,
    start_when_available: bool,
    allow_hard_terminate: bool,
    wake: bool,
    run_only_idle: bool,
    priority: i32,
    multiple: i32,
    restart_count: i32,
    restart_interval: String,
    delete_expired: String,
    compatibility: i32,
}

impl Default for SetData {
    /// Valeurs par défaut de Windows pour une nouvelle tâche.
    fn default() -> SetData {
        SetData {
            enabled: true,
            disallow_battery: true,
            stop_on_battery: true,
            network: false,
            time_limit: "PT72H".into(),
            hidden: false,
            allow_demand: true,
            start_when_available: false,
            allow_hard_terminate: true,
            wake: false,
            run_only_idle: false,
            priority: 7,
            multiple: 2,
            restart_count: 0,
            restart_interval: String::new(),
            delete_expired: String::new(),
            compatibility: 2,
        }
    }
}

#[derive(Clone, Default)]
struct PrinData {
    id: String,
    display: String,
    user: String,
    group: String,
    logon_type: i32,
    run_level: i32,
}

#[derive(Default)]
struct DefData {
    reg: RegData,
    triggers: Vec<Arc<Mutex<TrigData>>>,
    actions: Vec<Arc<Mutex<ActData>>>,
    settings: SetData,
    principal: PrinData,
    data: String,
    context: String,
}

type Def = Arc<Mutex<DefData>>;

// ---------- Durées ISO 8601 et dates ----------

/// `P1DT2H30M`, `PT72H`, `PT15M` → secondes.
fn parse_duration(s: &str) -> Option<i64> {
    let s = s.trim();
    let rest = s.strip_prefix('P')?;
    let mut total = 0i64;
    let mut num = String::new();
    let mut time = false;
    for c in rest.chars() {
        match c {
            'T' => time = true,
            '0'..='9' => num.push(c),
            _ => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                let unit: i64 = match (c, time) {
                    ('D', false) => 86_400,
                    ('W', false) => 7 * 86_400,
                    ('Y', false) => 365 * 86_400,
                    ('M', false) => 30 * 86_400,
                    ('H', true) => 3600,
                    ('M', true) => 60,
                    ('S', true) => 1,
                    _ => return None,
                };
                // Une durée absurde (« P99999999999Y ») ne doit pas déborder.
                total = n.checked_mul(unit)?.checked_add(total)?;
            }
        }
    }
    num.is_empty().then_some(total)
}

fn format_duration(minutes: u32) -> String {
    if minutes == 0 {
        "PT0S".into()
    } else if minutes.is_multiple_of(60) {
        format!("PT{}H", minutes / 60)
    } else {
        format!("PT{minutes}M")
    }
}

/// `2026-11-02T09:05:00` (fuseau éventuel ignoré) → date et heure.
fn parse_boundary(s: &str) -> Option<(i32, u32, u32, u32, u32)> {
    let (date, time) = s.trim().split_once('T')?;
    let mut d = date.split('-');
    let y = d.next()?.parse().ok()?;
    let m = d.next()?.parse().ok()?;
    let day = d.next()?.parse().ok()?;
    let mut t = time.split(':');
    let h = t.next()?.parse().ok()?;
    let mi = t.next()?.get(..2)?.parse().ok()?;
    Some((y, m, day, h, mi))
}

fn boundary(y: i32, m: u32, d: u32, h: u32, mi: u32) -> String {
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:00")
}

fn today_at(h: u32, mi: u32) -> String {
    let l = admin::local(admin::now());
    boundary(l.year, l.month, l.day, h, mi)
}

/// Date OLE Automation (jours depuis le 30/12/1899, heure locale).
fn ole_date(epoch: i64) -> f64 {
    let l = admin::local(epoch);
    (epoch + l.offset) as f64 / 86_400.0 + 25_569.0
}

// ---------- Conversions définition ↔ tâche ----------

fn def_to_task(def: &DefData, folder: &str, name: &str) -> (Task, bool) {
    let mut run_now = false;
    let mut t = Task {
        id: tasks::id_for(folder, name),
        name: name.to_string(),
        folder: folder.to_string(),
        description: def.reg.description.clone(),
        author: def.reg.author.clone(),
        enabled: def.settings.enabled,
        ac_only: def.settings.disallow_battery,
        network: def.settings.network,
        time_limit: parse_duration(&def.settings.time_limit)
            .map(|s| (s / 60) as u32)
            .unwrap_or(4320),
        ..Task::default()
    };
    for tr in &def.triggers {
        let tr = lock(tr).clone();
        if !tr.enabled {
            continue;
        }
        let start = parse_boundary(&tr.start);
        let (h, mi) = start.map_or((0, 0), |s| (s.3, s.4));
        if let Some(secs) = parse_duration(&tr.rep_interval).filter(|&s| s >= 60) {
            if matches!(tr.kind, TRIGGER_TIME | TRIGGER_DAILY) {
                t.triggers.push(Trigger::Interval {
                    minutes: (secs / 60) as u32,
                });
                if tr.kind == TRIGGER_TIME {
                    continue;
                }
            }
        }
        match tr.kind {
            TRIGGER_TIME => {
                if let Some((year, month, day, hour, minute)) = start {
                    t.triggers.push(Trigger::Once {
                        year,
                        month,
                        day,
                        hour,
                        minute,
                    });
                }
            }
            TRIGGER_DAILY => t.triggers.push(Trigger::Daily {
                hour: h,
                minute: mi,
                every: tr.days_interval.max(1) as u32,
            }),
            TRIGGER_WEEKLY => {
                let mut days = (tr.days_of_week & 0x7F) as u8;
                if days == 0 {
                    let w = start.map_or(admin::local(admin::now()).weekday, |s| {
                        admin::local(admin::epoch_of(s.0, s.1, s.2 as i64, 12, 0)).weekday
                    });
                    days = 1 << w;
                }
                t.triggers.push(Trigger::Weekly {
                    hour: h,
                    minute: mi,
                    days,
                });
            }
            TRIGGER_BOOT => t.triggers.push(Trigger::Boot),
            TRIGGER_LOGON => t.triggers.push(Trigger::Logon),
            TRIGGER_REGISTRATION => run_now = true,
            TRIGGER_EVENT => t.triggers.push(Trigger::Event("windows-log".into())),
            other => crate::log_warn!("taskschd", "déclencheur de type {other} non pris en charge"),
        }
    }
    for a in &def.actions {
        let a = lock(a).clone();
        if !a.path.is_empty() {
            t.actions.push(Action {
                path: a.path,
                arguments: a.args,
                workdir: a.workdir,
            });
        }
    }
    (t, run_now)
}

fn task_to_def(t: &Task) -> DefData {
    let mut d = DefData {
        reg: RegData {
            description: t.description.clone(),
            author: t.author.clone(),
            ..RegData::default()
        },
        settings: SetData {
            enabled: t.enabled,
            disallow_battery: t.ac_only,
            stop_on_battery: t.ac_only,
            network: t.network,
            time_limit: format_duration(t.time_limit),
            ..SetData::default()
        },
        ..DefData::default()
    };
    for tr in &t.triggers {
        let data = match *tr {
            Trigger::Once {
                year,
                month,
                day,
                hour,
                minute,
            } => {
                let mut x = TrigData::new(TRIGGER_TIME);
                x.start = boundary(year, month, day, hour, minute);
                x
            }
            Trigger::Daily {
                hour,
                minute,
                every,
            } => {
                let mut x = TrigData::new(TRIGGER_DAILY);
                x.start = today_at(hour, minute);
                x.days_interval = every.min(i16::MAX as u32) as i16;
                x
            }
            Trigger::Weekly { hour, minute, days } => {
                let mut x = TrigData::new(TRIGGER_WEEKLY);
                x.start = today_at(hour, minute);
                x.days_of_week = days as i16;
                x
            }
            Trigger::Interval { minutes } => {
                let mut x = TrigData::new(TRIGGER_TIME);
                let l = admin::local(admin::now());
                x.start = today_at(l.hour, l.minute);
                x.rep_interval = format!("PT{minutes}M");
                x
            }
            Trigger::Boot => TrigData::new(TRIGGER_BOOT),
            Trigger::Logon => TrigData::new(TRIGGER_LOGON),
            Trigger::Event(ref e) => {
                let mut x = TrigData::new(TRIGGER_EVENT);
                x.subscription = e.clone();
                x
            }
        };
        d.triggers.push(Arc::new(Mutex::new(data)));
    }
    for a in &t.actions {
        d.actions.push(Arc::new(Mutex::new(ActData {
            id: String::new(),
            path: a.path.clone(),
            args: a.arguments.clone(),
            workdir: a.workdir.clone(),
        })));
    }
    d
}

// ---------- XML du planificateur ----------

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn unesc(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

const DAY_NAMES: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

fn def_xml(d: &DefData) -> String {
    let b = |v: bool| if v { "true" } else { "false" };
    let mut x = String::from("<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n<Task version=\"1.2\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\n  <RegistrationInfo>\n");
    for (tag, v) in [
        ("Description", &d.reg.description),
        ("Author", &d.reg.author),
        ("Version", &d.reg.version),
        ("Date", &d.reg.date),
        ("URI", &d.reg.uri),
    ] {
        if !v.is_empty() {
            x += &format!("    <{tag}>{}</{tag}>\n", esc(v));
        }
    }
    x += "  </RegistrationInfo>\n  <Triggers>\n";
    for tr in &d.triggers {
        let t = lock(tr).clone();
        let mut inner = String::new();
        if !t.start.is_empty() {
            inner += &format!("      <StartBoundary>{}</StartBoundary>\n", esc(&t.start));
        }
        if !t.end.is_empty() {
            inner += &format!("      <EndBoundary>{}</EndBoundary>\n", esc(&t.end));
        }
        inner += &format!("      <Enabled>{}</Enabled>\n", b(t.enabled));
        if !t.rep_interval.is_empty() {
            inner += &format!(
                "      <Repetition>\n        <Interval>{}</Interval>\n      </Repetition>\n",
                esc(&t.rep_interval)
            );
        }
        let (tag, extra) = match t.kind {
            TRIGGER_TIME => ("TimeTrigger", String::new()),
            TRIGGER_DAILY => (
                "CalendarTrigger",
                format!(
                    "      <ScheduleByDay>\n        <DaysInterval>{}</DaysInterval>\n      </ScheduleByDay>\n",
                    t.days_interval.max(1)
                ),
            ),
            TRIGGER_WEEKLY => {
                let days: String = (0..7)
                    .filter(|i| t.days_of_week & (1 << i) != 0)
                    .map(|i| format!("<{}/>", DAY_NAMES[i]))
                    .collect();
                (
                    "CalendarTrigger",
                    format!(
                        "      <ScheduleByWeek>\n        <DaysOfWeek>{days}</DaysOfWeek>\n        <WeeksInterval>{}</WeeksInterval>\n      </ScheduleByWeek>\n",
                        t.weeks_interval.max(1)
                    ),
                )
            }
            TRIGGER_BOOT => ("BootTrigger", String::new()),
            TRIGGER_LOGON => ("LogonTrigger", String::new()),
            TRIGGER_REGISTRATION => ("RegistrationTrigger", String::new()),
            _ => (
                "EventTrigger",
                format!("      <Subscription>{}</Subscription>\n", esc(&t.subscription)),
            ),
        };
        x += &format!("    <{tag}>\n{inner}{extra}    </{tag}>\n");
    }
    let s = &d.settings;
    x += &format!(
        "  </Triggers>\n  <Settings>\n    <Enabled>{}</Enabled>\n    <DisallowStartIfOnBatteries>{}</DisallowStartIfOnBatteries>\n    <StopIfGoingOnBatteries>{}</StopIfGoingOnBatteries>\n    <RunOnlyIfNetworkAvailable>{}</RunOnlyIfNetworkAvailable>\n    <ExecutionTimeLimit>{}</ExecutionTimeLimit>\n    <Hidden>{}</Hidden>\n    <AllowStartOnDemand>{}</AllowStartOnDemand>\n  </Settings>\n  <Actions Context=\"Author\">\n",
        b(s.enabled),
        b(s.disallow_battery),
        b(s.stop_on_battery),
        b(s.network),
        esc(&s.time_limit),
        b(s.hidden),
        b(s.allow_demand)
    );
    for a in &d.actions {
        let a = lock(a).clone();
        x += &format!("    <Exec>\n      <Command>{}</Command>\n", esc(&a.path));
        if !a.args.is_empty() {
            x += &format!("      <Arguments>{}</Arguments>\n", esc(&a.args));
        }
        if !a.workdir.is_empty() {
            x += &format!(
                "      <WorkingDirectory>{}</WorkingDirectory>\n",
                esc(&a.workdir)
            );
        }
        x += "    </Exec>\n";
    }
    x += "  </Actions>\n</Task>\n";
    x
}

/// Contenus des éléments `<tag>` (les éléments vides `<tag/>` donnent "").
fn elements<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = xml;
    let open = format!("<{tag}");
    while let Some(i) = rest.find(&open) {
        let after = &rest[i + open.len()..];
        let Some(c) = after.chars().next() else {
            break;
        };
        if !matches!(c, '>' | ' ' | '/' | '\t' | '\r' | '\n') {
            rest = after;
            continue;
        }
        let Some(gt) = after.find('>') else {
            break;
        };
        if after[..gt].ends_with('/') {
            out.push("");
            rest = &after[gt + 1..];
            continue;
        }
        let body = &after[gt + 1..];
        let close = format!("</{tag}>");
        let Some(end) = body.find(&close) else {
            break;
        };
        out.push(&body[..end]);
        rest = &body[end + close.len()..];
    }
    out
}

fn text_of(xml: &str, tag: &str) -> Option<String> {
    elements(xml, tag).first().map(|s| unesc(s.trim()))
}

fn bool_of(xml: &str, tag: &str, default: bool) -> bool {
    text_of(xml, tag).map_or(default, |v| v == "true" || v == "1")
}

fn parse_xml(xml: &str) -> Option<DefData> {
    let task = elements(xml, "Task").into_iter().next()?;
    let mut d = DefData::default();
    if let Some(reg) = elements(task, "RegistrationInfo").first() {
        d.reg.description = text_of(reg, "Description").unwrap_or_default();
        d.reg.author = text_of(reg, "Author").unwrap_or_default();
        d.reg.version = text_of(reg, "Version").unwrap_or_default();
        d.reg.date = text_of(reg, "Date").unwrap_or_default();
        d.reg.uri = text_of(reg, "URI").unwrap_or_default();
    }
    let triggers = elements(task, "Triggers").first().copied().unwrap_or("");
    let mut push = |kind: i32, body: &str| {
        let mut t = TrigData::new(kind);
        t.start = text_of(body, "StartBoundary").unwrap_or_default();
        t.end = text_of(body, "EndBoundary").unwrap_or_default();
        t.enabled = bool_of(body, "Enabled", true);
        if let Some(rep) = elements(body, "Repetition").first() {
            t.rep_interval = text_of(rep, "Interval").unwrap_or_default();
        }
        t.subscription = text_of(body, "Subscription").unwrap_or_default();
        if let Some(day) = elements(body, "ScheduleByDay").first() {
            t.kind = TRIGGER_DAILY;
            t.days_interval = text_of(day, "DaysInterval")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);
        }
        if let Some(week) = elements(body, "ScheduleByWeek").first() {
            t.kind = TRIGGER_WEEKLY;
            for (i, name) in DAY_NAMES.iter().enumerate() {
                if !elements(week, name).is_empty() {
                    t.days_of_week |= 1 << i;
                }
            }
            t.weeks_interval = text_of(week, "WeeksInterval")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);
        }
        if kind == -1 && t.kind == -1 {
            return; // calendrier mensuel : non pris en charge
        }
        d.triggers.push(Arc::new(Mutex::new(t)));
    };
    for (tag, kind) in [
        ("TimeTrigger", TRIGGER_TIME),
        ("CalendarTrigger", -1),
        ("BootTrigger", TRIGGER_BOOT),
        ("LogonTrigger", TRIGGER_LOGON),
        ("RegistrationTrigger", TRIGGER_REGISTRATION),
        ("EventTrigger", TRIGGER_EVENT),
    ] {
        for body in elements(triggers, tag) {
            push(kind, body);
        }
    }
    if let Some(s) = elements(task, "Settings").first() {
        let def = SetData::default();
        d.settings.enabled = bool_of(s, "Enabled", true);
        d.settings.disallow_battery =
            bool_of(s, "DisallowStartIfOnBatteries", def.disallow_battery);
        d.settings.stop_on_battery = bool_of(s, "StopIfGoingOnBatteries", def.stop_on_battery);
        d.settings.network = bool_of(s, "RunOnlyIfNetworkAvailable", false);
        d.settings.hidden = bool_of(s, "Hidden", false);
        d.settings.allow_demand = bool_of(s, "AllowStartOnDemand", true);
        if let Some(limit) = text_of(s, "ExecutionTimeLimit") {
            d.settings.time_limit = limit;
        }
    }
    if let Some(actions) = elements(task, "Actions").first() {
        for exec in elements(actions, "Exec") {
            d.actions.push(Arc::new(Mutex::new(ActData {
                id: String::new(),
                path: text_of(exec, "Command").unwrap_or_default(),
                args: text_of(exec, "Arguments").unwrap_or_default(),
                workdir: text_of(exec, "WorkingDirectory").unwrap_or_default(),
            })));
        }
    }
    Some(d)
}

// ---------- Dossiers ----------

fn norm_folder(path: &str) -> String {
    let parts: Vec<&str> = path.split(['\\', '/']).filter(|p| !p.is_empty()).collect();
    format!("\\{}", parts.join("\\"))
}

fn join_folder(base: &str, rel: &str) -> String {
    if rel.starts_with('\\') || rel.starts_with('/') {
        norm_folder(rel)
    } else {
        norm_folder(&format!("{base}\\{rel}"))
    }
}

fn folders_file() -> std::path::PathBuf {
    admin::path(tasks::TASKS_DIR).join("folders")
}

fn known_folders() -> Vec<String> {
    let mut list: Vec<String> = admin::read_text(&folders_file())
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.is_empty())
        .map(norm_folder)
        .collect();
    for t in tasks::load_all() {
        // Chaque dossier parent d'une tâche existe aussi.
        let mut f = norm_folder(&t.folder);
        while f != "\\" {
            list.push(f.clone());
            f = match f.rfind('\\') {
                Some(0) | None => "\\".into(),
                Some(i) => f[..i].to_string(),
            };
        }
    }
    list.push("\\".into());
    list.sort_by_key(|f| f.to_lowercase());
    list.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    list
}

fn folder_exists(path: &str) -> bool {
    known_folders().iter().any(|f| f.eq_ignore_ascii_case(path))
}

fn tasks_in(folder: &str) -> Vec<Task> {
    tasks::load_all()
        .into_iter()
        .filter(|t| norm_folder(&t.folder).eq_ignore_ascii_case(folder))
        .collect()
}

/// Sépare `Sous\Dossier\Nom` relatif à `base` en (dossier, nom).
fn split_task_path(base: &str, path: &str) -> (String, String) {
    let full = join_folder(base, path);
    match full.rfind('\\') {
        Some(0) => ("\\".into(), full[1..].to_string()),
        Some(i) => (full[..i].to_string(), full[i + 1..].to_string()),
        None => ("\\".into(), full),
    }
}

fn find_task(folder: &str, name: &str) -> Option<Task> {
    tasks_in(folder)
        .into_iter()
        .find(|t| t.name.eq_ignore_ascii_case(name))
}

// ---------- Objets COM ----------

enum Kind {
    Service(AtomicBool),
    Folder(String),
    FolderCollection(Vec<String>),
    Registered(String),
    RegisteredCollection(Vec<String>),
    Definition(Def),
    RegInfo(Def),
    Settings(Def),
    Principal(Def),
    Triggers(Def),
    Trigger(Arc<Mutex<TrigData>>),
    Repetition(Arc<Mutex<TrigData>>),
    Actions(Def),
    Action(Arc<Mutex<ActData>>),
    Running(String),
    RunningCollection(Vec<String>),
}

#[repr(C)]
struct Obj {
    vtbl: *const usize,
    refs: AtomicU32,
    kind: Kind,
}

impl Kind {
    fn iids(&self) -> &'static [Guid] {
        match self {
            Kind::Service(_) => &[IID_ITASKSERVICE],
            Kind::Folder(_) => &[IID_ITASKFOLDER],
            Kind::FolderCollection(_) => &[IID_ITASKFOLDERCOLLECTION],
            Kind::Registered(_) => &[IID_IREGISTEREDTASK],
            Kind::RegisteredCollection(_) => &[IID_IREGISTEREDTASKCOLLECTION],
            Kind::Definition(_) => &[IID_ITASKDEFINITION],
            Kind::RegInfo(_) => &[IID_IREGISTRATIONINFO],
            Kind::Settings(_) => &[IID_ITASKSETTINGS],
            Kind::Principal(_) => &[IID_IPRINCIPAL],
            Kind::Triggers(_) => &[IID_ITRIGGERCOLLECTION],
            Kind::Trigger(t) => match lock(t).kind {
                TRIGGER_TIME => &[IID_ITRIGGER, IID_ITIMETRIGGER],
                TRIGGER_DAILY => &[IID_ITRIGGER, IID_IDAILYTRIGGER],
                TRIGGER_WEEKLY => &[IID_ITRIGGER, IID_IWEEKLYTRIGGER],
                TRIGGER_BOOT => &[IID_ITRIGGER, IID_IBOOTTRIGGER],
                TRIGGER_LOGON => &[IID_ITRIGGER, IID_ILOGONTRIGGER],
                TRIGGER_EVENT => &[IID_ITRIGGER, IID_IEVENTTRIGGER],
                TRIGGER_REGISTRATION => &[IID_ITRIGGER, IID_IREGISTRATIONTRIGGER],
                _ => &[IID_ITRIGGER],
            },
            Kind::Repetition(_) => &[IID_IREPETITIONPATTERN],
            Kind::Actions(_) => &[IID_IACTIONCOLLECTION],
            Kind::Action(_) => &[IID_IACTION, IID_IEXECACTION],
            Kind::Running(_) => &[IID_IRUNNINGTASK],
            Kind::RunningCollection(_) => &[IID_IRUNNINGTASKCOLLECTION],
        }
    }

    fn vtable(&self) -> *const usize {
        match self {
            Kind::Service(_) => vt(&VT_SERVICE, service_vt),
            Kind::Folder(_) => vt(&VT_FOLDER, folder_vt),
            Kind::FolderCollection(_)
            | Kind::RegisteredCollection(_)
            | Kind::RunningCollection(_) => vt(&VT_COLLECTION, collection_vt),
            Kind::Registered(_) => vt(&VT_REGISTERED, registered_vt),
            Kind::Definition(_) => vt(&VT_DEFINITION, definition_vt),
            Kind::RegInfo(_) => vt(&VT_REGINFO, reginfo_vt),
            Kind::Settings(_) => vt(&VT_SETTINGS, settings_vt),
            Kind::Principal(_) => vt(&VT_PRINCIPAL, principal_vt),
            Kind::Triggers(_) => vt(&VT_TRIGGERS, triggers_vt),
            Kind::Trigger(t) => match lock(t).kind {
                TRIGGER_DAILY => vt(&VT_TRIGGER_DAILY, || trigger_vt(TRIGGER_DAILY)),
                TRIGGER_WEEKLY => vt(&VT_TRIGGER_WEEKLY, || trigger_vt(TRIGGER_WEEKLY)),
                TRIGGER_BOOT | TRIGGER_REGISTRATION => {
                    vt(&VT_TRIGGER_DELAY, || trigger_vt(TRIGGER_BOOT))
                }
                TRIGGER_LOGON => vt(&VT_TRIGGER_LOGON, || trigger_vt(TRIGGER_LOGON)),
                TRIGGER_EVENT => vt(&VT_TRIGGER_EVENT, || trigger_vt(TRIGGER_EVENT)),
                _ => vt(&VT_TRIGGER_TIME, || trigger_vt(TRIGGER_TIME)),
            },
            Kind::Repetition(_) => vt(&VT_REPETITION, repetition_vt),
            Kind::Actions(_) => vt(&VT_ACTIONS, actions_vt),
            Kind::Action(_) => vt(&VT_ACTION, action_vt),
            Kind::Running(_) => vt(&VT_RUNNING, running_vt),
        }
    }
}

type VTable = OnceLock<Vec<usize>>;
static VT_SERVICE: VTable = OnceLock::new();
static VT_FOLDER: VTable = OnceLock::new();
static VT_COLLECTION: VTable = OnceLock::new();
static VT_REGISTERED: VTable = OnceLock::new();
static VT_DEFINITION: VTable = OnceLock::new();
static VT_REGINFO: VTable = OnceLock::new();
static VT_SETTINGS: VTable = OnceLock::new();
static VT_PRINCIPAL: VTable = OnceLock::new();
static VT_TRIGGERS: VTable = OnceLock::new();
static VT_TRIGGER_TIME: VTable = OnceLock::new();
static VT_TRIGGER_DAILY: VTable = OnceLock::new();
static VT_TRIGGER_WEEKLY: VTable = OnceLock::new();
static VT_TRIGGER_DELAY: VTable = OnceLock::new();
static VT_TRIGGER_LOGON: VTable = OnceLock::new();
static VT_TRIGGER_EVENT: VTable = OnceLock::new();
static VT_REPETITION: VTable = OnceLock::new();
static VT_ACTIONS: VTable = OnceLock::new();
static VT_ACTION: VTable = OnceLock::new();
static VT_RUNNING: VTable = OnceLock::new();

fn vt(cell: &'static VTable, build: impl FnOnce() -> Vec<usize>) -> *const usize {
    cell.get_or_init(build).as_ptr()
}

fn new_obj(kind: Kind) -> *mut c_void {
    let vtbl = kind.vtable();
    Box::into_raw(Box::new(Obj {
        vtbl,
        refs: AtomicU32::new(1),
        kind,
    })) as *mut c_void
}

unsafe fn obj<'a>(this: *mut Obj) -> &'a Obj {
    &*this
}

unsafe fn out_obj(out: *mut *mut c_void, kind: Kind) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_obj(kind);
    S_OK
}

// ----- IUnknown / IDispatch -----

unsafe extern "win64" fn qi(this: *mut Obj, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if iid.is_null() {
        return E_INVALIDARG;
    }
    let iid = *iid;
    let o = obj(this);
    if iid == IID_IUNKNOWN || iid == IID_IDISPATCH || o.kind.iids().contains(&iid) {
        o.refs.fetch_add(1, Ordering::SeqCst);
        *out = this as *mut c_void;
        S_OK
    } else {
        E_NOINTERFACE
    }
}

unsafe extern "win64" fn add_ref(this: *mut Obj) -> u32 {
    obj(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}

unsafe extern "win64" fn release(this: *mut Obj) -> u32 {
    let left = obj(this).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if left == 0 {
        drop(Box::from_raw(this));
    }
    left
}

unsafe extern "win64" fn type_info_count(_: *mut Obj, out: *mut u32) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    S_OK
}

unsafe extern "win64" fn not_impl(_: *mut Obj) -> i32 {
    E_NOTIMPL
}

unsafe extern "win64" fn put_ignored(_: *mut Obj, _: usize) -> i32 {
    S_OK
}

unsafe extern "win64" fn get_empty_bstr(_: *mut Obj, out: *mut *mut u16) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    S_OK
}

unsafe extern "win64" fn get_no_object(_: *mut Obj, out: *mut *mut c_void) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

fn dispatch_head() -> Vec<usize> {
    vec![
        qi as *const () as usize,
        add_ref as *const () as usize,
        release as *const () as usize,
        type_info_count as *const () as usize,
        not_impl as *const () as usize,
        not_impl as *const () as usize,
        not_impl as *const () as usize,
    ]
}

// ----- Conversions de types COM -----

unsafe fn bstr_in(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut n = 0;
    while n < 32_768 && *p.add(n) != 0 {
        n += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
}

unsafe fn bstr_out(out: *mut *mut u16, s: &str) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = s.encode_utf16().collect();
    *out = crate::dlls::oleaut32::sys_alloc_string_len(w.as_ptr(), w.len() as u32);
    S_OK
}

unsafe fn bool_out(out: *mut i16, v: bool) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = if v { VARIANT_TRUE } else { 0 };
    S_OK
}

unsafe fn int_out<T>(out: *mut T, v: T) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = v;
    S_OK
}

enum Index {
    Number(i64),
    Name(String),
    None,
}

/// VARIANT passé par valeur (donc par adresse en x64).
unsafe fn variant(v: *const u8) -> Index {
    if v.is_null() {
        return Index::None;
    }
    let vt = *(v as *const u16);
    let data = v.add(8);
    match vt {
        2 => Index::Number(*(data as *const i16) as i64),
        3 | 22 => Index::Number(*(data as *const i32) as i64),
        19 | 23 => Index::Number(*(data as *const u32) as i64),
        8 => Index::Name(bstr_in(*(data as *const *const u16))),
        _ => Index::None,
    }
}

// ----- ITaskService -----

fn service_vt() -> Vec<usize> {
    let mut v = dispatch_head();
    v.extend([
        service_get_folder as *const () as usize,
        service_get_running as *const () as usize,
        service_new_task as *const () as usize,
        service_connect as *const () as usize,
        service_connected as *const () as usize,
        service_target as *const () as usize,
        service_user as *const () as usize,
        service_target as *const () as usize,
        service_version as *const () as usize,
    ]);
    v
}

fn connected(this: *mut Obj) -> bool {
    // SAFETY: `this` est un objet service vivant.
    match unsafe { &obj(this).kind } {
        Kind::Service(c) => c.load(Ordering::SeqCst),
        _ => false,
    }
}

unsafe extern "win64" fn service_get_folder(
    this: *mut Obj,
    path: *const u16,
    out: *mut *mut c_void,
) -> i32 {
    if !connected(this) {
        return ERROR_ONLY_IF_CONNECTED;
    }
    let path = norm_folder(&bstr_in(path));
    if !folder_exists(&path) {
        return ERROR_FILE_NOT_FOUND;
    }
    out_obj(out, Kind::Folder(path))
}

fn running_ids() -> Vec<String> {
    tasks::load_all()
        .into_iter()
        .filter(|t| tasks::running_pid(&t.id).is_some())
        .map(|t| t.id)
        .collect()
}

unsafe extern "win64" fn service_get_running(
    this: *mut Obj,
    _flags: i32,
    out: *mut *mut c_void,
) -> i32 {
    if !connected(this) {
        return ERROR_ONLY_IF_CONNECTED;
    }
    out_obj(out, Kind::RunningCollection(running_ids()))
}

unsafe extern "win64" fn service_new_task(
    this: *mut Obj,
    flags: u32,
    out: *mut *mut c_void,
) -> i32 {
    if !connected(this) {
        return ERROR_ONLY_IF_CONNECTED;
    }
    if flags != 0 {
        return E_INVALIDARG;
    }
    out_obj(out, Kind::Definition(Arc::default()))
}

unsafe extern "win64" fn service_connect(this: *mut Obj, server: *const u8) -> i32 {
    // Seul l'ordinateur local est accessible.
    if let Index::Name(s) = variant(server) {
        let host = hostname();
        if !s.is_empty() && !s.eq_ignore_ascii_case(&host) && s != "localhost" && s != "." {
            return E_ACCESSDENIED;
        }
    }
    if let Kind::Service(c) = &obj(this).kind {
        c.store(true, Ordering::SeqCst);
    }
    S_OK
}

unsafe extern "win64" fn service_connected(this: *mut Obj, out: *mut i16) -> i32 {
    bool_out(out, connected(this))
}

fn hostname() -> String {
    admin::read_text(std::path::Path::new("/proc/sys/kernel/hostname"))
        .map(|h| h.trim().to_string())
        .unwrap_or_else(|| "winlux".into())
}

unsafe extern "win64" fn service_target(_: *mut Obj, out: *mut *mut u16) -> i32 {
    bstr_out(out, &hostname())
}

unsafe extern "win64" fn service_user(_: *mut Obj, out: *mut *mut u16) -> i32 {
    bstr_out(
        out,
        &std::env::var("USER").unwrap_or_else(|_| "root".into()),
    )
}

unsafe extern "win64" fn service_version(_: *mut Obj, out: *mut u32) -> i32 {
    int_out(out, 0x0001_0004)
}

// ----- ITaskFolder -----

fn folder_vt() -> Vec<usize> {
    let mut v = dispatch_head();
    v.extend([
        folder_name as *const () as usize,
        folder_path as *const () as usize,
        folder_get_folder as *const () as usize,
        folder_get_folders as *const () as usize,
        folder_create_folder as *const () as usize,
        folder_delete_folder as *const () as usize,
        folder_get_task as *const () as usize,
        folder_get_tasks as *const () as usize,
        folder_delete_task as *const () as usize,
        folder_register_task as *const () as usize,
        folder_register_definition as *const () as usize,
        get_empty_bstr as *const () as usize,
        put_ignored as *const () as usize,
    ]);
    v
}

unsafe fn folder_of(this: *mut Obj) -> String {
    match &obj(this).kind {
        Kind::Folder(p) => p.clone(),
        _ => "\\".into(),
    }
}

unsafe extern "win64" fn folder_name(this: *mut Obj, out: *mut *mut u16) -> i32 {
    let p = folder_of(this);
    let name = if p == "\\" {
        "\\".to_string()
    } else {
        p.rsplit('\\').next().unwrap_or("").to_string()
    };
    bstr_out(out, &name)
}

unsafe extern "win64" fn folder_path(this: *mut Obj, out: *mut *mut u16) -> i32 {
    bstr_out(out, &folder_of(this))
}

unsafe extern "win64" fn folder_get_folder(
    this: *mut Obj,
    path: *const u16,
    out: *mut *mut c_void,
) -> i32 {
    let p = join_folder(&folder_of(this), &bstr_in(path));
    if !folder_exists(&p) {
        return ERROR_FILE_NOT_FOUND;
    }
    out_obj(out, Kind::Folder(p))
}

unsafe extern "win64" fn folder_get_folders(
    this: *mut Obj,
    _flags: i32,
    out: *mut *mut c_void,
) -> i32 {
    let base = folder_of(this);
    let prefix = if base == "\\" {
        "\\".to_string()
    } else {
        format!("{base}\\")
    };
    let children = known_folders()
        .into_iter()
        .filter(|f| {
            f.len() > prefix.len()
                && f[..prefix.len()].eq_ignore_ascii_case(&prefix)
                && !f[prefix.len()..].contains('\\')
        })
        .collect();
    out_obj(out, Kind::FolderCollection(children))
}

unsafe extern "win64" fn folder_create_folder(
    this: *mut Obj,
    name: *const u16,
    _sddl: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    let p = join_folder(&folder_of(this), &bstr_in(name));
    if p == "\\" {
        return E_INVALIDARG;
    }
    if folder_exists(&p) {
        return ERROR_ALREADY_EXISTS;
    }
    admin::append_line(&folders_file(), &p, 64 * 1024);
    if out.is_null() {
        return S_OK;
    }
    out_obj(out, Kind::Folder(p))
}

unsafe extern "win64" fn folder_delete_folder(
    this: *mut Obj,
    name: *const u16,
    _flags: i32,
) -> i32 {
    let p = join_folder(&folder_of(this), &bstr_in(name));
    if !folder_exists(&p) {
        return ERROR_FILE_NOT_FOUND;
    }
    if !tasks_in(&p).is_empty() {
        return 0x8007_0091u32 as i32; // ERROR_DIR_NOT_EMPTY
    }
    let kept: Vec<String> = admin::read_text(&folders_file())
        .unwrap_or_default()
        .lines()
        .filter(|l| !norm_folder(l).eq_ignore_ascii_case(&p))
        .map(|l| format!("{l}\n"))
        .collect();
    let _ = admin::write_text(&folders_file(), &kept.concat());
    S_OK
}

unsafe extern "win64" fn folder_get_task(
    this: *mut Obj,
    path: *const u16,
    out: *mut *mut c_void,
) -> i32 {
    let (folder, name) = split_task_path(&folder_of(this), &bstr_in(path));
    match find_task(&folder, &name) {
        Some(t) => out_obj(out, Kind::Registered(t.id)),
        None => ERROR_FILE_NOT_FOUND,
    }
}

unsafe extern "win64" fn folder_get_tasks(
    this: *mut Obj,
    _flags: i32,
    out: *mut *mut c_void,
) -> i32 {
    let ids = tasks_in(&folder_of(this))
        .into_iter()
        .map(|t| t.id)
        .collect();
    out_obj(out, Kind::RegisteredCollection(ids))
}

unsafe extern "win64" fn folder_delete_task(this: *mut Obj, name: *const u16, _flags: i32) -> i32 {
    let (folder, name) = split_task_path(&folder_of(this), &bstr_in(name));
    match find_task(&folder, &name) {
        Some(t) if tasks::delete(&t.id) => S_OK,
        _ => ERROR_FILE_NOT_FOUND,
    }
}

/// Enregistre une définition sous `path` (relatif au dossier `base`).
fn register(base: &str, path: &str, def: &DefData, flags: i32) -> Result<String, i32> {
    let path = if path.trim().is_empty() {
        // Nom absent : Windows génère un GUID.
        let mut guid = Guid::default();
        // SAFETY: écrit dans une structure locale.
        unsafe { crate::dlls::ole32::co_create_guid(&mut guid) };
        format!(
            "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{}}}",
            guid.data1,
            guid.data2,
            guid.data3,
            guid.data4[0],
            guid.data4[1],
            guid.data4[2..]
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<String>()
        )
    } else {
        path.to_string()
    };
    let (folder, name) = split_task_path(base, &path);
    if name.is_empty() {
        return Err(E_INVALIDARG);
    }
    let existing = find_task(&folder, &name);
    if existing.is_some() && flags & TASK_UPDATE == 0 {
        return Err(ERROR_ALREADY_EXISTS);
    }
    if existing.is_none() && flags & TASK_CREATE == 0 {
        return Err(ERROR_FILE_NOT_FOUND);
    }
    let (mut task, run_now) = def_to_task(def, &folder, &name);
    if let Some(old) = existing {
        task.id = old.id;
    }
    if flags & TASK_DISABLE != 0 {
        task.enabled = false;
    }
    tasks::save(&task).map_err(|_| E_ACCESSDENIED)?;
    if run_now && task.enabled {
        start_task(&task.id);
    }
    Ok(task.id)
}

unsafe extern "win64" fn folder_register_task(
    this: *mut Obj,
    path: *const u16,
    xml: *const u16,
    flags: i32,
    _user: *const u8,
    _password: *const u8,
    _logon: i32,
    _sddl: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    let Some(def) = parse_xml(&bstr_in(xml)) else {
        return SCHED_E_MALFORMEDXML;
    };
    match register(&folder_of(this), &bstr_in(path), &def, flags) {
        Ok(id) if !out.is_null() => out_obj(out, Kind::Registered(id)),
        Ok(_) => S_OK,
        Err(e) => e,
    }
}

unsafe extern "win64" fn folder_register_definition(
    this: *mut Obj,
    path: *const u16,
    definition: *mut Obj,
    flags: i32,
    _user: *const u8,
    _password: *const u8,
    _logon: i32,
    _sddl: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if definition.is_null() {
        return E_POINTER;
    }
    let Kind::Definition(def) = &obj(definition).kind else {
        return E_INVALIDARG;
    };
    let result = register(&folder_of(this), &bstr_in(path), &lock(def), flags);
    match result {
        Ok(id) if !out.is_null() => out_obj(out, Kind::Registered(id)),
        Ok(_) => S_OK,
        Err(e) => e,
    }
}

// ----- Collections (dossiers, tâches, tâches en cours) -----

fn collection_vt() -> Vec<usize> {
    let mut v = dispatch_head();
    v.extend([
        collection_count as *const () as usize,
        collection_item as *const () as usize,
        get_no_object as *const () as usize,
    ]);
    v
}

unsafe extern "win64" fn collection_count(this: *mut Obj, out: *mut i32) -> i32 {
    let n = match &obj(this).kind {
        Kind::FolderCollection(v) | Kind::RegisteredCollection(v) | Kind::RunningCollection(v) => {
            v.len()
        }
        _ => 0,
    };
    int_out(out, n as i32)
}

unsafe extern "win64" fn collection_item(
    this: *mut Obj,
    index: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    let (items, make): (&Vec<String>, fn(String) -> Kind) = match &obj(this).kind {
        Kind::FolderCollection(v) => (v, Kind::Folder),
        Kind::RegisteredCollection(v) => (v, Kind::Registered),
        Kind::RunningCollection(v) => (v, Kind::Running),
        _ => return E_INVALIDARG,
    };
    let found = match variant(index) {
        Index::Number(n) if n >= 1 => items.get(n as usize - 1).cloned(),
        Index::Name(name) => items
            .iter()
            .find(|id| {
                id.eq_ignore_ascii_case(&name)
                    || tasks::load(id).is_some_and(|t| t.name.eq_ignore_ascii_case(&name))
                    || id
                        .rsplit('\\')
                        .next()
                        .is_some_and(|n| n.eq_ignore_ascii_case(&name))
            })
            .cloned(),
        _ => None,
    };
    match found {
        Some(item) => out_obj(out, make(item)),
        None => E_INVALIDARG,
    }
}

// ----- IRegisteredTask -----

fn registered_vt() -> Vec<usize> {
    let mut v = dispatch_head();
    v.extend([
        reg_name as *const () as usize,
        reg_path as *const () as usize,
        reg_state as *const () as usize,
        reg_enabled as *const () as usize,
        reg_put_enabled as *const () as usize,
        reg_run as *const () as usize,
        reg_run_ex as *const () as usize,
        reg_instances as *const () as usize,
        reg_last_run as *const () as usize,
        reg_last_result as *const () as usize,
        reg_missed as *const () as usize,
        reg_next_run as *const () as usize,
        reg_definition as *const () as usize,
        reg_xml as *const () as usize,
        get_empty_bstr as *const () as usize,
        put_ignored as *const () as usize,
        reg_stop as *const () as usize,
        not_impl as *const () as usize,
    ]);
    v
}

unsafe fn registered(this: *mut Obj) -> Option<Task> {
    match &obj(this).kind {
        Kind::Registered(id) | Kind::Running(id) => tasks::load(id),
        _ => None,
    }
}

unsafe extern "win64" fn reg_name(this: *mut Obj, out: *mut *mut u16) -> i32 {
    match registered(this) {
        Some(t) => bstr_out(out, &t.name),
        None => ERROR_FILE_NOT_FOUND,
    }
}

unsafe extern "win64" fn reg_path(this: *mut Obj, out: *mut *mut u16) -> i32 {
    match registered(this) {
        Some(t) => bstr_out(out, &t.full_path()),
        None => ERROR_FILE_NOT_FOUND,
    }
}

fn state_of(t: &Task) -> i32 {
    if tasks::running_pid(&t.id).is_some() {
        4
    } else if t.enabled {
        3
    } else {
        1
    }
}

unsafe extern "win64" fn reg_state(this: *mut Obj, out: *mut i32) -> i32 {
    match registered(this) {
        Some(t) => int_out(out, state_of(&t)),
        None => ERROR_FILE_NOT_FOUND,
    }
}

unsafe extern "win64" fn reg_enabled(this: *mut Obj, out: *mut i16) -> i32 {
    match registered(this) {
        Some(t) => bool_out(out, t.enabled),
        None => ERROR_FILE_NOT_FOUND,
    }
}

unsafe extern "win64" fn reg_put_enabled(this: *mut Obj, value: i16) -> i32 {
    match registered(this) {
        Some(mut t) => {
            t.enabled = value != 0;
            if tasks::save(&t).is_ok() {
                S_OK
            } else {
                E_ACCESSDENIED
            }
        }
        None => ERROR_FILE_NOT_FOUND,
    }
}

/// Lance `winlux --tasks run ID` en arrière-plan.
fn start_task(id: &str) -> bool {
    let exe = std::env::current_exe().unwrap_or_else(|_| "winlux".into());
    match std::process::Command::new(exe)
        .args(["--tasks", "run", id])
        .stdin(std::process::Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            true
        }
        Err(_) => false,
    }
}

unsafe extern "win64" fn reg_run(this: *mut Obj, _params: *const u8, out: *mut *mut c_void) -> i32 {
    let Some(t) = registered(this) else {
        return ERROR_FILE_NOT_FOUND;
    };
    if !t.enabled {
        return 0x8004_1326u32 as i32; // SCHED_E_TASK_DISABLED
    }
    if !start_task(&t.id) {
        return E_ACCESSDENIED;
    }
    if out.is_null() {
        S_OK
    } else {
        out_obj(out, Kind::Running(t.id))
    }
}

unsafe extern "win64" fn reg_run_ex(
    this: *mut Obj,
    params: *const u8,
    _flags: i32,
    _session: i32,
    _user: *const u16,
    out: *mut *mut c_void,
) -> i32 {
    reg_run(this, params, out)
}

unsafe extern "win64" fn reg_instances(this: *mut Obj, _flags: i32, out: *mut *mut c_void) -> i32 {
    let Some(t) = registered(this) else {
        return ERROR_FILE_NOT_FOUND;
    };
    let items = if tasks::running_pid(&t.id).is_some() {
        vec![t.id]
    } else {
        Vec::new()
    };
    out_obj(out, Kind::RunningCollection(items))
}

unsafe extern "win64" fn reg_last_run(this: *mut Obj, out: *mut f64) -> i32 {
    let Some(t) = registered(this) else {
        return ERROR_FILE_NOT_FOUND;
    };
    match tasks::last_run(&t.id) {
        Some(l) => int_out(out, ole_date(l.time)),
        None => {
            int_out(out, 0.0);
            SCHED_S_TASK_HAS_NOT_RUN
        }
    }
}

unsafe extern "win64" fn reg_last_result(this: *mut Obj, out: *mut i32) -> i32 {
    let Some(t) = registered(this) else {
        return ERROR_FILE_NOT_FOUND;
    };
    int_out(
        out,
        tasks::last_run(&t.id).map_or(SCHED_S_TASK_HAS_NOT_RUN, |l| l.result),
    )
}

unsafe extern "win64" fn reg_missed(_: *mut Obj, out: *mut i32) -> i32 {
    int_out(out, 0)
}

unsafe extern "win64" fn reg_next_run(this: *mut Obj, out: *mut f64) -> i32 {
    let Some(t) = registered(this) else {
        return ERROR_FILE_NOT_FOUND;
    };
    match t.next_run(admin::now(), None) {
        Some(n) => int_out(out, ole_date(n)),
        None => {
            int_out(out, 0.0);
            S_FALSE
        }
    }
}

unsafe extern "win64" fn reg_definition(this: *mut Obj, out: *mut *mut c_void) -> i32 {
    match registered(this) {
        Some(t) => out_obj(out, Kind::Definition(Arc::new(Mutex::new(task_to_def(&t))))),
        None => ERROR_FILE_NOT_FOUND,
    }
}

unsafe extern "win64" fn reg_xml(this: *mut Obj, out: *mut *mut u16) -> i32 {
    match registered(this) {
        Some(t) => bstr_out(out, &def_xml(&task_to_def(&t))),
        None => ERROR_FILE_NOT_FOUND,
    }
}

unsafe extern "win64" fn reg_stop(this: *mut Obj, _flags: i32) -> i32 {
    if let Some(t) = registered(this) {
        tasks::end_task(&t.id);
    }
    S_OK
}

// ----- IRunningTask -----

fn running_vt() -> Vec<usize> {
    let mut v = dispatch_head();
    v.extend([
        reg_name as *const () as usize,
        get_empty_bstr as *const () as usize,
        reg_path as *const () as usize,
        reg_state as *const () as usize,
        get_empty_bstr as *const () as usize,
        running_stop as *const () as usize,
        running_refresh as *const () as usize,
        running_pid as *const () as usize,
    ]);
    v
}

unsafe extern "win64" fn running_stop(this: *mut Obj) -> i32 {
    reg_stop(this, 0)
}

unsafe extern "win64" fn running_refresh(_: *mut Obj) -> i32 {
    S_OK
}

unsafe extern "win64" fn running_pid(this: *mut Obj, out: *mut u32) -> i32 {
    let pid = registered(this)
        .and_then(|t| tasks::running_pid(&t.id))
        .unwrap_or(0);
    int_out(out, pid as u32)
}

// ----- ITaskDefinition -----

fn definition_vt() -> Vec<usize> {
    let mut v = dispatch_head();
    v.extend([
        def_reg_info as *const () as usize,
        not_impl as *const () as usize,
        def_triggers as *const () as usize,
        not_impl as *const () as usize,
        def_settings as *const () as usize,
        not_impl as *const () as usize,
        def_get_data as *const () as usize,
        def_put_data as *const () as usize,
        def_principal as *const () as usize,
        not_impl as *const () as usize,
        def_actions as *const () as usize,
        not_impl as *const () as usize,
        def_get_xml as *const () as usize,
        def_put_xml as *const () as usize,
    ]);
    v
}

unsafe fn def_of(this: *mut Obj) -> Option<Def> {
    match &obj(this).kind {
        Kind::Definition(d)
        | Kind::RegInfo(d)
        | Kind::Settings(d)
        | Kind::Principal(d)
        | Kind::Triggers(d)
        | Kind::Actions(d) => Some(Arc::clone(d)),
        _ => None,
    }
}

macro_rules! def_child {
    ($name:ident, $kind:ident) => {
        unsafe extern "win64" fn $name(this: *mut Obj, out: *mut *mut c_void) -> i32 {
            match def_of(this) {
                Some(d) => out_obj(out, Kind::$kind(d)),
                None => E_INVALIDARG,
            }
        }
    };
}
def_child!(def_reg_info, RegInfo);
def_child!(def_triggers, Triggers);
def_child!(def_settings, Settings);
def_child!(def_principal, Principal);
def_child!(def_actions, Actions);

unsafe extern "win64" fn def_get_data(this: *mut Obj, out: *mut *mut u16) -> i32 {
    match def_of(this) {
        Some(d) => bstr_out(out, &lock(&d).data),
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn def_put_data(this: *mut Obj, v: *const u16) -> i32 {
    if let Some(d) = def_of(this) {
        lock(&d).data = bstr_in(v);
    }
    S_OK
}

unsafe extern "win64" fn def_get_xml(this: *mut Obj, out: *mut *mut u16) -> i32 {
    match def_of(this) {
        Some(d) => bstr_out(out, &def_xml(&lock(&d))),
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn def_put_xml(this: *mut Obj, v: *const u16) -> i32 {
    let Some(d) = def_of(this) else {
        return E_INVALIDARG;
    };
    match parse_xml(&bstr_in(v)) {
        Some(new) => {
            *lock(&d) = new;
            S_OK
        }
        None => SCHED_E_MALFORMEDXML,
    }
}

// ----- Accesseurs génériques -----

macro_rules! str_prop {
    ($get:ident, $put:ident, $lock:expr, $($field:ident).+) => {
        unsafe extern "win64" fn $get(this: *mut Obj, out: *mut *mut u16) -> i32 {
            match $lock(this) {
                Some(x) => bstr_out(out, &lock(&x).$($field).+),
                None => E_INVALIDARG,
            }
        }
        unsafe extern "win64" fn $put(this: *mut Obj, v: *const u16) -> i32 {
            match $lock(this) {
                Some(x) => {
                    lock(&x).$($field).+ = bstr_in(v);
                    S_OK
                }
                None => E_INVALIDARG,
            }
        }
    };
}

macro_rules! bool_prop {
    ($get:ident, $put:ident, $lock:expr, $($field:ident).+) => {
        unsafe extern "win64" fn $get(this: *mut Obj, out: *mut i16) -> i32 {
            match $lock(this) {
                Some(x) => bool_out(out, lock(&x).$($field).+),
                None => E_INVALIDARG,
            }
        }
        unsafe extern "win64" fn $put(this: *mut Obj, v: i16) -> i32 {
            match $lock(this) {
                Some(x) => {
                    lock(&x).$($field).+ = v != 0;
                    S_OK
                }
                None => E_INVALIDARG,
            }
        }
    };
}

macro_rules! int_prop {
    ($get:ident, $put:ident, $ty:ty, $lock:expr, $($field:ident).+) => {
        unsafe extern "win64" fn $get(this: *mut Obj, out: *mut $ty) -> i32 {
            match $lock(this) {
                Some(x) => int_out(out, lock(&x).$($field).+),
                None => E_INVALIDARG,
            }
        }
        unsafe extern "win64" fn $put(this: *mut Obj, v: $ty) -> i32 {
            match $lock(this) {
                Some(x) => {
                    lock(&x).$($field).+ = v;
                    S_OK
                }
                None => E_INVALIDARG,
            }
        }
    };
}

// ----- IRegistrationInfo -----

str_prop!(
    ri_get_description,
    ri_put_description,
    def_of,
    reg.description
);
str_prop!(ri_get_author, ri_put_author, def_of, reg.author);
str_prop!(ri_get_version, ri_put_version, def_of, reg.version);
str_prop!(ri_get_date, ri_put_date, def_of, reg.date);
str_prop!(ri_get_doc, ri_put_doc, def_of, reg.documentation);
str_prop!(ri_get_uri, ri_put_uri, def_of, reg.uri);
str_prop!(ri_get_source, ri_put_source, def_of, reg.source);

fn reginfo_vt() -> Vec<usize> {
    let mut v = dispatch_head();
    v.extend([
        ri_get_description as *const () as usize,
        ri_put_description as *const () as usize,
        ri_get_author as *const () as usize,
        ri_put_author as *const () as usize,
        ri_get_version as *const () as usize,
        ri_put_version as *const () as usize,
        ri_get_date as *const () as usize,
        ri_put_date as *const () as usize,
        ri_get_doc as *const () as usize,
        ri_put_doc as *const () as usize,
        get_empty_bstr as *const () as usize,
        put_ignored as *const () as usize,
        ri_get_uri as *const () as usize,
        ri_put_uri as *const () as usize,
        not_impl as *const () as usize,
        put_ignored as *const () as usize,
        ri_get_source as *const () as usize,
        ri_put_source as *const () as usize,
    ]);
    v
}

// ----- ITaskSettings -----

bool_prop!(ts_get_demand, ts_put_demand, def_of, settings.allow_demand);
str_prop!(
    ts_get_restart_interval,
    ts_put_restart_interval,
    def_of,
    settings.restart_interval
);
int_prop!(
    ts_get_restart_count,
    ts_put_restart_count,
    i32,
    def_of,
    settings.restart_count
);
int_prop!(
    ts_get_multiple,
    ts_put_multiple,
    i32,
    def_of,
    settings.multiple
);
bool_prop!(
    ts_get_stop_battery,
    ts_put_stop_battery,
    def_of,
    settings.stop_on_battery
);
bool_prop!(
    ts_get_disallow_battery,
    ts_put_disallow_battery,
    def_of,
    settings.disallow_battery
);
bool_prop!(
    ts_get_hard_terminate,
    ts_put_hard_terminate,
    def_of,
    settings.allow_hard_terminate
);
bool_prop!(
    ts_get_when_available,
    ts_put_when_available,
    def_of,
    settings.start_when_available
);
bool_prop!(ts_get_network, ts_put_network, def_of, settings.network);
str_prop!(
    ts_get_time_limit,
    ts_put_time_limit,
    def_of,
    settings.time_limit
);
bool_prop!(ts_get_enabled, ts_put_enabled, def_of, settings.enabled);
str_prop!(
    ts_get_delete_expired,
    ts_put_delete_expired,
    def_of,
    settings.delete_expired
);
int_prop!(
    ts_get_priority,
    ts_put_priority,
    i32,
    def_of,
    settings.priority
);
int_prop!(
    ts_get_compat,
    ts_put_compat,
    i32,
    def_of,
    settings.compatibility
);
bool_prop!(ts_get_hidden, ts_put_hidden, def_of, settings.hidden);
bool_prop!(ts_get_idle, ts_put_idle, def_of, settings.run_only_idle);
bool_prop!(ts_get_wake, ts_put_wake, def_of, settings.wake);

fn settings_vt() -> Vec<usize> {
    let mut v = dispatch_head();
    v.extend([
        ts_get_demand as *const () as usize,
        ts_put_demand as *const () as usize,
        ts_get_restart_interval as *const () as usize,
        ts_put_restart_interval as *const () as usize,
        ts_get_restart_count as *const () as usize,
        ts_put_restart_count as *const () as usize,
        ts_get_multiple as *const () as usize,
        ts_put_multiple as *const () as usize,
        ts_get_stop_battery as *const () as usize,
        ts_put_stop_battery as *const () as usize,
        ts_get_disallow_battery as *const () as usize,
        ts_put_disallow_battery as *const () as usize,
        ts_get_hard_terminate as *const () as usize,
        ts_put_hard_terminate as *const () as usize,
        ts_get_when_available as *const () as usize,
        ts_put_when_available as *const () as usize,
        get_empty_bstr as *const () as usize,
        put_ignored as *const () as usize,
        ts_get_network as *const () as usize,
        ts_put_network as *const () as usize,
        ts_get_time_limit as *const () as usize,
        ts_put_time_limit as *const () as usize,
        ts_get_enabled as *const () as usize,
        ts_put_enabled as *const () as usize,
        ts_get_delete_expired as *const () as usize,
        ts_put_delete_expired as *const () as usize,
        ts_get_priority as *const () as usize,
        ts_put_priority as *const () as usize,
        ts_get_compat as *const () as usize,
        ts_put_compat as *const () as usize,
        ts_get_hidden as *const () as usize,
        ts_put_hidden as *const () as usize,
        get_no_object as *const () as usize,
        put_ignored as *const () as usize,
        ts_get_idle as *const () as usize,
        ts_put_idle as *const () as usize,
        ts_get_wake as *const () as usize,
        ts_put_wake as *const () as usize,
        get_no_object as *const () as usize,
        put_ignored as *const () as usize,
    ]);
    v
}

// ----- IPrincipal -----

str_prop!(pr_get_id, pr_put_id, def_of, principal.id);
str_prop!(pr_get_display, pr_put_display, def_of, principal.display);
str_prop!(pr_get_user, pr_put_user, def_of, principal.user);
int_prop!(
    pr_get_logon,
    pr_put_logon,
    i32,
    def_of,
    principal.logon_type
);
str_prop!(pr_get_group, pr_put_group, def_of, principal.group);
int_prop!(pr_get_level, pr_put_level, i32, def_of, principal.run_level);

fn principal_vt() -> Vec<usize> {
    let mut v = dispatch_head();
    v.extend([
        pr_get_id as *const () as usize,
        pr_put_id as *const () as usize,
        pr_get_display as *const () as usize,
        pr_put_display as *const () as usize,
        pr_get_user as *const () as usize,
        pr_put_user as *const () as usize,
        pr_get_logon as *const () as usize,
        pr_put_logon as *const () as usize,
        pr_get_group as *const () as usize,
        pr_put_group as *const () as usize,
        pr_get_level as *const () as usize,
        pr_put_level as *const () as usize,
    ]);
    v
}

// ----- ITriggerCollection -----

fn triggers_vt() -> Vec<usize> {
    let mut v = dispatch_head();
    v.extend([
        tc_count as *const () as usize,
        tc_item as *const () as usize,
        get_no_object as *const () as usize,
        tc_create as *const () as usize,
        tc_remove as *const () as usize,
        tc_clear as *const () as usize,
    ]);
    v
}

unsafe extern "win64" fn tc_count(this: *mut Obj, out: *mut i32) -> i32 {
    match def_of(this) {
        Some(d) => int_out(out, lock(&d).triggers.len() as i32),
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn tc_item(this: *mut Obj, index: i32, out: *mut *mut c_void) -> i32 {
    let Some(d) = def_of(this) else {
        return E_INVALIDARG;
    };
    let item = lock(&d)
        .triggers
        .get((index as usize).wrapping_sub(1))
        .cloned();
    match item {
        Some(t) => out_obj(out, Kind::Trigger(t)),
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn tc_create(this: *mut Obj, kind: i32, out: *mut *mut c_void) -> i32 {
    let Some(d) = def_of(this) else {
        return E_INVALIDARG;
    };
    if !matches!(
        kind,
        TRIGGER_EVENT
            | TRIGGER_TIME
            | TRIGGER_DAILY
            | TRIGGER_WEEKLY
            | TRIGGER_REGISTRATION
            | TRIGGER_BOOT
            | TRIGGER_LOGON
    ) {
        crate::log_warn!("taskschd", "déclencheur de type {kind} non pris en charge");
        return E_NOTIMPL;
    }
    let t = Arc::new(Mutex::new(TrigData::new(kind)));
    lock(&d).triggers.push(Arc::clone(&t));
    out_obj(out, Kind::Trigger(t))
}

unsafe extern "win64" fn tc_remove(this: *mut Obj, index: *const u8) -> i32 {
    let Some(d) = def_of(this) else {
        return E_INVALIDARG;
    };
    let mut d = lock(&d);
    let pos = match variant(index) {
        Index::Number(n) if n >= 1 && (n as usize) <= d.triggers.len() => Some(n as usize - 1),
        Index::Name(id) => d.triggers.iter().position(|t| lock(t).id == id),
        _ => None,
    };
    match pos {
        Some(i) => {
            d.triggers.remove(i);
            S_OK
        }
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn tc_clear(this: *mut Obj) -> i32 {
    if let Some(d) = def_of(this) {
        lock(&d).triggers.clear();
    }
    S_OK
}

// ----- ITrigger et dérivés -----

unsafe fn trig_of(this: *mut Obj) -> Option<Arc<Mutex<TrigData>>> {
    match &obj(this).kind {
        Kind::Trigger(t) | Kind::Repetition(t) => Some(Arc::clone(t)),
        _ => None,
    }
}

str_prop!(tr_get_id, tr_put_id, trig_of, id);
str_prop!(tr_get_limit, tr_put_limit, trig_of, time_limit);
str_prop!(tr_get_start, tr_put_start, trig_of, start);
str_prop!(tr_get_end, tr_put_end, trig_of, end);
bool_prop!(tr_get_enabled, tr_put_enabled, trig_of, enabled);
str_prop!(tr_get_random, tr_put_random, trig_of, random_delay);
str_prop!(tr_get_delay, tr_put_delay, trig_of, delay);
str_prop!(tr_get_user, tr_put_user, trig_of, user);
str_prop!(
    tr_get_subscription,
    tr_put_subscription,
    trig_of,
    subscription
);
int_prop!(
    tr_get_days_interval,
    tr_put_days_interval,
    i16,
    trig_of,
    days_interval
);
int_prop!(
    tr_get_days_of_week,
    tr_put_days_of_week,
    i16,
    trig_of,
    days_of_week
);
int_prop!(
    tr_get_weeks_interval,
    tr_put_weeks_interval,
    i16,
    trig_of,
    weeks_interval
);
str_prop!(rp_get_interval, rp_put_interval, trig_of, rep_interval);
str_prop!(rp_get_duration, rp_put_duration, trig_of, rep_duration);
bool_prop!(rp_get_stop, rp_put_stop, trig_of, rep_stop);

unsafe extern "win64" fn tr_get_type(this: *mut Obj, out: *mut i32) -> i32 {
    match trig_of(this) {
        Some(t) => int_out(out, lock(&t).kind),
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn tr_get_repetition(this: *mut Obj, out: *mut *mut c_void) -> i32 {
    match trig_of(this) {
        Some(t) => out_obj(out, Kind::Repetition(t)),
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn tr_put_repetition(this: *mut Obj, pattern: *mut Obj) -> i32 {
    let (Some(t), false) = (trig_of(this), pattern.is_null()) else {
        return E_POINTER;
    };
    if let Some(src) = trig_of(pattern) {
        if !Arc::ptr_eq(&src, &t) {
            let s = lock(&src).clone();
            let mut d = lock(&t);
            d.rep_interval = s.rep_interval;
            d.rep_duration = s.rep_duration;
            d.rep_stop = s.rep_stop;
        }
    }
    S_OK
}

fn trigger_vt(kind: i32) -> Vec<usize> {
    let mut v = dispatch_head();
    v.extend([
        tr_get_type as *const () as usize,
        tr_get_id as *const () as usize,
        tr_put_id as *const () as usize,
        tr_get_repetition as *const () as usize,
        tr_put_repetition as *const () as usize,
        tr_get_limit as *const () as usize,
        tr_put_limit as *const () as usize,
        tr_get_start as *const () as usize,
        tr_put_start as *const () as usize,
        tr_get_end as *const () as usize,
        tr_put_end as *const () as usize,
        tr_get_enabled as *const () as usize,
        tr_put_enabled as *const () as usize,
    ]);
    match kind {
        TRIGGER_DAILY => v.extend([
            tr_get_days_interval as *const () as usize,
            tr_put_days_interval as *const () as usize,
            tr_get_random as *const () as usize,
            tr_put_random as *const () as usize,
        ]),
        TRIGGER_WEEKLY => v.extend([
            tr_get_days_of_week as *const () as usize,
            tr_put_days_of_week as *const () as usize,
            tr_get_weeks_interval as *const () as usize,
            tr_put_weeks_interval as *const () as usize,
            tr_get_random as *const () as usize,
            tr_put_random as *const () as usize,
        ]),
        TRIGGER_BOOT => v.extend([
            tr_get_delay as *const () as usize,
            tr_put_delay as *const () as usize,
        ]),
        TRIGGER_LOGON => v.extend([
            tr_get_delay as *const () as usize,
            tr_put_delay as *const () as usize,
            tr_get_user as *const () as usize,
            tr_put_user as *const () as usize,
        ]),
        TRIGGER_EVENT => v.extend([
            tr_get_subscription as *const () as usize,
            tr_put_subscription as *const () as usize,
            tr_get_delay as *const () as usize,
            tr_put_delay as *const () as usize,
            get_no_object as *const () as usize,
            put_ignored as *const () as usize,
        ]),
        _ => v.extend([
            tr_get_random as *const () as usize,
            tr_put_random as *const () as usize,
        ]),
    }
    v
}

fn repetition_vt() -> Vec<usize> {
    let mut v = dispatch_head();
    v.extend([
        rp_get_interval as *const () as usize,
        rp_put_interval as *const () as usize,
        rp_get_duration as *const () as usize,
        rp_put_duration as *const () as usize,
        rp_get_stop as *const () as usize,
        rp_put_stop as *const () as usize,
    ]);
    v
}

// ----- IActionCollection / IExecAction -----

str_prop!(ac_get_context, ac_put_context, def_of, context);

fn actions_vt() -> Vec<usize> {
    let mut v = dispatch_head();
    v.extend([
        ac_count as *const () as usize,
        ac_item as *const () as usize,
        get_no_object as *const () as usize,
        get_empty_bstr as *const () as usize,
        put_ignored as *const () as usize,
        ac_create as *const () as usize,
        ac_remove as *const () as usize,
        ac_clear as *const () as usize,
        ac_get_context as *const () as usize,
        ac_put_context as *const () as usize,
    ]);
    v
}

unsafe extern "win64" fn ac_count(this: *mut Obj, out: *mut i32) -> i32 {
    match def_of(this) {
        Some(d) => int_out(out, lock(&d).actions.len() as i32),
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn ac_item(this: *mut Obj, index: i32, out: *mut *mut c_void) -> i32 {
    let Some(d) = def_of(this) else {
        return E_INVALIDARG;
    };
    let item = lock(&d)
        .actions
        .get((index as usize).wrapping_sub(1))
        .cloned();
    match item {
        Some(a) => out_obj(out, Kind::Action(a)),
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn ac_create(this: *mut Obj, kind: i32, out: *mut *mut c_void) -> i32 {
    let Some(d) = def_of(this) else {
        return E_INVALIDARG;
    };
    if kind != 0 {
        // Seule l'action « démarrer un programme » existe hors de Windows.
        return E_NOTIMPL;
    }
    let a = Arc::new(Mutex::new(ActData::default()));
    lock(&d).actions.push(Arc::clone(&a));
    out_obj(out, Kind::Action(a))
}

unsafe extern "win64" fn ac_remove(this: *mut Obj, index: *const u8) -> i32 {
    let Some(d) = def_of(this) else {
        return E_INVALIDARG;
    };
    let mut d = lock(&d);
    let pos = match variant(index) {
        Index::Number(n) if n >= 1 && (n as usize) <= d.actions.len() => Some(n as usize - 1),
        Index::Name(id) => d.actions.iter().position(|a| lock(a).id == id),
        _ => None,
    };
    match pos {
        Some(i) => {
            d.actions.remove(i);
            S_OK
        }
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn ac_clear(this: *mut Obj) -> i32 {
    if let Some(d) = def_of(this) {
        lock(&d).actions.clear();
    }
    S_OK
}

unsafe fn action_of(this: *mut Obj) -> Option<Arc<Mutex<ActData>>> {
    match &obj(this).kind {
        Kind::Action(a) => Some(Arc::clone(a)),
        _ => None,
    }
}

str_prop!(ea_get_id, ea_put_id, action_of, id);
str_prop!(ea_get_path, ea_put_path, action_of, path);
str_prop!(ea_get_args, ea_put_args, action_of, args);
str_prop!(ea_get_dir, ea_put_dir, action_of, workdir);

unsafe extern "win64" fn ea_get_type(_: *mut Obj, out: *mut i32) -> i32 {
    int_out(out, 0)
}

fn action_vt() -> Vec<usize> {
    let mut v = dispatch_head();
    v.extend([
        ea_get_id as *const () as usize,
        ea_put_id as *const () as usize,
        ea_get_type as *const () as usize,
        ea_get_path as *const () as usize,
        ea_put_path as *const () as usize,
        ea_get_args as *const () as usize,
        ea_put_args as *const () as usize,
        ea_get_dir as *const () as usize,
        ea_put_dir as *const () as usize,
    ]);
    v
}

// ---------- Création ----------

/// Classe intégrée pour `CoCreateInstance` ; `None` si le CLSID est inconnu.
pub unsafe fn create_builtin(clsid: &Guid, iid: &Guid, out: *mut *mut c_void) -> Option<i32> {
    if *clsid != CLSID_TASK_SCHEDULER {
        return None;
    }
    let service = new_obj(Kind::Service(AtomicBool::new(false))) as *mut Obj;
    let hr = qi(service, iid, out);
    release(service);
    Some(hr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::test_root::Root;

    type Raw = *mut c_void;

    unsafe fn slot(p: Raw, i: usize) -> usize {
        *(*(p as *const *const usize)).add(i)
    }
    unsafe fn call_p(p: Raw, i: usize, a: usize) -> i32 {
        let f: unsafe extern "win64" fn(Raw, usize) -> i32 = std::mem::transmute(slot(p, i));
        f(p, a)
    }
    unsafe fn call_pp(p: Raw, i: usize, a: usize, b: usize) -> i32 {
        let f: unsafe extern "win64" fn(Raw, usize, usize) -> i32 = std::mem::transmute(slot(p, i));
        f(p, a, b)
    }
    unsafe fn get_obj(p: Raw, i: usize) -> Raw {
        let mut out: Raw = std::ptr::null_mut();
        assert_eq!(
            call_p(p, i, &mut out as *mut Raw as usize),
            S_OK,
            "slot {i}"
        );
        out
    }
    unsafe fn rel(p: Raw) {
        release(p as *mut Obj);
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }
    unsafe fn get_str(p: Raw, i: usize) -> String {
        let mut out: *mut u16 = std::ptr::null_mut();
        assert_eq!(call_p(p, i, &mut out as *mut _ as usize), S_OK);
        bstr_in(out)
    }
    unsafe fn put_str(p: Raw, i: usize, s: &str) {
        let w = wide(s);
        assert_eq!(call_p(p, i, w.as_ptr() as usize), S_OK);
    }

    #[test]
    fn durations_and_boundaries() {
        assert_eq!(parse_duration("PT72H"), Some(259_200));
        assert_eq!(parse_duration("P1DT30M"), Some(88_200));
        // Une durée absurde ne déborde pas : elle est refusée.
        assert_eq!(parse_duration("P9999999999999999Y"), None);
        assert_eq!(parse_duration("PT0S"), Some(0));
        assert_eq!(parse_duration("1H"), None);
        assert_eq!(format_duration(4320), "PT72H");
        assert_eq!(format_duration(90), "PT90M");
        assert_eq!(
            parse_boundary("2026-11-02T09:05:00+01:00"),
            Some((2026, 11, 2, 9, 5))
        );
    }

    #[test]
    fn service_registers_definitions_like_windows_programs() {
        let _r = Root::new("taskschd");
        unsafe {
            let mut svc: Raw = std::ptr::null_mut();
            assert_eq!(
                create_builtin(&CLSID_TASK_SCHEDULER, &IID_ITASKSERVICE, &mut svc),
                Some(S_OK)
            );
            let root_name = wide("\\");
            let mut folder: Raw = std::ptr::null_mut();
            // GetFolder avant Connect échoue comme sous Windows.
            assert_eq!(
                call_pp(
                    svc,
                    7,
                    root_name.as_ptr() as usize,
                    &mut folder as *mut Raw as usize
                ),
                ERROR_ONLY_IF_CONNECTED
            );
            let empty = [0u8; 24];
            let f: unsafe extern "win64" fn(
                Raw,
                *const u8,
                *const u8,
                *const u8,
                *const u8,
            ) -> i32 = std::mem::transmute(slot(svc, 10));
            assert_eq!(
                f(
                    svc,
                    empty.as_ptr(),
                    empty.as_ptr(),
                    empty.as_ptr(),
                    empty.as_ptr()
                ),
                S_OK
            );
            assert_eq!(
                call_pp(
                    svc,
                    7,
                    root_name.as_ptr() as usize,
                    &mut folder as *mut Raw as usize
                ),
                S_OK
            );
            // NewTask → définition.
            let mut def: Raw = std::ptr::null_mut();
            assert_eq!(call_pp(svc, 9, 0, &mut def as *mut Raw as usize), S_OK);
            let info = get_obj(def, 7);
            put_str(info, 8, "Mise à jour quotidienne");
            put_str(info, 10, "Éditeur");
            rel(info);
            let triggers = get_obj(def, 9);
            let mut trig: Raw = std::ptr::null_mut();
            assert_eq!(
                call_pp(
                    triggers,
                    10,
                    TRIGGER_DAILY as usize,
                    &mut trig as *mut Raw as usize
                ),
                S_OK
            );
            let mut daily: Raw = std::ptr::null_mut();
            assert_eq!(qi(trig as *mut Obj, &IID_IDAILYTRIGGER, &mut daily), S_OK);
            put_str(daily, 15, "2026-11-01T03:30:00");
            assert_eq!(call_p(daily, 21, 2), S_OK); // DaysInterval = 2
            rel(daily);
            rel(trig);
            let mut boot: Raw = std::ptr::null_mut();
            assert_eq!(
                call_pp(
                    triggers,
                    10,
                    TRIGGER_BOOT as usize,
                    &mut boot as *mut Raw as usize
                ),
                S_OK
            );
            let mut count = 0i32;
            assert_eq!(call_p(triggers, 7, &mut count as *mut i32 as usize), S_OK);
            assert_eq!(count, 2);
            rel(boot);
            rel(triggers);
            let actions = get_obj(def, 17);
            let mut action: Raw = std::ptr::null_mut();
            assert_eq!(
                call_pp(actions, 12, 0, &mut action as *mut Raw as usize),
                S_OK
            );
            let mut exec: Raw = std::ptr::null_mut();
            assert_eq!(qi(action as *mut Obj, &IID_IEXECACTION, &mut exec), S_OK);
            put_str(exec, 11, "C:\\Programmes\\maj.exe");
            put_str(exec, 13, "/silencieux");
            rel(exec);
            rel(action);
            rel(actions);
            let settings = get_obj(def, 11);
            assert_eq!(call_p(settings, 18, 0), S_OK); // DisallowStartIfOnBatteries = faux
            assert_eq!(call_p(settings, 26, VARIANT_TRUE as u16 as usize), S_OK); // réseau requis
            rel(settings);
            // RegisterTaskDefinition(L"Maj", def, TASK_CREATE_OR_UPDATE, …).
            let name = wide("Maj");
            let reg: unsafe extern "win64" fn(
                Raw,
                *const u16,
                Raw,
                i32,
                *const u8,
                *const u8,
                i32,
                *const u8,
                *mut Raw,
            ) -> i32 = std::mem::transmute(slot(folder, 17));
            let mut registered: Raw = std::ptr::null_mut();
            assert_eq!(
                reg(
                    folder,
                    name.as_ptr(),
                    def,
                    6,
                    empty.as_ptr(),
                    empty.as_ptr(),
                    3,
                    empty.as_ptr(),
                    &mut registered
                ),
                S_OK
            );
            assert_eq!(get_str(registered, 8), "\\Maj");
            let mut state = 0i32;
            assert_eq!(call_p(registered, 9, &mut state as *mut i32 as usize), S_OK);
            assert_eq!(state, 3);
            // Le fichier est celui du planificateur Winlux.
            let saved = tasks::load("Maj").unwrap();
            assert_eq!(saved.description, "Mise à jour quotidienne");
            assert_eq!(
                saved.triggers,
                vec![
                    Trigger::Daily {
                        hour: 3,
                        minute: 30,
                        every: 2
                    },
                    Trigger::Boot
                ]
            );
            assert_eq!(saved.actions[0].path, "C:\\Programmes\\maj.exe");
            assert!(!saved.ac_only && saved.network);
            // TASK_CREATE seul sur une tâche existante : déjà existante.
            let mut again: Raw = std::ptr::null_mut();
            assert_eq!(
                reg(
                    folder,
                    name.as_ptr(),
                    def,
                    TASK_CREATE,
                    empty.as_ptr(),
                    empty.as_ptr(),
                    3,
                    empty.as_ptr(),
                    &mut again
                ),
                ERROR_ALREADY_EXISTS
            );
            // Désactivation via IRegisteredTask::put_Enabled.
            assert_eq!(call_p(registered, 11, 0), S_OK);
            assert!(!tasks::load("Maj").unwrap().enabled);
            let xml = get_str(registered, 20);
            assert!(xml.contains("<DaysInterval>2</DaysInterval>"), "{xml}");
            rel(registered);
            rel(def);
            // GetTasks + get_Item par nom.
            let mut coll: Raw = std::ptr::null_mut();
            assert_eq!(call_pp(folder, 14, 0, &mut coll as *mut Raw as usize), S_OK);
            let mut n = 0i32;
            assert_eq!(call_p(coll, 7, &mut n as *mut i32 as usize), S_OK);
            assert_eq!(n, 1);
            let mut var = [0u8; 24];
            var[0] = 3;
            var[8] = 1;
            let item = {
                let mut out: Raw = std::ptr::null_mut();
                assert_eq!(
                    call_pp(
                        coll,
                        8,
                        var.as_ptr() as usize,
                        &mut out as *mut Raw as usize
                    ),
                    S_OK
                );
                out
            };
            assert_eq!(get_str(item, 7), "Maj");
            rel(item);
            rel(coll);
            // Sous-dossier et suppression.
            let sub = wide("Winlux");
            let mut subf: Raw = std::ptr::null_mut();
            let create: unsafe extern "win64" fn(Raw, *const u16, *const u8, *mut Raw) -> i32 =
                std::mem::transmute(slot(folder, 11));
            assert_eq!(
                create(folder, sub.as_ptr(), empty.as_ptr(), &mut subf),
                S_OK
            );
            assert_eq!(get_str(subf, 8), "\\Winlux");
            assert_eq!(
                create(
                    folder,
                    sub.as_ptr(),
                    empty.as_ptr(),
                    &mut std::ptr::null_mut()
                ),
                ERROR_ALREADY_EXISTS
            );
            rel(subf);
            assert_eq!(call_pp(folder, 15, name.as_ptr() as usize, 0), S_OK);
            assert!(tasks::load("Maj").is_none());
            rel(folder);
            rel(svc);
        }
    }

    #[test]
    fn register_task_from_xml() {
        let _r = Root::new("taskschd-xml");
        let xml = r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Sync &amp; sauvegarde</Description></RegistrationInfo>
  <Triggers>
    <LogonTrigger><Enabled>true</Enabled></LogonTrigger>
    <CalendarTrigger>
      <StartBoundary>2026-11-02T18:00:00</StartBoundary>
      <ScheduleByWeek><DaysOfWeek><Monday /><Friday/></DaysOfWeek><WeeksInterval>1</WeeksInterval></ScheduleByWeek>
    </CalendarTrigger>
    <TimeTrigger>
      <StartBoundary>2026-11-02T08:00:00</StartBoundary>
      <Repetition><Interval>PT10M</Interval></Repetition>
    </TimeTrigger>
  </Triggers>
  <Settings><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><ExecutionTimeLimit>PT2H</ExecutionTimeLimit></Settings>
  <Actions Context="Author"><Exec><Command>/usr/bin/true</Command><Arguments>--x</Arguments></Exec></Actions>
</Task>"#;
        let def = parse_xml(xml).unwrap();
        let id = register("\\", "Apps\\Sync", &def, 6).unwrap();
        let t = tasks::load(&id).unwrap();
        assert_eq!(t.folder, "\\Apps");
        assert_eq!(t.description, "Sync & sauvegarde");
        assert_eq!(t.time_limit, 120);
        assert!(!t.ac_only);
        assert_eq!(
            t.triggers,
            vec![
                Trigger::Interval { minutes: 10 },
                Trigger::Weekly {
                    hour: 18,
                    minute: 0,
                    days: 0b100010
                },
                Trigger::Logon,
            ]
        );
        assert!(folder_exists("\\Apps"));
        // Le XML produit se relit à l'identique.
        let again = parse_xml(&def_xml(&task_to_def(&t))).unwrap();
        let (t2, _) = def_to_task(&again, &t.folder, &t.name);
        assert_eq!(t2.triggers.len(), 3);
        assert_eq!(t2.actions, t.actions);
    }
}
