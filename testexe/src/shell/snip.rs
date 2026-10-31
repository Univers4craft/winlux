//! Outil Capture façon Windows 11 : Win+Maj+S (ou Impr. écran) fige l'écran,
//! l'assombrit et laisse choisir une zone (rectangle, fenêtre, plein écran,
//! forme libre). La capture part dans le presse-papiers puis s'ouvre dans un
//! éditeur rapide (rognage, stylo, surligneur, flèche, texte, copier,
//! enregistrer en PNG). Le mode vidéo (Win+Maj+R) enregistre une zone ou
//! tout l'écran en MP4 ou WebM, avec ou sans micro et son système ; un
//! indicateur dans la barre des tâches affiche le minuteur et le bouton stop.

use crate::cliphist::tr;
use crate::gfx::*;
use crate::wm;
use crate::wm::Mouse;
use core::ffi::c_void;
use winlux_testexe::*;

const SRCCOPY: u32 = 0x00CC_0020;
const TOOLBAR_BTN: i32 = 40;
const TOOLBAR_H: i32 = 48;
const MAX_POINTS: usize = 4096;
const UNDO_LEVELS: usize = 4;
pub const REC_W: i32 = 118;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Rect,
    Window,
    Full,
    Free,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    /// Écran figé, choix de la zone.
    Select,
    /// Zone vidéo choisie, réglages avant de démarrer.
    VideoSetup,
    /// Éditeur rapide ouvert.
    Edit,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    None,
    Crop,
    Pen,
    Highlighter,
    Arrow,
    Text,
}

/// Image 32 bpp « top-down » dans une DIB section.
#[derive(Clone, Copy)]
struct Dib {
    bmp: Handle,
    bits: *mut u8,
    w: i32,
    h: i32,
}

const NO_DIB: Dib = Dib {
    bmp: core::ptr::null_mut(),
    bits: core::ptr::null_mut(),
    w: 0,
    h: 0,
};

impl Dib {
    fn ok(&self) -> bool {
        !self.bmp.is_null() && !self.bits.is_null()
    }
    fn stride(&self) -> i32 {
        self.w * 4
    }
    fn bytes(&self) -> usize {
        (self.w * self.h * 4) as usize
    }
}

unsafe fn new_dib(w: i32, h: i32) -> Dib {
    if w <= 0 || h <= 0 {
        return NO_DIB;
    }
    let info = BitmapInfoHeader {
        size: 40,
        width: w,
        height: -h,
        planes: 1,
        bit_count: 32,
        compression: 0,
        size_image: 0,
        x_pels_per_meter: 0,
        y_pels_per_meter: 0,
        colors_used: 0,
        colors_important: 0,
    };
    let mut bits: *mut c_void = core::ptr::null_mut();
    let bmp = CreateDIBSection(
        GetDC(core::ptr::null_mut()),
        &info,
        0,
        &mut bits,
        core::ptr::null_mut(),
        0,
    );
    if bmp.is_null() || bits.is_null() {
        return NO_DIB;
    }
    Dib {
        bmp,
        bits: bits as *mut u8,
        w,
        h,
    }
}

unsafe fn free_dib(d: &mut Dib) {
    if !d.bmp.is_null() {
        DeleteObject(d.bmp);
    }
    *d = NO_DIB;
}

/// Copie d'une zone d'une image vers une nouvelle image.
unsafe fn crop_dib(src: &Dib, r: &Rect) -> Dib {
    let x0 = r.left.clamp(0, src.w);
    let y0 = r.top.clamp(0, src.h);
    let x1 = r.right.clamp(0, src.w);
    let y1 = r.bottom.clamp(0, src.h);
    let out = new_dib(x1 - x0, y1 - y0);
    if !out.ok() {
        return out;
    }
    for y in 0..out.h {
        core::ptr::copy_nonoverlapping(
            src.bits.add(((y0 + y) * src.stride() + x0 * 4) as usize),
            out.bits.add((y * out.stride()) as usize),
            (out.w * 4) as usize,
        );
    }
    out
}

static mut PHASE: Phase = Phase::Idle;
static mut MODE: Mode = Mode::Rect;
static mut VIDEO: bool = false;
static mut SCREEN_W: i32 = 0;
static mut SCREEN_H: i32 = 0;
static mut BAR_TOP: i32 = 0;
static mut FROZEN: Dib = NO_DIB;
static mut DIMMED: Dib = NO_DIB;
static mut MEM_DC: Handle = core::ptr::null_mut();
static mut DRAGGING: bool = false;
static mut START: (i32, i32) = (0, 0);
static mut CUR: (i32, i32) = (0, 0);
static mut POINTS: [(i32, i32); MAX_POINTS] = [(0, 0); MAX_POINTS];
static mut NPOINTS: usize = 0;
static mut HOVER_BTN: i32 = -1;
static mut SELECTION: Rect = rect(0, 0, 0, 0);
static mut PENDING_OPEN: u32 = 0;
static mut PENDING_VIDEO: bool = false;

// Réglages vidéo.
static mut REC_MIC: bool = false;
static mut REC_SYSTEM: bool = true;
static mut REC_WEBM: bool = false;
static mut REC_ACTIVE: bool = false;
static mut REC_PATH: [u8; 300] = [0; 300];
static mut REC_PATH_LEN: usize = 0;

// Éditeur.
static mut IMG: Dib = NO_DIB;
static mut UNDO: [Dib; UNDO_LEVELS] = [NO_DIB; UNDO_LEVELS];
static mut NUNDO: usize = 0;
static mut TOOL: Tool = Tool::Pen;
static mut COLOR: usize = 0;
static mut SIZE: usize = 1;
static mut TEXT_ON: bool = false;
static mut TEXT_POS: (i32, i32) = (0, 0);
static mut TEXT_BUF: [u8; 120] = [0; 120];
static mut TEXT_LEN: usize = 0;
static mut SAVED_NAME: [u8; 160] = [0; 160];
static mut SAVED_LEN: usize = 0;

const COLORS: [u32; 6] = [
    rgb(0xE8, 0x11, 0x23),
    rgb(0xFF, 0xD7, 0x00),
    rgb(0x10, 0xB9, 0x4A),
    rgb(0x00, 0x78, 0xD4),
    rgb(0x10, 0x10, 0x10),
    rgb(0xFF, 0xFF, 0xFF),
];
const SIZES: [i32; 3] = [2, 4, 8];

fn now() -> u32 {
    unsafe { GetTickCount() }
}

pub fn active() -> bool {
    unsafe { PHASE != Phase::Idle }
}
/// Vrai quand l'overlay plein écran remplace l'affichage du bureau.
pub fn overlay() -> bool {
    unsafe { matches!(PHASE, Phase::Select | Phase::VideoSetup) }
}
/// Vrai si un clic en (x, y) revient à l'outil (overlay ou fenêtre d'édition).
pub unsafe fn hit(x: i32, y: i32) -> bool {
    match PHASE {
        Phase::Select | Phase::VideoSetup => true,
        Phase::Edit => DRAGGING || inside(&ed_layout().win, x, y),
        Phase::Idle => false,
    }
}

pub fn recording() -> bool {
    unsafe { REC_ACTIVE }
}
pub fn mode() -> Mode {
    unsafe { MODE }
}
pub fn tool() -> Tool {
    unsafe { TOOL }
}
pub fn image_size() -> (i32, i32) {
    unsafe { (IMG.w, IMG.h) }
}

unsafe fn mem_dc() -> Handle {
    if MEM_DC.is_null() {
        MEM_DC = CreateCompatibleDC(core::ptr::null_mut());
    }
    MEM_DC
}

unsafe fn blit(hdc: Handle, d: &Dib, dx: i32, dy: i32, sx: i32, sy: i32, w: i32, h: i32) {
    if !d.ok() || w <= 0 || h <= 0 {
        return;
    }
    let dc = mem_dc();
    let old = SelectObject(dc, d.bmp);
    BitBlt(hdc, dx, dy, w, h, dc, sx, sy, SRCCOPY);
    SelectObject(dc, old);
}

unsafe fn stretch(hdc: Handle, d: &Dib, dst: &Rect) {
    if !d.ok() {
        return;
    }
    let (w, h) = (dst.right - dst.left, dst.bottom - dst.top);
    if w == d.w && h == d.h {
        blit(hdc, d, dst.left, dst.top, 0, 0, w, h);
        return;
    }
    let dc = mem_dc();
    let old = SelectObject(dc, d.bmp);
    StretchBlt(hdc, dst.left, dst.top, w, h, dc, 0, 0, d.w, d.h, SRCCOPY);
    SelectObject(dc, old);
}

/// Fige l'écran : lecture de la racine X11, sinon du tampon du bureau.
unsafe fn freeze(hwnd: Handle, w: i32, h: i32) -> bool {
    free_dib(&mut FROZEN);
    free_dib(&mut DIMMED);
    FROZEN = new_dib(w, h);
    DIMMED = new_dib(w, h);
    if !FROZEN.ok() || !DIMMED.ok() {
        return false;
    }
    let ok = WinluxCaptureScreen(0, 0, w, h, FROZEN.bits, FROZEN.stride()) != 0;
    if !ok {
        let dc = GetDC(hwnd);
        let mem = mem_dc();
        let old = SelectObject(mem, FROZEN.bmp);
        BitBlt(mem, 0, 0, w, h, dc, 0, 0, SRCCOPY);
        SelectObject(mem, old);
        ReleaseDC(hwnd, dc);
    }
    // Voile sombre : ~45 % de luminosité, comme Windows.
    let n = FROZEN.bytes();
    let src = core::slice::from_raw_parts(FROZEN.bits, n);
    let dst = core::slice::from_raw_parts_mut(DIMMED.bits, n);
    for i in 0..n {
        dst[i] = ((src[i] as u32 * 115) >> 8) as u8;
    }
    true
}

/// Ouvre l'outil (Win+Maj+S ; `video` pour Win+Maj+R).
pub unsafe fn open(hwnd: Handle, w: i32, h: i32, bar_top: i32, video: bool) {
    if REC_ACTIVE {
        // Pendant un enregistrement, le raccourci arrête l'enregistrement.
        stop_recording();
        return;
    }
    if PHASE == Phase::Edit {
        close_editor();
    }
    SCREEN_W = w;
    SCREEN_H = h;
    BAR_TOP = bar_top;
    if !freeze(hwnd, w, h) {
        crate::desktop::toast(tr(
            b"Capture impossible : m\xE9moire insuffisante.",
            b"Capture failed: out of memory.",
        ));
        return;
    }
    PHASE = Phase::Select;
    VIDEO = video;
    if VIDEO && MODE == Mode::Free {
        MODE = Mode::Rect;
    }
    DRAGGING = false;
    NPOINTS = 0;
    HOVER_BTN = -1;
}

/// Ouverture différée (menu Démarrer) : laisse le menu se refermer d'abord.
pub unsafe fn open_later(video: bool) {
    PENDING_OPEN = now().max(1);
    PENDING_VIDEO = video;
}

/// À appeler à chaque image : déclenche une ouverture différée.
pub unsafe fn frame(hwnd: Handle, w: i32, h: i32, bar_top: i32) -> bool {
    if PENDING_OPEN != 0 && now().wrapping_sub(PENDING_OPEN) > 260 {
        PENDING_OPEN = 0;
        open(hwnd, w, h, bar_top, PENDING_VIDEO);
        return true;
    }
    false
}

pub fn pending() -> bool {
    unsafe { PENDING_OPEN != 0 }
}

unsafe fn cancel() {
    PHASE = Phase::Idle;
    DRAGGING = false;
    free_dib(&mut FROZEN);
    free_dib(&mut DIMMED);
}

// ---------- Barre d'outils de sélection ----------

const SELECT_BUTTONS: usize = 6; // 4 modes, vidéo, fermer

fn toolbar_rect() -> Rect {
    let w = SELECT_BUTTONS as i32 * TOOLBAR_BTN + 2 * 12 + 16;
    let x = unsafe { (SCREEN_W - w) / 2 };
    rect(x, 16, x + w, 16 + TOOLBAR_H)
}

fn toolbar_button(i: usize) -> Rect {
    let t = toolbar_rect();
    let mut x = t.left + 8 + i as i32 * TOOLBAR_BTN;
    if i >= 4 {
        x += 12;
    }
    if i >= 5 {
        x += 12;
    }
    rect(x, t.top + 4, x + TOOLBAR_BTN, t.bottom - 4)
}

fn mode_of(i: usize) -> Mode {
    match i {
        0 => Mode::Rect,
        1 => Mode::Window,
        2 => Mode::Full,
        _ => Mode::Free,
    }
}

unsafe fn draw_mode_icon(hdc: Handle, b: &mut Brushes, i: usize, r: &Rect, c: u32, bg: u32) {
    let cx = (r.left + r.right) / 2;
    let cy = (r.top + r.bottom) / 2;
    match i {
        0 => {
            // Rectangle en pointillés.
            for k in 0..5 {
                fill(hdc, b, cx - 10 + k * 4, cy - 7, cx - 8 + k * 4, cy - 6, c);
                fill(hdc, b, cx - 10 + k * 4, cy + 6, cx - 8 + k * 4, cy + 7, c);
            }
            for k in 0..4 {
                fill(hdc, b, cx - 10, cy - 7 + k * 4, cx - 9, cy - 5 + k * 4, c);
                fill(hdc, b, cx + 9, cy - 7 + k * 4, cx + 10, cy - 5 + k * 4, c);
            }
        }
        1 => {
            // Fenêtre avec barre de titre.
            fill(hdc, b, cx - 10, cy - 8, cx + 10, cy + 8, c);
            fill(hdc, b, cx - 9, cy - 4, cx + 9, cy + 7, bg);
        }
        2 => {
            // Écran plein.
            fill(hdc, b, cx - 11, cy - 8, cx + 11, cy + 5, c);
            fill(hdc, b, cx - 9, cy - 6, cx + 9, cy + 3, bg);
            fill(hdc, b, cx - 2, cy + 5, cx + 2, cy + 8, c);
            fill(hdc, b, cx - 6, cy + 8, cx + 6, cy + 9, c);
        }
        3 => {
            // Forme libre : boucle.
            line(hdc, b, cx - 9, cy + 2, cx - 4, cy - 7, 2, c);
            line(hdc, b, cx - 4, cy - 7, cx + 7, cy - 5, 2, c);
            line(hdc, b, cx + 7, cy - 5, cx + 9, cy + 4, 2, c);
            line(hdc, b, cx + 9, cy + 4, cx - 2, cy + 8, 2, c);
            line(hdc, b, cx - 2, cy + 8, cx - 9, cy + 2, 2, c);
        }
        4 => {
            // Caméra.
            round_rect(hdc, b, cx - 11, cy - 6, cx + 4, cy + 6, 3, c);
            line(hdc, b, cx + 4, cy - 1, cx + 11, cy - 6, 2, c);
            line(hdc, b, cx + 4, cy + 1, cx + 11, cy + 6, 2, c);
            fill(hdc, b, cx + 10, cy - 6, cx + 12, cy + 7, c);
        }
        _ => {
            line(hdc, b, cx - 7, cy - 7, cx + 7, cy + 7, 2, c);
            line(hdc, b, cx + 7, cy - 7, cx - 7, cy + 7, 2, c);
        }
    }
}

fn button_label(i: usize) -> &'static [u8] {
    match i {
        0 => tr(b"Rectangle", b"Rectangle"),
        1 => tr(b"Fen\xEAtre", b"Window"),
        2 => tr(b"Plein \xE9cran", b"Full screen"),
        3 => tr(b"Forme libre", b"Freeform"),
        4 => tr(b"Enregistrer une vid\xE9o", b"Record video"),
        _ => tr(b"Fermer (\xC9chap)", b"Close (Esc)"),
    }
}

unsafe fn draw_toolbar(hdc: Handle, b: &mut Brushes, f: &Fonts) {
    let p = pal();
    let t = toolbar_rect();
    panel(
        hdc, b, t.left, t.top, t.right, t.bottom, 8, p.mica, p.border,
    );
    for i in 0..SELECT_BUTTONS {
        let r = toolbar_button(i);
        let selected = (i < 4 && mode_of(i) == MODE) || (i == 4 && VIDEO);
        let disabled = i == 3 && VIDEO;
        let bg = if selected {
            p.selected
        } else if HOVER_BTN == i as i32 && !disabled {
            p.hover
        } else {
            p.mica
        };
        round_rect(hdc, b, r.left, r.top, r.right, r.bottom, 5, bg);
        if selected {
            fill(
                hdc,
                b,
                r.left + 12,
                r.bottom - 3,
                r.right - 12,
                r.bottom - 1,
                p.accent,
            );
        }
        let c = if disabled {
            p.text3
        } else if i == 4 && VIDEO {
            rgb(0xE8, 0x11, 0x23)
        } else {
            p.text
        };
        draw_mode_icon(hdc, b, i, &r, c, bg);
    }
    // Séparateurs.
    for i in [4usize, 5] {
        let r = toolbar_button(i);
        fill(
            hdc,
            b,
            r.left - 7,
            t.top + 12,
            r.left - 6,
            t.bottom - 12,
            p.border,
        );
    }
    // Info-bulle du bouton survolé, sinon consigne.
    let hint: &[u8] = if HOVER_BTN >= 0 {
        button_label(HOVER_BTN as usize)
    } else if VIDEO {
        tr(
            b"Choisissez la zone \xE0 enregistrer",
            b"Choose the area to record",
        )
    } else {
        match MODE {
            Mode::Rect => tr(
                b"Faites glisser pour capturer une zone",
                b"Drag to capture an area",
            ),
            Mode::Window => tr(b"Cliquez sur une fen\xEAtre", b"Click a window"),
            Mode::Full => tr(
                b"Cliquez pour capturer l'\xE9cran",
                b"Click to capture the screen",
            ),
            Mode::Free => tr(
                b"Dessinez le contour \xE0 capturer",
                b"Draw the shape to capture",
            ),
        }
    };
    let w = text_width(hint, 12) + 24;
    let cx = (t.left + t.right) / 2;
    round_rect(
        hdc,
        b,
        cx - w / 2,
        t.bottom + 8,
        cx + w / 2,
        t.bottom + 34,
        6,
        p.surface,
    );
    text_centered(hdc, f.caption, 12, p.text, cx, t.bottom + 14, hint);
}

/// Zone de fenêtre (ou barre des tâches, ou écran) sous le pointeur.
unsafe fn window_under(x: i32, y: i32) -> Rect {
    if y >= BAR_TOP {
        return rect(0, BAR_TOP, SCREEN_W, SCREEN_H);
    }
    let mut list = [rect(0, 0, 0, 0); wm::APP_COUNT];
    let n = wm::visible_windows(&mut list);
    for r in list.iter().take(n) {
        if inside(r, x, y) {
            return *r;
        }
    }
    rect(0, 0, SCREEN_W, SCREEN_H)
}

fn norm(a: (i32, i32), b: (i32, i32)) -> Rect {
    rect(a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1))
}

unsafe fn frame_rect(hdc: Handle, b: &mut Brushes, r: &Rect, c: u32) {
    fill(hdc, b, r.left - 1, r.top - 1, r.right + 1, r.top, c);
    fill(hdc, b, r.left - 1, r.bottom, r.right + 1, r.bottom + 1, c);
    fill(hdc, b, r.left - 1, r.top, r.left, r.bottom, c);
    fill(hdc, b, r.right, r.top, r.right + 1, r.bottom, c);
}

unsafe fn write_num(buf: &mut [u8], mut n: usize, v: i32) -> usize {
    let mut tmp = [0u8; 10];
    let mut l = 0;
    let mut v = v.max(0) as u32;
    loop {
        tmp[l] = b'0' + (v % 10) as u8;
        l += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    for i in 0..l {
        if n < buf.len() {
            buf[n] = tmp[l - 1 - i];
            n += 1;
        }
    }
    n
}

unsafe fn draw_overlay(hdc: Handle, b: &mut Brushes, f: &Fonts) {
    let p = pal();
    blit(hdc, &DIMMED, 0, 0, 0, 0, SCREEN_W, SCREEN_H);
    let clear: Option<Rect> = if PHASE == Phase::VideoSetup {
        Some(SELECTION)
    } else {
        match MODE {
            Mode::Rect if DRAGGING => Some(norm(START, CUR)),
            Mode::Window => Some(window_under(CUR.0, CUR.1)),
            Mode::Full if !VIDEO || HOVER_BTN < 0 => None,
            _ => None,
        }
    };
    if let Some(r) = clear {
        if r.right > r.left && r.bottom > r.top {
            blit(
                hdc,
                &FROZEN,
                r.left,
                r.top,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
            );
            let c = if VIDEO {
                rgb(0xE8, 0x11, 0x23)
            } else {
                rgb(0xFF, 0xFF, 0xFF)
            };
            frame_rect(hdc, b, &r, c);
            // Dimensions de la zone.
            let mut dim = [0u8; 24];
            let mut n = write_num(&mut dim, 0, r.right - r.left);
            dim[n] = b' ';
            dim[n + 1] = 0xD7; // ×
            dim[n + 2] = b' ';
            n = write_num(&mut dim, n + 3, r.bottom - r.top);
            let ty = if r.bottom + 30 < SCREEN_H {
                r.bottom + 6
            } else {
                r.top - 26
            };
            let w = text_width(&dim[..n], 11) + 14;
            round_rect(hdc, b, r.left, ty, r.left + w, ty + 20, 4, p.surface);
            text(hdc, f.small, p.text, r.left + 7, ty + 3, &dim[..n]);
        }
    }
    if MODE == Mode::Free && DRAGGING && NPOINTS > 1 {
        for i in 1..NPOINTS {
            let (a, c) = (POINTS[i - 1], POINTS[i]);
            line(hdc, b, a.0, a.1, c.0, c.1, 2, rgb(0xFF, 0xFF, 0xFF));
        }
    }
    if PHASE == Phase::VideoSetup {
        draw_video_setup(hdc, b, f);
    } else {
        draw_toolbar(hdc, b, f);
    }
}

// ---------- Réglages vidéo ----------

fn video_bar() -> Rect {
    let w = 580;
    unsafe {
        let s = SELECTION;
        let x = ((s.left + s.right) / 2 - w / 2).clamp(8, (SCREEN_W - w - 8).max(8));
        let mut y = s.bottom + 34;
        if y + 52 > BAR_TOP {
            y = (s.top - 64).max(12);
        }
        if y < 12 || y + 52 > SCREEN_H {
            y = (BAR_TOP - 72).max(12);
        }
        rect(x, y, x + w, y + 52)
    }
}

fn video_button(i: usize) -> Rect {
    let bar = video_bar();
    // micro, son système, format, démarrer, annuler
    let widths = [118, 132, 92, 116, 82];
    let mut x = bar.left + 8;
    for (k, w) in widths.iter().enumerate() {
        if k == i {
            return rect(x, bar.top + 8, x + w, bar.bottom - 8);
        }
        x += w + 6;
    }
    rect(0, 0, 0, 0)
}

unsafe fn draw_video_setup(hdc: Handle, b: &mut Brushes, f: &Fonts) {
    let p = pal();
    let bar = video_bar();
    panel(
        hdc, b, bar.left, bar.top, bar.right, bar.bottom, 8, p.mica, p.border,
    );
    let labels: [(&[u8], bool); 5] = [
        (
            if REC_MIC {
                tr(b"Micro : oui", b"Mic: on")
            } else {
                tr(b"Micro : non", b"Mic: off")
            },
            REC_MIC,
        ),
        (
            if REC_SYSTEM {
                tr(b"Son syst\xE8me : oui", b"System audio: on")
            } else {
                tr(b"Son syst\xE8me : non", b"System audio: off")
            },
            REC_SYSTEM,
        ),
        (if REC_WEBM { b"WebM" } else { b"MP4" }, false),
        (tr(b"D\xE9marrer", b"Start"), true),
        (tr(b"Annuler", b"Cancel"), false),
    ];
    for (i, (label, on)) in labels.iter().enumerate() {
        let r = video_button(i);
        let start = i == 3;
        let bg = if start {
            rgb(0xC4, 0x2B, 0x1C)
        } else if *on {
            p.selected
        } else if HOVER_BTN == i as i32 {
            p.hover
        } else {
            p.surface
        };
        round_rect(hdc, b, r.left, r.top, r.right, r.bottom, 5, bg);
        let tc = if start { rgb(0xFF, 0xFF, 0xFF) } else { p.text };
        if start {
            ellipse(hdc, b, r.left + 12, r.top + 12, r.left + 24, r.top + 24, tc);
            text(hdc, f.semibold, tc, r.left + 32, r.top + 10, label);
        } else {
            text_centered(
                hdc,
                f.caption,
                12,
                tc,
                (r.left + r.right) / 2,
                r.top + 11,
                label,
            );
        }
    }
}

unsafe fn recording_path(out: &mut [u8]) -> usize {
    let english = GetFileAttributesA(c"C:\\Videos".as_ptr()) != u32::MAX;
    let (dir, sub, stem): (&[u8], &[u8], &[u8]) = if english {
        (
            b"C:\\Videos",
            b"\\Screen Recordings",
            b"\\Screen Recording ",
        )
    } else {
        (
            b"C:\\Vid\xE9os",
            b"\\Enregistrements d'\xE9cran",
            b"\\Enregistrement ",
        )
    };
    let ext: &[u8] = if REC_WEBM { b".webm" } else { b".mp4" };
    stamped_path(out, dir, sub, stem, ext)
}

unsafe fn stamped_path(out: &mut [u8], dir: &[u8], sub: &[u8], stem: &[u8], ext: &[u8]) -> usize {
    let mut st = SystemTime::default();
    GetLocalTime(&mut st);
    let mut n = 0;
    for part in [dir, sub] {
        for &c in part {
            out[n] = c;
            n += 1;
        }
        out[n] = 0;
        CreateDirectoryA(
            out.as_ptr() as *const core::ffi::c_char,
            core::ptr::null_mut(),
        );
    }
    for &c in stem {
        out[n] = c;
        n += 1;
    }
    let two = |out: &mut [u8], n: &mut usize, v: u16| {
        out[*n] = b'0' + (v / 10 % 10) as u8;
        out[*n + 1] = b'0' + (v % 10) as u8;
        *n += 2;
    };
    n = write_num(out, n, st.year as i32);
    out[n] = b'-';
    n += 1;
    two(out, &mut n, st.month);
    out[n] = b'-';
    n += 1;
    two(out, &mut n, st.day);
    out[n] = b' ';
    n += 1;
    two(out, &mut n, st.hour);
    two(out, &mut n, st.minute);
    two(out, &mut n, st.second);
    for &c in ext {
        out[n] = c;
        n += 1;
    }
    out[n] = 0;
    n
}

unsafe fn start_recording() {
    let r = SELECTION;
    cancel();
    let n = recording_path(&mut REC_PATH);
    REC_PATH_LEN = n;
    let mut flags = 0;
    if REC_MIC {
        flags |= 1;
    }
    if REC_SYSTEM {
        flags |= 2;
    }
    if REC_WEBM {
        flags |= 4;
    }
    if WinluxRecordStart(
        r.left,
        r.top,
        r.right - r.left,
        r.bottom - r.top,
        flags,
        REC_PATH.as_ptr() as *const core::ffi::c_char,
    ) != 0
    {
        REC_ACTIVE = true;
    } else {
        let msg: &[u8] = if GetLastError() == 2 {
            tr(
                b"Enregistrement impossible : ffmpeg est introuvable sur ce syst\xE8me.",
                b"Cannot record: ffmpeg was not found on this system.",
            )
        } else {
            tr(
                b"Enregistrement impossible : aucun affichage X11.",
                b"Cannot record: no X11 display.",
            )
        };
        crate::notifications::push(tr(b"Outil Capture", b"Snipping Tool"), msg);
        crate::desktop::toast(msg);
    }
}

/// Arrête l'enregistrement et annonce le fichier.
pub unsafe fn stop_recording() {
    if !REC_ACTIVE {
        return;
    }
    REC_ACTIVE = false;
    let ok = WinluxRecordStop() != 0;
    let name = &REC_PATH[..REC_PATH_LEN];
    let mut msg = [0u8; 360];
    let mut n = 0;
    let head: &[u8] = if ok {
        tr(b"Vid\xE9o enregistr\xE9e : ", b"Video saved: ")
    } else {
        tr(
            b"Enregistrement interrompu, fichier peut-\xEAtre incomplet : ",
            b"Recording interrupted, file may be incomplete: ",
        )
    };
    for &c in head.iter().chain(name.iter().skip(3)) {
        if n < msg.len() {
            msg[n] = c;
            n += 1;
        }
    }
    crate::notifications::push(tr(b"Outil Capture", b"Snipping Tool"), &msg[..n]);
    crate::desktop::toast(if ok {
        tr(
            b"Vid\xE9o enregistr\xE9e dans Vid\xE9os",
            b"Video saved to Videos",
        )
    } else {
        tr(b"Enregistrement interrompu", b"Recording interrupted")
    });
}

/// Appelé deux fois par seconde : suit l'état de ffmpeg (vrai s'il faut repeindre).
pub unsafe fn tick() -> bool {
    if !REC_ACTIVE {
        return false;
    }
    let mut ms = 0u32;
    if WinluxRecordState(&mut ms) != 1 {
        REC_ACTIVE = false;
        crate::notifications::push(
            tr(b"Outil Capture", b"Snipping Tool"),
            tr(
                b"L'enregistrement s'est arr\xEAt\xE9 (voir screen-recorder.log).",
                b"Recording stopped unexpectedly (see screen-recorder.log).",
            ),
        );
    }
    true
}

/// Indicateur d'enregistrement dans la barre des tâches (point rouge,
/// minuteur, bouton stop).
pub unsafe fn draw_rec_indicator(
    hdc: Handle,
    b: &mut Brushes,
    f: &Fonts,
    x: i32,
    top: i32,
    h: i32,
) {
    if !REC_ACTIVE {
        return;
    }
    let mut ms = 0u32;
    WinluxRecordState(&mut ms);
    let secs = (ms / 1000) as i32;
    let y = top + (h - 32) / 2;
    round_rect(hdc, b, x, y, x + REC_W, y + 32, 6, rgb(0x3A, 0x1E, 0x22));
    let blink = (now() / 600) % 2 == 0;
    let dot = if blink {
        rgb(0xFF, 0x3B, 0x30)
    } else {
        rgb(0xA0, 0x24, 0x20)
    };
    ellipse(hdc, b, x + 10, y + 11, x + 20, y + 21, dot);
    let mut t = [0u8; 12];
    let mut n = 0;
    if secs >= 3600 {
        n = write_num(&mut t, n, secs / 3600);
        t[n] = b':';
        n += 1;
    }
    let m = secs / 60 % 60;
    let s = secs % 60;
    t[n] = b'0' + (m / 10) as u8;
    t[n + 1] = b'0' + (m % 10) as u8;
    t[n + 2] = b':';
    t[n + 3] = b'0' + (s / 10) as u8;
    t[n + 4] = b'0' + (s % 10) as u8;
    n += 5;
    text(
        hdc,
        f.semibold,
        rgb(0xFF, 0xFF, 0xFF),
        x + 28,
        y + 8,
        &t[..n],
    );
    // Bouton stop.
    round_rect(
        hdc,
        b,
        x + REC_W - 34,
        y + 4,
        x + REC_W - 6,
        y + 28,
        4,
        rgb(0x5A, 0x2A, 0x2E),
    );
    fill(
        hdc,
        b,
        x + REC_W - 26,
        y + 10,
        x + REC_W - 14,
        y + 22,
        rgb(0xFF, 0xFF, 0xFF),
    );
}

// ---------- Fin de sélection : capture ----------

unsafe fn finish_capture(r: Rect, free: bool) {
    let r = rect(
        r.left.clamp(0, SCREEN_W),
        r.top.clamp(0, SCREEN_H),
        r.right.clamp(0, SCREEN_W),
        r.bottom.clamp(0, SCREEN_H),
    );
    if r.right - r.left < 2 || r.bottom - r.top < 2 {
        return;
    }
    if VIDEO {
        SELECTION = r;
        PHASE = Phase::VideoSetup;
        HOVER_BTN = -1;
        return;
    }
    let img = crop_dib(&FROZEN, &r);
    if !img.ok() {
        cancel();
        return;
    }
    if free && NPOINTS > 2 {
        // Hors du contour : blanc (règle pair-impair par ligne).
        for y in 0..img.h {
            let py = (r.top + y) as f32 + 0.5;
            let mut xs = [0f32; 64];
            let mut nx = 0;
            for i in 0..NPOINTS {
                let a = POINTS[i];
                let c = POINTS[(i + 1) % NPOINTS];
                let (ay, cy) = (a.1 as f32, c.1 as f32);
                if (ay <= py && cy > py) || (cy <= py && ay > py) {
                    let t = (py - ay) / (cy - ay);
                    if nx < xs.len() {
                        xs[nx] = a.0 as f32 + t * (c.0 - a.0) as f32;
                        nx += 1;
                    }
                }
            }
            xs[..nx]
                .sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
            for x in 0..img.w {
                let px = (r.left + x) as f32 + 0.5;
                let crossings = xs[..nx].iter().filter(|&&v| v < px).count();
                if crossings % 2 == 0 {
                    let o = (y * img.stride() + x * 4) as usize;
                    *img.bits.add(o) = 255;
                    *img.bits.add(o + 1) = 255;
                    *img.bits.add(o + 2) = 255;
                }
            }
        }
    }
    cancel();
    open_editor(img);
    copy_image();
    crate::notifications::push(
        tr(b"Outil Capture", b"Snipping Tool"),
        tr(
            b"Capture copi\xE9e dans le presse-papiers. Annotez-la ou enregistrez-la.",
            b"Snip copied to the clipboard. Annotate or save it.",
        ),
    );
}

unsafe fn select_mouse(ev: Mouse, x: i32, y: i32) -> bool {
    if PHASE == Phase::VideoSetup {
        return video_mouse(ev, x, y);
    }
    let mut hb = -1;
    for i in 0..SELECT_BUTTONS {
        if inside(&toolbar_button(i), x, y) {
            hb = i as i32;
        }
    }
    let on_bar = inside(&toolbar_rect(), x, y);
    match ev {
        Mouse::Move => {
            let changed = hb != HOVER_BTN || DRAGGING || MODE == Mode::Window;
            HOVER_BTN = hb;
            CUR = (x, y);
            if DRAGGING && MODE == Mode::Free && NPOINTS < MAX_POINTS {
                let last = POINTS[NPOINTS - 1];
                if (last.0 - x).abs() + (last.1 - y).abs() >= 3 {
                    POINTS[NPOINTS] = (x, y);
                    NPOINTS += 1;
                }
            }
            changed
        }
        Mouse::Down => {
            if on_bar {
                if hb >= 0 {
                    let i = hb as usize;
                    match i {
                        0..=3 => {
                            if !(VIDEO && i == 3) {
                                MODE = mode_of(i);
                                if MODE == Mode::Full {
                                    finish_capture(rect(0, 0, SCREEN_W, SCREEN_H), false);
                                }
                            }
                        }
                        4 => {
                            VIDEO = !VIDEO;
                            if VIDEO && MODE == Mode::Free {
                                MODE = Mode::Rect;
                            }
                        }
                        _ => cancel(),
                    }
                }
                return true;
            }
            CUR = (x, y);
            match MODE {
                Mode::Rect => {
                    DRAGGING = true;
                    START = (x, y);
                }
                Mode::Free => {
                    DRAGGING = true;
                    POINTS[0] = (x, y);
                    NPOINTS = 1;
                }
                Mode::Window => finish_capture(window_under(x, y), false),
                Mode::Full => finish_capture(rect(0, 0, SCREEN_W, SCREEN_H), false),
            }
            true
        }
        Mouse::Up => {
            if !DRAGGING {
                return false;
            }
            DRAGGING = false;
            match MODE {
                Mode::Rect => finish_capture(norm(START, (x, y)), false),
                Mode::Free => {
                    let mut r = rect(i32::MAX, i32::MAX, i32::MIN, i32::MIN);
                    for p in POINTS.iter().take(NPOINTS) {
                        r.left = r.left.min(p.0);
                        r.top = r.top.min(p.1);
                        r.right = r.right.max(p.0 + 1);
                        r.bottom = r.bottom.max(p.1 + 1);
                    }
                    if NPOINTS > 2 {
                        finish_capture(r, true);
                    }
                    NPOINTS = 0;
                }
                _ => {}
            }
            true
        }
        Mouse::RightDown => {
            cancel();
            true
        }
        _ => false,
    }
}

unsafe fn video_mouse(ev: Mouse, x: i32, y: i32) -> bool {
    let mut hb = -1;
    for i in 0..5 {
        if inside(&video_button(i), x, y) {
            hb = i as i32;
        }
    }
    match ev {
        Mouse::Move => {
            let changed = hb != HOVER_BTN;
            HOVER_BTN = hb;
            changed
        }
        Mouse::Down => {
            match hb {
                0 => REC_MIC = !REC_MIC,
                1 => REC_SYSTEM = !REC_SYSTEM,
                2 => REC_WEBM = !REC_WEBM,
                3 => start_recording(),
                4 => cancel(),
                _ => {
                    // Clic ailleurs : on rechoisit la zone.
                    if !inside(&video_bar(), x, y) {
                        PHASE = Phase::Select;
                    }
                }
            }
            true
        }
        Mouse::RightDown => {
            cancel();
            true
        }
        _ => false,
    }
}

// ---------- Éditeur ----------

unsafe fn open_editor(img: Dib) {
    close_editor();
    IMG = img;
    PHASE = Phase::Edit;
    TOOL = Tool::Pen;
    TEXT_ON = false;
    TEXT_LEN = 0;
    SAVED_LEN = 0;
    HOVER_BTN = -1;
}

unsafe fn close_editor() {
    free_dib(&mut IMG);
    for d in UNDO.iter_mut() {
        free_dib(d);
    }
    NUNDO = 0;
    PHASE = Phase::Idle;
    DRAGGING = false;
}

unsafe fn push_undo() {
    let snap = crop_dib(&IMG, &rect(0, 0, IMG.w, IMG.h));
    if !snap.ok() {
        return;
    }
    if NUNDO == UNDO_LEVELS {
        free_dib(&mut UNDO[0]);
        for i in 1..UNDO_LEVELS {
            UNDO[i - 1] = UNDO[i];
        }
        UNDO[UNDO_LEVELS - 1] = NO_DIB;
        NUNDO -= 1;
    }
    UNDO[NUNDO] = snap;
    NUNDO += 1;
}

pub unsafe fn undo() -> bool {
    if NUNDO == 0 {
        return false;
    }
    NUNDO -= 1;
    free_dib(&mut IMG);
    IMG = UNDO[NUNDO];
    UNDO[NUNDO] = NO_DIB;
    true
}

/// Copie l'image (CF_DIB 32 bpp, de bas en haut) dans le presse-papiers.
pub unsafe fn copy_image() -> bool {
    if !IMG.ok() {
        return false;
    }
    let size = 40 + IMG.bytes();
    let h = GlobalAlloc(0x0002, size);
    if h.is_null() {
        return false;
    }
    let p = GlobalLock(h) as *mut u8;
    if p.is_null() {
        GlobalFree(h);
        return false;
    }
    let header = BitmapInfoHeader {
        size: 40,
        width: IMG.w,
        height: IMG.h,
        planes: 1,
        bit_count: 32,
        compression: 0,
        size_image: IMG.bytes() as u32,
        x_pels_per_meter: 3780,
        y_pels_per_meter: 3780,
        colors_used: 0,
        colors_important: 0,
    };
    core::ptr::copy_nonoverlapping(&header as *const _ as *const u8, p, 40);
    for y in 0..IMG.h {
        core::ptr::copy_nonoverlapping(
            IMG.bits.add((y * IMG.stride()) as usize),
            p.add(40 + ((IMG.h - 1 - y) * IMG.stride()) as usize),
            IMG.stride() as usize,
        );
    }
    GlobalUnlock(h);
    if OpenClipboard(core::ptr::null_mut()) == 0 {
        return false;
    }
    EmptyClipboard();
    let ok = SetClipboardData(crate::cliphist::CF_DIB, h as usize) != 0;
    CloseClipboard();
    ok
}

/// Enregistre l'image en PNG dans Images\Captures d'écran.
pub unsafe fn save_image() -> bool {
    if !IMG.ok() {
        return false;
    }
    let english = GetFileAttributesA(c"C:\\Pictures".as_ptr()) != u32::MAX;
    let (dir, sub, stem): (&[u8], &[u8], &[u8]) = if english {
        (b"C:\\Pictures", b"\\Screenshots", b"\\Screenshot ")
    } else {
        (
            b"C:\\Images",
            b"\\Captures d'\xE9cran",
            b"\\Capture d'\xE9cran ",
        )
    };
    let mut path = [0u8; 300];
    let n = stamped_path(&mut path, dir, sub, stem, b".png");
    let ok = WinluxSavePng(
        path.as_ptr() as *const core::ffi::c_char,
        IMG.bits,
        IMG.w,
        IMG.h,
        IMG.stride(),
    ) != 0;
    let mut msg = [0u8; 360];
    let mut k = 0;
    let head: &[u8] = if ok {
        tr(b"Capture enregistr\xE9e : ", b"Snip saved: ")
    } else {
        tr(b"\xC9chec de l'enregistrement : ", b"Could not save: ")
    };
    for &c in head.iter().chain(path[3..n].iter()) {
        if k < msg.len() {
            msg[k] = c;
            k += 1;
        }
    }
    crate::notifications::push(tr(b"Outil Capture", b"Snipping Tool"), &msg[..k]);
    if ok {
        let l = (n - 3).min(SAVED_NAME.len());
        SAVED_NAME[..l].copy_from_slice(&path[3..3 + l]);
        SAVED_LEN = l;
    }
    ok
}

struct EdLayout {
    win: Rect,
    tools: Rect,
    canvas: Rect,
    img: Rect,
}

unsafe fn ed_layout() -> EdLayout {
    let max_w = (SCREEN_W - 80).max(480);
    let max_h = (BAR_TOP - 60).max(320);
    let chrome_h = 40 + 48 + 34;
    let w = (IMG.w + 48).clamp(880.min(max_w), max_w);
    let h = (IMG.h + chrome_h + 32).clamp(420.min(max_h), max_h);
    let x = (SCREEN_W - w) / 2;
    let y = ((BAR_TOP - h) / 2).max(8);
    let win = rect(x, y, x + w, y + h);
    let tools = rect(x + 1, y + 40, x + w - 1, y + 88);
    let canvas = rect(x + 16, y + 96, x + w - 16, y + h - 34);
    let (cw, ch) = (canvas.right - canvas.left, canvas.bottom - canvas.top);
    // Échelle ≤ 1 qui fait tenir l'image.
    let (mut dw, mut dh) = (IMG.w, IMG.h);
    if dw > cw || dh > ch {
        if IMG.w as i64 * ch as i64 > IMG.h as i64 * cw as i64 {
            dw = cw;
            dh = (IMG.h as i64 * cw as i64 / IMG.w.max(1) as i64) as i32;
        } else {
            dh = ch;
            dw = (IMG.w as i64 * ch as i64 / IMG.h.max(1) as i64) as i32;
        }
    }
    let ix = canvas.left + (cw - dw) / 2;
    let iy = canvas.top + (ch - dh) / 2;
    EdLayout {
        win,
        tools,
        canvas,
        img: rect(ix, iy, ix + dw.max(1), iy + dh.max(1)),
    }
}

/// Boutons de l'éditeur : 0 rogner, 1 stylo, 2 surligneur, 3 flèche,
/// 4 texte, 5-10 couleurs, 11-13 tailles, 14 annuler, 15 copier,
/// 16 enregistrer, 17 fermer.
const ED_BUTTONS: usize = 18;

unsafe fn ed_button(l: &EdLayout, i: usize) -> Rect {
    let y0 = l.tools.top + 8;
    let y1 = l.tools.bottom - 8;
    if i == 17 {
        return rect(
            l.win.right - 46,
            l.win.top + 1,
            l.win.right - 1,
            l.win.top + 39,
        );
    }
    let mut x = l.tools.left + 12;
    for k in 0..i {
        x += match k {
            0..=4 => 38,
            5..=10 => 28,
            11..=13 => 30,
            _ => 0,
        };
        if k == 0 || k == 4 || k == 10 || k == 13 {
            x += 14;
        }
    }
    match i {
        0..=4 => rect(x, y0, x + 34, y1),
        5..=10 => rect(x, y0 + 5, x + 22, y1 - 5),
        11..=13 => rect(x, y0, x + 26, y1),
        14 => rect(x, y0, x + 34, y1),
        15 => rect(l.tools.right - 228, y0, l.tools.right - 124, y1),
        _ => rect(l.tools.right - 116, y0, l.tools.right - 12, y1),
    }
}

fn tool_of(i: usize) -> Tool {
    match i {
        0 => Tool::Crop,
        1 => Tool::Pen,
        2 => Tool::Highlighter,
        3 => Tool::Arrow,
        _ => Tool::Text,
    }
}

unsafe fn draw_tool_icon(hdc: Handle, b: &mut Brushes, i: usize, r: &Rect, c: u32) {
    let cx = (r.left + r.right) / 2;
    let cy = (r.top + r.bottom) / 2;
    match i {
        0 => {
            // Rognage : deux équerres.
            fill(hdc, b, cx - 6, cy - 10, cx - 4, cy + 6, c);
            fill(hdc, b, cx - 6, cy + 4, cx + 10, cy + 6, c);
            fill(hdc, b, cx - 10, cy - 6, cx + 6, cy - 4, c);
            fill(hdc, b, cx + 4, cy - 6, cx + 6, cy + 10, c);
        }
        1 => {
            line(hdc, b, cx - 8, cy + 8, cx + 7, cy - 7, 3, c);
            fill(hdc, b, cx - 10, cy + 8, cx - 6, cy + 10, c);
        }
        2 => {
            round_rect(hdc, b, cx - 4, cy - 10, cx + 5, cy + 4, 2, c);
            fill(hdc, b, cx - 2, cy + 4, cx + 3, cy + 9, COLORS[1]);
        }
        3 => {
            line(hdc, b, cx - 8, cy + 8, cx + 7, cy - 7, 2, c);
            fill(hdc, b, cx + 1, cy - 9, cx + 9, cy - 7, c);
            fill(hdc, b, cx + 7, cy - 9, cx + 9, cy - 1, c);
        }
        4 => {
            fill(hdc, b, cx - 8, cy - 9, cx + 8, cy - 6, c);
            fill(hdc, b, cx - 1, cy - 9, cx + 2, cy + 9, c);
        }
        14 => {
            // Annuler : flèche courbe.
            line(hdc, b, cx - 7, cy - 2, cx + 3, cy - 2, 2, c);
            line(hdc, b, cx + 3, cy - 2, cx + 8, cy + 3, 2, c);
            line(hdc, b, cx + 8, cy + 3, cx + 3, cy + 8, 2, c);
            line(hdc, b, cx - 7, cy - 2, cx - 3, cy - 6, 2, c);
            line(hdc, b, cx - 7, cy - 2, cx - 3, cy + 2, 2, c);
        }
        _ => {}
    }
}

fn ed_label(i: usize) -> &'static [u8] {
    match i {
        0 => tr(b"Rogner", b"Crop"),
        1 => tr(b"Stylo", b"Pen"),
        2 => tr(b"Surligneur", b"Highlighter"),
        3 => tr(b"Fl\xE8che", b"Arrow"),
        4 => tr(b"Texte", b"Text"),
        11 => tr(b"Fin", b"Thin"),
        12 => tr(b"Moyen", b"Medium"),
        13 => tr(b"\xC9pais", b"Thick"),
        14 => tr(b"Annuler (Ctrl+Z)", b"Undo (Ctrl+Z)"),
        15 => tr(b"Copier (Ctrl+C)", b"Copy (Ctrl+C)"),
        16 => tr(b"Enregistrer (Ctrl+S)", b"Save (Ctrl+S)"),
        17 => tr(b"Fermer", b"Close"),
        _ => tr(b"Couleur", b"Color"),
    }
}

/// Point écran → pixel de l'image.
unsafe fn to_img(l: &EdLayout, x: i32, y: i32) -> (i32, i32) {
    let dw = (l.img.right - l.img.left).max(1);
    let dh = (l.img.bottom - l.img.top).max(1);
    (
        ((x - l.img.left) as i64 * IMG.w as i64 / dw as i64) as i32,
        ((y - l.img.top) as i64 * IMG.h as i64 / dh as i64) as i32,
    )
}

unsafe fn to_screen(l: &EdLayout, p: (i32, i32)) -> (i32, i32) {
    let dw = l.img.right - l.img.left;
    let dh = l.img.bottom - l.img.top;
    (
        l.img.left + (p.0 as i64 * dw as i64 / IMG.w.max(1) as i64) as i32,
        l.img.top + (p.1 as i64 * dh as i64 / IMG.h.max(1) as i64) as i32,
    )
}

fn pen_width(tool: Tool) -> i32 {
    let base = unsafe { SIZES[SIZE] };
    match tool {
        Tool::Highlighter => base * 4 + 6,
        Tool::Arrow => base + 1,
        _ => base,
    }
}

fn text_size() -> i32 {
    unsafe { [16, 22, 32][SIZE] }
}

unsafe fn draw_editor(hdc: Handle, b: &mut Brushes, f: &Fonts) {
    let p = pal();
    let l = ed_layout();
    let w = l.win;
    // Ombre légère puis fenêtre.
    round_rect(
        hdc,
        b,
        w.left - 1,
        w.top + 2,
        w.right + 2,
        w.bottom + 4,
        9,
        p.shadow,
    );
    panel(
        hdc, b, w.left, w.top, w.right, w.bottom, 8, p.window, p.border,
    );
    set_glyph_bg(Some(p.window));
    text(
        hdc,
        f.body,
        p.text,
        w.left + 14,
        w.top + 12,
        tr(b"Outil Capture", b"Snipping Tool"),
    );
    // Dimensions de l'image.
    let mut dim = [0u8; 24];
    let mut n = write_num(&mut dim, 0, IMG.w);
    dim[n] = b' ';
    dim[n + 1] = 0xD7;
    dim[n + 2] = b' ';
    n = write_num(&mut dim, n + 3, IMG.h);
    text(
        hdc,
        f.small,
        p.text3,
        w.left + 14 + text_width(tr(b"Outil Capture", b"Snipping Tool"), 13) + 12,
        w.top + 14,
        &dim[..n],
    );
    // Barre d'outils.
    fill(
        hdc,
        b,
        l.tools.left,
        l.tools.top,
        l.tools.right,
        l.tools.bottom,
        p.mica,
    );
    fill(
        hdc,
        b,
        l.tools.left,
        l.tools.bottom,
        l.tools.right,
        l.tools.bottom + 1,
        p.border,
    );
    for i in 0..ED_BUTTONS {
        let r = ed_button(&l, i);
        let hovered = HOVER_BTN == i as i32;
        match i {
            0..=4 => {
                let on = tool_of(i) == TOOL;
                let bg = if on {
                    p.selected
                } else if hovered {
                    p.hover
                } else {
                    p.mica
                };
                round_rect(hdc, b, r.left, r.top, r.right, r.bottom, 5, bg);
                set_glyph_bg(Some(bg));
                let c = if on && i > 0 && i < 4 {
                    COLORS[COLOR]
                } else {
                    p.text
                };
                let c = if c == COLORS[5] && !DARK { p.text } else { c };
                draw_tool_icon(hdc, b, i, &r, c);
                if on {
                    fill(
                        hdc,
                        b,
                        r.left + 10,
                        r.bottom - 3,
                        r.right - 10,
                        r.bottom - 1,
                        p.accent,
                    );
                }
            }
            5..=10 => {
                let k = i - 5;
                if k == COLOR {
                    ellipse(
                        hdc,
                        b,
                        r.left - 3,
                        r.top - 3,
                        r.right + 3,
                        r.bottom + 3,
                        p.accent,
                    );
                    ellipse(
                        hdc,
                        b,
                        r.left - 1,
                        r.top - 1,
                        r.right + 1,
                        r.bottom + 1,
                        p.mica,
                    );
                }
                ellipse(hdc, b, r.left, r.top, r.right, r.bottom, p.border);
                ellipse(
                    hdc,
                    b,
                    r.left + 1,
                    r.top + 1,
                    r.right - 1,
                    r.bottom - 1,
                    COLORS[k],
                );
            }
            11..=13 => {
                let k = i - 11;
                let bg = if k == SIZE {
                    p.selected
                } else if hovered {
                    p.hover
                } else {
                    p.mica
                };
                round_rect(hdc, b, r.left, r.top, r.right, r.bottom, 5, bg);
                let d = [4, 7, 11][k];
                let cx = (r.left + r.right) / 2;
                let cy = (r.top + r.bottom) / 2;
                ellipse(
                    hdc,
                    b,
                    cx - d / 2,
                    cy - d / 2,
                    cx + d / 2 + 1,
                    cy + d / 2 + 1,
                    p.text,
                );
            }
            14 => {
                let bg = if hovered { p.hover } else { p.mica };
                round_rect(hdc, b, r.left, r.top, r.right, r.bottom, 5, bg);
                set_glyph_bg(Some(bg));
                let c = if NUNDO > 0 { p.text } else { p.text3 };
                draw_tool_icon(hdc, b, 14, &r, c);
            }
            15 | 16 => {
                let primary = i == 16;
                let bg = if primary {
                    p.accent
                } else if hovered {
                    p.hover
                } else {
                    p.surface
                };
                round_rect(
                    hdc,
                    b,
                    r.left,
                    r.top,
                    r.right,
                    r.bottom,
                    5,
                    if primary { p.accent } else { p.border },
                );
                round_rect(
                    hdc,
                    b,
                    r.left + 1,
                    r.top + 1,
                    r.right - 1,
                    r.bottom - 1,
                    4,
                    bg,
                );
                let tc = if primary { p.on_accent } else { p.text };
                let label = if primary {
                    tr(b"Enregistrer", b"Save")
                } else {
                    tr(b"Copier", b"Copy")
                };
                text_centered(
                    hdc,
                    f.body,
                    13,
                    tc,
                    (r.left + r.right) / 2,
                    r.top + 9,
                    label,
                );
            }
            _ => {
                let bg = if hovered {
                    rgb(0xC4, 0x2B, 0x1C)
                } else {
                    p.window
                };
                fill(hdc, b, r.left, r.top, r.right, r.bottom, bg);
                let c = if hovered {
                    rgb(0xFF, 0xFF, 0xFF)
                } else {
                    p.text
                };
                let cx = (r.left + r.right) / 2;
                let cy = (r.top + r.bottom) / 2;
                line(hdc, b, cx - 5, cy - 5, cx + 5, cy + 5, 1, c);
                line(hdc, b, cx + 5, cy - 5, cx - 5, cy + 5, 1, c);
            }
        }
    }
    set_glyph_bg(None);
    // Zone de travail : damier discret puis image.
    fill(
        hdc,
        b,
        l.canvas.left,
        l.canvas.top,
        l.canvas.right,
        l.canvas.bottom,
        p.mica,
    );
    stretch(hdc, &IMG, &l.img);
    frame_rect(hdc, b, &l.img, p.border);
    // Aperçu du geste en cours.
    if DRAGGING {
        let c = COLORS[COLOR];
        match TOOL {
            Tool::Crop => {
                let a = to_screen(&l, START);
                let z = to_screen(&l, CUR);
                let r = norm(a, z);
                frame_rect(hdc, b, &r, rgb(0xFF, 0xFF, 0xFF));
                frame_rect(
                    hdc,
                    b,
                    &rect(r.left - 1, r.top - 1, r.right + 1, r.bottom + 1),
                    rgb(0, 0, 0),
                );
                for (hx, hy) in [
                    (r.left, r.top),
                    (r.right, r.top),
                    (r.left, r.bottom),
                    (r.right, r.bottom),
                ] {
                    fill(
                        hdc,
                        b,
                        hx - 4,
                        hy - 4,
                        hx + 4,
                        hy + 4,
                        rgb(0xFF, 0xFF, 0xFF),
                    );
                }
            }
            Tool::Pen | Tool::Highlighter => {
                let scale_w = (pen_width(TOOL) * (l.img.right - l.img.left) / IMG.w.max(1)).max(1);
                for i in 1..NPOINTS {
                    let a = to_screen(&l, POINTS[i - 1]);
                    let z = to_screen(&l, POINTS[i]);
                    line(hdc, b, a.0, a.1, z.0, z.1, scale_w, c);
                }
            }
            Tool::Arrow => {
                let a = to_screen(&l, START);
                let z = to_screen(&l, CUR);
                let scale_w = (pen_width(TOOL) * (l.img.right - l.img.left) / IMG.w.max(1)).max(1);
                line(hdc, b, a.0, a.1, z.0, z.1, scale_w, c);
                ellipse(hdc, b, z.0 - 5, z.1 - 5, z.0 + 5, z.1 + 5, c);
            }
            _ => {}
        }
    }
    if TEXT_ON {
        let pos = to_screen(&l, TEXT_POS);
        let size = (text_size() * (l.img.right - l.img.left) / IMG.w.max(1)).max(8);
        let font = CreateFontA(
            size,
            0,
            0,
            0,
            700,
            0,
            0,
            0,
            1,
            0,
            0,
            0,
            0,
            c"DejaVu Sans".as_ptr(),
        );
        text(
            hdc,
            font,
            COLORS[COLOR],
            pos.0,
            pos.1,
            &TEXT_BUF[..TEXT_LEN],
        );
        DeleteObject(font);
        let cx = pos.0 + text_width(&TEXT_BUF[..TEXT_LEN], size) + 1;
        if (now() / 500) % 2 == 0 {
            fill(hdc, b, cx, pos.1, cx + 2, pos.1 + size + 2, p.accent);
        }
    }
    // Barre d'état.
    let status: &[u8] = if HOVER_BTN >= 0 {
        ed_label(HOVER_BTN as usize)
    } else if SAVED_LEN > 0 {
        &SAVED_NAME[..SAVED_LEN]
    } else {
        match TOOL {
            Tool::Crop => tr(
                b"Faites glisser pour rogner l'image",
                b"Drag to crop the image",
            ),
            Tool::Text => tr(
                b"Cliquez puis tapez, Entr\xE9e pour valider",
                b"Click then type, Enter to apply",
            ),
            _ => tr(
                b"Dessinez sur la capture. Ctrl+Z annule, \xC9chap ferme.",
                b"Draw on the snip. Ctrl+Z undoes, Esc closes.",
            ),
        }
    };
    text_fit(
        hdc,
        f.small,
        11,
        p.text2,
        w.left + 16,
        w.bottom - 24,
        status,
        w.right - w.left - 32,
    );
}

unsafe fn commit_text() {
    if TEXT_ON && TEXT_LEN > 0 {
        push_undo();
        let mut buf = [0u8; 121];
        buf[..TEXT_LEN].copy_from_slice(&TEXT_BUF[..TEXT_LEN]);
        WinluxDrawText(
            IMG.bits,
            IMG.w,
            IMG.h,
            IMG.stride(),
            TEXT_POS.0,
            TEXT_POS.1,
            buf.as_ptr() as *const core::ffi::c_char,
            text_size(),
            COLORS[COLOR],
        );
    }
    TEXT_ON = false;
    TEXT_LEN = 0;
}

unsafe fn apply_stroke() {
    let mode = match TOOL {
        Tool::Pen => 0,
        Tool::Highlighter => 1,
        Tool::Arrow => 2,
        _ => return,
    };
    let mut pts = [0i32; MAX_POINTS * 2];
    let n = if TOOL == Tool::Arrow {
        pts[..4].copy_from_slice(&[START.0, START.1, CUR.0, CUR.1]);
        2
    } else {
        for i in 0..NPOINTS {
            pts[i * 2] = POINTS[i].0;
            pts[i * 2 + 1] = POINTS[i].1;
        }
        NPOINTS
    };
    if n == 0 || (TOOL == Tool::Arrow && START == CUR) {
        return;
    }
    push_undo();
    WinluxDrawStroke(
        IMG.bits,
        IMG.w,
        IMG.h,
        IMG.stride(),
        pts.as_ptr(),
        n as i32,
        COLORS[COLOR],
        pen_width(TOOL),
        mode,
    );
}

pub unsafe fn select_tool(t: Tool) {
    commit_text();
    TOOL = t;
    if t == Tool::Highlighter && COLOR == 0 {
        COLOR = 1;
    }
}

unsafe fn editor_mouse(ev: Mouse, x: i32, y: i32) -> bool {
    let l = ed_layout();
    let mut hb = -1;
    for i in 0..ED_BUTTONS {
        if inside(&ed_button(&l, i), x, y) {
            hb = i as i32;
        }
    }
    match ev {
        Mouse::Move => {
            let changed = hb != HOVER_BTN || DRAGGING;
            HOVER_BTN = hb;
            if DRAGGING {
                let p = to_img(&l, x, y);
                let p = (p.0.clamp(0, IMG.w - 1), p.1.clamp(0, IMG.h - 1));
                CUR = p;
                if matches!(TOOL, Tool::Pen | Tool::Highlighter) && NPOINTS < MAX_POINTS {
                    let last = POINTS[NPOINTS - 1];
                    if last != p {
                        POINTS[NPOINTS] = p;
                        NPOINTS += 1;
                    }
                }
            }
            changed
        }
        Mouse::Down => {
            if hb >= 0 {
                let i = hb as usize;
                match i {
                    0..=4 => select_tool(tool_of(i)),
                    5..=10 => COLOR = i - 5,
                    11..=13 => SIZE = i - 11,
                    14 => {
                        commit_text();
                        undo();
                    }
                    15 => {
                        commit_text();
                        if copy_image() {
                            crate::desktop::toast(tr(
                                b"Capture copi\xE9e dans le presse-papiers",
                                b"Snip copied to the clipboard",
                            ));
                        }
                    }
                    16 => {
                        commit_text();
                        save_image();
                    }
                    _ => {
                        commit_text();
                        close_editor();
                    }
                }
                return true;
            }
            if !inside(&l.win, x, y) {
                return false;
            }
            if !inside(&l.img, x, y) {
                commit_text();
                return true;
            }
            let p = to_img(&l, x, y);
            match TOOL {
                Tool::Text => {
                    commit_text();
                    TEXT_ON = true;
                    TEXT_POS = p;
                    TEXT_LEN = 0;
                }
                Tool::None => {}
                _ => {
                    commit_text();
                    DRAGGING = true;
                    START = p;
                    CUR = p;
                    POINTS[0] = p;
                    NPOINTS = 1;
                }
            }
            true
        }
        Mouse::Up => {
            if !DRAGGING {
                return false;
            }
            DRAGGING = false;
            if TOOL == Tool::Crop {
                let r = norm(START, CUR);
                if r.right - r.left >= 4 && r.bottom - r.top >= 4 {
                    let c = crop_dib(&IMG, &r);
                    if c.ok() {
                        push_undo();
                        free_dib(&mut IMG);
                        IMG = c;
                    }
                }
            } else {
                apply_stroke();
            }
            NPOINTS = 0;
            true
        }
        _ => false,
    }
}

// ---------- Entrées ----------

/// Souris quand l'outil est actif ; vrai s'il faut repeindre.
pub unsafe fn mouse(ev: Mouse, x: i32, y: i32) -> bool {
    match PHASE {
        Phase::Select | Phase::VideoSetup => select_mouse(ev, x, y),
        Phase::Edit => editor_mouse(ev, x, y),
        Phase::Idle => false,
    }
}

/// Clic dans la barre des tâches : arrêt de l'enregistrement.
pub unsafe fn rec_click(x: i32, rec_x: i32) -> bool {
    if REC_ACTIVE && x >= rec_x && x < rec_x + REC_W {
        stop_recording();
        return true;
    }
    false
}

pub unsafe fn key(vk: u32, ch: u16) -> bool {
    let ctrl = ((GetKeyState(0x11) as u16) & 0x8000) != 0;
    match PHASE {
        Phase::Idle => false,
        Phase::Select | Phase::VideoSetup => {
            if vk == 0x1B || ch == 27 {
                cancel();
            } else if ch == 0 && PHASE == Phase::Select {
                // Raccourcis Windows : flèches pour changer de mode.
                let idx = match MODE {
                    Mode::Rect => 0,
                    Mode::Window => 1,
                    Mode::Full => 2,
                    Mode::Free => 3,
                };
                if vk == 0x27 {
                    MODE = mode_of((idx + 1) % if VIDEO { 3 } else { 4 });
                } else if vk == 0x25 {
                    MODE = mode_of((idx + if VIDEO { 2 } else { 3 }) % if VIDEO { 3 } else { 4 });
                } else if vk == 0x0D && MODE == Mode::Full {
                    finish_capture(rect(0, 0, SCREEN_W, SCREEN_H), false);
                }
            } else if ch == 13 && PHASE == Phase::VideoSetup {
                start_recording();
            }
            true
        }
        Phase::Edit => {
            if TEXT_ON {
                match ch {
                    27 => {
                        TEXT_ON = false;
                        TEXT_LEN = 0;
                    }
                    13 => commit_text(),
                    8 => TEXT_LEN = TEXT_LEN.saturating_sub(1),
                    0x20..=0xFF if !ctrl && TEXT_LEN < TEXT_BUF.len() => {
                        TEXT_BUF[TEXT_LEN] = ch as u8;
                        TEXT_LEN += 1;
                    }
                    _ => {}
                }
                return true;
            }
            match (ch, vk) {
                (27, _) | (0, 0x1B) => close_editor(),
                (26, _) => {
                    undo();
                }
                (3, _) => {
                    if copy_image() {
                        crate::desktop::toast(tr(
                            b"Capture copi\xE9e dans le presse-papiers",
                            b"Snip copied to the clipboard",
                        ));
                    }
                }
                (19, _) => {
                    save_image();
                }
                _ => {}
            }
            true
        }
    }
}

pub unsafe fn draw(hdc: Handle, b: &mut Brushes, f: &Fonts) {
    match PHASE {
        Phase::Select | Phase::VideoSetup => draw_overlay(hdc, b, f),
        Phase::Edit => draw_editor(hdc, b, f),
        Phase::Idle => {}
    }
}

/// Vrai si l'animation (curseur texte, minuteur) demande des repeintes.
pub fn animating() -> bool {
    unsafe { TEXT_ON || DRAGGING }
}

// ---------- Régressions (sous le runtime, sans affichage) ----------

pub unsafe fn regression_checks() {
    SCREEN_W = 320;
    SCREEN_H = 200;
    BAR_TOP = 152;
    let mut src = new_dib(64, 32);
    check!(src.ok(), "Capture : DIB section 32 bpp creee");
    for i in 0..src.bytes() {
        *src.bits.add(i) = 200;
    }
    // Éditeur sur une image unie.
    open_editor(src);
    src = NO_DIB;
    let _ = src;
    check!(
        PHASE == Phase::Edit && image_size() == (64, 32),
        "Capture : editeur ouvert"
    );
    TOOL = Tool::Pen;
    COLOR = 0;
    START = (4, 16);
    POINTS[0] = (4, 16);
    POINTS[1] = (60, 16);
    NPOINTS = 2;
    apply_stroke();
    let px = IMG.bits.add((16 * IMG.stride() + 30 * 4) as usize);
    check!(
        *px.add(2) > 200 && *px < 60 && NUNDO == 1,
        "Capture : le stylo rouge marque l'image (et un annuler est memorise)"
    );
    let undone = undo();
    let px = IMG.bits.add((16 * IMG.stride() + 30 * 4) as usize);
    check!(
        undone && *px.add(2) == 200 && *px == 200,
        "Capture : Ctrl+Z restaure l'image"
    );
    START = (8, 4);
    CUR = (40, 24);
    TOOL = Tool::Crop;
    DRAGGING = true;
    let l = ed_layout();
    let up = to_screen(&l, CUR);
    editor_mouse(Mouse::Up, up.0, up.1);
    check!(
        image_size() == (32, 20),
        "Capture : le rognage recadre l'image"
    );
    check!(
        copy_image(),
        "Capture : copie CF_DIB dans le presse-papiers"
    );
    crate::cliphist::poll();
    check!(
        crate::cliphist::kind(0) == Some(crate::cliphist::Kind::Image),
        "Capture : l'image copiee rejoint l'historique Win+V"
    );
    close_editor();
    check!(!active(), "Capture : fermeture de l'editeur");
    crate::cliphist::clear_unpinned();
}
