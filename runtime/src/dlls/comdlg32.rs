//! `comdlg32` : boîtes d'impression natives (`PrintDlg`, `PrintDlgEx`,
//! `PageSetupDlg`) dessinées par Winlux au thème Windows 11 (aperçu à gauche,
//! réglages à droite, bouton « Imprimer » accentué).
//!
//! Sans affichage (tests, services), la boîte suit `WINLUX_PRINTDLG` :
//! `accept` valide les réglages proposés, toute autre valeur annule.

use crate::dlls::gui::{self, DrawCmd, FontSpec, Rect};
use crate::dlls::kernel32;
use crate::dlls::lock;
use crate::dlls::winspool::{self, DevMode, PrinterEntry};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

const W: i32 = 700;
const H: i32 = 540;
const PREVIEW_W: i32 = 300;
const LABEL_X: i32 = 320;
const CTRL_X: i32 = 432;
const CTRL_R: i32 = W - 20;
const ROW_H: i32 = 32;
const ROW_STEP: i32 = 42;
const FIRST_ROW: i32 = 70;

const CDERR_INITIALIZATION: u32 = 0x0002;
const CDERR_STRUCTSIZE: u32 = 0x0001;
const PDERR_NODEFAULTPRN: u32 = 0x1008;
const PDERR_PRINTERNOTFOUND: u32 = 0x100B;

static LAST_ERROR: AtomicU32 = AtomicU32::new(0);

pub extern "win64" fn comm_dlg_extended_error() -> u32 {
    LAST_ERROR.load(Ordering::Relaxed)
}

fn fail(code: u32) -> i32 {
    LAST_ERROR.store(code, Ordering::Relaxed);
    0
}

// ---------- État de la boîte ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Boîte « Imprimer » complète.
    Print,
    /// « Configuration de l'impression » / propriétés : sans copies ni pages.
    Setup,
    /// Propriétés d'une imprimante donnée (imprimante non modifiable).
    Properties,
}

#[derive(Clone, Debug)]
pub struct Options {
    printers: Vec<PrinterEntry>,
    selected: usize,
    pub devmode: DevMode,
    pub range: bool,
    pub from: u32,
    pub to: u32,
    pub min_page: u32,
    pub max_page: u32,
    allow_pages: bool,
    mode: Mode,
}

impl Options {
    pub fn printer(&self) -> &PrinterEntry {
        &self.printers[self.selected]
    }

    fn select(&mut self, i: usize) {
        if i >= self.printers.len() || i == self.selected {
            return;
        }
        self.selected = i;
        let p = &self.printers[i];
        // Réglages conservés quand la nouvelle imprimante les accepte.
        let mut dm = DevMode::for_printer(p);
        dm.orientation = self.devmode.orientation;
        dm.copies = self.devmode.copies;
        dm.collate = self.devmode.collate;
        dm.quality = self.devmode.quality;
        if p.papers.contains(&self.devmode.paper) {
            dm.paper = self.devmode.paper;
        }
        if p.duplex {
            dm.duplex = self.devmode.duplex;
        }
        if p.color {
            dm.color = self.devmode.color;
        }
        self.devmode = dm;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    PrinterList,
    PaperList,
    Pick(usize),
    CopiesDec,
    CopiesInc,
    Orientation(i16),
    PagesAll,
    PagesRange,
    FromDec,
    FromInc,
    ToDec,
    ToInc,
    Color(i16),
    Quality(i16),
    Duplex(i16),
    Print,
    Cancel,
    ClosePopup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Popup {
    Printers,
    Papers,
}

struct Ui {
    opts: Options,
    popup: Option<Popup>,
    zones: Vec<(Rect, Action)>,
    hover: Option<Action>,
    result: Option<bool>,
    dark: bool,
    hwnd: usize,
}

static UI: Mutex<Option<Ui>> = Mutex::new(None);

impl Ui {
    /// Applique une action ; `true` si l'affichage change.
    fn apply(&mut self, a: Action) -> bool {
        let o = &mut self.opts;
        let popup = self.popup.take();
        match a {
            Action::PrinterList if o.mode != Mode::Properties => {
                self.popup = if popup == Some(Popup::Printers) {
                    None
                } else {
                    Some(Popup::Printers)
                }
            }
            Action::PaperList => {
                self.popup = if popup == Some(Popup::Papers) {
                    None
                } else {
                    Some(Popup::Papers)
                }
            }
            Action::Pick(i) => match popup {
                Some(Popup::Printers) => o.select(i),
                Some(Popup::Papers) => {
                    if let Some(id) = o.printer().papers.get(i) {
                        o.devmode.paper = *id;
                    }
                }
                None => {}
            },
            Action::CopiesDec => o.devmode.copies = (o.devmode.copies - 1).max(1),
            Action::CopiesInc => o.devmode.copies = (o.devmode.copies + 1).min(999),
            Action::Orientation(v) => o.devmode.orientation = v,
            Action::PagesAll => o.range = false,
            Action::PagesRange if o.allow_pages => o.range = true,
            Action::FromDec => o.from = o.from.saturating_sub(1).max(o.min_page),
            Action::FromInc => {
                o.from = (o.from + 1).min(o.max_page);
                o.to = o.to.max(o.from);
            }
            Action::ToDec => {
                o.to = o.to.saturating_sub(1).max(o.min_page);
                o.from = o.from.min(o.to);
            }
            Action::ToInc => o.to = (o.to + 1).min(o.max_page),
            Action::Color(v) if o.printer().color || v == 1 => o.devmode.color = v,
            Action::Quality(v) => o.devmode.quality = v,
            Action::Duplex(v) if o.printer().duplex || v == 1 => o.devmode.duplex = v,
            Action::Print => self.result = Some(true),
            Action::Cancel => self.result = Some(false),
            _ => {}
        }
        true
    }
}

// ---------- Dessin ----------

struct Theme {
    mica: u32,
    pane: u32,
    surface: u32,
    hover: u32,
    border: u32,
    text: u32,
    text2: u32,
    accent: u32,
    on_accent: u32,
    selected: u32,
}

fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}

fn theme(dark: bool) -> Theme {
    if dark {
        Theme {
            mica: rgb(0x20, 0x20, 0x20),
            pane: rgb(0x1A, 0x1A, 0x1A),
            surface: rgb(0x2D, 0x2D, 0x2D),
            hover: rgb(0x38, 0x38, 0x38),
            border: rgb(0x45, 0x45, 0x45),
            text: rgb(0xF2, 0xF2, 0xF2),
            text2: rgb(0xB8, 0xB8, 0xB8),
            accent: rgb(0x4C, 0xC2, 0xFF),
            on_accent: rgb(0x00, 0x00, 0x00),
            selected: rgb(0x23, 0x3A, 0x4A),
        }
    } else {
        Theme {
            mica: rgb(0xF3, 0xF3, 0xF3),
            pane: rgb(0xE8, 0xEB, 0xF0),
            surface: rgb(0xFF, 0xFF, 0xFF),
            hover: rgb(0xF0, 0xF0, 0xF0),
            border: rgb(0xD6, 0xD6, 0xD6),
            text: rgb(0x1A, 0x1A, 0x1A),
            text2: rgb(0x5C, 0x5C, 0x5C),
            accent: rgb(0x00, 0x67, 0xC0),
            on_accent: rgb(0xFF, 0xFF, 0xFF),
            selected: rgb(0xD9, 0xE8, 0xF6),
        }
    }
}

/// Thème clair/sombre choisi dans Paramètres (`theme=dark`).
fn shell_dark() -> bool {
    std::fs::read_to_string("/run/winlux/control.txt")
        .map(|t| t.lines().any(|l| l.trim() == "theme=dark"))
        .unwrap_or(false)
}

fn rect(l: i32, t: i32, r: i32, b: i32) -> Rect {
    Rect {
        left: l,
        top: t,
        right: r,
        bottom: b,
    }
}

fn inside(r: &Rect, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

fn font(size: i32, weight: i32) -> FontSpec {
    FontSpec {
        height: -size,
        weight,
        face: "DejaVu Sans".into(),
        ..FontSpec::default()
    }
}

struct Painter {
    cmds: Vec<DrawCmd>,
    zones: Vec<(Rect, Action)>,
}

impl Painter {
    fn fill(&mut self, r: Rect, color: u32) {
        self.cmds.push(DrawCmd::FillRect { rect: r, color });
    }

    fn round(&mut self, r: Rect, color: u32, radius: i32) {
        self.cmds.push(DrawCmd::RoundRect {
            rect: r,
            color,
            rx: radius,
            ry: radius,
        });
    }

    fn text(&mut self, x: i32, y: i32, s: &str, color: u32, size: i32, weight: i32) {
        self.cmds.push(DrawCmd::Text {
            x,
            y,
            text: s.to_string(),
            color,
            bk: None,
            font: font(size, weight),
        });
    }

    /// Texte centré horizontalement et verticalement dans `r`.
    fn text_center(&mut self, r: Rect, s: &str, color: u32, size: i32, weight: i32) {
        let (w, h) = gui::measure_text(&font(size, weight), s);
        self.text(
            r.left + (r.right - r.left - w) / 2,
            r.top + (r.bottom - r.top - h) / 2,
            s,
            color,
            size,
            weight,
        );
    }

    /// Contrôle arrondi bordé (fond `surface`), avec zone cliquable.
    fn control(&mut self, t: &Theme, r: Rect, hover: bool, action: Action) {
        self.round(r, t.border, 8);
        self.round(
            rect(r.left + 1, r.top + 1, r.right - 1, r.bottom - 1),
            if hover { t.hover } else { t.surface },
            7,
        );
        self.zones.push((r, action));
    }
}

fn ellipsize(s: &str, max_w: i32, size: i32) -> String {
    let f = font(size, 400);
    if gui::measure_text(&f, s).0 <= max_w {
        return s.to_string();
    }
    let mut out: String = s.to_string();
    while !out.is_empty() && gui::measure_text(&f, &format!("{out}…")).0 > max_w {
        out.pop();
    }
    format!("{out}…")
}

fn paint(ui: &Ui) -> (Vec<DrawCmd>, Vec<(Rect, Action)>) {
    let t = theme(ui.dark);
    let o = &ui.opts;
    let p = o.printer();
    let hover = |a: Action| ui.hover == Some(a);
    let mut g = Painter {
        cmds: Vec::new(),
        zones: Vec::new(),
    };
    g.fill(rect(0, 0, W, H), t.mica);
    // Volet d'aperçu.
    g.fill(rect(0, 0, PREVIEW_W, H), t.pane);
    let landscape = o.devmode.landscape();
    let paper = winspool::paper(o.devmode.paper);
    let (pw, ph) = if landscape {
        (paper.height, paper.width)
    } else {
        (paper.width, paper.height)
    };
    let scale = f64::from(PREVIEW_W - 60) / f64::from(pw.max(ph)).max(1.0);
    let sw = (f64::from(pw) * scale) as i32;
    let sh = (f64::from(ph) * scale) as i32;
    let sx = (PREVIEW_W - sw) / 2;
    let sy = 60 + (300 - sh) / 2;
    g.fill(rect(sx + 3, sy + 4, sx + sw + 3, sy + sh + 4), t.border);
    g.fill(rect(sx, sy, sx + sw, sy + sh), rgb(0xFF, 0xFF, 0xFF));
    let ink = if o.devmode.color == 1 {
        rgb(0x60, 0x60, 0x60)
    } else {
        t.accent
    };
    let m = sw / 10;
    g.fill(rect(sx + m, sy + m, sx + sw / 2, sy + m + 6), ink);
    let mut ly = sy + m + 18;
    while ly + 4 < sy + sh - m {
        let right = if (ly / 10) % 4 == 3 {
            sx + sw * 3 / 5
        } else {
            sx + sw - m
        };
        g.fill(rect(sx + m, ly, right, ly + 3), rgb(0xC8, 0xC8, 0xC8));
        ly += 10;
    }
    g.text(20, 24, "Aperçu", t.text, 15, 600);
    let pages = if o.range {
        format!("Pages {} à {}", o.from, o.to)
    } else if o.max_page > o.min_page && o.max_page < 0xFFFF {
        format!("{} pages", o.max_page - o.min_page + 1)
    } else {
        "Toutes les pages".to_string()
    };
    g.text(
        20,
        380,
        &format!("{}  ·  {}", paper.name, pages),
        t.text2,
        12,
        400,
    );
    g.text(
        20,
        402,
        &format!("État : {}", p.status_text()),
        t.text2,
        12,
        400,
    );
    if p.jobs > 0 {
        g.text(
            20,
            424,
            &format!("{} document(s) en attente", p.jobs),
            t.text2,
            12,
            400,
        );
    }
    if !p.location.is_empty() {
        g.text(
            20,
            446,
            &ellipsize(&format!("Emplacement : {}", p.location), PREVIEW_W - 30, 12),
            t.text2,
            12,
            400,
        );
    }
    // Volet des réglages.
    let title = match o.mode {
        Mode::Print => "Imprimer",
        Mode::Setup => "Configuration de l'impression",
        Mode::Properties => "Préférences d'impression",
    };
    g.text(LABEL_X, 22, title, t.text, 20, 600);
    let mut row = 0;
    let mut next_row = || {
        let y = FIRST_ROW + row * ROW_STEP;
        row += 1;
        y
    };
    let label = |g: &mut Painter, y: i32, s: &str| g.text(LABEL_X, y + 8, s, t.text, 13, 400);
    // Liste déroulante.
    let dropdown = |g: &mut Painter, y: i32, value: &str, action: Action| {
        let r = rect(CTRL_X, y, CTRL_R, y + ROW_H);
        g.control(&t, r, hover(action), action);
        g.text(
            r.left + 10,
            y + 8,
            &ellipsize(value, r.right - r.left - 36, 13),
            t.text,
            13,
            400,
        );
        g.text(r.right - 22, y + 7, "⌄", t.text2, 13, 400);
    };
    // Boutons segmentés.
    let segmented = |g: &mut Painter, y: i32, items: &[(&str, Action, bool, bool)]| {
        let n = items.len() as i32;
        let w = (CTRL_R - CTRL_X - (n - 1) * 4) / n;
        for (i, (s, a, on, enabled)) in items.iter().enumerate() {
            let x = CTRL_X + i as i32 * (w + 4);
            let r = rect(x, y, x + w, y + ROW_H);
            if *on {
                g.round(r, t.accent, 8);
                g.zones.push((r, *a));
                g.text_center(r, s, t.on_accent, 12, 600);
            } else if *enabled {
                g.control(&t, r, hover(*a), *a);
                g.text_center(r, s, t.text, 12, 400);
            } else {
                g.round(r, t.border, 8);
                g.round(
                    rect(r.left + 1, r.top + 1, r.right - 1, r.bottom - 1),
                    t.mica,
                    7,
                );
                g.text_center(r, s, t.text2, 12, 400);
            }
        }
    };
    // Compteur − valeur +.
    let stepper = |g: &mut Painter, x: i32, w: i32, y: i32, v: u32, dec: Action, inc: Action| {
        let minus = rect(x, y, x + 32, y + ROW_H);
        let plus = rect(x + w - 32, y, x + w, y + ROW_H);
        g.control(&t, minus, hover(dec), dec);
        g.text_center(minus, "−", t.text, 14, 600);
        g.round(rect(x + 36, y, x + w - 36, y + ROW_H), t.border, 8);
        g.round(rect(x + 37, y + 1, x + w - 37, y + ROW_H - 1), t.surface, 7);
        g.text_center(
            rect(x + 36, y, x + w - 36, y + ROW_H),
            &v.to_string(),
            t.text,
            13,
            400,
        );
        g.control(&t, plus, hover(inc), inc);
        g.text_center(plus, "+", t.text, 14, 600);
    };

    let y = next_row();
    label(&mut g, y, "Imprimante");
    dropdown(&mut g, y, &p.name, Action::PrinterList);
    let printer_row = y;
    if o.mode == Mode::Print {
        let y = next_row();
        label(&mut g, y, "Copies");
        stepper(
            &mut g,
            CTRL_X,
            150,
            y,
            o.devmode.copies.max(1) as u32,
            Action::CopiesDec,
            Action::CopiesInc,
        );
    }
    let y = next_row();
    label(&mut g, y, "Orientation");
    segmented(
        &mut g,
        y,
        &[
            ("Portrait", Action::Orientation(1), !landscape, true),
            ("Paysage", Action::Orientation(2), landscape, true),
        ],
    );
    if o.mode == Mode::Print {
        let y = next_row();
        label(&mut g, y, "Pages");
        segmented(
            &mut g,
            y,
            &[
                ("Toutes", Action::PagesAll, !o.range, true),
                ("Plage", Action::PagesRange, o.range, o.allow_pages),
            ],
        );
        if o.range {
            let y = next_row();
            label(&mut g, y, "De … à");
            let half = (CTRL_R - CTRL_X - 12) / 2;
            stepper(
                &mut g,
                CTRL_X,
                half,
                y,
                o.from,
                Action::FromDec,
                Action::FromInc,
            );
            stepper(
                &mut g,
                CTRL_X + half + 12,
                half,
                y,
                o.to,
                Action::ToDec,
                Action::ToInc,
            );
        }
    }
    let y = next_row();
    label(&mut g, y, "Papier");
    dropdown(&mut g, y, paper.name, Action::PaperList);
    let paper_row = y;
    let y = next_row();
    label(&mut g, y, "Couleur");
    let color = o.devmode.color != 1;
    segmented(
        &mut g,
        y,
        &[
            ("Couleur", Action::Color(2), color, p.color),
            ("Noir et blanc", Action::Color(1), !color, true),
        ],
    );
    let y = next_row();
    label(&mut g, y, "Qualité");
    let q = o.devmode.quality;
    segmented(
        &mut g,
        y,
        &[
            ("Brouillon", Action::Quality(-1), q == -1 || q == -2, true),
            (
                "Normale",
                Action::Quality(-3),
                q == -3 || q > 0 && q < 600,
                true,
            ),
            ("Haute", Action::Quality(-4), q == -4 || q >= 600, true),
        ],
    );
    let y = next_row();
    label(&mut g, y, "Recto verso");
    let d = o.devmode.duplex;
    segmented(
        &mut g,
        y,
        &[
            ("Non", Action::Duplex(1), d <= 1, true),
            ("Bord long", Action::Duplex(2), d == 2, p.duplex),
            ("Bord court", Action::Duplex(3), d == 3, p.duplex),
        ],
    );
    // Boutons de validation.
    g.fill(rect(PREVIEW_W, H - 72, W, H - 71), t.border);
    let print = rect(W - 236, H - 54, W - 128, H - 22);
    let cancel = rect(W - 120, H - 54, W - 20, H - 22);
    g.round(print, t.accent, 8);
    g.zones.push((print, Action::Print));
    g.text_center(
        print,
        if o.mode == Mode::Print {
            "Imprimer"
        } else {
            "OK"
        },
        t.on_accent,
        13,
        600,
    );
    g.control(&t, cancel, hover(Action::Cancel), Action::Cancel);
    g.text_center(cancel, "Annuler", t.text, 13, 400);
    // Liste ouverte : par-dessus le reste, elle capte les clics en premier.
    if let Some(popup) = ui.popup {
        let (anchor, items, current): (i32, Vec<String>, usize) = match popup {
            Popup::Printers => (
                printer_row,
                o.printers
                    .iter()
                    .map(|p| {
                        if p.is_default {
                            format!("{} (par défaut)", p.name)
                        } else {
                            p.name.clone()
                        }
                    })
                    .collect(),
                o.selected,
            ),
            Popup::Papers => (
                paper_row,
                p.papers
                    .iter()
                    .map(|id| winspool::paper(*id).name.to_string())
                    .collect(),
                p.papers
                    .iter()
                    .position(|id| *id == o.devmode.paper)
                    .unwrap_or(0),
            ),
        };
        let visible = items.len().min(8) as i32;
        let top = (anchor + ROW_H + 4).min(H - 20 - visible * 30 - 8);
        let r = rect(CTRL_X, top, CTRL_R, top + visible * 30 + 8);
        let mut zones = vec![(rect(0, 0, W, H), Action::ClosePopup)];
        g.round(
            rect(r.left - 1, r.top - 1, r.right + 1, r.bottom + 2),
            t.border,
            9,
        );
        g.round(r, t.surface, 8);
        for (i, s) in items.iter().take(8).enumerate() {
            let ir = rect(
                r.left + 4,
                r.top + 4 + i as i32 * 30,
                r.right - 4,
                r.top + 4 + i as i32 * 30 + 28,
            );
            if i == current {
                g.round(ir, t.selected, 6);
                g.fill(
                    rect(ir.left, ir.top + 7, ir.left + 3, ir.bottom - 7),
                    t.accent,
                );
            } else if hover(Action::Pick(i)) {
                g.round(ir, t.hover, 6);
            }
            g.text(
                ir.left + 12,
                ir.top + 6,
                &ellipsize(s, ir.right - ir.left - 20, 13),
                t.text,
                13,
                400,
            );
            zones.push((ir, Action::Pick(i)));
        }
        // Les zones les plus récentes sont testées d'abord.
        g.zones.clear();
        g.zones.extend(zones);
    }
    (g.cmds, g.zones)
}

fn hit(zones: &[(Rect, Action)], x: i32, y: i32) -> Option<Action> {
    zones
        .iter()
        .rev()
        .find(|(r, _)| inside(r, x, y))
        .map(|(_, a)| *a)
}

// ---------- Fenêtre et boucle modale ----------

const CLASS: &str = "WinluxPrintDialog";

extern "win64" fn dialog_proc(hwnd: usize, msg: u32, wparam: usize, lparam: isize) -> isize {
    let point = || {
        (
            (lparam & 0xffff) as i16 as i32,
            ((lparam >> 16) & 0xffff) as i16 as i32,
        )
    };
    match msg {
        gui::WM_PAINT => {
            let mut ps = [0u8; 72];
            // SAFETY: tampon PAINTSTRUCT local de taille suffisante.
            let hdc = unsafe { crate::dlls::user32::begin_paint(hwnd, ps.as_mut_ptr().cast()) };
            let cmds = {
                let mut ui = lock(&UI);
                match ui.as_mut() {
                    Some(ui) => {
                        let (cmds, zones) = paint(ui);
                        ui.zones = zones;
                        cmds
                    }
                    None => Vec::new(),
                }
            };
            for c in cmds {
                gui::record(hdc, c);
            }
            // SAFETY: PAINTSTRUCT rempli par BeginPaint ci-dessus.
            unsafe { crate::dlls::user32::end_paint(hwnd, ps.as_ptr().cast()) };
            0
        }
        gui::WM_LBUTTONDOWN => {
            let (x, y) = point();
            let changed = {
                let mut ui = lock(&UI);
                match ui.as_mut() {
                    Some(ui) => match hit(&ui.zones, x, y) {
                        Some(a) => ui.apply(a),
                        None => false,
                    },
                    None => false,
                }
            };
            if changed {
                gui::request_paint(hwnd);
            }
            0
        }
        gui::WM_MOUSEMOVE => {
            let (x, y) = point();
            let changed = {
                let mut ui = lock(&UI);
                match ui.as_mut() {
                    Some(ui) => {
                        let h = hit(&ui.zones, x, y);
                        let changed = h != ui.hover;
                        ui.hover = h;
                        changed
                    }
                    None => false,
                }
            };
            if changed {
                gui::request_paint(hwnd);
            }
            0
        }
        gui::WM_KEYDOWN => {
            let action = match wparam {
                0x0D => Some(Action::Print),  // Entrée
                0x1B => Some(Action::Cancel), // Échap
                _ => None,
            };
            if let Some(a) = action {
                if let Some(ui) = lock(&UI).as_mut() {
                    ui.apply(a);
                }
            }
            0
        }
        gui::WM_CLOSE => {
            if let Some(ui) = lock(&UI).as_mut() {
                ui.result = Some(false);
            }
            0
        }
        _ => gui::def_window_proc(hwnd, msg, wparam, lparam),
    }
}

/// Affiche la boîte et attend le choix ; `None` si annulée.
fn run(opts: Options) -> Option<Options> {
    match std::env::var("WINLUX_PRINTDLG").as_deref() {
        Ok("accept") => return Some(opts),
        Ok(_) => return None,
        Err(_) => {}
    }
    if lock(&UI).is_some() {
        return None; // une boîte est déjà ouverte
    }
    // Sans affichage réel, impossible de demander : on annule.
    if !gui::wait_backend(0) {
        return None;
    }
    let class: Vec<u16> = CLASS.encode_utf16().chain([0]).collect();
    let wc = gui::WndClassA {
        style: 0,
        wnd_proc: dialog_proc as *const () as usize,
        cls_extra: 0,
        wnd_extra: 0,
        instance: 0,
        icon: 0,
        cursor: 0,
        background: 0,
        menu_name: std::ptr::null(),
        class_name: class.as_ptr().cast(),
    };
    let title: Vec<u16> = match opts.mode {
        Mode::Print => "Imprimer",
        Mode::Setup => "Configuration de l'impression",
        Mode::Properties => "Préférences d'impression",
    }
    .encode_utf16()
    .chain([0])
    .collect();
    let (sw, sh) = gui::screen_size();
    *lock(&UI) = Some(Ui {
        opts,
        popup: None,
        zones: Vec::new(),
        hover: None,
        result: None,
        dark: shell_dark(),
        hwnd: 0,
    });
    // SAFETY: structures locales valides le temps des appels.
    let hwnd = unsafe {
        crate::dlls::user32::register_class_w(&wc);
        crate::dlls::user32::create_window_ex_w(
            0x0000_0001, // WS_EX_DLGMODALFRAME
            class.as_ptr(),
            title.as_ptr(),
            0x1000_0000 | 0x00C0_0000 | 0x0008_0000, // VISIBLE | CAPTION | SYSMENU
            ((sw - W) / 2).max(0),
            ((sh - H) / 2).max(0),
            W,
            H,
            0,
            0,
            0,
            std::ptr::null_mut(),
        )
    };
    if hwnd == 0 {
        *lock(&UI) = None;
        return None;
    }
    if let Some(ui) = lock(&UI).as_mut() {
        ui.hwnd = hwnd;
    }
    let mut msg = gui::Msg::new(0, 0, 0, 0);
    loop {
        if lock(&UI).as_ref().and_then(|u| u.result).is_some() {
            break;
        }
        // SAFETY: `msg` est un MSG local.
        let got = unsafe { crate::dlls::user32::peek_message_w(&mut msg, 0, 0, 0, 1) };
        if got == -1 {
            break; // WM_QUIT : l'application se ferme
        }
        if got == 1 {
            // Les autres fenêtres de l'application continuent de se repeindre.
            unsafe {
                crate::dlls::user32::translate_message(&msg);
                crate::dlls::user32::dispatch_message_w(&msg);
            }
            continue;
        }
        if !gui::wait_backend(30) {
            break;
        }
    }
    let ui = lock(&UI).take()?;
    // SAFETY: fenêtre créée ci-dessus.
    unsafe { crate::dlls::user32::destroy_window(ui.hwnd) };
    match ui.result {
        Some(true) => Some(ui.opts),
        _ => None,
    }
}

fn options_for(printer: Option<&str>, devmode: Option<DevMode>, mode: Mode) -> Option<Options> {
    let printers = winspool::printers();
    let selected = printer
        .and_then(|n| printers.iter().position(|p| p.name.eq_ignore_ascii_case(n)))
        .or_else(|| printers.iter().position(|p| p.is_default))
        .unwrap_or(0);
    let p = printers.get(selected)?;
    let devmode = devmode
        .filter(|d| d.device.eq_ignore_ascii_case(&p.name))
        .unwrap_or_else(|| DevMode::for_printer(p));
    Some(Options {
        printers,
        selected,
        devmode,
        range: false,
        from: 1,
        to: 1,
        min_page: 1,
        max_page: 0xFFFF,
        allow_pages: false,
        mode,
    })
}

/// Propriétés d'une imprimante (`DocumentProperties` avec `DM_IN_PROMPT`).
pub fn properties_dialog(p: &PrinterEntry, devmode: &mut DevMode) -> bool {
    let Some(mut opts) = options_for(Some(&p.name), Some(devmode.clone()), Mode::Properties) else {
        return false;
    };
    opts.devmode = devmode.clone();
    match run(opts) {
        Some(o) => {
            *devmode = o.devmode;
            true
        }
        None => false,
    }
}

// ---------- HGLOBAL DEVMODE / DEVNAMES ----------

unsafe fn read_global(h: usize) -> Option<Vec<u8>> {
    if h == 0 {
        return None;
    }
    let size = kernel32::global_size(h);
    let p = kernel32::global_lock(h) as *const u8;
    if p.is_null() || size == 0 {
        return None;
    }
    let v = std::slice::from_raw_parts(p, size).to_vec();
    kernel32::global_unlock(h);
    Some(v)
}

unsafe fn new_global(bytes: &[u8], old: usize) -> usize {
    if old != 0 {
        kernel32::global_free(old);
    }
    let h = kernel32::global_alloc(0x0042, bytes.len()); // GMEM_MOVEABLE | ZEROINIT
    if h != 0 {
        let p = kernel32::global_lock(h) as *mut u8;
        if !p.is_null() {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        }
        kernel32::global_unlock(h);
    }
    h
}

/// Nom d'imprimante d'un `DEVNAMES` (offsets en caractères).
fn devnames_device(bytes: &[u8], wide: bool) -> Option<String> {
    let off = u16::from_le_bytes(bytes.get(2..4)?.try_into().ok()?) as usize;
    if wide {
        let words: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let s = words.get(off..)?;
        let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        Some(String::from_utf16_lossy(&s[..n]))
    } else {
        let s = bytes.get(off..)?;
        let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        Some(crate::dlls::decode_ansi(&s[..n]))
    }
}

fn devnames_bytes(p: &PrinterEntry, wide: bool) -> Vec<u8> {
    let parts = ["winspool", p.name.as_str(), p.port.as_str()];
    let mut text: Vec<u8> = Vec::new();
    let mut offsets = Vec::new();
    let cs = if wide { 2 } else { 1 };
    let mut pos = 8 / cs; // en caractères, après les 4 WORD
    for s in parts {
        offsets.push(pos as u16);
        let enc: Vec<u8> = if wide {
            s.encode_utf16()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect()
        } else {
            let mut a = crate::dlls::encode_ansi(s);
            a.push(0);
            a
        };
        pos += enc.len() / cs;
        text.extend(enc);
    }
    let mut out = Vec::new();
    for o in &offsets {
        out.extend_from_slice(&o.to_le_bytes());
    }
    out.extend_from_slice(&u16::from(p.is_default).to_le_bytes()); // DN_DEFAULTPRN
    out.extend(text);
    out
}

/// Lit les HGLOBAL d'entrée : (imprimante demandée, DEVMODE fusionné).
unsafe fn read_inputs(
    hdevmode: usize,
    hdevnames: usize,
    wide: bool,
) -> (Option<String>, Option<DevMode>) {
    let names = read_global(hdevnames).and_then(|b| devnames_device(&b, wide));
    let dm_bytes = read_global(hdevmode);
    let dm_name = dm_bytes.as_ref().map(|b| {
        let cs = if wide { 2 } else { 1 };
        let mut name = vec![0u8; 32 * cs];
        name.copy_from_slice(&b[..(32 * cs).min(b.len())]);
        if wide {
            let w: Vec<u16> = name
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .take_while(|&c| c != 0)
                .collect();
            String::from_utf16_lossy(&w)
        } else {
            crate::dlls::decode_ansi(
                &name[..name.iter().position(|&c| c == 0).unwrap_or(name.len())],
            )
        }
    });
    let printer = names.or(dm_name).filter(|n| !n.is_empty());
    let devmode = dm_bytes.and_then(|b| {
        let p = winspool::find_printer(printer.as_deref()?)?;
        let mut dm = DevMode::for_printer(&p);
        if b.len() >= if wide { 74 } else { 42 } {
            dm.merge(b.as_ptr(), wide);
        }
        Some(dm)
    });
    (printer, devmode)
}

// ---------- PrintDlg ----------

const PD_SELECTION: u32 = 0x1;
const PD_PAGENUMS: u32 = 0x2;
const PD_NOPAGENUMS: u32 = 0x8;
const PD_COLLATE: u32 = 0x10;
const PD_PRINTTOFILE: u32 = 0x20;
const PD_PRINTSETUP: u32 = 0x40;
const PD_RETURNDC: u32 = 0x100;
const PD_RETURNIC: u32 = 0x200;
const PD_RETURNDEFAULT: u32 = 0x400;
const PD_USEDEVMODECOPIESANDCOLLATE: u32 = 0x40000;
const PD_CURRENTPAGE: u32 = 0x0040_0000;

/// Champs communs de `PRINTDLG` et `PRINTDLGEX` après le choix.
struct Outcome {
    opts: Options,
}

unsafe fn finish(
    o: &Outcome,
    flags: u32,
    hdevmode: *mut usize,
    hdevnames: *mut usize,
    hdc: *mut usize,
    wide: bool,
) -> u32 {
    let p = o.opts.printer();
    let dm = &o.opts.devmode;
    std::ptr::write_unaligned(
        hdevmode,
        new_global(&dm.to_bytes(wide), std::ptr::read_unaligned(hdevmode)),
    );
    std::ptr::write_unaligned(
        hdevnames,
        new_global(
            &devnames_bytes(p, wide),
            std::ptr::read_unaligned(hdevnames),
        ),
    );
    if flags & (PD_RETURNDC | PD_RETURNIC) != 0 {
        let bytes = dm.to_bytes(wide);
        let dc = winspool::create_printer_dc(
            Some("WINSPOOL".into()),
            Some(p.name.clone()),
            bytes.as_ptr(),
            wide,
            flags & PD_RETURNDC == 0,
        )
        .unwrap_or(0);
        std::ptr::write_unaligned(hdc, dc);
    }
    let mut out = flags & !(PD_SELECTION | PD_PAGENUMS | PD_CURRENTPAGE | PD_PRINTTOFILE);
    if o.opts.range {
        out |= PD_PAGENUMS;
    }
    if dm.collate != 0 && dm.copies > 1 {
        out |= PD_COLLATE;
    } else {
        out &= !PD_COLLATE;
    }
    out
}

/// `PRINTDLG` (x64, alignement naturel).
const PRINTDLG_SIZE: u32 = 120;

unsafe fn print_dlg(pd: *mut u8, wide: bool) -> i32 {
    LAST_ERROR.store(0, Ordering::Relaxed);
    if pd.is_null() {
        return fail(CDERR_INITIALIZATION);
    }
    if std::ptr::read_unaligned(pd as *const u32) != PRINTDLG_SIZE {
        return fail(CDERR_STRUCTSIZE);
    }
    let hdevmode = pd.add(16) as *mut usize;
    let hdevnames = pd.add(24) as *mut usize;
    let hdc = pd.add(32) as *mut usize;
    let flags_p = pd.add(40) as *mut u32;
    let w16 = |off: usize| pd.add(off) as *mut u16;
    let flags = std::ptr::read_unaligned(flags_p);
    let (printer, devmode) = read_inputs(
        std::ptr::read_unaligned(hdevmode),
        std::ptr::read_unaligned(hdevnames),
        wide,
    );
    let mode = if flags & PD_PRINTSETUP != 0 {
        Mode::Setup
    } else {
        Mode::Print
    };
    let Some(mut opts) = options_for(printer.as_deref(), devmode, mode) else {
        return fail(PDERR_NODEFAULTPRN);
    };
    if printer.is_some()
        && !opts
            .printer()
            .name
            .eq_ignore_ascii_case(printer.as_deref().unwrap_or_default())
    {
        LAST_ERROR.store(PDERR_PRINTERNOTFOUND, Ordering::Relaxed);
    }
    let (from, to, min, max, copies) = (
        std::ptr::read_unaligned(w16(44)),
        std::ptr::read_unaligned(w16(46)),
        std::ptr::read_unaligned(w16(48)),
        std::ptr::read_unaligned(w16(50)),
        std::ptr::read_unaligned(w16(52)),
    );
    opts.min_page = u32::from(min.max(1));
    opts.max_page = u32::from(max.max(min).max(1));
    opts.allow_pages = flags & PD_NOPAGENUMS == 0 && max >= min;
    opts.from = u32::from(from).clamp(opts.min_page, opts.max_page);
    opts.to = u32::from(to).clamp(opts.from, opts.max_page);
    opts.range = flags & PD_PAGENUMS != 0 && opts.allow_pages;
    if copies > 1 && flags & PD_USEDEVMODECOPIESANDCOLLATE == 0 {
        opts.devmode.copies = copies.min(999) as i16;
    }
    let chosen = if flags & PD_RETURNDEFAULT != 0 {
        Some(opts)
    } else {
        run(opts)
    };
    let Some(opts) = chosen else {
        return 0; // annulé : CommDlgExtendedError() == 0
    };
    let o = Outcome { opts };
    let out = finish(&o, flags, hdevmode, hdevnames, hdc, wide);
    std::ptr::write_unaligned(flags_p, out);
    if o.opts.range {
        std::ptr::write_unaligned(w16(44), o.opts.from as u16);
        std::ptr::write_unaligned(w16(46), o.opts.to as u16);
    }
    // Sans PD_USEDEVMODECOPIESANDCOLLATE, l'application gère les copies.
    if flags & PD_USEDEVMODECOPIESANDCOLLATE == 0 {
        std::ptr::write_unaligned(w16(52), o.opts.devmode.copies.max(1) as u16);
    }
    1
}

pub unsafe extern "win64" fn print_dlg_a(pd: *mut u8) -> i32 {
    print_dlg(pd, false)
}

pub unsafe extern "win64" fn print_dlg_w(pd: *mut u8) -> i32 {
    print_dlg(pd, true)
}

// ---------- PrintDlgEx ----------

const PRINTDLGEX_SIZE: u32 = 136;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_FAIL: i32 = 0x8000_4005u32 as i32;

unsafe fn print_dlg_ex(pd: *mut u8, wide: bool) -> i32 {
    LAST_ERROR.store(0, Ordering::Relaxed);
    if pd.is_null() || std::ptr::read_unaligned(pd as *const u32) != PRINTDLGEX_SIZE {
        return E_INVALIDARG;
    }
    let hdevmode = pd.add(16) as *mut usize;
    let hdevnames = pd.add(24) as *mut usize;
    let hdc = pd.add(32) as *mut usize;
    let flags_p = pd.add(40) as *mut u32;
    let r32 = |off: usize| std::ptr::read_unaligned(pd.add(off) as *const u32);
    let flags = std::ptr::read_unaligned(flags_p);
    let (printer, devmode) = read_inputs(
        std::ptr::read_unaligned(hdevmode),
        std::ptr::read_unaligned(hdevnames),
        wide,
    );
    let Some(mut opts) = options_for(printer.as_deref(), devmode, Mode::Print) else {
        LAST_ERROR.store(PDERR_NODEFAULTPRN, Ordering::Relaxed);
        return E_FAIL;
    };
    let n_ranges = r32(52);
    let max_ranges = r32(56);
    let ranges = std::ptr::read_unaligned(pd.add(64) as *const *mut u32);
    let (min, max, copies) = (r32(72), r32(76), r32(80));
    opts.min_page = min.max(1);
    opts.max_page = max.max(opts.min_page);
    opts.allow_pages = flags & PD_NOPAGENUMS == 0 && max_ranges > 0 && !ranges.is_null();
    if n_ranges > 0 && !ranges.is_null() {
        opts.from = std::ptr::read_unaligned(ranges).clamp(opts.min_page, opts.max_page);
        opts.to = std::ptr::read_unaligned(ranges.add(1)).clamp(opts.from, opts.max_page);
    } else {
        opts.from = opts.min_page;
        opts.to = opts.max_page.min(opts.min_page.saturating_add(9999));
    }
    opts.range = flags & PD_PAGENUMS != 0 && opts.allow_pages;
    if copies > 1 {
        opts.devmode.copies = copies.min(999) as i16;
    }
    let result = if flags & PD_RETURNDEFAULT != 0 {
        Some(opts)
    } else {
        run(opts)
    };
    let action = pd.add(132) as *mut u32;
    let Some(opts) = result else {
        std::ptr::write_unaligned(action, 0); // PD_RESULT_CANCEL
        return 0;
    };
    let o = Outcome { opts };
    let out = finish(&o, flags, hdevmode, hdevnames, hdc, wide);
    std::ptr::write_unaligned(flags_p, out);
    if o.opts.range && !ranges.is_null() && max_ranges > 0 {
        std::ptr::write_unaligned(ranges, o.opts.from);
        std::ptr::write_unaligned(ranges.add(1), o.opts.to);
        std::ptr::write_unaligned(pd.add(52) as *mut u32, 1);
    }
    std::ptr::write_unaligned(pd.add(80) as *mut u32, o.opts.devmode.copies.max(1) as u32);
    std::ptr::write_unaligned(action, 1); // PD_RESULT_PRINT
    0 // S_OK
}

pub unsafe extern "win64" fn print_dlg_ex_a(pd: *mut u8) -> i32 {
    print_dlg_ex(pd, false)
}

pub unsafe extern "win64" fn print_dlg_ex_w(pd: *mut u8) -> i32 {
    print_dlg_ex(pd, true)
}

// ---------- PageSetupDlg ----------

const PAGESETUPDLG_SIZE: u32 = 128;
const PSD_MARGINS: u32 = 0x2;
const PSD_INTHOUSANDTHSOFINCHES: u32 = 0x4;
const PSD_INHUNDREDTHSOFMILLIMETERS: u32 = 0x8;
const PSD_RETURNDEFAULT: u32 = 0x400;

unsafe fn page_setup_dlg(ps: *mut u8, wide: bool) -> i32 {
    LAST_ERROR.store(0, Ordering::Relaxed);
    if ps.is_null() {
        return fail(CDERR_INITIALIZATION);
    }
    if std::ptr::read_unaligned(ps as *const u32) != PAGESETUPDLG_SIZE {
        return fail(CDERR_STRUCTSIZE);
    }
    let hdevmode = ps.add(16) as *mut usize;
    let hdevnames = ps.add(24) as *mut usize;
    let flags_p = ps.add(32) as *mut u32;
    let flags = std::ptr::read_unaligned(flags_p);
    let (printer, devmode) = read_inputs(
        std::ptr::read_unaligned(hdevmode),
        std::ptr::read_unaligned(hdevnames),
        wide,
    );
    let Some(opts) = options_for(printer.as_deref(), devmode, Mode::Setup) else {
        return fail(PDERR_NODEFAULTPRN);
    };
    let chosen = if flags & PSD_RETURNDEFAULT != 0 {
        Some(opts)
    } else {
        run(opts)
    };
    let Some(opts) = chosen else {
        return 0;
    };
    // Unités : millièmes de pouce ou centièmes de mm (défaut métrique).
    let inches = flags & PSD_INTHOUSANDTHSOFINCHES != 0;
    let mut out_flags = flags;
    if !inches {
        out_flags |= PSD_INHUNDREDTHSOFMILLIMETERS;
    }
    let conv = |tenth_mm: i32| {
        if inches {
            tenth_mm * 1000 / 254
        } else {
            tenth_mm * 10
        }
    };
    let paper = winspool::paper(opts.devmode.paper);
    let (mut w, mut h) = (conv(paper.width), conv(paper.height));
    if opts.devmode.landscape() {
        std::mem::swap(&mut w, &mut h);
    }
    let put = |off: usize, v: i32| std::ptr::write_unaligned(ps.add(off) as *mut i32, v);
    put(36, w);
    put(40, h);
    let min = conv(match opts.printer().kind {
        winspool::Kind::Pdf => 0,
        winspool::Kind::Cups => 42,
    });
    for i in 0..4 {
        put(44 + i * 4, min);
    }
    if flags & PSD_MARGINS == 0 {
        for i in 0..4 {
            put(60 + i * 4, conv(250).max(min));
        }
    }
    let o = Outcome { opts };
    let mut dummy_dc = 0usize;
    finish(&o, 0, hdevmode, hdevnames, &mut dummy_dc, wide);
    std::ptr::write_unaligned(flags_p, out_flags);
    1
}

pub unsafe extern "win64" fn page_setup_dlg_a(ps: *mut u8) -> i32 {
    page_setup_dlg(ps, false)
}

pub unsafe extern "win64" fn page_setup_dlg_w(ps: *mut u8) -> i32 {
    page_setup_dlg(ps, true)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "PrintDlgA" => fn print_dlg_a,
        "PrintDlgW" => fn print_dlg_w,
        "PrintDlgExA" => fn print_dlg_ex_a,
        "PrintDlgExW" => fn print_dlg_ex_w,
        "PageSetupDlgA" => fn page_setup_dlg_a,
        "PageSetupDlgW" => fn page_setup_dlg_w,
        "CommDlgExtendedError" => fn comm_dlg_extended_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_dlg_par_defaut_renvoie_un_dc() {
        let _g = lock(&winspool::TEST_LOCK);
        winspool::set_cups_server(None);
        let mut pd = [0u8; PRINTDLG_SIZE as usize];
        pd[..4].copy_from_slice(&PRINTDLG_SIZE.to_le_bytes());
        pd[40..44].copy_from_slice(&(PD_RETURNDEFAULT | PD_RETURNDC).to_le_bytes());
        assert_eq!(unsafe { print_dlg_w(pd.as_mut_ptr()) }, 1);
        let read = |off: usize| usize::from_le_bytes(pd[off..off + 8].try_into().unwrap());
        let (hdevmode, hdevnames, hdc) = (read(16), read(24), read(32));
        assert!(winspool::is_printer_dc(hdc));
        let names = unsafe { read_global(hdevnames) }.unwrap();
        assert_eq!(
            devnames_device(&names, true).as_deref(),
            Some(winspool::PDF_PRINTER)
        );
        let dm = unsafe { read_global(hdevmode) }.unwrap();
        assert_eq!(dm.len(), winspool::DEVMODEW_SIZE);
        // Relecture des HGLOBAL produits : même imprimante, même DEVMODE.
        let (printer, devmode) = unsafe { read_inputs(hdevmode, hdevnames, true) };
        assert_eq!(printer.as_deref(), Some(winspool::PDF_PRINTER));
        assert_eq!(devmode.unwrap().paper, 9);
        winspool::delete_printer_dc(hdc);
        gui::free_dc(hdc);
    }

    #[test]
    fn print_dlg_annule_sans_affichage() {
        let _g = lock(&winspool::TEST_LOCK);
        winspool::set_cups_server(None);
        let mut pd = [0u8; PRINTDLG_SIZE as usize];
        pd[..4].copy_from_slice(&PRINTDLG_SIZE.to_le_bytes());
        // Sans affichage ni WINLUX_PRINTDLG, la boîte est annulée proprement.
        if std::env::var_os("WINLUX_PRINTDLG").is_none() && std::env::var_os("DISPLAY").is_none() {
            assert_eq!(unsafe { print_dlg_w(pd.as_mut_ptr()) }, 0);
            assert_eq!(comm_dlg_extended_error(), 0);
        }
        // Taille de structure invalide.
        pd[..4].copy_from_slice(&64u32.to_le_bytes());
        assert_eq!(unsafe { print_dlg_w(pd.as_mut_ptr()) }, 0);
        assert_eq!(comm_dlg_extended_error(), CDERR_STRUCTSIZE);
    }

    #[test]
    fn options_et_actions_de_la_boite() {
        let _g = lock(&winspool::TEST_LOCK);
        winspool::set_cups_server(None);
        let mut opts = options_for(None, None, Mode::Print).unwrap();
        opts.allow_pages = true;
        opts.max_page = 5;
        let mut ui = Ui {
            opts,
            popup: None,
            zones: Vec::new(),
            hover: None,
            result: None,
            dark: false,
            hwnd: 0,
        };
        ui.apply(Action::CopiesInc);
        ui.apply(Action::Orientation(2));
        ui.apply(Action::PagesRange);
        ui.apply(Action::ToInc);
        ui.apply(Action::ToInc);
        ui.apply(Action::PaperList);
        assert_eq!(ui.popup, Some(Popup::Papers));
        ui.apply(Action::Pick(1));
        assert_eq!(ui.popup, None);
        let o = &ui.opts;
        assert_eq!(o.devmode.copies, 2);
        assert!(o.devmode.landscape());
        assert!(o.range);
        assert_eq!((o.from, o.to), (1, 3));
        assert_eq!(o.devmode.paper, 1);
        // Le dessin produit des zones cliquables pour chaque contrôle.
        let (cmds, zones) = paint(&ui);
        assert!(!cmds.is_empty());
        assert!(zones.iter().any(|(_, a)| *a == Action::Print));
        let print = zones.iter().find(|(_, a)| *a == Action::Print).unwrap().0;
        assert_eq!(
            hit(&zones, print.left + 2, print.top + 2),
            Some(Action::Print)
        );
        ui.apply(Action::Print);
        assert_eq!(ui.result, Some(true));
    }
}
