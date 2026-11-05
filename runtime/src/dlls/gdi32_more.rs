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
unsafe extern "win64" fn get_dib_color_table(
    _hdc: usize,
    _start: u32,
    _n: u32,
    _rgb: *mut u32,
) -> u32 {
    0
}

/// MaskBlt : sans masque, c'est un BitBlt de l'opération de premier plan.
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
    _mask: usize,
    _xm: i32,
    _ym: i32,
    rop: u32,
) -> i32 {
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

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "Arc" => fn arc, "Pie" => fn pie, "Chord" => fn chord,
        "PolyPolygon" => fn poly_polygon, "PolyPolyline" => fn poly_polyline,
        "CreatePolygonRgn" => fn create_polygon_rgn, "CreatePolyPolygonRgn" => fn create_poly_polygon_rgn,
        "ExtCreateRegion" => fn ext_create_region, "GetRegionData" => fn get_region_data,
        "CreateBitmapIndirect" => fn create_bitmap_indirect, "CreateDIBitmap" => fn create_di_bitmap,
        "GetDIBColorTable" => fn get_dib_color_table, "MaskBlt" => fn mask_blt, "ExtFloodFill" => fn ext_flood_fill,
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
}
