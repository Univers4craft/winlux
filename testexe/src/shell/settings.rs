//! Paramètres Linux/Windows : navigation Mica et commandes système réelles.
//! Les actions passent par winlux-control et les états viennent du système.

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;
const PAGES: [&[u8]; 9] = [
    b"Syst\xE8me Linux",
    b"Applications Windows",
    b"R\xE9seau",
    b"Son et batterie",
    b"Clavier et pav\xE9 tactile",
    b"Apparence",
    b"Mises \xE0 jour",
    b"\xC9cran et d\xE9finition",
    b"\xC0 propos",
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
static mut TOKEN: [u8; 256] = [0; 256];
static mut TOKEN_LEN: usize = 0;
static mut TOKEN_FOCUS: bool = false;

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
    changed |= read(c"Z:\\run\\winlux\\action.txt", &mut ACTION);
    changed |= read(
        if PAGE == 6 {
            c"Z:\\run\\winlux\\update.log"
        } else {
            c"Z:\\run\\winlux\\action.log"
        },
        &mut LOG,
    );
    changed |= read(c"Z:\\run\\winlux\\updates.txt", &mut UPDATES);
    changed |= read(c"Z:\\run\\winlux\\update-progress", &mut PROGRESS);
    if PAGE == 8 {
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
            b"light" => DARK = false,
            b"dark" => DARK = true,
            _ => {}
        }
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
    clear_token();
    PAGE = page.min(PAGES.len() - 1);
    SCROLL = 0;
    LAST_POLL = 0;
    crate::wm::open(crate::wm::APP_SETTINGS);
    system_tick();
}
unsafe fn clear_token() {
    TOKEN.fill(0);
    TOKEN_LEN = 0;
    TOKEN_FOCUS = false;
}
unsafe fn save_token(erase: bool) {
    if TOKEN_LEN == 0 && !erase {
        crate::desktop::toast(b"Saisissez un jeton GitHub, ou utilisez Effacer.");
        return;
    }
    let path = c"Z:\\run\\winlux\\update-token-request";
    let file = CreateFileA(
        path.as_ptr(),
        0x4000_0000,
        0,
        core::ptr::null_mut(),
        2,
        0,
        core::ptr::null_mut(),
    );
    if file as isize == -1 || file.is_null() {
        crate::desktop::toast(b"Impossible d'enregistrer la demande de jeton.");
        return;
    }
    let size = if erase { 0 } else { TOKEN_LEN as u32 };
    let mut written = 0;
    let success = WriteFile(
        file,
        TOKEN.as_ptr(),
        size,
        &mut written,
        core::ptr::null_mut(),
    ) != 0
        && written == size;
    CloseHandle(file);
    clear_token();
    if !success || !control(c"update-save-token") {
        DeleteFileA(path.as_ptr());
        crate::desktop::toast(b"Le jeton n'a pas pu \xEAtre enregistr\xE9.");
    }
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
fn token_rect(c: &Rect, part: i32) -> Rect {
    let reserve = ((c.right - c.left) / 3).clamp(140, 220);
    let left = c.right - reserve;
    match part {
        0 => rect(c.left, c.top + 232, left - 8, c.top + 268),
        1 => rect(left, c.top + 232, left + reserve / 2 - 4, c.top + 268),
        _ => rect(left + reserve / 2 + 4, c.top + 232, c.right, c.top + 268),
    }
}
unsafe fn button(g: &mut Gfx, c: &Rect, row: i32, col: i32, label: &[u8], id: i32) {
    let p = pal();
    let r = button_rect(c, row, col);
    if r.bottom > c.bottom - 24 {
        return;
    }
    let disabled = updating() && (PAGE == 0 || PAGE == 6 && id != 4);
    let selected = match (PAGE, id) {
        (4, 0) => value(b"layout") == b"fr",
        (4, 1) => value(b"layout") == b"us",
        (4, 3) => value(b"tap") == b"1",
        (4, 4) => value(b"tap") == b"0",
        (5, 0) => value(b"align") == b"left",
        (5, 1) => value(b"align") == b"center",
        (5, 3) => value(b"theme") == b"dark",
        (5, 4) => value(b"theme") == b"light",
        (7, 3) => value(b"display_policy") == b"fixed",
        (7, 4) => value(b"display_policy") != b"fixed",
        _ => false,
    };
    let primary = PAGE == 6 && id == 1 || PAGE == 7 && id == 2;
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
    let detailed = PAGE == 6 && !SHOW_LOG;
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
        8 => {
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
        5 => {
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
                214,
                b"Stockage",
                if value(b"persistent") == b"1" {
                    b"Les pr\xE9f\xE9rences sont conserv\xE9es sur le disque."
                } else {
                    b"Session Live : les r\xE9glages durent jusqu'au red\xE9marrage."
                },
            );
            button(g, &c, 4, 0, b"R\xE9glages de l'\xE9cran", 12);
        }
        7 => {
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
        _ => {
            if updating() {
                draw_installation(g, &c);
                return;
            }
            let restart = value(b"update_action");
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
                    b"Protection et mises \xE0 jour du syst\xE8me",
                    if value(b"update_auto") == b"enabled" {
                        b"Recherche automatique activ\xE9e - sans red\xE9marrage impos\xE9."
                    } else {
                        b"Installation sur demande - vos donn\xE9es sont conserv\xE9es."
                    },
                );
            }
            button(g, &c, 0, 0, b"Rechercher", 0);
            button(
                g,
                &c,
                0,
                1,
                if restart == b"reboot" {
                    b"Red\xE9marrer maintenant"
                } else {
                    b"Tout installer"
                },
                1,
            );
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
                if SHOW_LOG {
                    b"Voir les modifications"
                } else {
                    b"Afficher le journal"
                },
                4,
            );
            button(
                g,
                &c,
                1,
                2,
                if value(b"update_action") == b"reboot" {
                    b"Red\xE9marrer l'OS"
                } else {
                    b"Relancer le bureau"
                },
                5,
            );
            text(
                g.hdc,
                g.f.semibold,
                p.text,
                c.left,
                c.top + 208,
                if value(b"update_auth") == b"configured" {
                    b"GitHub priv\xE9 : jeton enregistr\xE9"
                } else {
                    b"GitHub priv\xE9 : jeton requis"
                },
            );
            let tr = token_rect(&c, 0);
            round_rect(
                g.hdc,
                g.b,
                tr.left,
                tr.top,
                tr.right,
                tr.bottom,
                4,
                if TOKEN_FOCUS { p.accent } else { p.border },
            );
            fill(
                g.hdc,
                g.b,
                tr.left + 1,
                tr.top + 1,
                tr.right - 1,
                tr.bottom - 1,
                p.mica,
            );
            if TOKEN_LEN == 0 {
                text(
                    g.hdc,
                    g.f.small,
                    p.text3,
                    tr.left + 8,
                    tr.top + 12,
                    b"Jeton GitHub (masqu\xE9)",
                );
            } else {
                let masked = [b'*'; 256];
                let n = TOKEN_LEN.min(((tr.right - tr.left - 20).max(0) / 7) as usize);
                text(
                    g.hdc,
                    g.f.body,
                    p.text,
                    tr.left + 8,
                    tr.top + 12,
                    &masked[..n],
                );
            }
            for (i, name) in [b"Enregistrer".as_slice(), b"Effacer"].iter().enumerate() {
                let r = token_rect(&c, i as i32 + 1);
                round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, p.surface2);
                text_centered(
                    g.hdc,
                    g.f.small,
                    11,
                    p.text,
                    (r.left + r.right) / 2,
                    r.top + 12,
                    name,
                );
            }
            text(g.hdc, g.f.small, p.text3, c.left, c.top + 280, b"Acc\xE8s aux releases du d\xE9p\xF4t priv\xE9 ; jeton conserv\xE9 uniquement dans Winlux.");
            text(
                g.hdc,
                g.f.semibold,
                p.text,
                c.left,
                c.top + 308,
                if SHOW_LOG {
                    b"Journal d'installation"
                } else {
                    b"Modifications apport\xE9es par la mise \xE0 jour"
                },
            );
            journal(
                g,
                &c,
                334,
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
    }
    let notice = if !available() {
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
            clear_token();
            PAGE = i;
            SCROLL = 0;
            HOVER = -1;
            LAST_POLL = 0;
            system_tick();
            return true;
        }
    }
    if PAGE == 6 {
        let c = content(f);
        if inside(&token_rect(&c, 0), x, y) {
            TOKEN_FOCUS = true;
            return true;
        }
        TOKEN_FOCUS = false;
        if inside(&token_rect(&c, 1), x, y) {
            save_token(false);
            return true;
        }
        if inside(&token_rect(&c, 2), x, y) {
            save_token(true);
            return true;
        }
    }
    let action = match (PAGE, hit(f, x, y)) {
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
            open_page(6);
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
        (5, 0) => Some(c"align-left"),
        (5, 1) => Some(c"align-center"),
        (5, 3) => Some(c"theme-dark"),
        (5, 4) => Some(c"theme-light"),
        (5, 12) => {
            open_page(7);
            return true;
        }
        (7, 0) => {
            SELECTED_MODE = SELECTED_MODE.saturating_sub(1);
            return true;
        }
        (7, 1) => {
            let n = value(b"display_modes").split(|&v| v == b',').count();
            SELECTED_MODE = (SELECTED_MODE + 1).min(n.saturating_sub(1));
            return true;
        }
        (7, 2) => {
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
        (7, 3) => Some(c"display-fixed"),
        (7, 4) => Some(c"display-follow"),
        (7, 5) => Some(c"status"),
        (7, 15) => Some(c"display-scale-down"),
        (7, 16) => Some(c"display-scale-auto"),
        (7, 17) => Some(c"display-scale-up"),
        (6, 0) => {
            SHOW_LOG = false;
            SCROLL = 0;
            Some(c"update-check")
        }
        (6, 1) => {
            SHOW_LOG = true;
            SCROLL = 0;
            Some(if value(b"update_action") == b"reboot" {
                c"reboot"
            } else {
                c"update-apply"
            })
        }
        (6, 2) => {
            SHOW_LOG = true;
            SCROLL = 0;
            Some(c"update-rollback")
        }
        (6, 3) => Some(if value(b"update_auto") == b"enabled" {
            c"update-auto-off"
        } else {
            c"update-auto-on"
        }),
        (6, 4) => {
            SHOW_LOG = !SHOW_LOG;
            SCROLL = 0;
            return true;
        }
        (6, 5) => Some(if value(b"update_action") == b"reboot" {
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
    if PAGE == 6 && TOKEN_FOCUS {
        match ch {
            8 => {
                if TOKEN_LEN > 0 {
                    TOKEN_LEN -= 1;
                    TOKEN[TOKEN_LEN] = 0;
                }
            }
            13 => save_token(false),
            27 => {
                TOKEN_FOCUS = false;
            }
            _ if ch < 128 && (ch as u8).is_ascii_alphanumeric() || ch == b'_' as u16 => {
                if TOKEN_LEN < TOKEN.len() - 1 {
                    TOKEN[TOKEN_LEN] = ch as u8;
                    TOKEN_LEN += 1;
                }
            }
            _ => {}
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
