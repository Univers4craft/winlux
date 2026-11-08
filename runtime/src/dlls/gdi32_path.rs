//! gdi32 : chemins (BeginPath … StrokePath/FillPath), ArcTo, AngleArc et
//! GetFontData (#14).
//!
//! Entre BeginPath et EndPath, MoveToEx, LineTo, Polyline(To), PolyBezier(To),
//! Polygon, Rectangle, Ellipse, ArcTo et AngleArc ne dessinent pas : ils
//! ajoutent leurs points au chemin du DC, comme sous Windows. Les courbes sont
//! aplaties à l'enregistrement (GetPath rend donc ce que rendrait FlattenPath).
//! Remplissage : chaque figure est remplie séparément (les trous d'une figure
//! dans une autre ne sont pas creusés) ; une région de chemin prend son
//! rectangle englobant, comme toutes les régions de Winlux.

use super::lock;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const PT_CLOSEFIGURE: u8 = 1;
const PT_LINETO: u8 = 2;
const PT_MOVETO: u8 = 6;
const GDI_ERROR: u32 = 0xFFFF_FFFF;

#[derive(Default)]
struct Path {
    /// Ouvert (entre BeginPath et EndPath) : les tracés y sont ajoutés.
    open: bool,
    pts: Vec<([i32; 2], u8)>,
    /// Position courante pendant l'enregistrement.
    pos: [i32; 2],
    /// Indice du MOVETO de la figure en cours, s'il y en a une.
    figure: Option<usize>,
}

impl Path {
    fn move_to(&mut self, p: [i32; 2]) {
        self.pos = p;
        self.figure = None;
    }
    fn line_to(&mut self, p: [i32; 2]) {
        if self.figure.is_none() {
            self.figure = Some(self.pts.len());
            self.pts.push((self.pos, PT_MOVETO));
        }
        self.pts.push((p, PT_LINETO));
        self.pos = p;
    }
    fn close(&mut self) {
        if let Some(start) = self.figure.take() {
            if let Some(last) = self.pts.last_mut() {
                last.1 |= PT_CLOSEFIGURE;
            }
            self.pos = self.pts[start].0;
        }
    }
    fn closed_figure(&mut self, pts: &[[i32; 2]]) {
        if pts.is_empty() {
            return;
        }
        self.move_to(pts[0]);
        for &p in &pts[1..] {
            self.line_to(p);
        }
        if pts.len() == 1 {
            self.line_to(pts[0]);
        }
        self.close();
    }
    /// Figures : (points, fermée).
    fn figures(&self) -> Vec<(Vec<[i32; 2]>, bool)> {
        let mut out: Vec<(Vec<[i32; 2]>, bool)> = Vec::new();
        for &(p, t) in &self.pts {
            if t & !PT_CLOSEFIGURE == PT_MOVETO || out.is_empty() {
                out.push((vec![p], false));
            } else if let Some(last) = out.last_mut() {
                last.0.push(p);
            }
            if t & PT_CLOSEFIGURE != 0 {
                if let Some(last) = out.last_mut() {
                    last.1 = true;
                }
            }
        }
        out
    }
}

fn paths() -> &'static Mutex<HashMap<usize, Path>> {
    static P: OnceLock<Mutex<HashMap<usize, Path>>> = OnceLock::new();
    P.get_or_init(Default::default)
}

/// Adresses des fonctions de dessin d'origine, remplacées par `hook`.
fn originals() -> &'static Mutex<HashMap<&'static str, usize>> {
    static O: OnceLock<Mutex<HashMap<&'static str, usize>>> = OnceLock::new();
    O.get_or_init(Default::default)
}

fn original(name: &str) -> Option<usize> {
    lock(originals()).get(name).copied()
}

macro_rules! call {
    ($name:literal, fn($($t:ty),*) -> $r:ty $(, $arg:expr)* $(,)?) => {{
        match original($name).or_else(|| super::lookup("gdi32", $name)) {
            Some(addr) => {
                // SAFETY: adresse d'une fonction gdi32 de Winlux, signature documentée.
                let f: unsafe extern "win64" fn($($t),*) -> $r = std::mem::transmute(addr);
                Some(f($($arg),*))
            }
            None => None,
        }
    }};
}

/// Exécute `f` sur le chemin ouvert du DC ; `None` si aucun n'est ouvert
/// (le tracé part alors à la fonction d'origine).
fn recording<R>(hdc: usize, f: impl FnOnce(&mut Path) -> R) -> Option<R> {
    let mut all = lock(paths());
    let path = all.get_mut(&hdc).filter(|p| p.open)?;
    Some(f(path))
}

unsafe fn points<'a>(pts: *const [i32; 2], n: usize) -> &'a [[i32; 2]] {
    if pts.is_null() || n == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(pts, n)
    }
}

fn bezier(p0: [i32; 2], p1: [i32; 2], p2: [i32; 2], p3: [i32; 2]) -> Vec<[i32; 2]> {
    let len =
        |a: [i32; 2], b: [i32; 2]| (((a[0] - b[0]).pow(2) + (a[1] - b[1]).pow(2)) as f64).sqrt();
    let steps = ((len(p0, p1) + len(p1, p2) + len(p2, p3)) / 4.0).clamp(4.0, 128.0) as usize;
    (1..=steps)
        .map(|i| {
            let t = i as f64 / steps as f64;
            let u = 1.0 - t;
            let c = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
            let axis = |k: usize| {
                (c[0] * p0[k] as f64
                    + c[1] * p1[k] as f64
                    + c[2] * p2[k] as f64
                    + c[3] * p3[k] as f64)
                    .round() as i32
            };
            [axis(0), axis(1)]
        })
        .collect()
}

/// Points d'un arc d'ellipse (boîte l,t,r,b), du rayon (xs,ys) au rayon
/// (xe,ye), dans le sens antihoraire (sens par défaut de GDI).
#[allow(clippy::too_many_arguments)]
fn arc_points(l: i32, t: i32, r: i32, b: i32, xs: i32, ys: i32, xe: i32, ye: i32) -> Vec<[i32; 2]> {
    let (cx, cy) = ((l + r) as f64 / 2.0, (t + b) as f64 / 2.0);
    let (rx, ry) = (((r - l) as f64 / 2.0).abs(), ((b - t) as f64 / 2.0).abs());
    let a0 = (-(ys as f64 - cy)).atan2(xs as f64 - cx);
    let mut a1 = (-(ye as f64 - cy)).atan2(xe as f64 - cx);
    if a1 <= a0 {
        a1 += std::f64::consts::TAU;
    }
    ellipse_points(cx, cy, rx, ry, a0, a1)
}

fn ellipse_points(cx: f64, cy: f64, rx: f64, ry: f64, a0: f64, a1: f64) -> Vec<[i32; 2]> {
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

/// Remplissage pair-impair (ALTERNATE) de plusieurs contours avec la
/// couleur du pinceau du DC, par segments horizontaux d'un pixel de haut :
/// les trous d'une figure dans une autre restent vides, et les bords droit
/// et bas sont exclus comme pour tout remplissage GDI.
unsafe fn fill_polys(hdc: usize, figures: &[Vec<[i32; 2]>]) {
    let Some(color) = super::gui::object_color(super::gui::dc_brush(hdc)) else {
        return; // NULL_BRUSH
    };
    let ys = figures.iter().flatten().map(|p| p[1]);
    let (Some(top), Some(bottom)) = (ys.clone().min(), ys.max()) else {
        return;
    };
    let mut xs: Vec<f64> = Vec::new();
    for y in top..bottom {
        let sy = y as f64 + 0.5;
        xs.clear();
        for fig in figures.iter().filter(|f| f.len() >= 3) {
            for (i, a) in fig.iter().enumerate() {
                let b = fig[(i + 1) % fig.len()];
                let (ay, by) = (a[1] as f64, b[1] as f64);
                if (ay <= sy) != (by <= sy) {
                    xs.push(a[0] as f64 + (sy - ay) * (b[0] - a[0]) as f64 / (by - ay));
                }
            }
        }
        xs.sort_by(|a, b| a.total_cmp(b));
        for pair in xs.as_chunks::<2>().0 {
            let (left, right) = ((pair[0] - 0.5).ceil() as i32, (pair[1] - 0.5).ceil() as i32);
            if right > left {
                let rect = super::gui::Rect {
                    left,
                    top: y,
                    right,
                    bottom: y + 1,
                };
                super::gui::record(hdc, super::gui::DrawCmd::FillRect { rect, color });
            }
        }
    }
}

// --- Tracés interceptés -------------------------------------------------------------------

unsafe extern "win64" fn move_to_ex(hdc: usize, x: i32, y: i32, old: *mut [i32; 2]) -> i32 {
    let recorded = recording(hdc, |p| {
        let before = p.pos;
        p.move_to([x, y]);
        before
    });
    match recorded {
        Some(before) => {
            if !old.is_null() {
                *old = before;
            }
            1
        }
        None => call!(
            "MoveToEx",
            fn(usize, i32, i32, *mut [i32; 2]) -> i32,
            hdc,
            x,
            y,
            old
        )
        .unwrap_or(0),
    }
}

unsafe extern "win64" fn line_to(hdc: usize, x: i32, y: i32) -> i32 {
    if recording(hdc, |p| p.line_to([x, y])).is_some() {
        return 1;
    }
    call!("LineTo", fn(usize, i32, i32) -> i32, hdc, x, y).unwrap_or(0)
}

unsafe extern "win64" fn polyline(hdc: usize, pts: *const [i32; 2], n: i32) -> i32 {
    let list = points(pts, n.max(0) as usize);
    let done = recording(hdc, |p| {
        let pos = p.pos;
        if let Some((&first, rest)) = list.split_first() {
            p.move_to(first);
            rest.iter().for_each(|&q| p.line_to(q));
        }
        // Polyline ne déplace pas la position courante.
        p.figure = None;
        p.pos = pos;
    });
    if done.is_some() {
        return 1;
    }
    call!(
        "Polyline",
        fn(usize, *const [i32; 2], i32) -> i32,
        hdc,
        pts,
        n
    )
    .unwrap_or(0)
}

unsafe extern "win64" fn polyline_to(hdc: usize, pts: *const [i32; 2], n: u32) -> i32 {
    let list = points(pts, n as usize);
    if recording(hdc, |p| list.iter().for_each(|&q| p.line_to(q))).is_some() {
        return 1;
    }
    // Le PolylineTo d'origine ne traçait rien : une ligne par point.
    for q in list {
        line_to(hdc, q[0], q[1]);
    }
    1
}

fn record_beziers(p: &mut Path, start: [i32; 2], list: &[[i32; 2]]) {
    let mut from = start;
    for c in list.as_chunks::<3>().0 {
        for q in bezier(from, c[0], c[1], c[2]) {
            p.line_to(q);
        }
        from = c[2];
    }
}

unsafe extern "win64" fn poly_bezier(hdc: usize, pts: *const [i32; 2], n: u32) -> i32 {
    let list = points(pts, n as usize);
    let done = recording(hdc, |p| {
        if let Some((&first, rest)) = list.split_first() {
            let pos = p.pos;
            p.move_to(first);
            record_beziers(p, first, rest);
            p.figure = None;
            p.pos = pos;
        }
    });
    if done.is_some() {
        return 1;
    }
    // Le PolyBezier d'origine ne traçait rien : courbes aplaties en Polyline.
    if let Some((&first, rest)) = list.split_first() {
        let mut flat = vec![first];
        let mut from = first;
        for c in rest.as_chunks::<3>().0 {
            flat.extend(bezier(from, c[0], c[1], c[2]));
            from = c[2];
        }
        polyline(hdc, flat.as_ptr(), flat.len() as i32);
    }
    1
}

unsafe extern "win64" fn poly_bezier_to(hdc: usize, pts: *const [i32; 2], n: u32) -> i32 {
    let list = points(pts, n as usize);
    if recording(hdc, |p| record_beziers(p, p.pos, list)).is_some() {
        return 1;
    }
    // Le PolyBezierTo d'origine ne traçait rien : courbes aplaties en lignes.
    let mut from = [0i32; 2];
    call!(
        "GetCurrentPositionEx",
        fn(usize, *mut [i32; 2]) -> i32,
        hdc,
        &mut from
    );
    for c in list.as_chunks::<3>().0 {
        for q in bezier(from, c[0], c[1], c[2]) {
            line_to(hdc, q[0], q[1]);
        }
        from = c[2];
    }
    1
}

unsafe extern "win64" fn polygon(hdc: usize, pts: *const [i32; 2], n: i32) -> i32 {
    let list = points(pts, n.max(0) as usize);
    if recording(hdc, |p| p.closed_figure(list)).is_some() {
        return 1;
    }
    // Le Polygon d'origine ne trace que le contour : l'intérieur prend le
    // pinceau, comme sous Windows.
    if list.len() >= 3 {
        fill_polys(hdc, &[list.to_vec()]);
    }
    call!(
        "Polygon",
        fn(usize, *const [i32; 2], i32) -> i32,
        hdc,
        pts,
        n
    )
    .unwrap_or(0)
}

unsafe extern "win64" fn rectangle(hdc: usize, l: i32, t: i32, r: i32, b: i32) -> i32 {
    let corners = [[l, t], [r - 1, t], [r - 1, b - 1], [l, b - 1]];
    if recording(hdc, |p| p.closed_figure(&corners)).is_some() {
        return 1;
    }
    call!(
        "Rectangle",
        fn(usize, i32, i32, i32, i32) -> i32,
        hdc,
        l,
        t,
        r,
        b
    )
    .unwrap_or(0)
}

unsafe extern "win64" fn ellipse(hdc: usize, l: i32, t: i32, r: i32, b: i32) -> i32 {
    let done = recording(hdc, |p| {
        let (cx, cy) = ((l + r) as f64 / 2.0, (t + b) as f64 / 2.0);
        let (rx, ry) = (((r - l) as f64 / 2.0).abs(), ((b - t) as f64 / 2.0).abs());
        let mut pts = ellipse_points(cx, cy, rx, ry, 0.0, std::f64::consts::TAU);
        pts.pop();
        p.closed_figure(&pts);
    });
    if done.is_some() {
        return 1;
    }
    call!(
        "Ellipse",
        fn(usize, i32, i32, i32, i32) -> i32,
        hdc,
        l,
        t,
        r,
        b
    )
    .unwrap_or(0)
}

/// Trace (ou enregistre) une ligne jusqu'au début de l'arc, puis l'arc ; la
/// position courante finit au bout de l'arc.
unsafe fn arc_to_points(hdc: usize, pts: &[[i32; 2]]) -> i32 {
    if pts.is_empty() {
        return 0;
    }
    if recording(hdc, |p| pts.iter().for_each(|&q| p.line_to(q))).is_some() {
        return 1;
    }
    line_to(hdc, pts[0][0], pts[0][1]);
    polyline_to(hdc, pts.as_ptr(), pts.len() as u32)
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
    arc_to_points(hdc, &arc_points(l, t, r, b, xs, ys, xe, ye))
}

/// AngleArc : centre, rayon, angle de départ et balayage en degrés
/// (sens antihoraire pour un balayage positif).
unsafe extern "win64" fn angle_arc(
    hdc: usize,
    x: i32,
    y: i32,
    radius: u32,
    start: f32,
    sweep: f32,
) -> i32 {
    let a0 = (start as f64).to_radians();
    let a1 = a0 + (sweep as f64).to_radians();
    let r = radius as f64;
    arc_to_points(hdc, &ellipse_points(x as f64, y as f64, r, r, a0, a1))
}

// --- Gestion du chemin --------------------------------------------------------------------

unsafe extern "win64" fn begin_path(hdc: usize) -> i32 {
    if hdc == 0 {
        return 0;
    }
    let mut pos = [0i32; 2];
    call!(
        "GetCurrentPositionEx",
        fn(usize, *mut [i32; 2]) -> i32,
        hdc,
        &mut pos
    );
    lock(paths()).insert(
        hdc,
        Path {
            open: true,
            pos,
            ..Path::default()
        },
    );
    1
}

unsafe extern "win64" fn end_path(hdc: usize) -> i32 {
    let pos = {
        let mut all = lock(paths());
        let Some(p) = all.get_mut(&hdc).filter(|p| p.open) else {
            return 0;
        };
        p.open = false;
        p.pos
    };
    call!(
        "MoveToEx",
        fn(usize, i32, i32, *mut [i32; 2]) -> i32,
        hdc,
        pos[0],
        pos[1],
        std::ptr::null_mut()
    );
    1
}

unsafe extern "win64" fn abort_path(hdc: usize) -> i32 {
    lock(paths()).remove(&hdc);
    1
}

unsafe extern "win64" fn close_figure(hdc: usize) -> i32 {
    recording(hdc, |p| p.close()).map_or(0, |_| 1)
}

/// Chemin fermé (après EndPath), retiré du DC : les fonctions qui le
/// consomment (StrokePath, FillPath…) le détruisent comme sous Windows.
fn take_closed(hdc: usize) -> Option<Path> {
    let mut all = lock(paths());
    if all.get(&hdc).is_some_and(|p| !p.open) {
        all.remove(&hdc)
    } else {
        None
    }
}

unsafe fn stroke(hdc: usize, path: &Path) {
    for (mut pts, closed) in path.figures() {
        if closed {
            pts.push(pts[0]);
        }
        call!(
            "Polyline",
            fn(usize, *const [i32; 2], i32) -> i32,
            hdc,
            pts.as_ptr(),
            pts.len() as i32
        );
    }
}

unsafe fn fill(hdc: usize, path: &Path) {
    let figures: Vec<Vec<[i32; 2]>> = path.figures().into_iter().map(|(pts, _)| pts).collect();
    fill_polys(hdc, &figures);
}

unsafe extern "win64" fn stroke_path(hdc: usize) -> i32 {
    let Some(path) = take_closed(hdc) else {
        return 0;
    };
    stroke(hdc, &path);
    1
}

unsafe extern "win64" fn fill_path(hdc: usize) -> i32 {
    let Some(path) = take_closed(hdc) else {
        return 0;
    };
    fill(hdc, &path);
    1
}

unsafe extern "win64" fn stroke_and_fill_path(hdc: usize) -> i32 {
    let Some(path) = take_closed(hdc) else {
        return 0;
    };
    fill(hdc, &path);
    stroke(hdc, &path);
    1
}

unsafe fn region_of(path: &Path) -> usize {
    let pts: Vec<[i32; 2]> = path.pts.iter().map(|&(p, _)| p).collect();
    if pts.is_empty() {
        return 0;
    }
    call!(
        "CreatePolygonRgn",
        fn(*const [i32; 2], i32, i32) -> usize,
        pts.as_ptr(),
        pts.len() as i32,
        1
    )
    .unwrap_or(0)
}

unsafe extern "win64" fn path_to_region(hdc: usize) -> usize {
    let Some(path) = take_closed(hdc) else {
        return 0;
    };
    region_of(&path)
}

unsafe extern "win64" fn select_clip_path(hdc: usize, mode: i32) -> i32 {
    let Some(path) = take_closed(hdc) else {
        return 0;
    };
    let rgn = region_of(&path);
    if rgn == 0 {
        return 0;
    }
    let r = call!(
        "ExtSelectClipRgn",
        fn(usize, usize, i32) -> i32,
        hdc,
        rgn,
        mode
    )
    .unwrap_or(0);
    call!("DeleteObject", fn(usize) -> i32, rgn);
    (r != 0) as i32
}

unsafe extern "win64" fn flatten_path(hdc: usize) -> i32 {
    // Les courbes sont déjà aplaties à l'enregistrement.
    lock(paths()).get(&hdc).is_some_and(|p| !p.open) as i32
}

unsafe extern "win64" fn widen_path(hdc: usize) -> i32 {
    // Le contour n'est pas élargi à l'épaisseur du stylo : le chemin reste
    // valable pour StrokePath et FillPath.
    flatten_path(hdc)
}

/// GetPath : nombre de points si `n` vaut 0, sinon copie points et types.
unsafe extern "win64" fn get_path(hdc: usize, pts: *mut [i32; 2], types: *mut u8, n: i32) -> i32 {
    let all = lock(paths());
    let Some(path) = all.get(&hdc).filter(|p| !p.open) else {
        super::kernel32::set_last_error(1); // ERROR_CAN_NOT_COMPLETE
        return -1;
    };
    let count = path.pts.len() as i32;
    if n == 0 {
        return count;
    }
    if n < count || pts.is_null() || types.is_null() {
        super::kernel32::set_last_error(87); // ERROR_INVALID_PARAMETER
        return -1;
    }
    for (i, &(p, t)) in path.pts.iter().enumerate() {
        *pts.add(i) = p;
        *types.add(i) = t;
    }
    count
}

static MITER: Mutex<Vec<(usize, f32)>> = Mutex::new(Vec::new());

unsafe extern "win64" fn get_miter_limit(hdc: usize, out: *mut f32) -> i32 {
    if !out.is_null() {
        *out = lock(&MITER)
            .iter()
            .find(|e| e.0 == hdc)
            .map_or(10.0, |e| e.1);
    }
    1
}

unsafe extern "win64" fn set_miter_limit(hdc: usize, limit: f32, old: *mut f32) -> i32 {
    get_miter_limit(hdc, old);
    let mut all = lock(&MITER);
    all.retain(|e| e.0 != hdc);
    all.push((hdc, limit));
    1
}

/// PolyDraw : suite de points avec types PT_MOVETO / PT_LINETO /
/// PT_BEZIERTO (+ PT_CLOSEFIGURE).
unsafe extern "win64" fn poly_draw(
    hdc: usize,
    pts: *const [i32; 2],
    types: *const u8,
    n: i32,
) -> i32 {
    if pts.is_null() || types.is_null() || n <= 0 {
        return 0;
    }
    let list = points(pts, n as usize);
    let kinds = std::slice::from_raw_parts(types, n as usize);
    let mut i = 0;
    while i < list.len() {
        let t = kinds[i];
        match t & !PT_CLOSEFIGURE {
            PT_MOVETO => {
                move_to_ex(hdc, list[i][0], list[i][1], std::ptr::null_mut());
                i += 1;
            }
            PT_LINETO => {
                line_to(hdc, list[i][0], list[i][1]);
                i += 1;
            }
            4 if i + 3 <= list.len() => {
                poly_bezier_to(hdc, list[i..].as_ptr(), 3);
                i += 3;
            }
            _ => return 0,
        }
        if kinds[i - 1] & PT_CLOSEFIGURE != 0 && recording(hdc, |p| p.close()).is_none() {
            // Hors chemin : refermer la figure par une ligne.
            let start = list[..i]
                .iter()
                .zip(kinds)
                .rev()
                .find(|(_, &k)| k & !PT_CLOSEFIGURE == PT_MOVETO)
                .map(|(p, _)| *p);
            if let Some(s) = start {
                line_to(hdc, s[0], s[1]);
            }
        }
    }
    1
}

// --- GetFontData --------------------------------------------------------------------------

/// Octets d'une table (étiquette en ordre des octets du fichier, comme
/// `dwTable`) d'une police TrueType/OpenType, collection comprise.
fn font_table(data: &[u8], index: u32, tag: u32) -> Option<&[u8]> {
    let be32 = |o: usize| {
        data.get(o..o + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    let be16 = |o: usize| data.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let start = if data.starts_with(b"ttcf") {
        be32(12 + 4 * index as usize)? as usize
    } else {
        0
    };
    let tables = be16(start + 4)? as usize;
    let wanted = tag.to_le_bytes();
    for k in 0..tables {
        let rec = start + 12 + 16 * k;
        if data.get(rec..rec + 4)? == wanted {
            let offset = be32(rec + 8)? as usize;
            let length = be32(rec + 12)? as usize;
            return data.get(offset..offset.checked_add(length)?);
        }
    }
    None
}

unsafe extern "win64" fn get_font_data(
    hdc: usize,
    table: u32,
    offset: u32,
    buf: *mut u8,
    size: u32,
) -> u32 {
    let spec = super::gui::font_spec(super::gui::dc_font(hdc)).unwrap_or_default();
    let face = match spec.face.as_str() {
        "" | "fixed" | "System" | "MS Shell Dlg" | "MS Shell Dlg 2" => "Segoe UI",
        f => f,
    };
    let weight = if spec.weight <= 0 {
        400
    } else {
        spec.weight as u32
    };
    let Some(info) = super::dwrite::gdi_face(face, weight, spec.italic) else {
        return GDI_ERROR;
    };
    let Ok(data) = std::fs::read(&info.path) else {
        return GDI_ERROR;
    };
    // 0 : tout le fichier ; « ttcf » : la collection entière.
    let bytes: &[u8] = if table == 0 || table == u32::from_le_bytes(*b"ttcf") {
        &data
    } else {
        match font_table(&data, info.index, table) {
            Some(b) => b,
            None => return GDI_ERROR,
        }
    };
    let offset = offset as usize;
    if offset > bytes.len() {
        return GDI_ERROR;
    }
    let rest = &bytes[offset..];
    if buf.is_null() || size == 0 {
        return rest.len() as u32;
    }
    let n = rest.len().min(size as usize);
    std::ptr::copy_nonoverlapping(rest.as_ptr(), buf, n);
    n as u32
}

/// Nouvelles fonctions de gdi32.
pub fn exports() -> super::Exports {
    crate::exports! {
        "BeginPath" => fn begin_path, "EndPath" => fn end_path, "AbortPath" => fn abort_path,
        "CloseFigure" => fn close_figure, "StrokePath" => fn stroke_path, "FillPath" => fn fill_path,
        "StrokeAndFillPath" => fn stroke_and_fill_path, "PathToRegion" => fn path_to_region,
        "SelectClipPath" => fn select_clip_path, "FlattenPath" => fn flatten_path,
        "WidenPath" => fn widen_path, "ArcTo" => fn arc_to, "AngleArc" => fn angle_arc,
        "PolyDraw" => fn poly_draw, "GetMiterLimit" => fn get_miter_limit,
        "SetMiterLimit" => fn set_miter_limit, "GetFontData" => fn get_font_data,
    }
}

/// Remplace les tracés de gdi32 par leur version qui enregistre dans le
/// chemin ouvert, et GetPath (qui échouait toujours). À appeler après toutes
/// les autres substitutions de gdi32.
pub fn hook(exports: &mut super::Exports) {
    let wrappers = crate::exports! {
        "MoveToEx" => fn move_to_ex, "LineTo" => fn line_to, "Polyline" => fn polyline,
        "PolylineTo" => fn polyline_to, "PolyBezier" => fn poly_bezier,
        "PolyBezierTo" => fn poly_bezier_to, "Polygon" => fn polygon,
        "Rectangle" => fn rectangle, "Ellipse" => fn ellipse,
    };
    let mut saved = lock(originals());
    for (name, addr) in wrappers {
        if let Some(&orig) = exports.get(name) {
            if orig != addr {
                saved.insert(name, orig);
            }
        }
        exports.insert(name, addr);
    }
    exports.insert("GetPath", get_path as *const () as usize);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chemin_figures_et_fermeture() {
        let mut p = Path {
            open: true,
            ..Path::default()
        };
        p.move_to([0, 0]);
        p.line_to([10, 0]);
        p.line_to([10, 10]);
        p.close();
        assert_eq!(p.pos, [0, 0]);
        p.closed_figure(&[[20, 20], [30, 20], [30, 30]]);
        let figs = p.figures();
        assert_eq!(figs.len(), 2);
        assert_eq!(figs[0], (vec![[0, 0], [10, 0], [10, 10]], true));
        assert_eq!(figs[1].0.len(), 3);
        assert_eq!(p.pts[0].1, PT_MOVETO);
        assert_eq!(p.pts[2].1, PT_LINETO | PT_CLOSEFIGURE);
    }

    #[test]
    fn bezier_finit_sur_le_dernier_point() {
        let pts = bezier([0, 0], [0, 100], [100, 100], [100, 0]);
        assert_eq!(*pts.last().unwrap(), [100, 0]);
        assert!(pts.iter().any(|q| q[1] > 50));
    }

    #[test]
    fn table_de_police_trouvee_par_etiquette() {
        // En-tête sfnt minimal : une table « head » de 4 octets.
        let mut f = vec![0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        f.extend_from_slice(b"head");
        f.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 28, 0, 0, 0, 4]);
        f.extend_from_slice(&[1, 2, 3, 4]);
        let tag = u32::from_le_bytes(*b"head");
        assert_eq!(font_table(&f, 0, tag), Some(&[1u8, 2, 3, 4][..]));
        assert_eq!(font_table(&f, 0, u32::from_le_bytes(*b"cmap")), None);
    }

    /// De bout en bout dans un DIB : Rectangle enregistré dans le chemin ne
    /// dessine rien, FillPath remplit, et le chemin est consommé.
    #[test]
    fn fill_path_remplit_un_dib() {
        unsafe {
            let f = |n: &str| super::super::lookup("gdi32", n).expect(n);
            let create_dc: unsafe extern "win64" fn(usize) -> usize =
                std::mem::transmute(f("CreateCompatibleDC"));
            type Dib =
                unsafe extern "win64" fn(usize, *const u8, u32, *mut *mut u32, usize, u32) -> usize;
            let create_dib: Dib = std::mem::transmute(f("CreateDIBSection"));
            let select: unsafe extern "win64" fn(usize, usize) -> usize =
                std::mem::transmute(f("SelectObject"));
            let stock: unsafe extern "win64" fn(i32) -> usize =
                std::mem::transmute(f("GetStockObject"));
            let rect: unsafe extern "win64" fn(usize, i32, i32, i32, i32) -> i32 =
                std::mem::transmute(f("Rectangle"));
            let one: unsafe extern "win64" fn(usize) -> i32 = std::mem::transmute(f("BeginPath"));
            let end: unsafe extern "win64" fn(usize) -> i32 = std::mem::transmute(f("EndPath"));
            let fillp: unsafe extern "win64" fn(usize) -> i32 = std::mem::transmute(f("FillPath"));
            let getp: unsafe extern "win64" fn(usize, *mut [i32; 2], *mut u8, i32) -> i32 =
                std::mem::transmute(f("GetPath"));
            let dc = create_dc(0);
            assert_ne!(dc, 0);
            let mut info = [0u8; 44];
            info[0] = 40;
            info[4..8].copy_from_slice(&20i32.to_le_bytes());
            info[8..12].copy_from_slice(&(-20i32).to_le_bytes());
            info[12] = 1;
            info[14] = 32;
            let mut bits: *mut u32 = std::ptr::null_mut();
            let bmp = create_dib(dc, info.as_ptr(), 0, &mut bits, 0, 0);
            assert!(bmp != 0 && !bits.is_null());
            select(dc, bmp);
            select(dc, stock(0)); // WHITE_BRUSH
            let px = |x: usize, y: usize| *bits.add(y * 20 + x) & 0xFF_FFFF;
            assert_eq!(one(dc), 1);
            rect(dc, 2, 2, 12, 12);
            assert_eq!(px(6, 6), 0, "rien n'est dessiné pendant l'enregistrement");
            assert_eq!(end(dc), 1);
            assert_eq!(getp(dc, std::ptr::null_mut(), std::ptr::null_mut(), 0), 4);
            assert_eq!(fillp(dc), 1);
            assert_eq!(px(6, 6), 0xFF_FFFF, "intérieur rempli");
            assert_eq!(px(16, 16), 0, "extérieur intact");
            assert_eq!(fillp(dc), 0, "chemin consommé");
        }
    }
}
