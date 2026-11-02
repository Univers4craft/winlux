//! `comdlg32` : outils communs des boîtes de Winlux, et boîtes « Couleur »
//! (`ChooseColor`) et « Police » (`ChooseFont`).
//!
//! Sans affichage : `WINLUX_CHOOSECOLOR=RRGGBB` choisit cette couleur et
//! `WINLUX_CHOOSEFONT=<police>,<taille>[,gras][,italique]` cette police ;
//! sans variable, la boîte est annulée.

use super::{dialogs, gdi32, gui, user32};
use std::cell::RefCell;

pub(crate) const CDERR_STRUCTSIZE: u32 = 0x0001;
pub(crate) const CDERR_INITIALIZATION: u32 = 0x0002;

pub(crate) fn set_error(code: u32) {
    super::comdlg32::set_extended_error(code);
}

pub(crate) fn lang() -> String {
    dialogs::session_lang()
}

/// Texte dans la langue de la session (français, anglais, allemand, espagnol).
pub(crate) fn tr(
    fr: &'static str,
    en: &'static str,
    de: &'static str,
    es: &'static str,
) -> &'static str {
    match lang().as_str() {
        "fr" => fr,
        "de" => de,
        "es" => es,
        _ => en,
    }
}

pub(crate) fn cancel_label() -> &'static str {
    tr("Annuler", "Cancel", "Abbrechen", "Cancelar")
}

pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

pub(crate) fn send(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    unsafe { user32::send_message_w(hwnd, msg, wp, lp) }
}

pub(crate) fn set_text(hwnd: usize, text: &str) {
    let t = wide(text);
    unsafe {
        user32::set_window_text_w(hwnd, t.as_ptr());
    }
}

pub(crate) fn get_text(hwnd: usize) -> String {
    let n = unsafe { user32::get_window_text_length_w(hwnd) }.max(0) as usize;
    let mut buf = vec![0u16; n + 1];
    let got = unsafe { user32::get_window_text_w(hwnd, buf.as_mut_ptr(), (n + 1) as i32) };
    String::from_utf16_lossy(&buf[..got.max(0) as usize])
}

pub(crate) unsafe fn rd<T: Copy>(base: *mut u8, off: usize) -> T {
    std::ptr::read_unaligned(base.add(off) as *const T)
}

pub(crate) unsafe fn wr<T: Copy>(base: *mut u8, off: usize, v: T) {
    std::ptr::write_unaligned(base.add(off) as *mut T, v)
}

/// Chaîne facultative (pointeur nul ou chaîne vide → `None`).
pub(crate) unsafe fn read_opt(ptr: usize, wide: bool) -> Option<String> {
    if ptr == 0 {
        return None;
    }
    let s = if wide {
        super::wstr(ptr as *const u16)
    } else {
        super::cstr(ptr as *const u8)
    };
    (!s.is_empty()).then_some(s)
}

#[repr(C)]
#[derive(Default)]
pub(crate) struct LvItemW {
    pub mask: u32,
    pub item: i32,
    pub sub_item: i32,
    pub state: u32,
    pub state_mask: u32,
    pub text: usize,
    pub text_max: i32,
    pub image: i32,
    pub param: isize,
}

#[repr(C)]
#[derive(Default)]
pub(crate) struct LvColumnW {
    pub mask: u32,
    pub fmt: i32,
    pub cx: i32,
    pub text: usize,
    pub text_max: i32,
    pub sub_item: i32,
}

#[repr(C)]
struct DrawItem {
    ctl_type: u32,
    ctl_id: u32,
    item_id: u32,
    item_action: u32,
    item_state: u32,
    hwnd_item: usize,
    hdc: usize,
    rc: gui::Rect,
    item_data: usize,
}

unsafe fn fill(hdc: usize, r: gui::Rect, color: u32) {
    let b = gdi32::create_solid_brush(color);
    user32::fill_rect(hdc, &r, b);
    gdi32::delete_object(b);
}

/// Cadre de `width` pixels autour de `r`.
unsafe fn frame(hdc: usize, r: gui::Rect, color: u32, width: i32) {
    for (l, t, rr, b) in [
        (r.left, r.top, r.right, r.top + width),
        (r.left, r.bottom - width, r.right, r.bottom),
        (r.left, r.top, r.left + width, r.bottom),
        (r.right - width, r.top, r.right, r.bottom),
    ] {
        fill(
            hdc,
            gui::Rect {
                left: l,
                top: t,
                right: rr,
                bottom: b,
            },
            color,
        );
    }
}

// =====================================================================
// ChooseColor
// =====================================================================

/// Les 48 couleurs de base de la boîte de Windows (COLORREF 0x00BBGGRR).
const BASIC: [u32; 48] = [
    0x8080FF, 0x80FFFF, 0x80FF80, 0x80FF00, 0xFFFF80, 0xFF8000, 0xC080FF, 0xFF80FF, //
    0x0000FF, 0x00FFFF, 0x00FF80, 0x40FF00, 0xFFFF00, 0xC08000, 0xC08080, 0xFF00FF, //
    0x404080, 0x4080FF, 0x00FF00, 0x808000, 0x804000, 0xFF8080, 0x400080, 0x8000FF, //
    0x000080, 0x0080FF, 0x008000, 0x408000, 0xFF0000, 0xA00000, 0x800080, 0xFF0080, //
    0x000040, 0x004080, 0x004000, 0x404000, 0x800000, 0x400000, 0x400040, 0x800040, //
    0x000000, 0x008080, 0x408080, 0x808080, 0x808040, 0xC0C0C0, 0x400040, 0xFFFFFF,
];

const ID_BASIC: usize = 0x2C0 + 0x100; // 0x3C0..0x3EF
const ID_CUSTOM: usize = 0x3F0; // 0x3F0..0x3FF
const ID_PREVIEW: usize = 0x2C5;
const ID_RED: usize = 0x2C2;
const ID_GREEN: usize = 0x2C3;
const ID_BLUE: usize = 0x2C4;
const ID_HEX: usize = 0x2D2;
const ID_ADD: usize = 0x2C8;

#[derive(Default)]
struct ColorState {
    color: u32,
    custom: [u32; 16],
    custom_slot: usize,
    brush: usize,
    updating: bool,
}

thread_local! {
    static COLOR: RefCell<Option<ColorState>> = const { RefCell::new(None) };
}

fn with_color<R>(f: impl FnOnce(&mut ColorState) -> R) -> Option<R> {
    COLOR.with(|s| s.borrow_mut().as_mut().map(f))
}

pub(crate) fn parse_hex(text: &str) -> Option<u32> {
    let t = text.trim().trim_start_matches('#');
    if t.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(t, 16).ok()?;
    // RRGGBB → COLORREF 0x00BBGGRR
    Some(((v & 0xFF) << 16) | (v & 0xFF00) | ((v >> 16) & 0xFF))
}

pub(crate) fn hex_text(c: u32) -> String {
    format!(
        "{:02X}{:02X}{:02X}",
        c & 0xFF,
        (c >> 8) & 0xFF,
        (c >> 16) & 0xFF
    )
}

/// Couleur choisie : champs, aperçu et cadres mis à jour.
unsafe fn set_color(dlg: usize, c: u32, from: usize) {
    with_color(|s| {
        s.color = c;
        s.updating = true;
        if s.brush != 0 {
            gdi32::delete_object(s.brush);
        }
        s.brush = gdi32::create_solid_brush(c);
    });
    for (id, v) in [
        (ID_RED, c & 0xFF),
        (ID_GREEN, (c >> 8) & 0xFF),
        (ID_BLUE, (c >> 16) & 0xFF),
    ] {
        // Les flèches suivent la valeur (et réécrivent le champ associé).
        let ud = user32::get_dlg_item(dlg, (id + 0x100) as i32);
        if ud != 0 && id != from {
            send(ud, 0x0471, 0, v as isize); // UDM_SETPOS32
        }
        if id != from {
            set_text(user32::get_dlg_item(dlg, id as i32), &v.to_string());
        }
    }
    if from != ID_HEX {
        set_text(user32::get_dlg_item(dlg, ID_HEX as i32), &hex_text(c));
    }
    with_color(|s| s.updating = false);
    for id in std::iter::once(ID_PREVIEW)
        .chain((0..48).map(|i| ID_BASIC + i))
        .chain((0..16).map(|i| ID_CUSTOM + i))
    {
        user32::invalidate_rect(user32::get_dlg_item(dlg, id as i32), std::ptr::null(), 1);
    }
}

unsafe fn rgb_from_fields(dlg: usize) -> u32 {
    let v = |id: usize| -> u32 {
        get_text(user32::get_dlg_item(dlg, id as i32))
            .trim()
            .parse::<u32>()
            .unwrap_or(0)
            .min(255)
    };
    v(ID_RED) | (v(ID_GREEN) << 8) | (v(ID_BLUE) << 16)
}

unsafe fn color_init(dlg: usize) {
    let initial = with_color(|s| {
        s.updating = true;
        s.color
    })
    .unwrap_or(0);
    let label = |text: &str, rect| {
        dialogs::add_control(dlg, "STATIC", text, 0xFFFF, 0, 0, rect);
    };
    label(
        tr(
            "Couleurs de &base :",
            "&Basic colors:",
            "&Grundfarben:",
            "Colores &básicos:",
        ),
        (16, 12, 260, 20),
    );
    for i in 0..48 {
        let (col, row) = ((i % 8) as i32, (i / 8) as i32);
        let style = 0xB | if i == 0 { 0x0003_0000 } else { 0 }; // BS_OWNERDRAW
        dialogs::add_control(
            dlg,
            "BUTTON",
            "",
            ID_BASIC + i,
            style,
            0,
            (16 + col * 32, 36 + row * 28, 28, 24),
        );
    }
    label(
        tr(
            "&Couleurs personnalisées :",
            "&Custom colors:",
            "&Benutzerdefinierte Farben:",
            "Colores &personalizados:",
        ),
        (16, 212, 260, 20),
    );
    for i in 0..16 {
        let (col, row) = ((i % 8) as i32, (i / 8) as i32);
        let style = 0xB | if i == 0 { 0x0003_0000 } else { 0 };
        dialogs::add_control(
            dlg,
            "BUTTON",
            "",
            ID_CUSTOM + i,
            style,
            0,
            (16 + col * 32, 236 + row * 28, 28, 24),
        );
    }
    dialogs::add_control(
        dlg,
        "BUTTON",
        tr(
            "&Ajouter aux couleurs personnalisées",
            "&Add to Custom Colors",
            "Zu benutzerdefinierten Farben &hinzufügen",
            "&Agregar a colores personalizados",
        ),
        ID_ADD,
        0x0001_0000,
        0,
        (16, 300, 252, 30),
    );
    dialogs::add_control(
        dlg,
        "STATIC",
        "",
        ID_PREVIEW,
        0x0080_0000, // WS_BORDER
        0,
        (290, 36, 170, 92),
    );
    let fields = [
        (tr("&Rouge :", "&Red:", "&Rot:", "&Rojo:"), ID_RED),
        (tr("&Vert :", "&Green:", "&Grün:", "&Verde:"), ID_GREEN),
        (tr("&Bleu :", "Bl&ue:", "&Blau:", "&Azul:"), ID_BLUE),
    ];
    for (i, (text, id)) in fields.iter().enumerate() {
        let y = 144 + i as i32 * 36;
        label(text, (290, y + 4, 70, 20));
        let edit = dialogs::add_control(
            dlg,
            "EDIT",
            "0",
            *id,
            0x0001_0000 | 0x2000, // ES_NUMBER
            0x200,
            (364, y, 70, 28),
        );
        let ud = dialogs::add_control(
            dlg,
            "msctls_updown32",
            "",
            id + 0x100,
            0x2 | 0x4 | 0x20 | 0x80, // SETBUDDYINT | ALIGNRIGHT | ARROWKEYS | NOTHOUSANDS
            0,
            (434, y, 18, 28),
        );
        send(ud, 0x0469, edit, 0); // UDM_SETBUDDY
        send(ud, 0x046F, 0, 255); // UDM_SETRANGE32
    }
    label("&Hex :", (290, 256, 70, 20));
    dialogs::add_control(
        dlg,
        "EDIT",
        "",
        ID_HEX,
        0x0001_0000,
        0x200,
        (364, 252, 96, 28),
    );
    dialogs::add_control(
        dlg,
        "BUTTON",
        "OK",
        1,
        0x0001_0000 | 0x1,
        0,
        (268, 344, 92, 30),
    );
    dialogs::add_control(
        dlg,
        "BUTTON",
        cancel_label(),
        2,
        0x0001_0000,
        0,
        (368, 344, 92, 30),
    );
    set_color(dlg, initial, 0);
    user32::set_focus(user32::get_dlg_item(dlg, 1));
    if !gui::backend_is_interactive() {
        if let Ok(v) = std::env::var("WINLUX_CHOOSECOLOR") {
            set_text(user32::get_dlg_item(dlg, ID_HEX as i32), &v);
            user32::post_message_w(dlg, 0x0111, 1, 0);
        }
    }
}

unsafe extern "win64" fn color_proc(dlg: usize, msg: u32, wp: usize, lp: isize) -> isize {
    match msg {
        0x0110 => {
            color_init(dlg);
            0 // le focus est sur OK
        }
        0x002B => {
            // WM_DRAWITEM : un carré de couleur.
            let d = &*(lp as *const DrawItem);
            let id = d.ctl_id as usize;
            let Some((cur, custom, slot)) = with_color(|s| (s.color, s.custom, s.custom_slot))
            else {
                return 0;
            };
            let (c, selected) = if (ID_BASIC..ID_BASIC + 48).contains(&id) {
                (BASIC[id - ID_BASIC], BASIC[id - ID_BASIC] == cur)
            } else if (ID_CUSTOM..ID_CUSTOM + 16).contains(&id) {
                (custom[id - ID_CUSTOM], id - ID_CUSTOM == slot)
            } else {
                return 0;
            };
            fill(d.hdc, d.rc, user32::get_sys_color(15));
            let inner = gui::Rect {
                left: d.rc.left + 3,
                top: d.rc.top + 3,
                right: d.rc.right - 3,
                bottom: d.rc.bottom - 3,
            };
            fill(d.hdc, inner, c);
            frame(d.hdc, inner, 0x00A0A0A0, 1);
            if selected || d.item_state & 0x10 != 0 {
                frame(d.hdc, d.rc, 0x00202020, 2);
            }
            1
        }
        0x0138 => {
            // WM_CTLCOLORSTATIC : l'aperçu prend la couleur choisie.
            if lp as usize == user32::get_dlg_item(dlg, ID_PREVIEW as i32) {
                return with_color(|s| s.brush as isize).unwrap_or(0);
            }
            0
        }
        0x0111 => {
            let id = wp & 0xFFFF;
            let code = (wp >> 16) & 0xFFFF;
            let updating = with_color(|s| s.updating).unwrap_or(true);
            match (id, code) {
                (1, _) => {
                    let hex = get_text(user32::get_dlg_item(dlg, ID_HEX as i32));
                    if let Some(c) = parse_hex(&hex) {
                        with_color(|s| s.color = c);
                    }
                    dialogs::end_dialog(dlg, 1);
                }
                (2, _) => {
                    dialogs::end_dialog(dlg, 0);
                }
                (ID_ADD, 0) => {
                    with_color(|s| {
                        s.custom[s.custom_slot] = s.color;
                        s.custom_slot = (s.custom_slot + 1) % 16;
                    });
                    let c = with_color(|s| s.color).unwrap_or(0);
                    set_color(dlg, c, 0);
                }
                (i, 0) if (ID_BASIC..ID_BASIC + 48).contains(&i) => {
                    set_color(dlg, BASIC[i - ID_BASIC], 0);
                }
                (i, 0) if (ID_CUSTOM..ID_CUSTOM + 16).contains(&i) => {
                    let c = with_color(|s| {
                        s.custom_slot = i - ID_CUSTOM;
                        s.custom[i - ID_CUSTOM]
                    })
                    .unwrap_or(0);
                    set_color(dlg, c, 0);
                }
                (ID_RED | ID_GREEN | ID_BLUE, 0x300) if !updating => {
                    let c = rgb_from_fields(dlg);
                    set_color(dlg, c, id);
                }
                (ID_HEX, 0x300) if !updating => {
                    let hex = get_text(user32::get_dlg_item(dlg, ID_HEX as i32));
                    if let Some(c) = parse_hex(&hex) {
                        set_color(dlg, c, ID_HEX);
                    }
                }
                _ => {}
            }
            1
        }
        _ => 0,
    }
}

/// CHOOSECOLOR (64 bits, A et W identiques) : 72 octets.
unsafe fn choose_color(cc: *mut u8) -> i32 {
    set_error(0);
    if cc.is_null() {
        set_error(CDERR_INITIALIZATION);
        return 0;
    }
    if rd::<u32>(cc, 0) != 72 {
        set_error(CDERR_STRUCTSIZE);
        return 0;
    }
    let owner = rd::<usize>(cc, 8);
    let flags = rd::<u32>(cc, 40);
    let custom_ptr = rd::<usize>(cc, 32);
    let mut custom = [0x00FF_FFFFu32; 16];
    if custom_ptr != 0 {
        for (i, c) in custom.iter_mut().enumerate() {
            *c = *(custom_ptr as *const u32).add(i);
        }
    }
    let color = if flags & 0x1 != 0 {
        rd::<u32>(cc, 24)
    } else {
        0
    }; // CC_RGBINIT
    COLOR.with(|s| {
        *s.borrow_mut() = Some(ColorState {
            color: color & 0x00FF_FFFF,
            custom,
            ..Default::default()
        })
    });
    let title = tr("Couleur", "Color", "Farbe", "Color");
    let r = dialogs::run_builtin(owner, title, 476, 390, color_proc as *const () as usize, 0);
    let st = COLOR.with(|s| s.borrow_mut().take()).unwrap_or_default();
    if st.brush != 0 {
        gdi32::delete_object(st.brush);
    }
    if custom_ptr != 0 {
        for (i, c) in st.custom.iter().enumerate() {
            *(custom_ptr as *mut u32).add(i) = *c;
        }
    }
    if r != 1 {
        return 0;
    }
    wr::<u32>(cc, 24, st.color);
    1
}

pub unsafe extern "win64" fn choose_color_a(cc: *mut u8) -> i32 {
    choose_color(cc)
}
pub unsafe extern "win64" fn choose_color_w(cc: *mut u8) -> i32 {
    choose_color(cc)
}

// =====================================================================
// ChooseFont
// =====================================================================

const ID_FACE_EDIT: usize = 0x3E8;
const ID_FACES: usize = 0x470;
const ID_STYLES: usize = 0x471;
const ID_SIZE_EDIT: usize = 0x3EA;
const ID_SIZES: usize = 0x472;
const ID_STRIKE: usize = 0x410;
const ID_UNDERLINE: usize = 0x411;
const ID_SAMPLE: usize = 0x444;

const CF_EFFECTS: u32 = 0x100;
const CF_INITTOLOGFONTSTRUCT: u32 = 0x40;
const CF_LIMITSIZE: u32 = 0x2000;
const CF_FIXEDPITCHONLY: u32 = 0x4000;

const SIZES: [i32; 16] = [8, 9, 10, 11, 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72];

#[derive(Clone, Default)]
struct FontState {
    faces: Vec<String>,
    face: String,
    points: i32,
    bold: bool,
    italic: bool,
    underline: bool,
    strikeout: bool,
    effects: bool,
    min: i32,
    max: i32,
    sample_font: usize,
    updating: bool,
}

thread_local! {
    static FONT: RefCell<Option<FontState>> = const { RefCell::new(None) };
}

fn with_font<R>(f: impl FnOnce(&mut FontState) -> R) -> Option<R> {
    FONT.with(|s| s.borrow_mut().as_mut().map(f))
}

fn style_labels() -> [&'static str; 4] {
    match lang().as_str() {
        "fr" => ["Normal", "Italique", "Gras", "Gras italique"],
        "de" => ["Standard", "Kursiv", "Fett", "Fett Kursiv"],
        "es" => ["Normal", "Cursiva", "Negrita", "Negrita cursiva"],
        _ => ["Regular", "Italic", "Bold", "Bold Italic"],
    }
}

/// Familles proposées (une par nom, triées), à espacement fixe si demandé.
fn font_families(fixed_only: bool) -> Vec<String> {
    let faces = super::gdi32_fonts::all_faces();
    let mut out: Vec<String> = super::gdi32_fonts::families(&faces)
        .into_iter()
        .filter(|f| !fixed_only || f.fixed_pitch)
        .map(|f| f.family)
        .collect();
    out.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    out
}

/// Hauteur LOGFONT (pixels, négative = taille du caractère) d'une taille en points.
pub(crate) fn height_for_points(points: i32) -> i32 {
    -((points * 96 + 36) / 72)
}

pub(crate) fn points_for_height(height: i32) -> i32 {
    if height == 0 {
        return 10;
    }
    let px = height.abs();
    // Une hauteur positive inclut l'interligne interne (~ 20 %).
    let px = if height > 0 { px * 5 / 6 } else { px };
    ((px * 72 + 48) / 96).max(1)
}

fn logfont(st: &FontState) -> gdi32::LogFontW {
    let mut face = [0u16; 32];
    for (i, u) in st.face.encode_utf16().take(31).enumerate() {
        face[i] = u;
    }
    gdi32::LogFontW {
        height: height_for_points(st.points),
        width: 0,
        escapement: 0,
        orientation: 0,
        weight: if st.bold { 700 } else { 400 },
        italic: st.italic as u8,
        underline: st.underline as u8,
        strikeout: st.strikeout as u8,
        charset: 1, // DEFAULT_CHARSET
        out_precision: 0,
        clip_precision: 0,
        quality: 0,
        pitch_and_family: 0,
        face,
    }
}

unsafe fn update_sample(dlg: usize) {
    let Some(st) = FONT.with(|s| s.borrow().clone()) else {
        return;
    };
    let lf = logfont(&st);
    let font = gdi32::create_font_indirect_w(&lf as *const gdi32::LogFontW as *const _);
    let old = with_font(|s| std::mem::replace(&mut s.sample_font, font)).unwrap_or(0);
    let sample = user32::get_dlg_item(dlg, ID_SAMPLE as i32);
    send(sample, 0x0030, font, 1); // WM_SETFONT
    if old != 0 {
        gdi32::delete_object(old);
    }
}

unsafe fn font_init(dlg: usize) {
    let Some(st) = FONT.with(|s| s.borrow().clone()) else {
        return;
    };
    let label = |text: &str, rect| {
        dialogs::add_control(dlg, "STATIC", text, 0xFFFF, 0, 0, rect);
    };
    label(
        tr("&Police :", "&Font:", "&Schriftart:", "&Fuente:"),
        (16, 12, 220, 20),
    );
    let face_edit = dialogs::add_control(
        dlg,
        "EDIT",
        &st.face,
        ID_FACE_EDIT,
        0x0001_0000 | 0x80,
        0x200,
        (16, 34, 230, 28),
    );
    let faces = dialogs::add_control(
        dlg,
        "LISTBOX",
        "",
        ID_FACES,
        0x0001_0000 | 0x1 | 0x0020_0000,
        0x200,
        (16, 66, 230, 168),
    );
    for f in &st.faces {
        let t = wide(f);
        send(faces, 0x180, 0, t.as_ptr() as isize);
    }
    label(
        tr("St&yle :", "Font st&yle:", "Schrifts&til:", "Es&tilo:"),
        (258, 12, 150, 20),
    );
    let styles = dialogs::add_control(
        dlg,
        "LISTBOX",
        "",
        ID_STYLES,
        0x0001_0000 | 0x1,
        0x200,
        (258, 34, 150, 200),
    );
    for s in style_labels() {
        let t = wide(s);
        send(styles, 0x180, 0, t.as_ptr() as isize);
    }
    label(
        tr("&Taille :", "&Size:", "&Größe:", "&Tamaño:"),
        (420, 12, 90, 20),
    );
    dialogs::add_control(
        dlg,
        "EDIT",
        &st.points.to_string(),
        ID_SIZE_EDIT,
        0x0001_0000 | 0x2000,
        0x200,
        (420, 34, 90, 28),
    );
    let sizes = dialogs::add_control(
        dlg,
        "LISTBOX",
        "",
        ID_SIZES,
        0x0001_0000 | 0x1 | 0x0020_0000,
        0x200,
        (420, 66, 90, 168),
    );
    for s in SIZES.iter().filter(|&&s| s >= st.min && s <= st.max) {
        let t = wide(&s.to_string());
        send(sizes, 0x180, 0, t.as_ptr() as isize);
    }
    if st.effects {
        dialogs::add_control(
            dlg,
            "BUTTON",
            tr("Effets", "Effects", "Effekte", "Efectos"),
            0xFFFF,
            0x7,
            0,
            (16, 246, 230, 92),
        );
        let strike = dialogs::add_control(
            dlg,
            "BUTTON",
            tr("&Barré", "Stri&keout", "&Durchgestrichen", "&Tachado"),
            ID_STRIKE,
            0x0001_0000 | 0x3,
            0,
            (30, 272, 200, 24),
        );
        let under = dialogs::add_control(
            dlg,
            "BUTTON",
            tr("&Souligné", "&Underline", "&Unterstrichen", "&Subrayado"),
            ID_UNDERLINE,
            0x0001_0000 | 0x3,
            0,
            (30, 302, 200, 24),
        );
        send(strike, 0xF1, st.strikeout as usize, 0);
        send(under, 0xF1, st.underline as usize, 0);
    }
    dialogs::add_control(
        dlg,
        "BUTTON",
        tr("Aperçu", "Sample", "Beispiel", "Muestra"),
        0xFFFF,
        0x7,
        0,
        (258, 246, 252, 92),
    );
    dialogs::add_control(
        dlg,
        "STATIC",
        "AaBbYyZz",
        ID_SAMPLE,
        0x1 | 0x200, // SS_CENTER | SS_CENTERIMAGE
        0,
        (268, 270, 232, 58),
    );
    dialogs::add_control(
        dlg,
        "BUTTON",
        "OK",
        1,
        0x0001_0000 | 0x1,
        0,
        (318, 352, 92, 30),
    );
    dialogs::add_control(
        dlg,
        "BUTTON",
        cancel_label(),
        2,
        0x0001_0000,
        0,
        (418, 352, 92, 30),
    );
    sync_lists(dlg);
    update_sample(dlg);
    user32::set_focus(face_edit);
    if !gui::backend_is_interactive() {
        if let Ok(v) = std::env::var("WINLUX_CHOOSEFONT") {
            let mut parts = v.split(',').map(str::trim);
            if let Some(face) = parts.next().filter(|f| !f.is_empty()) {
                set_text(face_edit, face);
            }
            if let Some(size) = parts.next() {
                set_text(user32::get_dlg_item(dlg, ID_SIZE_EDIT as i32), size);
            }
            for p in parts {
                with_font(|s| match p.to_ascii_lowercase().as_str() {
                    "gras" | "bold" => s.bold = true,
                    "italique" | "italic" => s.italic = true,
                    _ => {}
                });
            }
            user32::post_message_w(dlg, 0x0111, 1, 0);
        }
    }
}

/// Listes alignées sur l'état (police, style, taille).
unsafe fn sync_lists(dlg: usize) {
    let Some(st) = FONT.with(|s| s.borrow().clone()) else {
        return;
    };
    with_font(|s| s.updating = true);
    let faces = user32::get_dlg_item(dlg, ID_FACES as i32);
    let i = st
        .faces
        .iter()
        .position(|f| f.eq_ignore_ascii_case(&st.face))
        .map_or(-1isize, |i| i as isize);
    send(faces, 0x186, i as usize, 0);
    let styles = user32::get_dlg_item(dlg, ID_STYLES as i32);
    send(styles, 0x186, st.italic as usize + 2 * st.bold as usize, 0);
    let sizes = user32::get_dlg_item(dlg, ID_SIZES as i32);
    let list: Vec<i32> = SIZES
        .iter()
        .copied()
        .filter(|&s| s >= st.min && s <= st.max)
        .collect();
    let k = list
        .iter()
        .position(|&s| s == st.points)
        .map_or(-1isize, |k| k as isize);
    send(sizes, 0x186, k as usize, 0);
    with_font(|s| s.updating = false);
}

unsafe fn list_text(lb: usize, i: isize) -> String {
    if i < 0 {
        return String::new();
    }
    let n = send(lb, 0x18A, i as usize, 0).max(0) as usize; // LB_GETTEXTLEN
    let mut buf = vec![0u16; n + 1];
    let got = send(lb, 0x189, i as usize, buf.as_mut_ptr() as isize).max(0) as usize;
    String::from_utf16_lossy(&buf[..got.min(n)])
}

/// Valide la boîte : la police tapée doit exister (si la liste en a).
unsafe fn font_ok(dlg: usize) {
    let face = get_text(user32::get_dlg_item(dlg, ID_FACE_EDIT as i32));
    let size: i32 = get_text(user32::get_dlg_item(dlg, ID_SIZE_EDIT as i32))
        .trim()
        .parse()
        .unwrap_or(0);
    let Some(st) = FONT.with(|s| s.borrow().clone()) else {
        return;
    };
    let face = face.trim().to_string();
    let known = st
        .faces
        .iter()
        .find(|f| f.eq_ignore_ascii_case(&face))
        .cloned();
    if face.is_empty() || (!st.faces.is_empty() && known.is_none()) {
        dialogs::message_box(
            dlg,
            tr(
                "Aucune police de ce nom n'existe.\nChoisissez une police dans la liste.",
                "There is no font with that name.\nChoose a font from the list of fonts.",
                "Es ist keine Schriftart mit diesem Namen vorhanden.\nWählen Sie eine Schriftart aus der Liste.",
                "No hay ninguna fuente con ese nombre.\nElija una fuente de la lista.",
            ),
            &get_text(dlg),
            0x30,
        );
        return;
    }
    if size < st.min || size > st.max {
        let text = match lang().as_str() {
            "fr" => format!(
                "La taille doit être comprise entre {} et {} points.",
                st.min, st.max
            ),
            "de" => format!(
                "Die Größe muss zwischen {} und {} Punkt liegen.",
                st.min, st.max
            ),
            "es" => format!("El tamaño debe estar entre {} y {} puntos.", st.min, st.max),
            _ => format!("Size must be between {} and {} points.", st.min, st.max),
        };
        dialogs::message_box(dlg, &text, &get_text(dlg), 0x30);
        return;
    }
    with_font(|s| {
        s.face = known.unwrap_or(face);
        s.points = size;
    });
    dialogs::end_dialog(dlg, 1);
}

unsafe extern "win64" fn font_proc(dlg: usize, msg: u32, wp: usize, _lp: isize) -> isize {
    match msg {
        0x0110 => {
            font_init(dlg);
            0
        }
        0x0111 => {
            let id = wp & 0xFFFF;
            let code = (wp >> 16) & 0xFFFF;
            let updating = with_font(|s| s.updating).unwrap_or(true);
            match (id, code) {
                (1, _) => font_ok(dlg),
                (2, _) => {
                    dialogs::end_dialog(dlg, 0);
                }
                (ID_FACES, 1) if !updating => {
                    let lb = user32::get_dlg_item(dlg, ID_FACES as i32);
                    let face = list_text(lb, send(lb, 0x188, 0, 0));
                    with_font(|s| s.face = face.clone());
                    with_font(|s| s.updating = true);
                    set_text(user32::get_dlg_item(dlg, ID_FACE_EDIT as i32), &face);
                    with_font(|s| s.updating = false);
                    update_sample(dlg);
                }
                (ID_STYLES, 1) if !updating => {
                    let lb = user32::get_dlg_item(dlg, ID_STYLES as i32);
                    let i = send(lb, 0x188, 0, 0).max(0);
                    with_font(|s| {
                        s.italic = i & 1 != 0;
                        s.bold = i & 2 != 0;
                    });
                    update_sample(dlg);
                }
                (ID_SIZES, 1) if !updating => {
                    let lb = user32::get_dlg_item(dlg, ID_SIZES as i32);
                    let text = list_text(lb, send(lb, 0x188, 0, 0));
                    if let Ok(p) = text.parse::<i32>() {
                        with_font(|s| s.points = p);
                        with_font(|s| s.updating = true);
                        set_text(user32::get_dlg_item(dlg, ID_SIZE_EDIT as i32), &text);
                        with_font(|s| s.updating = false);
                        update_sample(dlg);
                    }
                }
                (ID_FACE_EDIT, 0x300) if !updating => {
                    // Saisie : la liste suit la première police qui commence ainsi.
                    let typed = get_text(user32::get_dlg_item(dlg, ID_FACE_EDIT as i32));
                    let lower = typed.to_lowercase();
                    let hit = with_font(|s| {
                        s.faces
                            .iter()
                            .find(|f| f.to_lowercase().starts_with(&lower))
                            .cloned()
                    })
                    .flatten();
                    if let Some(face) = hit.filter(|_| !lower.is_empty()) {
                        with_font(|s| s.face = face);
                        sync_lists(dlg);
                        update_sample(dlg);
                    }
                }
                (ID_SIZE_EDIT, 0x300) if !updating => {
                    let text = get_text(user32::get_dlg_item(dlg, ID_SIZE_EDIT as i32));
                    if let Ok(p) = text.trim().parse::<i32>() {
                        if (1..=999).contains(&p) {
                            with_font(|s| s.points = p);
                            sync_lists(dlg);
                            update_sample(dlg);
                        }
                    }
                }
                (ID_STRIKE | ID_UNDERLINE, 0) => {
                    let checked = send(user32::get_dlg_item(dlg, id as i32), 0xF0, 0, 0) == 1;
                    with_font(|s| {
                        if id == ID_STRIKE {
                            s.strikeout = checked;
                        } else {
                            s.underline = checked;
                        }
                    });
                    update_sample(dlg);
                }
                _ => {}
            }
            1
        }
        _ => 0,
    }
}

/// CHOOSEFONT (64 bits) : 104 octets ; LOGFONTW 92 octets, LOGFONTA 60.
unsafe fn choose_font(cf: *mut u8, wide: bool) -> i32 {
    set_error(0);
    if cf.is_null() {
        set_error(CDERR_INITIALIZATION);
        return 0;
    }
    if rd::<u32>(cf, 0) != 104 {
        set_error(CDERR_STRUCTSIZE);
        return 0;
    }
    let owner = rd::<usize>(cf, 8);
    let lf_ptr = rd::<usize>(cf, 24) as *mut u8;
    let flags = rd::<u32>(cf, 36);
    if lf_ptr.is_null() {
        set_error(CDERR_INITIALIZATION);
        return 0;
    }
    let (min, max) = if flags & CF_LIMITSIZE != 0 {
        let lo = rd::<i32>(cf, 92).max(1);
        let hi = rd::<i32>(cf, 96);
        (lo, if hi >= lo { hi } else { 999 })
    } else {
        (1, 999)
    };
    let mut st = FontState {
        faces: font_families(flags & CF_FIXEDPITCHONLY != 0),
        face: String::new(),
        points: 10,
        effects: flags & CF_EFFECTS != 0,
        min,
        max,
        ..Default::default()
    };
    if flags & CF_INITTOLOGFONTSTRUCT != 0 {
        let height = rd::<i32>(lf_ptr, 0);
        st.points = points_for_height(height).clamp(min, max);
        st.bold = rd::<i32>(lf_ptr, 16) >= 600;
        st.italic = rd::<u8>(lf_ptr, 20) != 0;
        st.underline = rd::<u8>(lf_ptr, 21) != 0;
        st.strikeout = rd::<u8>(lf_ptr, 22) != 0;
        st.face = if wide {
            super::wstr(lf_ptr.add(28) as *const u16)
        } else {
            super::cstr(lf_ptr.add(28))
        };
    }
    if st.face.is_empty() {
        st.face = st.faces.first().cloned().unwrap_or_default();
    } else if let Some(f) = st.faces.iter().find(|f| f.eq_ignore_ascii_case(&st.face)) {
        st.face = f.clone();
    } else {
        // Police absente (« Courier New » sans polices Microsoft) : la liste
        // garde le nom demandé, que Winlux remplace à l'affichage.
        st.faces.push(st.face.clone());
        st.faces.sort_by_key(|f| f.to_lowercase());
    }
    FONT.with(|s| *s.borrow_mut() = Some(st));
    let title = tr("Police", "Font", "Schriftart", "Fuente");
    let r = dialogs::run_builtin(owner, title, 526, 398, font_proc as *const () as usize, 0);
    let st = FONT.with(|s| s.borrow_mut().take()).unwrap_or_default();
    if st.sample_font != 0 {
        gdi32::delete_object(st.sample_font);
    }
    if r != 1 {
        return 0;
    }
    let lf = logfont(&st);
    if wide {
        std::ptr::write_unaligned(lf_ptr as *mut gdi32::LogFontW, lf);
    } else {
        // LOGFONTA : mêmes champs, nom en octets.
        std::ptr::copy_nonoverlapping(&lf as *const gdi32::LogFontW as *const u8, lf_ptr, 28);
        let name = st.face.clone();
        super::put_text_a(&name, lf_ptr.add(28), 32);
    }
    wr::<i32>(cf, 32, st.points * 10);
    let mut kind = 0x2000u16; // SCREEN_FONTTYPE
    kind |= if st.bold { 0x100 } else { 0x400 }; // BOLD / REGULAR
    if st.italic {
        kind |= 0x200;
    }
    wr::<u16>(cf, 88, kind);
    1
}

pub unsafe extern "win64" fn choose_font_a(cf: *mut u8) -> i32 {
    choose_font(cf, false)
}
pub unsafe extern "win64" fn choose_font_w(cf: *mut u8) -> i32 {
    choose_font(cf, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn couleurs_hexadecimales_et_colorref() {
        assert_eq!(parse_hex("#FF8000"), Some(0x000080FF));
        assert_eq!(parse_hex("00ff00"), Some(0x0000FF00));
        assert_eq!(parse_hex("12345"), None);
        assert_eq!(hex_text(0x000080FF), "FF8000");
        assert_eq!(BASIC.len(), 48);
    }

    #[test]
    fn tailles_en_points_et_hauteurs() {
        assert_eq!(height_for_points(12), -16);
        assert_eq!(height_for_points(10), -13);
        assert_eq!(points_for_height(-16), 12);
        assert_eq!(points_for_height(-13), 10);
        assert_eq!(points_for_height(0), 10);
    }

    #[test]
    fn boites_sans_affichage_annulees_sans_variable() {
        // Structure trop petite : erreur de taille.
        let mut cc = [0u8; 72];
        cc[0] = 10;
        assert_eq!(unsafe { choose_color_w(cc.as_mut_ptr()) }, 0);
        assert_eq!(
            super::super::comdlg32::comm_dlg_extended_error(),
            CDERR_STRUCTSIZE
        );
    }
}
