//! Paramètres > Écran : écrans multiples (disposition, écran principal,
//! position), définition et mise à l'échelle par écran.
//!
//! Les données viennent de `winlux-display status` (lignes `display_*`) et
//! les boutons passent par `winlux-control display-set ...`. Garde-fous :
//! la disposition et la position n'existent qu'avec plusieurs écrans, le
//! suivi de la fenêtre seulement en machine virtuelle. Rien n'est grisé.

use super::{content, control, value, SCROLL};
use crate::gfx::*;
use crate::gpuinfo::{fmt_u32, Line};
use crate::wm::{Frame, Gfx};
use winlux_testexe::*;

const ROW_H: i32 = 58;
const HEADER_H: i32 = 34;
const BTN_H: i32 = 30;
const DIAGRAM_H: i32 = 150;
const MAX: usize = 4;

const LAYOUT: u32 = 1;
const PRIMARY: u32 = 2;
const PLACE: u32 = 3;
const MODE_PREV: u32 = 4;
const MODE_NEXT: u32 = 5;
const MODE_APPLY: u32 = 6;
const SCALE: u32 = 7;
const VM_POLICY: u32 = 8;
const SELECT: u32 = 9;

pub static mut HOVER: u32 = 0;
/// Écran sélectionné (0..MAX) et définition choisie pour chacun.
static mut SELECTED: usize = 0;
static mut MODE_SEL: [Option<usize>; MAX] = [None; MAX];

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
    Diagram,
}

/// `display_<n>_<champ>` (n à partir de 1).
unsafe fn field(n: usize, name: &[u8]) -> &'static [u8] {
    let mut key = [0u8; 40];
    let mut l = 0;
    for part in [b"display_".as_slice(), &[b'1' + n as u8], b"_", name] {
        let k = part.len().min(key.len() - l);
        key[l..l + k].copy_from_slice(&part[..k]);
        l += k;
    }
    value(&key[..l])
}

fn number(v: &[u8]) -> u32 {
    v.iter()
        .take_while(|b| b.is_ascii_digit())
        .fold(0u32, |n, b| n.saturating_mul(10).saturating_add(u32::from(b - b'0')))
}

/// « 1.25 » → 125.
fn percent(v: &[u8]) -> u32 {
    let (int, frac) = match v.iter().position(|&b| b == b'.') {
        Some(i) => (&v[..i], &v[i + 1..]),
        None => (v, &b""[..]),
    };
    let mut f = number(frac);
    if frac.len() == 1 {
        f *= 10;
    }
    number(int) * 100 + f.min(99)
}

unsafe fn count() -> usize {
    (number(value(b"display_count")) as usize).min(MAX)
}

fn nth(list: &[u8], i: usize) -> Option<&[u8]> {
    list.split(|&b| b == b',').filter(|m| !m.is_empty()).nth(i)
}

fn len(list: &[u8]) -> usize {
    list.split(|&b| b == b',').filter(|m| !m.is_empty()).count()
}

/// Définition affichée pour l'écran `n` : choix en cours ou mode actuel.
unsafe fn mode_index(n: usize) -> usize {
    let modes = field(n, b"modes");
    if let Some(i) = MODE_SEL[n] {
        return i.min(len(modes).saturating_sub(1));
    }
    let current = field(n, b"mode");
    modes
        .split(|&b| b == b',')
        .filter(|m| !m.is_empty())
        .position(|m| m == current)
        .unwrap_or(0)
}

unsafe fn items(mut visit: impl FnMut(Item)) {
    let n = count();
    if n == 0 {
        visit(Item::Row(
            tr4(&[b"Aucun \xE9cran d\xE9tect\xE9", b"No display detected", b"No se detect\xF3 ninguna pantalla", b"Kein Bildschirm erkannt"]),
            tr4(&[b"Le serveur graphique ne r\xE9pond pas encore.", b"The display server is not answering yet.", b"El servidor gr\xE1fico a\xFAn no responde.", b"Der Grafikserver antwortet noch nicht."]),
            &[],
        ));
        return;
    }
    SELECTED = SELECTED.min(n - 1);
    let s = SELECTED;
    let layout = value(b"display_layout");
    let extend = layout == b"extend";
    if n > 1 {
        visit(Item::Header(tr4(&[b"Plusieurs \xE9crans", b"Multiple displays", b"Varias pantallas", b"Mehrere Bildschirme"])));
        visit(Item::Diagram);
        let only = |i: usize| -> bool {
            layout.strip_prefix(b"only:") == Some(field(i, b"name"))
        };
        let only_labels: [&[u8]; MAX] = [
            tr4(&[b"Seulement 1", b"Only 1", b"Solo 1", b"Nur 1"]),
            tr4(&[b"Seulement 2", b"Only 2", b"Solo 2", b"Nur 2"]),
            tr4(&[b"Seulement 3", b"Only 3", b"Solo 3", b"Nur 3"]),
            tr4(&[b"Seulement 4", b"Only 4", b"Solo 4", b"Nur 4"]),
        ];
        let mut buttons = [btn(b"", 0, false); 2 + MAX];
        buttons[0] = btn(tr4(&[b"\xC9tendre", b"Extend", b"Extender", b"Erweitern"]), act(LAYOUT, 0), extend);
        buttons[1] = btn(tr4(&[b"Dupliquer", b"Duplicate", b"Duplicar", b"Duplizieren"]), act(LAYOUT, 1), layout == b"duplicate");
        // Deux écrans : « Seulement 1/2 » ; au-delà, l'écran sélectionné.
        let mut k = 2;
        for (i, label) in only_labels.iter().enumerate().take(n) {
            if n <= 2 || i == s {
                buttons[k] = btn(label, act(LAYOUT, 2 + i as u32), only(i));
                k += 1;
            }
        }
        visit(Item::Row(
            tr4(&[b"Disposition", b"Arrangement", b"Disposici\xF3n", b"Anordnung"]),
            tr4(&[b"Cliquez sur un \xE9cran du sch\xE9ma pour le r\xE9gler.", b"Click a display in the diagram to adjust it.", b"Haga clic en una pantalla del esquema para ajustarla.", b"Klicken Sie im Schema auf einen Bildschirm, um ihn einzustellen."]),
            &buttons[..k],
        ));
    }
    // Écran sélectionné.
    let mut title = Line::new();
    if n > 1 {
        title.push(tr4(&[b"\xC9cran ", b"Display ", b"Pantalla ", b"Bildschirm "]));
        let mut t = [0u8; 4];
        let k = fmt_u32(&mut t, s as u32 + 1);
        title.push(&t[..k]).push(b" - ");
    }
    title.push(field(s, b"name"));
    if field(s, b"internal") == b"1" {
        title.push(tr4(&[b" (int\xE9gr\xE9)", b" (built-in)", b" (integrada)", b" (integriert)"]));
    }
    visit(Item::Header(title.get()));
    let active = field(s, b"active") == b"1";
    if !active {
        let lid = value(b"display_lid_closed") == b"1" && field(s, b"internal") == b"1";
        visit(Item::Row(
            tr4(&[b"\xC9cran \xE9teint", b"Display off", b"Pantalla apagada", b"Bildschirm aus"]),
            if lid {
                tr4(&[b"Capot ferm\xE9 : l'image passe sur l'\xE9cran externe.", b"Lid closed: the picture moves to the external display.", b"Tapa cerrada: la imagen pasa a la pantalla externa.", b"Deckel zu: das Bild wechselt auf den externen Bildschirm."])
            } else {
                tr4(&[b"Choisissez \xAB \xC9tendre \xBB pour l'utiliser aussi.", b"Choose \xABExtend\xBB to use it as well.", b"Elija \xABExtender\xBB para usarla tambi\xE9n.", b"W\xE4hlen Sie \xBBErweitern\xAB, um ihn ebenfalls zu nutzen."])
            },
            &[],
        ));
        return;
    }
    if n > 1 && extend {
        let primary = field(s, b"primary") == b"1";
        let make: [Btn; 1] = [btn(tr4(&[b"D\xE9finir principal", b"Make main", b"Hacer principal", b"Als Haupt"]), act(PRIMARY, 0), false)];
        visit(Item::Row(
            tr4(&[b"\xC9cran principal", b"Main display", b"Pantalla principal", b"Hauptbildschirm"]),
            if primary {
                tr4(&[b"Oui : barre des t\xE2ches et nouvelles fen\xEAtres.", b"Yes: taskbar and new windows.", b"S\xED: barra de tareas y ventanas nuevas.", b"Ja: Taskleiste und neue Fenster."])
            } else {
                tr4(&[b"Non", b"No", b"No", b"Nein"])
            },
            if primary { &[] } else { &make },
        ));
        if !primary {
            let place = field(s, b"place");
            let places = [
                btn(tr4(&[b"\xC0 gauche", b"Left", b"Izquierda", b"Links"]), act(PLACE, 0), place == b"left"),
                btn(tr4(&[b"\xC0 droite", b"Right", b"Derecha", b"Rechts"]), act(PLACE, 1), place == b"right"),
                btn(tr4(&[b"Au-dessus", b"Above", b"Arriba", b"Oben"]), act(PLACE, 2), place == b"above"),
                btn(tr4(&[b"En dessous", b"Below", b"Abajo", b"Unten"]), act(PLACE, 3), place == b"below"),
            ];
            visit(Item::Row(
                tr4(&[b"Position", b"Position", b"Posici\xF3n", b"Position"]),
                tr4(&[b"Par rapport \xE0 l'\xE9cran principal.", b"Relative to the main display.", b"Respecto a la pantalla principal.", b"Relativ zum Hauptbildschirm."]),
                &places,
            ));
        }
    }
    // Définition.
    let modes = field(s, b"modes");
    if len(modes) > 0 {
        let i = mode_index(s);
        let chosen = nth(modes, i).unwrap_or(b"");
        let mut l = Line::new();
        l.push(chosen);
        if chosen == field(s, b"mode") {
            l.push(tr4(&[b"  (actuelle)", b"  (current)", b"  (actual)", b"  (aktuell)"]));
        }
        let buttons = [
            btn(b"<", act(MODE_PREV, 0), false),
            btn(b">", act(MODE_NEXT, 0), false),
            btn(tr4(&[b"Appliquer", b"Apply", b"Aplicar", b"\xDCbernehmen"]), act(MODE_APPLY, 0), false),
        ];
        let line = l;
        visit(Item::Row(
            tr4(&[b"R\xE9solution de l'\xE9cran", b"Display resolution", b"Resoluci\xF3n de pantalla", b"Bildschirmaufl\xF6sung"]),
            line.get(),
            &buttons,
        ));
    }
    // Mise à l'échelle.
    let mut l = Line::new();
    l.num(percent(field(s, b"scale"))).push(b" %");
    if field(s, b"scale_auto") == b"1" {
        l.push(tr4(&[b" (recommand\xE9)", b" (recommended)", b" (recomendado)", b" (empfohlen)"]));
    }
    let auto = field(s, b"scale_auto") == b"1";
    let scales = [
        btn(b"-", act(SCALE, 0), false),
        btn(tr4(&[b"Recommand\xE9", b"Recommended", b"Recomendado", b"Empfohlen"]), act(SCALE, 1), auto),
        btn(b"+", act(SCALE, 2), false),
    ];
    let line = l;
    visit(Item::Row(
        tr4(&[b"Mise \xE0 l'\xE9chelle", b"Scale", b"Escala", b"Skalierung"]),
        line.get(),
        &scales,
    ));
    if value(b"display_restart_hint") == b"1" {
        visit(Item::Row(
            tr4(&[b"Image plus nette", b"Sharper picture", b"Imagen m\xE1s n\xEDtida", b"Sch\xE4rferes Bild"]),
            tr4(&[b"D\xE9j\xE0 appliqu\xE9 ; fermez puis rouvrez la session pour un texte plus net.", b"Already applied; sign out and back in for crisper text.", b"Ya aplicado; cierre y vuelva a abrir la sesi\xF3n para un texto m\xE1s n\xEDtido.", b"Bereits aktiv; melden Sie sich ab und wieder an f\xFCr sch\xE4rferen Text."]),
            &[],
        ));
    }
    // Machine virtuelle à un seul écran : suivre la taille de la fenêtre.
    if n == 1 && value(b"display_virtual") == b"1" {
        let fixed = value(b"display_policy") == b"fixed";
        let policies = [
            btn(tr4(&[b"Suivre la fen\xEAtre", b"Follow window", b"Seguir ventana", b"Fenster folgen"]), act(VM_POLICY, 0), !fixed),
            btn(tr4(&[b"Fixe", b"Fixed", b"Fija", b"Fest"]), act(VM_POLICY, 1), fixed),
        ];
        visit(Item::Row(
            tr4(&[b"Machine virtuelle", b"Virtual machine", b"M\xE1quina virtual", b"Virtuelle Maschine"]),
            tr4(&[b"La r\xE9solution peut suivre la taille de la fen\xEAtre.", b"The resolution can follow the window size.", b"La resoluci\xF3n puede seguir el tama\xF1o de la ventana.", b"Die Aufl\xF6sung kann der Fenstergr\xF6\xDFe folgen."]),
            &policies,
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
        Item::Diagram => DIAGRAM_H,
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

/// Rectangles des écrans dans le schéma, mis à l'échelle et centrés.
unsafe fn diagram_rects(c: &Rect, top: i32, out: &mut [Rect; MAX]) -> usize {
    let n = count();
    let mut r = [[0i32; 4]; MAX];
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    let mut shown = 0;
    for (i, slot) in r.iter_mut().enumerate().take(n) {
        let v = field(i, b"rect");
        let mut parts = v.split(|&b| b == b',').map(number);
        let (Some(x), Some(y), Some(w), Some(h)) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        if w == 0 || h == 0 {
            continue;
        }
        *slot = [x as i32, y as i32, w as i32, h as i32];
        min_x = min_x.min(x as i32);
        min_y = min_y.min(y as i32);
        max_x = max_x.max((x + w) as i32);
        max_y = max_y.max((y + h) as i32);
        shown += 1;
    }
    if shown == 0 {
        return 0;
    }
    let box_w = (c.right - c.left - 40).max(100);
    let box_h = DIAGRAM_H - 30;
    let (vw, vh) = ((max_x - min_x).max(1), (max_y - min_y).max(1));
    // Échelle commune en millièmes pour garder les proportions.
    let k = (box_w * 1000 / vw).min(box_h * 1000 / vh);
    let ox = c.left + (c.right - c.left - vw * k / 1000) / 2;
    let oy = top + 10 + (box_h - vh * k / 1000) / 2;
    for i in 0..n {
        let [x, y, w, h] = r[i];
        out[i] = if w == 0 {
            rect(0, 0, 0, 0)
        } else {
            rect(
                ox + (x - min_x) * k / 1000 + 2,
                oy + (y - min_y) * k / 1000 + 2,
                ox + (x - min_x + w) * k / 1000 - 2,
                oy + (y - min_y + h) * k / 1000 - 2,
            )
        };
    }
    n
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
        Item::Diagram => {
            if !visible(&c, y, DIAGRAM_H) {
                return;
            }
            round_rect(g.hdc, g.b, c.left - 8, y, c.right + 8, y + DIAGRAM_H - 6, 8, p.border);
            round_rect(g.hdc, g.b, c.left - 7, y + 1, c.right + 7, y + DIAGRAM_H - 7, 7, p.surface);
            let mut rects = [rect(0, 0, 0, 0); MAX];
            let n = diagram_rects(&c, y, &mut rects);
            for (i, r) in rects.iter().enumerate().take(n) {
                if r.right <= r.left {
                    continue;
                }
                let sel = i == SELECTED;
                round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 6, if sel { p.accent } else { p.border });
                round_rect(
                    g.hdc,
                    g.b,
                    r.left + 2,
                    r.top + 2,
                    r.right - 2,
                    r.bottom - 2,
                    5,
                    if sel {
                        p.selected
                    } else if HOVER == act(SELECT, i as u32) + 1 {
                        p.hover
                    } else {
                        p.surface2
                    },
                );
                let mut t = [0u8; 4];
                let k = fmt_u32(&mut t, i as u32 + 1);
                let cy = (r.top + r.bottom) / 2;
                text_centered(g.hdc, g.f.large, 20, p.text, (r.left + r.right) / 2, cy - 16, &t[..k]);
                if field(i, b"primary") == b"1" && r.bottom - r.top > 40 {
                    let label = tr4(&[b"principal", b"main", b"principal", b"Haupt"]);
                    text_centered(g.hdc, g.f.small, 11, p.text2, (r.left + r.right) / 2, cy + 10, label);
                }
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
        Item::Diagram if visible(&c, top, DIAGRAM_H) => {
            let mut rects = [rect(0, 0, 0, 0); MAX];
            let n = diagram_rects(&c, top, &mut rects);
            for (i, r) in rects.iter().enumerate().take(n) {
                if r.right > r.left && inside(r, x, y) {
                    found = act(SELECT, i as u32) + 1;
                }
            }
        }
        _ => {}
    });
    found
}

/// `winlux-control display-set <mots...>` (noms et modes : ASCII simple).
unsafe fn set(words: &[&[u8]]) {
    let mut cmd = [0u8; 96];
    let mut l = 0;
    for (i, w) in [b"display-set".as_slice()].iter().chain(words.iter()).enumerate() {
        if i > 0 {
            cmd[l] = b' ';
            l += 1;
        }
        if l + w.len() + 2 > cmd.len() || w.iter().any(|b| !(b.is_ascii_alphanumeric() || b"._:x-".contains(b))) {
            return;
        }
        cmd[l..l + w.len()].copy_from_slice(w);
        l += w.len();
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
    let (kind, arg) = (id >> 8, (id & 0xff) as usize);
    let s = SELECTED;
    let name = field(s, b"name");
    match kind {
        SELECT => {
            SELECTED = arg.min(MAX - 1);
            return true;
        }
        LAYOUT => match arg {
            0 => set(&[b"layout", b"extend"]),
            1 => set(&[b"layout", b"duplicate"]),
            i => set(&[b"layout", b"only", field(i - 2, b"name")]),
        },
        PRIMARY => set(&[b"primary", name]),
        PLACE => set(&[b"place", name, [b"left".as_slice(), b"right", b"above", b"below"][arg % 4]]),
        MODE_PREV => {
            MODE_SEL[s] = Some(mode_index(s).saturating_sub(1));
            return true;
        }
        MODE_NEXT => {
            let n = len(field(s, b"modes"));
            MODE_SEL[s] = Some((mode_index(s) + 1).min(n.saturating_sub(1)));
            return true;
        }
        MODE_APPLY => {
            if let Some(m) = nth(field(s, b"modes"), mode_index(s)) {
                set(&[b"mode", name, m]);
            }
            MODE_SEL[s] = None;
        }
        SCALE => match arg {
            0 => set(&[b"scale-step", name, b"down"]),
            1 => set(&[b"scale", name, b"auto"]),
            _ => set(&[b"scale-step", name, b"up"]),
        },
        VM_POLICY => set(&[if arg == 0 { b"follow".as_slice() } else { b"fixed" }]),
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
