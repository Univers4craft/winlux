//! Briques communes aux outils d'administration (Planificateur de tâches,
//! Pare-feu) : zones cliquables enregistrées au dessin, champs de saisie,
//! interrupteurs et boutons Windows 11, fichiers et langue de l'interface.

use crate::gfx::*;
use crate::wm::Gfx;
use winlux_testexe::*;

// ---------- Langue ----------

static mut LANG_CHECKED: bool = false;
static mut LANG_EN: bool = false;

/// Vrai si l'interface n'est pas en français (Paramètres > Langue, sinon
/// `LANG`) : ces outils existent en français et en anglais, l'anglais sert
/// aussi pour l'espagnol et l'allemand.
pub unsafe fn english() -> bool {
    if crate::locale::CURRENT_LANG != crate::locale::Lang::Fr {
        return true;
    }
    if !LANG_CHECKED {
        LANG_CHECKED = true;
        let mut v = [0i8; 64];
        let n = GetEnvironmentVariableA(c"LANG".as_ptr(), v.as_mut_ptr(), 64);
        let two = [v[0] as u8, v[1] as u8];
        LANG_EN = (2..64).contains(&n) && matches!(&two, b"en" | b"es" | b"de");
    }
    LANG_EN
}

/// Texte selon la langue (chaînes en Windows-1252).
pub fn t(fr: &'static [u8], en: &'static [u8]) -> &'static [u8] {
    if unsafe { english() } {
        en
    } else {
        fr
    }
}

// ---------- Zones cliquables ----------

const MAX_HITS: usize = 160;

/// Zones cliquables relevées pendant le dessin ; les clics sont résolus
/// sur la géométrie réellement affichée.
pub struct Hits {
    rects: [Rect; MAX_HITS],
    ids: [u16; MAX_HITS],
    n: usize,
    pub mx: i32,
    pub my: i32,
}

impl Hits {
    pub const fn new() -> Hits {
        Hits {
            rects: [rect(0, 0, 0, 0); MAX_HITS],
            ids: [0; MAX_HITS],
            n: 0,
            mx: -1,
            my: -1,
        }
    }
    pub fn clear(&mut self) {
        self.n = 0;
    }
    /// Enregistre une zone et indique si la souris la survole.
    pub fn add(&mut self, r: Rect, id: u16) -> bool {
        if self.n < MAX_HITS {
            self.rects[self.n] = r;
            self.ids[self.n] = id;
            self.n += 1;
        }
        inside(&r, self.mx, self.my)
    }
    /// Zone sous le point (la dernière dessinée l'emporte).
    pub fn find(&self, x: i32, y: i32) -> Option<u16> {
        (0..self.n)
            .rev()
            .find(|&i| inside(&self.rects[i], x, y))
            .map(|i| self.ids[i])
    }
}

// ---------- Texte ----------

/// Tampon de texte borné.
#[derive(Clone, Copy)]
pub struct Field<const N: usize> {
    pub buf: [u8; N],
    pub len: usize,
}

impl<const N: usize> Field<N> {
    pub const fn new() -> Self {
        Field {
            buf: [0; N],
            len: 0,
        }
    }
    pub fn get(&self) -> &[u8] {
        &self.buf[..self.len]
    }
    pub fn set(&mut self, s: &[u8]) {
        let n = s.len().min(N);
        self.buf[..n].copy_from_slice(&s[..n]);
        self.len = n;
    }
    pub fn push(&mut self, s: &[u8]) {
        for &c in s {
            if self.len < N {
                self.buf[self.len] = c;
                self.len += 1;
            }
        }
    }
    /// Saisie clavier : vrai si le texte a changé.
    pub fn key(&mut self, ch: u16) -> bool {
        match ch {
            8 => {
                self.len = self.len.saturating_sub(1);
                true
            }
            32..=126 | 160..=255 if self.len < N => {
                self.buf[self.len] = ch as u8;
                self.len += 1;
                true
            }
            _ => false,
        }
    }
}

/// Écrit un nombre décimal.
pub fn push_num<const N: usize>(f: &mut Field<N>, v: i64) {
    let mut tmp = [0u8; 20];
    let mut i = tmp.len();
    let neg = v < 0;
    let mut n = v.unsigned_abs();
    loop {
        i -= 1;
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    if neg {
        f.push(b"-");
    }
    f.push(&tmp[i..]);
}

pub fn parse_num(s: &[u8]) -> Option<u32> {
    if s.is_empty() || s.len() > 9 || !s.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(s.iter().fold(0u32, |n, &c| n * 10 + (c - b'0') as u32))
}

pub fn trim(s: &[u8]) -> &[u8] {
    let a = s.iter().position(|&c| c != b' ').unwrap_or(s.len());
    let b = s.iter().rposition(|&c| c != b' ').map_or(a, |i| i + 1);
    &s[a..b]
}

/// Valeur de `clé=` dans un texte `clé=valeur` (première occurrence).
pub fn field<'a>(data: &'a [u8], key: &[u8]) -> &'a [u8] {
    for line in data.split(|&c| c == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.len() > key.len() && line.starts_with(key) && line[key.len()] == b'=' {
            return &line[key.len() + 1..];
        }
    }
    b""
}

/// Champ séparé par des tabulations.
pub fn column(line: &[u8], index: usize) -> &[u8] {
    line.split(|&c| c == b'\t').nth(index).unwrap_or(b"")
}

// ---------- Fichiers ----------

/// Chemin `Z:\…` terminé par un zéro.
pub struct Path {
    buf: [u8; 260],
    len: usize,
}

impl Path {
    pub fn new(base: &[u8]) -> Path {
        let mut p = Path {
            buf: [0; 260],
            len: 0,
        };
        p.push(base);
        p
    }
    pub fn push(&mut self, s: &[u8]) -> &mut Path {
        for &c in s {
            if self.len < 258 {
                self.buf[self.len] = c;
                self.len += 1;
            }
        }
        self.buf[self.len] = 0;
        self
    }
    pub fn ptr(&self) -> *const i8 {
        self.buf.as_ptr() as *const i8
    }
}

pub unsafe fn exists(p: *const i8) -> bool {
    GetFileAttributesA(p) != u32::MAX
}

/// Lit un fichier ; avec `tail`, garde la fin s'il dépasse le tampon.
pub unsafe fn read_file(path: *const i8, buf: &mut [u8], tail: bool) -> usize {
    let f = CreateFileA(
        path,
        0x8000_0000,
        7,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if f.is_null() || f == -1isize as Handle {
        return 0;
    }
    if tail {
        let size = GetFileSize(f, core::ptr::null_mut());
        if size != u32::MAX && size as usize > buf.len() {
            SetFilePointer(
                f,
                (size as usize - buf.len()) as i32,
                core::ptr::null_mut(),
                0,
            );
        }
    }
    let mut total = 0usize;
    while total < buf.len() {
        let mut read = 0u32;
        let ok = ReadFile(
            f,
            buf[total..].as_mut_ptr(),
            (buf.len() - total) as u32,
            &mut read,
            core::ptr::null_mut(),
        );
        if ok == 0 || read == 0 {
            break;
        }
        total += read as usize;
    }
    CloseHandle(f);
    total
}

/// Remplace le contenu d'un fichier (fichier temporaire puis renommage).
pub unsafe fn write_file(path: &Path, data: &[u8]) -> bool {
    let mut tmp = Path::new(&path.buf[..path.len]);
    tmp.push(b".new");
    let f = CreateFileA(
        tmp.ptr(),
        0x4000_0000,
        0,
        core::ptr::null_mut(),
        2,
        0,
        core::ptr::null_mut(),
    );
    if f.is_null() || f == -1isize as Handle {
        return false;
    }
    let mut written = 0u32;
    let ok = WriteFile(
        f,
        data.as_ptr(),
        data.len() as u32,
        &mut written,
        core::ptr::null_mut(),
    );
    CloseHandle(f);
    if ok == 0 || written as usize != data.len() {
        DeleteFileA(tmp.ptr());
        return false;
    }
    DeleteFileA(path.ptr());
    MoveFileA(tmp.ptr(), path.ptr()) != 0
}

/// Lance `winlux ARGS` (services d'administration du runtime).
pub unsafe fn winlux(args: &[u8]) -> bool {
    let mut a = [0u8; 300];
    let n = args.len().min(298);
    a[..n].copy_from_slice(&args[..n]);
    ShellExecuteA(
        core::ptr::null_mut(),
        c"open".as_ptr(),
        c"winlux".as_ptr(),
        a.as_ptr() as *const i8,
        core::ptr::null(),
        0,
    ) as isize
        > 32
}

// ---------- Contrôles ----------

pub unsafe fn label(g: &mut Gfx, x: i32, y: i32, s: &[u8]) {
    text(g.hdc, g.f.caption, pal().text2, x, y, s);
}

/// Bouton de barre de commandes ou de boîte de dialogue.
pub unsafe fn button(
    g: &mut Gfx,
    h: &mut Hits,
    r: Rect,
    id: u16,
    s: &[u8],
    primary: bool,
    enabled: bool,
) {
    let p = pal();
    let hover = h.add(r, id) && enabled;
    if primary && enabled {
        let c = if hover {
            mix(p.accent, p.text, 1, 8)
        } else {
            p.accent
        };
        round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, c);
        text_centered(
            g.hdc,
            g.f.body,
            13,
            p.on_accent,
            (r.left + r.right) / 2,
            r.top + (r.bottom - r.top - 15) / 2,
            s,
        );
    } else {
        let bg = if hover { p.hover } else { p.surface };
        panel(
            g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, bg, p.border,
        );
        let c = if enabled { p.text } else { p.text3 };
        text_centered(
            g.hdc,
            g.f.body,
            13,
            c,
            (r.left + r.right) / 2,
            r.top + (r.bottom - r.top - 15) / 2,
            s,
        );
    }
}

/// Largeur d'un bouton pour son libellé.
pub fn button_w(s: &[u8]) -> i32 {
    text_width(s, 13) + 28
}

/// Puce sélectionnable (choix exclusif ou multiple).
pub unsafe fn chip(
    g: &mut Gfx,
    h: &mut Hits,
    x: i32,
    y: i32,
    id: u16,
    s: &[u8],
    selected: bool,
) -> i32 {
    let p = pal();
    let w = text_width(s, 13) + 24;
    let r = rect(x, y, x + w, y + 30);
    let hover = h.add(r, id);
    if selected {
        round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 15, p.accent);
        text(g.hdc, g.f.body, p.on_accent, x + 12, y + 7, s);
    } else {
        panel(
            g.hdc,
            g.b,
            r.left,
            r.top,
            r.right,
            r.bottom,
            15,
            if hover { p.hover } else { p.surface },
            p.border,
        );
        text(g.hdc, g.f.body, p.text, x + 12, y + 7, s);
    }
    w + 8
}

/// Interrupteur Windows 11 suivi de son libellé.
pub unsafe fn toggle(g: &mut Gfx, h: &mut Hits, x: i32, y: i32, id: u16, on: bool, s: &[u8]) {
    let p = pal();
    let w = 40 + 12 + text_width(s, 13);
    h.add(rect(x, y - 2, x + w, y + 22), id);
    if on {
        round_rect(g.hdc, g.b, x, y, x + 40, y + 20, 10, p.accent);
        ellipse(g.hdc, g.b, x + 24, y + 4, x + 36, y + 16, p.on_accent);
    } else {
        round_rect(g.hdc, g.b, x, y, x + 40, y + 20, 10, p.text2);
        round_rect(g.hdc, g.b, x + 1, y + 1, x + 39, y + 19, 9, p.surface);
        ellipse(g.hdc, g.b, x + 5, y + 5, x + 15, y + 15, p.text2);
    }
    text(g.hdc, g.f.body, p.text, x + 52, y + 2, s);
}

/// Champ de saisie ; le texte trop long montre sa fin.
pub unsafe fn input(
    g: &mut Gfx,
    h: &mut Hits,
    r: Rect,
    id: u16,
    value: &[u8],
    focused: bool,
    placeholder: &[u8],
) {
    let p = pal();
    h.add(r, id);
    panel(
        g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, p.surface, p.border,
    );
    if focused {
        fill(
            g.hdc,
            g.b,
            r.left + 2,
            r.bottom - 2,
            r.right - 2,
            r.bottom,
            p.accent,
        );
    }
    let max = r.right - r.left - 20;
    let mut start = 0;
    while start < value.len() && text_width(&value[start..], 13) > max {
        start += 1;
    }
    let ty = r.top + (r.bottom - r.top - 15) / 2;
    if value.is_empty() && !focused {
        text(g.hdc, g.f.body, p.text3, r.left + 10, ty, placeholder);
    } else {
        text(g.hdc, g.f.body, p.text, r.left + 10, ty, &value[start..]);
    }
    if focused {
        let cx = r.left + 10 + text_width(&value[start..], 13) + 1;
        fill(g.hdc, g.b, cx, ty, cx + 1, ty + 16, p.text);
    }
}

/// Cellule de tableau tronquée.
pub unsafe fn cell(g: &mut Gfx, x: i32, y: i32, w: i32, s: &[u8], color: u32) {
    text_fit(g.hdc, g.f.body, 13, color, x, y, s, w - 12);
}

/// En-tête de colonnes ; renvoie l'ordonnée de la première ligne.
pub unsafe fn header(g: &mut Gfx, x: i32, y: i32, right: i32, cols: &[(&[u8], i32)]) -> i32 {
    let p = pal();
    let mut cx = x;
    for (name, w) in cols {
        text_fit(g.hdc, g.f.caption, 12, p.text2, cx, y + 6, name, w - 10);
        cx += w;
    }
    fill(g.hdc, g.b, x - 8, y + 28, right, y + 29, p.border);
    y + 32
}

/// Navigation latérale ; renvoie vrai si l'entrée est survolée.
pub unsafe fn nav_item(
    g: &mut Gfx,
    h: &mut Hits,
    x: i32,
    y: i32,
    w: i32,
    id: u16,
    s: &[u8],
    glyph_kind: Glyph,
    active: bool,
) {
    let p = pal();
    let r = rect(x + 4, y, x + w - 4, y + 36);
    let hover = h.add(r, id);
    if active || hover {
        round_rect(
            g.hdc,
            g.b,
            r.left,
            r.top,
            r.right,
            r.bottom,
            4,
            if active { p.selected } else { p.hover },
        );
    }
    if active {
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
    set_glyph_bg(Some(if active {
        p.selected
    } else if hover {
        p.hover
    } else {
        p.mica
    }));
    glyph(g.hdc, g.b, g.f, glyph_kind, x + 16, y + 9, 18);
    set_glyph_bg(None);
    text_fit(g.hdc, g.f.body, 13, p.text, x + 44, y + 10, s, w - 52);
}

/// Carte de boîte de dialogue centrée sur la zone cliente.
pub unsafe fn dialog(g: &mut Gfx, h: &mut Hits, c: &Rect, w: i32, ht: i32, title: &[u8]) -> Rect {
    let p = pal();
    // Voile : un clic hors de la carte ne traverse pas.
    h.add(*c, 0xFFFF);
    let veil = mix(p.window, p.shadow, 4, 10);
    fill(g.hdc, g.b, c.left, c.top, c.right, c.bottom, veil);
    let w = w.min(c.right - c.left - 24);
    let ht = ht.min(c.bottom - c.top - 24);
    let x = (c.left + c.right - w) / 2;
    let y = (c.top + c.bottom - ht) / 2;
    panel(g.hdc, g.b, x, y, x + w, y + ht, 8, p.window, p.border);
    h.add(rect(x, y, x + w, y + ht), 0xFFFE);
    text(g.hdc, g.f.large, p.text, x + 24, y + 20, title);
    rect(x, y, x + w, y + ht)
}
