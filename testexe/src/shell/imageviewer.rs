//! Visionneuse d'images native - Zoom, rotation, précédent/suivant

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

static mut IMAGE_PATH: [u8; 256] = [0; 256];
static mut IMAGE_PATH_LEN: usize = 0;
static mut IMAGE_LOADED: bool = false;
static mut ZOOM: f32 = 1.0;
static mut ROTATION: u32 = 0;
static mut CURRENT_INDEX: usize = 0;
static mut FILE_LIST: [[u8; 256]; 256] = [[0; 256]; 256];
static mut FILE_COUNT: usize = 0;

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

    if !IMAGE_LOADED {
        text_centered(
            g.hdc,
            g.f.title,
            13,
            rgb(0xCC, 0xCC, 0xCC),
            (r.left + r.right) / 2,
            r.top + 40,
            b"Visionneuse d'images",
        );
        text_centered(
            g.hdc,
            g.f.body,
            11,
            rgb(0x88, 0x88, 0x88),
            (r.left + r.right) / 2,
            r.top + 80,
            b"Glissez une image ici",
        );
        text_centered(
            g.hdc,
            g.f.body,
            11,
            rgb(0x88, 0x88, 0x88),
            (r.left + r.right) / 2,
            r.top + 100,
            b"Formats : PNG, JPG, BMP, WEBP",
        );
        return;
    }

    let img_rect = get_image_rect(r);
    fill(
        g.hdc,
        g.b,
        img_rect.left,
        img_rect.top,
        img_rect.right,
        img_rect.bottom,
        rgb(0x30, 0x30, 0x30),
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0x88, 0x88, 0x88),
        (img_rect.left + img_rect.right) / 2,
        (img_rect.top + img_rect.bottom) / 2,
        &IMAGE_PATH[..IMAGE_PATH_LEN],
    );

    let toolbar_y = r.bottom - 60;
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

    if CURRENT_INDEX > 0 {
        fill(
            g.hdc,
            g.b,
            btn_x,
            toolbar_y + 10,
            btn_x + btn_w,
            toolbar_y + 50,
            rgb(0x0A, 0x84, 0xFF),
        );
        text_centered(
            g.hdc,
            g.f.body,
            11,
            rgb(0xFF, 0xFF, 0xFF),
            btn_x + btn_w / 2,
            toolbar_y + 25,
            b"Prec.",
        );
    }

    if FILE_COUNT > 0 && CURRENT_INDEX < FILE_COUNT - 1 {
        fill(
            g.hdc,
            g.b,
            btn_x + btn_w + 8,
            toolbar_y + 10,
            btn_x + btn_w * 2 + 8,
            toolbar_y + 50,
            rgb(0x0A, 0x84, 0xFF),
        );
        text_centered(
            g.hdc,
            g.f.body,
            11,
            rgb(0xFF, 0xFF, 0xFF),
            btn_x + btn_w + 8 + btn_w / 2,
            toolbar_y + 25,
            b"Suivant",
        );
    }

    let zoom_x = r.right - 16 - btn_w * 3 - 16;
    fill(
        g.hdc,
        g.b,
        zoom_x,
        toolbar_y + 10,
        zoom_x + btn_w,
        toolbar_y + 50,
        rgb(0x3C, 0x3C, 0x3C),
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0xFF, 0xFF, 0xFF),
        zoom_x + btn_w / 2,
        toolbar_y + 25,
        b"-",
    );

    fill(
        g.hdc,
        g.b,
        zoom_x + btn_w + 8,
        toolbar_y + 10,
        zoom_x + btn_w * 2 + 8,
        toolbar_y + 50,
        rgb(0x3C, 0x3C, 0x3C),
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0xFF, 0xFF, 0xFF),
        zoom_x + btn_w + 8 + btn_w / 2,
        toolbar_y + 25,
        b"+",
    );

    fill(
        g.hdc,
        g.b,
        zoom_x + btn_w * 2 + 16,
        toolbar_y + 10,
        zoom_x + btn_w * 3 + 16,
        toolbar_y + 50,
        rgb(0x3C, 0x3C, 0x3C),
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0xFF, 0xFF, 0xFF),
        zoom_x + btn_w * 2 + 16 + btn_w / 2,
        toolbar_y + 25,
        b"100%",
    );

    let rot_x = zoom_x - btn_w - 8;
    fill(
        g.hdc,
        g.b,
        rot_x,
        toolbar_y + 10,
        rot_x + btn_w,
        toolbar_y + 50,
        rgb(0x3C, 0x3C, 0x3C),
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0xFF, 0xFF, 0xFF),
        rot_x + btn_w / 2,
        toolbar_y + 25,
        b"Rot",
    );

    let zoom_label = format_zoom();
    text(
        g.hdc,
        g.f.small,
        rgb(0xCC, 0xCC, 0xCC),
        r.left + 16,
        r.top + 10,
        &zoom_label,
    );

    if FILE_COUNT > 0 {
        let info = format_index();
        let iw = text_width(&info, 11);
        text(
            g.hdc,
            g.f.small,
            rgb(0xCC, 0xCC, 0xCC),
            r.right - 16 - iw,
            r.top + 10,
            &info,
        );
    }
}

unsafe fn get_image_rect(r: Rect) -> Rect {
    let margin = 60;
    Rect {
        left: r.left + margin,
        top: r.top + 40,
        right: r.right - margin,
        bottom: r.bottom - 70,
    }
}

fn format_zoom() -> [u8; 16] {
    unsafe {
        let pct = (ZOOM * 100.0) as u32;
        let mut result = [0u8; 16];
        let mut pos = 0;
        result[pos..pos + 5].copy_from_slice(b"Zoom:");
        pos += 5;
        result[pos] = b' ';
        pos += 1;
        if pct >= 100 {
            result[pos] = b'0' + ((pct / 100) as u8);
            pos += 1;
        }
        if pct >= 10 {
            result[pos] = b'0' + (((pct % 100) / 10) as u8);
            pos += 1;
        }
        result[pos] = b'0' + ((pct % 10) as u8);
        pos += 1;
        result[pos] = b'%';
        result
    }
}

fn format_index() -> [u8; 16] {
    unsafe {
        let mut result = [0u8; 16];
        let mut pos = 0;
        let cur = (CURRENT_INDEX + 1) as u32;
        let tot = FILE_COUNT as u32;
        if cur >= 100 {
            result[pos] = b'0' + ((cur / 100) as u8);
            pos += 1;
        }
        if cur >= 10 {
            result[pos] = b'0' + (((cur % 100) / 10) as u8);
            pos += 1;
        }
        result[pos] = b'0' + ((cur % 10) as u8);
        pos += 1;
        result[pos] = b'/';
        pos += 1;
        if tot >= 100 {
            result[pos] = b'0' + ((tot / 100) as u8);
            pos += 1;
        }
        if tot >= 10 {
            result[pos] = b'0' + (((tot % 100) / 10) as u8);
            pos += 1;
        }
        result[pos] = b'0' + ((tot % 10) as u8);
        result
    }
}

pub unsafe fn mouse(_ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    let r = f.client;
    let toolbar_y = r.bottom - 60;
    let btn_w = 80;

    if y >= toolbar_y + 10 && y <= toolbar_y + 50 {
        let btn_x = r.left + 16;

        if x >= btn_x && x < btn_x + btn_w && CURRENT_INDEX > 0 {
            CURRENT_INDEX -= 1;
            load_image(CURRENT_INDEX);
            return true;
        }

        if x >= btn_x + btn_w + 8
            && x < btn_x + btn_w * 2 + 8
            && FILE_COUNT > 0
            && CURRENT_INDEX < FILE_COUNT - 1
        {
            CURRENT_INDEX += 1;
            load_image(CURRENT_INDEX);
            return true;
        }

        let zoom_x = r.right - 16 - btn_w * 3 - 16;
        if x >= zoom_x && x < zoom_x + btn_w {
            if ZOOM > 0.25 {
                ZOOM -= 0.25;
            }
            return true;
        }
        if x >= zoom_x + btn_w + 8 && x < zoom_x + btn_w * 2 + 8 {
            if ZOOM < 4.0 {
                ZOOM += 0.25;
            }
            return true;
        }
        if x >= zoom_x + btn_w * 2 + 16 && x < zoom_x + btn_w * 3 + 16 {
            ZOOM = 1.0;
            return true;
        }

        let rot_x = zoom_x - btn_w - 8;
        if x >= rot_x && x < rot_x + btn_w {
            ROTATION = (ROTATION + 1) % 4;
            return true;
        }
    }

    false
}

unsafe fn load_image(index: usize) {
    if index < FILE_COUNT {
        IMAGE_PATH = FILE_LIST[index];
        IMAGE_PATH_LEN = FILE_LIST[index].iter().position(|&b| b == 0).unwrap_or(256);
        IMAGE_LOADED = true;
    }
}
