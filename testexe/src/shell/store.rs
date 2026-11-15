//! Store Winlux : une seule bibliothèque pour les logiciels Linux (apt,
//! Flatpak, Snap, AppImage) et Windows (catalogues interchangeables, .exe
//! exécutés par le runtime). Le service `winlux-store` (voir docs/STORE.md)
//! écrit l'index, les fiches et l'avancement dans Z:\run\winlux ; cette
//! application les relit et lui transmet les actions.
//!
//! Vues : Accueil (sélection mise en avant et rangées par catégorie),
//! catégorie, recherche, Bibliothèque (mises à jour unifiées, logiciels
//! installés, programmes Windows, installation d'un fichier) et fiche
//! détaillée (captures en carrousel, description, informations, versions).

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

// ---------- Outils partagés (aussi utilisés par fonts.rs) ----------

/// Langue de l'interface : 0 français, 1 anglais, 2 espagnol, 3 allemand.
pub fn lang() -> usize {
    unsafe { crate::admin_ui::lang() as usize }
}
/// Libellé localisé (octets Windows-1252). Les appels donnent le français
/// et l'anglais ; l'espagnol et l'allemand viennent de `STORE_TRANSLATIONS`
/// (l'anglais sert si une entrée manque).
pub fn t(fr: &'static [u8], english: &'static [u8]) -> &'static [u8] {
    let col = lang();
    if col < 2 {
        return if col == 0 { fr } else { english };
    }
    STORE_TRANSLATIONS
        .iter()
        .find(|r| r.0 == fr && r.1 == english)
        .map_or(english, |r| if col == 2 { r.2 } else { r.3 })
}

/// Lit un fichier entier dans `buf` ; renvoie le nombre d'octets lus.
pub unsafe fn read_file(path: &core::ffi::CStr, buf: &mut [u8]) -> usize {
    let f = CreateFileA(
        path.as_ptr(),
        0x8000_0000,
        3,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if f.is_null() || f == -1isize as Handle {
        return 0;
    }
    let mut total = 0usize;
    while total < buf.len() {
        let mut read = 0u32;
        let chunk = (buf.len() - total).min(1 << 16) as u32;
        if ReadFile(
            f,
            buf.as_mut_ptr().add(total),
            chunk,
            &mut read,
            core::ptr::null_mut(),
        ) == 0
            || read == 0
        {
            break;
        }
        total += read as usize;
    }
    CloseHandle(f);
    total
}

/// Convertit en place un texte UTF-8 en Windows-1252 (TextOutA) ; les
/// octets qui ne forment pas une séquence UTF-8 valide sont gardés.
pub fn utf8_to_cp1252(buf: &mut [u8], len: usize) -> usize {
    let (mut i, mut o) = (0, 0);
    while i < len {
        let b = buf[i];
        let (cp, n) = if b < 0x80 {
            (b as u32, 1)
        } else if b & 0xE0 == 0xC0 && i + 1 < len && buf[i + 1] & 0xC0 == 0x80 {
            ((((b & 0x1F) as u32) << 6) | (buf[i + 1] & 0x3F) as u32, 2)
        } else if b & 0xF0 == 0xE0
            && i + 2 < len
            && buf[i + 1] & 0xC0 == 0x80
            && buf[i + 2] & 0xC0 == 0x80
        {
            (
                (((b & 0x0F) as u32) << 12)
                    | (((buf[i + 1] & 0x3F) as u32) << 6)
                    | (buf[i + 2] & 0x3F) as u32,
                3,
            )
        } else if b & 0xF8 == 0xF0 && i + 3 < len {
            (0x3F, 4)
        } else {
            (b as u32, 1)
        };
        buf[o] = match cp {
            0..=0x7F | 0xA0..=0xFF => cp as u8,
            0x80..=0x9F if n == 1 => cp as u8,
            0x20AC => 0x80,
            0x201A => 0x82,
            0x201E => 0x84,
            0x2026 => 0x85,
            0x2018 | 0x2019 | 0x2032 => b'\'',
            0x201C | 0x201D => b'"',
            0x2022 | 0x25CF => 0x95,
            0x2013 => 0x96,
            0x2014 => 0x97,
            0x2122 => 0x99,
            0x0152 => 0x8C,
            0x0153 => 0x9C,
            0x0160 => 0x8A,
            0x0161 => 0x9A,
            0x0178 => 0x9F,
            0x017D => 0x8E,
            0x017E => 0x9E,
            _ => b'?',
        };
        o += 1;
        i += n;
    }
    o
}

/// Lance `program args` (service Linux) sans attendre.
pub unsafe fn spawn(program: &core::ffi::CStr, args: &[u8]) -> bool {
    let mut buf = [0u8; 1100];
    let n = args.len().min(buf.len() - 1);
    buf[..n].copy_from_slice(&args[..n]);
    ShellExecuteA(
        core::ptr::null_mut(),
        c"open".as_ptr(),
        program.as_ptr(),
        buf.as_ptr().cast(),
        core::ptr::null(),
        0,
    ) as isize
        > 32
}

/// Petit tampon d'octets de taille fixe.
#[derive(Clone, Copy)]
pub struct Buf<const N: usize> {
    pub b: [u8; N],
    pub n: usize,
}
impl<const N: usize> Buf<N> {
    pub const fn new() -> Self {
        Self { b: [0; N], n: 0 }
    }
    pub fn get(&self) -> &[u8] {
        &self.b[..self.n]
    }
    pub fn clear(&mut self) {
        self.n = 0;
    }
    pub fn push(&mut self, s: &[u8]) -> &mut Self {
        let k = s.len().min(N - self.n);
        self.b[self.n..self.n + k].copy_from_slice(&s[..k]);
        self.n += k;
        self
    }
    pub fn num(&mut self, mut v: u64) -> &mut Self {
        let mut d = [0u8; 20];
        let mut i = 20;
        loop {
            i -= 1;
            d[i] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        self.push(&d[i..])
    }
    /// Copie terminée par un zéro (pour les API « A »).
    pub fn cstr(&self, out: &mut [u8]) -> *const core::ffi::c_char {
        let k = self.n.min(out.len() - 1);
        out[..k].copy_from_slice(&self.b[..k]);
        out[k] = 0;
        out.as_ptr().cast()
    }
}

pub fn lower(c: u8) -> u8 {
    match c {
        b'A'..=b'Z' => c + 32,
        0xC0..=0xDE if c != 0xD7 => c + 32,
        _ => c,
    }
}
pub fn lower_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| lower(*x) == lower(*y))
}
/// Recherche insensible à la casse et aux accents (Windows-1252).
pub fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    // Ni casse ni accents : « resume » trouve « Résumé ».
    crate::locale::contains_fold(hay, needle)
}
pub fn parse_num(s: &[u8]) -> u64 {
    s.iter()
        .take_while(|c| c.is_ascii_digit())
        .fold(0u64, |a, &c| a * 10 + (c - b'0') as u64)
}

/// Texte avec retour à la ligne aux espaces ; renvoie l'ordonnée suivante.
pub unsafe fn wrap(
    g: &mut Gfx,
    font: Handle,
    h: i32,
    color: u32,
    x: i32,
    mut y: i32,
    w: i32,
    s: &[u8],
    max_lines: i32,
    bottom: i32,
) -> i32 {
    let line_h = h + h / 2;
    let mut start = 0;
    let mut lines = 0;
    while start < s.len() && lines < max_lines {
        let mut end = start;
        let mut last_fit = start;
        while end < s.len() {
            let next = s[end..]
                .iter()
                .position(|&c| c == b' ')
                .map_or(s.len(), |p| end + p);
            if text_width(&s[start..next], h) > w && last_fit > start {
                break;
            }
            last_fit = next;
            end = next + 1;
            if next >= s.len() {
                break;
            }
        }
        let stop = last_fit.max(start + 1).min(s.len());
        if y + line_h <= bottom {
            if lines == max_lines - 1 && stop < s.len() {
                text_fit(g.hdc, font, h, color, x, y, &s[start..], w);
            } else {
                text(g.hdc, font, color, x, y, &s[start..stop]);
            }
        }
        y += line_h;
        lines += 1;
        start = stop;
        while start < s.len() && s[start] == b' ' {
            start += 1;
        }
    }
    y
}

/// Vignette : image PNG réelle, sinon pastille colorée avec l'initiale.
pub unsafe fn app_icon(g: &mut Gfx, path: &[u8], name: &[u8], x: i32, y: i32, size: i32, bg: u32) {
    if !path.is_empty() {
        let mut c = [0u8; 260];
        let k = path.len().min(259);
        c[..k].copy_from_slice(&path[..k]);
        if WinluxDrawIconA(g.hdc, c.as_ptr().cast(), x, y, size, bg) != 0 {
            return;
        }
    }
    let hash = name
        .iter()
        .fold(7u32, |a, &c| a.wrapping_mul(31).wrapping_add(c as u32));
    const COLORS: [u32; 8] = [
        rgb(0x00, 0x78, 0xD4),
        rgb(0x8E, 0x44, 0xAD),
        rgb(0x10, 0x89, 0x3E),
        rgb(0xCA, 0x50, 0x10),
        rgb(0xC2, 0x39, 0xB3),
        rgb(0x03, 0x83, 0x87),
        rgb(0xD1, 0x34, 0x38),
        rgb(0x5C, 0x6B, 0xC0),
    ];
    round_rect(
        g.hdc,
        g.b,
        x,
        y,
        x + size,
        y + size,
        size / 5,
        COLORS[(hash % 8) as usize],
    );
    let initial = [name.first().map_or(b'?', |c| c.to_ascii_uppercase())];
    let font = if size >= 64 { g.f.display } else { g.f.large };
    let fh = if size >= 64 { 28 } else { 22 };
    text_centered(
        g.hdc,
        font,
        fh,
        rgb(0xFF, 0xFF, 0xFF),
        x + size / 2,
        y + size / 2 - fh * 6 / 10,
        &initial,
    );
}

/// Bouton plein (accent) ou discret ; renvoie son rectangle.
pub unsafe fn button(g: &mut Gfx, x: i32, y: i32, label: &[u8], primary: bool, hot: bool) -> Rect {
    let p = pal();
    // Libellé en demi-gras : un peu plus large que la mesure normale.
    let w = text_width(label, 13) * 108 / 100 + 28;
    let r = rect(x, y, x + w, y + 32);
    let (bg, fg) = if primary {
        (
            if hot {
                mix(p.accent, p.text, 1, 8)
            } else {
                p.accent
            },
            p.on_accent,
        )
    } else {
        (if hot { p.hover } else { p.surface2 }, p.text)
    };
    if !primary {
        round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 5, p.border);
        round_rect(
            g.hdc,
            g.b,
            r.left + 1,
            r.top + 1,
            r.right - 1,
            r.bottom - 1,
            4,
            bg,
        );
    } else {
        round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 5, bg);
    }
    text(g.hdc, g.f.semibold, fg, x + 14, y + 8, label);
    r
}

pub unsafe fn progress_bar(g: &mut Gfx, x: i32, y: i32, w: i32, pct: i32) {
    let p = pal();
    round_rect(g.hdc, g.b, x, y, x + w, y + 4, 2, p.border);
    if pct < 0 {
        // Avancement inconnu : la piste seule, jamais un chiffre inventé.
        return;
    }
    let fill_w = (w * pct.clamp(0, 100) / 100).max(4);
    round_rect(g.hdc, g.b, x, y, x + fill_w, y + 4, 2, p.accent);
}

// ---------- Données relues depuis le service ----------

const INDEX_CAP: usize = 2 << 20;
const MAX_APPS: usize = 6000;
const MAX_WIN: usize = 32;
const F_KEY: usize = 0;
const F_CAT: usize = 1;
const F_NAME: usize = 2;
const F_SUMMARY: usize = 3;
const F_PUB: usize = 4;
const F_STATE: usize = 5;
const F_VERSION: usize = 6;
const F_AVAIL: usize = 7;
const F_ICON: usize = 9;
const F_BADGE: usize = 10;
const F_FEATURED: usize = 11;
const FIELDS: usize = 13;

static mut INDEX: [u8; INDEX_CAP] = [0; INDEX_CAP];
static mut INDEX_LEN: usize = 0;
static mut APPS: [[(u32, u16); FIELDS]; MAX_APPS] = [[(0, 0); FIELDS]; MAX_APPS];
static mut APP_COUNT: usize = 0;
static mut WIN_APPS: [[(u32, u16); 2]; MAX_WIN] = [[(0, 0); 2]; MAX_WIN];
static mut WIN_COUNT: usize = 0;
static mut UPDATES: u64 = 0;
static mut SYSTEM_UPDATES: u64 = 0;
static mut REFRESHING: bool = false;
static mut LOADED: bool = false;
static mut SOURCES: Buf<80> = Buf::new();

unsafe fn slice(r: (u32, u16)) -> &'static [u8] {
    &INDEX[r.0 as usize..r.0 as usize + r.1 as usize]
}
unsafe fn app_field(i: usize, f: usize) -> &'static [u8] {
    slice(APPS[i][f])
}

/// Boutique ouverte par son propre lanceur (winlux-store-catalogs) : seules
/// ses clés (« flatpak: », « windows:scoop-games/ ») sont affichées, sous son
/// nom. Vide : toute la bibliothèque.
static mut SOURCE_FILTER: Buf<48> = Buf::new();
static mut SOURCE_TITLE: Buf<48> = Buf::new();
static mut FILTER_READ: bool = false;

/// Dossier d'échange avec `winlux-store` : propre à chaque compte
/// (`WINLUX_STORE_RUN`, posé par winlux-shell-app), sinon `Z:\run\winlux`.
/// Partagé, le fichier d'index écrit par un premier compte ne pouvait pas
/// être remplacé par un second (dossier « sticky ») : Store vide.
static mut RUN_DIR: Buf<240> = Buf::new();

unsafe fn run_file(name: &[u8]) -> [u8; 300] {
    if RUN_DIR.n == 0 {
        let mut value = [0u8; 200];
        let n = GetEnvironmentVariableA(
            c"WINLUX_STORE_RUN".as_ptr(),
            value.as_mut_ptr().cast(),
            value.len() as u32,
        ) as usize;
        if n > 1
            && n < value.len()
            && value[0] == b'/'
            && value[..n].iter().all(|c| c.is_ascii_graphic())
        {
            RUN_DIR.push(b"Z:");
            for &c in &value[..n] {
                RUN_DIR.push(&[if c == b'/' { b'\\' } else { c }]);
            }
        } else {
            RUN_DIR.push(b"Z:\\run\\winlux");
        }
    }
    let mut out = [0u8; 300];
    let dir = RUN_DIR.get();
    let n = name.len().min(out.len() - dir.len() - 2);
    out[..dir.len()].copy_from_slice(dir);
    out[dir.len()] = b'\\';
    out[dir.len() + 1..dir.len() + 1 + n].copy_from_slice(&name[..n]);
    out
}

unsafe fn read_run(name: &[u8], buf: &mut [u8]) -> usize {
    let path = run_file(name);
    read_file(
        core::ffi::CStr::from_bytes_until_nul(&path).unwrap_or(c""),
        buf,
    )
}

unsafe fn read_filter() {
    if FILTER_READ {
        return;
    }
    FILTER_READ = true;
    let mut value = [0u8; 48];
    let n = GetEnvironmentVariableA(
        c"WINLUX_STORE_SOURCE".as_ptr(),
        value.as_mut_ptr().cast(),
        48,
    ) as usize;
    if n > 0 && n < 48 {
        SOURCE_FILTER.push(&value[..n]);
        let n = GetEnvironmentVariableA(
            c"WINLUX_STORE_TITLE".as_ptr(),
            value.as_mut_ptr().cast(),
            48,
        ) as usize;
        if n > 0
            && n < 48
            && value[..n]
                .iter()
                .all(|c| c.is_ascii_graphic() || *c == b' ')
        {
            SOURCE_TITLE.push(&value[..n]);
        }
    }
}

/// Titre du volet de navigation : « Store » ou le nom de la boutique.
unsafe fn store_title() -> &'static [u8] {
    read_filter();
    if SOURCE_TITLE.n > 0 {
        SOURCE_TITLE.get()
    } else {
        b"Store"
    }
}

unsafe fn load_index() {
    read_filter();
    let n = read_run(b"store.txt", &mut INDEX);
    if n == 0 {
        return;
    }
    INDEX_LEN = utf8_to_cp1252(&mut INDEX, n);
    APP_COUNT = 0;
    WIN_COUNT = 0;
    LOADED = true;
    let mut pos = 0usize;
    while pos < INDEX_LEN {
        let end = INDEX[pos..INDEX_LEN]
            .iter()
            .position(|&c| c == b'\n')
            .map_or(INDEX_LEN, |k| pos + k);
        let line = &INDEX[pos..end];
        let value = |prefix: &[u8]| line.strip_prefix(prefix);
        if let Some(v) = value(b"app=") {
            if APP_COUNT < MAX_APPS && (SOURCE_FILTER.n == 0 || v.starts_with(SOURCE_FILTER.get()))
            {
                split_fields(pos + 4, v, &mut APPS[APP_COUNT]);
                APP_COUNT += 1;
            }
        } else if let Some(v) = value(b"win=") {
            if WIN_COUNT < MAX_WIN {
                split_fields(pos + 4, v, &mut WIN_APPS[WIN_COUNT]);
                WIN_COUNT += 1;
            }
        } else if let Some(v) = value(b"updates=") {
            UPDATES = parse_num(v);
        } else if let Some(v) = value(b"system_updates=") {
            SYSTEM_UPDATES = parse_num(v);
        } else if let Some(v) = value(b"refreshing=") {
            REFRESHING = v == b"1";
        } else if let Some(v) = value(b"sources=") {
            SOURCES.clear();
            SOURCES.push(v);
        }
        pos = end + 1;
    }
}

fn split_fields<const K: usize>(base: usize, v: &[u8], out: &mut [(u32, u16); K]) {
    let mut start = 0;
    for (i, slot) in out.iter_mut().enumerate() {
        let end = v[start.min(v.len())..]
            .iter()
            .position(|&c| c == b'|')
            .map_or(v.len(), |k| start + k);
        *slot = if start <= v.len() {
            ((base + start) as u32, (end - start.min(end)) as u16)
        } else {
            (0, 0)
        };
        start = end + 1;
        if i + 1 == K {
            break;
        }
    }
}

// Fiche détaillée.
const DETAIL_CAP: usize = 64 * 1024;
static mut DETAIL: [u8; DETAIL_CAP] = [0; DETAIL_CAP];
static mut DETAIL_LEN: usize = 0;
static mut DETAIL_READY: bool = false;
static mut DETAIL_PARTIAL: bool = false;

unsafe fn detail_lines(prefix: &[u8], mut f: impl FnMut(&'static [u8])) {
    let all: &'static [u8] = &DETAIL[..DETAIL_LEN];
    for line in all.split(|&c| c == b'\n') {
        if let Some(v) = line.strip_prefix(prefix) {
            f(v);
        }
    }
}
unsafe fn detail_value(prefix: &[u8]) -> &'static [u8] {
    let mut out: &'static [u8] = b"";
    detail_lines(prefix, |v| {
        if out.is_empty() {
            out = v
        }
    });
    out
}

unsafe fn load_detail() {
    let n = read_run(b"store-detail.txt", &mut DETAIL);
    let len = utf8_to_cp1252(&mut DETAIL, n);
    DETAIL_LEN = len;
    let key = detail_value(b"key=");
    DETAIL_READY = len > 0 && key == SELECTED_KEY.get();
    // Fiche partielle : les captures arrivent ensuite, on continue à relire.
    DETAIL_PARTIAL = DETAIL_READY && detail_value(b"partial=") == b"1";
    if DETAIL_READY {
        SHOT_COUNT = 0;
        detail_lines(b"shot=", |_| SHOT_COUNT += 1);
    }
}

// Avancement de l'opération en cours.
static mut OP_ACTIVE: bool = false;
static mut OP_TARGET: Buf<160> = Buf::new();
static mut OP_PERCENT: i32 = 0;
static mut OP_MESSAGE: Buf<200> = Buf::new();
/// Résultat de la dernière opération, gardé dans le Store (#1449) : une
/// réussite 12 s, un échec ou une annulation jusqu'à la croix ou l'opération
/// suivante (la bulle seule disparaissait avant d'être lue).
static mut RESULT: Buf<200> = Buf::new();
static mut RESULT_ERROR: bool = false;
static mut RESULT_AT: u32 = 0;
pub const RESULT_MS: u32 = 12_000;
static mut OP_STARTED: u32 = 0;
/// Clic sur « Installer » pendant une opération : dit dans le bandeau même
/// qu'il faut attendre, la notification seule passait inaperçue (#2186).
static mut BUSY_AT: u32 = 0;
pub const BUSY_MS: u32 = 5_000;

unsafe fn poll_progress() -> bool {
    let mut buf = [0u8; 1024];
    let n = read_run(b"store-progress.txt", &mut buf);
    let n = utf8_to_cp1252(&mut buf, n);
    let text = &buf[..n];
    let mut state: &[u8] = b"";
    for line in text.split(|&c| c == b'\n') {
        if let Some(v) = line.strip_prefix(b"state=") {
            state = v;
        } else if let Some(v) = line.strip_prefix(b"percent=") {
            // Vide : étape sans avancement mesurable, aucun pourcentage (#1455).
            OP_PERCENT = if v.is_empty() { -1 } else { parse_num(v) as i32 };
        } else if let Some(v) = line.strip_prefix(b"message=") {
            OP_MESSAGE.clear();
            OP_MESSAGE.push(v);
        } else if let Some(v) = line.strip_prefix(b"target=") {
            if !v.is_empty() {
                OP_TARGET.clear();
                OP_TARGET.push(v);
            }
        }
    }
    // Le fichier est effacé avant chaque lancement : un état final est donc
    // celui de cette opération. Sans réponse du service, on abandonne.
    if n == 0 && GetTickCount().wrapping_sub(OP_STARTED) > 20_000 {
        OP_ACTIVE = false;
        return true;
    }
    if state == b"done" || state == b"error" {
        OP_ACTIVE = false;
        RESULT.clear();
        RESULT.push(OP_MESSAGE.get());
        RESULT_ERROR = state == b"error";
        RESULT_AT = GetTickCount();
        crate::desktop::toast(OP_MESSAGE.get());
        load_index();
        if VIEW == View::Detail {
            request_details();
        }
    }
    true
}

// ---------- État de l'interface ----------

#[derive(Clone, Copy, PartialEq)]
enum View {
    Home,
    /// Applications d'une catégorie (`ALL` : toutes sauf les jeux).
    Category(u8),
    Windows,
    Library,
    Detail,
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    None,
    Nav(u8),
    Open(u16),
    Install(u16),
    Remove(u16),
    Update(u16),
    UpdateAll,
    Back,
    Shot(i8),
    Alt(u8),
    SeeAll(u8),
    InstallFile(u8),
    Search,
    ClearSearch,
    Hero(i8),
    HeroPick(u8),
    Refresh,
    Homepage,
    DetailInstall,
    DetailRemove,
    DetailUpdate,
    DismissResult,
}

const MAX_HITS: usize = 320;
static mut HITS: [(Rect, Action); MAX_HITS] = [(rect(0, 0, 0, 0), Action::None); MAX_HITS];
static mut HIT_COUNT: usize = 0;
static mut MX: i32 = -1;
static mut MY: i32 = -1;
static mut VIEW: View = View::Home;
static mut PREV_VIEW: View = View::Home;
static mut SCROLL: i32 = 0;
static mut CONTENT_H: i32 = 0;
static mut SEARCH: Buf<48> = Buf::new();
static mut SEARCH_FOCUS: bool = false;
static mut SELECTED_KEY: Buf<160> = Buf::new();
static mut SELECTED: Option<usize> = None;
static mut SHOT: usize = 0;
static mut SHOT_COUNT: usize = 0;
static mut HERO: usize = 0;
static mut HERO_TICK: u32 = 0;
static mut LAST_LOAD: u32 = 0;
static mut CONFIRM_REMOVE: bool = false;
static mut FILES: [[u8; 96]; 24] = [[0; 96]; 24];
static mut FILE_LENS: [usize; 24] = [0; 24];
static mut FILE_COUNT: usize = 0;

/// Catégories : clé du catalogue, libellés (fr, en, es, de), icône. Même
/// ordre que `CATEGORY_RULES` de winlux-store (#716) ; `JEUX` a sa propre
/// entrée dans la barre de gauche.
const CATS: [(&[u8], [&[u8]; 4], Glyph); 11] = [
    (
        b"bureautique",
        [b"Bureautique", b"Productivity", b"Ofim\xE1tica", b"B\xFCro"],
        Glyph::Text,
    ),
    (
        b"internet",
        [b"Internet", b"Internet", b"Internet", b"Internet"],
        Glyph::Browser,
    ),
    (
        b"multimedia",
        [
            b"Audio et vid\xE9o",
            b"Audio and video",
            b"Audio y v\xEDdeo",
            b"Audio und Video",
        ],
        Glyph::Video,
    ),
    (
        b"graphisme",
        [
            b"Photo et graphisme",
            b"Photo and design",
            b"Foto y dise\xF1o",
            b"Foto und Grafik",
        ],
        Glyph::Image,
    ),
    (
        b"developpement",
        [
            b"D\xE9veloppement",
            b"Development",
            b"Desarrollo",
            b"Entwicklung",
        ],
        Glyph::Code,
    ),
    (
        b"jeux",
        [b"Jeux", b"Games", b"Juegos", b"Spiele"],
        Glyph::Star,
    ),
    (
        b"education",
        [b"\xC9ducation", b"Education", b"Educaci\xF3n", b"Bildung"],
        Glyph::Notes,
    ),
    (
        b"sciences",
        [b"Sciences", b"Science", b"Ciencia", b"Wissenschaft"],
        Glyph::Calc,
    ),
    (
        b"utilitaires",
        [
            b"Utilitaires",
            b"Utilities",
            b"Utilidades",
            b"Dienstprogramme",
        ],
        Glyph::Tools,
    ),
    (
        b"systeme",
        [b"Syst\xE8me", b"System", b"Sistema", b"System"],
        Glyph::Computer,
    ),
    (
        b"accessibilite",
        [
            b"Accessibilit\xE9",
            b"Accessibility",
            b"Accesibilidad",
            b"Barrierefreiheit",
        ],
        Glyph::Settings,
    ),
];
const JEUX: usize = 5;
const UTILITAIRES: usize = 8;
/// Vue « Applications » sans filtre de catégorie.
const ALL: u8 = 255;

fn cat_label(i: usize) -> &'static [u8] {
    CATS[i].1[lang()]
}
unsafe fn cat_of(i: usize) -> usize {
    let c = app_field(i, F_CAT);
    // Ancienne clé (catalogue local d'avant la refonte).
    let c: &[u8] = if c == b"navigateurs" { b"internet" } else { c };
    CATS.iter().position(|x| x.0 == c).unwrap_or(UTILITAIRES)
}
unsafe fn is_windows(i: usize) -> bool {
    app_field(i, F_KEY).starts_with(b"windows:")
}

/// Programme .exe installé et utilisé sur le banc de Winlux (#1515).
unsafe fn is_tested(i: usize) -> bool {
    app_field(i, F_KEY).starts_with(b"windows:valides/")
}

unsafe fn hit(r: Rect, a: Action) {
    if HIT_COUNT < MAX_HITS {
        HITS[HIT_COUNT] = (r, a);
        HIT_COUNT += 1;
    }
    // Atteignable par Tab (#28) ; la recherche est un champ de saisie.
    if matches!(a, Action::Search) {
        focus_field(&r);
    } else {
        focus_zone(&r);
    }
}
unsafe fn hot(r: &Rect) -> bool {
    inside(r, MX, MY)
}

unsafe fn request_refresh(force: bool) {
    spawn(
        c"winlux-store",
        if force {
            b"refresh --force"
        } else {
            b"refresh"
        },
    );
    REFRESHING = true;
    LAST_LOAD = GetTickCount();
}

unsafe fn request_details() {
    DETAIL_READY = false;
    let mut args: Buf<200> = Buf::new();
    args.push(b"details \"")
        .push(SELECTED_KEY.get())
        .push(b"\"");
    let mut raw = [0u8; 200];
    let n = cp1252_to_utf8(args.get(), &mut raw);
    spawn(c"winlux-store", &raw[..n]);
}

/// Les arguments passés au service sont en UTF-8 (chemins accentués).
pub fn cp1252_to_utf8(s: &[u8], out: &mut [u8]) -> usize {
    let mut o = 0;
    for &c in s {
        let cp: u32 = match c {
            0x80 => 0x20AC,
            0x85 => 0x2026,
            0x8C => 0x152,
            0x9C => 0x153,
            0x92 => 0x2019,
            _ => c as u32,
        };
        let mut tmp = [0u8; 4];
        let enc = char::from_u32(cp).unwrap_or('?').encode_utf8(&mut tmp);
        if o + enc.len() >= out.len() {
            break;
        }
        out[o..o + enc.len()].copy_from_slice(enc.as_bytes());
        o += enc.len();
    }
    o
}

unsafe fn operate(verb: &[u8], key: &[u8]) {
    if OP_ACTIVE {
        BUSY_AT = GetTickCount() | 1;
        crate::desktop::toast(t(
            b"Une installation est d\xE9j\xE0 en cours.",
            b"An installation is already running.",
        ));
        return;
    }
    let mut args: Buf<600> = Buf::new();
    args.push(verb);
    if !key.is_empty() {
        args.push(b" \"").push(key).push(b"\"");
    }
    let mut raw = [0u8; 1000];
    let n = cp1252_to_utf8(args.get(), &mut raw);
    DeleteFileA(run_file(b"store-progress.txt").as_ptr().cast());
    if spawn(c"winlux-store", &raw[..n]) {
        RESULT.clear();
        OP_ACTIVE = true;
        OP_STARTED = GetTickCount();
        OP_PERCENT = -1;
        OP_TARGET.clear();
        OP_TARGET.push(key);
        OP_MESSAGE.clear();
        OP_MESSAGE.push(t(b"Pr\xE9paration...", b"Preparing..."));
    } else {
        crate::desktop::toast(t(
            b"Le service du Store n'est pas disponible.",
            b"The Store service is not available.",
        ));
    }
}

unsafe fn open_detail(i: usize) {
    SELECTED = Some(i);
    SELECTED_KEY.clear();
    SELECTED_KEY.push(app_field(i, F_KEY));
    if VIEW != View::Detail {
        PREV_VIEW = VIEW;
    }
    VIEW = View::Detail;
    SCROLL = 0;
    SHOT = 0;
    SHOT_COUNT = 0;
    CONFIRM_REMOVE = false;
    request_details();
}

unsafe fn find_key(key: &[u8]) -> Option<usize> {
    (0..APP_COUNT).find(|&i| app_field(i, F_KEY) == key)
}

/// Installateurs trouvés dans Téléchargements (.exe, .msi, .deb, .AppImage).
unsafe fn scan_downloads() {
    FILE_COUNT = 0;
    for dir in [
        &b"C:\\T\xE9l\xE9chargements\\"[..],
        b"C:\\Downloads\\",
        b"C:\\Descargas\\",
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
            let name = core::slice::from_raw_parts(data.file_name.as_ptr().cast::<u8>(), 260);
            let len = name.iter().position(|&c| c == 0).unwrap_or(0);
            let name = &name[..len];
            let ok = [&b".exe"[..], b".msi", b".deb", b".appimage", b".flatpakref"]
                .iter()
                .any(|e| name.len() > e.len() && contains_ci(&name[name.len() - e.len()..], e));
            if ok && data.attributes & 0x10 == 0 && FILE_COUNT < 24 && dir.len() + len < 96 {
                let f = &mut FILES[FILE_COUNT];
                f[..dir.len()].copy_from_slice(dir);
                f[dir.len()..dir.len() + len].copy_from_slice(name);
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

/// Fichier glissé depuis l'Explorateur sur la fenêtre du Store.
pub unsafe fn install_dropped(path: &[u8]) {
    operate(b"install-file", path);
}

pub unsafe fn open() {
    if !LOADED {
        load_index();
    }
    request_refresh(false);
    crate::wm::open(crate::wm::APP_STORE);
}

// ---------- Dessin ----------

/// Largeur de la barre d'icônes de gauche (icône et libellé dessous).
const RAIL_W: i32 = 88;

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    HIT_COUNT = 0;
    let p = pal();
    let c = f.client;
    fill(g.hdc, g.b, c.left, c.top, c.right, c.bottom, p.mica);
    set_glyph_bg(Some(p.mica));
    draw_rail(g, f);
    let area = rect(c.left + RAIL_W, c.top, c.right, c.bottom);
    fill(
        g.hdc,
        g.b,
        area.left,
        area.top,
        area.right,
        area.bottom,
        p.window,
    );
    set_glyph_bg(Some(p.window));
    // Nom de la boutique (ou « Store ») à gauche de la recherche, s'il tient.
    if area.right - area.left > 900 {
        text(
            g.hdc,
            g.f.title,
            p.text,
            area.left + 24,
            area.top + 20,
            store_title(),
        );
    }
    // Barre de recherche.
    let sw = (area.right - area.left - 48).min(480);
    let sx = area.left + (area.right - area.left - sw) / 2;
    let sr = rect(sx, area.top + 12, sx + sw, area.top + 46);
    panel(
        g.hdc,
        g.b,
        sr.left,
        sr.top,
        sr.right,
        sr.bottom,
        6,
        if SEARCH_FOCUS { p.surface } else { p.surface2 },
        if SEARCH_FOCUS { p.accent } else { p.border },
    );
    set_glyph_bg(Some(if SEARCH_FOCUS { p.surface } else { p.surface2 }));
    // Loupe aux couleurs du thème (Glyph::Search est clair sur sombre et
    // disparaissait en thème clair).
    {
        let bg = if SEARCH_FOCUS { p.surface } else { p.surface2 };
        let (x, y) = (sr.left + 11, sr.top + 9);
        ellipse(g.hdc, g.b, x, y, x + 12, y + 12, p.text2);
        ellipse(g.hdc, g.b, x + 2, y + 2, x + 10, y + 10, bg);
        for k in 0..5 {
            fill(g.hdc, g.b, x + 9 + k, y + 9 + k, x + 12 + k, y + 12 + k, p.text2);
        }
    }
    if SEARCH.n == 0 {
        text(
            g.hdc,
            g.f.body,
            p.text3,
            sr.left + 36,
            sr.top + 9,
            t(
                b"Rechercher des applications et des jeux",
                b"Search apps and games",
            ),
        );
    } else {
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            sr.left + 36,
            sr.top + 9,
            SEARCH.get(),
            sw - 70,
        );
    }
    if SEARCH_FOCUS && (GetTickCount() / 500) % 2 == 0 {
        let cx = sr.left + 36 + text_width(SEARCH.get(), 13).min(sw - 70);
        fill(g.hdc, g.b, cx, sr.top + 8, cx + 1, sr.bottom - 8, p.text);
    }
    hit(sr, Action::Search);
    if SEARCH.n > 0 {
        // Après le champ : la dernière zone enregistrée l'emporte au clic, la
        // croix captait sinon « Search » et n'effaçait rien (#1447).
        let cr = rect(sr.right - 30, sr.top + 6, sr.right - 6, sr.bottom - 6);
        glyph(g.hdc, g.b, g.f, Glyph::Close, cr.left + 4, cr.top + 4, 14);
        hit(cr, Action::ClearSearch);
    }
    set_glyph_bg(Some(p.window));

    let mut body = rect(area.left + 28, area.top + 64, area.right - 28, area.bottom);
    if OP_ACTIVE {
        body.bottom -= 52;
    }
    if !LOADED {
        text_centered(
            g.hdc,
            g.f.title,
            15,
            p.text2,
            (body.left + body.right) / 2,
            body.top + 80,
            t(
                b"Chargement de la biblioth\xE8que...",
                b"Loading the library...",
            ),
        );
    } else if SEARCH.n > 0 && VIEW != View::Detail {
        draw_search(g, &body);
    } else {
        match VIEW {
            View::Home => draw_home(g, &body),
            View::Category(k) => draw_category(g, &body, k),
            View::Windows => draw_windows(g, &body),
            View::Library => draw_library(g, &body),
            View::Detail => draw_detail(g, &body),
        }
    }
    if OP_ACTIVE {
        let b = rect(area.left, area.bottom - 52, area.right, area.bottom);
        fill(g.hdc, g.b, b.left, b.top, b.right, b.bottom, p.surface);
        fill(g.hdc, g.b, b.left, b.top, b.right, b.top + 1, p.border);
        let mut line: Buf<220> = Buf::new();
        if BUSY_AT != 0 && GetTickCount().wrapping_sub(BUSY_AT) < BUSY_MS {
            line.push(t(
                b"Une installation est d\xE9j\xE0 en cours : attendez qu'elle se termine.",
                b"An installation is already running: wait for it to finish.",
            ));
        } else {
            line.push(OP_MESSAGE.get());
        }
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            b.left + 24,
            b.top + 10,
            line.get(),
            b.right - b.left - 120,
        );
        let mut pct: Buf<8> = Buf::new();
        if OP_PERCENT >= 0 {
            pct.num(OP_PERCENT as u64).push(b" %");
        }
        text(
            g.hdc,
            g.f.semibold,
            p.text2,
            b.right - 70,
            b.top + 10,
            pct.get(),
        );
        progress_bar(
            g,
            b.left + 24,
            b.top + 34,
            b.right - b.left - 48,
            OP_PERCENT,
        );
    } else if result_shown() {
        let b = rect(area.left, area.bottom - 44, area.right, area.bottom);
        fill(g.hdc, g.b, b.left, b.top, b.right, b.bottom, p.surface);
        fill(g.hdc, g.b, b.left, b.top, b.right, b.top + 1, p.border);
        if RESULT_ERROR {
            fill(g.hdc, g.b, b.left, b.top + 1, b.left + 4, b.bottom, p.accent);
        }
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            b.left + 24,
            b.top + 13,
            RESULT.get(),
            b.right - b.left - 84,
        );
        let cr = rect(b.right - 40, b.top + 10, b.right - 16, b.top + 34);
        glyph(g.hdc, g.b, g.f, Glyph::Close, cr.left + 5, cr.top + 5, 14);
        hit(cr, Action::DismissResult);
    }
    set_glyph_bg(None);
}

/// Bandeau de résultat visible (#1449).
pub unsafe fn result_shown() -> bool {
    RESULT.n > 0 && (RESULT_ERROR || GetTickCount().wrapping_sub(RESULT_AT) < RESULT_MS)
}

/// Barre d'icônes de gauche : Accueil, Applications, Jeux, Programmes
/// Windows ; Bibliothèque et Actualiser en bas.
unsafe fn draw_rail(g: &mut Gfx, f: &Frame) {
    let p = pal();
    let c = f.client;
    let item = |g: &mut Gfx, k: u8, y: i32, gl: Glyph, label: &[u8], active: bool, badge: bool, a: Action| {
        let r = rect(c.left + 6, y, c.left + RAIL_W - 6, y + 58);
        let h = hot(&r);
        let bg = if active {
            p.selected
        } else if h {
            p.hover
        } else {
            p.mica
        };
        if active || h {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 6, bg);
        }
        if active {
            round_rect(
                g.hdc,
                g.b,
                r.left,
                r.top + 18,
                r.left + 3,
                r.bottom - 18,
                2,
                p.accent,
            );
        }
        set_glyph_bg(Some(bg));
        if active {
            set_glyph_fg(Some(p.accent));
        }
        glyph(g.hdc, g.b, g.f, gl, (r.left + r.right) / 2 - 10, r.top + 9, 20);
        set_glyph_fg(None);
        if badge {
            ellipse(
                g.hdc,
                g.b,
                r.right - 22,
                r.top + 8,
                r.right - 13,
                r.top + 17,
                p.accent,
            );
        }
        let w = text_width(label, 11).min(RAIL_W - 16);
        text_fit(
            g.hdc,
            g.f.small,
            11,
            if active { p.text } else { p.text2 },
            (r.left + r.right) / 2 - w / 2,
            r.top + 36,
            label,
            RAIL_W - 16,
        );
        hit(r, a);
        let _ = k;
    };
    let searching = SEARCH.n > 0;
    let view = if VIEW == View::Detail { PREV_VIEW } else { VIEW };
    let mut y = c.top + 10;
    let entries: [(Glyph, &[u8], bool); 4] = [
        (Glyph::Home, t(b"Accueil", b"Home"), view == View::Home),
        (
            Glyph::Grid,
            t(b"Applis", b"Apps"),
            matches!(view, View::Category(k) if k as usize != JEUX),
        ),
        (
            Glyph::Star,
            t(b"Jeux", b"Games"),
            view == View::Category(JEUX as u8),
        ),
        (Glyph::Exe, b"Windows", view == View::Windows),
    ];
    for (k, (gl, label, active)) in entries.into_iter().enumerate() {
        item(g, k as u8, y, gl, label, active && !searching, false, Action::Nav(k as u8));
        y += 62;
    }
    let lib = view == View::Library && !searching;
    item(
        g,
        4,
        c.bottom - 132,
        Glyph::Download,
        t(b"Biblioth\xE8que", b"Library"),
        lib,
        UPDATES > 0,
        Action::Nav(4),
    );
    item(
        g,
        5,
        c.bottom - 68,
        Glyph::Refresh,
        rail_refresh_label(),
        false,
        false,
        Action::Refresh,
    );
    set_glyph_bg(Some(p.mica));
}

/// Libellé du bouton Actualiser de la barre latérale : au plus 11 caractères,
/// la barre fait 88 px (« Aktualisieren » et « Actualisation... » étaient
/// coupés, #1450).
pub fn rail_refresh_label() -> &'static [u8] {
    let busy = unsafe { REFRESHING };
    match (lang(), busy) {
        (0, false) => b"Actualiser",
        (0, true) => b"En cours...",
        (2, false) => b"Actualizar",
        (2, true) => b"Cargando...",
        (3, false) => b"Neu laden",
        (3, true) => b"L\xE4dt...",
        (_, false) => b"Refresh",
        (_, true) => b"Loading...",
    }
}

/// Libellé du bouton d'une carte selon l'état de l'application.
unsafe fn state_action(i: usize) -> (&'static [u8], bool, Action) {
    match app_field(i, F_STATE) {
        b"i" => (t(b"Voir", b"View"), false, Action::Open(i as u16)),
        b"u" => (
            t(b"Mettre \xE0 jour", b"Update"),
            true,
            Action::Update(i as u16),
        ),
        _ => (t(b"Installer", b"Install"), true, Action::Install(i as u16)),
    }
}

/// Carte carrée (icône, nom, éditeur, bouton) ; renvoie vrai si dessinée.
unsafe fn card(g: &mut Gfx, i: usize, r: &Rect, body: &Rect) -> bool {
    // Coupée en bas (la fenêtre la rogne), jamais sous la barre de recherche.
    if r.top < body.top || r.top >= body.bottom {
        return false;
    }
    let p = pal();
    let h = hot(r);
    let bg = if h { p.surface2 } else { p.surface };
    panel(
        g.hdc, g.b, r.left, r.top, r.right, r.bottom, 8, bg, p.border,
    );
    set_glyph_bg(Some(bg));
    hit(*r, Action::Open(i as u16));
    app_icon(
        g,
        app_field(i, F_ICON),
        app_field(i, F_NAME),
        r.left + 16,
        r.top + 16,
        64,
        bg,
    );
    let tw = r.right - r.left - 32;
    text_fit(
        g.hdc,
        g.f.semibold,
        13,
        p.text,
        r.left + 16,
        r.top + 92,
        app_field(i, F_NAME),
        tw,
    );
    let publisher = app_field(i, F_PUB);
    let mut sub: Buf<120> = Buf::new();
    sub.push(if publisher.is_empty() {
        cat_label(cat_of(i))
    } else {
        publisher
    });
    if !app_field(i, F_BADGE).is_empty() {
        sub.push(b" \x95 ").push(app_field(i, F_BADGE));
    }
    text_fit(
        g.hdc,
        g.f.caption,
        12,
        p.text2,
        r.left + 16,
        r.top + 112,
        sub.get(),
        tw,
    );
    let by = r.bottom - 46;
    if OP_ACTIVE && OP_TARGET.get() == app_field(i, F_KEY) {
        progress_bar(g, r.left + 16, by + 14, tw, OP_PERCENT);
    } else {
        let (label, primary, action) = state_action(i);
        let bw = text_width(label, 13) * 108 / 100 + 28;
        let bx = r.right - 14 - bw;
        if bx > r.left + 16 + text_width(t(b"Gratuit", b"Free"), 12) + 8 {
            text(
                g.hdc,
                g.f.caption,
                p.text2,
                r.left + 16,
                by + 8,
                if app_field(i, F_STATE) == b"i" {
                    t(b"Install\xE9", b"Installed")
                } else {
                    t(b"Gratuit", b"Free")
                },
            );
        }
        let probe = rect(bx, by, bx + bw, by + 32);
        let br = button(g, bx.max(r.left + 14), by, label, primary, hot(&probe));
        hit(br, action);
    }
    true
}

const CARD_W: i32 = 184;
const CARD_H: i32 = 196;
const GAP: i32 = 12;

fn columns(body: &Rect) -> (i32, i32) {
    let cols = ((body.right - body.left + GAP) / (CARD_W + GAP)).max(1);
    let w = (body.right - body.left - (cols - 1) * GAP) / cols;
    (cols, w)
}

unsafe fn grid(g: &mut Gfx, body: &Rect, y0: i32, mut pick: impl FnMut(usize) -> bool) -> i32 {
    let (cols, w) = columns(body);
    let mut n = 0;
    for i in 0..APP_COUNT {
        if !pick(i) {
            continue;
        }
        let x = body.left + (n % cols) * (w + GAP);
        let y = y0 + (n / cols) * (CARD_H + GAP);
        card(g, i, &rect(x, y, x + w, y + CARD_H), body);
        n += 1;
    }
    y0 + (n + cols - 1) / cols * (CARD_H + GAP)
}

unsafe fn heading(g: &mut Gfx, x: i32, y: i32, s: &[u8], body: &Rect) {
    if y >= body.top && y + 24 <= body.bottom {
        text(g.hdc, g.f.large, pal().text, x, y, s);
    }
}

/// Titre de rangée avec la flèche « Tout afficher » à droite.
unsafe fn row_heading(g: &mut Gfx, body: &Rect, y: i32, s: &[u8], a: Action) {
    heading(g, body.left, y, s, body);
    let ar = rect(body.right - 32, y - 2, body.right, y + 30);
    if ar.top >= body.top && ar.bottom <= body.bottom {
        let p = pal();
        if hot(&ar) {
            round_rect(g.hdc, g.b, ar.left, ar.top, ar.right, ar.bottom, 5, p.hover);
        }
        set_glyph_bg(Some(if hot(&ar) { p.hover } else { p.window }));
        glyph(g.hdc, g.b, g.f, Glyph::Forward, ar.left + 8, ar.top + 8, 16);
        set_glyph_bg(Some(p.window));
        hit(ar, a);
        // Le titre lui-même mène aussi à la liste complète.
        let tw = text_width(s, 22) + 8;
        hit(rect(body.left, y, body.left + tw, y + 28), a);
    }
}

/// Une rangée de cartes : les premières applications retenues par `pick`
/// dans l'ordre de l'index (sélection Winlux d'abord). Renvoie le nombre
/// de cartes dessinées.
unsafe fn card_row(g: &mut Gfx, body: &Rect, y: i32, mut pick: impl FnMut(usize) -> bool) -> i32 {
    let (cols, w) = columns(body);
    let mut n = 0;
    for i in 0..APP_COUNT {
        if n >= cols {
            break;
        }
        if pick(i) {
            let x = body.left + n * (w + GAP);
            card(g, i, &rect(x, y, x + w, y + CARD_H), body);
            n += 1;
        }
    }
    n
}

/// Grand bandeau : application mise en avant à gauche, grande icône et
/// vignettes des autres sélections à droite (dégradé vers l'accent).
unsafe fn draw_hero(g: &mut Gfx, body: &Rect, y: i32, featured: &[usize]) -> i32 {
    let p = pal();
    let nf = featured.len();
    let h = ((body.bottom - body.top) * 45 / 100).clamp(230, 320);
    let hr = rect(body.left, y, body.right, y + h);
    if hr.top < body.top || nf == 0 {
        return h;
    }
    let i = featured[HERO % nf];
    let w = hr.right - hr.left;
    let shade = |x: i32| -> u32 {
        // Neutre sur les 40 % de gauche (texte), puis montée vers l'accent.
        let k = ((x - hr.left) * 1000 / w.max(1) - 400).max(0) * 340 / 600;
        mix(p.surface, p.accent, k + 30, 1000)
    };
    round_rect(g.hdc, g.b, hr.left, hr.top, hr.right, hr.bottom, 12, shade(hr.left));
    round_rect(
        g.hdc,
        g.b,
        hr.right - 28,
        hr.top,
        hr.right,
        hr.bottom,
        12,
        shade(hr.right),
    );
    let mut x = hr.left + 12;
    while x < hr.right - 12 {
        let x2 = (x + 8).min(hr.right - 12);
        fill(g.hdc, g.b, x, hr.top, x2, hr.bottom, shade(x));
        x = x2;
    }
    let left_bg = shade(hr.left);
    set_glyph_bg(Some(left_bg));
    let tx = hr.left + 40;
    let text_w = (w * 45 / 100).max(220);
    text(
        g.hdc,
        g.f.caption,
        p.text2,
        tx,
        hr.bottom - 176,
        t(b"S\xC9LECTION WINLUX", b"WINLUX PICKS"),
    );
    text_fit(
        g.hdc,
        g.f.display,
        28,
        p.text,
        tx,
        hr.bottom - 152,
        app_field(i, F_NAME),
        text_w,
    );
    wrap(
        g,
        g.f.body,
        13,
        p.text2,
        tx,
        hr.bottom - 110,
        text_w,
        app_field(i, F_SUMMARY),
        2,
        hr.bottom,
    );
    let (label, _, action) = state_action(i);
    let probe = rect(tx, hr.bottom - 62, tx + 160, hr.bottom - 30);
    let br = button(g, tx, hr.bottom - 62, label, true, hot(&probe));
    let mut info: Buf<80> = Buf::new();
    info.push(t(b"Gratuit", b"Free"))
        .push(b" \x95 ")
        .push(cat_label(cat_of(i)));
    text(
        g.hdc,
        g.f.caption,
        p.text2,
        br.right + 14,
        br.top + 9,
        info.get(),
    );
    hit(hr, Action::Open(i as u16));
    hit(br, action);
    // Côté droit : grande icône, puis vignettes des autres sélections.
    if w > 640 {
        let big = 112;
        let ix = hr.right - 80 - big - (w - 640).min(260) / 2;
        // Aplat uni sous l'icône : le dégradé ne se voit pas à travers.
        let tile = shade(ix + big / 2);
        round_rect(
            g.hdc,
            g.b,
            ix - 14,
            hr.top + 20,
            ix + big + 14,
            hr.top + 48 + big,
            18,
            tile,
        );
        app_icon(
            g,
            app_field(i, F_ICON),
            app_field(i, F_NAME),
            ix,
            hr.top + 34,
            big,
            tile,
        );
        let shown = nf.min(((w - 560) / 72).clamp(1, 6) as usize);
        let first = (HERO % nf) / shown * shown;
        let mut vx = hr.right - 28 - shown as i32 * 72;
        for k in 0..shown {
            let idx = (first + k) % nf;
            let vr = rect(vx, hr.bottom - 84, vx + 60, hr.bottom - 24);
            let bg = shade(vx + 30);
            round_rect(g.hdc, g.b, vr.left, vr.top, vr.right, vr.bottom, 10, bg);
            if idx == HERO % nf {
                round_rect(g.hdc, g.b, vr.left - 3, vr.top - 3, vr.right + 3, vr.bottom + 3, 12, p.text);
                round_rect(g.hdc, g.b, vr.left - 1, vr.top - 1, vr.right + 1, vr.bottom + 1, 11, bg);
            } else if hot(&vr) {
                round_rect(g.hdc, g.b, vr.left - 2, vr.top - 2, vr.right + 2, vr.bottom + 2, 11, p.hover);
            }
            app_icon(
                g,
                app_field(featured[idx], F_ICON),
                app_field(featured[idx], F_NAME),
                vr.left + 6,
                vr.top + 6,
                48,
                bg,
            );
            hit(vr, Action::HeroPick(idx as u8));
            vx += 72;
        }
    }
    set_glyph_bg(Some(p.window));
    h
}

unsafe fn draw_home(g: &mut Gfx, body: &Rect) {
    let p = pal();
    let mut y = body.top - SCROLL;
    let mut featured = [0usize; 12];
    let mut nf = 0;
    for i in 0..APP_COUNT {
        if app_field(i, F_FEATURED) == b"1" && nf < 12 {
            featured[nf] = i;
            nf += 1;
        }
    }
    if nf > 0 {
        y += draw_hero(g, body, y, &featured[..nf]) + 28;
    }
    // Essentiels (hors jeux), jeux, programmes Windows, puis une rangée par
    // catégorie qui a des logiciels.
    row_heading(
        g,
        body,
        y,
        t(b"Applications essentielles", b"Essential apps"),
        Action::Nav(1),
    );
    let n = card_row(g, body, y + 40, |i| cat_of(i) != JEUX && !is_windows(i));
    if n > 0 {
        y += 40 + CARD_H + 32;
    } else {
        y += 40;
        if y >= body.top && y + 20 <= body.bottom {
            text(
                g.hdc,
                g.f.body,
                p.text3,
                body.left,
                y,
                t(
                    b"Aucun logiciel pour l'instant : actualisez la biblioth\xE8que.",
                    b"No software yet: refresh the library.",
                ),
            );
        }
        y += 40;
    }
    let mut sections = [(0usize, false); CATS.len() + 1];
    sections[0] = (JEUX, false);
    sections[1] = (0, true);
    let mut k = 2;
    for c in 0..CATS.len() {
        if c != JEUX {
            sections[k] = (c, false);
            k += 1;
        }
    }
    for (c, windows) in sections {
        // Section « Testés sur Winlux » juste avant les autres programmes Windows.
        if windows && (0..APP_COUNT).any(|i| is_tested(i)) {
            row_heading(g, body, y, t(b"Test\xE9s sur Winlux", b"Tested on Winlux"), Action::Nav(3));
            card_row(g, body, y + 40, |i| is_tested(i));
            y += 40 + CARD_H + 32;
        }
        let (label, action): (&[u8], Action) = if windows {
            (
                t(b"Programmes Windows", b"Windows programs"),
                Action::Nav(3),
            )
        } else {
            (cat_label(c), Action::SeeAll(c as u8))
        };
        // Une rangée vide n'est pas affichée (ni son titre).
        let any = (0..APP_COUNT).any(|i| {
            if windows {
                is_windows(i) && !is_tested(i)
            } else {
                cat_of(i) == c && (c == JEUX || !is_windows(i))
            }
        });
        if !any {
            continue;
        }
        row_heading(g, body, y, label, action);
        card_row(g, body, y + 40, |i| {
            if windows {
                is_windows(i) && !is_tested(i)
            } else {
                cat_of(i) == c && (c == JEUX || !is_windows(i))
            }
        });
        y += 40 + CARD_H + 32;
    }
    CONTENT_H = y + SCROLL - body.top;
}

/// Puce de filtre ; renvoie sa largeur.
unsafe fn chip(g: &mut Gfx, x: i32, y: i32, label: &[u8], count: usize, on: bool, a: Action, body: &Rect) -> i32 {
    let p = pal();
    let mut s: Buf<64> = Buf::new();
    s.push(label).push(b"  ").num(count as u64);
    let w = text_width(s.get(), 13) + 28;
    let r = rect(x, y, x + w, y + 32);
    if r.top >= body.top && r.bottom <= body.bottom {
        let bg = if on {
            p.selected
        } else if hot(&r) {
            p.hover
        } else {
            p.surface
        };
        panel(
            g.hdc,
            g.b,
            r.left,
            r.top,
            r.right,
            r.bottom,
            16,
            bg,
            if on { p.accent } else { p.border },
        );
        text(
            g.hdc,
            if on { g.f.semibold } else { g.f.body },
            p.text,
            x + 14,
            y + 8,
            label,
        );
        let lw = text_width(label, 13) + if on { 2 } else { 0 };
        let mut n: Buf<12> = Buf::new();
        n.num(count as u64);
        text(g.hdc, g.f.caption, p.text2, x + 14 + lw + 8, y + 9, n.get());
        hit(r, a);
    }
    w
}

unsafe fn draw_category(g: &mut Gfx, body: &Rect, k: u8) {
    let mut y = body.top - SCROLL;
    let games = k as usize == JEUX;
    let title: &[u8] = if games {
        cat_label(JEUX)
    } else if k == ALL {
        t(b"Applications", b"Apps")
    } else {
        cat_label(k as usize)
    };
    heading(g, body.left, y, title, body);
    y += 44;
    if !games {
        // Puces : toutes, puis chaque catégorie qui a des logiciels.
        let mut counts = [0usize; CATS.len()];
        let mut all = 0;
        for i in 0..APP_COUNT {
            if !is_windows(i) {
                counts[cat_of(i)] += 1;
            }
        }
        for (c, n) in counts.iter().enumerate() {
            if c != JEUX {
                all += n;
            }
        }
        let mut x = body.left;
        let w = chip(g, x, y, t(b"Toutes", b"All"), all, k == ALL, Action::SeeAll(ALL), body);
        x += w + 8;
        for c in 0..CATS.len() {
            if c == JEUX || counts[c] == 0 {
                continue;
            }
            let mut s: Buf<64> = Buf::new();
            s.push(cat_label(c)).push(b"  ").num(counts[c] as u64);
            let cw = text_width(s.get(), 13) + 28;
            if x + cw > body.right {
                x = body.left;
                y += 40;
            }
            x += chip(g, x, y, cat_label(c), counts[c], k as usize == c, Action::SeeAll(c as u8), body) + 8;
        }
        y += 52;
    }
    let end = grid(g, body, y, |i| {
        let c = cat_of(i);
        if games {
            c == JEUX
        } else if is_windows(i) {
            false
        } else if k == ALL {
            c != JEUX
        } else {
            c == k as usize
        }
    });
    CONTENT_H = end + SCROLL - body.top;
}

/// Programmes Windows des catalogues ajoutés (Scoop…), ou comment en avoir.
unsafe fn draw_windows(g: &mut Gfx, body: &Rect) {
    let p = pal();
    let mut y = body.top - SCROLL;
    heading(
        g,
        body.left,
        y,
        t(b"Programmes Windows", b"Windows programs"),
        body,
    );
    y += 40;
    let count = (0..APP_COUNT).filter(|&i| is_windows(i)).count();
    let note: &[u8] = if count > 0 {
        t(
            b"Programmes .exe ex\xE9cut\xE9s directement par Winlux. Chaque fichier est v\xE9rifi\xE9 par son empreinte avant l'installation.",
            b"Programs (.exe) run directly by Winlux. Every file is checked against its hash before installation.",
        )
    } else {
        t(
            b"Aucun catalogue de programmes Windows n'est ajout\xE9. Vous pouvez en activer un dans Param\xE8tres > Applications > Boutiques de logiciels, ou installer un fichier .exe ou .msi depuis la Biblioth\xE8que.",
            b"No Windows program catalog has been added. You can turn one on in Settings > Apps > Software stores, or install an .exe or .msi file from the Library.",
        )
    };
    if y >= body.top {
        wrap(
            g,
            g.f.body,
            13,
            p.text2,
            body.left,
            y,
            (body.right - body.left).min(760),
            note,
            3,
            body.bottom,
        );
    }
    y += 64;
    let tested = (0..APP_COUNT).filter(|&i| is_tested(i)).count();
    let mut end = y;
    if tested > 0 {
        heading(g, body.left, y, t(b"Test\xE9s sur Winlux", b"Tested on Winlux"), body);
        y += 36;
        if y >= body.top {
            wrap(
                g,
                g.f.body,
                13,
                p.text2,
                body.left,
                y,
                (body.right - body.left).min(760),
                t(
                    b"Install\xE9s et utilis\xE9s sur notre banc de test, depuis le site officiel de l'\xE9diteur.",
                    b"Installed and used on our test bench, from the publisher's official site.",
                ),
                2,
                body.bottom,
            );
        }
        y += 40;
        end = grid(g, body, y, |i| is_tested(i));
        y = end + 24;
        if count > tested {
            heading(g, body.left, y, t(b"Autres programmes Windows", b"Other Windows programs"), body);
            y += 44;
        }
    }
    if count > tested {
        end = grid(g, body, y, |i| is_windows(i) && !is_tested(i));
    }
    if count == 0 && y >= body.top && y + 32 <= body.bottom {
        let label = t(b"Ouvrir la Biblioth\xE8que", b"Open the Library");
        let probe = rect(body.left, y, body.left + 200, y + 32);
        let br = button(g, body.left, y, label, true, hot(&probe));
        hit(br, Action::Nav(4));
    }
    CONTENT_H = end + SCROLL - body.top + 40;
}

unsafe fn draw_search(g: &mut Gfx, body: &Rect) {
    let p = pal();
    let y = body.top - SCROLL;
    let mut title: Buf<96> = Buf::new();
    title
        .push(t(b"R\xE9sultats pour \xAB ", b"Results for \""))
        .push(SEARCH.get());
    title.push(t(b" \xBB", b"\""));
    heading(g, body.left, y, title.get(), body);
    let q = SEARCH.get();
    let mut found = false;
    let end = grid(g, body, y + 44, |i| {
        // Le nom du paquet aussi (sans « apt: ») : « mines » trouve le
        // Démineur (gnome-mines).
        let key = app_field(i, F_KEY);
        let reference = key
            .iter()
            .position(|&c| c == b':')
            .map_or(key, |p| &key[p + 1..]);
        let m = contains_ci(app_field(i, F_NAME), q)
            || contains_ci(app_field(i, F_SUMMARY), q)
            || contains_ci(app_field(i, F_PUB), q)
            || contains_ci(reference, q);
        found |= m;
        m
    });
    if !found && y + 60 >= body.top {
        text(
            g.hdc,
            g.f.body,
            p.text2,
            body.left,
            y + 52,
            t(b"Aucun r\xE9sultat.", b"No results."),
        );
    }
    CONTENT_H = end + SCROLL - body.top;
}

unsafe fn row(g: &mut Gfx, body: &Rect, y: i32, i: usize, mode: u8) {
    let p = pal();
    let r = rect(body.left, y, body.right, y + 64);
    if r.top < body.top || r.bottom > body.bottom {
        return;
    }
    let bg = if hot(&r) { p.surface2 } else { p.surface };
    panel(
        g.hdc, g.b, r.left, r.top, r.right, r.bottom, 6, bg, p.border,
    );
    app_icon(
        g,
        app_field(i, F_ICON),
        app_field(i, F_NAME),
        r.left + 14,
        r.top + 12,
        40,
        bg,
    );
    text_fit(
        g.hdc,
        g.f.semibold,
        13,
        p.text,
        r.left + 68,
        r.top + 12,
        app_field(i, F_NAME),
        r.right - r.left - 360,
    );
    let mut sub: Buf<120> = Buf::new();
    sub.push(app_field(i, F_BADGE)).push(b"  ");
    if mode == 0 {
        sub.push(app_field(i, F_VERSION))
            .push(b" \xBB ")
            .push(app_field(i, F_AVAIL));
    } else {
        sub.push(app_field(i, F_VERSION));
    }
    text_fit(
        g.hdc,
        g.f.caption,
        12,
        p.text2,
        r.left + 68,
        r.top + 34,
        sub.get(),
        r.right - r.left - 360,
    );
    hit(r, Action::Open(i as u16));
    if OP_ACTIVE && OP_TARGET.get() == app_field(i, F_KEY) {
        progress_bar(g, r.right - 240, r.top + 30, 220, OP_PERCENT);
        return;
    }
    let label = if mode == 0 {
        t(b"Mettre \xE0 jour", b"Update")
    } else {
        t(b"D\xE9sinstaller", b"Uninstall")
    };
    let bw = text_width(label, 13) + 28;
    let probe = rect(r.right - bw - 16, r.top + 16, r.right - 16, r.top + 48);
    let br = button(
        g,
        r.right - bw - 16,
        r.top + 16,
        label,
        mode == 0,
        hot(&probe),
    );
    hit(
        br,
        if mode == 0 {
            Action::Update(i as u16)
        } else {
            Action::Remove(i as u16)
        },
    );
}

unsafe fn draw_library(g: &mut Gfx, body: &Rect) {
    let p = pal();
    let mut y = body.top - SCROLL;
    let mut title: Buf<64> = Buf::new();
    title.push(t(b"Mises \xE0 jour", b"Updates"));
    title.push(b" (").num(UPDATES).push(b")");
    heading(g, body.left, y, title.get(), body);
    if y >= body.top {
        let label = t(b"Tout mettre \xE0 jour", b"Update all");
        let bw = text_width(label, 13) + 28;
        let probe = rect(body.right - bw, y, body.right, y + 32);
        let br = button(g, body.right - bw, y, label, true, hot(&probe));
        hit(br, Action::UpdateAll);
    }
    y += 40;
    if SYSTEM_UPDATES > 0 && y >= body.top && y + 20 <= body.bottom {
        let mut s: Buf<120> = Buf::new();
        s.num(SYSTEM_UPDATES).push(t(
            b" mise(s) \xE0 jour du syst\xE8me Linux (apt) incluse(s) dans \xAB Tout mettre \xE0 jour \xBB.",
            b" Linux system update(s) (apt) included in \"Update all\".",
        ));
        text(g.hdc, g.f.caption, p.text2, body.left, y, s.get());
    }
    y += 28;
    let mut any = false;
    for i in 0..APP_COUNT {
        if app_field(i, F_STATE) == b"u" {
            row(g, body, y, i, 0);
            y += 72;
            any = true;
        }
    }
    if !any {
        if y >= body.top && y + 20 <= body.bottom {
            text(
                g.hdc,
                g.f.body,
                p.text2,
                body.left,
                y,
                t(b"Tout est \xE0 jour.", b"Everything is up to date."),
            );
        }
        y += 32;
    }
    y += 16;
    heading(g, body.left, y, t(b"Install\xE9s", b"Installed"), body);
    y += 40;
    for i in 0..APP_COUNT {
        if app_field(i, F_STATE) == b"i" {
            row(g, body, y, i, 1);
            y += 72;
        }
    }
    y += 16;
    heading(
        g,
        body.left,
        y,
        t(b"Programmes Windows", b"Windows programs"),
        body,
    );
    y += 40;
    if WIN_COUNT == 0 && y >= body.top && y + 20 <= body.bottom {
        text(
            g.hdc,
            g.f.body,
            p.text2,
            body.left,
            y,
            t(
                b"Aucun programme Windows install\xE9 pour l'instant.",
                b"No Windows program installed yet.",
            ),
        );
        y += 32;
    }
    for k in 0..WIN_COUNT {
        let r = rect(body.left, y, body.right, y + 44);
        if r.top >= body.top && r.bottom <= body.bottom {
            panel(
                g.hdc, g.b, r.left, r.top, r.right, r.bottom, 6, p.surface, p.border,
            );
            set_glyph_bg(Some(p.surface));
            glyph(g.hdc, g.b, g.f, Glyph::Exe, r.left + 14, r.top + 12, 20);
            text_fit(
                g.hdc,
                g.f.semibold,
                13,
                p.text,
                r.left + 48,
                r.top + 6,
                slice(WIN_APPS[k][0]),
                r.right - r.left - 64,
            );
            text_fit(
                g.hdc,
                g.f.small,
                11,
                p.text3,
                r.left + 48,
                r.top + 25,
                slice(WIN_APPS[k][1]),
                r.right - r.left - 64,
            );
            set_glyph_bg(Some(p.window));
        }
        y += 52;
    }
    y += 16;
    heading(
        g,
        body.left,
        y,
        t(b"Installer depuis un fichier", b"Install from a file"),
        body,
    );
    y += 36;
    if y >= body.top && y + 20 <= body.bottom {
        text(
            g.hdc,
            g.f.caption,
            p.text2,
            body.left,
            y,
            t(
                b"Installateurs .exe/.msi (runtime Winlux), .deb, .AppImage du dossier T\xE9l\xE9chargements, ou glissez un fichier ici depuis l'Explorateur.",
                b"Installers (.exe/.msi via the Winlux runtime, .deb, .AppImage) from Downloads, or drop a file here from Explorer.",
            ),
        );
    }
    y += 28;
    for k in 0..FILE_COUNT {
        let r = rect(body.left, y, body.right, y + 44);
        if r.top >= body.top && r.bottom <= body.bottom {
            panel(
                g.hdc, g.b, r.left, r.top, r.right, r.bottom, 6, p.surface, p.border,
            );
            set_glyph_bg(Some(p.surface));
            glyph(
                g.hdc,
                g.b,
                g.f,
                Glyph::Download,
                r.left + 14,
                r.top + 12,
                20,
            );
            let path = &FILES[k][..FILE_LENS[k]];
            let name = path.rsplit(|&c| c == b'\\').next().unwrap_or(path);
            text_fit(
                g.hdc,
                g.f.body,
                13,
                p.text,
                r.left + 48,
                r.top + 13,
                name,
                r.right - r.left - 200,
            );
            let label = t(b"Installer", b"Install");
            let bw = text_width(label, 13) + 28;
            let probe = rect(r.right - bw - 8, r.top + 6, r.right - 8, r.top + 38);
            let br = button(g, r.right - bw - 8, r.top + 6, label, false, hot(&probe));
            hit(br, Action::InstallFile(k as u8));
            set_glyph_bg(Some(p.window));
        }
        y += 52;
    }
    // Sources actives (autrefois au bas du volet de navigation).
    y += 12;
    if y >= body.top && y + 16 <= body.bottom {
        let mut s: Buf<120> = Buf::new();
        s.push(t(b"Sources : ", b"Sources: "));
        s.push(if SOURCES.n > 0 { SOURCES.get() } else { b"-" });
        text_fit(
            g.hdc,
            g.f.caption,
            12,
            p.text3,
            body.left,
            y,
            s.get(),
            body.right - body.left,
        );
    }
    y += 24;
    CONTENT_H = y + SCROLL - body.top;
}

unsafe fn draw_detail(g: &mut Gfx, body: &Rect) {
    let p = pal();
    let Some(i) = SELECTED.filter(|&i| i < APP_COUNT && app_field(i, F_KEY) == SELECTED_KEY.get())
    else {
        VIEW = PREV_VIEW;
        return;
    };
    let mut y = body.top - SCROLL;
    let br = rect(body.left, y, body.left + 32, y + 32);
    if br.top >= body.top {
        if hot(&br) {
            round_rect(g.hdc, g.b, br.left, br.top, br.right, br.bottom, 5, p.hover);
            set_glyph_bg(Some(p.hover));
        }
        glyph(g.hdc, g.b, g.f, Glyph::Back, br.left + 8, br.top + 8, 16);
        set_glyph_bg(Some(p.window));
        hit(br, Action::Back);
    }
    y += 44;
    // En-tête.
    let icon = if DETAIL_READY && !detail_value(b"icon=").is_empty() {
        detail_value(b"icon=")
    } else {
        app_field(i, F_ICON)
    };
    if y >= body.top {
        app_icon(g, icon, app_field(i, F_NAME), body.left, y, 96, p.window);
        text_fit(
            g.hdc,
            g.f.display,
            28,
            p.text,
            body.left + 120,
            y,
            app_field(i, F_NAME),
            body.right - body.left - 140,
        );
        let mut sub: Buf<120> = Buf::new();
        sub.push(app_field(i, F_PUB));
        if !app_field(i, F_PUB).is_empty() {
            sub.push(b" \x95 ");
        }
        sub.push(cat_label(cat_of(i)));
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text2,
            body.left + 120,
            y + 40,
            sub.get(),
            body.right - body.left - 140,
        );
        // Source et sources alternatives.
        let mut sx = body.left + 120;
        let mut chip: Buf<64> = Buf::new();
        chip.push(t(b"Source : ", b"Source: "))
            .push(app_field(i, F_BADGE));
        let cw = text_width(chip.get(), 11) + 16;
        round_rect(g.hdc, g.b, sx, y + 64, sx + cw, y + 84, 10, p.surface2);
        text(g.hdc, g.f.small, p.text2, sx + 8, y + 67, chip.get());
        sx += cw + 8;
        if DETAIL_READY {
            let mut k = 0u8;
            detail_lines(b"alt=", |v| {
                let label = v.rsplit(|&c| c == b'|').next().unwrap_or(v);
                let w = text_width(label, 11) + 16;
                let r = rect(sx, y + 64, sx + w, y + 84);
                round_rect(
                    g.hdc,
                    g.b,
                    r.left,
                    r.top,
                    r.right,
                    r.bottom,
                    10,
                    if hot(&r) { p.hover } else { p.window },
                );
                text(g.hdc, g.f.small, p.accent, sx + 8, y + 67, label);
                hit(r, Action::Alt(k));
                sx += w + 6;
                k += 1;
            });
        }
    }
    y += 112;
    // Actions.
    if y >= body.top {
        let key = app_field(i, F_KEY);
        if OP_ACTIVE && OP_TARGET.get() == key {
            text_fit(
                g.hdc,
                g.f.body,
                13,
                p.text,
                body.left,
                y,
                OP_MESSAGE.get(),
                420,
            );
            progress_bar(g, body.left, y + 24, 320, OP_PERCENT);
        } else {
            let state = app_field(i, F_STATE);
            let mut x = body.left;
            let mut act = |g: &mut Gfx, label: &[u8], primary: bool, a: Action| {
                let w = text_width(label, 13) + 28;
                let probe = rect(x, y, x + w, y + 32);
                let r = button(g, x, y, label, primary, hot(&probe));
                hit(r, a);
                x = r.right + 10;
            };
            if state == b"a" {
                act(g, t(b"Installer", b"Install"), true, Action::DetailInstall);
            }
            if state == b"u" {
                act(
                    g,
                    t(b"Mettre \xE0 jour", b"Update"),
                    true,
                    Action::DetailUpdate,
                );
            }
            if state != b"a" {
                act(
                    g,
                    if CONFIRM_REMOVE {
                        t(b"Confirmer la d\xE9sinstallation", b"Confirm uninstall")
                    } else {
                        t(b"D\xE9sinstaller", b"Uninstall")
                    },
                    false,
                    Action::DetailRemove,
                );
            }
            if !detail_value(b"homepage=").is_empty() {
                act(g, t(b"Site web", b"Website"), false, Action::Homepage);
            }
        }
    }
    y += 56;
    if !DETAIL_READY {
        if y >= body.top {
            text(
                g.hdc,
                g.f.body,
                p.text2,
                body.left,
                y,
                t(b"Chargement de la fiche...", b"Loading details..."),
            );
        }
        CONTENT_H = y + 40 + SCROLL - body.top;
        return;
    }
    // Carrousel de captures.
    if SHOT_COUNT > 0 {
        heading(
            g,
            body.left,
            y,
            t(b"Captures d'\xE9cran", b"Screenshots"),
            body,
        );
        y += 36;
        // 16:9, limité pour tenir dans la fenêtre avec l'en-tête.
        let full = ((body.right - body.left).min(760) * 9 / 16).min(360);
        let room = body.bottom - (y + SCROLL) - 24;
        let h = if room >= 180 { full.min(room) } else { full };
        let w = h * 16 / 9;
        let r = rect(body.left, y, body.left + w, y + h);
        if r.top >= body.top && r.bottom <= body.bottom {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 8, p.surface2);
            let mut k = 0;
            let mut path: &[u8] = b"";
            detail_lines(b"shot=", |v| {
                if k == SHOT % SHOT_COUNT {
                    path = v;
                }
                k += 1;
            });
            let mut cpath = [0u8; 300];
            let n = path.len().min(299);
            cpath[..n].copy_from_slice(&path[..n]);
            WinluxDrawImageA(
                g.hdc,
                cpath.as_ptr().cast(),
                r.left + 4,
                r.top + 4,
                w - 8,
                h - 8,
                p.surface2,
            );
            if SHOT_COUNT > 1 {
                for (dir, x) in [(-1i8, r.left + 12), (1, r.right - 44)] {
                    let ar = rect(x, r.top + h / 2 - 16, x + 32, r.top + h / 2 + 16);
                    round_rect(
                        g.hdc,
                        g.b,
                        ar.left,
                        ar.top,
                        ar.right,
                        ar.bottom,
                        16,
                        if hot(&ar) { p.hover } else { p.surface },
                    );
                    set_glyph_bg(Some(if hot(&ar) { p.hover } else { p.surface }));
                    glyph(
                        g.hdc,
                        g.b,
                        g.f,
                        if dir < 0 { Glyph::Back } else { Glyph::Forward },
                        ar.left + 8,
                        ar.top + 8,
                        16,
                    );
                    hit(ar, Action::Shot(dir));
                }
                set_glyph_bg(Some(p.window));
            }
            for k in 0..SHOT_COUNT {
                let dx = r.left + w / 2 - SHOT_COUNT as i32 * 7 + k as i32 * 14;
                let col = if k == SHOT % SHOT_COUNT {
                    p.accent
                } else {
                    p.border
                };
                ellipse(g.hdc, g.b, dx, r.bottom + 8, dx + 7, r.bottom + 15, col);
            }
        }
        y += h + 32;
    }
    // Description.
    heading(g, body.left, y, t(b"Description", b"Description"), body);
    y += 36;
    let tw = (body.right - body.left).min(760);
    let mut paragraphs = 0;
    detail_lines(b"desc=", |v| {
        if y + 20 >= body.top {
            y = wrap(
                g,
                g.f.body,
                13,
                p.text2,
                body.left,
                y,
                tw,
                v,
                12,
                body.bottom,
            );
        } else {
            // Hors de la vue : estimer la hauteur sans dessiner.
            let lines = (text_width(v, 13) / tw.max(1) + 1).min(12);
            y += lines * 19;
        }
        y += 8;
        paragraphs += 1;
    });
    if paragraphs == 0 {
        y = wrap(
            g,
            g.f.body,
            13,
            p.text2,
            body.left,
            y,
            tw,
            app_field(i, F_SUMMARY),
            4,
            body.bottom,
        );
    }
    y += 16;
    // Informations.
    heading(g, body.left, y, t(b"Informations", b"Information"), body);
    y += 36;
    let mut size: Buf<24> = Buf::new();
    let kb = parse_num(detail_value(b"size_kb="));
    if kb >= 1024 {
        size.num(kb / 1024).push(t(b" Mo", b" MB"));
    } else if kb > 0 {
        size.num(kb).push(t(b" Ko", b" KB"));
    }
    let infos: [(&[u8], &[u8]); 7] = [
        (
            t(b"Version install\xE9e", b"Installed version"),
            detail_value(b"version="),
        ),
        (
            t(b"Derni\xE8re version", b"Latest version"),
            detail_value(b"available="),
        ),
        (t(b"Taille", b"Size"), size.get()),
        (
            t(b"\xC9\x64iteur", b"Publisher"),
            detail_value(b"publisher="),
        ),
        (t(b"Licence", b"License"), detail_value(b"license=")),
        (t(b"Source", b"Source"), detail_value(b"source=")),
        (t(b"Site web", b"Website"), detail_value(b"homepage=")),
    ];
    let cols = if body.right - body.left > 640 { 2 } else { 1 };
    let cw = (body.right - body.left).min(760) / cols;
    let mut n = 0;
    for (label, value) in infos {
        if value.is_empty() {
            continue;
        }
        let x = body.left + (n % cols) * cw;
        let yy = y + (n / cols) * 46;
        if yy >= body.top && yy + 40 <= body.bottom {
            text(g.hdc, g.f.caption, p.text3, x, yy, label);
            text_fit(g.hdc, g.f.body, 13, p.text, x, yy + 18, value, cw - 16);
        }
        n += 1;
    }
    y += ((n + cols - 1) / cols) * 46 + 16;
    // Historique des versions.
    let mut releases = 0;
    detail_lines(b"release=", |_| releases += 1);
    if releases > 0 {
        heading(g, body.left, y, t(b"Nouveaut\xE9s", b"What's new"), body);
        y += 36;
        detail_lines(b"release=", |v| {
            let mut parts = v.splitn(3, |&c| c == b'|');
            let ver = parts.next().unwrap_or(b"");
            let date = parts.next().unwrap_or(b"");
            let notes = parts.next().unwrap_or(b"");
            if y >= body.top && y + 20 <= body.bottom {
                let mut head: Buf<80> = Buf::new();
                head.push(ver);
                if !date.is_empty() {
                    head.push(b"  \x95  ").push(date);
                }
                text(g.hdc, g.f.semibold, p.text, body.left, y, head.get());
            }
            y += 22;
            if !notes.is_empty() {
                y = wrap(
                    g,
                    g.f.caption,
                    12,
                    p.text2,
                    body.left,
                    y,
                    tw,
                    notes,
                    3,
                    body.bottom,
                );
            }
            y += 10;
        });
    }
    CONTENT_H = y + SCROLL - body.top + 20;
}

// ---------- Événements ----------

unsafe fn act(a: Action) -> bool {
    match a {
        Action::None => return false,
        Action::Nav(i) => {
            SEARCH.clear();
            SEARCH_FOCUS = false;
            VIEW = match i {
                1 => View::Category(ALL),
                2 => View::Category(JEUX as u8),
                3 => View::Windows,
                4 => {
                    scan_downloads();
                    View::Library
                }
                _ => View::Home,
            };
            SCROLL = 0;
        }
        Action::SeeAll(k) => {
            VIEW = View::Category(k);
            SCROLL = 0;
        }
        Action::Open(i) => {
            SEARCH_FOCUS = false;
            open_detail(i as usize);
        }
        Action::Install(i) => operate(b"install", app_field(i as usize, F_KEY)),
        Action::Remove(i) => operate(b"remove", app_field(i as usize, F_KEY)),
        Action::Update(i) => operate(b"update", app_field(i as usize, F_KEY)),
        Action::UpdateAll => operate(b"update-all", b""),
        Action::DetailInstall => operate(b"install", SELECTED_KEY.get()),
        Action::DetailUpdate => operate(b"update", SELECTED_KEY.get()),
        Action::DetailRemove => {
            if CONFIRM_REMOVE {
                CONFIRM_REMOVE = false;
                operate(b"remove", SELECTED_KEY.get());
            } else {
                CONFIRM_REMOVE = true;
            }
        }
        Action::Homepage => {
            let url = detail_value(b"homepage=");
            let mut c = [0u8; 300];
            let n = url.len().min(299);
            c[..n].copy_from_slice(&url[..n]);
            if url.starts_with(b"http") {
                ShellExecuteA(
                    core::ptr::null_mut(),
                    c"open".as_ptr(),
                    c.as_ptr().cast(),
                    core::ptr::null(),
                    core::ptr::null(),
                    1,
                );
            }
        }
        Action::Back => {
            VIEW = PREV_VIEW;
            SCROLL = 0;
        }
        Action::Shot(d) => {
            if SHOT_COUNT > 0 {
                SHOT = (SHOT + SHOT_COUNT).wrapping_add_signed(d as isize) % SHOT_COUNT;
            }
        }
        Action::Hero(d) => {
            HERO = HERO.wrapping_add_signed(d as isize + 12) % 12;
            HERO_TICK = GetTickCount();
        }
        Action::HeroPick(k) => {
            HERO = k as usize;
            HERO_TICK = GetTickCount();
        }
        Action::Alt(k) => {
            let mut n = 0u8;
            let mut key: &[u8] = b"";
            detail_lines(b"alt=", |v| {
                if n == k {
                    key = v.split(|&c| c == b'|').next().unwrap_or(b"");
                }
                n += 1;
            });
            if let Some(i) = find_key(key) {
                open_detail(i);
            } else if !key.is_empty() {
                // Source fusionnée : la fiche est demandée directement.
                SELECTED_KEY.clear();
                SELECTED_KEY.push(key);
                request_details();
            }
        }
        Action::InstallFile(k) => {
            let k = k as usize;
            if k < FILE_COUNT {
                let mut path: Buf<96> = Buf::new();
                path.push(&FILES[k][..FILE_LENS[k]]);
                operate(b"install-file", path.get());
            }
        }
        Action::Search => SEARCH_FOCUS = true,
        Action::DismissResult => RESULT.clear(),
        Action::ClearSearch => {
            SEARCH.clear();
            SCROLL = 0;
        }
        Action::Refresh => request_refresh(true),
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
            // Les zones dessinées en dernier (boutons) sont prioritaires.
            for k in (0..HIT_COUNT).rev() {
                if inside(&HITS[k].0, x, y) {
                    chosen = HITS[k].1;
                    break;
                }
            }
            if chosen != Action::Search {
                SEARCH_FOCUS = false;
            }
            if chosen != Action::DetailRemove {
                CONFIRM_REMOVE = false;
            }
            act(chosen);
            true
        }
        Mouse::Wheel(delta) => {
            let view_h = f.client.bottom - f.client.top - 64;
            let max = (CONTENT_H - view_h + 40).max(0);
            SCROLL = (SCROLL - delta / 120 * 60).clamp(0, max);
            true
        }
        _ => false,
    }
}

pub unsafe fn key(vk: u32, ch: u16) -> bool {
    if ch != 0 {
        if !SEARCH_FOCUS {
            if ch >= 0x20 {
                SEARCH_FOCUS = true;
            } else {
                return false;
            }
        }
        match ch {
            8 => {
                SEARCH.n = SEARCH.n.saturating_sub(1);
            }
            13 => SEARCH_FOCUS = false,
            27 => {
                SEARCH.clear();
                SEARCH_FOCUS = false;
            }
            c if c >= 0x20 && c < 0x100 => {
                SEARCH.push(&[c as u8]);
            }
            _ => return false,
        }
        SCROLL = 0;
        return true;
    }
    match vk {
        0x1B if VIEW == View::Detail => {
            VIEW = PREV_VIEW;
            SCROLL = 0;
            true
        }
        0x25 if VIEW == View::Detail => act(Action::Shot(-1)),
        0x27 if VIEW == View::Detail => act(Action::Shot(1)),
        0x21 | 0x22 => {
            SCROLL = (SCROLL + if vk == 0x21 { -400 } else { 400 }).clamp(0, CONTENT_H.max(0));
            true
        }
        0x74 => act(Action::Refresh),
        _ => false,
    }
}

pub unsafe fn tick(_f: &Frame) -> bool {
    let now = GetTickCount();
    let mut repaint = SEARCH_FOCUS;
    if !LOADED || (REFRESHING && now.wrapping_sub(LAST_LOAD) > 2000) {
        load_index();
        LAST_LOAD = now;
        repaint = true;
    }
    if OP_ACTIVE {
        poll_progress();
        repaint = true;
    }
    if VIEW == View::Detail && (!DETAIL_READY || DETAIL_PARTIAL) {
        let was = (DETAIL_READY, DETAIL_PARTIAL);
        load_detail();
        repaint |= (DETAIL_READY, DETAIL_PARTIAL) != was;
    }
    if VIEW == View::Home && SEARCH.n == 0 && now.wrapping_sub(HERO_TICK) > 7000 {
        HERO = (HERO + 1) % 12;
        HERO_TICK = now;
        repaint = true;
    }
    repaint
}

// ---------- Traductions espagnoles et allemandes ----------

/// `(français, anglais, espagnol, allemand)`, retrouvées par le couple
/// français/anglais passé à `t` (aussi pour fonts.rs).
const STORE_TRANSLATIONS: &[(&[u8], &[u8], &[u8], &[u8])] = &[
    (
        b"Une installation est d\xE9j\xE0 en cours : attendez qu'elle se termine.",
        b"An installation is already running: wait for it to finish.",
        b"Ya hay una instalaci\xF3n en curso: espere a que termine.",
        b"Es l\xE4uft bereits eine Installation: Warten Sie, bis sie abgeschlossen ist.",
    ),
    (
        b"Test\xE9s sur Winlux",
        b"Tested on Winlux",
        b"Probados en Winlux",
        b"Auf Winlux getestet",
    ),
    (
        b"Install\xE9s et utilis\xE9s sur notre banc de test, depuis le site officiel de l'\xE9diteur.",
        b"Installed and used on our test bench, from the publisher's official site.",
        b"Instalados y usados en nuestro banco de pruebas, desde el sitio oficial del editor.",
        b"Auf unserem Pr\xFCfstand installiert und verwendet, von der offiziellen Seite des Herausgebers.",
    ),
    (
        b"Autres programmes Windows",
        b"Other Windows programs",
        b"Otros programas de Windows",
        b"Weitere Windows-Programme",
    ),
    (
        b"Applis",
        b"Apps",
        b"Apps",
        b"Apps",
    ),
    (
        b"Jeux",
        b"Games",
        b"Juegos",
        b"Spiele",
    ),
    (
        b"Gratuit",
        b"Free",
        b"Gratis",
        b"Kostenlos",
    ),
    (
        b"Applications essentielles",
        b"Essential apps",
        b"Aplicaciones esenciales",
        b"Wichtige Apps",
    ),
    (
        b"Applications",
        b"Apps",
        b"Aplicaciones",
        b"Apps",
    ),
    (
        b"Toutes",
        b"All",
        b"Todas",
        b"Alle",
    ),
    (
        b"Ouvrir la Biblioth\xE8que",
        b"Open the Library",
        b"Abrir la biblioteca",
        b"Bibliothek \xF6ffnen",
    ),
    (
        b"Aucun logiciel pour l\'instant : actualisez la biblioth\xE8que.",
        b"No software yet: refresh the library.",
        b"Todav\xEDa no hay programas: actualiza la biblioteca.",
        b"Noch keine Software: Bibliothek aktualisieren.",
    ),
    (
        b"Programmes .exe ex\xE9cut\xE9s directement par Winlux. Chaque fichier est v\xE9rifi\xE9 par son empreinte avant l\'installation.",
        b"Programs (.exe) run directly by Winlux. Every file is checked against its hash before installation.",
        b"Programas .exe ejecutados directamente por Winlux. Cada archivo se comprueba con su huella antes de instalarlo.",
        b"Von Winlux direkt ausgef\xFChrte .exe-Programme. Jede Datei wird vor der Installation anhand ihrer Pr\xFCfsumme kontrolliert.",
    ),
    (
        b"Aucun catalogue de programmes Windows n\'est ajout\xE9. Vous pouvez en activer un dans Param\xE8tres > Applications > Boutiques de logiciels, ou installer un fichier .exe ou .msi depuis la Biblioth\xE8que.",
        b"No Windows program catalog has been added. You can turn one on in Settings > Apps > Software stores, or install an .exe or .msi file from the Library.",
        b"No se ha a\xF1adido ning\xFAn cat\xE1logo de programas de Windows. Puedes activar uno en Configuraci\xF3n > Aplicaciones > Tiendas de programas, o instalar un archivo .exe o .msi desde la Biblioteca.",
        b"Es wurde kein Katalog mit Windows-Programmen hinzugef\xFCgt. Sie k\xF6nnen einen unter Einstellungen > Apps > Software-Stores aktivieren oder eine .exe- oder .msi-Datei aus der Bibliothek installieren.",
    ),
    (
        b"Une installation est d\xE9j\xE0 en cours.",
        b"An installation is already running.",
        b"Ya hay una instalaci\xF3n en curso.",
        b"Es l\xE4uft bereits eine Installation.",
    ),
    (
        b"Pr\xE9paration...",
        b"Preparing...",
        b"Preparando...",
        b"Vorbereitung...",
    ),
    (
        b"Le service du Store n'est pas disponible.",
        b"The Store service is not available.",
        b"El servicio de la Store no est\xE1 disponible.",
        b"Der Store-Dienst ist nicht verf\xFCgbar.",
    ),
    (
        b"Rechercher des applications et des jeux",
        b"Search apps and games",
        b"Buscar aplicaciones y juegos",
        b"Apps und Spiele suchen",
    ),
    (
        b"Chargement de la biblioth\xE8que...",
        b"Loading the library...",
        b"Cargando la biblioteca...",
        b"Bibliothek wird geladen...",
    ),
    (
        b"Accueil",
        b"Home",
        b"Inicio",
        b"Startseite",
    ),
    (
        b"Biblioth\xE8que",
        b"Library",
        b"Biblioteca",
        b"Bibliothek",
    ),
    (
        b"Actualisation...",
        b"Refreshing...",
        b"Actualizando...",
        b"Wird aktualisiert...",
    ),
    (
        b"Actualiser",
        b"Refresh",
        b"Actualizar",
        b"Aktualisieren",
    ),
    (
        b"Sources : ",
        b"Sources: ",
        b"Fuentes: ",
        b"Quellen: ",
    ),
    (
        b"Install\xE9",
        b"Installed",
        b"Instalado",
        b"Installiert",
    ),
    (
        b"Mise \xE0 jour",
        b"Update",
        b"Actualizaci\xF3n",
        b"Update",
    ),
    (
        b"Obtenir",
        b"Get",
        b"Obtener",
        b"Holen",
    ),
    (
        b"S\xC9LECTION WINLUX",
        b"WINLUX PICKS",
        b"SELECCI\xD3N WINLUX",
        b"WINLUX-AUSWAHL",
    ),
    (
        b"Voir",
        b"View",
        b"Ver",
        b"Ansehen",
    ),
    (
        b"Mettre \xE0 jour",
        b"Update",
        b"Actualizar",
        b"Aktualisieren",
    ),
    (
        b"Tout afficher",
        b"See all",
        b"Ver todo",
        b"Alle anzeigen",
    ),
    (
        b"Aucun logiciel dans cette cat\xE9gorie pour l'instant.",
        b"No software in this category yet.",
        b"Todav\xEDa no hay software en esta categor\xEDa.",
        b"In dieser Kategorie gibt es noch keine Software.",
    ),
    (
        b"R\xE9sultats pour \xAB ",
        b"Results for \"",
        b"Resultados de \xAB ",
        b"Ergebnisse f\xFCr \x84",
    ),
    (
        b" \xBB",
        b"\"",
        b" \xBB",
        b"\x93",
    ),
    (
        b"Aucun r\xE9sultat.",
        b"No results.",
        b"No hay resultados.",
        b"Keine Ergebnisse.",
    ),
    (
        b"D\xE9sinstaller",
        b"Uninstall",
        b"Desinstalar",
        b"Deinstallieren",
    ),
    (
        b"Mises \xE0 jour",
        b"Updates",
        b"Actualizaciones",
        b"Updates",
    ),
    (
        b"Tout mettre \xE0 jour",
        b"Update all",
        b"Actualizar todo",
        b"Alle aktualisieren",
    ),
    (
        b" mise(s) \xE0 jour du syst\xE8me Linux (apt) incluse(s) dans \xAB Tout mettre \xE0 jour \xBB.",
        b" Linux system update(s) (apt) included in \"Update all\".",
        b" actualizaci\xF3n(es) del sistema Linux (apt) incluida(s) en \xABActualizar todo\xBB.",
        b" Linux-Systemupdate(s) (apt) in \x84Alle aktualisieren\x93 enthalten.",
    ),
    (
        b"Tout est \xE0 jour.",
        b"Everything is up to date.",
        b"Todo est\xE1 actualizado.",
        b"Alles ist auf dem neuesten Stand.",
    ),
    (
        b"Install\xE9s",
        b"Installed",
        b"Instalados",
        b"Installiert",
    ),
    (
        b"Programmes Windows",
        b"Windows programs",
        b"Programas de Windows",
        b"Windows-Programme",
    ),
    (
        b"Aucun programme Windows install\xE9 pour l'instant.",
        b"No Windows program installed yet.",
        b"Todav\xEDa no hay ning\xFAn programa de Windows instalado.",
        b"Noch kein Windows-Programm installiert.",
    ),
    (
        b"Installer depuis un fichier",
        b"Install from a file",
        b"Instalar desde un archivo",
        b"Aus einer Datei installieren",
    ),
    (
        b"Installateurs .exe/.msi (runtime Winlux), .deb, .AppImage du dossier T\xE9l\xE9chargements, ou glissez un fichier ici depuis l'Explorateur.",
        b"Installers (.exe/.msi via the Winlux runtime, .deb, .AppImage) from Downloads, or drop a file here from Explorer.",
        b"Instaladores .exe/.msi (runtime de Winlux), .deb y .AppImage de la carpeta Descargas, o arrastre un archivo aqu\xED desde el Explorador.",
        b"Installationsprogramme .exe/.msi (Winlux-Laufzeit), .deb und .AppImage aus dem Ordner Downloads, oder ziehen Sie eine Datei aus dem Explorer hierher.",
    ),
    (
        b"Installer",
        b"Install",
        b"Instalar",
        b"Installieren",
    ),
    (
        b"Source : ",
        b"Source: ",
        b"Fuente: ",
        b"Quelle: ",
    ),
    (
        b"Confirmer la d\xE9sinstallation",
        b"Confirm uninstall",
        b"Confirmar la desinstalaci\xF3n",
        b"Deinstallation best\xE4tigen",
    ),
    (
        b"Site web",
        b"Website",
        b"Sitio web",
        b"Website",
    ),
    (
        b"Chargement de la fiche...",
        b"Loading details...",
        b"Cargando la ficha...",
        b"Details werden geladen...",
    ),
    (
        b"Captures d'\xE9cran",
        b"Screenshots",
        b"Capturas de pantalla",
        b"Screenshots",
    ),
    (
        b"Informations",
        b"Information",
        b"Informaci\xF3n",
        b"Informationen",
    ),
    (
        b" Mo",
        b" MB",
        b" MB",
        b" MB",
    ),
    (
        b" Ko",
        b" KB",
        b" KB",
        b" KB",
    ),
    (
        b"Version install\xE9e",
        b"Installed version",
        b"Versi\xF3n instalada",
        b"Installierte Version",
    ),
    (
        b"Derni\xE8re version",
        b"Latest version",
        b"\xDAltima versi\xF3n",
        b"Neueste Version",
    ),
    (
        b"Taille",
        b"Size",
        b"Tama\xF1o",
        b"Gr\xF6\xDFe",
    ),
    (
        b"\xC9diteur",
        b"Publisher",
        b"Editor",
        b"Herausgeber",
    ),
    (
        b"Licence",
        b"License",
        b"Licencia",
        b"Lizenz",
    ),
    (
        b"Source",
        b"Source",
        b"Fuente",
        b"Quelle",
    ),
    (
        b"Nouveaut\xE9s",
        b"What's new",
        b"Novedades",
        b"Neuigkeiten",
    ),
    (
        b"Portez ce vieux whisky au juge blond qui fume. 0123456789",
        b"The quick brown fox jumps over the lazy dog. 0123456789",
        b"El veloz murci\xE9lago hind\xFA com\xEDa feliz cardillo y kiwi. 0123456789",
        b"Victor jagt zw\xF6lf Boxk\xE4mpfer quer \xFCber den gro\xDFen Sylter Deich. 0123456789",
    ),
    (
        b"Le service des polices n'est pas disponible.",
        b"The font service is not available.",
        b"El servicio de fuentes no est\xE1 disponible.",
        b"Der Schriftartendienst ist nicht verf\xFCgbar.",
    ),
    (
        b"D\xE9posez un fichier de police .ttf, .otf ou .ttc.",
        b"Drop a .ttf, .otf or .ttc font file.",
        b"Suelte un archivo de fuente .ttf, .otf o .ttc.",
        b"Legen Sie eine Schriftdatei .ttf, .otf oder .ttc ab.",
    ),
    (
        b"Polices",
        b"Fonts",
        b"Fuentes",
        b"Schriftarten",
    ),
    (
        b" familles",
        b" families",
        b" familias",
        b" Familien",
    ),
    (
        b"Rechercher une police",
        b"Search fonts",
        b"Buscar una fuente",
        b"Schriftart suchen",
    ),
    (
        b"Installer une police",
        b"Install a font",
        b"Instalar una fuente",
        b"Schriftart installieren",
    ),
    (
        b"Vous",
        b"You",
        b"Usted",
        b"Sie",
    ),
    (
        b"Chargement des polices...",
        b"Loading fonts...",
        b"Cargando fuentes...",
        b"Schriftarten werden geladen...",
    ),
    (
        b"Install\xE9e pour vous",
        b"Installed for you",
        b"Instalada para usted",
        b"F\xFCr Sie installiert",
    ),
    (
        b"Police syst\xE8me",
        b"System font",
        b"Fuente del sistema",
        b"Systemschriftart",
    ),
    (
        b"Tapez un texte d'exemple",
        b"Type sample text",
        b"Escriba un texto de ejemplo",
        b"Beispieltext eingeben",
    ),
    (
        b"Disponible aussi pour les programmes .exe (EnumFontFamilies, CreateFont).",
        b"Also available to .exe programs (EnumFontFamilies, CreateFont).",
        b"Tambi\xE9n disponible para los programas .exe (EnumFontFamilies, CreateFont).",
        b"Auch f\xFCr .exe-Programme verf\xFCgbar (EnumFontFamilies, CreateFont).",
    ),
    (
        b"Aucune police trouv\xE9e. Installez un fichier .ttf ou .otf, ou glissez-le ici depuis l'Explorateur.",
        b"No fonts found. Install a .ttf or .otf file, or drop it here from Explorer.",
        b"No se encontr\xF3 ninguna fuente. Instale un archivo .ttf u .otf, o arr\xE1strelo aqu\xED desde el Explorador.",
        b"Keine Schriftarten gefunden. Installieren Sie eine .ttf- oder .otf-Datei oder ziehen Sie sie aus dem Explorer hierher.",
    ),
    (
        b"Polices trouv\xE9es dans T\xE9l\xE9chargements, Documents et Bureau. Vous pouvez aussi glisser un fichier depuis l'Explorateur sur cette fen\xEAtre.",
        b"Fonts found in Downloads, Documents and Desktop. You can also drop a file from Explorer onto this window.",
        b"Fuentes encontradas en Descargas, Documentos y Escritorio. Tambi\xE9n puede arrastrar un archivo desde el Explorador a esta ventana.",
        b"Schriftarten aus Downloads, Dokumente und Schreibtisch. Sie k\xF6nnen auch eine Datei aus dem Explorer auf dieses Fenster ziehen.",
    ),
    (
        b"Aucun fichier de police trouv\xE9.",
        b"No font file found.",
        b"No se encontr\xF3 ning\xFAn archivo de fuente.",
        b"Keine Schriftdatei gefunden.",
    ),
    (
        b"Fermer",
        b"Close",
        b"Cerrar",
        b"Schlie\xDFen",
    ),
    (
        b"Description",
        b"Description",
        b"Descripci\xF3n",
        b"Beschreibung",
    ),
];
