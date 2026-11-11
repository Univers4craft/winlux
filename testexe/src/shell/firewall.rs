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

const NAV_W: i32 = 260;
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
static mut PORTS: [u8; 16384] = [0; 16384];
static mut PORTS_LEN: usize = 0;
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
    PORTS_LEN = read_file(
        c"Z:\\run\\winlux\\firewall\\ports.txt".as_ptr(),
        &mut PORTS,
        false,
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

/// Nouvelle règle (`index` hors liste) ou modification d'une règle existante.
/// `app` : règle « application autorisée » (entrante, tout protocole, le
/// champ Programme reçoit le curseur).
unsafe fn open_editor(index: usize, app: bool) {
    EDITING = true;
    EDIT = index;
    ERROR = b"";
    FOCUS = 0;
    if index < RULE_COUNT {
        DRAFT = RULES[index];
    } else {
        DRAFT = EMPTY_RULE;
        DRAFT.inbound = app || RULES_IN;
        DRAFT.allow = DRAFT.inbound;
        DRAFT.proto = if app { 0 } else { 1 };
        if app {
            FOCUS = 2;
        }
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
    // Application sans nom : le nom du programme sert de nom de règle.
    if trim(DRAFT.name.get()).is_empty() && !trim(DRAFT.program.get()).is_empty() {
        let base = file_name(DRAFT.program.get());
        DRAFT.name.set(base);
    }
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

/// Dernier élément d'un chemin Windows ou Linux (`C:\Jeux\jeu.exe` → `jeu.exe`).
fn file_name(path: &[u8]) -> &[u8] {
    let path = trim(path);
    match path.iter().rposition(|&c| c == b'\\' || c == b'/') {
        Some(i) => &path[i + 1..],
        None => path,
    }
}

// ---------- Navigation ----------

/// Pages : vue d'ensemble, applications autorisées, règles avancées,
/// activité réseau, paramètres ; la page 5 détaille un type de réseau.
const PAGE_OVERVIEW: usize = 0;
const PAGE_APPS: usize = 1;
const PAGE_RULES: usize = 2;
const PAGE_ACTIVITY: usize = 3;
const PAGE_SETTINGS: usize = 4;
const PAGE_PROFILE: usize = 5;

const ID_PROFILE_OPEN: u16 = 300;
const ID_BACK: u16 = 310;
const ID_HERO: u16 = 311;
const ID_GO: u16 = 320;
const ID_TAB: u16 = 330;
const ID_NEW_APP: u16 = 340;
const ID_RESET: u16 = 350;
const ID_RESET_OK: u16 = 351;
const ID_RESET_NO: u16 = 352;
const ID_CHECK: u16 = 400;
const ID_RULE_ON: u16 = 600;

/// Règles avancées : trafic entrant (vrai) ou sortant ; Activité : ports
/// ouverts (vrai) ou connexions bloquées.
static mut RULES_IN: bool = true;
static mut ACT_PORTS: bool = false;
static mut DETAIL: usize = 1;
static mut CONFIRM_RESET: bool = false;

unsafe fn go(page: usize) {
    PAGE = page;
    SCROLL = 0;
    LOG_SCROLL = 0;
    SELECTED = usize::MAX;
    CONFIRM_RESET = false;
    FORCE = true;
}

// ---------- Briques de dessin ----------

fn ok_color(bg: u32) -> u32 {
    readable(rgb(0x0F, 0x9D, 0x58), &[bg], 450)
}

fn bad_color(bg: u32) -> u32 {
    readable(rgb(0xE8, 0x11, 0x23), &[bg], 450)
}

fn warn_color(bg: u32) -> u32 {
    readable(rgb(0xC1, 0x7A, 0x00), &[bg], 450)
}

/// Texte coupé aux espaces sur plusieurs lignes ; renvoie la hauteur écrite.
unsafe fn wrap(g: &mut Gfx, font: Handle, color: u32, x: i32, y: i32, w: i32, s: &[u8], max_lines: i32) -> i32 {
    let mut start = 0;
    let mut lines = 0;
    while start < s.len() && lines < max_lines {
        let mut end = s.len();
        if text_width(&s[start..end], 13) > w {
            end = start;
            let mut k = start;
            while k <= s.len() {
                if k == s.len() || s[k] == b' ' {
                    if text_width(&s[start..k], 13) > w {
                        break;
                    }
                    end = k;
                }
                k += 1;
            }
            if end == start {
                end = s.len();
            }
        }
        let last = lines + 1 == max_lines;
        if last && end < s.len() {
            text_fit(g.hdc, font, 13, color, x, y + lines * 19, &s[start..], w);
        } else {
            text(g.hdc, font, color, x, y + lines * 19, &s[start..end]);
        }
        start = end;
        while start < s.len() && s[start] == b' ' {
            start += 1;
        }
        lines += 1;
    }
    lines * 19
}

/// Carte (fond de surface, bord discret, coins de 8 px).
unsafe fn card(g: &mut Gfx, r: &Rect) {
    let p = pal();
    panel(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 8, p.surface, p.border);
}

/// Séparateur entre deux lignes d'une carte.
unsafe fn divider(g: &mut Gfx, x: i32, r: i32, y: i32) {
    fill(g.hdc, g.b, x, y, r, y + 1, mix(pal().surface, pal().border, 1, 2));
}

/// Pastille d'état arrondie (texte coloré sur fond teinté).
unsafe fn pill(g: &mut Gfx, x: i32, y: i32, s: &[u8], color: u32) -> i32 {
    let p = pal();
    let w = text_width(s, 12) + 20;
    let bg = mix(p.surface, color, 2, 10);
    round_rect(g.hdc, g.b, x, y, x + w, y + 22, 11, bg);
    text(g.hdc, g.f.caption, readable(color, &[bg], 450), x + 10, y + 4, s);
    w
}

/// Choix exclusif en segments (Bloquer | Autoriser, Entrant | Sortant…).
unsafe fn segmented(g: &mut Gfx, h: &mut Hits, x: i32, y: i32, ids: &[u16], labels: &[&[u8]], chosen: usize) -> i32 {
    let p = pal();
    // Le segment choisi est en semi-gras : largeur mesurée avec une marge.
    let widths: [i32; 4] = core::array::from_fn(|k| labels.get(k).map_or(0, |s| text_width(s, 13) * 11 / 10 + 32));
    let total: i32 = widths.iter().sum::<i32>() + 4;
    panel(g.hdc, g.b, x, y, x + total, y + 32, 6, p.surface2, p.border);
    let mut cx = x + 2;
    for (k, label_text) in labels.iter().enumerate() {
        let r = rect(cx, y + 2, cx + widths[k], y + 30);
        let hover = h.add(r, ids[k]);
        if k == chosen {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 5, p.accent);
            text_centered(g.hdc, g.f.semibold, 13, p.on_accent, (r.left + r.right) / 2, r.top + 6, label_text);
        } else {
            if hover {
                round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 5, p.hover);
            }
            text_centered(g.hdc, g.f.body, 13, p.text, (r.left + r.right) / 2, r.top + 6, label_text);
        }
        cx += widths[k];
    }
    total
}

/// Interrupteur seul (sans libellé), aligné à droite d'une ligne.
unsafe fn switch(g: &mut Gfx, h: &mut Hits, right: i32, cy: i32, id: u16, on: bool) {
    let p = pal();
    let x = right - 40;
    let y = cy - 10;
    h.add(rect(x - 4, y - 4, right + 4, y + 24), id);
    if on {
        round_rect(g.hdc, g.b, x, y, x + 40, y + 20, 10, p.accent);
        ellipse(g.hdc, g.b, x + 24, y + 4, x + 36, y + 16, p.on_accent);
    } else {
        round_rect(g.hdc, g.b, x, y, x + 40, y + 20, 10, p.text2);
        round_rect(g.hdc, g.b, x + 1, y + 1, x + 39, y + 19, 9, p.surface);
        ellipse(g.hdc, g.b, x + 5, y + 5, x + 15, y + 15, p.text2);
    }
}

/// Case à cocher Windows 11.
unsafe fn checkbox(g: &mut Gfx, h: &mut Hits, x: i32, y: i32, id: u16, on: bool) {
    let p = pal();
    let r = rect(x, y, x + 20, y + 20);
    let hover = h.add(rect(x - 6, y - 6, x + 26, y + 26), id);
    if on {
        round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 2, p.accent);
        line(g.hdc, g.b, x + 5, y + 10, x + 9, y + 14, 2, p.on_accent);
        line(g.hdc, g.b, x + 9, y + 14, x + 15, y + 6, 2, p.on_accent);
    } else {
        panel(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 2, if hover { p.hover } else { p.surface }, p.text2);
    }
}

/// Chevron « › » de fin de ligne cliquable.
unsafe fn chevron(g: &mut Gfx, x: i32, cy: i32) {
    let c = pal().text2;
    line(g.hdc, g.b, x, cy - 5, x + 5, cy, 2, c);
    line(g.hdc, g.b, x + 5, cy, x, cy + 5, 2, c);
}

/// Bouclier d'état : coche (protégé), point d'exclamation (attention) ou
/// croix (désactivé), dessiné dans la couleur de l'état.
unsafe fn status_shield(g: &mut Gfx, x: i32, y: i32, s: i32, color: u32, state: u8) {
    let cx = x + s / 2;
    let top = y + s / 12;
    let mid = y + s * 11 / 20;
    fill(g.hdc, g.b, x + s / 8, top + 3, x + s * 7 / 8, mid, color);
    round_rect(g.hdc, g.b, x + s / 8, top, x + s * 7 / 8, top + 8, 4, color);
    let h = (y + s - mid).max(1);
    for i in 0..h {
        let half = (s * 3 / 8) * (h - i) / h;
        fill(g.hdc, g.b, cx - half, mid + i, cx + half, mid + i + 1, color);
    }
    let fg = text_on(color);
    let w = (s / 12).max(2);
    match state {
        0 => {
            line(g.hdc, g.b, x + s * 3 / 10, y + s / 2, x + s * 9 / 20, y + s * 13 / 20, w, fg);
            line(g.hdc, g.b, x + s * 9 / 20, y + s * 13 / 20, x + s * 7 / 10, y + s * 7 / 20, w, fg);
        }
        1 => {
            fill(g.hdc, g.b, cx - w / 2, y + s * 3 / 10, cx + w - w / 2, y + s * 3 / 5, fg);
            fill(g.hdc, g.b, cx - w / 2, y + s * 2 / 3, cx + w - w / 2, y + s * 2 / 3 + w, fg);
        }
        _ => {
            line(g.hdc, g.b, x + s / 3, y + s / 3, x + s * 2 / 3, y + s * 2 / 3, w, fg);
            line(g.hdc, g.b, x + s * 2 / 3, y + s / 3, x + s / 3, y + s * 2 / 3, w, fg);
        }
    }
}

/// Icône d'un type de réseau : immeuble (domaine), maison (privé), globe (public).
unsafe fn profile_icon(g: &mut Gfx, i: usize, x: i32, y: i32, bg: u32) {
    let p = pal();
    let tile = mix(bg, p.accent, 2, 10);
    round_rect(g.hdc, g.b, x, y, x + 36, y + 36, 8, tile);
    let c = readable(p.accent, &[tile], 300);
    match i {
        0 => {
            // Immeuble : bloc et fenêtres.
            fill(g.hdc, g.b, x + 11, y + 8, x + 25, y + 29, c);
            for k in 0..3 {
                fill(g.hdc, g.b, x + 14, y + 11 + k * 6, x + 17, y + 14 + k * 6, tile);
                fill(g.hdc, g.b, x + 19, y + 11 + k * 6, x + 22, y + 14 + k * 6, tile);
            }
        }
        1 => {
            // Maison : toit et corps.
            for k in 0..9 {
                fill(g.hdc, g.b, x + 18 - k - 1, y + 8 + k, x + 18 + k + 1, y + 9 + k, c);
            }
            fill(g.hdc, g.b, x + 11, y + 17, x + 25, y + 28, c);
            fill(g.hdc, g.b, x + 16, y + 21, x + 20, y + 28, tile);
        }
        _ => {
            // Globe : disque, équateur et méridien.
            ring(g.hdc, g.b, x + 8, y + 8, x + 28, y + 28, 2, c, tile);
            fill(g.hdc, g.b, x + 9, y + 17, x + 27, y + 19, c);
            ring(g.hdc, g.b, x + 14, y + 8, x + 22, y + 28, 2, c, tile);
        }
    }
}

/// Résumé d'un profil : « Entrant bloqué · sortant autorisé ».
unsafe fn profile_summary(i: usize) -> Field<96> {
    let mut s = Field::<96>::new();
    if !ENABLED[i] {
        s.push(t(
            b"Aucune protection sur ce type de r\xE9seau",
            b"No protection on this type of network",
        ));
        return s;
    }
    s.push(if INBOUND_BLOCK[i] {
        t(b"Entrant bloqu\xE9", b"Inbound blocked")
    } else {
        t(b"Entrant autoris\xE9", b"Inbound allowed")
    });
    s.push(b" \xB7 ");
    s.push(if OUTBOUND_BLOCK[i] {
        t(b"sortant bloqu\xE9", b"outbound blocked")
    } else {
        t(b"sortant autoris\xE9", b"outbound allowed")
    });
    s
}

fn profile_text(i: usize) -> &'static [u8] {
    match i {
        0 => t(
            b"R\xE9seaux d'entreprise o\xF9 cet ordinateur appartient \xE0 un domaine.",
            b"Workplace networks where this computer belongs to a domain.",
        ),
        1 => t(
            b"R\xE9seaux de confiance, comme chez vous ou au travail : l'ordinateur peut y \xEAtre visible.",
            b"Trusted networks, like at home or at work: the computer can be visible there.",
        ),
        _ => t(
            b"R\xE9seaux dans les lieux publics, comme un caf\xE9 ou un a\xE9roport : l'ordinateur y reste invisible.",
            b"Networks in public places, like a caf\xE9 or an airport: the computer stays hidden there.",
        ),
    }
}

unsafe fn log_lines() -> usize {
    LOG[..LOG_LEN]
        .split(|&c| c == b'\n')
        .enumerate()
        .filter(|(k, l)| !l.is_empty() && !(*k == 0 && LOG_LEN == LOG.len()))
        .count()
}

unsafe fn active_rules() -> usize {
    (0..RULE_COUNT).filter(|&i| RULES[i].enabled).count()
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
    glyph(g.hdc, g.b, g.f, Glyph::Shield, c.left + 18, c.top + 18, 24);
    text_fit(
        g.hdc,
        g.f.title,
        15,
        p.text,
        c.left + 52,
        c.top + 21,
        t(b"Pare-feu Winlux", b"Winlux Firewall"),
        NAV_W - 64,
    );
    let nav: [(&[u8], Glyph); 5] = [
        (t(b"Vue d'ensemble", b"Overview"), Glyph::Home),
        (
            t(b"Applications autoris\xE9es", b"Allowed apps"),
            Glyph::Exe,
        ),
        (t(b"R\xE8gles avanc\xE9es", b"Advanced rules"), Glyph::List),
        (t(b"Activit\xE9 r\xE9seau", b"Network activity"), Glyph::Clock),
        (t(b"Param\xE8tres", b"Settings"), Glyph::Settings),
    ];
    for (i, (label_text, gl)) in nav.iter().enumerate() {
        let active = PAGE == i || (PAGE == PAGE_PROFILE && i == PAGE_OVERVIEW);
        nav_item(
            g,
            h,
            c.left + 4,
            c.top + 68 + i as i32 * 40,
            NAV_W - 8,
            ID_NAV + i as u16,
            label_text,
            *gl,
            active,
        );
    }
    let area = rect(c.left + NAV_W, c.top, c.right, c.bottom);
    match PAGE {
        PAGE_OVERVIEW => draw_overview(g, h, &area),
        PAGE_APPS => draw_apps(g, h, &area),
        PAGE_RULES => draw_rules(g, h, &area),
        PAGE_ACTIVITY => draw_activity(g, h, &area),
        PAGE_SETTINGS => draw_settings(g, h, &area),
        _ => draw_profile(g, h, &area),
    }
    if !NOTICE.is_empty() && !EDITING {
        text_fit(g.hdc, g.f.caption, 12, p.text2, area.left + 32, area.bottom - 22, NOTICE, area.right - area.left - 64);
    }
    if EDITING {
        draw_editor(g, h, c);
    } else if prompt_pending() {
        draw_prompt(g, h, c);
    }
}

/// Titre de page (et fil d'Ariane pour le détail d'un réseau).
unsafe fn page_title(g: &mut Gfx, a: &Rect, s: &[u8]) -> i32 {
    text(g.hdc, g.f.large, pal().text, a.left + 32, a.top + 22, s);
    a.top + 70
}

unsafe fn draw_overview(g: &mut Gfx, h: &mut Hits, a: &Rect) {
    let p = pal();
    let x = a.left + 32;
    let r = a.right - 32;
    let mut y = page_title(
        g,
        a,
        t(
            b"Pare-feu et protection du r\xE9seau",
            b"Firewall & network protection",
        ),
    );
    // Bandeau d'état : bouclier coloré, phrase principale, détail, correction.
    let st = &STATUS[..STATUS_LEN];
    let backend_absent = field(st, b"backend") == b"absent";
    let error = field(st, b"error");
    let (state, title, detail): (u8, &[u8], &[u8]) = if backend_absent {
        (
            2,
            t(b"Le pare-feu ne peut pas fonctionner", b"The firewall can't run"),
            t(
                b"nftables est absent : les r\xE8gles ne peuvent pas \xEAtre appliqu\xE9es.",
                b"nftables is missing: rules cannot be applied.",
            ),
        )
    } else if !ENABLED[PROFILE] {
        (
            2,
            t(
                b"Le pare-feu est d\xE9sactiv\xE9 sur le r\xE9seau actuel",
                b"The firewall is off on the current network",
            ),
            t(
                b"Les autres ordinateurs du r\xE9seau peuvent joindre toutes vos applications.",
                b"Other computers on the network can reach all of your apps.",
            ),
        )
    } else if !error.is_empty() {
        (1, t(b"Le pare-feu a besoin d'attention", b"The firewall needs attention"), error)
    } else if STATUS_LEN == 0 {
        (
            1,
            t(b"Le pare-feu d\xE9marre", b"The firewall is starting"),
            t(
                b"Le service de pare-feu n'a pas encore appliqu\xE9 la configuration.",
                b"The firewall service has not applied the configuration yet.",
            ),
        )
    } else {
        (
            0,
            t(b"Votre ordinateur est prot\xE9g\xE9", b"Your computer is protected"),
            t(
                b"Le pare-feu prot\xE8ge cet ordinateur.",
                b"The firewall is protecting this device.",
            ),
        )
    };
    let hero = rect(x, y, r, y + 112);
    card(g, &hero);
    let color = match state {
        0 => ok_color(p.surface),
        1 => warn_color(p.surface),
        _ => bad_color(p.surface),
    };
    status_shield(g, x + 24, y + 24, 64, color, state);
    let tx = x + 112;
    let fix: &[u8] = if state == 2 && !backend_absent {
        t(b"Activer", b"Turn on")
    } else {
        b""
    };
    let fw = if fix.is_empty() { 0 } else { button_w(fix).max(96) + 24 };
    text_fit(g.hdc, g.f.title, 15, p.text, tx, y + 24, title, r - tx - 24 - fw);
    wrap(g, g.f.body, p.text2, tx, y + 50, r - tx - 24 - fw, detail, 2);
    let mut info = Field::<120>::new();
    info.push(t(b"R\xE9seau actuel : ", b"Current network: "));
    info.push(profile_name(PROFILE));
    text_fit(g.hdc, g.f.caption, 12, p.text3, tx, y + 88, info.get(), r - tx - 24 - fw);
    if !fix.is_empty() {
        let bw = button_w(fix).max(96);
        button(g, h, rect(r - 24 - bw, y + 40, r - 24, y + 72), ID_HERO, fix, true, true);
    }
    y += 132;
    text(g.hdc, g.f.semibold, p.text, x, y, t(b"Types de r\xE9seau", b"Network types"));
    y += 28;
    let list = rect(x, y, r, y + 3 * 60);
    card(g, &list);
    for i in 0..3 {
        let row = rect(x + 1, y + i as i32 * 60 + 1, r - 1, y + i as i32 * 60 + 59);
        let hover = h.add(row, ID_PROFILE_OPEN + i as u16);
        let bg = if hover { p.hover } else { p.surface };
        if hover {
            fill(g.hdc, g.b, row.left, row.top, row.right, row.bottom, bg);
        }
        if i > 0 {
            divider(g, x + 1, r - 1, row.top - 1);
        }
        profile_icon(g, i, x + 16, row.top + 11, bg);
        // Semi-gras : un peu plus large que la mesure du corps de texte.
        let name_w = text_width(profile_name(i), 13) * 11 / 10 + 12;
        text(g.hdc, g.f.semibold, p.text, x + 66, row.top + 11, profile_name(i));
        if i == PROFILE {
            pill(g, x + 66 + name_w, row.top + 8, t(b"Actif", b"Active"), p.accent);
        }
        let summary = profile_summary(i);
        text_fit(g.hdc, g.f.caption, 12, p.text2, x + 66, row.top + 32, summary.get(), r - x - 300);
        let (s, c) = if ENABLED[i] {
            (t(b"Pare-feu activ\xE9", b"Firewall on"), ok_color(bg))
        } else {
            (t(b"Pare-feu d\xE9sactiv\xE9", b"Firewall off"), bad_color(bg))
        };
        let sw = text_width(s, 13);
        ellipse(g.hdc, g.b, r - 52 - sw - 16, row.top + 25, r - 52 - sw - 6, row.top + 35, c);
        text(g.hdc, g.f.body, c, r - 52 - sw, row.top + 21, s);
        chevron(g, r - 28, row.top + 30);
    }
    y += 3 * 60 + 20;
    text(g.hdc, g.f.semibold, p.text, x, y, t(b"Raccourcis", b"Shortcuts"));
    y += 28;
    let links: [(&[u8], &[u8], usize); 3] = [
        (
            t(b"Autoriser une application via le pare-feu", b"Allow an app through the firewall"),
            t(
                b"Choisir les applications qui peuvent recevoir des connexions.",
                b"Choose which apps can accept incoming connections.",
            ),
            PAGE_APPS,
        ),
        (
            t(b"R\xE8gles avanc\xE9es", b"Advanced rules"),
            t(
                b"Ports, protocoles et programmes, en entr\xE9e comme en sortie.",
                b"Ports, protocols and programs, inbound and outbound.",
            ),
            PAGE_RULES,
        ),
        (
            t(b"Activit\xE9 r\xE9seau", b"Network activity"),
            t(
                b"Connexions bloqu\xE9es et applications qui attendent des connexions.",
                b"Blocked connections and apps waiting for connections.",
            ),
            PAGE_ACTIVITY,
        ),
    ];
    let list = rect(x, y, r, y + 3 * 56);
    card(g, &list);
    for (k, (title, sub, page)) in links.iter().enumerate() {
        let row = rect(x + 1, y + k as i32 * 56 + 1, r - 1, y + k as i32 * 56 + 55);
        let hover = h.add(row, ID_GO + *page as u16);
        if hover {
            fill(g.hdc, g.b, row.left, row.top, row.right, row.bottom, p.hover);
        }
        if k > 0 {
            divider(g, x + 1, r - 1, row.top - 1);
        }
        text_fit(g.hdc, g.f.body, 13, p.text, x + 20, row.top + 9, title, r - x - 80);
        text_fit(g.hdc, g.f.caption, 12, p.text2, x + 20, row.top + 29, sub, r - x - 80);
        chevron(g, r - 28, row.top + 27);
    }
}

unsafe fn draw_profile(g: &mut Gfx, h: &mut Hits, a: &Rect) {
    let p = pal();
    let i = DETAIL.min(2);
    let x = a.left + 32;
    let r = a.right - 32;
    // Fil d'Ariane : « Vue d'ensemble › Réseau privé ».
    let crumb = t(b"Vue d'ensemble", b"Overview");
    let cw = text_width(crumb, 22) * 6 / 5;
    let hover = h.add(rect(x - 4, a.top + 18, x + cw + 4, a.top + 52), ID_BACK);
    text(g.hdc, g.f.large, if hover { p.text } else { p.text2 }, x, a.top + 22, crumb);
    chevron(g, x + cw + 4, a.top + 36);
    text(g.hdc, g.f.large, p.text, x + cw + 22, a.top + 22, profile_name(i));
    let mut y = a.top + 70;
    profile_icon(g, i, x, y, p.window);
    wrap(g, g.f.body, p.text2, x + 52, y + 8, r - x - 52, profile_text(i), 2);
    y += 60;
    // Interrupteur principal.
    let main = rect(x, y, r, y + 72);
    card(g, &main);
    text(g.hdc, g.f.semibold, p.text, x + 20, y + 16, t(b"Pare-feu Winlux", b"Winlux Firewall"));
    text_fit(
        g.hdc,
        g.f.caption,
        12,
        p.text2,
        x + 20,
        y + 38,
        t(
            b"Prot\xE8ge cet ordinateur sur les r\xE9seaux de ce type.",
            b"Protects this computer on networks of this type.",
        ),
        r - x - 140,
    );
    let state = if ENABLED[i] { t(b"Activ\xE9", b"On") } else { t(b"D\xE9sactiv\xE9", b"Off") };
    text(g.hdc, g.f.body, p.text, r - 72 - text_width(state, 13), y + 27, state);
    switch(g, h, r - 20, y + 36, ID_PROFILE_ON + i as u16, ENABLED[i]);
    y += 88;
    // Connexions entrantes et sortantes.
    let traffic = rect(x, y, r, y + 2 * 72);
    card(g, &traffic);
    let rows: [(&[u8], &[u8], u16, bool); 2] = [
        (
            t(b"Connexions entrantes", b"Incoming connections"),
            t(
                b"Bloquer : seules les applications autoris\xE9es les re\xE7oivent.",
                b"Block: only allowed apps receive them.",
            ),
            ID_INBOUND,
            INBOUND_BLOCK[i],
        ),
        (
            t(b"Connexions sortantes", b"Outgoing connections"),
            t(
                b"Autoriser : vos applications joignent Internet normalement.",
                b"Allow: your apps reach the Internet normally.",
            ),
            ID_OUTBOUND,
            OUTBOUND_BLOCK[i],
        ),
    ];
    for (k, (title, sub, base, blocked)) in rows.iter().enumerate() {
        let ry = y + k as i32 * 72;
        if k > 0 {
            divider(g, x + 1, r - 1, ry);
        }
        let labels: [&[u8]; 2] = [t(b"Bloquer", b"Block"), t(b"Autoriser", b"Allow")];
        let sw = text_width(labels[0], 13) * 11 / 10 + text_width(labels[1], 13) * 11 / 10 + 68;
        text_fit(g.hdc, g.f.semibold, 13, p.text, x + 20, ry + 16, title, r - x - sw - 48);
        text_fit(g.hdc, g.f.caption, 12, p.text2, x + 20, ry + 38, sub, r - x - sw - 48);
        let base = *base + i as u16 * 2;
        segmented(g, h, r - 20 - sw, ry + 20, &[base, base + 1], &labels, if *blocked { 0 } else { 1 });
    }
    y += 2 * 72 + 16;
    let current = rect(x, y, r, y + 64);
    card(g, &current);
    if i == PROFILE {
        ellipse(g.hdc, g.b, x + 20, y + 27, x + 30, y + 37, ok_color(p.surface));
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            x + 42,
            y + 23,
            t(
                b"Ce profil est utilis\xE9 pour le r\xE9seau actuel.",
                b"This profile is used for the current network.",
            ),
            r - x - 60,
        );
    } else {
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            x + 20,
            y + 23,
            t(
                b"Le r\xE9seau actuel utilise un autre profil.",
                b"The current network uses another profile.",
            ),
            r - x - 300,
        );
        let s = t(b"Utiliser pour le r\xE9seau actuel", b"Use for the current network");
        let w = button_w(s);
        button(g, h, rect(r - 20 - w, y + 16, r - 20, y + 48), ID_PROFILE_SET + i as u16, s, false, true);
    }
}

unsafe fn draw_apps(g: &mut Gfx, h: &mut Hits, a: &Rect) {
    let p = pal();
    let x = a.left + 32;
    let r = a.right - 32;
    let mut y = page_title(g, a, t(b"Applications autoris\xE9es", b"Allowed apps"));
    y += wrap(
        g,
        g.f.body,
        p.text2,
        x,
        y,
        r - x,
        t(
            b"Ces applications peuvent recevoir des connexions depuis le r\xE9seau. Les autres restent bloqu\xE9es, et vous \xEAtes averti la premi\xE8re fois qu'elles essaient.",
            b"These apps can accept connections from the network. Others stay blocked, and you're notified the first time they try.",
        ),
        2,
    ) + 12;
    VISIBLE_COUNT = 0;
    for i in 0..RULE_COUNT {
        if RULES[i].inbound && RULES[i].program.len > 0 {
            VISIBLE[VISIBLE_COUNT] = i;
            VISIBLE_COUNT += 1;
        }
    }
    let has = SELECTED < RULE_COUNT && (0..VISIBLE_COUNT).any(|k| VISIBLE[k] == SELECTED);
    let s_new = t(b"Autoriser une application", b"Allow an app");
    let s_edit = t(b"Modifier", b"Edit");
    let s_del = t(b"Supprimer", b"Delete");
    let mut bx = x;
    for (id, s, primary, on) in [(ID_NEW_APP, s_new, true, true), (ID_EDIT, s_edit, false, has), (ID_DELETE, s_del, false, has)] {
        let w = button_w(s);
        button(g, h, rect(bx, y, bx + w, y + 32), id, s, primary, on);
        bx += w + 8;
    }
    y += 48;
    let table = rect(x, y, r, a.bottom - 40);
    card(g, &table);
    let col_w = 84;
    let checks_x = r - 16 - 3 * col_w - 110;
    text(g.hdc, g.f.caption, p.text2, x + 20, y + 12, t(b"Application", b"App"));
    for k in 0..3 {
        text_centered(g.hdc, g.f.caption, 12, p.text2, checks_x + k as i32 * col_w + col_w / 2, y + 12, profile_short(k));
    }
    text(g.hdc, g.f.caption, p.text2, r - 116, y + 12, t(b"Acc\xE8s", b"Access"));
    divider(g, x + 1, r - 1, y + 36);
    let mut ry = y + 37;
    if VISIBLE_COUNT == 0 {
        wrap(
            g,
            g.f.body,
            p.text2,
            x + 20,
            ry + 16,
            r - x - 40,
            t(
                b"Aucune application autoris\xE9e pour l'instant. Quand une application demandera \xE0 recevoir des connexions, Winlux vous demandera votre accord.",
                b"No allowed apps yet. When an app asks to accept connections, Winlux will ask for your consent.",
            ),
            3,
        );
        return;
    }
    let visible = ((table.bottom - ry - 4) / 56).max(1) as usize;
    if SCROLL + visible > VISIBLE_COUNT {
        SCROLL = VISIBLE_COUNT.saturating_sub(visible);
    }
    for k in SCROLL..VISIBLE_COUNT.min(SCROLL + visible) {
        let i = VISIBLE[k];
        let rule = RULES[i];
        let row = rect(x + 1, ry, r - 1, ry + 56);
        let hover = h.row(row, ID_ROW + i as u16);
        let bg = if i == SELECTED {
            p.selected
        } else if hover {
            p.hover
        } else {
            p.surface
        };
        if bg != p.surface {
            fill(g.hdc, g.b, row.left, row.top, row.right, row.bottom, bg);
        }
        if k > SCROLL {
            divider(g, x + 1, r - 1, ry);
        }
        set_glyph_bg(Some(bg));
        glyph(g.hdc, g.b, g.f, Glyph::Exe, x + 18, ry + 14, 28);
        set_glyph_bg(None);
        let tw = checks_x - x - 76;
        let color = if rule.enabled { p.text } else { p.text3 };
        text_fit(g.hdc, g.f.semibold, 13, color, x + 60, ry + 10, rule.name.get(), tw);
        text_fit(g.hdc, g.f.caption, 12, p.text2, x + 60, ry + 31, rule.program.get(), tw);
        for pi in 0..3 {
            checkbox(
                g,
                h,
                checks_x + pi as i32 * col_w + col_w / 2 - 10,
                ry + 18,
                ID_CHECK + (i * 3 + pi) as u16,
                rule.profiles & (1 << pi) != 0,
            );
        }
        let (s, c) = if !rule.enabled {
            (t(b"D\xE9sactiv\xE9e", b"Disabled"), p.text2)
        } else if rule.allow {
            (t(b"Autoris\xE9e", b"Allowed"), ok_color(bg))
        } else {
            (t(b"Bloqu\xE9e", b"Blocked"), bad_color(bg))
        };
        pill(g, r - 116, ry + 17, s, c);
        ry += 56;
    }
}

unsafe fn draw_rules(g: &mut Gfx, h: &mut Hits, a: &Rect) {
    let p = pal();
    let x = a.left + 32;
    let r = a.right - 32;
    let mut y = page_title(g, a, t(b"R\xE8gles avanc\xE9es", b"Advanced rules"));
    VISIBLE_COUNT = 0;
    let (mut n_in, mut n_out) = (0, 0);
    for i in 0..RULE_COUNT {
        if RULES[i].inbound {
            n_in += 1;
        } else {
            n_out += 1;
        }
        if RULES[i].inbound == RULES_IN {
            VISIBLE[VISIBLE_COUNT] = i;
            VISIBLE_COUNT += 1;
        }
    }
    let mut l_in = Field::<48>::new();
    l_in.push(t(b"Trafic entrant", b"Inbound traffic"));
    l_in.push(b" (");
    push_num(&mut l_in, n_in);
    l_in.push(b")");
    let mut l_out = Field::<48>::new();
    l_out.push(t(b"Trafic sortant", b"Outbound traffic"));
    l_out.push(b" (");
    push_num(&mut l_out, n_out);
    l_out.push(b")");
    let labels: [&[u8]; 2] = [l_in.get(), l_out.get()];
    segmented(g, h, x, y, &[ID_TAB, ID_TAB + 1], &labels, if RULES_IN { 0 } else { 1 });
    y += 48;
    let has = SELECTED < RULE_COUNT && RULES[SELECTED].inbound == RULES_IN;
    let buttons: [(u16, &[u8], bool, bool); 3] = [
        (ID_NEW, t(b"Nouvelle r\xE8gle", b"New rule"), true, true),
        (ID_EDIT, t(b"Modifier", b"Edit"), false, has),
        (ID_DELETE, t(b"Supprimer", b"Delete"), false, has),
    ];
    let mut bx = x;
    for (id, label_text, primary, on) in buttons {
        let w = button_w(label_text);
        button(g, h, rect(bx, y, bx + w, y + 32), id, label_text, primary, on);
        bx += w + 8;
    }
    y += 48;
    let table = rect(x, y, r, a.bottom - 40);
    card(g, &table);
    let width = r - x - 40;
    let cols: [(&[u8], i32); 6] = [
        (t(b"Nom", b"Name"), width * 27 / 100),
        (t(b"Action", b"Action"), width * 15 / 100),
        (t(b"Protocole", b"Protocol"), width * 11 / 100),
        (t(b"Ports", b"Ports"), width * 12 / 100),
        (t(b"Profils", b"Profiles"), width * 20 / 100),
        (t(b"Activ\xE9e", b"Enabled"), width * 15 / 100),
    ];
    let mut cx = x + 20;
    for (name, w) in cols.iter() {
        text_fit(g.hdc, g.f.caption, 12, p.text2, cx, y + 12, name, w - 10);
        cx += w;
    }
    divider(g, x + 1, r - 1, y + 36);
    let mut ry = y + 37;
    if VISIBLE_COUNT == 0 {
        wrap(
            g,
            g.f.body,
            p.text2,
            x + 20,
            ry + 16,
            r - x - 40,
            t(
                b"Aucune r\xE8gle. Le comportement par d\xE9faut du profil s'applique.",
                b"No rules. The profile's default behavior applies.",
            ),
            2,
        );
        return;
    }
    let visible = ((table.bottom - ry - 4) / 52).max(1) as usize;
    if SCROLL + visible > VISIBLE_COUNT {
        SCROLL = VISIBLE_COUNT.saturating_sub(visible);
    }
    for k in SCROLL..VISIBLE_COUNT.min(SCROLL + visible) {
        let i = VISIBLE[k];
        let rule = RULES[i];
        let row = rect(x + 1, ry, r - 1, ry + 52);
        let hover = h.row(row, ID_ROW + i as u16);
        let bg = if i == SELECTED {
            p.selected
        } else if hover {
            p.hover
        } else {
            p.surface
        };
        if bg != p.surface {
            fill(g.hdc, g.b, row.left, row.top, row.right, row.bottom, bg);
        }
        if k > SCROLL {
            divider(g, x + 1, r - 1, ry);
        }
        let color = if rule.enabled { p.text } else { p.text3 };
        let mut cx = x + 20;
        text_fit(g.hdc, g.f.semibold, 13, color, cx, ry + 8, rule.name.get(), cols[0].1 - 12);
        let prog: &[u8] = if rule.program.len > 0 {
            file_name(rule.program.get())
        } else {
            t(b"Tous les programmes", b"All programs")
        };
        text_fit(g.hdc, g.f.caption, 12, p.text2, cx, ry + 28, prog, cols[0].1 - 12);
        cx += cols[0].1;
        let (s, c) = if rule.allow {
            (t(b"Autoriser", b"Allow"), ok_color(bg))
        } else {
            (t(b"Bloquer", b"Block"), bad_color(bg))
        };
        pill(g, cx, ry + 15, s, if rule.enabled { c } else { p.text2 });
        cx += cols[1].1;
        cell(g, cx, ry + 17, cols[2].1, proto_name(rule.proto), color);
        cx += cols[2].1;
        cell(
            g,
            cx,
            ry + 17,
            cols[3].1,
            if rule.ports.len > 0 { rule.ports.get() } else { t(b"Tous", b"Any") },
            color,
        );
        cx += cols[3].1;
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
        cell(g, cx, ry + 17, cols[4].1, prof.get(), color);
        cx += cols[4].1;
        switch(g, h, cx + 40, ry + 26, ID_RULE_ON + i as u16, rule.enabled);
        ry += 52;
    }
}

unsafe fn draw_activity(g: &mut Gfx, h: &mut Hits, a: &Rect) {
    let p = pal();
    let x = a.left + 32;
    let r = a.right - 32;
    let mut y = page_title(g, a, t(b"Activit\xE9 r\xE9seau", b"Network activity"));
    let mut l_log = Field::<48>::new();
    l_log.push(t(b"Connexions bloqu\xE9es", b"Blocked connections"));
    let n_log = log_lines();
    if n_log > 0 {
        l_log.push(b" (");
        push_num(&mut l_log, n_log as i64);
        l_log.push(b")");
    }
    let labels: [&[u8]; 2] = [l_log.get(), t(b"Ports ouverts", b"Open ports")];
    let sw = segmented(g, h, x, y, &[ID_TAB + 2, ID_TAB + 3], &labels, if ACT_PORTS { 1 } else { 0 });
    let s1 = t(b"Actualiser", b"Refresh");
    let w1 = button_w(s1);
    let mut bx = r - w1;
    if !ACT_PORTS {
        let s2 = t(b"Effacer le journal", b"Clear log");
        let w2 = button_w(s2);
        button(g, h, rect(r - w2, y, r, y + 32), ID_CLEAR_LOG, s2, false, LOG_LEN > 0);
        bx = r - w2 - 8 - w1;
    }
    if bx > x + sw + 8 {
        button(g, h, rect(bx, y, bx + w1, y + 32), ID_REFRESH, s1, false, true);
    }
    y += 48;
    if ACT_PORTS {
        y += wrap(
            g,
            g.f.body,
            p.text2,
            x,
            y,
            r - x,
            t(
                b"Les applications Windows et Linux partagent les m\xEAmes ports : chacune peut joindre l'autre sur cet ordinateur.",
                b"Windows and Linux apps share the same ports: each can reach the other on this computer.",
            ),
            2,
        ) + 12;
    }
    let table = rect(x, y, r, a.bottom - 40);
    card(g, &table);
    let width = r - x - 40;
    if ACT_PORTS {
        let cols: [(&[u8], i32); 4] = [
            (t(b"Application", b"App"), width * 40 / 100),
            (t(b"Port", b"Port"), width * 18 / 100),
            (t(b"Accessible depuis", b"Reachable from"), width * 30 / 100),
            (b"PID", width * 12 / 100),
        ];
        let mut cx = x + 20;
        for (name, w) in cols.iter() {
            text_fit(g.hdc, g.f.caption, 12, p.text2, cx, y + 12, name, w - 10);
            cx += w;
        }
        divider(g, x + 1, r - 1, y + 36);
        let mut ry = y + 37;
        let data = &PORTS[..PORTS_LEN];
        let n = data.split(|&c| c == b'\n').filter(|l| !l.is_empty()).count();
        if n == 0 {
            text(g.hdc, g.f.body, p.text2, x + 20, ry + 16, t(b"Aucune application n'attend de connexion.", b"No app is waiting for connections."));
            return;
        }
        let visible = ((table.bottom - ry - 4) / 40).max(1) as usize;
        if SCROLL + visible > n {
            SCROLL = n.saturating_sub(visible);
        }
        for (k, row) in data.split(|&c| c == b'\n').filter(|l| !l.is_empty()).skip(SCROLL).take(visible).enumerate() {
            if k > 0 {
                divider(g, x + 1, r - 1, ry);
            }
            let mut cx = x + 20;
            let name = column(row, 4);
            let name: &[u8] = if name.is_empty() { t(b"Syst\xE8me", b"System") } else { name };
            cell(g, cx, ry + 12, cols[0].1, name, p.text);
            cx += cols[0].1;
            let mut port = Field::<24>::new();
            port.push(column(row, 1));
            port.push(b" \xB7 ");
            port.push(column(row, 0));
            cell(g, cx, ry + 12, cols[1].1, port.get(), p.text);
            cx += cols[1].1;
            let local = column(row, 2) == b"local";
            let (s, c) = if local {
                (t(b"Cet ordinateur uniquement", b"This computer only"), ok_color(p.surface))
            } else {
                (t(b"R\xE9seau", b"Network"), warn_color(p.surface))
            };
            pill(g, cx, ry + 9, s, c);
            cx += cols[2].1;
            let pid = column(row, 3);
            cell(g, cx, ry + 12, r - 16 - cx, if pid == b"0" { b"" } else { pid }, p.text2);
            ry += 40;
        }
        return;
    }
    let cols: [(&[u8], i32); 5] = [
        (t(b"Date et heure", b"Date and time"), width * 22 / 100),
        (t(b"Sens", b"Direction"), width * 14 / 100),
        (t(b"Source", b"Source"), width * 26 / 100),
        (t(b"Destination", b"Destination"), width * 26 / 100),
        (t(b"Port", b"Port"), width * 12 / 100),
    ];
    let mut cx = x + 20;
    for (name, w) in cols.iter() {
        text_fit(g.hdc, g.f.caption, 12, p.text2, cx, y + 12, name, w - 10);
        cx += w;
    }
    divider(g, x + 1, r - 1, y + 36);
    let mut ry = y + 37;
    let data = &LOG[..LOG_LEN];
    let mut lines: [&[u8]; 400] = [b""; 400];
    let mut n = 0;
    for (k, row) in data.split(|&c| c == b'\n').enumerate() {
        if (k == 0 && LOG_LEN == LOG.len()) || row.is_empty() {
            continue;
        }
        if n == lines.len() {
            lines.copy_within(1.., 0);
            n -= 1;
        }
        lines[n] = row;
        n += 1;
    }
    if n == 0 {
        wrap(
            g,
            g.f.body,
            p.text2,
            x + 20,
            ry + 16,
            r - x - 40,
            t(
                b"Aucune connexion bloqu\xE9e n'a \xE9t\xE9 enregistr\xE9e.",
                b"No blocked connections have been logged.",
            ),
            2,
        );
        return;
    }
    let visible = ((table.bottom - ry - 4) / 40).max(1) as usize;
    if LOG_SCROLL + visible > n {
        LOG_SCROLL = n.saturating_sub(visible);
    }
    for k in LOG_SCROLL..n.min(LOG_SCROLL + visible) {
        let row = lines[n - 1 - k];
        if k > LOG_SCROLL {
            divider(g, x + 1, r - 1, ry);
        }
        let mut cx = x + 20;
        cell(g, cx, ry + 12, cols[0].1, column(row, 1), p.text2);
        cx += cols[0].1;
        let dir: &[u8] = if column(row, 2) == b"in" {
            t(b"Entrant", b"Inbound")
        } else {
            t(b"Sortant", b"Outbound")
        };
        pill(g, cx, ry + 9, dir, bad_color(p.surface));
        cx += cols[1].1;
        let mut src = Field::<64>::new();
        src.push(column(row, 4));
        if !column(row, 5).is_empty() {
            src.push(b" : ");
            src.push(column(row, 5));
        }
        cell(g, cx, ry + 12, cols[2].1, src.get(), p.text);
        cx += cols[2].1;
        cell(g, cx, ry + 12, cols[3].1, column(row, 6), p.text);
        cx += cols[3].1;
        let mut port = Field::<24>::new();
        port.push(column(row, 7));
        if !column(row, 3).is_empty() {
            port.push(b" \xB7 ");
            port.push(column(row, 3));
        }
        cell(g, cx, ry + 12, r - 16 - cx, port.get(), p.text2);
        ry += 40;
    }
}

unsafe fn draw_settings(g: &mut Gfx, h: &mut Hits, a: &Rect) {
    let p = pal();
    let x = a.left + 32;
    let r = a.right - 32;
    let mut y = page_title(g, a, t(b"Param\xE8tres", b"Settings"));
    let box1 = rect(x, y, r, y + 72);
    card(g, &box1);
    text(g.hdc, g.f.semibold, p.text, x + 20, y + 16, t(b"Notifications", b"Notifications"));
    text_fit(
        g.hdc,
        g.f.caption,
        12,
        p.text2,
        x + 20,
        y + 38,
        t(
            b"M'avertir lorsqu'une application commence \xE0 recevoir des connexions",
            b"Notify me when an app starts accepting incoming connections",
        ),
        r - x - 100,
    );
    switch(g, h, r - 20, y + 36, ID_NOTIFY, NOTIFY);
    y += 88;
    // Moteur de filtrage et fichier de configuration.
    let st = &STATUS[..STATUS_LEN];
    let absent = field(st, b"backend") == b"absent";
    let box2 = rect(x, y, r, y + 2 * 64);
    card(g, &box2);
    text(g.hdc, g.f.body, p.text, x + 20, y + 14, t(b"Moteur de filtrage", b"Filtering engine"));
    text(g.hdc, g.f.caption, p.text2, x + 20, y + 34, b"nftables");
    let (s, c) = if absent {
        (t(b"Absent", b"Missing"), bad_color(p.surface))
    } else if STATUS_LEN == 0 {
        (t(b"En attente", b"Waiting"), warn_color(p.surface))
    } else {
        (t(b"En service", b"Running"), ok_color(p.surface))
    };
    pill(g, r - 20 - text_width(s, 12) - 20, y + 20, s, c);
    divider(g, x + 1, r - 1, y + 64);
    text(g.hdc, g.f.body, p.text, x + 20, y + 78, t(b"Configuration", b"Configuration"));
    text(g.hdc, g.f.caption, p.text2, x + 20, y + 98, b"/etc/winlux/firewall.conf");
    let mut count = Field::<48>::new();
    push_num(&mut count, active_rules() as i64);
    count.push(t(b" r\xE8gle(s) active(s)", b" active rule(s)"));
    text(g.hdc, g.f.body, p.text2, r - 20 - text_width(count.get(), 13), y + 88, count.get());
    y += 2 * 64 + 16;
    // Rétablir les valeurs par défaut, après confirmation.
    let box3 = rect(x, y, r, y + 80);
    card(g, &box3);
    text(g.hdc, g.f.semibold, p.text, x + 20, y + 16, t(b"R\xE9tablir les param\xE8tres par d\xE9faut", b"Restore default settings"));
    wrap(
        g,
        g.f.caption,
        p.text2,
        x + 20,
        y + 38,
        r - x - 260,
        if CONFIRM_RESET {
            t(
                b"Toutes les r\xE8gles et applications autoris\xE9es seront supprim\xE9es. Continuer ?",
                b"All rules and allowed apps will be deleted. Continue?",
            )
        } else {
            t(
                b"Pare-feu activ\xE9 partout, entrant bloqu\xE9, sortant autoris\xE9, aucune r\xE8gle.",
                b"Firewall on everywhere, inbound blocked, outbound allowed, no rules.",
            )
        },
        2,
    );
    if CONFIRM_RESET {
        let s_no = t(b"Annuler", b"Cancel");
        let s_ok = t(b"R\xE9tablir", b"Restore");
        let wn = button_w(s_no);
        let wo = button_w(s_ok).max(96);
        button(g, h, rect(r - 20 - wn, y + 24, r - 20, y + 56), ID_RESET_NO, s_no, false, true);
        button(g, h, rect(r - 28 - wn - wo, y + 24, r - 28 - wn, y + 56), ID_RESET_OK, s_ok, true, true);
    } else {
        let s = t(b"R\xE9tablir", b"Restore");
        let w = button_w(s).max(96);
        button(g, h, rect(r - 20 - w, y + 24, r - 20, y + 56), ID_RESET, s, false, true);
    }
}

unsafe fn draw_editor(g: &mut Gfx, h: &mut Hits, c: &Rect) {
    let p = pal();
    let title = if EDIT < RULE_COUNT {
        t(b"Propri\xE9t\xE9s de la r\xE8gle", b"Rule properties")
    } else if FOCUS == 2 && DRAFT.proto == 0 && DRAFT.inbound && DRAFT.name.len == 0 {
        t(b"Autoriser une application", b"Allow an app")
    } else if DRAFT.inbound {
        t(b"Nouvelle r\xE8gle de trafic entrant", b"New inbound rule")
    } else {
        t(b"Nouvelle r\xE8gle de trafic sortant", b"New outbound rule")
    };
    let d = dialog(g, h, c, 660, 500, title);
    let x = d.left + 24;
    let r = d.right - 24;
    let lw = 150;
    let mut y = d.top + 70;
    // Libellés à gauche, réglages à droite (boîte de propriétés Windows 11).
    let row_label = |g: &mut Gfx, y: i32, s: &[u8]| text_fit(g.hdc, g.f.semibold, 13, p.text, x, y + 8, s, lw - 12);
    row_label(g, y, t(b"Nom", b"Name"));
    input(
        g,
        h,
        rect(x + lw, y, r, y + 32),
        ID_FIELD,
        DRAFT.name.get(),
        FOCUS == 0,
        t(b"Par d\xE9faut : nom du programme", b"Default: program name"),
    );
    y += 46;
    row_label(
        g,
        y,
        t(b"Programme", b"Program"),
    );
    input(
        g,
        h,
        rect(x + lw, y, r, y + 32),
        ID_FIELD + 2,
        DRAFT.program.get(),
        FOCUS == 2,
        t(b"Tous les programmes", b"All programs"),
    );
    text_fit(
        g.hdc,
        g.f.caption,
        12,
        p.text3,
        x + lw,
        y + 38,
        t(
            b"Chemin du .exe ou de l'ex\xE9cutable Linux ; vide = tous",
            b"Path of the .exe or Linux executable; empty = all",
        ),
        r - x - lw,
    );
    y += 64;
    row_label(g, y, t(b"Action", b"Action"));
    let labels: [&[u8]; 2] = [t(b"Autoriser", b"Allow"), t(b"Bloquer", b"Block")];
    segmented(g, h, x + lw, y, &[ID_ACTION, ID_ACTION + 1], &labels, if DRAFT.allow { 0 } else { 1 });
    y += 46;
    row_label(g, y, t(b"Protocole", b"Protocol"));
    let protos: [&[u8]; 4] = [proto_name(0), proto_name(1), proto_name(2), proto_name(3)];
    segmented(g, h, x + lw, y, &[ID_PROTO, ID_PROTO + 1, ID_PROTO + 2, ID_PROTO + 3], &protos, DRAFT.proto as usize);
    y += 46;
    if DRAFT.proto != 3 {
        row_label(
            g,
            y,
            if DRAFT.inbound {
                t(b"Ports locaux", b"Local ports")
            } else {
                t(b"Ports distants", b"Remote ports")
            },
        );
        input(
            g,
            h,
            rect(x + lw, y, r, y + 32),
            ID_FIELD + 1,
            DRAFT.ports.get(),
            FOCUS == 1,
            t(b"Tous les ports", b"All ports"),
        );
        text_fit(
            g.hdc,
            g.f.caption,
            12,
            p.text3,
            x + lw,
            y + 38,
            t(b"Exemple : 80, 443 ou 8000-8100", b"For example: 80, 443 or 8000-8100"),
            r - x - lw,
        );
        y += 64;
    }
    row_label(g, y, t(b"Profils", b"Profiles"));
    let mut cx = x + lw;
    for i in 0..3 {
        checkbox(g, h, cx, y + 6, ID_PROF + i as u16, DRAFT.profiles & (1 << i) != 0);
        text(g.hdc, g.f.body, p.text, cx + 28, y + 8, profile_short(i));
        cx += 28 + text_width(profile_short(i), 13) + 28;
    }
    y += 46;
    row_label(g, y, t(b"R\xE8gle activ\xE9e", b"Rule enabled"));
    switch(g, h, x + lw + 40, y + 16, ID_ENABLED, DRAFT.enabled);
    if !ERROR.is_empty() {
        text_fit(g.hdc, g.f.body, 13, bad_color(p.window), x, d.bottom - 82, ERROR, r - x);
    }
    fill(g.hdc, g.b, d.left + 1, d.bottom - 68, d.right - 1, d.bottom - 67, p.border);
    let cancel = t(b"Annuler", b"Cancel");
    let ok = t(b"Enregistrer", b"Save");
    let cw = button_w(cancel).max(110);
    let ow = button_w(ok).max(110);
    button(g, h, rect(r - cw, d.bottom - 50, r, d.bottom - 18), ID_CANCEL, cancel, false, true);
    button(g, h, rect(r - cw - 8 - ow, d.bottom - 50, r - cw - 8, d.bottom - 18), ID_SAVE, ok, true, true);
}

unsafe fn draw_prompt(g: &mut Gfx, h: &mut Hits, c: &Rect) {
    let p = pal();
    let data = &PROMPT[..PROMPT_LEN];
    let d = dialog(g, h, c, 600, 360, t(b"Alerte de s\xE9curit\xE9 Winlux", b"Winlux Security Alert"));
    let x = d.left + 24;
    let r = d.right - 24;
    let mut y = d.top + 66;
    status_shield(g, x, y, 40, warn_color(p.window), 1);
    wrap(
        g,
        g.f.semibold,
        p.text,
        x + 56,
        y + 2,
        r - x - 56,
        t(
            b"Le Pare-feu Winlux a bloqu\xE9 certaines fonctionnalit\xE9s de cette application.",
            b"Winlux Firewall has blocked some features of this app.",
        ),
        2,
    );
    y += 56;
    let info = rect(x, y, r, y + 92);
    panel(g.hdc, g.b, info.left, info.top, info.right, info.bottom, 6, p.surface, p.border);
    let mut iy = y + 12;
    for (label_text, key) in [
        (t(b"Nom :", b"Name:"), b"name" as &[u8]),
        (t(b"Chemin :", b"Path:"), b"program"),
        (t(b"Ports :", b"Ports:"), b"ports"),
    ] {
        label(g, x + 14, iy, label_text);
        text_fit(g.hdc, g.f.body, 13, p.text, x + 100, iy, field(data, key), r - x - 114);
        iy += 24;
    }
    y += 108;
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
    text_fit(g.hdc, g.f.body, 13, p.text2, x, y + 20, q.get(), r - x);
    let block = t(b"Bloquer", b"Block");
    let allow = t(b"Autoriser l'acc\xE8s", b"Allow access");
    let bw = button_w(block).max(110);
    let aw = button_w(allow);
    button(g, h, rect(r - bw, d.bottom - 50, r, d.bottom - 18), ID_BLOCK, block, false, true);
    button(g, h, rect(r - bw - 8 - aw, d.bottom - 50, r - bw - 8, d.bottom - 18), ID_ALLOW, allow, true, true);
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
        i if (ID_NAV..ID_NAV + 5).contains(&i) => go((i - ID_NAV) as usize),
        i if (ID_GO..ID_GO + 5).contains(&i) => go((i - ID_GO) as usize),
        i if (ID_PROFILE_OPEN..ID_PROFILE_OPEN + 3).contains(&i) => {
            DETAIL = (i - ID_PROFILE_OPEN) as usize;
            go(PAGE_PROFILE);
        }
        ID_BACK => go(PAGE_OVERVIEW),
        ID_HERO => {
            ENABLED[PROFILE] = true;
            save();
        }
        ID_TAB | 331 => {
            RULES_IN = id == ID_TAB;
            SCROLL = 0;
            SELECTED = usize::MAX;
        }
        332 | 333 => {
            ACT_PORTS = id == 333;
            SCROLL = 0;
            LOG_SCROLL = 0;
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
        ID_NEW => open_editor(usize::MAX, false),
        ID_NEW_APP => open_editor(usize::MAX, true),
        ID_EDIT if SELECTED < RULE_COUNT => open_editor(SELECTED, false),
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
        ID_RESET => CONFIRM_RESET = true,
        ID_RESET_NO => CONFIRM_RESET = false,
        ID_RESET_OK => {
            CONFIRM_RESET = false;
            ENABLED = [true; 3];
            INBOUND_BLOCK = [true; 3];
            OUTBOUND_BLOCK = [false; 3];
            NOTIFY = true;
            RULE_COUNT = 0;
            SELECTED = usize::MAX;
            save();
        }
        i if (ID_CHECK..ID_CHECK + 3 * MAX_RULES as u16).contains(&i) => {
            let k = ((i - ID_CHECK) / 3) as usize;
            let bit = 1u8 << ((i - ID_CHECK) % 3);
            // Au moins un type de réseau reste coché.
            if k < RULE_COUNT && RULES[k].profiles != bit {
                RULES[k].profiles ^= bit;
                save();
            }
        }
        i if (ID_RULE_ON..ID_RULE_ON + MAX_RULES as u16).contains(&i) => {
            let k = (i - ID_RULE_ON) as usize;
            if k < RULE_COUNT {
                RULES[k].enabled = !RULES[k].enabled;
                save();
            }
        }
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
            Some(i) if !EDITING && i >= ID_ROW && i < ID_ROW + MAX_RULES as u16 && ((i - ID_ROW) as usize) < RULE_COUNT => {
                SELECTED = (i - ID_ROW) as usize;
                open_editor(SELECTED, false);
                true
            }
            _ => false,
        },
        Mouse::Wheel(delta) if !EDITING => {
            let s = if PAGE == PAGE_ACTIVITY && !ACT_PORTS {
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
                // Ordre des champs à l'écran : nom, programme, ports.
                FOCUS = match (FOCUS, DRAFT.proto == 3) {
                    (0, _) => 2,
                    (2, false) => 1,
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
        0x0D if SELECTED < RULE_COUNT => open_editor(SELECTED, false),
        0x08 if PAGE == PAGE_PROFILE => go(PAGE_OVERVIEW),
        _ => return false,
    }
    true
}

pub unsafe fn tick(_f: &Frame) -> bool {
    !EDITING && GetTickCount().wrapping_sub(LAST_REFRESH) > 2000
}
