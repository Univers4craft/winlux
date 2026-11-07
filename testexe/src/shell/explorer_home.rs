//! Direction « Prisme » de l'Explorateur (#689) : l'Accueil en cartes
//! (accès rapide, lecteurs avec jauge, fichiers récents filtrables), le
//! volet Détails à droite et la pastille « Windows » des programmes .exe.
//!
//! L'Accueil garde le modèle des autres dossiers : ses entrées sont, dans
//! l'ordre, les six dossiers personnels, les lecteurs (comme « Ce PC »), puis
//! les fichiers récents (chemin Windows complet dans le nom). Seules la mise
//! en page et la peinture changent ; sélection, menus, glisser et clavier
//! restent ceux de l'Explorateur.
//!
//! « Récents » est calculé sur place, sans journal : les fichiers modifiés le
//! plus récemment au premier niveau des dossiers personnels.

use super::*;

pub(super) const RECENT_MAX: usize = 8;
const PAD: i32 = 20;
const GAP: i32 = 12;
const QUICK_H: i32 = 64;
const DRIVE_H: i32 = 76;
const ROW_H: i32 = 36;
pub(super) const DETAILS_W: i32 = 300;

/// Filtre des récents : 0 tout, 1 documents, 2 images, 3 programmes
/// Windows, 4 archives.
static mut FILTER: u8 = 0;
/// Fichiers de Téléchargements arrivés depuis moins de 24 h.
static mut NEW_DOWNLOADS: u32 = 0;
/// Volet Détails affiché (bouton « Détails » de la barre de commandes).
pub(super) static mut SHOW_DETAILS: bool = false;

const FILTERS: [Tr4; 5] = [
    [b"Tout", b"All", b"Todo", b"Alle"],
    [b"Documents", b"Documents", b"Documentos", b"Dokumente"],
    [b"Images", b"Pictures", b"Im\xE1genes", b"Bilder"],
    [b"Programmes Windows", b"Windows programs", b"Programas de Windows", b"Windows-Programme"],
    [b"Archives", b"Archives", b"Archivos comprimidos", b"Archive"],
];

/// L'Accueil se dessine en cartes (pas pendant une recherche profonde).
pub(super) unsafe fn cards(p: &Pane) -> bool {
    p.loc.is(HOME) && !p.deep
}

fn is_drive(k: Kind) -> bool {
    matches!(k, Kind::Drive | Kind::LinuxDrive | Kind::Removable | Kind::Network)
}

/// 0 dossier personnel, 1 lecteur, 2 fichier récent.
fn section(e: &Entry) -> u8 {
    if is_drive(e.kind) {
        1
    } else if e.dir {
        0
    } else {
        2
    }
}

/// Une entrée d'Accueil est-elle un dossier personnel ?
pub(super) fn is_place(e: &Entry) -> bool {
    section(e) == 0
}

/// Une entrée d'Accueil est-elle un fichier récent (chemin complet) ?
pub(super) fn is_recent(e: &Entry) -> bool {
    section(e) == 2
}

/// Le filtre des récents laisse-t-il passer cette entrée ?
pub(super) unsafe fn passes(e: &Entry) -> bool {
    if section(e) != 2 {
        return true;
    }
    match FILTER {
        1 => matches!(e.kind, Kind::Text | Kind::Pdf | Kind::Code),
        2 => e.kind == Kind::Image,
        3 => e.kind == Kind::Exe,
        4 => matches!(e.kind, Kind::Archive | Kind::DiskImage),
        _ => true,
    }
}

fn now_filetime() -> u64 {
    let mut t = 0u64;
    unsafe { GetSystemTimeAsFileTime(&mut t) };
    t
}

/// Ajoute à l'Accueil les lecteurs puis les fichiers récents.
pub(super) unsafe fn fill_home(p: &mut Pane) {
    add_entry(
        p,
        tr4(&[b"Dossier personnel (C:)", b"Home folder (C:)", b"Carpeta personal (C:)", b"Pers\xF6nlicher Ordner (C:)"]),
        true,
        false,
        0,
        0,
        Some(Kind::Drive),
    );
    add_entry(
        p,
        tr4(&[b"Syst\xE8me Linux (Z:)", b"Linux system (Z:)", b"Sistema Linux (Z:)", b"Linux-System (Z:)"]),
        true,
        false,
        0,
        0,
        Some(Kind::LinuxDrive),
    );
    refresh_drives();
    for i in 0..NDRIVES {
        let kind = if DRIVES[i].network { Kind::Network } else { Kind::Removable };
        add_entry(p, drive_label(i), true, false, 0, 0, Some(kind));
    }
    let mut best = [(0u64, 0u64, [0u8; PATH_LEN], 0u16); RECENT_MAX];
    let mut nb = 0usize;
    NEW_DOWNLOADS = 0;
    let day_ago = now_filetime().saturating_sub(24 * 3600 * 10_000_000);
    for k in 0..PLACES.len() {
        let dir = place_path(k);
        scan(dir.get(), &mut best, &mut nb, if k == 1 { day_ago } else { u64::MAX });
    }
    for (mtime, size, name, len) in best[..nb].iter() {
        add_entry(p, &name[..*len as usize], false, false, *size, *mtime, None);
    }
}

/// Garde les `RECENT_MAX` fichiers les plus récents du premier niveau de `dir`.
unsafe fn scan(dir: &[u8], best: &mut [(u64, u64, [u8; PATH_LEN], u16); RECENT_MAX], nb: &mut usize, fresh_after: u64) {
    let mut wpattern = [0u16; PATH_LEN + NAME_LEN + 2];
    if wjoin(&mut wpattern, dir, b"*") == 0 {
        return;
    }
    let mut data: Win32FindDataW = core::mem::zeroed();
    let h = FindFirstFileW(wpattern.as_ptr(), &mut data);
    if h as isize == -1 {
        return;
    }
    loop {
        let name = find_name_w(&data);
        let skip = name.is_empty()
            || name[0] == b'.'
            || data.attributes & 0x12 != 0
            || !valid_component(name);
        if !skip {
            if data.write_time > fresh_after {
                NEW_DOWNLOADS += 1;
            }
            let mut full = [0u8; PATH_LEN + NAME_LEN + 2];
            let n = join(&mut full, dir, name);
            if n > 0 && n < PATH_LEN && valid_path(&full[..n]) {
                let size = ((data.size_high as u64) << 32) | data.size_low as u64;
                let mtime = data.write_time;
                // Insertion triée, du plus récent au plus ancien.
                let mut at = *nb;
                while at > 0 && best[at - 1].0 < mtime {
                    at -= 1;
                }
                if at < RECENT_MAX {
                    let last = (*nb).min(RECENT_MAX - 1);
                    let mut i = last;
                    while i > at {
                        best[i] = best[i - 1];
                        i -= 1;
                    }
                    let mut slot = (mtime, size, [0u8; PATH_LEN], n as u16);
                    slot.2[..n].copy_from_slice(&full[..n]);
                    best[at] = slot;
                    if *nb < RECENT_MAX {
                        *nb += 1;
                    }
                }
            }
        }
        if FindNextFileW(h, &mut data) == 0 {
            break;
        }
    }
    FindClose(h);
}

fn recent_label() -> &'static [u8] {
    tr4(&[b"R\xE9cents", b"Recent", b"Recientes", b"Zuletzt verwendet"])
}

// ---------- Mise en page ----------

#[derive(Clone, Copy)]
pub(super) struct Lay {
    x: i32,
    w: i32,
    title_y: i32,
    quick_head: i32,
    quick_y: i32,
    qcols: i32,
    qw: i32,
    drive_head: i32,
    drive_y: i32,
    dcols: i32,
    dw: i32,
    recent_head: i32,
    chips_y: i32,
    recent_y: i32,
    counts: (usize, usize, usize),
    end: i32,
}

unsafe fn counts(p: &Pane) -> (usize, usize, usize) {
    let mut c = (0, 0, 0);
    for v in 0..p.nview {
        match section(&p.entries[p.view[v] as usize]) {
            0 => c.0 += 1,
            1 => c.1 += 1,
            _ => c.2 += 1,
        }
    }
    c
}

fn chips_width() -> i32 {
    FILTERS.iter().map(|f| text_width(tr4(f), 12) + 24 + 6).sum::<i32>() - 6
}

pub(super) unsafe fn layout(p: &Pane, r: &Rect) -> Lay {
    let x = r.left + PAD;
    let w = (r.right - SCROLL_W - PAD - x).max(120);
    let n = counts(p);
    let mut y = r.top + 8 - p.scroll;
    let title_y = y + 6;
    y += 52;
    let quick_head = y;
    y += 30;
    let quick_y = y;
    let qcols = ((w + GAP) / (190 + GAP)).clamp(1, 6);
    let qw = ((w - GAP * (qcols - 1)) / qcols).min(300);
    let qrows = (n.0 as i32 + qcols - 1) / qcols;
    y += qrows * (QUICK_H + GAP) + 10;
    let drive_head = y;
    y += 30;
    let drive_y = y;
    let dcols = ((w + GAP) / (260 + GAP)).clamp(1, 3);
    let dw = (w - GAP * (dcols - 1)) / dcols;
    let drows = (n.1 as i32 + dcols - 1) / dcols;
    y += drows * (DRIVE_H + GAP) + 10;
    let recent_head = y;
    let label_w = text_width(recent_label(), 15) + 24;
    let chips_y = if label_w + chips_width() <= w { y - 4 } else { y + 28 };
    let chip_rows = if chips_y < y { 1 } else { wrap_chips(l_dummy(x, w), chips_y).1 };
    y = chips_y + 34 * chip_rows + 4;
    let recent_y = y;
    let rows = if n.2 == 0 { 1 } else { n.2 as i32 };
    y += rows * ROW_H + 12;
    Lay {
        x,
        w,
        title_y,
        quick_head,
        quick_y,
        qcols,
        qw,
        drive_head,
        drive_y,
        dcols,
        dw,
        recent_head,
        chips_y,
        recent_y,
        counts: n,
        end: y,
    }
}

/// Hauteur totale du contenu d'Accueil (pour le défilement).
pub(super) unsafe fn height(p: &Pane, r: &Rect) -> i32 {
    let l = layout(p, r);
    l.end - (r.top + 8 - p.scroll) + 16
}

/// Rectangle de l'entrée `v` (indice dans la vue).
pub(super) unsafe fn item_rect(p: &Pane, r: &Rect, v: usize) -> Rect {
    let l = layout(p, r);
    item_rect_in(&l, v)
}

fn item_rect_in(l: &Lay, v: usize) -> Rect {
    let (q, d, _) = l.counts;
    if v < q {
        let (col, row) = (v as i32 % l.qcols, v as i32 / l.qcols);
        let x = l.x + col * (l.qw + GAP);
        let y = l.quick_y + row * (QUICK_H + GAP);
        rect(x, y, x + l.qw, y + QUICK_H)
    } else if v < q + d {
        let i = (v - q) as i32;
        let (col, row) = (i % l.dcols, i / l.dcols);
        let x = l.x + col * (l.dw + GAP);
        let y = l.drive_y + row * (DRIVE_H + GAP);
        rect(x, y, x + l.dw, y + DRIVE_H)
    } else {
        let y = l.recent_y + 6 + (v - q - d) as i32 * ROW_H;
        rect(l.x + 6, y, l.x + l.w - 6, y + ROW_H)
    }
}

pub(super) unsafe fn item_at(p: &Pane, r: &Rect, x: i32, y: i32) -> Option<usize> {
    if !inside(r, x, y) {
        return None;
    }
    let l = layout(p, r);
    (0..p.nview).find(|&v| inside(&item_rect_in(&l, v), x, y))
}

fn chip_rects(l: &Lay) -> [Rect; 5] {
    if l.chips_y > l.recent_head {
        return wrap_chips((l.x, l.w), l.chips_y).0;
    }
    let mut out = [rect(0, 0, 0, 0); 5];
    let mut x = l.x + l.w - chips_width();
    for (i, f) in FILTERS.iter().enumerate() {
        let w = text_width(tr4(f), 12) + 24;
        out[i] = rect(x, l.chips_y, x + w, l.chips_y + 28);
        x += w + 6;
    }
    out
}

fn l_dummy(x: i32, w: i32) -> (i32, i32) {
    (x, w)
}

/// Filtres sous le titre « Récents », sur plusieurs lignes si besoin
/// (fenêtre étroite, allemand) : (rectangles, nombre de lignes).
fn wrap_chips((x0, w): (i32, i32), y0: i32) -> ([Rect; 5], i32) {
    let mut out = [rect(0, 0, 0, 0); 5];
    let (mut x, mut y, mut rows) = (x0, y0, 1);
    for (i, f) in FILTERS.iter().enumerate() {
        let cw = (text_width(tr4(f), 12) + 24).min(w);
        if x > x0 && x + cw > x0 + w {
            x = x0;
            y += 34;
            rows += 1;
        }
        out[i] = rect(x, y, x + cw, y + 28);
        x += cw + 6;
    }
    (out, rows)
}

/// Clic sur un filtre des récents : vrai s'il a été pris.
pub(super) unsafe fn click_chip(p: &mut Pane, r: &Rect, x: i32, y: i32) -> bool {
    if !cards(p) {
        return false;
    }
    let l = layout(p, r);
    for (i, c) in chip_rects(&l).iter().enumerate() {
        if inside(c, x, y) {
            FILTER = i as u8;
            p.sel = [0; MAX_ENTRIES / 64];
            rebuild_view(p);
            return true;
        }
    }
    false
}

// ---------- Dessin ----------

/// Carte arrondie : bord puis fond.
pub(super) unsafe fn card(g: &mut Gfx, r: &Rect, bg: u32, edge: u32) {
    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 8, edge);
    round_rect(g.hdc, g.b, r.left + 1, r.top + 1, r.right - 1, r.bottom - 1, 7, bg);
}

/// Pastille « Windows » des programmes .exe (lancés directement par Winlux).
pub(super) fn badge_width() -> i32 {
    text_width(badge_label(), 11) + 14
}
fn badge_label() -> &'static [u8] {
    b"Windows"
}
pub(super) unsafe fn badge(g: &mut Gfx, x: i32, y: i32, bg: u32) {
    let p = pal();
    let tint = mix(bg, p.accent, 3, 16);
    let w = badge_width();
    round_rect(g.hdc, g.b, x, y, x + w, y + 18, 9, tint);
    let fg = readable(p.accent, &[tint], 4500);
    text(g.hdc, g.f.small, fg, x + 7, y + 2, badge_label());
}

/// Nom suivi de la pastille « Windows » pour un .exe, sinon nom seul.
pub(super) unsafe fn name_badge(g: &mut Gfx, e: &Entry, x: i32, y: i32, name: &[u8], max_w: i32, color: u32, bg: u32) {
    if e.dir || e.kind != Kind::Exe || max_w < badge_width() + 60 {
        text_fit(g.hdc, g.f.caption, 12, color, x, y, name, max_w);
        return;
    }
    let room = max_w - badge_width() - 8;
    text_fit(g.hdc, g.f.caption, 12, color, x, y, name, room);
    let w = text_width(name, 12).min(room);
    badge(g, x + w + 8, y - 2, bg);
}

pub(super) unsafe fn draw(g: &mut Gfx, pn: &Pane, r: &Rect, which: usize, focused: bool) {
    let p = pal();
    let t = tab();
    let l = layout(pn, r);
    // Le GDI du runtime ne découpe pas : ce qui a défilé au-dessus du volet
    // n'est pas dessiné. La barre de commandes, peinte après les volets,
    // recouvre seulement ce qui dépasse d'une ligne (#738, #757).
    let shown = |top: i32, bottom: i32| bottom > r.top && top < r.bottom;
    if shown(l.title_y, l.title_y + 40) {
        text(g.hdc, g.f.large, p.text, l.x, l.title_y, tr(S_HOME));
    }
    for (y, label) in [
        (l.quick_head, tr4(&[b"Acc\xE8s rapide", b"Quick access", b"Acceso r\xE1pido", b"Schnellzugriff"])),
        (l.drive_head, tr4(&[b"Lecteurs", b"Drives", b"Unidades", b"Laufwerke"])),
        (l.recent_head, recent_label()),
    ] {
        if shown(y, y + 28) {
            text(g.hdc, g.f.title, p.text, l.x, y, label);
        }
    }
    // Filtres des récents.
    for (i, c) in chip_rects(&l).iter().enumerate() {
        if !shown(c.top, c.bottom) {
            continue;
        }
        let on = FILTER as usize == i;
        let (bg, edge) = if on {
            (p.accent, p.accent)
        } else if hovered(c) {
            (p.hover, p.border)
        } else {
            (p.surface, p.border)
        };
        round_rect(g.hdc, g.b, c.left, c.top, c.right, c.bottom, 14, edge);
        round_rect(g.hdc, g.b, c.left + 1, c.top + 1, c.right - 1, c.bottom - 1, 13, bg);
        let fg = if on { p.on_accent } else { p.text2 };
        text(g.hdc, g.f.caption, fg, c.left + 12, c.top + 7, tr4(&FILTERS[i]));
    }
    // Cadre des récents.
    let (q, d, n) = l.counts;
    let frame = rect(l.x, l.recent_y, l.x + l.w, l.end - 12);
    // Cadre haut d'une longue liste : son bord défilé reste sous la barre.
    let framed = rect(frame.left, frame.top.max(r.top - 24), frame.right, frame.bottom.min(r.bottom + 24));
    if framed.bottom > framed.top {
        card(g, &framed, p.surface, p.border);
    }
    if n == 0 && shown(frame.top + 16, frame.top + 32) {
        text(
            g.hdc,
            g.f.caption,
            p.text3,
            frame.left + 16,
            frame.top + 16,
            tr4(&[b"Aucun fichier r\xE9cent ici.", b"No recent files here.", b"No hay archivos recientes aqu\xED.", b"Keine zuletzt verwendeten Dateien."]),
        );
    }
    let mut dbuf = [0u8; 24];
    for v in 0..pn.nview {
        let ir = item_rect_in(&l, v);
        if ir.bottom < r.top || ir.top > r.bottom {
            continue;
        }
        let idx = pn.view[v] as usize;
        let e = pn.entries[idx];
        let selected = is_sel(pn, idx);
        let hover = HOVER == v as i32 && HOVER_ZONE == which as u32 + 1;
        let base = p.surface;
        let bg = if selected {
            if focused { p.selected } else { mix(p.selected, base, 1, 2) }
        } else if hover {
            p.hover
        } else {
            base
        };
        let renaming = FOCUS == Focus::Rename && RENAME_INDEX == idx && which == t.focus;
        set_glyph_bg(Some(bg));
        if v < q {
            card(g, &ir, bg, if selected { p.accent } else { p.border });
            let k = (0..PLACES.len()).find(|&k| place_label(k) == e.name());
            let drawn = k.is_some_and(|k| icons::draw(g.hdc, icons::of_place(k), ir.left + 14, ir.top + 16, 32, 0));
            if !drawn {
                glyph(g.hdc, g.b, g.f, Glyph::Folder, ir.left + 14, ir.top + 16, 32);
            }
            let tx = ir.left + 58;
            if renaming {
                draw_rename(g, rect(tx - 4, ir.top + 10, ir.right - 8, ir.top + 32));
            } else {
                text_fit(g.hdc, g.f.semibold, 13, p.text, tx, ir.top + 14, e.name(), ir.right - tx - 10);
            }
            let mut sub = [0u8; 48];
            let sub_n = if k == Some(1) && NEW_DOWNLOADS > 0 {
                let mut m = write_num(&mut sub, 0, NEW_DOWNLOADS as u64);
                let word = if NEW_DOWNLOADS == 1 {
                    tr4(&[b" nouveau", b" new", b" nuevo", b" neu"])
                } else {
                    tr4(&[b" nouveaux", b" new", b" nuevos", b" neu"])
                };
                for &c in word {
                    sub[m] = c;
                    m += 1;
                }
                m
            } else {
                let s = tr4(&[b"\xC9pingl\xE9", b"Pinned", b"Anclado", b"Angeheftet"]);
                sub[..s.len()].copy_from_slice(s);
                s.len()
            };
            text_fit(g.hdc, g.f.small, 11, p.text3, tx, ir.top + 36, &sub[..sub_n], ir.right - tx - 10);
        } else if v < q + d {
            card(g, &ir, bg, if selected { p.accent } else { p.border });
            let ic = icons::of_glyph(glyph_of(e.kind));
            if !ic.is_some_and(|ic| icons::draw(g.hdc, ic, ir.left + 16, ir.top + 18, 40, 0)) {
                glyph(g.hdc, g.b, g.f, glyph_of(e.kind), ir.left + 16, ir.top + 18, 40);
            }
            let tx = ir.left + 70;
            let tw = ir.right - tx - 16;
            text_fit(g.hdc, g.f.semibold, 13, p.text, tx, ir.top + 14, e.name(), tw);
            if let Some((free, total)) = drive_space(&e) {
                usage_bar(g, tx, ir.top + 38, tw, free, total);
                let mut buf = [0u8; 64];
                let n = space_text(free, total, &mut buf);
                text_fit(g.hdc, g.f.small, 11, p.text3, tx, ir.top + 50, &buf[..n], tw);
            } else {
                let mut tb = [0u8; 48];
                text_fit(g.hdc, g.f.small, 11, p.text3, tx, ir.top + 40, type_name(&e, &mut tb), tw);
            }
        } else {
            if bg != base {
                round_rect(g.hdc, g.b, ir.left, ir.top, ir.right, ir.bottom, 4, bg);
            }
            row_icon(g, e.kind, ir.left + 10, ir.top + 10);
            let name = last_component(e.name());
            let date_w = 150;
            let loc_w = ((ir.right - ir.left) / 4).clamp(0, 220);
            let name_w = ir.right - ir.left - 40 - loc_w - date_w;
            let ty = ir.top + 11;
            name_badge(g, &e, ir.left + 36, ty, name, name_w, p.text, bg);
            // Dossier qui le contient, nommé comme dans le volet de navigation.
            let folder = parent(e.name()).unwrap_or(b"");
            let label = (0..PLACES.len())
                .find(|&k| place_path(k).get() == folder)
                .map(place_label)
                .unwrap_or_else(|| last_component(folder));
            let lx = ir.right - date_w - loc_w;
            if loc_w > 40 {
                text_fit(g.hdc, g.f.caption, 12, p.text2, lx, ty, label, loc_w - 12);
            }
            let dn = date_text(e.mtime, &mut dbuf);
            text_fit(g.hdc, g.f.caption, 12, p.text2, ir.right - date_w, ty, &dbuf[..dn], date_w - 8);
        }
        set_glyph_bg(None);
    }
}

// ---------- Volet Détails ----------

/// Rectangle du volet Détails dans le corps de la fenêtre, s'il a la place.
pub(super) unsafe fn details_rect(body: &Rect) -> Option<Rect> {
    if !SHOW_DETAILS || body.right - body.left < DETAILS_W + 420 {
        return None;
    }
    Some(rect(body.right - DETAILS_W, body.top, body.right, body.bottom))
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Action {
    Open,
    ExeSettings,
    Properties,
}

struct DLay {
    preview: Rect,
    actions: [(Action, Rect); 3],
    nactions: usize,
    info_y: i32,
}

unsafe fn dlayout(d: &Rect, pn: &Pane) -> DLay {
    let x = d.left + 18;
    let w = d.right - 18 - x;
    // Fenêtre basse (1366×768, fenêtre non agrandie) : aperçu réduit.
    let ph = if d.bottom - d.top < 600 { 96 } else { 150 };
    let preview = rect(x, d.top + 16, x + w, d.top + 16 + ph);
    let mut y = preview.bottom + 64;
    let mut actions = [(Action::Open, rect(0, 0, 0, 0)); 3];
    let mut n = 0;
    if let Some(i) = single(pn) {
        let e = pn.entries[i];
        if e.kind == Kind::Exe && !e.dir {
            y += 58;
        }
        actions[0] = (Action::Open, rect(x, y, x + w, y + 34));
        n = 1;
        y += 42;
        let half = (w - 8) / 2;
        if e.kind == Kind::Exe && !e.dir {
            actions[1] = (Action::ExeSettings, rect(x, y, x + half, y + 34));
            actions[2] = (Action::Properties, rect(x + half + 8, y, x + w, y + 34));
            n = 3;
        } else if !cards(pn) && !pn.loc.is(PC) {
            actions[1] = (Action::Properties, rect(x, y, x + w, y + 34));
            n = 2;
        }
        y += 50;
    }
    DLay { preview, actions, nactions: n, info_y: y }
}

/// Entrée unique sélectionnée dans le volet actif.
unsafe fn single(pn: &Pane) -> Option<usize> {
    if sel_count(pn) == 1 { first_sel(pn) } else { None }
}

fn action_label(a: Action) -> &'static [u8] {
    match a {
        Action::Open => tr4(&[b"Ouvrir", b"Open", b"Abrir", b"\xD6ffnen"]),
        Action::ExeSettings => tr4(&[b"Compatibilit\xE9", b"Compatibility", b"Compatibilidad", b"Kompatibilit\xE4t"]),
        Action::Properties => tr4(&[b"Propri\xE9t\xE9s", b"Properties", b"Propiedades", b"Eigenschaften"]),
    }
}

pub(super) unsafe fn details_hit(d: &Rect, x: i32, y: i32) -> Option<Action> {
    let l = dlayout(d, pane());
    l.actions[..l.nactions].iter().find(|(_, r)| inside(r, x, y)).map(|(a, _)| *a)
}

unsafe fn kv(g: &mut Gfx, d: &Rect, y: i32, key: &[u8], value: &[u8]) -> i32 {
    let p = pal();
    let x = d.left + 18;
    let w = d.right - 18 - x;
    text(g.hdc, g.f.caption, p.text3, x, y, key);
    let kw = text_width(key, 12) + 16;
    let vw = text_width(value, 12).min(w - kw);
    text_fit(g.hdc, g.f.caption, 12, p.text, d.right - 18 - vw, y, value, w - kw);
    y + 26
}

pub(super) unsafe fn draw_details(g: &mut Gfx, d: &Rect) {
    let p = pal();
    let pn = pane();
    fill(g.hdc, g.b, d.left, d.top, d.right, d.bottom, p.window);
    fill(g.hdc, g.b, d.left, d.top, d.left + 1, d.bottom, p.border);
    let l = dlayout(d, pn);
    let pv = l.preview;
    let tint = mix(p.surface, p.accent, 1, 10);
    card(g, &pv, tint, p.border);
    let x = d.left + 18;
    let w = d.right - 18 - x;
    let mut buf = [0u8; 64];
    let mut tb = [0u8; 48];
    let s = if pv.bottom - pv.top < 120 { 64 } else { 96 };
    let (gx, gy) = ((pv.left + pv.right - s) / 2, (pv.top + pv.bottom - s) / 2);
    set_glyph_bg(Some(tint));
    match single(pn) {
        Some(i) => {
            let e = pn.entries[i];
            if !draw_thumbnail(g, pn, i, gx, gy, s, tint) {
                let place = if pn.loc.is(HOME) {
                    (0..PLACES.len()).find(|&k| place_label(k) == e.name())
                } else {
                    None
                };
                let ic = place.map(icons::of_place).or_else(|| icons::of_glyph(glyph_of(e.kind)));
                if !ic.is_some_and(|ic| icons::draw(g.hdc, ic, gx, gy, s, 0)) {
                    glyph(g.hdc, g.b, g.f, glyph_of(e.kind), gx, gy, s);
                }
            }
            set_glyph_bg(None);
            let name = if is_recent(&e) { last_component(e.name()) } else { display_name(&e) };
            text_fit(g.hdc, g.f.title, 15, p.text, x, pv.bottom + 14, name, w);
            text_fit(g.hdc, g.f.caption, 12, p.text3, x, pv.bottom + 36, type_name(&e, &mut tb), w);
            if e.kind == Kind::Exe && !e.dir {
                let info = rect(x, pv.bottom + 64, x + w, pv.bottom + 112);
                card(g, &info, p.surface, p.border);
                let ok = readable(p.accent, &[p.surface], 3000);
                line(g.hdc, g.b, info.left + 12, info.top + 18, info.left + 16, info.top + 22, 2, ok);
                line(g.hdc, g.b, info.left + 16, info.top + 22, info.left + 23, info.top + 14, 2, ok);
                text_fit(g.hdc, g.f.caption, 12, p.text, info.left + 34, info.top + 9,
                    tr4(&[b"S'ex\xE9cute directement avec Winlux,", b"Runs directly with Winlux,", b"Se ejecuta directamente con Winlux,", b"L\xE4uft direkt mit Winlux,"]), w - 44);
                text_fit(g.hdc, g.f.caption, 12, p.text, info.left + 34, info.top + 26,
                    tr4(&[b"sans machine virtuelle.", b"no virtual machine.", b"sin m\xE1quina virtual.", b"ohne virtuelle Maschine."]), w - 44);
            }
            for (a, r) in l.actions[..l.nactions].iter() {
                let prim = *a == Action::Open;
                let bg = if prim {
                    if hovered(r) { mix(p.accent, p.text, 1, 8) } else { p.accent }
                } else if hovered(r) {
                    p.hover
                } else {
                    p.surface
                };
                if prim {
                    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 6, bg);
                } else {
                    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 6, p.border);
                    round_rect(g.hdc, g.b, r.left + 1, r.top + 1, r.right - 1, r.bottom - 1, 5, bg);
                }
                let label = action_label(*a);
                let fg = if prim { p.on_accent } else { p.text };
                text_centered(g.hdc, if prim { g.f.semibold } else { g.f.caption }, 12, fg, (r.left + r.right) / 2, r.top + 10, label);
            }
            let mut y = l.info_y;
            fill(g.hdc, g.b, x, y - 14, x + w, y - 13, p.border);
            if let Some((free, total)) = drive_space(&e) {
                let n = space_text(free, total, &mut buf);
                usage_bar(g, x, y, w, free, total);
                text_fit(g.hdc, g.f.caption, 12, p.text2, x, y + 14, &buf[..n], w);
                return;
            }
            if !e.dir {
                let mut sb = [0u8; 24];
                let n = size_text(e.size, &mut sb);
                y = kv(g, d, y, tr(S_SIZE), &sb[..n]);
            }
            if e.mtime != 0 {
                let mut db = [0u8; 24];
                let n = date_text(e.mtime, &mut db);
                y = kv(g, d, y, tr4(&[b"Modifi\xE9 le", b"Modified", b"Modificado", b"Ge\xE4ndert"]), &db[..n]);
            }
            let mut path = [0u8; PATH_LEN + NAME_LEN + 2];
            let n = item_path(pn, i, &mut path);
            if n > 0 {
                y += 6;
                text(g.hdc, g.f.caption, p.text3, x, y, tr4(&[b"Chemin Windows", b"Windows path", b"Ruta de Windows", b"Windows-Pfad"]));
                text_fit(g.hdc, g.f.caption, 12, p.text, x, y + 18, &path[..n], w);
                y += 44;
                let mut lin = [0u8; 600];
                let m = GetFullPathLinux(&path[..n], &mut lin);
                if m > 0 {
                    text(g.hdc, g.f.caption, p.text3, x, y, tr4(&[b"Chemin Linux", b"Linux path", b"Ruta de Linux", b"Linux-Pfad"]));
                    text_fit(g.hdc, g.f.caption, 12, p.text, x, y + 18, &lin[..m], w);
                }
            }
        }
        None => {
            // Rien ou plusieurs éléments : le dossier courant.
            let ic = place_icon(pn.loc.get()).or(if pn.loc.is(HOME) { icons::of_glyph(Glyph::Home) } else if pn.loc.is(PC) { icons::of_glyph(Glyph::Computer) } else { icons::of_glyph(Glyph::Folder) });
            if !ic.is_some_and(|ic| icons::draw(g.hdc, ic, gx, gy, s, 0)) {
                glyph(g.hdc, g.b, g.f, Glyph::Folder, gx, gy, s);
            }
            set_glyph_bg(None);
            let name = if pn.loc.is(HOME) { tr(S_HOME) } else if pn.loc.is(PC) { tr(S_THIS_PC) } else { last_component(pn.loc.get()) };
            text_fit(g.hdc, g.f.title, 15, p.text, x, pv.bottom + 14, name, w);
            let k = sel_count(pn);
            let (count, word): (usize, &[u8]) = if k > 1 {
                (k, tr4(&[b" \xE9l\xE9ments s\xE9lectionn\xE9s", b" items selected", b" elementos seleccionados", b" Elemente ausgew\xE4hlt"]))
            } else {
                (pn.nview, tr4(&[b" \xE9l\xE9ments", b" items", b" elementos", b" Elemente"]))
            };
            let mut m = write_num(&mut buf, 0, count as u64);
            for &c in word {
                if m < buf.len() {
                    buf[m] = c;
                    m += 1;
                }
            }
            text_fit(g.hdc, g.f.caption, 12, p.text3, x, pv.bottom + 36, &buf[..m], w);
            if k > 1 {
                let total: u64 = (0..pn.count).filter(|&i| is_sel(pn, i) && !pn.entries[i].dir).map(|i| pn.entries[i].size).sum();
                let mut sb = [0u8; 24];
                let n = size_text(total, &mut sb);
                kv(g, d, pv.bottom + 72, tr(S_SIZE), &sb[..n]);
            } else {
                text_fit(g.hdc, g.f.caption, 12, p.text3, x, pv.bottom + 72,
                    tr4(&[b"S\xE9lectionnez un \xE9l\xE9ment", b"Select an item", b"Seleccione un elemento", b"W\xE4hlen Sie ein Element aus,"]), w);
                text_fit(g.hdc, g.f.caption, 12, p.text3, x, pv.bottom + 90,
                    tr4(&[b"pour voir ses d\xE9tails.", b"to see its details.", b"para ver sus detalles.", b"um Details zu sehen."]), w);
            }
        }
    }
}

/// Vérifications de l'Accueil en cartes et du volet Détails (explorer_regression).
#[doc(hidden)]
pub(super) unsafe fn regression() {
    let p = pane();
    p.loc.set(HOME);
    p.deep = false;
    p.search.len = 0;
    p.count = 0;
    p.scroll = 0;
    p.sel = [0; MAX_ENTRIES / 64];
    for i in 0..PLACES.len() {
        add_entry(p, place_label(i), true, false, 0, 0, Some(Kind::Folder));
    }
    add_entry(p, b"Dossier personnel (C:)", true, false, 0, 0, Some(Kind::Drive));
    add_entry(p, b"Syst\xE8me Linux (Z:)", true, false, 0, 0, Some(Kind::LinuxDrive));
    add_entry(p, b"C:\\Documents\\rapport.pdf", false, false, 10, 30, None);
    add_entry(p, b"C:\\T\xE9l\xE9chargements\\jeu.exe", false, false, 10, 20, None);
    add_entry(p, b"C:\\Images\\photo.png", false, false, 10, 10, None);
    FILTER = 0;
    rebuild_view(p);
    check!(cards(p) && p.nview == 11, "Accueil : dossiers, lecteurs et recents dans la vue");
    check!(
        p.entries[p.view[8] as usize].name() == b"C:\\Documents\\rapport.pdf",
        "Accueil : les recents gardent l'ordre du plus recent, sans tri par nom"
    );
    for (w, h) in [(1366, 768), (1920, 1080), (560, 500), (3840, 2160)] {
        let r = rect(228, 140, w, h);
        let l = layout(p, &r);
        let mut ok = true;
        for v in 0..p.nview {
            let a = item_rect_in(&l, v);
            let (cx, cy) = ((a.left + a.right) / 2, (a.top + a.bottom) / 2);
            ok &= a.right <= r.right - SCROLL_W && a.left >= r.left;
            ok &= (0..p.nview).filter(|&u| u != v).all(|u| {
                let b = item_rect_in(&l, u);
                b.right <= a.left || b.left >= a.right || b.bottom <= a.top || b.top >= a.bottom
            });
            ok &= l.end > a.bottom;
            p.scroll = 0;
            if cy < r.bottom {
                ok &= item_at(p, &r, cx, cy) == Some(v);
            }
        }
        let chips = chip_rects(&l);
        ok &= chips.iter().all(|c| c.right <= l.x + l.w && c.left >= l.x);
        ok &= (1..chips.len()).all(|i| chips[i].left > chips[i - 1].right || chips[i].top > chips[i - 1].bottom);
        ok &= chips.iter().all(|c| c.bottom < l.recent_y);
        check!(ok, "Accueil : cartes et filtres sans chevauchement, cliquables, dans la fenetre");
    }
    let r = rect(228, 140, 1366, 768);
    let l = layout(p, &r);
    let c = chip_rects(&l)[3];
    check!(
        click_chip(p, &r, c.left + 4, c.top + 4) && p.nview == 9
            && p.entries[p.view[8] as usize].kind == Kind::Exe,
        "Accueil : le filtre Programmes Windows ne garde que les .exe recents"
    );
    let mut out = [0u8; PATH_LEN + NAME_LEN + 2];
    let n = item_path(p, p.view[8] as usize, &mut out);
    check!(&out[..n] == b"C:\\T\xE9l\xE9chargements\\jeu.exe", "Accueil : un recent s'ouvre par son chemin complet");
    let n = item_path(p, p.view[7] as usize, &mut out);
    check!(&out[..n] == b"Z:\\", "Accueil : la carte Systeme Linux ouvre Z:");
    FILTER = 0;
    rebuild_view(p);
    let body = rect(228, 140, 1366, 700);
    SHOW_DETAILS = false;
    let hidden = details_rect(&body).is_none();
    SHOW_DETAILS = true;
    let shown = details_rect(&body).is_some_and(|d| d.right == body.right && d.left == body.right - DETAILS_W);
    let narrow = details_rect(&rect(0, 0, 600, 500)).is_none();
    SHOW_DETAILS = false;
    check!(hidden && shown && narrow, "Volet Details : bouton, largeur fixe, cache quand la fenetre est trop etroite");
    let pdf = p.entries[p.view[8] as usize];
    let exe = p.entries[p.view[9] as usize];
    check!(exe.kind == Kind::Exe && pdf.kind != Kind::Exe, "Pastille Windows : seulement les programmes .exe");
    p.count = 0;
    p.nview = 0;
}
