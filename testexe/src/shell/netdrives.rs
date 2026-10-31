//! Paramètres > Lecteurs réseau : connecter un FTP, FTPS, SFTP, WebDAV, SMB,
//! NFS ou S3 comme un disque avec une lettre, à la manière de « Connecter
//! un lecteur réseau » sous Windows.
//!
//! La page ne monte rien elle-même : elle dépose la demande dans
//! `Z:\run\winlux\netdrive-request` (lue puis effacée aussitôt par
//! `winlux-drives`, le mot de passe ne passe jamais en argument) et affiche
//! l'état publié dans `Z:\run\winlux\netdrives.txt`. Seuls les types de
//! connexion réellement utilisables sur la machine sont proposés.

use crate::gfx::*;
use crate::wm::{Gfx, Mouse};
use winlux_testexe::*;

const STATUS_CAP: usize = 8192;
const FIELD_CAP: usize = 120;

// Champs du formulaire.
const F_HOST: usize = 0;
const F_PORT: usize = 1;
const F_USER: usize = 2;
const F_PASS: usize = 3;
const F_PATH: usize = 4;
const F_NAME: usize = 5;
const FIELDS: usize = 6;

const PROTOS: [(&[u8], &[u8]); 8] = [
    (b"ftp", b"FTP"),
    (b"ftps", b"FTPS"),
    (b"sftp", b"SFTP (SSH)"),
    (b"webdav", b"WebDAV"),
    (b"webdavs", b"WebDAV TLS"),
    (b"smb", b"SMB"),
    (b"nfs", b"NFS"),
    (b"s3", b"S3"),
];

static mut STATUS: [u8; STATUS_CAP] = [0; STATUS_CAP];
static mut STATUS_LEN: usize = 0;
static mut LAST_POLL: u32 = 0;
static mut ASKED_STATUS: bool = false;
static mut FIELD: [[u8; FIELD_CAP]; FIELDS] = [[0; FIELD_CAP]; FIELDS];
static mut FLEN: [usize; FIELDS] = [0; FIELDS];
static mut FOCUS: usize = F_HOST;
static mut PROTO: usize = usize::MAX;
static mut LETTER: u8 = 0;
static mut AUTO: bool = true;
static mut REMEMBER: bool = true;
static mut HOVER: i32 = -1;
/// Action en cours et numéro du dernier résultat vu (pour « en cours… »).
static mut PENDING: bool = false;
static mut SEEN_SERIAL: [u8; 24] = [0; 24];
static mut SEEN_LEN: usize = 0;
/// Lecteur dont le formulaire est chargé (reconnexion avec mot de passe).
static mut EDITING: u8 = 0;

// ------------------------------------------------------------ état publié

unsafe fn status() -> &'static [u8] {
    &STATUS[..STATUS_LEN]
}

fn field<'a>(data: &'a [u8], key: &[u8]) -> &'a [u8] {
    for line in data.split(|&c| c == b'\n') {
        if line.len() > key.len() && &line[..key.len()] == key && line[key.len()] == b'=' {
            return &line[key.len() + 1..];
        }
    }
    b""
}

/// Lit l'état (UTF-8) et le convertit en Windows-1252 pour TextOutA.
unsafe fn reload() -> bool {
    let h = CreateFileA(
        c"Z:\\run\\winlux\\netdrives.txt".as_ptr(),
        0x8000_0000,
        7,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if h as isize == -1 || h.is_null() {
        return false;
    }
    let mut raw = [0u8; STATUS_CAP];
    let mut n = 0u32;
    ReadFile(
        h,
        raw.as_mut_ptr(),
        STATUS_CAP as u32,
        &mut n,
        core::ptr::null_mut(),
    );
    CloseHandle(h);
    let text = core::str::from_utf8(&raw[..n as usize]).unwrap_or("");
    let mut out = [0u8; STATUS_CAP];
    let mut len = 0;
    for ch in text.chars() {
        if len >= STATUS_CAP {
            break;
        }
        out[len] = if (ch as u32) < 0x100 { ch as u8 } else { b'?' };
        len += 1;
    }
    let changed = len != STATUS_LEN || out[..len] != STATUS[..STATUS_LEN];
    STATUS[..len].copy_from_slice(&out[..len]);
    STATUS_LEN = len;
    let serial = field(status(), b"serial");
    if PENDING && serial != &SEEN_SERIAL[..SEEN_LEN] {
        PENDING = false;
    }
    changed
}

unsafe fn remember_serial() {
    let serial = field(status(), b"serial");
    let n = serial.len().min(SEEN_SERIAL.len());
    SEEN_SERIAL[..n].copy_from_slice(&serial[..n]);
    SEEN_LEN = n;
}

unsafe fn run(args: &core::ffi::CStr) -> bool {
    remember_serial();
    let ok = ShellExecuteA(
        core::ptr::null_mut(),
        c"open".as_ptr(),
        c"winlux-drives".as_ptr(),
        args.as_ptr(),
        core::ptr::null(),
        0,
    ) as isize
        > 32;
    PENDING = ok;
    LAST_POLL = 0;
    ok
}

/// Appelé à chaque tour des Paramètres : rafraîchit l'état et ouvre la page
/// quand un lecteur réclame son mot de passe.
pub unsafe fn poll(visible: bool) -> bool {
    let mut changed = false;
    if GetFileAttributesA(c"Z:\\run\\winlux\\netdrive-ask".as_ptr()) != u32::MAX {
        let mut letter = [0u8; 4];
        let h = CreateFileA(
            c"Z:\\run\\winlux\\netdrive-ask".as_ptr(),
            0x8000_0000,
            7,
            core::ptr::null_mut(),
            3,
            0,
            core::ptr::null_mut(),
        );
        if h as isize != -1 && !h.is_null() {
            let mut n = 0u32;
            ReadFile(h, letter.as_mut_ptr(), 4, &mut n, core::ptr::null_mut());
            CloseHandle(h);
        }
        DeleteFileA(c"Z:\\run\\winlux\\netdrive-ask".as_ptr());
        reload();
        if letter[0].is_ascii_uppercase() {
            edit(letter[0]);
        }
        crate::settings::open_page(crate::settings::PAGE_NETDRIVES);
        changed = true;
    }
    if !visible {
        return changed;
    }
    if !ASKED_STATUS {
        ASKED_STATUS = true;
        run(c"status");
        PENDING = false;
    }
    let now = GetTickCount();
    if LAST_POLL == 0 || now.wrapping_sub(LAST_POLL) > 700 {
        LAST_POLL = now;
        changed |= reload();
    }
    changed || PENDING
}

// ------------------------------------------------------------- formulaire

unsafe fn offered(i: usize) -> bool {
    field(status(), b"protocols")
        .split(|&c| c == b',')
        .any(|p| p == PROTOS[i].0)
}

unsafe fn default_port(i: usize) -> &'static [u8] {
    match PROTOS[i].0 {
        b"ftp" | b"ftps" => b"21",
        b"sftp" => b"22",
        b"webdav" => b"80",
        b"webdavs" | b"s3" => b"443",
        b"smb" => b"445",
        _ => b"2049",
    }
}

unsafe fn set_field(i: usize, v: &[u8]) {
    let n = v.len().min(FIELD_CAP);
    FIELD[i][..n].copy_from_slice(&v[..n]);
    FLEN[i] = n;
}

unsafe fn get(i: usize) -> &'static [u8] {
    &FIELD[i][..FLEN[i]]
}

unsafe fn letter() -> u8 {
    if LETTER == 0 {
        let l = field(status(), b"next_letter");
        LETTER = l.first().copied().filter(u8::is_ascii_uppercase).unwrap_or(b'N');
    }
    LETTER
}

unsafe fn select_proto(i: usize) {
    let old_default = if PROTO < PROTOS.len() {
        default_port(PROTO)
    } else {
        b""
    };
    PROTO = i;
    if FLEN[F_PORT] == 0 || get(F_PORT) == old_default {
        set_field(F_PORT, default_port(i));
    }
}

/// Charge un lecteur existant dans le formulaire (le mot de passe reste vide).
unsafe fn edit(l: u8) {
    let key = [b'd', b'r', b'i', b'v', b'e', b'.', l];
    let line = field(status(), &key);
    if line.is_empty() {
        return;
    }
    let mut parts = line.split(|&c| c == b'|');
    let name = parts.next().unwrap_or(b"");
    let proto = parts.next().unwrap_or(b"");
    let host = parts.next().unwrap_or(b"");
    let port = parts.next().unwrap_or(b"");
    let user = parts.next().unwrap_or(b"");
    let path = parts.next().unwrap_or(b"");
    AUTO = parts.next() != Some(b"0");
    REMEMBER = parts.next() == Some(b"1");
    if let Some(i) = PROTOS.iter().position(|p| p.0 == proto) {
        PROTO = i;
    }
    set_field(F_NAME, name);
    set_field(F_HOST, host);
    set_field(F_PORT, port);
    set_field(F_USER, user);
    set_field(F_PATH, path);
    set_field(F_PASS, b"");
    LETTER = l;
    EDITING = l;
    FOCUS = F_PASS;
}

unsafe fn clear_form() {
    for i in 0..FIELDS {
        FLEN[i] = 0;
    }
    if PROTO < PROTOS.len() {
        set_field(F_PORT, default_port(PROTO));
    }
    LETTER = 0;
    EDITING = 0;
    FOCUS = F_HOST;
}

/// Écrit la demande dans un fichier privé lu puis effacé par winlux-drives.
unsafe fn write_request() -> bool {
    let mut buf = [0u8; 1024];
    let mut n = 0;
    let mut put = |k: &[u8], v: &[u8]| {
        for &c in k.iter().chain(b"=").chain(v).chain(b"\n") {
            if n < buf.len() {
                buf[n] = c;
                n += 1;
            }
        }
    };
    put(b"letter", &[letter()]);
    put(b"protocol", if PROTO < PROTOS.len() { PROTOS[PROTO].0 } else { b"" });
    put(b"host", get(F_HOST));
    put(b"port", get(F_PORT));
    put(b"user", get(F_USER));
    put(b"password", get(F_PASS));
    put(b"path", get(F_PATH));
    put(b"name", get(F_NAME));
    put(b"auto", if AUTO { b"1" } else { b"0" });
    put(b"remember", if REMEMBER { b"1" } else { b"0" });
    let h = CreateFileA(
        c"Z:\\run\\winlux\\netdrive-request".as_ptr(),
        0x4000_0000,
        0,
        core::ptr::null_mut(),
        2,
        0,
        core::ptr::null_mut(),
    );
    if h as isize == -1 || h.is_null() {
        return false;
    }
    let mut written = 0u32;
    let ok = WriteFile(h, buf.as_ptr(), n as u32, &mut written, core::ptr::null_mut());
    CloseHandle(h);
    // Le mot de passe ne reste pas en mémoire de l'interface.
    for b in buf.iter_mut() {
        *b = 0;
    }
    ok != 0 && written == n as u32
}

unsafe fn submit(action: &core::ffi::CStr) {
    if PROTO >= PROTOS.len() || FLEN[F_HOST] == 0 {
        crate::desktop::toast(b"Choisissez un type de connexion et l'adresse du serveur.");
        return;
    }
    if !write_request() {
        crate::desktop::toast(b"Le service des lecteurs r\xE9seau n'est pas disponible.");
        return;
    }
    if !run(action) {
        crate::desktop::toast(b"winlux-drives est introuvable sur ce syst\xE8me.");
        return;
    }
    if action.to_bytes() == b"add" {
        set_field(F_PASS, b"");
        EDITING = 0;
    }
}

// ------------------------------------------------------------- géométrie

/// Zones cliquables : identifiants stables pour le survol.
#[derive(Clone, Copy, PartialEq)]
enum Hit {
    Proto(usize),
    Field(usize),
    LetterPrev,
    LetterNext,
    Auto,
    Remember,
    Test,
    Save,
    Clear,
    Toggle(usize),
    Remove(usize),
}

fn hit_id(h: Hit) -> i32 {
    match h {
        Hit::Proto(i) => i as i32,
        Hit::Field(i) => 20 + i as i32,
        Hit::LetterPrev => 40,
        Hit::LetterNext => 41,
        Hit::Auto => 42,
        Hit::Remember => 43,
        Hit::Test => 44,
        Hit::Save => 45,
        Hit::Clear => 46,
        Hit::Toggle(i) => 60 + i as i32,
        Hit::Remove(i) => 90 + i as i32,
    }
}

unsafe fn drives() -> &'static [u8] {
    let d = field(status(), b"drives");
    &d[..d.len().min(8)]
}

const DRIVE_H: i32 = 52;
const AUTO_LABEL: &[u8] = b"Reconnecter \xE0 l'ouverture de session";
const REMEMBER_LABEL: &[u8] = b"M\xE9moriser le mot de passe (chiffr\xE9)";

/// Défilement vertical de la page (molette) et zone réellement visible :
/// un élément n'est dessiné et cliquable que s'il y tient entièrement.
static mut OFFSET: i32 = 0;
static mut MAX_OFFSET: i32 = 0;
static mut VIEW: (i32, i32) = (0, i32::MAX);

fn shown(r: &Rect) -> bool {
    unsafe { r.top >= VIEW.0 && r.bottom <= VIEW.1 }
}

/// Rectangle virtuel de la page, décalé par le défilement.
unsafe fn virtual_rect(c: &Rect) -> Rect {
    VIEW = (c.top + 36, c.bottom - 4);
    OFFSET = OFFSET.clamp(0, MAX_OFFSET.max(0));
    rect(c.left, c.top - OFFSET, c.right, c.bottom - OFFSET)
}

fn drive_row(c: &Rect, i: usize) -> Rect {
    let y = c.top + 68 + i as i32 * (DRIVE_H + 6);
    rect(c.left, y, c.right, y + DRIVE_H)
}

fn drive_buttons(r: &Rect) -> (Rect, Rect) {
    let w = 112;
    let remove = rect(r.right - w - 10, r.top + 11, r.right - 10, r.bottom - 11);
    let toggle = rect(remove.left - w - 8, remove.top, remove.left - 8, remove.bottom);
    (toggle, remove)
}

unsafe fn form_top(c: &Rect) -> i32 {
    let n = drives().len().max(1) as i32;
    c.top + 68 + n * (DRIVE_H + 6) + 14
}

/// Types proposés sur une seule ligne quand la largeur le permet.
unsafe fn proto_cols(c: &Rect) -> i32 {
    let n = (0..PROTOS.len()).filter(|&i| offered(i)).count().max(1) as i32;
    if (c.right - c.left) / n >= 84 {
        n
    } else {
        4
    }
}

unsafe fn proto_rect(c: &Rect, slot: usize) -> Rect {
    let top = form_top(c) + 26;
    let cols = proto_cols(c);
    let w = (c.right - c.left - 8 * (cols - 1)) / cols;
    let col = (slot as i32) % cols;
    let row = (slot as i32) / cols;
    let x = c.left + col * (w + 8);
    let y = top + row * 40;
    rect(x, y, x + w, y + 34)
}

unsafe fn fields_top(c: &Rect) -> i32 {
    let shown = (0..PROTOS.len()).filter(|&i| offered(i)).count().max(1) as i32;
    let cols = proto_cols(c);
    form_top(c) + 26 + ((shown + cols - 1) / cols) * 40 + 4
}

/// Champ i : deux colonnes, libellé au-dessus de la zone de saisie.
unsafe fn field_rect(c: &Rect, i: usize) -> Rect {
    let top = fields_top(c);
    let w = (c.right - c.left - 16) / 2;
    let (col, row) = match i {
        F_HOST => (0, 0),
        F_PORT => (1, 0),
        F_USER => (0, 1),
        F_PASS => (1, 1),
        F_PATH => (0, 2),
        _ => (1, 2),
    };
    let x = c.left + col * (w + 16);
    let y = top + row * 58 + 20;
    rect(x, y, x + w, y + 32)
}

unsafe fn options_top(c: &Rect) -> i32 {
    fields_top(c) + 3 * 58 + 20
}

unsafe fn letter_rects(c: &Rect) -> (Rect, Rect, Rect) {
    let y = options_top(c);
    let prev = rect(c.left, y, c.left + 32, y + 32);
    let shown = rect(c.left + 36, y, c.left + 100, y + 32);
    let next = rect(c.left + 104, y, c.left + 136, y + 32);
    (prev, shown, next)
}

unsafe fn check_rect(c: &Rect, i: i32) -> Rect {
    let y = options_top(c);
    let w0 = 34 + text_width(AUTO_LABEL, 13);
    let w1 = 34 + text_width(REMEMBER_LABEL, 13);
    let x = c.left + 160 + if i == 0 { 0 } else { w0 + 20 };
    rect(x, y + 4, x + if i == 0 { w0 } else { w1 }, y + 28)
}

unsafe fn action_rect(c: &Rect, i: i32) -> Rect {
    let y = options_top(c) + 46;
    let w = ((c.right - c.left - 20) / 3).min(210);
    let x = c.left + i * (w + 10);
    rect(x, y, x + w, y + 36)
}

unsafe fn hit(c: &Rect, x: i32, y: i32) -> Option<Hit> {
    let inside = |r: &Rect, x: i32, y: i32| shown(r) && inside(r, x, y);
    for (i, _) in drives().iter().enumerate() {
        let (t, r) = drive_buttons(&drive_row(c, i));
        if inside(&t, x, y) {
            return Some(Hit::Toggle(i));
        }
        if inside(&r, x, y) {
            return Some(Hit::Remove(i));
        }
    }
    let mut slot = 0;
    for i in 0..PROTOS.len() {
        if offered(i) {
            if inside(&proto_rect(c, slot), x, y) {
                return Some(Hit::Proto(i));
            }
            slot += 1;
        }
    }
    if slot == 0 {
        return None;
    }
    for i in 0..FIELDS {
        if inside(&field_rect(c, i), x, y) {
            return Some(Hit::Field(i));
        }
    }
    let (prev, _, next) = letter_rects(c);
    if inside(&prev, x, y) {
        return Some(Hit::LetterPrev);
    }
    if inside(&next, x, y) {
        return Some(Hit::LetterNext);
    }
    if inside(&check_rect(c, 0), x, y) {
        return Some(Hit::Auto);
    }
    if inside(&check_rect(c, 1), x, y) {
        return Some(Hit::Remember);
    }
    for (i, h) in [Hit::Test, Hit::Save, Hit::Clear].into_iter().enumerate() {
        if inside(&action_rect(c, i as i32), x, y) {
            return Some(h);
        }
    }
    None
}

// ---------------------------------------------------------------- dessin

unsafe fn button(g: &mut Gfx, r: &Rect, label: &[u8], id: i32, primary: bool, on: bool) {
    if !shown(r) {
        return;
    }
    let p = pal();
    let bg = if primary {
        p.accent
    } else if on {
        p.selected
    } else if HOVER == id {
        p.hover
    } else {
        p.surface2
    };
    panel(
        g.hdc,
        g.b,
        r.left,
        r.top,
        r.right,
        r.bottom,
        6,
        bg,
        if on { p.accent } else { p.border },
    );
    let fg = if primary { p.on_accent } else { p.text };
    let mut end = label.len();
    while end > 0 && text_width(&label[..end], 11) > r.right - r.left - 10 {
        end -= 1;
    }
    text_centered(
        g.hdc,
        g.f.small,
        11,
        fg,
        (r.left + r.right) / 2,
        (r.top + r.bottom) / 2 - 7,
        &label[..end],
    );
}

fn human_size(mut v: u64, out: &mut [u8; 24]) -> usize {
    let units: [&[u8]; 5] = [b" o", b" Ko", b" Mo", b" Go", b" To"];
    let mut u = 0;
    while v >= 10 * 1024 && u < units.len() - 1 {
        v /= 1024;
        u += 1;
    }
    let mut digits = [0u8; 20];
    let mut n = 0;
    loop {
        digits[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    let mut len = 0;
    for i in (0..n).rev() {
        out[len] = digits[i];
        len += 1;
    }
    for &c in units[u] {
        out[len] = c;
        len += 1;
    }
    len
}

fn parse_u64(s: &[u8]) -> u64 {
    s.iter()
        .take_while(|c| c.is_ascii_digit())
        .fold(0u64, |n, &c| n.saturating_mul(10).saturating_add((c - b'0') as u64))
}

unsafe fn draw_drive(g: &mut Gfx, c: &Rect, i: usize, l: u8) {
    let p = pal();
    let r = drive_row(c, i);
    if !shown(&r) {
        return;
    }
    let key = [b'd', b'r', b'i', b'v', b'e', b'.', l];
    let line = field(status(), &key);
    let parts: [&[u8]; 11] = {
        let mut a: [&[u8]; 11] = [b""; 11];
        for (k, part) in line.split(|&c| c == b'|').take(11).enumerate() {
            a[k] = part;
        }
        a
    };
    let (name, proto, host, state, space) = (parts[0], parts[1], parts[2], parts[8], parts[9]);
    panel(g.hdc, g.b, r.left - 8, r.top, r.right + 8, r.bottom, 8, p.surface, p.border);
    glyph(g.hdc, g.b, g.f, Glyph::Drive, r.left + 4, r.top + 13, 30);
    // Petite prise réseau sous l'icône du disque.
    fill(g.hdc, g.b, r.left + 17, r.top + 41, r.left + 21, r.top + 47, p.text2);
    fill(g.hdc, g.b, r.left + 9, r.top + 47, r.left + 29, r.top + 49, p.text2);
    let mut title = [0u8; 96];
    let mut n = 0;
    for &ch in name.iter().take(80).chain(b" (").chain(&[l, b':', b')']) {
        title[n] = ch;
        n += 1;
    }
    let tx = r.left + 46;
    let (toggle, remove) = drive_buttons(&r);
    let max_w = toggle.left - tx - 10;
    text_fit(g.hdc, g.f.semibold, 13, p.text, tx, r.top + 9, &title[..n], max_w);
    let (state_txt, color): (&[u8], u32) = match state {
        b"ok" => (b"Connect\xE9", rgb(0x2E, 0xA0, 0x43)),
        b"connecting" => (b"Connexion en cours\x85", p.accent),
        b"offline" => (b"Hors ligne : reconnexion automatique", rgb(0xD1, 0x34, 0x38)),
        b"password" => (b"Mot de passe requis", rgb(0xC1, 0x7C, 0x00)),
        _ => (b"D\xE9connect\xE9", p.text3),
    };
    let mut sub = [0u8; 200];
    let mut m = 0;
    let mut push = |s: &[u8]| {
        for &ch in s {
            if m < sub.len() {
                sub[m] = ch;
                m += 1;
            }
        }
    };
    push(state_txt);
    push(b"  \xB7  ");
    let mut up = [0u8; 8];
    for (k, &ch) in proto.iter().take(8).enumerate() {
        up[k] = ch.to_ascii_uppercase();
    }
    push(&up[..proto.len().min(8)]);
    push(b" ");
    push(host);
    if let Some(slash) = space.iter().position(|&c| c == b'/') {
        let mut a = [0u8; 24];
        let mut t = [0u8; 24];
        let na = human_size(parse_u64(&space[..slash]), &mut a);
        let nt = human_size(parse_u64(&space[slash + 1..]), &mut t);
        push(b"  \xB7  ");
        push(&a[..na]);
        push(b" libres sur ");
        push(&t[..nt]);
    }
    fill(g.hdc, g.b, tx, r.top + 36, tx + 8, r.top + 44, color);
    text_fit(g.hdc, g.f.small, 11, p.text2, tx + 14, r.top + 32, &sub[..m], max_w - 14);
    let connected = state == b"ok" || state == b"connecting";
    button(
        g,
        &toggle,
        if connected { b"D\xE9connecter" } else { b"Connecter" },
        hit_id(Hit::Toggle(i)),
        false,
        false,
    );
    button(g, &remove, b"Supprimer", hit_id(Hit::Remove(i)), false, false);
}

unsafe fn draw_field(g: &mut Gfx, c: &Rect, i: usize) {
    let p = pal();
    let r = field_rect(c, i);
    if !shown(&rect(r.left, r.top - 20, r.right, r.bottom)) {
        return;
    }
    let s3 = PROTO < PROTOS.len() && PROTOS[PROTO].0 == b"s3";
    let smb = PROTO < PROTOS.len() && PROTOS[PROTO].0 == b"smb";
    let label: &[u8] = match i {
        F_HOST => b"Adresse du serveur",
        F_PORT => b"Port",
        F_USER if s3 => b"Cl\xE9 d'acc\xE8s",
        F_USER => b"Identifiant (vide = anonyme)",
        F_PASS if s3 => b"Cl\xE9 secr\xE8te",
        F_PASS => b"Mot de passe",
        F_PATH if s3 => b"Bucket et dossier (bucket/dossier)",
        F_PATH if smb => b"Partage et dossier (Partage/dossier)",
        F_PATH => b"Dossier distant (facultatif)",
        _ => b"Nom affich\xE9 dans Ce PC",
    };
    text(g.hdc, g.f.small, p.text2, r.left, r.top - 19, label);
    let focused = FOCUS == i;
    panel(
        g.hdc,
        g.b,
        r.left,
        r.top,
        r.right,
        r.bottom,
        5,
        p.window,
        if focused {
            p.accent
        } else if HOVER == hit_id(Hit::Field(i)) {
            p.text3
        } else {
            p.border
        },
    );
    if focused {
        fill(g.hdc, g.b, r.left + 4, r.bottom - 2, r.right - 4, r.bottom, p.accent);
    }
    let mut shown = [0u8; FIELD_CAP + 1];
    let mut n = FLEN[i];
    if i == F_PASS {
        for b in shown.iter_mut().take(n) {
            *b = 0x95; // puce
        }
    } else {
        shown[..n].copy_from_slice(get(i));
    }
    // Faire défiler le texte pour garder la fin (et le curseur) visible.
    let mut start = 0;
    while start < n && text_width(&shown[start..n], 13) > r.right - r.left - 22 {
        start += 1;
    }
    if focused && GetTickCount() / 530 % 2 == 0 {
        shown[n] = b'|';
        n += 1;
    }
    if FLEN[i] == 0 && !focused {
        let hint: &[u8] = match i {
            F_HOST => b"ex. 192.168.1.20 ou nas.local",
            F_PATH if smb => b"Documents",
            F_PATH => b"/",
            _ => b"",
        };
        text(g.hdc, g.f.body, p.text3, r.left + 10, r.top + 8, hint);
    } else {
        text(g.hdc, g.f.body, p.text, r.left + 10, r.top + 8, &shown[start..n]);
    }
}

unsafe fn checkbox(g: &mut Gfx, r: &Rect, on: bool, label: &[u8], id: i32) {
    if !shown(r) {
        return;
    }
    let p = pal();
    panel(
        g.hdc,
        g.b,
        r.left,
        r.top + 2,
        r.left + 20,
        r.top + 22,
        4,
        if on { p.accent } else { p.window },
        if on || HOVER == id { p.accent } else { p.text3 },
    );
    if on {
        text(g.hdc, g.f.small, p.on_accent, r.left + 5, r.top + 4, b"\xD7");
    }
    text(g.hdc, g.f.body, p.text, r.left + 28, r.top + 3, label);
}

pub unsafe fn draw(g: &mut Gfx, real: &Rect) {
    let p = pal();
    let vc = virtual_rect(real);
    let c = &vc;
    // Hauteur totale : action + message ; le reste se fait défiler.
    MAX_OFFSET = (action_rect(c, 0).bottom + 40 - c.top) - (real.bottom - real.top);
    if shown(&rect(c.left, c.top + 44, c.right, c.top + 60)) {
        text(
            g.hdc,
            g.f.body,
            p.text2,
            c.left,
            c.top + 44,
            b"Ajoutez un serveur distant : il appara\xEEt dans Ce PC comme un disque, avec sa lettre.",
        );
    }
    let list = drives();
    if list.is_empty() && shown(&drive_row(c, 0)) {
        let r = drive_row(c, 0);
        panel(g.hdc, g.b, r.left - 8, r.top, r.right + 8, r.bottom, 8, p.surface, p.border);
        text(
            g.hdc,
            g.f.body,
            p.text3,
            r.left + 6,
            r.top + 20,
            b"Aucun lecteur r\xE9seau pour l'instant.",
        );
    }
    for (i, &l) in list.iter().enumerate() {
        draw_drive(g, c, i, l);
    }

    let top = form_top(c);
    if shown(&rect(c.left, top, c.right, top + 18)) {
        text(
            g.hdc,
            g.f.semibold,
            p.text,
            c.left,
            top,
            if EDITING != 0 {
                b"Modifier le lecteur r\xE9seau"
            } else {
                b"Connecter un lecteur r\xE9seau"
            },
        );
    }
    if PROTO >= PROTOS.len() || !offered(PROTO) {
        // Premier type disponible, SFTP de préférence.
        let pick = (0..PROTOS.len())
            .find(|&i| PROTOS[i].0 == b"sftp" && offered(i))
            .or_else(|| (0..PROTOS.len()).find(|&i| offered(i)));
        if let Some(i) = pick {
            select_proto(i);
        }
    }
    let mut slot = 0;
    for i in 0..PROTOS.len() {
        if !offered(i) {
            continue;
        }
        let r = proto_rect(c, slot);
        button(g, &r, PROTOS[i].1, hit_id(Hit::Proto(i)), false, PROTO == i);
        slot += 1;
    }
    if slot == 0 && shown(&rect(c.left, top + 34, c.right, top + 50)) {
        text(
            g.hdc,
            g.f.body,
            p.text2,
            c.left,
            top + 34,
            if STATUS_LEN == 0 {
                b"Recherche des types de connexion disponibles\x85".as_slice()
            } else {
                b"Aucun outil de montage r\xE9seau n'est install\xE9 sur ce syst\xE8me.".as_slice()
            },
        );
    }
    if slot == 0 {
        return;
    }
    for i in 0..FIELDS {
        draw_field(g, c, i);
    }
    let (prev, mid, next) = letter_rects(c);
    if shown(&rect(prev.left, prev.top - 20, next.right, next.bottom)) {
        text(g.hdc, g.f.small, p.text2, prev.left, prev.top - 19, b"Lettre");
        button(g, &prev, b"<", hit_id(Hit::LetterPrev), false, false);
        panel(g.hdc, g.b, mid.left, mid.top, mid.right, mid.bottom, 5, p.window, p.border);
        text_centered(
            g.hdc,
            g.f.semibold,
            13,
            p.text,
            (mid.left + mid.right) / 2,
            mid.top + 8,
            &[letter(), b':'],
        );
        button(g, &next, b">", hit_id(Hit::LetterNext), false, false);
    }
    checkbox(
        g,
        &check_rect(c, 0),
        AUTO,
        AUTO_LABEL,
        hit_id(Hit::Auto),
    );
    checkbox(
        g,
        &check_rect(c, 1),
        REMEMBER,
        REMEMBER_LABEL,
        hit_id(Hit::Remember),
    );
    button(
        g,
        &action_rect(c, 0),
        b"Tester la connexion",
        hit_id(Hit::Test),
        false,
        false,
    );
    button(
        g,
        &action_rect(c, 1),
        if EDITING != 0 {
            b"Enregistrer et reconnecter"
        } else {
            b"Enregistrer et connecter"
        },
        hit_id(Hit::Save),
        true,
        false,
    );
    button(
        g,
        &action_rect(c, 2),
        b"Effacer le formulaire",
        hit_id(Hit::Clear),
        false,
        false,
    );
    let msg_y = action_rect(c, 0).bottom + 12;
    if !shown(&rect(c.left, msg_y, c.right, msg_y + 18)) {
        return;
    }
    if PENDING {
        text(g.hdc, g.f.body, p.accent, c.left, msg_y, b"Connexion au serveur en cours\x85");
    } else {
        let msg = field(status(), b"message");
        if !msg.is_empty() {
            let ok = field(status(), b"result") == b"ok";
            text_fit(
                g.hdc,
                g.f.body,
                13,
                if ok { rgb(0x2E, 0xA0, 0x43) } else { rgb(0xD1, 0x34, 0x38) },
                c.left,
                msg_y,
                msg,
                c.right - c.left,
            );
        }
    }
}

// ---------------------------------------------------------------- saisie

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, real: &Rect) -> bool {
    if let Mouse::Wheel(delta) = ev {
        let before = OFFSET;
        OFFSET = (OFFSET - delta.signum() * 48).clamp(0, MAX_OFFSET.max(0));
        return OFFSET != before;
    }
    let vc = virtual_rect(real);
    let c = &vc;
    let h = hit(c, x, y);
    if ev == Mouse::Move {
        let id = h.map(hit_id).unwrap_or(-1);
        let changed = id != HOVER;
        HOVER = id;
        return changed;
    }
    if ev != Mouse::Down && ev != Mouse::DoubleClick {
        return false;
    }
    let Some(h) = h else {
        return false;
    };
    match h {
        Hit::Proto(i) => select_proto(i),
        Hit::Field(i) => FOCUS = i,
        Hit::LetterPrev | Hit::LetterNext => {
            let mut l = letter();
            loop {
                l = if h == Hit::LetterNext {
                    if l >= b'Y' { b'D' } else { l + 1 }
                } else if l <= b'D' {
                    b'Y'
                } else {
                    l - 1
                };
                let taken = drives().contains(&l) && l != EDITING;
                if !taken || l == LETTER {
                    break;
                }
            }
            LETTER = l;
        }
        Hit::Auto => AUTO = !AUTO,
        Hit::Remember => REMEMBER = !REMEMBER,
        Hit::Test => submit(c"test"),
        Hit::Save => submit(c"add"),
        Hit::Clear => clear_form(),
        Hit::Toggle(i) => {
            let l = drives()[i];
            let key = [b'd', b'r', b'i', b'v', b'e', b'.', l];
            let line = field(status(), &key);
            let connected = line.ends_with(b"|ok")
                || line.windows(4).any(|w| w == b"|ok|")
                || line.windows(12).any(|w| w == b"|connecting|");
            let mut args = *b"connect X\0\0\0\0";
            if connected {
                args[..13].copy_from_slice(b"disconnect X\0");
                args[11] = l;
            } else {
                args[8] = l;
                // Mot de passe saisi pour ce lecteur : on le joint à la demande.
                if EDITING == l && FLEN[F_PASS] > 0 {
                    write_request();
                    set_field(F_PASS, b"");
                }
            }
            if let Ok(s) = core::ffi::CStr::from_bytes_until_nul(&args) {
                run(s);
            }
            if !connected && field(status(), &key).windows(10).any(|w| w == b"|password|") {
                edit(l);
            }
        }
        Hit::Remove(i) => {
            let l = drives()[i];
            let mut args = *b"remove X\0";
            args[7] = l;
            if let Ok(s) = core::ffi::CStr::from_bytes_until_nul(&args) {
                run(s);
            }
            if EDITING == l {
                clear_form();
            }
        }
    }
    true
}

pub unsafe fn key(vk: u32, ch: u16) -> bool {
    if vk == 0x2E && ch == 0 {
        // Suppr : vide le champ.
        FLEN[FOCUS] = 0;
        return true;
    }
    match ch {
        0 => false,
        8 => {
            FLEN[FOCUS] = FLEN[FOCUS].saturating_sub(1);
            true
        }
        9 => {
            FOCUS = (FOCUS + 1) % FIELDS;
            true
        }
        13 => {
            submit(c"add");
            true
        }
        27 => {
            clear_form();
            true
        }
        0x20..=0xFF => {
            let b = ch as u8;
            if FOCUS == F_PORT && !b.is_ascii_digit() {
                return true;
            }
            if FLEN[FOCUS] < FIELD_CAP {
                FIELD[FOCUS][FLEN[FOCUS]] = b;
                FLEN[FOCUS] += 1;
            }
            true
        }
        _ => false,
    }
}
