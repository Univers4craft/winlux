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

fn font_metrics(hdc: usize) -> (i32, i32, i32, i32, i32, i32, i32, bool, bool, bool) {
    let font = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
    // Métriques réelles du fichier TrueType, les mêmes que celles du rendu
    // (ExtTextOut, GetGlyphOutlineW) : avec une estimation, Qt (VLC)
    // coupait le bas des lignes et TA_BASELINE plaçait le texte trop bas.
    if let Some(f) = super::usp10::dc_font(hdc) {
        let mut tm = [0u8; 60];
        super::usp10::text_metrics(&f, &mut tm);
        let at = |i: usize| i32::from_le_bytes(tm[i * 4..i * 4 + 4].try_into().unwrap());
        return (
            at(0),
            at(1),
            at(2),
            at(3),
            at(4),
            at(5),
            font.weight,
            font.italic,
            font.underline,
            font.strikeout,
        );
    }
    let h = if font.height != 0 {
        font.height.abs()
    } else {
        16
    };
    // Estimation quand aucune police TrueType n'est trouvée
    let ascent = (h * 73) / 100; // ~73% de la hauteur est l'ascent
    let descent = h - ascent; // ~27% pour le descent
    let internal_leading = (h * 5) / 100; // espace interne pour accents
    let external_leading = (h * 12) / 100; // interligne
    let avg_w = (h * 47) / 100; // largeur moyenne ~47% de la hauteur
    let _max_w = avg_w * 2; // largeur max pour caractères larges (W, M)
    let weight = font.weight;
    (
        h,
        ascent,
        descent,
        internal_leading,
        external_leading,
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
    let (
        h,
        ascent,
        descent,
        internal_leading,
        external_leading,
        avg_w,
        weight,
        italic,
        underline,
        strikeout,
    ) = font_metrics(hdc);
    std::ptr::write(
        out,
        TextMetricA {
            height: h,
            ascent,
            descent,
            internal_leading,
            external_leading,
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
    let (
        h,
        ascent,
        descent,
        internal_leading,
        external_leading,
        avg_w,
        weight,
        italic,
        underline,
        strikeout,
    ) = font_metrics(hdc);
    std::ptr::write(
        out,
        TextMetricW {
            height: h,
            ascent,
            descent,
            internal_leading,
            external_leading,
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
    dx: *const i32,
) -> i32 {
    let s = if !text.is_null() && count > 0 {
        crate::dlls::decode_ansi(std::slice::from_raw_parts(text, count as usize))
    } else {
        String::new()
    };
    // lpDx compte des octets : il ne s'applique tel quel qu'à un texte où
    // chaque octet donne un caractère (pas de DBCS).
    let dx = if s.chars().count() == count as usize {
        dx
    } else {
        std::ptr::null()
    };
    ext_text_out(hdc, x, y, options, rect, &s, dx)
}

pub unsafe extern "win64" fn ext_text_out_w(
    hdc: usize,
    x: i32,
    y: i32,
    options: u32,
    rect: *const Rect,
    text: *const u16,
    count: u32,
    dx: *const i32,
) -> i32 {
    const ETO_GLYPH_INDEX: u32 = 0x10;
    let s = if text.is_null() || count == 0 {
        String::new()
    } else if options & ETO_GLYPH_INDEX != 0 {
        glyphs_to_text(hdc, std::slice::from_raw_parts(text, count as usize))
    } else {
        String::from_utf16_lossy(std::slice::from_raw_parts(text, count as usize))
    };
    // lpDx compte des unités UTF-16 : une paire de substitution (hors BMP)
    // fausserait l'appariement, on retombe alors sur l'avance naturelle.
    let dx = if s.chars().count() == count as usize {
        dx
    } else {
        std::ptr::null()
    };
    ext_text_out(hdc, x, y, options, rect, &s, dx)
}

/// Texte équivalent à des numéros de glyphes (ETO_GLYPH_INDEX, que Qt
/// emploie pour dessiner chaque glyphe dans une DIB) : chaque glyphe
/// redevient son caractère dans la police du DC ; un glyphe sans caractère
/// devient une espace pour garder l'appariement avec lpDx.
fn glyphs_to_text(hdc: usize, glyphs: &[u16]) -> String {
    let Some(f) = super::usp10::dc_font(hdc) else {
        return String::new();
    };
    glyphs
        .iter()
        .map(|&g| {
            f.font
                .char_of_glyph(g)
                .and_then(char::from_u32)
                .unwrap_or(' ')
        })
        .collect()
}

/// Cœur d'`ExtTextOut` : ETO_OPAQUE remplit le rectangle avec la couleur de
/// fond, ETO_CLIPPED limite le dessin au rectangle, et `lpDx` place chaque
/// caractère dans sa cellule (terminaux comme PuTTY, qui imposent la
/// largeur de cellule de leur police à espacement fixe).
unsafe fn ext_text_out(
    hdc: usize,
    x: i32,
    y: i32,
    options: u32,
    rect: *const Rect,
    s: &str,
    dx: *const i32,
) -> i32 {
    const ETO_OPAQUE: u32 = 0x2;
    const ETO_CLIPPED: u32 = 0x4;
    const ETO_PDY: u32 = 0x2000;
    if gui::dc_window(hdc).is_none() {
        return 0;
    }
    let r = (!rect.is_null()).then(|| std::ptr::read_unaligned(rect));
    let saved_clip = match r {
        Some(r) if options & ETO_CLIPPED != 0 => {
            let old = gui::dc_clip(hdc);
            gui::intersect_dc_clip(hdc, r);
            Some(old)
        }
        _ => None,
    };
    if let Some(r) = r.filter(|_| options & ETO_OPAQUE != 0) {
        let bk = gui::dc_bk_color(hdc);
        gui::record(hdc, DrawCmd::FillRect { rect: r, color: bk });
    }
    if !s.is_empty() {
        let color = gui::dc_text_color(hdc);
        let bk = if gui::dc_bk_mode(hdc) == 1 {
            None
        } else {
            Some(gui::dc_bk_color(hdc))
        };
        let font = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
        let n = s.chars().count();
        let step = if options & ETO_PDY != 0 { 2 } else { 1 };
        let advances: Option<Vec<(i32, i32)>> = (!dx.is_null()).then(|| {
            (0..n)
                .map(|i| {
                    let ax = std::ptr::read_unaligned(dx.add(i * step));
                    let ay = if step == 2 {
                        std::ptr::read_unaligned(dx.add(i * step + 1))
                    } else {
                        0
                    };
                    (ax, ay)
                })
                .collect()
        });
        let natural = gui::measure_text(&font, s).0;
        match advances {
            Some(adv)
                if adv.iter().any(|a| a.1 != 0)
                    || adv.iter().map(|a| a.0).sum::<i32>() != natural =>
            {
                // Chaque caractère à sa place : l'origine suit les avances
                // de lpDx et non celles de la police de remplacement.
                let total: i32 = adv.iter().map(|a| a.0).sum();
                let (ox, oy) = aligned_origin(hdc, x, y, "");
                let shift = match gui::dc_text_align(hdc) & 6 {
                    6 => total / 2,
                    2 => total,
                    _ => 0,
                };
                let (mut cx, mut cy) = (ox - shift, oy);
                for (ch, (ax, ay)) in s.chars().zip(adv) {
                    if !ch.is_whitespace() || bk.is_some() {
                        gui::record(
                            hdc,
                            DrawCmd::Text {
                                x: cx,
                                y: cy,
                                text: ch.to_string(),
                                color,
                                bk,
                                font: font.clone(),
                            },
                        );
                    }
                    cx += ax;
                    cy -= ay;
                }
            }
            _ => {
                let (x, y) = aligned_origin(hdc, x, y, s);
                gui::record(
                    hdc,
                    DrawCmd::Text {
                        x,
                        y,
                        text: s.to_string(),
                        color,
                        bk,
                        font,
                    },
                );
            }
        }
    }
    if let Some(old) = saved_clip {
        gui::set_dc_clip(hdc, old);
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
    let (x, y) = aligned_origin(hdc, x, y, &s);
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
    let width = gui::object_pen_width(pen);
    gui::record(
        hdc,
        DrawCmd::Line {
            x1: sx,
            y1: sy,
            x2: x,
            y2: y,
            color,
            width,
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
                width: gui::object_pen_width(pen),
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
                width: gui::object_pen_width(pen),
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
            width: gui::object_pen_width(pen),
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
    text_align: u32,
    clip: Option<Vec<gui::Rect>>,
    /// Origines de fenêtre et de viewport (#1232).
    origins: ((i32, i32), (i32, i32)),
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
        text_align: gui::dc_text_align(hdc),
        clip: gui::dc_clip(hdc),
        origins: gui::dc_origins(hdc),
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
    gui::set_dc_text_align(hdc, state.text_align);
    gui::select_object(hdc, state.brush);
    gui::select_object(hdc, state.pen);
    gui::select_object(hdc, state.font);
    pen_pos().lock().unwrap().insert(hdc, state.pos);
    gui::set_dc_clip(hdc, state.clip);
    let ((wx, wy), (vx, vy)) = state.origins;
    gui::set_dc_origin(hdc, false, wx, wy);
    gui::set_dc_origin(hdc, true, vx, vy);
    1
}

// ── Régions (stubs) ────────────────────────────────────────────────

/// Les régions sont de vrais objets GDI (voir `gdi32::create_rect_rgn`) ;
/// ces exports s'alignent sur eux plutôt que de renvoyer 1 (= WHITE_BRUSH).
pub extern "win64" fn create_rect_rgn(left: i32, top: i32, right: i32, bottom: i32) -> usize {
    crate::dlls::gdi32::create_rect_rgn(left, top, right, bottom)
}

pub unsafe extern "win64" fn create_rect_rgn_indirect(rect: *const Rect) -> usize {
    crate::dlls::gdi32::create_rect_rgn_indirect(rect)
}

pub extern "win64" fn select_clip_rgn(hdc: usize, rgn: usize) -> i32 {
    crate::dlls::gdi32::select_clip_rgn(hdc, rgn)
}

pub extern "win64" fn intersect_clip_rect(hdc: usize, l: i32, t: i32, r: i32, b: i32) -> i32 {
    crate::dlls::gdi32::intersect_clip_rect(hdc, l, t, r, b)
}

pub extern "win64" fn exclude_clip_rect(hdc: usize, l: i32, t: i32, r: i32, b: i32) -> i32 {
    crate::dlls::gdi32::exclude_clip_rect(hdc, l, t, r, b)
}

pub extern "win64" fn combine_rgn(dst: usize, src1: usize, src2: usize, mode: i32) -> i32 {
    crate::dlls::gdi32::combine_rgn(dst, src1, src2, mode)
}

pub extern "win64" fn set_rect_rgn(rgn: usize, l: i32, t: i32, r: i32, b: i32) -> i32 {
    crate::dlls::gdi32::set_rect_rgn(rgn, l, t, r, b)
}

// ── Pixel ──────────────────────────────────────────────────────────

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
pub unsafe extern "win64" fn offset_viewport_org_ex(
    hdc: usize,
    x: i32,
    y: i32,
    old: *mut gui::Point,
) -> i32 {
    crate::dlls::gdi32::offset_origin(hdc, true, x, y, old)
}

// ── Stretch mode ───────────────────────────────────────────────────

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
        // DIB bas-haut : l'origine de la source est le coin inférieur
        // gauche, et src_y compte les lignes depuis le bas (documentation de
        // StretchDIBits et SetDIBitsToDevice). La ligne du haut du rectangle
        // est donc la ligne mémoire src_y + src_h - 1. wxWidgets dessine
        // ainsi les boutons d'Audacity, découpés dans une grande DIB : on
        // lisait le bas de l'image, vide, et les barres d'outils restaient
        // blanches (#1485).
        let row = if top_down {
            (src_y as usize).wrapping_add(dy)
        } else {
            (src_y as usize)
                .checked_add(src_h)?
                .checked_sub(1)?
                .checked_sub(dy)?
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
    /// #1485 : dans une DIB bas-haut, src_y part du bas. Une bande de deux
    /// lignes prise à src_y = 2 sur quatre est le haut de l'image (lignes
    /// mémoire 3 puis 2), pas son bas.
    #[test]
    fn bottom_up_source_band_counts_from_the_bottom() {
        let mut info = header(32, 4);
        info.width = 1;
        // Lignes mémoire 0 à 3, de bas en haut : rouges 1, 2, 3, 4 (BGRA).
        let pixels: [u8; 16] = [0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 4, 0];
        unsafe {
            let base = &info as *const _ as *const u8;
            assert_eq!(
                decode_dib_pixels(&info, base, pixels.as_ptr(), 0, 2, 1, 2),
                Some(vec![4, 3]),
                "haut de l'image"
            );
            assert_eq!(
                decode_dib_pixels(&info, base, pixels.as_ptr(), 0, 0, 1, 2),
                Some(vec![2, 1]),
                "bas de l'image"
            );
            info.height = -4;
            assert_eq!(
                decode_dib_pixels(&info, base, pixels.as_ptr(), 0, 2, 1, 2),
                Some(vec![3, 4]),
                "haut-bas : src_y part du haut"
            );
        }
    }
}

/// GetDIBits sans tampon de pixels : description du format, comme Windows
/// pour un écran en 32 bits. Premier appel (biBitCount = 0) : en-tête seul,
/// 32 bits, BI_BITFIELDS ; second appel : les trois masques 8 bits par
/// composante après l'en-tête. SDL3 en tire le format de l'écran (Warzone
/// 2100 écartait tous les modes faute de 8 bits par composante).
/// La lecture des pixels eux-mêmes n'est pas encore prise en charge.
/// GetDIBits avec tampon : copie les pixels de la bitmap au format demandé
/// (32 bits BI_RGB ou BI_BITFIELDS, 24 bits BI_RGB), bas-haut si biHeight
/// est positif. Sans cela, wxWidgets recopiait des bitmaps vides : barre
/// d'outils de FileZilla sans icônes (#793).
unsafe fn copy_dib_bits(bmp: usize, start: u32, lines: u32, bits: *mut u8, head: *mut u8) -> i32 {
    let size = (head as *const u32).read_unaligned() as usize;
    if size < 40 {
        return 0;
    }
    let bit_count = (head.add(14) as *const u16).read_unaligned();
    let compression = (head.add(16) as *const u32).read_unaligned();
    if !matches!((bit_count, compression), (32, 0) | (32, 3) | (24, 0)) {
        return 0;
    }
    let Some((w, h, px, alpha)) = crate::dlls::gui::bitmap_pixels(bmp) else {
        return 0;
    };
    let dw = ((head.add(4) as *const i32).read_unaligned().max(0) as usize).min(w);
    let raw_h = (head.add(8) as *const i32).read_unaligned();
    let top_down = raw_h < 0;
    let dh = (raw_h.unsigned_abs() as usize).min(h);
    let bpp = bit_count as usize / 8;
    let stride = ((head.add(4) as *const i32).read_unaligned().max(0) as usize * bpp + 3) & !3;
    let first = start as usize;
    let count = (lines as usize).min(dh.saturating_sub(first));
    for r in 0..count {
        let scan = first + r;
        let y = if top_down { scan } else { dh - 1 - scan };
        let row = bits.add(r * stride);
        for x in 0..dw {
            let c = px[y * w + x];
            let o = row.add(x * bpp);
            *o = (c >> 16) as u8;
            *o.add(1) = (c >> 8) as u8;
            *o.add(2) = c as u8;
            if bpp == 4 {
                *o.add(3) = if alpha { (c >> 24) as u8 } else { 0 };
            }
        }
    }
    count as i32
}

pub unsafe extern "win64" fn get_dib_its(
    _hdc: usize,
    bmp: usize,
    _start: u32,
    lines: u32,
    bits: *mut c_void,
    info: *mut c_void,
    _usage: u32,
) -> i32 {
    const BI_BITFIELDS: u32 = 3;
    if info.is_null() {
        return 0;
    }
    if !bits.is_null() {
        return copy_dib_bits(bmp, _start, lines, bits.cast::<u8>(), info.cast::<u8>());
    }
    let head = info as *mut u8;
    let size = (head as *const u32).read_unaligned() as usize;
    if size < 40 {
        return 0;
    }
    let bit_count = (head.add(14) as *const u16).read_unaligned();
    let compression = (head.add(16) as *const u32).read_unaligned();
    if bit_count == 0 {
        let mut bm = [0u8; 32];
        let (w, h) =
            if crate::dlls::gdi32::get_object_w(bmp, 32, bm.as_mut_ptr() as *mut c_void) > 0 {
                (
                    i32::from_le_bytes([bm[4], bm[5], bm[6], bm[7]]).max(1),
                    i32::from_le_bytes([bm[8], bm[9], bm[10], bm[11]]).max(1),
                )
            } else {
                (1, 1)
            };
        (head.add(4) as *mut i32).write_unaligned(w);
        (head.add(8) as *mut i32).write_unaligned(h);
        (head.add(12) as *mut u16).write_unaligned(1);
        (head.add(14) as *mut u16).write_unaligned(32);
        (head.add(16) as *mut u32).write_unaligned(BI_BITFIELDS);
        (head.add(20) as *mut u32).write_unaligned((w * h * 4) as u32);
        (head.add(32) as *mut u32).write_unaligned(0);
        (head.add(36) as *mut u32).write_unaligned(0);
        return lines.max(1) as i32;
    }
    if bit_count == 32 && compression == BI_BITFIELDS {
        // BITMAPINFOHEADER : masques après l'en-tête ; V4/V5 : dans l'en-tête.
        let masks = if size >= 52 {
            head.add(40)
        } else {
            head.add(size)
        };
        for (i, m) in [0x00FF_0000u32, 0x0000_FF00, 0x0000_00FF]
            .iter()
            .enumerate()
        {
            (masks.add(i * 4) as *mut u32).write_unaligned(*m);
        }
        return lines.max(1) as i32;
    }
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

pub extern "win64" fn set_text_align(hdc: usize, align: u32) -> u32 {
    gui::set_dc_text_align(hdc, align)
}
pub extern "win64" fn get_text_align(hdc: usize) -> u32 {
    gui::dc_text_align(hdc)
}

/// Coin haut-gauche du texte d'après SetTextAlign : (x, y) désigne la
/// ligne de base (TA_BASELINE, utilisé par Scintilla) ou le bas (TA_BOTTOM),
/// et le bord droit (TA_RIGHT) ou le centre (TA_CENTER).
pub fn aligned_origin(hdc: usize, x: i32, y: i32, text: &str) -> (i32, i32) {
    const TA_RIGHT: u32 = 2;
    const TA_CENTER: u32 = 6;
    const TA_BOTTOM: u32 = 8;
    const TA_BASELINE: u32 = 24;
    let align = gui::dc_text_align(hdc);
    let mut origin = (x, y);
    if align & (TA_RIGHT | TA_CENTER) != 0 {
        let font = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
        let width = gui::measure_text(&font, text).0;
        origin.0 -= if align & TA_CENTER == TA_CENTER {
            width / 2
        } else {
            width
        };
    }
    if align & TA_BASELINE == TA_BASELINE {
        origin.1 -= font_metrics(hdc).1;
    } else if align & TA_BOTTOM != 0 {
        origin.1 -= font_metrics(hdc).0;
    }
    origin
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

// ── CreateBitmap ───────────────────────────────────────────────────

/// CreateBitmap : bitmap dépendante du périphérique, remplie avec `bits`
/// s'il est fourni. Les lignes partent du haut et sont alignées sur 16 bits.
/// Les masques monochromes (1 bit) donnent du blanc pour 1 et du noir pour
/// 0 : wxWidgets les passe ensuite à MaskBlt.
pub unsafe extern "win64" fn create_bitmap(
    width: i32,
    height: i32,
    planes: u32,
    bpp: u32,
    bits: *const c_void,
) -> usize {
    // Largeur ou hauteur nulle : bitmap 1x1 monochrome, comme Windows
    // (documentation de CreateBitmap, Remarks). Voir
    // docs/compat/gdi32-comportements.md (#905) : ne pas refuser ce cas.
    if width == 0 || height == 0 {
        return gui::create_compatible_bitmap(1, 1).unwrap_or(0);
    }
    let Some(handle) = gui::create_compatible_bitmap(width, height) else {
        crate::process::set_last_error(87);
        return 0;
    };
    let depth = planes.max(1) * bpp;
    if depth == 1 {
        gui::mark_monochrome(handle);
    }
    if !bits.is_null() {
        let (w, h) = (width as usize, height as usize);
        let stride = (w * depth as usize).div_ceil(16) * 2;
        let data = std::slice::from_raw_parts(bits as *const u8, stride * h);
        for (y, line) in data.chunks(stride).enumerate() {
            if let Some(row) = ddb_row(line, w, depth) {
                gui::write_bitmap_row(handle, y, &row);
            }
        }
    }
    handle
}

/// Une ligne de bitmap brute (1, 24 ou 32 bits par pixel) en COLORREF.
fn ddb_row(line: &[u8], width: usize, depth: u32) -> Option<Vec<u32>> {
    let bgr = |b: u8, g: u8, r: u8| r as u32 | (g as u32) << 8 | (b as u32) << 16;
    Some(match depth {
        1 => (0..width)
            .map(|x| {
                if line[x / 8] & (0x80 >> (x % 8)) != 0 {
                    0x00FF_FFFF
                } else {
                    0
                }
            })
            .collect(),
        24 => line
            .chunks(3)
            .take(width)
            .map(|p| bgr(p[0], p[1], p[2]))
            .collect(),
        32 => line
            .chunks(4)
            .take(width)
            .map(|p| bgr(p[0], p[1], p[2]))
            .collect(),
        _ => return None,
    })
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
        "SetPixel" => fn set_pixel,
        "SetPixelV" => fn set_pixel_v,
        "FillRect" => fn fill_rect,
        "FrameRect" => fn frame_rect,
        "InvertRect" => fn invert_rect,
        "SetMapMode" => fn set_map_mode,
        "GetMapMode" => fn get_map_mode,
        "OffsetViewportOrgEx" => fn offset_viewport_org_ex,
        "SetROP2" => fn set_rop2,
        "GetROP2" => fn get_rop2,
        "RealizePalette" => fn realize_palette,
        "SelectPalette" => fn select_palette,
        "GetDIBits" => fn get_dib_its,
        "SetDIBitsToDevice" => fn set_di_bits_to_device,
        "StretchDIBits" => fn stretch_di_bits,
        "CreateFontIndirectA" => fn create_font_indirect_a,
        "CreateFontIndirectW" => fn create_font_indirect_w,
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
        "SetTextAlign" => fn set_text_align,
        "GetTextAlign" => fn get_text_align,
        "SetTextJustification" => fn set_text_justification,
        "SetTextCharacterExtra" => fn set_text_char_extra,
        "GetTextCharacterExtra" => fn get_text_char_extra,
        "GetTextFaceA" => fn get_text_face_a,
        "GetTextFaceW" => fn get_text_face_w,
        "GetClipRgn" => fn get_clip_rgn,
        "GdiFlush" => fn gdi_flush,
        "GdiSetBatchLimit" => fn gdi_set_batch_limit,
        "CreateBitmap" => fn create_bitmap,
    }
}

#[cfg(test)]
mod text_align_tests {
    use super::*;

    /// Scintilla (Notepad++) écrit avec TA_BASELINE : y désigne la ligne de
    /// base, le texte doit donc remonter de l'ascent de la police.
    #[test]
    fn ta_baseline_remonte_le_texte_de_l_ascent() {
        let hdc = gui::create_dc(0);
        assert_eq!(get_text_align(hdc), 0);
        assert_eq!(aligned_origin(hdc, 10, 40, "abc"), (10, 40));
        assert_eq!(set_text_align(hdc, 24), 0); // TA_BASELINE
        assert_eq!(get_text_align(hdc), 24);
        let ascent = font_metrics(hdc).1;
        assert!(ascent > 0);
        assert_eq!(aligned_origin(hdc, 10, 40, "abc"), (10, 40 - ascent));
        // TA_RIGHT | TA_BOTTOM : coin bas-droit.
        set_text_align(hdc, 2 | 8);
        let font = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
        let width = gui::measure_text(&font, "abc").0;
        assert_eq!(
            aligned_origin(hdc, 100, 40, "abc"),
            (100 - width, 40 - font_metrics(hdc).0)
        );
        gui::free_dc(hdc);
        assert_eq!(set_text_align(hdc, 0), u32::MAX);
    }
    /// GetTextMetricsW donne les métriques du fichier TrueType (Segoe UI
    /// 9 pt : 16 px de haut, 13 au-dessus de la ligne de base), pas une
    /// estimation : Qt (VLC) en tire la hauteur de ses lignes.
    #[test]
    fn metriques_de_texte_reelles() {
        let hdc = gui::create_dc(0);
        let Some(f) = crate::dlls::usp10::dc_font(hdc) else {
            gui::free_dc(hdc);
            return;
        };
        let mut tm = [0u8; 60];
        crate::dlls::usp10::text_metrics(&f, &mut tm);
        let real = i32::from_le_bytes(tm[0..4].try_into().unwrap());
        let m = font_metrics(hdc);
        assert_eq!(m.0, real);
        assert_eq!(m.0, m.1 + m.2, "hauteur = ascent + descent");
        assert!(m.1 > m.2);
        gui::free_dc(hdc);
    }

    /// Qt (VLC) dessine chaque glyphe dans une DIB avec
    /// ExtTextOutW(ETO_GLYPH_INDEX) puis en lit les pixels : sans dessin,
    /// tout le texte de l'interface restait invisible.
    #[test]
    fn eto_glyph_index_dessine_les_glyphes() {
        let hdc = gui::create_dc(0);
        let Some(f) = crate::dlls::usp10::dc_font(hdc) else {
            eprintln!("aucune police TrueType : test sauté");
            gui::free_dc(hdc);
            return;
        };
        let glyphs: Vec<u16> = "Hi".chars().map(|c| f.font.glyph_index(c as u32)).collect();
        assert!(glyphs.iter().all(|&g| g != 0));
        assert_eq!(glyphs_to_text(hdc, &glyphs), "Hi");
        assert_eq!(glyphs_to_text(hdc, &[0xFFFF]), " ");

        let bmp = gui::create_compatible_bitmap(40, 30).unwrap();
        gui::select_object(hdc, bmp);
        gui::record(
            hdc,
            DrawCmd::FillRect {
                rect: Rect::from_size(0, 0, 40, 30),
                color: 0x00FF_FFFF,
            },
        );
        gui::set_dc_text_color(hdc, 0);
        gui::set_dc_bk_mode(hdc, 1);
        let ok = unsafe {
            ext_text_out_w(
                hdc,
                2,
                2,
                0x10,
                std::ptr::null(),
                glyphs.as_ptr(),
                glyphs.len() as u32,
                std::ptr::null(),
            )
        };
        assert_ne!(ok, 0);
        let snap = gui::bitmap_snapshot(hdc, Rect::from_size(0, 0, 40, 30)).unwrap();
        let dark = snap
            .pixels
            .iter()
            .filter(|&&p| p & 0x00FF_FFFF != 0x00FF_FFFF)
            .count();
        assert!(dark > 5, "aucun pixel de texte ({dark})");
        gui::select_object(hdc, gui::DEFAULT_BITMAP);
        gui::delete_object(bmp);
        gui::free_dc(hdc);
    }
}
