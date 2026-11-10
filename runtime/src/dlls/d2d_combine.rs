//! Combinaison booléenne de deux géométries aplaties, pour
//! ID2D1Geometry::CombineWithGeometry (#1549) : union, intersection, ou
//! exclusif et différence (`D2D1_COMBINE_MODE`, d2d1.h des en-têtes
//! mingw-w64).
//!
//! Méthode : les arêtes des deux jeux de polygones sont coupées à chacune
//! de leurs intersections ; chaque morceau est gardé s'il sépare l'intérieur
//! du résultat de l'extérieur (test d'un point de part et d'autre du
//! morceau), puis orienté pour avoir l'intérieur à sa gauche. Les morceaux
//! sont enfin chaînés en contours fermés. Avec cette orientation, le
//! nombre d'enroulement vaut 1 dans le résultat et 0 dehors, quelle que
//! soit la façon dont les contours sont chaînés : les règles pair-impair
//! et non nulle donnent le même remplissage.

use super::d2d_raster::{FillRule, Pt};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombineMode {
    Union,
    Intersect,
    Xor,
    Exclude,
}

impl CombineMode {
    pub fn from_d2d(v: u32) -> Option<CombineMode> {
        Some(match v {
            0 => CombineMode::Union,
            1 => CombineMode::Intersect,
            2 => CombineMode::Xor,
            3 => CombineMode::Exclude,
            _ => return None,
        })
    }

    fn apply(self, a: bool, b: bool) -> bool {
        match self {
            CombineMode::Union => a || b,
            CombineMode::Intersect => a && b,
            CombineMode::Xor => a != b,
            CombineMode::Exclude => a && !b,
        }
    }
}

/// Pas de la grille d'accrochage des sommets (1/4096 d'unité).
const GRID: f64 = 4096.0;
/// Distance du point de test de part et d'autre d'un morceau d'arête.
const PROBE: f64 = 2e-3;

type Key = (i64, i64);
type P = (f64, f64);

fn key(p: P) -> Key {
    ((p.0 * GRID).round() as i64, (p.1 * GRID).round() as i64)
}

fn unkey(k: Key) -> P {
    (k.0 as f64 / GRID, k.1 as f64 / GRID)
}

fn cross(a: P, b: P) -> f64 {
    a.0 * b.1 - a.1 * b.0
}

fn sub(a: P, b: P) -> P {
    (a.0 - b.0, a.1 - b.1)
}

/// Point dans des polygones selon la règle (enroulement par demi-droite).
fn contains(polys: &[Vec<Pt>], rule: FillRule, x: f64, y: f64) -> bool {
    let mut wind = 0i32;
    for poly in polys {
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            let (ax, ay, bx, by) = (a.0 as f64, a.1 as f64, b.0 as f64, b.1 as f64);
            if (ay <= y) != (by <= y) {
                let t = (y - ay) / (by - ay);
                if ax + t * (bx - ax) > x {
                    wind += if by > ay { 1 } else { -1 };
                }
            }
        }
    }
    match rule {
        FillRule::EvenOdd => wind % 2 != 0,
        FillRule::NonZero => wind != 0,
    }
}

fn edges_of(polys: &[Vec<Pt>], out: &mut Vec<(P, P)>) {
    for poly in polys {
        let n = poly.len();
        if n < 2 {
            continue;
        }
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            let (a, b) = ((a.0 as f64, a.1 as f64), (b.0 as f64, b.1 as f64));
            if key(a) != key(b) {
                out.push((a, b));
            }
        }
    }
}

/// Paramètres (0..1) le long de `e` où une autre arête la touche.
fn split_params(e: (P, P), others: &[(P, P)], skip: usize) -> Vec<f64> {
    let (p, r) = (e.0, sub(e.1, e.0));
    let rr = r.0 * r.0 + r.1 * r.1;
    let (emin, emax) = (
        (e.0 .0.min(e.1 .0), e.0 .1.min(e.1 .1)),
        (e.0 .0.max(e.1 .0), e.0 .1.max(e.1 .1)),
    );
    let tol = 1.0 / GRID;
    let mut ts = vec![0.0, 1.0];
    for (j, &(q0, q1)) in others.iter().enumerate() {
        if j == skip
            || q0.0.max(q1.0) < emin.0 - tol
            || q0.0.min(q1.0) > emax.0 + tol
            || q0.1.max(q1.1) < emin.1 - tol
            || q0.1.min(q1.1) > emax.1 + tol
        {
            continue;
        }
        let s = sub(q1, q0);
        let d = cross(r, s);
        let qp = sub(q0, p);
        let ss = s.0 * s.0 + s.1 * s.1;
        if d.abs() > 1e-12 * (rr * ss).sqrt().max(1e-300) {
            let t = cross(qp, s) / d;
            let u = cross(qp, r) / d;
            let et = tol / rr.sqrt();
            let eu = tol / ss.sqrt();
            if t > -et && t < 1.0 + et && u > -eu && u < 1.0 + eu {
                ts.push(t.clamp(0.0, 1.0));
            }
        } else if cross(qp, r).abs() / rr.sqrt() < tol {
            // Arêtes colinéaires : extrémités de l'autre sur celle-ci.
            for q in [q0, q1] {
                let t = (sub(q, p).0 * r.0 + sub(q, p).1 * r.1) / rr;
                if t > 0.0 && t < 1.0 {
                    ts.push(t);
                }
            }
        }
    }
    ts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    ts
}

/// Combine `a` (règle `ra`) et `b` (règle `rb`) : contours fermés du
/// résultat, intérieur à gauche de chaque arête.
pub fn combine(
    a: &[Vec<Pt>],
    ra: FillRule,
    b: &[Vec<Pt>],
    rb: FillRule,
    mode: CombineMode,
) -> Vec<Vec<Pt>> {
    let mut edges = Vec::new();
    edges_of(a, &mut edges);
    edges_of(b, &mut edges);

    // Morceaux d'arêtes, sans doublon (arêtes communes aux deux formes).
    let mut seen = HashSet::new();
    let mut pieces: Vec<(Key, Key)> = Vec::new();
    for (i, &e) in edges.iter().enumerate() {
        let r = sub(e.1, e.0);
        let ts = split_params(e, &edges, i);
        let mut prev = key(e.0);
        for &t in &ts[1..] {
            let k = key((e.0 .0 + t * r.0, e.0 .1 + t * r.1));
            if k != prev {
                let id = if prev < k { (prev, k) } else { (k, prev) };
                if seen.insert(id) {
                    pieces.push((prev, k));
                }
                prev = k;
            }
        }
    }

    // Morceaux de bord du résultat, orientés intérieur à gauche.
    let inside = |x: f64, y: f64| mode.apply(contains(a, ra, x, y), contains(b, rb, x, y));
    let mut border: Vec<(Key, Key)> = Vec::new();
    for &(k0, k1) in &pieces {
        let (p0, p1) = (unkey(k0), unkey(k1));
        let d = sub(p1, p0);
        let len = (d.0 * d.0 + d.1 * d.1).sqrt();
        if len <= 0.0 {
            continue;
        }
        let n = (-d.1 / len * PROBE, d.0 / len * PROBE);
        let m = ((p0.0 + p1.0) / 2.0, (p0.1 + p1.1) / 2.0);
        let left = inside(m.0 + n.0, m.1 + n.1);
        let right = inside(m.0 - n.0, m.1 - n.1);
        if left != right {
            border.push(if left { (k0, k1) } else { (k1, k0) });
        }
    }

    // Chaînage en contours fermés.
    let mut from: HashMap<Key, Vec<usize>> = HashMap::new();
    for (i, e) in border.iter().enumerate() {
        from.entry(e.0).or_default().push(i);
    }
    let mut used = vec![false; border.len()];
    let mut loops = Vec::new();
    for start in 0..border.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let first = border[start].0;
        let mut pts = vec![first];
        let mut at = border[start].1;
        while at != first {
            pts.push(at);
            let next = from
                .get(&at)
                .and_then(|v| v.iter().copied().find(|&i| !used[i]));
            let Some(i) = next else { break };
            used[i] = true;
            at = border[i].1;
        }
        let poly = simplify(&pts);
        if poly.len() >= 3 {
            loops.push(poly);
        }
    }
    loops
}

/// Retire les sommets alignés avec leurs voisins.
fn simplify(pts: &[Key]) -> Vec<Pt> {
    let n = pts.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
        let (ab, bc) = ((b.0 - a.0, b.1 - a.1), (c.0 - b.0, c.1 - b.1));
        let crossk = ab.0 as i128 * bc.1 as i128 - ab.1 as i128 * bc.0 as i128;
        let dot = ab.0 as i128 * bc.0 as i128 + ab.1 as i128 * bc.1 as i128;
        if crossk == 0 && dot > 0 {
            continue;
        }
        let p = unkey(b);
        out.push((p.0 as f32, p.1 as f32));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(l: f32, t: f32, r: f32, b: f32) -> Vec<Vec<Pt>> {
        vec![vec![(l, t), (r, t), (r, b), (l, b)]]
    }

    fn area(polys: &[Vec<Pt>]) -> f64 {
        polys
            .iter()
            .map(|p| {
                (0..p.len())
                    .map(|i| {
                        let (a, b) = (p[i], p[(i + 1) % p.len()]);
                        a.0 as f64 * b.1 as f64 - b.0 as f64 * a.1 as f64
                    })
                    .sum::<f64>()
                    / 2.0
            })
            .sum::<f64>()
            .abs()
    }

    fn check(mode: CombineMode, want_area: f64, inside: &[(f32, f32)], outside: &[(f32, f32)]) {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 5.0, 15.0, 15.0);
        let r = combine(&a, FillRule::EvenOdd, &b, FillRule::NonZero, mode);
        assert!(
            (area(&r) - want_area).abs() < 1e-3,
            "{mode:?} : aire {}",
            area(&r)
        );
        for &(x, y) in inside {
            assert!(
                contains(&r, FillRule::NonZero, x as f64, y as f64),
                "{mode:?} ({x}, {y})"
            );
            assert!(
                contains(&r, FillRule::EvenOdd, x as f64, y as f64),
                "{mode:?} ({x}, {y})"
            );
        }
        for &(x, y) in outside {
            assert!(
                !contains(&r, FillRule::NonZero, x as f64, y as f64),
                "{mode:?} ({x}, {y})"
            );
        }
    }

    #[test]
    fn quatre_modes_sur_deux_carres() {
        let (only_a, both, only_b, none) = ((2.0, 2.0), (7.0, 7.0), (12.0, 12.0), (12.0, 2.0));
        check(CombineMode::Union, 175.0, &[only_a, both, only_b], &[none]);
        check(
            CombineMode::Intersect,
            25.0,
            &[both],
            &[only_a, only_b, none],
        );
        check(CombineMode::Xor, 150.0, &[only_a, only_b], &[both, none]);
        check(CombineMode::Exclude, 75.0, &[only_a], &[both, only_b, none]);
    }

    #[test]
    fn bord_commun_et_trou() {
        // Deux carrés côte à côte : l'union est un seul rectangle.
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(10.0, 0.0, 20.0, 10.0);
        let r = combine(
            &a,
            FillRule::NonZero,
            &b,
            FillRule::NonZero,
            CombineMode::Union,
        );
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].len(), 4);
        assert!((area(&r) - 200.0).abs() < 1e-3);
        // Un carré retiré au milieu d'un autre : contour et trou.
        let inner = rect(3.0, 3.0, 7.0, 7.0);
        let r = combine(
            &a,
            FillRule::NonZero,
            &inner,
            FillRule::NonZero,
            CombineMode::Exclude,
        );
        assert_eq!(r.len(), 2);
        assert!(contains(&r, FillRule::NonZero, 1.0, 1.0));
        assert!(!contains(&r, FillRule::NonZero, 5.0, 5.0));
        assert!(!contains(&r, FillRule::EvenOdd, 5.0, 5.0));
    }
}
