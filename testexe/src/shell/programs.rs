//! « Applications installées » : liste des programmes Windows inscrits dans
//! le registre (clés `Uninstall` : paquets .msi, installateurs NSIS et Inno
//! Setup, programmes qui s'inscrivent eux-mêmes), recherche, tri, et
//! désinstallation en un clic.
//!
//! Le runtime fait le travail : `winlux --setup --programs --out FICHIER`
//! écrit la liste, `winlux --setup --remove CLÉ --out FICHIER` désinstalle
//! avec le bon outil (msiexec intégré, journal d'installation ou
//! désinstalleur de l'application) puis réécrit la liste.

use crate::gfx::*;
use crate::store::{
    button, contains_ci, cp1252_to_utf8, read_file, spawn, utf8_to_cp1252, wrap, Buf,
};
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

const OUT: &core::ffi::CStr = c"C:\\.local\\share\\winlux\\programs.txt";
const OUT_ARG: &[u8] = b"--out \"C:\\.local\\share\\winlux\\programs.txt\"";

const CAP: usize = 128 * 1024;
const MAX_APPS: usize = 600;
const FIELDS: usize = 7;
static mut DATA: [u8; CAP] = [0; CAP];
static mut DATA_LEN: usize = 0;
/// Clé, nom, version, éditeur, taille (Kio), date, type : positions dans DATA.
static mut APPS: [[(u32, u16); FIELDS]; MAX_APPS] = [[(0, 0); FIELDS]; MAX_APPS];
static mut APP_COUNT: usize = 0;
/// Ordre d'affichage (indices dans APPS) après recherche et tri.
static mut ORDER: [u16; MAX_APPS] = [0; MAX_APPS];
static mut SHOWN: usize = 0;

static mut LOADED: bool = false;
static mut WAITING: bool = false;
static mut WAIT_STARTED: u32 = 0;
/// Désinstallation en cours : nom de l'application.
static mut REMOVING: Buf<128> = Buf::new();
static mut CONFIRM: usize = usize::MAX;
static mut SEARCH: Buf<40> = Buf::new();
static mut SEARCH_FOCUS: bool = false;
static mut SORT: Sort = Sort::Name;
static mut SCROLL: i32 = 0;
static mut MX: i32 = -1;
static mut MY: i32 = -1;

#[derive(Clone, Copy, PartialEq)]
enum Sort {
    Name,
    Size,
    Date,
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    None,
    Search,
    Sort(Sort),
    Refresh,
    Uninstall(u16),
    Cancel,
}
const MAX_HITS: usize = 160;
static mut HITS: [(Rect, Action); MAX_HITS] = [(rect(0, 0, 0, 0), Action::None); MAX_HITS];
static mut HIT_COUNT: usize = 0;

unsafe fn hit(r: Rect, a: Action) {
    if HIT_COUNT < MAX_HITS {
        HITS[HIT_COUNT] = (r, a);
        HIT_COUNT += 1;
    }
}
unsafe fn hot(r: &Rect) -> bool {
    inside(r, MX, MY)
}
unsafe fn field(app: usize, k: usize) -> &'static [u8] {
    let (o, n) = APPS[app][k];
    &DATA[o as usize..o as usize + n as usize]
}
fn number(s: &[u8]) -> u64 {
    s.iter()
        .take_while(|c| c.is_ascii_digit())
        .fold(0u64, |v, &c| v * 10 + (c - b'0') as u64)
}

const ROW_H: i32 = 64;

/// Lance le runtime ; `verb` est `--programs` ou `--remove "clé"`.
unsafe fn run(verb: &[u8]) -> bool {
    let mut args: Buf<700> = Buf::new();
    args.push(b"--setup ").push(verb).push(b" ").push(OUT_ARG);
    let mut raw = [0u8; 1100];
    let n = cp1252_to_utf8(args.get(), &mut raw);
    DeleteFileA(OUT.as_ptr());
    if spawn(c"winlux", &raw[..n]) {
        WAITING = true;
        WAIT_STARTED = GetTickCount();
        true
    } else {
        false
    }
}

pub unsafe fn open() {
    CONFIRM = usize::MAX;
    if !WAITING {
        run(b"--programs");
    }
    crate::wm::open(crate::wm::APP_PROGRAMS);
}

/// Lit le fichier du runtime ; vrai quand il est complet.
unsafe fn load() -> bool {
    let n = read_file(OUT, &mut DATA);
    if n == 0 || !DATA[..n].windows(6).any(|w| w == b"done=1") {
        return false;
    }
    DATA_LEN = utf8_to_cp1252(&mut DATA, n);
    APP_COUNT = 0;
    let mut code: Option<u64> = None;
    let mut pos = 0;
    while pos < DATA_LEN {
        let end = DATA[pos..DATA_LEN]
            .iter()
            .position(|&c| c == b'\n')
            .map_or(DATA_LEN, |k| pos + k);
        let line = &DATA[pos..end];
        if let Some(v) = line.strip_prefix(b"code=") {
            code = Some(number(v));
        } else if line.starts_with(b"app=") && APP_COUNT < MAX_APPS {
            let mut start = pos + 4;
            let mut fields = [(0u32, 0u16); FIELDS];
            for (k, slot) in fields.iter_mut().enumerate() {
                let stop = if k == FIELDS - 1 {
                    end
                } else {
                    DATA[start..end]
                        .iter()
                        .position(|&c| c == b'\t')
                        .map_or(end, |p| start + p)
                };
                *slot = (start as u32, (stop - start) as u16);
                start = (stop + 1).min(end);
            }
            APPS[APP_COUNT] = fields;
            APP_COUNT += 1;
        }
        pos = end + 1;
    }
    if REMOVING.n > 0 {
        let mut msg: Buf<200> = Buf::new();
        msg.push(REMOVING.get());
        if code == Some(0) {
            msg.push(tr4(&[
                b" a \xE9t\xE9 d\xE9sinstall\xE9.",
                b" was uninstalled.",
                b" se ha desinstalado.",
                b" wurde deinstalliert.",
            ]));
        } else {
            msg.push(tr4(&[
                b" : la d\xE9sinstallation a \xE9chou\xE9.",
                b": uninstall failed.",
                b": no se pudo desinstalar.",
                b": Deinstallation fehlgeschlagen.",
            ]));
        }
        crate::desktop::toast(msg.get());
        REMOVING.n = 0;
    }
    LOADED = true;
    refilter();
    true
}

unsafe fn refilter() {
    SHOWN = 0;
    for k in 0..APP_COUNT {
        if SEARCH.n == 0
            || contains_ci(field(k, 1), SEARCH.get())
            || contains_ci(field(k, 3), SEARCH.get())
        {
            ORDER[SHOWN] = k as u16;
            SHOWN += 1;
        }
    }
    // Le runtime trie déjà par nom ; tri stable décroissant pour le reste.
    let col = match SORT {
        Sort::Name => 0,
        Sort::Size => 4,
        Sort::Date => 5,
    };
    if col != 0 {
        for i in 1..SHOWN {
            let k = ORDER[i];
            let v = number(field(k as usize, col));
            let mut j = i;
            while j > 0 && number(field(ORDER[j - 1] as usize, col)) < v {
                ORDER[j] = ORDER[j - 1];
                j -= 1;
            }
            ORDER[j] = k;
        }
    }
    CONFIRM = usize::MAX;
}

/// « 12,3 Mo » à partir d'une taille en Kio.
fn size_text(kb: u64, out: &mut Buf<24>) {
    if kb >= 1024 * 1024 {
        let tenth = kb * 10 / (1024 * 1024);
        out.num(tenth / 10)
            .push(b",")
            .num(tenth % 10)
            .push(tr4(&[b" Go", b" GB", b" GB", b" GB"]));
    } else if kb >= 1024 {
        let tenth = kb * 10 / 1024;
        out.num(tenth / 10)
            .push(b",")
            .num(tenth % 10)
            .push(tr4(&[b" Mo", b" MB", b" MB", b" MB"]));
    } else {
        out.num(kb).push(tr4(&[b" Ko", b" KB", b" KB", b" KB"]));
    }
}

/// AAAAMMJJ → JJ/MM/AAAA (ou AAAA-MM-JJ en anglais).
fn date_text(d: &[u8], out: &mut Buf<24>) {
    if d.len() != 8 || !d.iter().all(|c| c.is_ascii_digit()) {
        return;
    }
    if ui_lang() == 1 {
        out.push(&d[..4])
            .push(b"-")
            .push(&d[4..6])
            .push(b"-")
            .push(&d[6..8]);
    } else if ui_lang() == 3 {
        out.push(&d[6..8])
            .push(b".")
            .push(&d[4..6])
            .push(b".")
            .push(&d[..4]);
    } else {
        out.push(&d[6..8])
            .push(b"/")
            .push(&d[4..6])
            .push(b"/")
            .push(&d[..4]);
    }
}

fn kind_text(kind: &[u8]) -> &'static [u8] {
    match kind {
        b"msi" => b"Windows Installer",
        b"nsis" => b"NSIS",
        b"inno" => b"Inno Setup",
        _ => tr4(&[b"Programme", b"Program", b"Programa", b"Programm"]),
    }
}

unsafe fn search_box(g: &mut Gfx, r: Rect) {
    let p = pal();
    let bg = if SEARCH_FOCUS { p.surface } else { p.surface2 };
    panel(
        g.hdc,
        g.b,
        r.left,
        r.top,
        r.right,
        r.bottom,
        5,
        bg,
        if SEARCH_FOCUS { p.accent } else { p.border },
    );
    if SEARCH.n == 0 {
        text(
            g.hdc,
            g.f.body,
            p.text3,
            r.left + 10,
            r.top + 8,
            tr4(&[
                b"Rechercher une application",
                b"Search apps",
                b"Buscar aplicaciones",
                b"Apps suchen",
            ]),
        );
    } else {
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            r.left + 10,
            r.top + 8,
            SEARCH.get(),
            r.right - r.left - 20,
        );
    }
    if SEARCH_FOCUS && (GetTickCount() / 500) % 2 == 0 {
        let cx = r.left + 10 + text_width(SEARCH.get(), 13).min(r.right - r.left - 20);
        fill(g.hdc, g.b, cx, r.top + 7, cx + 1, r.bottom - 7, p.text);
    }
    hit(r, Action::Search);
}

unsafe fn chip(g: &mut Gfx, x: i32, y: i32, label: &[u8], on: bool, a: Action) -> i32 {
    let p = pal();
    let w = text_width(label, 12) + 24;
    let r = rect(x, y, x + w, y + 28);
    round_rect(
        g.hdc,
        g.b,
        r.left,
        r.top,
        r.right,
        r.bottom,
        14,
        if on {
            p.accent
        } else if hot(&r) {
            p.hover
        } else {
            p.surface2
        },
    );
    text(
        g.hdc,
        g.f.caption,
        if on { p.on_accent } else { p.text },
        x + 12,
        y + 7,
        label,
    );
    hit(r, a);
    x + w + 8
}

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    HIT_COUNT = 0;
    let p = pal();
    let c = f.client;
    fill(g.hdc, g.b, c.left, c.top, c.right, c.bottom, p.window);
    set_glyph_bg(Some(p.window));
    let left = c.left + 32;
    let right = c.right - 32;
    glyph(g.hdc, g.b, g.f, Glyph::Exe, left, c.top + 22, 22);
    text(
        g.hdc,
        g.f.title,
        p.text,
        left + 34,
        c.top + 22,
        tr4(&[
            b"Applications install\xE9es",
            b"Installed apps",
            b"Aplicaciones instaladas",
            b"Installierte Apps",
        ]),
    );
    let mut count: Buf<60> = Buf::new();
    count.num(APP_COUNT as u64).push(tr4(&[
        b" applications Windows",
        b" Windows apps",
        b" aplicaciones de Windows",
        b" Windows-Apps",
    ]));
    text(
        g.hdc,
        g.f.caption,
        p.text3,
        left + 34,
        c.top + 46,
        count.get(),
    );

    let sw = ((right - left) / 2).clamp(220, 380);
    search_box(g, rect(left, c.top + 76, left + sw, c.top + 108));
    let mut x = left + sw + 16;
    text(
        g.hdc,
        g.f.caption,
        p.text2,
        x,
        c.top + 84,
        tr4(&[b"Trier :", b"Sort:", b"Ordenar:", b"Sortieren:"]),
    );
    x += text_width(tr4(&[b"Trier :", b"Sort:", b"Ordenar:", b"Sortieren:"]), 12) + 10;
    x = chip(
        g,
        x,
        c.top + 78,
        tr4(&[b"Nom", b"Name", b"Nombre", b"Name"]),
        SORT == Sort::Name,
        Action::Sort(Sort::Name),
    );
    x = chip(
        g,
        x,
        c.top + 78,
        tr4(&[b"Taille", b"Size", b"Tama\xF1o", b"Gr\xF6\xDFe"]),
        SORT == Sort::Size,
        Action::Sort(Sort::Size),
    );
    x = chip(
        g,
        x,
        c.top + 78,
        tr4(&[
            b"Date d'installation",
            b"Install date",
            b"Fecha de instalaci\xF3n",
            b"Installationsdatum",
        ]),
        SORT == Sort::Date,
        Action::Sort(Sort::Date),
    );
    let label = tr4(&[b"Actualiser", b"Refresh", b"Actualizar", b"Aktualisieren"]);
    let bw = text_width(label, 13) * 108 / 100 + 28;
    if x + bw < right {
        let probe = rect(right - bw, c.top + 76, right, c.top + 108);
        let r = button(g, right - bw, c.top + 76, label, false, hot(&probe));
        hit(r, Action::Refresh);
    }

    let top = c.top + 124;
    let bottom = c.bottom - 16;
    if !LOADED {
        text(
            g.hdc,
            g.f.body,
            p.text2,
            left,
            top + 8,
            tr4(&[
                b"Recherche des applications...",
                b"Looking for apps...",
                b"Buscando aplicaciones...",
                b"Apps werden gesucht...",
            ]),
        );
        set_glyph_bg(None);
        return;
    }
    if SHOWN == 0 {
        let msg = if APP_COUNT == 0 {
            tr4(&[
                b"Aucune application Windows install\xE9e. Double-cliquez sur un installateur .exe ou .msi pour en ajouter une.",
                b"No Windows apps installed. Double-click an .exe or .msi installer to add one.",
                b"No hay aplicaciones de Windows instaladas. Haga doble clic en un instalador .exe o .msi para agregar una.",
                b"Keine Windows-Apps installiert. Doppelklicken Sie auf ein .exe- oder .msi-Installationsprogramm.",
            ])
        } else {
            tr4(&[
                b"Aucune application ne correspond \xE0 la recherche.",
                b"No apps match your search.",
                b"Ninguna aplicaci\xF3n coincide con la b\xFAsqueda.",
                b"Keine App entspricht der Suche.",
            ])
        };
        wrap(
            g,
            g.f.body,
            13,
            p.text2,
            left,
            top + 8,
            right - left,
            msg,
            3,
            bottom,
        );
        set_glyph_bg(None);
        return;
    }
    let mut y = top - SCROLL;
    for &k in ORDER[..SHOWN].iter() {
        let k = k as usize;
        if y + ROW_H <= top {
            y += ROW_H + 6;
            continue;
        }
        if y + ROW_H > bottom {
            break;
        }
        let r = rect(left, y, right, y + ROW_H);
        let bg = if hot(&r) { p.hover } else { p.surface };
        panel(
            g.hdc, g.b, r.left, r.top, r.right, r.bottom, 6, bg, p.border,
        );
        set_glyph_bg(Some(bg));
        glyph(g.hdc, g.b, g.f, Glyph::Exe, r.left + 16, r.top + 20, 22);
        // Bouton à droite.
        let busy = WAITING && REMOVING.n > 0;
        let label = if CONFIRM == k {
            tr4(&[b"Confirmer", b"Confirm", b"Confirmar", b"Best\xE4tigen"])
        } else {
            tr4(&[
                b"D\xE9sinstaller",
                b"Uninstall",
                b"Desinstalar",
                b"Deinstallieren",
            ])
        };
        let bw = text_width(label, 13) * 108 / 100 + 28;
        let bx = r.right - bw - 14;
        if !busy {
            let probe = rect(bx, r.top + 16, bx + bw, r.top + 48);
            let b = button(g, bx, r.top + 16, label, CONFIRM == k, hot(&probe));
            hit(b, Action::Uninstall(k as u16));
            if CONFIRM == k {
                let cl = tr4(&[b"Annuler", b"Cancel", b"Cancelar", b"Abbrechen"]);
                let cw = text_width(cl, 13) * 108 / 100 + 28;
                let probe = rect(bx - cw - 8, r.top + 16, bx - 8, r.top + 48);
                let b = button(g, bx - cw - 8, r.top + 16, cl, false, hot(&probe));
                hit(b, Action::Cancel);
            }
        }
        // Taille et date, alignées à droite avant le bouton.
        let info_right = bx - if CONFIRM == k { 140 } else { 24 };
        let mut size: Buf<24> = Buf::new();
        let kb = number(field(k, 4));
        if kb > 0 {
            size_text(kb, &mut size);
        }
        let mut date: Buf<24> = Buf::new();
        date_text(field(k, 5), &mut date);
        let tw = text_width(size.get(), 13);
        text(
            g.hdc,
            g.f.body,
            p.text,
            info_right - tw,
            r.top + 14,
            size.get(),
        );
        let dw = text_width(date.get(), 11);
        text(
            g.hdc,
            g.f.small,
            p.text3,
            info_right - dw,
            r.top + 36,
            date.get(),
        );
        // Nom, éditeur, version, type.
        let name_w = info_right - 120 - (r.left + 52);
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            r.left + 52,
            r.top + 13,
            field(k, 1),
            name_w.max(60),
        );
        let mut sub: Buf<200> = Buf::new();
        if !field(k, 3).is_empty() {
            sub.push(field(k, 3)).push(b"  \x95  ");
        }
        if !field(k, 2).is_empty() {
            sub.push(field(k, 2)).push(b"  \x95  ");
        }
        sub.push(kind_text(field(k, 6)));
        text_fit(
            g.hdc,
            g.f.small,
            11,
            p.text3,
            r.left + 52,
            r.top + 36,
            sub.get(),
            name_w.max(60),
        );
        y += ROW_H + 6;
    }
    if WAITING && REMOVING.n > 0 {
        let mut msg: Buf<200> = Buf::new();
        msg.push(tr4(&[
            b"D\xE9sinstallation de ",
            b"Uninstalling ",
            b"Desinstalando ",
            b"Deinstallation von ",
        ]))
        .push(REMOVING.get())
        .push(b"...");
        set_glyph_bg(Some(p.window));
        text(g.hdc, g.f.caption, p.accent, left, c.bottom - 14, msg.get());
    }
    set_glyph_bg(None);
}

unsafe fn act(a: Action) {
    match a {
        Action::None => {}
        Action::Search => SEARCH_FOCUS = true,
        Action::Sort(s) => {
            SORT = s;
            SCROLL = 0;
            refilter();
        }
        Action::Refresh => {
            if !WAITING {
                run(b"--programs");
            }
        }
        Action::Cancel => CONFIRM = usize::MAX,
        Action::Uninstall(k) => {
            let k = k as usize;
            if WAITING || k >= APP_COUNT {
                return;
            }
            if CONFIRM != k {
                CONFIRM = k;
                return;
            }
            CONFIRM = usize::MAX;
            let mut verb: Buf<600> = Buf::new();
            verb.push(b"--remove \"").push(field(k, 0)).push(b"\"");
            let mut name: Buf<128> = Buf::new();
            name.push(field(k, 1));
            if run(verb.get()) {
                REMOVING = name;
            } else {
                crate::desktop::toast(tr4(&[
                    b"Winlux ne peut pas lancer la d\xE9sinstallation.",
                    b"Winlux cannot start the uninstall.",
                    b"Winlux no puede iniciar la desinstalaci\xF3n.",
                    b"Winlux kann die Deinstallation nicht starten.",
                ]));
            }
        }
    }
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    match ev {
        Mouse::Move => {
            let changed =
                (0..HIT_COUNT).any(|k| inside(&HITS[k].0, x, y) != inside(&HITS[k].0, MX, MY));
            MX = x;
            MY = y;
            changed || inside(&f.client, x, y)
        }
        Mouse::Down | Mouse::DoubleClick => {
            MX = x;
            MY = y;
            if !inside(&f.client, x, y) {
                return false;
            }
            let mut chosen = Action::None;
            for k in (0..HIT_COUNT).rev() {
                if inside(&HITS[k].0, x, y) {
                    chosen = HITS[k].1;
                    break;
                }
            }
            if !matches!(chosen, Action::Search) {
                SEARCH_FOCUS = false;
            }
            if !matches!(chosen, Action::Uninstall(_) | Action::Cancel) {
                CONFIRM = usize::MAX;
            }
            act(chosen);
            true
        }
        Mouse::Wheel(delta) => {
            let visible = f.client.bottom - 16 - (f.client.top + 124);
            let max = (SHOWN as i32 * (ROW_H + 6) - visible).max(0);
            SCROLL = (SCROLL - delta / 120 * (ROW_H + 6)).clamp(0, max);
            true
        }
        _ => false,
    }
}

pub unsafe fn key(vk: u32, ch: u16) -> bool {
    if ch != 0 {
        if ch == 27 {
            SEARCH_FOCUS = false;
            CONFIRM = usize::MAX;
            return true;
        }
        if !SEARCH_FOCUS && ch < 0x20 {
            return false;
        }
        SEARCH_FOCUS = true;
        match ch {
            8 => SEARCH.n = SEARCH.n.saturating_sub(1),
            13 => SEARCH_FOCUS = false,
            c if (0x20..0x100).contains(&c) => {
                SEARCH.push(&[c as u8]);
            }
            _ => return false,
        }
        SCROLL = 0;
        refilter();
        return true;
    }
    if vk == 0x74 {
        // F5
        act(Action::Refresh);
        return true;
    }
    false
}

pub unsafe fn tick(_f: &Frame) -> bool {
    let mut repaint = SEARCH_FOCUS;
    if WAITING {
        repaint = true;
        if load() {
            WAITING = false;
        } else if GetTickCount().wrapping_sub(WAIT_STARTED) > 120_000 {
            // Désinstalleur bloqué ou runtime absent : on rend la main.
            WAITING = false;
            REMOVING.n = 0;
            LOADED = true;
        }
    }
    repaint
}
