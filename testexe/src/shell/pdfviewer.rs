//! Lecteur PDF simple - Viewer basique

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

static mut PDF_PATH: [u8; 512] = [0; 512];
static mut PDF_PATH_LEN: usize = 0;
static mut PDF_LOADED: bool = false;
static mut CURRENT_PAGE: u32 = 1;
static mut TOTAL_PAGES: u32 = 1;
static mut ZOOM: f32 = 1.0;

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let r = f.client;
    fill(
        g.hdc,
        g.b,
        r.left,
        r.top,
        r.right,
        r.bottom,
        rgb(0x1E, 0x1E, 0x1E),
    );

    if !PDF_LOADED {
        text_centered(
            g.hdc,
            g.f.title,
            13,
            rgb(0xCC, 0xCC, 0xCC),
            (r.left + r.right) / 2,
            r.top + 40,
            tr4(&[
                b"Lecteur PDF",
                b"PDF Reader",
                b"Lector de PDF",
                b"PDF-Reader",
            ]),
        );
        text_centered(
            g.hdc,
            g.f.body,
            11,
            rgb(0x88, 0x88, 0x88),
            (r.left + r.right) / 2,
            r.top + 80,
            tr4(&[
                b"Glissez un fichier PDF ici",
                b"Drop a PDF file here",
                b"Arrastre un archivo PDF aqu\xED",
                b"PDF-Datei hierher ziehen",
            ]),
        );
        text_centered(
            g.hdc,
            g.f.body,
            11,
            rgb(0x88, 0x88, 0x88),
            (r.left + r.right) / 2,
            r.top + 100,
            tr4(&[
                b"Support basique",
                b"Basic support",
                b"Compatibilidad b\xE1sica",
                b"Grundlegende Unterst\xFCtzung",
            ]),
        );
        return;
    }

    let pdf_rect = Rect {
        left: r.left + 16,
        top: r.top + 40,
        right: r.right - 16,
        bottom: r.bottom - 60,
    };
    fill(
        g.hdc,
        g.b,
        pdf_rect.left,
        pdf_rect.top,
        pdf_rect.right,
        pdf_rect.bottom,
        rgb(0xFF, 0xFF, 0xFF),
    );

    let page_info = format_page();
    let pi_len = page_info
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(page_info.len());
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0x00, 0x00, 0x00),
        (pdf_rect.left + pdf_rect.right) / 2,
        pdf_rect.top + 20,
        &page_info[..pi_len],
    );

    text(
        g.hdc,
        g.f.small,
        rgb(0x00, 0x00, 0x00),
        pdf_rect.left + 8,
        pdf_rect.bottom - 20,
        &PDF_PATH[..PDF_PATH_LEN],
    );

    let toolbar_y = r.bottom - 50;
    fill(
        g.hdc,
        g.b,
        r.left,
        toolbar_y,
        r.right,
        r.bottom,
        rgb(0x2D, 0x2D, 0x30),
    );

    let btn_w = 80;
    let btn_x = r.left + 16;

    if CURRENT_PAGE > 1 {
        fill(
            g.hdc,
            g.b,
            btn_x,
            toolbar_y + 10,
            btn_x + btn_w,
            toolbar_y + 40,
            rgb(0x0A, 0x84, 0xFF),
        );
        text_centered(
            g.hdc,
            g.f.body,
            11,
            rgb(0xFF, 0xFF, 0xFF),
            btn_x + btn_w / 2,
            toolbar_y + 22,
            tr4(&[b"Prec.", b"Prev.", b"Ant.", b"Zur\xFCck"]),
        );
    }

    if CURRENT_PAGE < TOTAL_PAGES {
        fill(
            g.hdc,
            g.b,
            btn_x + btn_w + 8,
            toolbar_y + 10,
            btn_x + btn_w * 2 + 8,
            toolbar_y + 40,
            rgb(0x0A, 0x84, 0xFF),
        );
        text_centered(
            g.hdc,
            g.f.body,
            11,
            rgb(0xFF, 0xFF, 0xFF),
            btn_x + btn_w + 8 + btn_w / 2,
            toolbar_y + 22,
            tr4(&[b"Suivant", b"Next", b"Siguiente", b"Weiter"]),
        );
    }

    let zoom_x = r.right - 16 - btn_w * 3 - 16;
    fill(
        g.hdc,
        g.b,
        zoom_x,
        toolbar_y + 10,
        zoom_x + btn_w,
        toolbar_y + 40,
        rgb(0x3C, 0x3C, 0x3C),
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0xFF, 0xFF, 0xFF),
        zoom_x + btn_w / 2,
        toolbar_y + 22,
        b"-",
    );

    fill(
        g.hdc,
        g.b,
        zoom_x + btn_w + 8,
        toolbar_y + 10,
        zoom_x + btn_w * 2 + 8,
        toolbar_y + 40,
        rgb(0x3C, 0x3C, 0x3C),
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0xFF, 0xFF, 0xFF),
        zoom_x + btn_w + 8 + btn_w / 2,
        toolbar_y + 22,
        b"+",
    );

    fill(
        g.hdc,
        g.b,
        zoom_x + btn_w * 2 + 16,
        toolbar_y + 10,
        zoom_x + btn_w * 3 + 16,
        toolbar_y + 40,
        rgb(0x3C, 0x3C, 0x3C),
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0xFF, 0xFF, 0xFF),
        zoom_x + btn_w * 2 + 16 + btn_w / 2,
        toolbar_y + 22,
        b"100%",
    );
}

pub unsafe fn mouse(_ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    let r = f.client;
    let toolbar_y = r.bottom - 50;
    let btn_w = 80;

    if y >= toolbar_y + 10 && y <= toolbar_y + 40 {
        let btn_x = r.left + 16;

        if x >= btn_x && x < btn_x + btn_w && CURRENT_PAGE > 1 {
            CURRENT_PAGE -= 1;
            return true;
        }

        if x >= btn_x + btn_w + 8 && x < btn_x + btn_w * 2 + 8 && CURRENT_PAGE < TOTAL_PAGES {
            CURRENT_PAGE += 1;
            return true;
        }

        let zoom_x = r.right - 16 - btn_w * 3 - 16;
        if x >= zoom_x && x < zoom_x + btn_w {
            if ZOOM > 0.5 {
                ZOOM -= 0.25;
            }
            return true;
        }
        if x >= zoom_x + btn_w + 8 && x < zoom_x + btn_w * 2 + 8 {
            if ZOOM < 3.0 {
                ZOOM += 0.25;
            }
            return true;
        }
        if x >= zoom_x + btn_w * 2 + 16 && x < zoom_x + btn_w * 3 + 16 {
            ZOOM = 1.0;
            return true;
        }
    }

    false
}

fn format_page() -> [u8; 32] {
    unsafe {
        let mut result = [0u8; 32];
        let mut pos = 0;
        let label = tr4(&[b"Page ", b"Page ", b"P\xE1gina ", b"Seite "]);
        result[pos..pos + label.len()].copy_from_slice(label);
        pos += label.len();
        let p = CURRENT_PAGE;
        if p >= 10 {
            result[pos] = b'0' + ((p / 10) as u8);
            pos += 1;
        }
        result[pos] = b'0' + ((p % 10) as u8);
        pos += 1;
        result[pos..pos + 3].copy_from_slice(b" / ");
        pos += 3;
        let t = TOTAL_PAGES;
        if t >= 10 {
            result[pos] = b'0' + ((t / 10) as u8);
            pos += 1;
        }
        result[pos] = b'0' + ((t % 10) as u8);
        result
    }
}
