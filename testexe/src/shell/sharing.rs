//! Partage et transfert : dossiers partagés sur le réseau (SMB) et transfert
//! des réglages d'un PC Winlux à un autre par fichier.
//!
//! - Paramètres > Partage et transfert (`settings::PAGE_SHARING`) : liste des
//!   partages, droits par compte, accès invité, interrupteur général, puis
//!   export et import du fichier `.wlxprofile`.
//! - Explorateur > clic droit sur un dossier > « Partager ce dossier… » :
//!   petit panneau qui partage le dossier et règle les mêmes droits.
//!
//! Rien n'est fait ici : les actions passent par `winlux-control share-*`
//! (script `winlux-share`, serveur Samba) et `winlux-control transfer-*`
//! (script `winlux-transfer`), qui publient leur état dans
//! `Z:\run\winlux\shares.txt` et `Z:\run\winlux\transfer.txt`. Si Samba
//! n'est pas installé, `available=0` et tout ce qui concerne le partage est
//! caché.

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

const CAP: usize = 8192;
const MAX_HITS: usize = 64;

struct Text {
    bytes: [u8; CAP],
    len: usize,
}
static mut SHARES: Text = Text {
    bytes: [0; CAP],
    len: 0,
};
static mut TRANSFER: Text = Text {
    bytes: [0; CAP],
    len: 0,
};
static mut LAST_POLL: u32 = 0;
static mut ASKED: bool = false;
static mut LOADED: bool = false;
static mut PENDING: u32 = 0;

#[derive(Clone, Copy, PartialEq)]
enum Act {
    Toggle,
    Guest(u32),
    User(u32, usize),
    Stop(u32),
    Export(usize),
    Import(usize),
    Undo,
    DlgShare,
    DlgClose,
}
#[derive(Clone, Copy)]
struct Hit {
    r: Rect,
    act: Act,
}
const NO_HIT: Hit = Hit {
    r: Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    },
    act: Act::DlgClose,
};
static mut HITS: [Hit; MAX_HITS] = [NO_HIT; MAX_HITS];
static mut NHITS: usize = 0;
static mut DLG_HITS: [Hit; MAX_HITS] = [NO_HIT; MAX_HITS];
static mut DLG_NHITS: usize = 0;
static mut HOVER: Option<Act> = None;
static mut OFFSET: i32 = 0;
static mut MAX_OFFSET: i32 = 0;
static mut VIEW: (i32, i32) = (0, i32::MAX);

/// Panneau de l'Explorateur : dossier visé (chemin Windows, ANSI).
static mut DLG_OPEN: bool = false;
static mut DLG_PATH: [u8; 600] = [0; 600];
static mut DLG_LEN: usize = 0;

// ------------------------------------------------------------ libellés

const LEVELS: [Tr4; 3] = [
    [
        b"Aucun acc\xE8s",
        b"No access",
        b"Sin acceso",
        b"Kein Zugriff",
    ],
    [b"Lecture", b"Read", b"Lectura", b"Lesen"],
    [b"Modification", b"Change", b"Modificaci\xF3n", b"\xC4ndern"],
];
const GUESTS: Tr4 = [b"Invit\xE9s", b"Guests", b"Invitados", b"G\xE4ste"];
const NO_PASSWORD: Tr4 = [
    b"sans mot de passe",
    b"no password",
    b"sin contrase\xF1a",
    b"kein Kennwort",
];
const STOP: Tr4 = [
    b"Arr\xEAter le partage",
    b"Stop sharing",
    b"Dejar de compartir",
    b"Freigabe beenden",
];

// ------------------------------------------------------------ état publié

fn field<'a>(data: &'a [u8], key: &[u8]) -> &'a [u8] {
    for line in data.split(|&c| c == b'\n') {
        if line.len() > key.len() && line.starts_with(key) && line[key.len()] == b'=' {
            return &line[key.len() + 1..];
        }
    }
    b""
}

/// Valeurs d'une clé répétée (`share=…`, `found=…`), dans l'ordre.
fn lines<'a>(data: &'a [u8], key: &'a [u8]) -> impl Iterator<Item = &'a [u8]> + 'a {
    data.split(|&c| c == b'\n').filter_map(move |line| {
        (line.len() > key.len() && line.starts_with(key) && line[key.len()] == b'=')
            .then(|| &line[key.len() + 1..])
    })
}

fn part(s: &[u8], i: usize) -> &[u8] {
    s.split(|&c| c == b'|').nth(i).unwrap_or(b"")
}

fn number(s: &[u8]) -> u32 {
    s.iter()
        .take_while(|c| c.is_ascii_digit())
        .fold(0u32, |n, &c| {
            n.wrapping_mul(10).wrapping_add((c - b'0') as u32)
        })
}

/// Lit un état UTF-8 et le convertit en Latin-1 pour TextOutA.
unsafe fn load(path: &core::ffi::CStr, into: *mut Text) -> bool {
    let into = &mut *into;
    let h = CreateFileA(
        path.as_ptr(),
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
    let mut raw = [0u8; CAP];
    let mut n = 0u32;
    ReadFile(
        h,
        raw.as_mut_ptr(),
        CAP as u32,
        &mut n,
        core::ptr::null_mut(),
    );
    CloseHandle(h);
    let text = core::str::from_utf8(&raw[..n as usize]).unwrap_or("");
    let mut out = [0u8; CAP];
    let mut len = 0;
    for ch in text.chars() {
        if len >= CAP {
            break;
        }
        out[len] = if (ch as u32) < 0x100 { ch as u8 } else { b'?' };
        len += 1;
    }
    let changed = len != into.len || out[..len] != into.bytes[..into.len];
    into.bytes[..len].copy_from_slice(&out[..len]);
    into.len = len;
    changed
}

unsafe fn shares() -> &'static [u8] {
    &SHARES.bytes[..SHARES.len]
}
unsafe fn transfer() -> &'static [u8] {
    &TRANSFER.bytes[..TRANSFER.len]
}

unsafe fn reload() -> bool {
    LOADED = true;
    let mut changed = load(c"Z:\\run\\winlux\\shares.txt", &raw mut SHARES);
    changed |= load(c"Z:\\run\\winlux\\transfer.txt", &raw mut TRANSFER);
    // Le script réécrit son état à la fin de chaque action.
    if changed {
        PENDING = 0;
    }
    changed
}

/// `winlux-control <args>` ; `dir` devient le dossier courant du script
/// (le dossier à partager, converti en chemin Linux par ShellExecute).
unsafe fn control(args: &[u8], dir: Option<&[u8]>) -> bool {
    let mut a = [0u8; 160];
    let n = args.len().min(a.len() - 1);
    a[..n].copy_from_slice(&args[..n]);
    let mut d = [0u8; 600];
    let dir_ptr = match dir {
        Some(p) => {
            let m = p.len().min(d.len() - 1);
            d[..m].copy_from_slice(&p[..m]);
            d.as_ptr()
        }
        None => core::ptr::null(),
    };
    let ok = ShellExecuteA(
        core::ptr::null_mut(),
        c"open".as_ptr(),
        c"winlux-control".as_ptr(),
        a.as_ptr() as *const i8,
        dir_ptr as *const i8,
        0,
    ) as isize
        > 32;
    if ok {
        PENDING = GetTickCount().max(1);
        LAST_POLL = 0;
    } else {
        crate::desktop::toast(tr4(&[
            b"Le service de contr\xF4le Winlux n'est pas disponible.",
            b"The Winlux control service is not available.",
            b"El servicio de control de Winlux no est\xE1 disponible.",
            b"Der Winlux-Steuerdienst ist nicht verf\xFCgbar.",
        ]));
    }
    ok
}

/// Vrai si le partage est possible sur cette machine (Samba installé).
pub unsafe fn available() -> bool {
    // Tant que l'état n'a pas été publié, on le relit (et on le demande une fois).
    if !LOADED || SHARES.len == 0 {
        reload();
        if SHARES.len == 0 && !ASKED {
            ASKED = true;
            control(b"share-status", None);
        }
    }
    field(shares(), b"available") == b"1"
}

/// Appelé à chaque tour des Paramètres.
pub unsafe fn poll(visible: bool) -> bool {
    if !visible {
        ASKED = false;
        return false;
    }
    if !ASKED {
        ASKED = true;
        control(b"share-status", None);
        control(b"transfer-status", None);
    }
    let now = GetTickCount();
    if LAST_POLL != 0 && now.wrapping_sub(LAST_POLL) < 400 {
        return false;
    }
    LAST_POLL = now;
    // Une action sans réponse au bout de 20 s n'est plus « en cours ».
    if PENDING != 0 && now.wrapping_sub(PENDING) > 20_000 {
        PENDING = 0;
    }
    reload()
}

// ------------------------------------------------------------ helpers

fn level_of(acl: &[u8], user: &[u8]) -> usize {
    for entry in acl.split(|&c| c == b',') {
        if entry.len() > user.len() && entry.starts_with(user) && entry[user.len()] == b':' {
            return match &entry[user.len() + 1..] {
                b"write" => 2,
                b"read" => 1,
                _ => 0,
            };
        }
    }
    0
}
fn level_word(l: usize) -> &'static [u8] {
    [b"none".as_slice(), b"read", b"write"][l.min(2)]
}
fn guest_level(s: &[u8]) -> usize {
    match s {
        b"write" => 2,
        b"read" => 1,
        _ => 0,
    }
}

struct Buf {
    b: [u8; 200],
    n: usize,
}
impl Buf {
    fn new() -> Buf {
        Buf { b: [0; 200], n: 0 }
    }
    fn push(&mut self, s: &[u8]) -> &mut Buf {
        let k = s.len().min(self.b.len() - self.n);
        self.b[self.n..self.n + k].copy_from_slice(&s[..k]);
        self.n += k;
        self
    }
    fn num(&mut self, mut v: u32) -> &mut Buf {
        let mut d = [0u8; 10];
        let mut i = d.len();
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
    fn get(&self) -> &[u8] {
        &self.b[..self.n]
    }
}
unsafe fn run_set(id: u32, user: &[u8], level: usize) {
    let mut c = Buf::new();
    c.push(b"share-set ")
        .num(id)
        .push(b" ")
        .push(user)
        .push(b" ")
        .push(level_word(level));
    control(c.get(), None);
}
unsafe fn run_id(verb: &[u8], id: u32, tail: &[u8]) {
    let mut c = Buf::new();
    c.push(verb).push(b" ").num(id).push(tail);
    control(c.get(), None);
}

fn eq_path(a: &[u8], b: &[u8]) -> bool {
    let trim = |s: &[u8]| -> usize {
        let mut n = s.len();
        while n > 3 && s[n - 1] == b'\\' {
            n -= 1;
        }
        n
    };
    let (na, nb) = (trim(a), trim(b));
    na == nb && a[..na].eq_ignore_ascii_case(&b[..nb])
}

/// Partage correspondant au dossier du panneau de l'Explorateur.
unsafe fn dialog_share() -> Option<&'static [u8]> {
    let path = &DLG_PATH[..DLG_LEN];
    lines(shares(), b"share").find(|s| eq_path(part(s, 2), path))
}

// ------------------------------------------------------------ dessin commun

fn shown(r: &Rect) -> bool {
    unsafe { r.top >= VIEW.0 && r.bottom <= VIEW.1 }
}

unsafe fn add_hit(dialog: bool, r: Rect, act: Act) {
    if dialog {
        if DLG_NHITS < MAX_HITS {
            DLG_HITS[DLG_NHITS] = Hit { r, act };
            DLG_NHITS += 1;
        }
    } else if NHITS < MAX_HITS && shown(&r) {
        HITS[NHITS] = Hit { r, act };
        NHITS += 1;
    }
}

/// Bouton (dessiné seulement s'il est visible) ; `act = None` : inactif.
unsafe fn button(
    g: &mut Gfx,
    r: Rect,
    label: &[u8],
    act: Option<Act>,
    primary: bool,
    dialog: bool,
) {
    if !dialog && !shown(&r) {
        return;
    }
    let p = pal();
    let hover = act.is_some() && HOVER == act;
    let bg = if primary {
        p.accent
    } else if hover {
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
        if hover { p.accent } else { p.border },
    );
    let fg = if primary {
        p.on_accent
    } else if act.is_none() {
        p.text3
    } else {
        p.text
    };
    let mut end = label.len();
    while end > 0 && text_width(&label[..end], 11) > r.right - r.left - 12 {
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
    if let Some(a) = act {
        add_hit(dialog, r, a);
    }
}

unsafe fn line(
    g: &mut Gfx,
    font: Handle,
    color: u32,
    x: i32,
    y: i32,
    s: &[u8],
    w: i32,
    dialog: bool,
) {
    if dialog || shown(&rect(x, y, x + 1, y + 16)) {
        text_fit(g.hdc, font, 11, color, x, y, s, w);
    }
}

/// Boutons de droits d'un partage, en lignes qui se replient ; renvoie le
/// bas de la dernière ligne.
unsafe fn rights(g: &mut Gfx, share: &[u8], left: i32, top: i32, right: i32, dialog: bool) -> i32 {
    let id = number(part(share, 0));
    let acl = part(share, 4);
    let bw = ((right - left - 16) / 3).clamp(120, 210);
    let (mut x, mut y) = (left, top);
    let mut place = |w: i32| -> Rect {
        if x + w > right && x > left {
            x = left;
            y += 38;
        }
        let r = rect(x, y, x + w, y + 30);
        x += w + 8;
        r
    };
    let mut label = Buf::new();
    label
        .push(tr4(&GUESTS))
        .push(b" : ")
        .push(tr4(&LEVELS[guest_level(part(share, 3))]));
    button(
        g,
        place(bw),
        label.get(),
        Some(Act::Guest(id)),
        false,
        dialog,
    );
    for (i, account) in lines(shares(), b"account").enumerate() {
        let name = part(account, 0);
        let mut label = Buf::new();
        label.push(name).push(b" : ");
        if part(account, 1) == b"1" {
            label.push(tr4(&LEVELS[level_of(acl, name)]));
            button(
                g,
                place(bw),
                label.get(),
                Some(Act::User(id, i)),
                false,
                dialog,
            );
        } else {
            label.push(tr4(&NO_PASSWORD));
            button(g, place(bw), label.get(), None, false, dialog);
        }
    }
    button(g, place(bw), tr4(&STOP), Some(Act::Stop(id)), false, dialog);
    y + 30
}

/// Chemin réseau `\\HOTE\partage` d'un partage.
fn unc(host: &[u8], name: &[u8]) -> Buf {
    let mut b = Buf::new();
    b.push(b"\\\\").push(host).push(b"\\").push(name);
    b
}

// ------------------------------------------------------------ page Paramètres

pub unsafe fn draw(g: &mut Gfx, real: &Rect) {
    let p = pal();
    NHITS = 0;
    VIEW = (real.top + 36, real.bottom - 4);
    OFFSET = OFFSET.clamp(0, MAX_OFFSET.max(0));
    let c = rect(
        real.left,
        real.top - OFFSET,
        real.right,
        real.bottom - OFFSET,
    );
    let w = c.right - c.left;
    let mut y = c.top + 44;
    let data = shares();

    if field(data, b"available") == b"1" {
        line(
            g,
            g.f.semibold,
            p.text,
            c.left,
            y,
            tr4(&[
                b"Dossiers partag\xE9s sur le r\xE9seau",
                b"Folders shared on the network",
                b"Carpetas compartidas en la red",
                b"Im Netzwerk freigegebene Ordner",
            ]),
            w,
            false,
        );
        y += 24;
        line(g, g.f.body, p.text2, c.left, y, tr4(&[
            b"Les PC Windows, les Mac et les autres Linux ouvrent ces dossiers avec un compte et un mot de passe Winlux.",
            b"Windows PCs, Macs and other Linux computers open these folders with a Winlux account and password.",
            b"Los PC con Windows, los Mac y otros Linux abren estas carpetas con una cuenta y contrase\xF1a de Winlux.",
            b"Windows-PCs, Macs und andere Linux-Rechner \xF6ffnen diese Ordner mit einem Winlux-Konto und Kennwort.",
        ]), w, false);
        y += 20;
        line(g, g.f.small, p.text3, c.left, y, tr4(&[
            b"Serveur SMB : Samba, l'outil standard de Linux. Ouvert sur les r\xE9seaux priv\xE9s seulement.",
            b"SMB server: Samba, the standard Linux tool. Open on private networks only.",
            b"Servidor SMB: Samba, la herramienta est\xE1ndar de Linux. Abierto solo en redes privadas.",
            b"SMB-Server: Samba, das Standardwerkzeug von Linux. Nur in privaten Netzwerken offen.",
        ]), w, false);
        y += 28;
        let enabled = field(data, b"enabled") == b"1";
        let any = lines(data, b"share").next().is_some();
        let host = field(data, b"host");
        let addr = field(data, b"address");
        let mut state = Buf::new();
        if !enabled {
            state.push(tr4(&[
                b"Partage d\xE9sactiv\xE9.",
                b"Sharing is off.",
                b"Uso compartido desactivado.",
                b"Freigabe ist aus.",
            ]));
        } else if !any {
            state.push(tr4(&[
                b"Aucun dossier partag\xE9 pour l'instant.",
                b"No shared folders yet.",
                b"Todav\xEDa no hay carpetas compartidas.",
                b"Noch keine freigegebenen Ordner.",
            ]));
        } else {
            state.push(tr4(&[
                b"Depuis un autre ordinateur, ouvrez ",
                b"From another computer, open ",
                b"Desde otro equipo, abra ",
                b"\xD6ffnen Sie auf einem anderen Computer ",
            ]));
            state.push(b"\\\\").push(host);
            if !addr.is_empty() {
                state
                    .push(tr4(&[b" ou ", b" or ", b" o ", b" oder "]))
                    .push(b"\\\\")
                    .push(addr);
            }
            if field(data, b"running") != b"1" && PENDING == 0 {
                state.push(tr4(&[
                    b" (serveur arr\xEAt\xE9)",
                    b" (server stopped)",
                    b" (servidor detenido)",
                    b" (Server gestoppt)",
                ]));
            }
        }
        let toggle = rect(c.right - 150, y - 4, c.right, y + 26);
        line(
            g,
            g.f.body,
            p.text,
            c.left,
            y + 2,
            state.get(),
            w - 166,
            false,
        );
        button(
            g,
            toggle,
            if enabled {
                tr4(&[b"D\xE9sactiver", b"Turn off", b"Desactivar", b"Ausschalten"])
            } else {
                tr4(&[b"Activer", b"Turn on", b"Activar", b"Einschalten"])
            },
            Some(Act::Toggle),
            !enabled,
            false,
        );
        y += 36;
        let error = field(data, b"error");
        if !error.is_empty() {
            line(g, g.f.body, p.accent, c.left, y, error, w, false);
            y += 24;
        }
        if field(data, b"firewall_profile") == b"public" {
            line(g, g.f.body, p.accent, c.left, y, tr4(&[
                b"R\xE9seau public : le pare-feu bloque le partage. Choisissez le profil Priv\xE9 dans le Pare-feu.",
                b"Public network: the firewall blocks sharing. Choose the Private profile in Firewall.",
                b"Red p\xFAblica: el cortafuegos bloquea el uso compartido. Elija el perfil Privado en el Cortafuegos.",
                b"\xD6ffentliches Netzwerk: Die Firewall blockiert die Freigabe. W\xE4hlen Sie in der Firewall das Profil \x84Privat\x93.",
            ]), w, false);
            y += 24;
        }
        if enabled {
            for share in lines(data, b"share") {
                let rows_h = {
                    // Hauteur de la carte : nombre de lignes de boutons.
                    let bw = ((w - 32 - 16) / 3).clamp(120, 210);
                    let per = ((w - 32 + 8) / (bw + 8)).max(1);
                    let n = 2 + lines(data, b"account").count() as i32;
                    (n + per - 1) / per
                };
                let card = rect(c.left, y, c.right, y + 58 + rows_h * 38);
                // Carte coupée par le défilement : seule sa partie visible.
                let (top, bottom) = (card.top.max(VIEW.0), card.bottom.min(VIEW.1));
                if bottom - top > 16 {
                    panel(
                        g.hdc,
                        g.b,
                        card.left,
                        top,
                        card.right,
                        bottom,
                        8,
                        p.surface,
                        p.border,
                    );
                }
                let mut title = unc(host, part(share, 1));
                if part(share, 5) == b"0" {
                    title.push(tr4(&[
                        b"  (dossier introuvable)",
                        b"  (folder missing)",
                        b"  (carpeta no encontrada)",
                        b"  (Ordner fehlt)",
                    ]));
                }
                line(
                    g,
                    g.f.semibold,
                    p.text,
                    card.left + 16,
                    card.top + 12,
                    title.get(),
                    w - 32,
                    false,
                );
                line(
                    g,
                    g.f.small,
                    p.text2,
                    card.left + 16,
                    card.top + 32,
                    part(share, 2),
                    w - 32,
                    false,
                );
                rights(
                    g,
                    share,
                    card.left + 16,
                    card.top + 54,
                    card.right - 16,
                    false,
                );
                y = card.bottom + 10;
            }
        }
        line(g, g.f.small, p.text3, c.left, y + 2, tr4(&[
            b"Pour partager un dossier : clic droit dessus dans l'Explorateur, puis \xAB Partager ce dossier \xBB.",
            b"To share a folder: right-click it in File Explorer, then \x93Share this folder\x94.",
            b"Para compartir una carpeta: clic derecho en el Explorador y \xABCompartir esta carpeta\xBB.",
            b"Ordner freigeben: im Explorer mit der rechten Maustaste anklicken, dann \x84Diesen Ordner freigeben\x93.",
        ]), w, false);
        y += 46;
    }

    // ---- Transfert des réglages
    let t = transfer();
    line(
        g,
        g.f.semibold,
        p.text,
        c.left,
        y,
        tr4(&[
            b"Transf\xE9rer mes r\xE9glages vers un autre PC Winlux",
            b"Move my settings to another Winlux PC",
            b"Transferir mis ajustes a otro PC con Winlux",
            b"Meine Einstellungen auf einen anderen Winlux-PC \xFCbertragen",
        ]),
        w,
        false,
    );
    y += 24;
    line(g, g.f.body, p.text2, c.left, y, tr4(&[
        b"Th\xE8me, barre des t\xE2ches, fond, lecteurs r\xE9seau (sans mot de passe), applis par d\xE9faut, langue et clavier.",
        b"Theme, taskbar, background, network drives (no passwords), default apps, language and keyboard.",
        b"Tema, barra de tareas, fondo, unidades de red (sin contrase\xF1as), apps predeterminadas, idioma y teclado.",
        b"Design, Taskleiste, Hintergrund, Netzlaufwerke (ohne Kennw\xF6rter), Standard-Apps, Sprache und Tastatur.",
    ]), w, false);
    y += 20;
    line(g, g.f.small, p.text3, c.left, y, tr4(&[
        b"Un simple fichier .wlxprofile, \xE0 copier sur une cl\xE9 USB ou un partage : aucun compte en ligne.",
        b"A plain .wlxprofile file to copy to a USB stick or a share: no online account.",
        b"Un simple archivo .wlxprofile para copiar a una memoria USB o a un recurso compartido: sin cuenta en l\xEDnea.",
        b"Eine einfache .wlxprofile-Datei f\xFCr USB-Stick oder Freigabe: kein Onlinekonto.",
    ]), w, false);
    y += 28;
    let mut x = c.left;
    for (i, target) in lines(t, b"target").enumerate().take(6) {
        let mut label = Buf::new();
        if part(target, 0) == b"documents" {
            label.push(tr4(&[
                b"Enregistrer dans Documents",
                b"Save to Documents",
                b"Guardar en Documentos",
                b"In Dokumenten speichern",
            ]));
        } else {
            label
                .push(tr4(&[
                    b"Enregistrer sur ",
                    b"Save to ",
                    b"Guardar en ",
                    b"Speichern auf ",
                ]))
                .push(part(target, 1));
        }
        let bw = (text_width(label.get(), 11) + 28).clamp(160, 300);
        if x + bw > c.right && x > c.left {
            x = c.left;
            y += 38;
        }
        button(
            g,
            rect(x, y, x + bw, y + 30),
            label.get(),
            Some(Act::Export(i)),
            i == 0,
            false,
        );
        x += bw + 8;
    }
    y += 46;
    line(
        g,
        g.f.semibold,
        p.text,
        c.left,
        y,
        tr4(&[
            b"Fichiers de r\xE9glages trouv\xE9s",
            b"Settings files found",
            b"Archivos de ajustes encontrados",
            b"Gefundene Einstellungsdateien",
        ]),
        w,
        false,
    );
    y += 26;
    let mut none = true;
    for (i, found) in lines(t, b"found").enumerate().take(8) {
        none = false;
        let row = rect(c.left, y, c.right, y + 46);
        if shown(&row) {
            panel(
                g.hdc, g.b, row.left, row.top, row.right, row.bottom, 6, p.surface, p.border,
            );
        }
        let path = part(found, 0);
        let name = path.rsplit(|&b| b == b'\\').next().unwrap_or(path);
        line(
            g,
            g.f.body,
            p.text,
            row.left + 12,
            row.top + 6,
            name,
            w - 160,
            false,
        );
        let mut from = Buf::new();
        from.push(part(found, 1))
            .push(b"  \x95  ")
            .push(part(found, 2))
            .push(b"  \x95  ")
            .push(path);
        line(
            g,
            g.f.small,
            p.text3,
            row.left + 12,
            row.top + 26,
            from.get(),
            w - 160,
            false,
        );
        button(
            g,
            rect(row.right - 126, row.top + 8, row.right - 10, row.bottom - 8),
            tr4(&[b"Importer", b"Import", b"Importar", b"Importieren"]),
            Some(Act::Import(i)),
            false,
            false,
        );
        y += 52;
    }
    if none {
        line(g, g.f.body, p.text3, c.left, y, tr4(&[
            b"Aucun fichier .wlxprofile dans Documents, Bureau, T\xE9l\xE9chargements ni sur une cl\xE9 USB.",
            b"No .wlxprofile file in Documents, Desktop, Downloads or on a USB stick.",
            b"No hay archivos .wlxprofile en Documentos, Escritorio, Descargas ni en una memoria USB.",
            b"Keine .wlxprofile-Datei in Dokumente, Desktop, Downloads oder auf einem USB-Stick.",
        ]), w, false);
        y += 26;
    }
    if field(t, b"backup") == b"1" {
        y += 6;
        button(
            g,
            rect(c.left, y, c.left + 260, y + 30),
            tr4(&[
                b"Annuler le dernier import",
                b"Undo the last import",
                b"Deshacer la \xFAltima importaci\xF3n",
                b"Letzten Import r\xFCckg\xE4ngig machen",
            ]),
            Some(Act::Undo),
            false,
            false,
        );
        y += 40;
    }
    let last = field(t, b"last");
    let mut msg = Buf::new();
    match part(last, 0) {
        b"exported" => {
            msg.push(tr4(&[
                b"R\xE9glages enregistr\xE9s : ",
                b"Settings saved: ",
                b"Ajustes guardados: ",
                b"Einstellungen gespeichert: ",
            ]))
            .push(part(last, 1));
        }
        b"imported" => {
            msg.push(tr4(&[
                b"R\xE9glages import\xE9s. Fermez puis rouvrez la session pour tout appliquer.",
                b"Settings imported. Sign out and back in to apply everything.",
                b"Ajustes importados. Cierre y vuelva a abrir la sesi\xF3n para aplicarlo todo.",
                b"Einstellungen importiert. Ab- und wieder anmelden, um alles zu \xFCbernehmen.",
            ]));
        }
        b"restored" => {
            msg.push(tr4(&[
                b"Import annul\xE9 : vos r\xE9glages d'avant sont remis.",
                b"Import undone: your previous settings are back.",
                b"Importaci\xF3n deshecha: se restauraron sus ajustes anteriores.",
                b"Import r\xFCckg\xE4ngig gemacht: Ihre vorherigen Einstellungen sind zur\xFCck.",
            ]));
        }
        b"error" => {
            msg.push(tr4(&[b"\xC9chec : ", b"Failed: ", b"Error: ", b"Fehler: "]))
                .push(part(last, 1));
        }
        _ => {}
    }
    if PENDING != 0 {
        msg = Buf::new();
        msg.push(tr4(&[
            b"Op\xE9ration en cours\x85",
            b"Working\x85",
            b"Operaci\xF3n en curso\x85",
            b"Vorgang l\xE4uft\x85",
        ]));
    }
    line(g, g.f.body, p.text2, c.left, y, msg.get(), w, false);
    y += 30;
    MAX_OFFSET = (y + OFFSET) - real.bottom + 20;
}

unsafe fn perform(act: Act) {
    let data = shares();
    match act {
        Act::Toggle => {
            control(
                if field(data, b"enabled") == b"1" {
                    b"share-disable"
                } else {
                    b"share-enable"
                },
                None,
            );
        }
        Act::Guest(id) => {
            if let Some(s) = lines(data, b"share").find(|s| number(part(s, 0)) == id) {
                let next = (guest_level(part(s, 3)) + 1) % 3;
                let mut tail = Buf::new();
                tail.push(b" ").push(level_word(next));
                run_id(b"share-guest", id, tail.get());
            }
        }
        Act::User(id, i) => {
            let share = lines(data, b"share").find(|s| number(part(s, 0)) == id);
            let account = lines(data, b"account").nth(i);
            if let (Some(s), Some(a)) = (share, account) {
                let name = part(a, 0);
                run_set(id, name, (level_of(part(s, 4), name) + 1) % 3);
            }
        }
        Act::Stop(id) => run_id(b"share-remove", id, b""),
        Act::Export(i) => {
            let mut c = Buf::new();
            c.push(b"transfer-export ").num(i as u32);
            control(c.get(), None);
        }
        Act::Import(i) => {
            let mut c = Buf::new();
            c.push(b"transfer-import ").num(i as u32);
            control(c.get(), None);
        }
        Act::Undo => {
            control(b"transfer-undo", None);
        }
        Act::DlgShare => {
            let path = &DLG_PATH[..DLG_LEN];
            control(b"share-add", Some(path));
        }
        Act::DlgClose => DLG_OPEN = false,
    }
}

fn find(hits: &[Hit], x: i32, y: i32) -> Option<Act> {
    hits.iter()
        .find(|h| x >= h.r.left && x < h.r.right && y >= h.r.top && y < h.r.bottom)
        .map(|h| h.act)
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, _real: &Rect) -> bool {
    if let Mouse::Wheel(delta) = ev {
        let before = OFFSET;
        OFFSET = (OFFSET - delta.signum() * 48).clamp(0, MAX_OFFSET.max(0));
        return OFFSET != before;
    }
    let hit = find(&HITS[..NHITS], x, y);
    match ev {
        Mouse::Move => {
            let changed = hit != HOVER;
            HOVER = hit;
            changed
        }
        Mouse::Down | Mouse::DoubleClick => {
            if let Some(a) = hit {
                perform(a);
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

pub unsafe fn key(_vk: u32, _ch: u16) -> bool {
    false
}

// ------------------------------------------------------------ Explorateur

pub unsafe fn dialog_open() -> bool {
    DLG_OPEN
}

/// Ouvre le panneau « Partager ce dossier » (chemin Windows ANSI).
pub unsafe fn open_dialog(path: &[u8]) {
    let n = path
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(path.len())
        .min(DLG_PATH.len() - 1);
    DLG_PATH[..n].copy_from_slice(&path[..n]);
    DLG_LEN = n;
    DLG_OPEN = true;
    HOVER = None;
    reload();
}

unsafe fn dialog_box(f: &Frame) -> Rect {
    let c = f.client;
    let w = (c.right - c.left - 40).min(560);
    let rows = if dialog_share().is_some() {
        let bw = ((w - 40 - 16) / 3).clamp(120, 210);
        let per = ((w - 40 + 8) / (bw + 8)).max(1);
        let n = 2 + lines(shares(), b"account").count() as i32;
        (n + per - 1) / per
    } else {
        1
    };
    let h = (150 + rows * 38).min(c.bottom - c.top - 20);
    let left = (c.left + c.right - w) / 2;
    let top = (c.top + c.bottom - h) / 2;
    rect(left, top, left + w, top + h)
}

pub unsafe fn draw_dialog(g: &mut Gfx, f: &Frame) {
    if !DLG_OPEN {
        return;
    }
    DLG_NHITS = 0;
    let p = pal();
    let r = dialog_box(f);
    round_rect(
        g.hdc,
        g.b,
        r.left - 1,
        r.top - 1,
        r.right + 1,
        r.bottom + 1,
        9,
        p.border,
    );
    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 8, p.surface);
    let path = &DLG_PATH[..DLG_LEN];
    let name = {
        let mut n = path.len();
        while n > 3 && path[n - 1] == b'\\' {
            n -= 1;
        }
        path[..n].rsplit(|&b| b == b'\\').next().unwrap_or(path)
    };
    let w = r.right - r.left - 40;
    let mut title = Buf::new();
    title
        .push(tr4(&[
            b"Partager \xAB ",
            b"Share \x93",
            b"Compartir \xAB",
            b"\x84",
        ]))
        .push(name);
    title.push(tr4(&[b" \xBB", b"\x94", b"\xBB", b"\x93 freigeben"]));
    line(
        g,
        g.f.semibold,
        p.text,
        r.left + 20,
        r.top + 18,
        title.get(),
        w,
        true,
    );
    let close = rect(r.right - 120, r.bottom - 46, r.right - 20, r.bottom - 16);
    if let Some(share) = dialog_share() {
        let host = field(shares(), b"host");
        let mut path_line = Buf::new();
        path_line.push(tr4(&[
            b"Chemin r\xE9seau : ",
            b"Network path: ",
            b"Ruta de red: ",
            b"Netzwerkpfad: ",
        ]));
        path_line.push(unc(host, part(share, 1)).get());
        line(
            g,
            g.f.body,
            p.text,
            r.left + 20,
            r.top + 46,
            path_line.get(),
            w,
            true,
        );
        line(g, g.f.small, p.text3, r.left + 20, r.top + 68, tr4(&[
            b"Cliquez sur un compte pour passer de Aucun acc\xE8s \xE0 Lecture puis \xE0 Modification.",
            b"Click an account to switch between No access, Read and Change.",
            b"Haga clic en una cuenta para pasar de Sin acceso a Lectura y Modificaci\xF3n.",
            b"Klicken Sie auf ein Konto, um zwischen Kein Zugriff, Lesen und \xC4ndern zu wechseln.",
        ]), w, true);
        rights(g, share, r.left + 20, r.top + 92, r.right - 20, true);
        button(
            g,
            close,
            tr4(&[b"Fermer", b"Close", b"Cerrar", b"Schlie\xDFen"]),
            Some(Act::DlgClose),
            true,
            true,
        );
    } else if PENDING != 0 {
        line(
            g,
            g.f.body,
            p.text2,
            r.left + 20,
            r.top + 52,
            tr4(&[
                b"Partage en cours\x85",
                b"Sharing\x85",
                b"Compartiendo\x85",
                b"Wird freigegeben\x85",
            ]),
            w,
            true,
        );
        button(
            g,
            close,
            tr4(&[b"Fermer", b"Close", b"Cerrar", b"Schlie\xDFen"]),
            Some(Act::DlgClose),
            false,
            true,
        );
    } else {
        line(g, g.f.body, p.text2, r.left + 20, r.top + 50, tr4(&[
            b"Ce dossier sera visible des PC Windows, des Mac et des Linux du r\xE9seau.",
            b"This folder will be visible to Windows PCs, Macs and Linux computers on the network.",
            b"Esta carpeta ser\xE1 visible para los PC con Windows, los Mac y los Linux de la red.",
            b"Dieser Ordner wird f\xFCr Windows-PCs, Macs und Linux-Rechner im Netzwerk sichtbar.",
        ]), w, true);
        let error = field(shares(), b"error");
        if !error.is_empty() {
            line(
                g,
                g.f.small,
                p.accent,
                r.left + 20,
                r.top + 72,
                error,
                w,
                true,
            );
        } else {
            line(g, g.f.small, p.text3, r.left + 20, r.top + 72, tr4(&[
            b"Par d\xE9faut, vous le modifiez et les autres comptes Winlux le lisent. Aucun acc\xE8s invit\xE9.",
            b"By default you can change it and other Winlux accounts can read it. No guest access.",
            b"Por defecto usted la modifica y las dem\xE1s cuentas de Winlux la leen. Sin acceso de invitados.",
            b"Standard: Sie k\xF6nnen \xE4ndern, andere Winlux-Konten lesen. Kein Gastzugriff.",
        ]), w, true);
        }
        let share = rect(close.left - 140, close.top, close.left - 10, close.bottom);
        button(
            g,
            share,
            tr4(&[b"Partager", b"Share", b"Compartir", b"Freigeben"]),
            Some(Act::DlgShare),
            true,
            true,
        );
        button(
            g,
            close,
            tr4(&[b"Annuler", b"Cancel", b"Cancelar", b"Abbrechen"]),
            Some(Act::DlgClose),
            false,
            true,
        );
    }
}

/// Le panneau garde la main tant qu'il est ouvert (comme une boîte modale).
pub unsafe fn dialog_mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    let hit = find(&DLG_HITS[..DLG_NHITS], x, y);
    match ev {
        Mouse::Move => {
            let changed = hit != HOVER;
            HOVER = hit;
            changed
        }
        Mouse::Down | Mouse::DoubleClick => {
            let r = dialog_box(f);
            if let Some(a) = hit {
                perform(a);
            } else if x < r.left || x >= r.right || y < r.top || y >= r.bottom {
                DLG_OPEN = false;
            }
            true
        }
        _ => true,
    }
}

pub unsafe fn dialog_key(_vk: u32, ch: u16) -> bool {
    match ch {
        27 => DLG_OPEN = false,
        13 => {
            if dialog_share().is_none() && PENDING == 0 {
                perform(Act::DlgShare);
            } else {
                DLG_OPEN = false;
            }
        }
        _ => {}
    }
    true
}

/// Rafraîchit l'état pendant que le panneau est ouvert.
pub unsafe fn dialog_tick() -> bool {
    if !DLG_OPEN {
        return false;
    }
    let now = GetTickCount();
    if LAST_POLL != 0 && now.wrapping_sub(LAST_POLL) < 400 {
        return false;
    }
    LAST_POLL = now;
    if PENDING != 0 && now.wrapping_sub(PENDING) > 20_000 {
        PENDING = 0;
    }
    reload()
}
