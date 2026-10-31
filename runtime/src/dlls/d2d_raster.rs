//! Rastériseur logiciel de Direct2D écrit pour Winlux : chemins remplis
//! avec anticrénelage (4 sous-lignes par pixel, couverture horizontale
//! exacte), traits épais à jointures rondes, courbes de Bézier et arcs
//! aplatis, transformations affines et composition « source over » en
//! BGRA prémultiplié, le format natif des cibles Direct2D.

pub type Pt = (f32, f32);

/// Matrice 3×2 de Direct2D : x' = x·m11 + y·m21 + dx, y' = x·m12 + y·m22 + dy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix {
    pub m11: f32,
    pub m12: f32,
    pub m21: f32,
    pub m22: f32,
    pub dx: f32,
    pub dy: f32,
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix {
        m11: 1.0,
        m12: 0.0,
        m21: 0.0,
        m22: 1.0,
        dx: 0.0,
        dy: 0.0,
    };

    pub fn apply(&self, p: Pt) -> Pt {
        (
            p.0 * self.m11 + p.1 * self.m21 + self.dx,
            p.0 * self.m12 + p.1 * self.m22 + self.dy,
        )
    }

    /// Produit « self puis other » (convention de Direct2D, vecteurs lignes).
    pub fn then(&self, o: &Matrix) -> Matrix {
        Matrix {
            m11: self.m11 * o.m11 + self.m12 * o.m21,
            m12: self.m11 * o.m12 + self.m12 * o.m22,
            m21: self.m21 * o.m11 + self.m22 * o.m21,
            m22: self.m21 * o.m12 + self.m22 * o.m22,
            dx: self.dx * o.m11 + self.dy * o.m21 + o.dx,
            dy: self.dx * o.m12 + self.dy * o.m22 + o.dy,
        }
    }

    pub fn invert(&self) -> Option<Matrix> {
        let det = self.m11 * self.m22 - self.m12 * self.m21;
        if det.abs() < 1e-12 {
            return None;
        }
        let i = 1.0 / det;
        Some(Matrix {
            m11: self.m22 * i,
            m12: -self.m12 * i,
            m21: -self.m21 * i,
            m22: self.m11 * i,
            dx: (self.m21 * self.dy - self.m22 * self.dx) * i,
            dy: (self.m12 * self.dx - self.m11 * self.dy) * i,
        })
    }

    /// Facteur d'échelle moyen, pour l'épaisseur des traits et la finesse
    /// d'aplatissement des courbes.
    pub fn scale(&self) -> f32 {
        ((self.m11 * self.m22 - self.m12 * self.m21).abs())
            .sqrt()
            .max(1e-6)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FillRule {
    EvenOdd,
    NonZero,
}

/// Rectangle de découpe en pixels (gauche, haut, droite, bas).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clip {
    pub l: f32,
    pub t: f32,
    pub r: f32,
    pub b: f32,
}

/// Surface BGRA prémultipliée, lignes de `stride` octets.
pub struct Canvas<'a> {
    pub w: u32,
    pub h: u32,
    pub stride: usize,
    pub px: &'a mut [u8],
}

/// Couleur prémultipliée (r, g, b, a) entre 0 et 1.
pub type Color = [f32; 4];

pub fn premul(r: f32, g: f32, b: f32, a: f32) -> Color {
    let a = a.clamp(0.0, 1.0);
    [
        r.clamp(0.0, 1.0) * a,
        g.clamp(0.0, 1.0) * a,
        b.clamp(0.0, 1.0) * a,
        a,
    ]
}

impl Canvas<'_> {
    /// Mélange « source over » de `c` avec la couverture `cov`.
    fn blend(&mut self, x: usize, y: usize, c: Color, cov: f32) {
        let o = y * self.stride + x * 4;
        let d = &mut self.px[o..o + 4];
        let sa = c[3] * cov;
        if sa <= 0.0 {
            return;
        }
        let inv = 1.0 - sa;
        let s = [c[2] * cov, c[1] * cov, c[0] * cov, sa];
        for i in 0..4 {
            d[i] = ((s[i] * 255.0 + d[i] as f32 * inv) + 0.5).min(255.0) as u8;
        }
    }

    /// Remplit les polygones (en pixels) avec la couleur donnée par `paint`
    /// au centre de chaque pixel.
    pub fn fill(
        &mut self,
        polys: &[Vec<Pt>],
        rule: FillRule,
        clip: Option<Clip>,
        paint: &dyn Fn(f32, f32) -> Color,
    ) {
        let mut edges = Vec::new();
        let (mut min_y, mut max_y) = (f32::MAX, f32::MIN);
        for poly in polys {
            if poly.len() < 2 {
                continue;
            }
            for i in 0..poly.len() {
                let a = poly[i];
                let b = poly[(i + 1) % poly.len()];
                if !(a.0.is_finite() && a.1.is_finite() && b.0.is_finite() && b.1.is_finite())
                    || a.1 == b.1
                {
                    continue;
                }
                min_y = min_y.min(a.1.min(b.1));
                max_y = max_y.max(a.1.max(b.1));
                edges.push((a, b));
            }
        }
        if edges.is_empty() {
            return;
        }
        let c = clip.unwrap_or(Clip {
            l: 0.0,
            t: 0.0,
            r: self.w as f32,
            b: self.h as f32,
        });
        let (cl, cr) = (c.l.max(0.0), c.r.min(self.w as f32));
        let y0 = min_y.max(c.t).max(0.0).floor() as i64;
        let y1 = max_y.min(c.b).min(self.h as f32).ceil() as i64;
        if cl >= cr || y0 >= y1 {
            return;
        }
        let mut cov = vec![0f32; self.w as usize + 1];
        let mut xs: Vec<(f32, i32)> = Vec::new();
        const SUB: usize = 4;
        for y in y0..y1 {
            cov.iter_mut().for_each(|v| *v = 0.0);
            let (mut lo, mut hi) = (usize::MAX, 0usize);
            for s in 0..SUB {
                let sy = y as f32 + (s as f32 + 0.5) / SUB as f32;
                if sy < c.t || sy >= c.b {
                    continue;
                }
                xs.clear();
                for &(a, b) in &edges {
                    let (top, bot, dir) = if a.1 < b.1 { (a, b, 1) } else { (b, a, -1) };
                    if sy < top.1 || sy >= bot.1 {
                        continue;
                    }
                    let t = (sy - top.1) / (bot.1 - top.1);
                    xs.push((top.0 + t * (bot.0 - top.0), dir));
                }
                xs.sort_by(|p, q| p.0.total_cmp(&q.0));
                let mut wind = 0;
                for i in 0..xs.len() {
                    wind += match rule {
                        FillRule::EvenOdd => 1,
                        FillRule::NonZero => xs[i].1,
                    };
                    let inside = match rule {
                        FillRule::EvenOdd => wind % 2 != 0,
                        FillRule::NonZero => wind != 0,
                    };
                    if inside && i + 1 < xs.len() {
                        let (a, b) = (xs[i].0.max(cl), xs[i + 1].0.min(cr));
                        if a < b {
                            add_span(&mut cov, a, b, 1.0 / SUB as f32);
                            lo = lo.min(a.floor() as usize);
                            hi = hi.max(b.ceil() as usize);
                        }
                    }
                }
            }
            if lo == usize::MAX {
                continue;
            }
            let hi = hi.min(self.w as usize).min(cov.len());
            for (x, &c) in cov.iter().enumerate().take(hi).skip(lo) {
                let v = c.min(1.0);
                if v > 0.0 {
                    let col = paint(x as f32 + 0.5, y as f32 + 0.5);
                    self.blend(x, y as usize, col, v);
                }
            }
        }
    }
}

/// Ajoute la couverture horizontale exacte de [a, b) pondérée par `w`.
fn add_span(cov: &mut [f32], a: f32, b: f32, w: f32) {
    let (ia, ib) = (a.floor() as usize, b.floor() as usize);
    if ia >= cov.len() {
        return;
    }
    if ia == ib {
        cov[ia] += (b - a) * w;
        return;
    }
    cov[ia] += (ia as f32 + 1.0 - a) * w;
    let end = ib.min(cov.len());
    for v in cov.iter_mut().take(end).skip(ia + 1) {
        *v += w;
    }
    if ib < cov.len() {
        cov[ib] += (b - ib as f32) * w;
    }
}

pub fn to_bgra(c: Color) -> [u8; 4] {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    [q(c[2]), q(c[1]), q(c[0]), q(c[3])]
}

// --- Construction des chemins -------------------------------------------------

/// Chemin en coordonnées utilisateur : sous-chemins de points déjà aplatis.
#[derive(Clone, Debug, Default)]
pub struct Path {
    pub figures: Vec<(Vec<Pt>, bool)>,
}

/// Nombre de segments pour une courbe d'étendue `len` pixels.
fn segments(len: f32) -> usize {
    ((len / 2.0).sqrt() * 2.0).clamp(4.0, 256.0) as usize
}

impl Path {
    pub fn begin(&mut self, p: Pt) {
        self.figures.push((vec![p], false));
    }
    fn last(&mut self) -> &mut Vec<Pt> {
        if self.figures.is_empty() {
            self.figures.push((vec![(0.0, 0.0)], false));
        }
        &mut self.figures.last_mut().unwrap().0
    }
    fn current(&mut self) -> Pt {
        *self.last().last().unwrap()
    }
    pub fn line(&mut self, p: Pt) {
        self.last().push(p);
    }
    pub fn cubic(&mut self, c1: Pt, c2: Pt, e: Pt, scale: f32) {
        let s = self.current();
        let len = (dist(s, c1) + dist(c1, c2) + dist(c2, e)) * scale;
        let n = segments(len);
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.line((
                a * s.0 + b * c1.0 + c * c2.0 + d * e.0,
                a * s.1 + b * c1.1 + c * c2.1 + d * e.1,
            ));
        }
    }
    pub fn quadratic(&mut self, c: Pt, e: Pt, scale: f32) {
        let s = self.current();
        let c1 = (s.0 + 2.0 / 3.0 * (c.0 - s.0), s.1 + 2.0 / 3.0 * (c.1 - s.1));
        let c2 = (e.0 + 2.0 / 3.0 * (c.0 - e.0), e.1 + 2.0 / 3.0 * (c.1 - e.1));
        self.cubic(c1, c2, e, scale);
    }
    /// Arc elliptique de Direct2D (D2D1_ARC_SEGMENT) : point final, rayons,
    /// rotation en degrés, sens (0 antihoraire, 1 horaire), grand arc.
    #[allow(clippy::too_many_arguments)]
    pub fn arc(
        &mut self,
        e: Pt,
        rx: f32,
        ry: f32,
        rot_deg: f32,
        clockwise: bool,
        large: bool,
        scale: f32,
    ) {
        let s = self.current();
        let (mut rx, mut ry) = (rx.abs(), ry.abs());
        if rx < 1e-6 || ry < 1e-6 || dist(s, e) < 1e-6 {
            self.line(e);
            return;
        }
        // Conversion en paramétrage centré (SVG 1.1, annexe F.6).
        let phi = rot_deg.to_radians();
        let (cp, sp) = (phi.cos(), phi.sin());
        let (dx, dy) = ((s.0 - e.0) / 2.0, (s.1 - e.1) / 2.0);
        let (x1, y1) = (cp * dx + sp * dy, -sp * dx + cp * dy);
        let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
        if lambda > 1.0 {
            rx *= lambda.sqrt();
            ry *= lambda.sqrt();
        }
        let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
        let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
        let mut k = (num / den).max(0.0).sqrt();
        if large == clockwise {
            k = -k;
        }
        let (cx1, cy1) = (k * rx * y1 / ry, -k * ry * x1 / rx);
        let (cx, cy) = (
            cp * cx1 - sp * cy1 + (s.0 + e.0) / 2.0,
            sp * cx1 + cp * cy1 + (s.1 + e.1) / 2.0,
        );
        let ang = |ux: f32, uy: f32| uy.atan2(ux);
        let t1 = ang((x1 - cx1) / rx, (y1 - cy1) / ry);
        let mut dt = ang((-x1 - cx1) / rx, (-y1 - cy1) / ry) - t1;
        let tau = std::f32::consts::TAU;
        if clockwise && dt < 0.0 {
            dt += tau;
        } else if !clockwise && dt > 0.0 {
            dt -= tau;
        }
        let n = segments(dt.abs() * rx.max(ry) * scale);
        for i in 1..=n {
            let t = t1 + dt * i as f32 / n as f32;
            let (x, y) = (rx * t.cos(), ry * t.sin());
            self.line((cp * x - sp * y + cx, sp * x + cp * y + cy));
        }
        // Le dernier point doit tomber exactement sur la cible.
        if let Some(p) = self.last().last_mut() {
            *p = e;
        }
    }
    pub fn close(&mut self) {
        if let Some(f) = self.figures.last_mut() {
            f.1 = true;
        }
    }

    pub fn rect(l: f32, t: f32, r: f32, b: f32) -> Path {
        Path {
            figures: vec![(vec![(l, t), (r, t), (r, b), (l, b)], true)],
        }
    }

    pub fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32, scale: f32) -> Path {
        let n = segments(std::f32::consts::TAU * rx.abs().max(ry.abs()) * scale);
        let pts = (0..n)
            .map(|i| {
                let t = std::f32::consts::TAU * i as f32 / n as f32;
                (cx + rx * t.cos(), cy + ry * t.sin())
            })
            .collect();
        Path {
            figures: vec![(pts, true)],
        }
    }

    pub fn rounded_rect(l: f32, t: f32, r: f32, b: f32, rx: f32, ry: f32, scale: f32) -> Path {
        let rx = rx.abs().min((r - l).abs() / 2.0);
        let ry = ry.abs().min((b - t).abs() / 2.0);
        if rx < 1e-3 || ry < 1e-3 {
            return Path::rect(l, t, r, b);
        }
        let mut p = Path::default();
        p.begin((l + rx, t));
        p.line((r - rx, t));
        p.arc((r, t + ry), rx, ry, 0.0, true, false, scale);
        p.line((r, b - ry));
        p.arc((r - rx, b), rx, ry, 0.0, true, false, scale);
        p.line((l + rx, b));
        p.arc((l, b - ry), rx, ry, 0.0, true, false, scale);
        p.line((l, t + ry));
        p.arc((l + rx, t), rx, ry, 0.0, true, false, scale);
        p.close();
        p
    }

    /// Polygones à remplir, transformés en pixels.
    pub fn fill_polys(&self, m: &Matrix) -> Vec<Vec<Pt>> {
        self.figures
            .iter()
            .map(|(pts, _)| pts.iter().map(|p| m.apply(*p)).collect())
            .collect()
    }

    /// Contour d'épaisseur `width` (en unités utilisateur), à jointures et
    /// extrémités rondes, sous forme de polygones orientés pour une union
    /// par règle non nulle.
    pub fn stroke_polys(&self, m: &Matrix, width: f32) -> Vec<Vec<Pt>> {
        let hw = (width * m.scale() / 2.0).max(0.35);
        let mut out = Vec::new();
        for (pts, closed) in &self.figures {
            let pts: Vec<Pt> = pts.iter().map(|p| m.apply(*p)).collect();
            if pts.len() == 1 {
                out.push(oriented(circle(pts[0], hw)));
                continue;
            }
            let n = if *closed { pts.len() } else { pts.len() - 1 };
            for i in 0..n {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                let len = dist(a, b);
                if len < 1e-6 {
                    continue;
                }
                let (nx, ny) = (-(b.1 - a.1) / len * hw, (b.0 - a.0) / len * hw);
                out.push(oriented(vec![
                    (a.0 + nx, a.1 + ny),
                    (b.0 + nx, b.1 + ny),
                    (b.0 - nx, b.1 - ny),
                    (a.0 - nx, a.1 - ny),
                ]));
            }
            // Jointures rondes à chaque sommet, extrémités rondes comprises.
            for p in &pts {
                out.push(oriented(circle(*p, hw)));
            }
        }
        out
    }
}

fn dist(a: Pt, b: Pt) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn circle(c: Pt, r: f32) -> Vec<Pt> {
    let n = segments(std::f32::consts::TAU * r).max(8);
    (0..n)
        .map(|i| {
            let t = std::f32::consts::TAU * i as f32 / n as f32;
            (c.0 + r * t.cos(), c.1 + r * t.sin())
        })
        .collect()
}

/// Oriente un polygone dans le sens positif, pour que l'union des pièces
/// d'un trait ne s'annule jamais avec la règle non nulle.
fn oriented(mut p: Vec<Pt>) -> Vec<Pt> {
    let area: f32 = (0..p.len())
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum();
    if area < 0.0 {
        p.reverse();
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas(px: &mut [u8], w: u32, h: u32) -> Canvas<'_> {
        Canvas {
            w,
            h,
            stride: w as usize * 4,
            px,
        }
    }

    fn alpha(px: &[u8], w: u32, x: u32, y: u32) -> u8 {
        px[(y * w + x) as usize * 4 + 3]
    }

    #[test]
    fn rectangle_aligne_sur_les_pixels_sans_bavure() {
        let mut px = vec![0u8; 8 * 8 * 4];
        let red = premul(1.0, 0.0, 0.0, 1.0);
        canvas(&mut px, 8, 8).fill(
            &Path::rect(2.0, 2.0, 6.0, 5.0).fill_polys(&Matrix::IDENTITY),
            FillRule::NonZero,
            None,
            &|_, _| red,
        );
        assert_eq!(&px[(2 * 8 + 2) * 4..][..4], &[0, 0, 255, 255]);
        assert_eq!(alpha(&px, 8, 5, 4), 255);
        assert_eq!(alpha(&px, 8, 6, 4), 0);
        assert_eq!(alpha(&px, 8, 1, 2), 0);
        assert_eq!(alpha(&px, 8, 3, 5), 0);
    }

    #[test]
    fn bord_a_mi_pixel_anticrenele() {
        let mut px = vec![0u8; 4 * 4 * 4];
        let c = premul(1.0, 1.0, 1.0, 1.0);
        canvas(&mut px, 4, 4).fill(
            &Path::rect(0.0, 0.0, 1.5, 4.0).fill_polys(&Matrix::IDENTITY),
            FillRule::NonZero,
            None,
            &|_, _| c,
        );
        let a = alpha(&px, 4, 1, 1);
        assert!((120..=135).contains(&a), "couverture {a}");
    }

    #[test]
    fn pair_impair_et_non_nul() {
        // Deux carrés imbriqués de même sens : trou en pair-impair seulement.
        let outer = vec![(0.0, 0.0), (8.0, 0.0), (8.0, 8.0), (0.0, 8.0)];
        let inner = vec![(2.0, 2.0), (6.0, 2.0), (6.0, 6.0), (2.0, 6.0)];
        let c = premul(0.0, 0.0, 1.0, 1.0);
        for (rule, center) in [(FillRule::EvenOdd, 0u8), (FillRule::NonZero, 255u8)] {
            let mut px = vec![0u8; 8 * 8 * 4];
            canvas(&mut px, 8, 8).fill(&[outer.clone(), inner.clone()], rule, None, &|_, _| c);
            assert_eq!(alpha(&px, 8, 4, 4), center);
            assert_eq!(alpha(&px, 8, 1, 1), 255);
        }
    }

    #[test]
    fn trait_et_decoupe() {
        let mut p = Path::default();
        p.begin((0.0, 4.0));
        p.line((16.0, 4.0));
        let mut px = vec![0u8; 16 * 8 * 4];
        let c = premul(0.0, 1.0, 0.0, 1.0);
        let clip = Clip {
            l: 0.0,
            t: 0.0,
            r: 8.0,
            b: 8.0,
        };
        canvas(&mut px, 16, 8).fill(
            &p.stroke_polys(&Matrix::IDENTITY, 2.0),
            FillRule::NonZero,
            Some(clip),
            &|_, _| c,
        );
        assert_eq!(alpha(&px, 16, 4, 3), 255);
        assert_eq!(alpha(&px, 16, 4, 4), 255);
        assert_eq!(alpha(&px, 16, 4, 1), 0);
        assert_eq!(alpha(&px, 16, 12, 4), 0, "hors de la découpe");
    }

    #[test]
    fn transformation_et_inverse() {
        let m = Matrix {
            m11: 2.0,
            m12: 0.0,
            m21: 0.0,
            m22: 3.0,
            dx: 5.0,
            dy: -1.0,
        };
        let p = m.apply((1.0, 2.0));
        assert_eq!(p, (7.0, 5.0));
        let back = m.invert().unwrap().apply(p);
        assert!((back.0 - 1.0).abs() < 1e-5 && (back.1 - 2.0).abs() < 1e-5);
        let t = Matrix {
            dx: 1.0,
            ..Matrix::IDENTITY
        };
        assert_eq!(m.then(&t).apply((0.0, 0.0)), (6.0, -1.0));
    }

    #[test]
    fn ellipse_et_arc_fermes() {
        let e = Path::ellipse(10.0, 10.0, 5.0, 3.0, 1.0);
        let mut px = vec![0u8; 20 * 20 * 4];
        let c = premul(1.0, 0.0, 0.0, 0.5);
        canvas(&mut px, 20, 20).fill(
            &e.fill_polys(&Matrix::IDENTITY),
            FillRule::NonZero,
            None,
            &|_, _| c,
        );
        assert_eq!(alpha(&px, 20, 10, 10), 128);
        assert_eq!(alpha(&px, 20, 10, 14), 0);
        let mut a = Path::default();
        a.begin((0.0, 0.0));
        a.arc((10.0, 0.0), 5.0, 5.0, 0.0, true, false, 1.0);
        assert_eq!(*a.figures[0].0.last().unwrap(), (10.0, 0.0));
        // Demi-cercle horaire de (0,0) à (10,0) : il passe au-dessus (y < 0).
        let mid = a.figures[0].0[a.figures[0].0.len() / 2];
        assert!(mid.1 < -4.0, "{mid:?}");
    }
}
