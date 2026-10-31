//! Paramètres Linux/Windows : navigation Mica et commandes système réelles.
//! Les actions passent par winlux-control et les états viennent du système.
#![deny(unreachable_patterns)]

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;
const PAGES: [&[u8]; 14] = [
    b"Syst\xE8me Linux",
    b"Applications Windows",
    b"R\xE9seau",
    b"Son et batterie",
    b"Clavier et pav\xE9 tactile",
    b"Souris",
    b"Apparence",
    b"Mises \xE0 jour",
    b"\xC9cran et d\xE9finition",
    b"\xC0 propos",
    b"Ex\xE9cutable",
    b"Navigateurs",
    b"Accessibilit\xE9",
    b"Effets visuels",
];
const CAPACITY: usize = 32768;
#[derive(Clone, Copy)]
struct Buffer {
    bytes: [u8; CAPACITY],
    len: usize,
}
const EMPTY: Buffer = Buffer {
    bytes: [0; CAPACITY],
    len: 0,
};
impl Buffer {
    fn get(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}
static mut STATUS: Buffer = EMPTY;
static mut ACTION: Buffer = EMPTY;
static mut LOG: Buffer = EMPTY;
static mut UPDATES: Buffer = EMPTY;
static mut PROGRESS: Buffer = EMPTY;
static mut BUILD_INFO: Buffer = EMPTY;
static mut SELECTED_MODE: usize = 0;
static mut LAST_ANIMATION: u32 = 0;
static mut PAGE: usize = 0;
static mut SCROLL: usize = 0;
static mut LAST_POLL: u32 = 0;
static mut HOVER: i32 = -1;
static mut SHOW_LOG: bool = false;
static mut EXE_PATH: [u16; 2048] = [0; 2048];
static mut EXE_LEN: usize = 0;
static mut EXE_SCALE: usize = 0;
static mut EXE_TRACE: bool = false;
static mut EXE_NOTICE: &[u8] = b"Les r\xE9glages sont facultatifs et propres \xE0 ce programme.";
const EXE_SCALES: [&[u8]; 5] = [b"auto", b"1", b"1.25", b"1.5", b"2"];
static mut MOUSE_SPEED: u32 = 50; // 0-100, 50 par défaut
static mut POINTER_SIZE: u32 = 1; // 1=normal, 2=large, 3=extra large
static mut HIGH_CONTRAST: bool = false; // Mode contraste élevé
use crate::gfx::{AUTO_THEME, ENABLE_BLUR, ENABLE_SMOOTH_ANIM, ENABLE_THUMBNAILS};

fn exe_profile_path(out: &mut [u16; 2060]) {
    unsafe {
        out[..EXE_LEN].copy_from_slice(&EXE_PATH[..EXE_LEN]);
        for (i, &byte) in b".winlux.ini".iter().enumerate() {
            out[EXE_LEN + i] = byte as u16;
        }
    }
}
pub unsafe fn open_exe_settings(path: &[u16]) {
    let len = path.iter().position(|&v| v == 0).unwrap_or(path.len());
    if len == 0 || len >= EXE_PATH.len() {
        return;
    }
    EXE_PATH.fill(0);
    EXE_PATH[..len].copy_from_slice(&path[..len]);
    EXE_LEN = len;
    EXE_SCALE = 0;
    EXE_TRACE = false;
    let mut profile = [0u16; 2060];
    exe_profile_path(&mut profile);
    let h = CreateFileW(
        profile.as_ptr(),
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if !h.is_null() && h as isize != -1 {
        let mut data = [0u8; 128];
        let mut read = 0;
        ReadFile(h, data.as_mut_ptr(), 128, &mut read, core::ptr::null_mut());
        CloseHandle(h);
        let data = &data[..(read as usize).min(128)];
        EXE_SCALE = EXE_SCALES
            .iter()
            .position(|&v| v == field(data, b"ui_scale"))
            .unwrap_or(0);
        EXE_TRACE = field(data, b"trace") == b"1";
    }
    EXE_NOTICE = b"Un double-clic lance le programme, m\xEAme sans r\xE9glages enregistr\xE9s.";
    PAGE = 10;
    HOVER = -1;
    SCROLL = 0;
    crate::wm::open(crate::wm::APP_SETTINGS);
}
unsafe fn save_exe_settings(reset: bool) {
    let mut path = [0u16; 2060];
    exe_profile_path(&mut path);
    if reset {
        let attributes = GetFileAttributesW(path.as_ptr());
        if attributes != u32::MAX && DeleteFileW(path.as_ptr()) == 0 {
            EXE_NOTICE = b"Impossible de supprimer les r\xE9glages : v\xE9rifiez les permissions.";
            return;
        }
        EXE_SCALE = 0;
        EXE_TRACE = false;
        EXE_NOTICE = b"Valeurs par d\xE9faut restaur\xE9es.";
        return;
    }
    let h = CreateFileW(
        path.as_ptr(),
        0x4000_0000,
        0,
        core::ptr::null_mut(),
        2,
        0,
        core::ptr::null_mut(),
    );
    if h.is_null() || h as isize == -1 {
        EXE_NOTICE = b"Enregistrement impossible : le dossier du programme doit \xEAtre accessible en \xE9criture.";
        return;
    }
    let mut data = [0u8; 48];
    let scale = EXE_SCALES[EXE_SCALE];
    data[..9].copy_from_slice(b"ui_scale=");
    data[9..9 + scale.len()].copy_from_slice(scale);
    let n = 9 + scale.len();
    data[n..n + 7].copy_from_slice(b"\ntrace=");
    data[n + 7] = if EXE_TRACE { b'1' } else { b'0' };
    data[n + 8] = b'\n';
    let mut written = 0;
    let ok = WriteFile(
        h,
        data.as_ptr(),
        (n + 9) as u32,
        &mut written,
        core::ptr::null_mut(),
    );
    CloseHandle(h);
    EXE_NOTICE = if ok != 0 && written == (n + 9) as u32 {
        b"R\xE9glages enregistr\xE9s pour le prochain lancement."
    } else {
        b"Enregistrement incomplet : r\xE9essayez."
    };
}

unsafe fn read(path: &core::ffi::CStr, target: &mut Buffer) -> bool {
    let file = CreateFileA(
        path.as_ptr(),
        0x8000_0000,
        7,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    let mut raw = [0u8; CAPACITY];
    let mut count = 0;
    if file as isize != -1 && !file.is_null() {
        let size = GetFileSize(file, core::ptr::null_mut());
        // Garder le début des notes (version/résumé), la fin des journaux.
        if path.to_bytes().ends_with(b".log") && size != u32::MAX && size > (CAPACITY - 1) as u32 {
            SetFilePointer(
                file,
                (size - (CAPACITY - 1) as u32) as i32,
                core::ptr::null_mut(),
                0,
            );
        }
        ReadFile(
            file,
            raw.as_mut_ptr(),
            (CAPACITY - 1) as u32,
            &mut count,
            core::ptr::null_mut(),
        );
        CloseHandle(file);
    }
    let mut next = EMPTY;
    // Les journaux Linux sont UTF-8, TextOutA attend Windows-1252.
    // Une fin de journal peut commencer au milieu d'un caractere UTF-8.
    let mut begin = 0;
    while begin < count as usize && raw[begin] & 0xc0 == 0x80 {
        begin += 1;
    }
    let source = core::str::from_utf8(&raw[begin..count as usize]).unwrap_or("");
    for ch in source.chars().take(CAPACITY - 1) {
        if ch.is_control() && ch != '\n' && ch != '\t' {
            continue;
        }
        next.bytes[next.len] = match ch as u32 {
            0..=255 => ch as u8,
            0x2019 => b'\'',
            0x2013 | 0x2014 => b'-',
            0x20ac => 0x80,
            0x153 => 0x9c,
            _ => b'?',
        };
        if ch == '\t' {
            next.bytes[next.len] = b' ';
        }
        next.len += 1;
    }
    let changed = next.len != target.len || next.get() != target.get();
    *target = next;
    changed
}
unsafe fn value(key: &[u8]) -> &'static [u8] {
    field(STATUS.get(), key)
}
fn field<'a>(data: &'a [u8], key: &[u8]) -> &'a [u8] {
    for line in data.split(|&c| c == b'\n') {
        if line.len() > key.len() && &line[..key.len()] == key && line[key.len()] == b'=' {
            return &line[key.len() + 1..];
        }
    }
    b""
}
unsafe fn available() -> bool {
    value(b"control_available") == b"1"
}
unsafe fn updating() -> bool {
    // The Linux helper checks the owner PID; a stale progress file must never
    // lock the interface forever after a killed updater.
    value(b"update_busy") == b"1"
}
unsafe fn already_up_to_date() -> bool {
    UPDATES
        .get()
        .split(|&c| c == b'\n')
        .any(|line| line == b"Winlux est \xE0 jour.")
}
unsafe fn restart_required() -> bool {
    matches!(value(b"update_action"), b"reboot" | b"restart-session")
}
fn restart_rect(c: &Rect) -> Rect {
    rect(c.left, c.top + 108, c.right, c.top + 156)
}
fn number(s: &[u8]) -> u32 {
    s.iter()
        .take_while(|c| c.is_ascii_digit())
        .fold(0u32, |n, &c| {
            n.saturating_mul(10).saturating_add((c - b'0') as u32)
        })
}

/// Telemetrie reelle pour la barre, meme quand Parametres est ferme.
pub unsafe fn system_tick() -> bool {
    let now = GetTickCount();
    if LAST_POLL != 0 && now.wrapping_sub(LAST_POLL) < 250 {
        return false;
    }
    LAST_POLL = now;
    let mut changed = read(c"Z:\\run\\winlux\\control.txt", &mut STATUS);
    if GetFileAttributesA(c"Z:\\run\\winlux\\browser-chooser".as_ptr()) != u32::MAX {
        DeleteFileA(c"Z:\\run\\winlux\\browser-chooser".as_ptr());
        open_page(11);
        changed = true;
    }
    changed |= read(c"Z:\\run\\winlux\\action.txt", &mut ACTION);
    changed |= read(
        if PAGE == 7 {
            c"Z:\\run\\winlux\\update.log"
        } else {
            c"Z:\\run\\winlux\\action.log"
        },
        &mut LOG,
    );
    changed |= read(c"Z:\\run\\winlux\\updates.txt", &mut UPDATES);
    changed |= read(c"Z:\\run\\winlux\\update-progress", &mut PROGRESS);
    if PAGE == 9 {
        changed |= read(c"Z:\\usr\\share\\winlux\\BUILD-INFO", &mut BUILD_INFO);
    }
    if available() {
        crate::desktop::NETWORK_CONNECTED = number(value(b"network_connected"));
        crate::desktop::BATTERY_PRESENT = number(value(b"battery_present"));
        crate::desktop::BATTERY_LEVEL = number(value(b"battery_level")).min(100);
        let audio = value(b"audio");
        if audio.starts_with(b"Volume: ") {
            let muted = audio.windows(5).any(|w| w == b"MUTED");
            let v = &audio[8..];
            let n = if v.starts_with(b"1") {
                100
            } else if v.len() >= 4 {
                number(&v[2..4])
            } else {
                0
            };
            crate::desktop::VOLUME_LEVEL = if muted { 0 } else { n };
        } else if audio.first().is_some_and(u8::is_ascii_digit) {
            crate::desktop::VOLUME_LEVEL = if audio.windows(3).any(|w| w == b"off") {
                0
            } else {
                number(audio).min(100)
            };
        } else {
            crate::desktop::VOLUME_LEVEL = 0;
        }
        match value(b"align") {
            b"left" => crate::desktop::ALIGN_LEFT = true,
            b"center" => crate::desktop::ALIGN_LEFT = false,
            _ => {}
        }
        match value(b"theme") {
            b"light" => crate::gfx::set_theme(false),
            b"dark" => crate::gfx::set_theme(true),
            _ => {}
        }
        crate::gfx::check_auto_theme();
    }
    changed
}
pub unsafe fn control(action: &core::ffi::CStr) -> bool {
    if updating()
        && matches!(
            action.to_bytes(),
            b"poweroff" | b"reboot" | b"restart-session" | b"update-apply" | b"update-rollback"
        )
    {
        crate::desktop::toast(b"Installation en cours : attendez la fin avant de red\xE9marrer.");
        return false;
    }
    if ShellExecuteA(
        core::ptr::null_mut(),
        c"open".as_ptr(),
        c"winlux-control".as_ptr(),
        action.as_ptr(),
        core::ptr::null(),
        0,
    ) as isize
        > 32
    {
        ACTION = EMPTY;
        let s = b"Operation en cours...";
        ACTION.bytes[..s.len()].copy_from_slice(s);
        ACTION.len = s.len();
        LAST_POLL = 0;
        true
    } else {
        crate::desktop::toast(b"Le service de contr\xF4le Winlux n'est pas disponible.");
        false
    }
}
pub unsafe fn open_page(page: usize) {
    if crate::wm::native_settings(page) {
        return;
    }
    PAGE = page.min(PAGES.len() - 1);
    SCROLL = 0;
    LAST_POLL = 0;
    crate::wm::open(crate::wm::APP_SETTINGS);
    system_tick();
}
fn content(f: &Frame) -> Rect {
    let nav = if f.client.right - f.client.left < 700 {
        156
    } else {
        210
    };
    rect(
        f.client.left + nav + 26,
        f.client.top + 42,
        f.client.right - 26,
        f.client.bottom - 20,
    )
}
fn nav_rect(f: &Frame, i: usize) -> Rect {
    let c = content(f);
    let step = ((f.client.bottom - f.client.top - 75) / PAGES.len() as i32).clamp(26, 42);
    rect(
        f.client.left + 10,
        f.client.top + 64 + i as i32 * step,
        c.left - 30,
        f.client.top + 64 + i as i32 * step + step - 4,
    )
}
fn button_rect(c: &Rect, row: i32, col: i32) -> Rect {
    let w = ((c.right - c.left - 16) / 3).max(70);
    rect(
        c.left + col * (w + 8),
        c.top + 108 + row * 44,
        c.left + col * (w + 8) + w,
        c.top + 144 + row * 44,
    )
}
unsafe fn button(g: &mut Gfx, c: &Rect, row: i32, col: i32, label: &[u8], id: i32) {
    let p = pal();
    let r = button_rect(c, row, col);
    if r.bottom > c.bottom - 24 {
        return;
    }
    let disabled = updating() && (PAGE == 0 || PAGE == 7 && id != 4);
    let selected = match (PAGE, id) {
        (4, 0) => value(b"layout") == b"fr",
        (4, 1) => value(b"layout") == b"us",
        (4, 3) => value(b"tap") == b"1",
        (4, 4) => value(b"tap") == b"0",
        (5, 0) => unsafe { MOUSE_SPEED == 25 },
        (5, 1) => unsafe { MOUSE_SPEED == 50 },
        (5, 2) => unsafe { MOUSE_SPEED == 75 },
        (6, 0) => value(b"align") == b"left",
        (6, 1) => value(b"align") == b"center",
        (6, 3) => value(b"theme") == b"dark",
        (6, 4) => value(b"theme") == b"light",
        (8, 3) => value(b"display_policy") == b"fixed",
        (8, 4) => value(b"display_policy") != b"fixed",
        (10, 0..=4) => EXE_SCALE == id as usize,
        (10, 5) => EXE_TRACE,
        (11, 1) => value(b"browser_default") == b"edge",
        (11, 4) => value(b"browser_default") == b"chrome",
        (11, 7) => value(b"browser_default") == b"firefox",
        _ => false,
    };
    let primary = PAGE == 7 && id == 1 || PAGE == 8 && id == 2;
    round_rect(
        g.hdc,
        g.b,
        r.left,
        r.top,
        r.right,
        r.bottom,
        7,
        if selected { p.accent } else { p.border },
    );
    round_rect(
        g.hdc,
        g.b,
        r.left + 1,
        r.top + 1,
        r.right - 1,
        r.bottom - 1,
        6,
        if disabled {
            p.surface
        } else if primary {
            p.accent
        } else if selected {
            p.selected
        } else if HOVER == id {
            p.hover
        } else {
            p.surface2
        },
    );
    let mut end = label.len();
    while end > 0 && text_width(&label[..end], 11) > r.right - r.left - 12 {
        end -= 1;
    }
    text_centered(
        g.hdc,
        g.f.small,
        11,
        if disabled {
            p.text3
        } else if primary {
            rgb(18, 24, 34)
        } else {
            p.text
        },
        (r.left + r.right) / 2,
        r.top + 12,
        &label[..end],
    );
}
unsafe fn row(g: &mut Gfx, c: &Rect, y: i32, label: &[u8], val: &[u8]) {
    if c.top + y + 42 >= c.bottom - 30 {
        return;
    }
    let p = pal();
    round_rect(
        g.hdc,
        g.b,
        c.left - 8,
        c.top + y - 7,
        c.right + 8,
        c.top + y + 45,
        8,
        p.border,
    );
    round_rect(
        g.hdc,
        g.b,
        c.left - 7,
        c.top + y - 6,
        c.right + 7,
        c.top + y + 44,
        7,
        p.surface,
    );
    let mut label_end = label.len();
    while label_end > 0 && text_width(&label[..label_end], 13) > c.right - c.left {
        label_end -= 1;
    }
    text(
        g.hdc,
        g.f.semibold,
        p.text,
        c.left,
        c.top + y,
        &label[..label_end],
    );
    let mut end = val.len();
    while end > 0 && text_width(&val[..end], 13) > c.right - c.left {
        end -= 1;
    }
    text(
        g.hdc,
        g.f.body,
        p.text2,
        c.left,
        c.top + y + 22,
        if val.is_empty() {
            b"Information indisponible"
        } else {
            &val[..end]
        },
    );
}
unsafe fn journal(g: &mut Gfx, c: &Rect, start: i32, data: &[u8]) {
    let p = pal();
    let bottom = c.bottom - 34;
    if c.top + start >= bottom {
        return;
    }
    fill(g.hdc, g.b, c.left, c.top + start, c.right, bottom, p.mica);
    let rows = ((bottom - c.top - start - 12) / 18).max(0) as usize;
    let width = (c.right - c.left - 20).max(20);
    let mut total: usize = 0;
    let detailed = PAGE == 7 && !SHOW_LOG;
    for line in data
        .split(|&v| v == b'\n')
        .filter_map(|line| journal_line(line, detailed))
    {
        let mut rest = line;
        loop {
            total += 1;
            let n = wrap_len(rest, width);
            if n == rest.len() {
                break;
            }
            rest = &rest[n..];
        }
    }
    SCROLL = SCROLL.min(total.saturating_sub(rows));
    let mut index = 0;
    for line in data
        .split(|&v| v == b'\n')
        .filter_map(|line| journal_line(line, detailed))
    {
        let mut rest = line;
        loop {
            let n = wrap_len(rest, width);
            if index >= SCROLL && index < SCROLL + rows {
                text(
                    g.hdc,
                    g.f.small,
                    p.text2,
                    c.left + 10,
                    c.top + start + 8 + (index - SCROLL) as i32 * 18,
                    &rest[..n],
                );
            }
            index += 1;
            if n == rest.len() {
                break;
            }
            rest = &rest[n..];
        }
    }
}

fn journal_line(mut line: &[u8], detailed: bool) -> Option<&[u8]> {
    if detailed {
        // Le titre/version est déjà affiché ; réserver la place aux détails
        // plutôt qu'aux lignes vides et aux marqueurs de titres Markdown.
        if line.is_empty() || line.starts_with(b"# ") {
            return None;
        }
        if line.starts_with(b"##") {
            while line.first() == Some(&b'#') {
                line = &line[1..];
            }
            if line.first() == Some(&b' ') {
                line = &line[1..];
            }
        }
    }
    Some(line)
}

// Retour à la ligne par mots, sans perdre les lignes longues ni les accents.
fn wrap_len(line: &[u8], width: i32) -> usize {
    let mut end = 0;
    while end < line.len() && text_width(&line[..end + 1], 11) <= width {
        end += 1;
    }
    if end == line.len() {
        return end;
    }
    let word = line[..end].iter().rposition(|&v| v == b' ');
    word.map(|n| n + 1).filter(|&n| n > 0).unwrap_or(end.max(1))
}

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    MOUSE_SPEED = match value(b"mouse_speed") {
        b"25" => 25,
        b"75" => 75,
        _ => 50,
    };
    let p = pal();
    let c = content(f);
    // Conserver le panneau Mica et les accents de la nouvelle interface,
    // sans ses bascules simulées ni ses numéros de version Windows fictifs.
    fill(
        g.hdc,
        g.b,
        f.client.left,
        f.client.top,
        f.client.right,
        f.client.bottom,
        p.mica,
    );
    fill(
        g.hdc,
        g.b,
        f.client.left,
        f.client.top,
        c.left - 20,
        f.client.bottom,
        p.surface,
    );
    fill(
        g.hdc,
        g.b,
        c.left - 20,
        f.client.top,
        c.left - 19,
        f.client.bottom,
        p.border,
    );
    text(
        g.hdc,
        g.f.large,
        p.text,
        f.client.left + 20,
        f.client.top + 20,
        b"Param\xE8tres",
    );
    for (i, name) in PAGES.iter().enumerate() {
        let r = nav_rect(f, i);
        if i == PAGE {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, p.selected);
            fill(
                g.hdc,
                g.b,
                r.left,
                r.top + 8,
                r.left + 3,
                r.bottom - 8,
                p.accent,
            );
        }
        let mut end = name.len();
        while end > 0 && text_width(&name[..end], 13) > r.right - r.left - 16 {
            end -= 1;
        }
        text(g.hdc, g.f.body, p.text, r.left + 8, r.top + 9, &name[..end]);
    }
    text(
        g.hdc,
        g.f.small,
        p.text3,
        c.left,
        c.top - 22,
        b"WINLUX  /  PR\xC9F\xC9RENCES DE L'ORDINATEUR",
    );
    text(g.hdc, g.f.large, p.text, c.left, c.top, PAGES[PAGE]);
    fill(
        g.hdc,
        g.b,
        c.left,
        c.top + 30,
        c.left + 44,
        c.top + 33,
        p.accent,
    );
    match PAGE {
        5 => {
            let speed_str = unsafe {
                if MOUSE_SPEED == 25 {
                    "Lente"
                } else if MOUSE_SPEED == 50 {
                    "Normale"
                } else {
                    "Rapide"
                }
            };
            row(g, &c, 44, b"Rapidit\xE9 de la souris", speed_str.as_bytes());
            button(g, &c, 0, 0, b"Lente", 0);
            button(g, &c, 0, 1, b"Normale", 1);
            button(g, &c, 0, 2, b"Rapide", 2);
        }
        12 => {
            row(
                g,
                &c,
                44,
                b"Accessibilit\xE9",
                b"Options d'accessibilit\xE9.",
            );
            let pointer_str = unsafe {
                if POINTER_SIZE == 1 {
                    "Normal"
                } else if POINTER_SIZE == 2 {
                    "Grand"
                } else {
                    "Tres grand"
                }
            };
            row(g, &c, 84, b"Taille du pointeur", pointer_str.as_bytes());
            button(g, &c, 0, 0, b"Normal", 0);
            button(g, &c, 0, 1, b"Grand", 1);
            button(g, &c, 0, 2, b"Tr\xE8s grand", 2);
            row(
                g,
                &c,
                124,
                b"Contraste \xE9lev\xE9",
                if unsafe { HIGH_CONTRAST } {
                    b"Activ\xE9"
                } else {
                    b"D\xE9sactiv\xE9"
                },
            );
            button(g, &c, 1, 0, b"Activer", 3);
            button(g, &c, 1, 1, b"D\xE9sactiver", 4);
        }
        13 => {
            row(g, &c, 44, b"Effets visuels", b"Options d'effet visuels.");
            row(
                g,
                &c,
                84,
                b"Effet de flou",
                if unsafe { ENABLE_BLUR } {
                    b"Activ\xE9"
                } else {
                    b"D\xE9sactiv\xE9"
                },
            );
            button(g, &c, 0, 0, b"Activer", 0);
            button(g, &c, 0, 1, b"D\xE9sactiver", 1);
            row(
                g,
                &c,
                124,
                b"Animations fluides",
                if unsafe { ENABLE_SMOOTH_ANIM } {
                    b"Activ\xE9es"
                } else {
                    b"D\xE9sactiv\xE9es"
                },
            );
            button(g, &c, 1, 0, b"Activer", 3);
            button(g, &c, 1, 1, b"D\xE9sactiver", 4);
            row(
                g,
                &c,
                164,
                b"Miniatures des fen\xEAtres",
                if unsafe { ENABLE_THUMBNAILS } {
                    b"Activ\xE9es"
                } else {
                    b"D\xE9sactiv\xE9es"
                },
            );
            button(g, &c, 2, 0, b"Activer", 6);
            button(g, &c, 2, 1, b"D\xE9sactiver", 7);
            row(
                g,
                &c,
                204,
                b"Th\xE8me automatique",
                if unsafe { AUTO_THEME } {
                    b"Activ\xE9"
                } else {
                    b"D\xE9sactiv\xE9"
                },
            );
            button(g, &c, 3, 0, b"Activer", 9);
            button(g, &c, 3, 1, b"D\xE9sactiver", 10);
        }
        11 => {
            row(
                g,
                &c,
                44,
                b"Navigateur par d\xE9faut",
                match value(b"browser_default") {
                    b"chrome" => b"Google Chrome",
                    b"firefox" => b"Mozilla Firefox",
                    _ => b"Microsoft Edge",
                },
            );
            for (i, labels) in [
                [
                    b"Installer Edge".as_slice(),
                    b"Choisir Edge",
                    b"Ouvrir Edge",
                ],
                [
                    b"Installer Chrome".as_slice(),
                    b"Choisir Chrome",
                    b"Ouvrir Chrome",
                ],
                [
                    b"Installer Firefox".as_slice(),
                    b"Choisir Firefox",
                    b"Ouvrir Firefox",
                ],
            ]
            .iter()
            .enumerate()
            {
                for (j, label) in labels.iter().enumerate() {
                    button(g, &c, i as i32, j as i32, label, (i * 3 + j) as i32);
                }
            }
            for (i, key) in [
                b"browser_edge".as_slice(),
                b"browser_chrome",
                b"browser_firefox",
            ]
            .iter()
            .enumerate()
            {
                row(
                    g,
                    &c,
                    272 + i as i32 * 65,
                    [
                        b"Microsoft Edge".as_slice(),
                        b"Google Chrome",
                        b"Mozilla Firefox",
                    ][i],
                    match value(key) {
                        b"installed" => b"Install\xE9",
                        b"available" => b"Paquet pr\xEAt - installation hors ligne",
                        b"update" => b"Mise \xE0 jour disponible",
                        _ => b"Paquet absent de cette image",
                    },
                );
            }
        }
        10 => {
            if EXE_LEN == 0 {
                row(
                    g,
                    &c,
                    44,
                    b"Choisir un programme",
                    b"Clic droit sur un .exe dans Explorer, puis R\xE9glages de l'ex\xE9cutable.",
                );
                return;
            }
            let mut label = [b'?'; 160];
            let n = EXE_LEN.min(label.len());
            for i in 0..n {
                label[i] = if EXE_PATH[i] <= 255 {
                    EXE_PATH[i] as u8
                } else {
                    b'?'
                };
            }
            text_fit(
                g.hdc,
                g.f.body,
                13,
                p.text2,
                c.left,
                c.top + 48,
                &label[..n],
                c.right - c.left,
            );
            for (i, label) in [
                b"\xC9chelle auto".as_slice(),
                b"100 %",
                b"125 %",
                b"150 %",
                b"200 %",
                b"Journal diagnostic",
                b"Enregistrer",
                b"Valeurs par d\xE9faut",
                b"Lancer le .exe",
            ]
            .iter()
            .enumerate()
            {
                button(g, &c, i as i32 / 3, i as i32 % 3, label, i as i32);
            }
            row(
                g,
                &c,
                280,
                b"Application des r\xE9glages",
                b"Enregistrer puis relancer le programme. Aucun profil n'est obligatoire.",
            );
            row(g, &c, 350, b"P\xE9rim\xE8tre", b"\xC9chelle du rendu Winlux et traces ; les API manquantes restent \xE0 impl\xE9menter.");
        }
        9 => {
            let info = BUILD_INFO.get();
            let version = field(info, b"version");
            row(
                g,
                &c,
                44,
                b"Version de Winlux",
                if version.is_empty() {
                    value(b"version")
                } else {
                    version
                },
            );
            row(g, &c, 104, b"Commit GitHub", field(info, b"commit"));
            row(
                g,
                &c,
                164,
                b"Construction (UTC)",
                field(info, b"built_at_utc"),
            );
            row(
                g,
                &c,
                224,
                b"Type de livrable",
                match field(info, b"scope") {
                    b"system" => b"Syst\xE8me complet",
                    b"runtime" => b"Runtime uniquement",
                    _ => b"Information indisponible",
                },
            );
            row(
                g,
                &c,
                284,
                b"Sources suivies lors de la construction",
                match field(info, b"tracked_sources") {
                    b"clean" => b"Sans modification locale suivie",
                    b"modified" => b"Modifications locales non valid\xE9es",
                    _ => b"Information indisponible",
                },
            );
            row(
                g,
                &c,
                344,
                b"Provenance",
                if info.is_empty() {
                    b"BUILD-INFO absent : ancienne image ou installation sans m\xE9tadonn\xE9es."
                } else {
                    b"Informations du livrable install\xE9, pas de la derni\xE8re release GitHub."
                },
            );
        }
        0 => {
            row(g, &c, 44, b"Version de Winlux", value(b"version"));
            button(g, &c, 0, 0, b"Relancer le bureau", 0);
            button(g, &c, 0, 1, b"Red\xE9marrer", 1);
            button(g, &c, 0, 2, b"Arr\xEAter", 2);
            row(g, &c, 170, b"Noyau Linux", value(b"kernel"));
            row(
                g,
                &c,
                230,
                b"P\xE9riph\xE9riques d'entr\xE9e d\xE9tect\xE9s",
                value(b"input_devices"),
            );
            row(
                g,
                &c,
                290,
                b"Stockage du syst\xE8me",
                if value(b"persistent") == b"1" {
                    b"Disque persistant : r\xE9glages et mises \xE0 jour conserv\xE9s."
                } else {
                    b"Session Live en RAM : modifications perdues au red\xE9marrage."
                },
            );
            journal(g, &c, 356, LOG.get());
        }
        1 => {
            row(
                g,
                &c,
                44,
                b"Couche Windows / Win32",
                b"Runtime Winlux - .exe via Explorer",
            );
            button(g, &c, 0, 0, b"Programmes .exe", 0);
            button(g, &c, 0, 1, b"Dossier personnel", 1);
            button(g, &c, 0, 2, b"Mises \xE0 jour", 2);
            row(
                g,
                &c,
                170,
                b"Installer une application Windows",
                b"Ouvrir son installateur .exe dans Explorer.",
            );
            row(
                g,
                &c,
                228,
                b"Linux et Windows",
                b"Les .exe partagent la session Linux et les fichiers.",
            );
            row(
                g,
                &c,
                286,
                b"Compatibilit\xE9",
                b"Le support d\xE9pend des API et composants requis par le logiciel.",
            );
        }
        2 => {
            row(g, &c, 44, b"Connexion / adresse IPv4", value(b"network"));
            button(g, &c, 0, 0, b"Renouveler DHCP", 0);
            button(g, &c, 0, 1, b"Actualiser", 1);
            row(
                g,
                &c,
                170,
                b"Machine virtuelle",
                b"Ethernet virtuel ; le Wi-Fi reste g\xE9r\xE9 par l'h\xF4te.",
            );
            journal(g, &c, 236, LOG.get());
        }
        3 => {
            row(g, &c, 44, b"Sortie audio / volume", value(b"audio"));
            button(g, &c, 0, 0, b"Volume -", 0);
            button(g, &c, 0, 1, b"Muet / r\xE9tablir", 1);
            button(g, &c, 0, 2, b"Volume +", 2);
            row(
                g,
                &c,
                170,
                b"Batterie expos\xE9e par le syst\xE8me",
                value(b"battery"),
            );
            journal(g, &c, 236, LOG.get());
        }
        4 => {
            row(g, &c, 44, b"Disposition actuelle", value(b"layout"));
            button(g, &c, 0, 0, b"AZERTY fran\xE7ais", 0);
            button(g, &c, 0, 1, b"QWERTY US", 1);
            button(g, &c, 1, 0, b"Activer toucher-clic", 3);
            button(g, &c, 1, 1, b"D\xE9sactiver", 4);
            row(
                g,
                &c,
                214,
                b"Toucher-clic (0 = non, 1 = oui)",
                value(b"tap"),
            );
            row(
                g,
                &c,
                274,
                b"Pav\xE9 tactile en VM",
                b"SPICE expose souvent une tablette, pas le pav\xE9 physique.",
            );
            journal(g, &c, 336, LOG.get());
        }
        6 => {
            row(
                g,
                &c,
                44,
                b"Barre des t\xE2ches",
                b"Pr\xE9f\xE9rences enregistr\xE9es pour la prochaine session.",
            );
            button(g, &c, 0, 0, b"Aligner \xE0 gauche", 0);
            button(g, &c, 0, 1, b"Centrer", 1);
            button(g, &c, 1, 0, b"Th\xE8me sombre", 3);
            button(g, &c, 1, 1, b"Th\xE8me clair", 4);
            row(
                g,
                &c,
                164,
                b"Th\xE8me automatique",
                if unsafe { AUTO_THEME } {
                    b"Jour clair / nuit sombre (8h\x9620h)"
                } else {
                    b"D\xE9sactiv\xE9"
                },
            );
            button(g, &c, 2, 0, b"Activer", 5);
            button(g, &c, 2, 1, b"D\xE9sactiver", 6);
            row(
                g,
                &c,
                254,
                b"Stockage",
                if value(b"persistent") == b"1" {
                    b"Les pr\xE9f\xE9rences sont conserv\xE9es sur le disque."
                } else {
                    b"Session Live : les r\xE9glages durent jusqu'au red\xE9marrage."
                },
            );
            row(
                g,
                &c,
                300,
                b"Ic\xF4nes du bureau",
                b"Ajoutez seulement les ic\xF4nes que vous voulez afficher.",
            );
            button(g, &c, 5, 0, b"Afficher Corbeille", 12);
            button(g, &c, 5, 1, b"Afficher Explorateur", 13);
            button(g, &c, 6, 0, b"Afficher Ce PC", 14);
            button(g, &c, 6, 1, b"Afficher Documents", 15);
            button(g, &c, 7, 0, b"R\xE9glages de l'\xE9cran", 18);
        }
        8 => {
            row(
                g,
                &c,
                44,
                b"D\xE9finition actuellement appliqu\xE9e",
                value(b"display_current"),
            );
            let modes = value(b"display_modes");
            let count = modes
                .split(|&v| v == b',')
                .filter(|v| !v.is_empty())
                .count();
            if count > 0 {
                SELECTED_MODE = SELECTED_MODE.min(count - 1);
            }
            let chosen = modes
                .split(|&v| v == b',')
                .nth(SELECTED_MODE)
                .unwrap_or(b"Indisponible");
            button(g, &c, 0, 0, b"D\xE9finition pr\xE9c\xE9dente", 0);
            button(g, &c, 0, 1, b"D\xE9finition suivante", 1);
            button(g, &c, 0, 2, b"Appliquer", 2);
            button(g, &c, 1, 0, b"Image fixe", 3);
            button(g, &c, 1, 1, b"Suivre la fen\xEAtre VM", 4);
            button(g, &c, 1, 2, b"Actualiser", 5);
            row(
                g,
                &c,
                214,
                b"D\xE9finition s\xE9lectionn\xE9e (modes r\xE9els de l'\xE9cran)",
                chosen,
            );
            row(
                g,
                &c,
                274,
                b"\xC9chelle du texte et des contr\xF4les",
                match value(b"display_scale") {
                    b"1" => b"100 %",
                    b"1.25" => b"125 %",
                    b"1.5" => b"150 %",
                    b"2" => b"200 %",
                    _ => b"Automatique",
                },
            );
            button(g, &c, 5, 0, b"\xC9chelle -", 15);
            button(g, &c, 5, 1, b"Automatique", 16);
            button(g, &c, 5, 2, b"\xC9chelle +", 17);
            row(
                g,
                &c,
                420,
                b"Application de l'\xE9chelle",
                b"Relancez le bureau apr\xE8s modification de l'\xE9chelle.",
            );
        }
        7 => {
            if updating() {
                draw_installation(g, &c);
                return;
            }
            let restart = value(b"update_action");
            if restart_required() {
                row(
                    g,
                    &c,
                    44,
                    b"Installation termin\xE9e",
                    b"Un red\xE9marrage est n\xE9cessaire pour terminer la mise \xE0 jour.",
                );
                let r = restart_rect(&c);
                round_rect(
                    g.hdc,
                    g.b,
                    r.left,
                    r.top,
                    r.right,
                    r.bottom,
                    5,
                    if HOVER == 0 { p.hover } else { p.accent },
                );
                text_centered(
                    g.hdc,
                    g.f.body,
                    13,
                    rgb(18, 24, 34),
                    (r.left + r.right) / 2,
                    r.top + 15,
                    b"Red\xE9marrer pour terminer la mise \xE0 jour",
                );
                journal(g, &c, 198, LOG.get());
                return;
            }
            if restart == b"reboot" {
                row(
                    g,
                    &c,
                    44,
                    b"Installation termin\xE9e - red\xE9marrage requis",
                    b"Red\xE9marrez maintenant, ou \xE9teignez puis rallumez la machine.",
                );
            } else if field(PROGRESS.get(), b"phase") == b"error" {
                row(
                    g,
                    &c,
                    44,
                    b"L'installation n'a pas abouti",
                    b"Consultez le journal avant de r\xE9essayer.",
                );
            } else {
                row(
                    g,
                    &c,
                    44,
                    b"Mises \xE0 jour Winlux",
                    if value(b"update_auto") == b"enabled" {
                        b"Recherche automatique activ\xE9e."
                    } else {
                        b"Installation sur demande."
                    },
                );
            }
            button(g, &c, 0, 0, b"Rechercher", 0);
            if !already_up_to_date() {
                button(g, &c, 0, 1, b"Tout installer", 1);
            }
            button(g, &c, 0, 2, b"Restaurer", 2);
            button(
                g,
                &c,
                1,
                0,
                if value(b"update_auto") == b"enabled" {
                    b"D\xE9sactiver auto"
                } else {
                    b"Activer auto"
                },
                3,
            );
            button(
                g,
                &c,
                1,
                1,
                if SHOW_LOG { b"Masquer" } else { b"Journal" },
                4,
            );
            if restart == b"reboot" {
                button(g, &c, 1, 2, b"Red\xE9marrer", 5);
            }
            text(g.hdc, g.f.small, p.text3, c.left, c.top + 208, b"Les mises \xE0 jour sont t\xE9l\xE9charg\xE9es depuis les releases publiques GitHub.");
            text(
                g.hdc,
                g.f.semibold,
                p.text,
                c.left,
                c.top + 244,
                if SHOW_LOG {
                    b"Journal d'installation"
                } else {
                    b"Modifications apport\xE9es par la mise \xE0 jour"
                },
            );
            journal(
                g,
                &c,
                270,
                if SHOW_LOG {
                    LOG.get()
                } else if UPDATES.len == 0 {
                    b"Cliquez Rechercher pour afficher les modifications.\nMolette ou touches Haut/Bas pour lire la suite."
                } else {
                    UPDATES.get()
                },
            );
            text(g.hdc, g.f.small, p.text3, c.left, c.bottom - 30, b"Molette / Haut-Bas : d\xE9filer ; Page suivante/pr\xE9c\xE9dente : lecture rapide.");
        }
        _ => {}
    }
    let notice = if PAGE == 10 {
        EXE_NOTICE
    } else if !available() {
        b"Service Winlux indisponible : les actions n\xE9cessitent Winlux.".as_slice()
    } else {
        ACTION.get().split(|&v| v == b'\n').next().unwrap_or(b"")
    };
    let mut end = notice.len();
    while end > 0 && text_width(&notice[..end], 11) > c.right - c.left {
        end -= 1;
    }
    text(
        g.hdc,
        g.f.small,
        p.text2,
        c.left,
        c.bottom - 16,
        &notice[..end],
    );
}

fn hit(f: &Frame, x: i32, y: i32) -> i32 {
    let c = content(f);
    if unsafe { PAGE == 7 && restart_required() } {
        return if inside(&restart_rect(&c), x, y) {
            0
        } else {
            -1
        };
    }
    for i in 0..24 {
        let r = button_rect(&c, i / 3, i % 3);
        if r.bottom <= c.bottom - 24 && inside(&r, x, y) {
            return i;
        }
    }
    -1
}
pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    if let Mouse::Wheel(delta) = ev {
        if delta < 0 {
            SCROLL = SCROLL.saturating_add(3);
        } else {
            SCROLL = SCROLL.saturating_sub(3);
        }
        return true;
    }
    if ev == Mouse::Move {
        let h = hit(f, x, y);
        let changed = h != HOVER;
        HOVER = h;
        return changed;
    }
    if ev != Mouse::Down && ev != Mouse::DoubleClick {
        return false;
    }
    for i in 0..PAGES.len() {
        if inside(&nav_rect(f, i), x, y) {
            PAGE = i;
            SCROLL = 0;
            HOVER = -1;
            LAST_POLL = 0;
            system_tick();
            return true;
        }
    }
    if PAGE == 7 {
        let c = content(f);
        if updating() {
            return true;
        }
        if restart_required() {
            if inside(&restart_rect(&c), x, y) {
                control(c"reboot");
            }
            return true;
        }
    }
    let action = match (PAGE, hit(f, x, y)) {
        (5, 0) => {
            unsafe { MOUSE_SPEED = 25 };
            Some(c"mouse-slow")
        }
        (5, 1) => {
            unsafe { MOUSE_SPEED = 50 };
            Some(c"mouse-normal")
        }
        (5, 2) => {
            unsafe { MOUSE_SPEED = 75 };
            Some(c"mouse-fast")
        }
        (6, 12) => Some(c"desktop-icon-recycle"),
        (6, 13) => Some(c"desktop-icon-explorer"),
        (6, 14) => Some(c"desktop-icon-computer"),
        (6, 15) => Some(c"desktop-icon-documents"),
        (11, id) if (0..=8).contains(&id) => Some(
            [
                c"browser-install-edge",
                c"browser-default-edge",
                c"browser-launch-edge",
                c"browser-install-chrome",
                c"browser-default-chrome",
                c"browser-launch-chrome",
                c"browser-install-firefox",
                c"browser-default-firefox",
                c"browser-launch-firefox",
            ][id as usize],
        ),
        (10, id) if EXE_LEN > 0 && (0..=8).contains(&id) => {
            match id {
                0..=4 => EXE_SCALE = id as usize,
                5 => EXE_TRACE = !EXE_TRACE,
                6 => save_exe_settings(false),
                7 => save_exe_settings(true),
                8 => {
                    let verb = [b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];
                    if ShellExecuteW(
                        core::ptr::null_mut(),
                        verb.as_ptr(),
                        EXE_PATH.as_ptr(),
                        core::ptr::null(),
                        core::ptr::null(),
                        1,
                    ) as isize
                        <= 32
                    {
                        EXE_NOTICE =
                            b"Lancement impossible : consultez le journal de lancement Winlux.";
                    }
                }
                _ => {}
            }
            return true;
        }
        (0, 0) => Some(c"restart-session"),
        (0, 1) => Some(c"reboot"),
        (0, 2) => Some(c"poweroff"),
        (1, 0) => {
            crate::explorer::open_pc();
            return true;
        }
        (1, 1) => {
            crate::explorer::open_home();
            return true;
        }
        (1, 2) => {
            open_page(7);
            return true;
        }
        (2, 0) => Some(c"network-renew"),
        (2, 1) => Some(c"status"),
        (3, 0) => Some(c"volume-down"),
        (3, 1) => Some(c"volume-mute"),
        (3, 2) => Some(c"volume-up"),
        (4, 0) => Some(c"keyboard-fr"),
        (4, 1) => Some(c"keyboard-us"),
        (4, 3) => Some(c"tap-on"),
        (4, 4) => Some(c"tap-off"),
        (6, 0) => Some(c"align-left"),
        (6, 1) => Some(c"align-center"),
        (6, 3) => {
            unsafe { crate::gfx::set_theme(true) };
            Some(c"theme-dark")
        }
        (6, 4) => {
            unsafe { crate::gfx::set_theme(false) };
            Some(c"theme-light")
        }
        (6, 5) => {
            unsafe { AUTO_THEME = true };
            unsafe { crate::gfx::check_auto_theme() };
            Some(c"auto-theme-on")
        }
        (6, 6) => {
            unsafe { AUTO_THEME = false };
            Some(c"auto-theme-off")
        }
        (6, 18) => {
            open_page(8);
            return true;
        }
        (8, 0) => {
            SELECTED_MODE = SELECTED_MODE.saturating_sub(1);
            return true;
        }
        (8, 1) => {
            let n = value(b"display_modes").split(|&v| v == b',').count();
            SELECTED_MODE = (SELECTED_MODE + 1).min(n.saturating_sub(1));
            return true;
        }
        (8, 2) => {
            let chosen = value(b"display_modes")
                .split(|&v| v == b',')
                .nth(SELECTED_MODE)
                .unwrap_or(b"");
            if chosen.is_empty() || chosen.len() > 24 {
                return true;
            }
            let mut command = [0u8; 64];
            let prefix = b"display-mode-";
            command[..prefix.len()].copy_from_slice(prefix);
            command[prefix.len()..prefix.len() + chosen.len()].copy_from_slice(chosen);
            if let Ok(cstr) = core::ffi::CStr::from_bytes_until_nul(&command) {
                control(cstr);
            }
            return true;
        }
        (8, 3) => Some(c"display-fixed"),
        (8, 4) => Some(c"display-follow"),
        (8, 5) => Some(c"status"),
        (8, 15) => Some(c"display-scale-down"),
        (8, 16) => Some(c"display-scale-auto"),
        (8, 17) => Some(c"display-scale-up"),
        (12, 0) => {
            unsafe { POINTER_SIZE = 1 };
            Some(c"pointer-normal")
        }
        (12, 1) => {
            unsafe { POINTER_SIZE = 2 };
            Some(c"pointer-large")
        }
        (12, 2) => {
            unsafe { POINTER_SIZE = 3 };
            Some(c"pointer-xlarge")
        }
        (12, 3) => {
            unsafe { HIGH_CONTRAST = true };
            Some(c"contrast-on")
        }
        (12, 4) => {
            unsafe { HIGH_CONTRAST = false };
            Some(c"contrast-off")
        }
        (13, 0) => {
            unsafe { ENABLE_BLUR = true };
            Some(c"blur-on")
        }
        (13, 1) => {
            unsafe { ENABLE_BLUR = false };
            Some(c"blur-off")
        }
        (13, 3) => {
            unsafe { ENABLE_SMOOTH_ANIM = true };
            Some(c"smooth-on")
        }
        (13, 4) => {
            unsafe { ENABLE_SMOOTH_ANIM = false };
            Some(c"smooth-off")
        }
        (13, 6) => {
            unsafe { ENABLE_THUMBNAILS = true };
            Some(c"thumb-on")
        }
        (13, 7) => {
            unsafe { ENABLE_THUMBNAILS = false };
            Some(c"thumb-off")
        }
        (13, 9) => {
            unsafe { AUTO_THEME = true };
            Some(c"auto-theme-on")
        }
        (13, 10) => {
            unsafe { AUTO_THEME = false };
            Some(c"auto-theme-off")
        }
        (7, 0) => {
            SHOW_LOG = false;
            SCROLL = 0;
            Some(c"update-check")
        }
        (7, 1) => {
            if already_up_to_date() {
                return true;
            }
            SHOW_LOG = true;
            SCROLL = 0;
            Some(if value(b"update_action") == b"reboot" {
                c"reboot"
            } else {
                c"update-apply"
            })
        }
        (7, 2) => {
            SHOW_LOG = true;
            SCROLL = 0;
            Some(c"update-rollback")
        }
        (7, 3) => Some(if value(b"update_auto") == b"enabled" {
            c"update-auto-off"
        } else {
            c"update-auto-on"
        }),
        (7, 4) => {
            SHOW_LOG = !SHOW_LOG;
            SCROLL = 0;
            return true;
        }
        (7, 5) => Some(if value(b"update_action") == b"reboot" {
            c"reboot"
        } else {
            c"restart-session"
        }),
        _ => None,
    };
    if let Some(a) = action {
        control(a);
        return true;
    }
    false
}
pub unsafe fn key(vk: u32, ch: u16) -> bool {
    if PAGE == 7 && restart_required() {
        if !updating() && ch == 13 {
            control(c"reboot");
        }
        return true;
    }
    match vk {
        0x26 => {
            SCROLL = SCROLL.saturating_sub(1);
            true
        }
        0x28 => {
            SCROLL = SCROLL.saturating_add(1);
            true
        }
        0x21 => {
            SCROLL = SCROLL.saturating_sub(10);
            true
        }
        0x22 => {
            SCROLL = SCROLL.saturating_add(10);
            true
        }
        0x24 => {
            SCROLL = 0;
            true
        }
        0x23 => {
            SCROLL = usize::MAX;
            true
        }
        0x74 => {
            control(c"status");
            true
        }
        _ => false,
    }
}
unsafe fn draw_installation(g: &mut Gfx, c: &Rect) {
    let p = pal();
    row(
        g,
        c,
        54,
        b"Installation en cours",
        b"N'\xE9teignez pas la machine. L'arr\xEAt est temporairement verrouill\xE9.",
    );
    let phase = field(PROGRESS.get(), b"phase");
    let title: &[u8] = match phase {
        b"downloading" => b"T\xE9l\xE9chargement du paquet",
        b"verifying" => b"V\xE9rification de l'int\xE9grit\xE9",
        b"backing-up" => b"Sauvegarde des fichiers pr\xE9c\xE9dents",
        b"installing" => b"Installation des composants",
        b"restoring" => b"Restauration de la sauvegarde",
        b"finalizing" => b"Finalisation et synchronisation",
        _ => b"Pr\xE9paration de la mise \xE0 jour",
    };
    text(g.hdc, g.f.large, p.text, c.left + 12, c.top + 140, title);
    let bar = rect(c.left, c.top + 188, c.right, c.top + 198);
    round_rect(
        g.hdc, g.b, bar.left, bar.top, bar.right, bar.bottom, 5, p.surface2,
    );
    let percent = number(field(PROGRESS.get(), b"percent")).min(100);
    if percent > 0 {
        round_rect(
            g.hdc,
            g.b,
            bar.left,
            bar.top,
            bar.left + (bar.right - bar.left) * percent as i32 / 100,
            bar.bottom,
            5,
            p.accent,
        );
    } else {
        let width = (bar.right - bar.left) / 5;
        let x =
            bar.left + (GetTickCount() / 12 % (bar.right - bar.left - width).max(1) as u32) as i32;
        round_rect(g.hdc, g.b, x, bar.top, x + width, bar.bottom, 5, p.accent);
    }
    row(
        g,
        c,
        238,
        b"\xC9l\xE9ment trait\xE9 en temps r\xE9el",
        field(PROGRESS.get(), b"item"),
    );
    row(
        g,
        c,
        300,
        b"Fichiers termin\xE9s dans cette \xE9tape",
        field(PROGRESS.get(), b"done"),
    );
    text(
        g.hdc,
        g.f.small,
        p.text3,
        c.right - 140,
        c.top + 300,
        b"Nombre total",
    );
    text(
        g.hdc,
        g.f.body,
        p.text,
        c.right - 140,
        c.top + 322,
        field(PROGRESS.get(), b"total"),
    );
    journal(g, c, 366, LOG.get());
}
pub unsafe fn tick(_f: &Frame) -> bool {
    let changed = system_tick();
    if updating() && GetTickCount().wrapping_sub(LAST_ANIMATION) > 90 {
        LAST_ANIMATION = GetTickCount();
        return true;
    }
    changed
}

pub unsafe fn regression_checks() {
    let saved = (STATUS, PAGE);
    PAGE = 7;
    STATUS = EMPTY;
    let status = b"update_action=reboot\nupdate_busy=0\n";
    STATUS.bytes[..status.len()].copy_from_slice(status);
    STATUS.len = status.len();
    let f = Frame {
        outer: rect(0, 0, 1100, 800),
        title: rect(0, 0, 1100, 40),
        client: rect(0, 40, 1100, 800),
    };
    let c = content(&f);
    let r = restart_rect(&c);
    check!(
        restart_required() && hit(&f, r.right - 10, r.top + 10) == 0,
        "Mise a jour terminee : le bouton Redemarrer occupe toute la ligne"
    );
    let hidden = button_rect(&c, 1, 2);
    check!(
        hit(&f, hidden.left + 5, hidden.top + 5) == -1,
        "Les anciens boutons de recherche et reinstallation ne restent pas cliquables"
    );
    let status = b"update_action=restart-session\nupdate_busy=0\n";
    STATUS = EMPTY;
    STATUS.bytes[..status.len()].copy_from_slice(status);
    STATUS.len = status.len();
    check!(
        restart_required(),
        "Une mise a jour du runtime propose aussi de redemarrer"
    );
    STATUS = EMPTY;
    check!(
        !restart_required(),
        "Sans redemarrage requis les actions normales sont conservees"
    );
    (STATUS, PAGE) = saved;
}

pub unsafe fn regression_exe_settings(path: &[u16]) {
    open_exe_settings(path);
    check!(
        EXE_SCALE == 0 && !EXE_TRACE,
        "EXE sans profil : valeurs automatiques sans configuration obligatoire"
    );
    EXE_SCALE = 3;
    EXE_TRACE = true;
    save_exe_settings(false);
    EXE_SCALE = 0;
    EXE_TRACE = false;
    open_exe_settings(path);
    check!(
        EXE_SCALE == 3 && EXE_TRACE,
        "Les reglages 150 % et diagnostic sont relus pour le bon EXE"
    );
    save_exe_settings(true);
    let mut sidecar = [0u16; 2060];
    exe_profile_path(&mut sidecar);
    check!(
        EXE_SCALE == 0 && !EXE_TRACE && GetFileAttributesW(sidecar.as_ptr()) == u32::MAX,
        "Restaurer les valeurs par defaut retire seulement le profil du programme"
    );
}
