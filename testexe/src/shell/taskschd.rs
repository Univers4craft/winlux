//! Planificateur de tâches façon Windows 11 : bibliothèque des tâches,
//! création et modification (déclencheur, action, conditions), exécution à la
//! demande et historique.
//!
//! Les tâches sont les fichiers `Z:\etc\winlux\tasks\<id>.task` lus et
//! déclenchés par le service `winlux --tasks daemon` ; les programmes Windows
//! qui utilisent `ITaskService` écrivent les mêmes fichiers.

use crate::admin_ui::*;
use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

const NAV_W: i32 = 230;
const ROW_H: i32 = 36;
const MAX_TASKS: usize = 48;
const RAW: usize = 1536;

const ID_NAV: u16 = 1;
const ID_CREATE: u16 = 10;
const ID_RUN: u16 = 11;
const ID_END: u16 = 12;
const ID_TOGGLE: u16 = 13;
const ID_DELETE: u16 = 14;
const ID_REFRESH: u16 = 15;
const ID_EDIT: u16 = 16;
const ID_ROW: u16 = 100;
const ID_FIELD: u16 = 200;
const ID_KIND: u16 = 220;
const ID_DAY: u16 = 230;
const ID_EVENT: u16 = 240;
const ID_AC: u16 = 250;
const ID_NET: u16 = 251;
const ID_ENABLED: u16 = 252;
const ID_SAVE: u16 = 260;
const ID_CANCEL: u16 = 261;

const F_NAME: u16 = 0;
const F_DESC: u16 = 1;
const F_DATE: u16 = 2;
const F_TIME: u16 = 3;
const F_EVERY: u16 = 4;
const F_PROG: u16 = 5;
const F_ARGS: u16 = 6;
const F_DIR: u16 = 7;

const EVENTS: [&[u8]; 5] = [
    b"network-up",
    b"network-down",
    b"power-ac",
    b"power-battery",
    b"usb-added",
];

fn event_label(e: &[u8]) -> &'static [u8] {
    match e {
        b"network-up" => t(b"R\xE9seau connect\xE9", b"Network connected"),
        b"network-down" => t(b"R\xE9seau d\xE9connect\xE9", b"Network disconnected"),
        b"power-ac" => t(b"Branchement sur secteur", b"Plugged in"),
        b"power-battery" => t(b"Passage sur batterie", b"On battery"),
        b"usb-added" => t(
            b"P\xE9riph\xE9rique USB branch\xE9",
            b"USB device connected",
        ),
        _ => t(b"Journal d'\xE9v\xE9nements Windows", b"Windows event log"),
    }
}

fn kind_label(k: usize) -> &'static [u8] {
    match k {
        0 => t(b"Une fois", b"One time"),
        1 => t(b"Quotidien", b"Daily"),
        2 => t(b"Hebdomadaire", b"Weekly"),
        3 => t(b"Intervalle", b"Interval"),
        4 => t(b"Au d\xE9marrage", b"At startup"),
        5 => t(b"\xC0 l'ouverture de session", b"At log on"),
        _ => t(b"Sur un \xE9v\xE9nement", b"On an event"),
    }
}

fn day_short(d: usize) -> &'static [u8] {
    const FR: [&[u8]; 7] = [
        b"dim.", b"lun.", b"mar.", b"mer.", b"jeu.", b"ven.", b"sam.",
    ];
    const EN: [&[u8]; 7] = [b"Sun", b"Mon", b"Tue", b"Wed", b"Thu", b"Fri", b"Sat"];
    if unsafe { english() } {
        EN[d % 7]
    } else {
        FR[d % 7]
    }
}

// ---------- Données ----------

#[derive(Clone, Copy)]
struct Item {
    id: Field<64>,
    raw: [u8; RAW],
    len: usize,
    running: bool,
    next: Field<24>,
    last: Field<24>,
    result: Field<16>,
}

const EMPTY_ITEM: Item = Item {
    id: Field::new(),
    raw: [0; RAW],
    len: 0,
    running: false,
    next: Field::new(),
    last: Field::new(),
    result: Field::new(),
};

static mut ITEMS: [Item; MAX_TASKS] = [EMPTY_ITEM; MAX_TASKS];
static mut COUNT: usize = 0;
static mut SELECTED: usize = 0;
static mut SCROLL: usize = 0;
static mut PAGE: usize = 0;
static mut LAST_REFRESH: u32 = 0;
static mut FORCE: bool = true;
static mut HITS: Hits = Hits::new();
static mut NEXT_BUF: [u8; 4096] = [0; 4096];
static mut NEXT_LEN: usize = 0;
static mut HISTORY: [u8; 32768] = [0; 32768];
static mut HISTORY_LEN: usize = 0;
static mut HISTORY_SCROLL: usize = 0;
static mut NOTICE: &[u8] = b"";

impl Item {
    fn data(&self) -> &[u8] {
        &self.raw[..self.len]
    }
    fn name(&self) -> &[u8] {
        let n = field(self.data(), b"name");
        if n.is_empty() {
            self.id.get()
        } else {
            n
        }
    }
    fn enabled(&self) -> bool {
        field(self.data(), b"enabled") != b"0"
    }
    fn lines(&self, key: &'static [u8]) -> impl Iterator<Item = &[u8]> {
        self.data().split(|&c| c == b'\n').filter_map(move |l| {
            let l = l.strip_suffix(b"\r").unwrap_or(l);
            (l.len() > key.len() && l.starts_with(key) && l[key.len()] == b'=')
                .then(|| &l[key.len() + 1..])
        })
    }
    /// Première action : `(programme, arguments, dossier)`.
    fn action(&self) -> (&[u8], &[u8], &[u8]) {
        let legacy = field(self.data(), b"command");
        if !legacy.is_empty() {
            return (
                legacy,
                field(self.data(), b"arguments"),
                field(self.data(), b"workdir"),
            );
        }
        match self.lines(b"action").next() {
            Some(a) => (column(a, 0), column(a, 1), column(a, 2)),
            None => (b"", b"", b""),
        }
    }
}

unsafe fn refresh() {
    LAST_REFRESH = GetTickCount();
    FORCE = false;
    let mut selected_id = Field::<64>::new();
    if SELECTED < COUNT {
        selected_id = ITEMS[SELECTED].id;
    }
    COUNT = 0;
    let mut data: Win32FindDataA = core::mem::zeroed();
    let h = FindFirstFileA(c"Z:\\etc\\winlux\\tasks\\*".as_ptr(), &mut data);
    if !h.is_null() && h != -1isize as Handle {
        loop {
            let name = &*(&data.file_name as *const [i8; 260] as *const [u8; 260]);
            let n = name.iter().position(|&c| c == 0).unwrap_or(0);
            if n > 5 && &name[n - 5..n] == b".task" && COUNT < MAX_TASKS {
                let it = &mut ITEMS[COUNT];
                *it = EMPTY_ITEM;
                it.id.set(&name[..n - 5]);
                let mut p = Path::new(b"Z:\\etc\\winlux\\tasks\\");
                p.push(&name[..n]);
                it.len = read_file(p.ptr(), &mut it.raw, false);
                let mut run = Path::new(b"Z:\\run\\winlux\\tasks\\running\\");
                run.push(it.id.get());
                it.running = exists(run.ptr());
                let mut last = Path::new(b"Z:\\var\\lib\\winlux\\tasks\\");
                last.push(it.id.get()).push(b".last");
                let mut buf = [0u8; 256];
                let ln = read_file(last.ptr(), &mut buf, false);
                it.last.set(field(&buf[..ln], b"last_run_text"));
                it.result.set(field(&buf[..ln], b"last_result"));
                COUNT += 1;
            }
            if FindNextFileA(h, &mut data) == 0 {
                break;
            }
        }
        FindClose(h);
    }
    // Ordre alphabétique stable.
    for i in 1..COUNT {
        let mut j = i;
        while j > 0 && lower_cmp(ITEMS[j - 1].name(), ITEMS[j].name()) {
            ITEMS.swap(j - 1, j);
            j -= 1;
        }
    }
    NEXT_LEN = read_file(
        c"Z:\\run\\winlux\\tasks\\next.txt".as_ptr(),
        &mut NEXT_BUF,
        false,
    );
    for i in 0..COUNT {
        let id = ITEMS[i].id;
        for line in NEXT_BUF[..NEXT_LEN].split(|&c| c == b'\n') {
            if column(line, 0) == id.get() {
                ITEMS[i].next.set(column(line, 2));
            }
        }
        if id.get() == selected_id.get() {
            SELECTED = i;
        }
    }
    if SELECTED >= COUNT {
        SELECTED = COUNT.saturating_sub(1);
    }
    HISTORY_LEN = read_file(
        c"Z:\\var\\lib\\winlux\\tasks\\history.log".as_ptr(),
        &mut HISTORY,
        true,
    );
}

/// Vrai si `a` doit passer après `b` (comparaison sans casse).
fn lower_cmp(a: &[u8], b: &[u8]) -> bool {
    for (x, y) in a.iter().zip(b.iter()) {
        let (x, y) = (x.to_ascii_lowercase(), y.to_ascii_lowercase());
        if x != y {
            return x > y;
        }
    }
    a.len() > b.len()
}

/// Résumé lisible d'une ligne `trigger=`.
fn describe(line: &[u8], out: &mut Field<128>) {
    let mut w = line.split(|&c| c == b' ').filter(|w| !w.is_empty());
    let kind = w.next().unwrap_or(b"");
    let a = w.next().unwrap_or(b"");
    let b = w.next().unwrap_or(b"");
    match kind {
        b"once" => {
            out.push(t(b"Le ", b"On "));
            if a.len() == 10 && !unsafe { english() } {
                out.push(&a[8..10]);
                out.push(b"/");
                out.push(&a[5..7]);
                out.push(b"/");
                out.push(&a[..4]);
            } else {
                out.push(a);
            }
            out.push(t(b" \xE0 ", b" at "));
            out.push(b);
        }
        b"daily" => {
            if b.is_empty() || b == b"1" {
                out.push(t(b"Tous les jours \xE0 ", b"Daily at "));
            } else {
                out.push(t(b"Tous les ", b"Every "));
                out.push(b);
                out.push(t(b" jours \xE0 ", b" days at "));
            }
            out.push(a);
        }
        b"weekly" => {
            out.push(t(b"Chaque semaine (", b"Weekly ("));
            let mut first = true;
            for d in b.split(|&c| c == b',') {
                if let Some(n) = parse_num(d) {
                    if !first {
                        out.push(b", ");
                    }
                    first = false;
                    out.push(day_short(n as usize));
                }
            }
            out.push(t(b") \xE0 ", b") at "));
            out.push(a);
        }
        b"interval" => {
            out.push(t(b"Toutes les ", b"Every "));
            out.push(a);
            out.push(b" min");
        }
        b"boot" => out.push(kind_label(4)),
        b"logon" => out.push(kind_label(5)),
        b"event" => {
            out.push(t(b"Sur \xE9v\xE9nement : ", b"On event: "));
            out.push(event_label(a));
        }
        _ => out.push(line),
    }
}

unsafe fn triggers_text(it: &Item, out: &mut Field<128>) {
    let mut n = 0;
    for line in it.lines(b"trigger") {
        if n == 0 {
            describe(line, out);
        }
        n += 1;
    }
    if n == 0 {
        out.push(t(b"\xC0 la demande", b"On demand"));
    } else if n > 1 {
        out.push(b" (+");
        push_num(out, n - 1);
        out.push(b")");
    }
}

fn status_text(it: &Item) -> &'static [u8] {
    if it.running {
        t(b"En cours", b"Running")
    } else if it.enabled() {
        t(b"Pr\xEAte", b"Ready")
    } else {
        t(b"D\xE9sactiv\xE9e", b"Disabled")
    }
}

fn result_text(it: &Item, out: &mut Field<64>) {
    let r = it.result.get();
    if r.is_empty() {
        out.push(t(b"Jamais ex\xE9cut\xE9e", b"Has not run yet"));
    } else if r == b"0" {
        out.push(t(b"R\xE9ussite (0x0)", b"Success (0x0)"));
    } else {
        out.push(t(b"Code de sortie ", b"Exit code "));
        out.push(r);
    }
}

// ---------- Éditeur ----------

static mut EDITING: bool = false;
static mut EDIT_INDEX: usize = usize::MAX;
static mut FIELDS: [Field<200>; 8] = [Field::new(); 8];
static mut FOCUS: u16 = F_NAME;
static mut KIND: usize = 1;
static mut WEEKDAYS: u8 = 0b0011_1110;
static mut EVENT: usize = 0;
static mut AC_ONLY: bool = false;
static mut NETWORK: bool = false;
static mut ENABLED: bool = true;
static mut ERROR: &[u8] = b"";

unsafe fn now_fields() {
    let mut now = SystemTime::default();
    GetLocalTime(&mut now);
    let f = &mut FIELDS[F_DATE as usize];
    f.len = 0;
    push_num(f, now.year as i64);
    f.push(b"-");
    two(f, now.month as i64);
    f.push(b"-");
    two(f, now.day as i64);
    let f = &mut FIELDS[F_TIME as usize];
    f.len = 0;
    two(f, now.hour as i64);
    f.push(b":");
    two(f, now.minute as i64);
}

fn two(f: &mut Field<200>, v: i64) {
    if v < 10 {
        f.push(b"0");
    }
    push_num(f, v);
}

unsafe fn open_editor(index: usize) {
    EDITING = true;
    EDIT_INDEX = index;
    ERROR = b"";
    FOCUS = F_NAME;
    for f in FIELDS.iter_mut() {
        f.len = 0;
    }
    now_fields();
    FIELDS[F_EVERY as usize].set(b"1");
    KIND = 1;
    WEEKDAYS = 0b0011_1110;
    EVENT = 0;
    AC_ONLY = false;
    NETWORK = false;
    ENABLED = true;
    if index >= COUNT {
        FIELDS[F_TIME as usize].set(b"08:00");
        return;
    }
    let it = &ITEMS[index];
    FIELDS[F_NAME as usize].set(it.name());
    FIELDS[F_DESC as usize].set(field(it.data(), b"description"));
    let (p, a, d) = it.action();
    FIELDS[F_PROG as usize].set(p);
    FIELDS[F_ARGS as usize].set(a);
    FIELDS[F_DIR as usize].set(d);
    AC_ONLY = field(it.data(), b"ac_only") == b"1";
    NETWORK = field(it.data(), b"network") == b"1";
    ENABLED = it.enabled();
    if let Some(tr) = it.lines(b"trigger").next() {
        let mut w = tr.split(|&c| c == b' ').filter(|w| !w.is_empty());
        let kind = w.next().unwrap_or(b"");
        let a = w.next().unwrap_or(b"");
        let b = w.next().unwrap_or(b"");
        match kind {
            b"once" => {
                KIND = 0;
                FIELDS[F_DATE as usize].set(a);
                FIELDS[F_TIME as usize].set(b);
            }
            b"daily" => {
                KIND = 1;
                FIELDS[F_TIME as usize].set(a);
                FIELDS[F_EVERY as usize].set(if b.is_empty() { b"1" } else { b });
            }
            b"weekly" => {
                KIND = 2;
                FIELDS[F_TIME as usize].set(a);
                WEEKDAYS = 0;
                for d in b.split(|&c| c == b',') {
                    if let Some(n) = parse_num(d).filter(|&n| n < 7) {
                        WEEKDAYS |= 1 << n;
                    }
                }
            }
            b"interval" => {
                KIND = 3;
                FIELDS[F_EVERY as usize].set(a);
            }
            b"boot" => KIND = 4,
            b"logon" => KIND = 5,
            b"event" => {
                KIND = 6;
                EVENT = EVENTS.iter().position(|e| *e == a).unwrap_or(0);
            }
            _ => {}
        }
    } else {
        KIND = 5;
    }
}

fn valid_time(s: &[u8]) -> bool {
    s.len() == 5
        && s[2] == b':'
        && parse_num(&s[..2]).is_some_and(|h| h < 24)
        && parse_num(&s[3..]).is_some_and(|m| m < 60)
}

fn valid_date(s: &[u8]) -> bool {
    s.len() == 10
        && s[4] == b'-'
        && s[7] == b'-'
        && parse_num(&s[..4]).is_some()
        && parse_num(&s[5..7]).is_some_and(|m| (1..=12).contains(&m))
        && parse_num(&s[8..]).is_some_and(|d| (1..=31).contains(&d))
}

/// Identifiant de fichier dérivé du nom, unique parmi les tâches.
unsafe fn new_id(name: &[u8], out: &mut Field<64>) {
    out.len = 0;
    for &c in name.iter().take(48) {
        out.push(&[if c.is_ascii_alphanumeric() || c == b'-' || c == b'.' {
            c
        } else {
            b'_'
        }]);
    }
    if out.len == 0 {
        out.push(b"tache");
    }
    let base = *out;
    let mut n = 2;
    while (0..COUNT).any(|i| ITEMS[i].id.get() == out.get()) {
        *out = base;
        out.push(b"_");
        push_num(out, n);
        n += 1;
    }
}

unsafe fn save_editor() {
    let name = trim(FIELDS[F_NAME as usize].get());
    let prog = trim(FIELDS[F_PROG as usize].get());
    let time = FIELDS[F_TIME as usize].get();
    let every = parse_num(FIELDS[F_EVERY as usize].get()).unwrap_or(0);
    ERROR = if name.is_empty() {
        t(
            b"Indiquez un nom pour la t\xE2che.",
            b"Enter a name for the task.",
        )
    } else if prog.is_empty() {
        t(
            b"Indiquez le programme ou le script \xE0 lancer.",
            b"Enter the program or script to start.",
        )
    } else if matches!(KIND, 0..=2) && !valid_time(time) {
        t(
            b"Heure invalide : utilisez HH:MM.",
            b"Invalid time: use HH:MM.",
        )
    } else if KIND == 0 && !valid_date(FIELDS[F_DATE as usize].get()) {
        t(
            b"Date invalide : utilisez AAAA-MM-JJ.",
            b"Invalid date: use YYYY-MM-DD.",
        )
    } else if matches!(KIND, 1 | 3) && every == 0 {
        t(
            b"Indiquez un nombre sup\xE9rieur \xE0 z\xE9ro.",
            b"Enter a number greater than zero.",
        )
    } else if KIND == 2 && WEEKDAYS == 0 {
        t(b"Choisissez au moins un jour.", b"Choose at least one day.")
    } else {
        b""
    };
    if !ERROR.is_empty() {
        return;
    }
    let mut out = [0u8; RAW + 512];
    let mut o = Out {
        buf: &mut out,
        len: 0,
    };
    o.line(b"name=", name);
    o.line(b"description=", FIELDS[F_DESC as usize].get());
    o.line(b"enabled=", if ENABLED { b"1" } else { b"0" });
    let mut tr = Field::<200>::new();
    match KIND {
        0 => {
            tr.push(b"once ");
            tr.push(FIELDS[F_DATE as usize].get());
            tr.push(b" ");
            tr.push(time);
        }
        1 => {
            tr.push(b"daily ");
            tr.push(time);
            if every > 1 {
                tr.push(b" ");
                push_num(&mut tr, every as i64);
            }
        }
        2 => {
            tr.push(b"weekly ");
            tr.push(time);
            tr.push(b" ");
            let mut first = true;
            for d in 0..7 {
                if WEEKDAYS & (1 << d) != 0 {
                    if !first {
                        tr.push(b",");
                    }
                    first = false;
                    push_num(&mut tr, d as i64);
                }
            }
        }
        3 => {
            tr.push(b"interval ");
            push_num(&mut tr, every as i64);
        }
        4 => tr.push(b"boot"),
        5 => tr.push(b"logon"),
        _ => {
            tr.push(b"event ");
            tr.push(EVENTS[EVENT]);
        }
    }
    o.line(b"trigger=", tr.get());
    let mut act = Field::<600>::new();
    act.push(prog);
    act.push(b"\t");
    act.push(trim(FIELDS[F_ARGS as usize].get()));
    act.push(b"\t");
    act.push(trim(FIELDS[F_DIR as usize].get()));
    o.line(b"action=", act.get());
    o.line(b"ac_only=", if AC_ONLY { b"1" } else { b"0" });
    o.line(b"network=", if NETWORK { b"1" } else { b"0" });
    // Lignes conservées : dossier, auteur, délai et déclencheurs ou actions
    // supplémentaires créés par un programme via ITaskService.
    let mut id = Field::<64>::new();
    if EDIT_INDEX < COUNT {
        let it = ITEMS[EDIT_INDEX];
        id = it.id;
        let legacy = !field(it.data(), b"command").is_empty();
        let (mut seen_trigger, mut seen_action) = (false, legacy);
        for line in it.data().split(|&c| c == b'\n') {
            let key = line.split(|&c| c == b'=').next().unwrap_or(b"");
            let keep = match key {
                b"name" | b"description" | b"enabled" | b"ac_only" | b"network" | b"command"
                | b"arguments" | b"workdir" => false,
                b"trigger" => core::mem::replace(&mut seen_trigger, true),
                b"action" => core::mem::replace(&mut seen_action, true),
                _ => !line.is_empty(),
            };
            if keep {
                o.raw(line);
                o.raw(b"\n");
            }
        }
    } else {
        new_id(name, &mut id);
        o.line(b"folder=", b"\\");
        o.line(b"time_limit=", b"4320");
    }
    CreateDirectoryA(c"Z:\\etc\\winlux".as_ptr(), core::ptr::null_mut());
    CreateDirectoryA(c"Z:\\etc\\winlux\\tasks".as_ptr(), core::ptr::null_mut());
    let mut p = Path::new(b"Z:\\etc\\winlux\\tasks\\");
    p.push(id.get()).push(b".task");
    let len = o.len;
    if write_file(&p, &out[..len]) {
        EDITING = false;
        FORCE = true;
        NOTICE = t(b"T\xE2che enregistr\xE9e.", b"Task saved.");
    } else {
        ERROR = t(
            b"Enregistrement impossible : droits insuffisants sur /etc/winlux/tasks.",
            b"Cannot save: no write access to /etc/winlux/tasks.",
        );
    }
}

struct Out<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl Out<'_> {
    fn raw(&mut self, s: &[u8]) {
        for &c in s {
            if self.len < self.buf.len() {
                self.buf[self.len] = c;
                self.len += 1;
            }
        }
    }
    fn line(&mut self, key: &[u8], value: &[u8]) {
        self.raw(key);
        // Une valeur ne peut pas couper le format ligne par ligne.
        for &c in value {
            self.raw(&[if c == b'\n' || c == b'\r' { b' ' } else { c }]);
        }
        self.raw(b"\n");
    }
}

// ---------- Actions ----------

unsafe fn command(verb: &[u8]) {
    if SELECTED >= COUNT {
        return;
    }
    let mut args = Field::<120>::new();
    args.push(b"--tasks ");
    args.push(verb);
    args.push(b" ");
    args.push(ITEMS[SELECTED].id.get());
    NOTICE = if winlux(args.get()) {
        match verb {
            b"run" => t(b"Ex\xE9cution demand\xE9e.", b"Run requested."),
            b"end" => t(b"Arr\xEAt demand\xE9.", b"Stop requested."),
            b"delete" => t(b"T\xE2che supprim\xE9e.", b"Task deleted."),
            _ => t(b"Modification enregistr\xE9e.", b"Change saved."),
        }
    } else {
        t(
            b"Le service Winlux est introuvable.",
            b"The Winlux service was not found.",
        )
    };
    FORCE = true;
    LAST_REFRESH = GetTickCount().wrapping_sub(1700);
}

// ---------- Dessin ----------

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    if FORCE || GetTickCount().wrapping_sub(LAST_REFRESH) > 2000 {
        refresh();
    }
    let h = &mut HITS;
    h.clear();
    let c = &f.client;
    let p = pal();
    fill(g.hdc, g.b, c.left, c.top, c.right, c.bottom, p.window);
    fill(g.hdc, g.b, c.left, c.top, c.left + NAV_W, c.bottom, p.mica);
    fill(
        g.hdc,
        g.b,
        c.left + NAV_W,
        c.top,
        c.left + NAV_W + 1,
        c.bottom,
        p.border,
    );
    glyph(g.hdc, g.b, g.f, Glyph::Clock, c.left + 16, c.top + 16, 24);
    text_fit(
        g.hdc,
        g.f.semibold,
        13,
        p.text,
        c.left + 48,
        c.top + 20,
        t(b"Planificateur de t\xE2ches", b"Task Scheduler"),
        NAV_W - 64,
    );
    nav_item(
        g,
        h,
        c.left,
        c.top + 64,
        NAV_W,
        ID_NAV,
        t(b"Biblioth\xE8que", b"Library"),
        Glyph::Folder,
        PAGE == 0,
    );
    nav_item(
        g,
        h,
        c.left,
        c.top + 104,
        NAV_W,
        ID_NAV + 1,
        t(b"Historique", b"History"),
        Glyph::List,
        PAGE == 1,
    );
    let area = rect(c.left + NAV_W + 1, c.top, c.right, c.bottom);
    if PAGE == 0 {
        draw_library(g, h, &area);
    } else {
        draw_history(g, h, &area);
    }
    if EDITING {
        draw_editor(g, h, c);
    }
}

unsafe fn draw_library(g: &mut Gfx, h: &mut Hits, a: &Rect) {
    let p = pal();
    let x = a.left + 24;
    text(
        g.hdc,
        g.f.large,
        p.text,
        x,
        a.top + 18,
        t(
            b"Biblioth\xE8que du Planificateur",
            b"Task Scheduler Library",
        ),
    );
    // Barre de commandes.
    let has = SELECTED < COUNT;
    let running = has && ITEMS[SELECTED].running;
    let enabled = has && ITEMS[SELECTED].enabled();
    let buttons: [(u16, &[u8], bool, bool); 7] = [
        (
            ID_CREATE,
            t(b"Cr\xE9er une t\xE2che", b"Create task"),
            true,
            true,
        ),
        (ID_EDIT, t(b"Propri\xE9t\xE9s", b"Properties"), false, has),
        (ID_RUN, t(b"Ex\xE9cuter", b"Run"), false, has && !running),
        (ID_END, t(b"Fin", b"End"), false, running),
        (
            ID_TOGGLE,
            if enabled {
                t(b"D\xE9sactiver", b"Disable")
            } else {
                t(b"Activer", b"Enable")
            },
            false,
            has,
        ),
        (ID_DELETE, t(b"Supprimer", b"Delete"), false, has),
        (ID_REFRESH, t(b"Actualiser", b"Refresh"), false, true),
    ];
    let mut bx = x;
    for (id, label, primary, on) in buttons {
        let w = button_w(label);
        if bx + w > a.right - 16 {
            break;
        }
        button(
            g,
            h,
            rect(bx, a.top + 60, bx + w, a.top + 92),
            id,
            label,
            primary,
            on,
        );
        bx += w + 8;
    }
    let width = a.right - x - 16;
    let cols: [(&[u8], i32); 6] = [
        (t(b"Nom", b"Name"), width * 20 / 100),
        (t(b"\xC9tat", b"Status"), width * 10 / 100),
        (t(b"D\xE9clencheurs", b"Triggers"), width * 22 / 100),
        (
            t(b"Prochaine ex\xE9cution", b"Next run time"),
            width * 17 / 100,
        ),
        (
            t(b"Derni\xE8re ex\xE9cution", b"Last run time"),
            width * 17 / 100,
        ),
        (t(b"R\xE9sultat", b"Last run result"), width * 14 / 100),
    ];
    let mut y = header(g, x, a.top + 108, a.right - 16, &cols);
    let details_h = if has { 132 } else { 0 };
    let list_bottom = a.bottom - details_h - 36;
    if COUNT == 0 {
        text(g.hdc, g.f.body, p.text2, x, y + 12, t(
            b"Aucune t\xE2che. Cliquez sur \xAB Cr\xE9er une t\xE2che \xBB pour en ajouter une.",
            b"No tasks yet. Click \"Create task\" to add one.",
        ));
    }
    let visible = ((list_bottom - y) / ROW_H).max(1) as usize;
    if SCROLL + visible > COUNT {
        SCROLL = COUNT.saturating_sub(visible);
    }
    for i in SCROLL..COUNT.min(SCROLL + visible) {
        let it = &ITEMS[i];
        let r = rect(x - 8, y, a.right - 16, y + ROW_H - 2);
        let hover = h.add(r, ID_ROW + i as u16);
        if i == SELECTED {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, p.selected);
        } else if hover {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, p.hover);
        }
        let ty = y + 9;
        let mut cx = x;
        cell(g, cx, ty, cols[0].1, it.name(), p.text);
        cx += cols[0].1;
        let sc = if it.running {
            p.accent
        } else if it.enabled() {
            p.text
        } else {
            p.text3
        };
        cell(g, cx, ty, cols[1].1, status_text(it), sc);
        cx += cols[1].1;
        let mut tr = Field::<128>::new();
        triggers_text(it, &mut tr);
        cell(g, cx, ty, cols[2].1, tr.get(), p.text2);
        cx += cols[2].1;
        cell(
            g,
            cx,
            ty,
            cols[3].1,
            if it.next.len > 0 { it.next.get() } else { b"-" },
            p.text2,
        );
        cx += cols[3].1;
        cell(
            g,
            cx,
            ty,
            cols[4].1,
            if it.last.len > 0 { it.last.get() } else { b"-" },
            p.text2,
        );
        cx += cols[4].1;
        let mut res = Field::<64>::new();
        result_text(it, &mut res);
        cell(g, cx, ty, a.right - 16 - cx, res.get(), p.text2);
        y += ROW_H;
    }
    if has {
        let it = &ITEMS[SELECTED];
        let top = a.bottom - details_h - 24;
        panel(
            g.hdc,
            g.b,
            x - 8,
            top,
            a.right - 16,
            a.bottom - 24,
            6,
            p.surface,
            p.border,
        );
        let dx = x + 8;
        text_fit(
            g.hdc,
            g.f.semibold,
            13,
            p.text,
            dx,
            top + 12,
            it.name(),
            a.right - dx - 32,
        );
        let desc = field(it.data(), b"description");
        let desc: &[u8] = if desc.is_empty() {
            t(b"(aucune description)", b"(no description)")
        } else {
            desc
        };
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text2,
            dx,
            top + 34,
            desc,
            a.right - dx - 32,
        );
        let (prog, args, _) = it.action();
        let mut line = Field::<200>::new();
        line.push(t(b"Action : d\xE9marrer ", b"Action: start "));
        line.push(prog);
        if !args.is_empty() {
            line.push(b" ");
            line.push(args);
        }
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            dx,
            top + 58,
            line.get(),
            a.right - dx - 32,
        );
        let mut cond = Field::<200>::new();
        cond.push(t(b"Conditions : ", b"Conditions: "));
        let ac = field(it.data(), b"ac_only") == b"1";
        let net = field(it.data(), b"network") == b"1";
        if !ac && !net {
            cond.push(t(b"aucune", b"none"));
        }
        if ac {
            cond.push(t(b"uniquement sur secteur", b"only on AC power"));
        }
        if net {
            if ac {
                cond.push(b", ");
            }
            cond.push(t(
                b"seulement si le r\xE9seau est disponible",
                b"only if a network is available",
            ));
        }
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text2,
            dx,
            top + 80,
            cond.get(),
            a.right - dx - 32,
        );
        let mut loc = Field::<200>::new();
        loc.push(t(b"Emplacement : ", b"Location: "));
        let folder = field(it.data(), b"folder");
        loc.push(if folder.is_empty() { b"\\" } else { folder });
        let author = field(it.data(), b"author");
        if !author.is_empty() {
            loc.push(t(b"  \xB7  Auteur : ", b"  \xB7  Author: "));
            loc.push(author);
        }
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text3,
            dx,
            top + 102,
            loc.get(),
            a.right - dx - 32,
        );
    }
    if !NOTICE.is_empty() {
        text(g.hdc, g.f.caption, p.text3, x, a.bottom - 20, NOTICE);
    }
}

/// Libellé d'un événement d'historique et de son détail.
fn history_event(kind: &[u8]) -> &'static [u8] {
    match kind {
        b"start" => t(b"T\xE2che d\xE9marr\xE9e", b"Task started"),
        b"end" => t(b"T\xE2che termin\xE9e", b"Task completed"),
        b"skip" => t(b"T\xE2che non d\xE9marr\xE9e", b"Task not started"),
        b"timeout" => t(
            b"T\xE2che arr\xEAt\xE9e (dur\xE9e d\xE9pass\xE9e)",
            b"Task stopped (time limit)",
        ),
        _ => t(b"\xC9chec du lancement", b"Launch failure"),
    }
}

fn history_detail(kind: &[u8], detail: &[u8], out: &mut Field<128>) {
    match (kind, detail) {
        (b"start", b"manual") => out.push(t(b"Lanc\xE9e manuellement", b"Started manually")),
        (b"start", b"time") => out.push(t(b"D\xE9clencheur horaire", b"Time trigger")),
        (b"start", b"interval") => {
            out.push(t(b"D\xE9clencheur \xE0 intervalle", b"Interval trigger"))
        }
        (b"start", b"boot") => out.push(t(b"D\xE9marrage du syst\xE8me", b"System startup")),
        (b"start", b"logon") => out.push(t(b"Ouverture de session", b"User log on")),
        (b"skip", b"battery") => out.push(t(
            b"L'ordinateur est sur batterie",
            b"The computer is on battery",
        )),
        (b"skip", b"network") => {
            out.push(t(b"Aucun r\xE9seau disponible", b"No network available"))
        }
        (b"skip", b"running") => out.push(t(
            b"Une instance est d\xE9j\xE0 en cours",
            b"An instance is already running",
        )),
        (b"end", code) => {
            out.push(t(b"Code de sortie ", b"Exit code "));
            out.push(code);
        }
        (b"timeout", mins) => {
            out.push(t(b"Limite : ", b"Limit: "));
            out.push(mins);
            out.push(b" min");
        }
        (b"start", ev) if ev.starts_with(b"event:") => {
            out.push(t(b"\xC9v\xE9nement : ", b"Event: "));
            out.push(event_label(&ev[6..]));
        }
        _ => out.push(detail),
    }
}

unsafe fn draw_history(g: &mut Gfx, h: &mut Hits, a: &Rect) {
    let p = pal();
    let x = a.left + 24;
    text(
        g.hdc,
        g.f.large,
        p.text,
        x,
        a.top + 18,
        t(b"Historique d'ex\xE9cution", b"Run history"),
    );
    button(
        g,
        h,
        rect(
            x,
            a.top + 60,
            x + button_w(t(b"Actualiser", b"Refresh")),
            a.top + 92,
        ),
        ID_REFRESH,
        t(b"Actualiser", b"Refresh"),
        false,
        true,
    );
    let width = a.right - x - 16;
    let cols: [(&[u8], i32); 4] = [
        (t(b"Date et heure", b"Date and time"), 150),
        (t(b"T\xE2che", b"Task"), width * 25 / 100),
        (t(b"\xC9v\xE9nement", b"Event"), width * 25 / 100),
        (t(b"D\xE9tails", b"Details"), width - 150 - width / 2),
    ];
    let mut y = header(g, x, a.top + 108, a.right - 16, &cols);
    // Lignes du plus récent au plus ancien ; une première ligne coupée par
    // la lecture de fin de fichier est ignorée.
    let data = &HISTORY[..HISTORY_LEN];
    let mut lines: [&[u8]; 400] = [b""; 400];
    let mut n = 0;
    for (k, line) in data.split(|&c| c == b'\n').enumerate() {
        if (k == 0 && HISTORY_LEN == HISTORY.len()) || line.is_empty() {
            continue;
        }
        if n == lines.len() {
            lines.copy_within(1.., 0);
            n -= 1;
        }
        lines[n] = line;
        n += 1;
    }
    if n == 0 {
        text(
            g.hdc,
            g.f.body,
            p.text2,
            x,
            y + 12,
            t(
                b"Aucune ex\xE9cution enregistr\xE9e.",
                b"No runs recorded yet.",
            ),
        );
    }
    let visible = ((a.bottom - 24 - y) / 30).max(1) as usize;
    if HISTORY_SCROLL + visible > n {
        HISTORY_SCROLL = n.saturating_sub(visible);
    }
    for k in HISTORY_SCROLL..n.min(HISTORY_SCROLL + visible) {
        let line = lines[n - 1 - k];
        let kind = column(line, 4);
        let mut cx = x;
        cell(g, cx, y + 6, cols[0].1, column(line, 1), p.text2);
        cx += cols[0].1;
        cell(g, cx, y + 6, cols[1].1, column(line, 3), p.text);
        cx += cols[1].1;
        let color = match kind {
            b"error" | b"timeout" => rgb(0xE8, 0x11, 0x23),
            b"skip" => p.text3,
            _ => p.text,
        };
        cell(g, cx, y + 6, cols[2].1, history_event(kind), color);
        cx += cols[2].1;
        let mut d = Field::<128>::new();
        history_detail(kind, column(line, 5), &mut d);
        cell(g, cx, y + 6, a.right - 16 - cx, d.get(), p.text2);
        y += 30;
    }
}

unsafe fn draw_editor(g: &mut Gfx, h: &mut Hits, c: &Rect) {
    let p = pal();
    let title = if EDIT_INDEX < COUNT {
        t(b"Propri\xE9t\xE9s de la t\xE2che", b"Task properties")
    } else {
        t(b"Cr\xE9er une t\xE2che", b"Create task")
    };
    let d = dialog(g, h, c, 720, 640, title);
    let x = d.left + 24;
    let r = d.right - 24;
    let mut y = d.top + 64;
    let fieldw = |x: i32, r: i32, y: i32| rect(x, y, r, y + 32);
    label(g, x, y, t(b"Nom", b"Name"));
    input(
        g,
        h,
        fieldw(x, (x + r) / 2 - 8, y + 18),
        ID_FIELD + F_NAME,
        FIELDS[0].get(),
        FOCUS == F_NAME,
        b"",
    );
    label(g, (x + r) / 2 + 8, y, t(b"Description", b"Description"));
    input(
        g,
        h,
        fieldw((x + r) / 2 + 8, r, y + 18),
        ID_FIELD + F_DESC,
        FIELDS[1].get(),
        FOCUS == F_DESC,
        t(b"Facultative", b"Optional"),
    );
    y += 64;
    text(
        g.hdc,
        g.f.semibold,
        p.text,
        x,
        y,
        t(b"D\xE9clencheur", b"Trigger"),
    );
    y += 22;
    let mut cx = x;
    for k in 0..7 {
        let w = text_width(kind_label(k), 13) + 32;
        if cx + w > r {
            cx = x;
            y += 36;
        }
        cx += chip(g, h, cx, y, ID_KIND + k as u16, kind_label(k), KIND == k);
    }
    y += 42;
    match KIND {
        0 => {
            label(g, x, y, t(b"Date (AAAA-MM-JJ)", b"Date (YYYY-MM-DD)"));
            input(
                g,
                h,
                fieldw(x, x + 160, y + 18),
                ID_FIELD + F_DATE,
                FIELDS[2].get(),
                FOCUS == F_DATE,
                b"",
            );
            label(g, x + 180, y, t(b"Heure (HH:MM)", b"Time (HH:MM)"));
            input(
                g,
                h,
                fieldw(x + 180, x + 300, y + 18),
                ID_FIELD + F_TIME,
                FIELDS[3].get(),
                FOCUS == F_TIME,
                b"",
            );
        }
        1 => {
            label(g, x, y, t(b"Heure (HH:MM)", b"Time (HH:MM)"));
            input(
                g,
                h,
                fieldw(x, x + 120, y + 18),
                ID_FIELD + F_TIME,
                FIELDS[3].get(),
                FOCUS == F_TIME,
                b"",
            );
            label(g, x + 140, y, t(b"Tous les (jours)", b"Recur every (days)"));
            input(
                g,
                h,
                fieldw(x + 140, x + 240, y + 18),
                ID_FIELD + F_EVERY,
                FIELDS[4].get(),
                FOCUS == F_EVERY,
                b"",
            );
        }
        2 => {
            label(g, x, y, t(b"Heure (HH:MM)", b"Time (HH:MM)"));
            input(
                g,
                h,
                fieldw(x, x + 120, y + 18),
                ID_FIELD + F_TIME,
                FIELDS[3].get(),
                FOCUS == F_TIME,
                b"",
            );
            let mut dx = x + 140;
            for d in [1usize, 2, 3, 4, 5, 6, 0] {
                dx += chip(
                    g,
                    h,
                    dx,
                    y + 19,
                    ID_DAY + d as u16,
                    day_short(d),
                    WEEKDAYS & (1 << d) != 0,
                );
            }
        }
        3 => {
            label(
                g,
                x,
                y,
                t(
                    b"R\xE9p\xE9ter toutes les (minutes)",
                    b"Repeat every (minutes)",
                ),
            );
            input(
                g,
                h,
                fieldw(x, x + 120, y + 18),
                ID_FIELD + F_EVERY,
                FIELDS[4].get(),
                FOCUS == F_EVERY,
                b"",
            );
        }
        6 => {
            let mut ex = x;
            let mut ey = y;
            for (k, e) in EVENTS.iter().enumerate() {
                let w = text_width(event_label(e), 13) + 32;
                if ex + w > r {
                    ex = x;
                    ey += 36;
                }
                ex += chip(
                    g,
                    h,
                    ex,
                    ey,
                    ID_EVENT + k as u16,
                    event_label(e),
                    EVENT == k,
                );
            }
        }
        _ => {
            let s = if KIND == 4 {
                t(
                    b"La t\xE2che d\xE9marre \xE0 chaque d\xE9marrage de l'ordinateur.",
                    b"The task starts every time the computer starts.",
                )
            } else {
                t(
                    b"La t\xE2che d\xE9marre \xE0 chaque ouverture de session.",
                    b"The task starts every time a user logs on.",
                )
            };
            text(g.hdc, g.f.body, p.text2, x, y + 8, s);
        }
    }
    y += 64;
    text(
        g.hdc,
        g.f.semibold,
        p.text,
        x,
        y,
        t(
            b"Action : d\xE9marrer un programme",
            b"Action: start a program",
        ),
    );
    y += 22;
    label(
        g,
        x,
        y,
        t(
            b"Programme ou script (.exe, .sh, .py ou commande Linux)",
            b"Program or script (.exe, .sh, .py or Linux command)",
        ),
    );
    input(
        g,
        h,
        fieldw(x, r, y + 18),
        ID_FIELD + F_PROG,
        FIELDS[5].get(),
        FOCUS == F_PROG,
        b"C:\\Program Files\\...\\app.exe",
    );
    y += 60;
    label(g, x, y, t(b"Arguments", b"Arguments"));
    input(
        g,
        h,
        fieldw(x, (x + r) / 2 - 8, y + 18),
        ID_FIELD + F_ARGS,
        FIELDS[6].get(),
        FOCUS == F_ARGS,
        t(b"Facultatifs", b"Optional"),
    );
    label(g, (x + r) / 2 + 8, y, t(b"Commencer dans", b"Start in"));
    input(
        g,
        h,
        fieldw((x + r) / 2 + 8, r, y + 18),
        ID_FIELD + F_DIR,
        FIELDS[7].get(),
        FOCUS == F_DIR,
        t(b"Facultatif", b"Optional"),
    );
    y += 66;
    text(
        g.hdc,
        g.f.semibold,
        p.text,
        x,
        y,
        t(b"Conditions", b"Conditions"),
    );
    y += 26;
    toggle(
        g,
        h,
        x,
        y,
        ID_AC,
        AC_ONLY,
        t(
            b"Ne d\xE9marrer que si l'ordinateur est sur secteur",
            b"Start only if the computer is on AC power",
        ),
    );
    y += 30;
    toggle(
        g,
        h,
        x,
        y,
        ID_NET,
        NETWORK,
        t(
            b"Ne d\xE9marrer que si une connexion r\xE9seau est disponible",
            b"Start only if a network connection is available",
        ),
    );
    y += 30;
    toggle(
        g,
        h,
        x,
        y,
        ID_ENABLED,
        ENABLED,
        t(b"T\xE2che activ\xE9e", b"Task enabled"),
    );
    if !ERROR.is_empty() {
        text(
            g.hdc,
            g.f.body,
            rgb(0xE8, 0x11, 0x23),
            x,
            d.bottom - 44,
            ERROR,
        );
    }
    let cancel = t(b"Annuler", b"Cancel");
    let ok = t(b"Enregistrer", b"Save");
    let cw = button_w(cancel);
    let ow = button_w(ok).max(100);
    button(
        g,
        h,
        rect(r - cw, d.bottom - 52, r, d.bottom - 20),
        ID_CANCEL,
        cancel,
        false,
        true,
    );
    button(
        g,
        h,
        rect(r - cw - 8 - ow, d.bottom - 52, r - cw - 8, d.bottom - 20),
        ID_SAVE,
        ok,
        true,
        true,
    );
}

// ---------- Entrées ----------

unsafe fn click(id: u16) -> bool {
    if EDITING {
        match id {
            ID_SAVE => save_editor(),
            ID_CANCEL => EDITING = false,
            ID_AC => AC_ONLY = !AC_ONLY,
            ID_NET => NETWORK = !NETWORK,
            ID_ENABLED => ENABLED = !ENABLED,
            i if (ID_FIELD..ID_FIELD + 8).contains(&i) => FOCUS = i - ID_FIELD,
            i if (ID_KIND..ID_KIND + 7).contains(&i) => KIND = (i - ID_KIND) as usize,
            i if (ID_DAY..ID_DAY + 7).contains(&i) => WEEKDAYS ^= 1 << (i - ID_DAY),
            i if (ID_EVENT..ID_EVENT + EVENTS.len() as u16).contains(&i) => {
                EVENT = (i - ID_EVENT) as usize
            }
            _ => return false,
        }
        return true;
    }
    match id {
        i if (ID_NAV..ID_NAV + 2).contains(&i) => {
            PAGE = (i - ID_NAV) as usize;
            FORCE = true;
        }
        ID_CREATE => open_editor(usize::MAX),
        ID_EDIT => {
            if SELECTED < COUNT {
                open_editor(SELECTED)
            }
        }
        ID_RUN => command(b"run"),
        ID_END => command(b"end"),
        ID_TOGGLE => {
            let verb: &[u8] = if SELECTED < COUNT && ITEMS[SELECTED].enabled() {
                b"disable"
            } else {
                b"enable"
            };
            command(verb)
        }
        ID_DELETE => command(b"delete"),
        ID_REFRESH => FORCE = true,
        i if i >= ID_ROW && ((i - ID_ROW) as usize) < COUNT => SELECTED = (i - ID_ROW) as usize,
        _ => return false,
    }
    true
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, _f: &Frame) -> bool {
    match ev {
        Mouse::Move => {
            HITS.mx = x;
            HITS.my = y;
            true
        }
        Mouse::Down => match HITS.find(x, y) {
            Some(id) => click(id),
            None => false,
        },
        Mouse::DoubleClick => match HITS.find(x, y) {
            Some(i) if !EDITING && i >= ID_ROW && ((i - ID_ROW) as usize) < COUNT => {
                SELECTED = (i - ID_ROW) as usize;
                open_editor(SELECTED);
                true
            }
            _ => false,
        },
        Mouse::Wheel(delta) if !EDITING => {
            let s = if PAGE == 0 {
                &mut SCROLL
            } else {
                &mut HISTORY_SCROLL
            };
            *s = if delta > 0 {
                s.saturating_sub(3)
            } else {
                *s + 3
            };
            true
        }
        _ => false,
    }
}

pub unsafe fn key(vk: u32, ch: u16) -> bool {
    if EDITING {
        match ch {
            27 => EDITING = false,
            13 => save_editor(),
            9 => {
                // Champs visibles pour ce déclencheur, dans l'ordre.
                let order: &[u16] = match KIND {
                    0 => &[F_NAME, F_DESC, F_DATE, F_TIME, F_PROG, F_ARGS, F_DIR],
                    1 => &[F_NAME, F_DESC, F_TIME, F_EVERY, F_PROG, F_ARGS, F_DIR],
                    2 => &[F_NAME, F_DESC, F_TIME, F_PROG, F_ARGS, F_DIR],
                    3 => &[F_NAME, F_DESC, F_EVERY, F_PROG, F_ARGS, F_DIR],
                    _ => &[F_NAME, F_DESC, F_PROG, F_ARGS, F_DIR],
                };
                let i = order
                    .iter()
                    .position(|&f| f == FOCUS)
                    .map_or(0, |i| (i + 1) % order.len());
                FOCUS = order[i];
            }
            0 => return false,
            _ => return FIELDS[FOCUS as usize].key(ch),
        }
        return true;
    }
    match vk {
        0x26 if SELECTED > 0 => SELECTED -= 1,
        0x28 if SELECTED + 1 < COUNT => SELECTED += 1,
        0x2E if SELECTED < COUNT => command(b"delete"),
        0x74 => FORCE = true,
        0x0D if SELECTED < COUNT => open_editor(SELECTED),
        _ => return false,
    }
    true
}

pub unsafe fn tick(_f: &Frame) -> bool {
    if EDITING {
        return false;
    }
    // Rafraîchit l'état (en cours, prochaine exécution) toutes les 2 s.
    GetTickCount().wrapping_sub(LAST_REFRESH) > 2000
}

/// Ouvre directement l'éditeur d'une nouvelle tâche.
pub unsafe fn create_task() {
    PAGE = 0;
    FORCE = true;
    refresh();
    open_editor(usize::MAX);
}
