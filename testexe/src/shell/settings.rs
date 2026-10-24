//! Paramètres de Winlux. Fenêtre gérée par `wm.rs` : voir le contrat
//! d'application en tête de ce fichier (draw, mouse, key, tick).

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};

pub const CUSTOM_TITLE: bool = false;

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let p = pal();
    let c = f.client;
    text(g.hdc, g.f.large, p.text, c.left + 32, c.top + 24, b"Param\xE8tres");
    text(g.hdc, g.f.body, p.text2, c.left + 32, c.top + 64, b"Cette page arrive bient\xF4t.");
}

pub unsafe fn mouse(_ev: Mouse, _x: i32, _y: i32, _f: &Frame) -> bool { false }
pub unsafe fn key(_vk: u32, _ch: u16) -> bool { false }
pub unsafe fn tick(_f: &Frame) -> bool { false }
