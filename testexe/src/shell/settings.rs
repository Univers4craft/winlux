//! Centre de controle Linux/Windows. Les actions sont detachees via
//! winlux-control ; les etats viennent du systeme. Aucun reseau dans l'UI.

use winlux_testexe::*;
use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};

pub const CUSTOM_TITLE: bool = false;
const PAGES: [&[u8]; 7] = [b"Syst\xE8me Linux", b"Applications Windows", b"R\xE9seau", b"Son et batterie", b"Clavier et pav\xE9 tactile", b"Apparence", b"Mises \xE0 jour"];
const CAPACITY: usize = 8192;
#[derive(Clone, Copy)]
struct Buffer { bytes: [u8; CAPACITY], len: usize }
const EMPTY: Buffer = Buffer { bytes: [0; CAPACITY], len: 0 };
impl Buffer { fn get(&self) -> &[u8] { &self.bytes[..self.len] } }
static mut STATUS: Buffer = EMPTY;
static mut ACTION: Buffer = EMPTY;
static mut LOG: Buffer = EMPTY;
static mut UPDATES: Buffer = EMPTY;
static mut PAGE: usize = 0;
static mut SCROLL: usize = 0;
static mut LAST_POLL: u32 = 0;
static mut HOVER: i32 = -1;
static mut SHOW_LOG: bool = false;
static mut TOKEN: [u8; 256] = [0; 256];
static mut TOKEN_LEN: usize = 0;
static mut TOKEN_FOCUS: bool = false;

unsafe fn read(path: &core::ffi::CStr, target: &mut Buffer) -> bool {
    let file = CreateFileA(path.as_ptr(), 0x8000_0000, 7, core::ptr::null_mut(), 3, 0, core::ptr::null_mut());
    let mut raw = [0u8; CAPACITY];
    let mut count = 0;
    if file as isize != -1 && !file.is_null() {
        let size = GetFileSize(file, core::ptr::null_mut());
        if size != u32::MAX && size > (CAPACITY - 1) as u32 { SetFilePointer(file, (size - (CAPACITY - 1) as u32) as i32, core::ptr::null_mut(), 0); }
        ReadFile(file, raw.as_mut_ptr(), (CAPACITY - 1) as u32, &mut count, core::ptr::null_mut());
        CloseHandle(file);
    }
    let mut next = EMPTY;
    // Les journaux Linux sont UTF-8, TextOutA attend Windows-1252.
    // Une fin de journal peut commencer au milieu d'un caractere UTF-8.
    let mut begin = 0;
    while begin < count as usize && raw[begin] & 0xc0 == 0x80 { begin += 1; }
    let source = core::str::from_utf8(&raw[begin..count as usize]).unwrap_or("");
    for ch in source.chars().take(CAPACITY - 1) {
        next.bytes[next.len] = match ch as u32 { 0..=255 => ch as u8, 0x2019 => b'\'', 0x2013 | 0x2014 => b'-', 0x20ac => 0x80, 0x153 => 0x9c, _ => b'?' };
        next.len += 1;
    }
    let changed = next.len != target.len || next.get() != target.get();
    *target = next;
    changed
}
unsafe fn value(key: &[u8]) -> &'static [u8] {
    for line in STATUS.get().split(|&c| c == b'\n') {
        if line.len() > key.len() && &line[..key.len()] == key && line[key.len()] == b'=' { return &line[key.len() + 1..]; }
    }
    b""
}
unsafe fn available() -> bool { value(b"control_available") == b"1" }
fn number(s: &[u8]) -> u32 { s.iter().take_while(|c| c.is_ascii_digit()).fold(0u32, |n, &c| n.saturating_mul(10).saturating_add((c - b'0') as u32)) }

/// Telemetrie reelle pour la barre, meme quand Parametres est ferme.
pub unsafe fn system_tick() -> bool {
    let now = GetTickCount();
    if LAST_POLL != 0 && now.wrapping_sub(LAST_POLL) < 1500 { return false; }
    LAST_POLL = now;
    let mut changed = read(c"Z:\\run\\winlux\\control.txt", &mut STATUS);
    changed |= read(c"Z:\\run\\winlux\\action.txt", &mut ACTION);
    changed |= read(if PAGE == 6 { c"Z:\\run\\winlux\\update.log" } else { c"Z:\\run\\winlux\\action.log" }, &mut LOG);
    changed |= read(c"Z:\\run\\winlux\\updates.txt", &mut UPDATES);
    if available() {
        crate::desktop::NETWORK_CONNECTED = number(value(b"network_connected"));
        crate::desktop::BATTERY_PRESENT = number(value(b"battery_present"));
        crate::desktop::BATTERY_LEVEL = number(value(b"battery_level")).min(100);
        let audio = value(b"audio");
        if audio.starts_with(b"Volume: ") {
            let muted = audio.windows(5).any(|w| w == b"MUTED");
            let v = &audio[8..];
            let n = if v.starts_with(b"1") { 100 } else if v.len() >= 4 { number(&v[2..4]) } else { 0 };
            crate::desktop::VOLUME_LEVEL = if muted { 0 } else { n };
        } else if audio.first().is_some_and(u8::is_ascii_digit) {
            crate::desktop::VOLUME_LEVEL = if audio.windows(3).any(|w| w == b"off") { 0 } else { number(audio).min(100) };
        } else { crate::desktop::VOLUME_LEVEL = 0; }
        match value(b"align") { b"left" => crate::desktop::ALIGN_LEFT = true, b"center" => crate::desktop::ALIGN_LEFT = false, _ => {} }
        match value(b"theme") { b"light" => DARK = false, b"dark" => DARK = true, _ => {} }
    }
    changed
}
pub unsafe fn control(action: &core::ffi::CStr) -> bool {
    if ShellExecuteA(core::ptr::null_mut(), c"open".as_ptr(), c"winlux-control".as_ptr(), action.as_ptr(), core::ptr::null(), 0) as isize > 32 {
        ACTION = EMPTY;
        let s = b"Operation en cours..."; ACTION.bytes[..s.len()].copy_from_slice(s); ACTION.len = s.len();
        LAST_POLL = 0;
        true
    } else { crate::desktop::toast(b"Le service de contr\xF4le Winlux n'est pas disponible."); false }
}
pub unsafe fn open_page(page: usize) {
    clear_token();
    PAGE = page.min(PAGES.len() - 1); SCROLL = 0; LAST_POLL = 0;
    crate::wm::open(crate::wm::APP_SETTINGS); system_tick();
}
unsafe fn clear_token() { TOKEN.fill(0); TOKEN_LEN = 0; TOKEN_FOCUS = false; }
unsafe fn save_token(erase: bool) {
    if TOKEN_LEN == 0 && !erase { crate::desktop::toast(b"Saisissez un jeton GitHub, ou utilisez Effacer."); return; }
    let path = c"Z:\\run\\winlux\\update-token-request";
    let file = CreateFileA(path.as_ptr(), 0x4000_0000, 0, core::ptr::null_mut(), 2, 0, core::ptr::null_mut());
    if file as isize == -1 || file.is_null() { crate::desktop::toast(b"Impossible d'enregistrer la demande de jeton."); return; }
    let size = if erase { 0 } else { TOKEN_LEN as u32 };
    let mut written = 0;
    let success = WriteFile(file, TOKEN.as_ptr(), size, &mut written, core::ptr::null_mut()) != 0 && written == size;
    CloseHandle(file);
    clear_token();
    if !success || !control(c"update-save-token") { DeleteFileA(path.as_ptr()); crate::desktop::toast(b"Le jeton n'a pas pu \xEAtre enregistr\xE9."); }
}
fn content(f: &Frame) -> Rect {
    let nav = if f.client.right - f.client.left < 700 { 156 } else { 210 };
    rect(f.client.left + nav + 20, f.client.top + 20, f.client.right - 20, f.client.bottom - 20)
}
fn nav_rect(f: &Frame, i: usize) -> Rect {
    let c = content(f);
    let step = ((f.client.bottom - f.client.top - 75) / PAGES.len() as i32).clamp(26, 42);
    rect(f.client.left + 10, f.client.top + 64 + i as i32 * step, c.left - 30, f.client.top + 64 + i as i32 * step + step - 4)
}
fn button_rect(c: &Rect, row: i32, col: i32) -> Rect {
    let w = ((c.right - c.left - 16) / 3).max(70);
    rect(c.left + col * (w + 8), c.top + 108 + row * 44, c.left + col * (w + 8) + w, c.top + 144 + row * 44)
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
    let p = pal(); let r = button_rect(c, row, col);
    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, if HOVER == id { p.hover } else { p.surface2 });
    let mut end = label.len(); while end > 0 && text_width(&label[..end], 11) > r.right - r.left - 12 { end -= 1; }
    text_centered(g.hdc, g.f.small, 11, p.text, (r.left + r.right) / 2, r.top + 12, &label[..end]);
}
unsafe fn row(g: &mut Gfx, c: &Rect, y: i32, label: &[u8], val: &[u8]) {
    if c.top + y + 42 >= c.bottom - 30 { return; }
    let p = pal();
    let mut label_end = label.len(); while label_end > 0 && text_width(&label[..label_end], 13) > c.right - c.left { label_end -= 1; }
    text(g.hdc, g.f.semibold, p.text, c.left, c.top + y, &label[..label_end]);
    let mut end = val.len(); while end > 0 && text_width(&val[..end], 13) > c.right - c.left { end -= 1; }
    text(g.hdc, g.f.body, p.text2, c.left, c.top + y + 22, if val.is_empty() { b"Information indisponible" } else { &val[..end] });
}
unsafe fn journal(g: &mut Gfx, c: &Rect, start: i32, data: &[u8]) {
    let p = pal(); let bottom = c.bottom - 34;
    if c.top + start >= bottom { return; }
    fill(g.hdc, g.b, c.left, c.top + start, c.right, bottom, p.mica);
    let rows = ((bottom - c.top - start - 12) / 18).max(0) as usize;
    for (i, line) in data.split(|&v| v == b'\n').skip(SCROLL).take(rows).enumerate() {
        let mut end = line.len(); while end > 0 && text_width(&line[..end], 11) > c.right - c.left - 20 { end -= 1; }
        text(g.hdc, g.f.small, p.text2, c.left + 10, c.top + start + 8 + i as i32 * 18, &line[..end]);
    }
}

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let p = pal(); let c = content(f);
    text(g.hdc, g.f.large, p.text, f.client.left + 20, f.client.top + 20, b"Param\xE8tres");
    for (i, name) in PAGES.iter().enumerate() {
        let r = nav_rect(f, i);
        if i == PAGE { round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, p.selected); }
        let mut end = name.len(); while end > 0 && text_width(&name[..end], 13) > r.right - r.left - 16 { end -= 1; }
        text(g.hdc, g.f.body, p.text, r.left + 8, r.top + 9, &name[..end]);
    }
    text(g.hdc, g.f.large, p.text, c.left, c.top, PAGES[PAGE]);
    match PAGE {
        0 => {
            row(g, &c, 44, b"Winlux / noyau Linux", value(b"version"));
            text(g.hdc, g.f.small, p.text2, c.left, c.top + 88, value(b"kernel"));
            button(g, &c, 0, 0, b"Relancer le bureau", 0); button(g, &c, 0, 1, b"Red\xE9marrer", 1); button(g, &c, 0, 2, b"Arr\xEAter", 2);
            row(g, &c, 170, b"P\xE9riph\xE9riques d'entr\xE9e d\xE9tect\xE9s", value(b"input_devices"));
            row(g, &c, 230, b"Stockage du syst\xE8me", if value(b"persistent") == b"1" { b"Disque persistant : r\xE9glages et mises \xE0 jour conserv\xE9s." } else { b"Session Live en RAM : modifications perdues au red\xE9marrage." });
            journal(g, &c, 296, LOG.get());
        }
        1 => {
            row(g, &c, 44, b"Couche Windows / Win32", b"Runtime Winlux - .exe via Explorer");
            button(g, &c, 0, 0, b"Programmes .exe", 0); button(g, &c, 0, 1, b"Dossier personnel", 1); button(g, &c, 0, 2, b"Mises \xE0 jour", 2);
            row(g, &c, 170, b"Installer une application Windows", b"Ouvrir son installateur .exe dans Explorer.");
            row(g, &c, 228, b"Linux et Windows", b"Les .exe partagent la session Linux et les fichiers.");
            row(g, &c, 286, b"Compatibilit\xE9", b"Le support d\xE9pend des API et composants requis par le logiciel.");
        }
        2 => {
            row(g, &c, 44, b"Connexion / adresse IPv4", value(b"network"));
            button(g, &c, 0, 0, b"Renouveler DHCP", 0); button(g, &c, 0, 1, b"Actualiser", 1);
            row(g, &c, 170, b"Machine virtuelle", b"Ethernet virtuel ; le Wi-Fi reste g\xE9r\xE9 par l'h\xF4te."); journal(g, &c, 236, LOG.get());
        }
        3 => {
            row(g, &c, 44, b"Sortie audio / volume", value(b"audio"));
            button(g, &c, 0, 0, b"Volume -", 0); button(g, &c, 0, 1, b"Muet / r\xE9tablir", 1); button(g, &c, 0, 2, b"Volume +", 2);
            row(g, &c, 170, b"Batterie expos\xE9e par le syst\xE8me", value(b"battery")); journal(g, &c, 236, LOG.get());
        }
        4 => {
            row(g, &c, 44, b"Disposition actuelle", value(b"layout"));
            button(g, &c, 0, 0, b"AZERTY fran\xE7ais", 0); button(g, &c, 0, 1, b"QWERTY US", 1);
            button(g, &c, 1, 0, b"Activer toucher-clic", 3); button(g, &c, 1, 1, b"D\xE9sactiver", 4);
            row(g, &c, 214, b"Toucher-clic (0 = non, 1 = oui)", value(b"tap"));
            row(g, &c, 274, b"Pav\xE9 tactile en VM", b"SPICE expose souvent une tablette, pas le pav\xE9 physique."); journal(g, &c, 336, LOG.get());
        }
        5 => {
            row(g, &c, 44, b"Barre des t\xE2ches", b"Pr\xE9f\xE9rences enregistr\xE9es pour la prochaine session.");
            button(g, &c, 0, 0, b"Aligner \xE0 gauche", 0); button(g, &c, 0, 1, b"Centrer", 1);
            button(g, &c, 1, 0, b"Th\xE8me sombre", 3); button(g, &c, 1, 1, b"Th\xE8me clair", 4);
            row(g, &c, 214, b"Stockage", if value(b"persistent") == b"1" { b"Les pr\xE9f\xE9rences sont conserv\xE9es sur le disque." } else { b"Session Live : les r\xE9glages durent jusqu'au red\xE9marrage." });
        }
        _ => {
            row(g, &c, 44, b"Automatique (syst\xE8me et outil de mise \xE0 jour)", value(b"update_auto"));
            button(g, &c, 0, 0, b"Rechercher", 0); button(g, &c, 0, 1, b"Tout installer", 1); button(g, &c, 0, 2, b"Restaurer", 2);
            button(g, &c, 1, 0, if value(b"update_auto") == b"enabled" { b"D\xE9sactiver auto" } else { b"Activer auto" }, 3);
            button(g, &c, 1, 1, if SHOW_LOG { b"Liste des composants" } else { b"Afficher le journal" }, 4);
            button(g, &c, 1, 2, if value(b"update_action") == b"reboot" { b"Red\xE9marrer l'OS" } else { b"Relancer le bureau" }, 5);
            text(g.hdc, g.f.semibold, p.text, c.left, c.top + 208, if value(b"update_auth") == b"configured" { b"GitHub priv\xE9 : jeton enregistr\xE9" } else { b"GitHub priv\xE9 : jeton requis" });
            let tr = token_rect(&c, 0);
            round_rect(g.hdc, g.b, tr.left, tr.top, tr.right, tr.bottom, 4, if TOKEN_FOCUS { p.accent } else { p.border });
            fill(g.hdc, g.b, tr.left + 1, tr.top + 1, tr.right - 1, tr.bottom - 1, p.mica);
            if TOKEN_LEN == 0 {
                text(g.hdc, g.f.small, p.text3, tr.left + 8, tr.top + 12, b"Jeton GitHub (masqu\xE9)");
            } else {
                let masked = [b'*'; 256];
                let n = TOKEN_LEN.min(((tr.right - tr.left - 20).max(0) / 7) as usize);
                text(g.hdc, g.f.body, p.text, tr.left + 8, tr.top + 12, &masked[..n]);
            }
            for (i, name) in [b"Enregistrer".as_slice(), b"Effacer"].iter().enumerate() {
                let r = token_rect(&c, i as i32 + 1);
                round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, p.surface2);
                text_centered(g.hdc, g.f.small, 11, p.text, (r.left + r.right) / 2, r.top + 12, name);
            }
            text(g.hdc, g.f.small, p.text3, c.left, c.top + 280, b"Acc\xE8s aux releases du d\xE9p\xF4t priv\xE9 ; jeton conserv\xE9 uniquement dans Winlux.");
            journal(g, &c, 310, if SHOW_LOG { LOG.get() } else if UPDATES.len == 0 { b"Cliquez Rechercher pour recuperer la liste des mises a jour." } else { UPDATES.get() });
        }
    }
    let notice = if !available() { b"Service Winlux indisponible : les actions n\xE9cessitent Winlux.".as_slice() } else { ACTION.get().split(|&v| v == b'\n').next().unwrap_or(b"") };
    let mut end = notice.len(); while end > 0 && text_width(&notice[..end], 11) > c.right - c.left { end -= 1; }
    text(g.hdc, g.f.small, p.text2, c.left, c.bottom - 16, &notice[..end]);
}

fn hit(f: &Frame, x: i32, y: i32) -> i32 { let c = content(f); for i in 0..6 { if inside(&button_rect(&c, i / 3, i % 3), x, y) { return i; } } -1 }
pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    if let Mouse::Wheel(delta) = ev { if delta < 0 { SCROLL = SCROLL.saturating_add(3); } else { SCROLL = SCROLL.saturating_sub(3); } return true; }
    if ev == Mouse::Move { let h = hit(f, x, y); let changed = h != HOVER; HOVER = h; return changed; }
    if ev != Mouse::Down && ev != Mouse::DoubleClick { return false; }
    for i in 0..PAGES.len() { if inside(&nav_rect(f, i), x, y) { clear_token(); PAGE = i; SCROLL = 0; HOVER = -1; LAST_POLL = 0; system_tick(); return true; } }
    if PAGE == 6 {
        let c = content(f);
        if inside(&token_rect(&c, 0), x, y) { TOKEN_FOCUS = true; return true; }
        TOKEN_FOCUS = false;
        if inside(&token_rect(&c, 1), x, y) { save_token(false); return true; }
        if inside(&token_rect(&c, 2), x, y) { save_token(true); return true; }
    }
    let action = match (PAGE, hit(f, x, y)) {
        (0, 0) => Some(c"restart-session"), (0, 1) => Some(c"reboot"), (0, 2) => Some(c"poweroff"),
        (1, 0) => { crate::explorer::open_pc(); return true; }, (1, 1) => { crate::explorer::open_home(); return true; }, (1, 2) => { open_page(6); return true; },
        (2, 0) => Some(c"network-renew"), (2, 1) => Some(c"status"),
        (3, 0) => Some(c"volume-down"), (3, 1) => Some(c"volume-mute"), (3, 2) => Some(c"volume-up"),
        (4, 0) => Some(c"keyboard-fr"), (4, 1) => Some(c"keyboard-us"), (4, 3) => Some(c"tap-on"), (4, 4) => Some(c"tap-off"),
        (5, 0) => Some(c"align-left"), (5, 1) => Some(c"align-center"), (5, 3) => Some(c"theme-dark"), (5, 4) => Some(c"theme-light"),
        (6, 0) => { SHOW_LOG = false; SCROLL = 0; Some(c"update-check") }, (6, 1) => { SHOW_LOG = true; SCROLL = 0; Some(c"update-apply") }, (6, 2) => { SHOW_LOG = true; SCROLL = 0; Some(c"update-rollback") },
        (6, 3) => Some(if value(b"update_auto") == b"enabled" { c"update-auto-off" } else { c"update-auto-on" }),
        (6, 4) => { SHOW_LOG = !SHOW_LOG; SCROLL = 0; return true; },
        (6, 5) => Some(if value(b"update_action") == b"reboot" { c"reboot" } else { c"restart-session" }),
        _ => None,
    };
    if let Some(a) = action { control(a); return true; } false
}
pub unsafe fn key(vk: u32, ch: u16) -> bool {
    if PAGE == 6 && TOKEN_FOCUS {
        match ch {
            8 => { if TOKEN_LEN > 0 { TOKEN_LEN -= 1; TOKEN[TOKEN_LEN] = 0; } },
            13 => save_token(false),
            27 => { TOKEN_FOCUS = false; },
            _ if ch < 128 && (ch as u8).is_ascii_alphanumeric() || ch == b'_' as u16 => {
                if TOKEN_LEN < TOKEN.len() - 1 { TOKEN[TOKEN_LEN] = ch as u8; TOKEN_LEN += 1; }
            }
            _ => {}
        }
        return true;
    }
    match vk { 0x26 => { SCROLL = SCROLL.saturating_sub(1); true }, 0x28 => { SCROLL = SCROLL.saturating_add(1); true }, 0x74 => { control(c"status"); true }, _ => false }
}
pub unsafe fn tick(_f: &Frame) -> bool { system_tick() }
