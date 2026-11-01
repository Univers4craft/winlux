//! Paramètres > Clavier et pavé tactile.
//!
//! Le pavé tactile n'apparaît que si `winlux-touchpad` en trouve un (rien
//! sur un ordinateur fixe ni en VM) et chaque réglage seulement si le pavé
//! l'accepte (lignes `touchpad_*` absentes sinon). Défilement et clic à deux
//! doigts, défilement naturel, désactivation pendant la frappe, vitesse ;
//! le pincement pour zoomer est traduit par le runtime en Ctrl + molette.

use super::{content, control, value, SCROLL};
use crate::gfx::*;
use crate::gpuinfo::Line;
use crate::wm::{Frame, Gfx};
use winlux_testexe::*;

const ROW_H: i32 = 58;
const HEADER_H: i32 = 34;
const BTN_H: i32 = 30;

const KEYBOARD: u32 = 1;
const TOGGLE: u32 = 2;
const SPEED: u32 = 3;

/// Réglages oui/non : (clé d'état et de commande).
const TOGGLES: [&[u8]; 5] = [b"tap", b"twofinger", b"clickfinger", b"natural", b"dwt"];

pub static mut HOVER: u32 = 0;

#[derive(Clone, Copy)]
struct Btn {
    label: &'static [u8],
    action: u32,
    selected: bool,
}

const fn btn(label: &'static [u8], action: u32, selected: bool) -> Btn {
    Btn {
        label,
        action,
        selected,
    }
}

const fn act(kind: u32, arg: u32) -> u32 {
    (kind << 8) | arg
}

enum Item<'a> {
    Header(&'a [u8]),
    Row(&'a [u8], &'a [u8], &'a [Btn]),
}

/// `touchpad_<nom>` ; vide si le pavé ne propose pas ce réglage.
unsafe fn tp(name: &[u8]) -> &'static [u8] {
    let mut key = [0u8; 32];
    let p = b"touchpad_";
    let n = name.len().min(key.len() - p.len());
    key[..p.len()].copy_from_slice(p);
    key[p.len()..p.len() + n].copy_from_slice(&name[..n]);
    value(&key[..p.len() + n])
}

fn number(v: &[u8]) -> u32 {
    v.iter()
        .take_while(|b| b.is_ascii_digit())
        .fold(0u32, |n, b| n.saturating_mul(10).saturating_add(u32::from(b - b'0')))
}

unsafe fn items(mut visit: impl FnMut(Item)) {
    visit(Item::Header(tr4(&[b"Clavier", b"Keyboard", b"Teclado", b"Tastatur"])));
    let layout = value(b"layout");
    let keyboards = [
        btn(tr4(&[b"AZERTY fran\xE7ais", b"French AZERTY", b"AZERTY franc\xE9s", b"AZERTY Franz\xF6sisch"]), act(KEYBOARD, 0), layout == b"fr"),
        btn(b"QWERTY US", act(KEYBOARD, 1), layout == b"us"),
    ];
    visit(Item::Row(
        tr4(&[b"Disposition actuelle", b"Current layout", b"Distribuci\xF3n actual", b"Aktuelles Layout"]),
        tr4(&[b"Autres dispositions : page Localisation.", b"Other layouts: Localization page.", b"Otras distribuciones: p\xE1gina Localizaci\xF3n.", b"Weitere Layouts: Seite Lokalisierung."]),
        &keyboards,
    ));
    if tp(b"present") != b"1" {
        return;
    }
    visit(Item::Header(tr4(&[b"Pav\xE9 tactile", b"Touchpad", b"Panel t\xE1ctil", b"Touchpad"])));
    let labels: [(&[u8], &[u8]); 5] = [
        (
            tr4(&[b"Taper pour cliquer", b"Tap to click", b"Tocar para hacer clic", b"Tippen zum Klicken"]),
            tr4(&[b"Une touche l\xE9g\xE8re vaut un clic.", b"A light tap counts as a click.", b"Un toque ligero equivale a un clic.", b"Leichtes Tippen gilt als Klick."]),
        ),
        (
            tr4(&[b"D\xE9filer avec deux doigts", b"Two-finger scrolling", b"Desplazar con dos dedos", b"Mit zwei Fingern scrollen"]),
            tr4(&[b"Sinon, d\xE9filement sur le bord du pav\xE9.", b"Otherwise, scroll along the edge of the pad.", b"Si no, desplazamiento por el borde del panel.", b"Sonst Scrollen am Rand des Touchpads."]),
        ),
        (
            tr4(&[b"Clic droit \xE0 deux doigts", b"Two-finger right-click", b"Clic derecho con dos dedos", b"Rechtsklick mit zwei Fingern"]),
            tr4(&[b"Sinon, appui dans le coin inf\xE9rieur droit.", b"Otherwise, press the bottom-right corner.", b"Si no, pulse la esquina inferior derecha.", b"Sonst unten rechts dr\xFCcken."]),
        ),
        (
            tr4(&[b"D\xE9filement naturel", b"Natural scrolling", b"Desplazamiento natural", b"Nat\xFCrliches Scrollen"]),
            tr4(&[b"Le contenu suit les doigts, comme sur un t\xE9l\xE9phone.", b"Content follows your fingers, like on a phone.", b"El contenido sigue a los dedos, como en un tel\xE9fono.", b"Inhalt folgt den Fingern wie beim Telefon."]),
        ),
        (
            tr4(&[b"D\xE9sactiver pendant la frappe", b"Turn off while typing", b"Desactivar al escribir", b"Beim Tippen deaktivieren"]),
            tr4(&[b"\xC9vite les clics involontaires avec la paume.", b"Avoids accidental clicks with your palm.", b"Evita clics involuntarios con la palma.", b"Verhindert versehentliche Klicks mit dem Handballen."]),
        ),
    ];
    let yes = tr4(&[b"Oui", b"On", b"S\xED", b"Ein"]);
    let no = tr4(&[b"Non", b"Off", b"No", b"Aus"]);
    for (i, key) in TOGGLES.iter().enumerate() {
        let v = tp(key);
        if v.is_empty() {
            continue;
        }
        let on = v == b"1";
        let buttons = [
            btn(yes, act(TOGGLE, (i as u32) << 1 | 1), on),
            btn(no, act(TOGGLE, (i as u32) << 1), !on),
        ];
        visit(Item::Row(labels[i].0, labels[i].1, &buttons));
    }
    let speed = tp(b"speed");
    if !speed.is_empty() {
        let mut l = Line::new();
        l.num(number(speed)).push(b" %");
        let line = l;
        let buttons = [
            btn(b"-", act(SPEED, 0), false),
            btn(tr4(&[b"Par d\xE9faut", b"Default", b"Predeterminada", b"Standard"]), act(SPEED, 1), speed == b"50"),
            btn(b"+", act(SPEED, 2), false),
        ];
        visit(Item::Row(
            tr4(&[b"Vitesse du pointeur", b"Pointer speed", b"Velocidad del puntero", b"Zeigergeschwindigkeit"]),
            line.get(),
            &buttons,
        ));
    }
    if tp(b"gestures") == b"1" {
        visit(Item::Row(
            tr4(&[b"Pincer pour zoomer", b"Pinch to zoom", b"Pellizcar para ampliar", b"Zum Zoomen zusammenziehen"]),
            tr4(&[b"\xC9cartez ou rapprochez deux doigts : les applications zooment (Ctrl + molette).", b"Spread or pinch two fingers: apps zoom (Ctrl + wheel).", b"Separe o junte dos dedos: las aplicaciones hacen zoom (Ctrl + rueda).", b"Zwei Finger spreizen oder zusammenziehen: Apps zoomen (Strg + Mausrad)."]),
            &[],
        ));
    }
}

fn button_width(c: &Rect, n: usize) -> i32 {
    let w = c.right - c.left;
    (w * 60 / 100 / n.max(1) as i32 - 6).clamp(44, 112)
}

fn button_rect(c: &Rect, top: i32, k: usize, n: usize) -> Rect {
    let bw = button_width(c, n);
    let right = c.right - (n - 1 - k) as i32 * (bw + 6);
    let y = top + (ROW_H - 8 - BTN_H) / 2;
    rect(right - bw, y, right, y + BTN_H)
}

fn item_h(item: &Item) -> i32 {
    match item {
        Item::Header(_) => HEADER_H,
        Item::Row(..) => ROW_H,
    }
}

unsafe fn walk(c: &Rect, mut visit: impl FnMut(Item, i32)) {
    let mut y = c.top + 44 - SCROLL as i32 * 12;
    items(|item| {
        let h = item_h(&item);
        visit(item, y);
        y += h;
    });
}

fn visible(c: &Rect, top: i32, h: i32) -> bool {
    top >= c.top + 40 && top + h <= c.bottom - 24
}

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let c = content(f);
    let mut total = 0;
    items(|item| total += item_h(&item));
    let room = c.bottom - 24 - (c.top + 44);
    let max_scroll = ((total - room).max(0) + 11) / 12;
    SCROLL = SCROLL.min(max_scroll as usize);
    let p = pal();
    walk(&c, |item, y| match item {
        Item::Header(title) => {
            if visible(&c, y, HEADER_H) {
                text_fit(g.hdc, g.f.semibold, 13, p.text, c.left, y + 12, title, c.right - c.left);
            }
        }
        Item::Row(label, val, buttons) => {
            if !visible(&c, y, ROW_H) {
                return;
            }
            round_rect(g.hdc, g.b, c.left - 8, y, c.right + 8, y + ROW_H - 6, 8, p.border);
            round_rect(g.hdc, g.b, c.left - 7, y + 1, c.right + 7, y + ROW_H - 7, 7, p.surface);
            let text_right = if buttons.is_empty() {
                c.right
            } else {
                button_rect(&c, y, 0, buttons.len()).left - 10
            };
            let tw = (text_right - c.left).max(40);
            text_fit(g.hdc, g.f.semibold, 13, p.text, c.left, y + 8, label, tw);
            text_fit(g.hdc, g.f.small, 11, p.text2, c.left, y + 28, val, tw);
            for (k, b) in buttons.iter().enumerate() {
                let r = button_rect(&c, y, k, buttons.len());
                round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 6, if b.selected { p.accent } else { p.border });
                round_rect(
                    g.hdc,
                    g.b,
                    r.left + 1,
                    r.top + 1,
                    r.right - 1,
                    r.bottom - 1,
                    5,
                    if b.selected {
                        p.selected
                    } else if HOVER == b.action + 1 {
                        p.hover
                    } else {
                        p.surface2
                    },
                );
                let mut end = b.label.len();
                while end > 0 && text_width(&b.label[..end], 11) > r.right - r.left - 8 {
                    end -= 1;
                }
                text_centered(g.hdc, g.f.small, 11, p.text, (r.left + r.right) / 2, r.top + 9, &b.label[..end]);
            }
        }
    });
}

pub unsafe fn hit(f: &Frame, x: i32, y: i32) -> u32 {
    let c = content(f);
    let mut found = 0;
    walk(&c, |item, top| match item {
        Item::Row(_, _, buttons) if visible(&c, top, ROW_H) => {
            for (k, b) in buttons.iter().enumerate() {
                if inside(&button_rect(&c, top, k, buttons.len()), x, y) {
                    found = b.action + 1;
                }
            }
        }
        _ => {}
    });
    found
}

/// `winlux-control touchpad-set CLÉ VALEUR`.
unsafe fn set(key: &[u8], val: &[u8]) {
    let mut cmd = [0u8; 48];
    let mut l = 0;
    for part in [b"touchpad-set ".as_slice(), key, b" ", val] {
        cmd[l..l + part.len()].copy_from_slice(part);
        l += part.len();
    }
    if let Ok(c) = core::ffi::CStr::from_bytes_until_nul(&cmd[..=l]) {
        control(c);
    }
}

pub unsafe fn click(f: &Frame, x: i32, y: i32) -> bool {
    let id = hit(f, x, y);
    if id == 0 {
        return false;
    }
    let id = id - 1;
    let (kind, arg) = (id >> 8, id & 0xff);
    match kind {
        KEYBOARD => {
            control(if arg == 0 { c"keyboard-fr" } else { c"keyboard-us" });
        }
        TOGGLE => {
            let key = TOGGLES[(arg >> 1) as usize % TOGGLES.len()];
            set(key, if arg & 1 == 1 { b"1" } else { b"0" });
        }
        SPEED => {
            let cur = number(tp(b"speed"));
            let next = match arg {
                0 => cur.saturating_sub(10),
                1 => 50,
                _ => (cur + 10).min(100),
            };
            let mut t = [0u8; 4];
            let n = crate::gpuinfo::fmt_u32(&mut t, next);
            set(b"speed", &t[..n]);
        }
        _ => return false,
    }
    true
}

pub unsafe fn hover(f: &Frame, x: i32, y: i32) -> bool {
    let h = hit(f, x, y);
    let changed = h != HOVER;
    HOVER = h;
    changed
}
