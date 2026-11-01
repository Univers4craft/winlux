//! Observateur d'événements façon Windows 11.  Journaux : Système et
//! Sécurité (journald, sinon messages du noyau, exportés par
//! `winlux-journal events`), Applications (.exe) et Winlux (journal du
//! runtime /run/winlux/journal/winlux.log), Plantages (crash.log).
//! Filtres par journal et par niveau, recherche, détail de l'événement,
//! export CSV et « Signaler » (winlux-report).

use crate::admin_ui::{self as ui, Field, Hits, Path};
use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

const MAX_ENTRIES: usize = 3000;
const MAX_MSG: usize = 400;
const MAX_SRC: usize = 40;

// Journaux
const LOG_ALL: u8 = 0;
const LOG_SYSTEM: u8 = 1;
const LOG_APP: u8 = 2;
const LOG_SECURITY: u8 = 3;
const LOG_WINLUX: u8 = 4;
const LOG_CRASH: u8 = 5;
const LOG_COUNT: usize = 6;

const LOG_NAMES: [Tr4; LOG_COUNT] = [
    [
        b"Tous les \xE9v\xE9nements",
        b"All events",
        b"Todos los eventos",
        b"Alle Ereignisse",
    ],
    [b"Syst\xE8me", b"System", b"Sistema", b"System"],
    [
        b"Applications (.exe)",
        b"Applications (.exe)",
        b"Aplicaciones (.exe)",
        b"Anwendungen (.exe)",
    ],
    [b"S\xE9curit\xE9", b"Security", b"Seguridad", b"Sicherheit"],
    [b"Winlux", b"Winlux", b"Winlux", b"Winlux"],
    [b"Plantages", b"Crashes", b"Bloqueos", b"Abst\xFCrze"],
];

// Niveaux (ordre Windows)
const LEVEL_NAMES: [Tr4; 5] = [
    [b"Critique", b"Critical", b"Cr\xEDtico", b"Kritisch"],
    [b"Erreur", b"Error", b"Error", b"Fehler"],
    [b"Avertissement", b"Warning", b"Advertencia", b"Warnung"],
    [
        b"Information",
        b"Information",
        b"Informaci\xF3n",
        b"Informationen",
    ],
    [b"D\xE9tail", b"Verbose", b"Detallado", b"Ausf\xFChrlich"],
];

fn level_color(l: u8) -> u32 {
    match l {
        0 | 1 => rgb(0xE8, 0x11, 0x23),
        2 => rgb(0xFF, 0xB9, 0x00),
        3 => rgb(0x00, 0x78, 0xD4),
        _ => rgb(0x80, 0x80, 0x80),
    }
}

#[derive(Clone, Copy)]
struct Entry {
    /// Secondes UTC depuis 1970 (0 = inconnue).
    ts: u64,
    level: u8,
    log: u8,
    pid: u32,
    /// Rang dans crash.log (pour `winlux-report --crash-at`), sinon -1.
    crash: i32,
    src: [u8; MAX_SRC],
    src_len: usize,
    msg: [u8; MAX_MSG],
    msg_len: usize,
    /// Ordre de lecture, pour départager deux événements de la même seconde.
    seq: u32,
}

const EMPTY: Entry = Entry {
    ts: 0,
    level: 3,
    log: LOG_WINLUX,
    pid: 0,
    crash: -1,
    src: [0; MAX_SRC],
    src_len: 0,
    msg: [0; MAX_MSG],
    msg_len: 0,
    seq: 0,
};

impl Entry {
    fn src(&self) -> &[u8] {
        &self.src[..self.src_len]
    }
    fn msg(&self) -> &[u8] {
        &self.msg[..self.msg_len]
    }
    fn set_src(&mut self, s: &[u8]) {
        self.src_len = to_1252(&mut self.src, s);
    }
    fn push_msg(&mut self, s: &[u8]) {
        self.msg_len += to_1252(&mut self.msg[self.msg_len..], s);
    }
    /// Texte déjà en Windows-1252 (libellés traduits).
    fn push_text(&mut self, s: &[u8]) {
        let n = s.len().min(MAX_MSG - self.msg_len);
        self.msg[self.msg_len..self.msg_len + n].copy_from_slice(&s[..n]);
        self.msg_len += n;
    }
}

static mut ENTRIES: [Entry; MAX_ENTRIES] = [EMPTY; MAX_ENTRIES];
static mut COUNT: usize = 0;
static mut COUNTS: [usize; LOG_COUNT] = [0; LOG_COUNT];
/// Index des événements visibles, du plus récent au plus ancien.
static mut VISIBLE: [u16; MAX_ENTRIES] = [0; MAX_ENTRIES];
static mut VISIBLE_COUNT: usize = 0;

static mut HITS: Hits = Hits::new();
static mut HOVER: u16 = 0;
static mut LOG: u8 = LOG_ALL;
/// Niveaux affichés (bit = niveau).  Le détail est masqué au départ.
static mut LEVELS: u8 = 0b01111;
static mut SEARCH: Field<40> = Field::new();
static mut SEARCH_FOCUS: bool = false;
static mut SCROLL: usize = 0;
static mut ROWS_SHOWN: usize = 10;
static mut SELECTED: usize = usize::MAX;
static mut LOADED: bool = false;
static mut LAST_LOAD: u32 = 0;
/// Export des journaux système demandé à `winlux-journal`, en attente.
static mut EVENTS_PENDING: u32 = 0;
static mut MESSAGE: Field<200> = Field::new();
static mut MESSAGE_AT: u32 = 0;
/// Décalage de l'heure locale sur UTC, en secondes.
static mut TZ_OFFSET: i64 = 0;

const ID_LOG: u16 = 1;
const ID_SEARCH: u16 = 30;
const ID_LEVEL: u16 = 40;
const ID_REFRESH: u16 = 50;
const ID_EXPORT: u16 = 51;
const ID_REPORT: u16 = 52;
const ID_CLEAR: u16 = 53;
const ID_ROW: u16 = 1000;

// ---------- Texte ----------

/// UTF-8 vers Windows-1252 (latin-1, guillemets et tirets typographiques).
fn to_1252(dst: &mut [u8], src: &[u8]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < src.len() && n < dst.len() {
        let c = src[i];
        let (out, len) = if c < 0x80 {
            (if c == b'\t' || c == b'\r' { b' ' } else { c }, 1)
        } else if c & 0xE0 == 0xC0 && i + 1 < src.len() {
            let cp = ((c as u32 & 0x1F) << 6) | (src[i + 1] as u32 & 0x3F);
            (
                if (0xA0..=0xFF).contains(&cp) {
                    cp as u8
                } else {
                    b'?'
                },
                2,
            )
        } else if c & 0xF0 == 0xE0 && i + 2 < src.len() {
            let cp = ((c as u32 & 0x0F) << 12)
                | ((src[i + 1] as u32 & 0x3F) << 6)
                | (src[i + 2] as u32 & 0x3F);
            let b = match cp {
                0x2018 => 0x91,
                0x2019 => 0x92,
                0x201C => 0x93,
                0x201D => 0x94,
                0x2013 => 0x96,
                0x2014 => 0x97,
                0x2026 => 0x85,
                0x20AC => 0x80,
                _ => b'?',
            };
            (b, 3)
        } else if c & 0xF8 == 0xF0 {
            (b'?', 4)
        } else {
            // Octet isolé : déjà en latin-1 (ancien journal).
            (c, 1)
        };
        dst[n] = out;
        n += 1;
        i += len;
    }
    n
}

/// Windows-1252 vers UTF-8 pour les fichiers exportés.
fn push_utf8(out: &mut [u8], n: &mut usize, s: &[u8]) {
    for &c in s {
        let cp: u32 = match c {
            0x80 => 0x20AC,
            0x85 => 0x2026,
            0x91 => 0x2018,
            0x92 => 0x2019,
            0x93 => 0x201C,
            0x94 => 0x201D,
            0x96 => 0x2013,
            0x97 => 0x2014,
            0x9C => 0x0153,
            0x8C => 0x0152,
            c => c as u32,
        };
        let mut buf = [0u8; 3];
        let len = if cp < 0x80 {
            buf[0] = cp as u8;
            1
        } else if cp < 0x800 {
            buf[0] = 0xC0 | (cp >> 6) as u8;
            buf[1] = 0x80 | (cp & 0x3F) as u8;
            2
        } else {
            buf[0] = 0xE0 | (cp >> 12) as u8;
            buf[1] = 0x80 | ((cp >> 6) & 0x3F) as u8;
            buf[2] = 0x80 | (cp & 0x3F) as u8;
            3
        };
        if *n + len <= out.len() {
            out[*n..*n + len].copy_from_slice(&buf[..len]);
            *n += len;
        }
    }
}

fn num(s: &[u8]) -> u64 {
    let mut v = 0u64;
    for &c in s {
        if !c.is_ascii_digit() {
            break;
        }
        v = v * 10 + (c - b'0') as u64;
    }
    v
}

fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    // Ni casse ni accents : « resume » trouve « Résumé ».
    crate::locale::contains_fold(hay, needle)
}

struct T {
    b: [u8; 160],
    n: usize,
}

impl T {
    const fn new() -> T {
        T { b: [0; 160], n: 0 }
    }
    fn s(&mut self, s: &[u8]) -> &mut T {
        let k = s.len().min(self.b.len() - self.n);
        self.b[self.n..self.n + k].copy_from_slice(&s[..k]);
        self.n += k;
        self
    }
    fn u2(&mut self, v: u64) -> &mut T {
        self.s(&[b'0' + (v / 10 % 10) as u8, b'0' + (v % 10) as u8])
    }
    fn u(&mut self, v: u64) -> &mut T {
        let mut d = [0u8; 20];
        let mut i = 20;
        let mut v = v;
        loop {
            i -= 1;
            d[i] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        self.s(&d[i..])
    }
    fn get(&self) -> &[u8] {
        &self.b[..self.n]
    }
}

// ---------- Dates ----------

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if m <= 2 {
            yoe + era * 400 + 1
        } else {
            yoe + era * 400
        },
        m,
        d,
    )
}

/// « 2026-10-31T16:39:07.123Z », « …+01:00 », « …+0100 » ou sans fuseau.
fn parse_iso(s: &[u8]) -> u64 {
    if s.len() < 19 || s[4] != b'-' || s[7] != b'-' || s[13] != b':' {
        return 0;
    }
    let y = num(&s[0..4]) as i64;
    let mo = num(&s[5..7]) as i64;
    let d = num(&s[8..10]) as i64;
    let h = num(&s[11..13]) as i64;
    let mi = num(&s[14..16]) as i64;
    let se = num(&s[17..19]) as i64;
    let mut t = days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + se;
    let mut i = 19;
    while i < s.len() && (s[i] == b'.' || s[i] == b',' || s[i].is_ascii_digit()) {
        i += 1;
    }
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') && i + 3 <= s.len() {
        let oh = num(&s[i + 1..i + 3]) as i64;
        let rest = &s[i + 3..];
        let om = match rest {
            [b':', a, b, ..] | [a, b, ..] if a.is_ascii_digit() => num(&[*a, *b]) as i64,
            _ => 0,
        };
        let off = oh * 3600 + om * 60;
        t -= if s[i] == b'+' { off } else { -off };
    }
    t.max(0) as u64
}

unsafe fn compute_tz() {
    let mut ft = 0u64;
    GetSystemTimeAsFileTime(&mut ft);
    let utc = (ft / 10_000_000).saturating_sub(11_644_473_600) as i64;
    let mut lt: SystemTime = core::mem::zeroed();
    GetLocalTime(&mut lt);
    let local = days_from_civil(lt.year as i64, lt.month as i64, lt.day as i64) * 86400
        + lt.hour as i64 * 3600
        + lt.minute as i64 * 60
        + lt.second as i64;
    // Arrondi au quart d'heure (les fuseaux réels en sont des multiples).
    TZ_OFFSET = (local - utc + 450).div_euclid(900) * 900;
}

/// Date locale « 31/10/2026 17:39:07 » (« 10/31/2026 5:39:07 PM » en anglais).
unsafe fn fmt_date(t: &mut T, ts: u64) {
    if ts == 0 {
        t.s(b"\x97");
        return;
    }
    let local = ts as i64 + TZ_OFFSET;
    let (y, m, d) = civil_from_days(local.div_euclid(86400));
    let sec = local.rem_euclid(86400) as u64;
    let (h, mi, s) = (sec / 3600, sec / 60 % 60, sec % 60);
    if ui_lang() == 1 {
        t.u(m as u64)
            .s(b"/")
            .u(d as u64)
            .s(b"/")
            .u(y as u64)
            .s(b" ");
        let h12 = if h % 12 == 0 { 12 } else { h % 12 };
        t.u(h12).s(b":").u2(mi).s(b":").u2(s);
        t.s(if h < 12 { b" AM" } else { b" PM" });
    } else {
        let sep: &[u8] = if ui_lang() == 3 { b"." } else { b"/" };
        t.u2(d as u64)
            .s(sep)
            .u2(m as u64)
            .s(sep)
            .u(y as u64)
            .s(b" ");
        t.u2(h).s(b":").u2(mi).s(b":").u2(s);
    }
}

// ---------- Chargement ----------

/// Chemin Linux du fichier d'événements exporté pour ce processus.
fn events_linux_path(out: &mut [u8; 64]) -> usize {
    let mut t = T::new();
    t.s(b"/tmp/winlux-events-")
        .u(unsafe { GetCurrentProcessId() } as u64)
        .s(b".tsv");
    let n = t.n;
    out[..n].copy_from_slice(t.get());
    n
}

fn zpath(linux: &[u8]) -> Path {
    let mut p = Path::new(b"Z:");
    for &c in linux {
        p.push(if c == b'/' {
            b"\\"
        } else {
            core::slice::from_ref(&c)
        });
    }
    p
}

/// Demande à `winlux-journal` l'export des journaux système et sécurité.
unsafe fn request_system_events() {
    let mut lp = [0u8; 64];
    let n = events_linux_path(&mut lp);
    DeleteFileA(zpath(&lp[..n]).ptr());
    let mut args = T::new();
    args.s(b"events ").s(&lp[..n]).s(b"\0");
    let ok = ShellExecuteA(
        core::ptr::null_mut(),
        c"open".as_ptr(),
        c"winlux-journal".as_ptr(),
        args.b.as_ptr().cast(),
        core::ptr::null(),
        0,
    ) as isize
        > 32;
    EVENTS_PENDING = if ok { GetTickCount().max(1) } else { 0 };
}

static mut BUF: [u8; 524288] = [0; 524288];

unsafe fn add(e: Entry) {
    if COUNT < MAX_ENTRIES {
        ENTRIES[COUNT] = e;
        ENTRIES[COUNT].seq = COUNT as u32;
        COUNT += 1;
    }
}

unsafe fn load() {
    COUNT = 0;
    if TZ_OFFSET == 0 {
        compute_tz();
    }
    let buf = &mut *core::ptr::addr_of_mut!(BUF);
    // Journaux système et sécurité (déjà exportés).
    let mut lp = [0u8; 64];
    let n = events_linux_path(&mut lp);
    let k = ui::read_file(zpath(&lp[..n]).ptr(), &mut buf[..], true);
    parse_events(&buf[..k]);
    // Plantages
    let k = ui::read_file(
        c"Z:\\run\\winlux\\journal\\crash.log".as_ptr(),
        &mut buf[..262144],
        true,
    );
    parse_crash(&buf[..k]);
    // Journal du runtime
    let room = buf.len();
    let k = ui::read_file(
        c"Z:\\run\\winlux\\journal\\winlux.log".as_ptr(),
        &mut buf[..room],
        true,
    );
    parse_winlux(&buf[..k]);
    let entries = &mut *core::ptr::addr_of_mut!(ENTRIES);
    let list = &mut entries[..COUNT];
    list.sort_unstable_by(|a, b| b.ts.cmp(&a.ts).then(b.seq.cmp(&a.seq)));
    COUNTS = [0; LOG_COUNT];
    for e in list.iter() {
        COUNTS[e.log as usize] += 1;
        if e.log != LOG_SECURITY {
            COUNTS[0] += 1;
        }
    }
    LOADED = true;
    LAST_LOAD = GetTickCount();
    filter();
}

/// `journal\tniveau\tdate\tsource\tmessage` (winlux-journal events).
unsafe fn parse_events(data: &[u8]) {
    for line in data.split(|&c| c == b'\n') {
        let mut f = line.splitn(5, |&c| c == b'\t');
        let (Some(log), Some(lvl), Some(ts), Some(src), Some(msg)) =
            (f.next(), f.next(), f.next(), f.next(), f.next())
        else {
            continue;
        };
        let mut e = EMPTY;
        e.log = if log == b"security" {
            LOG_SECURITY
        } else {
            LOG_SYSTEM
        };
        e.level = match lvl {
            b"error" => 1,
            b"warn" => 2,
            _ => 3,
        };
        e.ts = parse_iso(ts);
        // « sudo[123] » : source et PID.
        match src.iter().position(|&c| c == b'[') {
            Some(i) => {
                e.set_src(&src[..i]);
                e.pid = num(&src[i + 1..]) as u32;
            }
            None => e.set_src(src),
        }
        e.push_msg(msg);
        add(e);
    }
}

/// `[CRASH] ts=… stop=… exception=… exe=… pid=…` suivi de lignes indentées.
unsafe fn parse_crash(data: &[u8]) {
    // Les rangs comptent depuis le début du fichier ; s'il a été tronqué en
    // tête, on ne peut plus les retrouver : on les compte quand même.
    let mut rank = 0i32;
    let mut current: Option<Entry> = None;
    for line in data.split(|&c| c == b'\n') {
        if line.starts_with(b"[CRASH]") {
            if let Some(e) = current.take() {
                add(e);
            }
            let mut e = EMPTY;
            e.log = LOG_CRASH;
            e.level = 0;
            e.crash = rank;
            rank += 1;
            e.ts = num(field(line, b"ts="));
            e.pid = num(field(line, b"pid=")) as u32;
            let exe = field(line, b"exe=");
            e.set_src(if exe.is_empty() { b"winlux" } else { exe });
            e.push_text(tr4(&[
                b"Arr\xEAt du programme : ",
                b"Program stopped: ",
                b"Programa detenido: ",
                b"Programm angehalten: ",
            ]));
            e.push_msg(field(line, b"stop="));
            let ex = field(line, b"exception=");
            if !ex.is_empty() {
                e.push_msg(b" (");
                e.push_msg(ex);
                e.push_msg(b")");
            }
            let module = field(line, b"module=");
            if !module.is_empty() {
                e.push_text(tr4(&[
                    b", module ",
                    b", module ",
                    b", m\xF3dulo ",
                    b", Modul ",
                ]));
                e.push_msg(module);
            }
            current = Some(e);
        } else if let Some(e) = current.as_mut() {
            let l = line.trim_ascii();
            if !l.is_empty() && e.msg_len + 4 < MAX_MSG {
                e.push_msg(b"\n");
                if let Some(path) = l.strip_prefix(b"path=") {
                    e.push_text(tr4(&[
                        b"Programme : ",
                        b"Program: ",
                        b"Programa: ",
                        b"Programm: ",
                    ]));
                    e.push_msg(path);
                } else {
                    e.push_msg(l.strip_prefix(b"detail=").unwrap_or(l));
                }
            }
        }
    }
    if let Some(e) = current {
        add(e);
    }
}

fn field<'a>(line: &'a [u8], key: &[u8]) -> &'a [u8] {
    let Some(p) = line.windows(key.len()).position(|w| w == key) else {
        return b"";
    };
    let v = &line[p + key.len()..];
    &v[..v.iter().position(|&c| c == b' ').unwrap_or(v.len())]
}

/// `DATE [niveau] programme[pid] cible: message` (l'origine est absente
/// des journaux écrits par les versions précédentes).
unsafe fn parse_winlux(data: &[u8]) {
    for line in data.split(|&c| c == b'\n') {
        let Some(sp) = line.iter().position(|&c| c == b' ') else {
            continue;
        };
        let rest = &line[sp + 1..];
        if rest.first() != Some(&b'[') {
            continue;
        }
        let Some(close) = rest.iter().position(|&c| c == b']') else {
            continue;
        };
        let mut e = EMPTY;
        e.ts = parse_iso(&line[..sp]);
        e.level = match &rest[1..close] {
            b"error" => 1,
            b"warn" => 2,
            b"info" => 3,
            _ => 4,
        };
        let mut body = rest.get(close + 2..).unwrap_or(b"");
        let mut origin: &[u8] = b"";
        if let Some(sp) = body.iter().position(|&c| c == b' ') {
            let word = &body[..sp];
            if word.ends_with(b"]") && word.contains(&b'[') {
                origin = word;
                body = &body[sp + 1..];
            }
        }
        let (target, msg) = match body.iter().position(|&c| c == b':') {
            Some(c) => (&body[..c], body[c + 1..].trim_ascii_start()),
            None => (&b""[..], body),
        };
        let (oname, opid) = match origin.iter().position(|&c| c == b'[') {
            Some(i) => (&origin[..i], num(&origin[i + 1..]) as u32),
            None => (&b""[..], 0),
        };
        e.pid = opid;
        let exe = oname.len() > 4 && oname[oname.len() - 4..].eq_ignore_ascii_case(b".exe");
        if exe && !oname.eq_ignore_ascii_case(b"gui_demo.exe") {
            e.log = LOG_APP;
            e.set_src(oname);
            if !target.is_empty() {
                e.push_msg(target);
                e.push_msg(b" : ");
            }
        } else {
            e.log = LOG_WINLUX;
            e.set_src(if target.is_empty() { b"winlux" } else { target });
        }
        e.push_msg(msg);
        add(e);
    }
}

unsafe fn filter() {
    let list = &*core::ptr::addr_of!(ENTRIES);
    let vis = &mut *core::ptr::addr_of_mut!(VISIBLE);
    let search = (*core::ptr::addr_of!(SEARCH)).get();
    VISIBLE_COUNT = 0;
    for (i, e) in list[..COUNT].iter().enumerate() {
        let log_ok = if LOG == LOG_ALL {
            e.log != LOG_SECURITY
        } else {
            e.log == LOG
        };
        let level_ok = LEVELS & (1 << e.level) != 0;
        if log_ok && level_ok && (contains_ci(e.msg(), search) || contains_ci(e.src(), search)) {
            vis[VISIBLE_COUNT] = i as u16;
            VISIBLE_COUNT += 1;
        }
    }
    if SELECTED >= VISIBLE_COUNT {
        SELECTED = usize::MAX;
    }
    SCROLL = SCROLL.min(VISIBLE_COUNT.saturating_sub(1));
}

unsafe fn selected() -> Option<&'static Entry> {
    let entries = &*core::ptr::addr_of!(ENTRIES);
    let vis = &*core::ptr::addr_of!(VISIBLE);
    (SELECTED < VISIBLE_COUNT).then(|| &entries[vis[SELECTED] as usize])
}

unsafe fn say(s: &[u8]) {
    MESSAGE.set(s);
    MESSAGE_AT = GetTickCount();
}

// ---------- Dessin ----------

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    if !LOADED {
        request_system_events();
        load();
    }
    let h = &mut *core::ptr::addr_of_mut!(HITS);
    h.clear();
    let r = f.client;
    let p = pal();
    fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, p.mica);

    // Journaux à gauche
    let nav_w = 230.min((r.right - r.left) / 4);
    text(
        g.hdc,
        g.f.caption,
        p.text2,
        r.left + 16,
        r.top + 18,
        tr4(&[b"Journaux", b"Logs", b"Registros", b"Protokolle"]),
    );
    let mut y = r.top + 40;
    let counts = *core::ptr::addr_of!(COUNTS);
    for i in 0..LOG_COUNT {
        let ir = rect(r.left + 6, y, r.left + nav_w - 6, y + 36);
        let hover = h.add(ir, ID_LOG + i as u16);
        let active = LOG as usize == i;
        if active || hover {
            round_rect(
                g.hdc,
                g.b,
                ir.left,
                ir.top,
                ir.right,
                ir.bottom,
                4,
                if active { p.selected } else { p.hover },
            );
        }
        if active {
            round_rect(
                g.hdc,
                g.b,
                ir.left,
                ir.top + 10,
                ir.left + 3,
                ir.bottom - 10,
                2,
                p.accent,
            );
        }
        let mut t = T::new();
        t.u(counts[i] as u64);
        let cw = text_width(t.get(), 12);
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            ir.left + 14,
            y + 10,
            tr4(&LOG_NAMES[i]),
            nav_w - cw - 44,
        );
        text(
            g.hdc,
            g.f.caption,
            p.text2,
            ir.right - cw - 10,
            y + 11,
            t.get(),
        );
        y += 40;
    }

    // Contenu
    let cx = r.left + nav_w;
    fill(g.hdc, g.b, cx, r.top, r.right, r.bottom, p.window);
    fill(g.hdc, g.b, cx, r.top, cx + 1, r.bottom, p.border);
    let x = cx + 20;
    let right = r.right - 16;
    text(
        g.hdc,
        g.f.large,
        p.text,
        x,
        r.top + 14,
        tr4(&LOG_NAMES[LOG as usize]),
    );

    // Commandes
    let mut bx = right;
    let sel = selected();
    for (id, label, enabled) in [
        (
            ID_REPORT,
            tr4(&[b"Signaler", b"Report", b"Informar", b"Melden"]),
            true,
        ),
        (
            ID_EXPORT,
            tr4(&[b"Exporter", b"Export", b"Exportar", b"Exportieren"]),
            VISIBLE_COUNT > 0,
        ),
        (
            ID_REFRESH,
            tr4(&[b"Actualiser", b"Refresh", b"Actualizar", b"Aktualisieren"]),
            true,
        ),
    ] {
        let w = ui::button_w(label);
        ui::button(
            g,
            h,
            rect(bx - w, r.top + 12, bx, r.top + 44),
            id,
            label,
            id == ID_REPORT && sel.is_some_and(|e| e.log == LOG_CRASH),
            enabled,
        );
        bx -= w + 8;
    }
    if LOG == LOG_WINLUX {
        let label = tr4(&[b"Effacer", b"Clear", b"Borrar", b"L\xF6schen"]);
        let w = ui::button_w(label);
        ui::button(
            g,
            h,
            rect(bx - w, r.top + 12, bx, r.top + 44),
            ID_CLEAR,
            label,
            false,
            COUNTS[LOG_WINLUX as usize] > 0,
        );
    }

    // Recherche et niveaux
    let fy = r.top + 58;
    let sw = 240.min((right - x) / 3);
    ui::input(
        g,
        h,
        rect(x, fy, x + sw, fy + 30),
        ID_SEARCH,
        (*core::ptr::addr_of!(SEARCH)).get(),
        SEARCH_FOCUS,
        tr4(&[b"Rechercher", b"Search", b"Buscar", b"Suchen"]),
    );
    let mut chx = x + sw + 12;
    for lvl in 0..5u8 {
        let label = tr4(&LEVEL_NAMES[lvl as usize]);
        if chx + text_width(label, 13) + 24 > right {
            break;
        }
        chx += ui::chip(
            g,
            h,
            chx,
            fy,
            ID_LEVEL + lvl as u16,
            label,
            LEVELS & (1 << lvl) != 0,
        );
    }

    // Tableau
    let detail_h = 170.min((r.bottom - r.top) / 3);
    let ty = fy + 44;
    let w = right - x;
    let cols: [(&[u8], i32); 5] = [
        (
            tr4(&[b"Niveau", b"Level", b"Nivel", b"Ebene"]),
            w * 16 / 100,
        ),
        (
            tr4(&[
                b"Date et heure",
                b"Date and time",
                b"Fecha y hora",
                b"Datum und Uhrzeit",
            ]),
            w * 20 / 100,
        ),
        (
            tr4(&[b"Source", b"Source", b"Origen", b"Quelle"]),
            w * 17 / 100,
        ),
        (
            tr4(&[b"Journal", b"Log", b"Registro", b"Protokoll"]),
            w * 13 / 100,
        ),
        (
            tr4(&[b"Message", b"Message", b"Mensaje", b"Meldung"]),
            w * 34 / 100,
        ),
    ];
    let top = ui::header(g, x, ty, right, &cols);
    let row_h = 26;
    let list_bottom = r.bottom - detail_h - 8;
    ROWS_SHOWN = ((list_bottom - top) / row_h).max(1) as usize;
    let list = &*core::ptr::addr_of!(ENTRIES);
    let vis = &*core::ptr::addr_of!(VISIBLE);
    let mut ry = top;
    for (slot, k) in (SCROLL..VISIBLE_COUNT).take(ROWS_SHOWN).enumerate() {
        let e = &list[vis[k] as usize];
        let rr = rect(x - 8, ry, right, ry + row_h);
        let hover = h.add(rr, ID_ROW + slot as u16);
        if SELECTED == k {
            fill(
                g.hdc,
                g.b,
                rr.left,
                rr.top,
                rr.right,
                rr.bottom,
                mix(p.selected, p.window, 3, 10),
            );
            fill(
                g.hdc,
                g.b,
                rr.left,
                rr.top,
                rr.left + 3,
                rr.bottom,
                p.accent,
            );
        } else if hover {
            fill(g.hdc, g.b, rr.left, rr.top, rr.right, rr.bottom, p.hover);
        }
        let mut cxx = x;
        ellipse(
            g.hdc,
            g.b,
            cxx,
            ry + 9,
            cxx + 9,
            ry + 18,
            level_color(e.level),
        );
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            cxx + 16,
            ry + 5,
            tr4(&LEVEL_NAMES[e.level as usize]),
            cols[0].1 - 22,
        );
        cxx += cols[0].1;
        let mut t = T::new();
        fmt_date(&mut t, e.ts);
        ui::cell(g, cxx, ry + 5, cols[1].1, t.get(), p.text2);
        cxx += cols[1].1;
        ui::cell(g, cxx, ry + 5, cols[2].1, e.src(), p.text);
        cxx += cols[2].1;
        ui::cell(
            g,
            cxx,
            ry + 5,
            cols[3].1,
            tr4(&LOG_NAMES[e.log as usize]),
            p.text2,
        );
        cxx += cols[3].1;
        let first = e.msg().split(|&c| c == b'\n').next().unwrap_or(b"");
        ui::cell(g, cxx, ry + 5, right - cxx, first, p.text);
        ry += row_h;
    }
    if VISIBLE_COUNT == 0 {
        let msg: &[u8] = if EVENTS_PENDING != 0 && (LOG == LOG_SYSTEM || LOG == LOG_SECURITY) {
            tr4(&[
                b"Lecture des journaux\x85",
                b"Reading logs\x85",
                b"Leyendo registros\x85",
                b"Protokolle werden gelesen\x85",
            ])
        } else {
            tr4(&[
                b"Aucun \xE9v\xE9nement \xE0 afficher.",
                b"No events to display.",
                b"No hay eventos que mostrar.",
                b"Keine Ereignisse vorhanden.",
            ])
        };
        text(g.hdc, g.f.body, p.text2, x, top + 12, msg);
    }
    if VISIBLE_COUNT > ROWS_SHOWN {
        let hgt = list_bottom - top;
        let thumb = ((hgt as usize * ROWS_SHOWN / VISIBLE_COUNT) as i32).max(24);
        let ty2 =
            top + ((hgt - thumb) as usize * SCROLL / (VISIBLE_COUNT - ROWS_SHOWN).max(1)) as i32;
        round_rect(
            g.hdc,
            g.b,
            right + 6,
            ty2,
            right + 10,
            ty2 + thumb,
            2,
            p.text3,
        );
    }

    // Détail de l'événement
    let dy = r.bottom - detail_h;
    panel(
        g.hdc,
        g.b,
        x - 8,
        dy,
        right,
        r.bottom - 10,
        6,
        p.surface,
        p.border,
    );
    match sel {
        Some(e) => draw_detail(g, e, x + 6, dy + 12, right - 14, r.bottom - 16),
        None => text(
            g.hdc,
            g.f.body,
            p.text2,
            x + 6,
            dy + 14,
            tr4(&[
                b"S\xE9lectionnez un \xE9v\xE9nement pour voir son d\xE9tail.",
                b"Select an event to see its details.",
                b"Seleccione un evento para ver sus detalles.",
                b"W\xE4hlen Sie ein Ereignis aus, um Details anzuzeigen.",
            ]),
        ),
    }

    // Message temporaire
    let m = (*core::ptr::addr_of!(MESSAGE)).get();
    if !m.is_empty() && GetTickCount().wrapping_sub(MESSAGE_AT) < 7000 {
        let mw = (text_width(m, 13) + 32).min(right - x);
        let mx = x + (right - x - mw) / 2;
        let my = dy - 46;
        panel(
            g.hdc,
            g.b,
            mx,
            my,
            mx + mw,
            my + 36,
            6,
            p.surface2,
            p.border,
        );
        text_fit(g.hdc, g.f.body, 13, p.text, mx + 16, my + 10, m, mw - 32);
    }
}

unsafe fn draw_detail(g: &mut Gfx, e: &Entry, x: i32, y: i32, right: i32, bottom: i32) {
    let p = pal();
    // Résumé : niveau, date, source, journal, PID
    let mut t = T::new();
    t.s(tr4(&LEVEL_NAMES[e.level as usize])).s(b"   \xB7   ");
    fmt_date(&mut t, e.ts);
    t.s(b"   \xB7   ")
        .s(e.src())
        .s(b"   \xB7   ")
        .s(tr4(&LOG_NAMES[e.log as usize]));
    if e.pid > 0 {
        t.s(b"   \xB7   PID ").u(e.pid as u64);
    }
    ellipse(g.hdc, g.b, x, y + 4, x + 9, y + 13, level_color(e.level));
    text_fit(
        g.hdc,
        g.f.semibold,
        13,
        p.text,
        x + 16,
        y,
        t.get(),
        right - x - 16,
    );
    // Message complet, coupé aux retours à la ligne et à la largeur.
    let mut ly = y + 24;
    let maxw = right - x;
    for para in e.msg().split(|&c| c == b'\n') {
        let mut rest = para;
        while !rest.is_empty() {
            if ly + 16 > bottom {
                return;
            }
            let mut cut = rest.len();
            while cut > 1 && text_width(&rest[..cut], 13) > maxw {
                cut = rest[..cut - 1]
                    .iter()
                    .rposition(|&c| c == b' ')
                    .map_or(cut - 1, |s| s.max(1));
            }
            text(g.hdc, g.f.body, p.text, x, ly, &rest[..cut]);
            rest = rest[cut..].trim_ascii_start();
            ly += 18;
        }
        if para.is_empty() {
            ly += 18;
        }
    }
}

// ---------- Actions ----------

/// « Signaler » : le plantage choisi (rang dans crash.log), sinon un
/// signalement libre.
unsafe fn report_selected() {
    let mut args = T::new();
    if let Some(e) = selected().filter(|e| e.log == LOG_CRASH && e.crash >= 0) {
        args.s(b"--crash-at ").u(e.crash as u64);
    }
    args.s(b"\0");
    ShellExecuteA(
        core::ptr::null_mut(),
        c"open".as_ptr(),
        c"winlux-report".as_ptr(),
        args.b.as_ptr().cast(),
        core::ptr::null(),
        1,
    );
}

/// Exporte les événements affichés en CSV (UTF-8, séparateur « ; ») dans
/// Documents, sinon le dossier personnel.
unsafe fn export() {
    let mut home = [0u8; 200];
    let n = GetEnvironmentVariableA(c"HOME".as_ptr(), home.as_mut_ptr().cast(), 200) as usize;
    let home = if n == 0 || n >= 200 {
        &b"/root"[..]
    } else {
        &home[..n]
    };
    let mut dir = zpath(home);
    let mut docs = zpath(home);
    docs.push(b"\\Documents");
    if ui::exists(docs.ptr()) {
        dir = docs;
    }
    let mut lt: SystemTime = core::mem::zeroed();
    GetLocalTime(&mut lt);
    let mut name = T::new();
    name.s(b"\\")
        .s(tr4(&[
            b"evenements-",
            b"events-",
            b"eventos-",
            b"ereignisse-",
        ]))
        .u(lt.year as u64)
        .u2(lt.month as u64)
        .u2(lt.day as u64)
        .s(b"-")
        .u2(lt.hour as u64)
        .u2(lt.minute as u64)
        .u2(lt.second as u64)
        .s(b".csv");
    dir.push(name.get());
    let out = &mut *core::ptr::addr_of_mut!(BUF);
    let mut k = 0usize;
    // Marque d'ordre UTF-8 : les tableurs reconnaissent les accents.
    out[..3].copy_from_slice(b"\xEF\xBB\xBF");
    k += 3;
    let head = [
        tr4(&[b"Niveau", b"Level", b"Nivel", b"Ebene"]),
        tr4(&[
            b"Date et heure",
            b"Date and time",
            b"Fecha y hora",
            b"Datum und Uhrzeit",
        ]),
        tr4(&[b"Source", b"Source", b"Origen", b"Quelle"]),
        tr4(&[b"Journal", b"Log", b"Registro", b"Protokoll"]),
        b"PID",
        tr4(&[b"Message", b"Message", b"Mensaje", b"Meldung"]),
    ];
    for (i, hcol) in head.iter().enumerate() {
        push_utf8(out, &mut k, hcol);
        push_utf8(out, &mut k, if i + 1 < head.len() { b";" } else { b"\r\n" });
    }
    let list = &*core::ptr::addr_of!(ENTRIES);
    let vis = &*core::ptr::addr_of!(VISIBLE);
    for &i in vis[..VISIBLE_COUNT].iter() {
        if k + 1200 > out.len() {
            break;
        }
        let e = &list[i as usize];
        let mut d = T::new();
        fmt_date(&mut d, e.ts);
        let mut pid = T::new();
        if e.pid > 0 {
            pid.u(e.pid as u64);
        }
        for (j, v) in [
            tr4(&LEVEL_NAMES[e.level as usize]),
            d.get(),
            e.src(),
            tr4(&LOG_NAMES[e.log as usize]),
            pid.get(),
            e.msg(),
        ]
        .iter()
        .enumerate()
        {
            push_utf8(out, &mut k, b"\"");
            for part in v.split(|&c| c == b'"').enumerate() {
                if part.0 > 0 {
                    push_utf8(out, &mut k, b"\"\"");
                }
                push_utf8(out, &mut k, part.1);
            }
            push_utf8(out, &mut k, if j < 5 { b"\";" } else { b"\"\r\n" });
        }
    }
    let ok = ui::write_file(&dir, &out[..k]);
    let mut m = T::new();
    if ok {
        m.s(tr4(&[
            b"Export\xE9 dans ",
            b"Exported to ",
            b"Exportado a ",
            b"Exportiert nach ",
        ]));
        // Chemin affiché sans « Z: », avec des barres obliques.
        let shown = &dir_bytes(&dir)[2..];
        for &c in shown {
            m.s(if c == b'\\' {
                b"/"
            } else {
                core::slice::from_ref(&c)
            });
        }
    } else {
        m.s(tr4(&[
            b"Export impossible.",
            b"Export failed.",
            b"No se pudo exportar.",
            b"Export fehlgeschlagen.",
        ]));
    }
    let msg = m.get();
    let mut copy = [0u8; 160];
    copy[..msg.len()].copy_from_slice(msg);
    say(&copy[..msg.len()]);
}

fn dir_bytes(p: &Path) -> &[u8] {
    unsafe {
        let ptr = p.ptr() as *const u8;
        let mut n = 0;
        while *ptr.add(n) != 0 {
            n += 1;
        }
        core::slice::from_raw_parts(ptr, n)
    }
}

// ---------- Souris et clavier ----------

pub unsafe fn mouse(ev: Mouse, mx: i32, my: i32, _f: &Frame) -> bool {
    let h = &mut *core::ptr::addr_of_mut!(HITS);
    match ev {
        Mouse::Move => {
            h.mx = mx;
            h.my = my;
            let id = h.find(mx, my).unwrap_or(0);
            let changed = id != HOVER;
            HOVER = id;
            changed
        }
        Mouse::Wheel(delta) => {
            let max = VISIBLE_COUNT.saturating_sub(ROWS_SHOWN);
            let next = (SCROLL as i32 - delta * 3).clamp(0, max as i32) as usize;
            let changed = next != SCROLL;
            SCROLL = next;
            changed
        }
        Mouse::Down | Mouse::DoubleClick => {
            let id = h.find(mx, my).unwrap_or(0);
            SEARCH_FOCUS = id == ID_SEARCH;
            match id {
                i if (ID_LOG..ID_LOG + LOG_COUNT as u16).contains(&i) => {
                    LOG = (i - ID_LOG) as u8;
                    SCROLL = 0;
                    SELECTED = usize::MAX;
                    filter();
                }
                i if (ID_LEVEL..ID_LEVEL + 5).contains(&i) => {
                    let bit = 1u8 << (i - ID_LEVEL);
                    // Au moins un niveau reste affiché.
                    if LEVELS != bit {
                        LEVELS ^= bit;
                    }
                    filter();
                }
                ID_REFRESH => {
                    request_system_events();
                    load();
                }
                ID_EXPORT => export(),
                ID_REPORT => report_selected(),
                ID_CLEAR => {
                    ShellExecuteA(
                        core::ptr::null_mut(),
                        c"open".as_ptr(),
                        c"winlux-journal".as_ptr(),
                        c"clear".as_ptr(),
                        core::ptr::null(),
                        0,
                    );
                    say(tr4(&[
                        b"Journal Winlux effac\xE9.",
                        b"Winlux log cleared.",
                        b"Registro de Winlux borrado.",
                        b"Winlux-Protokoll gel\xF6scht.",
                    ]));
                    LAST_LOAD = GetTickCount().wrapping_sub(4000);
                }
                i if i >= ID_ROW => {
                    let k = SCROLL + (i - ID_ROW) as usize;
                    if k < VISIBLE_COUNT {
                        SELECTED = k;
                    }
                }
                _ => {}
            }
            true
        }
        _ => false,
    }
}

pub unsafe fn key(vk: u32, ch: u16) -> bool {
    if SEARCH_FOCUS && ch != 0 {
        if ch == 27 {
            SEARCH_FOCUS = false;
            return true;
        }
        let search = &mut *core::ptr::addr_of_mut!(SEARCH);
        if search.key(ch) {
            SCROLL = 0;
            filter();
            return true;
        }
        return false;
    }
    match vk {
        0x26 | 0x28 if VISIBLE_COUNT > 0 => {
            SELECTED = match (SELECTED, vk) {
                (usize::MAX, _) => 0,
                (s, 0x26) => s.saturating_sub(1),
                (s, _) => (s + 1).min(VISIBLE_COUNT - 1),
            };
            if SELECTED < SCROLL {
                SCROLL = SELECTED;
            } else if SELECTED >= SCROLL + ROWS_SHOWN {
                SCROLL = SELECTED + 1 - ROWS_SHOWN;
            }
            true
        }
        0x74 => {
            // F5
            request_system_events();
            load();
            true
        }
        _ => false,
    }
}

pub unsafe fn tick(_f: &Frame) -> bool {
    let now = GetTickCount();
    // L'export système arrive quelques instants après la demande.
    if EVENTS_PENDING != 0 {
        let mut lp = [0u8; 64];
        let n = events_linux_path(&mut lp);
        if ui::exists(zpath(&lp[..n]).ptr()) || now.wrapping_sub(EVENTS_PENDING) > 15000 {
            EVENTS_PENDING = 0;
            load();
            return true;
        }
    }
    if LOADED && now.wrapping_sub(LAST_LOAD) > 5000 {
        let before = COUNT;
        load();
        let m = (*core::ptr::addr_of!(MESSAGE)).len;
        return COUNT != before || m > 0;
    }
    false
}
