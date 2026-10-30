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
    let p = pal();
    fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, rgb(0x1E, 0x1E, 0x1E));
    
    if !PDF_LOADED {
        text_centered(g.hdc, g.f.title, 13, rgb(0xCC, 0xCC, 0xCC), (r.left + r.right) / 2, r.top + 40, b"Lecteur PDF");
        text_centered(g.hdc, g.f.body, 11, rgb(0x88, 0x88, 0x88), (r.left + r.right) / 2, r.top + 80, b"Glissez un fichier PDF ici");
        text_centered(g.hdc, g.f.body, 11, rgb(0x88, 0x88, 0x88), (r.left + r.right) / 2, r.top + 100, b"Support basique - affichage de pages");
        return;
    }
    
    // Affichage PDF (placeholder)
    let pdf_rect = Rect {
        left: r.left + 16,
        top: r.top + 40,
        right: r.right - 16,
        bottom: r.bottom - 60,
    };
    fill(g.hdc, g.b, pdf_rect.left, pdf_rect.top, pdf_rect.right, pdf_rect.bottom, rgb(0xFF, 0xFF, 0xFF));
    
    // Simulation de page
    let page_info = format!("Page {}/{}", CURRENT_PAGE, TOTAL_PAGES);
    text_centered(g.hdc, g.f.body, 11, rgb(0x00, 0x00, 0x00), (pdf_rect.left + pdf_rect.right) / 2, pdf_rect.top + 20, &page_info);
    
    // Nom du fichier
    text(g.hdc, g.f.small, rgb(0x00, 0x00, 0x00), pdf_rect.left + 8, pdf_rect.bottom - 20, &PDF_PATH[..PDF_PATH_LEN]);
    
    // Barre d'outils
    let toolbar_y = r.bottom - 50;
    fill(g.hdc, g.b, r.left, toolbar_y, r.right, r.bottom, rgb(0x2D, 0x2D, 0x30));
    
    let btn_w = 80;
    let btn_x = r.left + 16;
    
    // Page précédente
    if CURRENT_PAGE > 1 {
        fill(g.hdc, g.b, btn_x, toolbar_y + 10, btn_x + btn_w, toolbar_y + 40, rgb(0x0A, 0x84, 0xFF));
        text_centered(g.hdc, g.f.body, 11, rgb(0xFF, 0xFF, 0xFF), btn_x + btn_w / 2, toolbar_y + 22, b"Précédent");
    }
    
    // Page suivante
    if CURRENT_PAGE < TOTAL_PAGES {
        fill(g.hdc, g.b, btn_x + btn_w + 8, toolbar_y + 10, btn_x + btn_w * 2 + 8, toolbar_y + 40, rgb(0x0A, 0x84, 0xFF));
        text_centered(g.hdc, g.f.body, 11, rgb(0xFF, 0xFF, 0xFF), btn_x + btn_w + 8 + btn_w / 2, toolbar_y + 22, b"Suivant");
    }
    
    // Zoom
    let zoom_x = r.right - 16 - btn_w * 3 - 16;
    fill(g.hdc, g.b, zoom_x, toolbar_y + 10, zoom_x + btn_w, toolbar_y + 40, rgb(0x3C, 0x3C, 0x3C));
    text_centered(g.hdc, g.f.body, 11, rgb(0xFF, 0xFF, 0xFF), zoom_x + btn_w / 2, toolbar_y + 22, b"-");
    
    fill(g.hdc, g.b, zoom_x + btn_w + 8, toolbar_y + 10, zoom_x + btn_w * 2 + 8, toolbar_y + 40, rgb(0x3C, 0x3C, 0x3C));
    text_centered(g.hdc, g.f.body, 11, rgb(0xFF, 0xFF, 0xFF), zoom_x + btn_w + 8 + btn_w / 2, toolbar_y + 22, b"+");
    
    fill(g.hdc, g.b, zoom_x + btn_w * 2 + 16, toolbar_y + 10, zoom_x + btn_w * 3 + 16, toolbar_y + 40, rgb(0x3C, 0x3C, 0x3C));
    text_centered(g.hdc, g.f.body, 11, rgb(0xFF, 0xFF, 0xFF), zoom_x + btn_w * 2 + 16 + btn_w / 2, toolbar_y + 22, b"100%");
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
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
            ZOOM = (ZOOM - 0.25).max(0.5);
            return true;
        }
        if x >= zoom_x + btn_w + 8 && x < zoom_x + btn_w * 2 + 8 {
            ZOOM = (ZOOM + 0.25).min(3.0);
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
        result[pos..pos + 5].copy_from_slice(b"Page ");
        pos += 5;
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
        pos += 1;
        result
    }
}

fn text_centered(hdc: Handle, font: usize, height: i32, color: u32, x: i32, y: i32, text: &[u8]) {
    let w = text_width(text, height);
    unsafe {
        text(hdc, font, color, x - w / 2, y, text);
    }
}
