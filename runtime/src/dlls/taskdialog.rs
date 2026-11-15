//! `TaskDialog` et `TaskDialogIndirect` (comctl32 6) en vraie fenêtre.
//!
//! La boîte de tâche de Vista et de Windows 11 remplace MessageBox dans les
//! programmes récents : instruction principale en bleu, texte, boutons
//! personnalisés ou « liens de commande », boutons radio, case « Ne plus
//! afficher », détails dépliables, pied de page, barre de progression et
//! procédure de rappel (`TDN_*`) qui pilote la boîte par messages `TDM_*`.
//!
//! La fenêtre est une boîte modale de Winlux (`dialogs::run_builtin`) dont
//! les contrôles sont placés en pixels par [`plan`], fonction pure qui sert
//! aussi à replacer les contrôles quand un texte change ou que les détails
//! se déplient.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::gui::{self, FontSpec, Rect};
use super::{dialogs, lock, user32};

// Drapeaux TDF_*.
const TDF_ENABLE_HYPERLINKS: u32 = 0x0001;
const TDF_USE_HICON_MAIN: u32 = 0x0002;
const TDF_ALLOW_DIALOG_CANCELLATION: u32 = 0x0008;
const TDF_USE_COMMAND_LINKS: u32 = 0x0010;
const TDF_USE_COMMAND_LINKS_NO_ICON: u32 = 0x0020;
const TDF_EXPAND_FOOTER_AREA: u32 = 0x0040;
const TDF_EXPANDED_BY_DEFAULT: u32 = 0x0080;
const TDF_VERIFICATION_FLAG_CHECKED: u32 = 0x0100;
const TDF_SHOW_PROGRESS_BAR: u32 = 0x0200;
const TDF_SHOW_MARQUEE_PROGRESS_BAR: u32 = 0x0400;
const TDF_CALLBACK_TIMER: u32 = 0x0800;
const TDF_NO_DEFAULT_RADIO_BUTTON: u32 = 0x4000;

// Notifications TDN_*.
const TDN_CREATED: u32 = 0;
const TDN_BUTTON_CLICKED: u32 = 2;
const TDN_TIMER: u32 = 4;
const TDN_DESTROYED: u32 = 5;
const TDN_RADIO_BUTTON_CLICKED: u32 = 6;
const TDN_DIALOG_CONSTRUCTED: u32 = 7;
const TDN_VERIFICATION_CLICKED: u32 = 8;
const TDN_EXPANDO_BUTTON_CLICKED: u32 = 10;

// Messages TDM_* (WM_USER + n).
const TDM_CLICK_BUTTON: u32 = 0x0466;
const TDM_SET_MARQUEE_PROGRESS_BAR: u32 = 0x0467;
const TDM_SET_PROGRESS_BAR_STATE: u32 = 0x0468;
const TDM_SET_PROGRESS_BAR_RANGE: u32 = 0x0469;
const TDM_SET_PROGRESS_BAR_POS: u32 = 0x046A;
const TDM_SET_PROGRESS_BAR_MARQUEE: u32 = 0x046B;
const TDM_SET_ELEMENT_TEXT: u32 = 0x046C;
const TDM_CLICK_RADIO_BUTTON: u32 = 0x046E;
const TDM_ENABLE_BUTTON: u32 = 0x046F;
const TDM_ENABLE_RADIO_BUTTON: u32 = 0x0470;
const TDM_CLICK_VERIFICATION: u32 = 0x0471;
const TDM_UPDATE_ELEMENT_TEXT: u32 = 0x0472;
const TDM_SET_BUTTON_ELEVATION_REQUIRED_STATE: u32 = 0x0473;
const TDM_UPDATE_ICON: u32 = 0x0474;

const WM_DESTROY: u32 = 0x0002;
const WM_SETFONT: u32 = 0x0030;
const WM_INITDIALOG: u32 = 0x0110;
const WM_COMMAND: u32 = 0x0111;
const WM_TIMER: u32 = 0x0113;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const BM_GETCHECK: u32 = 0x00F0;
const BM_SETCHECK: u32 = 0x00F1;
const DM_SETDEFID: u32 = 0x0401;

const WS_TABSTOP: u32 = 0x0001_0000;
const WS_GROUP: u32 = 0x0002_0000;
const BS_DEFPUSHBUTTON: u32 = 0x0001;
const BS_AUTOCHECKBOX: u32 = 0x0003;
const BS_AUTORADIOBUTTON: u32 = 0x0009;
const BS_LEFT: u32 = 0x0100;
const BS_MULTILINE: u32 = 0x2000;
const SS_CENTER: u32 = 0x0001;
const SS_NOPREFIX: u32 = 0x0080;
const PBS_MARQUEE: u32 = 0x0008;

const IDCANCEL: i32 = 2;
const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_INVALIDARG: i32 = 0x8007_0057_u32 as i32;

// Identifiants internes des contrôles (hors plage des programmes).
const ID_MAIN: usize = 0xF100;
const ID_CONTENT: usize = 0xF101;
const ID_EXPANDED: usize = 0xF102;
const ID_FOOTER: usize = 0xF103;
const ID_ICON: usize = 0xF104;
const ID_EXPANDO: usize = 0xF105;
const ID_VERIFY: usize = 0xF106;
const ID_PROGRESS: usize = 0xF107;
const TIMER_ID: usize = 0xF1;

/// Marge et dimensions de la boîte, en pixels.
const MARGIN: i32 = 12;
const ICON: i32 = 32;
const BUTTON_H: i32 = 28;
const LINK_H: i32 = 44;

/// Icônes prédéfinies (MAKEINTRESOURCEW(-1) à -4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Icon {
    Warning,
    Error,
    Information,
    Shield,
}

impl Icon {
    fn from_resource(value: usize) -> Option<Icon> {
        match value {
            0xFFFF => Some(Icon::Warning),
            0xFFFE => Some(Icon::Error),
            0xFFFD => Some(Icon::Information),
            0xFFFC => Some(Icon::Shield),
            _ => None,
        }
    }
    fn glyph(self) -> &'static str {
        match self {
            Icon::Warning => "⚠",
            Icon::Error => "✖",
            Icon::Information => "ℹ",
            Icon::Shield => "⛨",
        }
    }
    /// Couleur COLORREF (0x00BBGGRR).
    fn color(self) -> u32 {
        match self {
            Icon::Warning => 0x0000_A0E8,
            Icon::Error => 0x001C_2BC4,
            Icon::Information | Icon::Shield => 0x00B8_5F00,
        }
    }
}

/// Description lue dans TASKDIALOGCONFIG, textes déjà chargés.
#[derive(Clone, Debug, Default)]
pub struct Spec {
    pub flags: u32,
    pub title: String,
    pub main: String,
    pub content: String,
    pub expanded: String,
    pub expanded_label: String,
    pub collapsed_label: String,
    pub verification: String,
    pub footer: String,
    pub icon: Option<Icon>,
    /// Boutons (identifiant, libellé) : personnalisés puis communs.
    pub buttons: Vec<(i32, String)>,
    /// Nombre de boutons personnalisés en tête de `buttons`.
    pub custom: usize,
    pub radios: Vec<(i32, String)>,
    pub default_button: i32,
    pub default_radio: i32,
    /// Largeur de la zone cliente demandée, en pixels (0 = automatique).
    pub width: i32,
}

impl Spec {
    fn command_links(&self) -> bool {
        self.flags & (TDF_USE_COMMAND_LINKS | TDF_USE_COMMAND_LINKS_NO_ICON) != 0
    }
    /// Bouton par défaut : celui demandé s'il existe, sinon le premier.
    pub fn default_id(&self) -> i32 {
        if self.buttons.iter().any(|b| b.0 == self.default_button) {
            self.default_button
        } else {
            self.buttons.first().map_or(1, |b| b.0)
        }
    }
    fn default_radio_id(&self) -> i32 {
        if self.flags & TDF_NO_DEFAULT_RADIO_BUTTON != 0 {
            0
        } else if self.radios.iter().any(|r| r.0 == self.default_radio) {
            self.default_radio
        } else {
            self.radios.first().map_or(0, |r| r.0)
        }
    }
    fn cancellable(&self) -> bool {
        self.flags & TDF_ALLOW_DIALOG_CANCELLATION != 0
            || self.buttons.iter().any(|b| b.0 == IDCANCEL)
    }
    fn expando_label(&self, expanded: bool) -> String {
        let (show, hide) = expando_defaults();
        if expanded {
            if self.expanded_label.is_empty() {
                hide.to_string()
            } else {
                self.expanded_label.clone()
            }
        } else if !self.collapsed_label.is_empty() {
            self.collapsed_label.clone()
        } else if !self.expanded_label.is_empty() {
            self.expanded_label.clone()
        } else {
            show.to_string()
        }
    }
}

/// Élément placé par [`plan`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Elem {
    Icon,
    Main,
    Content,
    Expanded,
    Progress,
    Radio(usize),
    Button(usize),
    Expando,
    Verification,
    Footer,
}

fn expando_defaults() -> (&'static str, &'static str) {
    match dialogs::session_lang().as_str() {
        "fr" => ("Afficher les détails", "Masquer les détails"),
        "de" => ("Details anzeigen", "Details ausblenden"),
        "es" => ("Ver detalles", "Ocultar detalles"),
        _ => ("See details", "Hide details"),
    }
}

fn close_label() -> &'static str {
    match dialogs::session_lang().as_str() {
        "fr" => "Fermer",
        "de" => "Schließen",
        "es" => "Cerrar",
        _ => "Close",
    }
}

/// Boutons communs TDCBF_* dans l'ordre de Windows.
pub fn common_buttons(flags: u32) -> Vec<(i32, String)> {
    let l = dialogs::labels();
    // OK, Oui, Non, Réessayer, Annuler, Fermer.
    let table: [(u32, i32, &str); 6] = [
        (0x01, 1, l[0]),
        (0x02, 6, l[5]),
        (0x04, 7, l[6]),
        (0x10, 4, l[3]),
        (0x08, 2, l[1]),
        (0x20, 8, close_label()),
    ];
    table
        .iter()
        .filter(|(bit, _, _)| flags & bit != 0)
        .map(|&(_, id, label)| (id, label.to_string()))
        .collect()
}

/// Retire les balises `<a href="…">` et `</a>` (TDF_ENABLE_HYPERLINKS).
pub fn strip_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let lower = tail.to_ascii_lowercase();
        if lower.starts_with("<a ") || lower.starts_with("<a>") || lower.starts_with("</a>") {
            match tail.find('>') {
                Some(end) => rest = &tail[end + 1..],
                None => {
                    out.push_str(tail);
                    rest = "";
                }
            }
        } else {
            out.push('<');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

fn normal_font() -> FontSpec {
    gui::font_spec(super::gdi32::get_stock_object(17)).unwrap_or_default()
}

fn main_font() -> FontSpec {
    FontSpec {
        height: -16,
        weight: 400,
        face: "Segoe UI".into(),
        ..FontSpec::default()
    }
}

fn icon_font() -> FontSpec {
    FontSpec {
        height: -28,
        weight: 700,
        face: "Segoe UI Symbol".into(),
        ..FontSpec::default()
    }
}

/// Hauteur d'un texte coupé en lignes de `width` pixels au plus.
pub fn text_height(font: &FontSpec, text: &str, width: i32) -> i32 {
    let line_h = gui::measure_text(font, "Ag").1.max(12);
    let width = width.max(40);
    let mut lines = 0;
    for paragraph in text.split('\n') {
        let paragraph = paragraph.trim_end_matches('\r');
        let mut current = String::new();
        lines += 1;
        for word in paragraph.split(' ') {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if !current.is_empty() && gui::measure_text(font, &candidate).0 > width {
                lines += 1;
                current = word.to_string();
            } else {
                current = candidate;
            }
            // Mot plus long que la ligne : coupé au caractère.
            let w = gui::measure_text(font, &current).0;
            if w > width {
                lines += (w / width) as usize;
                current.clear();
            }
        }
    }
    lines as i32 * line_h
}

fn button_width(font: &FontSpec, label: &str) -> i32 {
    (gui::measure_text(font, label).0 + 28).max(88)
}

/// Place les éléments de la boîte : renvoie les rectangles et la taille de la
/// zone cliente (largeur, hauteur).
pub fn plan(spec: &Spec, expanded: bool) -> (Vec<(Elem, Rect)>, i32, i32) {
    let normal = normal_font();
    let big = main_font();
    let links = spec.command_links();
    let in_footer = spec.flags & TDF_EXPAND_FOOTER_AREA != 0;
    let show_expanded = expanded && !spec.expanded.is_empty();

    // Rangée du bas : expando à gauche, boutons poussoirs à droite.
    let pushes: Vec<usize> = (0..spec.buttons.len())
        .filter(|&i| !(links && i < spec.custom))
        .collect();
    let widths: Vec<i32> = pushes
        .iter()
        .map(|&i| button_width(&normal, &spec.buttons[i].1))
        .collect();
    let expando_w = if spec.expanded.is_empty() {
        0
    } else {
        let a = gui::measure_text(&normal, &spec.expando_label(false)).0;
        let b = gui::measure_text(&normal, &spec.expando_label(true)).0;
        a.max(b) + 28
    };
    let row_w = widths.iter().sum::<i32>() + 8 * widths.len().saturating_sub(1) as i32;
    let min_w = row_w + if expando_w > 0 { expando_w + 16 } else { 0 } + 2 * MARGIN;
    let width = if spec.width > 0 {
        spec.width.max(min_w)
    } else {
        // Automatique : selon la longueur des textes, entre 360 et 520 pixels.
        let longest = [&spec.main, &spec.content]
            .iter()
            .flat_map(|t| t.split('\n'))
            .map(|l| gui::measure_text(&normal, l).0)
            .max()
            .unwrap_or(0);
        (longest / 2 + 2 * MARGIN + 60).clamp(360, 520).max(min_w)
    };

    let mut out = Vec::new();
    let x0 = MARGIN + if spec.icon.is_some() { ICON + 12 } else { 0 };
    let tw = width - x0 - MARGIN;
    let mut y = MARGIN + 4;
    if spec.icon.is_some() {
        out.push((Elem::Icon, Rect::from_size(MARGIN, y, ICON, ICON + 4)));
    }
    let block =
        |elem: Elem, font: &FontSpec, text: &str, out: &mut Vec<(Elem, Rect)>, y: &mut i32| {
            let h = text_height(font, text, tw) + 2;
            out.push((elem, Rect::from_size(x0, *y, tw, h)));
            *y += h + 10;
        };
    if !spec.main.is_empty() {
        block(Elem::Main, &big, &spec.main, &mut out, &mut y);
    }
    if !spec.content.is_empty() {
        block(Elem::Content, &normal, &spec.content, &mut out, &mut y);
    }
    if show_expanded && !in_footer {
        block(Elem::Expanded, &normal, &spec.expanded, &mut out, &mut y);
    }
    if spec.flags & (TDF_SHOW_PROGRESS_BAR | TDF_SHOW_MARQUEE_PROGRESS_BAR) != 0 {
        out.push((Elem::Progress, Rect::from_size(x0, y, tw, 16)));
        y += 26;
    }
    for i in 0..spec.radios.len() {
        out.push((Elem::Radio(i), Rect::from_size(x0, y, tw, 22)));
        y += 24;
    }
    if !spec.radios.is_empty() {
        y += 4;
    }
    if links {
        for (i, bouton) in spec.buttons.iter().enumerate().take(spec.custom) {
            let h = if bouton.1.contains('\n') {
                LINK_H + 8
            } else {
                LINK_H
            };
            out.push((Elem::Button(i), Rect::from_size(x0, y, tw, h)));
            y += h + 4;
        }
    }
    if spec.icon.is_some() {
        y = y.max(MARGIN + 4 + ICON + 12);
    }

    // Rangée des boutons.
    let row_y = y + 4;
    if expando_w > 0 {
        out.push((
            Elem::Expando,
            Rect::from_size(MARGIN, row_y, expando_w, BUTTON_H),
        ));
    }
    let mut x = width - MARGIN - row_w;
    for (k, &i) in pushes.iter().enumerate() {
        out.push((
            Elem::Button(i),
            Rect::from_size(x, row_y, widths[k], BUTTON_H),
        ));
        x += widths[k] + 8;
    }
    y = if pushes.is_empty() && expando_w == 0 {
        y
    } else {
        row_y + BUTTON_H + 10
    };
    if !spec.verification.is_empty() {
        out.push((
            Elem::Verification,
            Rect::from_size(MARGIN, y, width - 2 * MARGIN, 22),
        ));
        y += 28;
    }
    if !spec.footer.is_empty() {
        let h = text_height(&normal, &spec.footer, width - 2 * MARGIN) + 2;
        out.push((
            Elem::Footer,
            Rect::from_size(MARGIN, y, width - 2 * MARGIN, h),
        ));
        y += h + 10;
    }
    if show_expanded && in_footer {
        let h = text_height(&normal, &spec.expanded, width - 2 * MARGIN) + 2;
        out.push((
            Elem::Expanded,
            Rect::from_size(MARGIN, y, width - 2 * MARGIN, h),
        ));
        y += h + 10;
    }
    (out, width, y + MARGIN - 6)
}

// ---------- Fenêtre ----------

type Callback = unsafe extern "win64" fn(usize, u32, usize, isize, isize) -> i32;

struct State {
    spec: Spec,
    instance: usize,
    callback: usize,
    data: isize,
    hwnd: usize,
    controls: HashMap<Elem, usize>,
    fonts: Vec<usize>,
    expanded: bool,
    radio: i32,
    verified: bool,
    result: i32,
    closed: bool,
    timer_start: std::time::Instant,
}

type Shared = Arc<Mutex<State>>;

fn boxes() -> &'static Mutex<HashMap<usize, Shared>> {
    static B: OnceLock<Mutex<HashMap<usize, Shared>>> = OnceLock::new();
    B.get_or_init(|| Mutex::new(HashMap::new()))
}

fn state_of(hwnd: usize) -> Option<Shared> {
    lock(boxes()).get(&hwnd).cloned()
}

/// Appelle la procédure du programme, sans tenir aucun verrou.
unsafe fn notify(st: &Shared, code: u32, wp: usize, lp: isize) -> i32 {
    let (callback, hwnd, data) = {
        let s = lock(st);
        (s.callback, s.hwnd, s.data)
    };
    if callback == 0 {
        return S_OK;
    }
    let f: Callback = std::mem::transmute(callback);
    f(hwnd, code, wp, lp, data)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

unsafe fn set_text(hwnd: usize, text: &str) {
    let w = wide(text);
    user32::set_window_text_w(hwnd, w.as_ptr());
}

/// Replace les contrôles et ajuste la hauteur de la fenêtre.
unsafe fn relayout(st: &Shared) {
    let (dlg, rects, width, height, controls) = {
        let s = lock(st);
        let (rects, w, h) = plan(&s.spec, s.expanded);
        (s.hwnd, rects, w, h, s.controls.clone())
    };
    for (&elem, &hwnd) in &controls {
        match rects.iter().find(|(e, _)| *e == elem) {
            Some((_, r)) => {
                user32::move_window(hwnd, r.left, r.top, r.right - r.left, r.bottom - r.top, 1);
                user32::show_window(hwnd, 5);
            }
            None => {
                user32::show_window(hwnd, 0);
            }
        }
    }
    let mut wr = Rect::default();
    let mut cr = Rect::default();
    if user32::get_window_rect(dlg, &mut wr) != 0 && user32::get_client_rect(dlg, &mut cr) != 0 {
        let fw = (wr.right - wr.left) - (cr.right - cr.left);
        let fh = (wr.bottom - wr.top) - (cr.bottom - cr.top);
        user32::set_window_pos(
            dlg,
            0,
            0,
            0,
            width + fw,
            height + fh,
            0x0002 | 0x0004 | 0x0010,
        );
    }
    user32::invalidate_rect(dlg, std::ptr::null(), 1);
}

/// Crée les contrôles pendant WM_INITDIALOG.
unsafe fn build(st: &Shared) {
    let (dlg, spec, expanded) = {
        let s = lock(st);
        (s.hwnd, s.spec.clone(), s.expanded)
    };
    let (rects, _, _) = plan(&spec, true);
    let rect_of = |elem: Elem| {
        rects
            .iter()
            .find(|(e, _)| *e == elem)
            .map_or((0, 0, 10, 10), |(_, r)| {
                (r.left, r.top, r.right - r.left, r.bottom - r.top)
            })
    };
    let mut controls = HashMap::new();
    let mut fonts = Vec::new();
    let static_style = SS_NOPREFIX;
    if let Some(icon) = spec.icon {
        let h = dialogs::add_control(
            dlg,
            "STATIC",
            icon.glyph(),
            ID_ICON,
            SS_CENTER | SS_NOPREFIX,
            0,
            rect_of(Elem::Icon),
        );
        let font = gui::create_font(icon_font());
        user32::send_message_w(h, WM_SETFONT, font, 0);
        fonts.push(font);
        controls.insert(Elem::Icon, h);
    }
    if !spec.main.is_empty() {
        let h = dialogs::add_control(
            dlg,
            "STATIC",
            &spec.main,
            ID_MAIN,
            static_style,
            0,
            rect_of(Elem::Main),
        );
        let font = gui::create_font(main_font());
        user32::send_message_w(h, WM_SETFONT, font, 0);
        fonts.push(font);
        controls.insert(Elem::Main, h);
    }
    if !spec.content.is_empty() {
        let h = dialogs::add_control(
            dlg,
            "STATIC",
            &spec.content,
            ID_CONTENT,
            static_style,
            0,
            rect_of(Elem::Content),
        );
        controls.insert(Elem::Content, h);
    }
    if !spec.expanded.is_empty() {
        let h = dialogs::add_control(
            dlg,
            "STATIC",
            &spec.expanded,
            ID_EXPANDED,
            static_style,
            0,
            rect_of(Elem::Expanded),
        );
        controls.insert(Elem::Expanded, h);
        let h = dialogs::add_control(
            dlg,
            "BUTTON",
            &spec.expando_label(expanded),
            ID_EXPANDO,
            WS_TABSTOP,
            0,
            rect_of(Elem::Expando),
        );
        controls.insert(Elem::Expando, h);
    }
    if spec.flags & (TDF_SHOW_PROGRESS_BAR | TDF_SHOW_MARQUEE_PROGRESS_BAR) != 0 {
        let marquee = if spec.flags & TDF_SHOW_MARQUEE_PROGRESS_BAR != 0 {
            PBS_MARQUEE
        } else {
            0
        };
        let h = dialogs::add_control(
            dlg,
            "msctls_progress32",
            "",
            ID_PROGRESS,
            marquee,
            0,
            rect_of(Elem::Progress),
        );
        controls.insert(Elem::Progress, h);
    }
    for (i, (id, label)) in spec.radios.iter().enumerate() {
        let group = if i == 0 { WS_GROUP } else { 0 };
        let h = dialogs::add_control(
            dlg,
            "BUTTON",
            label,
            *id as usize,
            BS_AUTORADIOBUTTON | WS_TABSTOP | group,
            0,
            rect_of(Elem::Radio(i)),
        );
        controls.insert(Elem::Radio(i), h);
    }
    let default = spec.default_id();
    let links = spec.command_links();
    for (i, (id, label)) in spec.buttons.iter().enumerate() {
        let link = links && i < spec.custom;
        let mut style = WS_TABSTOP;
        if *id == default {
            style |= BS_DEFPUSHBUTTON;
        }
        if link {
            style |= BS_LEFT | BS_MULTILINE;
        }
        let text = if link {
            format!("➜  {label}")
        } else {
            label.clone()
        };
        let h = dialogs::add_control(
            dlg,
            "BUTTON",
            &text,
            *id as usize,
            style,
            0,
            rect_of(Elem::Button(i)),
        );
        controls.insert(Elem::Button(i), h);
    }
    if !spec.verification.is_empty() {
        let h = dialogs::add_control(
            dlg,
            "BUTTON",
            &spec.verification,
            ID_VERIFY,
            BS_AUTOCHECKBOX | WS_TABSTOP,
            0,
            rect_of(Elem::Verification),
        );
        if spec.flags & TDF_VERIFICATION_FLAG_CHECKED != 0 {
            user32::send_message_w(h, BM_SETCHECK, 1, 0);
        }
        controls.insert(Elem::Verification, h);
    }
    if !spec.footer.is_empty() {
        let h = dialogs::add_control(
            dlg,
            "STATIC",
            &spec.footer,
            ID_FOOTER,
            static_style,
            0,
            rect_of(Elem::Footer),
        );
        controls.insert(Elem::Footer, h);
    }
    let radio = spec.default_radio_id();
    {
        let mut s = lock(st);
        s.controls = controls;
        s.fonts = fonts;
        s.radio = radio;
        s.verified = spec.flags & TDF_VERIFICATION_FLAG_CHECKED != 0;
    }
    if radio != 0 {
        check_radio(st, radio);
    }
    user32::send_message_w(dlg, DM_SETDEFID, default as usize, 0);
    relayout(st);
    if let Some(h) = control_by_id(st, default) {
        user32::set_focus(h);
    }
}

fn control_by_id(st: &Shared, id: i32) -> Option<usize> {
    let s = lock(st);
    let index = s.spec.buttons.iter().position(|b| b.0 == id)?;
    s.controls.get(&Elem::Button(index)).copied()
}

fn radio_control(st: &Shared, id: i32) -> Option<usize> {
    let s = lock(st);
    let index = s.spec.radios.iter().position(|r| r.0 == id)?;
    s.controls.get(&Elem::Radio(index)).copied()
}

unsafe fn check_radio(st: &Shared, id: i32) {
    let all: Vec<(i32, usize)> = {
        let s = lock(st);
        s.spec
            .radios
            .iter()
            .enumerate()
            .filter_map(|(i, r)| s.controls.get(&Elem::Radio(i)).map(|&h| (r.0, h)))
            .collect()
    };
    for (rid, h) in all {
        user32::send_message_w(h, BM_SETCHECK, (rid == id) as usize, 0);
    }
    lock(st).radio = id;
}

/// Un bouton de la boîte a été choisi : la procédure peut refuser la
/// fermeture en renvoyant S_FALSE.
unsafe fn button_clicked(st: &Shared, id: i32) {
    if notify(st, TDN_BUTTON_CLICKED, id as usize, 0) == S_FALSE {
        return;
    }
    let dlg = {
        let mut s = lock(st);
        s.result = id;
        s.closed = true;
        s.hwnd
    };
    dialogs::end_dialog(dlg, id as isize);
}

unsafe fn set_verification(st: &Shared, checked: bool) {
    let h = lock(st).controls.get(&Elem::Verification).copied();
    if let Some(h) = h {
        user32::send_message_w(h, BM_SETCHECK, checked as usize, 0);
    }
    lock(st).verified = checked;
    notify(st, TDN_VERIFICATION_CLICKED, checked as usize, 0);
}

unsafe fn toggle_expando(st: &Shared) {
    let (h, label, expanded) = {
        let mut s = lock(st);
        s.expanded = !s.expanded;
        let label = s.spec.expando_label(s.expanded);
        (s.controls.get(&Elem::Expando).copied(), label, s.expanded)
    };
    if let Some(h) = h {
        set_text(h, &label);
    }
    relayout(st);
    notify(st, TDN_EXPANDO_BUTTON_CLICKED, expanded as usize, 0);
}

unsafe fn element_text(st: &Shared, element: usize, text: isize) -> isize {
    let instance = lock(st).instance;
    let value = load_text(instance, text as usize, lock(st).spec.flags);
    let h = {
        let mut s = lock(st);
        let elem = match element {
            0 => {
                s.spec.content = value.clone();
                Elem::Content
            }
            1 => {
                s.spec.expanded = value.clone();
                Elem::Expanded
            }
            2 => {
                s.spec.footer = value.clone();
                Elem::Footer
            }
            3 => {
                s.spec.main = value.clone();
                Elem::Main
            }
            _ => return 0,
        };
        s.controls.get(&elem).copied()
    };
    match h {
        Some(h) => {
            set_text(h, &value);
            relayout(st);
            1
        }
        // Un élément absent à la création ne peut pas apparaître ensuite.
        None => 0,
    }
}

unsafe extern "win64" fn task_dialog_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    if msg == WM_INITDIALOG {
        let shared = Arc::from_raw(lp as *const Mutex<State>);
        lock(&shared).hwnd = hwnd;
        lock(boxes()).insert(hwnd, shared.clone());
        build(&shared);
        if lock(&shared).spec.flags & TDF_CALLBACK_TIMER != 0 {
            user32::set_timer(hwnd, TIMER_ID, 200, 0);
        }
        notify(&shared, TDN_DIALOG_CONSTRUCTED, 0, 0);
        notify(&shared, TDN_CREATED, 0, 0);
        return 0; // le focus est déjà sur le bouton par défaut
    }
    let Some(st) = state_of(hwnd) else {
        return 0;
    };
    let result = handle(&st, hwnd, msg, wp, lp);
    if (TDM_CLICK_BUTTON..=TDM_UPDATE_ICON).contains(&msg) {
        // Résultat d'un message TDM_* : DWLP_MSGRESULT, comme une boîte de dialogue.
        user32::set_window_long_ptr_w(hwnd, 0, result);
        return 1;
    }
    result
}

unsafe fn handle(st: &Shared, hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    let st = st.clone();
    match msg {
        WM_COMMAND => {
            let id = (wp & 0xFFFF) as i32;
            let source = lp as usize;
            let (radio_ids, button_ids, controls) = {
                let s = lock(&st);
                (
                    s.spec.radios.iter().map(|r| r.0).collect::<Vec<_>>(),
                    s.spec.buttons.iter().map(|b| b.0).collect::<Vec<_>>(),
                    s.controls.clone(),
                )
            };
            let from = |elem: Elem| controls.get(&elem).copied() == Some(source) && source != 0;
            if from(Elem::Expando) {
                toggle_expando(&st);
            } else if from(Elem::Verification) {
                let h = controls[&Elem::Verification];
                let checked = user32::send_message_w(h, BM_GETCHECK, 0, 0) != 0;
                lock(&st).verified = checked;
                notify(&st, TDN_VERIFICATION_CLICKED, checked as usize, 0);
            } else if let Some(i) = radio_ids
                .iter()
                .enumerate()
                .position(|(i, _)| from(Elem::Radio(i)))
            {
                check_radio(&st, radio_ids[i]);
                notify(&st, TDN_RADIO_BUTTON_CLICKED, radio_ids[i] as usize, 0);
            } else if button_ids.contains(&id) {
                button_clicked(&st, id);
            } else if id == IDCANCEL && lock(&st).spec.cancellable() {
                // Échap ou fermeture de la fenêtre.
                button_clicked(&st, IDCANCEL);
            }
            1
        }
        WM_TIMER if wp == TIMER_ID => {
            let elapsed = lock(&st).timer_start.elapsed().as_millis() as usize;
            if notify(&st, TDN_TIMER, elapsed, 0) == S_FALSE {
                lock(&st).timer_start = std::time::Instant::now();
            }
            1
        }
        WM_CTLCOLORSTATIC => {
            let (main, icon, color) = {
                let s = lock(&st);
                (
                    s.controls.get(&Elem::Main).copied(),
                    s.controls.get(&Elem::Icon).copied(),
                    s.spec.icon.map(Icon::color),
                )
            };
            let color = if Some(lp as usize) == main {
                Some(0x0099_3300) // bleu de l'instruction principale
            } else if Some(lp as usize) == icon {
                color
            } else {
                None
            };
            match color {
                Some(c) => {
                    super::gdi32::set_text_color(wp, c);
                    user32::get_sys_color_brush(15) as isize
                }
                None => 0,
            }
        }
        TDM_CLICK_BUTTON => {
            button_clicked(&st, wp as i32);
            1
        }
        TDM_CLICK_RADIO_BUTTON => {
            if radio_control(&st, wp as i32).is_some() {
                check_radio(&st, wp as i32);
                notify(&st, TDN_RADIO_BUTTON_CLICKED, wp, 0);
            }
            1
        }
        TDM_CLICK_VERIFICATION => {
            set_verification(&st, wp != 0);
            1
        }
        TDM_ENABLE_BUTTON => {
            if let Some(h) = control_by_id(&st, wp as i32) {
                user32::enable_window(h, (lp != 0) as i32);
            }
            1
        }
        TDM_ENABLE_RADIO_BUTTON => {
            if let Some(h) = radio_control(&st, wp as i32) {
                user32::enable_window(h, (lp != 0) as i32);
            }
            1
        }
        TDM_SET_ELEMENT_TEXT | TDM_UPDATE_ELEMENT_TEXT => element_text(&st, wp, lp),
        TDM_SET_PROGRESS_BAR_RANGE
        | TDM_SET_PROGRESS_BAR_POS
        | TDM_SET_PROGRESS_BAR_STATE
        | TDM_SET_PROGRESS_BAR_MARQUEE => {
            let h = lock(&st).controls.get(&Elem::Progress).copied();
            let Some(h) = h else {
                return 0;
            };
            let pbm = match msg {
                TDM_SET_PROGRESS_BAR_RANGE => 0x0406, // PBM_SETRANGE32
                TDM_SET_PROGRESS_BAR_POS => 0x0402,   // PBM_SETPOS
                TDM_SET_PROGRESS_BAR_STATE => 0x0410, // PBM_SETSTATE
                _ => 0x040A,                          // PBM_SETMARQUEE
            };
            if msg == TDM_SET_PROGRESS_BAR_RANGE {
                // lParam = MAKELPARAM(min, max).
                let (lo, hi) = ((lp & 0xFFFF) as usize, (lp >> 16) & 0xFFFF);
                return user32::send_message_w(h, pbm, lo, hi);
            }
            user32::send_message_w(h, pbm, wp, lp)
        }
        TDM_SET_MARQUEE_PROGRESS_BAR => {
            let h = lock(&st).controls.get(&Elem::Progress).copied();
            if let Some(h) = h {
                let style = gui::window_style(h).unwrap_or(0);
                let style = if wp != 0 {
                    style | PBS_MARQUEE
                } else {
                    style & !PBS_MARQUEE
                };
                user32::set_window_long_ptr_w(h, -16, style as isize);
            }
            1
        }
        TDM_SET_BUTTON_ELEVATION_REQUIRED_STATE | TDM_UPDATE_ICON => 1,
        WM_DESTROY => {
            notify(&st, TDN_DESTROYED, 0, 0);
            user32::kill_timer(hwnd, TIMER_ID);
            lock(boxes()).remove(&hwnd);
            0
        }
        _ => 0,
    }
}

// ---------- Lecture de TASKDIALOGCONFIG ----------

/// Chaîne d'un champ PCWSTR : texte, ou MAKEINTRESOURCE chargé depuis les
/// ressources du module.
unsafe fn load_text(instance: usize, value: usize, flags: u32) -> String {
    let text = if value == 0 {
        String::new()
    } else if value < 0x1_0000 {
        let mut buf = vec![0u16; 2048];
        let n = super::resource::load_string_w(
            instance,
            value as u32,
            buf.as_mut_ptr(),
            buf.len() as i32,
        );
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    } else {
        super::wstr(value as *const u16)
    };
    if flags & TDF_ENABLE_HYPERLINKS != 0 {
        strip_links(&text)
    } else {
        text
    }
}

unsafe fn rd<T: Copy>(base: *const u8, offset: usize) -> T {
    std::ptr::read_unaligned(base.add(offset) as *const T)
}

/// TASKDIALOGCONFIG (structure compactée sur 1 octet, 160 octets en 64 bits).
unsafe fn read_config(config: *const u8) -> Option<(Spec, usize, usize, usize, isize)> {
    if config.is_null() || rd::<u32>(config, 0) < 160 {
        return None;
    }
    let parent: usize = rd(config, 4);
    let instance: usize = rd(config, 12);
    let flags: u32 = rd(config, 20);
    let common: u32 = rd(config, 24);
    let text = |offset: usize| load_text(instance, rd::<usize>(config, offset), flags);
    let main_icon: usize = rd(config, 36);
    let count = rd::<u32>(config, 60).min(256) as usize;
    let list: usize = rd(config, 64);
    let radio_count = rd::<u32>(config, 76).min(256) as usize;
    let radio_list: usize = rd(config, 80);
    let read_buttons = |list: usize, count: usize| -> Vec<(i32, String)> {
        if list == 0 {
            return Vec::new();
        }
        (0..count)
            .map(|i| {
                let item = (list + i * 12) as *const u8;
                (
                    rd::<i32>(item, 0),
                    load_text(instance, rd::<usize>(item, 4), flags),
                )
            })
            .collect()
    };
    let mut buttons = read_buttons(list, count);
    let custom = buttons.len();
    buttons.extend(common_buttons(common));
    if buttons.is_empty() {
        buttons = common_buttons(1);
    }
    let base_x = super::win32_basics::DIALOG_BASE_X;
    let width_du: u32 = rd(config, 156);
    let spec = Spec {
        flags,
        title: text(28),
        main: text(44),
        content: text(52),
        expanded: text(100),
        expanded_label: text(108),
        collapsed_label: text(116),
        verification: text(92),
        footer: text(132),
        icon: if flags & TDF_USE_HICON_MAIN != 0 {
            None
        } else {
            Icon::from_resource(main_icon)
        },
        buttons,
        custom,
        radios: read_buttons(radio_list, radio_count),
        default_button: rd(config, 72),
        default_radio: rd(config, 88),
        width: (width_du as i32).min(2000) * base_x / 4,
    };
    let callback: usize = rd(config, 140);
    let data: isize = rd(config, 148);
    Some((spec, parent, instance, callback, data))
}

/// Titre par défaut : le nom de l'exécutable, comme Windows.
fn default_title() -> String {
    crate::loader::main_module()
        .and_then(|m| m.path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "Winlux".into())
}

/// Affiche la boîte et renvoie (bouton, radio, case cochée).
pub unsafe fn run(
    spec: Spec,
    parent: usize,
    instance: usize,
    callback: usize,
    data: isize,
) -> (i32, i32, bool) {
    let mut spec = spec;
    if spec.title.is_empty() {
        spec.title = default_title();
    }
    let expanded = spec.flags & TDF_EXPANDED_BY_DEFAULT != 0;
    let (_, width, height) = plan(&spec, expanded);
    let title = spec.title.clone();
    let default = spec.default_id();
    let radio = spec.default_radio_id();
    let verified = spec.flags & TDF_VERIFICATION_FLAG_CHECKED != 0;
    let shared: Shared = Arc::new(Mutex::new(State {
        spec,
        instance,
        callback,
        data,
        hwnd: 0,
        controls: HashMap::new(),
        fonts: Vec::new(),
        expanded,
        radio,
        verified,
        result: default,
        closed: false,
        timer_start: std::time::Instant::now(),
    }));
    let init = Arc::into_raw(shared.clone()) as isize;
    dialogs::run_builtin(
        parent,
        &title,
        width,
        height,
        task_dialog_proc as *const () as usize,
        init,
    );
    let s = lock(&shared);
    for &font in &s.fonts {
        super::gdi32::delete_object(font);
    }
    if !gui::backend_is_interactive() && !s.closed {
        eprintln!(
            "winlux: TaskDialog [{}] {} {}",
            s.spec.title, s.spec.main, s.spec.content
        );
    }
    // Sans affichage, personne ne répond : le bouton par défaut est choisi.
    let button = if s.closed { s.result } else { default };
    (button, s.radio, s.verified)
}

/// TaskDialogIndirect
pub unsafe extern "win64" fn task_dialog_indirect(
    config: *const u8,
    button: *mut i32,
    radio: *mut i32,
    verification: *mut i32,
) -> i32 {
    let Some((spec, parent, instance, callback, data)) = read_config(config) else {
        return E_INVALIDARG;
    };
    // Diagnostic : titre tel que le programme l'a passé (NULL, identifiant de
    // ressource ou texte), pour distinguer un titre vide d'un texte reçu.
    crate::log_info!(
        "comctl32",
        "TaskDialogIndirect : pszWindowTitle={:#x} « {} »",
        rd::<usize>(config, 28),
        spec.title
    );
    let (b, r, v) = run(spec, parent, instance, callback, data);
    if !button.is_null() {
        *button = b;
    }
    if !radio.is_null() {
        *radio = r;
    }
    if !verification.is_null() {
        *verification = v as i32;
    }
    S_OK
}

/// TaskDialog : la forme simple, sans procédure de rappel.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn task_dialog(
    parent: usize,
    instance: usize,
    title: *const u16,
    main: *const u16,
    content: *const u16,
    common: i32,
    icon: *const u16,
    button: *mut i32,
) -> i32 {
    let mut buttons = common_buttons(common as u32);
    if buttons.is_empty() {
        buttons = common_buttons(1);
    }
    let spec = Spec {
        title: load_text(instance, title as usize, 0),
        main: load_text(instance, main as usize, 0),
        content: load_text(instance, content as usize, 0),
        icon: Icon::from_resource(icon as usize),
        buttons,
        ..Spec::default()
    };
    let (b, _, _) = run(spec, parent, instance, 0, 0);
    if !button.is_null() {
        *button = b;
    }
    S_OK
}

#[cfg(test)]
mod tests;
