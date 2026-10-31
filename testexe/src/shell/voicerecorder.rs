//! Enregistreur vocal natif - capture audio via waveIn (ALSA),
//! sauvegarde en WAV PCM 16 bits mono 44100 Hz.

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

// --- Constantes audio ---------------------------------------------------

const SAMPLE_RATE: u32 = 44100;
const CHANNELS: u16 = 1;
const BITS: u16 = 16;
const BLOCK_ALIGN: u16 = CHANNELS * (BITS / 8);
const BYTES_PER_SEC: u32 = SAMPLE_RATE * BLOCK_ALIGN as u32;

const WAVE_MAPPER: u32 = 0xFFFF_FFFF;
const WHDR_DONE: u32 = 0x01;
const WHDR_PREPARED: u32 = 0x02;
const SND_MEMORY: u32 = 0x0004;

const CAPTURE_BUF: usize = BYTES_PER_SEC as usize; // 1 s par buffer
const MAX_SECONDS: usize = 120;
const MAX_RECORD: usize = BYTES_PER_SEC as usize * MAX_SECONDS;

// --- Etat global --------------------------------------------------------

static mut STATE: u32 = 0; // 0 = idle, 1 = recording, 2 = stopped
static mut RECORD_START: u32 = 0;
static mut ELAPSED_MS: u32 = 0;
static mut DURATION_S: u32 = 0;
static mut HWI: Handle = core::ptr::null_mut();

static mut BUF1: [u8; CAPTURE_BUF] = [0; CAPTURE_BUF];
static mut BUF2: [u8; CAPTURE_BUF] = [0; CAPTURE_BUF];
static mut HDR1: WaveHdr = WaveHdr {
    data: core::ptr::null_mut(),
    buffer_length: 0,
    bytes_recorded: 0,
    flags: 0,
    loops: 0,
    next: core::ptr::null_mut(),
    reserved: 0,
};
static mut HDR2: WaveHdr = WaveHdr {
    data: core::ptr::null_mut(),
    buffer_length: 0,
    bytes_recorded: 0,
    flags: 0,
    loops: 0,
    next: core::ptr::null_mut(),
    reserved: 0,
};

static mut RECORDED: [u8; MAX_RECORD] = [0; MAX_RECORD];
static mut RECORDED_LEN: usize = 0;
static mut SAVE_PATH: [u8; 512] = [0; 512];
static mut SAVE_PATH_LEN: usize = 0;

static mut WAV_FILE: [u8; 44 + MAX_RECORD] = [0; 44 + MAX_RECORD];

// --- Couleurs -----------------------------------------------------------

const BG: u32 = rgb(0x1E, 0x1E, 0x1E);
const RED: u32 = rgb(0xE8, 0x11, 0x23);
const RED_DARK: u32 = rgb(0xA0, 0x0C, 0x18);
const GRAY: u32 = rgb(0x3C, 0x3C, 0x3C);
const BLUE: u32 = rgb(0x0A, 0x84, 0xFF);
const GREEN: u32 = rgb(0x10, 0x7C, 0x10);
const TXT: u32 = rgb(0xCC, 0xCC, 0xCC);
const TXT2: u32 = rgb(0x88, 0x88, 0x88);
const WHITE: u32 = rgb(0xFF, 0xFF, 0xFF);

// --- Dessin -------------------------------------------------------------

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let r = f.client;
    let cx = (r.left + r.right) / 2;
    fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, BG);

    match STATE {
        0 => draw_idle(g, &r, cx),
        1 => draw_recording(g, &r, cx),
        _ => draw_stopped(g, &r, cx),
    }
}

unsafe fn draw_idle(g: &mut Gfx, r: &Rect, cx: i32) {
    text_centered(
        g.hdc,
        g.f.title,
        15,
        TXT,
        cx,
        r.top + 24,
        b"Enregistreur vocal",
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        TXT2,
        cx,
        r.top + 52,
        b"Appuyez pour enregistrer",
    );

    // Bouton rond rouge (approximation rectangulaire)
    let btn_size = 64;
    let bx = cx - btn_size / 2;
    let by = r.top + 90;
    fill(g.hdc, g.b, bx, by, bx + btn_size, by + btn_size, RED);

    // Cercle interieur (micro)
    let inner = 20;
    let ix = cx - inner / 2;
    let iy = by + btn_size / 2 - inner / 2;
    fill(g.hdc, g.b, ix, iy, ix + inner, iy + inner, WHITE);
}

unsafe fn draw_recording(g: &mut Gfx, r: &Rect, cx: i32) {
    // Indicateur rouge clignotant
    let blink = (GetTickCount() / 500) & 1;
    let dot_color = if blink == 0 { RED } else { RED_DARK };
    fill(
        g.hdc,
        g.b,
        cx - 6,
        r.top + 20,
        cx + 6,
        r.top + 32,
        dot_color,
    );

    text_centered(
        g.hdc,
        g.f.body,
        11,
        TXT2,
        cx,
        r.top + 38,
        b"Enregistrement en cours...",
    );

    // Timer MM:SS
    let elapsed = GetTickCount().wrapping_sub(RECORD_START) / 1000;
    ELAPSED_MS = GetTickCount().wrapping_sub(RECORD_START);
    let timer = format_mmss(elapsed);
    text_centered(g.hdc, g.f.display, 28, TXT, cx, r.top + 70, &timer);

    // Bouton stop (carre gris)
    let btn_size = 64;
    let bx = cx - btn_size / 2;
    let by = r.top + 130;
    fill(g.hdc, g.b, bx, by, bx + btn_size, by + btn_size, GRAY);
    let inner = 24;
    let ix = cx - inner / 2;
    let iy = by + btn_size / 2 - inner / 2;
    fill(g.hdc, g.b, ix, iy, ix + inner, iy + inner, WHITE);

    // Barre de niveau (simplifiee)
    let bar_y = by + btn_size + 16;
    fill(
        g.hdc,
        g.b,
        r.left + 32,
        bar_y,
        r.right - 32,
        bar_y + 4,
        GRAY,
    );
    let level = ((GetTickCount() / 50) % 100) as i32;
    let w = (r.right - r.left - 64) * level / 100;
    fill(
        g.hdc,
        g.b,
        r.left + 32,
        bar_y,
        r.left + 32 + w,
        bar_y + 4,
        GREEN,
    );
}

unsafe fn draw_stopped(g: &mut Gfx, r: &Rect, cx: i32) {
    text_centered(
        g.hdc,
        g.f.title,
        15,
        TXT,
        cx,
        r.top + 20,
        b"Enregistrement termin\xE9",
    );

    let timer = format_mmss(DURATION_S);
    text_centered(g.hdc, g.f.large, 20, TXT2, cx, r.top + 48, &timer);

    // Bouton Ecouter (bleu)
    let btn_w = 120;
    let btn_h = 36;
    let gap = 12;
    let total_w = btn_w * 3 + gap * 2;
    let bx = cx - total_w / 2;
    let by = r.top + 100;

    fill(g.hdc, g.b, bx, by, bx + btn_w, by + btn_h, BLUE);
    text_centered(
        g.hdc,
        g.f.body,
        11,
        WHITE,
        bx + btn_w / 2,
        by + 10,
        b"Ecouter",
    );

    // Bouton Sauvegarder (vert)
    let bx2 = bx + btn_w + gap;
    fill(g.hdc, g.b, bx2, by, bx2 + btn_w, by + btn_h, GREEN);
    text_centered(
        g.hdc,
        g.f.body,
        11,
        WHITE,
        bx2 + btn_w / 2,
        by + 10,
        b"Sauvegarder",
    );

    // Bouton Nouveau (gris)
    let bx3 = bx2 + btn_w + gap;
    fill(g.hdc, g.b, bx3, by, bx3 + btn_w, by + btn_h, GRAY);
    text_centered(
        g.hdc,
        g.f.body,
        11,
        WHITE,
        bx3 + btn_w / 2,
        by + 10,
        b"Nouveau",
    );

    if SAVE_PATH_LEN > 0 {
        text_centered(
            g.hdc,
            g.f.small,
            9,
            GREEN,
            cx,
            by + btn_h + 16,
            b"Fichier sauvegard\xE9 !",
        );
    }
}

// --- Interaction --------------------------------------------------------

pub unsafe fn mouse(_ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    let r = f.client;
    let cx = (r.left + r.right) / 2;

    match STATE {
        0 => {
            let btn_size = 64;
            let bx = cx - btn_size / 2;
            let by = r.top + 90;
            if x >= bx && x <= bx + btn_size && y >= by && y <= by + btn_size {
                start_recording();
                return true;
            }
        }
        1 => {
            let btn_size = 64;
            let bx = cx - btn_size / 2;
            let by = r.top + 130;
            if x >= bx && x <= bx + btn_size && y >= by && y <= by + btn_size {
                stop_recording();
                return true;
            }
        }
        _ => {
            let btn_w = 120;
            let btn_h = 36;
            let gap = 12;
            let total_w = btn_w * 3 + gap * 2;
            let bx = cx - total_w / 2;
            let by = r.top + 100;

            if y >= by && y <= by + btn_h {
                if x >= bx && x < bx + btn_w {
                    play_recording();
                    return true;
                }
                let bx2 = bx + btn_w + gap;
                if x >= bx2 && x < bx2 + btn_w {
                    save_recording();
                    return true;
                }
                let bx3 = bx2 + btn_w + gap;
                if x >= bx3 && x < bx3 + btn_w {
                    new_recording();
                    return true;
                }
            }
        }
    }
    false
}

pub unsafe fn key(_vk: u32, _ch: u16) -> bool {
    false
}

pub unsafe fn tick(_f: &Frame) -> bool {
    if STATE != 1 {
        return false;
    }
    collect_buffers();
    ELAPSED_MS = GetTickCount().wrapping_sub(RECORD_START);
    if RECORDED_LEN >= MAX_RECORD {
        stop_recording();
    }
    true
}

// --- Capture audio waveIn -----------------------------------------------

unsafe fn start_recording() {
    RECORDED_LEN = 0;
    SAVE_PATH_LEN = 0;

    let fmt = WaveFormatEx {
        format_tag: 1, // WAVE_FORMAT_PCM
        channels: CHANNELS,
        samples_per_sec: SAMPLE_RATE,
        avg_bytes_per_sec: BYTES_PER_SEC,
        block_align: BLOCK_ALIGN,
        bits_per_sample: BITS,
        cb_size: 0,
    };

    let rc = waveInOpen(
        &mut HWI,
        WAVE_MAPPER,
        &fmt,
        0,
        0,
        0, // CALLBACK_NULL
    );
    if rc != 0 {
        return;
    }

    HDR1 = WaveHdr {
        data: BUF1.as_mut_ptr(),
        buffer_length: CAPTURE_BUF as u32,
        bytes_recorded: 0,
        flags: 0,
        loops: 0,
        next: core::ptr::null_mut(),
        reserved: 0,
    };
    HDR2 = WaveHdr {
        data: BUF2.as_mut_ptr(),
        buffer_length: CAPTURE_BUF as u32,
        bytes_recorded: 0,
        flags: 0,
        loops: 0,
        next: core::ptr::null_mut(),
        reserved: 0,
    };

    let hdr_size = core::mem::size_of::<WaveHdr>() as u32;
    waveInPrepareHeader(HWI, &mut HDR1, hdr_size);
    waveInPrepareHeader(HWI, &mut HDR2, hdr_size);
    waveInAddBuffer(HWI, &mut HDR1, hdr_size);
    waveInAddBuffer(HWI, &mut HDR2, hdr_size);

    waveInStart(HWI);
    RECORD_START = GetTickCount();
    ELAPSED_MS = 0;
    STATE = 1;
}

unsafe fn collect_buffers() {
    let hdr_size = core::mem::size_of::<WaveHdr>() as u32;

    if HDR1.flags & WHDR_DONE != 0 {
        let n = (HDR1.bytes_recorded as usize).min(MAX_RECORD - RECORDED_LEN);
        if n > 0 {
            core::ptr::copy_nonoverlapping(
                BUF1.as_ptr(),
                RECORDED.as_mut_ptr().add(RECORDED_LEN),
                n,
            );
            RECORDED_LEN += n;
        }
        HDR1.bytes_recorded = 0;
        HDR1.flags = WHDR_PREPARED;
        waveInAddBuffer(HWI, &mut HDR1, hdr_size);
    }

    if HDR2.flags & WHDR_DONE != 0 {
        let n = (HDR2.bytes_recorded as usize).min(MAX_RECORD - RECORDED_LEN);
        if n > 0 {
            core::ptr::copy_nonoverlapping(
                BUF2.as_ptr(),
                RECORDED.as_mut_ptr().add(RECORDED_LEN),
                n,
            );
            RECORDED_LEN += n;
        }
        HDR2.bytes_recorded = 0;
        HDR2.flags = WHDR_PREPARED;
        waveInAddBuffer(HWI, &mut HDR2, hdr_size);
    }
}

unsafe fn stop_recording() {
    waveInStop(HWI);
    waveInReset(HWI);
    collect_buffers();

    let hdr_size = core::mem::size_of::<WaveHdr>() as u32;
    waveInUnprepareHeader(HWI, &mut HDR1, hdr_size);
    waveInUnprepareHeader(HWI, &mut HDR2, hdr_size);
    waveInClose(HWI);
    HWI = core::ptr::null_mut();

    DURATION_S = ELAPSED_MS / 1000;
    STATE = 2;
}

// --- Lecture / sauvegarde -----------------------------------------------

unsafe fn play_recording() {
    if RECORDED_LEN == 0 {
        return;
    }
    let wav_len = build_wav_memory();
    PlaySoundA(
        WAV_FILE.as_ptr() as *const core::ffi::c_char,
        core::ptr::null_mut(),
        SND_MEMORY,
    );
    let _ = wav_len;
}

unsafe fn build_wav_memory() -> usize {
    let data_len = RECORDED_LEN;
    let file_len = 36 + data_len;
    let buf = &mut WAV_FILE;

    // RIFF header
    buf[0..4].copy_from_slice(b"RIFF");
    write_u32_le(buf, 4, file_len as u32);
    buf[8..12].copy_from_slice(b"WAVE");

    // fmt chunk
    buf[12..16].copy_from_slice(b"fmt ");
    write_u32_le(buf, 16, 16); // chunk size
    write_u16_le(buf, 20, 1); // PCM
    write_u16_le(buf, 22, CHANNELS);
    write_u32_le(buf, 24, SAMPLE_RATE);
    write_u32_le(buf, 28, BYTES_PER_SEC);
    write_u16_le(buf, 32, BLOCK_ALIGN);
    write_u16_le(buf, 34, BITS);

    // data chunk
    buf[36..40].copy_from_slice(b"data");
    write_u32_le(buf, 40, data_len as u32);
    core::ptr::copy_nonoverlapping(RECORDED.as_ptr(), buf.as_mut_ptr().add(44), data_len);

    44 + data_len
}

unsafe fn save_recording() {
    if RECORDED_LEN == 0 {
        return;
    }
    let wav_len = build_wav_memory();

    let path = c"C:\\enregistrement.wav";
    let handle = CreateFileA(
        path.as_ptr(),
        0x4000_0000, // GENERIC_WRITE
        0,
        core::ptr::null_mut(),
        2,    // CREATE_ALWAYS
        0x80, // FILE_ATTRIBUTE_NORMAL
        core::ptr::null_mut(),
    );
    if handle.is_null() || handle == -1isize as Handle {
        return;
    }
    let mut written = 0u32;
    WriteFile(
        handle,
        WAV_FILE.as_ptr(),
        wav_len as u32,
        &mut written,
        core::ptr::null_mut(),
    );
    CloseHandle(handle);

    SAVE_PATH_LEN = 1; // signal pour l'affichage
}

unsafe fn new_recording() {
    STATE = 0;
    RECORDED_LEN = 0;
    SAVE_PATH_LEN = 0;
    DURATION_S = 0;
    ELAPSED_MS = 0;
}

// --- Utilitaires --------------------------------------------------------

fn write_u16_le(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}
fn write_u32_le(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn format_mmss(seconds: u32) -> [u8; 8] {
    let m = seconds / 60;
    let s = seconds % 60;
    let mut r = [b'0', b'0', b':', b'0', b'0', 0, 0, 0];
    r[0] = b'0' + ((m / 10) % 10) as u8;
    r[1] = b'0' + (m % 10) as u8;
    r[3] = b'0' + ((s / 10) % 10) as u8;
    r[4] = b'0' + (s % 10) as u8;
    r
}
