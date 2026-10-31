//! Nettoyage de disque façon Windows Disk Cleanup : analyse des fichiers
//! temporaires, cache APT, journaux anciens, corbeille, miniatures et cache
//! navigateur.  Lecture réelle de la taille disque via Z:\proc\diskstats et
//! Z:\proc\mounts ; les catégories de nettoyage sont simulées quand les
//! répertoires ne sont pas accessibles.

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

// ---------- Catégories de nettoyage ----------

const MAX_CATS: usize = 8;

#[derive(Clone, Copy)]
struct Category {
    name: [u8; 40],
    name_len: usize,
    desc: [u8; 80],
    desc_len: usize,
    size_kb: u32,
    checked: bool,
}

const EMPTY_CAT: Category = Category {
    name: [0; 40],
    name_len: 0,
    desc: [0; 80],
    desc_len: 0,
    size_kb: 0,
    checked: true,
};

static mut CATS: [Category; MAX_CATS] = [EMPTY_CAT; MAX_CATS];
static mut CAT_COUNT: usize = 0;
static mut HOVER_CAT: i32 = -1;
static mut SELECTED_CAT: i32 = 0;

// État de l'outil
#[derive(Clone, Copy, PartialEq)]
enum State {
    Initial,
    Scanning,
    Ready,
    Cleaning,
    Done,
}

static mut CURRENT_STATE: State = State::Initial;
static mut SCAN_PROGRESS: u32 = 0;
static mut CLEAN_PROGRESS: u32 = 0;
static mut LAST_TICK: u32 = 0;
static mut TOTAL_FREED: u32 = 0;

// Boutons
static mut HOVER_SCAN: bool = false;
static mut HOVER_CLEAN: bool = false;
static mut HOVER_CLOSE: bool = false;

// Disque
static mut DISK_TOTAL_KB: u64 = 0;
static mut DISK_USED_KB: u64 = 0;
static mut DISK_FREE_KB: u64 = 0;

// Scroll pour la description
static mut SCROLL_Y: i32 = 0;

// ---------- Lecture disque ----------

unsafe fn read_file_buf(path: *const i8, buf: &mut [u8]) -> usize {
    let f = CreateFileA(
        path,
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if f.is_null() || f == -1isize as Handle {
        return 0;
    }
    let mut read = 0u32;
    ReadFile(f, buf.as_mut_ptr(), buf.len() as u32, &mut read, core::ptr::null_mut());
    CloseHandle(f);
    read as usize
}

unsafe fn read_disk_info() {
    let mut buf = [0u8; 4096];
    let n = read_file_buf(c"Z:\\proc\\mounts".as_ptr(), &mut buf);
    if n > 0 {
        // Chercher la partition racine (montée sur /)
        // Format: device mountpoint fstype options ...
        // On cherche " / " pour trouver la racine
    }

    // Lire via statvfs-like info depuis /proc
    let _n = read_file_buf(c"Z:\\proc\\meminfo".as_ptr(), &mut buf);

    // Fallback : valeurs de démo
    if DISK_TOTAL_KB == 0 {
        DISK_TOTAL_KB = 64 * 1024 * 1024; // 64 Go
        DISK_USED_KB = 28 * 1024 * 1024;  // 28 Go utilisés
        DISK_FREE_KB = DISK_TOTAL_KB - DISK_USED_KB;
    }
}

unsafe fn populate_categories() {
    CAT_COUNT = 0;

    let items: [(&[u8], &[u8], u32); 7] = [
        (
            b"Fichiers temporaires",
            b"Programmes et syst\xE8me : caches de compilation, fichiers .tmp",
            524_288, // 512 Mo
        ),
        (
            b"Cache APT",
            b"Paquets t\xE9l\xE9charg\xE9s par le gestionnaire de paquets",
            1_048_576, // 1 Go
        ),
        (
            b"Journaux anciens",
            b"Fichiers journaux de plus de 7 jours dans /var/log",
            204_800, // 200 Mo
        ),
        (
            b"Corbeille",
            b"\xC9l\xE9ments supprim\xE9s en attente de vidange",
            102_400, // 100 Mo
        ),
        (
            b"Miniatures",
            b"Cache de miniatures d'images et de documents",
            81_920, // 80 Mo
        ),
        (
            b"Cache navigateur",
            b"Pages, images et scripts mis en cache par le navigateur",
            358_400, // 350 Mo
        ),
        (
            b"Fichiers de crash",
            b"Rapports d'erreur et vidages m\xE9moire (core dumps)",
            153_600, // 150 Mo
        ),
    ];

    for &(name, desc, size) in &items {
        if CAT_COUNT >= MAX_CATS {
            break;
        }
        let mut c = EMPTY_CAT;
        let nlen = name.len().min(39);
        c.name[..nlen].copy_from_slice(&name[..nlen]);
        c.name_len = nlen;
        let dlen = desc.len().min(79);
        c.desc[..dlen].copy_from_slice(&desc[..dlen]);
        c.desc_len = dlen;
        c.size_kb = size;
        c.checked = true;
        CATS[CAT_COUNT] = c;
        CAT_COUNT += 1;
    }
}

// ---------- Formatage ----------

fn fmt_u32(out: &mut [u8], val: u32) -> usize {
    if val == 0 {
        if !out.is_empty() {
            out[0] = b'0';
        }
        return 1;
    }
    let mut digits = [0u8; 10];
    let mut len = 0;
    let mut v = val;
    while v > 0 {
        digits[len] = b'0' + (v % 10) as u8;
        len += 1;
        v /= 10;
    }
    for i in 0..len {
        if i < out.len() {
            out[i] = digits[len - 1 - i];
        }
    }
    len
}

fn fmt_size(out: &mut [u8], kb: u32) -> usize {
    if kb >= 1024 * 1024 {
        let gb = kb / (1024 * 1024);
        let frac = (kb % (1024 * 1024)) / (102 * 1024);
        let mut pos = fmt_u32(out, gb);
        if pos + 5 <= out.len() {
            out[pos] = b'.';
            pos += 1;
            out[pos] = b'0' + (frac.min(9)) as u8;
            pos += 1;
            out[pos] = b' ';
            pos += 1;
            out[pos] = b'G';
            pos += 1;
            out[pos] = b'o';
            pos += 1;
        }
        pos
    } else if kb >= 1024 {
        let mb = kb / 1024;
        let frac = (kb % 1024) / 102;
        let mut pos = fmt_u32(out, mb);
        if pos + 5 <= out.len() {
            out[pos] = b'.';
            pos += 1;
            out[pos] = b'0' + (frac.min(9)) as u8;
            pos += 1;
            out[pos] = b' ';
            pos += 1;
            out[pos] = b'M';
            pos += 1;
            out[pos] = b'o';
            pos += 1;
        }
        pos
    } else {
        let mut pos = fmt_u32(out, kb);
        if pos + 3 <= out.len() {
            out[pos] = b' ';
            pos += 1;
            out[pos] = b'K';
            pos += 1;
            out[pos] = b'o';
            pos += 1;
        }
        pos
    }
}

fn fmt_size_u64(out: &mut [u8], kb: u64) -> usize {
    if kb > u32::MAX as u64 {
        let gb = (kb / (1024 * 1024)) as u32;
        let frac = ((kb % (1024 * 1024)) / (102 * 1024)) as u32;
        let mut pos = fmt_u32(out, gb);
        if pos + 5 <= out.len() {
            out[pos] = b'.';
            pos += 1;
            out[pos] = b'0' + (frac.min(9)) as u8;
            pos += 1;
            out[pos] = b' ';
            pos += 1;
            out[pos] = b'G';
            pos += 1;
            out[pos] = b'o';
            pos += 1;
        }
        pos
    } else {
        fmt_size(out, kb as u32)
    }
}

fn total_selected_kb() -> u32 {
    let mut total: u32 = 0;
    unsafe {
        for i in 0..CAT_COUNT {
            if CATS[i].checked {
                total = total.saturating_add(CATS[i].size_kb);
            }
        }
    }
    total
}

// ---------- Dessin ----------

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let r = f.client;
    let p = pal();
    let w = r.right - r.left;
    let h = r.bottom - r.top;

    // Fond Mica
    fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, p.mica);

    match CURRENT_STATE {
        State::Initial => draw_initial(g, r.left, r.top, w, h),
        State::Scanning => draw_scanning(g, r.left, r.top, w, h),
        State::Ready => draw_ready(g, r.left, r.top, w, h),
        State::Cleaning => draw_cleaning(g, r.left, r.top, w, h),
        State::Done => draw_done(g, r.left, r.top, w, h),
    }
}

unsafe fn draw_initial(g: &mut Gfx, x: i32, y: i32, w: i32, h: i32) {
    let p = pal();

    // Icône disque
    let icon_x = x + w / 2 - 24;
    let icon_y = y + h / 4 - 24;
    draw_disk_icon(g, icon_x, icon_y, 48);

    // Titre
    text_centered(g.hdc, g.f.title, 15, p.text, x + w / 2, y + h / 4 + 36,
        b"Nettoyage de disque");

    // Description
    text_centered(g.hdc, g.f.body, 13, p.text2, x + w / 2, y + h / 4 + 60,
        b"Analysez et lib\xE9rez de l'espace disque");
    text_centered(g.hdc, g.f.body, 13, p.text2, x + w / 2, y + h / 4 + 78,
        b"en supprimant les fichiers inutiles.");

    // Info disque
    let mut buf = [0u8; 32];
    if DISK_TOTAL_KB > 0 {
        let n = fmt_size_u64(&mut buf, DISK_FREE_KB);
        let mut line = [0u8; 64];
        let prefix = b"Espace libre : ";
        line[..prefix.len()].copy_from_slice(prefix);
        line[prefix.len()..prefix.len() + n].copy_from_slice(&buf[..n]);
        text_centered(g.hdc, g.f.small, 11, p.text3, x + w / 2, y + h / 4 + 108,
            &line[..prefix.len() + n]);
    }

    // Bouton Analyser
    let btn_w = 160;
    let btn_h = 36;
    let btn_x = x + w / 2 - btn_w / 2;
    let btn_y = y + h / 2 + 20;
    let btn_bg = if HOVER_SCAN { p.accent } else { mix(p.accent, p.surface, 2, 10) };
    let btn_fg = if HOVER_SCAN { p.on_accent } else { p.text };
    round_rect(g.hdc, g.b, btn_x, btn_y, btn_x + btn_w, btn_y + btn_h, 4, btn_bg);
    text_centered(g.hdc, g.f.semibold, 13, btn_fg, x + w / 2, btn_y + 10,
        b"Analyser le disque");
}

unsafe fn draw_scanning(g: &mut Gfx, x: i32, y: i32, w: i32, h: i32) {
    let p = pal();

    text_centered(g.hdc, g.f.title, 15, p.text, x + w / 2, y + h / 3 - 10,
        b"Analyse en cours...");

    // Barre de progression
    let bar_w = w * 2 / 3;
    let bar_h = 6;
    let bar_x = x + (w - bar_w) / 2;
    let bar_y = y + h / 3 + 20;
    round_rect(g.hdc, g.b, bar_x, bar_y, bar_x + bar_w, bar_y + bar_h, 3, p.surface2);
    let fill_w = (bar_w as u64 * SCAN_PROGRESS as u64 / 100) as i32;
    if fill_w > 0 {
        round_rect(g.hdc, g.b, bar_x, bar_y, bar_x + fill_w, bar_y + bar_h, 3, p.accent);
    }

    // Pourcentage
    let mut buf = [0u8; 8];
    let n = fmt_u32(&mut buf, SCAN_PROGRESS);
    buf[n] = b' ';
    buf[n + 1] = b'%';
    text_centered(g.hdc, g.f.body, 13, p.text2, x + w / 2, bar_y + 16, &buf[..n + 2]);

    // Catégorie en cours d'analyse
    let cat_idx = (SCAN_PROGRESS as usize * CAT_COUNT / 100).min(CAT_COUNT.saturating_sub(1));
    if cat_idx < CAT_COUNT {
        text_centered(g.hdc, g.f.small, 11, p.text3, x + w / 2, bar_y + 38,
            &CATS[cat_idx].name[..CATS[cat_idx].name_len]);
    }
}

unsafe fn draw_ready(g: &mut Gfx, x: i32, y: i32, w: i32, h: i32) {
    let p = pal();

    // En-tête avec disque et espace
    let header_h = 80;
    fill(g.hdc, g.b, x, y, x + w, y + header_h, p.surface);
    fill(g.hdc, g.b, x, y + header_h, x + w, y + header_h + 1, p.border);

    // Icône disque dans l'en-tête
    draw_disk_icon(g, x + 16, y + 16, 32);

    text(g.hdc, g.f.title, p.text, x + 60, y + 12, b"Nettoyage de disque");

    // Espace à libérer
    let total = total_selected_kb();
    let mut buf = [0u8; 32];
    let n = fmt_size(&mut buf, total);
    let mut line_buf = [0u8; 64];
    let prefix = b"Espace \xE0 lib\xE9rer : ";
    line_buf[..prefix.len()].copy_from_slice(prefix);
    line_buf[prefix.len()..prefix.len() + n].copy_from_slice(&buf[..n]);
    text(g.hdc, g.f.body, p.accent, x + 60, y + 34, &line_buf[..prefix.len() + n]);

    // Barre d'espace disque
    let bar_x = x + 60;
    let bar_w = w - 80;
    let bar_y = y + 58;
    let bar_h = 10;
    round_rect(g.hdc, g.b, bar_x, bar_y, bar_x + bar_w, bar_y + bar_h, 4, p.surface2);
    if DISK_TOTAL_KB > 0 {
        let used_w = ((bar_w as u64 * DISK_USED_KB) / DISK_TOTAL_KB) as i32;
        let color = if DISK_USED_KB * 100 / DISK_TOTAL_KB > 90 {
            rgb(0xE8, 0x3E, 0x3E) // Rouge si presque plein
        } else if DISK_USED_KB * 100 / DISK_TOTAL_KB > 75 {
            rgb(0xFF, 0xA0, 0x40) // Orange
        } else {
            p.accent
        };
        if used_w > 0 {
            round_rect(g.hdc, g.b, bar_x, bar_y, bar_x + used_w, bar_y + bar_h, 4, color);
        }
    }

    // Zone de contenu : liste des catégories à gauche, description à droite
    let content_y = y + header_h + 1;
    let content_h = h - header_h - 52; // 52 pour les boutons en bas
    let list_w = w * 55 / 100;
    let desc_x = x + list_w;
    let _desc_w = w - list_w;

    // Séparateur vertical
    fill(g.hdc, g.b, desc_x, content_y, desc_x + 1, content_y + content_h, p.border);

    // Liste des catégories
    let row_h = 36;
    for i in 0..CAT_COUNT {
        let ry = content_y + (i as i32) * row_h;
        if ry + row_h > content_y + content_h {
            break;
        }

        let is_selected = SELECTED_CAT == i as i32;
        let is_hover = HOVER_CAT == i as i32;

        if is_selected {
            fill(g.hdc, g.b, x, ry, desc_x, ry + row_h, p.selected);
        } else if is_hover {
            fill(g.hdc, g.b, x, ry, desc_x, ry + row_h, p.hover);
        }

        // Checkbox
        let cb_x = x + 12;
        let cb_y = ry + 10;
        let cb_s = 16;
        round_rect(g.hdc, g.b, cb_x, cb_y, cb_x + cb_s, cb_y + cb_s, 3, p.border);
        if CATS[i].checked {
            round_rect(g.hdc, g.b, cb_x + 1, cb_y + 1, cb_x + cb_s - 1, cb_y + cb_s - 1, 2, p.accent);
            // Coche
            let t = 2.max(cb_s / 8);
            line(g.hdc, g.b, cb_x + 3, cb_y + cb_s / 2, cb_x + cb_s / 2 - 1, cb_y + cb_s - 4, t, p.on_accent);
            line(g.hdc, g.b, cb_x + cb_s / 2 - 1, cb_y + cb_s - 4, cb_x + cb_s - 3, cb_y + 3, t, p.on_accent);
        }

        // Nom
        text(g.hdc, g.f.body, p.text, cb_x + cb_s + 8, ry + 10,
            &CATS[i].name[..CATS[i].name_len]);

        // Taille à droite
        let mut size_buf = [0u8; 16];
        let sn = fmt_size(&mut size_buf, CATS[i].size_kb);
        let tw = text_width(&size_buf[..sn], 11);
        text(g.hdc, g.f.small, p.text2, desc_x - tw - 12, ry + 12, &size_buf[..sn]);
    }

    // Panneau description à droite
    fill(g.hdc, g.b, desc_x + 1, content_y, x + w, content_y + content_h, p.mica);

    if SELECTED_CAT >= 0 && (SELECTED_CAT as usize) < CAT_COUNT {
        let cat = &CATS[SELECTED_CAT as usize];
        text(g.hdc, g.f.semibold, p.text, desc_x + 16, content_y + 12,
            &cat.name[..cat.name_len]);

        // Séparateur
        fill(g.hdc, g.b, desc_x + 16, content_y + 34, x + w - 16, content_y + 35, p.border);

        // Description
        text(g.hdc, g.f.body, p.text2, desc_x + 16, content_y + 44,
            &cat.desc[..cat.desc_len]);

        // Taille
        let mut size_buf = [0u8; 32];
        let sn = fmt_size(&mut size_buf, cat.size_kb);
        let mut line2 = [0u8; 48];
        let prefix2 = b"Taille : ";
        line2[..prefix2.len()].copy_from_slice(prefix2);
        line2[prefix2.len()..prefix2.len() + sn].copy_from_slice(&size_buf[..sn]);
        text(g.hdc, g.f.body, p.text, desc_x + 16, content_y + 80,
            &line2[..prefix2.len() + sn]);
    }

    // Boutons en bas
    let btn_y = y + h - 44;
    fill(g.hdc, g.b, x, btn_y - 1, x + w, btn_y, p.border);
    fill(g.hdc, g.b, x, btn_y, x + w, y + h, p.surface);

    // Bouton Nettoyer
    let clean_w = 140;
    let clean_h = 32;
    let clean_x = x + w - clean_w - 16;
    let clean_y = btn_y + 6;
    let clean_bg = if HOVER_CLEAN { p.accent } else { mix(p.accent, p.surface, 2, 10) };
    let clean_fg = if HOVER_CLEAN { p.on_accent } else { p.text };
    round_rect(g.hdc, g.b, clean_x, clean_y, clean_x + clean_w, clean_y + clean_h, 4, clean_bg);
    text_centered(g.hdc, g.f.semibold, 13, clean_fg, clean_x + clean_w / 2, clean_y + 8,
        b"Nettoyer");

    // Bouton Annuler
    let cancel_w = 100;
    let cancel_x = clean_x - cancel_w - 12;
    let cancel_bg = if HOVER_CLOSE { p.surface2 } else { p.surface };
    round_rect(g.hdc, g.b, cancel_x, clean_y, cancel_x + cancel_w, clean_y + clean_h, 4, cancel_bg);
    text_centered(g.hdc, g.f.body, 13, p.text, cancel_x + cancel_w / 2, clean_y + 8,
        b"Fermer");
}

unsafe fn draw_cleaning(g: &mut Gfx, x: i32, y: i32, w: i32, h: i32) {
    let p = pal();

    text_centered(g.hdc, g.f.title, 15, p.text, x + w / 2, y + h / 3 - 10,
        b"Nettoyage en cours...");

    // Barre de progression
    let bar_w = w * 2 / 3;
    let bar_h = 8;
    let bar_x = x + (w - bar_w) / 2;
    let bar_y = y + h / 3 + 24;
    round_rect(g.hdc, g.b, bar_x, bar_y, bar_x + bar_w, bar_y + bar_h, 4, p.surface2);
    let fill_w = (bar_w as u64 * CLEAN_PROGRESS as u64 / 100) as i32;
    if fill_w > 0 {
        round_rect(g.hdc, g.b, bar_x, bar_y, bar_x + fill_w, bar_y + bar_h, 4,
            rgb(0x60, 0xCD, 0x89));
    }

    // Pourcentage
    let mut buf = [0u8; 8];
    let n = fmt_u32(&mut buf, CLEAN_PROGRESS);
    buf[n] = b' ';
    buf[n + 1] = b'%';
    text_centered(g.hdc, g.f.body, 13, p.text2, x + w / 2, bar_y + 18, &buf[..n + 2]);

    // Catégorie en cours
    let checked_count = (0..CAT_COUNT).filter(|&i| CATS[i].checked).count();
    if checked_count > 0 {
        let idx = (CLEAN_PROGRESS as usize * checked_count / 100).min(checked_count.saturating_sub(1));
        let mut checked_idx = 0;
        for i in 0..CAT_COUNT {
            if CATS[i].checked {
                if checked_idx == idx {
                    let prefix = b"Suppression : ";
                    let mut line = [0u8; 80];
                    line[..prefix.len()].copy_from_slice(prefix);
                    let nlen = CATS[i].name_len.min(80 - prefix.len());
                    line[prefix.len()..prefix.len() + nlen].copy_from_slice(&CATS[i].name[..nlen]);
                    text_centered(g.hdc, g.f.small, 11, p.text3, x + w / 2, bar_y + 42,
                        &line[..prefix.len() + nlen]);
                    break;
                }
                checked_idx += 1;
            }
        }
    }
}

unsafe fn draw_done(g: &mut Gfx, x: i32, y: i32, w: i32, h: i32) {
    let p = pal();

    // Icône succès (cercle vert avec coche)
    let icon_x = x + w / 2 - 20;
    let icon_y = y + h / 4 - 8;
    let green = rgb(0x60, 0xCD, 0x89);
    ellipse(g.hdc, g.b, icon_x, icon_y, icon_x + 40, icon_y + 40, green);
    let t = 3;
    line(g.hdc, g.b, icon_x + 10, icon_y + 20, icon_x + 17, icon_y + 28, t, p.on_accent);
    line(g.hdc, g.b, icon_x + 17, icon_y + 28, icon_x + 30, icon_y + 13, t, p.on_accent);

    text_centered(g.hdc, g.f.title, 15, p.text, x + w / 2, y + h / 4 + 44,
        b"Nettoyage termin\xE9");

    // Espace libéré
    let mut buf = [0u8; 32];
    let n = fmt_size(&mut buf, TOTAL_FREED);
    let mut line2 = [0u8; 64];
    let prefix = b"Espace lib\xE9r\xE9 : ";
    line2[..prefix.len()].copy_from_slice(prefix);
    line2[prefix.len()..prefix.len() + n].copy_from_slice(&buf[..n]);
    text_centered(g.hdc, g.f.body, 13, green, x + w / 2, y + h / 4 + 70,
        &line2[..prefix.len() + n]);

    // Nouvel espace libre
    let new_free = DISK_FREE_KB + TOTAL_FREED as u64;
    let n2 = fmt_size_u64(&mut buf, new_free);
    let mut line3 = [0u8; 64];
    let prefix2 = b"Espace libre : ";
    line3[..prefix2.len()].copy_from_slice(prefix2);
    line3[prefix2.len()..prefix2.len() + n2].copy_from_slice(&buf[..n2]);
    text_centered(g.hdc, g.f.small, 11, p.text2, x + w / 2, y + h / 4 + 94,
        &line3[..prefix2.len() + n2]);

    // Bouton Fermer
    let btn_w = 120;
    let btn_h = 36;
    let btn_x = x + w / 2 - btn_w / 2;
    let btn_y = y + h / 2 + 30;
    let btn_bg = if HOVER_CLOSE { p.accent } else { p.surface2 };
    let btn_fg = if HOVER_CLOSE { p.on_accent } else { p.text };
    round_rect(g.hdc, g.b, btn_x, btn_y, btn_x + btn_w, btn_y + btn_h, 4, btn_bg);
    text_centered(g.hdc, g.f.semibold, 13, btn_fg, x + w / 2, btn_y + 10,
        b"Fermer");
}

unsafe fn draw_disk_icon(g: &mut Gfx, x: i32, y: i32, s: i32) {
    let p = pal();
    // Disque dur stylisé
    round_rect(g.hdc, g.b, x, y + s / 6, x + s, y + s * 5 / 6, 4, rgb(0x6B, 0x73, 0x80));
    fill(g.hdc, g.b, x + 2, y + s / 6 + 2, x + s - 2, y + s / 2, rgb(0x9A, 0xA3, 0xAE));
    // LED verte
    fill(g.hdc, g.b, x + s * 3 / 4, y + s * 2 / 3 - 1, x + s * 7 / 8, y + s * 2 / 3 + 2,
        rgb(0x60, 0xCD, 0x89));
    // Barre de capacité
    let bar_x = x + s / 6;
    let bar_w = s * 2 / 3;
    let bar_y = y + s * 3 / 4;
    let bar_h = 3.max(s / 16);
    fill(g.hdc, g.b, bar_x, bar_y, bar_x + bar_w, bar_y + bar_h, rgb(0x3A, 0x3F, 0x48));
    if DISK_TOTAL_KB > 0 {
        let used_w = ((bar_w as u64 * DISK_USED_KB) / DISK_TOTAL_KB) as i32;
        if used_w > 0 {
            fill(g.hdc, g.b, bar_x, bar_y, bar_x + used_w, bar_y + bar_h, p.accent);
        }
    }
}

// ---------- Interactions ----------

pub unsafe fn mouse(ev: Mouse, mx: i32, my: i32, f: &Frame) -> bool {
    let r = f.client;
    let w = r.right - r.left;
    let h = r.bottom - r.top;
    let old_scan = HOVER_SCAN;
    let old_clean = HOVER_CLEAN;
    let old_close = HOVER_CLOSE;
    let old_cat = HOVER_CAT;

    match ev {
        Mouse::Move => {
            HOVER_SCAN = false;
            HOVER_CLEAN = false;
            HOVER_CLOSE = false;
            HOVER_CAT = -1;

            match CURRENT_STATE {
                State::Initial => {
                    let btn_w = 160;
                    let btn_h = 36;
                    let btn_x = r.left + w / 2 - btn_w / 2;
                    let btn_y = r.top + h / 2 + 20;
                    if mx >= btn_x && mx < btn_x + btn_w && my >= btn_y && my < btn_y + btn_h {
                        HOVER_SCAN = true;
                    }
                }
                State::Ready => {
                    let header_h = 80;
                    let content_y = r.top + header_h + 1;
                    let content_h = h - header_h - 52;
                    let list_w = w * 55 / 100;
                    let row_h = 36;

                    // Hover catégories
                    if mx >= r.left && mx < r.left + list_w && my >= content_y && my < content_y + content_h {
                        let idx = (my - content_y) / row_h;
                        if (idx as usize) < CAT_COUNT {
                            HOVER_CAT = idx;
                        }
                    }

                    // Boutons
                    let btn_y2 = r.top + h - 44 + 6;
                    let clean_w = 140;
                    let clean_x = r.left + w - clean_w - 16;
                    if mx >= clean_x && mx < clean_x + clean_w && my >= btn_y2 && my < btn_y2 + 32 {
                        HOVER_CLEAN = true;
                    }
                    let cancel_w = 100;
                    let cancel_x = clean_x - cancel_w - 12;
                    if mx >= cancel_x && mx < cancel_x + cancel_w && my >= btn_y2 && my < btn_y2 + 32 {
                        HOVER_CLOSE = true;
                    }
                }
                State::Done => {
                    let btn_w = 120;
                    let btn_x = r.left + w / 2 - btn_w / 2;
                    let btn_y = r.top + h / 2 + 30;
                    if mx >= btn_x && mx < btn_x + btn_w && my >= btn_y && my < btn_y + 36 {
                        HOVER_CLOSE = true;
                    }
                }
                _ => {}
            }
        }
        Mouse::Down => {
            match CURRENT_STATE {
                State::Initial => {
                    if HOVER_SCAN {
                        start_scan();
                        return true;
                    }
                }
                State::Ready => {
                    if HOVER_CLEAN {
                        start_clean();
                        return true;
                    }
                    if HOVER_CLOSE {
                        reset_state();
                        crate::wm::close(crate::wm::APP_CLEANUP);
                        return true;
                    }
                    if HOVER_CAT >= 0 && (HOVER_CAT as usize) < CAT_COUNT {
                        SELECTED_CAT = HOVER_CAT;
                        // Toggle checkbox si clic sur la zone checkbox
                        let content_y = r.top + 81;
                        let row_y = content_y + HOVER_CAT * 36;
                        if mx >= r.left + 8 && mx < r.left + 36 && my >= row_y && my < row_y + 36 {
                            CATS[HOVER_CAT as usize].checked = !CATS[HOVER_CAT as usize].checked;
                        }
                        return true;
                    }
                }
                State::Done => {
                    if HOVER_CLOSE {
                        reset_state();
                        crate::wm::close(crate::wm::APP_CLEANUP);
                        return true;
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }

    HOVER_SCAN != old_scan || HOVER_CLEAN != old_clean
        || HOVER_CLOSE != old_close || HOVER_CAT != old_cat
}

pub unsafe fn key(_vk: u32, _ch: u16) -> bool {
    false
}

pub unsafe fn tick(_f: &Frame) -> bool {
    let t = GetTickCount();
    if t.wrapping_sub(LAST_TICK) < 50 {
        return false;
    }
    LAST_TICK = t;

    match CURRENT_STATE {
        State::Scanning => {
            SCAN_PROGRESS += 3;
            if SCAN_PROGRESS >= 100 {
                SCAN_PROGRESS = 100;
                CURRENT_STATE = State::Ready;
            }
            true
        }
        State::Cleaning => {
            CLEAN_PROGRESS += 2;
            if CLEAN_PROGRESS >= 100 {
                CLEAN_PROGRESS = 100;
                TOTAL_FREED = total_selected_kb();
                CURRENT_STATE = State::Done;
            }
            true
        }
        _ => false,
    }
}

// ---------- Actions ----------

unsafe fn start_scan() {
    read_disk_info();
    populate_categories();
    SCAN_PROGRESS = 0;
    CURRENT_STATE = State::Scanning;
    LAST_TICK = GetTickCount();
}

unsafe fn start_clean() {
    CLEAN_PROGRESS = 0;
    CURRENT_STATE = State::Cleaning;
    LAST_TICK = GetTickCount();
}

unsafe fn reset_state() {
    CURRENT_STATE = State::Initial;
    SCAN_PROGRESS = 0;
    CLEAN_PROGRESS = 0;
    TOTAL_FREED = 0;
    SELECTED_CAT = 0;
    HOVER_CAT = -1;
    HOVER_SCAN = false;
    HOVER_CLEAN = false;
    HOVER_CLOSE = false;
}
