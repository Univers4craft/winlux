//! gdi32 minimal : objets GDI (pinceaux, plumes) et primitives de dessin qui
//! alimentent la liste de commandes du noyau GUI. Le backend (X11) rejoue cette
//! liste à chaque cycle de peinture.

use crate::dlls::gui::{self, DrawCmd, FontSpec, Rect};
use std::ffi::c_void;

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
    if index >= 0 { (index as usize) + 1 } else { 0 }
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
        face: if face.is_empty() { "fixed".to_string() } else { face },
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
    let face = if face.is_null() { String::new() } else { crate::dlls::cstr(face) };
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
    let face = if face.is_null() { String::new() } else { crate::dlls::wstr(face) };
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
    let Some(info) = gui::object_info(obj) else { return 0 };
    let needed = match &info {
        gui::GdiObjectInfo::Brush { .. } => core::mem::size_of::<LogBrush>(),
        gui::GdiObjectInfo::Pen { .. } => core::mem::size_of::<LogPen>(),
        gui::GdiObjectInfo::Font(_) => {
            if wide { core::mem::size_of::<LogFontW>() } else { core::mem::size_of::<LogFontA>() }
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
        gui::GdiObjectInfo::Brush { style, color, hatch } => {
            std::ptr::write_unaligned(out.cast::<LogBrush>(), LogBrush { style, color, hatch });
        }
        gui::GdiObjectInfo::Pen { style, width, color } => {
            std::ptr::write_unaligned(out.cast::<LogPen>(), LogPen {
                style,
                width: WinPoint { x: width, y: 0 },
                color,
            });
        }
        gui::GdiObjectInfo::Bitmap { width, height, width_bytes, bits_per_pixel, bits } => {
            std::ptr::write_unaligned(out.cast::<BitmapObject>(), BitmapObject {
                bm_type: 0,
                bm_width: width as i32,
                bm_height: height as i32,
                bm_width_bytes: width_bytes as i32,
                bm_planes: 1,
                bm_bits_pixel: bits_per_pixel,
                bm_bits: bits as *mut c_void,
            });
        }
        gui::GdiObjectInfo::Font(font) if wide => {
            let mut face = [0u16; 32];
            for (dst, ch) in face.iter_mut().zip(font.face.encode_utf16()) {
                *dst = ch;
            }
            std::ptr::write_unaligned(out.cast::<LogFontW>(), LogFontW {
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
            });
        }
        gui::GdiObjectInfo::Font(font) => {
            let mut face = [0u8; 32];
            let ansi = crate::dlls::encode_ansi(&font.face);
            let count = ansi.len().min(face.len() - 1);
            face[..count].copy_from_slice(&ansi[..count]);
            std::ptr::write_unaligned(out.cast::<LogFontA>(), LogFontA {
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
            });
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
        0x0055_0009 => (Some(0), gui::RasterOp::Invert), // DSTINVERT
        0x00A0_00C9 => (gui::object_color(gui::dc_brush(hdc)), gui::RasterOp::And), // PATAND
        0x00FA_0089 => (gui::object_color(gui::dc_brush(hdc)), gui::RasterOp::Or), // PATOR
        _ => {
            crate::process::set_last_error(87);
            return 0;
        }
    };
    if let Some(color) = color {
        gui::record(hdc, DrawCmd::PatternRect {
            rect: Rect::from_size(x, y, width, height),
            color,
            rop: operation,
        });
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
    blit_common(dst_hdc, x, y, width, height, src_hdc, src_x, src_y, width, height, rop, false)
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
        dst_hdc, x, y, width, height, src_hdc, src_x, src_y,
        src_width, src_height, rop, true,
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
    let source = Rect { left: src_x, top: src_y, right: src_right, bottom: src_bottom };
    let destination = Rect { left: x, top: y, right: dst_right, bottom: dst_bottom };
    let cmd = if src_hwnd != 0 && dst_hwnd != 0 {
        DrawCmd::Blit { source_hwnd: src_hwnd, source, destination, stretch, rop: operation }
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
        DrawCmd::Bitmap { rect: destination, pixels, rop: operation }
    };
    if gui::record(dst_hdc, cmd) { 1 } else { 0 }
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

fn resize_bitmap(snapshot: gui::BitmapSnapshot, width: usize, height: usize) -> Option<Vec<u32>> {
    let count = width.checked_mul(height)?;
    if width == 0 || height == 0 || count > 4 * 1024 * 1024
        || snapshot.width == 0 || snapshot.height == 0
    {
        return None;
    }
    if snapshot.pixels.len() < snapshot.width.checked_mul(snapshot.height)? {
        return None;
    }
    if snapshot.width == width && snapshot.height == height {
        return Some(snapshot.pixels);
    }
    let mut output = vec![0; count];
    for y in 0..height {
        let sy = y * snapshot.height / height;
        for x in 0..width {
            let sx = x * snapshot.width / width;
            output[y * width + x] = snapshot.pixels[sy * snapshot.width + sx];
        }
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
    let Some((bitmap, pixel_bits)) =
        gui::create_dib_section(header.width, header.height.abs(), header.bit_count, header.height < 0)
    else {
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
        8 => 1920,   // HORZRES
        10 => 1080,  // VERTRES
        12 => 24,    // BITSPIXEL
        90 => 96,    // LOGPIXELSX
        88 => 96,    // LOGPIXELSY
        _ => 0,
    }
}

/// `TextOutA(hdc, x, y, texte, count)` : enregistre le texte au coin haut-gauche.
///
/// # Sécurité
/// `text` doit pointer sur `count` octets lisibles.
pub unsafe extern "win64" fn text_out_a(hdc: usize, x: i32, y: i32, text: *const u8, count: i32) -> i32 {
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
    gui::record(hdc, DrawCmd::Text { x, y, text: s, color, bk, font });
    1
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
    let rect = Rect { left, top, right, bottom };
    if let Some(color) = gui::object_color(brush) {
        let inner = Rect { left: left + 1, top: top + 1, right: right - 1, bottom: bottom - 1 };
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
    let rect = Rect { left, top, right, bottom };
    let brush = gui::dc_brush(hdc);
    if let Some(color) = gui::object_color(brush) {
        gui::record(hdc, DrawCmd::Ellipse { rect, color });
    }
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetStockObject"=>fn get_stock_object,
        "CreateSolidBrush"=>fn create_solid_brush,
        "CreatePen"=>fn create_pen,
        "CreateFontA"=>fn create_font_a, "CreateFontW"=>fn create_font_w,
        "GetObjectA"=>fn get_object_a, "GetObjectW"=>fn get_object_w,
        "SelectObject"=>fn select_object,
        "DeleteObject"=>fn delete_object,
        "SetTextColor"=>fn set_text_color,
        "SetBkColor"=>fn set_bk_color,
        "SetBkMode"=>fn set_bk_mode,
        "PatBlt"=>fn pat_blt,
        "BitBlt"=>fn bit_blt, "StretchBlt"=>fn stretch_blt,
        "CreateCompatibleDC"=>fn create_compatible_dc,
        "CreateCompatibleBitmap"=>fn create_compatible_bitmap,
        "CreateDIBSection"=>fn create_dib_section,
        "DeleteDC"=>fn delete_dc,
        "GetDeviceCaps"=>fn get_device_caps,
        "TextOutA"=>fn text_out_a,
        "Rectangle"=>fn rectangle,
        "Ellipse"=>fn ellipse,
    }
}
