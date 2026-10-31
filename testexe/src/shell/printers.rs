//! Paramètres › Imprimantes et scanners : liste des imprimantes (winspool),
//! imprimante par défaut, file d'attente, page de test, suppression et ajout
//! d'imprimantes USB ou réseau (détection CUPS ou adresse saisie).

use crate::gfx::*;
use crate::wm::{Gfx, Mouse};
use core::ffi::c_void;
use winlux_testexe::*;

const MAX_PRINTERS: usize = 12;
const MAX_JOBS: usize = 8;
const MAX_DEVICES: usize = 10;

#[derive(Clone, Copy)]
struct Printer {
    name: [u16; 128],
    len: usize,
    port: [u8; 64],
    port_len: usize,
    status: u32,
    jobs: u32,
    default: bool,
}

const NO_PRINTER: Printer = Printer {
    name: [0; 128],
    len: 0,
    port: [0; 64],
    port_len: 0,
    status: 0,
    jobs: 0,
    default: false,
};

#[derive(Clone, Copy)]
struct Job {
    id: u32,
    doc: [u8; 80],
    doc_len: usize,
    status: u32,
}

const NO_JOB: Job = Job {
    id: 0,
    doc: [0; 80],
    doc_len: 0,
    status: 0,
};

#[derive(Clone, Copy)]
struct Device {
    uri: [u16; 256],
    label: [u8; 80],
    label_len: usize,
    usb: bool,
}

const NO_DEVICE: Device = Device {
    uri: [0; 256],
    label: [0; 80],
    label_len: 0,
    usb: false,
};

static mut PRINTERS: [Printer; MAX_PRINTERS] = [NO_PRINTER; MAX_PRINTERS];
static mut COUNT: usize = 0;
static mut SELECTED: usize = 0;
static mut JOBS: [Job; MAX_JOBS] = [NO_JOB; MAX_JOBS];
static mut JOB_COUNT: usize = 0;
static mut DEVICES: [Device; MAX_DEVICES] = [NO_DEVICE; MAX_DEVICES];
static mut DEVICE_COUNT: usize = 0;
static mut SEARCHING: bool = false;
static mut SEARCHED: bool = false;
static mut DISCOVERY: [u16; 8192] = [0; 8192];
static mut ADDRESS: [u16; 200] = [0; 200];
static mut ADDRESS_LEN: usize = 0;
static mut FOCUSED: bool = false;
static mut MESSAGE: [u8; 200] = [0; 200];
static mut MESSAGE_LEN: usize = 0;
static mut LAST_REFRESH: u32 = 0;
static mut HOVER: i32 = -1;
static mut ZONES: [(Rect, i32); 64] = [(rect(0, 0, 0, 0), 0); 64];
static mut ZONE_COUNT: usize = 0;
static mut BUF: [u64; 8192] = [0; 8192];

// Identifiants des zones cliquables.
const SET_DEFAULT: i32 = 1;
const TEST_PAGE: i32 = 2;
const REMOVE: i32 = 3;
const SEARCH: i32 = 4;
const ADDRESS_FIELD: i32 = 5;
const ADD_ADDRESS: i32 = 6;
const PRINTER_BASE: i32 = 100;
const JOB_BASE: i32 = 200;
const DEVICE_BASE: i32 = 300;

/// UTF-16 → octets Windows-1252 pour `TextOutA` (hors Latin-1 : « ? »).
fn latin1(src: &[u16], dst: &mut [u8]) -> usize {
    let mut n = 0;
    for &c in src {
        if c == 0 || n == dst.len() {
            break;
        }
        dst[n] = if c < 256 { c as u8 } else { b'?' };
        n += 1;
    }
    n
}

unsafe fn wide_at(p: *const u16, dst: &mut [u16]) -> usize {
    if p.is_null() {
        return 0;
    }
    let mut n = 0;
    while n + 1 < dst.len() && *p.add(n) != 0 {
        dst[n] = *p.add(n);
        n += 1;
    }
    dst[n] = 0;
    n
}

unsafe fn set_message(s: &[u8]) {
    let n = s.len().min(MESSAGE.len());
    MESSAGE[..n].copy_from_slice(&s[..n]);
    MESSAGE_LEN = n;
}

/// Message du spooler (succès, erreur, état de CUPS).
unsafe fn load_status() {
    let mut w = [0u16; 200];
    WinluxPrintStatusW(w.as_mut_ptr(), w.len() as u32);
    MESSAGE_LEN = latin1(&w, &mut MESSAGE);
}

unsafe fn open_selected() -> Handle {
    if SELECTED >= COUNT {
        return core::ptr::null_mut();
    }
    let mut h: Handle = core::ptr::null_mut();
    if OpenPrinterW(PRINTERS[SELECTED].name.as_ptr(), &mut h, core::ptr::null()) == 0 {
        return core::ptr::null_mut();
    }
    h
}

/// Relit les imprimantes (`EnumPrintersW` niveau 2) et la file de la sélection.
pub unsafe fn refresh() {
    LAST_REFRESH = GetTickCount();
    let mut needed = 0u32;
    let mut count = 0u32;
    let buf = core::ptr::addr_of_mut!(BUF) as *mut u8;
    let ok = EnumPrintersW(
        6,
        core::ptr::null(),
        2,
        buf,
        (8192 * 8) as u32,
        &mut needed,
        &mut count,
    );
    let previous = if SELECTED < COUNT {
        PRINTERS[SELECTED].name
    } else {
        [0; 128]
    };
    COUNT = 0;
    if ok != 0 {
        for i in 0..(count as usize).min(MAX_PRINTERS) {
            let base = buf.add(i * 136);
            let mut p = NO_PRINTER;
            p.len = wide_at(*(base.add(8) as *const *const u16), &mut p.name);
            let mut port = [0u16; 64];
            wide_at(*(base.add(24) as *const *const u16), &mut port);
            p.port_len = latin1(&port, &mut p.port);
            let attributes = *(base.add(104) as *const u32);
            p.status = *(base.add(124) as *const u32);
            p.jobs = *(base.add(128) as *const u32);
            p.default = attributes & 0x4 != 0;
            PRINTERS[COUNT] = p;
            COUNT += 1;
        }
    }
    SELECTED = (0..COUNT)
        .find(|&i| PRINTERS[i].name == previous)
        .or_else(|| (0..COUNT).find(|&i| PRINTERS[i].default))
        .unwrap_or(0);
    refresh_jobs();
    if MESSAGE_LEN == 0 {
        load_status();
    }
}

unsafe fn refresh_jobs() {
    JOB_COUNT = 0;
    let h = open_selected();
    if h.is_null() {
        return;
    }
    let mut needed = 0u32;
    let mut count = 0u32;
    let buf = core::ptr::addr_of_mut!(BUF) as *mut u8;
    if EnumJobsW(
        h,
        0,
        MAX_JOBS as u32,
        1,
        buf,
        (8192 * 8) as u32,
        &mut needed,
        &mut count,
    ) != 0
    {
        for i in 0..(count as usize).min(MAX_JOBS) {
            let base = buf.add(i * 96);
            let mut j = NO_JOB;
            j.id = *(base as *const u32);
            let mut doc = [0u16; 80];
            wide_at(*(base.add(32) as *const *const u16), &mut doc);
            j.doc_len = latin1(&doc, &mut j.doc);
            j.status = *(base.add(56) as *const u32);
            JOBS[JOB_COUNT] = j;
            JOB_COUNT += 1;
        }
    }
    ClosePrinter(h);
}

/// Fil de détection : `CUPS-Get-Devices` peut durer plusieurs secondes.
extern "system" fn discover_thread(_: *mut c_void) -> u32 {
    unsafe {
        let buf = core::ptr::addr_of_mut!(DISCOVERY) as *mut u16;
        let n = WinluxDiscoverPrintersW(buf, 8192) as usize;
        let text = core::slice::from_raw_parts(buf, n.min(8191));
        let mut found = 0;
        for line in text.split(|&c| c == u16::from(b'\n')) {
            if found == MAX_DEVICES {
                break;
            }
            let mut fields = line.split(|&c| c == u16::from(b'\t'));
            let (Some(uri), Some(label)) = (fields.next(), fields.next()) else {
                continue;
            };
            if uri.is_empty() || uri.len() >= 255 {
                continue;
            }
            let kind = fields.next().unwrap_or(&[]);
            let mut d = NO_DEVICE;
            d.uri[..uri.len()].copy_from_slice(uri);
            d.label_len = latin1(label, &mut d.label);
            d.usb = kind.first() == Some(&u16::from(b'u'));
            DEVICES[found] = d;
            found += 1;
        }
        DEVICE_COUNT = found;
        if found == 0 {
            load_status();
            if MESSAGE_LEN == 0 {
                set_message(tr4(&[
                    b"Aucune nouvelle imprimante d\xE9tect\xE9e (USB ou r\xE9seau).",
                    b"No new printer detected (USB or network).",
                    b"No se detect\xF3 ninguna impresora nueva (USB o red).",
                    b"Kein neuer Drucker erkannt (USB oder Netzwerk).",
                ]));
            }
        } else {
            set_message(tr4(&[
                b"Imprimantes d\xE9tect\xE9es : choisissez celle \xE0 ajouter.",
                b"Printers found: choose the one to add.",
                b"Impresoras detectadas: elija la que desea agregar.",
                b"Gefundene Drucker: W\xE4hlen Sie den hinzuzuf\xFCgenden aus.",
            ]));
        }
        core::ptr::write_volatile(core::ptr::addr_of_mut!(SEARCHED), true);
        core::ptr::write_volatile(core::ptr::addr_of_mut!(SEARCHING), false);
    }
    0
}

unsafe fn start_search() {
    if SEARCHING {
        return;
    }
    SEARCHING = true;
    DEVICE_COUNT = 0;
    set_message(tr4(&[
        b"Recherche des imprimantes USB et r\xE9seau...",
        b"Looking for USB and network printers...",
        b"Buscando impresoras USB y de red...",
        b"USB- und Netzwerkdrucker werden gesucht...",
    ]));
    let h = CreateThread(
        core::ptr::null_mut(),
        0,
        discover_thread,
        core::ptr::null_mut(),
        0,
        core::ptr::null_mut(),
    );
    if h.is_null() {
        SEARCHING = false;
        set_message(tr4(&[
            b"Recherche impossible.",
            b"Search failed.",
            b"No se pudo buscar.",
            b"Suche fehlgeschlagen.",
        ]));
    } else {
        CloseHandle(h);
    }
}

unsafe fn add(name: *const u16, uri: *const u16) {
    MESSAGE_LEN = 0;
    WinluxAddPrinterW(name, uri);
    load_status();
    refresh();
}

/// Clic sur une zone ; `true` si l'affichage change.
unsafe fn activate(id: i32) -> bool {
    FOCUSED = id == ADDRESS_FIELD;
    match id {
        SET_DEFAULT if SELECTED < COUNT => {
            if SetDefaultPrinterW(PRINTERS[SELECTED].name.as_ptr()) != 0 {
                set_message(tr4(&[
                    b"Imprimante par d\xE9faut modifi\xE9e.",
                    b"Default printer changed.",
                    b"Impresora predeterminada cambiada.",
                    b"Standarddrucker ge\xE4ndert.",
                ]));
            } else {
                set_message(tr4(&[
                    b"Impossible de changer l'imprimante par d\xE9faut.",
                    b"Couldn't change the default printer.",
                    b"No se pudo cambiar la impresora predeterminada.",
                    b"Standarddrucker konnte nicht ge\xE4ndert werden.",
                ]));
            }
            refresh();
        }
        TEST_PAGE if SELECTED < COUNT => {
            MESSAGE_LEN = 0;
            if WinluxPrintTestPageW(PRINTERS[SELECTED].name.as_ptr()) != 0 {
                load_status();
            } else {
                load_status();
            }
            refresh();
        }
        REMOVE if SELECTED < COUNT => {
            let h = open_selected();
            MESSAGE_LEN = 0;
            if !h.is_null() {
                if DeletePrinter(h) != 0 {
                    set_message(tr4(&[
                        b"Imprimante supprim\xE9e.",
                        b"Printer removed.",
                        b"Impresora quitada.",
                        b"Drucker entfernt.",
                    ]));
                } else {
                    load_status();
                }
                ClosePrinter(h);
            }
            refresh();
        }
        SEARCH => start_search(),
        ADD_ADDRESS if ADDRESS_LEN > 0 => {
            ADDRESS[ADDRESS_LEN] = 0;
            add(core::ptr::null(), ADDRESS.as_ptr());
            ADDRESS_LEN = 0;
        }
        ADD_ADDRESS => set_message(tr4(&[
            b"Saisissez l'adresse IP ou l'URI de l'imprimante.",
            b"Enter the printer's IP address or URI.",
            b"Escriba la direcci\xF3n IP o el URI de la impresora.",
            b"Geben Sie die IP-Adresse oder URI des Druckers ein.",
        ])),
        id if (PRINTER_BASE..PRINTER_BASE + MAX_PRINTERS as i32).contains(&id) => {
            SELECTED = (id - PRINTER_BASE) as usize;
            refresh_jobs();
        }
        id if (JOB_BASE..JOB_BASE + MAX_JOBS as i32).contains(&id) => {
            let j = JOBS[(id - JOB_BASE) as usize];
            let h = open_selected();
            if !h.is_null() {
                if SetJobW(h, j.id, 0, core::ptr::null(), 3) != 0 {
                    set_message(tr4(&[
                        b"Impression annul\xE9e.",
                        b"Print job cancelled.",
                        b"Impresi\xF3n cancelada.",
                        b"Druckauftrag abgebrochen.",
                    ]));
                } else {
                    set_message(tr4(&[
                        b"Le document n'a pas pu \xEAtre annul\xE9.",
                        b"The document couldn't be cancelled.",
                        b"No se pudo cancelar el documento.",
                        b"Das Dokument konnte nicht abgebrochen werden.",
                    ]));
                }
                ClosePrinter(h);
            }
            refresh();
        }
        id if (DEVICE_BASE..DEVICE_BASE + MAX_DEVICES as i32).contains(&id) => {
            let k = (id - DEVICE_BASE) as usize;
            if k < DEVICE_COUNT && !SEARCHING {
                let d = DEVICES[k];
                // Nom de file : le modèle détecté, sans espaces.
                let mut name = [0u16; 64];
                let mut n = 0;
                for &b in &d.label[..d.label_len] {
                    if n == name.len() - 1 {
                        break;
                    }
                    if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
                        name[n] = u16::from(b);
                        n += 1;
                    } else if n > 0 && name[n - 1] != u16::from(b'_') {
                        name[n] = u16::from(b'_');
                        n += 1;
                    }
                }
                add(name.as_ptr(), d.uri.as_ptr());
                DEVICES[k] = DEVICES[DEVICE_COUNT - 1];
                DEVICE_COUNT -= 1;
            }
        }
        _ => {}
    }
    true
}

unsafe fn zone(r: Rect, id: i32) {
    if ZONE_COUNT < 64 {
        ZONES[ZONE_COUNT] = (r, id);
        ZONE_COUNT += 1;
    }
}

pub unsafe fn hit(x: i32, y: i32) -> i32 {
    for i in (0..ZONE_COUNT).rev() {
        if inside(&ZONES[i].0, x, y) {
            return ZONES[i].1;
        }
    }
    -1
}

/// Bouton façon Windows 11 (accentué si `primary`).
unsafe fn button(g: &mut Gfx, r: Rect, label: &[u8], id: i32, primary: bool, enabled: bool) {
    let p = pal();
    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 7, p.border);
    round_rect(
        g.hdc,
        g.b,
        r.left + 1,
        r.top + 1,
        r.right - 1,
        r.bottom - 1,
        6,
        if !enabled {
            p.surface
        } else if primary {
            p.accent
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
        if !enabled {
            p.text3
        } else if primary {
            p.on_accent
        } else {
            p.text
        },
        (r.left + r.right) / 2,
        r.top + (r.bottom - r.top - 13) / 2,
        &label[..end],
    );
    if enabled {
        zone(r, id);
    }
}

fn status_text(status: u32) -> &'static [u8] {
    if status & 0x80 != 0 {
        tr4(&[
            b"Hors connexion",
            b"Offline",
            b"Sin conexi\xF3n",
            b"Offline",
        ])
    } else if status & 0x1 != 0 {
        tr4(&[b"En pause", b"Paused", b"En pausa", b"Angehalten"])
    } else if status & 0x400 != 0 {
        tr4(&[
            b"Impression en cours",
            b"Printing",
            b"Imprimiendo",
            b"Druckt",
        ])
    } else {
        tr4(&[b"Pr\xEAte", b"Ready", b"Lista", b"Bereit"])
    }
}

fn job_status(status: u32) -> &'static [u8] {
    match status {
        0x10 => tr4(&[b"Impression", b"Printing", b"Imprimiendo", b"Druckt"]),
        0x1 => tr4(&[b"En pause", b"Paused", b"En pausa", b"Angehalten"]),
        0x4 => tr4(&[
            b"Annulation",
            b"Cancelling",
            b"Cancelando",
            b"Wird abgebrochen",
        ]),
        _ => tr4(&[b"En attente", b"Waiting", b"En espera", b"Wartend"]),
    }
}

unsafe fn clipped(
    g: &mut Gfx,
    font: Handle,
    size: i32,
    color: u32,
    x: i32,
    y: i32,
    s: &[u8],
    w: i32,
) {
    let mut end = s.len();
    while end > 0 && text_width(&s[..end], size) > w {
        end -= 1;
    }
    text(g.hdc, font, color, x, y, &s[..end]);
}

/// Dessine la page dans la zone de contenu `c` (sous le titre).
pub unsafe fn draw(g: &mut Gfx, c: &Rect, scroll: usize) {
    if LAST_REFRESH == 0 || GetTickCount().wrapping_sub(LAST_REFRESH) > 4000 {
        refresh();
    }
    ZONE_COUNT = 0;
    let p = pal();
    let top = c.top + 52;
    let bottom = c.bottom - 26;
    let mut y = top - (scroll.min(40) as i32) * 24;
    let visible = |y0: i32, h: i32| y0 >= top && y0 + h <= bottom;
    // Liste des imprimantes.
    if visible(y, 18) {
        text(
            g.hdc,
            g.f.semibold,
            p.text,
            c.left,
            y,
            tr4(&[
                b"Imprimantes install\xE9es",
                b"Installed printers",
                b"Impresoras instaladas",
                b"Installierte Drucker",
            ]),
        );
    }
    y += 26;
    for i in 0..COUNT {
        let pr = PRINTERS[i];
        let r = rect(c.left - 6, y, c.right + 6, y + 50);
        if visible(y, 50) {
            let selected = i == SELECTED;
            round_rect(
                g.hdc,
                g.b,
                r.left,
                r.top,
                r.right,
                r.bottom,
                8,
                if selected { p.accent } else { p.border },
            );
            round_rect(
                g.hdc,
                g.b,
                r.left + 1,
                r.top + 1,
                r.right - 1,
                r.bottom - 1,
                7,
                if selected {
                    p.selected
                } else if HOVER == PRINTER_BASE + i as i32 {
                    p.hover
                } else {
                    p.surface
                },
            );
            // Icône d'imprimante stylisée.
            round_rect(
                g.hdc,
                g.b,
                r.left + 12,
                r.top + 18,
                r.left + 40,
                r.top + 36,
                3,
                p.text2,
            );
            fill(
                g.hdc,
                g.b,
                r.left + 17,
                r.top + 12,
                r.left + 35,
                r.top + 19,
                p.text3,
            );
            fill(
                g.hdc,
                g.b,
                r.left + 17,
                r.top + 31,
                r.left + 35,
                r.top + 40,
                p.surface,
            );
            let mut name = [0u8; 128];
            let n = latin1(&pr.name[..pr.len], &mut name);
            clipped(
                g,
                g.f.semibold,
                13,
                p.text,
                r.left + 52,
                r.top + 8,
                &name[..n],
                r.right - r.left - 210,
            );
            let mut detail = [0u8; 160];
            let mut d = 0;
            for part in [
                status_text(pr.status),
                b"  \xB7  ".as_slice(),
                &pr.port[..pr.port_len],
            ] {
                let k = part.len().min(detail.len() - d);
                detail[d..d + k].copy_from_slice(&part[..k]);
                d += k;
            }
            clipped(
                g,
                g.f.small,
                11,
                p.text2,
                r.left + 52,
                r.top + 28,
                &detail[..d],
                r.right - r.left - 210,
            );
            if pr.default {
                let b = rect(r.right - 120, r.top + 14, r.right - 14, r.top + 36);
                round_rect(g.hdc, g.b, b.left, b.top, b.right, b.bottom, 11, p.accent);
                text_centered(
                    g.hdc,
                    g.f.small,
                    11,
                    p.on_accent,
                    (b.left + b.right) / 2,
                    b.top + 4,
                    tr4(&[b"Par d\xE9faut", b"Default", b"Predeterminada", b"Standard"]),
                );
            } else if pr.jobs > 0 {
                let mut s = [0u8; 24];
                let mut k = 0;
                let mut v = pr.jobs;
                let mut digits = [0u8; 10];
                let mut nd = 0;
                loop {
                    digits[nd] = b'0' + (v % 10) as u8;
                    nd += 1;
                    v /= 10;
                    if v == 0 {
                        break;
                    }
                }
                while nd > 0 {
                    nd -= 1;
                    s[k] = digits[nd];
                    k += 1;
                }
                for &b in b" en attente" {
                    s[k] = b;
                    k += 1;
                }
                text(
                    g.hdc,
                    g.f.small,
                    p.text2,
                    r.right - 110,
                    r.top + 18,
                    &s[..k],
                );
            }
            zone(r, PRINTER_BASE + i as i32);
        }
        y += 56;
    }
    // Actions sur l'imprimante sélectionnée.
    if visible(y, 32) {
        let w = ((c.right - c.left - 16) / 3).clamp(90, 180);
        let has = SELECTED < COUNT;
        let removable = has
            && PRINTERS[SELECTED].port_len > 0
            && &PRINTERS[SELECTED].port[..4.min(PRINTERS[SELECTED].port_len)] != b"PDF:";
        button(
            g,
            rect(c.left, y, c.left + w, y + 32),
            tr4(&[
                b"D\xE9finir par d\xE9faut",
                b"Set as default",
                b"Establecer como predeterminada",
                b"Als Standard festlegen",
            ]),
            SET_DEFAULT,
            false,
            has && !PRINTERS[SELECTED].default,
        );
        button(
            g,
            rect(c.left + w + 8, y, c.left + 2 * w + 8, y + 32),
            tr4(&[
                b"Imprimer une page de test",
                b"Print a test page",
                b"Imprimir una p\xE1gina de prueba",
                b"Testseite drucken",
            ]),
            TEST_PAGE,
            false,
            has,
        );
        button(
            g,
            rect(c.left + 2 * (w + 8), y, c.left + 3 * w + 16, y + 32),
            tr4(&[b"Supprimer", b"Delete", b"Eliminar", b"L\xF6schen"]),
            REMOVE,
            false,
            removable,
        );
    }
    y += 48;
    // File d'attente.
    if visible(y, 18) {
        text(
            g.hdc,
            g.f.semibold,
            p.text,
            c.left,
            y,
            tr4(&[
                b"File d'attente",
                b"Print queue",
                b"Cola de impresi\xF3n",
                b"Druckwarteschlange",
            ]),
        );
    }
    y += 26;
    if JOB_COUNT == 0 {
        if visible(y, 16) {
            text(
                g.hdc,
                g.f.body,
                p.text2,
                c.left,
                y,
                tr4(&[
                    b"Aucun document en attente.",
                    b"No documents waiting.",
                    b"No hay documentos en espera.",
                    b"Keine wartenden Dokumente.",
                ]),
            );
        }
        y += 28;
    }
    for j in 0..JOB_COUNT {
        let job = JOBS[j];
        if visible(y, 30) {
            clipped(
                g,
                g.f.body,
                13,
                p.text,
                c.left,
                y + 7,
                &job.doc[..job.doc_len],
                c.right - c.left - 260,
            );
            text(
                g.hdc,
                g.f.small,
                p.text2,
                c.right - 200,
                y + 9,
                job_status(job.status),
            );
            button(
                g,
                rect(c.right - 96, y, c.right, y + 30),
                tr4(&[b"Annuler", b"Cancel", b"Cancelar", b"Abbrechen"]),
                JOB_BASE + j as i32,
                false,
                true,
            );
        }
        y += 36;
    }
    y += 8;
    // Ajout d'imprimantes.
    if visible(y, 18) {
        text(
            g.hdc,
            g.f.semibold,
            p.text,
            c.left,
            y,
            tr4(&[
                b"Ajouter une imprimante",
                b"Add a printer",
                b"Agregar una impresora",
                b"Drucker hinzuf\xFCgen",
            ]),
        );
    }
    y += 26;
    if visible(y, 32) {
        let field = rect(c.left, y, c.right - 290, y + 32);
        round_rect(
            g.hdc,
            g.b,
            field.left,
            field.top,
            field.right,
            field.bottom,
            7,
            if FOCUSED { p.accent } else { p.border },
        );
        round_rect(
            g.hdc,
            g.b,
            field.left + 1,
            field.top + 1,
            field.right - 1,
            field.bottom - 1,
            6,
            p.surface,
        );
        if FOCUSED {
            fill(
                g.hdc,
                g.b,
                field.left + 6,
                field.bottom - 3,
                field.right - 6,
                field.bottom - 1,
                p.accent,
            );
        }
        let mut shown = [0u8; 200];
        let n = latin1(&ADDRESS[..ADDRESS_LEN], &mut shown);
        if n == 0 && !FOCUSED {
            clipped(
                g,
                g.f.body,
                13,
                p.text3,
                field.left + 10,
                field.top + 8,
                tr4(&[
                    b"Adresse IP ou URI (ex. 192.168.1.20)",
                    b"IP address or URI (e.g. 192.168.1.20)",
                    b"Direcci\xF3n IP o URI (p. ej. 192.168.1.20)",
                    b"IP-Adresse oder URI (z. B. 192.168.1.20)",
                ]),
                field.right - field.left - 20,
            );
        } else {
            // Fin du texte visible quand il dépasse le champ.
            let mut start = 0;
            while start < n && text_width(&shown[start..n], 13) > field.right - field.left - 24 {
                start += 1;
            }
            text(
                g.hdc,
                g.f.body,
                p.text,
                field.left + 10,
                field.top + 8,
                &shown[start..n],
            );
            if FOCUSED {
                let x = field.left + 11 + text_width(&shown[start..n], 13);
                fill(
                    g.hdc,
                    g.b,
                    x,
                    field.top + 7,
                    x + 1,
                    field.bottom - 7,
                    p.text,
                );
            }
        }
        zone(field, ADDRESS_FIELD);
        button(
            g,
            rect(c.right - 282, y, c.right - 196, y + 32),
            tr4(&[b"Ajouter", b"Add", b"Agregar", b"Hinzuf\xFCgen"]),
            ADD_ADDRESS,
            true,
            ADDRESS_LEN > 0,
        );
        button(
            g,
            rect(c.right - 188, y, c.right, y + 32),
            if SEARCHING {
                tr4(&[
                    b"Recherche...",
                    b"Searching...",
                    b"Buscando...",
                    b"Suche l\xE4uft...",
                ])
            } else {
                tr4(&[
                    b"Rechercher (USB, r\xE9seau)",
                    b"Search (USB, network)",
                    b"Buscar (USB, red)",
                    b"Suchen (USB, Netzwerk)",
                ])
            },
            SEARCH,
            false,
            !SEARCHING,
        );
    }
    y += 42;
    if !SEARCHING {
        for k in 0..DEVICE_COUNT {
            let d = DEVICES[k];
            if visible(y, 30) {
                text(
                    g.hdc,
                    g.f.small,
                    p.text2,
                    c.left,
                    y + 9,
                    if d.usb {
                        b"USB".as_slice()
                    } else {
                        tr4(&[b"R\xE9seau", b"Network", b"Red", b"Netzwerk"])
                    },
                );
                clipped(
                    g,
                    g.f.body,
                    13,
                    p.text,
                    c.left + 64,
                    y + 7,
                    &d.label[..d.label_len],
                    c.right - c.left - 180,
                );
                button(
                    g,
                    rect(c.right - 96, y, c.right, y + 30),
                    tr4(&[b"Ajouter", b"Add", b"Agregar", b"Hinzuf\xFCgen"]),
                    DEVICE_BASE + k as i32,
                    true,
                    true,
                );
            }
            y += 36;
        }
    } else if visible(y, 16) {
        text(
            g.hdc,
            g.f.body,
            p.text2,
            c.left,
            y,
            tr4(&[
                b"Recherche des imprimantes USB et r\xE9seau...",
                b"Looking for USB and network printers...",
                b"Buscando impresoras USB y de red...",
                b"USB- und Netzwerkdrucker werden gesucht...",
            ]),
        );
    }
}

/// Message d'état affiché en bas de la page.
pub unsafe fn notice() -> &'static [u8] {
    &*core::ptr::slice_from_raw_parts(core::ptr::addr_of!(MESSAGE) as *const u8, MESSAGE_LEN)
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32) -> bool {
    match ev {
        Mouse::Move => {
            let h = hit(x, y);
            let changed = h != HOVER;
            HOVER = h;
            changed
        }
        Mouse::Down | Mouse::DoubleClick => {
            let id = hit(x, y);
            if id < 0 {
                let was = FOCUSED;
                FOCUSED = false;
                return was;
            }
            activate(id)
        }
        _ => false,
    }
}

/// Saisie de l'adresse ; `true` si la touche est consommée.
pub unsafe fn key(vk: u32, ch: u16) -> bool {
    if !FOCUSED {
        return false;
    }
    match (vk, ch) {
        // Entrée peut arriver en touche et en caractère : l'ajout vide le champ,
        // la seconde occurrence ne fait donc rien.
        (_, 13) | (0x0D, _) => {
            if ADDRESS_LEN > 0 {
                activate(ADD_ADDRESS);
                FOCUSED = true;
            }
        }
        (_, 27) | (0x1B, _) => FOCUSED = false,
        (_, 8) => ADDRESS_LEN = ADDRESS_LEN.saturating_sub(1),
        (_, c) if c >= 0x20 && c != 0x7f && ADDRESS_LEN < ADDRESS.len() - 1 => {
            ADDRESS[ADDRESS_LEN] = c;
            ADDRESS_LEN += 1;
        }
        _ => return false,
    }
    true
}

/// Le fil de détection a fini : la page doit être redessinée.
pub unsafe fn tick() -> bool {
    let done = core::ptr::read_volatile(core::ptr::addr_of!(SEARCHED));
    if done {
        core::ptr::write_volatile(core::ptr::addr_of_mut!(SEARCHED), false);
        return true;
    }
    SEARCHING
}

/// Contrôles headless de la page (exécutés par `explorer_regression`).
pub unsafe fn regression_checks() {
    refresh();
    check!(COUNT >= 1, "Imprimantes : l'imprimante PDF est list\u{e9}e");
    let pdf = (0..COUNT).any(|i| &PRINTERS[i].port[..PRINTERS[i].port_len] == b"PDF:");
    check!(
        pdf,
        "Imprimantes : port PDF de l'imprimante int\u{e9}gr\u{e9}e"
    );
    check!(
        (0..COUNT).filter(|&i| PRINTERS[i].default).count() == 1,
        "Imprimantes : une seule imprimante par d\u{e9}faut"
    );
    FOCUSED = true;
    key(0, u16::from(b'1'));
    key(0, u16::from(b'0'));
    key(0, 8);
    check!(
        ADDRESS_LEN == 1 && ADDRESS[0] == u16::from(b'1'),
        "Imprimantes : saisie de l'adresse"
    );
    key(0x1B, 27);
    check!(!FOCUSED, "Imprimantes : Echap quitte le champ d'adresse");
    ADDRESS_LEN = 0;
    ZONE_COUNT = 0;
    zone(rect(10, 10, 50, 50), SET_DEFAULT);
    zone(rect(20, 20, 30, 30), PRINTER_BASE);
    check!(
        hit(25, 25) == PRINTER_BASE && hit(12, 12) == SET_DEFAULT && hit(60, 60) == -1,
        "Imprimantes : zones cliquables"
    );
    ZONE_COUNT = 0;
}
