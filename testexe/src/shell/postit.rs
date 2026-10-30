//! Post-it / Notes rapides - Fenêtres flottantes

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

static mut NOTES: [[u8; 1024]; 8] = [[0; 1024]; 8];
static mut NOTE_LEN: [usize; 8] = [0; 8];
static mut NOTE_COLORS: [u32; 8] = [0xFFFF_CC, 0xCCFFCC, 0xCCCCFF, 0xFFFF99, 0xFFCCFF, 0xCCFFFF, 0xFFCCCC, 0xDDDDDD];
static mut CURRENT_NOTE: usize = 0;
static mut EDITING: bool = false;

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let r = f.client;
    let p = pal();
    fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, rgb(0x1E, 0x1E, 0x1E));
    
    // Barre de titre
    fill(g.hdc, g.b, r.left, r.top, r.right, r.top + 32, rgb(0x2D, 0x2D, 0x30));
    text(g.hdc, g.f.body, rgb(0xCC, 0xCC, 0xCC), r.left + 8, r.top + 8, b"Post-it");
    
    // Sélecteur de note
    let selector_y = r.top + 40;
    let note_w = 100;
    for i in 0..8 {
        let nx = r.left + 16 + (i as i32) * (note_w + 8);
        let bg = if i == CURRENT_NOTE { NOTE_COLORS[i] } else { rgb(0x3C, 0x3C, 0x3C) };
        fill(g.hdc, g.b, nx, selector_y, nx + note_w, selector_y + 60, bg);
        let label = if NOTE_LEN[i] > 0 { b"✓" } else { b"+" };
        text_centered(g.hdc, g.f.body, 11, rgb(0x00, 0x00, 0x00), nx + note_w / 2, selector_y + 22, label);
    }
    
    // Zone de texte
    let text_y = selector_y + 70;
    fill(g.hdc, g.b, r.left + 16, text_y, r.right - 16, r.bottom - 20, NOTE_COLORS[CURRENT_NOTE]);
    
    // Texte
    let text = &NOTES[CURRENT_NOTE][..NOTE_LEN[CURRENT_NOTE]];
    let mut y_pos = text_y + 8;
    for line in text.split(|&b| b == b'\n') {
        text(g.hdc, g.f.body, rgb(0x00, 0x00, 0x00), r.left + 24, y_pos, line);
        y_pos += 16;
    }
    
    // Boutons
    let btn_y = r.bottom - 50;
    fill(g.hdc, g.b, r.left + 16, btn_y, r.left + 116, btn_y + 30, rgb(0x0A, 0x84, 0xFF));
    text(g.hdc, g.f.body, rgb(0xFF, 0xFF, 0xFF), r.left + 24, btn_y + 8, b"Nouvelle");
    
    fill(g.hdc, g.b, r.left + 124, btn_y, r.left + 224, btn_y + 30, rgb(0x5A, 0x5A, 0x5C));
    text(g.hdc, g.f.body, rgb(0xFF, 0xFF, 0xFF), r.left + 132, btn_y + 8, b"Supprimer");
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    let r = f.client;
    let selector_y = r.top + 40;
    let note_w = 100;
    
    // Sélection de note
    if y >= selector_y && y <= selector_y + 60 {
        for i in 0..8 {
            let nx = r.left + 16 + (i as i32) * (note_w + 8);
            if x >= nx && x < nx + note_w {
                CURRENT_NOTE = i;
                return true;
            }
        }
    }
    
    // Boutons
    let btn_y = r.bottom - 50;
    if y >= btn_y && y <= btn_y + 30 {
        if x >= r.left + 16 && x < r.left + 116 {
            // Nouvelle note
            for i in 0..8 {
                if NOTE_LEN[i] == 0 {
                    CURRENT_NOTE = i;
                    break;
                }
            }
            return true;
        }
        if x >= r.left + 124 && x < r.left + 224 {
            // Supprimer
            NOTES[CURRENT_NOTE] = [0; 1024];
            NOTE_LEN[CURRENT_NOTE] = 0;
            return true;
        }
    }
    
    false
}

fn text_centered(hdc: Handle, font: usize, height: i32, color: u32, x: i32, y: i32, text: &[u8]) {
    let w = text_width(text, height);
    unsafe {
        text(hdc, font, color, x - w / 2, y, text);
    }
}
