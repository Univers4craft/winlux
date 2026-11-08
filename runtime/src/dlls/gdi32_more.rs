//! gdi32 : fonctions complémentaires demandées par wxWidgets (Audacity) :
//! arcs et secteurs, polygones multiples, régions polygonales, bitmaps
//! indirectes, modes graphiques et transformations du monde, métafiles.
//!
//! Les régions de Winlux sont rectangulaires : une région polygonale prend
//! son rectangle englobant. Les transformations du monde sont retenues par
//! DC mais pas encore appliquées au dessin (journalisé une fois).

use super::lock;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

macro_rules! api {
    ($dll:literal, $name:literal, fn($($t:ty),*) -> $r:ty $(, $arg:expr)* $(,)?) => {{
        match super::lookup($dll, $name) {
            Some(addr) => {
                // SAFETY: adresse de la table d'exports de Winlux, signature
                // de l'API documentée.
                let f: unsafe extern "win64" fn($($t),*) -> $r = std::mem::transmute(addr);
                Some(f($($arg),*))
            }
            None => None,
        }
    }};
}

// --- Arcs, secteurs, cordes -------------------------------------------------------------

/// Points d'un arc d'ellipse (boîte l,t,r,b) du rayon (xs,ys) au rayon
/// (xe,ye), dans le sens antihoraire comme le mode par défaut de GDI.
fn arc_points(l: i32, t: i32, r: i32, b: i32, xs: i32, ys: i32, xe: i32, ye: i32) -> Vec<[i32; 2]> {
    let (cx, cy) = ((l + r) as f64 / 2.0, (t + b) as f64 / 2.0);
    let (rx, ry) = (((r - l) as f64 / 2.0).abs(), ((b - t) as f64 / 2.0).abs());
    // Angles en repère mathématique (y vers le haut).
    let a0 = (-(ys as f64 - cy)).atan2(xs as f64 - cx);
    let mut a1 = (-(ye as f64 - cy)).atan2(xe as f64 - cx);
    if a1 <= a0 {
        a1 += std::f64::consts::TAU;
    }
    let steps = (((a1 - a0) * (rx.max(ry) + 1.0)).abs() / 4.0).clamp(4.0, 256.0) as usize;
    (0..=steps)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f64 / steps as f64;
            [
                (cx + rx * a.cos()).round() as i32,
                (cy - ry * a.sin()).round() as i32,
            ]
        })
        .collect()
}

unsafe fn polyline(hdc: usize, pts: &[[i32; 2]]) -> i32 {
    api!(
        "gdi32",
        "Polyline",
        fn(usize, *const [i32; 2], i32) -> i32,
        hdc,
        pts.as_ptr(),
        pts.len() as i32
    )
    .unwrap_or(0)
}

unsafe fn polygon(hdc: usize, pts: &[[i32; 2]]) -> i32 {
    api!(
        "gdi32",
        "Polygon",
        fn(usize, *const [i32; 2], i32) -> i32,
        hdc,
        pts.as_ptr(),
        pts.len() as i32
    )
    .unwrap_or(0)
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn arc(
    hdc: usize,
    l: i32,
    t: i32,
    r: i32,
    b: i32,
    xs: i32,
    ys: i32,
    xe: i32,
    ye: i32,
) -> i32 {
    polyline(hdc, &arc_points(l, t, r, b, xs, ys, xe, ye))
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn pie(
    hdc: usize,
    l: i32,
    t: i32,
    r: i32,
    b: i32,
    xs: i32,
    ys: i32,
    xe: i32,
    ye: i32,
) -> i32 {
    let mut pts = arc_points(l, t, r, b, xs, ys, xe, ye);
    pts.push([(l + r) / 2, (t + b) / 2]);
    polygon(hdc, &pts)
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn chord(
    hdc: usize,
    l: i32,
    t: i32,
    r: i32,
    b: i32,
    xs: i32,
    ys: i32,
    xe: i32,
    ye: i32,
) -> i32 {
    polygon(hdc, &arc_points(l, t, r, b, xs, ys, xe, ye))
}

unsafe extern "win64" fn poly_polygon(
    hdc: usize,
    pts: *const [i32; 2],
    counts: *const i32,
    n: i32,
) -> i32 {
    if pts.is_null() || counts.is_null() || n <= 0 {
        return 0;
    }
    let mut at = 0usize;
    for i in 0..n as usize {
        let c = (*counts.add(i)).max(0) as usize;
        polygon(hdc, std::slice::from_raw_parts(pts.add(at), c));
        at += c;
    }
    1
}

unsafe extern "win64" fn poly_polyline(
    hdc: usize,
    pts: *const [i32; 2],
    counts: *const u32,
    n: u32,
) -> i32 {
    if pts.is_null() || counts.is_null() {
        return 0;
    }
    let mut at = 0usize;
    for i in 0..n as usize {
        let c = *counts.add(i) as usize;
        polyline(hdc, std::slice::from_raw_parts(pts.add(at), c));
        at += c;
    }
    1
}

// --- Régions ----------------------------------------------------------------------------

fn bounds(pts: &[[i32; 2]]) -> super::gui::Rect {
    let mut r = super::gui::Rect {
        left: i32::MAX,
        top: i32::MAX,
        right: i32::MIN,
        bottom: i32::MIN,
    };
    for p in pts {
        r.left = r.left.min(p[0]);
        r.top = r.top.min(p[1]);
        r.right = r.right.max(p[0]);
        r.bottom = r.bottom.max(p[1]);
    }
    if pts.is_empty() {
        r = super::gui::Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
    }
    r
}

unsafe extern "win64" fn create_polygon_rgn(pts: *const [i32; 2], n: i32, _mode: i32) -> usize {
    if pts.is_null() || n < 0 {
        return 0;
    }
    super::gui::create_region(bounds(std::slice::from_raw_parts(pts, n as usize)))
}

unsafe extern "win64" fn create_poly_polygon_rgn(
    pts: *const [i32; 2],
    counts: *const i32,
    n: i32,
    _mode: i32,
) -> usize {
    if pts.is_null() || counts.is_null() || n <= 0 {
        return 0;
    }
    let total: usize = (0..n as usize)
        .map(|i| (*counts.add(i)).max(0) as usize)
        .sum();
    super::gui::create_region(bounds(std::slice::from_raw_parts(pts, total)))
}

/// ExtCreateRegion : RGNDATAHEADER { dwSize, iType, nCount, nRgnSize, rcBound }.
unsafe extern "win64" fn ext_create_region(
    _xform: *const f32,
    size: u32,
    data: *const u8,
) -> usize {
    if data.is_null() || size < 32 {
        return 0;
    }
    let rc = *(data.add(16) as *const super::gui::Rect);
    super::gui::create_region(rc)
}

/// GetRegionData : un seul rectangle (la région rectangulaire de Winlux).
unsafe extern "win64" fn get_region_data(rgn: usize, size: u32, data: *mut u8) -> u32 {
    let Some(rc) = super::gui::region_rect(rgn) else {
        return 0;
    };
    let empty = rc.right <= rc.left || rc.bottom <= rc.top;
    let need = 32 + if empty { 0 } else { 16 };
    if data.is_null() {
        return need;
    }
    if size < need {
        return 0;
    }
    let w = |o: usize, v: u32| (data.add(o) as *mut u32).write_unaligned(v);
    w(0, 32);
    w(4, 1); // RDH_RECTANGLES
    w(8, if empty { 0 } else { 1 });
    w(12, need - 32);
    (data.add(16) as *mut super::gui::Rect).write_unaligned(rc);
    if !empty {
        (data.add(32) as *mut super::gui::Rect).write_unaligned(rc);
    }
    need
}

// --- Bitmaps ----------------------------------------------------------------------------

/// CreateBitmapIndirect : BITMAP { type, width, height, widthBytes, planes, bpp, bits }.
unsafe extern "win64" fn create_bitmap_indirect(bm: *const u8) -> usize {
    if bm.is_null() {
        return 0;
    }
    let rd = |o: usize| (bm.add(o) as *const i32).read_unaligned();
    let planes = (bm.add(16) as *const u16).read_unaligned() as u32;
    let bpp = (bm.add(18) as *const u16).read_unaligned() as u32;
    let bits = (bm.add(24) as *const usize).read_unaligned();
    api!(
        "gdi32",
        "CreateBitmap",
        fn(i32, i32, u32, u32, usize) -> usize,
        rd(4),
        rd(8),
        planes,
        bpp,
        bits
    )
    .unwrap_or(0)
}

/// CreateDIBitmap : bitmap compatible, remplie depuis les bits DIB si CBM_INIT.
unsafe extern "win64" fn create_di_bitmap(
    hdc: usize,
    header: *const u8,
    init: u32,
    bits: *const u8,
    info: *const u8,
    usage: u32,
) -> usize {
    if header.is_null() {
        return 0;
    }
    let w = (header.add(4) as *const i32).read_unaligned();
    let h = (header.add(8) as *const i32).read_unaligned().abs();
    let dc = if hdc == 0 {
        api!("user32", "GetDC", fn(usize) -> usize, 0).unwrap_or(0)
    } else {
        hdc
    };
    let hbm = api!(
        "gdi32",
        "CreateCompatibleBitmap",
        fn(usize, i32, i32) -> usize,
        dc,
        w,
        h
    )
    .unwrap_or(0);
    if hbm != 0 && init & 4 != 0 && !bits.is_null() && !info.is_null() {
        api!(
            "gdi32",
            "SetDIBits",
            fn(usize, usize, u32, u32, *const u8, *const u8, u32) -> i32,
            dc,
            hbm,
            0,
            h as u32,
            bits,
            info,
            usage
        );
    }
    if hdc == 0 && dc != 0 {
        api!("user32", "ReleaseDC", fn(usize, usize) -> i32, 0, dc);
    }
    hbm
}

/// GetDIBColorTable : les bitmaps de Winlux sont en couleurs vraies.
/// GetDIBColorTable : entrées RGBQUAD de la table de couleurs de la DIB
/// à palette sélectionnée dans le DC ; 0 pour une DIB sans palette.
pub unsafe extern "win64" fn get_dib_color_table(
    hdc: usize,
    start: u32,
    n: u32,
    rgb: *mut u32,
) -> u32 {
    let table = crate::dlls::gui::dc_dib_color_table(hdc);
    if rgb.is_null() {
        return 0;
    }
    let mut copied = 0;
    for (i, &c) in table
        .iter()
        .skip(start as usize)
        .take(n as usize)
        .enumerate()
    {
        // COLORREF 0x00BBGGRR → RGBQUAD (bleu, vert, rouge, 0).
        let quad = ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF);
        rgb.add(i).write_unaligned(quad);
        copied += 1;
    }
    copied
}

/// SetDIBColorTable : remplace des entrées de la table de couleurs.
pub unsafe extern "win64" fn set_dib_color_table(
    hdc: usize,
    start: u32,
    n: u32,
    rgb: *const u32,
) -> u32 {
    if rgb.is_null() {
        return 0;
    }
    let colors: Vec<u32> = (0..n as usize)
        .map(|i| {
            let q = rgb.add(i).read_unaligned();
            ((q & 0xFF) << 16) | (q & 0xFF00) | ((q >> 16) & 0xFF)
        })
        .collect();
    crate::dlls::gui::set_dc_dib_color_table(hdc, start as usize, &colors) as u32
}

/// MaskBlt : là où le masque monochrome vaut 1, l'opération de premier plan
/// (octets bas de `rop`) ; là où il vaut 0, celle d'arrière-plan (octet
/// haut). wxWidgets dessine ainsi ses bitmaps à masque, avec
/// MAKEROP4(SRCCOPY, DSTCOPY) : seuls les pixels opaques sont copiés.
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn mask_blt(
    dst: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    src: usize,
    xs: i32,
    ys: i32,
    mask: usize,
    xm: i32,
    ym: i32,
    rop: u32,
) -> i32 {
    const SRCCOPY: u32 = 0xCC;
    const DSTCOPY: u32 = 0xAA;
    let fore = (rop >> 16) & 0xFF;
    let back = rop >> 24;
    if mask != 0 && w > 0 && h > 0 {
        let copies = |code: u32| code == SRCCOPY || code == DSTCOPY;
        if copies(fore) && copies(back) {
            if let Some(img) = masked_source(src, xs, ys, w, h, mask, xm, ym, fore, back) {
                return super::common_apps::blend_image(dst, x, y, &img) as i32;
            }
        }
    }
    api!(
        "gdi32",
        "BitBlt",
        fn(usize, i32, i32, i32, i32, usize, i32, i32, u32) -> i32,
        dst,
        x,
        y,
        w,
        h,
        src,
        xs,
        ys,
        rop & 0x00FF_FFFF
    )
    .unwrap_or(0)
}

/// Pixels de la source avec un alpha tout ou rien tiré du masque : opaque
/// là où l'opération retenue est SRCCOPY, transparent pour DSTCOPY.
#[allow(clippy::too_many_arguments)]
fn masked_source(
    src: usize,
    xs: i32,
    ys: i32,
    w: i32,
    h: i32,
    mask: usize,
    xm: i32,
    ym: i32,
    fore: u32,
    back: u32,
) -> Option<super::icons::Image> {
    let bmp = super::gui::dc_bitmap(src);
    let (sw, sh, spx, _) = (bmp != 0)
        .then(|| super::gui::bitmap_pixels(bmp))
        .flatten()?;
    let (mw, mh, mpx, _) = super::gui::bitmap_pixels(mask)?;
    let at = |px: &[u32], pw: usize, ph: usize, x: i32, y: i32| {
        (x >= 0 && y >= 0 && (x as usize) < pw && (y as usize) < ph)
            .then(|| px[y as usize * pw + x as usize])
    };
    let mut out = Vec::with_capacity((w * h) as usize);
    for j in 0..h {
        for i in 0..w {
            let c = at(&spx, sw, sh, xs + i, ys + j).unwrap_or(0) & 0x00FF_FFFF;
            let bit = at(&mpx, mw, mh, xm + i, ym + j).is_some_and(|m| m & 0x00FF_FFFF != 0);
            let code = if bit { fore } else { back };
            let alpha = if code == 0xCC { 0xFF00_0000 } else { 0 };
            out.push(c | alpha);
        }
    }
    Some(super::icons::Image {
        w: w as usize,
        h: h as usize,
        px: out,
    })
}

/// ExtFloodFill : remplissage par diffusion non pris en charge.
unsafe extern "win64" fn ext_flood_fill(_hdc: usize, _x: i32, _y: i32, _c: u32, _t: u32) -> i32 {
    0
}

// --- Modes et transformations -----------------------------------------------------------

#[derive(Clone, Copy)]
struct DcExtra {
    graphics_mode: i32,
    layout: u32,
    xform: [f32; 6],
}

const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn extras() -> &'static Mutex<HashMap<usize, DcExtra>> {
    static E: OnceLock<Mutex<HashMap<usize, DcExtra>>> = OnceLock::new();
    E.get_or_init(Default::default)
}

fn with_extra<R>(hdc: usize, f: impl FnOnce(&mut DcExtra) -> R) -> R {
    let mut m = lock(extras());
    if m.len() > 4096 {
        m.clear();
    }
    let e = m.entry(hdc).or_insert(DcExtra {
        graphics_mode: 1,
        layout: 0,
        xform: IDENTITY,
    });
    f(e)
}

extern "win64" fn get_graphics_mode(hdc: usize) -> i32 {
    with_extra(hdc, |e| e.graphics_mode)
}

extern "win64" fn set_graphics_mode(hdc: usize, mode: i32) -> i32 {
    if !(1..=2).contains(&mode) {
        return 0;
    }
    with_extra(hdc, |e| std::mem::replace(&mut e.graphics_mode, mode))
}

extern "win64" fn get_layout(hdc: usize) -> u32 {
    with_extra(hdc, |e| e.layout)
}

extern "win64" fn set_layout(hdc: usize, l: u32) -> u32 {
    with_extra(hdc, |e| std::mem::replace(&mut e.layout, l))
}

fn note_xform(x: &[f32; 6]) {
    if *x != IDENTITY {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            crate::log_warn!(
                "gdi32",
                "transformation du monde retenue mais pas encore appliquée au dessin"
            )
        });
    }
}

unsafe extern "win64" fn get_world_transform(hdc: usize, x: *mut [f32; 6]) -> i32 {
    if x.is_null() {
        return 0;
    }
    *x = with_extra(hdc, |e| e.xform);
    1
}

unsafe extern "win64" fn set_world_transform(hdc: usize, x: *const [f32; 6]) -> i32 {
    if x.is_null() {
        return 0;
    }
    let v = *x;
    note_xform(&v);
    with_extra(hdc, |e| e.xform = v);
    1
}

/// a × b (XFORM : eM11, eM12, eM21, eM22, eDx, eDy).
fn mul(a: [f32; 6], b: [f32; 6]) -> [f32; 6] {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

/// MWT_IDENTITY 1, MWT_LEFTMULTIPLY 2, MWT_RIGHTMULTIPLY 3.
unsafe extern "win64" fn modify_world_transform(hdc: usize, x: *const [f32; 6], mode: u32) -> i32 {
    let v = if x.is_null() { IDENTITY } else { *x };
    let ok = with_extra(hdc, |e| {
        e.xform = match mode {
            1 => IDENTITY,
            2 => mul(v, e.xform),
            3 => mul(e.xform, v),
            _ => return false,
        };
        note_xform(&e.xform);
        true
    });
    ok as i32
}

// --- Métafiles --------------------------------------------------------------------------

/// Les métafiles ne sont pas encore pris en charge : leur création échoue,
/// ce que les programmes savent traiter (copie en image seulement).
unsafe extern "win64" fn meta_fail(_a: usize, _b: usize, _c: usize, _d: usize) -> usize {
    0
}

unsafe extern "win64" fn meta_ok(_a: usize) -> i32 {
    1
}

// --- Arcs reliés à la position courante, régions, couleurs DC_PEN/DC_BRUSH --------------

unsafe fn line_to(hdc: usize, p: [i32; 2]) {
    api!(
        "gdi32",
        "LineTo",
        fn(usize, i32, i32) -> i32,
        hdc,
        p[0],
        p[1]
    );
}

unsafe fn move_to(hdc: usize, p: [i32; 2]) {
    let none: *mut [i32; 2] = std::ptr::null_mut();
    api!(
        "gdi32",
        "MoveToEx",
        fn(usize, i32, i32, *mut [i32; 2]) -> i32,
        hdc,
        p[0],
        p[1],
        none
    );
}

/// Trace `pts` depuis la position courante (segment jusqu'au premier
/// point), puis place la position courante sur le dernier (ArcTo, AngleArc).
unsafe fn path_to(hdc: usize, pts: &[[i32; 2]]) -> i32 {
    let (Some(&first), Some(&last)) = (pts.first(), pts.last()) else {
        return 0;
    };
    line_to(hdc, first);
    let r = polyline(hdc, pts);
    move_to(hdc, last);
    r
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn arc_to(
    hdc: usize,
    l: i32,
    t: i32,
    r: i32,
    b: i32,
    xs: i32,
    ys: i32,
    xe: i32,
    ye: i32,
) -> i32 {
    path_to(hdc, &arc_points(l, t, r, b, xs, ys, xe, ye))
}

/// Points d'AngleArc : cercle de centre (x, y), angles en degrés, sens
/// trigonométrique pour un balayage positif.
fn angle_arc_points(x: i32, y: i32, radius: u32, start: f32, sweep: f32) -> Vec<[i32; 2]> {
    let (r, a0, da) = (
        radius as f64,
        (start as f64).to_radians(),
        (sweep as f64).to_radians(),
    );
    let steps = ((da.abs() * (r + 1.0)) / 4.0).clamp(1.0, 256.0) as usize;
    (0..=steps)
        .map(|i| {
            let a = a0 + da * i as f64 / steps as f64;
            [
                (x as f64 + r * a.cos()).round() as i32,
                (y as f64 - r * a.sin()).round() as i32,
            ]
        })
        .collect()
}

unsafe extern "win64" fn angle_arc(
    hdc: usize,
    x: i32,
    y: i32,
    r: u32,
    start: f32,
    sweep: f32,
) -> i32 {
    path_to(hdc, &angle_arc_points(x, y, r, start, sweep))
}

/// LOGPEN : style, largeur (POINT, x seul compte), couleur. Même taille en
/// x86 et en x64.
unsafe extern "win64" fn create_pen_indirect(lp: *const u32) -> usize {
    if lp.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let (style, width, color) = (*lp, *lp.add(1) as i32, *lp.add(3));
    crate::dlls::gui::create_pen(style, width, color)
}

unsafe fn fill_rect(hdc: usize, r: crate::dlls::gui::Rect, brush: usize) -> i32 {
    api!(
        "user32",
        "FillRect",
        fn(usize, *const crate::dlls::gui::Rect, usize) -> i32,
        hdc,
        &r,
        brush
    )
    .unwrap_or(0)
}

/// FillRgn : les régions de Winlux sont des rectangles.
unsafe extern "win64" fn fill_rgn(hdc: usize, rgn: usize, brush: usize) -> i32 {
    match crate::dlls::gui::region_rect(rgn) {
        Some(r) => fill_rect(hdc, r, brush).min(1),
        None => 0,
    }
}

/// PaintRgn : FillRgn avec le pinceau choisi dans le DC.
unsafe extern "win64" fn paint_rgn(hdc: usize, rgn: usize) -> i32 {
    fill_rgn(hdc, rgn, crate::dlls::gui::dc_brush(hdc))
}

/// FrameRgn : bord de `w` × `h` pixels autour de la région.
unsafe extern "win64" fn frame_rgn(hdc: usize, rgn: usize, brush: usize, w: i32, h: i32) -> i32 {
    use crate::dlls::gui::Rect;
    let Some(r) = crate::dlls::gui::region_rect(rgn) else {
        return 0;
    };
    let (w, h) = (w.max(1), h.max(1));
    for side in [
        Rect {
            bottom: r.top + h,
            ..r
        },
        Rect {
            top: r.bottom - h,
            ..r
        },
        Rect {
            right: r.left + w,
            ..r
        },
        Rect {
            left: r.right - w,
            ..r
        },
    ] {
        fill_rect(hdc, side, brush);
    }
    1
}

/// Origine des pinceaux : Winlux ne décale pas les motifs, elle reste (0, 0).
unsafe extern "win64" fn get_brush_org_ex(_hdc: usize, pt: *mut [i32; 2]) -> i32 {
    if !pt.is_null() {
        *pt = [0, 0];
    }
    1
}

/// OffsetClipRgn : le découpage n'est pas encore appliqué au dessin ; la
/// zone reste un rectangle simple (SIMPLEREGION).
extern "win64" fn offset_clip_rgn(_hdc: usize, _x: i32, _y: i32) -> i32 {
    2
}

/// Couleurs des objets DC_PEN et DC_BRUSH, par DC (noir et blanc au départ).
fn dc_colors() -> &'static Mutex<HashMap<(usize, bool), u32>> {
    static C: OnceLock<Mutex<HashMap<(usize, bool), u32>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn swap_dc_color(hdc: usize, pen: bool, color: Option<u32>) -> u32 {
    let mut m = lock(dc_colors());
    let default = if pen { 0x0000_0000 } else { 0x00FF_FFFF };
    let old = m.get(&(hdc, pen)).copied().unwrap_or(default);
    if let Some(c) = color {
        m.insert((hdc, pen), c & 0x00FF_FFFF);
    }
    old
}

extern "win64" fn set_dc_pen_color(hdc: usize, color: u32) -> u32 {
    swap_dc_color(hdc, true, Some(color))
}

extern "win64" fn set_dc_brush_color(hdc: usize, color: u32) -> u32 {
    swap_dc_color(hdc, false, Some(color))
}

extern "win64" fn get_dc_pen_color(hdc: usize) -> u32 {
    swap_dc_color(hdc, true, None)
}

extern "win64" fn get_dc_brush_color(hdc: usize) -> u32 {
    swap_dc_color(hdc, false, None)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "Arc" => fn arc, "Pie" => fn pie, "Chord" => fn chord,
        "ArcTo" => fn arc_to, "AngleArc" => fn angle_arc, "CreatePenIndirect" => fn create_pen_indirect,
        "FillRgn" => fn fill_rgn, "PaintRgn" => fn paint_rgn, "FrameRgn" => fn frame_rgn,
        "GetBrushOrgEx" => fn get_brush_org_ex, "OffsetClipRgn" => fn offset_clip_rgn,
        "SetDCPenColor" => fn set_dc_pen_color, "SetDCBrushColor" => fn set_dc_brush_color,
        "GetDCPenColor" => fn get_dc_pen_color, "GetDCBrushColor" => fn get_dc_brush_color,
        "GetEnhMetaFileBits" => fn meta_fail, "SetEnhMetaFileBits" => fn meta_fail,
        "GetEnhMetaFileDescriptionW" => fn meta_fail, "GetEnhMetaFileDescriptionA" => fn meta_fail,
        "GetEnhMetaFilePaletteEntries" => fn meta_fail,
        "PolyPolygon" => fn poly_polygon, "PolyPolyline" => fn poly_polyline,
        "CreatePolygonRgn" => fn create_polygon_rgn, "CreatePolyPolygonRgn" => fn create_poly_polygon_rgn,
        "ExtCreateRegion" => fn ext_create_region, "GetRegionData" => fn get_region_data,
        "CreateBitmapIndirect" => fn create_bitmap_indirect, "CreateDIBitmap" => fn create_di_bitmap,
        "GetDIBColorTable" => fn get_dib_color_table, "SetDIBColorTable" => fn set_dib_color_table, "MaskBlt" => fn mask_blt, "ExtFloodFill" => fn ext_flood_fill,
        "GetGraphicsMode" => fn get_graphics_mode, "SetGraphicsMode" => fn set_graphics_mode,
        "GetLayout" => fn get_layout, "SetLayout" => fn set_layout,
        "GetWorldTransform" => fn get_world_transform, "SetWorldTransform" => fn set_world_transform,
        "ModifyWorldTransform" => fn modify_world_transform,
        "CreateEnhMetaFileW" => fn meta_fail, "CreateEnhMetaFileA" => fn meta_fail,
        "CloseEnhMetaFile" => fn meta_fail, "CopyEnhMetaFileW" => fn meta_fail,
        "GetEnhMetaFileW" => fn meta_fail, "GetEnhMetaFileHeader" => fn meta_fail,
        "PlayEnhMetaFile" => fn meta_fail, "GetWinMetaFileBits" => fn meta_fail,
        "SetWinMetaFileBits" => fn meta_fail, "SetMetaFileBitsEx" => fn meta_fail,
        "GetMetaFileBitsEx" => fn meta_fail, "DeleteEnhMetaFile" => fn meta_ok, "DeleteMetaFile" => fn meta_ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DIB 16x16 en 1 bit par pixel avec sa palette (masques des listes
    /// d'images de la VCL, WinSCP x86 #905), puis 4, 8 et 16 bits.
    #[test]
    fn dib_section_a_palette_et_16_bits() {
        use crate::dlls::gdi32;
        use crate::dlls::gui::{self, Rect};
        unsafe {
            // BITMAPINFOHEADER (40) + 2 RGBQUAD : noir, blanc.
            let mut info = [0u8; 48];
            info[0] = 40;
            info[4] = 16; // largeur
            info[8..12].copy_from_slice(&(-16i32).to_le_bytes()); // haut-bas
            info[12] = 1; // plans
            info[14] = 1; // bits par pixel
            info[44..48].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0]);
            let mut bits = std::ptr::null_mut();
            let bmp = gdi32::create_dib_section(0, info.as_ptr().cast(), 0, &mut bits, 0, 0);
            assert!(bmp != 0 && !bits.is_null());
            let dc = gdi32::create_compatible_dc(0);
            gdi32::select_object(dc, bmp);
            gui::record(
                dc,
                gui::DrawCmd::FillRect {
                    rect: Rect::from_size(3, 0, 1, 1),
                    color: 0x00FF_FFFF,
                },
            );
            // Pixel (3, 0) blanc : bit 4 (de poids fort) du premier octet.
            assert_eq!(*(bits as *const u8), 0b0001_0000);
            assert_eq!(gdi32::get_pixel(dc, 3, 0), 0x00FF_FFFF);
            assert_eq!(gdi32::get_pixel(dc, 4, 0), 0);
            // Le programme écrit lui-même dans le tampon : pixel (0, 1).
            *(bits as *mut u8).add(4) = 0x80;
            assert_eq!(gdi32::get_pixel(dc, 0, 1), 0x00FF_FFFF);
            let mut table = [0u32; 4];
            assert_eq!(get_dib_color_table(dc, 0, 4, table.as_mut_ptr()), 2);
            assert_eq!(table[..2], [0, 0x00FF_FFFF]);
            // Palette changée : le blanc devient rouge (RGBQUAD 0x00FF0000).
            assert_eq!(set_dib_color_table(dc, 1, 1, [0x00FF_0000u32].as_ptr()), 1);
            assert_eq!(gdi32::get_pixel(dc, 3, 0), 0x0000_00FF);
            gdi32::delete_dc(dc);
            gdi32::delete_object(bmp);
            // 4, 8 et 16 bits sont acceptés aussi.
            for depth in [4u8, 8, 16] {
                let mut big = vec![0u8; 40 + 256 * 4];
                big[..48].copy_from_slice(&info);
                big[14] = depth;
                let mut b = std::ptr::null_mut();
                let h = gdi32::create_dib_section(0, big.as_ptr().cast(), 0, &mut b, 0, 0);
                assert!(h != 0, "{depth} bits");
                gdi32::delete_object(h);
            }
            // 16 bits 5-5-5 : un pixel rouge s'écrit 0x7C00.
            let mut i16 = info;
            i16[14] = 16;
            let mut b = std::ptr::null_mut();
            let h = gdi32::create_dib_section(0, i16.as_ptr().cast(), 0, &mut b, 0, 0);
            let dc = gdi32::create_compatible_dc(0);
            gdi32::select_object(dc, h);
            gui::record(
                dc,
                gui::DrawCmd::FillRect {
                    rect: Rect::from_size(0, 0, 1, 1),
                    color: 0x0000_00FF,
                },
            );
            assert_eq!(*(b as *const u16), 0x7C00);
            assert_eq!(gdi32::get_pixel(dc, 0, 0), 0x0000_00FF);
            gdi32::delete_dc(dc);
            gdi32::delete_object(h);
        }
    }

    #[test]
    fn angle_arc_balaye_dans_le_sens_trigonometrique() {
        let p = angle_arc_points(10, 10, 10, 0.0, 90.0);
        assert_eq!(p.first(), Some(&[20, 10]));
        assert_eq!(p.last(), Some(&[10, 0]));
        assert_eq!(swap_dc_color(0x7777_0002, true, Some(0x123456)), 0);
        assert_eq!(swap_dc_color(0x7777_0002, true, None), 0x123456);
        assert_eq!(swap_dc_color(0x7777_0002, false, None), 0x00FF_FFFF);
    }

    #[test]
    fn arc_d_un_quart_de_cercle() {
        // Cercle de rayon 10 centré en (10,10), de l'est au nord.
        let p = arc_points(0, 0, 20, 20, 20, 10, 10, 0);
        assert_eq!(p.first(), Some(&[20, 10]));
        assert_eq!(p.last(), Some(&[10, 0]));
        assert!(p.iter().all(|q| q[0] >= 10 && q[1] <= 10));
    }

    #[test]
    fn transformations_composees() {
        let t = [1.0, 0.0, 0.0, 1.0, 5.0, 0.0];
        let s = [2.0, 0.0, 0.0, 2.0, 0.0, 0.0];
        // Translation puis échelle : (5,0) → (10,0).
        assert_eq!(mul(t, s), [2.0, 0.0, 0.0, 2.0, 10.0, 0.0]);
        let hdc = 0x7777_0001;
        unsafe {
            assert_eq!(set_graphics_mode(hdc, 2), 1);
            assert_eq!(modify_world_transform(hdc, &t, 2), 1);
            let mut out = [0f32; 6];
            get_world_transform(hdc, &mut out);
            assert_eq!(out, t);
            assert_eq!(modify_world_transform(hdc, std::ptr::null(), 1), 1);
            get_world_transform(hdc, &mut out);
            assert_eq!(out, IDENTITY);
        }
    }

    #[test]
    fn maskblt_ne_copie_que_les_pixels_du_masque() {
        type Dc = unsafe extern "win64" fn(usize) -> usize;
        type Sel = unsafe extern "win64" fn(usize, usize) -> usize;
        type Bmp = unsafe extern "win64" fn(i32, i32, u32, u32, *const u8) -> usize;
        let f = |name: &str| crate::dlls::lookup("gdi32", name).unwrap();
        unsafe {
            let dc: Dc = std::mem::transmute(f("CreateCompatibleDC"));
            let select: Sel = std::mem::transmute(f("SelectObject"));
            let create_bitmap: Bmp = std::mem::transmute(f("CreateBitmap"));
            // Masque 2×1 : premier pixel opaque (1), second transparent (0).
            // Lignes alignées sur 16 bits.
            let mask = create_bitmap(2, 1, 1, 1, [0x80u8, 0].as_ptr());
            assert_eq!(
                super::super::gui::bitmap_pixels(mask).unwrap().2,
                vec![0x00FF_FFFF, 0]
            );
            let src_bmp = super::super::gui::bitmap_from_pixels(2, 1, &[0x0000_00FF; 2]).unwrap();
            let dst_bmp = super::super::gui::bitmap_from_pixels(2, 1, &[0x00FF_0000; 2]).unwrap();
            let (src, dst) = (dc(0), dc(0));
            select(src, src_bmp);
            select(dst, dst_bmp);
            // MAKEROP4(SRCCOPY, DSTCOPY), comme wxWidgets.
            assert_eq!(
                mask_blt(dst, 0, 0, 2, 1, src, 0, 0, mask, 0, 0, 0xAACC_0020),
                1
            );
            let px = super::super::gui::bitmap_pixels(dst_bmp).unwrap().2;
            assert_eq!(px[0] & 0x00FF_FFFF, 0x0000_00FF);
            assert_eq!(px[1] & 0x00FF_FFFF, 0x00FF_0000);
        }
    }

    #[test]
    fn masque_monochrome_tire_de_la_couleur_de_fond() {
        // #1121 : wxWidgets fabrique le masque d'une bitmap en la copiant
        // dans une bitmap 1 bit avec NOTSRCCOPY : la couleur de fond du DC
        // source (ici un PALETTERGB, comme Audacity) devient transparente.
        type Dc = unsafe extern "win64" fn(usize) -> usize;
        type Sel = unsafe extern "win64" fn(usize, usize) -> usize;
        type Bmp = unsafe extern "win64" fn(i32, i32, u32, u32, *const u8) -> usize;
        type Bk = unsafe extern "win64" fn(usize, u32) -> u32;
        type Blt = unsafe extern "win64" fn(usize, i32, i32, i32, i32, usize, i32, i32, u32) -> i32;
        let f = |name: &str| crate::dlls::lookup("gdi32", name).unwrap();
        unsafe {
            let dc: Dc = std::mem::transmute(f("CreateCompatibleDC"));
            let select: Sel = std::mem::transmute(f("SelectObject"));
            let create_bitmap: Bmp = std::mem::transmute(f("CreateBitmap"));
            let set_bk: Bk = std::mem::transmute(f("SetBkColor"));
            let bit_blt: Blt = std::mem::transmute(f("BitBlt"));
            let gui = |h| super::super::gui::bitmap_pixels(h).unwrap().2;
            let fond = 0x00F9_F8F8;
            let image =
                super::super::gui::bitmap_from_pixels(3, 1, &[fond, 0x0000_0000, fond]).unwrap();
            let mask = create_bitmap(3, 1, 1, 1, std::ptr::null());
            let (src, mdc) = (dc(0), dc(0));
            select(src, image);
            select(mdc, mask);
            set_bk(src, 0x0200_0000 | fond);
            assert_eq!(bit_blt(mdc, 0, 0, 3, 1, src, 0, 0, 0x0033_0008), 1);
            let m: Vec<u32> = gui(mask).iter().map(|p| p & 0x00FF_FFFF).collect();
            assert_eq!(m, vec![0, 0x00FF_FFFF, 0], "seul le trait est opaque");
            select(mdc, super::super::gui::DEFAULT_BITMAP);
            let dst_bmp = super::super::gui::bitmap_from_pixels(3, 1, &[0x00AB_CDEF; 3]).unwrap();
            let dst = dc(0);
            select(dst, dst_bmp);
            assert_eq!(
                mask_blt(dst, 0, 0, 3, 1, src, 0, 0, mask, 0, 0, 0xAACC_0020),
                1
            );
            let px: Vec<u32> = gui(dst_bmp).iter().map(|p| p & 0x00FF_FFFF).collect();
            assert_eq!(
                px,
                vec![0x00AB_CDEF, 0, 0x00AB_CDEF],
                "le fond reste visible"
            );
        }
    }

    #[test]
    fn rectangle_sans_plume_commence_au_coin_haut_gauche() {
        // #1239 : sans plume, Rectangle remplit de (left, top) à
        // (right - 1, bottom - 1) exclus ; rogner aussi le haut et la gauche
        // laissait le noir d'une bitmap neuve au bord des vumètres d'Audacity.
        type Dc = unsafe extern "win64" fn(usize) -> usize;
        type Sel = unsafe extern "win64" fn(usize, usize) -> usize;
        type Stock = unsafe extern "win64" fn(i32) -> usize;
        type Brush = unsafe extern "win64" fn(u32) -> usize;
        type Rect = unsafe extern "win64" fn(usize, i32, i32, i32, i32) -> i32;
        let f = |name: &str| crate::dlls::lookup("gdi32", name).unwrap();
        unsafe {
            let dc: Dc = std::mem::transmute(f("CreateCompatibleDC"));
            let select: Sel = std::mem::transmute(f("SelectObject"));
            let stock: Stock = std::mem::transmute(f("GetStockObject"));
            let brush: Brush = std::mem::transmute(f("CreateSolidBrush"));
            let rectangle: Rect = std::mem::transmute(f("Rectangle"));
            let bmp = super::super::gui::bitmap_from_pixels(4, 4, &[0; 16]).unwrap();
            let hdc = dc(0);
            select(hdc, bmp);
            select(hdc, stock(8)); // NULL_PEN
            select(hdc, brush(0x00FF_FFFF));
            assert_eq!(rectangle(hdc, 0, 0, 4, 4), 1);
            let px: Vec<u32> = super::super::gui::bitmap_pixels(bmp)
                .unwrap()
                .2
                .iter()
                .map(|p| p & 0x00FF_FFFF)
                .collect();
            let w = 0x00FF_FFFF;
            assert_eq!(
                px,
                vec![w, w, w, 0, w, w, w, 0, w, w, w, 0, 0, 0, 0, 0],
                "3x3 rempli depuis (0, 0)"
            );
        }
    }
}
