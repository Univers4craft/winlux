//! Rastériseur vectoriel maison : remplissage anti-crénelé (non-zéro ou
//! pair-impair) de polygones, contours épaissis, peintures unies et
//! dégradés. Sert au SVG et aux aperçus 3D.

pub type Pt = (f32, f32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat(pub [f32; 6]); // a b c d e f : x' = a x + c y + e

impl Mat {
    pub const ID: Mat = Mat([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    pub fn mul(&self, o: &Mat) -> Mat {
        // self * o (o appliqué d'abord)
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = o.0;
        Mat([
            a * a2 + c * b2,
            b * a2 + d * b2,
            a * c2 + c * d2,
            b * c2 + d * d2,
            a * e2 + c * f2 + e,
            b * e2 + d * f2 + f,
        ])
    }
    pub fn apply(&self, p: Pt) -> Pt {
        let [a, b, c, d, e, f] = self.0;
        (a * p.0 + c * p.1 + e, b * p.0 + d * p.1 + f)
    }
    pub fn inv(&self) -> Option<Mat> {
        let [a, b, c, d, e, f] = self.0;
        let det = a * d - b * c;
        if det.abs() < 1e-12 {
            return None;
        }
        let id = 1.0 / det;
        Some(Mat([
            d * id,
            -b * id,
            -c * id,
            a * id,
            (c * f - d * e) * id,
            (b * e - a * f) * id,
        ]))
    }
    pub fn scale_factor(&self) -> f32 {
        let [a, b, c, d, _, _] = self.0;
        (a * d - b * c).abs().sqrt()
    }
    pub fn translate(x: f32, y: f32) -> Mat {
        Mat([1.0, 0.0, 0.0, 1.0, x, y])
    }
    pub fn scale(x: f32, y: f32) -> Mat {
        Mat([x, 0.0, 0.0, y, 0.0, 0.0])
    }
}

/// Tampon RGBA prémultiplié en flottants.
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[f32; 4]>,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Canvas {
        Canvas {
            w,
            h,
            px: vec![[0.0; 4]; w * h],
        }
    }
    /// Couleur RGBA 8 bits non prémultipliée.
    pub fn to_rgba8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.w * self.h * 4);
        for p in &self.px {
            let a = p[3].clamp(0.0, 1.0);
            if a <= 0.0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                for c in &p[..3] {
                    out.push(((c / a).clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
                }
                out.push((a * 255.0 + 0.5) as u8);
            }
        }
        out
    }
}

#[derive(Clone, Debug)]
pub struct Stop {
    pub off: f32,
    pub color: [f32; 4], // non prémultiplié
}

#[derive(Clone, Debug)]
pub enum Paint {
    Solid([f32; 4]),
    /// (inverse de la matrice vers l'espace du dégradé, x1,y1,x2,y2)
    Linear(Mat, [f32; 4], Vec<Stop>, u8),
    /// (inverse, cx,cy,r,fx,fy)
    Radial(Mat, [f32; 5], Vec<Stop>, u8),
}

fn stops_at(stops: &[Stop], t: f32, spread: u8) -> [f32; 4] {
    let t = match spread {
        1 => {
            let m = t.rem_euclid(2.0);
            if m > 1.0 {
                2.0 - m
            } else {
                m
            }
        }
        2 => t.rem_euclid(1.0),
        _ => t.clamp(0.0, 1.0),
    };
    if stops.is_empty() {
        return [0.0; 4];
    }
    if t <= stops[0].off {
        return stops[0].color;
    }
    for w in stops.windows(2) {
        if t <= w[1].off {
            let span = (w[1].off - w[0].off).max(1e-6);
            let k = (t - w[0].off) / span;
            let mut c = [0.0; 4];
            for (i, v) in c.iter_mut().enumerate() {
                *v = w[0].color[i] * (1.0 - k) + w[1].color[i] * k;
            }
            return c;
        }
    }
    stops[stops.len() - 1].color
}

impl Paint {
    fn at(&self, x: f32, y: f32) -> [f32; 4] {
        match self {
            Paint::Solid(c) => *c,
            Paint::Linear(inv, l, stops, spread) => {
                let (u, v) = inv.apply((x, y));
                let (dx, dy) = (l[2] - l[0], l[3] - l[1]);
                let len = dx * dx + dy * dy;
                let t = if len > 0.0 {
                    ((u - l[0]) * dx + (v - l[1]) * dy) / len
                } else {
                    0.0
                };
                stops_at(stops, t, *spread)
            }
            Paint::Radial(inv, g, stops, spread) => {
                let (u, v) = inv.apply((x, y));
                let (cx, cy, r, fx, fy) = (g[0], g[1], g[2], g[3], g[4]);
                // distance relative focale -> cercle
                let (dx, dy) = (u - fx, v - fy);
                let (cdx, cdy) = (cx - fx, cy - fy);
                let a = dx * dx + dy * dy;
                let t = if a <= 1e-12 || r <= 0.0 {
                    0.0
                } else {
                    let b = dx * cdx + dy * cdy;
                    let c = cdx * cdx + cdy * cdy - r * r;
                    let disc = b * b - a * c;
                    let s = (b + disc.max(0.0).sqrt()) / a;
                    if s > 0.0 {
                        1.0 / s
                    } else {
                        1.0
                    }
                };
                stops_at(stops, t, *spread)
            }
        }
    }
}

struct Edge {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    dir: i32,
}

const SUB: usize = 5;

/// Remplit des polygones (déjà en coordonnées pixels).
pub fn fill(cv: &mut Canvas, polys: &[Vec<Pt>], evenodd: bool, paint: &Paint, opacity: f32) {
    let mut edges: Vec<Edge> = Vec::new();
    let (mut ymin, mut ymax) = (f32::MAX, f32::MIN);
    for poly in polys {
        if poly.len() < 2 {
            continue;
        }
        for i in 0..poly.len() {
            let a = poly[i];
            let b = poly[(i + 1) % poly.len()];
            if !(a.0.is_finite() && a.1.is_finite() && b.0.is_finite() && b.1.is_finite()) {
                continue;
            }
            if (a.1 - b.1).abs() < 1e-9 {
                continue;
            }
            let e = if a.1 < b.1 {
                Edge {
                    x0: a.0,
                    y0: a.1,
                    x1: b.0,
                    y1: b.1,
                    dir: 1,
                }
            } else {
                Edge {
                    x0: b.0,
                    y0: b.1,
                    x1: a.0,
                    y1: a.1,
                    dir: -1,
                }
            };
            ymin = ymin.min(e.y0);
            ymax = ymax.max(e.y1);
            edges.push(e);
        }
    }
    if edges.is_empty() {
        return;
    }
    edges.sort_by(|a, b| a.y0.partial_cmp(&b.y0).unwrap_or(std::cmp::Ordering::Equal));
    let y_start = (ymin.floor().max(0.0)) as usize;
    let y_end = (ymax.ceil().min(cv.h as f32)) as usize;
    let mut cover = vec![0f32; cv.w + 2];
    let mut active: Vec<usize> = Vec::new();
    let mut next = 0;
    let mut xs: Vec<(f32, i32)> = Vec::new();
    for py in y_start..y_end {
        let (mut minx, mut maxx) = (usize::MAX, 0usize);
        for s in 0..SUB {
            let sy = py as f32 + (s as f32 + 0.5) / SUB as f32;
            while next < edges.len() && edges[next].y0 <= sy {
                active.push(next);
                next += 1;
            }
            active.retain(|&i| edges[i].y1 > sy);
            xs.clear();
            for &i in &active {
                let e = &edges[i];
                if e.y0 <= sy && sy < e.y1 {
                    let t = (sy - e.y0) / (e.y1 - e.y0);
                    xs.push((e.x0 + (e.x1 - e.x0) * t, e.dir));
                }
            }
            xs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let mut wind = 0;
            for k in 0..xs.len() {
                let was_in = if evenodd { wind % 2 != 0 } else { wind != 0 };
                wind += if evenodd { 1 } else { xs[k].1 };
                let _ = was_in;
                let inside = if evenodd { wind % 2 != 0 } else { wind != 0 };
                if inside && k + 1 < xs.len() {
                    let xa = xs[k].0.max(0.0);
                    let xb = xs[k + 1].0.min(cv.w as f32);
                    if xb > xa {
                        span(&mut cover, xa, xb, 1.0 / SUB as f32);
                        minx = minx.min(xa as usize);
                        maxx = maxx.max((xb.ceil() as usize).min(cv.w));
                    }
                }
            }
        }
        if minx == usize::MAX {
            continue;
        }
        let row = py * cv.w;
        for (x, c) in cover.iter_mut().enumerate().take(maxx.min(cv.w)).skip(minx) {
            let a = c.min(1.0);
            *c = 0.0;
            if a <= 0.0 {
                continue;
            }
            let col = paint.at(x as f32 + 0.5, py as f32 + 0.5);
            let sa = col[3] * opacity * a;
            if sa <= 0.0 {
                continue;
            }
            let d = &mut cv.px[row + x];
            let k = 1.0 - sa;
            d[0] = col[0] * sa + d[0] * k;
            d[1] = col[1] * sa + d[1] * k;
            d[2] = col[2] * sa + d[2] * k;
            d[3] = sa + d[3] * k;
        }
        for c in cover.iter_mut() {
            *c = 0.0;
        }
    }
}

fn span(cover: &mut [f32], xa: f32, xb: f32, w: f32) {
    let ia = xa as usize;
    let ib = xb as usize;
    if ia >= cover.len() {
        return;
    }
    if ia == ib {
        cover[ia] += (xb - xa) * w;
        return;
    }
    cover[ia] += (1.0 - (xa - ia as f32)) * w;
    let end = ib.min(cover.len());
    for c in cover.iter_mut().take(end).skip(ia + 1) {
        *c += w;
    }
    if ib < cover.len() {
        cover[ib] += (xb - ib as f32) * w;
    }
}

fn area(p: &[Pt]) -> f32 {
    let mut s = 0.0;
    for i in 0..p.len() {
        let a = p[i];
        let b = p[(i + 1) % p.len()];
        s += a.0 * b.1 - b.0 * a.1;
    }
    s / 2.0
}

fn oriented(mut p: Vec<Pt>) -> Vec<Pt> {
    if area(&p) < 0.0 {
        p.reverse();
    }
    p
}

fn disc(c: Pt, r: f32) -> Vec<Pt> {
    let n = ((r * 2.0) as usize).clamp(8, 64);
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            (c.0 + r * a.cos(), c.1 + r * a.sin())
        })
        .collect()
}

/// Contour épaissi : segments en quadrilatères + jointures/terminaisons.
/// `cap` : 0 bout, 1 rond, 2 carré. `join` : 0 miter, 1 rond, 2 biseau.
pub fn stroke_polys(lines: &[(Vec<Pt>, bool)], width: f32, cap: u8, join: u8) -> Vec<Vec<Pt>> {
    let hw = (width / 2.0).max(0.35);
    let mut out = Vec::new();
    for (pts, closed) in lines {
        let mut p: Vec<Pt> = Vec::with_capacity(pts.len());
        for &q in pts {
            if p.last().is_none_or(|l: &Pt| (l.0 - q.0).abs() > 1e-4 || (l.1 - q.1).abs() > 1e-4) {
                p.push(q);
            }
        }
        if *closed && p.len() > 2 && p[0] == p[p.len() - 1] {
            p.pop();
        }
        if p.len() == 1 {
            if cap == 1 {
                out.push(disc(p[0], hw));
            } else if cap == 2 {
                let c = p[0];
                out.push(vec![(c.0 - hw, c.1 - hw), (c.0 + hw, c.1 - hw), (c.0 + hw, c.1 + hw), (c.0 - hw, c.1 + hw)]);
            }
            continue;
        }
        let n = p.len();
        let segs = if *closed { n } else { n - 1 };
        let mut normals = Vec::with_capacity(segs);
        for i in 0..segs {
            let a = p[i];
            let b = p[(i + 1) % n];
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let l = (dx * dx + dy * dy).sqrt().max(1e-6);
            let (ux, uy) = (dx / l, dy / l);
            let (mut a2, mut b2) = (a, b);
            if !*closed && cap == 2 {
                if i == 0 {
                    a2 = (a.0 - ux * hw, a.1 - uy * hw);
                }
                if i == segs - 1 {
                    b2 = (b.0 + ux * hw, b.1 + uy * hw);
                }
            }
            let (nx, ny) = (-uy * hw, ux * hw);
            normals.push((nx, ny));
            out.push(oriented(vec![
                (a2.0 + nx, a2.1 + ny),
                (b2.0 + nx, b2.1 + ny),
                (b2.0 - nx, b2.1 - ny),
                (a2.0 - nx, a2.1 - ny),
            ]));
        }
        // jointures
        let joints: Vec<usize> = if *closed { (0..n).collect() } else { (1..n - 1).collect() };
        for &j in &joints {
            let c = p[j];
            let n0 = normals[(j + segs - 1) % segs];
            let n1 = normals[j % segs];
            if join == 1 || hw < 1.5 {
                out.push(disc(c, hw));
                continue;
            }
            // côté extérieur
            let cross = n0.0 * n1.1 - n0.1 * n1.0;
            let s = if cross > 0.0 { -1.0 } else { 1.0 };
            let a = (c.0 + n0.0 * s, c.1 + n0.1 * s);
            let b = (c.0 + n1.0 * s, c.1 + n1.1 * s);
            let mut poly = vec![c, a];
            if join == 0 {
                // pointe si l'angle le permet (limite 4)
                let (mx, my) = (n0.0 + n1.0, n0.1 + n1.1);
                let ml = (mx * mx + my * my).sqrt();
                if ml > 1e-6 {
                    let cosh = ml / (2.0 * hw);
                    let len = hw / cosh.max(1e-6);
                    if len / hw <= 4.0 {
                        poly.push((c.0 + mx / ml * len * s, c.1 + my / ml * len * s));
                    }
                }
            }
            poly.push(b);
            out.push(oriented(poly));
        }
        if !*closed && cap == 1 {
            out.push(disc(p[0], hw));
            out.push(disc(p[n - 1], hw));
        }
    }
    out
}

/// Courbe de Bézier cubique aplatie (points ajoutés à `out`, sans le départ).
pub fn cubic(out: &mut Vec<Pt>, p0: Pt, p1: Pt, p2: Pt, p3: Pt, tol: f32) {
    let d = ((p1.0 - p0.0).abs() + (p1.1 - p0.1).abs())
        + ((p2.0 - p1.0).abs() + (p2.1 - p1.1).abs())
        + ((p3.0 - p2.0).abs() + (p3.1 - p2.1).abs());
    let n = ((d / tol.max(0.05)).sqrt() as usize).clamp(1, 256);
    for i in 1..=n {
        let t = i as f32 / n as f32;
        let u = 1.0 - t;
        let a = u * u * u;
        let b = 3.0 * u * u * t;
        let c = 3.0 * u * t * t;
        let e = t * t * t;
        out.push((
            a * p0.0 + b * p1.0 + c * p2.0 + e * p3.0,
            a * p0.1 + b * p1.1 + c * p2.1 + e * p3.1,
        ));
    }
}
