//! Accessibilité du bureau : Loupe, Narrateur, touches rémanentes et
//! filtres, sous-titres des sons système.
//!
//! Les réglages vivent dans `gfx::A11Y` (fichier partagé avec la page
//! Paramètres > Accessibilité et le runtime). Ce module applique ce qui
//! dépend du bureau : rendu de la loupe, synthèse vocale par
//! `winlux-speak` (espeak-ng ou piper) et filtrage du clavier.

use crate::gfx::*;
use winlux_testexe::*;

fn now() -> u32 {
    unsafe { GetTickCount() }
}
fn since(t: u32) -> u32 {
    now().wrapping_sub(t)
}

static mut MOUSE_X: i32 = 0;
static mut MOUSE_Y: i32 = 0;
static mut SCREEN: (i32, i32) = (1280, 800);

// ---------- Synthèse vocale ----------

static mut LAST_SPOKEN: [u8; 96] = [0; 96];
static mut LAST_SPOKEN_LEN: usize = 0;
static mut LAST_SPOKEN_AT: u32 = 0;

/// Prépare les paramètres de `winlux-speak` : débit puis texte entre
/// guillemets (les guillemets et barres obliques inverses du texte sont
/// remplacés pour respecter le découpage de ligne de commande Windows).
pub fn speech_command(text: &[u8], rate: u8, lang: &[u8], out: &mut [u8; 600]) -> usize {
    fn push(out: &mut [u8; 600], n: &mut usize, bytes: &[u8]) {
        for &c in bytes {
            if *n < out.len() - 2 {
                out[*n] = c;
                *n += 1;
            }
        }
    }
    let mut n = 0;
    push(out, &mut n, b"--lang ");
    push(out, &mut n, lang);
    push(out, &mut n, b" --rate ");
    let rate = rate.clamp(1, 10);
    if rate == 10 {
        push(out, &mut n, b"10");
    } else {
        push(out, &mut n, &[b'0' + rate]);
    }
    push(out, &mut n, b" \"");
    let mut last_space = true;
    for &c in text.iter().take(480) {
        let c = match c {
            b'"' | b'\\' | b'\r' | b'\n' | b'\t' => b' ',
            0..=0x1F => continue,
            c => c,
        };
        if c == b' ' && last_space {
            continue;
        }
        last_space = c == b' ';
        push(out, &mut n, &[c]);
    }
    while n > 0 && out[n - 1] == b' ' {
        n -= 1;
    }
    push(out, &mut n, b"\"");
    out[n] = 0;
    n
}

unsafe fn run_speak(params: &[u8]) {
    ShellExecuteA(
        core::ptr::null_mut(),
        c"open".as_ptr(),
        c"winlux-speak".as_ptr(),
        params.as_ptr().cast(),
        core::ptr::null(),
        0,
    );
}

/// Lit un texte à voix haute (interrompt la lecture précédente).
pub unsafe fn speak(text: &[u8]) {
    if text.iter().all(|&c| c <= b' ') {
        return;
    }
    let n = text.len().min(96);
    // Évite de répéter la même annonce en rafale (survol, repeintures).
    if n == LAST_SPOKEN_LEN && LAST_SPOKEN[..n] == text[..n] && since(LAST_SPOKEN_AT) < 1200 {
        return;
    }
    LAST_SPOKEN[..n].copy_from_slice(&text[..n]);
    LAST_SPOKEN_LEN = n;
    LAST_SPOKEN_AT = now();
    let mut params = [0u8; 600];
    speech_command(
        text,
        A11Y.narrator_rate,
        t(&[b"fr", b"en", b"es", b"de"]),
        &mut params,
    );
    run_speak(&params);
}

/// Annonce réservée au Narrateur actif.
pub unsafe fn announce(text: &[u8]) {
    if A11Y.narrator {
        speak(text);
    }
}

pub unsafe fn stop_speech() {
    LAST_SPOKEN_LEN = 0;
    run_speak(b"--stop\0");
}

unsafe fn announce_pair(a: &[u8], b: &[u8]) {
    let mut buf = [0u8; 200];
    let n = join(&mut buf, &[a, b]);
    announce(&buf[..n]);
}

fn join(out: &mut [u8], parts: &[&[u8]]) -> usize {
    let mut n = 0;
    for part in parts {
        for &c in part.iter() {
            if n < out.len() {
                out[n] = c;
                n += 1;
            }
        }
    }
    n
}

fn write_num(out: &mut [u8], value: u32) -> usize {
    let mut digits = [0u8; 10];
    let mut i = digits.len();
    let mut v = value;
    loop {
        i -= 1;
        digits[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    let n = (digits.len() - i).min(out.len());
    out[..n].copy_from_slice(&digits[i..i + n]);
    n
}

// ---------- Sons système et sous-titres ----------

static mut CAPTION: [u8; 120] = [0; 120];
static mut CAPTION_LEN: usize = 0;
static mut CAPTION_AT: u32 = 0;
static mut FLASH_AT: u32 = 0;
const CAPTION_MS: u32 = 4000;
const FLASH_MS: u32 = 140;

/// Type de son système joué par le bureau.
#[derive(Clone, Copy, PartialEq)]
pub enum Sound {
    Notification,
    Warning,
    Toggle,
}

/// Joue un son système et, selon les réglages, affiche son sous-titre
/// et fait clignoter l'écran.
pub unsafe fn sound_event(kind: Sound, detail: &[u8]) {
    let (name, label): (&[u8], &[u8]) = match kind {
        Sound::Notification => (
            b"--sound notification\0",
            t(&[
                b"[Son de notification] ",
                b"[Notification sound] ",
                b"[Sonido de notificaci\xF3n] ",
                b"[Benachrichtigungston] ",
            ]),
        ),
        Sound::Warning => (
            b"--sound warning\0",
            t(&[
                b"[Son d'avertissement] ",
                b"[Warning sound] ",
                b"[Sonido de advertencia] ",
                b"[Warnton] ",
            ]),
        ),
        Sound::Toggle => (
            b"--sound toggle\0",
            t(&[
                b"[Son de confirmation] ",
                b"[Confirmation sound] ",
                b"[Sonido de confirmaci\xF3n] ",
                b"[Best\xE4tigungston] ",
            ]),
        ),
    };
    // Le Narrateur lit déjà le message : pas de son par-dessus la voix.
    if !A11Y.narrator {
        run_speak(name);
    }
    if A11Y.captions {
        CAPTION_LEN = join(&mut CAPTION, &[label, detail]);
        CAPTION_AT = now();
    }
    if A11Y.flash {
        FLASH_AT = now();
    }
}

// ---------- Cycle de vie ----------

static mut LAST_ACTIVE: usize = usize::MAX;
static mut WAS_NARRATOR: bool = false;

pub unsafe fn init() {
    a11y_reload();
    SPAN_CAPTURE = A11Y.narrator;
    WAS_NARRATOR = A11Y.narrator;
}

/// Changement de réglage venu d'ailleurs (page Paramètres native).
pub unsafe fn tick() -> bool {
    let changed = a11y_reload();
    if changed {
        on_settings_changed();
    }
    changed
}

/// À appeler après toute modification de `A11Y` par le bureau.
pub unsafe fn on_settings_changed() {
    SPAN_CAPTURE = A11Y.narrator;
    if A11Y.narrator != WAS_NARRATOR {
        WAS_NARRATOR = A11Y.narrator;
        if A11Y.narrator {
            speak(t(&[
                b"Narrateur activ\xE9. Touche Ins\xE9rer plus fl\xE8ches pour parcourir l'\xE9cran, Ctrl pour faire taire la voix.",
                b"Narrator on. Insert plus arrows to browse the screen, Ctrl to stop speaking.",
                b"Narrador activado. Insert m\xE1s flechas para recorrer la pantalla, Ctrl para callar.",
                b"Sprachausgabe ein. Einfg und Pfeile zum Navigieren, Strg zum Stoppen.",
            ]));
        } else {
            speak(t(&[
                b"Narrateur d\xE9sactiv\xE9.",
                b"Narrator off.",
                b"Narrador desactivado.",
                b"Sprachausgabe aus.",
            ]));
        }
    }
    if !A11Y.sticky {
        STICKY_LATCHED = 0;
        STICKY_LOCKED = 0;
    }
}

unsafe fn save_and_apply() {
    a11y_save();
    on_settings_changed();
}

/// Début d'une peinture : les textes seront de nouveau relevés.
pub fn begin_frame() {
    unsafe {
        SPAN_COUNT = 0;
    }
}

/// Vrai si l'écran doit être repeint à chaque image (loupe, sous-titre).
pub fn animating() -> bool {
    unsafe { FLASH_AT != 0 && since(FLASH_AT) < FLASH_MS + 60 }
}

/// Boucle d'images : touches lentes, annonce du survol et de la fenêtre active.
pub unsafe fn frame(hwnd: Handle) {
    if PENDING_VK != 0 {
        let held = GetKeyState(PENDING_VK as i32) as u16 & 0x8000 != 0;
        if !held {
            PENDING_VK = 0;
            PENDING_CH = 0;
        } else if since(PENDING_AT) >= A11Y.slow_ms as u32 {
            let (vk, ch) = (PENDING_VK, PENDING_CH);
            PENDING_VK = 0;
            PENDING_CH = 0;
            crate::desktop::on_key(hwnd, vk, 0);
            if ch != 0 {
                deliver_char(hwnd, ch);
            }
        }
    }
    // Fin d'un sous-titre ou d'un clignotement : effacer l'écran.
    if (CAPTION_LEN > 0 && since(CAPTION_AT) > CAPTION_MS)
        || (FLASH_AT != 0 && since(FLASH_AT) > FLASH_MS)
    {
        if since(CAPTION_AT) > CAPTION_MS {
            CAPTION_LEN = 0;
        }
        if since(FLASH_AT) > FLASH_MS {
            FLASH_AT = 0;
        }
        InvalidateRect(hwnd, core::ptr::null(), 0);
    }
    if !A11Y.narrator {
        return;
    }
    let active = crate::wm::active_window().map(|(app, _)| app);
    let active = active.unwrap_or(usize::MAX);
    if active != LAST_ACTIVE {
        LAST_ACTIVE = active;
        if active != usize::MAX {
            announce_pair(
                crate::wm::title_of(active),
                t(&[b", fen\xEAtre", b", window", b", ventana", b", Fenster"]),
            );
        }
    }
    if HOVER_PENDING && since(HOVER_AT) >= 280 {
        HOVER_PENDING = false;
        if let Some(i) = span_at(MOUSE_X, MOUSE_Y) {
            let s = SPANS[i];
            speak(&s.text[..s.len as usize]);
        }
    }
}

// ---------- Souris ----------

static mut HOVER_SPAN: usize = usize::MAX;
static mut HOVER_PENDING: bool = false;
static mut HOVER_AT: u32 = 0;

/// Texte affiché sous un point (le dernier dessiné est au premier plan).
pub fn span_at(x: i32, y: i32) -> Option<usize> {
    unsafe {
        (0..SPAN_COUNT).rev().find(|&i| {
            let s = &SPANS[i];
            x >= s.x - 2 && x < s.x + s.w + 2 && y >= s.y - 2 && y < s.y + s.h + 2
        })
    }
}

/// Mouvement de la souris sur le bureau ; vrai s'il faut repeindre.
pub unsafe fn on_mouse_move(x: i32, y: i32) -> bool {
    let moved = x != MOUSE_X || y != MOUSE_Y;
    MOUSE_X = x;
    MOUSE_Y = y;
    if A11Y.narrator && A11Y.narrator_pointer {
        let hit = span_at(x, y).unwrap_or(usize::MAX);
        if hit != HOVER_SPAN {
            HOVER_SPAN = hit;
            HOVER_PENDING = hit != usize::MAX;
            HOVER_AT = now();
        }
    }
    moved && A11Y.magnifier
}

// ---------- Clavier ----------

static mut DOWN: [bool; 256] = [false; 256];
static mut LAST_UP: [u32; 256] = [0; 256];
/// Modificateurs pressés sans autre touche depuis leur appui.
static mut MOD_ALONE: u8 = 0;
/// Une touche ordinaire a été tapée avec des modificateurs rémanents.
static mut CONSUME_LATCH: bool = false;
static mut SHIFT_COUNT: u8 = 0;
static mut SHIFT_FIRST: u32 = 0;
static mut SWALLOW_CHAR: bool = false;
static mut PENDING_VK: u32 = 0;
static mut PENDING_CH: u16 = 0;
static mut PENDING_AT: u32 = 0;
static mut NAV_INDEX: usize = 0;

fn pressed(vk: i32) -> bool {
    unsafe { key_state(vk) as u16 & 0x8000 != 0 }
}

unsafe fn toggle_message(on: bool, what: &[u8]) {
    let mut buf = [0u8; 96];
    let n = join(
        &mut buf,
        &[
            what,
            if on {
                t(&[b" activ\xE9es", b" on", b" activadas", b" ein"])
            } else {
                t(&[b" d\xE9sactiv\xE9es", b" off", b" desactivadas", b" aus"])
            },
        ],
    );
    crate::desktop::toast(&buf[..n]);
    sound_event(Sound::Toggle, &buf[..n]);
}

const ZOOMS: [u16; 10] = [125, 150, 200, 300, 400, 500, 600, 800, 1000, 1600];

pub fn zoom_step(current: u16, up: bool) -> u16 {
    if up {
        ZOOMS
            .iter()
            .copied()
            .find(|&z| z > current)
            .unwrap_or(ZOOMS[ZOOMS.len() - 1])
    } else {
        ZOOMS
            .iter()
            .rev()
            .copied()
            .find(|&z| z < current)
            .unwrap_or(ZOOMS[0])
    }
}

unsafe fn announce_zoom() {
    let mut buf = [0u8; 64];
    let mut n = join(&mut buf, &[t(&ITEMS[5].label), b" "]);
    n += write_num(&mut buf[n..], A11Y.mag_zoom as u32);
    n += join(&mut buf[n..], &[b" %"]);
    announce(&buf[..n]);
}

/// Raccourcis d'accessibilité globaux. Vrai si la touche est consommée.
unsafe fn hotkey(hwnd: Handle, vk: u32) -> bool {
    let win = pressed(0x5B) || pressed(0x5C);
    let ctrl = pressed(0x11);
    let alt = pressed(0x12);
    let shift = pressed(0x10);
    let repaint = || InvalidateRect(hwnd, core::ptr::null(), 0);
    match vk {
        // Win+Ctrl+Entrée : Narrateur.
        0x0D if win && ctrl => {
            A11Y.narrator = !A11Y.narrator;
            save_and_apply();
            repaint();
            true
        }
        // Win+Plus / Win+Moins : Loupe.
        0xBB | 0x6B if win => {
            if A11Y.magnifier {
                A11Y.mag_zoom = zoom_step(A11Y.mag_zoom, true);
            } else {
                A11Y.magnifier = true;
            }
            save_and_apply();
            announce_zoom();
            repaint();
            true
        }
        0xBD | 0x6D if win && A11Y.magnifier => {
            A11Y.mag_zoom = zoom_step(A11Y.mag_zoom, false);
            save_and_apply();
            announce_zoom();
            repaint();
            true
        }
        // Win+Échap : quitter la Loupe.
        0x1B if win && A11Y.magnifier => {
            A11Y.magnifier = false;
            save_and_apply();
            announce(t(&[
                b"Loupe ferm\xE9e",
                b"Magnifier closed",
                b"Lupa cerrada",
                b"Lupe geschlossen",
            ]));
            repaint();
            true
        }
        // Ctrl+Alt+F / L / D : modes plein écran, lentille, ancré ; I : inverser.
        0x46 | 0x4C | 0x44 | 0x49 if ctrl && alt && !win && A11Y.magnifier => {
            match vk {
                0x46 => A11Y.mag_mode = 0,
                0x4C => A11Y.mag_mode = 1,
                0x44 => A11Y.mag_mode = 2,
                _ => A11Y.mag_invert = !A11Y.mag_invert,
            }
            save_and_apply();
            announce(match vk {
                0x46 => t(&ITEMS[7].options[0]),
                0x4C => t(&ITEMS[7].options[1]),
                0x44 => t(&ITEMS[7].options[2]),
                _ => t(&ITEMS[9].label),
            });
            repaint();
            true
        }
        // Win+U : Paramètres > Accessibilité.
        0x55 if win => {
            crate::settings::open_page(12);
            repaint();
            true
        }
        // Alt gauche + Maj gauche + Impr. écran : contraste élevé.
        0x2C if alt && shift => {
            A11Y.contrast = if A11Y.contrast == 0 { 1 } else { 0 };
            save_and_apply();
            toggle_message(
                A11Y.contrast != 0,
                t(&[
                    b"Couleurs \xE0 contraste \xE9lev\xE9",
                    b"High contrast",
                    b"Alto contraste",
                    b"Hoher Kontrast",
                ]),
            );
            repaint();
            true
        }
        _ => false,
    }
}

/// Navigation du Narrateur (touche Insérer + flèches).
unsafe fn narrator_key(vk: u32) -> bool {
    if !A11Y.narrator {
        return false;
    }
    // Ctrl seul : faire taire la voix.
    if vk == 0x11 {
        stop_speech();
        return false;
    }
    if !pressed(0x2D) {
        return false;
    }
    match vk {
        0x27 | 0x25 => {
            if SPAN_COUNT == 0 {
                return true;
            }
            NAV_INDEX = if vk == 0x27 {
                (NAV_INDEX + 1) % SPAN_COUNT
            } else if NAV_INDEX == 0 || NAV_INDEX >= SPAN_COUNT {
                SPAN_COUNT - 1
            } else {
                NAV_INDEX - 1
            };
            let s = SPANS[NAV_INDEX];
            speak(&s.text[..s.len as usize]);
            true
        }
        0x26 => {
            match crate::wm::active_window() {
                Some((app, _)) => announce_pair(
                    t(&[b"Fen\xEAtre ", b"Window ", b"Ventana ", b"Fenster "]),
                    crate::wm::title_of(app),
                ),
                None => speak(t(&[b"Bureau", b"Desktop", b"Escritorio", b"Desktop"])),
            }
            true
        }
        0x28 => {
            // Lit tout le texte de la fenêtre active (ou de l'écran).
            let area = crate::wm::active_window()
                .map(|(_, r)| r)
                .unwrap_or(rect(0, 0, SCREEN.0, SCREEN.1));
            let mut buf = [0u8; 480];
            let mut n = 0;
            for i in 0..SPAN_COUNT {
                let s = SPANS[i];
                if inside(&area, s.x, s.y) && n + s.len as usize + 2 < buf.len() {
                    n += join(&mut buf[n..], &[&s.text[..s.len as usize], b". "]);
                }
            }
            speak(&buf[..n]);
            true
        }
        _ => false,
    }
}

unsafe fn sticky_down(vk: u32, repeat: bool) {
    let bit = modifier_bit(vk as i32);
    if bit != 0 {
        if !repeat {
            MOD_ALONE |= bit;
        }
    } else {
        MOD_ALONE = 0;
        if STICKY_LATCHED != 0 {
            CONSUME_LATCH = true;
        }
    }
}

unsafe fn sticky_up(hwnd: Handle, vk: u32) {
    let bit = modifier_bit(vk as i32);
    if bit == 0 {
        if CONSUME_LATCH {
            CONSUME_LATCH = false;
            STICKY_LATCHED = 0;
            InvalidateRect(hwnd, core::ptr::null(), 0);
        }
        return;
    }
    if MOD_ALONE & bit == 0 {
        return;
    }
    MOD_ALONE &= !bit;
    // Un appui verrouille pour la touche suivante, deux appuis bloquent,
    // un troisième libère (comportement des touches rémanentes de Windows).
    if STICKY_LOCKED & bit != 0 {
        STICKY_LOCKED &= !bit;
        STICKY_LATCHED &= !bit;
    } else if STICKY_LATCHED & bit != 0 {
        STICKY_LATCHED &= !bit;
        STICKY_LOCKED |= bit;
    } else {
        STICKY_LATCHED |= bit;
    }
    InvalidateRect(hwnd, core::ptr::null(), 0);
}

/// Appui reçu par le bureau. Vrai si la touche ne doit pas être transmise.
pub unsafe fn key_down(hwnd: Handle, vk: u32) -> bool {
    let index = (vk & 0xFF) as usize;
    let repeat = DOWN[index];
    DOWN[index] = true;
    if !repeat {
        // Cinq appuis sur Maj : touches rémanentes.
        if vk == 0x10 || vk == 0xA0 || vk == 0xA1 {
            if SHIFT_COUNT == 0 || since(SHIFT_FIRST) > 3000 {
                SHIFT_COUNT = 0;
                SHIFT_FIRST = now();
            }
            SHIFT_COUNT += 1;
            if SHIFT_COUNT >= 5 {
                SHIFT_COUNT = 0;
                A11Y.sticky = !A11Y.sticky;
                save_and_apply();
                toggle_message(A11Y.sticky, t(&ITEMS[16].label));
                return true;
            }
        } else {
            SHIFT_COUNT = 0;
        }
    }
    // Touches rebond : une frappe trop rapprochée de la précédente est ignorée.
    if A11Y.filter && !repeat && A11Y.bounce_ms > 0 && LAST_UP[index] != 0 {
        if since(LAST_UP[index]) < A11Y.bounce_ms as u32 {
            SWALLOW_CHAR = true;
            return true;
        }
    }
    // Touches lentes : la frappe n'est acceptée qu'après un appui prolongé.
    if A11Y.filter && A11Y.slow_ms > 0 && modifier_bit(vk as i32) == 0 && vk != 0x2D {
        if !repeat {
            PENDING_VK = vk;
            PENDING_CH = 0;
            PENDING_AT = now();
        }
        SWALLOW_CHAR = true;
        return true;
    }
    if A11Y.sticky && !repeat {
        sticky_down(vk, repeat);
    }
    if hotkey(hwnd, vk) || narrator_key(vk) {
        SWALLOW_CHAR = true;
        return true;
    }
    false
}

pub unsafe fn key_up(hwnd: Handle, vk: u32) {
    let index = (vk & 0xFF) as usize;
    DOWN[index] = false;
    LAST_UP[index] = now();
    if A11Y.sticky {
        sticky_up(hwnd, vk);
    }
}

/// Caractère majuscule correspondant (ASCII et lettres Windows-1252).
pub fn upper(ch: u16) -> u16 {
    match ch {
        0x61..=0x7A => ch - 0x20,
        0xE0..=0xFE if ch != 0xF7 => ch - 0x20,
        _ => ch,
    }
}

/// Caractère produit en tenant compte des modificateurs rémanents.
pub fn sticky_char(ch: u16, latched: u8) -> u16 {
    if latched & 2 != 0 && (0x61..=0x7A).contains(&(ch | 0x20)) {
        return (ch | 0x20) & 0x1F;
    }
    if latched & 1 != 0 {
        return upper(ch);
    }
    ch
}

unsafe fn deliver_char(hwnd: Handle, ch: u16) {
    let ch = sticky_char(ch, STICKY_LATCHED | STICKY_LOCKED);
    echo(ch);
    crate::desktop::on_key(hwnd, 0, ch);
}

/// Caractère reçu (WM_CHAR) : `None` s'il est retenu ou ignoré.
pub unsafe fn filter_char(ch: u16) -> Option<u16> {
    if PENDING_VK != 0 && PENDING_CH == 0 {
        PENDING_CH = ch;
        SWALLOW_CHAR = false;
        return None;
    }
    if SWALLOW_CHAR {
        SWALLOW_CHAR = false;
        return None;
    }
    let ch = sticky_char(ch, STICKY_LATCHED | STICKY_LOCKED);
    echo(ch);
    Some(ch)
}

unsafe fn echo(ch: u16) {
    if !A11Y.narrator || !A11Y.narrator_echo {
        return;
    }
    match ch {
        0x20 => speak(t(&[b"espace", b"space", b"espacio", b"Leerzeichen"])),
        0x08 => speak(t(&[
            b"retour arri\xE8re",
            b"backspace",
            b"retroceso",
            b"R\xFCcktaste",
        ])),
        0x0D => speak(t(&[b"entr\xE9e", b"enter", b"intro", b"Eingabe"])),
        0x21..=0xFF => speak(&[ch as u8]),
        _ => {}
    }
}

// ---------- Rendu : sous-titres, touches rémanentes, loupe ----------

/// Rectangle de la lentille ou de la zone ancrée, et la source agrandie.
#[derive(Clone, Copy)]
pub struct MagView {
    pub dst: Rect,
    pub src: Rect,
}

/// Géométrie de la loupe pour un écran `w`×`h` et le pointeur en (`mx`,`my`).
pub fn mag_view(mode: u8, zoom: u16, w: i32, h: i32, mx: i32, my: i32) -> MagView {
    let z = (zoom.max(100)) as i32;
    let mx = mx.clamp(0, w - 1);
    let my = my.clamp(0, h - 1);
    let (dst, cx, cy) = match mode {
        // Lentille : rectangle centré sur le pointeur.
        1 => {
            let lw = (w / 3).max(260).min(w);
            let lh = (h / 4).max(180).min(h);
            let left = (mx - lw / 2).clamp(0, w - lw);
            let top = (my - lh / 2).clamp(0, h - lh);
            (rect(left, top, left + lw, top + lh), mx, my)
        }
        // Ancré : bandeau en haut (en bas si le pointeur y est).
        2 => {
            let dh = h / 3;
            let top = if my < dh + 20 { h - dh } else { 0 };
            (rect(0, top, w, top + dh), mx, my)
        }
        _ => (rect(0, 0, w, h), mx, my),
    };
    let sw = ((dst.right - dst.left) * 100 / z).max(1);
    let sh = ((dst.bottom - dst.top) * 100 / z).max(1);
    let (sx, sy) = if mode == 0 {
        // Plein écran : le point sous le pointeur reste sous le pointeur,
        // les clics tombent donc exactement sur ce qui est agrandi.
        (mx - mx * 100 / z, my - my * 100 / z)
    } else {
        (cx - sw / 2, cy - sh / 2)
    };
    let sx = sx.clamp(0, (w - sw).max(0));
    let sy = sy.clamp(0, (h - sh).max(0));
    MagView {
        dst,
        src: rect(sx, sy, sx + sw, sy + sh),
    }
}

unsafe fn draw_caption(hdc: Handle, b: &mut Brushes, f: &Fonts, w: i32, bar_top: i32) {
    if CAPTION_LEN == 0 || since(CAPTION_AT) > CAPTION_MS {
        return;
    }
    let text_w = text_width(&CAPTION[..CAPTION_LEN], 15).min(w - 80);
    let bw = text_w + 40;
    let left = (w - bw) / 2;
    let top = bar_top - 74;
    round_rect(hdc, b, left, top, left + bw, top + 44, 8, rgb(0, 0, 0));
    round_rect(
        hdc,
        b,
        left + 1,
        top + 1,
        left + bw - 1,
        top + 43,
        7,
        rgb(0x10, 0x10, 0x10),
    );
    text_fit(
        hdc,
        f.semibold,
        15,
        rgb(0xFF, 0xFF, 0xFF),
        left + 20,
        top + 12,
        &CAPTION[..CAPTION_LEN],
        text_w,
    );
}

unsafe fn draw_sticky(hdc: Handle, b: &mut Brushes, f: &Fonts, w: i32, bar_top: i32) {
    if !A11Y.sticky || STICKY_LATCHED | STICKY_LOCKED == 0 {
        return;
    }
    let p = pal();
    let names: [(u8, &[u8]); 4] = [(1, b"Maj"), (2, b"Ctrl"), (4, b"Alt"), (8, b"Win")];
    let mut x = w - 16;
    for (bit, name) in names.iter().rev() {
        let on = (STICKY_LATCHED | STICKY_LOCKED) & bit != 0;
        if !on {
            continue;
        }
        let bw = text_width(name, 13) + 20;
        x -= bw + 6;
        let locked = STICKY_LOCKED & bit != 0;
        round_rect(hdc, b, x, bar_top - 40, x + bw, bar_top - 10, 6, p.accent);
        round_rect(
            hdc,
            b,
            x + 1,
            bar_top - 39,
            x + bw - 1,
            bar_top - 11,
            5,
            if locked { p.accent } else { p.surface },
        );
        text(
            hdc,
            f.semibold,
            if locked { p.on_accent } else { p.text },
            x + 10,
            bar_top - 33,
            name,
        );
    }
}

unsafe fn magnify(hdc: Handle, b: &mut Brushes, w: i32, h: i32) {
    if !A11Y.magnifier || A11Y.mag_zoom <= 100 && !A11Y.mag_invert {
        return;
    }
    let v = mag_view(A11Y.mag_mode, A11Y.mag_zoom, w, h, MOUSE_X, MOUSE_Y);
    let old = SetStretchBltMode(hdc, if A11Y.mag_smooth { 4 } else { 3 });
    StretchBlt(
        hdc,
        v.dst.left,
        v.dst.top,
        v.dst.right - v.dst.left,
        v.dst.bottom - v.dst.top,
        hdc,
        v.src.left,
        v.src.top,
        v.src.right - v.src.left,
        v.src.bottom - v.src.top,
        0x00CC_0020,
    );
    if old > 0 {
        SetStretchBltMode(hdc, old);
    }
    if A11Y.mag_invert {
        PatBlt(
            hdc,
            v.dst.left,
            v.dst.top,
            v.dst.right - v.dst.left,
            v.dst.bottom - v.dst.top,
            0x0055_0009,
        );
    }
    if A11Y.mag_mode != 0 {
        // Cadre de la lentille ou séparation du bandeau ancré.
        let c = pal().accent;
        let d = v.dst;
        fill(hdc, b, d.left, d.top, d.right, d.top + 2, c);
        fill(hdc, b, d.left, d.bottom - 2, d.right, d.bottom, c);
        fill(hdc, b, d.left, d.top, d.left + 2, d.bottom, c);
        fill(hdc, b, d.right - 2, d.top, d.right, d.bottom, c);
    }
}

/// Dernière couche du bureau : sous-titres, modificateurs rémanents,
/// loupe puis clignotement visuel des alertes.
pub unsafe fn draw_overlay(hdc: Handle, b: &mut Brushes, f: &Fonts, w: i32, h: i32, bar_top: i32) {
    SCREEN = (w, h);
    draw_caption(hdc, b, f, w, bar_top);
    draw_sticky(hdc, b, f, w, bar_top);
    magnify(hdc, b, w, h);
    if FLASH_AT != 0 && since(FLASH_AT) < FLASH_MS {
        PatBlt(hdc, 0, 0, w, h, 0x0055_0009);
    }
}

pub unsafe fn regression_checks() {
    // Loupe plein écran : le point sous le pointeur ne bouge pas.
    let v = mag_view(0, 200, 1000, 800, 600, 400);
    check!(
        (v.src.left, v.src.top, v.src.right, v.src.bottom) == (300, 200, 800, 600),
        "Loupe 200 % : source de moitie, centree sur le pointeur"
    );
    let px = v.src.left + (600 - v.dst.left) * 100 / 200;
    check!(
        px == 600,
        "Loupe plein ecran : le clic tombe sous le pointeur"
    );
    let v = mag_view(1, 400, 1200, 800, 5, 5);
    check!(
        v.dst.left == 0 && v.dst.top == 0 && v.src.left == 0,
        "Lentille bornee au coin de l'ecran"
    );
    let v = mag_view(2, 300, 1200, 900, 600, 100);
    check!(
        v.dst.top == 600,
        "Loupe ancree : le bandeau evite le pointeur"
    );
    check!(
        zoom_step(200, true) == 300 && zoom_step(125, false) == 125,
        "Paliers de zoom de la loupe"
    );
    // Touches rémanentes.
    check!(
        sticky_char(b'a' as u16, 1) == b'A' as u16 && sticky_char(0xE9, 1) == 0xC9,
        "Maj remanente : majuscule y compris accentuee"
    );
    check!(sticky_char(b'c' as u16, 2) == 3, "Ctrl remanent : Ctrl+C");
    let saved = (STICKY_LATCHED, STICKY_LOCKED);
    STICKY_LATCHED = 2;
    check!(
        key_state(0x11) as u16 & 0x8000 != 0,
        "Ctrl verrouille vu comme enfonce par les raccourcis"
    );
    (STICKY_LATCHED, STICKY_LOCKED) = saved;
    // Synthèse vocale : texte nettoyé pour la ligne de commande.
    let mut out = [0u8; 600];
    let n = speech_command(b"Ouvrir \"C:\\x\"  maintenant", 7, b"fr", &mut out);
    check!(
        &out[..n] == b"--lang fr --rate 7 \"Ouvrir C: x maintenant\"",
        "Narrateur : parametres de winlux-speak"
    );
    // Réglages : aller-retour du fichier.
    let mut a = A11Y_DEFAULT;
    a.contrast = 3;
    a.mag_zoom = 400;
    a.sticky = true;
    a.slow_ms = 700;
    let mut data = [0u8; 512];
    let n = a11y_format(&a, &mut data);
    check!(
        a11y_parse(&data[..n]) == a,
        "Accessibilite : reglages relus a l'identique"
    );
    check!(
        contrast_colors(2).is_some_and(|c| c.background != c.text) && contrast_colors(5).is_none(),
        "Themes de contraste disponibles"
    );
    page_regression_checks();
}

// ---------- Page Paramètres > Accessibilité ----------

/// Texte traduit : français, anglais, espagnol, allemand (Windows-1252).
pub type Tr = [&'static [u8]; 4];

/// Texte dans la langue de l'interface (Paramètres > Localisation).
pub fn t(s: &Tr) -> &'static [u8] {
    s[match unsafe { crate::locale::CURRENT_LANG } {
        crate::locale::Lang::Fr => 0,
        crate::locale::Lang::En => 1,
        crate::locale::Lang::Es => 2,
        crate::locale::Lang::De => 3,
    }]
}

const fn same(s: &'static [u8]) -> Tr {
    [s, s, s, s]
}

/// Réglage de la page : libellé, aide et choix proposés. Les réglages
/// « pas à pas » (zoom, débit) ont deux choix : moins et plus.
struct Item {
    section: Tr,
    label: Tr,
    help: Tr,
    options: &'static [Tr],
}

const NONE: Tr = same(b"");
const OFF_ON: &[Tr] = &[
    [b"D\xE9sactiv\xE9", b"Off", b"Desactivado", b"Aus"],
    [b"Activ\xE9", b"On", b"Activado", b"Ein"],
];
const OFF_ON_F: &[Tr] = &[
    [b"D\xE9sactiv\xE9es", b"Off", b"Desactivadas", b"Aus"],
    [b"Activ\xE9es", b"On", b"Activadas", b"Ein"],
];
const NO_YES: &[Tr] = &[
    [b"Non", b"No", b"No", b"Nein"],
    [b"Oui", b"Yes", b"S\xED", b"Ja"],
];
const STEP: &[Tr] = &[same(b"-"), same(b"+")];
const DIGITS5: &[Tr] = &[same(b"1"), same(b"2"), same(b"3"), same(b"4"), same(b"5")];
const CARET_LABELS: &[Tr] = &[same(b"1"), same(b"2"), same(b"3"), same(b"4"), same(b"6")];
const CARETS: [u8; 5] = [1, 2, 3, 4, 6];
const SLOW: [u16; 4] = [0, 300, 700, 1000];
const BOUNCE: [u16; 4] = [0, 300, 500, 1000];
const DELAYS: &[Tr] = &[
    [b"Aucun", b"None", b"Ninguno", b"Keine"],
    [b"0,3 s", b"0.3 s", b"0,3 s", b"0,3 s"],
    [b"0,7 s", b"0.7 s", b"0,7 s", b"0,7 s"],
    [b"1 s", b"1 s", b"1 s", b"1 s"],
];
const BOUNCES: &[Tr] = &[
    [b"Aucun", b"None", b"Ninguno", b"Keine"],
    [b"0,3 s", b"0.3 s", b"0,3 s", b"0,3 s"],
    [b"0,5 s", b"0.5 s", b"0,5 s", b"0,5 s"],
    [b"1 s", b"1 s", b"1 s", b"1 s"],
];

/// Noms des thèmes de contraste (mêmes couleurs que `gfx::contrast_colors`).
const CONTRAST_TR: &[Tr] = &[
    [b"Aucun", b"None", b"Ninguno", b"Keins"],
    [b"Aquatique", b"Aquatic", b"Acu\xE1tico", b"Aquatisch"],
    [b"D\xE9sert", b"Desert", b"Desierto", b"W\xFCste"],
    [b"Cr\xE9puscule", b"Dusk", b"Crep\xFAsculo", b"D\xE4mmerung"],
    [
        b"Ciel nocturne",
        b"Night sky",
        b"Cielo nocturno",
        b"Nachthimmel",
    ],
];

const ITEMS: [Item; 20] = [
    Item {
        section: [b"Vision", b"Vision", b"Visi\xF3n", b"Sehen"],
        label: [b"Th\xE8mes de contraste", b"Contrast themes", b"Temas de contraste", b"Kontrastdesigns"],
        help: [
            b"Couleurs \xE0 fort contraste pour le bureau et les applications (Alt + Maj + Impr. \xE9cran).",
            b"High contrast colours for the desktop and apps (Alt + Shift + Print Screen).",
            b"Colores de alto contraste para el escritorio y las aplicaciones (Alt + May\xFAs + Impr Pant).",
            b"Kontrastfarben f\xFCr Desktop und Apps (Alt + Umschalt + Druck).",
        ],
        options: CONTRAST_TR,
    },
    Item {
        section: NONE,
        label: [b"Taille du pointeur", b"Pointer size", b"Tama\xF1o del puntero", b"Zeigergr\xF6\xDFe"],
        help: [
            b"Agrandit la fl\xE8che de la souris.",
            b"Makes the mouse arrow bigger.",
            b"Agranda la flecha del rat\xF3n.",
            b"Vergr\xF6\xDFert den Mauszeiger.",
        ],
        options: DIGITS5,
    },
    Item {
        section: NONE,
        label: [b"Couleur du pointeur", b"Pointer colour", b"Color del puntero", b"Zeigerfarbe"],
        help: [
            b"Un pointeur noir ou vert vif reste visible sur tous les fonds.",
            b"A black or bright green pointer stays visible on any background.",
            b"Un puntero negro o verde vivo se ve sobre cualquier fondo.",
            b"Ein schwarzer oder hellgr\xFCner Zeiger bleibt \xFCberall sichtbar.",
        ],
        options: &[
            [b"Blanc", b"White", b"Blanco", b"Wei\xDF"],
            [b"Noir", b"Black", b"Negro", b"Schwarz"],
            [b"Vert vif", b"Lime", b"Verde vivo", b"Hellgr\xFCn"],
        ],
    },
    Item {
        section: NONE,
        label: [
            b"\xC9paisseur du curseur de texte",
            b"Text cursor thickness",
            b"Grosor del cursor de texto",
            b"Breite des Textcursors",
        ],
        help: [
            b"Barre clignotante des champs de saisie, en pixels.",
            b"Blinking bar in text fields, in pixels.",
            b"Barra parpadeante de los campos de texto, en p\xEDxeles.",
            b"Blinkender Strich in Eingabefeldern, in Pixeln.",
        ],
        options: CARET_LABELS,
    },
    Item {
        section: NONE,
        label: [b"Effets d'animation", b"Animation effects", b"Efectos de animaci\xF3n", b"Animationseffekte"],
        help: [
            b"Ouverture des fen\xEAtres, menus et transitions.",
            b"Window opening, menus and transitions.",
            b"Apertura de ventanas, men\xFAs y transiciones.",
            b"Fenster\xF6ffnung, Men\xFCs und \xDCberg\xE4nge.",
        ],
        options: OFF_ON_F,
    },
    Item {
        section: [b"Loupe", b"Magnifier", b"Lupa", b"Bildschirmlupe"],
        label: [b"Loupe", b"Magnifier", b"Lupa", b"Bildschirmlupe"],
        help: [
            b"Win + Plus pour zoomer, Win + Moins pour r\xE9duire, Win + \xC9chap pour quitter.",
            b"Win + Plus to zoom in, Win + Minus to zoom out, Win + Esc to quit.",
            b"Win + M\xE1s para ampliar, Win + Menos para reducir, Win + Esc para salir.",
            b"Win + Plus vergr\xF6\xDFert, Win + Minus verkleinert, Win + Esc beendet.",
        ],
        options: OFF_ON_F,
    },
    Item {
        section: NONE,
        label: [b"Niveau de zoom", b"Zoom level", b"Nivel de zoom", b"Zoomstufe"],
        help: NONE,
        options: STEP,
    },
    Item {
        section: NONE,
        label: [b"Affichage", b"View", b"Vista", b"Ansicht"],
        help: [
            b"Plein \xE9cran, lentille autour du pointeur ou bandeau ancr\xE9 (Ctrl + Alt + F, L, D).",
            b"Full screen, lens around the pointer or docked band (Ctrl + Alt + F, L, D).",
            b"Pantalla completa, lente junto al puntero o banda acoplada (Ctrl + Alt + F, L, D).",
            b"Vollbild, Linse am Zeiger oder angedockt (Strg + Alt + F, L, D).",
        ],
        options: &[
            [b"Plein \xE9cran", b"Full screen", b"Completa", b"Vollbild"],
            [b"Lentille", b"Lens", b"Lente", b"Linse"],
            [b"Ancr\xE9", b"Docked", b"Acoplada", b"Angedockt"],
        ],
    },
    Item {
        section: NONE,
        label: [b"Lisser les bords", b"Smooth edges", b"Suavizar bordes", b"Kanten gl\xE4tten"],
        help: [
            b"Adoucit les images et le texte agrandis.",
            b"Softens magnified images and text.",
            b"Suaviza las im\xE1genes y el texto ampliados.",
            b"Gl\xE4ttet vergr\xF6\xDFerte Bilder und Texte.",
        ],
        options: NO_YES,
    },
    Item {
        section: NONE,
        label: [b"Inverser les couleurs", b"Invert colours", b"Invertir colores", b"Farben umkehren"],
        help: [
            b"Dans la zone agrandie (Ctrl + Alt + I).",
            b"In the magnified area (Ctrl + Alt + I).",
            b"En la zona ampliada (Ctrl + Alt + I).",
            b"Im vergr\xF6\xDFerten Bereich (Strg + Alt + I).",
        ],
        options: NO_YES,
    },
    Item {
        section: [b"Narrateur", b"Narrator", b"Narrador", b"Sprachausgabe"],
        label: [b"Narrateur", b"Narrator", b"Narrador", b"Sprachausgabe"],
        help: [
            b"Lit l'\xE9cran \xE0 voix haute (Win + Ctrl + Entr\xE9e). Ins\xE9rer + fl\xE8ches pour parcourir.",
            b"Reads the screen aloud (Win + Ctrl + Enter). Insert + arrows to browse.",
            b"Lee la pantalla en voz alta (Win + Ctrl + Intro). Insert + flechas para recorrer.",
            b"Liest den Bildschirm vor (Win + Strg + Eingabe). Einfg + Pfeile zum Navigieren.",
        ],
        options: OFF_ON,
    },
    Item {
        section: NONE,
        label: [b"Vitesse de la voix", b"Voice speed", b"Velocidad de la voz", b"Sprechtempo"],
        help: NONE,
        options: STEP,
    },
    Item {
        section: NONE,
        label: [
            b"Lire les caract\xE8res saisis",
            b"Read typed characters",
            b"Leer los caracteres escritos",
            b"Eingegebene Zeichen vorlesen",
        ],
        help: [
            b"Annonce chaque touche tap\xE9e.",
            b"Announces each key you type.",
            b"Anuncia cada tecla pulsada.",
            b"Sagt jede getippte Taste an.",
        ],
        options: NO_YES,
    },
    Item {
        section: NONE,
        label: [
            b"Lire le texte sous le pointeur",
            b"Read text under the pointer",
            b"Leer el texto bajo el puntero",
            b"Text unter dem Zeiger vorlesen",
        ],
        help: [
            b"Boutons, menus et titres survol\xE9s par la souris.",
            b"Buttons, menus and titles under the mouse.",
            b"Botones, men\xFAs y t\xEDtulos bajo el rat\xF3n.",
            b"Schaltfl\xE4chen, Men\xFCs und Titel unter der Maus.",
        ],
        options: NO_YES,
    },
    Item {
        section: [b"Audio", b"Audio", b"Audio", b"Audio"],
        label: [
            b"Sous-titres des sons syst\xE8me",
            b"Captions for system sounds",
            b"Subt\xEDtulos de sonidos del sistema",
            b"Untertitel f\xFCr Systemkl\xE4nge",
        ],
        help: [
            b"Affiche le nom du son jou\xE9 au-dessus de la barre des t\xE2ches.",
            b"Shows the name of the sound above the taskbar.",
            b"Muestra el nombre del sonido sobre la barra de tareas.",
            b"Zeigt den Namen des Klangs \xFCber der Taskleiste.",
        ],
        options: NO_YES,
    },
    Item {
        section: NONE,
        label: [
            b"Faire clignoter l'\xE9cran",
            b"Flash the screen",
            b"Hacer parpadear la pantalla",
            b"Bildschirm blinken lassen",
        ],
        help: [
            b"Signale visuellement les alertes sonores.",
            b"Shows audio alerts visually.",
            b"Indica visualmente las alertas sonoras.",
            b"Zeigt akustische Warnungen sichtbar an.",
        ],
        options: NO_YES,
    },
    Item {
        section: [b"Clavier", b"Keyboard", b"Teclado", b"Tastatur"],
        label: [b"Touches r\xE9manentes", b"Sticky Keys", b"Teclas especiales", b"Einrastfunktion"],
        help: [
            b"Maj, Ctrl, Alt ou Windows restent actives pour la touche suivante. Maj cinq fois : marche/arr\xEAt.",
            b"Shift, Ctrl, Alt or Windows stay active for the next key. Press Shift five times to toggle.",
            b"May\xFAs, Ctrl, Alt o Windows siguen activas para la tecla siguiente. May\xFAs cinco veces: activar.",
            b"Umschalt, Strg, Alt oder Windows bleiben f\xFCr die n\xE4chste Taste aktiv. F\xFCnfmal Umschalt: ein/aus.",
        ],
        options: OFF_ON_F,
    },
    Item {
        section: NONE,
        label: [b"Touches filtres", b"Filter Keys", b"Teclas filtro", b"Anschlagverz\xF6gerung"],
        help: [
            b"Ignore les frappes br\xE8ves ou r\xE9p\xE9t\xE9es.",
            b"Ignores brief or repeated keystrokes.",
            b"Ignora pulsaciones breves o repetidas.",
            b"Ignoriert kurze oder wiederholte Anschl\xE4ge.",
        ],
        options: OFF_ON_F,
    },
    Item {
        section: NONE,
        label: [
            b"Dur\xE9e d'appui requise",
            b"Required hold time",
            b"Tiempo de pulsaci\xF3n",
            b"Erforderliche Haltedauer",
        ],
        help: [
            b"Une touche doit rester enfonc\xE9e ce temps pour compter.",
            b"A key must be held this long to count.",
            b"La tecla debe mantenerse este tiempo para contar.",
            b"Eine Taste muss so lange gedr\xFCckt werden.",
        ],
        options: DELAYS,
    },
    Item {
        section: NONE,
        label: [
            b"Ignorer les frappes r\xE9p\xE9t\xE9es",
            b"Ignore repeated keystrokes",
            b"Ignorar pulsaciones repetidas",
            b"Wiederholte Anschl\xE4ge ignorieren",
        ],
        help: [
            b"D\xE9lai pendant lequel une m\xEAme touche n'est pas reprise.",
            b"Delay during which the same key is not accepted again.",
            b"Tiempo durante el que no se acepta la misma tecla.",
            b"Zeit, in der dieselbe Taste nicht erneut z\xE4hlt.",
        ],
        options: BOUNCES,
    },
];

fn position<T: PartialEq + Copy>(values: &[T], v: T) -> usize {
    values.iter().position(|&x| x == v).unwrap_or(usize::MAX)
}

/// Choix sélectionné d'un réglage (`usize::MAX` pour un pas à pas).
pub fn item_value(a: &A11y, item: usize) -> usize {
    match item {
        0 => a.contrast as usize,
        1 => a.pointer_size as usize - 1,
        2 => a.pointer_color as usize,
        3 => position(&CARETS, a.caret_width),
        4 => a.animations as usize,
        5 => a.magnifier as usize,
        7 => a.mag_mode as usize,
        8 => a.mag_smooth as usize,
        9 => a.mag_invert as usize,
        10 => a.narrator as usize,
        12 => a.narrator_echo as usize,
        13 => a.narrator_pointer as usize,
        14 => a.captions as usize,
        15 => a.flash as usize,
        16 => a.sticky as usize,
        17 => a.filter as usize,
        18 => position(&SLOW, a.slow_ms),
        19 => position(&BOUNCE, a.bounce_ms),
        _ => usize::MAX,
    }
}

/// Applique le choix `option` du réglage `item`.
pub fn item_set(a: &mut A11y, item: usize, option: usize) {
    let on = option == 1;
    match item {
        0 => a.contrast = option.min(4) as u8,
        1 => a.pointer_size = (option.min(4) + 1) as u8,
        2 => a.pointer_color = option.min(2) as u8,
        3 => a.caret_width = CARETS[option.min(4)],
        4 => a.animations = on,
        5 => a.magnifier = on,
        6 => a.mag_zoom = zoom_step(a.mag_zoom, on),
        7 => a.mag_mode = option.min(2) as u8,
        8 => a.mag_smooth = on,
        9 => a.mag_invert = on,
        10 => a.narrator = on,
        11 => {
            a.narrator_rate = if on {
                (a.narrator_rate + 1).min(10)
            } else {
                a.narrator_rate.saturating_sub(1).max(1)
            }
        }
        12 => a.narrator_echo = on,
        13 => a.narrator_pointer = on,
        14 => a.captions = on,
        15 => a.flash = on,
        16 => a.sticky = on,
        17 => a.filter = on,
        18 => a.slow_ms = SLOW[option.min(3)],
        19 => a.bounce_ms = BOUNCE[option.min(3)],
        _ => {}
    }
}

/// Valeur affichée sous le libellé des réglages pas à pas.
fn item_status(a: &A11y, item: usize, out: &mut [u8; 48]) -> usize {
    match item {
        6 => {
            let n = write_num(out, a.mag_zoom as u32);
            n + join(&mut out[n..], &[b" %"])
        }
        11 => {
            let n = write_num(out, a.narrator_rate as u32);
            n + join(
                &mut out[n..],
                &[t(&[b" sur 10", b" of 10", b" de 10", b" von 10"])],
            )
        }
        _ => 0,
    }
}

static mut PAGE_SCROLL: i32 = 0;
static mut PAGE_HOVER: (usize, usize) = (usize::MAX, 0);
static mut PAGE_FOCUS: usize = 0;
const PREVIEW_H: i32 = 132;
const SECTION_H: i32 = 34;
const ROW_H: i32 = 58;

/// Disposition d'un réglage : rangée et choix (rectangles en coordonnées écran).
struct RowGeo {
    row: Rect,
    chips: [Rect; 5],
}

fn chip_width(label: &[u8]) -> i32 {
    (text_width(label, 11) + 26).max(44)
}

fn layout_rows(c: &Rect, scroll: i32, out: &mut [RowGeo; 20]) -> i32 {
    let mut y = c.top + 44 + PREVIEW_H + 12 - scroll;
    for (i, item) in ITEMS.iter().enumerate() {
        if !item.section[0].is_empty() {
            y += SECTION_H;
        }
        let row = rect(c.left - 8, y, c.right + 8, y + ROW_H - 6);
        let mut x = c.right - 4;
        let mut chips = [rect(0, 0, 0, 0); 5];
        for (j, label) in item.options.iter().enumerate().rev() {
            let w = chip_width(t(label));
            chips[j] = rect(x - w, y + 13, x, y + 41);
            x -= w + 6;
        }
        out[i] = RowGeo { row, chips };
        y += ROW_H;
    }
    y + scroll - (c.top + 44)
}

const EMPTY_GEO: RowGeo = RowGeo {
    row: rect(0, 0, 0, 0),
    chips: [rect(0, 0, 0, 0); 5],
};

unsafe fn draw_preview(g: &mut crate::wm::Gfx, c: &Rect) {
    let top = c.top + 44 - PAGE_SCROLL;
    if top + PREVIEW_H < c.top + 40 {
        return;
    }
    let p = pal();
    let box_ = rect(c.left - 8, top, c.right + 8, top + PREVIEW_H);
    round_rect(
        g.hdc,
        g.b,
        box_.left,
        box_.top,
        box_.right,
        box_.bottom,
        8,
        p.border,
    );
    round_rect(
        g.hdc,
        g.b,
        box_.left + 1,
        box_.top + 1,
        box_.right - 1,
        box_.bottom - 1,
        7,
        p.surface,
    );
    text(
        g.hdc,
        g.f.small,
        p.text3,
        c.left + 4,
        top + 10,
        t(&[
            b"APER\xC7U EN DIRECT",
            b"LIVE PREVIEW",
            b"VISTA PREVIA",
            b"LIVE-VORSCHAU",
        ]),
    );
    // Fenêtre miniature dans la palette active.
    let w = rect(
        c.left + 4,
        top + 30,
        c.left + 4 + 260.min(c.right - c.left - 120),
        top + PREVIEW_H - 12,
    );
    round_rect(g.hdc, g.b, w.left, w.top, w.right, w.bottom, 6, p.border);
    round_rect(
        g.hdc,
        g.b,
        w.left + 1,
        w.top + 1,
        w.right - 1,
        w.bottom - 1,
        5,
        p.window,
    );
    text(
        g.hdc,
        g.f.semibold,
        p.text,
        w.left + 10,
        w.top + 7,
        t(&[
            b"Document - \xC9diteur",
            b"Document - Editor",
            b"Documento - Editor",
            b"Dokument - Editor",
        ]),
    );
    fill(
        g.hdc,
        g.b,
        w.left + 1,
        w.top + 27,
        w.right - 1,
        w.top + 28,
        p.border,
    );
    text(
        g.hdc,
        g.f.body,
        p.text2,
        w.left + 10,
        w.top + 36,
        b"Bonjour Winlux",
    );
    let caret_x = w.left + 12 + text_width(b"Bonjour Winlux", 13);
    fill(
        g.hdc,
        g.b,
        caret_x,
        w.top + 35,
        caret_x + A11Y.caret_width as i32,
        w.top + 52,
        p.text,
    );
    let button = rect(w.left + 10, w.bottom - 30, w.left + 92, w.bottom - 8);
    round_rect(
        g.hdc,
        g.b,
        button.left,
        button.top,
        button.right,
        button.bottom,
        5,
        p.accent,
    );
    text_centered(
        g.hdc,
        g.f.small,
        11,
        p.on_accent,
        (button.left + button.right) / 2,
        button.top + 5,
        t(&[b"Enregistrer", b"Save", b"Guardar", b"Speichern"]),
    );
    text(
        g.hdc,
        g.f.small,
        p.text3,
        button.right + 10,
        button.top + 5,
        t(&[
            b"Lien d\xE9sactiv\xE9",
            b"Disabled link",
            b"Enlace inactivo",
            b"Inaktiver Link",
        ]),
    );
    // Pointeur à la taille et à la couleur choisies.
    const ARROW: [&[u8; 12]; 19] = [
        b"X           ",
        b"XX          ",
        b"XoX         ",
        b"XooX        ",
        b"XoooX       ",
        b"XooooX      ",
        b"XoooooX     ",
        b"XooooooX    ",
        b"XoooooooX   ",
        b"XooooooooX  ",
        b"XoooooooooX ",
        b"XooooooXXXXX",
        b"XoooXooX    ",
        b"XooXXooX    ",
        b"XoX  XooX   ",
        b"XX   XooX   ",
        b"X     XooX  ",
        b"      XooX  ",
        b"       XX   ",
    ];
    let (inner, outline) = match A11Y.pointer_color {
        1 => (rgb(0, 0, 0), rgb(0xFF, 0xFF, 0xFF)),
        2 => (rgb(0xC8, 0xFF, 0x00), rgb(0, 0, 0)),
        _ => (rgb(0xFF, 0xFF, 0xFF), rgb(0, 0, 0)),
    };
    // Taille 1 à 5 : 1×, 1,5×, 2×, 2,5×, 3× (en demi-pixels).
    let half = 1 + A11Y.pointer_size as i32;
    let px = w.right + 24;
    let py = top + 34;
    for (row, line) in ARROW.iter().enumerate() {
        for (col, &cell) in line.iter().enumerate() {
            if cell == b' ' {
                continue;
            }
            let x0 = px + col as i32 * half / 2;
            let y0 = py + row as i32 * half / 2;
            let x1 = px + (col as i32 + 1) * half / 2;
            let y1 = py + (row as i32 + 1) * half / 2;
            fill(
                g.hdc,
                g.b,
                x0,
                y0,
                x1.max(x0 + 1),
                y1.max(y0 + 1),
                if cell == b'o' { inner } else { outline },
            );
        }
    }
    // État des aides actives, à droite.
    let mut y = top + 34;
    let x = px + 60;
    if x + 120 < box_.right {
        let mut zoom = [0u8; 48];
        let mut narr = [0u8; 48];
        let narr_len = join(
            &mut narr,
            &[
                t(&ITEMS[10].label),
                b" : ",
                t(&OFF_ON[A11Y.narrator as usize]),
            ],
        );
        let mut n = join(&mut zoom, &[t(&ITEMS[5].label), b" : "]);
        if A11Y.magnifier {
            n += write_num(&mut zoom[n..], A11Y.mag_zoom as u32);
            n += join(&mut zoom[n..], &[b" %"]);
        } else {
            n += join(&mut zoom[n..], &[t(&OFF_ON_F[0])]);
        }
        for (label, on) in [
            (&zoom[..n], A11Y.magnifier),
            (&narr[..narr_len], A11Y.narrator),
            (
                if A11Y.sticky {
                    t(&ITEMS[16].label)
                } else {
                    t(&ITEMS[16].section)
                },
                A11Y.sticky || A11Y.filter,
            ),
        ] {
            ellipse(
                g.hdc,
                g.b,
                x,
                y + 4,
                x + 8,
                y + 12,
                if on { p.accent } else { p.text3 },
            );
            text_fit(
                g.hdc,
                g.f.body,
                13,
                p.text,
                x + 16,
                y,
                label,
                box_.right - x - 30,
            );
            y += 26;
        }
    }
}

/// Dessine la page Accessibilité dans le cadre de contenu des Paramètres.
pub unsafe fn page_draw(g: &mut crate::wm::Gfx, c: &Rect) {
    let p = pal();
    let mut geo = [EMPTY_GEO; 20];
    let total = layout_rows(c, PAGE_SCROLL, &mut geo);
    let visible = c.bottom - (c.top + 44);
    PAGE_SCROLL = PAGE_SCROLL.clamp(0, (total - visible).max(0));
    layout_rows(c, PAGE_SCROLL, &mut geo);
    draw_preview(g, c);
    let min_top = c.top + 40;
    for (i, item) in ITEMS.iter().enumerate() {
        let r = geo[i].row;
        if !item.section[0].is_empty() && r.top - SECTION_H + 6 >= min_top && r.bottom <= c.bottom {
            text(
                g.hdc,
                g.f.title,
                p.text,
                c.left,
                r.top - SECTION_H + 8,
                t(&item.section),
            );
        }
        if r.top < min_top || r.bottom > c.bottom {
            continue;
        }
        let focused = PAGE_FOCUS == i;
        round_rect(
            g.hdc,
            g.b,
            r.left,
            r.top,
            r.right,
            r.bottom,
            8,
            if focused { p.accent } else { p.border },
        );
        round_rect(
            g.hdc,
            g.b,
            r.left + 1,
            r.top + 1,
            r.right - 1,
            r.bottom - 1,
            7,
            p.surface,
        );
        let chips_left = geo[i].chips[0].left;
        let label_w = (chips_left - c.left - 16).max(40);
        text_fit(
            g.hdc,
            g.f.semibold,
            13,
            p.text,
            c.left,
            r.top + 9,
            t(&item.label),
            label_w,
        );
        let mut status = [0u8; 48];
        let n = item_status(&A11Y, i, &mut status);
        let help: &[u8] = if n > 0 { &status[..n] } else { t(&item.help) };
        text_fit(
            g.hdc,
            g.f.small,
            11,
            p.text3,
            c.left,
            r.top + 29,
            help,
            label_w,
        );
        let selected = item_value(&A11Y, i);
        for (j, label) in item.options.iter().enumerate() {
            let chip = geo[i].chips[j];
            let on = selected == j;
            let hover = PAGE_HOVER == (i, j);
            round_rect(
                g.hdc,
                g.b,
                chip.left,
                chip.top,
                chip.right,
                chip.bottom,
                6,
                if on { p.accent } else { p.border },
            );
            round_rect(
                g.hdc,
                g.b,
                chip.left + 1,
                chip.top + 1,
                chip.right - 1,
                chip.bottom - 1,
                5,
                if on {
                    p.accent
                } else if hover {
                    p.hover
                } else {
                    p.surface2
                },
            );
            text_centered(
                g.hdc,
                g.f.small,
                11,
                if on { p.on_accent } else { p.text },
                (chip.left + chip.right) / 2,
                chip.top + 7,
                t(label),
            );
        }
    }
}

fn page_hit(c: &Rect, x: i32, y: i32) -> Option<(usize, usize)> {
    let mut geo = [EMPTY_GEO; 20];
    layout_rows(c, unsafe { PAGE_SCROLL }, &mut geo);
    for (i, item) in ITEMS.iter().enumerate() {
        if geo[i].row.top < c.top + 40 || geo[i].row.bottom > c.bottom {
            continue;
        }
        for j in 0..item.options.len() {
            if inside(&geo[i].chips[j], x, y) {
                return Some((i, j));
            }
        }
    }
    None
}

unsafe fn choose(item: usize, option: usize) {
    item_set(&mut A11Y, item, option);
    PAGE_FOCUS = item;
    if !a11y_save() {
        announce(t(&[
            b"Enregistrement impossible",
            b"Could not save",
            b"No se pudo guardar",
            b"Speichern fehlgeschlagen",
        ]));
    }
    on_settings_changed();
    let mut buf = [0u8; 120];
    let mut status = [0u8; 48];
    let n = item_status(&A11Y, item, &mut status);
    let value: &[u8] = if n > 0 {
        &status[..n]
    } else {
        t(&ITEMS[item].options[option.min(ITEMS[item].options.len() - 1)])
    };
    let n = join(&mut buf, &[t(&ITEMS[item].label), b" : ", value]);
    announce(&buf[..n]);
}

/// Souris sur la page ; vrai s'il faut repeindre.
pub unsafe fn page_mouse(ev: crate::wm::Mouse, x: i32, y: i32, c: &Rect) -> bool {
    use crate::wm::Mouse;
    match ev {
        Mouse::Wheel(delta) => {
            PAGE_SCROLL = (PAGE_SCROLL - delta.signum() * 3 * ROW_H / 2).max(0);
            true
        }
        Mouse::Move => {
            let h = page_hit(c, x, y).unwrap_or((usize::MAX, 0));
            let changed = h != PAGE_HOVER;
            PAGE_HOVER = h;
            changed
        }
        Mouse::Down | Mouse::DoubleClick => match page_hit(c, x, y) {
            Some((i, j)) => {
                choose(i, j);
                true
            }
            None => false,
        },
        _ => false,
    }
}

/// Clavier : flèches haut/bas pour changer de réglage, gauche/droite pour
/// changer la valeur. Vrai si la touche est utilisée.
pub unsafe fn page_key(vk: u32) -> bool {
    match vk {
        0x26 | 0x28 => {
            PAGE_FOCUS = if vk == 0x28 {
                (PAGE_FOCUS + 1).min(ITEMS.len() - 1)
            } else {
                PAGE_FOCUS.saturating_sub(1)
            };
            // Garder le réglage visible.
            let target = PAGE_FOCUS as i32 * ROW_H + PREVIEW_H - 2 * ROW_H;
            PAGE_SCROLL = target.max(0);
            let mut buf = [0u8; 120];
            let n = join(&mut buf, &[t(&ITEMS[PAGE_FOCUS].label)]);
            announce(&buf[..n]);
            true
        }
        0x25 | 0x27 => {
            let item = PAGE_FOCUS;
            let count = ITEMS[item].options.len();
            let current = item_value(&A11Y, item);
            let option = if current == usize::MAX {
                (vk == 0x27) as usize
            } else if vk == 0x27 {
                (current + 1).min(count - 1)
            } else {
                current.saturating_sub(1)
            };
            choose(item, option);
            true
        }
        _ => false,
    }
}

pub unsafe fn page_regression_checks() {
    let mut a = A11Y_DEFAULT;
    let valid = (0..ITEMS.len()).all(|i| {
        let v = item_value(&a, i);
        v == usize::MAX && ITEMS[i].options.len() == 2 || v < ITEMS[i].options.len()
    });
    check!(
        valid,
        "Accessibilite : chaque reglage affiche un choix valide"
    );
    item_set(&mut a, 0, 4);
    item_set(&mut a, 3, 4);
    item_set(&mut a, 6, 1);
    item_set(&mut a, 18, 2);
    check!(
        a.contrast == 4 && a.caret_width == 6 && a.mag_zoom == 300 && a.slow_ms == 700,
        "Accessibilite : les choix de la page modifient les reglages"
    );
    let c = rect(300, 100, 1000, 3000);
    let mut geo = [EMPTY_GEO; 20];
    layout_rows(&c, 0, &mut geo);
    let chip = geo[16].chips[1];
    check!(
        page_hit(&c, chip.left + 3, chip.top + 3) == Some((16, 1)),
        "Accessibilite : un clic sur Activees cible les touches remanentes"
    );
}
