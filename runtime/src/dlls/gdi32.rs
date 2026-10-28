//! gdi32 minimal : objets GDI (pinceaux, plumes) et primitives de dessin qui
//! alimentent la liste de commandes du noyau GUI. Le backend (X11) rejoue cette
//! liste à chaque cycle de peinture.

use crate::dlls::gui::{self, DrawCmd, FontSpec, Rect};
use std::collections::HashMap;
use std::ffi::c_void;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BitmapInfoHeader {
    pub size: u32,
    pub width: i32,
    pub height: i32,
    pub planes: u16,
    pub bit_count: u16,
    pub compression: u32,
    pub size_image: u32,
    pub x_pels_per_meter: i32,
    pub y_pels_per_meter: i32,
    pub colors_used: u32,
    pub colors_important: u32,
}

#[repr(C)]
pub struct BitmapObject {
    pub bm_type: i32,
    pub bm_width: i32,
    pub bm_height: i32,
    pub bm_width_bytes: i32,
    pub bm_planes: u16,
    pub bm_bits_pixel: u16,
    pub bm_bits: *mut c_void,
}

#[repr(C)]
pub struct LogBrush {
    pub style: u32,
    pub color: u32,
    pub hatch: usize,
}

#[repr(C)]
pub struct WinPoint {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
pub struct LogPen {
    pub style: u32,
    pub width: WinPoint,
    pub color: u32,
}

#[repr(C)]
pub struct LogFontA {
    pub height: i32,
    pub width: i32,
    pub escapement: i32,
    pub orientation: i32,
    pub weight: i32,
    pub italic: u8,
    pub underline: u8,
    pub strikeout: u8,
    pub charset: u8,
    pub out_precision: u8,
    pub clip_precision: u8,
    pub quality: u8,
    pub pitch_and_family: u8,
    pub face: [u8; 32],
}

#[repr(C)]
pub struct LogFontW {
    pub height: i32,
    pub width: i32,
    pub escapement: i32,
    pub orientation: i32,
    pub weight: i32,
    pub italic: u8,
    pub underline: u8,
    pub strikeout: u8,
    pub charset: u8,
    pub out_precision: u8,
    pub clip_precision: u8,
    pub quality: u8,
    pub pitch_and_family: u8,
    pub face: [u16; 32],
}

pub extern "win64" fn get_stock_object(index: i32) -> usize {
    if index >= 0 {
        (index as usize) + 1
    } else {
        0
    }
}

pub extern "win64" fn create_solid_brush(color: u32) -> usize {
    gui::create_solid_brush(color)
}

pub extern "win64" fn create_pen(style: u32, width: i32, color: u32) -> usize {
    gui::create_pen(style, width, color)
}

fn create_font_common(
    height: i32,
    width: i32,
    weight: i32,
    italic: u32,
    underline: u32,
    strikeout: u32,
    face: String,
) -> usize {
    gui::create_font(FontSpec {
        height,
        width,
        weight,
        italic: italic != 0,
        underline: underline != 0,
        strikeout: strikeout != 0,
        face: if face.is_empty() {
            "fixed".to_string()
        } else {
            face
        },
    })
}

pub unsafe extern "win64" fn create_font_a(
    height: i32,
    width: i32,
    _escapement: i32,
    _orientation: i32,
    weight: i32,
    italic: u32,
    underline: u32,
    strikeout: u32,
    _charset: u32,
    _out_precision: u32,
    _clip_precision: u32,
    _quality: u32,
    _pitch_and_family: u32,
    face: *const u8,
) -> usize {
    let face = if face.is_null() {
        String::new()
    } else {
        crate::dlls::cstr(face)
    };
    create_font_common(height, width, weight, italic, underline, strikeout, face)
}

pub unsafe extern "win64" fn create_font_w(
    height: i32,
    width: i32,
    _escapement: i32,
    _orientation: i32,
    weight: i32,
    italic: u32,
    underline: u32,
    strikeout: u32,
    _charset: u32,
    _out_precision: u32,
    _clip_precision: u32,
    _quality: u32,
    _pitch_and_family: u32,
    face: *const u16,
) -> usize {
    let face = if face.is_null() {
        String::new()
    } else {
        crate::dlls::wstr(face)
    };
    create_font_common(height, width, weight, italic, underline, strikeout, face)
}

pub extern "win64" fn select_object(hdc: usize, obj: usize) -> usize {
    gui::select_object(hdc, obj)
}

pub extern "win64" fn delete_object(obj: usize) -> i32 {
    gui::delete_object(obj) as i32
}

pub unsafe extern "win64" fn get_object_a(obj: usize, size: i32, out: *mut c_void) -> i32 {
    get_object_common(obj, size, out, false)
}

pub unsafe extern "win64" fn get_object_w(obj: usize, size: i32, out: *mut c_void) -> i32 {
    get_object_common(obj, size, out, true)
}

unsafe fn get_object_common(obj: usize, size: i32, out: *mut c_void, wide: bool) -> i32 {
    let Some(info) = gui::object_info(obj) else {
        return 0;
    };
    let needed = match &info {
        gui::GdiObjectInfo::Brush { .. } => core::mem::size_of::<LogBrush>(),
        gui::GdiObjectInfo::Pen { .. } => core::mem::size_of::<LogPen>(),
        gui::GdiObjectInfo::Font(_) => {
            if wide {
                core::mem::size_of::<LogFontW>()
            } else {
                core::mem::size_of::<LogFontA>()
            }
        }
        gui::GdiObjectInfo::Bitmap { .. } => core::mem::size_of::<BitmapObject>(),
    };
    if out.is_null() && size == 0 {
        return needed as i32;
    }
    if out.is_null() || size < needed as i32 {
        crate::process::set_last_error(87); // ERROR_INVALID_PARAMETER
        return 0;
    }
    match info {
        gui::GdiObjectInfo::Brush {
            style,
            color,
            hatch,
        } => {
            std::ptr::write_unaligned(
                out.cast::<LogBrush>(),
                LogBrush {
                    style,
                    color,
                    hatch,
                },
            );
        }
        gui::GdiObjectInfo::Pen {
            style,
            width,
            color,
        } => {
            std::ptr::write_unaligned(
                out.cast::<LogPen>(),
                LogPen {
                    style,
                    width: WinPoint { x: width, y: 0 },
                    color,
                },
            );
        }
        gui::GdiObjectInfo::Bitmap {
            width,
            height,
            width_bytes,
            bits_per_pixel,
            bits,
        } => {
            std::ptr::write_unaligned(
                out.cast::<BitmapObject>(),
                BitmapObject {
                    bm_type: 0,
                    bm_width: width as i32,
                    bm_height: height as i32,
                    bm_width_bytes: width_bytes as i32,
                    bm_planes: 1,
                    bm_bits_pixel: bits_per_pixel,
                    bm_bits: bits as *mut c_void,
                },
            );
        }
        gui::GdiObjectInfo::Font(font) if wide => {
            let mut face = [0u16; 32];
            for (dst, ch) in face.iter_mut().zip(font.face.encode_utf16()) {
                *dst = ch;
            }
            std::ptr::write_unaligned(
                out.cast::<LogFontW>(),
                LogFontW {
                    height: font.height,
                    width: font.width,
                    escapement: 0,
                    orientation: 0,
                    weight: font.weight,
                    italic: font.italic as u8,
                    underline: font.underline as u8,
                    strikeout: font.strikeout as u8,
                    charset: 1,
                    out_precision: 0,
                    clip_precision: 0,
                    quality: 0,
                    pitch_and_family: 0,
                    face,
                },
            );
        }
        gui::GdiObjectInfo::Font(font) => {
            let mut face = [0u8; 32];
            let ansi = crate::dlls::encode_ansi(&font.face);
            let count = ansi.len().min(face.len() - 1);
            face[..count].copy_from_slice(&ansi[..count]);
            std::ptr::write_unaligned(
                out.cast::<LogFontA>(),
                LogFontA {
                    height: font.height,
                    width: font.width,
                    escapement: 0,
                    orientation: 0,
                    weight: font.weight,
                    italic: font.italic as u8,
                    underline: font.underline as u8,
                    strikeout: font.strikeout as u8,
                    charset: 1,
                    out_precision: 0,
                    clip_precision: 0,
                    quality: 0,
                    pitch_and_family: 0,
                    face,
                },
            );
        }
    }
    needed as i32
}

pub extern "win64" fn set_text_color(hdc: usize, color: u32) -> u32 {
    gui::set_dc_text_color(hdc, color)
}

pub extern "win64" fn set_bk_color(hdc: usize, color: u32) -> u32 {
    gui::set_dc_bk_color(hdc, color)
}

pub extern "win64" fn set_bk_mode(hdc: usize, mode: u32) -> u32 {
    gui::set_dc_bk_mode(hdc, mode)
}

/// Remplit un rectangle par le pinceau courant ou une couleur constante.
pub unsafe extern "win64" fn pat_blt(
    hdc: usize,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    rop: u32,
) -> i32 {
    if gui::dc_window(hdc).is_none() {
        crate::process::set_last_error(1400); // ERROR_INVALID_HANDLE
        return 0;
    }
    if width <= 0 || height <= 0 {
        crate::process::set_last_error(87); // ERROR_INVALID_PARAMETER
        return 0;
    }
    let (color, operation) = match rop {
        0x00F0_0021 => (gui::object_color(gui::dc_brush(hdc)), gui::RasterOp::Copy),
        0x0000_0042 => (Some(0), gui::RasterOp::Clear),
        0x00FF_0062 => (Some(0x00FF_FFFF), gui::RasterOp::Set),
        0x005A_0049 => (gui::object_color(gui::dc_brush(hdc)), gui::RasterOp::Xor), // PATINVERT
        0x0055_0009 => (Some(0), gui::RasterOp::Invert),                            // DSTINVERT
        0x00A0_00C9 => (gui::object_color(gui::dc_brush(hdc)), gui::RasterOp::And), // PATAND
        0x00FA_0089 => (gui::object_color(gui::dc_brush(hdc)), gui::RasterOp::Or),  // PATOR
        _ => {
            crate::process::set_last_error(87);
            return 0;
        }
    };
    if let Some(color) = color {
        gui::record(
            hdc,
            DrawCmd::PatternRect {
                rect: Rect::from_size(x, y, width, height),
                color,
                rop: operation,
            },
        );
    }
    1
}

/// Copie une aire entre deux DC de fenêtre. ROP pris en charge : SRCCOPY.
pub extern "win64" fn bit_blt(
    dst_hdc: usize,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    src_hdc: usize,
    src_x: i32,
    src_y: i32,
    rop: u32,
) -> i32 {
    blit_common(
        dst_hdc, x, y, width, height, src_hdc, src_x, src_y, width, height, rop, false,
    )
}

/// Copie/redimensionne une aire entre DC de fenêtre par rééchantillonnage X11.
pub extern "win64" fn stretch_blt(
    dst_hdc: usize,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    src_hdc: usize,
    src_x: i32,
    src_y: i32,
    src_width: i32,
    src_height: i32,
    rop: u32,
) -> i32 {
    blit_common(
        dst_hdc, x, y, width, height, src_hdc, src_x, src_y, src_width, src_height, rop, true,
    )
}

fn blit_common(
    dst_hdc: usize,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    src_hdc: usize,
    src_x: i32,
    src_y: i32,
    src_width: i32,
    src_height: i32,
    rop: u32,
    stretch: bool,
) -> i32 {
    let Some(operation) = bitblt_rop(rop) else {
        crate::process::set_last_error(87); // ERROR_INVALID_PARAMETER
        return 0;
    };
    if width <= 0 || height <= 0 || src_width <= 0 || src_height <= 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let Some(dst_hwnd) = gui::dc_window(dst_hdc) else {
        crate::process::set_last_error(1400);
        return 0;
    };
    let Some(src_hwnd) = gui::dc_window(src_hdc) else {
        crate::process::set_last_error(1400);
        return 0;
    };
    if (dst_hwnd != 0 && gui::wnd_proc_of(dst_hwnd).is_none())
        || (src_hwnd != 0 && gui::wnd_proc_of(src_hwnd).is_none())
    {
        crate::process::set_last_error(1400);
        return 0;
    }
    let Some(dst_right) = x.checked_add(width) else {
        crate::process::set_last_error(87);
        return 0;
    };
    let Some(dst_bottom) = y.checked_add(height) else {
        crate::process::set_last_error(87);
        return 0;
    };
    let Some(src_right) = src_x.checked_add(src_width) else {
        crate::process::set_last_error(87);
        return 0;
    };
    let Some(src_bottom) = src_y.checked_add(src_height) else {
        crate::process::set_last_error(87);
        return 0;
    };
    let source = Rect {
        left: src_x,
        top: src_y,
        right: src_right,
        bottom: src_bottom,
    };
    let destination = Rect {
        left: x,
        top: y,
        right: dst_right,
        bottom: dst_bottom,
    };
    let cmd = if src_hwnd != 0 && dst_hwnd != 0 {
        DrawCmd::Blit {
            source_hwnd: src_hwnd,
            source,
            destination,
            stretch,
            rop: operation,
        }
    } else {
        let snapshot = if src_hwnd == 0 {
            gui::bitmap_snapshot(src_hdc, source)
        } else {
            gui::capture_window(src_hwnd, source)
        };
        let Some(snapshot) = snapshot else {
            crate::process::set_last_error(1400); // source bitmap/window unavailable
            return 0;
        };
        let Some(pixels) = resize_bitmap(snapshot, width as usize, height as usize) else {
            crate::process::set_last_error(8); // ERROR_NOT_ENOUGH_MEMORY
            return 0;
        };
        DrawCmd::Bitmap {
            rect: destination,
            pixels,
            rop: operation,
        }
    };
    if gui::record(dst_hdc, cmd) {
        1
    } else {
        0
    }
}

fn bitblt_rop(rop: u32) -> Option<gui::RasterOp> {
    Some(match rop {
        0x00CC_0020 => gui::RasterOp::Copy,         // SRCCOPY
        0x00EE_0086 => gui::RasterOp::Or,           // SRCPAINT
        0x0088_00C6 => gui::RasterOp::And,          // SRCAND
        0x0066_0046 => gui::RasterOp::Xor,          // SRCINVERT
        0x0033_0008 => gui::RasterOp::CopyInverted, // NOTSRCCOPY
        0x0055_0009 => gui::RasterOp::Invert,       // DSTINVERT
        0x0000_0042 => gui::RasterOp::Clear,        // BLACKNESS
        0x00FF_0062 => gui::RasterOp::Set,          // WHITENESS
        _ => return None,
    })
}

struct ResizeCache {
    key: (usize, usize, usize, usize),
    source: Vec<u32>,
    output: Vec<u32>,
}
static RESIZE_CACHE: Mutex<Option<ResizeCache>> = Mutex::new(None);

#[cfg(test)]
mod scaling_tests {
    use super::*;
    #[test]
    fn agrandissement_lisse_et_bitmap_4k() {
        let image = gui::BitmapSnapshot {
            width: 2,
            height: 1,
            pixels: vec![0, 0x00ff_ffff],
        };
        let output = resize_bitmap(image, 3, 1).unwrap();
        assert_eq!(output, [0, 0x0080_8080, 0x00ff_ffff]);
        let image = gui::BitmapSnapshot {
            width: 1,
            height: 1,
            pixels: vec![0x0012_3456],
        };
        let output = resize_bitmap(image, 3840, 2160).unwrap();
        assert_eq!(output.len(), 3840 * 2160);
        assert_eq!(output[0], 0x0012_3456);
        assert_eq!(output[output.len() - 1], 0x0012_3456);
    }
}

fn resize_bitmap(snapshot: gui::BitmapSnapshot, width: usize, height: usize) -> Option<Vec<u32>> {
    let count = width.checked_mul(height)?;
    if width == 0
        || height == 0
        || count > gui::MAX_BITMAP_PIXELS
        || snapshot.width == 0
        || snapshot.height == 0
    {
        return None;
    }
    if snapshot.pixels.len() < snapshot.width.checked_mul(snapshot.height)? {
        return None;
    }
    if snapshot.width == width && snapshot.height == height {
        return Some(snapshot.pixels);
    }
    let key = (snapshot.width, snapshot.height, width, height);
    let cacheable = count >= 65536;
    if cacheable {
        if let Some(cached) = RESIZE_CACHE
            .lock()
            .ok()?
            .as_ref()
            .filter(|c| c.key == key && c.source == snapshot.pixels)
        {
            return Some(cached.output.clone());
        }
    }
    let mut output = vec![0; count];
    for y in 0..height {
        let sy = (y as f32 + 0.5) * snapshot.height as f32 / height as f32 - 0.5;
        for x in 0..width {
            let sx = (x as f32 + 0.5) * snapshot.width as f32 / width as f32 - 0.5;
            output[y * width + x] =
                gui::bilinear_colorref(&snapshot.pixels, snapshot.width, snapshot.height, sx, sy);
        }
    }
    if cacheable {
        *RESIZE_CACHE.lock().ok()? = Some(ResizeCache {
            key,
            source: snapshot.pixels,
            output: output.clone(),
        });
    }
    Some(output)
}

pub extern "win64" fn create_compatible_dc(_dc: usize) -> usize {
    // Les opérations de dessin écrivent dans la bitmap sélectionnée.
    gui::create_dc(0)
}

pub extern "win64" fn create_compatible_bitmap(dc: usize, width: i32, height: i32) -> usize {
    if gui::dc_window(dc).is_none() {
        crate::process::set_last_error(1400); // ERROR_INVALID_HANDLE
        return 0;
    }
    if width <= 0 || height <= 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    match gui::create_compatible_bitmap(width, height) {
        Some(bitmap) => bitmap,
        None => {
            crate::process::set_last_error(8); // ERROR_NOT_ENOUGH_MEMORY
            0
        }
    }
}

/// Crée une DIB section 24/32bpp BI_RGB; le tampon exposé utilise des pixels BGR(X).
pub unsafe extern "win64" fn create_dib_section(
    dc: usize,
    info: *const BitmapInfoHeader,
    usage: u32,
    bits: *mut *mut c_void,
    section: usize,
    offset: u32,
) -> usize {
    if gui::dc_window(dc).is_none() {
        crate::process::set_last_error(1400);
        return 0;
    }
    if info.is_null() || bits.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let header = std::ptr::read_unaligned(info);
    if header.size < std::mem::size_of::<BitmapInfoHeader>() as u32
        || header.width <= 0
        || header.height == 0
        || header.height == i32::MIN
        || header.planes != 1
        || !matches!(header.bit_count, 24 | 32)
        || header.compression != 0 // BI_RGB
        || usage != 0 // DIB_RGB_COLORS
        || section != 0
        || offset != 0
    {
        crate::process::set_last_error(87);
        return 0;
    }
    let Some((bitmap, pixel_bits)) = gui::create_dib_section(
        header.width,
        header.height.abs(),
        header.bit_count,
        header.height < 0,
    ) else {
        crate::process::set_last_error(8); // ERROR_NOT_ENOUGH_MEMORY
        return 0;
    };
    std::ptr::write_unaligned(bits, pixel_bits.cast::<c_void>());
    bitmap
}

pub extern "win64" fn delete_dc(dc: usize) -> i32 {
    if gui::dc_window(dc).is_some() {
        gui::free_dc(dc);
        1
    } else {
        0
    }
}

pub extern "win64" fn get_device_caps(_dc: usize, index: i32) -> i32 {
    match index {
        8 => 1920,  // HORZRES
        10 => 1080, // VERTRES
        12 => 24,   // BITSPIXEL
        90 => 96,   // LOGPIXELSX
        88 => 96,   // LOGPIXELSY
        _ => 0,
    }
}

/// `TextOutA(hdc, x, y, texte, count)` : enregistre le texte au coin haut-gauche.
///
/// # Sécurité
/// `text` doit pointer sur `count` octets lisibles.
pub unsafe extern "win64" fn text_out_a(
    hdc: usize,
    x: i32,
    y: i32,
    text: *const u8,
    count: i32,
) -> i32 {
    if text.is_null() || gui::dc_window(hdc).is_none() {
        crate::process::set_last_error(87); // ERROR_INVALID_PARAMETER
        return 0;
    }
    let s = if count < 0 {
        crate::dlls::cstr(text)
    } else {
        crate::dlls::decode_ansi(std::slice::from_raw_parts(text, count as usize))
    };
    let color = gui::dc_text_color(hdc);
    // Fond opaque (mode 2, valeur par défaut Win32) : la couleur du fond est
    // dessinée derrière les glyphes ; mode transparent : aucun fond.
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

/// `SIZE` Win32.
#[repr(C)]
pub struct Size {
    cx: i32,
    cy: i32,
}

unsafe fn text_extent(hdc: usize, s: &str, size: *mut Size) -> i32 {
    if size.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let font = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
    let (cx, cy) = gui::measure_text(&font, s);
    std::ptr::write_unaligned(size, Size { cx, cy });
    1
}

/// `GetTextExtentPoint32A(hdc, text, count, size)` : taille du texte dans la
/// police sélectionnée du DC (fenêtre ou mémoire).
pub unsafe extern "win64" fn get_text_extent_point32_a(
    hdc: usize,
    text: *const u8,
    count: i32,
    size: *mut Size,
) -> i32 {
    if text.is_null() || count < 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let s = crate::dlls::decode_ansi(std::slice::from_raw_parts(text, count as usize));
    text_extent(hdc, &s, size)
}

pub unsafe extern "win64" fn get_text_extent_point32_w(
    hdc: usize,
    text: *const u16,
    count: i32,
    size: *mut Size,
) -> i32 {
    if text.is_null() || count < 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let s = String::from_utf16_lossy(std::slice::from_raw_parts(text, count as usize));
    text_extent(hdc, &s, size)
}

type IconCache = HashMap<(String, usize, u32), Vec<u32>>;
static ICON_CACHE: OnceLock<Mutex<IconCache>> = OnceLock::new();

fn desktop_icon_name(request: &str) -> Option<String> {
    if !request.ends_with(".desktop") {
        return Some(request.to_string());
    }
    let mut candidates = Vec::new();
    if Path::new(request).is_absolute() {
        candidates.push(PathBuf::from(request));
    } else {
        if let Ok(home) = std::env::var("HOME") {
            candidates.push(
                PathBuf::from(home)
                    .join(".local/share/applications")
                    .join(request),
            );
        }
        candidates.push(PathBuf::from("/usr/local/share/applications").join(request));
        candidates.push(PathBuf::from("/usr/share/applications").join(request));
    }
    for path in candidates {
        let Ok(file) = File::open(path) else { continue };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if let Some(icon) = line.strip_prefix("Icon=") {
                let icon = icon.trim();
                if !icon.is_empty() {
                    return Some(icon.to_string());
                }
            }
        }
    }
    None
}

fn resolve_icon_path(request: &str) -> Option<PathBuf> {
    let icon = desktop_icon_name(request)?;
    let direct = PathBuf::from(&icon);
    if direct.is_file()
        && direct
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
    {
        return Some(direct);
    }
    let icon = icon.strip_suffix(".png").unwrap_or(&icon);
    let themes = [
        "Mint-L",
        "Mint-Y",
        "Mint-X",
        "hicolor",
        "HighContrast",
        "Yaru",
    ];
    let sections = [
        "apps",
        "places",
        "categories",
        "status",
        "devices",
        "mimetypes",
        "legacy",
    ];
    let sizes = [
        "512", "256@2x", "256x256", "256", "128@2x", "128x128", "128", "96x96", "64@2x", "64x64",
        "64", "48x48", "48", "32x32", "32",
    ];
    for theme in themes {
        for size in sizes {
            for section in sections {
                for candidate in [
                    PathBuf::from("/usr/share/icons")
                        .join(theme)
                        .join(section)
                        .join(size)
                        .join(format!("{icon}.png")),
                    PathBuf::from("/usr/share/icons")
                        .join(theme)
                        .join(size)
                        .join(section)
                        .join(format!("{icon}.png")),
                ] {
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
    }
    let pixmap = PathBuf::from("/usr/share/pixmaps").join(format!("{icon}.png"));
    pixmap.is_file().then_some(pixmap)
}

fn decode_png(path: &Path) -> Option<(usize, usize, Vec<[u8; 4]>)> {
    let file = File::open(path).ok()?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let info = reader.info();
    if info.width == 0 || info.height == 0 || info.width > 4096 || info.height > 4096 {
        return None;
    }
    let output_size = reader.output_buffer_size()?;
    if output_size > 64 * 1024 * 1024 {
        return None;
    }
    let mut buffer = vec![0; output_size];
    let info = reader.next_frame(&mut buffer).ok()?;
    let bytes = &buffer[..info.buffer_size()];
    let count = (info.width as usize).checked_mul(info.height as usize)?;
    let mut rgba = Vec::with_capacity(count);
    match info.color_type {
        png::ColorType::Rgba => rgba.extend(
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| [p[0], p[1], p[2], p[3]]),
        ),
        png::ColorType::Rgb => rgba.extend(
            bytes
                .as_chunks::<3>()
                .0
                .iter()
                .map(|p| [p[0], p[1], p[2], 255]),
        ),
        png::ColorType::GrayscaleAlpha => rgba.extend(
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|p| [p[0], p[0], p[0], p[1]]),
        ),
        png::ColorType::Grayscale => rgba.extend(bytes.iter().map(|p| [*p, *p, *p, 255])),
        png::ColorType::Indexed => return None,
    }
    (rgba.len() == count).then_some((info.width as usize, info.height as usize, rgba))
}

fn icon_pixels(request: &str, size: usize, background: u32) -> Option<Vec<u32>> {
    let key = (request.to_string(), size, background);
    let cache = ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(pixels) = cache.lock().ok()?.get(&key).cloned() {
        return Some(pixels);
    }
    let (width, height, source) = decode_png(&resolve_icon_path(request)?)?;
    let scale = (size as f32 / width as f32).min(size as f32 / height as f32);
    let fitted_width = ((width as f32 * scale).round() as usize).clamp(1, size);
    let fitted_height = ((height as f32 * scale).round() as usize).clamp(1, size);
    let offset_x = (size - fitted_width) / 2;
    let offset_y = (size - fitted_height) / 2;
    let bg = [
        (background & 0xff) as u8,
        ((background >> 8) & 0xff) as u8,
        ((background >> 16) & 0xff) as u8,
    ];
    let mut output = vec![background; size * size];
    for y in 0..fitted_height {
        let sy = ((y as f32 + 0.5) * height as f32 / fitted_height as f32 - 0.5)
            .clamp(0.0, height.saturating_sub(1) as f32);
        let y0 = sy.floor() as usize;
        let y1 = (y0 + 1).min(height - 1);
        let fy = sy - y0 as f32;
        for x in 0..fitted_width {
            let sx = ((x as f32 + 0.5) * width as f32 / fitted_width as f32 - 0.5)
                .clamp(0.0, width.saturating_sub(1) as f32);
            let x0 = sx.floor() as usize;
            let x1 = (x0 + 1).min(width - 1);
            let fx = sx - x0 as f32;
            let sample = |channel: usize| {
                let top = source[y0 * width + x0][channel] as f32 * (1.0 - fx)
                    + source[y0 * width + x1][channel] as f32 * fx;
                let bottom = source[y1 * width + x0][channel] as f32 * (1.0 - fx)
                    + source[y1 * width + x1][channel] as f32 * fx;
                top * (1.0 - fy) + bottom * fy
            };
            let alpha = sample(3) / 255.0;
            let channel = |index: usize| {
                (sample(index) * alpha + bg[index] as f32 * (1.0 - alpha)).round() as u32
            };
            output[(offset_y + y) * size + offset_x + x] =
                channel(0) | (channel(1) << 8) | (channel(2) << 16);
        }
    }
    cache.lock().ok()?.insert(key, output.clone());
    Some(output)
}

/// Extension Winlux : charge l'icône PNG réelle déclarée par un fichier
/// `.desktop`, la redimensionne avec filtrage et la dessine via le chemin GDI.
pub unsafe extern "win64" fn winlux_draw_icon_a(
    hdc: usize,
    request: *const u8,
    x: i32,
    y: i32,
    size: i32,
    background: u32,
) -> i32 {
    if request.is_null() || gui::dc_window(hdc).is_none() || !(1..=512).contains(&size) {
        crate::process::set_last_error(87);
        return 0;
    }
    let request = crate::dlls::cstr(request);
    let Some(pixels) = icon_pixels(&request, size as usize, background) else {
        return 0;
    };
    let rect = Rect {
        left: x,
        top: y,
        right: x + size,
        bottom: y + size,
    };
    gui::record(
        hdc,
        DrawCmd::Bitmap {
            rect,
            pixels,
            rop: gui::RasterOp::Copy,
        },
    ) as i32
}

/// `Rectangle(hdc, left, top, right, bottom, pen, brush)` : cadre + intérieur.
pub unsafe extern "win64" fn rectangle(
    hdc: usize,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    pen: usize,
    brush: usize,
) -> i32 {
    if gui::dc_window(hdc).is_none() {
        crate::process::set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
        return 0;
    }
    let rect = Rect {
        left,
        top,
        right,
        bottom,
    };
    if let Some(color) = gui::object_color(brush) {
        let inner = Rect {
            left: left + 1,
            top: top + 1,
            right: right - 1,
            bottom: bottom - 1,
        };
        if inner.width() > 0 && inner.height() > 0 {
            gui::record(hdc, DrawCmd::FillRect { rect: inner, color });
        }
    }
    if let Some(color) = gui::object_color(pen) {
        let width = gui::object_pen_width(pen);
        gui::record(hdc, DrawCmd::FrameRect { rect, color, width });
    }
    1
}

/// `Ellipse(hdc, left, top, right, bottom)` : ellipse pleine (pinceau).
pub unsafe extern "win64" fn ellipse(
    hdc: usize,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
) -> i32 {
    if gui::dc_window(hdc).is_none() {
        crate::process::set_last_error(1400);
        return 0;
    }
    let rect = Rect {
        left,
        top,
        right,
        bottom,
    };
    let brush = gui::dc_brush(hdc);
    if let Some(color) = gui::object_color(brush) {
        gui::record(hdc, DrawCmd::Ellipse { rect, color });
    }
    1
}

// =====================================================================
// TextOutW (manquant)
// =====================================================================

pub unsafe extern "win64" fn text_out_w(
    hdc: usize,
    x: i32,
    y: i32,
    text: *const u16,
    count: i32,
) -> i32 {
    if text.is_null() || gui::dc_window(hdc).is_none() {
        crate::process::set_last_error(87);
        return 0;
    }
    let len = if count < 0 {
        let mut p = text;
        let mut n = 0;
        while *p != 0 {
            p = p.add(1);
            n += 1;
        }
        n
    } else {
        count as usize
    };
    let chars = std::slice::from_raw_parts(text, len);
    let s = String::from_utf16_lossy(chars);
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

// =====================================================================
// ExtTextOutA/W
// =====================================================================

pub unsafe extern "win64" fn ext_text_out_a(
    hdc: usize,
    x: i32,
    y: i32,
    _options: u32,
    _clip: *const Rect,
    text: *const u8,
    count: u32,
    _dx: *const i32,
) -> i32 {
    if count == 0 || text.is_null() {
        return 1; // Rien à dessiner, mais succès
    }
    text_out_a(hdc, x, y, text, count as i32)
}

pub unsafe extern "win64" fn ext_text_out_w(
    hdc: usize,
    x: i32,
    y: i32,
    _options: u32,
    _clip: *const Rect,
    text: *const u16,
    count: u32,
    _dx: *const i32,
) -> i32 {
    if count == 0 || text.is_null() {
        return 1;
    }
    text_out_w(hdc, x, y, text, count as i32)
}

// =====================================================================
// DrawTextA/W (fonctions complètes dans gdi32, contrairement à user32)
// =====================================================================

pub unsafe extern "win64" fn draw_text_a(
    hdc: usize,
    text: *const u8,
    count: i32,
    rect: *mut Rect,
    format: u32,
) -> i32 {
    if rect.is_null() {
        return 0;
    }
    let len = if count == -1 {
        let mut p = text;
        let mut n = 0;
        while !p.is_null() && *p != 0 {
            p = p.add(1);
            n += 1;
        }
        n
    } else {
        count as usize
    };
    let bytes = std::slice::from_raw_parts(text, len);
    let s = crate::dlls::decode_ansi(bytes);
    draw_text_gdi(hdc, &s, &mut *rect, format)
}

pub unsafe extern "win64" fn draw_text_w(
    hdc: usize,
    text: *const u16,
    count: i32,
    rect: *mut Rect,
    format: u32,
) -> i32 {
    if rect.is_null() {
        return 0;
    }
    let len = if count == -1 {
        let mut p = text;
        let mut n = 0;
        while !p.is_null() && *p != 0 {
            p = p.add(1);
            n += 1;
        }
        n
    } else {
        count as usize
    };
    let chars: Vec<u16> = std::slice::from_raw_parts(text, len).to_vec();
    let s = String::from_utf16_lossy(&chars);
    draw_text_gdi(hdc, &s, &mut *rect, format)
}

fn draw_text_gdi(hdc: usize, text: &str, rect: &mut Rect, format: u32) -> i32 {
    const DT_CALCRECT: u32 = 0x0400;
    const DT_CENTER: u32 = 0x0001;
    const DT_VCENTER: u32 = 0x0004;
    const DT_SINGLELINE: u32 = 0x0020;

    let font = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
    let (tw, th) = gui::measure_text(&font, text);

    if format & DT_CALCRECT != 0 {
        rect.right = rect.left + tw;
        rect.bottom = rect.top + th;
        return th;
    }

    let mut x = rect.left;
    let mut y = rect.top;
    if format & DT_CENTER != 0 {
        x = rect.left + (rect.width() - tw) / 2;
    }
    if format & DT_SINGLELINE != 0 && format & DT_VCENTER != 0 {
        y = rect.top + (rect.height() - th) / 2;
    }

    let color = gui::dc_text_color(hdc);
    let bk = if gui::dc_bk_mode(hdc) == 1 {
        None
    } else {
        Some(gui::dc_bk_color(hdc))
    };
    gui::record(
        hdc,
        DrawCmd::Text {
            x,
            y,
            text: text.to_string(),
            color,
            bk,
            font,
        },
    );
    th
}

// =====================================================================
// Line drawing : MoveToEx, LineTo, Polyline, Polygon
// =====================================================================

// Position courante du « crayon » par DC (approximation thread-local)
fn pen_pos() -> &'static Mutex<HashMap<usize, (i32, i32)>> {
    static P: OnceLock<Mutex<HashMap<usize, (i32, i32)>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_pen() -> std::sync::MutexGuard<'static, HashMap<usize, (i32, i32)>> {
    pen_pos()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub unsafe extern "win64" fn move_to_ex(hdc: usize, x: i32, y: i32, prev: *mut gui::Point) -> i32 {
    let mut map = lock_pen();
    let old = map.insert(hdc, (x, y)).unwrap_or((0, 0));
    if !prev.is_null() {
        (*prev).x = old.0;
        (*prev).y = old.1;
    }
    1
}

pub unsafe extern "win64" fn line_to(hdc: usize, x: i32, y: i32) -> i32 {
    let (x0, y0) = {
        let mut map = lock_pen();
        let pos = map.entry(hdc).or_insert((0, 0));
        let old = *pos;
        *pos = (x, y);
        old
    };
    let pen = gui::dc_pen(hdc);
    let color = gui::object_color(pen).unwrap_or(0);
    gui::record(
        hdc,
        DrawCmd::Line {
            x1: x0,
            y1: y0,
            x2: x,
            y2: y,
            color,
            width: gui::object_pen_width(pen),
        },
    );
    1
}

pub unsafe extern "win64" fn polyline(hdc: usize, points: *const gui::Point, count: i32) -> i32 {
    if points.is_null() || count < 2 {
        return 0;
    }
    let pts = std::slice::from_raw_parts(points, count as usize);
    let color = gui::object_color(gui::dc_brush(hdc)).unwrap_or(0);
    for w in pts.windows(2) {
        let rect = Rect {
            left: w[0].x.min(w[1].x),
            top: w[0].y.min(w[1].y),
            right: w[0].x.max(w[1].x) + 1,
            bottom: w[0].y.max(w[1].y) + 1,
        };
        gui::record(
            hdc,
            DrawCmd::FrameRect {
                rect,
                color,
                width: 1,
            },
        );
    }
    1
}

pub unsafe extern "win64" fn polygon(hdc: usize, points: *const gui::Point, count: i32) -> i32 {
    if points.is_null() || count < 2 {
        return 0;
    }
    // Dessine les arêtes comme un polyline fermé
    polyline(hdc, points, count);
    // Fermer le polygone
    let pts = std::slice::from_raw_parts(points, count as usize);
    let first = pts[0];
    let last = pts[count as usize - 1];
    let color = gui::object_color(gui::dc_brush(hdc)).unwrap_or(0);
    let rect = Rect {
        left: first.x.min(last.x),
        top: first.y.min(last.y),
        right: first.x.max(last.x) + 1,
        bottom: first.y.max(last.y) + 1,
    };
    gui::record(
        hdc,
        DrawCmd::FrameRect {
            rect,
            color,
            width: 1,
        },
    );
    1
}

pub unsafe extern "win64" fn polyline_to(hdc: usize, points: *const gui::Point, count: u32) -> i32 {
    let _ = (hdc, points, count);
    1
}

pub unsafe extern "win64" fn poly_bezier(hdc: usize, points: *const gui::Point, count: u32) -> i32 {
    let _ = (hdc, points, count);
    1
}

pub unsafe extern "win64" fn poly_bezier_to(
    hdc: usize,
    points: *const gui::Point,
    count: u32,
) -> i32 {
    let _ = (hdc, points, count);
    1
}

// =====================================================================
// RoundRect
// =====================================================================

pub unsafe extern "win64" fn round_rect(
    hdc: usize,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    _rx: i32,
    _ry: i32,
) -> i32 {
    // On dessine un rectangle simple (sans arrondi) comme approximation
    // Utiliser le pen et brush courants du DC
    let pen = gui::dc_pen(hdc);
    let brush = gui::dc_brush(hdc);
    rectangle(hdc, left, top, right, bottom, pen, brush)
}

// =====================================================================
// SetPixel / GetPixel
// =====================================================================

pub unsafe extern "win64" fn set_pixel(hdc: usize, x: i32, y: i32, color: u32) -> u32 {
    let rect = Rect {
        left: x,
        top: y,
        right: x + 1,
        bottom: y + 1,
    };
    gui::record(hdc, DrawCmd::FillRect { rect, color });
    color
}

pub unsafe extern "win64" fn get_pixel(_hdc: usize, _x: i32, _y: i32) -> u32 {
    // Pas d'accès en lecture aux pixels pour l'instant
    0x00FF_FFFF // CLR_INVALID
}

pub unsafe extern "win64" fn set_pixel_v(hdc: usize, x: i32, y: i32, color: u32) -> i32 {
    set_pixel(hdc, x, y, color);
    1
}

// =====================================================================
// Region functions
// =====================================================================

static NEXT_REGION: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0x0020_0000);

pub extern "win64" fn create_rect_rgn(_left: i32, _top: i32, _right: i32, _bottom: i32) -> usize {
    NEXT_REGION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub extern "win64" fn create_rect_rgn_indirect(_rect: *const Rect) -> usize {
    NEXT_REGION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub extern "win64" fn create_elliptic_rgn(
    _left: i32,
    _top: i32,
    _right: i32,
    _bottom: i32,
) -> usize {
    NEXT_REGION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub extern "win64" fn combine_rgn(_dest: usize, _src1: usize, _src2: usize, _mode: i32) -> i32 {
    1 // SIMPLEREGION
}

pub extern "win64" fn set_rect_rgn(
    _rgn: usize,
    _left: i32,
    _top: i32,
    _right: i32,
    _bottom: i32,
) -> i32 {
    1
}

#[allow(dead_code)]
pub extern "win64" fn delete_rgn(rgn: usize) -> i32 {
    let _ = rgn;
    1
}

pub extern "win64" fn offset_rgn(_rgn: usize, _x: i32, _y: i32) -> i32 {
    1
}

pub unsafe extern "win64" fn get_rgn_box(_rgn: usize, rect: *mut Rect) -> i32 {
    if !rect.is_null() {
        *rect = Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
    }
    1
}

pub extern "win64" fn pt_in_region(_rgn: usize, _x: i32, _y: i32) -> i32 {
    0
}

pub extern "win64" fn rect_in_region(_rgn: usize, _rect: *const Rect) -> i32 {
    0
}

pub extern "win64" fn equal_rgn(_rgn1: usize, _rgn2: usize) -> i32 {
    0
}

// =====================================================================
// Clipping
// =====================================================================

pub extern "win64" fn select_clip_rgn(_hdc: usize, _rgn: usize) -> i32 {
    1 // SIMPLEREGION
}

pub extern "win64" fn ext_select_clip_rgn(_hdc: usize, _rgn: usize, _mode: i32) -> i32 {
    1
}

pub extern "win64" fn intersect_clip_rect(
    _hdc: usize,
    _left: i32,
    _top: i32,
    _right: i32,
    _bottom: i32,
) -> i32 {
    1
}

pub extern "win64" fn exclude_clip_rect(
    _hdc: usize,
    _left: i32,
    _top: i32,
    _right: i32,
    _bottom: i32,
) -> i32 {
    1
}

pub unsafe extern "win64" fn get_clip_box(_hdc: usize, rect: *mut Rect) -> i32 {
    if !rect.is_null() {
        let (w, h) = gui::screen_size();
        *rect = Rect {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
    }
    1
}

pub extern "win64" fn get_clip_rgn(_hdc: usize, _rgn: usize) -> i32 {
    0 // Pas de clipping region
}

// =====================================================================
// DC state : SaveDC / RestoreDC
// =====================================================================

pub extern "win64" fn save_dc(hdc: usize) -> i32 {
    gui::save_dc(hdc)
}

pub extern "win64" fn restore_dc(hdc: usize, saved_dc: i32) -> i32 {
    gui::restore_dc(hdc, saved_dc) as i32
}

// =====================================================================
// Map mode / viewport / window origin
// =====================================================================

pub extern "win64" fn get_map_mode(hdc: usize) -> i32 {
    gui::dc_map_mode(hdc)
}

pub extern "win64" fn set_map_mode(hdc: usize, mode: i32) -> i32 {
    gui::set_dc_map_mode(hdc, mode)
}

pub unsafe extern "win64" fn set_viewport_org_ex(
    _hdc: usize,
    _x: i32,
    _y: i32,
    prev: *mut gui::Point,
) -> i32 {
    if !prev.is_null() {
        (*prev).x = 0;
        (*prev).y = 0;
    }
    1
}

pub unsafe extern "win64" fn get_viewport_org_ex(_hdc: usize, pt: *mut gui::Point) -> i32 {
    if !pt.is_null() {
        (*pt).x = 0;
        (*pt).y = 0;
    }
    1
}

pub unsafe extern "win64" fn set_window_org_ex(
    _hdc: usize,
    _x: i32,
    _y: i32,
    prev: *mut gui::Point,
) -> i32 {
    if !prev.is_null() {
        (*prev).x = 0;
        (*prev).y = 0;
    }
    1
}

pub unsafe extern "win64" fn get_window_org_ex(_hdc: usize, pt: *mut gui::Point) -> i32 {
    if !pt.is_null() {
        (*pt).x = 0;
        (*pt).y = 0;
    }
    1
}

pub unsafe extern "win64" fn set_viewport_ext_ex(
    _hdc: usize,
    _x: i32,
    _y: i32,
    prev: *mut Size,
) -> i32 {
    if !prev.is_null() {
        (*prev).cx = 1;
        (*prev).cy = 1;
    }
    1
}

pub unsafe extern "win64" fn get_viewport_ext_ex(_hdc: usize, sz: *mut Size) -> i32 {
    if !sz.is_null() {
        (*sz).cx = 1;
        (*sz).cy = 1;
    }
    1
}

pub unsafe extern "win64" fn set_window_ext_ex(
    _hdc: usize,
    _x: i32,
    _y: i32,
    prev: *mut Size,
) -> i32 {
    if !prev.is_null() {
        (*prev).cx = 1;
        (*prev).cy = 1;
    }
    1
}

pub unsafe extern "win64" fn get_window_ext_ex(_hdc: usize, sz: *mut Size) -> i32 {
    if !sz.is_null() {
        (*sz).cx = 1;
        (*sz).cy = 1;
    }
    1
}

pub unsafe extern "win64" fn dp_to_lp(_hdc: usize, _points: *mut gui::Point, _count: i32) -> i32 {
    1 // En mode MM_TEXT, pas de transformation
}

pub unsafe extern "win64" fn lp_to_dp(_hdc: usize, _points: *mut gui::Point, _count: i32) -> i32 {
    1
}

// =====================================================================
// Font : CreateFontIndirectA/W
// =====================================================================

#[repr(C)]
#[derive(Clone)]
struct LogFontFields {
    height: i32,
    width: i32,
    escapement: i32,
    orientation: i32,
    weight: i32,
    italic: u8,
    underline: u8,
    strikeout: u8,
    charset: u8,
    out_precision: u8,
    clip_precision: u8,
    quality: u8,
    pitch_and_family: u8,
    // face_name : 32 chars (A) or 32 wchars (W) follows
}

pub unsafe extern "win64" fn create_font_indirect_a(lf: *const c_void) -> usize {
    if lf.is_null() {
        return 0;
    }
    let f = &*(lf as *const LogFontFields);
    let face_ptr = (lf as *const u8).add(std::mem::size_of::<LogFontFields>());
    let mut face_bytes = Vec::new();
    for i in 0..32 {
        let b = *face_ptr.add(i);
        if b == 0 {
            break;
        }
        face_bytes.push(b);
    }
    let face = crate::dlls::decode_ansi(&face_bytes);
    let spec = FontSpec {
        height: f.height.abs(),
        width: f.width,
        weight: f.weight,
        italic: f.italic != 0,
        underline: f.underline != 0,
        strikeout: f.strikeout != 0,
        face,
    };
    gui::create_font(spec)
}

pub unsafe extern "win64" fn create_font_indirect_w(lf: *const c_void) -> usize {
    if lf.is_null() {
        return 0;
    }
    let f = &*(lf as *const LogFontFields);
    let face_ptr = (lf as *const u8).add(std::mem::size_of::<LogFontFields>()) as *const u16;
    let mut face_chars = Vec::new();
    for i in 0..32 {
        let c = *face_ptr.add(i);
        if c == 0 {
            break;
        }
        face_chars.push(c);
    }
    let face = String::from_utf16_lossy(&face_chars);
    let spec = FontSpec {
        height: f.height.abs(),
        width: f.width,
        weight: f.weight,
        italic: f.italic != 0,
        underline: f.underline != 0,
        strikeout: f.strikeout != 0,
        face,
    };
    gui::create_font(spec)
}

// =====================================================================
// GetTextMetrics A/W
// =====================================================================

/// TEXTMETRICA minimal (56 bytes en Win32, 60 en Win64 — on écrit les champs
/// utiles aux offsets corrects).
#[repr(C)]
struct TextMetric {
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
    charset: u8,
    _pad: [u8; 3],
}

pub unsafe extern "win64" fn get_text_metrics_a(hdc: usize, tm: *mut c_void) -> i32 {
    if tm.is_null() {
        return 0;
    }
    let font = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
    let h = if font.height > 0 { font.height } else { 13 };
    let out = &mut *(tm as *mut TextMetric);
    out.height = h;
    out.ascent = (h * 80) / 100;
    out.descent = h - out.ascent;
    out.internal_leading = 0;
    out.external_leading = 0;
    out.ave_char_width = (h * 45) / 100;
    out.max_char_width = h;
    out.weight = font.weight;
    out.overhang = 0;
    out.digitized_aspect_x = 96;
    out.digitized_aspect_y = 96;
    out.first_char = 0x20;
    out.last_char = 0xFF;
    out.default_char = b'?';
    out.break_char = b' ';
    out.italic = font.italic as u8;
    out.underlined = font.underline as u8;
    out.struck_out = font.strikeout as u8;
    out.pitch_and_family = 0x22; // VARIABLE_PITCH | FF_SWISS
    out.charset = 0; // ANSI_CHARSET
    1
}

pub unsafe extern "win64" fn get_text_metrics_w(hdc: usize, tm: *mut c_void) -> i32 {
    // La structure TEXTMETRICW a les mêmes champs avant les chars, qui passent de
    // u8 à u16 — mais en pratique les offsets sont proches. On réutilise la version A.
    get_text_metrics_a(hdc, tm)
}

// =====================================================================
// EnumFontFamiliesEx
// =====================================================================

pub unsafe extern "win64" fn enum_font_families_ex_a(
    _hdc: usize,
    _logfont: *const c_void,
    _callback: usize,
    _lparam: isize,
    _flags: u32,
) -> i32 {
    // Stub : ne rapporte aucune police
    1
}

pub unsafe extern "win64" fn enum_font_families_ex_w(
    _hdc: usize,
    _logfont: *const c_void,
    _callback: usize,
    _lparam: isize,
    _flags: u32,
) -> i32 {
    1
}

pub unsafe extern "win64" fn enum_fonts_a(
    _hdc: usize,
    _name: *const u8,
    _callback: usize,
    _lparam: isize,
) -> i32 {
    1
}

pub unsafe extern "win64" fn enum_fonts_w(
    _hdc: usize,
    _name: *const u16,
    _callback: usize,
    _lparam: isize,
) -> i32 {
    1
}

// =====================================================================
// GetTextFaceA/W
// =====================================================================

pub unsafe extern "win64" fn get_text_face_a(hdc: usize, count: i32, buf: *mut u8) -> i32 {
    let font = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
    if buf.is_null() {
        return font.face.len() as i32 + 1;
    }
    crate::dlls::put_a(&font.face, buf, count as u32) as i32
}

pub unsafe extern "win64" fn get_text_face_w(hdc: usize, count: i32, buf: *mut u16) -> i32 {
    let font = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
    if buf.is_null() {
        return font.face.len() as i32 + 1;
    }
    crate::dlls::put_w(&font.face, buf, count as u32) as i32
}

// =====================================================================
// Bitmap : GetDIBits, SetDIBitsToDevice, StretchDIBits, GetBitmapBits
// =====================================================================

pub unsafe extern "win64" fn get_dib_its(
    _hdc: usize,
    _bmp: usize,
    _start: u32,
    _lines: u32,
    _bits: *mut c_void,
    _info: *mut c_void,
    _usage: u32,
) -> i32 {
    0 // Stub
}

pub unsafe extern "win64" fn set_dib_its_to_device(
    _hdc: usize,
    _x: i32,
    _y: i32,
    _w: u32,
    _h: u32,
    _src_x: i32,
    _src_y: i32,
    _start: u32,
    _lines: u32,
    _bits: *const c_void,
    _info: *const c_void,
    _usage: u32,
) -> i32 {
    0
}

pub unsafe extern "win64" fn stretch_dib_its(
    _hdc: usize,
    _xd: i32,
    _yd: i32,
    _wd: i32,
    _hd: i32,
    _xs: i32,
    _ys: i32,
    _ws: i32,
    _hs: i32,
    _bits: *const c_void,
    _info: *const c_void,
    _usage: u32,
    _rop: u32,
) -> i32 {
    0
}

pub unsafe extern "win64" fn get_bitmap_bits(_bmp: usize, _count: i32, _bits: *mut c_void) -> i32 {
    0
}

pub unsafe extern "win64" fn set_bitmap_bits(
    _bmp: usize,
    _count: u32,
    _bits: *const c_void,
) -> i32 {
    0
}

// =====================================================================
// Misc GDI
// =====================================================================

pub extern "win64" fn get_current_object(hdc: usize, obj_type: u32) -> usize {
    match obj_type {
        1 => gui::dc_bitmap(hdc), // OBJ_PEN → bitmap (approx)
        2 => gui::dc_brush(hdc),  // OBJ_BRUSH
        6 => gui::dc_font(hdc),   // OBJ_FONT
        7 => gui::dc_bitmap(hdc), // OBJ_BITMAP
        _ => 0,
    }
}

pub extern "win64" fn get_object_type(obj: usize) -> u32 {
    if let Some(info) = gui::object_info(obj) {
        match info {
            gui::GdiObjectInfo::Brush { .. } => 2,
            gui::GdiObjectInfo::Pen { .. } => 1,
            gui::GdiObjectInfo::Font { .. } => 6,
            gui::GdiObjectInfo::Bitmap { .. } => 7,
        }
    } else {
        0
    }
}

#[allow(dead_code)]
pub extern "win64" fn get_stock_brush(index: i32) -> usize {
    get_stock_object(index)
}

pub extern "win64" fn get_text_color(hdc: usize) -> u32 {
    gui::dc_text_color(hdc)
}

pub extern "win64" fn get_bk_color(hdc: usize) -> u32 {
    gui::dc_bk_color(hdc)
}

pub extern "win64" fn get_bk_mode(hdc: usize) -> i32 {
    gui::dc_bk_mode(hdc) as i32
}

pub extern "win64" fn set_rop2(_hdc: usize, _mode: i32) -> i32 {
    13 // R2_COPYPEN
}

pub extern "win64" fn get_rop2(_hdc: usize) -> i32 {
    13
}

pub extern "win64" fn set_stretch_blt_mode(_hdc: usize, _mode: i32) -> i32 {
    1 // ancien mode
}

pub extern "win64" fn get_stretch_blt_mode(_hdc: usize) -> i32 {
    4 // HALFTONE
}

pub extern "win64" fn set_poly_fill_mode(_hdc: usize, _mode: i32) -> i32 {
    1 // ancien mode
}

pub extern "win64" fn get_poly_fill_mode(_hdc: usize) -> i32 {
    1 // ALTERNATE
}

pub extern "win64" fn set_text_align(_hdc: usize, _mode: u32) -> u32 {
    0
}

pub extern "win64" fn get_text_align(_hdc: usize) -> u32 {
    0 // TA_LEFT | TA_TOP
}

pub unsafe extern "win64" fn get_text_character_extra(_hdc: usize) -> i32 {
    0
}

pub unsafe extern "win64" fn set_text_character_extra(_hdc: usize, _extra: i32) -> i32 {
    0
}

#[allow(dead_code)]
pub extern "win64" fn create_compatible_dc_from(dc: usize) -> usize {
    create_compatible_dc(dc)
}

pub extern "win64" fn create_dc_a(
    _driver: *const u8,
    _device: *const u8,
    _output: *const u8,
    _init: *const c_void,
) -> usize {
    gui::create_dc(0) // DC sans fenêtre
}

pub extern "win64" fn create_dc_w(
    _driver: *const u16,
    _device: *const u16,
    _output: *const u16,
    _init: *const c_void,
) -> usize {
    gui::create_dc(0)
}

pub extern "win64" fn create_ic_a(
    _driver: *const u8,
    _device: *const u8,
    _output: *const u8,
    _init: *const c_void,
) -> usize {
    gui::create_dc(0)
}

pub extern "win64" fn create_ic_w(
    _driver: *const u16,
    _device: *const u16,
    _output: *const u16,
    _init: *const c_void,
) -> usize {
    gui::create_dc(0)
}

pub unsafe extern "win64" fn get_current_position_ex(hdc: usize, pt: *mut gui::Point) -> i32 {
    if pt.is_null() {
        return 0;
    }
    let map = lock_pen();
    let (x, y) = map.get(&hdc).copied().unwrap_or((0, 0));
    (*pt).x = x;
    (*pt).y = y;
    1
}

pub extern "win64" fn set_brush_org_ex(
    _hdc: usize,
    _x: i32,
    _y: i32,
    _prev: *mut gui::Point,
) -> i32 {
    1
}

pub extern "win64" fn get_nearest_color(_hdc: usize, color: u32) -> u32 {
    color
}

pub extern "win64" fn realize_palette(_hdc: usize) -> u32 {
    0
}

pub extern "win64" fn select_palette(_hdc: usize, _pal: usize, _force: i32) -> usize {
    0
}

pub extern "win64" fn create_palette(_pal: *const c_void) -> usize {
    0
}

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
    let avg_w = if font.height > 0 {
        (font.height * 45) / 100
    } else {
        7
    };
    for i in 0..=(last - first) {
        *widths.add(i as usize) = avg_w;
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

pub unsafe extern "win64" fn get_char_abc_widths_a(
    hdc: usize,
    first: u32,
    last: u32,
    abc: *mut c_void,
) -> i32 {
    let _ = (hdc, first, last, abc);
    0
}

pub unsafe extern "win64" fn get_char_abc_widths_w(
    hdc: usize,
    first: u32,
    last: u32,
    abc: *mut c_void,
) -> i32 {
    let _ = (hdc, first, last, abc);
    0
}

pub extern "win64" fn gdi_flush() -> i32 {
    1
}

pub extern "win64" fn gdi_set_batch_limit(_limit: u32) -> u32 {
    1
}

pub extern "win64" fn gdi_get_batch_limit() -> u32 {
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        // --- Objects ---
        "GetStockObject"=>fn get_stock_object,
        "CreateSolidBrush"=>fn create_solid_brush,
        "CreatePen"=>fn create_pen,
        "CreateFontA"=>fn create_font_a, "CreateFontW"=>fn create_font_w,
        "CreateFontIndirectA"=>fn create_font_indirect_a,
        "CreateFontIndirectW"=>fn create_font_indirect_w,
        "GetObjectA"=>fn get_object_a, "GetObjectW"=>fn get_object_w,
        "GetObjectType"=>fn get_object_type,
        "GetCurrentObject"=>fn get_current_object,
        "SelectObject"=>fn select_object,
        "DeleteObject"=>fn delete_object,
        // --- DC colors ---
        "SetTextColor"=>fn set_text_color, "GetTextColor"=>fn get_text_color,
        "SetBkColor"=>fn set_bk_color, "GetBkColor"=>fn get_bk_color,
        "SetBkMode"=>fn set_bk_mode, "GetBkMode"=>fn get_bk_mode,
        // --- Text output ---
        "TextOutA"=>fn text_out_a, "TextOutW"=>fn text_out_w,
        "ExtTextOutA"=>fn ext_text_out_a, "ExtTextOutW"=>fn ext_text_out_w,
        "DrawTextA"=>fn draw_text_a, "DrawTextW"=>fn draw_text_w,
        "GetTextExtentPoint32A"=>fn get_text_extent_point32_a,
        "GetTextExtentPoint32W"=>fn get_text_extent_point32_w,
        "GetTextExtentPointA"=>fn get_text_extent_point32_a,
        "GetTextExtentPointW"=>fn get_text_extent_point32_w,
        "SetTextAlign"=>fn set_text_align, "GetTextAlign"=>fn get_text_align,
        "SetTextCharacterExtra"=>fn set_text_character_extra,
        "GetTextCharacterExtra"=>fn get_text_character_extra,
        // --- Text metrics ---
        "GetTextMetricsA"=>fn get_text_metrics_a, "GetTextMetricsW"=>fn get_text_metrics_w,
        "GetTextFaceA"=>fn get_text_face_a, "GetTextFaceW"=>fn get_text_face_w,
        "GetCharWidth32A"=>fn get_char_width_32a, "GetCharWidth32W"=>fn get_char_width_32w,
        "GetCharABCWidthsA"=>fn get_char_abc_widths_a, "GetCharABCWidthsW"=>fn get_char_abc_widths_w,
        // --- Font enum ---
        "EnumFontFamiliesExA"=>fn enum_font_families_ex_a,
        "EnumFontFamiliesExW"=>fn enum_font_families_ex_w,
        "EnumFontsA"=>fn enum_fonts_a, "EnumFontsW"=>fn enum_fonts_w,
        // --- Raster ops ---
        "PatBlt"=>fn pat_blt,
        "BitBlt"=>fn bit_blt, "StretchBlt"=>fn stretch_blt,
        "SetROP2"=>fn set_rop2, "GetROP2"=>fn get_rop2,
        "SetStretchBltMode"=>fn set_stretch_blt_mode, "GetStretchBltMode"=>fn get_stretch_blt_mode,
        // --- Drawing primitives ---
        "Rectangle"=>fn rectangle, "Ellipse"=>fn ellipse, "RoundRect"=>fn round_rect,
        "MoveToEx"=>fn move_to_ex, "LineTo"=>fn line_to,
        "Polyline"=>fn polyline, "Polygon"=>fn polygon,
        "PolylineTo"=>fn polyline_to,
        "PolyBezier"=>fn poly_bezier, "PolyBezierTo"=>fn poly_bezier_to,
        "SetPixel"=>fn set_pixel, "GetPixel"=>fn get_pixel, "SetPixelV"=>fn set_pixel_v,
        "GetCurrentPositionEx"=>fn get_current_position_ex,
        "SetPolyFillMode"=>fn set_poly_fill_mode, "GetPolyFillMode"=>fn get_poly_fill_mode,
        // --- DC management ---
        "CreateCompatibleDC"=>fn create_compatible_dc,
        "CreateCompatibleBitmap"=>fn create_compatible_bitmap,
        "CreateDIBSection"=>fn create_dib_section,
        "CreateDCA"=>fn create_dc_a, "CreateDCW"=>fn create_dc_w,
        "CreateICA"=>fn create_ic_a, "CreateICW"=>fn create_ic_w,
        "DeleteDC"=>fn delete_dc,
        "GetDeviceCaps"=>fn get_device_caps,
        "SaveDC"=>fn save_dc, "RestoreDC"=>fn restore_dc,
        // --- Map mode ---
        "GetMapMode"=>fn get_map_mode, "SetMapMode"=>fn set_map_mode,
        "SetViewportOrgEx"=>fn set_viewport_org_ex, "GetViewportOrgEx"=>fn get_viewport_org_ex,
        "SetWindowOrgEx"=>fn set_window_org_ex, "GetWindowOrgEx"=>fn get_window_org_ex,
        "SetViewportExtEx"=>fn set_viewport_ext_ex, "GetViewportExtEx"=>fn get_viewport_ext_ex,
        "SetWindowExtEx"=>fn set_window_ext_ex, "GetWindowExtEx"=>fn get_window_ext_ex,
        "DPtoLP"=>fn dp_to_lp, "LPtoDP"=>fn lp_to_dp,
        // --- Clipping ---
        "SelectClipRgn"=>fn select_clip_rgn, "ExtSelectClipRgn"=>fn ext_select_clip_rgn,
        "IntersectClipRect"=>fn intersect_clip_rect, "ExcludeClipRect"=>fn exclude_clip_rect,
        "GetClipBox"=>fn get_clip_box, "GetClipRgn"=>fn get_clip_rgn,
        // --- Regions ---
        "CreateRectRgn"=>fn create_rect_rgn, "CreateRectRgnIndirect"=>fn create_rect_rgn_indirect,
        "CreateEllipticRgn"=>fn create_elliptic_rgn,
        "CombineRgn"=>fn combine_rgn, "SetRectRgn"=>fn set_rect_rgn,
        "OffsetRgn"=>fn offset_rgn, "GetRgnBox"=>fn get_rgn_box,
        "PtInRegion"=>fn pt_in_region, "RectInRegion"=>fn rect_in_region,
        "EqualRgn"=>fn equal_rgn,
        // --- Bitmap ---
        "GetDIBits"=>fn get_dib_its, "SetDIBitsToDevice"=>fn set_dib_its_to_device,
        "StretchDIBits"=>fn stretch_dib_its,
        "GetBitmapBits"=>fn get_bitmap_bits, "SetBitmapBits"=>fn set_bitmap_bits,
        // --- Misc ---
        "SetBrushOrgEx"=>fn set_brush_org_ex,
        "GetNearestColor"=>fn get_nearest_color,
        "RealizePalette"=>fn realize_palette,
        "SelectPalette"=>fn select_palette,
        "CreatePalette"=>fn create_palette,
        "GdiFlush"=>fn gdi_flush,
        "GdiSetBatchLimit"=>fn gdi_set_batch_limit, "GdiGetBatchLimit"=>fn gdi_get_batch_limit,
        // --- Custom ---
        "WinluxDrawIconA"=>fn winlux_draw_icon_a,
    }
}
