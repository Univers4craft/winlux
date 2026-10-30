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
static mut POSITION: u32 = 0;
static mut DURATION: u32 = 0;
static mut VOLUME: u32 = 75;

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let r = f.client;
    fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, rgb(0x1E, 0x1E, 0x1E));

    if !AUDIO_LOADED {
        text_centered(g.hdc, g.f.title, 13, rgb(0xCC, 0xCC, 0xCC), (r.left + r.right) / 2, r.top + 40, b"Lecteur audio");
        text_centered(g.hdc, g.f.body, 11, rgb(0x88, 0x88, 0x88), (r.left + r.right) / 2, r.top + 80, b"Glissez un fichier MP3 ou WAV ici");
        text_centered(g.hdc, g.f.body, 11, rgb(0x88, 0x88, 0x88), (r.left + r.right) / 2, r.top + 100, b"Lecture simple");
        return;
    }

    text(g.hdc, g.f.body, rgb(0xCC, 0xCC, 0xCC), r.left + 16, r.top + 16, &AUDIO_PATH[..AUDIO_PATH_LEN]);

    let progress_y = r.top + 50;
    fill(g.hdc, g.b, r.left + 16, progress_y, r.right - 16, progress_y + 20, rgb(0x3C, 0x3C, 0x3C));

    if DURATION > 0 {
        let progress = (POSITION as f32 / DURATION as f32) * ((r.right - r.left - 32) as f32);
        fill(g.hdc, g.b, r.left + 16, progress_y, r.left + 16 + progress as i32, progress_y + 20, rgb(0x0A, 0x84, 0xFF));
    }

    let time_str = format_time(POSITION);
    let ts_len = time_str.iter().position(|&b| b == 0).unwrap_or(time_str.len());
    let duration_str = format_time(DURATION);
    let ds_len = duration_str.iter().position(|&b| b == 0).unwrap_or(duration_str.len());
    text(g.hdc, g.f.body, rgb(0xCC, 0xCC, 0xCC), r.left + 16, progress_y + 25, &time_str[..ts_len]);
    let dw = text_width(&duration_str[..ds_len], 11);
    text(g.hdc, g.f.body, rgb(0x88, 0x88, 0x88), r.right - 16 - dw, progress_y + 25, &duration_str[..ds_len]);

    let controls_y = r.top + 100;
    let btn_w = 80;
    let btn_x = (r.left + r.right) / 2 - btn_w * 2 - 8;

    let play_label: &[u8] = if PLAYING && !PAUSED { b"||" } else { b">" };

    fill(g.hdc, g.b, btn_x, controls_y, btn_x + btn_w, controls_y + 50, rgb(0x0A, 0x84, 0xFF));
    text_centered(g.hdc, g.f.body, 13, rgb(0xFF, 0xFF, 0xFF), btn_x + btn_w / 2, controls_y + 15, play_label);

    fill(g.hdc, g.b, btn_x + btn_w + 8, controls_y, btn_x + btn_w * 2 + 8, controls_y + 50, rgb(0x3C, 0x3C, 0x3C));
    text_centered(g.hdc, g.f.body, 13, rgb(0xFF, 0xFF, 0xFF), btn_x + btn_w + 8 + btn_w / 2, controls_y + 15, b"|<");

    fill(g.hdc, g.b, btn_x + btn_w * 2 + 16, controls_y, btn_x + btn_w * 3 + 16, controls_y + 50, rgb(0x3C, 0x3C, 0x3C));
    text_centered(g.hdc, g.f.body, 13, rgb(0xFF, 0xFF, 0xFF), btn_x + btn_w * 2 + 16 + btn_w / 2, controls_y + 15, b">|");

    let vol_y = controls_y + 60;
    text(g.hdc, g.f.body, rgb(0xCC, 0xCC, 0xCC), r.left + 16, vol_y + 8, b"Volume");
    fill(g.hdc, g.b, r.left + 16, vol_y + 25, r.right - 16, vol_y + 45, rgb(0x3C, 0x3C, 0x3C));
    let vol_w = (r.right - r.left - 32) * VOLUME as i32 / 100;
    fill(g.hdc, g.b, r.left + 16, vol_y + 25, r.left + 16 + vol_w, vol_y + 45, rgb(0x0A, 0x84, 0xFF));
}

pub unsafe fn mouse(_ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    let r = f.client;
    let controls_y = r.top + 100;
    let btn_w = 80;
    let btn_x = (r.left + r.right) / 2 - btn_w * 2 - 8;

    if y >= controls_y && y <= controls_y + 50 {
        if x >= btn_x && x < btn_x + btn_w {
            if PLAYING {
                PAUSED = !PAUSED;
            } else {
                PLAYING = true;
                PAUSED = false;
            }
            return true;
        }
        if x >= btn_x + btn_w + 8 && x < btn_x + btn_w * 2 + 8 {
            POSITION = 0;
            return true;
        }
        if x >= btn_x + btn_w * 2 + 16 && x < btn_x + btn_w * 3 + 16 {
            POSITION = DURATION;
            return true;
        }
    }

    let vol_y = controls_y + 60;
    if y >= vol_y + 25 && y <= vol_y + 45 {
        let total_w = r.right - r.left - 32;
        if total_w > 0 {
            let mut vol_pos = ((x - r.left - 16) * 100 / total_w) as i32;
            if vol_pos < 0 {
                vol_pos = 0;
            }
            if vol_pos > 100 {
                vol_pos = 100;
            }
            VOLUME = vol_pos as u32;
        }
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

    result[pos] = b'0' + ((sec / 10) as u8);
    pos += 1;
    result[pos] = b'0' + ((sec % 10) as u8);

    result
}
