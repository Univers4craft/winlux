//! Lecteur audio basique - MP3, WAV

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

static mut AUDIO_PATH: [u8; 512] = [0; 512];
static mut AUDIO_PATH_LEN: usize = 0;
static mut AUDIO_LOADED: bool = false;
static mut PLAYING: bool = false;
static mut PAUSED: bool = false;
static mut POSITION: u32 = 0; // en secondes
static mut DURATION: u32 = 0; // en secondes
static mut VOLUME: u32 = 75; // 0-100

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let r = f.client;
    let p = pal();
    fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, rgb(0x1E, 0x1E, 0x1E));
    
    if !AUDIO_LOADED {
        text_centered(g.hdc, g.f.title, 13, rgb(0xCC, 0xCC, 0xCC), (r.left + r.right) / 2, r.top + 40, b"Lecteur audio");
        text_centered(g.hdc, g.f.body, 11, rgb(0x88, 0x88, 0x88), (r.left + r.right) / 2, r.top + 80, b"Glissez un fichier MP3 ou WAV ici");
        text_centered(g.hdc, g.f.body, 11, rgb(0x88, 0x88, 0x88), (r.left + r.right) / 2, r.top + 100, b"Support basique - lecture simple");
        return;
    }
    
    // Nom du fichier
    text(g.hdc, g.f.body, rgb(0xCC, 0xCC, 0xCC), r.left + 16, r.top + 16, &AUDIO_PATH[..AUDIO_PATH_LEN]);
    
    // Barre de progression
    let progress_y = r.top + 50;
    fill(g.hdc, g.b, r.left + 16, progress_y, r.right - 16, progress_y + 20, rgb(0x3C, 0x3C, 0x3C));
    
    if DURATION > 0 {
        let progress = (POSITION as f32 / DURATION as f32) * ((r.right - r.left - 32) as f32);
        fill(g.hdc, g.b, r.left + 16, progress_y, r.left + 16 + progress as i32, progress_y + 20, rgb(0x0A, 0x84, 0xFF));
    }
    
    // Temps
    let time_str = format_time(POSITION);
    let duration_str = format_time(DURATION);
    text(g.hdc, g.f.body, rgb(0xCC, 0xCC, 0xCC), r.left + 16, progress_y + 25, &time_str);
    text(g.hdc, g.f.body, rgb(0x88, 0x88, 0x88), r.right - 16 - text_width(&duration_str, 11), progress_y + 25, &duration_str);
    
    // Contrôles
    let controls_y = r.top + 100;
    let btn_w = 80;
    let btn_x = (r.left + r.right) / 2 - btn_w * 2 - 8;
    
    let play_label = if PLAYING {
        if PAUSED { b"▶" } else { b"⏸" }
    } else {
        b"▶"
    };
    
    fill(g.hdc, g.b, btn_x, controls_y, btn_x + btn_w, controls_y + 50, rgb(0x0A, 0x84, 0xFF));
    text_centered(g.hdc, g.f.body, 13, rgb(0xFF, 0xFF, 0xFF), btn_x + btn_w / 2, controls_y + 15, play_label);
    
    fill(g.hdc, g.b, btn_x + btn_w + 8, controls_y, btn_x + btn_w * 2 + 8, controls_y + 50, rgb(0x3C, 0x3C, 0x3C));
    text_centered(g.hdc, g.f.body, 13, rgb(0xFF, 0xFF, 0xFF), btn_x + btn_w + 8 + btn_w / 2, controls_y + 15, b"⏮");
    
    fill(g.hdc, g.b, btn_x + btn_w * 2 + 16, controls_y, btn_x + btn_w * 3 + 16, controls_y + 50, rgb(0x3C, 0x3C, 0x3C));
    text_centered(g.hdc, g.f.body, 13, rgb(0xFF, 0xFF, 0xFF), btn_x + btn_w * 2 + 16 + btn_w / 2, controls_y + 15, b"⏭");
    
    // Volume
    let vol_y = controls_y + 60;
    text(g.hdc, g.f.body, rgb(0xCC, 0xCC, 0xCC), r.left + 16, vol_y + 8, b"Volume");
    fill(g.hdc, g.b, r.left + 16, vol_y + 25, r.right - 16, vol_y + 45, rgb(0x3C, 0x3C, 0x3C));
    fill(g.hdc, g.b, r.left + 16, vol_y + 25, r.left + 16 + ((r.right - r.left - 32) * VOLUME as i32 / 100), vol_y + 45, rgb(0x0A, 0x84, 0xFF));
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    let r = f.client;
    let controls_y = r.top + 100;
    let btn_w = 80;
    let btn_x = (r.left + r.right) / 2 - btn_w * 2 - 8;
    
    if y >= controls_y && y <= controls_y + 50 {
        if x >= btn_x && x < btn_x + btn_w {
            PLAYING = !PLAYING;
            PAUSED = false;
            return true;
        }
        if x >= btn_x + btn_w + 8 && x < btn_x + btn_w * 2 + 8 {
            PAUSED = !PAUSED;
            return true;
        }
        if x >= btn_x + btn_w * 2 + 16 && x < btn_x + btn_w * 3 + 16 {
            POSITION = 0;
            return true;
        }
    }
    
    // Volume
    let vol_y = controls_y + 60;
    if y >= vol_y + 25 && y <= vol_y + 45 {
        let vol_pos = ((x - r.left - 16) * 100 / (r.right - r.left - 32)).max(0).min(100) as u32;
        VOLUME = vol_pos;
        return true;
    }
    
    false
}

fn format_time(seconds: u32) -> [u8; 16] {
    let min = seconds / 60;
    let sec = seconds % 60;
    let mut result = [0u8; 16];
    let mut pos = 0;
    
    if min >= 10 {
        result[pos] = b'0' + ((min / 10) as u8);
        pos += 1;
    }
    result[pos] = b'0' + ((min % 10) as u8);
    pos += 1;
    result[pos] = b':';
    pos += 1;
    
    if sec >= 10 {
        result[pos] = b'0' + ((sec / 10) as u8);
        pos += 1;
    }
    result[pos] = b'0' + ((sec % 10) as u8);
    pos += 1;
    
    result
}

fn text_centered(hdc: Handle, font: usize, height: i32, color: u32, x: i32, y: i32, text: &[u8]) {
    let w = text_width(text, height);
    unsafe {
        text(hdc, font, color, x - w / 2, y, text);
    }
}
