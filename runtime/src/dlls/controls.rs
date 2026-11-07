//! Contrôles standard de Windows : classes prédéfinies BUTTON, STATIC, EDIT,
//! LISTBOX, COMBOBOX et SCROLLBAR, dessinées dans le style de Windows 11.
//!
//! Sans elles, un programme Win32 classique (PuTTY, les assistants
//! d'installation, les boîtes de réglages) crée ses champs et ses boutons
//! sans erreur… et n'affiche rien. Chaque contrôle est une fenêtre enfant
//! (sous-fenêtre X11 de son parent) dont la procédure est écrite ici : elle
//! peint avec le GDI de Winlux, gère la souris et le clavier, et prévient son
//! parent par `WM_COMMAND` comme le fait Windows.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use super::gui::{self, PaintStruct, Rect};
use super::{gdi32, lock, user32, Exports};

// ---------- Messages et constantes ----------

const WM_CREATE: u32 = 0x0001;
const WM_DESTROY: u32 = 0x0002;
const WM_SIZE: u32 = 0x0005;
const WM_SETFOCUS: u32 = 0x0007;
const WM_KILLFOCUS: u32 = 0x0008;
const WM_ENABLE: u32 = 0x000A;
const WM_SETTEXT: u32 = 0x000C;
const WM_GETTEXT: u32 = 0x000D;
const WM_GETTEXTLENGTH: u32 = 0x000E;
const WM_PAINT: u32 = 0x000F;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_SETFONT: u32 = 0x0030;
const WM_GETFONT: u32 = 0x0031;
const WM_DRAWITEM: u32 = 0x002B;
const WM_NCCREATE: u32 = 0x0081;
const WM_NCDESTROY: u32 = 0x0082;
const WM_GETDLGCODE: u32 = 0x0087;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_CHAR: u32 = 0x0102;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_COMMAND: u32 = 0x0111;
const WM_HSCROLL: u32 = 0x0114;
const WM_VSCROLL: u32 = 0x0115;
const WM_CTLCOLOREDIT: u32 = 0x0133;
const WM_CTLCOLORLISTBOX: u32 = 0x0134;
const WM_CTLCOLORBTN: u32 = 0x0135;
const WM_CTLCOLORSCROLLBAR: u32 = 0x0137;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_LBUTTONDBLCLK: u32 = 0x0203;
const WM_MOUSEWHEEL: u32 = 0x020A;
const WM_CUT: u32 = 0x0300;
const WM_COPY: u32 = 0x0301;
const WM_PASTE: u32 = 0x0302;
const WM_CLEAR: u32 = 0x0303;
const WM_UNDO: u32 = 0x0304;

const WS_BORDER: u32 = 0x0080_0000;
const WS_DISABLED: u32 = 0x0800_0000;
const WS_GROUP: u32 = 0x0002_0000;
const WS_EX_CLIENTEDGE: u32 = 0x0000_0200;

const DLGC_WANTARROWS: isize = 0x0001;
const DLGC_WANTALLKEYS: isize = 0x0004;
const DLGC_HASSETSEL: isize = 0x0008;
const DLGC_DEFPUSHBUTTON: isize = 0x0010;
const DLGC_UNDEFPUSHBUTTON: isize = 0x0020;
const DLGC_RADIOBUTTON: isize = 0x0040;
const DLGC_WANTCHARS: isize = 0x0080;
const DLGC_STATIC: isize = 0x0100;
const DLGC_BUTTON: isize = 0x2000;

const VK_RETURN: usize = 0x0D;
const VK_SHIFT: usize = 0x10;
const VK_CONTROL: usize = 0x11;
const VK_ESCAPE: usize = 0x1B;
const VK_SPACE: usize = 0x20;
const VK_PRIOR: usize = 0x21;
const VK_NEXT: usize = 0x22;
const VK_END: usize = 0x23;
const VK_HOME: usize = 0x24;
const VK_LEFT: usize = 0x25;
const VK_UP: usize = 0x26;
const VK_RIGHT: usize = 0x27;
const VK_DOWN: usize = 0x28;
const VK_DELETE: usize = 0x2E;
const VK_F4: usize = 0x73;

// Couleurs COLORREF (0x00BBGGRR) de Windows 11, thème clair.
const ACCENT: u32 = 0x00F0_3D6A; // #6A3DF0, violet Prisme
const TEXT: u32 = 0x001B_1B1B;
const TEXT_DISABLED: u32 = 0x00A0_A0A0;
const CONTROL_FILL: u32 = 0x00FD_FDFD;
const CONTROL_PRESSED: u32 = 0x00F0_F0F0;
const CONTROL_BORDER: u32 = 0x00D0_D0D0;
const STRONG_BORDER: u32 = 0x0086_8686;
const FIELD: u32 = 0x00FF_FFFF;
const SELECTION_SOFT: u32 = 0x00E5_E5E5;

const LB_ERR: isize = -1;

// ---------- Jeu de caractères des messages ----------

thread_local! {
    /// Vrai pendant un `SendMessageW`/`CreateWindowExW`, faux pendant la
    /// variante A : les chaînes passées en `lParam` changent de codage.
    static WIDE: Cell<Option<bool>> = const { Cell::new(None) };
}

/// Exécute `f` avec le jeu de caractères des chaînes de message fixé.
pub fn with_charset<R>(wide: bool, f: impl FnOnce() -> R) -> R {
    let old = WIDE.with(|w| w.replace(Some(wide)));
    let r = f();
    WIDE.with(|w| w.set(old));
    r
}

/// Codage des chaînes du message en cours : celui de l'appel, sinon celui
/// de la fonction qui a créé la fenêtre.
pub fn message_is_wide(hwnd: usize) -> bool {
    wide_for(hwnd)
}

fn wide_for(hwnd: usize) -> bool {
    WIDE.with(|w| w.get())
        .or_else(|| lock(state()).get(&hwnd).map(|c| c.wide))
        .unwrap_or(true)
}

unsafe fn read_str(hwnd: usize, lp: isize) -> String {
    if lp == 0 {
        return String::new();
    }
    if wide_for(hwnd) {
        super::wstr(lp as *const u16)
    } else {
        super::cstr(lp as *const u8)
    }
}

/// Copie `s` dans un tampon du programme (A ou W) ; renvoie la longueur
/// copiée sans le NUL final.
unsafe fn write_str(hwnd: usize, s: &str, buf: isize, cap: u32) -> isize {
    if buf == 0 || cap == 0 {
        return 0;
    }
    if wide_for(hwnd) {
        super::put_text_w(s, buf as *mut u16, cap) as isize
    } else {
        super::put_text_a(s, buf as *mut u8, cap) as isize
    }
}

fn text_len(hwnd: usize, s: &str) -> isize {
    if wide_for(hwnd) {
        s.encode_utf16().count() as isize
    } else {
        super::encode_ansi(s).len() as isize
    }
}

// ---------- État des contrôles ----------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Button,
    Static,
    Edit,
    ListBox,
    ComboBox,
    ScrollBar,
    TreeView,
    ListView,
    Tab,
    StatusBar,
    Progress,
    TrackBar,
    UpDown,
    ToolTip,
    ToolBar,
    ReBar,
    ComboBoxEx,
}

#[derive(Clone, Default)]
struct EditState {
    caret: usize,
    anchor: usize,
    scroll_x: i32,
    top_line: usize,
    limit: usize,
    password: Option<char>,
    modified: bool,
    dragging: bool,
}

#[derive(Clone)]
struct Ctl {
    kind: Kind,
    wide: bool,
    font: usize,
    // Bouton
    check: u32,
    pressed: bool,
    focused: bool,
    // Champ de saisie (EDIT, partie éditable d'une COMBOBOX)
    edit: EditState,
    // Liste (LISTBOX, liste d'une COMBOBOX)
    items: Vec<(String, isize)>,
    selected: Vec<bool>,
    sel: i32,
    top: i32,
    item_h: i32,
    last_click: Option<(Instant, i32)>,
    // COMBOBOX déroulante
    dropped: bool,
    closed_h: i32,
    full_h: i32,
    // SCROLLBAR
    min: i32,
    max: i32,
    pos: i32,
    page: u32,
    drag: Option<(i32, i32)>,
}

impl Ctl {
    fn new(kind: Kind, wide: bool) -> Self {
        Ctl {
            kind,
            wide,
            font: 0,
            check: 0,
            pressed: false,
            focused: false,
            edit: EditState {
                limit: 30_000,
                ..EditState::default()
            },
            items: Vec::new(),
            selected: Vec::new(),
            sel: -1,
            top: 0,
            item_h: 0,
            last_click: None,
            dropped: false,
            closed_h: 0,
            full_h: 0,
            min: 0,
            max: 100,
            pos: 0,
            page: 0,
            drag: None,
        }
    }
}

fn state() -> &'static Mutex<HashMap<usize, Ctl>> {
    static S: OnceLock<Mutex<HashMap<usize, Ctl>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn with_ctl<R>(hwnd: usize, f: impl FnOnce(&mut Ctl) -> R) -> Option<R> {
    lock(state()).get_mut(&hwnd).map(f)
}

fn snapshot(hwnd: usize) -> Option<Ctl> {
    lock(state()).get(&hwnd).cloned()
}

pub fn kind_of(hwnd: usize) -> Option<Kind> {
    lock(state()).get(&hwnd).map(|c| c.kind)
}

// ---------- Classes prédéfinies ----------

macro_rules! control_proc {
    ($name:ident, $kind:expr) => {
        unsafe extern "win64" fn $name(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
            control_proc_impl($kind, hwnd, msg, wp, lp)
        }
    };
}

control_proc!(button_proc, Kind::Button);
control_proc!(static_proc, Kind::Static);
control_proc!(edit_proc, Kind::Edit);
control_proc!(listbox_proc, Kind::ListBox);
control_proc!(combobox_proc, Kind::ComboBox);
control_proc!(scrollbar_proc, Kind::ScrollBar);
control_proc!(treeview_proc, Kind::TreeView);
control_proc!(listview_proc, Kind::ListView);
control_proc!(tab_proc, Kind::Tab);
control_proc!(statusbar_proc, Kind::StatusBar);
control_proc!(progress_proc, Kind::Progress);
control_proc!(trackbar_proc, Kind::TrackBar);
control_proc!(updown_proc, Kind::UpDown);
control_proc!(tooltip_proc, Kind::ToolTip);
control_proc!(toolbar_proc, Kind::ToolBar);
control_proc!(rebar_proc, Kind::ReBar);
control_proc!(comboboxex_proc, Kind::ComboBoxEx);

/// Procédure d'une classe prédéfinie (le nom ne tient pas compte de la casse).
/// `ComboLBox` est la liste déroulante d'une COMBOBOX créée à part par
/// certains programmes ; elle se comporte en LISTBOX.
pub fn builtin_class(name: &str) -> Option<usize> {
    let lower = name.to_ascii_lowercase();
    let proc_addr: unsafe extern "win64" fn(usize, u32, usize, isize) -> isize =
        match lower.as_str() {
            "button" => button_proc,
            "static" => static_proc,
            "edit" => edit_proc,
            "listbox" | "combolbox" => listbox_proc,
            "combobox" => combobox_proc,
            "scrollbar" => scrollbar_proc,
            "#32768" => popup::popup_proc,
            _ => match comctl::class_kind(&lower).or_else(|| bars::class_kind(&lower))? {
                Kind::TreeView => treeview_proc,
                Kind::ListView => listview_proc,
                Kind::Tab => tab_proc,
                Kind::StatusBar => statusbar_proc,
                Kind::Progress => progress_proc,
                Kind::TrackBar => trackbar_proc,
                Kind::UpDown => updown_proc,
                Kind::ToolTip => tooltip_proc,
                Kind::ToolBar => toolbar_proc,
                Kind::ReBar => rebar_proc,
                Kind::ComboBoxEx => comboboxex_proc,
                _ => return None,
            },
        };
    Some(proc_addr as usize)
}

/// Classe désignée par un atome prédéfini (modèles de boîte de dialogue).
#[allow(dead_code)] // utilisée par le gestionnaire de dialogues
pub fn atom_class(atom: u16) -> Option<&'static str> {
    Some(match atom {
        0x0080 => "Button",
        0x0081 => "Edit",
        0x0082 => "Static",
        0x0083 => "ListBox",
        0x0084 => "ScrollBar",
        0x0085 => "ComboBox",
        _ => return None,
    })
}

// ---------- Outils communs ----------

fn style(hwnd: usize) -> u32 {
    gui::window_style(hwnd).unwrap_or(0)
}

fn ex_style(hwnd: usize) -> u32 {
    gui::window_ex_style(hwnd).unwrap_or(0)
}

fn enabled(hwnd: usize) -> bool {
    style(hwnd) & WS_DISABLED == 0
}

fn client(hwnd: usize) -> Rect {
    gui::window_client_rect(hwnd).unwrap_or_default()
}

fn title(hwnd: usize) -> String {
    gui::title_of(hwnd).unwrap_or_default()
}

fn set_title(hwnd: usize, s: &str) {
    gui::set_title(hwnd, s);
}

fn invalidate(hwnd: usize) {
    unsafe {
        user32::invalidate_rect(hwnd, std::ptr::null(), 1);
    }
}

fn parent(hwnd: usize) -> usize {
    gui::window_parent(hwnd).unwrap_or(0)
}

fn send(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    if hwnd == 0 {
        return 0;
    }
    unsafe { user32::send_message_w(hwnd, msg, wp, lp) }
}

/// Prévient le parent par `WM_COMMAND` (identifiant, code de notification).
fn notify(hwnd: usize, code: u16) {
    let id = user32::control_id(hwnd) as usize & 0xFFFF;
    send(
        parent(hwnd),
        WM_COMMAND,
        ((code as usize) << 16) | id,
        hwnd as isize,
    );
}

fn shift_down() -> bool {
    gui::get_key_state(VK_SHIFT) < 0
}

fn ctrl_down() -> bool {
    gui::get_key_state(VK_CONTROL) < 0
}

fn point(lp: isize) -> (i32, i32) {
    (
        lp as u32 as u16 as i16 as i32,
        (lp as u32 >> 16) as u16 as i16 as i32,
    )
}

fn font_of(c: &Ctl) -> usize {
    if c.font != 0 {
        c.font
    } else {
        gdi32::get_stock_object(17) // DEFAULT_GUI_FONT
    }
}

fn measure(font: usize, s: &str) -> (i32, i32) {
    let spec = gui::font_spec(font).unwrap_or_default();
    gui::measure_text(&spec, s)
}

fn line_height(font: usize) -> i32 {
    measure(font, "Ag").1.max(8)
}

/// Retire les marques de raccourci (« &Fichier » → « Fichier », « && » → « & »).
pub(crate) fn strip_prefix(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '&' {
            if chars.peek() == Some(&'&') {
                out.push('&');
                chars.next();
            }
            continue;
        }
        out.push(ch);
    }
    out
}

/// Coupe un texte en lignes qui tiennent dans `width` (retours à la ligne
/// explicites conservés, mots trop longs coupés au caractère).
/// Tabulations remplacées par des espaces jusqu'au prochain multiple de 8
/// caractères, comme DT_EXPANDTABS (#818).
fn expand_tabs(text: &str) -> String {
    if !text.contains('\t') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 8);
    let mut col = 0;
    for ch in text.chars() {
        match ch {
            '\t' => {
                let n = 8 - col % 8;
                out.extend(std::iter::repeat_n(' ', n));
                col += n;
            }
            '\n' => {
                out.push(ch);
                col = 0;
            }
            _ => {
                out.push(ch);
                col += 1;
            }
        }
    }
    out
}

fn wrap(font: usize, text: &str, width: i32) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in text.split('\n') {
        let raw = raw.trim_end_matches('\r');
        if width <= 0 || measure(font, raw).0 <= width {
            lines.push(raw.to_string());
            continue;
        }
        let mut line = String::new();
        for word in raw.split_inclusive(' ') {
            let candidate = format!("{line}{word}");
            if measure(font, candidate.trim_end()).0 <= width || line.is_empty() {
                if measure(font, candidate.trim_end()).0 > width && line.is_empty() {
                    // Mot plus large que la zone : coupe au caractère.
                    for ch in word.chars() {
                        let next = format!("{line}{ch}");
                        if measure(font, &next).0 > width && !line.is_empty() {
                            lines.push(std::mem::take(&mut line));
                        }
                        line.push(ch);
                    }
                } else {
                    line = candidate;
                }
            } else {
                lines.push(line.trim_end().to_string());
                line = word.to_string();
            }
        }
        lines.push(line.trim_end().to_string());
    }
    lines
}

// ---------- Dessin ----------

struct Painter {
    hdc: usize,
    font: usize,
}

impl Painter {
    fn fill(&self, r: Rect, color: u32) {
        let brush = gdi32::create_solid_brush(color);
        unsafe {
            user32::fill_rect(self.hdc, &r, brush);
        }
        gdi32::delete_object(brush);
    }

    fn fill_brush(&self, r: Rect, brush: usize) {
        unsafe {
            user32::fill_rect(self.hdc, &r, brush);
        }
    }

    fn frame(&self, r: Rect, color: u32) {
        let brush = gdi32::create_solid_brush(color);
        unsafe {
            user32::frame_rect(self.hdc, &r, brush);
        }
        gdi32::delete_object(brush);
    }

    /// Forme pleine bordée d'un pixel : le GDI de Winlux remplit RoundRect et
    /// Ellipse avec le pinceau seul, la bordure est donc une forme dessous.
    fn shape(&self, r: Rect, fill: u32, border: u32, radius: i32, ellipse: bool) {
        if border != fill && r.width() > 2 && r.height() > 2 {
            self.solid(r, border, radius, ellipse);
            let inner = Rect {
                left: r.left + 1,
                top: r.top + 1,
                right: r.right - 1,
                bottom: r.bottom - 1,
            };
            self.solid(inner, fill, (radius - 1).max(0), ellipse);
        } else {
            self.solid(r, fill, radius, ellipse);
        }
    }

    fn solid(&self, r: Rect, fill: u32, radius: i32, ellipse: bool) {
        let border = fill;
        let pen = gdi32::create_pen(0, 1, border);
        let brush = gdi32::create_solid_brush(fill);
        let old_pen = gdi32::select_object(self.hdc, pen);
        let old_brush = gdi32::select_object(self.hdc, brush);
        unsafe {
            if ellipse {
                gdi32::ellipse(self.hdc, r.left, r.top, r.right, r.bottom);
            } else {
                gdi32::round_rect(
                    self.hdc,
                    r.left,
                    r.top,
                    r.right,
                    r.bottom,
                    radius * 2,
                    radius * 2,
                );
            }
        }
        gdi32::select_object(self.hdc, old_pen);
        gdi32::select_object(self.hdc, old_brush);
        gdi32::delete_object(pen);
        gdi32::delete_object(brush);
    }

    fn line(&self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32, width: i32) {
        let pen = gdi32::create_pen(0, width, color);
        let old = gdi32::select_object(self.hdc, pen);
        unsafe {
            gdi32::move_to_ex(self.hdc, x1, y1, std::ptr::null_mut());
            gdi32::line_to(self.hdc, x2, y2);
        }
        gdi32::select_object(self.hdc, old);
        gdi32::delete_object(pen);
    }

    /// Texte sur une ligne ; `align` 0 gauche, 1 centre, 2 droite, centré
    /// verticalement dans `r`.
    /// Le texte trop long est raccourci avec « … ».
    fn text(&self, r: Rect, s: &str, color: u32, align: u32) {
        if s.is_empty() || r.width() <= 0 {
            return;
        }
        let mut shown = s.to_string();
        let mut w = measure(self.font, &shown).0;
        if w > r.width() {
            let cs: Vec<char> = s.chars().collect();
            let mut n = cs.len();
            while n > 0 {
                n -= 1;
                shown = cs[..n].iter().collect::<String>() + "…";
                w = measure(self.font, &shown).0;
                if w <= r.width() {
                    break;
                }
            }
        }
        let x = match align {
            1 => r.left + (r.width() - w) / 2,
            2 => r.right - w,
            _ => r.left,
        };
        self.raw_text(x, r.top, r.height(), &shown, color);
    }

    /// Texte à partir de `x`, coupé hors de [`left`, `right`] (champ défilé).
    fn clipped(&self, x: i32, y: i32, h: i32, s: &str, color: u32, left: i32, right: i32) {
        let cs: Vec<char> = s.chars().collect();
        let width_of = |k: usize| measure(self.font, &cs[..k].iter().collect::<String>()).0;
        let mut first = 0;
        while first < cs.len() && x + width_of(first + 1) <= left {
            first += 1;
        }
        let mut last = first;
        while last < cs.len() && x + width_of(last + 1) <= right + 1 {
            last += 1;
        }
        if last > first {
            let part: String = cs[first..last].iter().collect();
            self.raw_text(x + width_of(first), y, h, &part, color);
        }
    }

    fn raw_text(&self, x: i32, y: i32, h: i32, s: &str, color: u32) {
        gdi32::set_bk_mode(self.hdc, 1);
        gdi32::set_text_color(self.hdc, color);
        let old = gdi32::select_object(self.hdc, self.font);
        let mut wide: Vec<u16> = s.encode_utf16().collect();
        wide.push(0);
        let mut rect = Rect::from_size(x, y, measure(self.font, s).0 + 4, h);
        // DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX
        unsafe {
            user32::draw_text_w(
                self.hdc,
                wide.as_ptr(),
                -1,
                &mut rect,
                0x0020 | 0x0004 | 0x0800,
            );
        }
        gdi32::select_object(self.hdc, old);
    }
}

/// Fond donné par le parent (`WM_CTLCOLOR*`) ; à défaut, `default`. Renvoie
/// aussi la couleur du texte que le parent a pu choisir.
fn background(hwnd: usize, hdc: usize, msg: u32, default: u32, p: &Painter, r: Rect) -> u32 {
    gdi32::set_text_color(hdc, TEXT);
    // msg = 0 : pas de WM_CTLCOLOR* (TreeView, ListView).
    let brush = if msg == 0 {
        0
    } else {
        send(parent(hwnd), msg, hdc, hwnd as isize)
    };
    if brush != 0 {
        p.fill_brush(r, brush as usize);
        let color = gdi32::get_text_color(hdc);
        if color == 0 {
            TEXT
        } else {
            color
        }
    } else {
        p.fill(r, default);
        TEXT
    }
}

fn face_color() -> u32 {
    user32::get_sys_color(15) // COLOR_BTNFACE
}

#[repr(C)]
struct DrawItemStruct {
    ctl_type: u32,
    ctl_id: u32,
    item_id: u32,
    item_action: u32,
    item_state: u32,
    hwnd_item: usize,
    hdc: usize,
    rc_item: Rect,
    item_data: usize,
}

fn draw_item(
    hwnd: usize,
    hdc: usize,
    ctl_type: u32,
    item: i32,
    state: u32,
    rect: Rect,
    data: isize,
) {
    let dis = DrawItemStruct {
        ctl_type,
        ctl_id: user32::control_id(hwnd) as u32,
        item_id: item as u32,
        item_action: 1, // ODA_DRAWENTIRE
        item_state: state,
        hwnd_item: hwnd,
        hdc,
        rc_item: rect,
        item_data: data as usize,
    };
    send(
        parent(hwnd),
        WM_DRAWITEM,
        user32::control_id(hwnd) as usize,
        &dis as *const DrawItemStruct as isize,
    );
}

/// Peint le contrôle. `target` : DC fourni par le programme (WM_PAINT avec
/// wParam, WM_PRINTCLIENT) ; Notepad++ y fait peindre onglets et barre
/// d'état dans un DC mémoire qu'il recopie ensuite à l'écran.
fn paint(hwnd: usize, target: usize) {
    let Some(c) = snapshot(hwnd) else {
        return;
    };
    let mut ps: PaintStruct = unsafe { std::mem::zeroed() };
    let hdc = if target != 0 {
        target
    } else {
        unsafe { user32::begin_paint(hwnd, &mut ps) }
    };
    if hdc == 0 {
        return;
    }
    let p = Painter {
        hdc,
        font: font_of(&c),
    };
    let r = client(hwnd);
    match c.kind {
        Kind::Button => paint_button(hwnd, &c, &p, r),
        Kind::Static => paint_static(hwnd, &c, &p, r),
        Kind::Edit => paint_edit(hwnd, &c, &p, r),
        Kind::ListBox => paint_listbox(hwnd, &c, &p, r),
        Kind::ComboBox => paint_combobox(hwnd, &c, &p, r),
        Kind::ScrollBar => paint_scrollbar(hwnd, &c, &p, r),
        kind => comctl::paint(kind, hwnd, &c, &p, r),
    }
    if target == 0 {
        unsafe {
            user32::end_paint(hwnd, &ps);
        }
    }
}

// ---------- BUTTON ----------

const BS_DEFPUSHBUTTON: u32 = 0x1;
const BS_CHECKBOX: u32 = 0x2;
const BS_AUTOCHECKBOX: u32 = 0x3;
const BS_RADIOBUTTON: u32 = 0x4;
const BS_3STATE: u32 = 0x5;
const BS_AUTO3STATE: u32 = 0x6;
const BS_GROUPBOX: u32 = 0x7;
const BS_AUTORADIOBUTTON: u32 = 0x9;
const BS_OWNERDRAW: u32 = 0xB;
const BS_LEFTTEXT: u32 = 0x20;
const BS_NOTIFY: u32 = 0x4000;

/// Contrôle de fond qui en entoure d'autres : cadre de groupe, onglets,
/// rectangle ou cadre STATIC. Sous Windows ils ne recouvrent pas leurs
/// voisins ; en X11, où chaque contrôle est une fenêtre opaque, ils doivent
/// rester derrière.
/// Hauteur imposée par un contrôle qui se dimensionne seul (ReBar).
pub fn forced_height(hwnd: usize) -> Option<i32> {
    match kind_of(hwnd)? {
        Kind::ReBar => bars::rebar_forced_height(hwnd),
        _ => None,
    }
}

pub fn is_backdrop(hwnd: usize) -> bool {
    match kind_of(hwnd) {
        Some(Kind::Button) => button_type(hwnd) == BS_GROUPBOX,
        Some(Kind::Tab) => true,
        // SS_OWNERDRAW aussi : le logo de « À propos » de Notepad++ occupe
        // 192 × 192 pixels mais n'en dessine que 80 ; devant, il masquerait
        // le numéro de version et le lien posés à côté.
        Some(Kind::Static) => matches!(style(hwnd) & 0x1F, 0x4..=0x9 | 0x0D | 0x10..=0x12),
        _ => false,
    }
}

fn button_type(hwnd: usize) -> u32 {
    style(hwnd) & 0xF
}

fn is_check_like(t: u32) -> bool {
    matches!(t, BS_CHECKBOX | BS_AUTOCHECKBOX | BS_3STATE | BS_AUTO3STATE)
}

fn is_radio(t: u32) -> bool {
    matches!(t, BS_RADIOBUTTON | BS_AUTORADIOBUTTON)
}

fn paint_button(hwnd: usize, c: &Ctl, p: &Painter, r: Rect) {
    let t = button_type(hwnd);
    let on = enabled(hwnd);
    let label = strip_prefix(&title(hwnd));
    if t == BS_OWNERDRAW {
        let mut state = 0;
        if c.pressed {
            state |= 0x1; // ODS_SELECTED
        }
        if !on {
            state |= 0x4; // ODS_DISABLED
        }
        if c.focused {
            state |= 0x10; // ODS_FOCUS
        }
        draw_item(hwnd, p.hdc, 4, 0, state, r, 0); // ODT_BUTTON
        return;
    }
    if t == BS_GROUPBOX {
        let text_color = background(hwnd, p.hdc, WM_CTLCOLORSTATIC, face_color(), p, r);
        let lh = line_height(p.font);
        let frame = Rect {
            top: r.top + lh / 2,
            ..r
        };
        p.shape(frame, face_color(), CONTROL_BORDER, 4, false);
        if !label.is_empty() {
            let w = measure(p.font, &label).0;
            let back = Rect::from_size(r.left + 6, r.top, w + 6, lh);
            p.fill(back, face_color());
            p.text(
                Rect::from_size(r.left + 9, r.top, w + 2, lh),
                &label,
                if on { text_color } else { TEXT_DISABLED },
                0,
            );
        }
        return;
    }
    if is_check_like(t) || is_radio(t) {
        let text_color = background(hwnd, p.hdc, WM_CTLCOLORSTATIC, face_color(), p, r);
        let size = 16.min(r.height().max(10));
        let left_text = style(hwnd) & BS_LEFTTEXT != 0;
        let bx = if left_text {
            r.right - size - 1
        } else {
            r.left + 1
        };
        let by = r.top + (r.height() - size) / 2;
        let bxr = Rect::from_size(bx, by, size, size);
        let filled = c.check != 0;
        let border = if !on {
            TEXT_DISABLED
        } else if filled {
            ACCENT
        } else {
            STRONG_BORDER
        };
        let fill = if filled && on { ACCENT } else { FIELD };
        if is_radio(t) {
            p.shape(bxr, fill, border, 0, true);
            if filled {
                let d = size / 2 - 1;
                let dot = Rect::from_size(bx + (size - d) / 2, by + (size - d) / 2, d, d);
                p.shape(dot, FIELD, FIELD, 0, true);
            }
        } else {
            p.shape(bxr, fill, border, 3, false);
            if c.check == 1 {
                let x = bx + 3;
                let y = by + size / 2;
                p.line(x, y, x + 3, y + 3, FIELD, 2);
                p.line(x + 3, y + 3, bx + size - 4, by + 4, FIELD, 2);
            } else if c.check == 2 {
                let y = by + size / 2;
                p.line(bx + 4, y, bx + size - 4, y, FIELD, 2);
            }
        }
        let tr = if left_text {
            Rect { right: bx - 6, ..r }
        } else {
            Rect {
                left: bx + size + 8,
                ..r
            }
        };
        p.text(tr, &label, if on { text_color } else { TEXT_DISABLED }, 0);
        if c.focused {
            let w = measure(p.font, &label).0;
            let fr = Rect::from_size(tr.left - 2, r.top + 1, w + 4, r.height() - 2);
            p.frame(fr, STRONG_BORDER);
        }
        return;
    }
    // Bouton poussoir.
    background(hwnd, p.hdc, WM_CTLCOLORBTN, face_color(), p, r);
    let default = t == BS_DEFPUSHBUTTON;
    let (fill, border, text) = if !on {
        (CONTROL_PRESSED, CONTROL_BORDER, TEXT_DISABLED)
    } else if default {
        (if c.pressed { 0x00A0_5A00 } else { ACCENT }, ACCENT, FIELD)
    } else if c.pressed {
        (CONTROL_PRESSED, CONTROL_BORDER, 0x005D_5D5D)
    } else {
        (CONTROL_FILL, CONTROL_BORDER, TEXT)
    };
    let body = Rect {
        right: r.right - 1,
        bottom: r.bottom - 1,
        ..r
    };
    p.shape(body, fill, border, 4, false);
    if c.focused && on {
        let f = Rect {
            left: r.left + 2,
            top: r.top + 2,
            right: r.right - 3,
            bottom: r.bottom - 3,
        };
        p.frame(f, if default { FIELD } else { TEXT });
    }
    p.text(r, &label, text, 1);
}

/// Coche les boutons radio d'un groupe : `hwnd` coché, les autres boutons
/// radio automatiques du même groupe (entre deux `WS_GROUP`) décochés.
fn check_radio_group(hwnd: usize) {
    let par = parent(hwnd);
    let siblings = gui::child_windows(par);
    let Some(me) = siblings.iter().position(|&h| h == hwnd) else {
        return;
    };
    let mut start = me;
    while start > 0 && style(siblings[start]) & WS_GROUP == 0 {
        start -= 1;
    }
    let mut end = me + 1;
    while end < siblings.len() && style(siblings[end]) & WS_GROUP == 0 {
        end += 1;
    }
    for &h in &siblings[start..end] {
        if h != hwnd
            && kind_of(h) == Some(Kind::Button)
            && button_type(h) == BS_AUTORADIOBUTTON
            && with_ctl(h, |c| std::mem::replace(&mut c.check, 0)).unwrap_or(0) != 0
        {
            invalidate(h);
        }
    }
}

fn button_click(hwnd: usize) {
    let t = button_type(hwnd);
    match t {
        BS_AUTOCHECKBOX => {
            with_ctl(hwnd, |c| c.check = if c.check == 0 { 1 } else { 0 });
        }
        BS_AUTO3STATE => {
            with_ctl(hwnd, |c| c.check = (c.check + 1) % 3);
        }
        BS_AUTORADIOBUTTON => {
            with_ctl(hwnd, |c| c.check = 1);
            check_radio_group(hwnd);
        }
        _ => {}
    }
    invalidate(hwnd);
    notify(hwnd, 0); // BN_CLICKED
}

unsafe fn button_proc_impl(hwnd: usize, msg: u32, wp: usize, lp: isize) -> Option<isize> {
    let t = button_type(hwnd);
    match msg {
        0x00F0 => Some(with_ctl(hwnd, |c| c.check as isize).unwrap_or(0)), // BM_GETCHECK
        0x00F1 => {
            // BM_SETCHECK
            with_ctl(hwnd, |c| c.check = (wp as u32).min(2));
            if wp == 1 && t == BS_AUTORADIOBUTTON {
                check_radio_group(hwnd);
            }
            invalidate(hwnd);
            Some(0)
        }
        0x00F2 => Some(
            // BM_GETSTATE
            with_ctl(hwnd, |c| {
                c.check as isize | if c.pressed { 0x4 } else { 0 } | if c.focused { 0x8 } else { 0 }
            })
            .unwrap_or(0),
        ),
        0x00F3 => {
            // BM_SETSTATE
            with_ctl(hwnd, |c| c.pressed = wp != 0);
            invalidate(hwnd);
            Some(0)
        }
        0x00F4 => {
            // BM_SETSTYLE
            let s = style(hwnd);
            gui::set_window_style(hwnd, (s & !0xFFFF) | (wp as u32 & 0xFFFF));
            if lp != 0 {
                invalidate(hwnd);
            }
            Some(0)
        }
        0x00F5 => {
            // BM_CLICK
            if enabled(hwnd) {
                button_click(hwnd);
            }
            Some(0)
        }
        0x00F6 | 0x00F7 => Some(0), // BM_GETIMAGE / BM_SETIMAGE
        WM_GETDLGCODE => Some(match t {
            BS_DEFPUSHBUTTON => DLGC_BUTTON | DLGC_DEFPUSHBUTTON,
            BS_GROUPBOX => DLGC_STATIC,
            _ if is_radio(t) => DLGC_BUTTON | DLGC_RADIOBUTTON,
            _ if is_check_like(t) => DLGC_BUTTON,
            _ => DLGC_BUTTON | DLGC_UNDEFPUSHBUTTON,
        }),
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK if t != BS_GROUPBOX => {
            if enabled(hwnd) {
                with_ctl(hwnd, |c| c.pressed = true);
                user32::set_focus(hwnd);
                gui::set_capture(hwnd);
                invalidate(hwnd);
            }
            Some(0)
        }
        WM_LBUTTONUP if t != BS_GROUPBOX => {
            let was = with_ctl(hwnd, |c| std::mem::replace(&mut c.pressed, false)).unwrap_or(false);
            gui::release_capture();
            invalidate(hwnd);
            let (x, y) = point(lp);
            let r = client(hwnd);
            if was && x >= 0 && y >= 0 && x < r.width() && y < r.height() {
                button_click(hwnd);
            }
            Some(0)
        }
        WM_KEYDOWN if wp == VK_SPACE => {
            with_ctl(hwnd, |c| c.pressed = true);
            invalidate(hwnd);
            Some(0)
        }
        WM_KEYUP if wp == VK_SPACE => {
            let was = with_ctl(hwnd, |c| std::mem::replace(&mut c.pressed, false)).unwrap_or(false);
            invalidate(hwnd);
            if was {
                button_click(hwnd);
            }
            Some(0)
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            with_ctl(hwnd, |c| {
                c.focused = msg == WM_SETFOCUS;
                if !c.focused {
                    c.pressed = false;
                }
            });
            invalidate(hwnd);
            if style(hwnd) & BS_NOTIFY != 0 {
                notify(hwnd, if msg == WM_SETFOCUS { 6 } else { 7 }); // BN_SETFOCUS / BN_KILLFOCUS
            }
            Some(0)
        }
        _ => None,
    }
}

// ---------- STATIC ----------

fn paint_static(hwnd: usize, c: &Ctl, p: &Painter, r: Rect) {
    let s = style(hwnd);
    let kind = s & 0x1F;
    if kind == 0x0D {
        // SS_OWNERDRAW : le parent dessine tout, fond compris (WM_DRAWITEM,
        // ODT_STATIC) ; aucun remplissage qui masquerait ses voisins.
        let state = if enabled(hwnd) { 0 } else { 4 }; // ODS_DISABLED
        draw_item(hwnd, p.hdc, 5, 0, state, r, 0);
        return;
    }
    let text_color = background(hwnd, p.hdc, WM_CTLCOLORSTATIC, face_color(), p, r);
    match kind {
        0x04 => p.fill(r, TEXT),           // SS_BLACKRECT
        0x05 => p.fill(r, STRONG_BORDER),  // SS_GRAYRECT
        0x06 => p.fill(r, FIELD),          // SS_WHITERECT
        0x07 => p.frame(r, TEXT),          // SS_BLACKFRAME
        0x08 => p.frame(r, STRONG_BORDER), // SS_GRAYFRAME
        0x09 => p.frame(r, FIELD),         // SS_WHITEFRAME
        0x10 => p.line(r.left, r.top, r.right, r.top, CONTROL_BORDER, 1), // SS_ETCHEDHORZ
        0x11 => p.line(r.left, r.top, r.left, r.bottom, CONTROL_BORDER, 1), // SS_ETCHEDVERT
        0x12 => p.frame(r, CONTROL_BORDER), // SS_ETCHEDFRAME
        0x03 | 0x0E | 0x0F => {}           // icône, bitmap, métafichier
        _ => {
            let raw = title(hwnd);
            let text = if s & 0x80 != 0 {
                raw
            } else {
                strip_prefix(&raw)
            }; // SS_NOPREFIX
            let color = if enabled(hwnd) {
                text_color
            } else {
                TEXT_DISABLED
            };
            let align = match kind {
                0x01 => 1,
                0x02 => 2,
                _ => 0,
            };
            let lh = line_height(p.font);
            // DrawText des statiques : DT_EXPANDTABS.
            let text = expand_tabs(&text);
            let lines = match kind {
                0x0B => vec![text.replace(['\r', '\n'], "")], // SS_SIMPLE : une ligne
                // SS_LEFTNOWORDWRAP : pas de coupure automatique, mais les
                // fins de ligne du texte comptent (boîte « Copier » de 7-Zip).
                0x0C => text
                    .split('\n')
                    .map(|l| l.trim_end_matches('\r').to_string())
                    .collect(),
                _ => wrap(p.font, &text, r.width()),
            };
            let total = lh * lines.len() as i32;
            let mut y = if s & 0x200 != 0 {
                r.top + (r.height() - total) / 2 // SS_CENTERIMAGE
            } else {
                r.top
            };
            for line in lines {
                p.text(
                    Rect::from_size(r.left, y, r.width(), lh),
                    &line,
                    color,
                    align,
                );
                y += lh;
            }
        }
    }
    let _ = c;
}

unsafe fn static_proc_impl(hwnd: usize, msg: u32, _wp: usize, _lp: isize) -> Option<isize> {
    let notify_style = style(hwnd) & 0x100 != 0; // SS_NOTIFY
    match msg {
        WM_GETDLGCODE => Some(DLGC_STATIC),
        WM_LBUTTONDOWN if notify_style => {
            notify(hwnd, 0); // STN_CLICKED
            Some(0)
        }
        WM_LBUTTONDBLCLK if notify_style => {
            notify(hwnd, 1); // STN_DBLCLK
            Some(0)
        }
        0x0170..=0x0173 => Some(0), // STM_SETICON, GETICON, SETIMAGE, GETIMAGE
        _ => None,
    }
}

// ---------- EDIT (et partie éditable d'une COMBOBOX) ----------

const ES_CENTER: u32 = 0x1;
const ES_RIGHT: u32 = 0x2;
const ES_MULTILINE: u32 = 0x4;
const ES_UPPERCASE: u32 = 0x8;
const ES_LOWERCASE: u32 = 0x10;
const ES_PASSWORD: u32 = 0x20;
const ES_READONLY: u32 = 0x800;
const ES_AUTOHSCROLL: u32 = 0x80;
const WS_HSCROLL: u32 = 0x0010_0000;
const ES_NUMBER: u32 = 0x2000;

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

/// Ligne et colonne d'une position (en caractères) dans un texte multiligne.
fn line_col(text: &[char], pos: usize) -> (usize, usize) {
    let mut line = 0;
    let mut start = 0;
    for (i, &ch) in text.iter().enumerate().take(pos.min(text.len())) {
        if ch == '\n' {
            line += 1;
            start = i + 1;
        }
    }
    (line, pos.min(text.len()) - start)
}

/// Débuts de ligne (en caractères).
fn line_starts(text: &[char]) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, &ch) in text.iter().enumerate() {
        if ch == '\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// Lignes affichées d'un texte coupé aux espaces pour tenir dans `width` :
/// (indice du premier caractère, contenu sans `\r\n`).
fn wrapped_rows(font: usize, text: &[char], width: i32) -> Vec<(usize, String)> {
    let starts = line_starts(text);
    let mut rows = Vec::new();
    for l in 0..starts.len() {
        let line: Vec<char> = line_text(text, &starts, l).chars().collect();
        let mut from = 0;
        loop {
            let rest: String = line[from..].iter().collect();
            if width <= 0 || from >= line.len() || measure(font, &rest).0 <= width {
                rows.push((starts[l] + from, rest));
                break;
            }
            // Plus long préfixe qui tient, coupé après un espace si possible.
            let mut fit = from + 1;
            while fit < line.len()
                && measure(font, &line[from..fit + 1].iter().collect::<String>()).0 <= width
            {
                fit += 1;
            }
            let cut = (from + 1..=fit)
                .rev()
                .find(|&i| line[i - 1] == ' ' || line.get(i) == Some(&' '))
                .unwrap_or(fit);
            rows.push((starts[l] + from, line[from..cut].iter().collect()));
            // Les espaces à la coupure restent en fin de ligne.
            from = cut;
            while from < line.len() && line[from] == ' ' {
                from += 1;
            }
        }
    }
    rows
}

/// Contenu d'une ligne sans `\r\n`.
fn line_text(text: &[char], starts: &[usize], line: usize) -> String {
    let start = starts[line];
    let end = starts.get(line + 1).map_or(text.len(), |&n| n - 1);
    text[start..end.max(start)]
        .iter()
        .filter(|&&c| c != '\r')
        .collect()
}

fn displayed(edit: &EditState, s: &str) -> String {
    match edit.password {
        Some(mask) => std::iter::repeat_n(mask, s.chars().count()).collect(),
        None => s.to_string(),
    }
}

/// Position (caractère) la plus proche de `x` pixels dans `line`.
fn hit_char(font: usize, line: &str, x: i32) -> usize {
    let cs: Vec<char> = line.chars().collect();
    let mut prev = 0;
    for i in 1..=cs.len() {
        let w = measure(font, &cs[..i].iter().collect::<String>()).0;
        if x < (prev + w) / 2 {
            return i - 1;
        }
        prev = w;
    }
    cs.len()
}

fn has_border(hwnd: usize) -> bool {
    style(hwnd) & WS_BORDER != 0 || ex_style(hwnd) & WS_EX_CLIENTEDGE != 0
}

/// Zone du texte d'un champ (sans la bordure).
fn edit_area(hwnd: usize, r: Rect) -> Rect {
    let inset = if has_border(hwnd) || kind_of(hwnd) == Some(Kind::ComboBox) {
        5
    } else {
        1
    };
    Rect {
        left: r.left + inset,
        top: r.top + if has_border(hwnd) { 2 } else { 0 },
        right: r.right - inset,
        bottom: r.bottom - if has_border(hwnd) { 2 } else { 0 },
    }
}

fn paint_field_frame(hwnd: usize, p: &Painter, r: Rect, focused: bool) {
    let on = enabled(hwnd);
    let body = Rect {
        right: r.right - 1,
        bottom: r.bottom - 1,
        ..r
    };
    p.shape(
        body,
        if on { FIELD } else { CONTROL_PRESSED },
        CONTROL_BORDER,
        4,
        false,
    );
    if focused && on {
        p.line(
            r.left + 2,
            r.bottom - 2,
            r.right - 3,
            r.bottom - 2,
            ACCENT,
            2,
        );
    } else {
        p.line(
            r.left + 3,
            r.bottom - 2,
            r.right - 4,
            r.bottom - 2,
            STRONG_BORDER,
            1,
        );
    }
}

/// Dessine le texte d'un champ (une ou plusieurs lignes) avec sélection et
/// curseur ; `area` est la zone du texte.
fn paint_edit_text(
    hwnd: usize,
    c: &Ctl,
    p: &Painter,
    area: Rect,
    text: &str,
    multiline: bool,
    color: u32,
) {
    let e = &c.edit;
    let all = chars(text);
    let lh = line_height(p.font);
    let (sel_a, sel_b) = (e.anchor.min(e.caret), e.anchor.max(e.caret));
    // Retour à la ligne automatique (champ multiligne sans ES_AUTOHSCROLL
    // ni barre horizontale), pour l'instant sur les champs en lecture
    // seule : licence de « À propos » de Notepad++.
    let wraps =
        multiline && style(hwnd) & (ES_AUTOHSCROLL | ES_READONLY | WS_HSCROLL) == ES_READONLY;
    let rows: Vec<(usize, String)> = if wraps {
        wrapped_rows(p.font, &all, area.width())
    } else if multiline {
        let starts = line_starts(&all);
        (0..starts.len())
            .map(|l| (starts[l], line_text(&all, &starts, l)))
            .collect()
    } else {
        vec![(0, all.iter().collect())]
    };
    let first = if multiline {
        e.top_line.min(rows.len() - 1)
    } else {
        0
    };
    // Pour une COMBOBOX, ces bits sont CBS_SIMPLE / CBS_DROPDOWN : le champ
    // d'une liste modifiable reste aligné à gauche.
    let align = if kind_of(hwnd) == Some(Kind::Edit) {
        style(hwnd) & (ES_CENTER | ES_RIGHT)
    } else {
        0
    };
    for (line, (start, content)) in rows.iter().enumerate().skip(first) {
        let start = *start;
        let y = if multiline {
            area.top + (line - first) as i32 * lh
        } else {
            area.top + (area.height() - lh) / 2
        };
        if y > area.bottom {
            break;
        }
        let shown = displayed(e, content);
        let width = measure(p.font, &shown).0;
        let x0 = if !multiline && align == ES_CENTER {
            area.left + (area.width() - width) / 2
        } else if !multiline && align == ES_RIGHT {
            area.right - width
        } else {
            area.left - if multiline { 0 } else { e.scroll_x }
        };
        let n = shown.chars().count();
        let prefix =
            |k: usize| -> i32 { measure(p.font, &shown.chars().take(k).collect::<String>()).0 };
        // Sélection sur cette ligne.
        let (la, lb) = (sel_a.max(start), sel_b.min(start + n));
        if c.focused && sel_a != sel_b && la < lb {
            let xa = x0 + prefix(la - start);
            let xb = x0 + prefix(lb - start);
            let sr = Rect {
                left: xa.max(area.left),
                top: y,
                right: xb.min(area.right),
                bottom: y + lh,
            };
            p.fill(sr, ACCENT);
            p.clipped(x0, y, lh, &shown, color, area.left, area.right);
            let sel_text: String = shown.chars().skip(la - start).take(lb - la).collect();
            p.clipped(xa, y, lh, &sel_text, FIELD, area.left, area.right);
        } else {
            p.clipped(x0, y, lh, &shown, color, area.left, area.right);
        }
        // Curseur.
        if c.focused && e.caret >= start && e.caret <= start + n {
            let on_row = if wraps {
                rows.get(line + 1).is_none_or(|(next, _)| e.caret < *next)
            } else {
                line_col(&all, e.caret).0 == line
            };
            if !multiline || on_row {
                let x = x0 + prefix(e.caret - start);
                if x >= area.left - 1 && x <= area.right {
                    p.line(x, y + 1, x, y + lh - 1, TEXT, 1);
                }
            }
        }
    }
}

fn paint_edit(hwnd: usize, c: &Ctl, p: &Painter, r: Rect) {
    let on = enabled(hwnd);
    let readonly = style(hwnd) & ES_READONLY != 0;
    let msg = if readonly || !on {
        WM_CTLCOLORSTATIC
    } else {
        WM_CTLCOLOREDIT
    };
    let default = if readonly || !on { face_color() } else { FIELD };
    let text_color = background(hwnd, p.hdc, msg, default, p, r);
    if has_border(hwnd) {
        paint_field_frame(hwnd, p, r, c.focused && !readonly);
    }
    let area = edit_area(hwnd, r);
    let color = if on { text_color } else { TEXT_DISABLED };
    let multiline = style(hwnd) & ES_MULTILINE != 0;
    paint_edit_text(hwnd, c, p, area, &title(hwnd), multiline, color);
}

/// Garde le curseur visible : défilement horizontal (une ligne) ou vertical.
fn keep_caret_visible(hwnd: usize) {
    let Some(c) = snapshot(hwnd) else {
        return;
    };
    let font = font_of(&c);
    let text = title(hwnd);
    let all = chars(&text);
    let r = client(hwnd);
    let area = edit_area(hwnd, combo_text_rect(hwnd, &c, r));
    let multiline = c.kind == Kind::Edit && style(hwnd) & ES_MULTILINE != 0;
    if multiline {
        let lh = line_height(font);
        let visible = (area.height() / lh).max(1) as usize;
        let (line, _) = line_col(&all, c.edit.caret);
        with_ctl(hwnd, |c| {
            if line < c.edit.top_line {
                c.edit.top_line = line;
            } else if line >= c.edit.top_line + visible {
                c.edit.top_line = line + 1 - visible;
            }
        });
    } else {
        let shown = displayed(&c.edit, &text);
        let x = measure(font, &shown.chars().take(c.edit.caret).collect::<String>()).0;
        let w = area.width().max(1);
        with_ctl(hwnd, |c| {
            if x - c.edit.scroll_x > w - 2 {
                c.edit.scroll_x = x - w + 2;
            } else if x < c.edit.scroll_x {
                c.edit.scroll_x = (x - w / 3).max(0);
            }
            if measure(font, &shown).0 <= w {
                c.edit.scroll_x = 0;
            }
        });
    }
}

fn edit_readonly(hwnd: usize) -> bool {
    kind_of(hwnd) == Some(Kind::Edit) && style(hwnd) & ES_READONLY != 0
}

/// Remplace la sélection par `insert` (filtrée selon le style) ; renvoie vrai
/// si le texte a changé.
fn edit_replace(hwnd: usize, insert: &str, from_user: bool) -> bool {
    let s = style(hwnd);
    let is_edit = kind_of(hwnd) == Some(Kind::Edit);
    let multiline = is_edit && s & ES_MULTILINE != 0;
    let mut ins: String = insert
        .chars()
        .filter(|&ch| multiline || (ch != '\r' && ch != '\n'))
        .collect();
    if is_edit && s & ES_UPPERCASE != 0 {
        ins = ins.to_uppercase();
    }
    if is_edit && s & ES_LOWERCASE != 0 {
        ins = ins.to_lowercase();
    }
    if from_user && is_edit && s & ES_NUMBER != 0 && !ins.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let text = title(hwnd);
    let mut all = chars(&text);
    let Some((a, b, limit)) = with_ctl(hwnd, |c| {
        let a = c.edit.anchor.min(c.edit.caret).min(all.len());
        let b = c.edit.anchor.max(c.edit.caret).min(all.len());
        (a, b, c.edit.limit)
    }) else {
        return false;
    };
    let room = limit.saturating_sub(all.len() - (b - a));
    let ins: Vec<char> = ins.chars().take(room).collect();
    if a == b && ins.is_empty() {
        return false;
    }
    all.splice(a..b, ins.iter().copied());
    let new: String = all.iter().collect();
    set_title(hwnd, &new);
    with_ctl(hwnd, |c| {
        c.edit.caret = a + ins.len();
        c.edit.anchor = c.edit.caret;
        c.edit.modified = true;
    });
    keep_caret_visible(hwnd);
    invalidate(hwnd);
    if kind_of(hwnd) == Some(Kind::ComboBox) {
        notify(hwnd, 6); // CBN_EDITUPDATE
        notify(hwnd, 5); // CBN_EDITCHANGE
    } else {
        notify(hwnd, 0x0400); // EN_UPDATE
        notify(hwnd, 0x0300); // EN_CHANGE
    }
    true
}

fn selected_text(hwnd: usize) -> String {
    let all = chars(&title(hwnd));
    with_ctl(hwnd, |c| {
        let a = c.edit.anchor.min(c.edit.caret).min(all.len());
        let b = c.edit.anchor.max(c.edit.caret).min(all.len());
        all[a..b].iter().collect()
    })
    .unwrap_or_default()
}

unsafe fn clipboard_set(hwnd: usize, s: &str) {
    let mut wide: Vec<u16> = s.encode_utf16().collect();
    wide.push(0);
    let Some(alloc) = super::lookup("kernel32", "GlobalAlloc") else {
        return;
    };
    let alloc: unsafe extern "win64" fn(u32, usize) -> usize = std::mem::transmute(alloc);
    let (Some(lockf), Some(unlock)) = (
        super::lookup("kernel32", "GlobalLock"),
        super::lookup("kernel32", "GlobalUnlock"),
    ) else {
        return;
    };
    let lockf: unsafe extern "win64" fn(usize) -> *mut u16 = std::mem::transmute(lockf);
    let unlock: unsafe extern "win64" fn(usize) -> i32 = std::mem::transmute(unlock);
    let (Some(open), Some(empty), Some(set), Some(close)) = (
        super::lookup("user32", "OpenClipboard"),
        super::lookup("user32", "EmptyClipboard"),
        super::lookup("user32", "SetClipboardData"),
        super::lookup("user32", "CloseClipboard"),
    ) else {
        return;
    };
    let open: unsafe extern "win64" fn(usize) -> i32 = std::mem::transmute(open);
    let empty: unsafe extern "win64" fn() -> i32 = std::mem::transmute(empty);
    let set: unsafe extern "win64" fn(u32, usize) -> usize = std::mem::transmute(set);
    let close: unsafe extern "win64" fn() -> i32 = std::mem::transmute(close);
    let mem = alloc(0x0002, wide.len() * 2); // GMEM_MOVEABLE
    if mem == 0 {
        return;
    }
    let p = lockf(mem);
    if !p.is_null() {
        std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
        unlock(mem);
    }
    if open(hwnd) != 0 {
        empty();
        set(13, mem); // CF_UNICODETEXT
        close();
    }
}

unsafe fn clipboard_get(hwnd: usize) -> Option<String> {
    let open: unsafe extern "win64" fn(usize) -> i32 =
        std::mem::transmute(super::lookup("user32", "OpenClipboard")?);
    let get: unsafe extern "win64" fn(u32) -> usize =
        std::mem::transmute(super::lookup("user32", "GetClipboardData")?);
    let close: unsafe extern "win64" fn() -> i32 =
        std::mem::transmute(super::lookup("user32", "CloseClipboard")?);
    let lockf: unsafe extern "win64" fn(usize) -> *const u16 =
        std::mem::transmute(super::lookup("kernel32", "GlobalLock")?);
    let unlock: unsafe extern "win64" fn(usize) -> i32 =
        std::mem::transmute(super::lookup("kernel32", "GlobalUnlock")?);
    if open(hwnd) == 0 {
        return None;
    }
    let mem = get(13);
    let mut out = None;
    if mem != 0 {
        let p = lockf(mem);
        if !p.is_null() {
            out = Some(super::wstr(p));
        }
        unlock(mem);
    }
    close();
    out
}

/// Touches de déplacement et d'effacement d'un champ ; vrai si traitée.
fn edit_key(hwnd: usize, vk: usize) -> bool {
    let text = title(hwnd);
    let all = chars(&text);
    let multiline = kind_of(hwnd) == Some(Kind::Edit) && style(hwnd) & ES_MULTILINE != 0;
    let shift = shift_down();
    let ctrl = ctrl_down();
    let Some(c) = snapshot(hwnd) else {
        return false;
    };
    let caret = c.edit.caret.min(all.len());
    let word_left = |mut i: usize| {
        while i > 0 && all[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !all[i - 1].is_whitespace() {
            i -= 1;
        }
        i
    };
    let word_right = |mut i: usize| {
        while i < all.len() && !all[i].is_whitespace() {
            i += 1;
        }
        while i < all.len() && all[i].is_whitespace() {
            i += 1;
        }
        i
    };
    let starts = line_starts(&all);
    let (line, col) = line_col(&all, caret);
    let line_end = |l: usize| {
        starts.get(l + 1).map_or(all.len(), |&n| {
            if n >= 2 && all[n - 2] == '\r' {
                n - 2
            } else {
                n - 1
            }
        })
    };
    let new_caret = match vk {
        VK_LEFT if ctrl => Some(word_left(caret)),
        VK_LEFT => Some(if !shift && c.edit.anchor != caret {
            c.edit.anchor.min(caret)
        } else {
            caret.saturating_sub(1)
        }),
        VK_RIGHT if ctrl => Some(word_right(caret)),
        VK_RIGHT => Some(if !shift && c.edit.anchor != caret {
            c.edit.anchor.max(caret)
        } else {
            (caret + 1).min(all.len())
        }),
        VK_HOME if ctrl || !multiline => Some(0),
        VK_HOME => Some(starts[line]),
        VK_END if ctrl || !multiline => Some(all.len()),
        VK_END => Some(line_end(line)),
        VK_UP if multiline && line > 0 => Some((starts[line - 1] + col).min(line_end(line - 1))),
        VK_DOWN if multiline && line + 1 < starts.len() => {
            Some((starts[line + 1] + col).min(line_end(line + 1)))
        }
        VK_DELETE => {
            if !edit_readonly(hwnd) {
                if c.edit.anchor == caret && caret < all.len() {
                    let n = if all[caret] == '\r' && all.get(caret + 1) == Some(&'\n') {
                        2
                    } else {
                        1
                    };
                    with_ctl(hwnd, |c| c.edit.anchor = caret + n);
                }
                if shift {
                    unsafe { clipboard_set(hwnd, &selected_text(hwnd)) };
                }
                edit_replace(hwnd, "", true);
            }
            return true;
        }
        _ => None,
    };
    let Some(mut pos) = new_caret else {
        return false;
    };
    // Ne jamais s'arrêter entre \r et \n.
    if pos > 0 && pos < all.len() && all[pos - 1] == '\r' && all[pos] == '\n' {
        pos = if pos < caret { pos - 1 } else { pos + 1 };
    }
    with_ctl(hwnd, |c| {
        c.edit.caret = pos;
        if !shift {
            c.edit.anchor = pos;
        }
    });
    keep_caret_visible(hwnd);
    invalidate(hwnd);
    true
}

/// Caractère tapé (WM_CHAR) dans un champ ; vrai si traité.
fn edit_char(hwnd: usize, ch: u32) -> bool {
    let readonly = edit_readonly(hwnd);
    let multiline = kind_of(hwnd) == Some(Kind::Edit) && style(hwnd) & ES_MULTILINE != 0;
    match ch {
        0x01 => {
            // Ctrl+A
            let n = title(hwnd).chars().count();
            with_ctl(hwnd, |c| {
                c.edit.anchor = 0;
                c.edit.caret = n;
            });
            invalidate(hwnd);
        }
        0x03 => unsafe { clipboard_set(hwnd, &selected_text(hwnd)) },
        0x16 if !readonly => {
            if let Some(s) = unsafe { clipboard_get(hwnd) } {
                edit_replace(hwnd, &s, true);
            }
        }
        0x18 if !readonly => {
            unsafe { clipboard_set(hwnd, &selected_text(hwnd)) };
            edit_replace(hwnd, "", true);
        }
        0x08 if !readonly => {
            let all = chars(&title(hwnd));
            let collapsed = with_ctl(hwnd, |c| c.edit.anchor == c.edit.caret).unwrap_or(true);
            if collapsed {
                with_ctl(hwnd, |c| {
                    let caret = c.edit.caret.min(all.len());
                    let n = if caret >= 2 && all[caret - 1] == '\n' && all[caret - 2] == '\r' {
                        2
                    } else {
                        1
                    };
                    c.edit.anchor = caret.saturating_sub(n);
                });
            }
            edit_replace(hwnd, "", true);
        }
        0x0D if multiline && !readonly => {
            edit_replace(hwnd, "\r\n", true);
        }
        0x09 if multiline && !readonly => {
            edit_replace(hwnd, "\t", true);
        }
        c if c >= 0x20 && c != 0x7F && !readonly => {
            if let Some(ch) = char::from_u32(c) {
                edit_replace(hwnd, &ch.to_string(), true);
            }
        }
        _ => return false,
    }
    true
}

/// Position du clic dans un champ (zone `area`).
fn edit_hit(hwnd: usize, c: &Ctl, area: Rect, x: i32, y: i32) -> usize {
    let font = font_of(c);
    let text = title(hwnd);
    let all = chars(&text);
    let multiline = c.kind == Kind::Edit && style(hwnd) & ES_MULTILINE != 0;
    if multiline {
        let starts = line_starts(&all);
        let lh = line_height(font);
        let line = (c.edit.top_line + ((y - area.top).max(0) / lh) as usize).min(starts.len() - 1);
        let content = line_text(&all, &starts, line);
        starts[line] + hit_char(font, &displayed(&c.edit, &content), x - area.left)
    } else {
        let shown = displayed(&c.edit, &text);
        let width = measure(font, &shown).0;
        let align = style(hwnd) & (ES_CENTER | ES_RIGHT);
        let x0 = if c.kind == Kind::Edit && align == ES_CENTER {
            area.left + (area.width() - width) / 2
        } else if c.kind == Kind::Edit && align == ES_RIGHT {
            area.right - width
        } else {
            area.left - c.edit.scroll_x
        };
        hit_char(font, &shown, x - x0)
    }
}

unsafe fn edit_common(hwnd: usize, msg: u32, wp: usize, lp: isize, area: Rect) -> Option<isize> {
    match msg {
        0x00B0 => {
            // EM_GETSEL
            let (a, b) = with_ctl(hwnd, |c| {
                (
                    c.edit.anchor.min(c.edit.caret),
                    c.edit.anchor.max(c.edit.caret),
                )
            })?;
            if wp != 0 {
                *(wp as *mut u32) = a as u32;
            }
            if lp != 0 {
                *(lp as *mut u32) = b as u32;
            }
            Some(((b.min(0xFFFF) << 16) | a.min(0xFFFF)) as isize)
        }
        0x00B1 => {
            // EM_SETSEL
            let n = title(hwnd).chars().count();
            let start = wp as isize as i32;
            let end = lp as i32;
            with_ctl(hwnd, |c| {
                if start == -1 {
                    c.edit.anchor = c.edit.caret;
                } else {
                    let e = if end == -1 {
                        n
                    } else {
                        (end.max(0) as usize).min(n)
                    };
                    c.edit.anchor = (start.max(0) as usize).min(n);
                    c.edit.caret = e;
                }
            });
            keep_caret_visible(hwnd);
            invalidate(hwnd);
            Some(1)
        }
        0x00C2 => {
            // EM_REPLACESEL
            let s = read_str(hwnd, lp);
            edit_replace(hwnd, &s, false);
            Some(0)
        }
        0x00C5 => {
            // EM_LIMITTEXT / EM_SETLIMITTEXT
            with_ctl(hwnd, |c| {
                c.edit.limit = if wp == 0 { 0x7FFF_FFFE } else { wp };
            });
            Some(0)
        }
        0x00D5 => Some(with_ctl(hwnd, |c| c.edit.limit as isize).unwrap_or(0)), // EM_GETLIMITTEXT
        0x00B8 => Some(with_ctl(hwnd, |c| c.edit.modified as isize).unwrap_or(0)), // EM_GETMODIFY
        0x00B9 => {
            with_ctl(hwnd, |c| c.edit.modified = wp != 0);
            Some(0)
        }
        0x00B7 => {
            keep_caret_visible(hwnd);
            Some(1)
        } // EM_SCROLLCARET
        0x00C6 | 0x00CD | 0x00D3 | 0x00D4 => Some(0), // CANUNDO, EMPTYUNDOBUFFER, SET/GETMARGINS
        WM_CUT => {
            clipboard_set(hwnd, &selected_text(hwnd));
            if !edit_readonly(hwnd) {
                edit_replace(hwnd, "", false);
            }
            Some(0)
        }
        WM_COPY => {
            clipboard_set(hwnd, &selected_text(hwnd));
            Some(0)
        }
        WM_PASTE => {
            if !edit_readonly(hwnd) {
                if let Some(s) = clipboard_get(hwnd) {
                    edit_replace(hwnd, &s, false);
                }
            }
            Some(0)
        }
        WM_CLEAR => {
            if !edit_readonly(hwnd) {
                edit_replace(hwnd, "", false);
            }
            Some(0)
        }
        WM_UNDO => Some(0),
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            if !enabled(hwnd) {
                return Some(0);
            }
            user32::set_focus(hwnd);
            let c = snapshot(hwnd)?;
            let (x, y) = point(lp);
            let pos = edit_hit(hwnd, &c, area, x, y);
            let shift = shift_down();
            with_ctl(hwnd, |c| {
                c.edit.caret = pos;
                if !shift {
                    c.edit.anchor = pos;
                }
                c.edit.dragging = true;
            });
            gui::set_capture(hwnd);
            keep_caret_visible(hwnd);
            invalidate(hwnd);
            Some(0)
        }
        WM_MOUSEMOVE => {
            let c = snapshot(hwnd)?;
            if c.edit.dragging && wp & 1 != 0 {
                let (x, y) = point(lp);
                let pos = edit_hit(hwnd, &c, area, x, y);
                if pos != c.edit.caret {
                    with_ctl(hwnd, |c| c.edit.caret = pos);
                    keep_caret_visible(hwnd);
                    invalidate(hwnd);
                }
            }
            Some(0)
        }
        WM_LBUTTONUP => {
            with_ctl(hwnd, |c| c.edit.dragging = false);
            gui::release_capture();
            Some(0)
        }
        _ => None,
    }
}

unsafe fn edit_proc_impl(hwnd: usize, msg: u32, wp: usize, lp: isize) -> Option<isize> {
    let s = style(hwnd);
    let multiline = s & ES_MULTILINE != 0;
    let area = edit_area(hwnd, client(hwnd));
    if let Some(r) = edit_common(hwnd, msg, wp, lp, area) {
        return Some(r);
    }
    match msg {
        WM_GETDLGCODE => {
            let mut code = DLGC_WANTCHARS | DLGC_HASSETSEL | DLGC_WANTARROWS;
            if multiline {
                code |= DLGC_WANTALLKEYS;
            }
            Some(code)
        }
        WM_KEYDOWN => {
            if enabled(hwnd) {
                edit_key(hwnd, wp);
            }
            Some(0)
        }
        WM_CHAR => {
            if enabled(hwnd) {
                edit_char(hwnd, wp as u32);
            }
            Some(0)
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            with_ctl(hwnd, |c| c.focused = msg == WM_SETFOCUS);
            invalidate(hwnd);
            notify(hwnd, if msg == WM_SETFOCUS { 0x0100 } else { 0x0200 }); // EN_SETFOCUS / EN_KILLFOCUS
            Some(0)
        }
        WM_MOUSEWHEEL if multiline => {
            let delta = (wp >> 16) as u16 as i16;
            let lines = title(hwnd).matches('\n').count();
            with_ctl(hwnd, |c| {
                if delta > 0 {
                    c.edit.top_line = c.edit.top_line.saturating_sub(3);
                } else {
                    c.edit.top_line = (c.edit.top_line + 3).min(lines);
                }
            });
            invalidate(hwnd);
            Some(0)
        }
        0x00CC => {
            // EM_SETPASSWORDCHAR
            with_ctl(hwnd, |c| {
                c.edit.password = if wp == 0 {
                    None
                } else {
                    char::from_u32(wp as u32)
                }
            });
            invalidate(hwnd);
            Some(0)
        }
        0x00D2 => Some(with_ctl(hwnd, |c| c.edit.password.map_or(0, |p| p as isize)).unwrap_or(0)), // EM_GETPASSWORDCHAR
        0x00CF => {
            // EM_SETREADONLY
            let st = style(hwnd);
            gui::set_window_style(
                hwnd,
                if wp != 0 {
                    st | ES_READONLY
                } else {
                    st & !ES_READONLY
                },
            );
            invalidate(hwnd);
            Some(1)
        }
        0x00BA => Some(title(hwnd).matches('\n').count() as isize + 1), // EM_GETLINECOUNT
        0x00BB => {
            // EM_LINEINDEX
            let all = chars(&title(hwnd));
            let starts = line_starts(&all);
            let line = if wp as isize == -1 {
                line_col(&all, with_ctl(hwnd, |c| c.edit.caret).unwrap_or(0)).0
            } else {
                wp
            };
            Some(starts.get(line).map_or(-1, |&s| s as isize))
        }
        0x00C1 => {
            // EM_LINELENGTH (wp = position d'un caractère de la ligne)
            let all = chars(&title(hwnd));
            let starts = line_starts(&all);
            let pos = if wp as isize == -1 {
                with_ctl(hwnd, |c| c.edit.caret).unwrap_or(0)
            } else {
                wp
            };
            let (line, _) = line_col(&all, pos);
            Some(line_text(&all, &starts, line).chars().count() as isize)
        }
        0x00C4 => {
            // EM_GETLINE : le premier WORD du tampon donne sa taille.
            if lp == 0 {
                return Some(0);
            }
            let all = chars(&title(hwnd));
            let starts = line_starts(&all);
            if wp >= starts.len() {
                return Some(0);
            }
            let line = line_text(&all, &starts, wp);
            let wide = wide_for(hwnd);
            let cap = *(lp as *const u16) as usize;
            if wide {
                let units: Vec<u16> = line.encode_utf16().take(cap).collect();
                std::ptr::copy_nonoverlapping(units.as_ptr(), lp as *mut u16, units.len());
                Some(units.len() as isize)
            } else {
                let bytes: Vec<u8> = super::encode_ansi(&line).into_iter().take(cap).collect();
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), lp as *mut u8, bytes.len());
                Some(bytes.len() as isize)
            }
        }
        0x00C9 => {
            // EM_LINEFROMCHAR
            let all = chars(&title(hwnd));
            let pos = if wp as isize == -1 {
                with_ctl(hwnd, |c| c.edit.caret).unwrap_or(0)
            } else {
                wp
            };
            Some(line_col(&all, pos).0 as isize)
        }
        0x00CE => Some(with_ctl(hwnd, |c| c.edit.top_line as isize).unwrap_or(0)), // EM_GETFIRSTVISIBLELINE
        0x00B6 => {
            // EM_LINESCROLL
            let lines = title(hwnd).matches('\n').count();
            with_ctl(hwnd, |c| {
                c.edit.top_line = (c.edit.top_line as isize + lp).clamp(0, lines as isize) as usize;
            });
            invalidate(hwnd);
            Some(1)
        }
        _ => None,
    }
}

// ---------- LISTBOX ----------

const LBS_NOTIFY: u32 = 0x1;
const LBS_SORT: u32 = 0x2;
const LBS_MULTIPLESEL: u32 = 0x8;
const LBS_OWNERDRAWFIXED: u32 = 0x10;
const LBS_OWNERDRAWVARIABLE: u32 = 0x20;
const LBS_HASSTRINGS: u32 = 0x40;
const LBS_EXTENDEDSEL: u32 = 0x800;

fn item_height(c: &Ctl) -> i32 {
    if c.item_h > 0 {
        c.item_h
    } else {
        line_height(font_of(c)) + 4
    }
}

fn multi_select(hwnd: usize) -> bool {
    style(hwnd) & (LBS_MULTIPLESEL | LBS_EXTENDEDSEL) != 0
}

fn owner_draw_list(hwnd: usize) -> bool {
    let s = style(hwnd);
    let bits = if kind_of(hwnd) == Some(Kind::ComboBox) {
        0x30 // CBS_OWNERDRAWFIXED | CBS_OWNERDRAWVARIABLE
    } else {
        LBS_OWNERDRAWFIXED | LBS_OWNERDRAWVARIABLE
    };
    s & bits != 0
}

fn has_strings(hwnd: usize) -> bool {
    let s = style(hwnd);
    let has = if kind_of(hwnd) == Some(Kind::ComboBox) {
        s & 0x200 != 0 // CBS_HASSTRINGS
    } else {
        s & LBS_HASSTRINGS != 0
    };
    !owner_draw_list(hwnd) || has
}

/// Dessine une liste d'éléments dans `area` (LISTBOX ou liste déroulée).
fn paint_items(hwnd: usize, c: &Ctl, p: &Painter, area: Rect, odt: u32) {
    let ih = item_height(c);
    let visible = (area.height() / ih).max(1);
    let multi = c.kind == Kind::ListBox && multi_select(hwnd);
    let owner = owner_draw_list(hwnd);
    let scroll = c.items.len() as i32 > visible;
    let right = if scroll { area.right - 8 } else { area.right };
    for i in c.top..(c.top + visible + 1).min(c.items.len() as i32) {
        let y = area.top + (i - c.top) * ih;
        let row = Rect {
            left: area.left,
            top: y,
            right,
            bottom: (y + ih).min(area.bottom),
        };
        if row.height() <= 0 {
            break;
        }
        let (text, data) = &c.items[i as usize];
        let is_sel = if multi {
            c.selected.get(i as usize).copied().unwrap_or(false)
        } else {
            i == c.sel
        };
        if owner {
            let mut state = 0;
            if is_sel {
                state |= 0x1;
            }
            if c.focused && i == c.sel {
                state |= 0x10;
            }
            draw_item(hwnd, p.hdc, odt, i, state, row, *data);
            continue;
        }
        let (bg, fg) = if is_sel && c.focused {
            (ACCENT, FIELD)
        } else if is_sel {
            (SELECTION_SOFT, TEXT)
        } else {
            (FIELD, TEXT)
        };
        if bg != FIELD {
            p.fill(row, bg);
        }
        let tr = Rect {
            left: row.left + 6,
            right: row.right - 2,
            ..row
        };
        let color = if enabled(hwnd) { fg } else { TEXT_DISABLED };
        if text.contains('\t') {
            // Tabulations (LBS_USETABSTOPS, journal d'événements de PuTTY) :
            // colonnes tous les 8 caractères moyens, comme les arrêts par
            // défaut de Windows, au lieu d'un carré.
            let tab = (measure(p.font, "nnnnnnnn").0).max(8);
            let mut x = tr.left;
            for (k, part) in text.split('\t').enumerate() {
                if k > 0 {
                    x = tr.left + ((x - tr.left) / tab + 1) * tab;
                }
                if x >= tr.right {
                    break;
                }
                p.text(Rect { left: x, ..tr }, part, color, 0);
                x += measure(p.font, part).0;
            }
        } else {
            p.text(tr, text, color, 0);
        }
    }
    if scroll {
        // Barre de défilement fine, comme celles de Windows 11 au repos.
        let n = c.items.len() as i32;
        let track = Rect {
            left: area.right - 6,
            right: area.right - 2,
            top: area.top + 2,
            bottom: area.bottom - 2,
        };
        let th = (track.height() * visible / n).max(12);
        let ty = track.top + (track.height() - th) * c.top / (n - visible).max(1);
        p.shape(
            Rect::from_size(track.left, ty, 4, th),
            STRONG_BORDER,
            STRONG_BORDER,
            2,
            false,
        );
    }
}

fn paint_listbox(hwnd: usize, c: &Ctl, p: &Painter, r: Rect) {
    background(hwnd, p.hdc, WM_CTLCOLORLISTBOX, FIELD, p, r);
    let area = if has_border(hwnd) {
        p.frame(r, CONTROL_BORDER);
        Rect {
            left: r.left + 1,
            top: r.top + 1,
            right: r.right - 1,
            bottom: r.bottom - 1,
        }
    } else {
        r
    };
    paint_items(hwnd, c, p, area, 2); // ODT_LISTBOX
}

fn list_insert(hwnd: usize, index: isize, text: String, data: isize, sorted: bool) -> isize {
    with_ctl(hwnd, |c| {
        let at = if sorted {
            let key = text.to_lowercase();
            c.items
                .iter()
                .position(|(t, _)| t.to_lowercase() > key)
                .unwrap_or(c.items.len())
        } else if index < 0 || index as usize > c.items.len() {
            c.items.len()
        } else {
            index as usize
        };
        c.items.insert(at, (text, data));
        c.selected.insert(at, false);
        if c.sel >= at as i32 {
            c.sel += 1;
        }
        at as isize
    })
    .unwrap_or(LB_ERR)
}

fn list_find(c: &Ctl, start: isize, needle: &str, exact: bool) -> isize {
    let n = c.items.len();
    if n == 0 {
        return LB_ERR;
    }
    let needle = needle.to_lowercase();
    let first = if start < 0 {
        0
    } else {
        (start as usize + 1) % n
    };
    for k in 0..n {
        let i = (first + k) % n;
        let t = c.items[i].0.to_lowercase();
        if (exact && t == needle) || (!exact && t.starts_with(&needle)) {
            return i as isize;
        }
    }
    LB_ERR
}

fn ensure_visible(hwnd: usize, rows: i32) {
    with_ctl(hwnd, |c| {
        let n = c.items.len() as i32;
        if c.sel < 0 {
            return;
        }
        if c.sel < c.top {
            c.top = c.sel;
        } else if c.sel >= c.top + rows {
            c.top = c.sel - rows + 1;
        }
        c.top = c.top.clamp(0, (n - rows).max(0));
    });
}

/// Messages communs à une LISTBOX et à la liste d'une COMBOBOX ; `base`
/// est le premier message de la famille (LB_* = 0x180, CB_* traduits).
unsafe fn list_message(hwnd: usize, msg: u32, wp: usize, lp: isize) -> Option<isize> {
    let item_text = |lp: isize| -> (String, isize) {
        if has_strings(hwnd) {
            (read_str(hwnd, lp), 0)
        } else {
            (String::new(), lp)
        }
    };
    let sorted_style = if kind_of(hwnd) == Some(Kind::ComboBox) {
        style(hwnd) & 0x100 != 0 // CBS_SORT
    } else {
        style(hwnd) & LBS_SORT != 0
    };
    let r = match msg {
        0x180 => {
            // ADDSTRING
            let (t, d) = item_text(lp);
            let i = list_insert(hwnd, -1, t, d, sorted_style && has_strings(hwnd));
            invalidate(hwnd);
            i
        }
        0x181 => {
            // INSERTSTRING
            let (t, d) = item_text(lp);
            let i = list_insert(hwnd, wp as isize, t, d, false);
            invalidate(hwnd);
            i
        }
        0x182 => {
            // DELETESTRING
            let left = with_ctl(hwnd, |c| {
                if wp >= c.items.len() {
                    return LB_ERR;
                }
                c.items.remove(wp);
                c.selected.remove(wp);
                if c.sel == wp as i32 {
                    c.sel = -1;
                } else if c.sel > wp as i32 {
                    c.sel -= 1;
                }
                c.items.len() as isize
            })?;
            invalidate(hwnd);
            left
        }
        0x184 => {
            // RESETCONTENT
            with_ctl(hwnd, |c| {
                c.items.clear();
                c.selected.clear();
                c.sel = -1;
                c.top = 0;
            });
            invalidate(hwnd);
            0
        }
        0x185 => {
            // LB_SETSEL (multi) : wp = état, lp = index (-1 = tous)
            with_ctl(hwnd, |c| {
                if lp == -1 {
                    for s in c.selected.iter_mut() {
                        *s = wp != 0;
                    }
                } else if let Some(s) = c.selected.get_mut(lp as usize) {
                    *s = wp != 0;
                    c.sel = lp as i32;
                }
            });
            invalidate(hwnd);
            0
        }
        0x186 => {
            // SETCURSEL
            let ok = with_ctl(hwnd, |c| {
                let i = wp as isize;
                if i < 0 || i as usize >= c.items.len() {
                    c.sel = -1;
                    i == -1
                } else {
                    c.sel = i as i32;
                    true
                }
            })?;
            let rows = list_rows(hwnd);
            ensure_visible(hwnd, rows);
            sync_combo_text(hwnd);
            invalidate(hwnd);
            if ok && wp as isize >= 0 {
                wp as isize
            } else {
                LB_ERR
            }
        }
        0x187 => {
            // LB_GETSEL
            with_ctl(hwnd, |c| {
                if wp >= c.items.len() {
                    LB_ERR
                } else if multi_select(hwnd) {
                    c.selected[wp] as isize
                } else {
                    (c.sel == wp as i32) as isize
                }
            })?
        }
        0x188 => with_ctl(hwnd, |c| if c.sel < 0 { LB_ERR } else { c.sel as isize })?, // GETCURSEL
        0x189 => {
            // GETTEXT
            let c = snapshot(hwnd)?;
            match c.items.get(wp) {
                Some((t, d)) if has_strings(hwnd) => write_str(hwnd, t, lp, u32::MAX / 4),
                Some((_, d)) => {
                    if lp != 0 {
                        *(lp as *mut isize) = *d;
                    }
                    std::mem::size_of::<isize>() as isize
                }
                None => LB_ERR,
            }
        }
        0x18A => {
            // GETTEXTLEN
            let c = snapshot(hwnd)?;
            match c.items.get(wp) {
                Some((t, _)) => text_len(hwnd, t),
                None => LB_ERR,
            }
        }
        0x18B => with_ctl(hwnd, |c| c.items.len() as isize)?, // GETCOUNT
        0x18C => {
            // SELECTSTRING
            let needle = read_str(hwnd, lp);
            let c = snapshot(hwnd)?;
            let i = list_find(&c, wp as isize, &needle, false);
            if i >= 0 {
                with_ctl(hwnd, |c| c.sel = i as i32);
                sync_combo_text(hwnd);
                invalidate(hwnd);
            }
            i
        }
        0x18E => with_ctl(hwnd, |c| c.top as isize)?, // GETTOPINDEX
        0x18F | 0x1A2 => {
            // FINDSTRING / FINDSTRINGEXACT
            let needle = read_str(hwnd, lp);
            let c = snapshot(hwnd)?;
            list_find(&c, wp as isize, &needle, msg == 0x1A2)
        }
        0x190 => with_ctl(hwnd, |c| {
            // LB_GETSELCOUNT
            if multi_select(hwnd) {
                c.selected.iter().filter(|&&s| s).count() as isize
            } else {
                LB_ERR
            }
        })?,
        0x191 => {
            // LB_GETSELITEMS
            let c = snapshot(hwnd)?;
            let mut n = 0;
            for (i, &s) in c.selected.iter().enumerate() {
                if s && n < wp && lp != 0 {
                    *(lp as *mut i32).add(n) = i as i32;
                    n += 1;
                }
            }
            n as isize
        }
        0x197 => {
            // SETTOPINDEX
            with_ctl(hwnd, |c| {
                c.top = (wp as i32).clamp(0, (c.items.len() as i32 - 1).max(0))
            });
            invalidate(hwnd);
            0
        }
        0x198 => {
            // LB_GETITEMRECT
            let c = snapshot(hwnd)?;
            if wp >= c.items.len() || lp == 0 {
                return Some(LB_ERR);
            }
            let ih = item_height(&c);
            let r = client(hwnd);
            *(lp as *mut Rect) = Rect::from_size(0, (wp as i32 - c.top) * ih, r.width(), ih);
            1
        }
        0x199 => {
            // GETITEMDATA
            let c = snapshot(hwnd)?;
            c.items.get(wp).map_or(LB_ERR, |(_, d)| *d)
        }
        0x19A => {
            // SETITEMDATA
            with_ctl(hwnd, |c| match c.items.get_mut(wp) {
                Some(item) => {
                    item.1 = lp;
                    1
                }
                None => LB_ERR,
            })?
        }
        0x19E => {
            // LB_SETCARETINDEX
            with_ctl(hwnd, |c| c.sel = wp as i32);
            invalidate(hwnd);
            0
        }
        0x19F => with_ctl(hwnd, |c| c.sel.max(0) as isize)?, // LB_GETCARETINDEX
        0x1A0 => {
            // SETITEMHEIGHT
            with_ctl(hwnd, |c| c.item_h = (lp as i32).clamp(1, 255));
            invalidate(hwnd);
            0
        }
        0x1A1 => with_ctl(hwnd, |c| item_height(c) as isize)?, // GETITEMHEIGHT
        0x1A8 => with_ctl(hwnd, |c| c.items.len() as isize)?,  // LB_INITSTORAGE
        0x1A9 => {
            // LB_ITEMFROMPOINT
            let c = snapshot(hwnd)?;
            let (_, y) = point(lp);
            let i = c.top + y / item_height(&c);
            let outside = i < 0 || i as usize >= c.items.len();
            let idx = i.clamp(0, (c.items.len() as i32 - 1).max(0)) as isize;
            idx | if outside { 0x1_0000 } else { 0 }
        }
        0x183 | 0x193 | 0x194 | 0x195 | 0x196 | 0x1A5 | 0x1A6 => 0, // DIR, extents, colonnes, locale
        _ => return None,
    };
    Some(r)
}

fn list_rows(hwnd: usize) -> i32 {
    let Some(c) = snapshot(hwnd) else {
        return 1;
    };
    let ih = item_height(&c);
    let h = if c.kind == Kind::ComboBox {
        combo_list_rect(hwnd, &c, client(hwnd)).height()
    } else {
        client(hwnd).height()
    };
    (h / ih).max(1)
}

/// Sélection au clavier ; vrai si la sélection a changé.
fn list_key(hwnd: usize, vk: usize) -> bool {
    let rows = list_rows(hwnd);
    let changed = with_ctl(hwnd, |c| {
        let n = c.items.len() as i32;
        if n == 0 {
            return false;
        }
        let old = c.sel;
        let cur = c.sel.max(0);
        let next = match vk {
            VK_UP | VK_LEFT => cur - 1,
            VK_DOWN | VK_RIGHT => {
                if old < 0 {
                    0
                } else {
                    cur + 1
                }
            }
            VK_PRIOR => cur - rows + 1,
            VK_NEXT => cur + rows - 1,
            VK_HOME => 0,
            VK_END => n - 1,
            _ => return false,
        };
        c.sel = next.clamp(0, n - 1);
        c.sel != old
    })
    .unwrap_or(false);
    if changed {
        ensure_visible(hwnd, rows);
        sync_combo_text(hwnd);
        invalidate(hwnd);
    }
    changed
}

unsafe fn listbox_proc_impl(hwnd: usize, msg: u32, wp: usize, lp: isize) -> Option<isize> {
    if let Some(r) = list_message(hwnd, msg, wp, lp) {
        return Some(r);
    }
    let notify_style = style(hwnd) & LBS_NOTIFY != 0;
    match msg {
        WM_GETDLGCODE => Some(DLGC_WANTARROWS | DLGC_WANTCHARS),
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            if !enabled(hwnd) {
                return Some(0);
            }
            user32::set_focus(hwnd);
            let c = snapshot(hwnd)?;
            let (_, y) = point(lp);
            let area_top = if has_border(hwnd) { 1 } else { 0 };
            let i = c.top + (y - area_top) / item_height(&c);
            if i < 0 || i as usize >= c.items.len() {
                return Some(0);
            }
            let multi = multi_select(hwnd);
            let now = Instant::now();
            let double = msg == WM_LBUTTONDBLCLK
                || c.last_click
                    .is_some_and(|(t, j)| j == i && now.duration_since(t).as_millis() < 500);
            let changed = with_ctl(hwnd, |c| {
                c.last_click = if double { None } else { Some((now, i)) };
                if multi {
                    let s = &mut c.selected[i as usize];
                    *s = !*s;
                    c.sel = i;
                    true
                } else {
                    let old = c.sel;
                    c.sel = i;
                    old != i
                }
            })?;
            invalidate(hwnd);
            if notify_style {
                if changed {
                    notify(hwnd, 1); // LBN_SELCHANGE
                }
                if double {
                    notify(hwnd, 2); // LBN_DBLCLK
                }
            }
            Some(0)
        }
        WM_KEYDOWN => {
            if list_key(hwnd, wp) && notify_style {
                notify(hwnd, 1);
            }
            Some(0)
        }
        WM_CHAR => {
            // Saut à l'élément qui commence par la lettre tapée.
            if let Some(ch) = char::from_u32(wp as u32).filter(|c| !c.is_control()) {
                let c = snapshot(hwnd)?;
                let i = list_find(&c, c.sel as isize, &ch.to_string(), false);
                if i >= 0 && i as i32 != c.sel {
                    with_ctl(hwnd, |c| c.sel = i as i32);
                    ensure_visible(hwnd, list_rows(hwnd));
                    invalidate(hwnd);
                    if notify_style {
                        notify(hwnd, 1);
                    }
                }
            }
            Some(0)
        }
        WM_MOUSEWHEEL => {
            let delta = (wp >> 16) as u16 as i16;
            let rows = list_rows(hwnd);
            with_ctl(hwnd, |c| {
                let n = c.items.len() as i32;
                c.top = (c.top - (delta as i32 / 120) * 3).clamp(0, (n - rows).max(0));
            });
            invalidate(hwnd);
            Some(0)
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            with_ctl(hwnd, |c| c.focused = msg == WM_SETFOCUS);
            invalidate(hwnd);
            if notify_style {
                notify(hwnd, if msg == WM_SETFOCUS { 4 } else { 5 }); // LBN_SETFOCUS / KILLFOCUS
            }
            Some(0)
        }
        _ => None,
    }
}

// ---------- COMBOBOX ----------

const CBS_SIMPLE: u32 = 0x1;
const CBS_DROPDOWNLIST: u32 = 0x3;

fn combo_type(hwnd: usize) -> u32 {
    style(hwnd) & 0x3
}

fn combo_editable(hwnd: usize) -> bool {
    combo_type(hwnd) != CBS_DROPDOWNLIST
}

/// Zone fermée (champ + flèche) d'une COMBOBOX.
fn combo_text_rect(hwnd: usize, c: &Ctl, r: Rect) -> Rect {
    if c.kind != Kind::ComboBox {
        return r;
    }
    let h = if c.closed_h > 0 {
        c.closed_h
    } else {
        r.height()
    };
    let _ = hwnd;
    Rect {
        bottom: r.top + h.min(r.height()),
        ..r
    }
}

fn combo_list_rect(hwnd: usize, c: &Ctl, r: Rect) -> Rect {
    let top = combo_text_rect(hwnd, c, r).bottom;
    Rect {
        left: r.left,
        top: top + 1,
        right: r.right,
        bottom: r.bottom - 1,
    }
}

/// Le texte d'une COMBOBOX suit l'élément sélectionné.
fn sync_combo_text(hwnd: usize) {
    let Some(c) = snapshot(hwnd) else {
        return;
    };
    if c.kind != Kind::ComboBox {
        return;
    }
    if let Some((t, _)) = c.items.get(c.sel.max(0) as usize).filter(|_| c.sel >= 0) {
        set_title(hwnd, t);
        let n = t.chars().count();
        with_ctl(hwnd, |c| {
            c.edit.anchor = 0;
            c.edit.caret = n;
            c.edit.scroll_x = 0;
        });
    }
}

fn paint_combobox(hwnd: usize, c: &Ctl, p: &Painter, r: Rect) {
    let on = enabled(hwnd);
    let editable = combo_editable(hwnd);
    let face = background(
        hwnd,
        p.hdc,
        if editable {
            WM_CTLCOLOREDIT
        } else {
            WM_CTLCOLORSTATIC
        },
        face_color(),
        p,
        r,
    );
    let tr = combo_text_rect(hwnd, c, r);
    let simple = combo_type(hwnd) == CBS_SIMPLE;
    if editable {
        paint_field_frame(hwnd, p, tr, c.focused);
    } else {
        let body = Rect {
            right: tr.right - 1,
            bottom: tr.bottom - 1,
            ..tr
        };
        p.shape(
            body,
            if on { CONTROL_FILL } else { CONTROL_PRESSED },
            CONTROL_BORDER,
            4,
            false,
        );
    }
    let arrow_w = if simple { 0 } else { 22 };
    let text_area = Rect {
        right: tr.right - arrow_w,
        ..edit_area(hwnd, tr)
    };
    let color = if on { face } else { TEXT_DISABLED };
    if editable {
        paint_edit_text(hwnd, c, p, text_area, &title(hwnd), false, color);
    } else if owner_draw_list(hwnd) {
        if let Some((_, data)) = c.items.get(c.sel.max(0) as usize).filter(|_| c.sel >= 0) {
            let state = 0x1000 | if c.focused { 0x11 } else { 0 }; // ODS_COMBOBOXEDIT
            draw_item(hwnd, p.hdc, 3, c.sel, state, text_area, *data);
        }
    } else {
        let label = c
            .items
            .get(c.sel.max(0) as usize)
            .filter(|_| c.sel >= 0)
            .map(|(t, _)| t.clone())
            .unwrap_or_default();
        if c.focused && !c.dropped {
            let hr = Rect {
                left: text_area.left - 2,
                top: text_area.top + 3,
                right: text_area.right,
                bottom: text_area.bottom - 3,
            };
            p.fill(hr, ACCENT);
            p.text(text_area, &label, FIELD, 0);
        } else {
            p.text(text_area, &label, color, 0);
        }
    }
    if !simple {
        // Chevron vers le bas.
        let cx = tr.right - 12;
        let cy = tr.top + tr.height() / 2;
        let col = if on { 0x005D_5D5D } else { TEXT_DISABLED };
        p.line(cx - 4, cy - 2, cx, cy + 2, col, 1);
        p.line(cx, cy + 2, cx + 4, cy - 2, col, 1);
    }
    if c.dropped || simple {
        let lr = combo_list_rect(hwnd, c, r);
        p.fill(lr, FIELD);
        p.frame(lr, CONTROL_BORDER);
        let inner = Rect {
            left: lr.left + 1,
            top: lr.top + 1,
            right: lr.right - 1,
            bottom: lr.bottom - 1,
        };
        paint_items(hwnd, c, p, inner, 3); // ODT_COMBOBOX
    }
}

/// Ouvre ou ferme la liste déroulante : la fenêtre s'agrandit vers le bas
/// pour la contenir et passe devant ses voisines.
fn combo_drop(hwnd: usize, open: bool) {
    if combo_type(hwnd) == CBS_SIMPLE {
        return;
    }
    let Some(c) = snapshot(hwnd) else {
        return;
    };
    if c.dropped == open {
        return;
    }
    let rect = gui::window_rect(hwnd).unwrap_or_default();
    let ih = item_height(&c);
    let wanted = ih * (c.items.len() as i32).clamp(1, 8) + 4;
    let list_h = (c.full_h - c.closed_h).max(wanted.min(ih * 8 + 4));
    let h = if open {
        c.closed_h + list_h
    } else {
        c.closed_h
    };
    if open {
        notify(hwnd, 7); // CBN_DROPDOWN
    }
    with_ctl(hwnd, |c| {
        c.dropped = open;
        if open && c.sel >= 0 {
            c.top = (c.sel - 3).max(0);
        }
    });
    unsafe {
        user32::set_window_pos(
            hwnd,
            0,
            rect.left,
            rect.top,
            rect.width(),
            h,
            0x0004 | 0x0010,
        );
    }
    if open {
        gui::with_backend_public(|b| b.raise_window(hwnd));
        gui::set_capture(hwnd);
    } else {
        gui::release_capture();
        notify(hwnd, 8); // CBN_CLOSEUP
    }
    invalidate(hwnd);
}

/// Traduit un message CB_* vers son équivalent LB_* pour la liste.
fn cb_to_lb(msg: u32) -> Option<u32> {
    Some(match msg {
        0x143 => 0x180, // ADDSTRING
        0x144 => 0x182, // DELETESTRING
        0x146 => 0x18B, // GETCOUNT
        0x147 => 0x188, // GETCURSEL
        0x148 => 0x189, // GETLBTEXT
        0x149 => 0x18A, // GETLBTEXTLEN
        0x14A => 0x181, // INSERTSTRING
        0x14B => 0x184, // RESETCONTENT
        0x14C => 0x18F, // FINDSTRING
        0x14D => 0x18C, // SELECTSTRING
        0x14E => 0x186, // SETCURSEL
        0x150 => 0x199, // GETITEMDATA
        0x151 => 0x19A, // SETITEMDATA
        0x158 => 0x1A2, // FINDSTRINGEXACT
        0x15B => 0x18E, // GETTOPINDEX
        0x15C => 0x197, // SETTOPINDEX
        0x161 => 0x1A8, // INITSTORAGE
        _ => return None,
    })
}

unsafe fn combobox_proc_impl(hwnd: usize, msg: u32, wp: usize, lp: isize) -> Option<isize> {
    let editable = combo_editable(hwnd);
    // Liste d'une ComboBoxEx : Entrée et Échap terminent la saisie.
    if msg == WM_KEYDOWN
        && editable
        && kind_of(parent(hwnd)) == Some(Kind::ComboBoxEx)
        && !with_ctl(hwnd, |c| c.dropped).unwrap_or(false)
        && bars::combo_end_edit(parent(hwnd), wp)
    {
        return Some(0);
    }
    if let Some(lb) = cb_to_lb(msg) {
        let r = list_message(hwnd, lb, wp, lp);
        return r.map(|v| if v == LB_ERR { -1 } else { v });
    }
    let c = snapshot(hwnd)?;
    let r = client(hwnd);
    let tr = combo_text_rect(hwnd, &c, r);
    let arrow_w = if combo_type(hwnd) == CBS_SIMPLE {
        0
    } else {
        22
    };
    let text_area = Rect {
        right: tr.right - arrow_w,
        ..edit_area(hwnd, tr)
    };
    if editable
        && !matches!(
            msg,
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_MOUSEMOVE | WM_LBUTTONUP
        )
    {
        if let Some(v) = edit_common(hwnd, msg, wp, lp, text_area) {
            return Some(v);
        }
    }
    match msg {
        WM_GETDLGCODE => {
            Some(DLGC_WANTARROWS | DLGC_WANTCHARS | if editable { DLGC_HASSETSEL } else { 0 })
        }
        0x0140 => {
            // CB_GETEDITSEL
            edit_common(hwnd, 0x00B0, wp, lp, text_area)
        }
        0x0142 => {
            // CB_SETEDITSEL : lp = MAKELPARAM(début, fin)
            let (a, b) = point(lp);
            edit_common(hwnd, 0x00B1, a as isize as usize, b as isize, text_area)
        }
        0x0141 => edit_common(hwnd, 0x00C5, wp, lp, text_area), // CB_LIMITTEXT
        0x014F => {
            combo_drop(hwnd, wp != 0); // CB_SHOWDROPDOWN
            Some(1)
        }
        0x0157 => Some(c.dropped as isize), // CB_GETDROPPEDSTATE
        0x0152 => Some(0),                  // CB_GETDROPPEDCONTROLRECT (géré ci-dessous)
        0x0153 => {
            // CB_SETITEMHEIGHT : wp = -1 pour le champ, sinon les éléments
            if wp as isize == -1 {
                with_ctl(hwnd, |c| c.closed_h = (lp as i32).clamp(8, 200) + 6);
            } else {
                with_ctl(hwnd, |c| c.item_h = (lp as i32).clamp(1, 255));
            }
            invalidate(hwnd);
            Some(0)
        }
        0x0154 => Some(if wp as isize == -1 {
            (c.closed_h - 6) as isize
        } else {
            item_height(&c) as isize
        }), // CB_GETITEMHEIGHT
        0x0155 | 0x0156 | 0x015F | 0x0160 | 0x0159 | 0x015A | 0x015E | 0x0164 => Some(0),
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            if !enabled(hwnd) {
                return Some(0);
            }
            user32::set_focus(hwnd);
            let (x, y) = point(lp);
            let inside_text = y < tr.bottom && y >= 0;
            if c.dropped || combo_type(hwnd) == CBS_SIMPLE {
                let lr = combo_list_rect(hwnd, &c, r);
                if y > lr.top && y < lr.bottom && x >= 0 && x < r.width() {
                    let i = c.top + (y - lr.top - 1) / item_height(&c);
                    if i >= 0 && (i as usize) < c.items.len() {
                        let changed = with_ctl(hwnd, |c| std::mem::replace(&mut c.sel, i) != i)?;
                        sync_combo_text(hwnd);
                        combo_drop(hwnd, false);
                        if changed {
                            notify(hwnd, 1); // CBN_SELCHANGE
                        }
                        if msg == WM_LBUTTONDBLCLK {
                            notify(hwnd, 2); // CBN_DBLCLK
                        }
                        notify(hwnd, 9); // CBN_SELENDOK
                        invalidate(hwnd);
                    }
                    return Some(0);
                }
                if !inside_text || x < 0 || x >= r.width() {
                    combo_drop(hwnd, false);
                    notify(hwnd, 10); // CBN_SELENDCANCEL
                    return Some(0);
                }
            }
            if editable && x < tr.right - arrow_w {
                return edit_common(hwnd, msg, wp, lp, text_area);
            }
            combo_drop(hwnd, !c.dropped);
            Some(0)
        }
        WM_MOUSEMOVE | WM_LBUTTONUP => {
            if editable {
                edit_common(hwnd, msg, wp, lp, text_area);
            }
            Some(0)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let alt_down = msg == WM_SYSKEYDOWN;
            if wp == VK_F4 || (alt_down && (wp == VK_DOWN || wp == VK_UP)) {
                combo_drop(hwnd, !c.dropped);
                return Some(0);
            }
            if c.dropped && (wp == VK_RETURN || wp == VK_ESCAPE) {
                combo_drop(hwnd, false);
                notify(hwnd, if wp == VK_RETURN { 9 } else { 10 });
                return Some(0);
            }
            if matches!(wp, VK_UP | VK_DOWN | VK_PRIOR | VK_NEXT)
                || (!editable && matches!(wp, VK_HOME | VK_END | VK_LEFT | VK_RIGHT))
            {
                if list_key(hwnd, wp) {
                    notify(hwnd, 1);
                }
                return Some(0);
            }
            if editable {
                edit_key(hwnd, wp);
            }
            if alt_down {
                return None;
            }
            Some(0)
        }
        WM_CHAR => {
            if editable {
                if wp as u32 != 0x0D && wp as u32 != 0x1B {
                    edit_char(hwnd, wp as u32);
                }
            } else if let Some(ch) = char::from_u32(wp as u32).filter(|c| !c.is_control()) {
                let i = list_find(&c, c.sel as isize, &ch.to_string(), false);
                if i >= 0 && i as i32 != c.sel {
                    with_ctl(hwnd, |c| c.sel = i as i32);
                    sync_combo_text(hwnd);
                    invalidate(hwnd);
                    notify(hwnd, 1);
                }
            }
            Some(0)
        }
        WM_MOUSEWHEEL => {
            let delta = (wp >> 16) as u16 as i16;
            if c.dropped {
                let rows = list_rows(hwnd);
                with_ctl(hwnd, |c| {
                    let n = c.items.len() as i32;
                    c.top = (c.top - (delta as i32 / 120) * 3).clamp(0, (n - rows).max(0));
                });
                invalidate(hwnd);
            } else if list_key(hwnd, if delta > 0 { VK_UP } else { VK_DOWN }) {
                notify(hwnd, 1);
            }
            Some(0)
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            with_ctl(hwnd, |c| c.focused = msg == WM_SETFOCUS);
            if msg == WM_KILLFOCUS {
                combo_drop(hwnd, false);
            }
            invalidate(hwnd);
            notify(hwnd, if msg == WM_SETFOCUS { 3 } else { 4 }); // CBN_SETFOCUS / KILLFOCUS
            Some(0)
        }
        _ => None,
    }
}

// ---------- SCROLLBAR ----------

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ScrollInfo {
    pub size: u32,
    pub mask: u32,
    pub min: i32,
    pub max: i32,
    pub page: u32,
    pub pos: i32,
    pub track_pos: i32,
}

fn vertical(hwnd: usize) -> bool {
    style(hwnd) & 0x1 != 0 // SBS_VERT
}

/// Pouce (début, longueur) le long de la barre, en pixels.
fn thumb(c: &Ctl, len: i32) -> (i32, i32) {
    let range = (c.max - c.min + 1).max(1);
    let page = (c.page as i32).clamp(0, range);
    let size = if page > 0 {
        (len * page / range).max(16)
    } else {
        (len / 8).max(16)
    }
    .min(len);
    let span = (range - page.max(1)).max(1);
    let start = (len - size) * (c.pos - c.min).clamp(0, span) / span;
    (start, size)
}

fn paint_scrollbar(hwnd: usize, c: &Ctl, p: &Painter, r: Rect) {
    background(hwnd, p.hdc, WM_CTLCOLORSCROLLBAR, 0x00F9_F9F9, p, r);
    let vert = vertical(hwnd);
    let len = if vert { r.height() } else { r.width() };
    let (start, size) = thumb(c, len);
    let tr = if vert {
        Rect::from_size(r.left + r.width() / 2 - 3, r.top + start, 6, size)
    } else {
        Rect::from_size(r.left + start, r.top + r.height() / 2 - 3, size, 6)
    };
    p.shape(tr, STRONG_BORDER, STRONG_BORDER, 3, false);
}

fn scroll_to_parent(hwnd: usize, code: usize, pos: i32) {
    let msg = if vertical(hwnd) {
        WM_VSCROLL
    } else {
        WM_HSCROLL
    };
    send(
        parent(hwnd),
        msg,
        code | ((pos as u16 as usize) << 16),
        hwnd as isize,
    );
}

/// SetScrollInfo / GetScrollInfo sur un contrôle SCROLLBAR (SB_CTL).
pub unsafe fn scroll_info(
    hwnd: usize,
    info: *mut ScrollInfo,
    set: bool,
    redraw: bool,
) -> Option<i32> {
    if kind_of(hwnd) != Some(Kind::ScrollBar) || info.is_null() {
        return None;
    }
    let i = &mut *info;
    if set {
        with_ctl(hwnd, |c| {
            if i.mask & 0x1 != 0 {
                c.min = i.min;
                c.max = i.max;
            }
            if i.mask & 0x2 != 0 {
                c.page = i.page;
            }
            if i.mask & 0x4 != 0 {
                c.pos = i.pos;
            }
            let top = (c.max - (c.page as i32 - 1).max(0)).max(c.min);
            c.pos = c.pos.clamp(c.min, top);
        });
        if redraw {
            invalidate(hwnd);
        }
        with_ctl(hwnd, |c| c.pos)
    } else {
        with_ctl(hwnd, |c| {
            if i.mask & 0x1 != 0 {
                i.min = c.min;
                i.max = c.max;
            }
            if i.mask & 0x2 != 0 {
                i.page = c.page;
            }
            if i.mask & 0x4 != 0 {
                i.pos = c.pos;
            }
            if i.mask & 0x10 != 0 {
                i.track_pos = c.drag.map_or(c.pos, |(_, p)| p);
            }
            1
        })
    }
}

unsafe fn scrollbar_proc_impl(hwnd: usize, msg: u32, wp: usize, lp: isize) -> Option<isize> {
    match msg {
        0x00E0 => {
            // SBM_SETPOS
            let old = with_ctl(hwnd, |c| {
                let old = c.pos;
                c.pos = (wp as i32).clamp(c.min, c.max.max(c.min));
                old
            })?;
            if lp != 0 {
                invalidate(hwnd);
            }
            Some(old as isize)
        }
        0x00E1 => Some(with_ctl(hwnd, |c| c.pos as isize)?), // SBM_GETPOS
        0x00E2 | 0x00E6 => {
            // SBM_SETRANGE / SBM_SETRANGEREDRAW
            with_ctl(hwnd, |c| {
                c.min = wp as i32;
                c.max = lp as i32;
                c.pos = c.pos.clamp(c.min, c.max.max(c.min));
            });
            invalidate(hwnd);
            Some(0)
        }
        0x00E3 => {
            // SBM_GETRANGE
            let (min, max) = with_ctl(hwnd, |c| (c.min, c.max))?;
            if wp != 0 {
                *(wp as *mut i32) = min;
            }
            if lp != 0 {
                *(lp as *mut i32) = max;
            }
            Some(0)
        }
        0x00E4 => Some(1), // SBM_ENABLE_ARROWS
        0x00E9 => {
            Some(scroll_info(hwnd, lp as *mut ScrollInfo, true, wp != 0).unwrap_or(0) as isize)
        }
        0x00EA => {
            Some(scroll_info(hwnd, lp as *mut ScrollInfo, false, false).unwrap_or(0) as isize)
        }
        WM_LBUTTONDOWN => {
            let c = snapshot(hwnd)?;
            let r = client(hwnd);
            let vert = vertical(hwnd);
            let len = if vert { r.height() } else { r.width() };
            let (x, y) = point(lp);
            let at = if vert { y } else { x };
            let (start, size) = thumb(&c, len);
            if at < start {
                scroll_to_parent(hwnd, 2, c.pos); // SB_PAGEUP
            } else if at >= start + size {
                scroll_to_parent(hwnd, 3, c.pos); // SB_PAGEDOWN
            } else {
                with_ctl(hwnd, |c| c.drag = Some((at - start, c.pos)));
                gui::set_capture(hwnd);
            }
            Some(0)
        }
        WM_MOUSEMOVE => {
            let c = snapshot(hwnd)?;
            if let Some((grab, _)) = c.drag {
                let r = client(hwnd);
                let vert = vertical(hwnd);
                let len = if vert { r.height() } else { r.width() };
                let (x, y) = point(lp);
                let at = if vert { y } else { x };
                let (_, size) = thumb(&c, len);
                let range = (c.max - c.min + 1).max(1);
                let span = (range - (c.page as i32).max(1)).max(1);
                let track = (len - size).max(1);
                let pos = c.min + ((at - grab).clamp(0, track) * span + track / 2) / track;
                with_ctl(hwnd, |c| c.drag = Some((grab, pos)));
                scroll_to_parent(hwnd, 5, pos); // SB_THUMBTRACK
            }
            Some(0)
        }
        WM_LBUTTONUP => {
            if let Some((_, pos)) = with_ctl(hwnd, |c| c.drag.take())? {
                gui::release_capture();
                scroll_to_parent(hwnd, 4, pos); // SB_THUMBPOSITION
            }
            scroll_to_parent(hwnd, 8, 0); // SB_ENDSCROLL
            Some(0)
        }
        WM_GETDLGCODE => Some(0),
        _ => None,
    }
}

// ---------- Procédure commune ----------

unsafe fn control_proc_impl(kind: Kind, hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    if let Some(r) = bars::pre_proc(kind, hwnd, msg, wp, lp) {
        return r;
    }
    match msg {
        WM_NCCREATE => {
            let wide = WIDE.with(|w| w.get()).unwrap_or(true);
            lock(state()).insert(hwnd, Ctl::new(kind, wide));
            return 1;
        }
        WM_CREATE => {
            if kind == Kind::ComboBox {
                let font = font_of(&snapshot(hwnd).unwrap_or(Ctl::new(kind, true)));
                let full = client(hwnd).height();
                let closed = (line_height(font) + 10).max(22);
                with_ctl(hwnd, |c| {
                    c.full_h = full;
                    c.closed_h = closed;
                });
                if combo_type(hwnd) != CBS_SIMPLE && full > closed {
                    if let Some(rect) = gui::window_rect(hwnd) {
                        gui::set_window_rect(
                            hwnd,
                            Rect {
                                bottom: rect.top + closed,
                                ..rect
                            },
                        );
                    }
                }
            }
            if kind == Kind::Edit {
                let n = title(hwnd).chars().count();
                let password = style(hwnd) & ES_PASSWORD != 0;
                with_ctl(hwnd, |c| {
                    c.edit.caret = 0;
                    c.edit.anchor = 0;
                    let _ = n;
                    if password {
                        c.edit.password = Some('●');
                    }
                });
            }
            if kind == Kind::ScrollBar {
                with_ctl(hwnd, |c| c.max = 100);
            }
            comctl::init(kind, hwnd);
            return 0;
        }
        WM_NCDESTROY => {
            lock(state()).remove(&hwnd);
            comctl::forget(hwnd);
            bars::forget(hwnd);
            return 0;
        }
        WM_DESTROY => return 0,
        WM_PAINT => {
            paint(hwnd, wp);
            return 0;
        }
        // WM_PRINTCLIENT : même rendu, dans le DC du programme.
        0x0318 => {
            paint(hwnd, wp);
            return 0;
        }
        WM_ERASEBKGND => return 1,
        WM_SETFONT => {
            with_ctl(hwnd, |c| c.font = wp);
            if lp != 0 {
                invalidate(hwnd);
            }
            return 0;
        }
        WM_GETFONT => return with_ctl(hwnd, |c| c.font as isize).unwrap_or(0),
        WM_ENABLE => {
            invalidate(hwnd);
            return 0;
        }
        WM_SETTEXT => {
            let s = read_str(hwnd, lp);
            set_title(hwnd, &s);
            text_changed(hwnd);
            return 1;
        }
        WM_GETTEXT => return write_str(hwnd, &title(hwnd), lp, wp as u32),
        WM_GETTEXTLENGTH => return text_len(hwnd, &title(hwnd)),
        WM_SIZE => {
            if kind == Kind::Edit || kind == Kind::ComboBox {
                keep_caret_visible(hwnd);
            }
            invalidate(hwnd);
            // Les contrôles qui se placent seuls (barre d'état, barres
            // d'outils, bandes) suivent la nouvelle taille de leur parent.
            if !matches!(
                kind,
                Kind::StatusBar | Kind::ToolBar | Kind::ReBar | Kind::ComboBoxEx
            ) {
                return 0;
            }
        }
        _ => {}
    }
    let handled = match kind {
        Kind::Button => button_proc_impl(hwnd, msg, wp, lp),
        Kind::Static => static_proc_impl(hwnd, msg, wp, lp),
        Kind::Edit => edit_proc_impl(hwnd, msg, wp, lp),
        Kind::ListBox => listbox_proc_impl(hwnd, msg, wp, lp),
        Kind::ComboBox => combobox_proc_impl(hwnd, msg, wp, lp),
        Kind::ScrollBar => scrollbar_proc_impl(hwnd, msg, wp, lp),
        kind => comctl::proc_impl(kind, hwnd, msg, wp, lp),
    };
    match handled {
        Some(r) => r,
        None => gui::def_window_proc(hwnd, msg, wp, lp),
    }
}

/// Appelé quand le texte d'un contrôle change (SetWindowText, WM_SETTEXT) :
/// remet le curseur au début, prévient le parent pour un EDIT et redessine.
pub fn text_changed(hwnd: usize) {
    let Some(kind) = kind_of(hwnd) else {
        return;
    };
    if kind == Kind::ComboBoxEx {
        // SetWindowText sur une ComboBoxEx remplit son champ (barre
        // d'adresse de 7-Zip), comme le WM_SETTEXT que Windows lui envoie.
        let combo = unsafe { bars::cbex_combo(hwnd) };
        if combo != 0 {
            let text: Vec<u16> = title(hwnd).encode_utf16().chain([0]).collect();
            with_charset(true, || send(combo, WM_SETTEXT, 0, text.as_ptr() as isize));
        }
        return;
    }
    if kind == Kind::Edit || kind == Kind::ComboBox {
        with_ctl(hwnd, |c| {
            c.edit.caret = 0;
            c.edit.anchor = 0;
            c.edit.scroll_x = 0;
            c.edit.top_line = 0;
            c.edit.modified = false;
        });
        if kind == Kind::Edit {
            notify(hwnd, 0x0400);
            notify(hwnd, 0x0300);
        }
    }
    invalidate(hwnd);
}

// ---------- Fonctions de dialogue qui s'appuient sur les contrôles ----------

unsafe extern "win64" fn check_dlg_button(dlg: usize, id: i32, check: u32) -> i32 {
    let item = user32::get_dlg_item(dlg, id);
    if item == 0 {
        return 0;
    }
    send(item, 0x00F1, check as usize, 0);
    1
}

unsafe extern "win64" fn is_dlg_button_checked(dlg: usize, id: i32) -> u32 {
    let item = user32::get_dlg_item(dlg, id);
    if item == 0 {
        return 0;
    }
    send(item, 0x00F0, 0, 0) as u32
}

/// Exports de user32 qui remplacent les anciennes versions vides.
pub fn user32_overrides() -> Exports {
    crate::exports! {
        "CheckDlgButton" => fn check_dlg_button,
        "IsDlgButtonChecked" => fn is_dlg_button_checked,
    }
}

mod bars;
mod comctl;
pub(crate) mod popup;

#[cfg(test)]
mod tests;
