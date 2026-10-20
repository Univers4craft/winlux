//! gdi32 minimal : objets GDI (pinceaux, plumes) et primitives de dessin qui
//! alimentent la liste de commandes du noyau GUI. Le backend (X11) rejoue cette
//! liste à chaque cycle de peinture.

use crate::dlls::gui::{self, DrawCmd, FontSpec, Rect};

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
    let color = match rop {
        0x00F0_0021 => gui::object_color(gui::dc_brush(hdc)), // PATCOPY
        0x0000_0042 => Some(0x0000_0000),                    // BLACKNESS
        0x00FF_0062 => Some(0x00FF_FFFF),                    // WHITENESS
        _ => {
            crate::process::set_last_error(87);
            return 0;
        }
    };
    if let Some(color) = color {
        gui::record(hdc, DrawCmd::FillRect { rect: Rect::from_size(x, y, width, height), color });
    }
    1
}

pub extern "win64" fn create_compatible_dc(_dc: usize) -> usize {
    // DC mémoire : pas de cible de rendu, les commandes sont ignorées.
    gui::create_dc(0)
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
        "SelectObject"=>fn select_object,
        "DeleteObject"=>fn delete_object,
        "SetTextColor"=>fn set_text_color,
        "SetBkColor"=>fn set_bk_color,
        "SetBkMode"=>fn set_bk_mode,
        "PatBlt"=>fn pat_blt,
        "CreateCompatibleDC"=>fn create_compatible_dc,
        "DeleteDC"=>fn delete_dc,
        "GetDeviceCaps"=>fn get_device_caps,
        "TextOutA"=>fn text_out_a,
        "Rectangle"=>fn rectangle,
        "Ellipse"=>fn ellipse,
    }
}
