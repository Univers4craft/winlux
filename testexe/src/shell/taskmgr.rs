//! Gestionnaire des tâches façon Windows 11 : Processus (groupés, triables,
//! recherche, fin de tâche, priorité), Performances (graphiques du
//! processeur, de la mémoire, de chaque disque, carte réseau et GPU sur
//! 60 mesures), Historique des applications, Applications de démarrage,
//! Utilisateurs, Détails, Services et Paramètres (vitesse d'actualisation).
//! Les mesures viennent de `sysmon` (lecture réelle de /proc et /sys).

use crate::admin_ui::{self as ui, Field, Hits};
use crate::gfx::*;
use crate::sysmon::{self as sm, Proc, HIST};
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

// ---------- Onglets ----------

const TAB_PROCESSES: usize = 0;
const TAB_PERF: usize = 1;
const TAB_HISTORY: usize = 2;
const TAB_STARTUP: usize = 3;
const TAB_USERS: usize = 4;
const TAB_DETAILS: usize = 5;
const TAB_SERVICES: usize = 6;
const TAB_SETTINGS: usize = 7;
const TAB_COUNT: usize = 8;

const TAB_NAMES: [Tr4; TAB_COUNT] = [
    [b"Processus", b"Processes", b"Procesos", b"Prozesse"],
    [b"Performances", b"Performance", b"Rendimiento", b"Leistung"],
    [
        b"Historique des applications",
        b"App history",
        b"Historial de aplicaciones",
        b"App-Verlauf",
    ],
    [
        b"Applications de d\xE9marrage",
        b"Startup apps",
        b"Aplicaciones de arranque",
        b"Autostart-Apps",
    ],
    [b"Utilisateurs", b"Users", b"Usuarios", b"Benutzer"],
    [b"D\xE9tails", b"Details", b"Detalles", b"Details"],
    [b"Services", b"Services", b"Servicios", b"Dienste"],
    [
        b"Param\xE8tres",
        b"Settings",
        b"Configuraci\xF3n",
        b"Einstellungen",
    ],
];

// ---------- Identifiants des zones cliquables ----------

const ID_NAV: u16 = 1; // + onglet
const ID_HAMBURGER: u16 = 20;
const ID_SEARCH: u16 = 30;
const ID_END: u16 = 31;
const ID_RUN: u16 = 32;
const ID_STARTUP_TOGGLE: u16 = 33;
const ID_HISTORY_CLEAR: u16 = 34;
const ID_COL: u16 = 40; // + colonne
const ID_PERF: u16 = 100; // + ressource
const ID_DLG_OK: u16 = 200;
const ID_DLG_CANCEL: u16 = 201;
const ID_DLG_INPUT: u16 = 202;
const ID_MENU: u16 = 300; // + entrée
const ID_SPEED: u16 = 400; // + vitesse
const ID_ROW: u16 = 1000; // + ligne visible

static mut HITS: Hits = Hits::new();
static mut HOVER: u16 = 0;
static mut TAB: usize = TAB_PROCESSES;
static mut NAV_EXPANDED: bool = false;
static mut SEARCH: Field<40> = Field::new();
static mut SEARCH_FOCUS: bool = false;
/// Colonne de tri (-1 = ordre naturel) et sens.
static mut SORT_COL: i32 = -1;
static mut SORT_DESC: bool = true;
static mut SCROLL: usize = 0;
static mut VISIBLE_ROWS: usize = 10;
static mut SELECTED_PID: u32 = 0;
static mut SELECTED_ITEM: usize = usize::MAX;
static mut PERF_SEL: usize = 0;

/// Boîte de dialogue ouverte.
#[derive(Clone, Copy, PartialEq)]
enum Dialog {
    None,
    EndTask(u32),
    Run,
}
static mut DIALOG: Dialog = Dialog::None;
static mut RUN: Field<160> = Field::new();

/// Menu contextuel d'un processus (clic droit).
static mut MENU_PID: u32 = 0;
static mut MENU_X: i32 = 0;
static mut MENU_Y: i32 = 0;

/// Message affiché quelques secondes en bas de la fenêtre.
static mut MESSAGE: Field<160> = Field::new();
static mut MESSAGE_AT: u32 = 0;

/// Ligne visible -> PID ; `GROUP` et plus marque un en-tête de groupe.
const MAX_ROWS: usize = 120;
const GROUP: u32 = 0xFFFF_FF00;
static mut ROWS: [u32; MAX_ROWS] = [0; MAX_ROWS];
static mut ROW_COUNT: usize = 0;

// ---------- Couleurs des ressources (Windows 11) ----------

const C_CPU: u32 = rgb(0x11, 0x7D, 0xBB);
const C_MEM: u32 = rgb(0x8B, 0x12, 0xAE);
const C_DISK: u32 = rgb(0x4D, 0xA6, 0x0C);
const C_NET: u32 = rgb(0xA7, 0x4F, 0x01);
const C_GPU: u32 = rgb(0x26, 0x8C, 0x8C);

// ---------- Historique des applications ----------

const MAX_APPS: usize = 64;
#[derive(Clone, Copy)]
struct AppUse {
    name: [u8; 48],
    len: usize,
    ticks: u64,
    launches: u32,
    running: bool,
    exe: bool,
}
const NO_APP: AppUse = AppUse {
    name: [0; 48],
    len: 0,
    ticks: 0,
    launches: 0,
    running: false,
    exe: false,
};
static mut APPS: [AppUse; MAX_APPS] = [NO_APP; MAX_APPS];
static mut APP_COUNT: usize = 0;

// GPU : un onglet par carte qui fournit au moins une mesure (au plus deux).
const MAX_GPU_TABS: usize = 2;
static mut GPU_HIST: [[u32; HIST]; MAX_GPU_TABS] = [[0; HIST]; MAX_GPU_TABS];
static mut GPU_TABS: [usize; MAX_GPU_TABS] = [0; MAX_GPU_TABS];
static mut GPU_TAB_COUNT: usize = 0;

static mut STARTUP_LOADED: bool = false;
static mut SERVICES_AT: u32 = 0;

// ---------- Texte ----------

/// Petit tampon pour composer une ligne (nombres, unités).
struct T {
    b: [u8; 128],
    n: usize,
}

impl T {
    const fn new() -> T {
        T { b: [0; 128], n: 0 }
    }
    fn s(&mut self, s: &[u8]) -> &mut T {
        let k = s.len().min(self.b.len() - self.n);
        self.b[self.n..self.n + k].copy_from_slice(&s[..k]);
        self.n += k;
        self
    }
    fn u(&mut self, v: u64) -> &mut T {
        let mut d = [0u8; 20];
        let s = sm::fmt_u64(&mut d, v);
        self.s(s)
    }
    /// Nombre au dixième près (`tenths` = valeur × 10).
    fn dec(&mut self, tenths: u64) -> &mut T {
        self.u(tenths / 10);
        let sep: &[u8] = if ui_lang() == 1 { b"." } else { b"," };
        self.s(sep).u(tenths % 10)
    }
    fn get(&self) -> &[u8] {
        &self.b[..self.n]
    }
}

/// Pourcentage depuis une valeur en pour mille : « 12,3 % ».
fn pct(t: &mut T, permille: u32) -> &[u8] {
    t.n = 0;
    t.dec(permille as u64).s(b" %");
    t.get()
}

/// Pourcentage entier : « 12 % ».
fn pct0(t: &mut T, permille: u32) -> &[u8] {
    t.n = 0;
    t.u(((permille + 5) / 10) as u64).s(b" %");
    t.get()
}

const U_KB: Tr4 = [b" Ko", b" KB", b" KB", b" KB"];
const U_MB: Tr4 = [b" Mo", b" MB", b" MB", b" MB"];
const U_GB: Tr4 = [b" Go", b" GB", b" GB", b" GB"];
const U_TB: Tr4 = [b" To", b" TB", b" TB", b" TB"];

/// Taille en Ko vers « 512 Ko », « 12,4 Mo », « 3,1 Go ».
fn size_kb(t: &mut T, kb: u64) -> &[u8] {
    t.n = 0;
    if kb < 1024 {
        t.u(kb).s(tr4(&U_KB));
    } else if kb < 1024 * 1024 {
        t.dec(kb * 10 / 1024).s(tr4(&U_MB));
    } else if kb < 1024 * 1024 * 1024 {
        t.dec(kb * 10 / (1024 * 1024)).s(tr4(&U_GB));
    } else {
        t.dec(kb * 10 / (1024 * 1024 * 1024)).s(tr4(&U_TB));
    }
    t.get()
}

/// Débit d'octets par seconde : « 0 Mo/s », « 120 Ko/s », « 1,5 Mo/s ».
fn rate(t: &mut T, bps: u64) -> &[u8] {
    t.n = 0;
    if bps == 0 {
        t.s(b"0").s(tr4(&U_MB));
    } else if bps < 1024 * 1024 {
        t.u(bps.div_ceil(1024)).s(tr4(&U_KB));
    } else {
        t.dec(bps * 10 / (1024 * 1024)).s(tr4(&U_MB));
    }
    t.s(b"/s");
    t.get()
}

/// Débit réseau en bits par seconde.
fn bits(t: &mut T, bps: u64) -> &[u8] {
    t.n = 0;
    let (k, m, g): (&[u8], &[u8], &[u8]) = match ui_lang() {
        0 => (b" Kbits/s", b" Mbits/s", b" Gbits/s"),
        3 => (b" KBit/s", b" MBit/s", b" GBit/s"),
        _ => (b" Kbps", b" Mbps", b" Gbps"),
    };
    if bps < 1_000_000 {
        t.u(bps / 1000).s(k);
    } else if bps < 1_000_000_000 {
        t.dec(bps / 100_000).s(m);
    } else {
        t.dec(bps / 100_000_000).s(g);
    }
    t.get()
}

/// Durée « j:hh:mm:ss » comme Windows.
fn duration(t: &mut T, secs: u64) -> &[u8] {
    t.n = 0;
    let (d, h, m, s) = (secs / 86400, secs / 3600 % 24, secs / 60 % 60, secs % 60);
    t.u(d).s(b":");
    for (i, v) in [h, m, s].into_iter().enumerate() {
        if v < 10 {
            t.s(b"0");
        }
        t.u(v);
        if i < 2 {
            t.s(b":");
        }
    }
    t.get()
}

fn mhz(t: &mut T, mhz: u32) -> &[u8] {
    t.n = 0;
    t.dec(mhz as u64 / 100).s(b" GHz");
    t.get()
}

fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    needle.is_empty()
        || hay
            .windows(needle.len())
            .any(|w| w.eq_ignore_ascii_case(needle))
}

unsafe fn say(s: &[u8]) {
    MESSAGE.set(s);
    MESSAGE_AT = GetTickCount();
}

fn yes_no(v: bool) -> &'static [u8] {
    if v {
        tr4(&[b"Oui", b"Yes", b"S\xED", b"Ja"])
    } else {
        tr4(&[b"Non", b"No", b"No", b"Nein"])
    }
}

// ---------- Actualisation ----------

unsafe fn refresh() -> bool {
    if !sm::refresh() {
        return false;
    }
    refresh_gpu_tabs();
    record_history();
    true
}

/// Garde-fou : un GPU sans aucune mesure exposée n'a pas d'onglet.
unsafe fn refresh_gpu_tabs() {
    crate::gpuinfo::refresh(false);
    GPU_TAB_COUNT = 0;
    let pos = (sm::HIST_POS + HIST - 1) % HIST;
    for i in 0..crate::gpuinfo::count() {
        if GPU_TAB_COUNT < MAX_GPU_TABS && crate::gpuinfo::has_metrics(i) {
            GPU_TABS[GPU_TAB_COUNT] = i;
            GPU_HIST[GPU_TAB_COUNT][pos] =
                crate::gpuinfo::num(i, b"load").unwrap_or(0).min(100) * 10;
            GPU_TAB_COUNT += 1;
        }
    }
}

/// Cumule le temps processeur des applications de l'utilisateur.
unsafe fn record_history() {
    let apps = &mut *core::ptr::addr_of_mut!(APPS);
    for a in apps[..APP_COUNT].iter_mut() {
        a.running = false;
    }
    let procs = &*core::ptr::addr_of!(sm::PROCS);
    for p in procs[..sm::COUNT].iter() {
        if p.kind == sm::KIND_KERNEL || p.uid != sm::MY_UID {
            continue;
        }
        let name = p.name();
        let i = match (0..APP_COUNT).find(|&i| apps[i].name[..apps[i].len] == *name) {
            Some(i) => i,
            None if APP_COUNT < MAX_APPS => {
                let a = &mut apps[APP_COUNT];
                *a = NO_APP;
                a.len = name.len().min(48);
                a.name[..a.len].copy_from_slice(&name[..a.len]);
                a.exe = p.kind == sm::KIND_EXE;
                // Déjà lancé avant l'ouverture : compte pour un lancement.
                a.launches = if p.fresh { 0 } else { 1 };
                APP_COUNT += 1;
                APP_COUNT - 1
            }
            None => continue,
        };
        let a = &mut apps[i];
        a.ticks += p.dticks;
        a.running = true;
        if p.fresh {
            a.launches += 1;
        }
    }
}

// ---------- Groupes et tri des processus ----------

/// 0 = Applications, 1 = arrière-plan, 2 = processus système, 3 = noyau.
unsafe fn group_of(p: &Proc) -> usize {
    match p.kind {
        sm::KIND_KERNEL => 3,
        sm::KIND_EXE | sm::KIND_SHELL if p.uid == sm::MY_UID => 0,
        _ if p.uid == sm::MY_UID => 1,
        _ => 2,
    }
}

const GROUP_NAMES: [Tr4; 3] = [
    [b"Applications", b"Apps", b"Aplicaciones", b"Apps"],
    [
        b"Processus en arri\xE8re-plan",
        b"Background processes",
        b"Procesos en segundo plano",
        b"Hintergrundprozesse",
    ],
    [
        b"Processus syst\xE8me",
        b"System processes",
        b"Procesos del sistema",
        b"Systemprozesse",
    ],
];

static mut ORDER: [u16; sm::MAX_PROCS] = [0; sm::MAX_PROCS];

fn cmp_name(a: &Proc, b: &Proc) -> core::cmp::Ordering {
    let (x, y) = (a.name(), b.name());
    for i in 0..x.len().min(y.len()) {
        let c = x[i].to_ascii_lowercase().cmp(&y[i].to_ascii_lowercase());
        if c != core::cmp::Ordering::Equal {
            return c;
        }
    }
    x.len().cmp(&y.len()).then(a.pid.cmp(&b.pid))
}

/// Ordonne dans `ORDER` les processus retenus par `filter` et la recherche.
/// La colonne 0 trie par nom ; les autres par `col_key`.
unsafe fn sorted(filter: &dyn Fn(&Proc) -> bool, col_key: &dyn Fn(&Proc, i32) -> u64) -> usize {
    let procs = &*core::ptr::addr_of!(sm::PROCS);
    let order = &mut *core::ptr::addr_of_mut!(ORDER);
    let search = (*core::ptr::addr_of!(SEARCH)).get();
    let mut n = 0;
    for (i, p) in procs[..sm::COUNT].iter().enumerate() {
        if filter(p) && contains_ci(p.name(), search) {
            order[n] = i as u16;
            n += 1;
        }
    }
    let (col, desc) = (SORT_COL, SORT_DESC);
    order[..n].sort_unstable_by(|&a, &b| {
        let (pa, pb) = (&procs[a as usize], &procs[b as usize]);
        let o = if col <= 0 {
            cmp_name(pa, pb)
        } else {
            col_key(pa, col)
                .cmp(&col_key(pb, col))
                .then_with(|| cmp_name(pa, pb).reverse())
        };
        if col >= 0 && desc {
            o.reverse()
        } else {
            o
        }
    });
    n
}

// ---------- Dessin commun ----------

unsafe fn nav_icon(g: &mut Gfx, tab: usize, x: i32, y: i32, color: u32) {
    match tab {
        TAB_PROCESSES => {
            for k in 0..3 {
                fill(g.hdc, g.b, x, y + 2 + k * 6, x + 4, y + 6 + k * 6, color);
                fill(
                    g.hdc,
                    g.b,
                    x + 6,
                    y + 3 + k * 6,
                    x + 18,
                    y + 5 + k * 6,
                    color,
                );
            }
        }
        TAB_PERF => {
            let pts = [(0, 14), (5, 8), (9, 12), (13, 3), (18, 9)];
            for w in pts.windows(2) {
                line(
                    g.hdc,
                    g.b,
                    x + w[0].0,
                    y + w[0].1,
                    x + w[1].0,
                    y + w[1].1,
                    2,
                    color,
                );
            }
            fill(g.hdc, g.b, x, y + 17, x + 18, y + 18, color);
        }
        TAB_HISTORY => glyph(g.hdc, g.b, g.f, Glyph::Clock, x, y, 18),
        TAB_STARTUP => {
            // Jauge : demi-cercle et aiguille.
            ring(g.hdc, g.b, x, y + 2, x + 18, y + 20, 2, color, glyph_bg());
            fill(g.hdc, g.b, x, y + 13, x + 18, y + 20, glyph_bg());
            fill(g.hdc, g.b, x + 1, y + 13, x + 17, y + 15, color);
            line(g.hdc, g.b, x + 9, y + 12, x + 13, y + 6, 2, color);
        }
        TAB_USERS => {
            ellipse(g.hdc, g.b, x + 5, y + 1, x + 13, y + 9, color);
            round_rect(g.hdc, g.b, x + 2, y + 10, x + 16, y + 18, 6, color);
        }
        TAB_DETAILS => glyph(g.hdc, g.b, g.f, Glyph::Details, x, y, 18),
        TAB_SERVICES => glyph(g.hdc, g.b, g.f, Glyph::Tools, x, y, 18),
        _ => glyph(g.hdc, g.b, g.f, Glyph::Settings, x, y, 18),
    }
}

unsafe fn tab_visible(tab: usize) -> bool {
    tab != TAB_SERVICES || sm::services_available()
}

unsafe fn draw_nav(g: &mut Gfx, h: &mut Hits, r: &Rect) -> i32 {
    let p = pal();
    let w = if NAV_EXPANDED { 220 } else { 52 };
    fill(g.hdc, g.b, r.left, r.top, r.left + w, r.bottom, p.mica);
    // Bouton menu (trois barres)
    let hr = rect(r.left + 6, r.top + 6, r.left + 46, r.top + 40);
    if h.add(hr, ID_HAMBURGER) {
        round_rect(g.hdc, g.b, hr.left, hr.top, hr.right, hr.bottom, 4, p.hover);
    }
    for k in 0..3 {
        let y = r.top + 15 + k * 6;
        fill(g.hdc, g.b, r.left + 17, y, r.left + 35, y + 2, p.text);
    }
    let mut y = r.top + 48;
    for tab in 0..TAB_COUNT {
        if !tab_visible(tab) {
            continue;
        }
        let iy = if tab == TAB_SETTINGS {
            r.bottom - 48
        } else {
            y
        };
        let ir = rect(r.left + 4, iy, r.left + w - 4, iy + 40);
        let hover = h.add(ir, ID_NAV + tab as u16);
        let active = TAB == tab;
        let bg = if active {
            p.selected
        } else if hover {
            p.hover
        } else {
            p.mica
        };
        if active || hover {
            round_rect(g.hdc, g.b, ir.left, ir.top, ir.right, ir.bottom, 4, bg);
        }
        if active {
            round_rect(
                g.hdc,
                g.b,
                ir.left,
                ir.top + 11,
                ir.left + 3,
                ir.bottom - 11,
                2,
                p.accent,
            );
        }
        set_glyph_bg(Some(bg));
        nav_icon(
            g,
            tab,
            r.left + 17,
            iy + 11,
            if active { p.accent } else { p.text },
        );
        set_glyph_bg(None);
        if NAV_EXPANDED {
            text_fit(
                g.hdc,
                g.f.body,
                13,
                p.text,
                r.left + 50,
                iy + 12,
                tr4(&TAB_NAMES[tab]),
                w - 60,
            );
        }
        if tab != TAB_SETTINGS {
            y += 44;
        }
    }
    w
}

unsafe fn page_title(g: &mut Gfx, x: i32, y: i32, title: &[u8]) {
    text(g.hdc, g.f.large, pal().text, x + 20, y + 16, title);
}

/// En-tête de colonnes triables.  `cols` : (libellé, largeur, total affiché
/// au-dessus du libellé comme dans Windows 11).  La dernière colonne prend
/// la place restante.
unsafe fn columns(
    g: &mut Gfx,
    h: &mut Hits,
    x: i32,
    y: i32,
    right: i32,
    cols: &[(&[u8], i32, &[u8])],
) -> i32 {
    let p = pal();
    let with_totals = cols.iter().any(|c| !c.2.is_empty());
    let hh = if with_totals { 46 } else { 30 };
    let mut cx = x;
    for (i, (label, w, total)) in cols.iter().enumerate() {
        let cw = if i + 1 == cols.len() { right - cx } else { *w };
        let r = rect(cx, y, cx + cw, y + hh);
        if h.add(r, ID_COL + i as u16) {
            fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, p.hover);
        }
        if i > 0 {
            fill(g.hdc, g.b, cx, y + 4, cx + 1, y + hh - 4, p.border);
        }
        if !total.is_empty() {
            let tw = text_width(total, 15);
            text(g.hdc, g.f.title, p.text, cx + cw - tw - 10, y + 4, total);
        }
        let ly = if with_totals { y + 25 } else { y + 8 };
        if !total.is_empty() {
            let lw = text_width(label, 12).min(cw - 20);
            text_fit(
                g.hdc,
                g.f.caption,
                12,
                p.text2,
                cx + cw - lw - 10,
                ly,
                label,
                cw - 20,
            );
        } else {
            text_fit(g.hdc, g.f.caption, 12, p.text2, cx + 10, ly, label, cw - 24);
        }
        if SORT_COL == i as i32 {
            // Chevron de tri centré en haut de la colonne.
            let ax = cx + cw / 2;
            let (a, b) = if SORT_DESC {
                (y + 2, y + 6)
            } else {
                (y + 6, y + 2)
            };
            line(g.hdc, g.b, ax - 4, a, ax, b, 1, p.text2);
            line(g.hdc, g.b, ax, b, ax + 4, a, 1, p.text2);
        }
        cx += cw;
    }
    fill(g.hdc, g.b, x, y + hh, right, y + hh + 1, p.border);
    y + hh + 1
}

/// Teinte « chaleur » d'une cellule : plus l'usage est fort, plus elle est
/// orangée, comme les colonnes de Windows 11.
unsafe fn heat(g: &mut Gfx, x: i32, y: i32, w: i32, hgt: i32, permille: u32) {
    let p = pal();
    let level = match permille {
        900.. => 6,
        500.. => 4,
        100.. => 2,
        5.. => 1,
        _ => 0,
    };
    if level == 0 {
        return;
    }
    let base = if level >= 4 {
        rgb(0xF2, 0x8C, 0x28)
    } else {
        rgb(0xFF, 0xD2, 0x4A)
    };
    fill(
        g.hdc,
        g.b,
        x + 1,
        y,
        x + w,
        y + hgt,
        mix(p.window, base, level + 1, 10),
    );
}

unsafe fn right_text(g: &mut Gfx, x: i32, w: i32, y: i32, s: &[u8], color: u32) {
    let tw = text_width(s, 13);
    text(g.hdc, g.f.body, color, x + w - tw - 10, y, s);
}

unsafe fn scrollbar(g: &mut Gfx, x: i32, y: i32, h: i32, total: usize, visible: usize) {
    if total <= visible || h < 20 {
        return;
    }
    let thumb = ((h as usize * visible / total) as i32).max(24);
    let top = y + ((h - thumb) as usize * SCROLL.min(total - visible) / (total - visible)) as i32;
    round_rect(g.hdc, g.b, x, top, x + 4, top + thumb, 2, pal().text3);
}

/// Bouton de commande aligné à droite ; renvoie sa largeur.
unsafe fn command(
    g: &mut Gfx,
    h: &mut Hits,
    right: i32,
    y: i32,
    id: u16,
    s: &[u8],
    enabled: bool,
) -> i32 {
    let w = ui::button_w(s);
    ui::button(
        g,
        h,
        rect(right - w, y, right, y + 32),
        id,
        s,
        false,
        enabled,
    );
    w
}

/// Fond d'une ligne de tableau (sélection, survol).
unsafe fn row_bg(g: &mut Gfx, r: &Rect, selected: bool, hover: bool) {
    let p = pal();
    if selected {
        fill(
            g.hdc,
            g.b,
            r.left,
            r.top,
            r.right,
            r.bottom,
            mix(p.selected, p.window, 3, 10),
        );
        fill(g.hdc, g.b, r.left, r.top, r.left + 3, r.bottom, p.accent);
    } else if hover {
        fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, p.hover);
    }
}

// ---------- Point d'entrée du dessin ----------

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    refresh();
    let h = &mut *core::ptr::addr_of_mut!(HITS);
    h.clear();
    let r = f.client;
    let p = pal();
    fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, p.mica);
    if !tab_visible(TAB) {
        TAB = TAB_PROCESSES;
    }
    let nav = draw_nav(g, h, &r);
    let cx = r.left + nav;
    let content = rect(cx, r.top, r.right, r.bottom);
    fill(g.hdc, g.b, cx, r.top, r.right, r.bottom, p.window);
    fill(g.hdc, g.b, cx, r.top, cx + 1, r.bottom, p.border);
    match TAB {
        TAB_PROCESSES => draw_processes(g, h, &content),
        TAB_PERF => draw_performance(g, h, &content),
        TAB_HISTORY => draw_history(g, h, &content),
        TAB_STARTUP => draw_startup(g, h, &content),
        TAB_USERS => draw_users(g, h, &content),
        TAB_DETAILS => draw_details(g, h, &content),
        TAB_SERVICES => draw_services(g, h, &content),
        _ => draw_settings(g, h, &content),
    }
    // Message temporaire
    let message = (*core::ptr::addr_of!(MESSAGE)).get();
    if !message.is_empty() && GetTickCount().wrapping_sub(MESSAGE_AT) < 6000 {
        let w = (text_width(message, 13) + 32).min(content.right - content.left - 32);
        let x = content.left + (content.right - content.left - w) / 2;
        let y = r.bottom - 52;
        panel(g.hdc, g.b, x, y, x + w, y + 36, 6, p.surface2, p.border);
        text_fit(g.hdc, g.f.body, 13, p.text, x + 16, y + 10, message, w - 32);
    }
    if MENU_PID != 0 {
        draw_menu(g, h, &r);
    }
    match DIALOG {
        Dialog::EndTask(pid) => draw_end_dialog(g, h, &r, pid),
        Dialog::Run => draw_run_dialog(g, h, &r),
        Dialog::None => {}
    }
}

// ---------- Processus ----------

fn proc_col_key(p: &Proc, col: i32) -> u64 {
    match col {
        1 => p.state as u64,
        2 => p.cpu as u64,
        3 => p.mem_kb as u64,
        _ if p.io_bps == sm::UNKNOWN => 0,
        _ => p.io_bps as u64,
    }
}

fn status_text(p: &Proc) -> &'static [u8] {
    match p.state {
        b'T' | b't' => tr4(&[b"Suspendu", b"Suspended", b"Suspendido", b"Angehalten"]),
        b'Z' => tr4(&[b"Termin\xE9", b"Ended", b"Finalizado", b"Beendet"]),
        _ => b"",
    }
}

unsafe fn proc_glyph(g: &mut Gfx, p: &Proc, x: i32, y: i32) {
    let kind = match p.kind {
        sm::KIND_EXE => Glyph::Exe,
        sm::KIND_SHELL => Glyph::Start,
        _ if p.uid == sm::MY_UID => Glyph::Terminal,
        _ => Glyph::Computer,
    };
    glyph(g.hdc, g.b, g.f, kind, x, y, 16);
}

unsafe fn draw_processes(g: &mut Gfx, h: &mut Hits, c: &Rect) {
    let p = pal();
    let (x, right) = (c.left, c.right);
    page_title(g, x, c.top, tr4(&TAB_NAMES[TAB_PROCESSES]));
    // Barre de commandes : recherche, nouvelle tâche, fin de tâche.
    let selected = SELECTED_PID != 0 && sm::proc_by_pid(SELECTED_PID).is_some();
    let mut bx = right - 20;
    bx -= command(
        g,
        h,
        bx,
        c.top + 12,
        ID_END,
        tr4(&[
            b"Fin de t\xE2che",
            b"End task",
            b"Finalizar tarea",
            b"Task beenden",
        ]),
        selected,
    ) + 8;
    bx -= command(
        g,
        h,
        bx,
        c.top + 12,
        ID_RUN,
        tr4(&[
            b"Ex\xE9cuter une nouvelle t\xE2che",
            b"Run new task",
            b"Ejecutar nueva tarea",
            b"Neuen Task ausf\xFChren",
        ]),
        true,
    ) + 12;
    let search_w = (bx - x - 160).clamp(110, 300);
    ui::input(
        g,
        h,
        rect(bx - search_w, c.top + 12, bx, c.top + 44),
        ID_SEARCH,
        (*core::ptr::addr_of!(SEARCH)).get(),
        SEARCH_FOCUS,
        tr4(&[
            b"Rechercher un nom",
            b"Type a name",
            b"Buscar un nombre",
            b"Namen eingeben",
        ]),
    );

    // Colonnes avec totaux
    let w = right - x;
    let mut t_cpu = T::new();
    let mut t_mem = T::new();
    let mut t_disk = T::new();
    let disks = &*core::ptr::addr_of!(sm::DISKS);
    let disk_active = disks[..sm::DISK_COUNT]
        .iter()
        .map(|d| d.active)
        .max()
        .unwrap_or(0);
    let name_w = (w * 36 / 100).max(180);
    let col_w = (w - name_w) / 4;
    let cols: [(&[u8], i32, &[u8]); 5] = [
        (tr4(&[b"Nom", b"Name", b"Nombre", b"Name"]), name_w, b""),
        (
            tr4(&[b"Statut", b"Status", b"Estado", b"Status"]),
            col_w,
            b"",
        ),
        (
            tr4(&[b"Processeur", b"CPU", b"CPU", b"CPU"]),
            col_w,
            pct0(&mut t_cpu, sm::CPU_LOAD),
        ),
        (
            tr4(&[b"M\xE9moire", b"Memory", b"Memoria", b"Arbeitsspeicher"]),
            col_w,
            pct0(&mut t_mem, sm::mem_used_permille()),
        ),
        (
            tr4(&[b"Disque", b"Disk", b"Disco", b"Datentr\xE4ger"]),
            col_w,
            pct0(&mut t_disk, disk_active),
        ),
    ];
    let top = columns(g, h, x, c.top + 58, right, &cols);

    // Lignes : en-tête de chaque groupe puis ses processus.
    let n = sorted(&|p| p.kind != sm::KIND_KERNEL, &proc_col_key);
    let order = &*core::ptr::addr_of!(ORDER);
    let procs = &*core::ptr::addr_of!(sm::PROCS);
    let mut counts = [0usize; 4];
    for &i in &order[..n] {
        counts[group_of(&procs[i as usize])] += 1;
    }
    let rows = &mut *core::ptr::addr_of_mut!(ROWS);
    let row_h = 32;
    VISIBLE_ROWS = (((c.bottom - top - 8) / row_h).max(1) as usize).min(MAX_ROWS);
    let total = n + counts[..3].iter().filter(|&&k| k > 0).count();
    if SCROLL > total.saturating_sub(VISIBLE_ROWS) {
        SCROLL = total.saturating_sub(VISIBLE_ROWS);
    }
    let mut all = 0usize;
    let mut drawn = 0usize;
    let mut y = top;
    for gi in 0..3 {
        if counts[gi] == 0 {
            continue;
        }
        let members = order[..n]
            .iter()
            .filter(|&&i| group_of(&procs[i as usize]) == gi)
            .map(|&i| procs[i as usize].pid);
        for id in core::iter::once(GROUP + gi as u32).chain(members) {
            if all >= SCROLL && drawn < VISIBLE_ROWS {
                rows[drawn] = id;
                if id >= GROUP {
                    let mut tt = T::new();
                    tt.s(tr4(&GROUP_NAMES[gi]))
                        .s(b" (")
                        .u(counts[gi] as u64)
                        .s(b")");
                    text(g.hdc, g.f.semibold, p.text, x + 20, y + 10, tt.get());
                } else if let Some(pr) = sm::proc_by_pid(id) {
                    draw_proc_row(g, h, pr, x, y, right, name_w, col_w, drawn);
                }
                y += row_h;
                drawn += 1;
            }
            all += 1;
        }
    }
    ROW_COUNT = drawn;
    scrollbar(
        g,
        right - 7,
        top + 2,
        c.bottom - top - 6,
        total,
        VISIBLE_ROWS,
    );
    if n == 0 {
        text(
            g.hdc,
            g.f.body,
            p.text2,
            x + 20,
            top + 16,
            tr4(&[
                b"Aucun processus ne correspond.",
                b"No matching process.",
                b"Ning\xFAn proceso coincide.",
                b"Kein passender Prozess.",
            ]),
        );
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn draw_proc_row(
    g: &mut Gfx,
    h: &mut Hits,
    pr: &Proc,
    x: i32,
    y: i32,
    right: i32,
    name_w: i32,
    col_w: i32,
    slot: usize,
) {
    let p = pal();
    let row_h = 32;
    let r = rect(x, y, right - 10, y + row_h);
    let hover = h.add(r, ID_ROW + slot as u16);
    let cx = x + name_w;
    heat(g, cx + col_w, y, col_w, row_h, pr.cpu);
    let mem_pm = if sm::MEM_TOTAL_KB > 0 {
        (pr.mem_kb as u64 * 1000 / sm::MEM_TOTAL_KB) as u32
    } else {
        0
    };
    heat(g, cx + col_w * 2, y, col_w, row_h, mem_pm * 4);
    row_bg(g, &r, SELECTED_PID == pr.pid, hover);
    proc_glyph(g, pr, x + 36, y + 8);
    let ty = y + 9;
    text_fit(
        g.hdc,
        g.f.body,
        13,
        p.text,
        x + 60,
        ty,
        pr.name(),
        name_w - 70,
    );
    text_fit(
        g.hdc,
        g.f.body,
        13,
        p.text2,
        cx + 10,
        ty,
        status_text(pr),
        col_w - 16,
    );
    let mut t = T::new();
    right_text(g, cx + col_w, col_w, ty, pct(&mut t, pr.cpu), p.text);
    right_text(
        g,
        cx + col_w * 2,
        col_w,
        ty,
        size_kb(&mut t, pr.mem_kb as u64),
        p.text,
    );
    let disk: &[u8] = if pr.io_bps == sm::UNKNOWN {
        b"\x97"
    } else {
        rate(&mut t, pr.io_bps as u64)
    };
    right_text(
        g,
        cx + col_w * 3,
        right - cx - col_w * 3 - 10,
        ty,
        disk,
        p.text,
    );
}

// ---------- Menu contextuel et boîtes de dialogue ----------

const PRIORITY_NAMES: [Tr4; 5] = [
    [b"Haute", b"High", b"Alta", b"Hoch"],
    [
        b"Sup\xE9rieure \xE0 la normale",
        b"Above normal",
        b"Superior a la normal",
        b"H\xF6her als normal",
    ],
    [b"Normale", b"Normal", b"Normal", b"Normal"],
    [
        b"Inf\xE9rieure \xE0 la normale",
        b"Below normal",
        b"Inferior a la normal",
        b"Niedriger als normal",
    ],
    [b"Basse", b"Low", b"Baja", b"Niedrig"],
];

/// Entrées : 0 = fin de tâche, 1..=5 = priorités, 6 = aller aux détails.
unsafe fn draw_menu(g: &mut Gfx, h: &mut Hits, client: &Rect) {
    let p = pal();
    let Some(pr) = sm::proc_by_pid(MENU_PID) else {
        MENU_PID = 0;
        return;
    };
    let current = sm::priority_index(pr.nice);
    let items = if TAB == TAB_PROCESSES { 7 } else { 6 };
    let w = 260;
    let ht = 12 + items as i32 * 32 + 26;
    let x = MENU_X.min(client.right - w - 4).max(client.left);
    let y = MENU_Y.min(client.bottom - ht - 4).max(client.top);
    // Un clic hors du menu le ferme.
    h.add(*client, ID_MENU + 50);
    panel(g.hdc, g.b, x, y, x + w, y + ht, 8, p.surface2, p.border);
    let mut iy = y + 6;
    for i in 0..items {
        if i == 1 {
            text(
                g.hdc,
                g.f.caption,
                p.text3,
                x + 16,
                iy + 6,
                tr4(&[b"Priorit\xE9", b"Priority", b"Prioridad", b"Priorit\xE4t"]),
            );
            iy += 26;
        }
        let r = rect(x + 4, iy, x + w - 4, iy + 30);
        if h.add(r, ID_MENU + i as u16) {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, p.hover);
        }
        let label: &[u8] = match i {
            0 => tr4(&[
                b"Fin de t\xE2che",
                b"End task",
                b"Finalizar tarea",
                b"Task beenden",
            ]),
            1..=5 => tr4(&PRIORITY_NAMES[i - 1]),
            _ => tr4(&[
                b"Acc\xE9der aux d\xE9tails",
                b"Go to details",
                b"Ir a detalles",
                b"Zu Details wechseln",
            ]),
        };
        if (1..=5).contains(&i) && current == i - 1 {
            ellipse(g.hdc, g.b, x + 18, iy + 12, x + 24, iy + 18, p.accent);
        }
        text(g.hdc, g.f.body, p.text, x + 36, iy + 7, label);
        iy += 32;
    }
}

unsafe fn dialog_buttons(g: &mut Gfx, h: &mut Hits, d: &Rect, ok: &[u8], ok_enabled: bool) {
    let by = d.bottom - 52;
    let bw = (d.right - d.left - 60) / 2;
    ui::button(
        g,
        h,
        rect(d.left + 24, by, d.left + 24 + bw, by + 32),
        ID_DLG_OK,
        ok,
        true,
        ok_enabled,
    );
    ui::button(
        g,
        h,
        rect(d.right - 24 - bw, by, d.right - 24, by + 32),
        ID_DLG_CANCEL,
        tr4(&[b"Annuler", b"Cancel", b"Cancelar", b"Abbrechen"]),
        false,
        true,
    );
}

unsafe fn draw_end_dialog(g: &mut Gfx, h: &mut Hits, c: &Rect, pid: u32) {
    let p = pal();
    let mut title = T::new();
    title.s(tr4(&[
        b"Terminer \xAB ",
        b"End \x93",
        b"\xBFFinalizar \xAB",
        b"\x84",
    ]));
    if let Some(pr) = sm::proc_by_pid(pid) {
        title.s(pr.name());
    }
    title.s(tr4(&[b" \xBB ?", b"\x94?", b"\xBB?", b"\x93 beenden?"]));
    let d = ui::dialog(g, h, c, 540, 200, title.get());
    let body = tr4(&[
        b"Les donn\xE9es non enregistr\xE9es de ce programme seront perdues.",
        b"Any unsaved data in this program will be lost.",
        b"Se perder\xE1n los datos no guardados de este programa.",
        b"Nicht gespeicherte Daten dieses Programms gehen verloren.",
    ]);
    text_fit(
        g.hdc,
        g.f.body,
        13,
        p.text,
        d.left + 24,
        d.top + 62,
        body,
        d.right - d.left - 48,
    );
    let mut t = T::new();
    t.s(b"PID ").u(pid as u64);
    text(
        g.hdc,
        g.f.caption,
        p.text2,
        d.left + 24,
        d.top + 88,
        t.get(),
    );
    dialog_buttons(
        g,
        h,
        &d,
        tr4(&[
            b"Fin de t\xE2che",
            b"End task",
            b"Finalizar tarea",
            b"Task beenden",
        ]),
        true,
    );
}

unsafe fn draw_run_dialog(g: &mut Gfx, h: &mut Hits, c: &Rect) {
    let p = pal();
    let d = ui::dialog(
        g,
        h,
        c,
        520,
        230,
        tr4(&[
            b"Cr\xE9er une t\xE2che",
            b"Create new task",
            b"Crear nueva tarea",
            b"Neuen Task erstellen",
        ]),
    );
    text_fit(
        g.hdc,
        g.f.body,
        13,
        p.text2,
        d.left + 24,
        d.top + 58,
        tr4(&[
            b"Programme .exe, commande Linux ou adresse \xE0 ouvrir.",
            b"A .exe program, Linux command or address to open.",
            b"Programa .exe, comando de Linux o direcci\xF3n que abrir.",
            b".exe-Programm, Linux-Befehl oder Adresse zum \xD6ffnen.",
        ]),
        d.right - d.left - 48,
    );
    let run = (*core::ptr::addr_of!(RUN)).get();
    ui::input(
        g,
        h,
        rect(d.left + 24, d.top + 86, d.right - 24, d.top + 120),
        ID_DLG_INPUT,
        run,
        true,
        b"",
    );
    dialog_buttons(g, h, &d, b"OK", !run.is_empty());
}

const GONE: Tr4 = [
    b"Ce processus s'est d\xE9j\xE0 arr\xEAt\xE9.",
    b"This process has already stopped.",
    b"Este proceso ya se ha detenido.",
    b"Dieser Prozess wurde bereits beendet.",
];

unsafe fn end_task(pid: u32) {
    let msg: &[u8] = match sm::end_task(pid) {
        sm::Outcome::Done => tr4(&[b"T\xE2che termin\xE9e.", b"Task ended.", b"Tarea finalizada.", b"Task beendet."]),
        sm::Outcome::Gone => tr4(&GONE),
        _ => tr4(&[
            b"Acc\xE8s refus\xE9 : ce processus appartient au syst\xE8me ou \xE0 un autre utilisateur.",
            b"Access denied: this process belongs to the system or another user.",
            b"Acceso denegado: este proceso pertenece al sistema o a otro usuario.",
            b"Zugriff verweigert: Dieser Prozess geh\xF6rt dem System oder einem anderen Benutzer.",
        ]),
    };
    say(msg);
    SELECTED_PID = 0;
}

unsafe fn set_priority(pid: u32, index: usize) {
    let msg: &[u8] = match sm::set_priority(pid, index) {
        sm::Outcome::Done => tr4(&[
            b"Priorit\xE9 modifi\xE9e.",
            b"Priority changed.",
            b"Prioridad cambiada.",
            b"Priorit\xE4t ge\xE4ndert.",
        ]),
        sm::Outcome::Limited => tr4(&[
            b"Priorit\xE9 limit\xE9e : la relever demande les droits d'administrateur.",
            b"Priority limited: raising it requires administrator rights.",
            b"Prioridad limitada: subirla requiere derechos de administrador.",
            b"Priorit\xE4t begrenzt: Eine Erh\xF6hung erfordert Administratorrechte.",
        ]),
        sm::Outcome::Gone => tr4(&GONE),
        sm::Outcome::Denied => tr4(&[
            b"Acc\xE8s refus\xE9.",
            b"Access denied.",
            b"Acceso denegado.",
            b"Zugriff verweigert.",
        ]),
    };
    say(msg);
}

/// Lance la commande saisie : premier mot (ou texte entre guillemets) =
/// programme, le reste = arguments.  ShellExecute choisit le lanceur.
unsafe fn run_task() {
    let cmd = sm::trim((*core::ptr::addr_of!(RUN)).get());
    if cmd.is_empty() {
        return;
    }
    let (a, b, rest) = if cmd[0] == b'"' {
        match cmd[1..].iter().position(|&c| c == b'"') {
            Some(i) => (1, i + 1, i + 2),
            None => (1, cmd.len(), cmd.len()),
        }
    } else {
        let sp = cmd.iter().position(|&c| c == b' ').unwrap_or(cmd.len());
        (0, sp, sp)
    };
    let mut file = [0u8; 162];
    let mut args = [0u8; 162];
    file[..b - a].copy_from_slice(&cmd[a..b]);
    let tail = sm::trim(&cmd[rest.min(cmd.len())..]);
    args[..tail.len()].copy_from_slice(tail);
    let ok = ShellExecuteA(
        core::ptr::null_mut(),
        c"open".as_ptr(),
        file.as_ptr().cast(),
        if tail.is_empty() {
            core::ptr::null()
        } else {
            args.as_ptr().cast()
        },
        core::ptr::null(),
        1,
    ) as isize
        > 32;
    if !ok {
        say(tr4(&[
            b"Impossible de lancer cette commande.",
            b"Could not start this command.",
            b"No se pudo iniciar este comando.",
            b"Dieser Befehl konnte nicht gestartet werden.",
        ]));
    }
}

// ---------- Performances ----------

/// Ressource affichée dans l'onglet Performances.
#[derive(Clone, Copy, PartialEq)]
enum Res {
    Cpu,
    Mem,
    Disk(usize),
    Net(usize),
    Gpu(usize),
}

unsafe fn resources(out: &mut [Res; 12]) -> usize {
    let mut n = 0;
    let mut push = |r: Res| {
        if n < out.len() {
            out[n] = r;
            n += 1;
        }
    };
    push(Res::Cpu);
    push(Res::Mem);
    for i in 0..sm::DISK_COUNT {
        push(Res::Disk(i));
    }
    for i in 0..sm::NET_COUNT {
        push(Res::Net(i));
    }
    for i in 0..GPU_TAB_COUNT {
        push(Res::Gpu(i));
    }
    n
}

fn res_color(r: Res) -> u32 {
    match r {
        Res::Cpu => C_CPU,
        Res::Mem => C_MEM,
        Res::Disk(_) => C_DISK,
        Res::Net(_) => C_NET,
        Res::Gpu(_) => C_GPU,
    }
}

unsafe fn res_hist(r: Res) -> &'static [u32; HIST] {
    match r {
        Res::Cpu => &*core::ptr::addr_of!(sm::CPU_HIST),
        Res::Mem => &*core::ptr::addr_of!(sm::MEM_HIST),
        Res::Disk(i) => &(*core::ptr::addr_of!(sm::DISKS))[i].hist,
        Res::Net(i) => &(*core::ptr::addr_of!(sm::NETS))[i].hist,
        Res::Gpu(i) => &(*core::ptr::addr_of!(GPU_HIST))[i],
    }
}

/// Échelle du graphique : 1000 pour les pourcentages ; pour le réseau, le
/// premier palier 1-2-5 au-dessus du maximum (100 Kbits/s au moins).
unsafe fn res_scale(r: Res) -> u32 {
    let Res::Net(i) = r else {
        return 1000;
    };
    let max = (*core::ptr::addr_of!(sm::NETS))[i]
        .hist
        .iter()
        .copied()
        .max()
        .unwrap_or(0) as u64;
    let mut base = 100_000u64;
    loop {
        for m in [1u64, 2, 5] {
            if base * m >= max {
                return (base * m).min(u32::MAX as u64) as u32;
            }
        }
        base *= 10;
    }
}

unsafe fn res_title(r: Res, t: &mut T) -> &[u8] {
    t.n = 0;
    match r {
        Res::Cpu => t.s(tr4(&[b"Processeur", b"CPU", b"CPU", b"CPU"])),
        Res::Mem => t.s(tr4(&[
            b"M\xE9moire",
            b"Memory",
            b"Memoria",
            b"Arbeitsspeicher",
        ])),
        Res::Disk(i) => t
            .s(tr4(&[b"Disque ", b"Disk ", b"Disco ", b"Datentr\xE4ger "]))
            .u(i as u64)
            .s(b" (")
            .s((*core::ptr::addr_of!(sm::DISKS))[i].name())
            .s(b")"),
        Res::Net(i) => t.s(if sm::NETS[i].wifi {
            b"Wi-Fi"
        } else {
            b"Ethernet"
        }),
        Res::Gpu(i) => t.s(b"GPU ").u(i as u64),
    };
    t.get()
}

/// Résumé sous le nom dans la liste de gauche.
unsafe fn res_summary(r: Res, t: &mut T) -> &[u8] {
    t.n = 0;
    let mut a = T::new();
    let mut b = T::new();
    match r {
        Res::Cpu => {
            t.u(((sm::CPU_LOAD + 5) / 10) as u64).s(b" %");
            if sm::CPU_MHZ > 0 {
                t.s(b"  ").dec(sm::CPU_MHZ as u64 / 100).s(b" GHz");
            }
        }
        Res::Mem => {
            // « 0,5/15,7 Go (3 %) » : une seule unité, comme Windows.
            let gb = 1024 * 1024;
            if sm::MEM_TOTAL_KB >= gb {
                t.dec(sm::mem_used_kb() * 10 / gb)
                    .s(b"/")
                    .dec(sm::MEM_TOTAL_KB * 10 / gb)
                    .s(tr4(&U_GB));
            } else {
                t.s(sm::trim(size_kb(&mut a, sm::mem_used_kb())))
                    .s(b"/")
                    .s(sm::trim(size_kb(&mut b, sm::MEM_TOTAL_KB)));
            }
            t.s(b" (")
                .u(((sm::mem_used_permille() + 5) / 10) as u64)
                .s(b" %)");
        }
        Res::Disk(i) => {
            let d = &(*core::ptr::addr_of!(sm::DISKS))[i];
            t.s(if d.removable {
                tr4(&[b"Amovible", b"Removable", b"Extra\xEDble", b"Wechselmedium"])
            } else if d.ssd {
                b"SSD"
            } else {
                b"HDD"
            })
            .s(b"  ")
            .u(((d.active + 5) / 10) as u64)
            .s(b" %");
        }
        Res::Net(i) => {
            let n = &(*core::ptr::addr_of!(sm::NETS))[i];
            t.s(tr4(&[b"E : ", b"S: ", b"E: ", b"S: "]))
                .s(sm::trim(bits(&mut a, n.tx_bps)))
                .s(tr4(&[b"  R : ", b"  R: ", b"  R: ", b"  E: "]))
                .s(sm::trim(bits(&mut b, n.rx_bps)));
        }
        Res::Gpu(i) => {
            let v = crate::gpuinfo::num(GPU_TABS[i], b"load").unwrap_or(0);
            t.u(v as u64).s(b" %");
        }
    }
    t.get()
}

/// Graphique à aire façon Windows 11 : quadrillage, aire teintée, courbe.
#[allow(clippy::too_many_arguments)]
unsafe fn graph(
    g: &mut Gfx,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    hist: &[u32; HIST],
    scale: u32,
    color: u32,
    grid: bool,
) {
    let bg = pal().window;
    fill(g.hdc, g.b, x, y, x + w, y + h, bg);
    if grid {
        let gc = mix(bg, color, 2, 10);
        for k in 1..10 {
            let gy = y + h * k / 10;
            fill(g.hdc, g.b, x, gy, x + w, gy + 1, gc);
        }
        // Le quadrillage vertical défile avec les mesures, comme Windows.
        let shift = (sm::samples() % 6) as i32;
        for k in 0..=10 {
            let gx = x + w - ((k * 6 + shift) * w) / (HIST as i32 - 1);
            if gx > x && gx < x + w {
                fill(g.hdc, g.b, gx, y, gx + 1, y + h, gc);
            }
        }
    }
    let shown = (sm::samples() as usize).min(HIST);
    let area = mix(bg, color, 2, 10);
    let mut prev: Option<(i32, i32)> = None;
    for k in (0..shown).rev() {
        // k = nombre de mesures en arrière ; la plus récente est à droite.
        let v = sm::hist_at(hist, k).min(scale) as i64;
        let px = x + w - 1 - (k as i32 * (w - 1)) / (HIST as i32 - 1);
        let py = y + h - 1 - ((v * (h as i64 - 2)) / scale.max(1) as i64) as i32;
        if let Some((ox, oy)) = prev {
            let mut cx = ox;
            while cx < px {
                let cy = oy + ((py - oy) * (cx - ox)) / (px - ox).max(1);
                fill(g.hdc, g.b, cx, cy, cx + 1, y + h - 1, area);
                cx += 1;
            }
            line(g.hdc, g.b, ox, oy, px, py, 1, color);
        }
        prev = Some((px, py));
    }
    fill(g.hdc, g.b, x, y, x + w, y + 1, color);
    fill(g.hdc, g.b, x, y + h - 1, x + w, y + h, color);
    fill(g.hdc, g.b, x, y, x + 1, y + h, color);
    fill(g.hdc, g.b, x + w - 1, y, x + w, y + h, color);
}

unsafe fn draw_performance(g: &mut Gfx, h: &mut Hits, c: &Rect) {
    let p = pal();
    let mut list = [Res::Cpu; 12];
    let n = resources(&mut list);
    if PERF_SEL >= n {
        PERF_SEL = 0;
    }
    page_title(g, c.left, c.top, tr4(&TAB_NAMES[TAB_PERF]));
    // Liste des ressources avec miniatures
    let lw = 260.min((c.right - c.left) / 3);
    let mut y = c.top + 60;
    for (i, &r) in list[..n].iter().enumerate() {
        let ir = rect(c.left + 8, y, c.left + lw, y + 64);
        let hover = h.add(ir, ID_PERF + i as u16);
        if PERF_SEL == i || hover {
            let bg = if PERF_SEL == i { p.selected } else { p.hover };
            round_rect(g.hdc, g.b, ir.left, ir.top, ir.right, ir.bottom, 4, bg);
        }
        graph(
            g,
            ir.left + 8,
            y + 10,
            64,
            44,
            res_hist(r),
            res_scale(r),
            res_color(r),
            false,
        );
        let mut t = T::new();
        text_fit(
            g.hdc,
            g.f.semibold,
            13,
            p.text,
            ir.left + 84,
            y + 14,
            res_title(r, &mut t),
            lw - 96,
        );
        text_fit(
            g.hdc,
            g.f.caption,
            12,
            p.text2,
            ir.left + 84,
            y + 34,
            res_summary(r, &mut t),
            lw - 96,
        );
        y += 68;
    }
    // Détail de la ressource choisie
    let r = list[PERF_SEL];
    let x = c.left + lw + 24;
    let w = c.right - x - 24;
    let mut t = T::new();
    text(
        g.hdc,
        g.f.large,
        p.text,
        x,
        c.top + 58,
        res_title(r, &mut t),
    );
    let model: &[u8] = match r {
        Res::Cpu => {
            let cpu = &*core::ptr::addr_of!(sm::CPU);
            &cpu.model[..cpu.model_len]
        }
        Res::Disk(i) => (*core::ptr::addr_of!(sm::DISKS))[i].model(),
        Res::Net(i) => (*core::ptr::addr_of!(sm::NETS))[i].name(),
        Res::Gpu(i) => crate::gpuinfo::get(GPU_TABS[i], b"name").unwrap_or(b""),
        Res::Mem => b"",
    };
    if !model.is_empty() {
        let mw = text_width(model, 13).min(w / 2);
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text2,
            x + w - mw,
            c.top + 66,
            model,
            w / 2,
        );
    }
    let caption: &[u8] = match r {
        Res::Net(_) => tr4(&[b"D\xE9bit", b"Throughput", b"Rendimiento", b"Durchsatz"]),
        Res::Disk(_) => tr4(&[
            b"Temps d'activit\xE9",
            b"Active time",
            b"Tiempo activo",
            b"Aktive Zeit",
        ]),
        Res::Mem => tr4(&[
            b"Utilisation de la m\xE9moire",
            b"Memory usage",
            b"Uso de memoria",
            b"Speichernutzung",
        ]),
        _ => tr4(&[
            b"% d'utilisation",
            b"% Utilization",
            b"% de uso",
            b"% Auslastung",
        ]),
    };
    let gy = c.top + 112;
    text(g.hdc, g.f.caption, p.text2, x, gy - 18, caption);
    let scale = res_scale(r);
    let top_label: &[u8] = match r {
        Res::Net(_) => bits(&mut t, scale as u64),
        _ => b"100 %",
    };
    let tw = text_width(top_label, 12);
    text(g.hdc, g.f.caption, p.text2, x + w - tw, gy - 18, top_label);
    let gh = ((c.bottom - gy) * 45 / 100).max(80);
    graph(g, x, gy, w, gh, res_hist(r), scale, res_color(r), true);
    let secs = HIST as u64 * sm::INTERVAL_MS as u64 / 1000;
    t.n = 0;
    t.u(secs).s(tr4(&[
        b" secondes",
        b" seconds",
        b" segundos",
        b" Sekunden",
    ]));
    text(g.hdc, g.f.caption, p.text2, x, gy + gh + 4, t.get());
    text(g.hdc, g.f.caption, p.text2, x + w - 8, gy + gh + 4, b"0");
    let sy = gy + gh + 30;
    match r {
        Res::Cpu => perf_cpu(g, x, sy, w),
        Res::Mem => perf_mem(g, x, sy, w),
        Res::Disk(i) => perf_disk(g, x, sy, w, i),
        Res::Net(i) => perf_net(g, x, sy, w, i),
        Res::Gpu(i) => perf_gpu(g, x, sy, w, i),
    }
}

/// Grande valeur sous son libellé, comme les statistiques de Windows.
unsafe fn big_stat(g: &mut Gfx, x: i32, y: i32, label: &[u8], value: &[u8]) {
    let p = pal();
    text(g.hdc, g.f.caption, p.text2, x, y, label);
    text(g.hdc, g.f.large, p.text, x, y + 16, value);
}

/// Libellé et valeur sur une ligne (colonne de droite).
unsafe fn kv(g: &mut Gfx, x: i32, y: i32, w: i32, label: &[u8], value: &[u8]) {
    let p = pal();
    text_fit(g.hdc, g.f.caption, 12, p.text2, x, y, label, w / 2 - 8);
    text_fit(g.hdc, g.f.caption, 12, p.text, x + w / 2, y, value, w / 2);
}

unsafe fn perf_cpu(g: &mut Gfx, x: i32, y: i32, w: i32) {
    let half = w / 2;
    let mut t = T::new();
    big_stat(
        g,
        x,
        y,
        tr4(&[b"Utilisation", b"Utilization", b"Uso", b"Auslastung"]),
        pct0(&mut t, sm::CPU_LOAD),
    );
    if sm::CPU_MHZ > 0 {
        big_stat(
            g,
            x + half / 2,
            y,
            tr4(&[b"Vitesse", b"Speed", b"Velocidad", b"Geschwindigkeit"]),
            mhz(&mut t, sm::CPU_MHZ),
        );
    }
    t.n = 0;
    t.u(sm::COUNT as u64);
    big_stat(
        g,
        x,
        y + 52,
        tr4(&[b"Processus", b"Processes", b"Procesos", b"Prozesse"]),
        t.get(),
    );
    t.n = 0;
    t.u(sm::THREADS as u64);
    big_stat(
        g,
        x + half / 2,
        y + 52,
        tr4(&[b"Threads", b"Threads", b"Subprocesos", b"Threads"]),
        t.get(),
    );
    big_stat(
        g,
        x,
        y + 104,
        tr4(&[
            b"Dur\xE9e de fonctionnement",
            b"Up time",
            b"Tiempo activo",
            b"Betriebszeit",
        ]),
        duration(&mut t, sm::UPTIME_SEC),
    );
    // Colonne de droite
    let rx = x + half;
    let mut ry = y;
    let cpu = &*core::ptr::addr_of!(sm::CPU);
    let mut row = |g: &mut Gfx, label: &[u8], value: &[u8]| {
        kv(g, rx, ry, half, label, value);
        ry += 22;
    };
    if cpu.base_mhz > 0 {
        row(
            g,
            tr4(&[
                b"Vitesse de base :",
                b"Base speed:",
                b"Velocidad base:",
                b"Basisgeschwindigkeit:",
            ]),
            mhz(&mut t, cpu.base_mhz),
        );
    }
    t.n = 0;
    t.u(cpu.sockets.max(1) as u64);
    row(g, b"Sockets :", t.get());
    if cpu.cores > 0 {
        t.n = 0;
        t.u(cpu.cores as u64);
        row(
            g,
            tr4(&[b"C\x9Curs :", b"Cores:", b"N\xFAcleos:", b"Kerne:"]),
            t.get(),
        );
    }
    t.n = 0;
    t.u(cpu.logical as u64);
    row(
        g,
        tr4(&[
            b"Processeurs logiques :",
            b"Logical processors:",
            b"Procesadores l\xF3gicos:",
            b"Logische Prozessoren:",
        ]),
        t.get(),
    );
    row(
        g,
        tr4(&[
            b"Machine virtuelle :",
            b"Virtual machine:",
            b"M\xE1quina virtual:",
            b"Virtueller Computer:",
        ]),
        yes_no(cpu.virtual_machine),
    );
    for (label, kb) in [
        (b"Cache L1 :".as_slice(), cpu.l1_kb),
        (b"Cache L2 :", cpu.l2_kb),
        (b"Cache L3 :", cpu.l3_kb),
    ] {
        if kb > 0 {
            row(g, label, size_kb(&mut t, kb as u64));
        }
    }
}

unsafe fn perf_mem(g: &mut Gfx, x: i32, y: i32, w: i32) {
    let half = w / 2;
    let mut t = T::new();
    let mut a = T::new();
    let mut b = T::new();
    big_stat(
        g,
        x,
        y,
        tr4(&[b"Utilis\xE9e", b"In use", b"En uso", b"In Verwendung"]),
        size_kb(&mut t, sm::mem_used_kb()),
    );
    big_stat(
        g,
        x + half / 2,
        y,
        tr4(&[b"Disponible", b"Available", b"Disponible", b"Verf\xFCgbar"]),
        size_kb(&mut t, sm::MEM_AVAIL_KB),
    );
    if sm::COMMIT_LIMIT_KB > 0 {
        t.n = 0;
        t.s(sm::trim(size_kb(&mut a, sm::COMMIT_KB)))
            .s(b"/")
            .s(sm::trim(size_kb(&mut b, sm::COMMIT_LIMIT_KB)));
        big_stat(
            g,
            x,
            y + 52,
            tr4(&[b"Valid\xE9e", b"Committed", b"Confirmada", b"Zugesichert"]),
            t.get(),
        );
    }
    big_stat(
        g,
        x + half / 2,
        y + 52,
        tr4(&[b"En cache", b"Cached", b"En cach\xE9", b"Im Cache"]),
        size_kb(&mut t, sm::MEM_CACHED_KB),
    );
    let rx = x + half;
    kv(
        g,
        rx,
        y,
        half,
        b"Total :",
        size_kb(&mut t, sm::MEM_TOTAL_KB),
    );
    if sm::SWAP_TOTAL_KB > 0 {
        t.n = 0;
        let used = sm::SWAP_TOTAL_KB - sm::SWAP_FREE_KB.min(sm::SWAP_TOTAL_KB);
        t.s(sm::trim(size_kb(&mut a, used)))
            .s(b"/")
            .s(sm::trim(size_kb(&mut b, sm::SWAP_TOTAL_KB)));
        kv(
            g,
            rx,
            y + 22,
            half,
            tr4(&[
                b"Fichier d'\xE9change :",
                b"Swap:",
                b"Archivo de intercambio:",
                b"Auslagerung:",
            ]),
            t.get(),
        );
    }
}

unsafe fn perf_disk(g: &mut Gfx, x: i32, y: i32, w: i32, i: usize) {
    let d = &(*core::ptr::addr_of!(sm::DISKS))[i];
    let half = w / 2;
    let mut t = T::new();
    big_stat(
        g,
        x,
        y,
        tr4(&[
            b"Temps d'activit\xE9",
            b"Active time",
            b"Tiempo activo",
            b"Aktive Zeit",
        ]),
        pct0(&mut t, d.active),
    );
    big_stat(
        g,
        x,
        y + 52,
        tr4(&[
            b"Vitesse de lecture",
            b"Read speed",
            b"Velocidad de lectura",
            b"Lesegeschwindigkeit",
        ]),
        rate(&mut t, d.read_bps),
    );
    big_stat(
        g,
        x + half / 2,
        y + 52,
        tr4(&[
            b"Vitesse d'\xE9criture",
            b"Write speed",
            b"Velocidad de escritura",
            b"Schreibgeschwindigkeit",
        ]),
        rate(&mut t, d.write_bps),
    );
    let rx = x + half;
    kv(
        g,
        rx,
        y,
        half,
        tr4(&[
            b"Capacit\xE9 :",
            b"Capacity:",
            b"Capacidad:",
            b"Kapazit\xE4t:",
        ]),
        size_kb(&mut t, d.size_bytes / 1024),
    );
    kv(
        g,
        rx,
        y + 22,
        half,
        tr4(&[b"Type :", b"Type:", b"Tipo:", b"Typ:"]),
        if d.ssd { b"SSD" } else { b"HDD" },
    );
    kv(
        g,
        rx,
        y + 44,
        half,
        tr4(&[
            b"Amovible :",
            b"Removable:",
            b"Extra\xEDble:",
            b"Wechselmedium:",
        ]),
        yes_no(d.removable),
    );
}

unsafe fn perf_net(g: &mut Gfx, x: i32, y: i32, w: i32, i: usize) {
    let n = &(*core::ptr::addr_of!(sm::NETS))[i];
    let half = w / 2;
    let mut t = T::new();
    big_stat(
        g,
        x,
        y,
        tr4(&[b"Envoi", b"Send", b"Env\xEDo", b"Senden"]),
        bits(&mut t, n.tx_bps),
    );
    big_stat(
        g,
        x + half / 2,
        y,
        tr4(&[b"R\xE9ception", b"Receive", b"Recepci\xF3n", b"Empfangen"]),
        bits(&mut t, n.rx_bps),
    );
    let rx = x + half;
    kv(
        g,
        rx,
        y,
        half,
        tr4(&[b"Carte :", b"Adapter:", b"Adaptador:", b"Adapter:"]),
        n.name(),
    );
    kv(
        g,
        rx,
        y + 22,
        half,
        tr4(&[
            b"Type de connexion :",
            b"Connection type:",
            b"Tipo de conexi\xF3n:",
            b"Verbindungstyp:",
        ]),
        if n.wifi { b"Wi-Fi" } else { b"Ethernet" },
    );
    let mut ry = y + 44;
    if n.speed_mbps > 0 {
        kv(
            g,
            rx,
            ry,
            half,
            tr4(&[
                b"Vitesse de liaison :",
                b"Link speed:",
                b"Velocidad de v\xEDnculo:",
                b"Verbindungsgeschwindigkeit:",
            ]),
            bits(&mut t, n.speed_mbps as u64 * 1_000_000),
        );
        ry += 22;
    }
    if n.mac[0] != 0 {
        let len = n.mac.iter().position(|&c| c == 0).unwrap_or(17);
        kv(
            g,
            rx,
            ry,
            half,
            tr4(&[
                b"Adresse physique :",
                b"Physical address:",
                b"Direcci\xF3n f\xEDsica:",
                b"Physische Adresse:",
            ]),
            &n.mac[..len],
        );
    }
}

unsafe fn perf_gpu(g: &mut Gfx, x: i32, y: i32, w: i32, slot: usize) {
    use crate::gpuinfo::{self as gpu, Line};
    let i = GPU_TABS[slot];
    let half = w / 2;
    let mut t = T::new();
    if let Some(v) = gpu::num(i, b"load") {
        big_stat(
            g,
            x,
            y,
            tr4(&[b"Utilisation", b"Utilization", b"Uso", b"Auslastung"]),
            pct0(&mut t, v.min(100) * 10),
        );
    }
    let rx = x + half;
    let mut ry = y;
    if let (Some(u), Some(tot)) = (gpu::num(i, b"vram_used_mb"), gpu::num(i, b"vram_total_mb")) {
        let mut l = Line::new();
        l.num(u).push(b" / ").num(tot).push(tr4(&U_MB));
        kv(
            g,
            rx,
            ry,
            half,
            tr4(&[
                b"M\xE9moire d\xE9di\xE9e :",
                b"Dedicated memory:",
                b"Memoria dedicada:",
                b"Dedizierter Speicher:",
            ]),
            l.get(),
        );
        ry += 22;
    }
    let rows: [(&[u8], &[u8], &[u8]); 5] = [
        (
            b"temp_c",
            tr4(&[
                b"Temp\xE9rature :",
                b"Temperature:",
                b"Temperatura:",
                b"Temperatur:",
            ]),
            b" \xB0C",
        ),
        (
            b"freq_mhz",
            tr4(&[
                b"Fr\xE9quence :",
                b"Frequency:",
                b"Frecuencia:",
                b"Frequenz:",
            ]),
            b" MHz",
        ),
        (
            b"power_w",
            tr4(&[
                b"Consommation :",
                b"Power usage:",
                b"Consumo:",
                b"Leistungsaufnahme:",
            ]),
            b" W",
        ),
        (
            b"encoder",
            tr4(&[
                b"Encodeur vid\xE9o :",
                b"Video encode:",
                b"Codificador de v\xEDdeo:",
                b"Video-Encoder:",
            ]),
            b" %",
        ),
        (
            b"decoder",
            tr4(&[
                b"D\xE9codeur vid\xE9o :",
                b"Video decode:",
                b"Decodificador de v\xEDdeo:",
                b"Video-Decoder:",
            ]),
            b" %",
        ),
    ];
    for (key, label, unit) in rows {
        if let Some(v) = gpu::num(i, key) {
            let mut l = Line::new();
            l.num(v).push(unit);
            kv(g, rx, ry, half, label, l.get());
            ry += 22;
        }
    }
    if let Some(d) = gpu::get(i, b"driver") {
        kv(
            g,
            rx,
            ry,
            half,
            tr4(&[b"Pilote :", b"Driver:", b"Controlador:", b"Treiber:"]),
            d,
        );
    }
}

// ---------- Historique des applications ----------

unsafe fn draw_history(g: &mut Gfx, h: &mut Hits, c: &Rect) {
    let p = pal();
    page_title(g, c.left, c.top, tr4(&TAB_NAMES[TAB_HISTORY]));
    command(
        g,
        h,
        c.right - 20,
        c.top + 12,
        ID_HISTORY_CLEAR,
        tr4(&[
            b"Effacer l'historique",
            b"Delete usage history",
            b"Borrar historial",
            b"Verlauf l\xF6schen",
        ]),
        APP_COUNT > 0,
    );
    text_fit(
        g.hdc,
        g.f.caption,
        12,
        p.text2,
        c.left + 20,
        c.top + 48,
        tr4(&[
            b"Temps processeur de vos applications depuis l'ouverture du Gestionnaire des t\xE2ches.",
            b"CPU time used by your apps since Task Manager was opened.",
            b"Tiempo de CPU de sus aplicaciones desde que se abri\xF3 el Administrador de tareas.",
            b"CPU-Zeit Ihrer Apps seit dem \xD6ffnen des Task-Managers.",
        ]),
        c.right - c.left - 40,
    );
    let w = c.right - c.left;
    let cols: [(&[u8], i32, &[u8]); 4] = [
        (
            tr4(&[b"Nom", b"Name", b"Nombre", b"Name"]),
            w * 45 / 100,
            b"",
        ),
        (
            tr4(&[
                b"Temps processeur",
                b"CPU time",
                b"Tiempo de CPU",
                b"CPU-Zeit",
            ]),
            w * 20 / 100,
            b"",
        ),
        (
            tr4(&[b"Lancements", b"Launches", b"Inicios", b"Starts"]),
            w * 15 / 100,
            b"",
        ),
        (tr4(&[b"Statut", b"Status", b"Estado", b"Status"]), 0, b""),
    ];
    let top = columns(g, h, c.left, c.top + 72, c.right, &cols);
    let apps = &mut *core::ptr::addr_of_mut!(APPS);
    let list = &mut apps[..APP_COUNT];
    match SORT_COL {
        0 => list.sort_unstable_by(|a, b| a.name[..a.len].cmp(&b.name[..b.len])),
        2 => list.sort_unstable_by(|a, b| b.launches.cmp(&a.launches)),
        3 => list.sort_unstable_by(|a, b| b.running.cmp(&a.running)),
        _ => list.sort_unstable_by(|a, b| b.ticks.cmp(&a.ticks)),
    }
    // Ordre par défaut : décroissant ; le nom se lit de A à Z.
    if (SORT_COL == 0) == SORT_DESC && SORT_COL >= 0 {
        list.reverse();
    }
    let row_h = 32;
    VISIBLE_ROWS = ((c.bottom - top - 8) / row_h).max(1) as usize;
    let mut y = top;
    let mut t = T::new();
    for (i, a) in list.iter().enumerate().skip(SCROLL).take(VISIBLE_ROWS) {
        let r = rect(c.left, y, c.right - 10, y + row_h);
        let hover = h.add(r, ID_ROW + (i - SCROLL) as u16);
        row_bg(g, &r, false, hover);
        let kind = if a.exe { Glyph::Exe } else { Glyph::Terminal };
        glyph(g.hdc, g.b, g.f, kind, c.left + 20, y + 8, 16);
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            c.left + 46,
            y + 9,
            &a.name[..a.len],
            cols[0].1 - 56,
        );
        let x1 = c.left + cols[0].1;
        right_text(
            g,
            x1,
            cols[1].1,
            y + 9,
            duration(&mut t, a.ticks / 100),
            p.text,
        );
        t.n = 0;
        t.u(a.launches as u64);
        right_text(g, x1 + cols[1].1, cols[2].1, y + 9, t.get(), p.text);
        let status = if a.running {
            tr4(&[
                b"En cours",
                b"Running",
                b"En ejecuci\xF3n",
                b"Wird ausgef\xFChrt",
            ])
        } else {
            tr4(&[b"Ferm\xE9e", b"Closed", b"Cerrada", b"Geschlossen"])
        };
        text(
            g.hdc,
            g.f.body,
            p.text2,
            x1 + cols[1].1 + cols[2].1 + 10,
            y + 9,
            status,
        );
        y += row_h;
    }
    scrollbar(
        g,
        c.right - 7,
        top + 2,
        c.bottom - top - 6,
        APP_COUNT,
        VISIBLE_ROWS,
    );
}

// ---------- Applications de démarrage ----------

unsafe fn draw_startup(g: &mut Gfx, h: &mut Hits, c: &Rect) {
    let p = pal();
    if !STARTUP_LOADED {
        STARTUP_LOADED = true;
        sm::load_startup();
    }
    page_title(g, c.left, c.top, tr4(&TAB_NAMES[TAB_STARTUP]));
    let list = &*core::ptr::addr_of!(sm::STARTUP);
    let sel = list[..sm::STARTUP_COUNT].get(SELECTED_ITEM);
    let label = match sel {
        Some(e) if e.enabled => tr4(&[
            b"D\xE9sactiver",
            b"Disable",
            b"Deshabilitar",
            b"Deaktivieren",
        ]),
        _ => tr4(&[b"Activer", b"Enable", b"Habilitar", b"Aktivieren"]),
    };
    command(
        g,
        h,
        c.right - 20,
        c.top + 12,
        ID_STARTUP_TOGGLE,
        label,
        sel.is_some(),
    );
    let w = c.right - c.left;
    let cols: [(&[u8], i32, &[u8]); 4] = [
        (
            tr4(&[b"Nom", b"Name", b"Nombre", b"Name"]),
            w * 30 / 100,
            b"",
        ),
        (
            tr4(&[b"Commande", b"Command", b"Comando", b"Befehl"]),
            w * 40 / 100,
            b"",
        ),
        (
            tr4(&[b"Port\xE9e", b"Scope", b"\xC1mbito", b"Bereich"]),
            w * 14 / 100,
            b"",
        ),
        (tr4(&[b"Statut", b"Status", b"Estado", b"Status"]), 0, b""),
    ];
    let top = columns(g, h, c.left, c.top + 58, c.right, &cols);
    let row_h = 32;
    VISIBLE_ROWS = ((c.bottom - top - 8) / row_h).max(1) as usize;
    let mut y = top;
    for (i, e) in list[..sm::STARTUP_COUNT]
        .iter()
        .enumerate()
        .skip(SCROLL)
        .take(VISIBLE_ROWS)
    {
        let r = rect(c.left, y, c.right - 10, y + row_h);
        let hover = h.add(r, ID_ROW + (i - SCROLL) as u16);
        row_bg(g, &r, SELECTED_ITEM == i, hover);
        let mut x = c.left;
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            x + 20,
            y + 9,
            e.name(),
            cols[0].1 - 30,
        );
        x += cols[0].1;
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text2,
            x + 10,
            y + 9,
            e.exec(),
            cols[1].1 - 20,
        );
        x += cols[1].1;
        let scope = if e.user {
            tr4(&[b"Utilisateur", b"User", b"Usuario", b"Benutzer"])
        } else {
            tr4(&[b"Syst\xE8me", b"System", b"Sistema", b"System"])
        };
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text2,
            x + 10,
            y + 9,
            scope,
            cols[2].1 - 20,
        );
        x += cols[2].1;
        let (s, col): (&[u8], u32) = if e.enabled {
            (
                tr4(&[b"Activ\xE9", b"Enabled", b"Habilitado", b"Aktiviert"]),
                p.text,
            )
        } else {
            (
                tr4(&[
                    b"D\xE9sactiv\xE9",
                    b"Disabled",
                    b"Deshabilitado",
                    b"Deaktiviert",
                ]),
                p.text3,
            )
        };
        text(g.hdc, g.f.body, col, x + 10, y + 9, s);
        y += row_h;
    }
    if sm::STARTUP_COUNT == 0 {
        text(
            g.hdc,
            g.f.body,
            p.text2,
            c.left + 20,
            top + 16,
            tr4(&[
                b"Aucune application ne se lance \xE0 l'ouverture de session.",
                b"No app starts when you sign in.",
                b"Ninguna aplicaci\xF3n se inicia al iniciar sesi\xF3n.",
                b"Beim Anmelden wird keine App gestartet.",
            ]),
        );
    }
    scrollbar(
        g,
        c.right - 7,
        top + 2,
        c.bottom - top - 6,
        sm::STARTUP_COUNT,
        VISIBLE_ROWS,
    );
}

// ---------- Utilisateurs ----------

unsafe fn draw_users(g: &mut Gfx, h: &mut Hits, c: &Rect) {
    let p = pal();
    page_title(g, c.left, c.top, tr4(&TAB_NAMES[TAB_USERS]));
    // Processus regroupés par compte (comptes personnels et vous-même).
    let mut uids = [0u32; 16];
    let mut cpu = [0u32; 16];
    let mut mem = [0u64; 16];
    let mut cnt = [0u32; 16];
    let mut n = 0usize;
    let procs = &*core::ptr::addr_of!(sm::PROCS);
    for pr in procs[..sm::COUNT].iter() {
        if pr.kind == sm::KIND_KERNEL || !(pr.uid == sm::MY_UID || (1000..60000).contains(&pr.uid))
        {
            continue;
        }
        let i = match uids[..n].iter().position(|&u| u == pr.uid) {
            Some(i) => i,
            None if n < 16 => {
                uids[n] = pr.uid;
                n += 1;
                n - 1
            }
            None => continue,
        };
        cpu[i] += pr.cpu;
        mem[i] += pr.mem_kb as u64;
        cnt[i] += 1;
    }
    let w = c.right - c.left;
    let mut t1 = T::new();
    let mut t2 = T::new();
    let cols: [(&[u8], i32, &[u8]); 5] = [
        (
            tr4(&[b"Utilisateur", b"User", b"Usuario", b"Benutzer"]),
            w * 34 / 100,
            b"",
        ),
        (
            tr4(&[b"Statut", b"Status", b"Estado", b"Status"]),
            w * 16 / 100,
            b"",
        ),
        (
            tr4(&[b"Processeur", b"CPU", b"CPU", b"CPU"]),
            w * 16 / 100,
            pct0(&mut t1, sm::CPU_LOAD),
        ),
        (
            tr4(&[b"M\xE9moire", b"Memory", b"Memoria", b"Arbeitsspeicher"]),
            w * 18 / 100,
            pct0(&mut t2, sm::mem_used_permille()),
        ),
        (
            tr4(&[b"Processus", b"Processes", b"Procesos", b"Prozesse"]),
            0,
            b"",
        ),
    ];
    let top = columns(g, h, c.left, c.top + 58, c.right, &cols);
    let mut y = top;
    let mut t = T::new();
    for i in 0..n {
        let mut scratch = [0u8; 20];
        let name = sm::user_name(uids[i], &mut scratch);
        ellipse(
            g.hdc,
            g.b,
            c.left + 20,
            y + 6,
            c.left + 40,
            y + 26,
            p.accent,
        );
        let initial = [name.first().copied().unwrap_or(b'?').to_ascii_uppercase()];
        text_centered(
            g.hdc,
            g.f.semibold,
            13,
            p.on_accent,
            c.left + 30,
            y + 9,
            &initial,
        );
        t.n = 0;
        t.s(name);
        if uids[i] == sm::MY_UID {
            t.s(tr4(&[b" (vous)", b" (you)", b" (usted)", b" (Sie)"]));
        }
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            c.left + 52,
            y + 9,
            t.get(),
            cols[0].1 - 60,
        );
        let mut x = c.left + cols[0].1;
        text(
            g.hdc,
            g.f.body,
            p.text2,
            x + 10,
            y + 9,
            tr4(&[b"Connect\xE9", b"Signed in", b"Conectado", b"Angemeldet"]),
        );
        x += cols[1].1;
        heat(g, x, y, cols[2].1, 32, cpu[i]);
        right_text(
            g,
            x,
            cols[2].1,
            y + 9,
            pct(&mut t, cpu[i].min(1000)),
            p.text,
        );
        x += cols[2].1;
        right_text(g, x, cols[3].1, y + 9, size_kb(&mut t, mem[i]), p.text);
        x += cols[3].1;
        t.n = 0;
        t.u(cnt[i] as u64);
        text(g.hdc, g.f.body, p.text, x + 10, y + 9, t.get());
        y += 34;
    }
}

// ---------- Détails ----------

fn detail_key(p: &Proc, col: i32) -> u64 {
    match col {
        1 => p.pid as u64,
        2 => p.state as u64,
        3 => p.uid as u64,
        4 => p.cpu as u64,
        5 => p.mem_kb as u64,
        6 => (20 - p.nice as i64) as u64,
        _ => p.threads as u64,
    }
}

fn state_name(s: u8) -> &'static [u8] {
    match s {
        b'R' => tr4(&[
            b"En cours",
            b"Running",
            b"En ejecuci\xF3n",
            b"Wird ausgef\xFChrt",
        ]),
        b'S' | b'I' => tr4(&[b"En attente", b"Waiting", b"En espera", b"Wartend"]),
        b'D' => tr4(&[
            b"E/S en cours",
            b"I/O wait",
            b"E/S en curso",
            b"E/A-Wartezeit",
        ]),
        b'T' | b't' => tr4(&[b"Suspendu", b"Suspended", b"Suspendido", b"Angehalten"]),
        b'Z' => tr4(&[b"Termin\xE9", b"Ended", b"Finalizado", b"Beendet"]),
        _ => b"?",
    }
}

unsafe fn draw_details(g: &mut Gfx, h: &mut Hits, c: &Rect) {
    let p = pal();
    page_title(g, c.left, c.top, tr4(&TAB_NAMES[TAB_DETAILS]));
    let selected = SELECTED_PID != 0 && sm::proc_by_pid(SELECTED_PID).is_some();
    let bx = c.right - 20;
    let used = command(
        g,
        h,
        bx,
        c.top + 12,
        ID_END,
        tr4(&[
            b"Fin de t\xE2che",
            b"End task",
            b"Finalizar tarea",
            b"Task beenden",
        ]),
        selected,
    );
    let sw = (bx - used - c.left - 220).clamp(110, 260);
    ui::input(
        g,
        h,
        rect(bx - used - 12 - sw, c.top + 12, bx - used - 12, c.top + 44),
        ID_SEARCH,
        (*core::ptr::addr_of!(SEARCH)).get(),
        SEARCH_FOCUS,
        tr4(&[
            b"Rechercher un nom",
            b"Type a name",
            b"Buscar un nombre",
            b"Namen eingeben",
        ]),
    );
    let w = c.right - c.left;
    let cw = |pc: i32| w * pc / 100;
    let cols: [(&[u8], i32, &[u8]); 8] = [
        (tr4(&[b"Nom", b"Name", b"Nombre", b"Name"]), cw(24), b""),
        (b"PID", cw(8), b""),
        (
            tr4(&[b"Statut", b"Status", b"Estado", b"Status"]),
            cw(12),
            b"",
        ),
        (
            tr4(&[b"Utilisateur", b"User name", b"Usuario", b"Benutzername"]),
            cw(12),
            b"",
        ),
        (tr4(&[b"Processeur", b"CPU", b"CPU", b"CPU"]), cw(9), b""),
        (
            tr4(&[b"M\xE9moire", b"Memory", b"Memoria", b"Arbeitsspeicher"]),
            cw(11),
            b"",
        ),
        (
            tr4(&[b"Priorit\xE9", b"Priority", b"Prioridad", b"Priorit\xE4t"]),
            cw(15),
            b"",
        ),
        (b"Threads", 0, b""),
    ];
    let top = columns(g, h, c.left, c.top + 58, c.right, &cols);
    let n = sorted(&|_| true, &detail_key);
    let row_h = 26;
    let bottom = c.bottom - 34;
    VISIBLE_ROWS = (((bottom - top - 4) / row_h).max(1) as usize).min(MAX_ROWS);
    if SCROLL > n.saturating_sub(VISIBLE_ROWS) {
        SCROLL = n.saturating_sub(VISIBLE_ROWS);
    }
    let order = &*core::ptr::addr_of!(ORDER);
    let procs = &*core::ptr::addr_of!(sm::PROCS);
    let rows = &mut *core::ptr::addr_of_mut!(ROWS);
    let mut y = top;
    let mut t = T::new();
    let mut slot = 0;
    for &i in order[..n].iter().skip(SCROLL).take(VISIBLE_ROWS) {
        let pr = &procs[i as usize];
        rows[slot] = pr.pid;
        let r = rect(c.left, y, c.right - 10, y + row_h);
        let hover = h.add(r, ID_ROW + slot as u16);
        row_bg(g, &r, SELECTED_PID == pr.pid, hover);
        let ty = y + 6;
        let mut x = c.left;
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            x + 14,
            ty,
            pr.name(),
            cols[0].1 - 20,
        );
        x += cols[0].1;
        t.n = 0;
        t.u(pr.pid as u64);
        right_text(g, x, cols[1].1, ty, t.get(), p.text2);
        x += cols[1].1;
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text2,
            x + 10,
            ty,
            state_name(pr.state),
            cols[2].1 - 16,
        );
        x += cols[2].1;
        let mut scratch = [0u8; 20];
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text2,
            x + 10,
            ty,
            sm::user_name(pr.uid, &mut scratch),
            cols[3].1 - 16,
        );
        x += cols[3].1;
        right_text(g, x, cols[4].1, ty, pct(&mut t, pr.cpu), p.text);
        x += cols[4].1;
        right_text(
            g,
            x,
            cols[5].1,
            ty,
            size_kb(&mut t, pr.mem_kb as u64),
            p.text,
        );
        x += cols[5].1;
        let prio = tr4(&PRIORITY_NAMES[sm::priority_index(pr.nice)]);
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text2,
            x + 10,
            ty,
            prio,
            cols[6].1 - 16,
        );
        x += cols[6].1;
        t.n = 0;
        t.u(pr.threads as u64);
        text(g.hdc, g.f.body, p.text2, x + 10, ty, t.get());
        y += row_h;
        slot += 1;
    }
    ROW_COUNT = slot;
    scrollbar(g, c.right - 7, top + 2, bottom - top - 4, n, VISIBLE_ROWS);
    // Ligne de commande du processus choisi
    fill(g.hdc, g.b, c.left, bottom, c.right, bottom + 1, p.border);
    if let Some(pr) = sm::proc_by_pid(SELECTED_PID) {
        t.n = 0;
        t.s(tr4(&[
            b"Ligne de commande : ",
            b"Command line: ",
            b"L\xEDnea de comandos: ",
            b"Befehlszeile: ",
        ]));
        t.s(if pr.cmd().is_empty() {
            pr.name()
        } else {
            pr.cmd()
        });
        text_fit(
            g.hdc,
            g.f.caption,
            12,
            p.text2,
            c.left + 14,
            bottom + 10,
            t.get(),
            c.right - c.left - 28,
        );
    }
}

// ---------- Services ----------

unsafe fn draw_services(g: &mut Gfx, h: &mut Hits, c: &Rect) {
    let p = pal();
    let now = GetTickCount();
    if SERVICES_AT == 0 || now.wrapping_sub(SERVICES_AT) > 5000 {
        SERVICES_AT = now.max(1);
        sm::load_services();
    }
    page_title(g, c.left, c.top, tr4(&TAB_NAMES[TAB_SERVICES]));
    let w = c.right - c.left;
    let cols: [(&[u8], i32, &[u8]); 4] = [
        (
            tr4(&[b"Nom", b"Name", b"Nombre", b"Name"]),
            w * 28 / 100,
            b"",
        ),
        (b"PID", w * 10 / 100, b""),
        (
            tr4(&[
                b"Description",
                b"Description",
                b"Descripci\xF3n",
                b"Beschreibung",
            ]),
            w * 46 / 100,
            b"",
        ),
        (tr4(&[b"Statut", b"Status", b"Estado", b"Status"]), 0, b""),
    ];
    let top = columns(g, h, c.left, c.top + 58, c.right, &cols);
    let row_h = 26;
    VISIBLE_ROWS = ((c.bottom - top - 8) / row_h).max(1) as usize;
    let mut y = top;
    let mut t = T::new();
    let list = &*core::ptr::addr_of!(sm::SERVICES);
    for (i, s) in list[..sm::SERVICE_COUNT]
        .iter()
        .enumerate()
        .skip(SCROLL)
        .take(VISIBLE_ROWS)
    {
        let r = rect(c.left, y, c.right - 10, y + row_h);
        let hover = h.add(r, ID_ROW + (i - SCROLL) as u16);
        row_bg(g, &r, false, hover);
        let ty = y + 6;
        let mut x = c.left;
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            x + 14,
            ty,
            s.name(),
            cols[0].1 - 20,
        );
        x += cols[0].1;
        if s.pid > 0 {
            t.n = 0;
            t.u(s.pid as u64);
            right_text(g, x, cols[1].1, ty, t.get(), p.text2);
        }
        x += cols[1].1;
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text2,
            x + 10,
            ty,
            s.desc(),
            cols[2].1 - 16,
        );
        x += cols[2].1;
        let (label, color) = if s.running {
            (
                tr4(&[
                    b"En cours",
                    b"Running",
                    b"En ejecuci\xF3n",
                    b"Wird ausgef\xFChrt",
                ]),
                p.text,
            )
        } else {
            (
                tr4(&[b"Arr\xEAt\xE9", b"Stopped", b"Detenido", b"Beendet"]),
                p.text3,
            )
        };
        text(g.hdc, g.f.body, color, x + 10, ty, label);
        y += row_h;
    }
    scrollbar(
        g,
        c.right - 7,
        top + 2,
        c.bottom - top - 6,
        sm::SERVICE_COUNT,
        VISIBLE_ROWS,
    );
}

// ---------- Paramètres ----------

const SPEEDS: [(u32, Tr4); 4] = [
    (500, [b"\xC9lev\xE9e", b"High", b"Alta", b"Hoch"]),
    (1000, [b"Normale", b"Normal", b"Normal", b"Normal"]),
    (4000, [b"Basse", b"Low", b"Baja", b"Niedrig"]),
    (0, [b"En pause", b"Paused", b"En pausa", b"Angehalten"]),
];

unsafe fn draw_settings(g: &mut Gfx, h: &mut Hits, c: &Rect) {
    let p = pal();
    page_title(g, c.left, c.top, tr4(&TAB_NAMES[TAB_SETTINGS]));
    let x = c.left + 20;
    let y = c.top + 70;
    panel(
        g.hdc,
        g.b,
        x,
        y,
        c.right - 20,
        y + 100,
        6,
        p.surface,
        p.border,
    );
    text(
        g.hdc,
        g.f.semibold,
        p.text,
        x + 16,
        y + 14,
        tr4(&[
            b"Vitesse d'actualisation en temps r\xE9el",
            b"Real time update speed",
            b"Velocidad de actualizaci\xF3n en tiempo real",
            b"Echtzeit-Aktualisierungsgeschwindigkeit",
        ]),
    );
    text_fit(
        g.hdc,
        g.f.caption,
        12,
        p.text2,
        x + 16,
        y + 34,
        tr4(&[
            b"Fr\xE9quence de relev\xE9 des mesures et des graphiques.",
            b"How often measurements and graphs are refreshed.",
            b"Frecuencia con la que se actualizan medidas y gr\xE1ficos.",
            b"Wie oft Messwerte und Diagramme aktualisiert werden.",
        ]),
        c.right - x - 56,
    );
    let mut cx = x + 16;
    for (i, (ms, label)) in SPEEDS.iter().enumerate() {
        let on = if *ms == 0 {
            sm::PAUSED
        } else {
            !sm::PAUSED && sm::INTERVAL_MS == *ms
        };
        cx += ui::chip(g, h, cx, y + 58, ID_SPEED + i as u16, tr4(label), on);
    }
}

// ---------- Souris ----------

unsafe fn row_target(id: u16) -> Option<u32> {
    let slot = id.checked_sub(ID_ROW)? as usize;
    (slot < ROW_COUNT)
        .then(|| (*core::ptr::addr_of!(ROWS))[slot])
        .filter(|&v| v < GROUP)
}

unsafe fn switch_tab(tab: usize) {
    if TAB != tab {
        TAB = tab;
        SCROLL = 0;
        SORT_COL = -1;
        SORT_DESC = true;
        SELECTED_ITEM = usize::MAX;
        SEARCH_FOCUS = false;
        STARTUP_LOADED = false;
    }
}

unsafe fn ask_end(pid: u32) {
    if pid == GetCurrentProcessId() {
        say(tr4(&[
            b"Fermez le Gestionnaire des t\xE2ches avec sa fen\xEAtre.",
            b"Close Task Manager with its window.",
            b"Cierre el Administrador de tareas con su ventana.",
            b"Schlie\xDFen Sie den Task-Manager \xFCber sein Fenster.",
        ]));
        return;
    }
    DIALOG = Dialog::EndTask(pid);
}

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
            if DIALOG != Dialog::None || MENU_PID != 0 {
                return false;
            }
            let total = match TAB {
                TAB_STARTUP => sm::STARTUP_COUNT,
                TAB_HISTORY => APP_COUNT,
                TAB_SERVICES => sm::SERVICE_COUNT,
                _ => sm::COUNT + 3,
            };
            let max = total.saturating_sub(VISIBLE_ROWS);
            let next = (SCROLL as i32 - delta * 3).clamp(0, max as i32) as usize;
            let changed = next != SCROLL;
            SCROLL = next;
            changed
        }
        Mouse::RightDown => {
            let Some(id) = h.find(mx, my) else {
                return false;
            };
            if (TAB == TAB_PROCESSES || TAB == TAB_DETAILS) && DIALOG == Dialog::None {
                if let Some(pid) = row_target(id) {
                    SELECTED_PID = pid;
                    MENU_PID = pid;
                    MENU_X = mx;
                    MENU_Y = my;
                    return true;
                }
            }
            false
        }
        Mouse::Down | Mouse::DoubleClick => {
            match h.find(mx, my) {
                Some(id) => click(id, ev == Mouse::DoubleClick),
                None => SEARCH_FOCUS = false,
            }
            true
        }
        _ => false,
    }
}

unsafe fn click(id: u16, double: bool) {
    // Boîte de dialogue ouverte : seuls ses boutons répondent.
    if DIALOG != Dialog::None {
        match (id, DIALOG) {
            (ID_DLG_OK, Dialog::EndTask(pid)) => {
                DIALOG = Dialog::None;
                end_task(pid);
            }
            (ID_DLG_OK, Dialog::Run) if RUN.len > 0 => {
                DIALOG = Dialog::None;
                run_task();
            }
            (ID_DLG_CANCEL, _) => DIALOG = Dialog::None,
            _ => {}
        }
        return;
    }
    if MENU_PID != 0 {
        let pid = MENU_PID;
        MENU_PID = 0;
        match id.wrapping_sub(ID_MENU) {
            0 => ask_end(pid),
            k @ 1..=5 => set_priority(pid, k as usize - 1),
            6 => {
                switch_tab(TAB_DETAILS);
                SELECTED_PID = pid;
                reveal_details(pid);
            }
            _ => {}
        }
        return;
    }
    SEARCH_FOCUS = id == ID_SEARCH;
    match id {
        ID_HAMBURGER => NAV_EXPANDED = !NAV_EXPANDED,
        i if (ID_NAV..ID_NAV + TAB_COUNT as u16).contains(&i) => switch_tab((i - ID_NAV) as usize),
        ID_END => {
            if SELECTED_PID != 0 {
                ask_end(SELECTED_PID);
            }
        }
        ID_RUN => {
            RUN.len = 0;
            DIALOG = Dialog::Run;
        }
        ID_STARTUP_TOGGLE => toggle_startup(),
        ID_HISTORY_CLEAR => {
            APP_COUNT = 0;
            SCROLL = 0;
        }
        i if (ID_COL..ID_COL + 10).contains(&i) => {
            let col = (i - ID_COL) as i32;
            if SORT_COL == col {
                SORT_DESC = !SORT_DESC;
            } else {
                SORT_COL = col;
                // Le nom se trie de A à Z, les valeurs du plus grand au plus petit.
                SORT_DESC = col != 0;
            }
        }
        i if (ID_PERF..ID_PERF + 12).contains(&i) => PERF_SEL = (i - ID_PERF) as usize,
        i if (ID_SPEED..ID_SPEED + 4).contains(&i) => {
            let (ms, _) = SPEEDS[(i - ID_SPEED) as usize];
            sm::PAUSED = ms == 0;
            if ms != 0 {
                sm::INTERVAL_MS = ms;
            }
        }
        i if i >= ID_ROW => match TAB {
            TAB_PROCESSES | TAB_DETAILS => {
                if let Some(pid) = row_target(i) {
                    SELECTED_PID = pid;
                    if double && TAB == TAB_PROCESSES {
                        switch_tab(TAB_DETAILS);
                        SELECTED_PID = pid;
                        reveal_details(pid);
                    }
                }
            }
            TAB_STARTUP => SELECTED_ITEM = SCROLL + (i - ID_ROW) as usize,
            _ => {}
        },
        _ => {}
    }
}

unsafe fn toggle_startup() {
    let list = &*core::ptr::addr_of!(sm::STARTUP);
    let Some(e) = list[..sm::STARTUP_COUNT].get(SELECTED_ITEM).copied() else {
        return;
    };
    if !sm::set_startup(SELECTED_ITEM, !e.enabled) {
        say(tr4(&[
            b"Impossible d'enregistrer ce r\xE9glage.",
            b"Could not save this setting.",
            b"No se pudo guardar este ajuste.",
            b"Diese Einstellung konnte nicht gespeichert werden.",
        ]));
    }
    // La liste a été relue : on garde la même entrée choisie.
    let list = &*core::ptr::addr_of!(sm::STARTUP);
    SELECTED_ITEM = list[..sm::STARTUP_COUNT]
        .iter()
        .position(|x| x.file() == e.file())
        .unwrap_or(usize::MAX);
}

/// Fait défiler la liste Détails jusqu'au processus choisi.
unsafe fn reveal_details(pid: u32) {
    let n = sorted(&|_| true, &detail_key);
    let order = &*core::ptr::addr_of!(ORDER);
    let procs = &*core::ptr::addr_of!(sm::PROCS);
    if let Some(k) = order[..n]
        .iter()
        .position(|&i| procs[i as usize].pid == pid)
    {
        SCROLL = k.saturating_sub(3);
    }
}

// ---------- Clavier ----------

pub unsafe fn key(vk: u32, ch: u16) -> bool {
    let run = &mut *core::ptr::addr_of_mut!(RUN);
    let search = &mut *core::ptr::addr_of_mut!(SEARCH);
    if DIALOG == Dialog::Run && ch != 0 {
        return match ch {
            13 => {
                DIALOG = Dialog::None;
                run_task();
                true
            }
            27 => {
                DIALOG = Dialog::None;
                true
            }
            _ => run.key(ch),
        };
    }
    if SEARCH_FOCUS && ch != 0 {
        return match ch {
            27 => {
                SEARCH_FOCUS = false;
                true
            }
            13 => false,
            _ => {
                SCROLL = 0;
                search.key(ch)
            }
        };
    }
    let list_tab = TAB == TAB_PROCESSES || TAB == TAB_DETAILS;
    match vk {
        0x1B => {
            // Échap : ferme menu et boîte de dialogue.
            let open = MENU_PID != 0 || DIALOG != Dialog::None;
            MENU_PID = 0;
            DIALOG = Dialog::None;
            open
        }
        0x0D => match DIALOG {
            Dialog::EndTask(pid) => {
                DIALOG = Dialog::None;
                end_task(pid);
                true
            }
            _ => false,
        },
        0x2E if DIALOG == Dialog::None && list_tab && SELECTED_PID != 0 => {
            ask_end(SELECTED_PID);
            true
        }
        0x26 | 0x28 if DIALOG == Dialog::None && list_tab => {
            // Haut/Bas : processus précédent ou suivant parmi les lignes affichées.
            let rows = &*core::ptr::addr_of!(ROWS);
            let pids = &rows[..ROW_COUNT];
            let cur = pids.iter().position(|&p| p == SELECTED_PID);
            let next = match (cur, vk) {
                (None, _) => pids.iter().position(|&p| p < GROUP),
                (Some(i), 0x26) => (0..i).rev().find(|&k| pids[k] < GROUP),
                (Some(i), _) => (i + 1..pids.len()).find(|&k| pids[k] < GROUP),
            };
            match next {
                Some(k) => SELECTED_PID = pids[k],
                None if vk == 0x26 && SCROLL > 0 => SCROLL -= 1,
                None if vk == 0x28 => SCROLL += 1,
                None => return false,
            }
            true
        }
        0x09 if DIALOG == Dialog::None => {
            let mut next = (TAB + 1) % TAB_COUNT;
            while !tab_visible(next) {
                next = (next + 1) % TAB_COUNT;
            }
            switch_tab(next);
            true
        }
        _ => false,
    }
}

pub unsafe fn tick(_f: &Frame) -> bool {
    let expired = MESSAGE.len > 0 && GetTickCount().wrapping_sub(MESSAGE_AT) >= 6000;
    if expired {
        MESSAGE.len = 0;
    }
    refresh() || expired
}
