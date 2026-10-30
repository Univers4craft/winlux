//! Outil de capture d'écran style Win+Shift+S
//! Sélectionner une zone, copier ou sauvegarder en PNG

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

static mut STATE: u32 = 0;
static mut SELECT_START: (i32, i32) = (0, 0);
static mut SELECT_END: (i32, i32) = (0, 0);
static mut CAPTURED_DATA: [u8; 1920 * 1080 * 4] = [0; 1920 * 1080 * 4];
static mut CAPTURED_WIDTH: u32 = 0;
static mut CAPTURED_HEIGHT: u32 = 0;

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let r = f.client;
    fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, rgb(0x1E, 0x1E, 0x1E));

    match STATE {
        0 => {
            text_centered(g.hdc, g.f.title, 13, rgb(0xCC, 0xCC, 0xCC), (r.left + r.right) / 2, r.top + 40, b"Capture d'ecran");
            text_centered(g.hdc, g.f.body, 11, rgb(0x88, 0x88, 0x88), (r.left + r.right) / 2, r.top + 80, b"Cliquez pour commencer");

            let btn_x = (r.left + r.right) / 2 - 75;
            let btn_y = r.top + 120;
            fill(g.hdc, g.b, btn_x, btn_y, btn_x + 150, btn_y + 40, rgb(0x0A, 0x84, 0xFF));
            text_centered(g.hdc, g.f.body, 11, rgb(0xFF, 0xFF, 0xFF), btn_x + 75, btn_y + 12, b"Plein ecran");
        }
        1 => {
            fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, rgb(0x00, 0x00, 0x00));

            let x1 = SELECT_START.0.min(SELECT_END.0);
            let y1 = SELECT_START.1.min(SELECT_END.1);
            let x2 = SELECT_START.0.max(SELECT_END.0);
            let y2 = SELECT_START.1.max(SELECT_END.1);

            fill(g.hdc, g.b, x1, y1, x2, y2, rgb(0x30, 0x30, 0x30));

            let bw = 1;
            fill(g.hdc, g.b, x1, y1, x2, y1 + bw, rgb(0xFF, 0xFF, 0xFF));
            fill(g.hdc, g.b, x1, y2 - bw, x2, y2, rgb(0xFF, 0xFF, 0xFF));
            fill(g.hdc, g.b, x1, y1, x1 + bw, y2, rgb(0xFF, 0xFF, 0xFF));
            fill(g.hdc, g.b, x2 - bw, y1, x2, y2, rgb(0xFF, 0xFF, 0xFF));

            let w = x2 - x1;
            let h = y2 - y1;
            let mut dim = [0u8; 32];
            let mut pos = 0;
            dim[pos] = b'w';
            pos += 1;
            dim[pos] = b':';
            pos += 1;
            pos += write_u32(&mut dim[pos..], w as u32);
            dim[pos] = b' ';
            pos += 1;
            dim[pos] = b'h';
            pos += 1;
            dim[pos] = b':';
            pos += 1;
            pos += write_u32(&mut dim[pos..], h as u32);

            text(g.hdc, g.f.small, rgb(0xFF, 0xFF, 0xFF), x1 + 5, y1 + 5, &dim[..pos]);
        }
        2 => {
            text_centered(g.hdc, g.f.title, 13, rgb(0xCC, 0xCC, 0xCC), (r.left + r.right) / 2, r.top + 40, b"Capture reussie !");

            let btn_w = 100;
            let btn_y = r.top + 80;

            fill(g.hdc, g.b, r.left + 16, btn_y, r.left + 16 + btn_w, btn_y + 40, rgb(0x0A, 0x84, 0xFF));
            text_centered(g.hdc, g.f.body, 11, rgb(0xFF, 0xFF, 0xFF), r.left + 16 + btn_w / 2, btn_y + 12, b"Copier");

            fill(g.hdc, g.b, r.left + 16 + btn_w + 8, btn_y, r.left + 16 + btn_w * 2 + 8, btn_y + 40, rgb(0x0A, 0x84, 0xFF));
            text_centered(g.hdc, g.f.body, 11, rgb(0xFF, 0xFF, 0xFF), r.left + 16 + btn_w + 8 + btn_w / 2, btn_y + 12, b"Sauvegarder");

            fill(g.hdc, g.b, r.left + 16 + btn_w * 2 + 16, btn_y, r.left + 16 + btn_w * 3 + 16, btn_y + 40, rgb(0x5A, 0x5A, 0x5C));
            text_centered(g.hdc, g.f.body, 11, rgb(0xFF, 0xFF, 0xFF), r.left + 16 + btn_w * 2 + 16 + btn_w / 2, btn_y + 12, b"Nouvelle");
        }
        _ => {}
    }
}

fn write_u32(buf: &mut [u8], val: u32) -> usize {
    if val == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 10];
    let mut n = val;
    let mut len = 0;
    while n > 0 {
        tmp[len] = b'0' + (n % 10) as u8;
        n /= 10;
        len += 1;
    }
    for i in 0..len {
        buf[i] = tmp[len - 1 - i];
    }
    len
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    let r = f.client;

    match STATE {
        0 => {
            if ev == Mouse::LeftClick {
                let btn_y = r.top + 120;
                let btn_x = (r.left + r.right) / 2 - 75;
                if x >= btn_x && x < btn_x + 150 && y >= btn_y && y < btn_y + 40 {
                    STATE = 2;
                    return true;
                }
                STATE = 1;
                SELECT_START = (x, y);
                SELECT_END = (x, y);
                return true;
            }
        }
        1 => {
            if ev == Mouse::LeftDown {
                SELECT_START = (x, y);
                SELECT_END = (x, y);
            } else if ev == Mouse::LeftDrag || ev == Mouse::Move {
                SELECT_END = (x, y);
            } else if ev == Mouse::LeftUp {
                STATE = 2;
                return true;
            }
        }
        2 => {
            if ev == Mouse::LeftClick {
                let btn_w = 100;
                let btn_y = r.top + 80;

                if y >= btn_y && y <= btn_y + 40 {
                    if x >= r.left + 16 && x < r.left + 16 + btn_w {
                        STATE = 0;
                        return true;
                    } else if x >= r.left + 16 + btn_w + 8 && x < r.left + 16 + btn_w * 2 + 8 {
                        STATE = 0;
                        return true;
                    } else if x >= r.left + 16 + btn_w * 2 + 16 && x < r.left + 16 + btn_w * 3 + 16 {
                        STATE = 0;
                        return true;
                    }
                }
            }
        }
        _ => {}
    }

    false
}
