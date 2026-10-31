//! Historique des fichiers façon Windows : choix des dossiers protégés, de la
//! destination (disque externe, dossier ou partage réseau), fréquence et
//! conservation, puis navigation dans les instantanés pour restaurer un
//! fichier, un dossier ou une version antérieure.
//!
//! Le travail est fait par `winlux-backup` (sauvegardes incrémentales à liens
//! durs). Le bureau lit ses résultats sous Z:\run\winlux\backup\<compte>\ et
//! lui transmet les chemins dans un fichier de requête UTF-8, pour que les
//! espaces, guillemets et accents ne passent jamais par une ligne de commande.

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

const NAV_W: i32 = 214;
const SECTIONS: [&[u8]; 6] = [
    b"Vue d'ensemble",
    b"Dossiers",
    b"Destination",
    b"Fr\xE9quence",
    b"Restaurer des fichiers",
    b"Restauration syst\xE8me",
];
static mut SECTION: usize = 0;

const CAP: usize = 16384;
#[derive(Clone, Copy)]
struct Text {
    bytes: [u8; CAP],
    len: usize,
}
const EMPTY: Text = Text {
    bytes: [0; CAP],
    len: 0,
};
impl Text {
    fn get(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}
static mut STATUS: Text = EMPTY;
static mut SNAPS: Text = EMPTY;
static mut BROWSE: Text = EMPTY;
static mut VERSIONS: Text = EMPTY;
static mut DRIVES: Text = EMPTY;
static mut LOG: Text = EMPTY;

// Zones cliquables enregistrées pendant le dessin.
const MAX_HITS: usize = 96;
static mut HITS: [(i32, Rect); MAX_HITS] = [(0, rect(0, 0, 0, 0)); MAX_HITS];
static mut HIT_COUNT: usize = 0;
static mut HOVER: i32 = -1;

// Champs de saisie (Windows-1252) : dossier, destination, partage, compte, mot de passe.
const FIELD_COUNT: usize = 5;
const F_FOLDER: usize = 0;
const F_DEST: usize = 1;
const F_SHARE: usize = 2;
const F_ACCOUNT: usize = 3;
const F_PASSWORD: usize = 4;
static mut FIELDS: [[u8; 200]; FIELD_COUNT] = [[0; 200]; FIELD_COUNT];
static mut FIELD_LEN: [usize; FIELD_COUNT] = [0; FIELD_COUNT];
static mut FOCUS: i32 = -1;

// Navigation dans les instantanés : chemin UTF-8 brut, "/" = dossiers protégés.
static mut SNAP_INDEX: usize = usize::MAX;
static mut CUR_DIR: [u8; 1024] = [0; 1024];
static mut CUR_DIR_LEN: usize = 0;
static mut SELECTED: i32 = -1;
static mut SCROLL: i32 = 0;
static mut SHOW_VERSIONS: bool = false;
// Nom à resélectionner quand un autre instantané du même dossier est chargé.
static mut PENDING_SELECT: [u8; 256] = [0; 256];
static mut PENDING_SELECT_LEN: usize = 0;

static mut NOTICE: [u8; 160] = [0; 160];
static mut NOTICE_LEN: usize = 0;
static mut LAST_POLL: u32 = 0;
static mut LAST_STATUS: u32 = 0;
static mut OPENED: bool = false;

// ---------- Utilitaires ----------

fn field<'a>(data: &'a [u8], key: &[u8]) -> &'a [u8] {
    for line in data.split(|&c| c == b'\n') {
        if line.len() > key.len() && &line[..key.len()] == key && line[key.len()] == b'=' {
            return &line[key.len() + 1..];
        }
    }
    b""
}
fn lines_with<'a>(data: &'a [u8], key: &'a [u8]) -> impl Iterator<Item = &'a [u8]> + 'a {
    data.split(|&c| c == b'\n').filter_map(move |line| {
        if line.len() > key.len() && &line[..key.len()] == key && line[key.len()] == b'=' {
            Some(&line[key.len() + 1..])
        } else {
            None
        }
    })
}
fn part(line: &[u8], n: usize) -> &[u8] {
    line.split(|&c| c == b'|').nth(n).unwrap_or(b"")
}
fn number(s: &[u8]) -> u64 {
    s.iter()
        .take_while(|c| c.is_ascii_digit())
        .fold(0u64, |n, &c| n.saturating_mul(10).saturating_add((c - b'0') as u64))
}

/// UTF-8 (fichiers Linux) vers Windows-1252 pour `TextOutA`.
fn to_ansi(src: &[u8], out: &mut [u8]) -> usize {
    let s = match core::str::from_utf8(src) {
        Ok(s) => s,
        Err(e) => core::str::from_utf8(&src[..e.valid_up_to()]).unwrap_or(""),
    };
    let mut n = 0;
    for ch in s.chars() {
        if n >= out.len() {
            break;
        }
        out[n] = match ch as u32 {
            0..=0x7f | 0xa0..=0xff => ch as u8,
            0x20ac => 0x80,
            0x2019 => b'\'',
            0x2013 | 0x2014 => b'-',
            0x153 => 0x9c,
            _ => b'?',
        };
        n += 1;
    }
    n
}
/// Windows-1252 (saisie clavier) vers UTF-8 pour `winlux-backup`.
fn to_utf8(src: &[u8], out: &mut [u8]) -> usize {
    let mut n = 0;
    for &b in src {
        if n + 3 > out.len() {
            break;
        }
        match b {
            0..=0x7f => {
                out[n] = b;
                n += 1;
            }
            0x80 => {
                out[n..n + 3].copy_from_slice(&[0xe2, 0x82, 0xac]);
                n += 3;
            }
            0x81..=0x9f => {
                out[n] = b'?';
                n += 1;
            }
            _ => {
                out[n] = 0xc0 | (b >> 6);
                out[n + 1] = 0x80 | (b & 0x3f);
                n += 2;
            }
        }
    }
    n
}
fn fmt_size(bytes: u64, out: &mut [u8; 24]) -> usize {
    let (value, unit): (u64, &[u8]) = if bytes >= 1 << 30 {
        (bytes * 10 / (1 << 30), b" Go")
    } else if bytes >= 1 << 20 {
        (bytes * 10 / (1 << 20), b" Mo")
    } else if bytes >= 1024 {
        (bytes * 10 / 1024, b" Ko")
    } else {
        (bytes * 10, b" octets")
    };
    let mut digits = [0u8; 20];
    let mut d = 0;
    let mut v = value / 10;
    loop {
        digits[d] = b'0' + (v % 10) as u8;
        d += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    let mut n = 0;
    while d > 0 {
        d -= 1;
        out[n] = digits[d];
        n += 1;
    }
    if bytes >= 1024 && value % 10 != 0 {
        out[n] = b',';
        out[n + 1] = b'0' + (value % 10) as u8;
        n += 2;
    }
    out[n..n + unit.len()].copy_from_slice(unit);
    n + unit.len()
}

unsafe fn notice(msg: &[u8]) {
    let n = msg.len().min(NOTICE.len());
    NOTICE[..n].copy_from_slice(&msg[..n]);
    NOTICE_LEN = n;
}

/// Chemin `Z:\run\winlux\backup\<compte>\<nom>` terminé par NUL.
unsafe fn run_path(name: &[u8], out: &mut [u8; 192]) -> *const i8 {
    let user = crate::login::current_user_name();
    let mut n = 0;
    for part in [b"Z:\\run\\winlux\\backup\\".as_slice(), user, b"\\", name] {
        let k = part.len().min(out.len() - 1 - n);
        out[n..n + k].copy_from_slice(&part[..k]);
        n += k;
    }
    out[n] = 0;
    out.as_ptr().cast()
}

unsafe fn read_into(name: &[u8], target: &mut Text) -> bool {
    let mut path = [0u8; 192];
    let h = CreateFileA(
        run_path(name, &mut path),
        0x8000_0000,
        7,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    let mut next = EMPTY;
    if !h.is_null() && h as isize != -1 {
        let mut read = 0u32;
        ReadFile(
            h,
            next.bytes.as_mut_ptr(),
            CAP as u32,
            &mut read,
            core::ptr::null_mut(),
        );
        CloseHandle(h);
        next.len = (read as usize).min(CAP);
    }
    let changed = next.get() != target.get();
    *target = next;
    changed
}

/// Lance `winlux-backup <commande> <compte>` détaché.
unsafe fn spawn(command: &[u8]) {
    let user = crate::login::current_user_name();
    let mut args = [0u8; 96];
    let mut n = 0;
    for part in [command, b" ", user] {
        let k = part.len().min(args.len() - 1 - n);
        args[n..n + k].copy_from_slice(&part[..k]);
        n += k;
    }
    args[n] = 0;
    if ShellExecuteA(
        core::ptr::null_mut(),
        c"open".as_ptr(),
        c"winlux-backup".as_ptr(),
        args.as_ptr().cast(),
        core::ptr::null(),
        0,
    ) as isize
        <= 32
    {
        notice(b"Le service d'historique des fichiers (winlux-backup) est introuvable.");
    }
}

/// Écrit une requête (une valeur UTF-8 par ligne) puis la fait exécuter.
unsafe fn request(lines: &[&[u8]]) -> bool {
    let mut path = [0u8; 192];
    let p = run_path(b"request", &mut path);
    if GetFileAttributesA(p) != u32::MAX {
        notice(b"Une op\xE9ration est d\xE9j\xE0 en cours, r\xE9essayez dans un instant.");
        return false;
    }
    let mut data = [0u8; 2048];
    let mut n = 0;
    for line in lines {
        let k = line.len().min(data.len() - 1 - n);
        data[n..n + k].copy_from_slice(&line[..k]);
        n += k;
        data[n] = b'\n';
        n += 1;
    }
    let h = CreateFileA(p, 0x4000_0000, 0, core::ptr::null_mut(), 2, 0, core::ptr::null_mut());
    if h.is_null() || h as isize == -1 {
        notice(b"Impossible de pr\xE9parer la demande (dossier /run/winlux/backup).");
        return false;
    }
    let mut written = 0u32;
    WriteFile(h, data.as_ptr(), n as u32, &mut written, core::ptr::null_mut());
    CloseHandle(h);
    spawn(b"request");
    LAST_POLL = 0;
    true
}

unsafe fn status(key: &[u8]) -> &'static [u8] {
    field(STATUS.get(), key)
}

unsafe fn snapshot_count() -> usize {
    SNAPS.get().split(|&c| c == b'\n').filter(|l| !l.is_empty()).count()
}
unsafe fn snapshot(i: usize) -> &'static [u8] {
    SNAPS
        .get()
        .split(|&c| c == b'\n')
        .filter(|l| !l.is_empty())
        .nth(i)
        .unwrap_or(b"")
}
unsafe fn cur_dir() -> &'static [u8] {
    if CUR_DIR_LEN == 0 {
        b"/"
    } else {
        &CUR_DIR[..CUR_DIR_LEN]
    }
}
unsafe fn set_dir(dir: &[u8]) {
    let n = dir.len().min(CUR_DIR.len());
    CUR_DIR[..n].copy_from_slice(&dir[..n]);
    CUR_DIR_LEN = if dir == b"/" { 0 } else { n };
    SELECTED = -1;
    SCROLL = 0;
    SHOW_VERSIONS = false;
}
/// Les entrées de BROWSE ne valent que pour l'instantané et le dossier affichés.
unsafe fn browse_current() -> bool {
    let id = part(snapshot(SNAP_INDEX), 0);
    !id.is_empty() && field(BROWSE.get(), b"snapshot") == id && field(BROWSE.get(), b"dir") == cur_dir()
}
unsafe fn entry(i: usize) -> &'static [u8] {
    lines_with(BROWSE.get(), b"entry").nth(i).unwrap_or(b"")
}
unsafe fn entry_count() -> usize {
    if browse_current() {
        lines_with(BROWSE.get(), b"entry").count()
    } else {
        0
    }
}
/// Chemin absolu UTF-8 de l'entrée i du dossier affiché.
unsafe fn entry_path(i: usize, out: &mut [u8; 1100]) -> usize {
    let name = part(entry(i), 1);
    if CUR_DIR_LEN == 0 {
        let n = name.len().min(out.len());
        out[..n].copy_from_slice(&name[..n]);
        return n;
    }
    let mut n = CUR_DIR_LEN.min(out.len() - 1);
    out[..n].copy_from_slice(&CUR_DIR[..n]);
    out[n] = b'/';
    n += 1;
    let k = name.len().min(out.len() - n);
    out[n..n + k].copy_from_slice(&name[..k]);
    n + k
}
unsafe fn browse() {
    let id = part(snapshot(SNAP_INDEX), 0);
    if id.is_empty() {
        return;
    }
    let mut snap = [0u8; 32];
    let n = id.len().min(32);
    snap[..n].copy_from_slice(&id[..n]);
    let mut dir = [0u8; 1024];
    let d = cur_dir();
    dir[..d.len()].copy_from_slice(d);
    request(&[b"browse", &snap[..n], &dir[..d.len()]]);
}

// ---------- Rafraîchissement ----------

pub unsafe fn open() {
    if !OPENED {
        OPENED = true;
        spawn(b"status");
    }
    LAST_POLL = 0;
}

pub unsafe fn tick(_f: &Frame) -> bool {
    let now = GetTickCount();
    if LAST_POLL != 0 && now.wrapping_sub(LAST_POLL) < 400 {
        return false;
    }
    LAST_POLL = now;
    if LAST_STATUS == 0 || now.wrapping_sub(LAST_STATUS) > 5000 {
        LAST_STATUS = now;
        let mut path = [0u8; 192];
        // Pas de nouvel état pendant une requête : elle le régénère elle-même.
        if GetFileAttributesA(run_path(b"request", &mut path)) == u32::MAX {
            spawn(b"status");
        }
    }
    let mut changed = read_into(b"status.txt", &mut STATUS);
    changed |= read_into(b"snapshots.txt", &mut SNAPS);
    changed |= read_into(b"browse.txt", &mut BROWSE);
    changed |= read_into(b"versions.txt", &mut VERSIONS);
    let mut drives = EMPTY;
    let h = CreateFileA(
        c"Z:\\run\\winlux\\backup\\drives.txt".as_ptr(),
        0x8000_0000,
        7,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if !h.is_null() && h as isize != -1 {
        let mut read = 0u32;
        ReadFile(h, drives.bytes.as_mut_ptr(), CAP as u32, &mut read, core::ptr::null_mut());
        CloseHandle(h);
        drives.len = (read as usize).min(CAP);
    }
    if drives.get() != DRIVES.get() {
        DRIVES = drives;
        changed = true;
    }
    let mut user = [0u8; 40];
    let name = crate::login::current_user_name();
    let n = name.len().min(32);
    user[..n].copy_from_slice(&name[..n]);
    user[n..n + 4].copy_from_slice(b".log");
    changed |= read_into(&user[..n + 4], &mut LOG);
    if PENDING_SELECT_LEN > 0 && browse_current() {
        let wanted = &PENDING_SELECT[..PENDING_SELECT_LEN];
        if let Some(i) = (0..entry_count()).find(|&i| part(entry(i), 1) == wanted) {
            SELECTED = i as i32;
        } else {
            SELECTED = -1;
            SHOW_VERSIONS = false;
        }
        PENDING_SELECT_LEN = 0;
        changed = true;
    }
    let count = snapshot_count();
    if count > 0 && SNAP_INDEX >= count {
        // Ouverture : l'instantané le plus récent, comme Windows.
        SNAP_INDEX = count - 1;
        browse();
        changed = true;
    }
    changed
}

// ---------- Dessin ----------

unsafe fn hit_add(id: i32, r: Rect) {
    if HIT_COUNT < MAX_HITS {
        HITS[HIT_COUNT] = (id, r);
        HIT_COUNT += 1;
    }
}
fn hit_at(x: i32, y: i32) -> i32 {
    unsafe {
        for i in (0..HIT_COUNT).rev() {
            if inside(&HITS[i].1, x, y) {
                return HITS[i].0;
            }
        }
    }
    -1
}

unsafe fn button(g: &mut Gfx, x: i32, y: i32, w: i32, label: &[u8], id: i32, primary: bool, selected: bool) -> i32 {
    let p = pal();
    let r = rect(x, y, x + w, y + 32);
    let hover = HOVER == id;
    let (bg, fg) = if primary {
        (if hover { blend(p.accent, p.window, 220) } else { p.accent }, p.on_accent)
    } else if selected {
        (p.selected, p.accent)
    } else {
        (if hover { p.hover } else { p.surface }, p.text)
    };
    panel(
        g.hdc,
        g.b,
        r.left,
        r.top,
        r.right,
        r.bottom,
        5,
        bg,
        if selected { p.accent } else { p.border },
    );
    text_centered(g.hdc, g.f.body, 13, fg, (r.left + r.right) / 2, r.top + 9, label);
    hit_add(id, r);
    x + w + 8
}

unsafe fn input(g: &mut Gfx, x: i32, y: i32, w: i32, index: usize, placeholder: &[u8]) {
    let p = pal();
    let focused = FOCUS == index as i32;
    panel(
        g.hdc,
        g.b,
        x,
        y,
        x + w,
        y + 32,
        5,
        p.window,
        if focused { p.accent } else { p.border },
    );
    if focused {
        fill(g.hdc, g.b, x + 4, y + 29, x + w - 4, y + 31, p.accent);
    }
    let len = FIELD_LEN[index];
    let mut shown = [0u8; 200];
    let content: &[u8] = if len == 0 {
        placeholder
    } else if index == F_PASSWORD {
        shown[..len].fill(0x95);
        &shown[..len]
    } else {
        &FIELDS[index][..len]
    };
    text_fit(
        g.hdc,
        g.f.body,
        13,
        if len == 0 { p.text3 } else { p.text },
        x + 10,
        y + 9,
        content,
        w - 24,
    );
    if focused && (GetTickCount() / 530) % 2 == 0 {
        let cx = x + 10 + if len == 0 { 0 } else { text_width(content, 13).min(w - 24) };
        fill(g.hdc, g.b, cx, y + 8, cx + 1, y + 24, p.text);
    }
    hit_add(1000 + index as i32, rect(x, y, x + w, y + 32));
}

unsafe fn label_line(g: &mut Gfx, x: i32, y: i32, title: &[u8], value: &[u8], max_w: i32) {
    let p = pal();
    text(g.hdc, g.f.semibold, p.text, x, y, title);
    let mut ansi = [0u8; 240];
    let n = to_ansi(value, &mut ansi);
    text_fit(g.hdc, g.f.body, 13, p.text2, x, y + 20, &ansi[..n], max_w);
}

unsafe fn card(g: &mut Gfx, l: i32, t: i32, r: i32, b: i32) {
    let p = pal();
    panel(g.hdc, g.b, l, t, r, b, 8, p.surface, p.border);
}

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let c = &f.client;
    let p = pal();
    HIT_COUNT = 0;
    SetBkMode(g.hdc, 1);
    fill(g.hdc, g.b, c.left, c.top, c.right, c.bottom, p.window);
    fill(g.hdc, g.b, c.left, c.top, c.left + NAV_W, c.bottom, p.mica);
    fill(g.hdc, g.b, c.left + NAV_W, c.top, c.left + NAV_W + 1, c.bottom, p.border);
    glyph(g.hdc, g.b, g.f, Glyph::Refresh, c.left + 16, c.top + 16, 22);
    text(g.hdc, g.f.semibold, p.text, c.left + 46, c.top + 19, b"Historique des fichiers");
    for (i, name) in SECTIONS.iter().enumerate() {
        let r = rect(c.left + 8, c.top + 56 + i as i32 * 38, c.left + NAV_W - 8, c.top + 90 + i as i32 * 38);
        let id = 900 + i as i32;
        if SECTION == i {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 5, p.selected);
            fill(g.hdc, g.b, r.left, r.top + 9, r.left + 3, r.bottom - 9, p.accent);
        } else if HOVER == id {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 5, p.hover);
        }
        text_fit(g.hdc, g.f.body, 13, p.text, r.left + 14, r.top + 10, name, r.right - r.left - 20);
        hit_add(id, r);
    }
    let x = c.left + NAV_W + 28;
    let w = (c.right - 28 - x).max(200);
    let top = c.top + 20;
    text(g.hdc, g.f.large, p.text, x, top, SECTIONS[SECTION]);
    let y = top + 46;
    match SECTION {
        0 => draw_overview(g, x, y, w),
        1 => draw_folders(g, x, y, w, c.bottom),
        2 => draw_destination(g, x, y, w, c.bottom),
        3 => draw_frequency(g, x, y, w),
        4 => draw_restore(g, x, y, w, c.bottom),
        _ => draw_system(g, x, y, w),
    }
    if NOTICE_LEN > 0 {
        let ny = c.bottom - 34;
        fill(g.hdc, g.b, c.left + NAV_W + 1, ny - 6, c.right, c.bottom, p.surface);
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text2,
            x,
            ny + 2,
            &NOTICE[..NOTICE_LEN],
            w,
        );
    }
}

unsafe fn draw_overview(g: &mut Gfx, x: i32, mut y: i32, w: i32) {
    let p = pal();
    if STATUS.len == 0 {
        text(g.hdc, g.f.body, p.text2, x, y, b"Lecture de l'\xE9tat de la sauvegarde\x85");
        return;
    }
    let enabled = status(b"enabled") == b"1";
    let dest = status(b"destination");
    card(g, x, y, x + w, y + 96);
    let dot = if !enabled {
        p.text3
    } else if status(b"available") == b"1" {
        rgb(0x3C, 0xB3, 0x71)
    } else {
        rgb(0xE8, 0xA3, 0x17)
    };
    ellipse(g.hdc, g.b, x + 18, y + 22, x + 30, y + 34, dot);
    text(
        g.hdc,
        g.f.semibold,
        p.text,
        x + 42,
        y + 20,
        if enabled {
            b"L'historique des fichiers est activ\xE9"
        } else {
            b"L'historique des fichiers est d\xE9sactiv\xE9"
        },
    );
    let mut line = [0u8; 200];
    let detail: &[u8] = if status(b"running") == b"1" {
        b"Sauvegarde en cours\x85"
    } else if dest.is_empty() {
        b"Choisissez d'abord un lecteur dans Destination."
    } else if status(b"available") != b"1" {
        b"Destination indisponible : reconnectez le lecteur ou v\xE9rifiez le partage r\xE9seau."
    } else if status(b"last_result") == b"ok" {
        let mut n = 0;
        for part in [b"Derni\xE8re sauvegarde : ".as_slice(), status(b"last_run_text")] {
            let k = part.len().min(line.len() - n);
            line[n..n + k].copy_from_slice(&part[..k]);
            n += k;
        }
        &line[..n]
    } else {
        b"Aucune sauvegarde n'a encore \xE9t\xE9 effectu\xE9e."
    };
    text_fit(g.hdc, g.f.body, 13, p.text2, x + 42, y + 44, detail, w - 60);
    let mut bx = x + 42;
    bx = button(g, bx, y + 60 - 2, 150, if enabled { b"D\xE9sactiver" } else { b"Activer" }, 1, !enabled, false);
    if !dest.is_empty() {
        button(g, bx, y + 58, 200, b"Sauvegarder maintenant", 2, enabled, false);
    }
    y += 116;
    label_line(g, x, y, b"Destination", if dest.is_empty() { b"Aucune" } else { dest }, w);
    y += 50;
    if status(b"available") == b"1" {
        let mut buf = [0u8; 120];
        let mut n = 0;
        for part in [
            status(b"free"),
            b" libres".as_slice(),
            if status(b"used").is_empty() { b"" } else { b" \x95 historique : " },
            status(b"used"),
        ] {
            let k = part.len().min(buf.len() - n);
            buf[n..n + k].copy_from_slice(&part[..k]);
            n += k;
        }
        if !status(b"free").is_empty() {
            label_line(g, x, y, b"Espace", &buf[..n], w);
            y += 50;
        }
    }
    let mut folders = [0u8; 64];
    let count = status(b"folders");
    let mut n = count.len().min(8);
    folders[..n].copy_from_slice(&count[..n]);
    let suffix = b" dossier(s) prot\xE9g\xE9(s)";
    folders[n..n + suffix.len()].copy_from_slice(suffix);
    n += suffix.len();
    text(g.hdc, g.f.semibold, p.text, x, y, b"Dossiers");
    text(g.hdc, g.f.body, p.text2, x, y + 20, &folders[..n]);
    y += 50;
    if status(b"same_disk") == b"1" {
        text_fit(
            g.hdc,
            g.f.body,
            13,
            rgb(0xE8, 0xA3, 0x17),
            x,
            y,
            b"Attention : la destination est sur le m\xEAme disque que vos fichiers. Pr\xE9f\xE9rez un disque externe.",
            w,
        );
        y += 28;
    }
    if LOG.len > 0 && status(b"last_result") != b"ok" {
        let last = LOG
            .get()
            .split(|&c| c == b'\n')
            .filter(|l| !l.is_empty())
            .last()
            .unwrap_or(b"");
        let mut ansi = [0u8; 200];
        let n = to_ansi(last, &mut ansi);
        text_fit(g.hdc, g.f.body, 13, p.text2, x, y, &ansi[..n], w);
    }
}

unsafe fn draw_folders(g: &mut Gfx, x: i32, mut y: i32, w: i32, bottom: i32) {
    let p = pal();
    text_fit(
        g.hdc,
        g.f.body,
        13,
        p.text2,
        x,
        y,
        b"Les fichiers de ces dossiers et de leurs sous-dossiers sont sauvegard\xE9s.",
        w,
    );
    y += 30;
    for (i, line) in lines_with(STATUS.get(), b"folder").enumerate() {
        if y > bottom - 190 {
            break;
        }
        card(g, x, y, x + w, y + 48);
        glyph(g.hdc, g.b, g.f, Glyph::Folder, x + 14, y + 13, 20);
        let mut ansi = [0u8; 240];
        let n = to_ansi(part(line, 1), &mut ansi);
        text_fit(g.hdc, g.f.body, 13, p.text, x + 46, y + 9, &ansi[..n], w - 170);
        if part(line, 0) == b"0" {
            text(g.hdc, g.f.small, p.text3, x + 46, y + 27, b"Dossier absent pour l'instant");
        }
        button(g, x + w - 110, y + 8, 98, b"Retirer", 100 + i as i32, false, false);
        y += 56;
    }
    y += 8;
    text(g.hdc, g.f.semibold, p.text, x, y, b"Ajouter un dossier");
    y += 26;
    let libraries: [&[u8]; 6] = [b"Documents", b"Images", b"Bureau", b"Musique", b"Vid\xE9os", b"T\xE9l\xE9chargements"];
    let mut bx = x;
    for (i, name) in libraries.iter().enumerate() {
        let bw = text_width(name, 13) + 28;
        if bx + bw > x + w {
            bx = x;
            y += 40;
        }
        bx = button(g, bx, y, bw, name, 200 + i as i32, false, false);
    }
    y += 44;
    input(g, x, y, (w - 120).max(120), F_FOLDER, b"Autre dossier, par exemple /home/moi/Projets");
    button(g, x + (w - 120).max(120) + 8, y, 112, b"Ajouter", 3, true, false);
}

unsafe fn draw_destination(g: &mut Gfx, x: i32, mut y: i32, w: i32, bottom: i32) {
    let p = pal();
    label_line(
        g,
        x,
        y,
        b"Destination actuelle",
        if status(b"destination").is_empty() { b"Aucune" } else { status(b"destination") },
        w,
    );
    y += 54;
    text(g.hdc, g.f.semibold, p.text, x, y, b"Lecteurs externes d\xE9tect\xE9s");
    button(g, x + w - 120, y - 6, 120, b"Actualiser", 4, false, false);
    y += 30;
    let mut any = false;
    for (i, line) in lines_with(DRIVES.get(), b"drive").enumerate() {
        if y > bottom - 260 {
            break;
        }
        any = true;
        card(g, x, y, x + w, y + 48);
        glyph(g.hdc, g.b, g.f, Glyph::Drive, x + 14, y + 13, 20);
        let mut ansi = [0u8; 240];
        let n = to_ansi(part(line, 0), &mut ansi);
        text_fit(g.hdc, g.f.body, 13, p.text, x + 46, y + 8, &ansi[..n], w - 200);
        let mut free = [0u8; 40];
        let k = part(line, 1).len().min(30);
        free[..k].copy_from_slice(&part(line, 1)[..k]);
        free[k..k + 7].copy_from_slice(b" libres");
        text(g.hdc, g.f.small, p.text2, x + 46, y + 27, &free[..k + 7]);
        let selected = part(line, 0) == status(b"destination");
        button(g, x + w - 120, y + 8, 108, if selected { b"Utilis\xE9" } else { b"Utiliser" }, 300 + i as i32, false, selected);
        y += 56;
    }
    if !any {
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text3,
            x,
            y,
            b"Aucun disque externe mont\xE9. Branchez-en un puis cliquez sur Actualiser.",
            w,
        );
        y += 28;
    }
    y += 10;
    text(g.hdc, g.f.semibold, p.text, x, y, b"Dossier ou disque mont\xE9");
    y += 26;
    let fw = (w - 120).max(120);
    input(g, x, y, fw, F_DEST, b"/media/moi/Sauvegardes");
    button(g, x + fw + 8, y, 112, b"Utiliser", 5, false, false);
    y += 50;
    text(g.hdc, g.f.semibold, p.text, x, y, b"Partage r\xE9seau (SMB ou NFS)");
    y += 26;
    input(g, x, y, w, F_SHARE, b"smb://serveur/partage ou serveur:/export");
    y += 40;
    let half = (w - 8) / 2;
    input(g, x, y, half, F_ACCOUNT, b"Compte (facultatif)");
    input(g, x + half + 8, y, half, F_PASSWORD, b"Mot de passe");
    y += 42;
    let bx = button(g, x, y, 150, b"Se connecter", 6, true, false);
    if !status(b"destination").is_empty() {
        button(g, bx, y, 190, b"Ne plus utiliser", 7, false, false);
    }
}

const FREQUENCIES: [(&[u8], &[u8]); 8] = [
    (b"10", b"10 minutes"),
    (b"15", b"15 minutes"),
    (b"30", b"30 minutes"),
    (b"60", b"Toutes les heures"),
    (b"180", b"3 heures"),
    (b"360", b"6 heures"),
    (b"720", b"12 heures"),
    (b"1440", b"Tous les jours"),
];
const RETENTIONS: [(&[u8], &[u8]); 7] = [
    (b"0", b"Ind\xE9finiment"),
    (b"30", b"1 mois"),
    (b"90", b"3 mois"),
    (b"180", b"6 mois"),
    (b"365", b"1 an"),
    (b"730", b"2 ans"),
    (b"7", b"1 semaine"),
];

unsafe fn draw_frequency(g: &mut Gfx, x: i32, mut y: i32, w: i32) {
    let p = pal();
    text(g.hdc, g.f.semibold, p.text, x, y, b"Enregistrer des copies des fichiers");
    y += 28;
    let bw = ((w - 24) / 4).max(90);
    for (i, (value, label)) in FREQUENCIES.iter().enumerate() {
        let col = (i % 4) as i32;
        let row = (i / 4) as i32;
        button(g, x + col * (bw + 8), y + row * 40, bw, label, 400 + i as i32, false, status(b"frequency") == *value);
    }
    y += 100;
    text(g.hdc, g.f.semibold, p.text, x, y, b"Conserver les versions enregistr\xE9es");
    y += 28;
    for (i, (value, label)) in RETENTIONS.iter().enumerate() {
        let col = (i % 4) as i32;
        let row = (i / 4) as i32;
        button(g, x + col * (bw + 8), y + row * 40, bw, label, 500 + i as i32, false, status(b"keep_days") == *value);
    }
    y += 100;
    text_fit(
        g.hdc,
        g.f.body,
        13,
        p.text2,
        x,
        y,
        b"Les copies plus anciennes sont supprim\xE9es apr\xE8s chaque sauvegarde ; la plus r\xE9cente est toujours gard\xE9e.",
        w,
    );
    y += 32;
    button(g, x, y, 220, b"Nettoyer les versions", 8, false, false);
}

unsafe fn draw_restore(g: &mut Gfx, x: i32, mut y: i32, w: i32, bottom: i32) {
    let p = pal();
    let count = snapshot_count();
    if count == 0 {
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text2,
            x,
            y,
            if status(b"available") == b"1" {
                b"Aucune sauvegarde trouv\xE9e sur la destination."
            } else {
                b"Destination indisponible : branchez le lecteur de sauvegarde."
            },
            w,
        );
        button(g, x, y + 30, 140, b"Actualiser", 9, false, false);
        return;
    }
    // Barre de temps : instantané précédent / suivant.
    let snap = snapshot(SNAP_INDEX.min(count - 1));
    card(g, x, y, x + w, y + 52);
    button(g, x + 10, y + 10, 40, b"<", 10, false, false);
    button(g, x + w - 50, y + 10, 40, b">", 11, false, false);
    let mut title = [0u8; 120];
    let mut n = 0;
    let mut idx = [0u8; 24];
    let mut k = 0;
    for v in [SNAP_INDEX.min(count - 1) + 1, count] {
        let mut digits = [0u8; 10];
        let mut d = 0;
        let mut v = v;
        loop {
            digits[d] = b'0' + (v % 10) as u8;
            d += 1;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        while d > 0 {
            d -= 1;
            idx[k] = digits[d];
            k += 1;
        }
        if k < 12 {
            idx[k..k + 3].copy_from_slice(b" / ");
            k += 3;
        }
    }
    for part_bytes in [part(snap, 1), b"   (".as_slice(), &idx[..k], b")"] {
        let m = part_bytes.len().min(title.len() - n);
        title[n..n + m].copy_from_slice(&part_bytes[..m]);
        n += m;
    }
    text_centered(g.hdc, g.f.semibold, 13, p.text, x + w / 2, y + 18, &title[..n]);
    y += 64;
    // Fil d'Ariane et actions.
    let mut bx = button(g, x, y, 130, b"Dossier parent", 12, false, false);
    let mut ansi = [0u8; 240];
    let dn = to_ansi(cur_dir(), &mut ansi);
    let shown: &[u8] = if CUR_DIR_LEN == 0 { b"Dossiers sauvegard\xE9s" } else { &ansi[..dn] };
    text_fit(g.hdc, g.f.body, 13, p.text2, bx + 4, y + 9, shown, w - 140);
    y += 42;
    let list_bottom = bottom - 96;
    let row_h = 34;
    fill(g.hdc, g.b, x, y, x + w, y + 26, p.surface);
    text(g.hdc, g.f.semibold, p.text2, x + 40, y + 6, b"Nom");
    text(g.hdc, g.f.semibold, p.text2, x + w - 120, y + 6, b"Taille");
    y += 28;
    if !browse_current() {
        text(g.hdc, g.f.body, p.text3, x + 10, y + 8, b"Chargement\x85");
    } else if entry_count() == 0 {
        text(g.hdc, g.f.body, p.text3, x + 10, y + 8, b"Ce dossier \xE9tait vide.");
    }
    let visible = ((list_bottom - y) / row_h).max(1);
    let total = entry_count() as i32;
    SCROLL = SCROLL.clamp(0, (total - visible).max(0));
    for i in SCROLL..(SCROLL + visible).min(total) {
        let e = entry(i as usize);
        let ry = y + (i - SCROLL) * row_h;
        let id = 2000 + i;
        let r = rect(x, ry, x + w, ry + row_h - 2);
        if SELECTED == i {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, p.selected);
        } else if HOVER == id {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, p.hover);
        }
        let dir = part(e, 0) == b"d";
        glyph(g.hdc, g.b, g.f, if dir { Glyph::Folder } else { Glyph::File }, x + 10, ry + 7, 18);
        let mut name = [0u8; 240];
        let n = to_ansi(part(e, 1), &mut name);
        text_fit(g.hdc, g.f.body, 13, p.text, x + 40, ry + 9, &name[..n], w - 180);
        if !dir {
            let mut s = [0u8; 24];
            let n = fmt_size(number(part(e, 2)), &mut s);
            text(g.hdc, g.f.body, p.text2, x + w - 120, ry + 9, &s[..n]);
        }
        hit_add(id, r);
    }
    // Versions du fichier sélectionné.
    if SHOW_VERSIONS && SELECTED >= 0 {
        let vx = x + w / 2;
        let vy = y;
        panel(g.hdc, g.b, vx, vy, x + w, list_bottom, 8, p.surface, p.border);
        text(g.hdc, g.f.semibold, p.text, vx + 14, vy + 10, b"Versions de ce fichier");
        let mut path = [0u8; 1100];
        let n = entry_path(SELECTED as usize, &mut path);
        let mut ry = vy + 38;
        if field(VERSIONS.get(), b"path") != &path[..n] {
            text(g.hdc, g.f.body, p.text3, vx + 14, ry, b"Recherche des versions\x85");
        } else {
            for (i, v) in lines_with(VERSIONS.get(), b"version").enumerate() {
                if ry > list_bottom - 34 {
                    break;
                }
                let r = rect(vx + 6, ry, x + w - 6, ry + 30);
                let id = 3000 + i as i32;
                let current = part(v, 0) == part(snap, 0);
                if current || HOVER == id {
                    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, if current { p.selected } else { p.hover });
                }
                text(g.hdc, g.f.body, p.text, vx + 14, ry + 8, part(v, 1));
                let mut s = [0u8; 24];
                let k = fmt_size(number(part(v, 2)), &mut s);
                text(g.hdc, g.f.body, p.text2, x + w - 110, ry + 8, &s[..k]);
                hit_add(id, r);
                ry += 32;
            }
        }
    }
    let by = bottom - 84;
    let has = SELECTED >= 0 || CUR_DIR_LEN > 0;
    bx = x;
    if has {
        bx = button(g, bx, by, 200, b"Restaurer \xE0 l'emplacement", 13, true, false);
        bx = button(g, bx, by, 190, b"Restaurer une copie", 14, false, false);
    }
    if SELECTED >= 0 && part(entry(SELECTED as usize), 0) == b"f" {
        button(g, bx, by, 170, b"Versions ant\xE9rieures", 15, false, SHOW_VERSIONS);
    }
    if !has {
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text3,
            x,
            by + 9,
            b"Ouvrez un dossier ou s\xE9lectionnez un \xE9l\xE9ment pour le restaurer.",
            w,
        );
    }
}

unsafe fn draw_system(g: &mut Gfx, x: i32, mut y: i32, w: i32) {
    let p = pal();
    for line in [
        b"L'historique des fichiers prot\xE8ge vos documents personnels.".as_slice(),
        b"Les points de restauration syst\xE8me prot\xE8gent la configuration de Winlux",
        b"(comptes, r\xE9glages, paquets install\xE9s). Les deux se compl\xE8tent :",
        b"aucune sauvegarde ne d\xE9marre pendant une mise \xE0 jour du syst\xE8me.",
    ] {
        text_fit(g.hdc, g.f.body, 13, p.text2, x, y, line, w);
        y += 22;
    }
    y += 16;
    button(g, x, y, 260, b"Ouvrir la restauration syst\xE8me", 16, true, false);
}

// ---------- Interaction ----------

unsafe fn selected_path(out: &mut [u8; 1100]) -> usize {
    if SELECTED >= 0 {
        entry_path(SELECTED as usize, out)
    } else {
        let d = cur_dir();
        out[..d.len()].copy_from_slice(d);
        d.len()
    }
}

unsafe fn restore(mode: &[u8]) {
    let count = snapshot_count();
    if count == 0 {
        return;
    }
    let mut path = [0u8; 1100];
    let n = selected_path(&mut path);
    if n <= 1 {
        return;
    }
    let id = part(snapshot(SNAP_INDEX.min(count - 1)), 0);
    let mut snap = [0u8; 32];
    let k = id.len().min(32);
    snap[..k].copy_from_slice(&id[..k]);
    if request(&[b"restore", &snap[..k], &path[..n], mode]) {
        notice(if mode == b"replace" {
            b"Restauration lanc\xE9e : les fichiers actuels sont remplac\xE9s par cette version."
        } else {
            b"Restauration lanc\xE9e : une copie dat\xE9e est cr\xE9\xE9e \xE0 c\xF4t\xE9 de l'original."
        });
    }
}

unsafe fn field_utf8(index: usize, out: &mut [u8; 600]) -> usize {
    to_utf8(&FIELDS[index][..FIELD_LEN[index]], out)
}

unsafe fn submit(index: usize) {
    let mut value = [0u8; 600];
    let n = field_utf8(index, &mut value);
    match index {
        F_FOLDER if n > 0 => {
            if value[0] != b'/' {
                notice(b"Indiquez un chemin complet commen\xE7ant par /.");
                return;
            }
            if request(&[b"add", &value[..n]]) {
                FIELD_LEN[F_FOLDER] = 0;
                notice(b"Dossier ajout\xE9 \xE0 la sauvegarde.");
            }
        }
        F_DEST if n > 0 => {
            if request(&[b"set", b"DESTINATION", &value[..n]]) {
                notice(b"Destination enregistr\xE9e.");
            }
        }
        F_SHARE | F_ACCOUNT | F_PASSWORD => {
            let mut url = [0u8; 600];
            let u = field_utf8(F_SHARE, &mut url);
            if u == 0 {
                notice(b"Indiquez l'adresse du partage r\xE9seau.");
                return;
            }
            let mut account = [0u8; 600];
            let a = field_utf8(F_ACCOUNT, &mut account);
            let mut password = [0u8; 600];
            let pw = field_utf8(F_PASSWORD, &mut password);
            if request(&[b"share", &url[..u], &account[..a], &password[..pw]]) {
                FIELD_LEN[F_PASSWORD] = 0;
                notice(b"Connexion au partage r\xE9seau\x85");
            }
        }
        _ => {}
    }
}

unsafe fn home_folder(name: &[u8]) {
    let home = status(b"home");
    let mut path = [0u8; 300];
    let mut n = 0;
    for part in [home, b"/".as_slice()] {
        let k = part.len().min(200);
        path[n..n + k].copy_from_slice(&part[..k]);
        n += k;
    }
    let mut utf = [0u8; 600];
    let k = to_utf8(name, &mut utf);
    path[n..n + k.min(90)].copy_from_slice(&utf[..k.min(90)]);
    n += k.min(90);
    if request(&[b"add", &path[..n]]) {
        notice(b"Dossier ajout\xE9 \xE0 la sauvegarde.");
    }
}

unsafe fn activate(id: i32, double: bool) -> bool {
    match id {
        900..=905 => {
            SECTION = (id - 900) as usize;
            FOCUS = -1;
            NOTICE_LEN = 0;
            if SECTION == 2 {
                spawn(b"drives");
            }
            if SECTION == 4 {
                spawn(b"list");
                SNAP_INDEX = usize::MAX;
            }
        }
        1 => {
            let enable = status(b"enabled") != b"1";
            if enable && status(b"destination").is_empty() {
                SECTION = 2;
                spawn(b"drives");
                notice(b"Choisissez o\xF9 enregistrer les copies, puis activez l'historique.");
            } else {
                request(&[b"set", b"ENABLED", if enable { b"1" } else { b"0" }]);
            }
        }
        2 => {
            let user = crate::login::current_user_name();
            let mut args = [0u8; 64];
            let prefix = b"backup-run ";
            args[..prefix.len()].copy_from_slice(prefix);
            let k = user.len().min(40);
            args[prefix.len()..prefix.len() + k].copy_from_slice(&user[..k]);
            if let Ok(cmd) = core::ffi::CStr::from_bytes_until_nul(&args) {
                crate::settings::control(cmd);
            }
            notice(b"Sauvegarde lanc\xE9e. Vous pouvez continuer \xE0 travailler.");
        }
        3 => submit(F_FOLDER),
        4 => spawn(b"drives"),
        5 => submit(F_DEST),
        6 => submit(F_SHARE),
        7 => {
            request(&[b"set", b"DESTINATION"]);
        }
        8 => spawn(b"prune"),
        9 => spawn(b"list"),
        10 | 11 => {
            let count = snapshot_count();
            if count > 0 {
                let i = SNAP_INDEX.min(count - 1);
                SNAP_INDEX = if id == 10 { i.saturating_sub(1) } else { (i + 1).min(count - 1) };
                if SELECTED >= 0 {
                    let name = part(entry(SELECTED as usize), 1);
                    let n = name.len().min(PENDING_SELECT.len());
                    PENDING_SELECT[..n].copy_from_slice(&name[..n]);
                    PENDING_SELECT_LEN = n;
                }
                browse();
            }
        }
        12 => {
            if CUR_DIR_LEN > 0 {
                let d = cur_dir();
                let cut = d.iter().rposition(|&c| c == b'/').unwrap_or(0);
                let mut parent = [0u8; 1024];
                parent[..cut].copy_from_slice(&d[..cut]);
                // Au-dessus d'un dossier protégé : retour à la liste des dossiers.
                let protected = lines_with(STATUS.get(), b"folder").any(|l| part(l, 1) == d);
                if protected || cut == 0 {
                    set_dir(b"/");
                } else {
                    set_dir(&parent[..cut]);
                }
                browse();
            }
        }
        13 => restore(b"replace"),
        14 => restore(b"keep-both"),
        15 => {
            SHOW_VERSIONS = !SHOW_VERSIONS;
            if SHOW_VERSIONS && SELECTED >= 0 {
                let mut path = [0u8; 1100];
                let n = entry_path(SELECTED as usize, &mut path);
                request(&[b"versions", &path[..n]]);
            }
        }
        16 => crate::wm::open(crate::wm::APP_RESTORE),
        100..=131 => {
            if let Some(line) = lines_with(STATUS.get(), b"folder").nth((id - 100) as usize) {
                let mut path = [0u8; 600];
                let n = part(line, 1).len().min(600);
                path[..n].copy_from_slice(&part(line, 1)[..n]);
                request(&[b"remove", &path[..n]]);
            }
        }
        200..=205 => {
            let names: [&[u8]; 6] = [b"Documents", b"Images", b"Bureau", b"Musique", b"Vid\xE9os", b"T\xE9l\xE9chargements"];
            home_folder(names[(id - 200) as usize]);
        }
        300..=331 => {
            if let Some(line) = lines_with(DRIVES.get(), b"drive").nth((id - 300) as usize) {
                let mut path = [0u8; 600];
                let n = part(line, 0).len().min(600);
                path[..n].copy_from_slice(&part(line, 0)[..n]);
                if request(&[b"set", b"DESTINATION", &path[..n]]) {
                    notice(b"Destination enregistr\xE9e. Activez l'historique dans Vue d'ensemble.");
                }
            }
        }
        400..=407 => {
            request(&[b"set", b"FREQUENCY", FREQUENCIES[(id - 400) as usize].0]);
        }
        500..=506 => {
            request(&[b"set", b"KEEP_DAYS", RETENTIONS[(id - 500) as usize].0]);
        }
        1000..=1004 => {
            FOCUS = id - 1000;
        }
        2000..=2999 => {
            let i = id - 2000;
            let e = entry(i as usize);
            if double && part(e, 0) == b"d" {
                let mut path = [0u8; 1100];
                let n = entry_path(i as usize, &mut path);
                set_dir(&path[..n]);
                browse();
            } else {
                SELECTED = i;
                if SHOW_VERSIONS {
                    if part(e, 0) == b"f" {
                        let mut path = [0u8; 1100];
                        let n = entry_path(i as usize, &mut path);
                        request(&[b"versions", &path[..n]]);
                    } else {
                        SHOW_VERSIONS = false;
                    }
                }
            }
        }
        3000..=3999 => {
            // Aller à l'instantané qui contient cette version.
            if let Some(v) = lines_with(VERSIONS.get(), b"version").nth((id - 3000) as usize) {
                let target = part(v, 0);
                if let Some(i) = (0..snapshot_count()).find(|&i| part(snapshot(i), 0) == target) {
                    if SELECTED >= 0 {
                        let name = part(entry(SELECTED as usize), 1);
                        let n = name.len().min(PENDING_SELECT.len());
                        PENDING_SELECT[..n].copy_from_slice(&name[..n]);
                        PENDING_SELECT_LEN = n;
                    }
                    SNAP_INDEX = i;
                    browse();
                }
            }
        }
        _ => return false,
    }
    true
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, _f: &Frame) -> bool {
    match ev {
        Mouse::Move => {
            let h = hit_at(x, y);
            let changed = h != HOVER;
            HOVER = h;
            changed
        }
        Mouse::Wheel(delta) => {
            if SECTION == 4 {
                SCROLL += if delta < 0 { 3 } else { -3 };
                SCROLL = SCROLL.max(0);
                return true;
            }
            false
        }
        Mouse::Down | Mouse::DoubleClick => {
            let id = hit_at(x, y);
            if !(1000..=1004).contains(&id) {
                FOCUS = -1;
            }
            activate(id, ev == Mouse::DoubleClick);
            true
        }
        _ => false,
    }
}

pub unsafe fn key(vk: u32, ch: u16) -> bool {
    if FOCUS >= 0 {
        let i = FOCUS as usize;
        if vk == 0x0D || ch == 13 {
            submit(i);
            return true;
        }
        if vk == 0x1B {
            FOCUS = -1;
            return true;
        }
        if vk == 0x09 || ch == 9 {
            // Tab : champ suivant de la même section.
            FOCUS = match i {
                F_SHARE => F_ACCOUNT as i32,
                F_ACCOUNT => F_PASSWORD as i32,
                F_PASSWORD => F_SHARE as i32,
                _ => FOCUS,
            };
            return true;
        }
        if ch == 8 || vk == 0x08 {
            FIELD_LEN[i] = FIELD_LEN[i].saturating_sub(1);
            return true;
        }
        if ch >= 32 && ch < 256 && ch != 127 && FIELD_LEN[i] < FIELDS[i].len() - 1 {
            FIELDS[i][FIELD_LEN[i]] = ch as u8;
            FIELD_LEN[i] += 1;
            return true;
        }
        return ch != 0;
    }
    if SECTION == 4 {
        let total = entry_count() as i32;
        match vk {
            0x26 if total > 0 => {
                SELECTED = (SELECTED - 1).max(0);
                return true;
            }
            0x28 if total > 0 => {
                SELECTED = (SELECTED + 1).min(total - 1);
                return true;
            }
            0x0D if SELECTED >= 0 => return activate(2000 + SELECTED, true),
            0x08 => return activate(12, false),
            0x25 => return activate(10, false),
            0x27 => return activate(11, false),
            _ => {}
        }
    }
    false
}

/// Ouvre l'historique directement sur la restauration d'un dossier.
pub unsafe fn open_restore(dir: &[u8]) {
    SECTION = 4;
    set_dir(dir);
    SNAP_INDEX = usize::MAX;
    open();
    spawn(b"list");
    crate::wm::open(crate::wm::APP_FILEHISTORY);
}

/// Vérifications sans affichage pour explorer_regression.
pub unsafe fn regression_checks() {
    let mut ansi = [0u8; 32];
    let n = to_ansi("Vidéos €".as_bytes(), &mut ansi);
    let mut utf = [0u8; 32];
    let m = to_utf8(&ansi[..n], &mut utf);
    check!(
        &ansi[..n] == b"Vid\xE9os \x80" && &utf[..m] == "Vidéos €".as_bytes(),
        "Historique : les chemins accentues passent de l'UTF-8 a Windows-1252 et retour"
    );
    let mut size = [0u8; 24];
    let k = fmt_size(1536, &mut size);
    check!(&size[..k] == b"1,5 Ko", "Historique : taille lisible des versions");
    // Navigation : les entrées ne valent que pour l'instantané affiché.
    let saved = (SNAPS, BROWSE, SNAP_INDEX, CUR_DIR_LEN);
    SNAPS = EMPTY;
    let snaps = b"20260101-100000|01/01/2026 10:00|3\n20260102-100000|02/01/2026 10:00|4\n";
    SNAPS.bytes[..snaps.len()].copy_from_slice(snaps);
    SNAPS.len = snaps.len();
    BROWSE = EMPTY;
    let listing = b"snapshot=20260102-100000\ndir=/home/a/Documents\nentry=d|Projet|0|1\nentry=f|r.txt|3|1\n";
    BROWSE.bytes[..listing.len()].copy_from_slice(listing);
    BROWSE.len = listing.len();
    SNAP_INDEX = 1;
    set_dir(b"/home/a/Documents");
    let mut path = [0u8; 1100];
    let n = entry_path(1, &mut path);
    check!(
        entry_count() == 2 && &path[..n] == b"/home/a/Documents/r.txt",
        "Historique : chemin complet d'un fichier d'instantane"
    );
    SNAP_INDEX = 0;
    check!(entry_count() == 0, "Historique : pas de liste perimee apres changement d'instantane");
    (SNAPS, BROWSE, SNAP_INDEX, CUR_DIR_LEN) = saved;
}
