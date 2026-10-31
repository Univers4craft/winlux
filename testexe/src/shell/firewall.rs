//! Pare-feu Winlux façon « Pare-feu et protection du réseau » de Windows 11 :
//! profils domaine/privé/public, règles entrantes et sortantes (port,
//! protocole, application), journal des connexions bloquées et alerte quand
//! une application écoute pour la première fois.
//!
//! La configuration est `Z:\etc\winlux\firewall.conf` ; `winlux --firewall
//! apply` la traduit en règles nftables. Le service écrit l'état, le journal
//! et l'alerte en attente dans `Z:\run\winlux\firewall` et
//! `Z:\var\lib\winlux\firewall`.

use crate::admin_ui::*;
use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

const NAV_W: i32 = 230;
const ROW_H: i32 = 36;
const MAX_RULES: usize = 64;
const PROFILES: [&[u8]; 3] = [b"domain", b"private", b"public"];

const ID_NAV: u16 = 1;
const ID_PROFILE_ON: u16 = 20;
const ID_PROFILE_SET: u16 = 30;
const ID_INBOUND: u16 = 40;
const ID_OUTBOUND: u16 = 50;
const ID_NOTIFY: u16 = 60;
const ID_NEW: u16 = 70;
const ID_EDIT: u16 = 71;
const ID_TOGGLE: u16 = 72;
const ID_DELETE: u16 = 73;
const ID_CLEAR_LOG: u16 = 80;
const ID_REFRESH: u16 = 81;
const ID_ROW: u16 = 100;
const ID_FIELD: u16 = 200;
const ID_ACTION: u16 = 210;
const ID_PROTO: u16 = 220;
const ID_PROF: u16 = 230;
const ID_ENABLED: u16 = 240;
const ID_SAVE: u16 = 250;
const ID_CANCEL: u16 = 251;
const ID_ALLOW: u16 = 260;
const ID_BLOCK: u16 = 261;

fn profile_name(i: usize) -> &'static [u8] {
    match i {
        0 => t(b"R\xE9seau de domaine", b"Domain network"),
        1 => t(b"R\xE9seau priv\xE9", b"Private network"),
        _ => t(b"R\xE9seau public", b"Public network"),
    }
}

fn profile_short(i: usize) -> &'static [u8] {
    match i {
        0 => t(b"Domaine", b"Domain"),
        1 => t(b"Priv\xE9", b"Private"),
        _ => t(b"Public", b"Public"),
    }
}

fn proto_name(p: u8) -> &'static [u8] {
    match p {
        1 => b"TCP",
        2 => b"UDP",
        3 => b"ICMP",
        _ => t(b"Tous", b"Any"),
    }
}

// ---------- Configuration ----------

#[derive(Clone, Copy)]
struct Rule {
    id: u32,
    name: Field<64>,
    inbound: bool,
    allow: bool,
    proto: u8,
    ports: Field<48>,
    program: Field<200>,
    profiles: u8,
    enabled: bool,
}

const EMPTY_RULE: Rule = Rule {
    id: 0,
    name: Field::new(),
    inbound: true,
    allow: true,
    proto: 0,
    ports: Field::new(),
    program: Field::new(),
    profiles: 7,
    enabled: true,
};

static mut RULES: [Rule; MAX_RULES] = [EMPTY_RULE; MAX_RULES];
static mut RULE_COUNT: usize = 0;
static mut ENABLED: [bool; 3] = [true; 3];
static mut INBOUND_BLOCK: [bool; 3] = [true; 3];
static mut OUTBOUND_BLOCK: [bool; 3] = [false; 3];
static mut PROFILE: usize = 1;
static mut NOTIFY: bool = true;

static mut BUF: [u8; 24576] = [0; 24576];
static mut STATUS: [u8; 1024] = [0; 1024];
static mut STATUS_LEN: usize = 0;
static mut LOG: [u8; 32768] = [0; 32768];
static mut LOG_LEN: usize = 0;
static mut PROMPT: [u8; 1024] = [0; 1024];
static mut PROMPT_LEN: usize = 0;
static mut ANSWERED: Field<200> = Field::new();
static mut LAST_PROMPT: Field<200> = Field::new();
static mut LAST_POLL: u32 = 0;

static mut PAGE: usize = 0;
static mut SELECTED: usize = usize::MAX;
static mut SCROLL: usize = 0;
static mut LOG_SCROLL: usize = 0;
static mut LAST_REFRESH: u32 = 0;
static mut FORCE: bool = true;
static mut HITS: Hits = Hits::new();
static mut NOTICE: &[u8] = b"";
static mut VISIBLE: [usize; MAX_RULES] = [0; MAX_RULES];
static mut VISIBLE_COUNT: usize = 0;

fn profile_index(s: &[u8]) -> Option<usize> {
    PROFILES.iter().position(|p| *p == s)
}

unsafe fn load() {
    let n = read_file(c"Z:\\etc\\winlux\\firewall.conf".as_ptr(), &mut BUF, false);
    ENABLED = [true; 3];
    INBOUND_BLOCK = [true; 3];
    OUTBOUND_BLOCK = [false; 3];
    PROFILE = 1;
    NOTIFY = true;
    RULE_COUNT = 0;
    for line in BUF[..n].split(|&c| c == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let Some(eq) = line.iter().position(|&c| c == b'=') else {
            continue;
        };
        let (key, value) = (&line[..eq], &line[eq + 1..]);
        let suffix = |prefix: &[u8]| key.strip_prefix(prefix).and_then(profile_index);
        if key == b"profile" {
            PROFILE = profile_index(value).unwrap_or(1);
        } else if key == b"notify" {
            NOTIFY = value != b"0";
        } else if let Some(i) = suffix(b"enabled.") {
            ENABLED[i] = value != b"0";
        } else if let Some(i) = suffix(b"inbound.") {
            INBOUND_BLOCK[i] = value == b"block";
        } else if let Some(i) = suffix(b"outbound.") {
            OUTBOUND_BLOCK[i] = value == b"block";
        } else if key == b"rule" && RULE_COUNT < MAX_RULES {
            let mut r = EMPTY_RULE;
            r.id = parse_num(column(value, 0)).unwrap_or(0);
            r.name.set(column(value, 1));
            r.inbound = column(value, 2) != b"out";
            r.allow = column(value, 3) != b"block";
            r.proto = match column(value, 4) {
                b"tcp" => 1,
                b"udp" => 2,
                b"icmp" => 3,
                _ => 0,
            };
            r.ports.set(column(value, 5));
            r.program.set(column(value, 6));
            r.profiles = 0;
            for p in column(value, 7).split(|&c| c == b',') {
                if p == b"any" {
                    r.profiles = 7;
                } else if let Some(i) = profile_index(p) {
                    r.profiles |= 1 << i;
                }
            }
            if r.profiles == 0 {
                r.profiles = 7;
            }
            r.enabled = column(value, 8) != b"0";
            RULES[RULE_COUNT] = r;
            RULE_COUNT += 1;
        }
    }
}

unsafe fn save() -> bool {
    let mut out = [0u8; 24576];
    let mut o = Out {
        buf: &mut out,
        len: 0,
    };
    o.raw(b"profile=");
    o.raw(PROFILES[PROFILE]);
    o.raw(if NOTIFY {
        b"\nnotify=1\n"
    } else {
        b"\nnotify=0\n"
    });
    for i in 0..3 {
        for (key, value) in [
            (
                b"enabled." as &[u8],
                if ENABLED[i] { b"1" as &[u8] } else { b"0" },
            ),
            (
                b"inbound.",
                if INBOUND_BLOCK[i] { b"block" } else { b"allow" },
            ),
            (
                b"outbound.",
                if OUTBOUND_BLOCK[i] {
                    b"block"
                } else {
                    b"allow"
                },
            ),
        ] {
            o.raw(key);
            o.raw(PROFILES[i]);
            o.raw(b"=");
            o.raw(value);
            o.raw(b"\n");
        }
    }
    for k in 0..RULE_COUNT {
        let r = &RULES[k];
        let mut id = Field::<12>::new();
        push_num(&mut id, r.id as i64);
        o.raw(b"rule=");
        o.raw(id.get());
        o.tab();
        o.clean(r.name.get());
        o.tab();
        o.raw(if r.inbound { b"in" } else { b"out" });
        o.tab();
        o.raw(if r.allow { b"allow" } else { b"block" });
        o.tab();
        o.raw(match r.proto {
            1 => b"tcp",
            2 => b"udp",
            3 => b"icmp",
            _ => b"any",
        });
        o.tab();
        o.clean(r.ports.get());
        o.tab();
        o.clean(r.program.get());
        o.tab();
        let mut first = true;
        for i in 0..3 {
            if r.profiles & (1 << i) != 0 {
                if !first {
                    o.raw(b",");
                }
                first = false;
                o.raw(PROFILES[i]);
            }
        }
        o.tab();
        o.raw(if r.enabled { b"1\n" } else { b"0\n" });
    }
    let len = o.len;
    CreateDirectoryA(c"Z:\\etc\\winlux".as_ptr(), core::ptr::null_mut());
    let ok = write_file(&Path::new(b"Z:\\etc\\winlux\\firewall.conf"), &out[..len]);
    NOTICE = if !ok {
        t(
            b"Enregistrement impossible : droits insuffisants sur /etc/winlux.",
            b"Cannot save: no write access to /etc/winlux.",
        )
    } else if winlux(b"--firewall apply") {
        t(b"Param\xE8tres appliqu\xE9s.", b"Settings applied.")
    } else {
        t(
            b"Enregistr\xE9, mais le service Winlux est introuvable.",
            b"Saved, but the Winlux service was not found.",
        )
    };
    FORCE = true;
    LAST_REFRESH = GetTickCount().wrapping_sub(1500);
    ok
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
    fn tab(&mut self) {
        self.raw(b"\t");
    }
    /// Valeur sans tabulation ni saut de ligne (séparateurs du format).
    fn clean(&mut self, s: &[u8]) {
        for &c in s {
            self.raw(&[if matches!(c, b'\t' | b'\n' | b'\r') {
                b' '
            } else {
                c
            }]);
        }
    }
}

unsafe fn refresh() {
    LAST_REFRESH = GetTickCount();
    FORCE = false;
    load();
    STATUS_LEN = read_file(
        c"Z:\\run\\winlux\\firewall\\status.txt".as_ptr(),
        &mut STATUS,
        false,
    );
    LOG_LEN = read_file(
        c"Z:\\var\\lib\\winlux\\firewall\\blocked.log".as_ptr(),
        &mut LOG,
        true,
    );
    read_prompt();
}

unsafe fn read_prompt() {
    PROMPT_LEN = read_file(
        c"Z:\\run\\winlux\\firewall\\prompt.txt".as_ptr(),
        &mut PROMPT,
        false,
    );
}

unsafe fn prompt_program() -> &'static [u8] {
    field(&PROMPT[..PROMPT_LEN], b"program")
}

unsafe fn prompt_pending() -> bool {
    let p = prompt_program();
    !p.is_empty() && p != ANSWERED.get()
}

/// Surveillance globale (appelée par le bureau) : à la première écoute
/// d'une application, notification et ouverture de l'alerte.
pub unsafe fn system_tick() -> bool {
    let now = GetTickCount();
    if now.wrapping_sub(LAST_POLL) < 1500 {
        return false;
    }
    LAST_POLL = now;
    read_prompt();
    let program = prompt_program();
    if program.is_empty() || program == LAST_PROMPT.get() || program == ANSWERED.get() {
        return false;
    }
    LAST_PROMPT.set(program);
    let mut msg = Field::<200>::new();
    msg.push(field(&PROMPT[..PROMPT_LEN], b"name"));
    msg.push(t(
        b" souhaite recevoir des connexions entrantes.",
        b" wants to accept incoming connections.",
    ));
    crate::notifications::push(t(b"Pare-feu Winlux", b"Winlux Firewall"), msg.get());
    crate::wm::open(crate::wm::APP_FIREWALL);
    true
}

// ---------- Éditeur de règle ----------

static mut EDITING: bool = false;
static mut EDIT: usize = usize::MAX;
static mut DRAFT: Rule = EMPTY_RULE;
static mut FOCUS: u16 = 0;
static mut ERROR: &[u8] = b"";

unsafe fn open_editor(index: usize) {
    EDITING = true;
    EDIT = index;
    ERROR = b"";
    FOCUS = 0;
    if index < RULE_COUNT {
        DRAFT = RULES[index];
    } else {
        DRAFT = EMPTY_RULE;
        DRAFT.inbound = PAGE != 2;
        DRAFT.allow = DRAFT.inbound;
        DRAFT.proto = 1;
    }
}

fn valid_ports(s: &[u8]) -> bool {
    s.split(|&c| c == b',').all(|part| {
        let part = trim(part);
        if part.is_empty() {
            return true;
        }
        let mut it = part.split(|&c| c == b'-');
        let a = it.next().map(trim).and_then(parse_num);
        let b = it.next().map(trim).map(parse_num);
        if it.next().is_some() {
            return false;
        }
        match (a, b) {
            (Some(a), None) => (1..=65535).contains(&a),
            (Some(a), Some(Some(b))) => a >= 1 && b <= 65535 && a <= b,
            _ => false,
        }
    })
}

unsafe fn save_editor() {
    if trim(DRAFT.name.get()).is_empty() {
        ERROR = t(
            b"Indiquez un nom pour la r\xE8gle.",
            b"Enter a name for the rule.",
        );
        return;
    }
    if DRAFT.proto == 3 {
        DRAFT.ports.len = 0;
    }
    if !valid_ports(DRAFT.ports.get()) {
        ERROR = t(
            b"Ports invalides : exemple 80, 443 ou 8000-8100.",
            b"Invalid ports: for example 80, 443 or 8000-8100.",
        );
        return;
    }
    if DRAFT.profiles == 0 {
        ERROR = t(
            b"Choisissez au moins un profil.",
            b"Choose at least one profile.",
        );
        return;
    }
    if EDIT < RULE_COUNT {
        RULES[EDIT] = DRAFT;
    } else if RULE_COUNT < MAX_RULES {
        DRAFT.id = (0..RULE_COUNT).map(|i| RULES[i].id).max().unwrap_or(0) + 1;
        RULES[RULE_COUNT] = DRAFT;
        RULE_COUNT += 1;
    } else {
        ERROR = t(
            b"Nombre maximal de r\xE8gles atteint.",
            b"Maximum number of rules reached.",
        );
        return;
    }
    if save() {
        EDITING = false;
    } else {
        ERROR = NOTICE;
    }
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
    glyph(g.hdc, g.b, g.f, Glyph::Shield, c.left + 16, c.top + 16, 24);
    text_fit(
        g.hdc,
        g.f.semibold,
        13,
        p.text,
        c.left + 48,
        c.top + 20,
        t(b"Pare-feu Winlux", b"Winlux Firewall"),
        NAV_W - 64,
    );
    let nav: [(&[u8], Glyph); 4] = [
        (t(b"Vue d'ensemble", b"Overview"), Glyph::Home),
        (
            t(b"R\xE8gles de trafic entrant", b"Inbound rules"),
            Glyph::Download,
        ),
        (
            t(b"R\xE8gles de trafic sortant", b"Outbound rules"),
            Glyph::Up,
        ),
        (
            t(b"Connexions bloqu\xE9es", b"Blocked connections"),
            Glyph::List,
        ),
    ];
    for (i, (label, gl)) in nav.iter().enumerate() {
        nav_item(
            g,
            h,
            c.left,
            c.top + 64 + i as i32 * 40,
            NAV_W,
            ID_NAV + i as u16,
            label,
            *gl,
            PAGE == i,
        );
    }
    let area = rect(c.left + NAV_W + 1, c.top, c.right, c.bottom);
    match PAGE {
        0 => draw_overview(g, h, &area),
        1 | 2 => draw_rules(g, h, &area),
        _ => draw_log(g, h, &area),
    }
    if EDITING {
        draw_editor(g, h, c);
    } else if prompt_pending() {
        draw_prompt(g, h, c);
    }
}

unsafe fn draw_overview(g: &mut Gfx, h: &mut Hits, a: &Rect) {
    let p = pal();
    let x = a.left + 24;
    let r = a.right - 24;
    text(
        g.hdc,
        g.f.large,
        p.text,
        x,
        a.top + 18,
        t(
            b"Pare-feu et protection du r\xE9seau",
            b"Firewall & network protection",
        ),
    );
    // Bandeau d'état.
    let st = &STATUS[..STATUS_LEN];
    let backend_absent = field(st, b"backend") == b"absent";
    let error = field(st, b"error");
    let (ok, msg): (bool, &[u8]) = if backend_absent {
        (
            false,
            t(
                b"nftables est absent : les r\xE8gles ne peuvent pas \xEAtre appliqu\xE9es.",
                b"nftables is missing: rules cannot be applied.",
            ),
        )
    } else if !error.is_empty() {
        (false, error)
    } else if !ENABLED[PROFILE] {
        (
            false,
            t(
                b"Le pare-feu est d\xE9sactiv\xE9 pour le r\xE9seau actuel.",
                b"The firewall is off for the current network.",
            ),
        )
    } else if STATUS_LEN == 0 {
        (
            false,
            t(
                b"Le service de pare-feu n'a pas encore appliqu\xE9 la configuration.",
                b"The firewall service has not applied the configuration yet.",
            ),
        )
    } else {
        (
            true,
            t(
                b"Le pare-feu prot\xE8ge cet ordinateur.",
                b"The firewall is protecting this device.",
            ),
        )
    };
    let by = a.top + 60;
    panel(g.hdc, g.b, x, by, r, by + 44, 6, p.surface, p.border);
    let dot = if ok {
        rgb(0x0F, 0x9D, 0x58)
    } else {
        rgb(0xE8, 0x11, 0x23)
    };
    ellipse(g.hdc, g.b, x + 14, by + 15, x + 28, by + 29, dot);
    text_fit(
        g.hdc,
        g.f.body,
        13,
        p.text,
        x + 40,
        by + 14,
        msg,
        r - x - 56,
    );
    let mut y = by + 60;
    for i in 0..3 {
        let card = rect(x, y, r, y + 112);
        panel(
            g.hdc,
            g.b,
            card.left,
            card.top,
            card.right,
            card.bottom,
            6,
            p.surface,
            p.border,
        );
        let mut title = Field::<80>::new();
        title.push(profile_name(i));
        if i == PROFILE {
            title.push(t(b" (actif)", b" (active)"));
        }
        text(g.hdc, g.f.semibold, p.text, x + 16, y + 14, title.get());
        if i != PROFILE {
            let s = t(
                b"Utiliser pour le r\xE9seau actuel",
                b"Use for the current network",
            );
            let w = button_w(s);
            button(
                g,
                h,
                rect(r - 16 - w, y + 8, r - 16, y + 38),
                ID_PROFILE_SET + i as u16,
                s,
                false,
                true,
            );
        }
        toggle(
            g,
            h,
            x + 16,
            y + 44,
            ID_PROFILE_ON + i as u16,
            ENABLED[i],
            t(b"Pare-feu Winlux", b"Winlux Firewall"),
        );
        label(g, x + 16, y + 80, t(b"Entrant :", b"Inbound:"));
        let mut cx = x + 90;
        cx += chip(
            g,
            h,
            cx,
            y + 74,
            ID_INBOUND + i as u16 * 2,
            t(b"Bloquer", b"Block"),
            INBOUND_BLOCK[i],
        );
        chip(
            g,
            h,
            cx,
            y + 74,
            ID_INBOUND + i as u16 * 2 + 1,
            t(b"Autoriser", b"Allow"),
            !INBOUND_BLOCK[i],
        );
        let ox = x + (r - x) / 2;
        label(g, ox, y + 80, t(b"Sortant :", b"Outbound:"));
        let mut cx = ox + 80;
        cx += chip(
            g,
            h,
            cx,
            y + 74,
            ID_OUTBOUND + i as u16 * 2,
            t(b"Bloquer", b"Block"),
            OUTBOUND_BLOCK[i],
        );
        chip(
            g,
            h,
            cx,
            y + 74,
            ID_OUTBOUND + i as u16 * 2 + 1,
            t(b"Autoriser", b"Allow"),
            !OUTBOUND_BLOCK[i],
        );
        y += 124;
    }
    toggle(
        g,
        h,
        x,
        y + 4,
        ID_NOTIFY,
        NOTIFY,
        t(
            b"M'avertir lorsqu'une application commence \xE0 recevoir des connexions",
            b"Notify me when an app starts accepting incoming connections",
        ),
    );
    if !NOTICE.is_empty() {
        text(g.hdc, g.f.caption, p.text3, x, a.bottom - 20, NOTICE);
    }
}

unsafe fn draw_rules(g: &mut Gfx, h: &mut Hits, a: &Rect) {
    let p = pal();
    let inbound = PAGE == 1;
    let x = a.left + 24;
    text(
        g.hdc,
        g.f.large,
        p.text,
        x,
        a.top + 18,
        if inbound {
            t(b"R\xE8gles de trafic entrant", b"Inbound rules")
        } else {
            t(b"R\xE8gles de trafic sortant", b"Outbound rules")
        },
    );
    VISIBLE_COUNT = 0;
    for i in 0..RULE_COUNT {
        if RULES[i].inbound == inbound {
            VISIBLE[VISIBLE_COUNT] = i;
            VISIBLE_COUNT += 1;
        }
    }
    let has = SELECTED < RULE_COUNT && RULES[SELECTED].inbound == inbound;
    let enabled = has && RULES[SELECTED].enabled;
    let buttons: [(u16, &[u8], bool, bool); 4] = [
        (ID_NEW, t(b"Nouvelle r\xE8gle", b"New rule"), true, true),
        (ID_EDIT, t(b"Propri\xE9t\xE9s", b"Properties"), false, has),
        (
            ID_TOGGLE,
            if enabled {
                t(b"D\xE9sactiver la r\xE8gle", b"Disable rule")
            } else {
                t(b"Activer la r\xE8gle", b"Enable rule")
            },
            false,
            has,
        ),
        (ID_DELETE, t(b"Supprimer", b"Delete"), false, has),
    ];
    let mut bx = x;
    for (id, label, primary, on) in buttons {
        let w = button_w(label);
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
    let cols: [(&[u8], i32); 7] = [
        (t(b"Nom", b"Name"), width * 22 / 100),
        (t(b"Profil", b"Profile"), width * 14 / 100),
        (t(b"Activ\xE9e", b"Enabled"), width * 8 / 100),
        (t(b"Action", b"Action"), width * 10 / 100),
        (t(b"Protocole", b"Protocol"), width * 9 / 100),
        (t(b"Ports", b"Ports"), width * 11 / 100),
        (t(b"Programme", b"Program"), width * 26 / 100),
    ];
    let mut y = header(g, x, a.top + 108, a.right - 16, &cols);
    if VISIBLE_COUNT == 0 {
        text(
            g.hdc,
            g.f.body,
            p.text2,
            x,
            y + 12,
            t(
                b"Aucune r\xE8gle. Le comportement par d\xE9faut du profil s'applique.",
                b"No rules. The profile's default behavior applies.",
            ),
        );
    }
    let visible = ((a.bottom - 36 - y) / ROW_H).max(1) as usize;
    if SCROLL + visible > VISIBLE_COUNT {
        SCROLL = VISIBLE_COUNT.saturating_sub(visible);
    }
    for k in SCROLL..VISIBLE_COUNT.min(SCROLL + visible) {
        let i = VISIBLE[k];
        let rule = &RULES[i];
        let row = rect(x - 8, y, a.right - 16, y + ROW_H - 2);
        let hover = h.add(row, ID_ROW + i as u16);
        if i == SELECTED {
            round_rect(
                g.hdc, g.b, row.left, row.top, row.right, row.bottom, 4, p.selected,
            );
        } else if hover {
            round_rect(
                g.hdc, g.b, row.left, row.top, row.right, row.bottom, 4, p.hover,
            );
        }
        let ty = y + 9;
        let color = if rule.enabled { p.text } else { p.text3 };
        let mut cx = x;
        cell(g, cx, ty, cols[0].1, rule.name.get(), color);
        cx += cols[0].1;
        let mut prof = Field::<48>::new();
        if rule.profiles == 7 {
            prof.push(t(b"Tous", b"All"));
        } else {
            for pi in 0..3 {
                if rule.profiles & (1 << pi) != 0 {
                    if prof.len > 0 {
                        prof.push(b", ");
                    }
                    prof.push(profile_short(pi));
                }
            }
        }
        cell(g, cx, ty, cols[1].1, prof.get(), color);
        cx += cols[1].1;
        cell(
            g,
            cx,
            ty,
            cols[2].1,
            if rule.enabled {
                t(b"Oui", b"Yes")
            } else {
                t(b"Non", b"No")
            },
            color,
        );
        cx += cols[2].1;
        let action_color = if !rule.enabled {
            p.text3
        } else if rule.allow {
            rgb(0x0F, 0x9D, 0x58)
        } else {
            rgb(0xE8, 0x11, 0x23)
        };
        cell(
            g,
            cx,
            ty,
            cols[3].1,
            if rule.allow {
                t(b"Autoriser", b"Allow")
            } else {
                t(b"Bloquer", b"Block")
            },
            action_color,
        );
        cx += cols[3].1;
        cell(g, cx, ty, cols[4].1, proto_name(rule.proto), color);
        cx += cols[4].1;
        cell(
            g,
            cx,
            ty,
            cols[5].1,
            if rule.ports.len > 0 {
                rule.ports.get()
            } else {
                t(b"Tous", b"Any")
            },
            color,
        );
        cx += cols[5].1;
        let prog: &[u8] = if rule.program.len > 0 {
            rule.program.get()
        } else {
            t(b"Tous les programmes", b"All programs")
        };
        cell(g, cx, ty, a.right - 16 - cx, prog, color);
        y += ROW_H;
    }
    if !NOTICE.is_empty() {
        text(g.hdc, g.f.caption, p.text3, x, a.bottom - 20, NOTICE);
    }
}

unsafe fn draw_log(g: &mut Gfx, h: &mut Hits, a: &Rect) {
    let p = pal();
    let x = a.left + 24;
    text(
        g.hdc,
        g.f.large,
        p.text,
        x,
        a.top + 18,
        t(b"Connexions bloqu\xE9es", b"Blocked connections"),
    );
    let s1 = t(b"Actualiser", b"Refresh");
    let s2 = t(b"Effacer le journal", b"Clear log");
    let w1 = button_w(s1);
    button(
        g,
        h,
        rect(x, a.top + 60, x + w1, a.top + 92),
        ID_REFRESH,
        s1,
        false,
        true,
    );
    button(
        g,
        h,
        rect(
            x + w1 + 8,
            a.top + 60,
            x + w1 + 8 + button_w(s2),
            a.top + 92,
        ),
        ID_CLEAR_LOG,
        s2,
        false,
        LOG_LEN > 0,
    );
    let width = a.right - x - 16;
    let cols: [(&[u8], i32); 6] = [
        (t(b"Date et heure", b"Date and time"), 150),
        (t(b"Sens", b"Direction"), width * 10 / 100),
        (t(b"Protocole", b"Protocol"), width * 10 / 100),
        (t(b"Source", b"Source"), width * 25 / 100),
        (t(b"Destination", b"Destination"), width * 25 / 100),
        (t(b"Port", b"Port"), 80),
    ];
    let mut y = header(g, x, a.top + 108, a.right - 16, &cols);
    let data = &LOG[..LOG_LEN];
    let mut lines: [&[u8]; 400] = [b""; 400];
    let mut n = 0;
    for (k, line) in data.split(|&c| c == b'\n').enumerate() {
        if (k == 0 && LOG_LEN == LOG.len()) || line.is_empty() {
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
                b"Aucune connexion bloqu\xE9e n'a \xE9t\xE9 enregistr\xE9e.",
                b"No blocked connections have been logged.",
            ),
        );
    }
    let visible = ((a.bottom - 24 - y) / 30).max(1) as usize;
    if LOG_SCROLL + visible > n {
        LOG_SCROLL = n.saturating_sub(visible);
    }
    for k in LOG_SCROLL..n.min(LOG_SCROLL + visible) {
        let line = lines[n - 1 - k];
        let mut cx = x;
        cell(g, cx, y + 6, cols[0].1, column(line, 1), p.text2);
        cx += cols[0].1;
        let dir: &[u8] = if column(line, 2) == b"in" {
            t(b"Entrant", b"Inbound")
        } else {
            t(b"Sortant", b"Outbound")
        };
        cell(g, cx, y + 6, cols[1].1, dir, p.text);
        cx += cols[1].1;
        cell(g, cx, y + 6, cols[2].1, column(line, 3), p.text);
        cx += cols[2].1;
        let mut src = Field::<64>::new();
        src.push(column(line, 4));
        if !column(line, 5).is_empty() {
            src.push(b" : ");
            src.push(column(line, 5));
        }
        cell(g, cx, y + 6, cols[3].1, src.get(), p.text2);
        cx += cols[3].1;
        cell(g, cx, y + 6, cols[4].1, column(line, 6), p.text2);
        cx += cols[4].1;
        cell(g, cx, y + 6, a.right - 16 - cx, column(line, 7), p.text);
        y += 30;
    }
}

unsafe fn draw_editor(g: &mut Gfx, h: &mut Hits, c: &Rect) {
    let p = pal();
    let title = if EDIT < RULE_COUNT {
        t(b"Propri\xE9t\xE9s de la r\xE8gle", b"Rule properties")
    } else if DRAFT.inbound {
        t(b"Nouvelle r\xE8gle de trafic entrant", b"New inbound rule")
    } else {
        t(b"Nouvelle r\xE8gle de trafic sortant", b"New outbound rule")
    };
    let d = dialog(g, h, c, 640, 520, title);
    let x = d.left + 24;
    let r = d.right - 24;
    let mut y = d.top + 64;
    label(g, x, y, t(b"Nom", b"Name"));
    input(
        g,
        h,
        rect(x, y + 18, r, y + 50),
        ID_FIELD,
        DRAFT.name.get(),
        FOCUS == 0,
        b"",
    );
    y += 64;
    text(g.hdc, g.f.semibold, p.text, x, y, t(b"Action", b"Action"));
    let mut cx = x + 120;
    cx += chip(
        g,
        h,
        cx,
        y - 6,
        ID_ACTION,
        t(b"Autoriser la connexion", b"Allow the connection"),
        DRAFT.allow,
    );
    chip(
        g,
        h,
        cx,
        y - 6,
        ID_ACTION + 1,
        t(b"Bloquer la connexion", b"Block the connection"),
        !DRAFT.allow,
    );
    y += 40;
    text(
        g.hdc,
        g.f.semibold,
        p.text,
        x,
        y,
        t(b"Protocole", b"Protocol"),
    );
    let mut cx = x + 120;
    for k in 0..4u8 {
        cx += chip(
            g,
            h,
            cx,
            y - 6,
            ID_PROTO + k as u16,
            proto_name(k),
            DRAFT.proto == k,
        );
    }
    y += 40;
    if DRAFT.proto != 3 {
        label(
            g,
            x,
            y,
            if DRAFT.inbound {
                t(
                    b"Ports locaux (vide = tous ; ex. 80, 443, 8000-8100)",
                    b"Local ports (empty = all; e.g. 80, 443, 8000-8100)",
                )
            } else {
                t(
                    b"Ports distants (vide = tous ; ex. 80, 443)",
                    b"Remote ports (empty = all; e.g. 80, 443)",
                )
            },
        );
        input(
            g,
            h,
            rect(x, y + 18, r, y + 50),
            ID_FIELD + 1,
            DRAFT.ports.get(),
            FOCUS == 1,
            t(b"Tous les ports", b"All ports"),
        );
        y += 64;
    }
    label(
        g,
        x,
        y,
        t(
            b"Programme (chemin du .exe ou de l'ex\xE9cutable Linux ; vide = tous)",
            b"Program (path of the .exe or Linux executable; empty = all)",
        ),
    );
    input(
        g,
        h,
        rect(x, y + 18, r, y + 50),
        ID_FIELD + 2,
        DRAFT.program.get(),
        FOCUS == 2,
        t(b"Tous les programmes", b"All programs"),
    );
    y += 66;
    text(
        g.hdc,
        g.f.semibold,
        p.text,
        x,
        y,
        t(b"Profils", b"Profiles"),
    );
    let mut cx = x + 120;
    for i in 0..3 {
        cx += chip(
            g,
            h,
            cx,
            y - 6,
            ID_PROF + i as u16,
            profile_short(i),
            DRAFT.profiles & (1 << i) != 0,
        );
    }
    y += 40;
    toggle(
        g,
        h,
        x,
        y,
        ID_ENABLED,
        DRAFT.enabled,
        t(b"R\xE8gle activ\xE9e", b"Rule enabled"),
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

unsafe fn draw_prompt(g: &mut Gfx, h: &mut Hits, c: &Rect) {
    let p = pal();
    let data = &PROMPT[..PROMPT_LEN];
    let d = dialog(
        g,
        h,
        c,
        600,
        330,
        t(b"Alerte de s\xE9curit\xE9 Winlux", b"Winlux Security Alert"),
    );
    let x = d.left + 24;
    let r = d.right - 24;
    let mut y = d.top + 62;
    text_fit(
        g.hdc,
        g.f.semibold,
        13,
        p.text,
        x,
        y,
        t(
            b"Le Pare-feu Winlux a bloqu\xE9 certaines fonctionnalit\xE9s de cette application.",
            b"Winlux Firewall has blocked some features of this app.",
        ),
        r - x,
    );
    y += 34;
    for (label_text, key) in [
        (t(b"Nom :", b"Name:"), b"name" as &[u8]),
        (t(b"Chemin :", b"Path:"), b"program"),
        (t(b"Ports :", b"Ports:"), b"ports"),
    ] {
        label(g, x, y, label_text);
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            x + 90,
            y,
            field(data, key),
            r - x - 90,
        );
        y += 26;
    }
    y += 8;
    let profile = profile_index(field(data, b"profile")).unwrap_or(PROFILE);
    text_fit(
        g.hdc,
        g.f.body,
        13,
        p.text2,
        x,
        y,
        t(
            b"Autoriser cette application \xE0 recevoir des connexions",
            b"Allow this app to accept incoming connections",
        ),
        r - x,
    );
    let mut q = Field::<120>::new();
    q.push(t(b"sur les r\xE9seaux de type \xAB ", b"on \""));
    q.push(profile_short(profile));
    q.push(t(b" \xBB ?", b"\" networks?"));
    text_fit(g.hdc, g.f.body, 13, p.text2, x, y + 22, q.get(), r - x);
    let block = t(b"Bloquer", b"Block");
    let allow = t(b"Autoriser l'acc\xE8s", b"Allow access");
    let bw = button_w(block).max(100);
    let aw = button_w(allow);
    button(
        g,
        h,
        rect(r - bw, d.bottom - 52, r, d.bottom - 20),
        ID_BLOCK,
        block,
        false,
        true,
    );
    button(
        g,
        h,
        rect(r - bw - 8 - aw, d.bottom - 52, r - bw - 8, d.bottom - 20),
        ID_ALLOW,
        allow,
        true,
        true,
    );
}

// ---------- Entrées ----------

unsafe fn answer(allow: bool) {
    ANSWERED.set(prompt_program());
    NOTICE = if winlux(if allow {
        b"--firewall answer allow"
    } else {
        b"--firewall answer block"
    }) {
        if allow {
            t(
                b"L'application peut recevoir des connexions.",
                b"The app can now accept connections.",
            )
        } else {
            t(
                b"L'application reste bloqu\xE9e.",
                b"The app stays blocked.",
            )
        }
    } else {
        t(
            b"Le service Winlux est introuvable.",
            b"The Winlux service was not found.",
        )
    };
    FORCE = true;
    LAST_REFRESH = GetTickCount().wrapping_sub(1500);
}

unsafe fn click(id: u16) -> bool {
    if EDITING {
        match id {
            ID_SAVE => save_editor(),
            ID_CANCEL => EDITING = false,
            ID_ENABLED => DRAFT.enabled = !DRAFT.enabled,
            i if (ID_FIELD..ID_FIELD + 3).contains(&i) => FOCUS = i - ID_FIELD,
            i if (ID_ACTION..ID_ACTION + 2).contains(&i) => DRAFT.allow = i == ID_ACTION,
            i if (ID_PROTO..ID_PROTO + 4).contains(&i) => DRAFT.proto = (i - ID_PROTO) as u8,
            i if (ID_PROF..ID_PROF + 3).contains(&i) => DRAFT.profiles ^= 1 << (i - ID_PROF),
            _ => return false,
        }
        return true;
    }
    if prompt_pending() {
        match id {
            ID_ALLOW => answer(true),
            ID_BLOCK => answer(false),
            _ => return false,
        }
        return true;
    }
    match id {
        i if (ID_NAV..ID_NAV + 4).contains(&i) => {
            PAGE = (i - ID_NAV) as usize;
            SCROLL = 0;
            SELECTED = usize::MAX;
            FORCE = true;
        }
        i if (ID_PROFILE_ON..ID_PROFILE_ON + 3).contains(&i) => {
            let k = (i - ID_PROFILE_ON) as usize;
            ENABLED[k] = !ENABLED[k];
            save();
        }
        i if (ID_PROFILE_SET..ID_PROFILE_SET + 3).contains(&i) => {
            PROFILE = (i - ID_PROFILE_SET) as usize;
            save();
        }
        i if (ID_INBOUND..ID_INBOUND + 6).contains(&i) => {
            INBOUND_BLOCK[((i - ID_INBOUND) / 2) as usize] = (i - ID_INBOUND) % 2 == 0;
            save();
        }
        i if (ID_OUTBOUND..ID_OUTBOUND + 6).contains(&i) => {
            OUTBOUND_BLOCK[((i - ID_OUTBOUND) / 2) as usize] = (i - ID_OUTBOUND) % 2 == 0;
            save();
        }
        ID_NOTIFY => {
            NOTIFY = !NOTIFY;
            save();
        }
        ID_NEW => open_editor(usize::MAX),
        ID_EDIT if SELECTED < RULE_COUNT => open_editor(SELECTED),
        ID_TOGGLE if SELECTED < RULE_COUNT => {
            RULES[SELECTED].enabled = !RULES[SELECTED].enabled;
            save();
        }
        ID_DELETE if SELECTED < RULE_COUNT => delete_selected(),
        ID_CLEAR_LOG => {
            winlux(b"--firewall clear-log");
            LOG_LEN = 0;
            LOG_SCROLL = 0;
        }
        ID_REFRESH => FORCE = true,
        i if i >= ID_ROW && ((i - ID_ROW) as usize) < RULE_COUNT => {
            SELECTED = (i - ID_ROW) as usize
        }
        _ => return false,
    }
    true
}

unsafe fn delete_selected() {
    if SELECTED >= RULE_COUNT {
        return;
    }
    for i in SELECTED..RULE_COUNT - 1 {
        RULES[i] = RULES[i + 1];
    }
    RULE_COUNT -= 1;
    SELECTED = usize::MAX;
    save();
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
            Some(i) if !EDITING && i >= ID_ROW && ((i - ID_ROW) as usize) < RULE_COUNT => {
                SELECTED = (i - ID_ROW) as usize;
                open_editor(SELECTED);
                true
            }
            _ => false,
        },
        Mouse::Wheel(delta) if !EDITING => {
            let s = if PAGE == 3 {
                &mut LOG_SCROLL
            } else {
                &mut SCROLL
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
                FOCUS = match (FOCUS, DRAFT.proto == 3) {
                    (0, false) => 1,
                    (0, true) | (1, _) => 2,
                    _ => 0,
                }
            }
            0 => return false,
            _ => {
                return match FOCUS {
                    0 => DRAFT.name.key(ch),
                    1 => DRAFT.ports.key(ch),
                    _ => DRAFT.program.key(ch),
                }
            }
        }
        return true;
    }
    match vk {
        0x2E if SELECTED < RULE_COUNT => delete_selected(),
        0x74 => FORCE = true,
        0x0D if SELECTED < RULE_COUNT => open_editor(SELECTED),
        _ => return false,
    }
    true
}

pub unsafe fn tick(_f: &Frame) -> bool {
    !EDITING && GetTickCount().wrapping_sub(LAST_REFRESH) > 2000
}
