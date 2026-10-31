//! Gestionnaire de polices : liste des polices installées (système et
//! utilisateur, via fontconfig), aperçu d'une police à plusieurs tailles avec
//! un texte modifiable, installation d'un .ttf/.otf/.ttc/.otc (bouton ou
//! glisser-déposer depuis l'Explorateur) et désinstallation des polices de
//! l'utilisateur. Le service `winlux-fonts` écrit Z:\run\winlux\fonts.txt.
//!
//! Les polices installées ici sont aussi celles que voient les .exe
//! (EnumFontFamilies, CreateFont) ; l'aperçu d'une police fraîchement
//! installée passe par AddFontResourceA, comme une application Windows.

use crate::gfx::*;
use crate::store::{
    button, contains_ci, cp1252_to_utf8, read_file, spawn, t, utf8_to_cp1252, wrap, Buf,
};
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

const CAP: usize = 256 * 1024;
const MAX_FACES: usize = 3000;
const MAX_FAMILIES: usize = 1500;
static mut DATA: [u8; CAP] = [0; CAP];
static mut DATA_LEN: usize = 0;
/// (famille, style, portée, chemin) en positions dans DATA.
static mut FACES: [[(u32, u16); 4]; MAX_FACES] = [[(0, 0); 4]; MAX_FACES];
static mut FACE_COUNT: usize = 0;
/// Première face de chaque famille et nombre de styles.
static mut FAMILIES: [(u16, u16); MAX_FAMILIES] = [(0, 0); MAX_FAMILIES];
static mut FAMILY_COUNT: usize = 0;
static mut LOADED: bool = false;
static mut WAIT_UNTIL: u32 = 0;

static mut SELECTED: usize = 0;
static mut STYLE: usize = 0;
static mut LIST_SCROLL: i32 = 0;
static mut SEARCH: Buf<40> = Buf::new();
static mut SEARCH_FOCUS: bool = false;
static mut SAMPLE: Buf<96> = Buf::new();
static mut SAMPLE_FOCUS: bool = false;
static mut PICKER: bool = false;
static mut CONFIRM: bool = false;
static mut ACTION_PENDING: bool = false;
static mut ACTION_STARTED: u32 = 0;
static mut MX: i32 = -1;
static mut MY: i32 = -1;

const SIZES: [i32; 6] = [12, 16, 24, 32, 48, 72];
static mut PREVIEW: [Handle; 7] = [core::ptr::null_mut(); 7];
static mut PREVIEW_FOR: (usize, usize) = (usize::MAX, usize::MAX);

static mut FILES: [[u8; 128]; 24] = [[0; 128]; 24];
static mut FILE_LENS: [usize; 24] = [0; 24];
static mut FILE_COUNT: usize = 0;

#[derive(Clone, Copy, PartialEq)]
enum Action {
    None,
    Family(u16),
    Style(u8),
    Search,
    Sample,
    Install,
    Pick(u8),
    ClosePicker,
    Remove,
}
const MAX_HITS: usize = 200;
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
unsafe fn part(face: usize, k: usize) -> &'static [u8] {
    let (o, n) = FACES[face][k];
    &DATA[o as usize..o as usize + n as usize]
}

unsafe fn load() {
    let n = read_file(c"Z:\\run\\winlux\\fonts.txt", &mut DATA);
    if n == 0 {
        return;
    }
    let previous: Buf<64> = {
        let mut b = Buf::new();
        if SELECTED < FAMILY_COUNT {
            b.push(part(FAMILIES[SELECTED].0 as usize, 0));
        }
        b
    };
    DATA_LEN = utf8_to_cp1252(&mut DATA, n);
    FACE_COUNT = 0;
    FAMILY_COUNT = 0;
    let mut pos = 0;
    while pos < DATA_LEN && FACE_COUNT < MAX_FACES {
        let end = DATA[pos..DATA_LEN]
            .iter()
            .position(|&c| c == b'\n')
            .map_or(DATA_LEN, |k| pos + k);
        if DATA[pos..end].starts_with(b"font=") {
            let mut start = pos + 5;
            let mut fields = [(0u32, 0u16); 4];
            for (k, slot) in fields.iter_mut().enumerate() {
                let stop = if k == 3 {
                    end
                } else {
                    DATA[start..end]
                        .iter()
                        .position(|&c| c == b'|')
                        .map_or(end, |p| start + p)
                };
                *slot = (start as u32, (stop - start) as u16);
                start = (stop + 1).min(end);
            }
            FACES[FACE_COUNT] = fields;
            let fam = part(FACE_COUNT, 0);
            let same = FAMILY_COUNT > 0
                && crate::store::lower_eq(part(FAMILIES[FAMILY_COUNT - 1].0 as usize, 0), fam);
            if same {
                FAMILIES[FAMILY_COUNT - 1].1 += 1;
            } else if FAMILY_COUNT < MAX_FAMILIES {
                FAMILIES[FAMILY_COUNT] = (FACE_COUNT as u16, 1);
                FAMILY_COUNT += 1;
            }
            FACE_COUNT += 1;
        }
        pos = end + 1;
    }
    LOADED = true;
    SELECTED = (0..FAMILY_COUNT)
        .find(|&f| part(FAMILIES[f].0 as usize, 0) == previous.get())
        .unwrap_or(SELECTED.min(FAMILY_COUNT.saturating_sub(1)));
    if previous.n == 0 {
        STYLE = regular_style(SELECTED);
    }
    PREVIEW_FOR = (usize::MAX, usize::MAX);
}

pub unsafe fn open() {
    if SAMPLE.n == 0 {
        SAMPLE.push(t(
            b"Portez ce vieux whisky au juge blond qui fume. 0123456789",
            b"The quick brown fox jumps over the lazy dog. 0123456789",
        ));
    }
    spawn(c"winlux-fonts", b"list");
    WAIT_UNTIL = GetTickCount().wrapping_add(6000);
    crate::wm::open(crate::wm::APP_FONTS);
}

unsafe fn run_action(verb: &[u8], path: &[u8]) {
    let mut args: Buf<300> = Buf::new();
    args.push(verb).push(b" \"").push(path).push(b"\"");
    let mut raw = [0u8; 600];
    let n = cp1252_to_utf8(args.get(), &mut raw);
    DeleteFileA(c"Z:\\run\\winlux\\fonts-action.txt".as_ptr());
    if spawn(c"winlux-fonts", &raw[..n]) {
        ACTION_PENDING = true;
        ACTION_STARTED = GetTickCount();
    } else {
        crate::desktop::toast(t(
            b"Le service des polices n'est pas disponible.",
            b"The font service is not available.",
        ));
    }
}

fn is_font(name: &[u8]) -> bool {
    [&b".ttf"[..], b".otf", b".ttc", b".otc"]
        .iter()
        .any(|e| name.len() > 4 && contains_ci(&name[name.len() - 4..], e))
}

/// Fichier glissé depuis l'Explorateur sur la fenêtre Polices.
pub unsafe fn install_dropped(path: &[u8]) {
    if is_font(path) {
        run_action(b"install", path);
    } else {
        crate::desktop::toast(t(
            b"D\xE9posez un fichier de police .ttf, .otf ou .ttc.",
            b"Drop a .ttf, .otf or .ttc font file.",
        ));
    }
}

unsafe fn scan_files() {
    FILE_COUNT = 0;
    for dir in [
        &b"C:\\T\xE9l\xE9chargements\\"[..],
        b"C:\\Downloads\\",
        b"C:\\Documents\\",
        b"C:\\Bureau\\",
        b"C:\\Desktop\\",
    ] {
        let mut pat = [0u8; 64];
        pat[..dir.len()].copy_from_slice(dir);
        pat[dir.len()] = b'*';
        let mut data: Win32FindDataA = core::mem::zeroed();
        let h = FindFirstFileA(pat.as_ptr().cast(), &mut data);
        if h.is_null() || h == -1isize as Handle {
            continue;
        }
        loop {
            let raw = core::slice::from_raw_parts(data.file_name.as_ptr().cast::<u8>(), 260);
            let len = raw.iter().position(|&c| c == 0).unwrap_or(0);
            if is_font(&raw[..len]) && FILE_COUNT < 24 && dir.len() + len < 128 {
                let f = &mut FILES[FILE_COUNT];
                f[..dir.len()].copy_from_slice(dir);
                f[dir.len()..dir.len() + len].copy_from_slice(&raw[..len]);
                FILE_LENS[FILE_COUNT] = dir.len() + len;
                FILE_COUNT += 1;
            }
            if FindNextFileA(h, &mut data) == 0 {
                break;
            }
        }
        FindClose(h);
    }
}

/// Graisse et italique déduits du nom de style fontconfig.
fn style_attrs(style: &[u8]) -> (i32, u32) {
    let italic = contains_ci(style, b"italic") || contains_ci(style, b"oblique");
    let weight = if contains_ci(style, b"thin") {
        100
    } else if contains_ci(style, b"extralight") || contains_ci(style, b"ultralight") {
        200
    } else if contains_ci(style, b"light") {
        300
    } else if contains_ci(style, b"medium") {
        500
    } else if contains_ci(style, b"semibold") || contains_ci(style, b"demi") {
        600
    } else if contains_ci(style, b"extrabold") || contains_ci(style, b"ultrabold") {
        800
    } else if contains_ci(style, b"black") || contains_ci(style, b"heavy") {
        900
    } else if contains_ci(style, b"bold") || contains_ci(style, b"gras") {
        700
    } else {
        400
    };
    (weight, italic as u32)
}

/// Style affiché par défaut : le style normal de la famille s'il existe.
unsafe fn regular_style(fam: usize) -> usize {
    if fam >= FAMILY_COUNT {
        return 0;
    }
    let (first, count) = FAMILIES[fam];
    (0..count as usize)
        .min_by_key(|&k| {
            let (w, it) = style_attrs(part(first as usize + k, 1));
            (it as i32) * 1000 + (w - 400).abs()
        })
        .unwrap_or(0)
}

unsafe fn release_preview() {
    for h in PREVIEW.iter_mut() {
        if !h.is_null() {
            DeleteObject(*h);
            *h = core::ptr::null_mut();
        }
    }
}

unsafe fn ensure_preview() {
    if PREVIEW_FOR == (SELECTED, STYLE) || SELECTED >= FAMILY_COUNT {
        return;
    }
    release_preview();
    let (first, count) = FAMILIES[SELECTED];
    let face = first as usize + STYLE.min(count as usize - 1);
    if part(face, 2) == b"user" {
        // Police ajoutée après le démarrage : comme une application Windows,
        // on la rend disponible au processus avant de l'utiliser.
        let mut path = [0u8; 300];
        let p = part(face, 3);
        let n = p.len().min(299);
        path[..n].copy_from_slice(&p[..n]);
        AddFontResourceA(path.as_ptr().cast());
    }
    let mut name = [0u8; 64];
    let fam = part(face, 0);
    let n = fam.len().min(63);
    name[..n].copy_from_slice(&fam[..n]);
    let (weight, italic) = style_attrs(part(face, 1));
    for (k, h) in PREVIEW.iter_mut().enumerate() {
        let size = if k < SIZES.len() { SIZES[k] } else { 34 };
        *h = CreateFontA(
            size,
            0,
            0,
            0,
            weight,
            italic,
            0,
            0,
            1,
            0,
            0,
            0,
            0,
            name.as_ptr().cast(),
        );
    }
    PREVIEW_FOR = (SELECTED, STYLE);
}

unsafe fn search_box(g: &mut Gfx, r: Rect, value: &[u8], hint: &[u8], focus: bool, a: Action) {
    let p = pal();
    let bg = if focus { p.surface } else { p.surface2 };
    panel(
        g.hdc,
        g.b,
        r.left,
        r.top,
        r.right,
        r.bottom,
        5,
        bg,
        if focus { p.accent } else { p.border },
    );
    if value.is_empty() {
        text(g.hdc, g.f.body, p.text3, r.left + 10, r.top + 8, hint);
    } else {
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            r.left + 10,
            r.top + 8,
            value,
            r.right - r.left - 20,
        );
    }
    if focus && (GetTickCount() / 500) % 2 == 0 {
        let cx = r.left + 10 + text_width(value, 13).min(r.right - r.left - 20);
        fill(g.hdc, g.b, cx, r.top + 7, cx + 1, r.bottom - 7, p.text);
    }
    hit(r, a);
}

unsafe fn visible(f: usize) -> bool {
    SEARCH.n == 0 || contains_ci(part(FAMILIES[f].0 as usize, 0), SEARCH.get())
}

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    HIT_COUNT = 0;
    let p = pal();
    let c = f.client;
    let list_w = ((c.right - c.left) * 34 / 100).clamp(220, 320);
    fill(g.hdc, g.b, c.left, c.top, c.left + list_w, c.bottom, p.mica);
    fill(
        g.hdc,
        g.b,
        c.left + list_w,
        c.top,
        c.right,
        c.bottom,
        p.window,
    );
    set_glyph_bg(Some(p.mica));
    glyph(g.hdc, g.b, g.f, Glyph::Text, c.left + 18, c.top + 16, 20);
    text(
        g.hdc,
        g.f.title,
        p.text,
        c.left + 48,
        c.top + 17,
        t(b"Polices", b"Fonts"),
    );
    let mut count: Buf<40> = Buf::new();
    count
        .num(FAMILY_COUNT as u64)
        .push(t(b" familles", b" families"));
    text(
        g.hdc,
        g.f.caption,
        p.text3,
        c.left + 48,
        c.top + 38,
        count.get(),
    );
    search_box(
        g,
        rect(c.left + 14, c.top + 62, c.left + list_w - 14, c.top + 94),
        SEARCH.get(),
        t(b"Rechercher une police", b"Search fonts"),
        SEARCH_FOCUS,
        Action::Search,
    );
    let ir = rect(c.left + 14, c.top + 104, c.left + list_w - 14, c.top + 136);
    let label = t(b"Installer une police", b"Install a font");
    let br = button(g, ir.left, ir.top, label, true, hot(&ir));
    hit(br, Action::Install);

    // Liste des familles.
    let top = c.top + 148;
    let row_h = 34;
    let mut y = top - LIST_SCROLL;
    for fam in 0..FAMILY_COUNT {
        if !visible(fam) {
            continue;
        }
        if y >= top && y + row_h <= c.bottom - 4 {
            let r = rect(c.left + 8, y, c.left + list_w - 8, y + row_h - 2);
            let sel = fam == SELECTED;
            if sel || hot(&r) {
                round_rect(
                    g.hdc,
                    g.b,
                    r.left,
                    r.top,
                    r.right,
                    r.bottom,
                    5,
                    if sel { p.selected } else { p.hover },
                );
            }
            if sel {
                round_rect(
                    g.hdc,
                    g.b,
                    r.left,
                    r.top + 9,
                    r.left + 3,
                    r.bottom - 9,
                    2,
                    p.accent,
                );
            }
            let first = FAMILIES[fam].0 as usize;
            text_fit(
                g.hdc,
                g.f.body,
                13,
                p.text,
                r.left + 14,
                r.top + 8,
                part(first, 0),
                r.right - r.left - 90,
            );
            let mut tag: Buf<24> = Buf::new();
            if (0..FAMILIES[fam].1 as usize).any(|k| part(first + k, 2) == b"user") {
                tag.push(t(b"Vous", b"You"));
            } else {
                tag.num(FAMILIES[fam].1 as u64);
            }
            let tw = text_width(tag.get(), 11);
            text(
                g.hdc,
                g.f.small,
                p.text3,
                r.right - tw - 10,
                r.top + 10,
                tag.get(),
            );
            hit(r, Action::Family(fam as u16));
        }
        y += row_h;
    }
    if !LOADED {
        text(
            g.hdc,
            g.f.body,
            p.text2,
            c.left + 22,
            top + 8,
            t(b"Chargement des polices...", b"Loading fonts..."),
        );
    }

    // Aperçu.
    set_glyph_bg(Some(p.window));
    let a = rect(
        c.left + list_w + 32,
        c.top + 20,
        c.right - 32,
        c.bottom - 20,
    );
    if SELECTED < FAMILY_COUNT {
        ensure_preview();
        let (first, count) = FAMILIES[SELECTED];
        let face = first as usize + STYLE.min(count as usize - 1);
        text_fit(
            g.hdc,
            PREVIEW[6],
            34,
            p.text,
            a.left,
            a.top,
            part(face, 0),
            a.right - a.left,
        );
        let mut sub: Buf<80> = Buf::new();
        sub.num(count as u64)
            .push(t(b" style(s)  \x95  ", b" style(s)  \x95  "));
        sub.push(if part(face, 2) == b"user" {
            t(b"Install\xE9e pour vous", b"Installed for you")
        } else {
            t(b"Police syst\xE8me", b"System font")
        });
        text(g.hdc, g.f.caption, p.text2, a.left, a.top + 48, sub.get());
        // Styles.
        let mut x = a.left;
        let mut sy = a.top + 74;
        for k in 0..count as usize {
            let st = part(first as usize + k, 1);
            let w = text_width(st, 12) + 20;
            if x + w > a.right {
                x = a.left;
                sy += 30;
            }
            let r = rect(x, sy, x + w, sy + 24);
            let on = k == STYLE;
            round_rect(
                g.hdc,
                g.b,
                r.left,
                r.top,
                r.right,
                r.bottom,
                12,
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
                x + 10,
                sy + 5,
                st,
            );
            hit(r, Action::Style(k as u8));
            x += w + 6;
        }
        let mut y = sy + 40;
        search_box(
            g,
            rect(a.left, y, a.right, y + 34),
            SAMPLE.get(),
            t(b"Tapez un texte d'exemple", b"Type sample text"),
            SAMPLE_FOCUS,
            Action::Sample,
        );
        y += 50;
        for (k, size) in SIZES.iter().enumerate() {
            if y + size + 12 > a.bottom - 70 {
                break;
            }
            let mut lbl: Buf<8> = Buf::new();
            lbl.num(*size as u64);
            text(g.hdc, g.f.small, p.text3, a.left, y + size / 3, lbl.get());
            text_fit(
                g.hdc,
                PREVIEW[k],
                *size,
                p.text,
                a.left + 36,
                y,
                SAMPLE.get(),
                a.right - a.left - 36,
            );
            y += size + size / 3 + 10;
        }
        // Fichier et désinstallation.
        let by = a.bottom - 56;
        fill(g.hdc, g.b, a.left, by - 8, a.right, by - 7, p.border);
        text_fit(
            g.hdc,
            g.f.small,
            11,
            p.text3,
            a.left,
            by + 2,
            part(face, 3),
            a.right - a.left - 240,
        );
        text(
            g.hdc,
            g.f.small,
            p.text3,
            a.left,
            by + 20,
            t(
                b"Disponible aussi pour les programmes .exe (EnumFontFamilies, CreateFont).",
                b"Also available to .exe programs (EnumFontFamilies, CreateFont).",
            ),
        );
        if part(face, 2) == b"user" {
            let label = if CONFIRM {
                t(b"Confirmer la d\xE9sinstallation", b"Confirm uninstall")
            } else {
                t(b"D\xE9sinstaller", b"Uninstall")
            };
            let w = text_width(label, 13) + 28;
            let probe = rect(a.right - w, by, a.right, by + 32);
            let r = button(g, a.right - w, by, label, false, hot(&probe));
            hit(r, Action::Remove);
        }
    } else if LOADED {
        wrap(
            g,
            g.f.body,
            13,
            p.text2,
            a.left,
            a.top + 40,
            a.right - a.left,
            t(
                b"Aucune police trouv\xE9e. Installez un fichier .ttf ou .otf, ou glissez-le ici depuis l'Explorateur.",
                b"No fonts found. Install a .ttf or .otf file, or drop it here from Explorer.",
            ),
            4,
            a.bottom,
        );
    }

    if PICKER {
        draw_picker(g, f);
    }
    set_glyph_bg(None);
}

unsafe fn draw_picker(g: &mut Gfx, f: &Frame) {
    let p = pal();
    let c = f.client;
    let w = (c.right - c.left - 80).min(560);
    let h = (c.bottom - c.top - 80).min(420);
    let x = c.left + (c.right - c.left - w) / 2;
    let y = c.top + (c.bottom - c.top - h) / 2;
    let full = rect(c.left, c.top, c.right, c.bottom);
    hit(full, Action::ClosePicker);
    panel(g.hdc, g.b, x, y, x + w, y + h, 8, p.surface, p.border);
    set_glyph_bg(Some(p.surface));
    hit(rect(x, y, x + w, y + h), Action::None);
    text(
        g.hdc,
        g.f.title,
        p.text,
        x + 24,
        y + 20,
        t(b"Installer une police", b"Install a font"),
    );
    wrap(
        g,
        g.f.caption,
        12,
        p.text2,
        x + 24,
        y + 46,
        w - 48,
        t(
            b"Polices trouv\xE9es dans T\xE9l\xE9chargements, Documents et Bureau. Vous pouvez aussi glisser un fichier depuis l'Explorateur sur cette fen\xEAtre.",
            b"Fonts found in Downloads, Documents and Desktop. You can also drop a file from Explorer onto this window.",
        ),
        3,
        y + h,
    );
    let mut ry = y + 96;
    if FILE_COUNT == 0 {
        text(
            g.hdc,
            g.f.body,
            p.text3,
            x + 24,
            ry,
            t(
                b"Aucun fichier de police trouv\xE9.",
                b"No font file found.",
            ),
        );
    }
    for k in 0..FILE_COUNT {
        if ry + 36 > y + h - 56 {
            break;
        }
        let r = rect(x + 16, ry, x + w - 16, ry + 34);
        if hot(&r) {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 5, p.hover);
            set_glyph_bg(Some(p.hover));
        }
        glyph(g.hdc, g.b, g.f, Glyph::File, r.left + 8, r.top + 8, 18);
        set_glyph_bg(Some(p.surface));
        let path = &FILES[k][..FILE_LENS[k]];
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            r.left + 36,
            r.top + 9,
            path,
            r.right - r.left - 48,
        );
        hit(r, Action::Pick(k as u8));
        ry += 38;
    }
    let label = t(b"Fermer", b"Close");
    let bw = text_width(label, 13) + 28;
    let probe = rect(x + w - bw - 20, y + h - 48, x + w - 20, y + h - 16);
    let r = button(g, x + w - bw - 20, y + h - 48, label, false, hot(&probe));
    hit(r, Action::ClosePicker);
}

unsafe fn act(a: Action) -> bool {
    match a {
        Action::None => return PICKER,
        Action::Family(k) => {
            SELECTED = k as usize;
            STYLE = regular_style(SELECTED);
            CONFIRM = false;
        }
        Action::Style(k) => STYLE = k as usize,
        Action::Search => {
            SEARCH_FOCUS = true;
            SAMPLE_FOCUS = false;
        }
        Action::Sample => {
            SAMPLE_FOCUS = true;
            SEARCH_FOCUS = false;
        }
        Action::Install => {
            scan_files();
            PICKER = true;
        }
        Action::Pick(k) => {
            let k = k as usize;
            if k < FILE_COUNT {
                let mut path: Buf<128> = Buf::new();
                path.push(&FILES[k][..FILE_LENS[k]]);
                run_action(b"install", path.get());
            }
            PICKER = false;
        }
        Action::ClosePicker => PICKER = false,
        Action::Remove => {
            if !CONFIRM {
                CONFIRM = true;
            } else if SELECTED < FAMILY_COUNT {
                CONFIRM = false;
                let (first, count) = FAMILIES[SELECTED];
                let face = first as usize + STYLE.min(count as usize - 1);
                let mut path: Buf<256> = Buf::new();
                path.push(part(face, 3));
                let mut c = [0u8; 300];
                let n = path.n.min(299);
                c[..n].copy_from_slice(&path.b[..n]);
                RemoveFontResourceA(c.as_ptr().cast());
                release_preview();
                PREVIEW_FOR = (usize::MAX, usize::MAX);
                run_action(b"remove", path.get());
            }
        }
    }
    true
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    match ev {
        Mouse::Move => {
            let changed =
                (0..HIT_COUNT).any(|k| inside(&HITS[k].0, x, y) != inside(&HITS[k].0, MX, MY));
            MX = x;
            MY = y;
            changed
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
            if !matches!(chosen, Action::Sample) {
                SAMPLE_FOCUS = false;
            }
            if !matches!(chosen, Action::Remove) {
                CONFIRM = false;
            }
            act(chosen);
            true
        }
        Mouse::Wheel(delta) => {
            if PICKER {
                return false;
            }
            let shown = (0..FAMILY_COUNT).filter(|&k| visible(k)).count() as i32;
            let max = (shown * 34 - (f.client.bottom - f.client.top - 160)).max(0);
            LIST_SCROLL = (LIST_SCROLL - delta / 120 * 68).clamp(0, max);
            true
        }
        _ => false,
    }
}

pub unsafe fn key(vk: u32, ch: u16) -> bool {
    if ch != 0 {
        if ch == 27 {
            PICKER = false;
            SEARCH_FOCUS = false;
            SAMPLE_FOCUS = false;
            return true;
        }
        if SAMPLE_FOCUS {
            match ch {
                8 => SAMPLE.n = SAMPLE.n.saturating_sub(1),
                13 => SAMPLE_FOCUS = false,
                c if (0x20..0x100).contains(&c) => {
                    SAMPLE.push(&[c as u8]);
                }
                _ => return false,
            }
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
        LIST_SCROLL = 0;
        if let Some(k) = (0..FAMILY_COUNT).find(|&k| visible(k)) {
            if !visible(SELECTED) {
                SELECTED = k;
                STYLE = regular_style(k);
            }
        }
        return true;
    }
    let step = |dir: i32| {
        let mut k = SELECTED as i32;
        loop {
            k += dir;
            if k < 0 || k >= FAMILY_COUNT as i32 {
                return false;
            }
            if visible(k as usize) {
                SELECTED = k as usize;
                STYLE = regular_style(SELECTED);
                return true;
            }
        }
    };
    match vk {
        0x26 => step(-1),
        0x28 => step(1),
        _ => false,
    }
}

pub unsafe fn tick(_f: &Frame) -> bool {
    let now = GetTickCount();
    let mut repaint = SEARCH_FOCUS || SAMPLE_FOCUS;
    if !LOADED || (now.wrapping_sub(WAIT_UNTIL) as i32) < 0 {
        load();
        repaint = true;
    }
    if ACTION_PENDING && now.wrapping_sub(ACTION_STARTED) > 30_000 {
        ACTION_PENDING = false;
    }
    if ACTION_PENDING && now.wrapping_sub(ACTION_STARTED) > 300 {
        let mut buf = [0u8; 512];
        let n = read_file(c"Z:\\run\\winlux\\fonts-action.txt", &mut buf);
        let n = utf8_to_cp1252(&mut buf, n);
        let text = &buf[..n];
        let mut message: &[u8] = b"";
        for line in text.split(|&c| c == b'\n') {
            if let Some(v) = line.strip_prefix(b"message=") {
                message = v;
            }
        }
        // Le service réécrit fonts-action.txt à chaque opération.
        if !message.is_empty() {
            ACTION_PENDING = false;
            crate::desktop::toast(message);
            WAIT_UNTIL = now.wrapping_add(3000);
            load();
            repaint = true;
        }
    }
    repaint
}
