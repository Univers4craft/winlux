//! Gestionnaire de boîtes de dialogue de user32 : `DialogBoxParam`,
//! `CreateDialogParam` et leurs variantes « Indirect », `DefDlgProc`,
//! `IsDialogMessage` (Tab, Entrée, Échap, flèches, Alt+lettre), `EndDialog`
//! et `MessageBox` en vraie fenêtre.
//!
//! Un programme Win32 classique décrit ses fenêtres de réglages dans ses
//! ressources (modèle DLGTEMPLATE ou DLGTEMPLATEEX). Le gestionnaire lit ce
//! modèle, crée la fenêtre et ses contrôles (voir `controls.rs`), puis
//! appelle la procédure de dialogue du programme.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::gui::{self, Msg, Rect};
use super::{controls, lock, user32, Exports};

/// Index de la procédure de dialogue dans les données de fenêtre (x64).
const DWLP_DLGPROC: i32 = 8;
const WM_DESTROY: u32 = 0x0002;
const WM_SETFOCUS: u32 = 0x0007;
const WM_CLOSE: u32 = 0x0010;
const WM_QUIT: u32 = 0x0012;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_NEXTDLGCTL: u32 = 0x0028;
const WM_SETFONT: u32 = 0x0030;
const WM_GETFONT: u32 = 0x0031;
const WM_COMPAREITEM: u32 = 0x0039;
const WM_GETDLGCODE: u32 = 0x0087;
const WM_KEYDOWN: u32 = 0x0100;
const WM_CHAR: u32 = 0x0102;
const WM_SYSCHAR: u32 = 0x0106;
const WM_INITDIALOG: u32 = 0x0110;
const WM_COMMAND: u32 = 0x0111;
const WM_VKEYTOITEM: u32 = 0x002E;
const WM_CHARTOITEM: u32 = 0x002F;
const WM_QUERYDRAGICON: u32 = 0x0037;
const WM_CTLCOLORMSGBOX: u32 = 0x0132;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const DM_GETDEFID: u32 = 0x0400;
const DM_SETDEFID: u32 = 0x0401;
const DM_REPOSITION: u32 = 0x0402;

const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_DISABLED: u32 = 0x0800_0000;
const WS_GROUP: u32 = 0x0002_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const DS_SETFONT: u32 = 0x40;
const DS_CENTER: u32 = 0x800;
const DS_CONTROL: u32 = 0x400;

const DLGC_WANTARROWS: isize = 0x0001;
const DLGC_WANTTAB: isize = 0x0002;
const DLGC_WANTMESSAGE: isize = 0x0004;
const DLGC_HASSETSEL: isize = 0x0008;
const DLGC_DEFPUSHBUTTON: isize = 0x0010;
const DLGC_UNDEFPUSHBUTTON: isize = 0x0020;
const DLGC_RADIOBUTTON: isize = 0x0040;
const DLGC_STATIC: isize = 0x0100;
const DLGC_BUTTON: isize = 0x2000;

const IDOK: usize = 1;
const IDCANCEL: usize = 2;

/// Classe des boîtes de dialogue standard (`WC_DIALOG`).
pub const DIALOG_CLASS: &str = "#32770";

// ---------- État des boîtes ----------

struct Dialog {
    wide: bool,
    font: usize,
    def_id: usize,
    result: isize,
    ended: bool,
    focus: usize,
}

fn dialogs() -> &'static Mutex<HashMap<usize, Dialog>> {
    static D: OnceLock<Mutex<HashMap<usize, Dialog>>> = OnceLock::new();
    D.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn is_dialog(hwnd: usize) -> bool {
    lock(dialogs()).contains_key(&hwnd)
}

// ---------- Lecture des modèles ----------

#[derive(Debug, Clone, PartialEq)]
pub enum NameOrId {
    None,
    Id(u16),
    Name(String),
}

#[derive(Debug, Clone)]
pub struct ItemTemplate {
    pub style: u32,
    pub ex_style: u32,
    pub x: i16,
    pub y: i16,
    pub cx: i16,
    pub cy: i16,
    pub id: u32,
    pub class: NameOrId,
    pub title: NameOrId,
    pub extra: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct DialogTemplate {
    pub style: u32,
    pub ex_style: u32,
    pub x: i16,
    pub y: i16,
    pub cx: i16,
    pub cy: i16,
    #[allow(dead_code)] // barre de menus des boîtes : pas encore affichée
    pub menu: NameOrId,
    pub class: NameOrId,
    pub title: String,
    pub point_size: u16,
    pub weight: u16,
    pub italic: bool,
    pub face: String,
    pub items: Vec<ItemTemplate>,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn u16(&mut self) -> Option<u16> {
        let b = self.data.get(self.pos..self.pos + 2)?;
        self.pos += 2;
        Some(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        let b = self.data.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i16(&mut self) -> Option<i16> {
        self.u16().map(|v| v as i16)
    }
    fn u8(&mut self) -> Option<u8> {
        let b = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }
    fn wstr(&mut self) -> Option<String> {
        let mut units = Vec::new();
        loop {
            let u = self.u16()?;
            if u == 0 {
                break;
            }
            units.push(u);
        }
        Some(String::from_utf16_lossy(&units))
    }
    fn name_or_id(&mut self) -> Option<NameOrId> {
        match self.u16()? {
            0 => Some(NameOrId::None),
            0xFFFF => Some(NameOrId::Id(self.u16()?)),
            first => {
                let mut units = vec![first];
                loop {
                    let u = self.u16()?;
                    if u == 0 {
                        break;
                    }
                    units.push(u);
                }
                Some(NameOrId::Name(String::from_utf16_lossy(&units)))
            }
        }
    }
    fn align4(&mut self) {
        self.pos = (self.pos + 3) & !3;
    }
}

/// Lit un modèle DLGTEMPLATE ou DLGTEMPLATEEX.
pub fn parse_template(data: &[u8]) -> Option<DialogTemplate> {
    let mut r = Reader { data, pos: 0 };
    let ex = data.len() >= 4 && data[0..2] == [1, 0] && data[2..4] == [0xFF, 0xFF];
    let (style, ex_style, count);
    if ex {
        r.pos = 4;
        let _help = r.u32()?;
        ex_style = r.u32()?;
        style = r.u32()?;
    } else {
        style = r.u32()?;
        ex_style = r.u32()?;
    }
    count = r.u16()?;
    let (x, y, cx, cy) = (r.i16()?, r.i16()?, r.i16()?, r.i16()?);
    let menu = r.name_or_id()?;
    let class = r.name_or_id()?;
    let title = r.wstr()?;
    let (mut point_size, mut weight, mut italic, mut face) = (8, 400, false, String::new());
    // Seul DS_SETFONT annonce la police dans l'en-tête ; DS_FIXEDSYS (0x8)
    // seul n'en porte pas (DS_SHELLFONT = DS_SETFONT | DS_FIXEDSYS, #1685).
    if style & DS_SETFONT != 0 {
        point_size = r.u16()?;
        if ex {
            weight = r.u16()?;
            italic = r.u8()? != 0;
            let _charset = r.u8()?;
        }
        face = r.wstr()?;
    }
    let mut items = Vec::with_capacity(count as usize);
    for _ in 0..count {
        r.align4();
        let (style, ex_style, id);
        if ex {
            let _help = r.u32()?;
            ex_style = r.u32()?;
            style = r.u32()?;
        } else {
            style = r.u32()?;
            ex_style = r.u32()?;
        }
        let (x, y, cx, cy) = (r.i16()?, r.i16()?, r.i16()?, r.i16()?);
        id = if ex { r.u32()? } else { r.u16()? as u32 };
        let class = r.name_or_id()?;
        let title = r.name_or_id()?;
        let extra_len = r.u16()? as usize;
        let extra = if extra_len > 0 {
            // DLGTEMPLATE : la taille compte ce mot ; EX : seulement les données.
            let n = if ex {
                extra_len
            } else {
                extra_len.saturating_sub(2)
            };
            let b = data.get(r.pos..r.pos + n)?.to_vec();
            r.pos += n;
            b
        } else {
            Vec::new()
        };
        items.push(ItemTemplate {
            style,
            ex_style,
            x,
            y,
            cx,
            cy,
            id,
            class,
            title,
            extra,
        });
    }
    Some(DialogTemplate {
        style,
        ex_style,
        x,
        y,
        cx,
        cy,
        menu,
        class,
        title,
        point_size,
        weight,
        italic,
        face,
        items,
    })
}

/// Taille d'un modèle en mémoire (pour les variantes « Indirect », qui ne
/// donnent qu'un pointeur) : on lit jusqu'à 64 Kio, la lecture s'arrête au
/// dernier contrôle.
unsafe fn template_bytes(ptr: *const u8) -> &'static [u8] {
    std::slice::from_raw_parts(ptr, 0x1_0000)
}

// ---------- Unités de dialogue ----------

/// Unités de base : largeur moyenne et hauteur de la police du dialogue.
fn base_units(font: usize) -> (i32, i32) {
    let spec = gui::font_spec(font).unwrap_or_default();
    let (w, h) = gui::measure_text(
        &spec,
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz",
    );
    let bx = ((w / 26) + 1) / 2;
    if bx <= 0 || h <= 0 {
        (
            super::win32_basics::DIALOG_BASE_X,
            super::win32_basics::DIALOG_BASE_Y,
        )
    } else {
        (bx.max(5), h.max(12))
    }
}

fn du_rect(base: (i32, i32), x: i16, y: i16, cx: i16, cy: i16) -> Rect {
    Rect::from_size(
        x as i32 * base.0 / 4,
        y as i32 * base.1 / 8,
        cx as i32 * base.0 / 4,
        cy as i32 * base.1 / 8,
    )
}

fn dialog_font(t: &DialogTemplate) -> usize {
    if t.style & (DS_SETFONT | 0x8) == 0 {
        return super::gdi32::get_stock_object(17);
    }
    let face = match t.face.to_ascii_lowercase().as_str() {
        "" | "ms shell dlg" | "ms shell dlg 2" | "ms sans serif" | "system" => {
            "Segoe UI".to_string()
        }
        _ => t.face.clone(),
    };
    let pt = if t.point_size == 0 || t.point_size > 72 {
        9
    } else {
        t.point_size as i32
    };
    gui::create_font(gui::FontSpec {
        height: -((pt * 96 + 36) / 72),
        weight: t.weight as i32,
        italic: t.italic,
        face,
        ..gui::FontSpec::default()
    })
}

// ---------- Création ----------

fn wide_str(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

fn class_name(c: &NameOrId) -> Option<String> {
    match c {
        NameOrId::None => None,
        NameOrId::Id(atom) => controls::atom_class(*atom).map(str::to_string),
        NameOrId::Name(n) => Some(n.clone()),
    }
}

/// Crée la boîte décrite par `t` (cachée tant que le modèle ne demande pas
/// WS_VISIBLE) et renvoie son HWND.
unsafe fn create_from_template(
    t: &DialogTemplate,
    instance: usize,
    parent: usize,
    proc_addr: usize,
    init: isize,
    wide: bool,
) -> usize {
    ensure_dialog_class();
    let font = dialog_font(t);
    let base = base_units(font);
    let mut rect = du_rect(base, t.x, t.y, t.cx, t.cy);
    let child = t.style & WS_CHILD != 0 || t.style & DS_CONTROL != 0;
    if !child {
        // Centrée sur la fenêtre propriétaire, ou sur l'écran.
        let around = if parent != 0 {
            gui::screen_rect(top_level(parent))
        } else {
            None
        };
        let area = around.unwrap_or_else(|| {
            let w = user32::get_system_metrics(0);
            let h = user32::get_system_metrics(1);
            Rect::from_size(0, 0, w, h)
        });
        if t.style & DS_CENTER != 0 || parent == 0 || (t.x == 0 && t.y == 0) {
            rect = Rect::from_size(
                area.left + (area.width() - rect.width()) / 2,
                area.top + (area.height() - rect.height()) / 3,
                rect.width(),
                rect.height(),
            );
        } else {
            rect = Rect::from_size(
                area.left + rect.left,
                area.top + rect.top,
                rect.width(),
                rect.height(),
            );
        }
        // Jamais hors de l'écran (propriétaire petit ou près d'un bord).
        let screen = gui::work_area();
        if screen.width() > 0 && screen.height() > 0 {
            let x = rect.left.min(screen.right - rect.width()).max(screen.left);
            let y = rect.top.min(screen.bottom - rect.height()).max(screen.top);
            rect = Rect::from_size(x, y, rect.width(), rect.height());
        }
    }
    let class = class_name(&t.class).unwrap_or_else(|| DIALOG_CLASS.to_string());
    let class_w = wide_str(&class);
    let title_w = wide_str(&t.title);
    let style = t.style & !WS_VISIBLE;
    let hwnd = controls::with_charset(wide, || {
        user32::create_window_ex_w(
            t.ex_style,
            class_w.as_ptr(),
            title_w.as_ptr(),
            style,
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            parent,
            0,
            instance,
            std::ptr::null_mut(),
        )
    });
    if hwnd == 0 {
        crate::trace!(
            "boîte de dialogue : CreateWindowEx({class:?}) a échoué (erreur {})",
            crate::process::last_error()
        );
        return 0;
    }
    lock(dialogs()).insert(
        hwnd,
        Dialog {
            wide,
            font,
            def_id: IDOK,
            result: 0,
            ended: false,
            focus: 0,
        },
    );
    user32::set_window_long_ptr_w(hwnd, DWLP_DLGPROC, proc_addr as isize);
    // Contrôles du modèle, dans l'ordre (c'est l'ordre de tabulation).
    for item in &t.items {
        let Some(cls) = class_name(&item.class) else {
            continue;
        };
        let text = match &item.title {
            NameOrId::Name(s) => s.clone(),
            _ => String::new(),
        };
        let r = du_rect(base, item.x, item.y, item.cx, item.cy);
        let cls_w = wide_str(&cls);
        let text_w = wide_str(&text);
        let param = if item.extra.is_empty() {
            std::ptr::null_mut()
        } else {
            item.extra.as_ptr() as *mut std::ffi::c_void
        };
        let ctl = controls::with_charset(wide, || {
            user32::create_window_ex_w(
                item.ex_style,
                cls_w.as_ptr(),
                text_w.as_ptr(),
                item.style | WS_CHILD,
                r.left,
                r.top,
                r.width(),
                r.height(),
                hwnd,
                item.id as usize,
                instance,
                param,
            )
        });
        if ctl != 0 {
            user32::send_message_w(ctl, WM_SETFONT, font, 0);
        } else {
            crate::trace!(
                "boîte de dialogue : contrôle {} de classe {cls:?} non créé",
                item.id
            );
        }
    }
    // WM_INITDIALOG : TRUE demande le focus sur le premier contrôle.
    let first = next_tab_item(hwnd, 0, false);
    let focus_default = call_dlg_proc(hwnd, WM_INITDIALOG, first, init);
    if !gui::is_window(hwnd) {
        crate::trace!("boîte de dialogue : détruite pendant WM_INITDIALOG");
        return 0;
    }
    // Le choix se refait après WM_INITDIALOG : la procédure a pu désactiver
    // ou masquer ce contrôle (l'heure de compilation de Notepad++ est un
    // champ désactivé, qui ne doit ni prendre le focus ni être surligné, #704).
    let first = next_tab_item(hwnd, 0, false);
    if focus_default != 0 && first != 0 {
        set_dlg_focus(hwnd, first);
    }
    if t.style & WS_VISIBLE != 0 {
        user32::show_window(hwnd, 5);
    }
    hwnd
}

fn top_level(mut hwnd: usize) -> usize {
    let mut guard = 0;
    while let Some(p) = gui::window_parent(hwnd).filter(|&p| p != 0) {
        if gui::window_style(hwnd).unwrap_or(0) & WS_CHILD == 0 || guard > 64 {
            break;
        }
        hwnd = p;
        guard += 1;
    }
    hwnd
}

/// Appelle la procédure de dialogue du programme ; 0 si elle n'en a pas.
/// C'est celle de DWLP_DLGPROC à cet instant : un programme peut la
/// remplacer par SetWindowLongPtr(DWLP_DLGPROC), comme l'installeur de
/// Google Drive, dont les boutons dessinés par le programme restaient vides
/// faute de WM_DRAWITEM dans la nouvelle procédure (#2421).
unsafe fn call_dlg_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    let wide = match lock(dialogs()).get(&hwnd) {
        Some(d) => d.wide,
        None => return 0,
    };
    let proc_addr = user32::get_window_long_ptr_w(hwnd, DWLP_DLGPROC) as usize;
    if proc_addr == 0 {
        return 0;
    }
    controls::with_charset(wide, || gui::call_wndproc(proc_addr, hwnd, msg, wp, lp))
}

fn ensure_dialog_class() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        gui::register_class(
            DIALOG_CLASS,
            gui::ClassInfo {
                wnd_proc: dialog_class_proc as *const () as usize,
                background: 16, // COLOR_BTNFACE + 1
                instance: 0,
            },
        );
    });
}

unsafe extern "win64" fn dialog_class_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    def_dlg_proc(hwnd, msg, wp, lp)
}

/// Messages dont la procédure de dialogue renvoie directement le résultat
/// (et non par DWLP_MSGRESULT).
fn returns_directly(msg: u32) -> bool {
    matches!(
        msg,
        WM_COMPAREITEM | WM_VKEYTOITEM | WM_CHARTOITEM | WM_QUERYDRAGICON | WM_INITDIALOG
    ) || (WM_CTLCOLORMSGBOX..=WM_CTLCOLORSTATIC).contains(&msg)
}

/// DefDlgProc : procédure de dialogue du programme, puis comportement par
/// défaut d'une boîte de dialogue.
pub unsafe fn def_dlg_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    if !is_dialog(hwnd) {
        // Classe privée (PuTTY) dont la fenêtre a été créée par CreateWindow :
        // elle devient une boîte de dialogue sans procédure.
        if msg == 0x0081 {
            lock(dialogs()).insert(
                hwnd,
                Dialog {
                    wide: true,
                    font: 0,
                    def_id: IDOK,
                    result: 0,
                    ended: false,
                    focus: 0,
                },
            );
        }
        return gui::def_window_proc(hwnd, msg, wp, lp);
    }
    user32::set_window_long_ptr_w(hwnd, 0, 0); // DWLP_MSGRESULT
    let handled = call_dlg_proc(hwnd, msg, wp, lp);
    if !gui::is_window(hwnd) && msg != WM_DESTROY {
        return handled;
    }
    if handled != 0 {
        return if returns_directly(msg) {
            handled
        } else {
            user32::get_window_long_ptr_w(hwnd, 0)
        };
    }
    match msg {
        WM_CLOSE => {
            let cancel = user32::get_dlg_item(hwnd, IDCANCEL as i32);
            if cancel == 0 || user32::is_window_enabled(cancel) != 0 {
                user32::post_message_w(hwnd, WM_COMMAND, IDCANCEL, cancel as isize);
            }
            0
        }
        WM_ERASEBKGND => {
            gui::erase_background(hwnd, wp);
            1
        }
        WM_CTLCOLORMSGBOX..=WM_CTLCOLORSTATIC => user32::get_sys_color_brush(15) as isize,
        WM_GETFONT => lock(dialogs()).get(&hwnd).map_or(0, |d| d.font as isize),
        WM_SETFONT => {
            if let Some(d) = lock(dialogs()).get_mut(&hwnd) {
                d.font = wp;
            }
            0
        }
        WM_SETFOCUS => {
            // Rend le focus au contrôle qui l'avait.
            let saved = lock(dialogs()).get(&hwnd).map_or(0, |d| d.focus);
            let target = if saved != 0 && gui::is_window(saved) {
                saved
            } else {
                next_tab_item(hwnd, 0, false)
            };
            if target != 0 {
                user32::set_focus(target);
            }
            0
        }
        DM_GETDEFID => {
            let id = lock(dialogs()).get(&hwnd).map_or(IDOK, |d| d.def_id);
            ((0x534B << 16) | (id & 0xFFFF)) as isize // DC_HASDEFID
        }
        DM_SETDEFID => {
            set_default_button(hwnd, wp);
            1
        }
        DM_REPOSITION => 0,
        WM_NEXTDLGCTL => {
            let target = if lp & 0xFFFF != 0 {
                wp
            } else {
                let current = gui::focused_window();
                next_tab_item(hwnd, current, wp != 0)
            };
            if target != 0 {
                set_dlg_focus(hwnd, target);
            }
            0
        }
        WM_DESTROY => {
            lock(dialogs()).remove(&hwnd);
            0
        }
        _ => gui::def_window_proc(hwnd, msg, wp, lp),
    }
}

/// Bouton par défaut : son style passe à BS_DEFPUSHBUTTON, l'ancien redevient
/// un bouton ordinaire.
unsafe fn set_default_button(dlg: usize, id: usize) {
    let old = lock(dialogs()).get(&dlg).map_or(0, |d| d.def_id);
    if let Some(d) = lock(dialogs()).get_mut(&dlg) {
        d.def_id = id;
    }
    for (button, def) in [(old, false), (id, true)] {
        let h = user32::get_dlg_item(dlg, button as i32);
        if h != 0 && controls::kind_of(h) == Some(controls::Kind::Button) {
            let style = gui::window_style(h).unwrap_or(0);
            let kind = style & 0xF;
            if kind <= 1 {
                let new = (style & !0xF) | def as u32;
                user32::send_message_w(h, 0x00F4, new as usize, 1); // BM_SETSTYLE
            }
        }
    }
}

// ---------- Navigation au clavier ----------

fn dlg_code(hwnd: usize, msg: Option<&Msg>) -> isize {
    let lp = msg.map_or(0, |m| m as *const Msg as isize);
    let wp = msg.map_or(0, |m| m.wparam);
    unsafe { user32::send_message_w(hwnd, WM_GETDLGCODE, wp, lp) }
}

fn usable(hwnd: usize) -> bool {
    let style = gui::window_style(hwnd).unwrap_or(0);
    style & WS_VISIBLE != 0 && style & WS_DISABLED == 0 && gui::is_window_visible(hwnd)
}

/// Contrôles d'une boîte, dans l'ordre de création (ordre de tabulation).
fn items(dlg: usize) -> Vec<usize> {
    gui::child_windows(dlg)
}

/// Contrôle suivant (ou précédent) portant WS_TABSTOP ; `from` = 0 donne le
/// premier.
pub fn next_tab_item(dlg: usize, from: usize, previous: bool) -> usize {
    let list = items(dlg);
    let n = list.len();
    if n == 0 {
        return 0;
    }
    let start = list.iter().position(|&h| h == from);
    for k in 1..=n {
        let i = match (start, previous) {
            (None, false) => (k - 1) % n,
            (None, true) => (n - k) % n,
            (Some(s), false) => (s + k) % n,
            (Some(s), true) => (s + n - k) % n,
        };
        let h = list[i];
        if gui::window_style(h).unwrap_or(0) & WS_TABSTOP != 0 && usable(h) {
            return h;
        }
    }
    0
}

/// Contrôle suivant (ou précédent) du même groupe (entre deux WS_GROUP).
pub fn next_group_item(dlg: usize, from: usize, previous: bool) -> usize {
    let list = items(dlg);
    let Some(me) = list.iter().position(|&h| h == from) else {
        return 0;
    };
    let group = |h: usize| gui::window_style(h).unwrap_or(0) & WS_GROUP != 0;
    let mut start = me;
    while start > 0 && !group(list[start]) {
        start -= 1;
    }
    let mut end = me + 1;
    while end < list.len() && !group(list[end]) {
        end += 1;
    }
    let members: Vec<usize> = list[start..end].to_vec();
    let n = members.len();
    let pos = me - start;
    for k in 1..=n {
        let i = if previous {
            (pos + n - k) % n
        } else {
            (pos + k) % n
        };
        if usable(members[i]) {
            return members[i];
        }
    }
    from
}

unsafe fn set_dlg_focus(dlg: usize, target: usize) {
    if let Some(d) = lock(dialogs()).get_mut(&dlg) {
        d.focus = target;
    }
    user32::set_focus(target);
    // Un champ reçoit tout son texte sélectionné, comme sous Windows.
    if dlg_code(target, None) & DLGC_HASSETSEL != 0 {
        user32::send_message_w(target, 0x00B1, 0, -1);
    }
}

/// SetFocus sur un contrôle : son dialogue s'en souvient, pour le lui
/// rendre quand le focus revient au dialogue. Sans cela, un clic sur « Non »
/// donnait le focus au bouton, puis le retour du focus X au dialogue le
/// rendait à « Oui » et le bouton perdait son appui (#709).
pub fn note_focus(hwnd: usize) {
    let mut h = gui::window_parent(hwnd).unwrap_or(0);
    let mut guard = 0;
    while h != 0 && guard < 64 {
        if let Some(d) = lock(dialogs()).get_mut(&h) {
            d.focus = hwnd;
            return;
        }
        h = gui::window_parent(h).unwrap_or(0);
        guard += 1;
    }
}

/// Le dialogue qui contient `hwnd` (lui-même ou un ancêtre).
fn owning_dialog(dlg: usize, hwnd: usize) -> bool {
    let mut h = hwnd;
    let mut guard = 0;
    while h != 0 && guard < 64 {
        if h == dlg {
            return true;
        }
        h = gui::window_parent(h).unwrap_or(0);
        guard += 1;
    }
    false
}

/// Contrôle direct du dialogue qui contient `hwnd`.
fn direct_child(dlg: usize, mut hwnd: usize) -> usize {
    let mut guard = 0;
    while hwnd != 0 && guard < 64 {
        let p = gui::window_parent(hwnd).unwrap_or(0);
        if p == dlg {
            return hwnd;
        }
        hwnd = p;
        guard += 1;
    }
    0
}

/// Lettre soulignée (« &Ouvrir » → 'o') d'un texte de contrôle.
fn mnemonic(text: &str) -> Option<char> {
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '&' {
            match chars.next() {
                Some('&') => continue,
                Some(m) => return m.to_lowercase().next(),
                None => return None,
            }
        }
    }
    None
}

unsafe fn press_button(dlg: usize, id: usize) {
    let h = user32::get_dlg_item(dlg, id as i32);
    if h != 0 && !usable(h) {
        return;
    }
    user32::send_message_w(dlg, WM_COMMAND, id & 0xFFFF, h as isize);
}

/// IsDialogMessage : traite les touches de navigation pour `dlg` et
/// distribue les autres messages de ses fenêtres. Vrai si le message lui
/// appartenait.
pub unsafe fn is_dialog_message(dlg: usize, m: &Msg) -> bool {
    if dlg == 0 || !gui::is_window(dlg) || !owning_dialog(dlg, m.hwnd) {
        return false;
    }
    let focus = gui::focused_window();
    let code = if focus != 0 && owning_dialog(dlg, focus) {
        dlg_code(focus, Some(m))
    } else {
        0
    };
    let current = direct_child(dlg, focus);
    match m.message {
        WM_KEYDOWN if code & DLGC_WANTMESSAGE == 0 => match m.wparam {
            0x09 if code & DLGC_WANTTAB == 0 => {
                let shift = gui::get_key_state(0x10) < 0;
                let next = next_tab_item(dlg, current, shift);
                if next != 0 {
                    set_dlg_focus(dlg, next);
                }
                return true;
            }
            0x25..=0x28 if code & DLGC_WANTARROWS == 0 && current != 0 => {
                let previous = matches!(m.wparam, 0x25 | 0x26);
                let next = next_group_item(dlg, current, previous);
                if next != 0 && next != current {
                    user32::set_focus(next);
                    if let Some(d) = lock(dialogs()).get_mut(&dlg) {
                        d.focus = next;
                    }
                    if dlg_code(next, None) & DLGC_RADIOBUTTON != 0 {
                        user32::send_message_w(next, 0x00F5, 0, 0); // BM_CLICK
                    }
                }
                return true;
            }
            0x0D => {
                let fc = dlg_code(focus, None);
                if fc & (DLGC_DEFPUSHBUTTON | DLGC_UNDEFPUSHBUTTON) != 0 {
                    press_button(dlg, user32::control_id(focus) as usize);
                } else {
                    let def = user32::send_message_w(dlg, DM_GETDEFID, 0, 0);
                    let id = if (def >> 16) as u16 == 0x534B {
                        (def & 0xFFFF) as usize
                    } else {
                        IDOK
                    };
                    press_button(dlg, id);
                }
                return true;
            }
            0x1B => {
                press_button(dlg, IDCANCEL);
                return true;
            }
            _ => {}
        },
        WM_CHAR if code & (DLGC_WANTMESSAGE | 0x80) == 0 => {
            // Lettre seule sur un bouton ou une case : raccourci « &x ».
            if mnemonic_activate(dlg, m.wparam as u32) {
                return true;
            }
        }
        WM_SYSCHAR if mnemonic_activate(dlg, m.wparam as u32) => return true,
        _ => {}
    }
    user32::translate_message(m);
    user32::dispatch_message_w(m);
    true
}

/// Alt+lettre : active le contrôle dont le texte souligne cette lettre (un
/// texte statique donne le focus au contrôle qui le suit).
unsafe fn mnemonic_activate(dlg: usize, ch: u32) -> bool {
    let Some(c) = char::from_u32(ch).and_then(|c| c.to_lowercase().next()) else {
        return false;
    };
    let list = items(dlg);
    for (i, &h) in list.iter().enumerate() {
        if !usable(h) || controls::kind_of(h).is_none() {
            continue;
        }
        let text = gui::title_of(h).unwrap_or_default();
        if mnemonic(&text) != Some(c) {
            continue;
        }
        let code = dlg_code(h, None);
        if code & DLGC_STATIC != 0 {
            if let Some(&next) = list[i + 1..]
                .iter()
                .find(|&&n| gui::window_style(n).unwrap_or(0) & WS_TABSTOP != 0 && usable(n))
            {
                set_dlg_focus(dlg, next);
            }
        } else if code & DLGC_BUTTON != 0 {
            user32::set_focus(h);
            user32::send_message_w(h, 0x00F5, 0, 0);
        } else {
            set_dlg_focus(dlg, h);
        }
        return true;
    }
    false
}

// ---------- Boucle modale ----------

/// Fait tourner la boucle de messages tant que le dialogue n'est pas fermé.
unsafe fn run_modal(dlg: usize, owner: usize) -> isize {
    let owner_enabled = owner != 0 && user32::is_window_enabled(owner) != 0;
    if owner_enabled {
        user32::enable_window(owner, 0);
    }
    user32::show_window(dlg, 5);
    let interactive = gui::backend_is_interactive();
    let mut msg: Msg = std::mem::zeroed();
    loop {
        let ended = lock(dialogs()).get(&dlg).is_none_or(|d| d.ended);
        if ended || !gui::is_window(dlg) {
            break;
        }
        if !interactive {
            // Sans affichage, personne ne fermera la boîte : une fois les
            // messages en attente traités, elle est annulée.
            if user32::peek_message_w(&mut msg, 0, 0, 0, 1) == 0 {
                if let Some(d) = lock(dialogs()).get_mut(&dlg) {
                    d.ended = true;
                    d.result = IDCANCEL as isize;
                }
                break;
            }
        } else {
            let r = user32::get_message_w(&mut msg, 0, 0, 0);
            if r <= 0 {
                // WM_QUIT : remis en file pour la boucle principale.
                if r == 0 {
                    user32::post_quit_message(msg.wparam as i32);
                }
                break;
            }
        }
        if msg.message == WM_QUIT {
            user32::post_quit_message(msg.wparam as i32);
            break;
        }
        if !is_dialog_message(dlg, &msg) {
            user32::translate_message(&msg);
            user32::dispatch_message_w(&msg);
        }
    }
    let result = lock(dialogs())
        .get(&dlg)
        .map_or(IDCANCEL as isize, |d| d.result);
    if owner_enabled {
        user32::enable_window(owner, 1);
    }
    if gui::is_window(dlg) {
        gui::destroy_window(dlg);
    }
    lock(dialogs()).remove(&dlg);
    if owner != 0 && gui::is_window(owner) {
        user32::set_focus(owner);
    }
    result
}

// ---------- Fonctions exportées ----------

unsafe fn resource_template(instance: usize, name: usize, wide: bool) -> Option<&'static [u8]> {
    let hrsrc = if wide {
        super::resource::find_resource_w(instance, name as *const u16, 5 as *const u16)
    } else {
        super::resource::find_resource_a(instance, name as *const u8, 5 as *const u8)
    };
    if hrsrc == 0 {
        return None;
    }
    let size = super::resource::sizeof_resource(instance, hrsrc) as usize;
    let ptr = super::resource::load_resource(instance, hrsrc);
    (ptr != 0).then(|| std::slice::from_raw_parts(ptr as *const u8, size))
}

unsafe fn create_indirect(
    instance: usize,
    data: &[u8],
    parent: usize,
    proc_addr: usize,
    init: isize,
    wide: bool,
) -> usize {
    match parse_template(data) {
        Some(t) => {
            crate::trace!(
                "boîte de dialogue : {} contrôle(s), classe {:?}",
                t.items.len(),
                class_name(&t.class)
            );
            create_from_template(&t, instance, parent, proc_addr, init, wide)
        }
        None => {
            crate::trace!("boîte de dialogue : modèle illisible, création refusée");
            crate::process::set_last_error(1812); // ERROR_RESOURCE_DATA_NOT_FOUND
            0
        }
    }
}

unsafe fn dialog_box(
    instance: usize,
    data: Option<&[u8]>,
    parent: usize,
    proc_addr: usize,
    init: isize,
    wide: bool,
) -> isize {
    let Some(data) = data else {
        crate::process::set_last_error(1814); // ERROR_RESOURCE_NAME_NOT_FOUND
        return -1;
    };
    let owner = if parent != 0 { top_level(parent) } else { 0 };
    let dlg = create_indirect(instance, data, owner, proc_addr, init, wide);
    if dlg == 0 {
        return -1;
    }
    run_modal(dlg, owner)
}

pub unsafe extern "win64" fn dialog_box_param_a(
    i: usize,
    t: *const u8,
    p: usize,
    f: usize,
    l: isize,
) -> isize {
    dialog_box(i, resource_template(i, t as usize, false), p, f, l, false)
}
pub unsafe extern "win64" fn dialog_box_param_w(
    i: usize,
    t: *const u16,
    p: usize,
    f: usize,
    l: isize,
) -> isize {
    dialog_box(i, resource_template(i, t as usize, true), p, f, l, true)
}
pub unsafe extern "win64" fn dialog_box_indirect_param_a(
    i: usize,
    t: *const u8,
    p: usize,
    f: usize,
    l: isize,
) -> isize {
    let data = (!t.is_null()).then(|| template_bytes(t));
    dialog_box(i, data, p, f, l, false)
}
pub unsafe extern "win64" fn dialog_box_indirect_param_w(
    i: usize,
    t: *const u8,
    p: usize,
    f: usize,
    l: isize,
) -> isize {
    let data = (!t.is_null()).then(|| template_bytes(t));
    dialog_box(i, data, p, f, l, true)
}
pub unsafe extern "win64" fn create_dialog_param_a(
    i: usize,
    t: *const u8,
    p: usize,
    f: usize,
    l: isize,
) -> usize {
    match resource_template(i, t as usize, false) {
        Some(data) => create_indirect(i, data, p, f, l, false),
        None => 0,
    }
}
pub unsafe extern "win64" fn create_dialog_param_w(
    i: usize,
    t: *const u16,
    p: usize,
    f: usize,
    l: isize,
) -> usize {
    match resource_template(i, t as usize, true) {
        Some(data) => create_indirect(i, data, p, f, l, true),
        None => 0,
    }
}
pub unsafe extern "win64" fn create_dialog_indirect_param_a(
    i: usize,
    t: *const u8,
    p: usize,
    f: usize,
    l: isize,
) -> usize {
    if t.is_null() {
        return 0;
    }
    create_indirect(i, template_bytes(t), p, f, l, false)
}
pub unsafe extern "win64" fn create_dialog_indirect_param_w(
    i: usize,
    t: *const u8,
    p: usize,
    f: usize,
    l: isize,
) -> usize {
    if t.is_null() {
        return 0;
    }
    create_indirect(i, template_bytes(t), p, f, l, true)
}

pub unsafe extern "win64" fn end_dialog(dlg: usize, result: isize) -> i32 {
    let mut map = lock(dialogs());
    let Some(d) = map.get_mut(&dlg) else {
        return 0;
    };
    d.ended = true;
    d.result = result;
    drop(map);
    // La boîte disparaît tout de suite ; la boucle modale la détruit ensuite.
    user32::show_window(dlg, 0);
    user32::post_message_w(dlg, 0, 0, 0); // WM_NULL : réveille la boucle
    1
}

pub unsafe extern "win64" fn is_dialog_message_a(dlg: usize, msg: *const Msg) -> i32 {
    if msg.is_null() {
        return 0;
    }
    controls::with_charset(false, || is_dialog_message(dlg, &*msg)) as i32
}
pub unsafe extern "win64" fn is_dialog_message_w(dlg: usize, msg: *const Msg) -> i32 {
    if msg.is_null() {
        return 0;
    }
    controls::with_charset(true, || is_dialog_message(dlg, &*msg)) as i32
}

pub unsafe extern "win64" fn def_dlg_proc_a(h: usize, m: u32, w: usize, l: isize) -> isize {
    controls::with_charset(false, || def_dlg_proc(h, m, w, l))
}
pub unsafe extern "win64" fn def_dlg_proc_w(h: usize, m: u32, w: usize, l: isize) -> isize {
    controls::with_charset(true, || def_dlg_proc(h, m, w, l))
}

pub extern "win64" fn get_next_dlg_tab_item(dlg: usize, ctl: usize, previous: i32) -> usize {
    next_tab_item(dlg, ctl, previous != 0)
}
pub extern "win64" fn get_next_dlg_group_item(dlg: usize, ctl: usize, previous: i32) -> usize {
    next_group_item(dlg, ctl, previous != 0)
}

pub unsafe extern "win64" fn map_dialog_rect(dlg: usize, rect: *mut Rect) -> i32 {
    if rect.is_null() {
        return 0;
    }
    let font = lock(dialogs()).get(&dlg).map_or(0, |d| d.font);
    let base = if font != 0 {
        base_units(font)
    } else {
        (
            super::win32_basics::DIALOG_BASE_X,
            super::win32_basics::DIALOG_BASE_Y,
        )
    };
    let r = &mut *rect;
    r.left = r.left * base.0 / 4;
    r.right = r.right * base.0 / 4;
    r.top = r.top * base.1 / 8;
    r.bottom = r.bottom * base.1 / 8;
    1
}

// ---------- MessageBox ----------

/// Langue de la session (deux lettres : fr, de, es, en…).
pub fn session_lang() -> String {
    let lang = std::env::var("LC_ALL")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var("LC_MESSAGES").ok().filter(|v| !v.is_empty()))
        .or_else(|| std::env::var("LANG").ok())
        .unwrap_or_default();
    lang.get(..2).unwrap_or("en").to_ascii_lowercase()
}

/// Libellés des boutons dans la langue de la session.
pub(crate) fn labels() -> [&'static str; 9] {
    // OK, Annuler, Abandonner, Réessayer, Ignorer, Oui, Non, Recommencer, Continuer
    match session_lang().as_str() {
        "fr" => [
            "OK",
            "Annuler",
            "Abandonner",
            "Réessayer",
            "Ignorer",
            "Oui",
            "Non",
            "Recommencer",
            "Continuer",
        ],
        "de" => [
            "OK",
            "Abbrechen",
            "Abbrechen",
            "Wiederholen",
            "Ignorieren",
            "Ja",
            "Nein",
            "Wiederholen",
            "Weiter",
        ],
        "es" => [
            "Aceptar",
            "Cancelar",
            "Anular",
            "Reintentar",
            "Omitir",
            "Sí",
            "No",
            "Reintentar",
            "Continuar",
        ],
        _ => [
            "OK",
            "Cancel",
            "Abort",
            "Retry",
            "Ignore",
            "Yes",
            "No",
            "Try Again",
            "Continue",
        ],
    }
}

/// Boutons (identifiant, libellé) d'un type MB_*.
fn message_box_buttons(kind: u32) -> Vec<(usize, &'static str)> {
    let l = labels();
    match kind & 0xF {
        1 => vec![(1, l[0]), (2, l[1])],
        2 => vec![(3, l[2]), (4, l[3]), (5, l[4])],
        3 => vec![(6, l[5]), (7, l[6]), (2, l[1])],
        4 => vec![(6, l[5]), (7, l[6])],
        5 => vec![(4, l[3]), (2, l[1])],
        6 => vec![(2, l[1]), (10, l[7]), (11, l[8])],
        _ => vec![(1, l[0])],
    }
}

/// Fins de ligne d'une boîte de message : CR LF, CR seul et LF deviennent LF.
fn message_box_lines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Zone de travail de l'écran où s'ouvre la boîte : celui qui contient le
/// centre de la fenêtre propriétaire, sinon l'écran principal, limité à la
/// zone de travail publiée (barre des tâches exclue). Une boîte longue doit
/// tenir sur un portable en 1366 x 768 comme sur un écran 1920 x 1080 (#1161).
fn message_box_area(owner: usize) -> Rect {
    let work = gui::work_area();
    let monitors = gui::monitors();
    let center = gui::screen_rect(owner).map(|r| ((r.left + r.right) / 2, (r.top + r.bottom) / 2));
    let monitor = center
        .and_then(|(x, y)| {
            monitors.iter().find(|m| {
                (m.rect.left..m.rect.right).contains(&x) && (m.rect.top..m.rect.bottom).contains(&y)
            })
        })
        .or_else(|| monitors.iter().find(|m| m.primary))
        .or(monitors.first());
    let Some(m) = monitor else {
        return work;
    };
    clip_to_work_area(m.rect, work)
}

/// Écran `monitor` privé de ce que la zone de travail en exclut ; l'écran
/// entier si les deux ne se recouvrent pas.
fn clip_to_work_area(monitor: Rect, work: Rect) -> Rect {
    let r = Rect {
        left: monitor.left.max(work.left),
        top: monitor.top.max(work.top),
        right: monitor.right.min(work.right),
        bottom: monitor.bottom.min(work.bottom),
    };
    if r.width() > 0 && r.height() > 0 {
        r
    } else {
        monitor
    }
}

/// Nombre de lignes et largeur de la plus longue d'un texte de boîte de
/// message coupé aux espaces à `max_w` pixels. Un mot plus long que la
/// ligne en occupe une à lui seul.
fn message_box_wrap(text: &str, max_w: i32, measure: &dyn Fn(&str) -> i32) -> (i32, i32) {
    let space = measure(" ");
    let mut lines = 0;
    let mut widest = 0;
    for raw in text.split('\n') {
        let mut cur = 0;
        let mut first = true;
        for word in raw.trim_end_matches('\r').split(' ') {
            let w = measure(word);
            if first {
                cur = w;
                first = false;
            } else if cur + space + w <= max_w {
                cur += space + w;
            } else {
                lines += 1;
                widest = widest.max(cur.min(max_w));
                cur = w;
            }
        }
        lines += 1;
        widest = widest.max(cur.min(max_w));
    }
    (lines, widest)
}

/// Construit un DLGTEMPLATE en mémoire pour une boîte de message.
fn message_box_template(text: &str, caption: &str, kind: u32, area: Rect) -> Vec<u8> {
    let font = super::gdi32::get_stock_object(17);
    let spec = gui::font_spec(font).unwrap_or_default();
    let base = base_units(font);
    let to_du_x = |px: i32| (px * 4 + base.0 - 1) / base.0;
    let to_du_y = |px: i32| (px * 8 + base.1 - 1) / base.1;
    let icon = if kind & 0xF0 != 0 { 40 } else { 0 };
    // Texte coupé à 420 pixels pour un message court ; un long texte (pile
    // d'appels de Paint.NET) élargit la boîte jusqu'aux 5/8 de la zone de
    // travail, comme sous Windows, au lieu de s'allonger sans fin (#1161).
    let measure = |s: &str| gui::measure_text(&spec, s).0;
    let mut max_w = 420;
    let (mut lines, mut widest) = message_box_wrap(text, max_w, &measure);
    let wide = (area.width() * 5 / 8 - 48 - icon).max(420);
    let lh = gui::measure_text(&spec, "Ag").1.max(12);
    if lines * lh > area.height() / 2 && wide > max_w {
        max_w = wide;
        (lines, widest) = message_box_wrap(text, max_w, &measure);
    }
    let buttons = message_box_buttons(kind);
    let bw = 88;
    let gap = 8;
    let row = buttons.len() as i32 * (bw + gap) - gap;
    let width = (widest + icon).max(row).max(200) + 48;
    // La boîte tient dans la zone de travail (barre de titre comprise) : le
    // texte trop long est coupé, les boutons restent visibles.
    let room = (area.height() - 40 - (24 + 24 + 32 + 16)).max(lh);
    let text_h = (lines * lh).min(room);
    let height = 24 + text_h + 24 + 32 + 16;

    let mut out: Vec<u8> = Vec::new();
    let push16 = |o: &mut Vec<u8>, v: u16| o.extend_from_slice(&v.to_le_bytes());
    let push32 = |o: &mut Vec<u8>, v: u32| o.extend_from_slice(&v.to_le_bytes());
    let pushs = |o: &mut Vec<u8>, s: &str| {
        for u in s.encode_utf16() {
            o.extend_from_slice(&u.to_le_bytes());
        }
        o.extend_from_slice(&[0, 0]);
    };
    let align = |o: &mut Vec<u8>| {
        while !o.len().is_multiple_of(4) {
            o.push(0);
        }
    };
    // 0x80C80000 = WS_POPUP | WS_CAPTION | WS_SYSMENU, DS_CENTER | DS_MODALFRAME.
    push32(&mut out, 0x80C8_0000 | DS_CENTER | 0x80);
    push32(&mut out, 0); // pas de style étendu
    push16(&mut out, (buttons.len() + 1) as u16);
    for v in [0, 0, to_du_x(width), to_du_y(height)] {
        push16(&mut out, v as i16 as u16);
    }
    push16(&mut out, 0); // pas de menu
    push16(&mut out, 0); // classe par défaut
    pushs(&mut out, caption);
    // Texte.
    align(&mut out);
    push32(&mut out, 0x5000_0000 | 0x80); // WS_CHILD | WS_VISIBLE | SS_NOPREFIX
    push32(&mut out, 0);
    for v in [
        to_du_x(24 + icon),
        to_du_y(24),
        to_du_x(width - 48 - icon),
        to_du_y(text_h + 4),
    ] {
        push16(&mut out, v as i16 as u16);
    }
    push16(&mut out, 0xFFFF); // id -1
    push16(&mut out, 0xFFFF);
    push16(&mut out, 0x0082); // STATIC
    pushs(&mut out, text);
    push16(&mut out, 0);
    // Boutons, alignés à droite.
    let mut x = width - 24 - row;
    let y = height - 16 - 32 + 4;
    for (i, (id, label)) in buttons.iter().enumerate() {
        align(&mut out);
        let default = i == ((kind >> 8) & 0xF) as usize;
        push32(
            &mut out,
            0x5001_0000 | default as u32 | if i == 0 { WS_GROUP } else { 0 },
        );
        push32(&mut out, 0);
        for v in [to_du_x(x), to_du_y(y), to_du_x(bw), to_du_y(28)] {
            push16(&mut out, v as i16 as u16);
        }
        push16(&mut out, *id as u16);
        push16(&mut out, 0xFFFF);
        push16(&mut out, 0x0080); // BUTTON
        pushs(&mut out, label);
        push16(&mut out, 0);
        x += bw + gap;
    }
    out
}

/// Boîte modale propre à Winlux (boîtes communes de comdlg32) : le modèle ne
/// décrit que le cadre, `w` × `h` pixels de zone cliente ; la procédure crée
/// ses contrôles en pixels pendant `WM_INITDIALOG` (voir [`add_control`]).
pub unsafe fn run_builtin(
    owner: usize,
    caption: &str,
    w: i32,
    h: i32,
    proc_addr: usize,
    init: isize,
) -> isize {
    let out = builtin_template(caption, w, h, 0);
    dialog_box(0, Some(&out), owner, proc_addr, init, true)
}

/// Boîte non modale propre à Winlux (Rechercher, Remplacer) : même cadre que
/// [`run_builtin`], visible dès sa création, rendue sans attendre. Le
/// programme la fait vivre dans sa propre boucle de messages.
pub unsafe fn create_builtin(
    owner: usize,
    caption: &str,
    w: i32,
    h: i32,
    proc_addr: usize,
    init: isize,
) -> usize {
    let out = builtin_template(caption, w, h, 0x1000_0000); // WS_VISIBLE
    let owner = if owner != 0 { top_level(owner) } else { 0 };
    create_indirect(0, &out, owner, proc_addr, init, true)
}

/// DLGTEMPLATE sans élément : cadre de `w` × `h` pixels de zone cliente.
fn builtin_template(caption: &str, w: i32, h: i32, extra_style: u32) -> Vec<u8> {
    let base = base_units(super::gdi32::get_stock_object(17));
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&(0x80C8_0000 | DS_CENTER | 0x80 | extra_style).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // aucun élément
    let du_w = (w * 4 + base.0 - 1) / base.0;
    let du_h = (h * 8 + base.1 - 1) / base.1;
    for v in [0, 0, du_w, du_h] {
        out.extend_from_slice(&(v as i16 as u16).to_le_bytes());
    }
    out.extend_from_slice(&[0, 0, 0, 0]); // pas de menu, classe par défaut
    for u in caption.encode_utf16().chain([0]) {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

/// Contrôle enfant d'une boîte de Winlux, placé en pixels, avec la police
/// de la boîte. `WS_CHILD | WS_VISIBLE` sont ajoutés.
pub unsafe fn add_control(
    dlg: usize,
    class: &str,
    text: &str,
    id: usize,
    style: u32,
    ex_style: u32,
    rect: (i32, i32, i32, i32),
) -> usize {
    let class = wide_str(class);
    let text = wide_str(text);
    let hwnd = user32::create_window_ex_w(
        ex_style,
        class.as_ptr(),
        text.as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        rect.0,
        rect.1,
        rect.2,
        rect.3,
        dlg,
        id,
        0,
        std::ptr::null_mut(),
    );
    let font = user32::send_message_w(dlg, WM_GETFONT, 0, 0);
    if hwnd != 0 && font != 0 {
        user32::send_message_w(hwnd, WM_SETFONT, font as usize, 0);
    }
    hwnd
}

unsafe extern "win64" fn message_box_proc(hwnd: usize, msg: u32, wp: usize, _lp: isize) -> isize {
    match msg {
        WM_INITDIALOG => 1,
        WM_COMMAND => {
            let id = wp & 0xFFFF;
            // Échap sans bouton Annuler : seulement pour une boîte « OK ».
            let has = user32::get_dlg_item(hwnd, id as i32) != 0;
            if has {
                end_dialog(hwnd, id as isize);
            } else if id == IDCANCEL
                && user32::get_dlg_item(hwnd, IDOK as i32) != 0
                && items(hwnd).len() == 2
            {
                end_dialog(hwnd, IDOK as isize);
            }
            1
        }
        _ => 0,
    }
}

/// MessageBox : vraie fenêtre modale quand un affichage est là ; sinon le
/// texte va dans le journal et le premier bouton est choisi.
pub unsafe fn message_box(owner: usize, text: &str, caption: &str, kind: u32) -> i32 {
    // Un retour chariot seul coupe la ligne comme CR LF (Notepad++ écrit
    // « \r\r » dans sa boîte « Recharger »).
    let text = &message_box_lines(text);
    let caption = if caption.is_empty() {
        "Erreur"
    } else {
        caption
    };
    if !gui::backend_is_interactive() {
        eprintln!("winlux: MessageBox [{caption}] {text}");
        let buttons = message_box_buttons(kind);
        let default = ((kind >> 8) & 0xF) as usize;
        return buttons.get(default).or(buttons.first()).map_or(1, |b| b.0) as i32;
    }
    let owner = if owner != 0 { top_level(owner) } else { 0 };
    let data = message_box_template(text, caption, kind, message_box_area(owner));
    let dlg = create_indirect(
        0,
        &data,
        owner,
        message_box_proc as *const () as usize,
        0,
        true,
    );
    if dlg == 0 {
        return 0;
    }
    run_modal(dlg, owner) as i32
}

pub unsafe extern "win64" fn message_box_a(
    owner: usize,
    text: *const u8,
    caption: *const u8,
    kind: u32,
) -> i32 {
    let t = if text.is_null() {
        String::new()
    } else {
        super::cstr(text)
    };
    let c = if caption.is_null() {
        String::new()
    } else {
        super::cstr(caption)
    };
    message_box(owner, &t, &c, kind)
}
pub unsafe extern "win64" fn message_box_w(
    owner: usize,
    text: *const u16,
    caption: *const u16,
    kind: u32,
) -> i32 {
    let t = if text.is_null() {
        String::new()
    } else {
        super::wstr(text)
    };
    let c = if caption.is_null() {
        String::new()
    } else {
        super::wstr(caption)
    };
    message_box(owner, &t, &c, kind)
}
pub unsafe extern "win64" fn message_box_ex_a(
    o: usize,
    t: *const u8,
    c: *const u8,
    k: u32,
    _lang: u16,
) -> i32 {
    message_box_a(o, t, c, k)
}
pub unsafe extern "win64" fn message_box_ex_w(
    o: usize,
    t: *const u16,
    c: *const u16,
    k: u32,
    _lang: u16,
) -> i32 {
    message_box_w(o, t, c, k)
}

/// Exports de user32 qui remplacent les versions vides.
pub fn user32_overrides() -> Exports {
    crate::exports! {
        "DialogBoxParamA" => fn dialog_box_param_a,
        "DialogBoxParamW" => fn dialog_box_param_w,
        "DialogBoxIndirectParamA" => fn dialog_box_indirect_param_a,
        "DialogBoxIndirectParamW" => fn dialog_box_indirect_param_w,
        "CreateDialogParamA" => fn create_dialog_param_a,
        "CreateDialogParamW" => fn create_dialog_param_w,
        "CreateDialogIndirectParamA" => fn create_dialog_indirect_param_a,
        "CreateDialogIndirectParamW" => fn create_dialog_indirect_param_w,
        "EndDialog" => fn end_dialog,
        "IsDialogMessageA" => fn is_dialog_message_a,
        "IsDialogMessageW" => fn is_dialog_message_w,
        "IsDialogMessage" => fn is_dialog_message_a,
        "DefDlgProcA" => fn def_dlg_proc_a,
        "DefDlgProcW" => fn def_dlg_proc_w,
        "GetNextDlgTabItem" => fn get_next_dlg_tab_item,
        "GetNextDlgGroupItem" => fn get_next_dlg_group_item,
        "MapDialogRect" => fn map_dialog_rect,
        "MessageBoxA" => fn message_box_a,
        "MessageBoxW" => fn message_box_w,
        "MessageBoxExA" => fn message_box_ex_a,
        "MessageBoxExW" => fn message_box_ex_w,
    }
}

#[cfg(test)]
pub(crate) mod tests;
