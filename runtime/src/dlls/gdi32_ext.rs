//! gdi32 extensions — fonctions GDI additionnelles nécessaires à Notepad++
//! et paint.net : GetTextMetrics, ExtTextOut, MoveToEx/LineTo, SaveDC/RestoreDC,
//! régions de clipping, GetPixel/SetPixel, etc.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::OnceLock;

use crate::dlls::gui::{self, DrawCmd, Rect};

// ── TEXTMETRIC ─────────────────────────────────────────────────────

#[repr(C)]
pub(crate) struct TextMetricA {
    height: i32,
    ascent: i32,
    descent: i32,
    internal_leading: i32,
    external_leading: i32,
    ave_char_width: i32,
    max_char_width: i32,
    weight: i32,
    overhang: i32,
    digitized_aspect_x: i32,
    digitized_aspect_y: i32,
    first_char: u8,
    last_char: u8,
    default_char: u8,
    break_char: u8,
    italic: u8,
    underlined: u8,
    struck_out: u8,
    pitch_and_family: u8,
    char_set: u8,
}

#[repr(C)]
pub(crate) struct TextMetricW {
    height: i32,
    ascent: i32,
    descent: i32,
    internal_leading: i32,
    external_leading: i32,
    ave_char_width: i32,
    max_char_width: i32,
    weight: i32,
    overhang: i32,
    digitized_aspect_x: i32,
    digitized_aspect_y: i32,
    first_char: u16,
    last_char: u16,
    default_char: u16,
    break_char: u16,
    italic: u8,
    underlined: u8,
    struck_out: u8,
    pitch_and_family: u8,
    char_set: u8,
}

fn font_metrics(hdc: usize) -> (i32, i32, i32, i32, i32, bool, bool, bool) {
    let font = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
    let h = if font.height != 0 {
        font.height.abs()
    } else {
        16
    };
    let ascent = h * 80 / 100;
    let descent = h - ascent;
    let avg_w = h * 45 / 100;
    let weight = font.weight;
    (
        h,
        ascent,
        descent,
        avg_w,
        weight,
        font.italic,
        font.underline,
        font.strikeout,
    )
}

pub unsafe extern "win64" fn get_text_metrics_a(hdc: usize, out: *mut TextMetricA) -> i32 {
    if out.is_null() || gui::dc_window(hdc).is_none() {
        return 0;
    }
    let (h, ascent, descent, avg_w, weight, italic, underline, strikeout) = font_metrics(hdc);
    std::ptr::write(
        out,
        TextMetricA {
            height: h,
            ascent,
            descent,
            internal_leading: 0,
            external_leading: 0,
            ave_char_width: avg_w,
            max_char_width: avg_w * 2,
            weight,
            overhang: 0,
            digitized_aspect_x: 96,
            digitized_aspect_y: 96,
            first_char: 0x20,
            last_char: 0xFF,
            default_char: b'?',
            break_char: b' ',
            italic: italic as u8,
            underlined: underline as u8,
            struck_out: strikeout as u8,
            pitch_and_family: 0x31,
            char_set: 0,
        },
    );
    1
}

pub unsafe extern "win64" fn get_text_metrics_w(hdc: usize, out: *mut TextMetricW) -> i32 {
    if out.is_null() || gui::dc_window(hdc).is_none() {
        return 0;
    }
    let (h, ascent, descent, avg_w, weight, italic, underline, strikeout) = font_metrics(hdc);
    std::ptr::write(
        out,
        TextMetricW {
            height: h,
            ascent,
            descent,
            internal_leading: 0,
            external_leading: 0,
            ave_char_width: avg_w,
            max_char_width: avg_w * 2,
            weight,
            overhang: 0,
            digitized_aspect_x: 96,
            digitized_aspect_y: 96,
            first_char: 0x20,
            last_char: 0xFFFF,
            default_char: b'?' as u16,
            break_char: b' ' as u16,
            italic: italic as u8,
            underlined: underline as u8,
            struck_out: strikeout as u8,
            pitch_and_family: 0x31,
            char_set: 0,
        },
    );
    1
}

// ── ExtTextOut ──────────────────────────────────────────────────────

pub unsafe extern "win64" fn ext_text_out_a(
    hdc: usize,
    x: i32,
    y: i32,
    options: u32,
    rect: *const Rect,
    text: *const u8,
    count: u32,
    _dx: *const i32,
) -> i32 {
    let _ = options;
    if !rect.is_null() {
        let r = std::ptr::read_unaligned(rect);
        let bk = gui::dc_bk_color(hdc);
        gui::record(hdc, DrawCmd::FillRect { rect: r, color: bk });
    }
    if !text.is_null() && count > 0 {
        let s = crate::dlls::decode_ansi(std::slice::from_raw_parts(text, count as usize));
        let color = gui::dc_text_color(hdc);
        let bk = if gui::dc_bk_mode(hdc) == 1 {
            None
        } else {
            Some(gui::dc_bk_color(hdc))
        };
        let font = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
        gui::record(
            hdc,
            DrawCmd::Text {
                x,
                y,
                text: s,
                color,
                bk,
                font,
            },
        );
    }
    1
}

pub unsafe extern "win64" fn ext_text_out_w(
    hdc: usize,
    x: i32,
    y: i32,
    options: u32,
    rect: *const Rect,
    text: *const u16,
    count: u32,
    _dx: *const i32,
) -> i32 {
    let _ = options;
    if !rect.is_null() {
        let r = std::ptr::read_unaligned(rect);
        let bk = gui::dc_bk_color(hdc);
        gui::record(hdc, DrawCmd::FillRect { rect: r, color: bk });
    }
    if !text.is_null() && count > 0 {
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(text, count as usize));
        let color = gui::dc_text_color(hdc);
        let bk = if gui::dc_bk_mode(hdc) == 1 {
            None
        } else {
            Some(gui::dc_bk_color(hdc))
        };
        let font = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
        gui::record(
            hdc,
            DrawCmd::Text {
                x,
                y,
                text: s,
                color,
                bk,
                font,
            },
        );
    }
    1
}

// ── TextOut W ───────────────────────────────────────────────────────

pub unsafe extern "win64" fn text_out_w(
    hdc: usize,
    x: i32,
    y: i32,
    text: *const u16,
    count: i32,
) -> i32 {
    if text.is_null() || gui::dc_window(hdc).is_none() {
        return 0;
    }
    let s = if count < 0 {
        crate::dlls::wstr(text)
    } else {
        String::from_utf16_lossy(std::slice::from_raw_parts(text, count as usize))
    };
    let color = gui::dc_text_color(hdc);
    let bk = if gui::dc_bk_mode(hdc) == 1 {
        None
    } else {
        Some(gui::dc_bk_color(hdc))
    };
    let font = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
    gui::record(
        hdc,
        DrawCmd::Text {
            x,
            y,
            text: s,
            color,
            bk,
            font,
        },
    );
    1
}

// ── MoveToEx / LineTo ──────────────────────────────────────────────

static PEN_POS: OnceLock<Mutex<HashMap<usize, (i32, i32)>>> = OnceLock::new();

fn pen_pos() -> &'static Mutex<HashMap<usize, (i32, i32)>> {
    PEN_POS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub unsafe extern "win64" fn move_to_ex(hdc: usize, x: i32, y: i32, old: *mut [i32; 2]) -> i32 {
    let prev = pen_pos()
        .lock()
        .unwrap()
        .insert(hdc, (x, y))
        .unwrap_or((0, 0));
    if !old.is_null() {
        std::ptr::write(old, [prev.0, prev.1]);
    }
    1
}

pub extern "win64" fn line_to(hdc: usize, x: i32, y: i32) -> i32 {
    let (sx, sy) = pen_pos()
        .lock()
        .unwrap()
        .insert(hdc, (x, y))
        .unwrap_or((0, 0));
    let pen = gui::dc_pen(hdc);
    let color = gui::object_color(pen).unwrap_or(0);
    let _w = gui::object_pen_width(pen);
    gui::record(
        hdc,
        DrawCmd::Line {
            x1: sx,
            y1: sy,
            x2: x,
            y2: y,
            color,
        },
    );
    1
}

pub unsafe extern "win64" fn polyline(hdc: usize, points: *const [i32; 2], count: i32) -> i32 {
    if points.is_null() || count < 2 {
        return 0;
    }
    let pen = gui::dc_pen(hdc);
    let color = gui::object_color(pen).unwrap_or(0);
    let pts = std::slice::from_raw_parts(points, count as usize);
    for w in pts.windows(2) {
        gui::record(
            hdc,
            DrawCmd::Line {
                x1: w[0][0],
                y1: w[0][1],
                x2: w[1][0],
                y2: w[1][1],
                color,
            },
        );
    }
    1
}

pub unsafe extern "win64" fn polygon(hdc: usize, points: *const [i32; 2], count: i32) -> i32 {
    if points.is_null() || count < 2 {
        return 0;
    }
    let pen = gui::dc_pen(hdc);
    let color = gui::object_color(pen).unwrap_or(0);
    let pts = std::slice::from_raw_parts(points, count as usize);
    for w in pts.windows(2) {
        gui::record(
            hdc,
            DrawCmd::Line {
                x1: w[0][0],
                y1: w[0][1],
                x2: w[1][0],
                y2: w[1][1],
                color,
            },
        );
    }
    // Close the polygon
    let last = pts[count as usize - 1];
    let first = pts[0];
    gui::record(
        hdc,
        DrawCmd::Line {
            x1: last[0],
            y1: last[1],
            x2: first[0],
            y2: first[1],
            color,
        },
    );
    1
}

// ── SaveDC / RestoreDC ─────────────────────────────────────────────

static DC_STACK: OnceLock<Mutex<HashMap<usize, Vec<DcState>>>> = OnceLock::new();

#[derive(Clone)]
struct DcState {
    text_color: u32,
    bk_color: u32,
    bk_mode: u32,
    brush: usize,
    pen: usize,
    font: usize,
    pos: (i32, i32),
}

fn dc_stack() -> &'static Mutex<HashMap<usize, Vec<DcState>>> {
    DC_STACK.get_or_init(|| Mutex::new(HashMap::new()))
}

pub extern "win64" fn save_dc(hdc: usize) -> i32 {
    if gui::dc_window(hdc).is_none() {
        return 0;
    }
    let state = DcState {
        text_color: gui::dc_text_color(hdc),
        bk_color: gui::dc_bk_color(hdc),
        bk_mode: gui::dc_bk_mode(hdc),
        brush: gui::dc_brush(hdc),
        pen: gui::dc_pen(hdc),
        font: gui::dc_font(hdc),
        pos: pen_pos()
            .lock()
            .unwrap()
            .get(&hdc)
            .copied()
            .unwrap_or((0, 0)),
    };
    let mut stacks = dc_stack().lock().unwrap();
    let stack = stacks.entry(hdc).or_default();
    stack.push(state);
    stack.len() as i32
}

pub extern "win64" fn restore_dc(hdc: usize, saved: i32) -> i32 {
    let mut stacks = dc_stack().lock().unwrap();
    let Some(stack) = stacks.get_mut(&hdc) else {
        return 0;
    };
    let idx = if saved < 0 {
        let len = stack.len() as i32;
        (len + saved) as usize
    } else if saved > 0 {
        (saved - 1) as usize
    } else {
        return 0;
    };
    if idx >= stack.len() {
        return 0;
    }
    let state = stack[idx].clone();
    stack.truncate(idx);
    gui::set_dc_text_color(hdc, state.text_color);
    gui::set_dc_bk_color(hdc, state.bk_color);
    gui::set_dc_bk_mode(hdc, state.bk_mode);
    gui::select_object(hdc, state.brush);
    gui::select_object(hdc, state.pen);
    gui::select_object(hdc, state.font);
    pen_pos().lock().unwrap().insert(hdc, state.pos);
    1
}

// ── Régions (stubs) ────────────────────────────────────────────────

pub extern "win64" fn create_rect_rgn(left: i32, top: i32, right: i32, bottom: i32) -> usize {
    let _ = (left, top, right, bottom);
    0x1 // factice non nul
}

pub extern "win64" fn create_rect_rgn_indirect(_rect: *const Rect) -> usize {
    0x1
}

pub extern "win64" fn select_clip_rgn(_hdc: usize, _rgn: usize) -> i32 {
    1 // SIMPLEREGION
}

pub extern "win64" fn intersect_clip_rect(_hdc: usize, _l: i32, _t: i32, _r: i32, _b: i32) -> i32 {
    1 // SIMPLEREGION
}

pub extern "win64" fn exclude_clip_rect(_hdc: usize, _l: i32, _t: i32, _r: i32, _b: i32) -> i32 {
    1 // SIMPLEREGION
}

pub extern "win64" fn combine_rgn(_dst: usize, _src1: usize, _src2: usize, _mode: i32) -> i32 {
    1 // SIMPLEREGION
}

pub extern "win64" fn set_rect_rgn(_rgn: usize, _l: i32, _t: i32, _r: i32, _b: i32) -> i32 {
    1
}

// ── Pixel ──────────────────────────────────────────────────────────

pub extern "win64" fn get_pixel(_hdc: usize, _x: i32, _y: i32) -> u32 {
    0x00FF_FFFF // CLR_INVALID stub → blanc
}

pub extern "win64" fn set_pixel(hdc: usize, x: i32, y: i32, color: u32) -> u32 {
    let rect = Rect {
        left: x,
        top: y,
        right: x + 1,
        bottom: y + 1,
    };
    gui::record(
        hdc,
        DrawCmd::FillRect {
            rect,
            color: color & 0x00FF_FFFF,
        },
    );
    color & 0x00FF_FFFF
}

pub extern "win64" fn set_pixel_v(hdc: usize, x: i32, y: i32, color: u32) -> i32 {
    set_pixel(hdc, x, y, color);
    1
}

// ── FillRect / FrameRect / InvertRect ──────────────────────────────

pub unsafe extern "win64" fn fill_rect(hdc: usize, rect: *const Rect, brush: usize) -> i32 {
    if rect.is_null() {
        return 0;
    }
    let r = std::ptr::read_unaligned(rect);
    let color = gui::object_color(brush).unwrap_or(0x00FF_FFFF);
    gui::record(hdc, DrawCmd::FillRect { rect: r, color });
    1
}

pub unsafe extern "win64" fn frame_rect(hdc: usize, rect: *const Rect, brush: usize) -> i32 {
    if rect.is_null() {
        return 0;
    }
    let r = std::ptr::read_unaligned(rect);
    let color = gui::object_color(brush).unwrap_or(0);
    gui::record(
        hdc,
        DrawCmd::FrameRect {
            rect: r,
            color,
            width: 1,
        },
    );
    1
}

pub unsafe extern "win64" fn invert_rect(_hdc: usize, _rect: *const Rect) -> i32 {
    1
}

// ── Mapping / coordonnées ──────────────────────────────────────────

pub extern "win64" fn set_map_mode(_hdc: usize, _mode: i32) -> i32 {
    1
} // MM_TEXT
pub extern "win64" fn get_map_mode(_hdc: usize) -> i32 {
    1
} // MM_TEXT
pub extern "win64" fn set_viewport_org_ex(_hdc: usize, _x: i32, _y: i32, _old: usize) -> i32 {
    1
}
pub extern "win64" fn set_window_org_ex(_hdc: usize, _x: i32, _y: i32, _old: usize) -> i32 {
    1
}
pub extern "win64" fn offset_viewport_org_ex(_hdc: usize, _x: i32, _y: i32, _old: usize) -> i32 {
    1
}

// ── Stretch mode ───────────────────────────────────────────────────

pub extern "win64" fn set_stretch_blt_mode(_hdc: usize, _mode: i32) -> i32 {
    1
}
pub extern "win64" fn get_stretch_blt_mode(_hdc: usize) -> i32 {
    3
} // COLORONCOLOR

// ── ROP2 ───────────────────────────────────────────────────────────

pub extern "win64" fn set_rop2(_hdc: usize, _mode: i32) -> i32 {
    13
} // R2_COPYPEN
pub extern "win64" fn get_rop2(_hdc: usize) -> i32 {
    13
}

// ── Palettes (stubs) ──────────────────────────────────────────────

pub extern "win64" fn realize_palette(_hdc: usize) -> u32 {
    0
}
pub extern "win64" fn select_palette(_hdc: usize, _pal: usize, _force: i32) -> usize {
    0
}

// ── GetDIBits / SetDIBitsToDevice / StretchDIBits ──────────────────

use crate::dlls::gdi32::BitmapInfoHeader;

unsafe fn decode_dib_pixels(
    info: &BitmapInfoHeader,
    info_base: *const u8,
    bits: *const u8,
    src_x: i32,
    src_y: i32,
    src_w: usize,
    src_h: usize,
) -> Option<Vec<u32>> {
    if bits.is_null()
        || src_w == 0
        || src_h == 0
        || info.width <= 0
        || info.height == 0
        || info.size < 40
        || info.size > 124
        || info.planes != 1
        || info.compression != 0
        || src_x < 0
        || src_y < 0
    {
        return None;
    }
    let bpp = info.bit_count as usize;
    if !matches!(bpp, 1 | 4 | 8 | 16 | 24 | 32) {
        return None;
    }
    let abs_h = info.height.unsigned_abs() as usize;
    let width = info.width as usize;
    if (src_x as usize).checked_add(src_w)? > width || (src_y as usize).checked_add(src_h)? > abs_h
    {
        return None;
    }
    if width.checked_mul(abs_h)? > 8 * 1024 * 1024 {
        return None;
    }
    let top_down = info.height < 0;
    let stride = width.checked_mul(bpp)?.checked_add(31)? / 32 * 4;
    let count = src_w.checked_mul(src_h)?;
    if count > 8 * 1024 * 1024 {
        return None;
    }
    let palette_offset = info.size as usize;
    let palette_len = if bpp <= 8 {
        let max = 1usize << bpp;
        if info.colors_used == 0 {
            max
        } else {
            (info.colors_used as usize).min(max)
        }
    } else {
        0
    };
    let mut pixels = Vec::with_capacity(count);
    for dy in 0..src_h {
        let row = if top_down {
            (src_y as usize).wrapping_add(dy)
        } else {
            abs_h
                .checked_sub(1)?
                .checked_sub((src_y as usize).wrapping_add(dy))?
        };
        if row >= abs_h {
            pixels.extend(std::iter::repeat_n(0u32, src_w));
            continue;
        }
        let row_ptr = bits.add(row * stride);
        for dx in 0..src_w {
            let x = (src_x as usize).wrapping_add(dx);
            if x >= info.width as usize {
                pixels.push(0);
                continue;
            }
            let color = match bpp {
                32 => {
                    let off = x * 4;
                    let b = *row_ptr.add(off) as u32;
                    let g = *row_ptr.add(off + 1) as u32;
                    let r = *row_ptr.add(off + 2) as u32;
                    r | (g << 8) | (b << 16)
                }
                24 => {
                    let off = x * 3;
                    let b = *row_ptr.add(off) as u32;
                    let g = *row_ptr.add(off + 1) as u32;
                    let r = *row_ptr.add(off + 2) as u32;
                    r | (g << 8) | (b << 16)
                }
                16 => {
                    let off = x * 2;
                    let v = u16::from_le_bytes([*row_ptr.add(off), *row_ptr.add(off + 1)]);
                    let r = ((v >> 10) & 0x1F) as u32 * 255 / 31;
                    let g = ((v >> 5) & 0x1F) as u32 * 255 / 31;
                    let b = (v & 0x1F) as u32 * 255 / 31;
                    r | (g << 8) | (b << 16)
                }
                8 => {
                    let idx = *row_ptr.add(x) as usize;
                    if idx >= palette_len {
                        return None;
                    }
                    read_palette_entry(info_base, palette_offset, idx)
                }
                4 => {
                    let byte = *row_ptr.add(x / 2);
                    let idx = if x % 2 == 0 {
                        (byte >> 4) as usize
                    } else {
                        (byte & 0x0F) as usize
                    };
                    if idx >= palette_len {
                        return None;
                    }
                    read_palette_entry(info_base, palette_offset, idx)
                }
                1 => {
                    let byte = *row_ptr.add(x / 8);
                    let bit = (byte >> (7 - (x % 8))) & 1;
                    if bit as usize >= palette_len {
                        return None;
                    }
                    read_palette_entry(info_base, palette_offset, bit as usize)
                }
                _ => 0,
            };
            pixels.push(color);
        }
    }
    Some(pixels)
}

unsafe fn read_palette_entry(info_base: *const u8, palette_offset: usize, idx: usize) -> u32 {
    let entry = info_base.add(palette_offset + idx * 4);
    let b = *entry as u32;
    let g = *entry.add(1) as u32;
    let r = *entry.add(2) as u32;
    r | (g << 8) | (b << 16)
}

#[cfg(test)]
mod reviewed_dib_tests {
    use super::*;
    fn header(bpp: u16, height: i32) -> BitmapInfoHeader {
        BitmapInfoHeader {
            size: 40,
            width: 2,
            height,
            planes: 1,
            bit_count: bpp,
            compression: 0,
            size_image: 0,
            x_pels_per_meter: 0,
            y_pels_per_meter: 0,
            colors_used: 0,
            colors_important: 0,
        }
    }
    #[test]
    fn palette_is_read_from_bitmap_info_not_pixel_buffer() {
        #[repr(C)]
        struct Info {
            header: BitmapInfoHeader,
            colors: [[u8; 4]; 2],
        }
        let mut info = Info {
            header: header(8, -1),
            colors: [[0, 0, 255, 0], [255, 0, 0, 0]],
        };
        info.header.colors_used = 2;
        let pixels = [0, 1, 0, 0];
        unsafe {
            assert_eq!(
                decode_dib_pixels(
                    &info.header,
                    &info as *const _ as *const u8,
                    pixels.as_ptr(),
                    0,
                    0,
                    2,
                    1
                ),
                Some(vec![0x0000ff, 0xff0000])
            );
            let invalid = [2, 0, 0, 0];
            assert!(decode_dib_pixels(
                &info.header,
                &info as *const _ as *const u8,
                invalid.as_ptr(),
                0,
                0,
                2,
                1
            )
            .is_none());
        }
    }
    #[test]
    fn rgb_orientation_and_invalid_geometry() {
        let mut info = header(24, 2);
        let pixels = [255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 255, 0, 0, 255, 0, 0];
        unsafe {
            let base = &info as *const _ as *const u8;
            assert_eq!(
                decode_dib_pixels(&info, base, pixels.as_ptr(), 0, 0, 2, 2),
                Some(vec![0xff, 0xff, 0xff0000, 0xff0000])
            );
            assert!(decode_dib_pixels(&info, base, pixels.as_ptr(), -1, 0, 2, 1).is_none());
            assert!(decode_dib_pixels(&info, base, pixels.as_ptr(), 0, 2, 2, 1).is_none());
            info.width = -2;
            assert!(decode_dib_pixels(&info, base, pixels.as_ptr(), 0, 0, 2, 1).is_none());
        }
    }
}

pub unsafe extern "win64" fn get_dib_its(
    _hdc: usize,
    _bmp: usize,
    _start: u32,
    _lines: u32,
    _bits: *mut c_void,
    _info: *mut c_void,
    _usage: u32,
) -> i32 {
    0
}

pub unsafe extern "win64" fn set_di_bits_to_device(
    hdc: usize,
    x_dst: i32,
    y_dst: i32,
    w: u32,
    h: u32,
    src_x: i32,
    src_y: i32,
    _start: u32,
    lines: u32,
    bits: *const c_void,
    info: *const c_void,
    _usage: u32,
) -> i32 {
    if bits.is_null()
        || info.is_null()
        || w == 0
        || h == 0
        || lines == 0
        || _usage != 0
        || _start != 0
    {
        return 0;
    }
    let header = std::ptr::read_unaligned(info as *const BitmapInfoHeader);
    // Le transfert de bandes partielles nécessite une autre origine de bits.
    if lines < header.height.unsigned_abs() {
        return 0;
    }
    let draw_h = (h as usize).min(lines as usize);
    let draw_w = w as usize;
    let Some(pixels) = decode_dib_pixels(
        &header,
        info as *const u8,
        bits as *const u8,
        src_x,
        src_y,
        draw_w,
        draw_h,
    ) else {
        return 0;
    };
    let Some(right) = x_dst.checked_add(draw_w as i32) else {
        return 0;
    };
    let Some(bottom) = y_dst.checked_add(draw_h as i32) else {
        return 0;
    };
    let rect = Rect {
        left: x_dst,
        top: y_dst,
        right,
        bottom,
    };
    gui::record(
        hdc,
        DrawCmd::Bitmap {
            rect,
            pixels,
            rop: gui::RasterOp::Copy,
        },
    );
    draw_h as i32
}

pub unsafe extern "win64" fn stretch_di_bits(
    hdc: usize,
    x_dst: i32,
    y_dst: i32,
    w_dst: i32,
    h_dst: i32,
    src_x: i32,
    src_y: i32,
    src_w: i32,
    src_h: i32,
    bits: *const c_void,
    info: *const c_void,
    _usage: u32,
    rop: u32,
) -> i32 {
    if bits.is_null()
        || info.is_null()
        || w_dst == 0
        || h_dst == 0
        || src_w <= 0
        || src_h <= 0
        || _usage != 0
    {
        return 0;
    }
    let header = std::ptr::read_unaligned(info as *const BitmapInfoHeader);
    let Some(src_pixels) = decode_dib_pixels(
        &header,
        info as *const u8,
        bits as *const u8,
        src_x,
        src_y,
        src_w as usize,
        src_h as usize,
    ) else {
        return 0;
    };
    let operation = match rop {
        0x00CC_0020 => gui::RasterOp::Copy,
        0x00EE_0086 => gui::RasterOp::Or,
        0x0088_00C6 => gui::RasterOp::And,
        0x0066_0046 => gui::RasterOp::Xor,
        0x0033_0008 => gui::RasterOp::CopyInverted,
        _ => return 0,
    };
    let abs_w = w_dst.unsigned_abs() as usize;
    let abs_h = h_dst.unsigned_abs() as usize;
    let count = match abs_w.checked_mul(abs_h) {
        Some(c) if c <= 8 * 1024 * 1024 => c,
        _ => return 0,
    };
    let mut dst_pixels = Vec::with_capacity(count);
    for dy in 0..abs_h {
        for dx in 0..abs_w {
            let sy = (if h_dst < 0 { abs_h - 1 - dy } else { dy }) * src_h as usize / abs_h;
            let sx = (if w_dst < 0 { abs_w - 1 - dx } else { dx }) * src_w as usize / abs_w;
            dst_pixels.push(src_pixels[sy * src_w as usize + sx]);
        }
    }
    let Some(x_end) = x_dst.checked_add(w_dst) else {
        return 0;
    };
    let Some(y_end) = y_dst.checked_add(h_dst) else {
        return 0;
    };
    let (left, right) = (x_dst.min(x_end), x_dst.max(x_end));
    let (top, bottom) = (y_dst.min(y_end), y_dst.max(y_end));
    let rect = Rect {
        left,
        top,
        right,
        bottom,
    };
    gui::record(
        hdc,
        DrawCmd::Bitmap {
            rect,
            pixels: dst_pixels,
            rop: operation,
        },
    );
    abs_h as i32
}

// ── CreateFontIndirect ─────────────────────────────────────────────

pub unsafe extern "win64" fn create_font_indirect_a(
    lf: *const crate::dlls::gdi32::LogFontA,
) -> usize {
    if lf.is_null() {
        return 0;
    }
    let lf = std::ptr::read_unaligned(lf);
    let face_len = lf
        .face
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(lf.face.len());
    let face = crate::dlls::decode_ansi(&lf.face[..face_len]);
    gui::create_font(gui::FontSpec {
        height: lf.height,
        width: lf.width,
        weight: lf.weight,
        italic: lf.italic != 0,
        underline: lf.underline != 0,
        strikeout: lf.strikeout != 0,
        face: if face.is_empty() {
            "fixed".into()
        } else {
            face
        },
    })
}

pub unsafe extern "win64" fn create_font_indirect_w(
    lf: *const crate::dlls::gdi32::LogFontW,
) -> usize {
    if lf.is_null() {
        return 0;
    }
    let lf = std::ptr::read_unaligned(lf);
    let face_len = lf
        .face
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(lf.face.len());
    let face = String::from_utf16_lossy(&lf.face[..face_len]);
    gui::create_font(gui::FontSpec {
        height: lf.height,
        width: lf.width,
        weight: lf.weight,
        italic: lf.italic != 0,
        underline: lf.underline != 0,
        strikeout: lf.strikeout != 0,
        face: if face.is_empty() {
            "fixed".into()
        } else {
            face
        },
    })
}

// ── EnumFontFamilies (stub — renvoie 1, pas d'appel du callback) ──

pub unsafe extern "win64" fn enum_font_families_ex_a(
    _hdc: usize,
    _logfont: *const c_void,
    _proc: usize,
    _lparam: isize,
    _flags: u32,
) -> i32 {
    1
}

pub unsafe extern "win64" fn enum_font_families_ex_w(
    _hdc: usize,
    _logfont: *const c_void,
    _proc: usize,
    _lparam: isize,
    _flags: u32,
) -> i32 {
    1
}

// ── GetCharWidth / ABC (stub) ──────────────────────────────────────

pub unsafe extern "win64" fn get_char_width_32a(
    hdc: usize,
    first: u32,
    last: u32,
    widths: *mut i32,
) -> i32 {
    if widths.is_null() || first > last {
        return 0;
    }
    let font = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
    let avg_w = if font.height != 0 {
        font.height.abs() * 45 / 100
    } else {
        8
    };
    for i in 0..=(last - first) as usize {
        *widths.add(i) = avg_w;
    }
    1
}

pub unsafe extern "win64" fn get_char_width_32w(
    hdc: usize,
    first: u32,
    last: u32,
    widths: *mut i32,
) -> i32 {
    get_char_width_32a(hdc, first, last, widths)
}

#[repr(C)]
pub(crate) struct Abc {
    a: i32,
    b: u32,
    c: i32,
}

pub unsafe extern "win64" fn get_char_abc_widths_a(
    hdc: usize,
    first: u32,
    last: u32,
    abc: *mut Abc,
) -> i32 {
    if abc.is_null() || first > last {
        return 0;
    }
    let font = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
    let avg_w = if font.height != 0 {
        font.height.abs() * 45 / 100
    } else {
        8
    };
    for i in 0..=(last - first) as usize {
        std::ptr::write(
            abc.add(i),
            Abc {
                a: 0,
                b: avg_w as u32,
                c: 0,
            },
        );
    }
    1
}

pub unsafe extern "win64" fn get_char_abc_widths_w(
    hdc: usize,
    first: u32,
    last: u32,
    abc: *mut Abc,
) -> i32 {
    get_char_abc_widths_a(hdc, first, last, abc)
}

// ── Brush patterns ─────────────────────────────────────────────────

pub extern "win64" fn create_hatch_brush(_style: i32, color: u32) -> usize {
    gui::create_solid_brush(color)
}

pub extern "win64" fn create_pattern_brush(_bitmap: usize) -> usize {
    gui::create_solid_brush(0x00C0_C0C0) // gris
}

pub unsafe extern "win64" fn create_brush_indirect(
    lb: *const crate::dlls::gdi32::LogBrush,
) -> usize {
    if lb.is_null() {
        return 0;
    }
    let lb = std::ptr::read_unaligned(lb);
    gui::create_solid_brush(lb.color)
}

// ── Pen étendu ─────────────────────────────────────────────────────

pub unsafe extern "win64" fn ext_create_pen(
    style: u32,
    width: u32,
    _brush: *const c_void,
    _n_styles: u32,
    _styles: *const u32,
) -> usize {
    let color = if !_brush.is_null() {
        (*(_brush as *const crate::dlls::gdi32::LogBrush)).color
    } else {
        0
    };
    gui::create_pen(style, width as i32, color)
}

// ── Divers ─────────────────────────────────────────────────────────

pub extern "win64" fn get_current_object(hdc: usize, obj_type: u32) -> usize {
    match obj_type {
        1 => gui::dc_pen(hdc),   // OBJ_PEN
        2 => gui::dc_brush(hdc), // OBJ_BRUSH
        6 => gui::dc_font(hdc),  // OBJ_FONT
        _ => 0,
    }
}

pub extern "win64" fn get_object_type(obj: usize) -> u32 {
    if obj == 0 {
        return 0;
    }
    if gui::object_info(obj).is_some() {
        1
    } else {
        0
    } // OBJ_PEN par défaut
}

pub extern "win64" fn set_text_align(_hdc: usize, _align: u32) -> u32 {
    0
}
pub extern "win64" fn get_text_align(_hdc: usize) -> u32 {
    0
}
pub extern "win64" fn set_text_justification(_hdc: usize, _extra: i32, _count: i32) -> i32 {
    1
}
pub extern "win64" fn set_text_char_extra(_hdc: usize, _extra: i32) -> i32 {
    0
}
pub extern "win64" fn get_text_char_extra(_hdc: usize) -> i32 {
    0
}

pub unsafe extern "win64" fn get_text_face_a(hdc: usize, count: i32, buf: *mut u8) -> i32 {
    let font = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
    if buf.is_null() {
        return font.face.len() as i32 + 1;
    }
    let b = font.face.as_bytes();
    let n = b.len().min((count as usize).saturating_sub(1));
    std::ptr::copy_nonoverlapping(b.as_ptr(), buf, n);
    *buf.add(n) = 0;
    n as i32
}

pub unsafe extern "win64" fn get_text_face_w(hdc: usize, count: i32, buf: *mut u16) -> i32 {
    let font = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
    let w: Vec<u16> = font.face.encode_utf16().collect();
    if buf.is_null() {
        return w.len() as i32 + 1;
    }
    let n = w.len().min((count as usize).saturating_sub(1));
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, n);
    *buf.add(n) = 0;
    n as i32
}

pub unsafe extern "win64" fn get_clip_box(_hdc: usize, rect: *mut Rect) -> i32 {
    if !rect.is_null() {
        std::ptr::write(
            rect,
            Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
        );
    }
    1 // SIMPLEREGION
}

pub extern "win64" fn get_clip_rgn(_hdc: usize, _rgn: usize) -> i32 {
    0
} // pas de région

// ── GdiFlush / GdiSetBatchLimit ────────────────────────────────────

pub extern "win64" fn gdi_flush() -> i32 {
    1
}
pub extern "win64" fn gdi_set_batch_limit(_limit: u32) -> u32 {
    1
}

// ── CreateBitmap (stub) ────────────────────────────────────────────

pub unsafe extern "win64" fn create_bitmap(
    width: i32,
    height: i32,
    _planes: u32,
    _bpp: u32,
    _bits: *const c_void,
) -> usize {
    gui::create_compatible_bitmap(width, height).unwrap_or(0)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetTextMetricsA" => fn get_text_metrics_a,
        "GetTextMetricsW" => fn get_text_metrics_w,
        "ExtTextOutA" => fn ext_text_out_a,
        "ExtTextOutW" => fn ext_text_out_w,
        "TextOutW" => fn text_out_w,
        "MoveToEx" => fn move_to_ex,
        "LineTo" => fn line_to,
        "Polyline" => fn polyline,
        "Polygon" => fn polygon,
        "SaveDC" => fn save_dc,
        "RestoreDC" => fn restore_dc,
        "CreateRectRgn" => fn create_rect_rgn,
        "CreateRectRgnIndirect" => fn create_rect_rgn_indirect,
        "SelectClipRgn" => fn select_clip_rgn,
        "IntersectClipRect" => fn intersect_clip_rect,
        "ExcludeClipRect" => fn exclude_clip_rect,
        // DeleteObject reste fourni par gdi32 : le stub de région ne doit
        // pas accepter les objets système ni empêcher la libération réelle.
        "CombineRgn" => fn combine_rgn,
        "SetRectRgn" => fn set_rect_rgn,
        "GetPixel" => fn get_pixel,
        "SetPixel" => fn set_pixel,
        "SetPixelV" => fn set_pixel_v,
        "FillRect" => fn fill_rect,
        "FrameRect" => fn frame_rect,
        "InvertRect" => fn invert_rect,
        "SetMapMode" => fn set_map_mode,
        "GetMapMode" => fn get_map_mode,
        "SetViewportOrgEx" => fn set_viewport_org_ex,
        "SetWindowOrgEx" => fn set_window_org_ex,
        "OffsetViewportOrgEx" => fn offset_viewport_org_ex,
        "SetStretchBltMode" => fn set_stretch_blt_mode,
        "GetStretchBltMode" => fn get_stretch_blt_mode,
        "SetROP2" => fn set_rop2,
        "GetROP2" => fn get_rop2,
        "RealizePalette" => fn realize_palette,
        "SelectPalette" => fn select_palette,
        "GetDIBits" => fn get_dib_its,
        "SetDIBitsToDevice" => fn set_di_bits_to_device,
        "StretchDIBits" => fn stretch_di_bits,
        "CreateFontIndirectA" => fn create_font_indirect_a,
        "CreateFontIndirectW" => fn create_font_indirect_w,
        "EnumFontFamiliesExA" => fn enum_font_families_ex_a,
        "EnumFontFamiliesExW" => fn enum_font_families_ex_w,
        "GetCharWidth32A" => fn get_char_width_32a,
        "GetCharWidth32W" => fn get_char_width_32w,
        "GetCharWidthA" => fn get_char_width_32a,
        "GetCharWidthW" => fn get_char_width_32w,
        "GetCharABCWidthsA" => fn get_char_abc_widths_a,
        "GetCharABCWidthsW" => fn get_char_abc_widths_w,
        "CreateHatchBrush" => fn create_hatch_brush,
        "CreatePatternBrush" => fn create_pattern_brush,
        "CreateBrushIndirect" => fn create_brush_indirect,
        "ExtCreatePen" => fn ext_create_pen,
        "GetCurrentObject" => fn get_current_object,
        "GetObjectType" => fn get_object_type,
        "SetTextAlign" => fn set_text_align,
        "GetTextAlign" => fn get_text_align,
        "SetTextJustification" => fn set_text_justification,
        "SetTextCharacterExtra" => fn set_text_char_extra,
        "GetTextCharacterExtra" => fn get_text_char_extra,
        "GetTextFaceA" => fn get_text_face_a,
        "GetTextFaceW" => fn get_text_face_w,
        "GetClipBox" => fn get_clip_box,
        "GetClipRgn" => fn get_clip_rgn,
        "GdiFlush" => fn gdi_flush,
        "GdiSetBatchLimit" => fn gdi_set_batch_limit,
        "CreateBitmap" => fn create_bitmap,
    }
}
