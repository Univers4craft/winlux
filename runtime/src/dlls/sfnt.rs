//! Lecture maison des contours TrueType (`glyf`), pour DirectWrite.
//!
//! Tables lues : `head`, `hhea`, `maxp`, `hmtx`, `cmap` (formats 4 et 12),
//! `loca`, `glyf` (glyphes simples et composites), `OS/2` et `post`. Les
//! collections `.ttc` sont acceptées. Les polices à contours CFF (`.otf`
//! PostScript) ne sont pas encore dessinées : `Font::parse` les refuse, et
//! DirectWrite se rabat alors sur une police TrueType.
//!
//! Les coordonnées rendues sont en unités de police, axe y vers le haut.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub type Pt = (f32, f32);

/// Reçoit un contour de glyphe.
pub trait OutlineSink {
    fn move_to(&mut self, p: Pt);
    fn line_to(&mut self, p: Pt);
    fn quad_to(&mut self, c: Pt, p: Pt);
    fn close(&mut self);
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    pub units_per_em: u16,
    /// Hauteur au-dessus de la ligne de base (positive).
    pub ascent: u16,
    /// Profondeur sous la ligne de base (positive).
    pub descent: u16,
    pub line_gap: i16,
    pub cap_height: u16,
    pub x_height: u16,
    pub underline_position: i16,
    pub underline_thickness: u16,
    pub strikeout_position: i16,
    pub strikeout_thickness: u16,
}

pub struct Font {
    data: Vec<u8>,
    glyf: (usize, usize),
    loca: usize,
    long_loca: bool,
    hmtx: usize,
    n_hmetrics: u16,
    num_glyphs: u16,
    cmap: Option<(usize, u16)>,
    symbol: bool,
    pub metrics: Metrics,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn i16_at(b: &[u8], at: usize) -> Option<i16> {
    u16_at(b, at).map(|v| v as i16)
}
fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Table `tag` de la face commençant à `base` : (début, longueur).
fn table(b: &[u8], base: usize, tag: &[u8; 4]) -> Option<(usize, usize)> {
    let n = u16_at(b, base + 4)? as usize;
    for i in 0..n {
        let rec = base + 12 + i * 16;
        if b.get(rec..rec + 4)? == tag {
            let off = u32_at(b, rec + 8)? as usize;
            let len = u32_at(b, rec + 12)? as usize;
            if off.checked_add(len)? <= b.len() {
                return Some((off, len));
            }
            return None;
        }
    }
    None
}

impl Font {
    /// Lit la face `index` (0 hors collection). `None` pour une police CFF
    /// ou un fichier illisible.
    pub fn parse(data: Vec<u8>, index: u32) -> Option<Font> {
        let base = if data.get(0..4)? == b"ttcf" {
            let n = u32_at(&data, 8)?;
            if index >= n {
                return None;
            }
            u32_at(&data, 12 + index as usize * 4)? as usize
        } else {
            0
        };
        let t = |tag: &[u8; 4]| table(&data, base, tag);
        let (head, _) = t(b"head")?;
        let (hhea, _) = t(b"hhea")?;
        let (maxp, _) = t(b"maxp")?;
        let (hmtx, _) = t(b"hmtx")?;
        let (loca, _) = t(b"loca")?;
        let glyf = t(b"glyf")?;
        let units_per_em = u16_at(&data, head + 18)?.max(16);
        let long_loca = i16_at(&data, head + 50)? != 0;
        let num_glyphs = u16_at(&data, maxp + 4)?;
        let n_hmetrics = u16_at(&data, hhea + 34)?.clamp(1, num_glyphs.max(1));
        let (mut ascent, mut descent) = (
            i16_at(&data, hhea + 4)?.max(0) as u16,
            i16_at(&data, hhea + 6)?.unsigned_abs(),
        );
        let line_gap = i16_at(&data, hhea + 8)?;
        let mut m = Metrics {
            units_per_em,
            line_gap,
            cap_height: units_per_em * 7 / 10,
            x_height: units_per_em / 2,
            underline_position: -(units_per_em as i16 / 10),
            underline_thickness: units_per_em / 20,
            strikeout_position: units_per_em as i16 / 4,
            strikeout_thickness: units_per_em / 20,
            ..Metrics::default()
        };
        if let Some((os2, len)) = t(b"OS/2") {
            if len >= 78 {
                ascent = u16_at(&data, os2 + 74)?;
                descent = u16_at(&data, os2 + 76)?;
            }
            if len >= 30 {
                m.strikeout_thickness = i16_at(&data, os2 + 26)?.max(1) as u16;
                m.strikeout_position = i16_at(&data, os2 + 28)?;
            }
            if len >= 90 && u16_at(&data, os2)? >= 2 {
                let xh = i16_at(&data, os2 + 86)?;
                let ch = i16_at(&data, os2 + 88)?;
                if xh > 0 {
                    m.x_height = xh as u16;
                }
                if ch > 0 {
                    m.cap_height = ch as u16;
                }
            }
        }
        if let Some((post, len)) = t(b"post") {
            if len >= 12 {
                m.underline_position = i16_at(&data, post + 8)?;
                m.underline_thickness = i16_at(&data, post + 10)?.max(1) as u16;
            }
        }
        m.ascent = ascent;
        m.descent = descent;
        let (cmap, symbol) = match t(b"cmap") {
            Some((c, _)) => pick_cmap(&data, c),
            None => (None, false),
        };
        Some(Font {
            data,
            glyf,
            loca,
            long_loca,
            hmtx,
            n_hmetrics,
            num_glyphs,
            cmap,
            symbol,
            metrics: m,
        })
    }

    pub fn glyph_count(&self) -> u16 {
        self.num_glyphs
    }

    /// Indice du glyphe du caractère `ch` (0 = glyphe manquant).
    pub fn glyph_index(&self, ch: u32) -> u16 {
        let g = self.lookup(ch);
        if g == 0 && self.symbol && ch < 0x100 {
            return self.lookup(ch | 0xF000);
        }
        g
    }

    fn lookup(&self, ch: u32) -> u16 {
        let Some((at, format)) = self.cmap else {
            return 0;
        };
        let b = &self.data;
        let r = match format {
            4 => cmap4(b, at, ch),
            12 => cmap12(b, at, ch),
            _ => None,
        };
        r.filter(|&g| g < self.num_glyphs).unwrap_or(0)
    }

    /// Avance horizontale en unités de police.
    pub fn advance(&self, gid: u16) -> u16 {
        let i = gid.min(self.n_hmetrics - 1) as usize;
        u16_at(&self.data, self.hmtx + i * 4).unwrap_or(0)
    }

    /// Approche gauche en unités de police.
    pub fn left_bearing(&self, gid: u16) -> i16 {
        let n = self.n_hmetrics as usize;
        let g = gid as usize;
        let at = if g < n {
            self.hmtx + g * 4 + 2
        } else {
            self.hmtx + n * 4 + (g - n) * 2
        };
        i16_at(&self.data, at).unwrap_or(0)
    }

    fn glyph_range(&self, gid: u16) -> Option<(usize, usize)> {
        if gid >= self.num_glyphs {
            return None;
        }
        let g = gid as usize;
        let (a, b) = if self.long_loca {
            (
                u32_at(&self.data, self.loca + g * 4)? as usize,
                u32_at(&self.data, self.loca + g * 4 + 4)? as usize,
            )
        } else {
            (
                u16_at(&self.data, self.loca + g * 2)? as usize * 2,
                u16_at(&self.data, self.loca + g * 2 + 2)? as usize * 2,
            )
        };
        if b <= a || b > self.glyf.1 {
            return None;
        }
        Some((self.glyf.0 + a, b - a))
    }

    /// Boîte englobante (xmin, ymin, xmax, ymax) ; `None` pour un glyphe vide.
    pub fn bbox(&self, gid: u16) -> Option<[i16; 4]> {
        let (at, _) = self.glyph_range(gid)?;
        let b = &self.data;
        Some([
            i16_at(b, at + 2)?,
            i16_at(b, at + 4)?,
            i16_at(b, at + 6)?,
            i16_at(b, at + 8)?,
        ])
    }

    /// Envoie le contour du glyphe à `sink`. Faux si le glyphe est illisible.
    pub fn outline(&self, gid: u16, sink: &mut dyn OutlineSink) -> bool {
        self.outline_with(gid, &[1.0, 0.0, 0.0, 1.0, 0.0, 0.0], sink, 0)
            .is_some()
    }

    fn outline_with(
        &self,
        gid: u16,
        m: &[f32; 6],
        sink: &mut dyn OutlineSink,
        depth: u32,
    ) -> Option<()> {
        if depth > 8 {
            return None;
        }
        let Some((at, len)) = self.glyph_range(gid) else {
            return Some(()); // glyphe vide (espace)
        };
        let b = &self.data[..at + len];
        let contours = i16_at(b, at)?;
        if contours >= 0 {
            return simple_glyph(b, at, contours as usize, m, sink);
        }
        // Glyphe composite.
        let mut p = at + 10;
        loop {
            let flags = u16_at(b, p)?;
            let child = u16_at(b, p + 2)?;
            p += 4;
            let (dx, dy) = if flags & 1 != 0 {
                let v = (i16_at(b, p)?, i16_at(b, p + 2)?);
                p += 4;
                v
            } else {
                let v = (*b.get(p)? as i8 as i16, *b.get(p + 1)? as i8 as i16);
                p += 2;
                v
            };
            let (dx, dy) = if flags & 2 != 0 {
                (dx as f32, dy as f32)
            } else {
                (0.0, 0.0)
            };
            let f2 = |at: usize| i16_at(b, at).map(|v| v as f32 / 16384.0);
            let (mut a, mut bb, mut c, mut d) = (1.0, 0.0, 0.0, 1.0);
            if flags & 0x8 != 0 {
                a = f2(p)?;
                d = a;
                p += 2;
            } else if flags & 0x40 != 0 {
                a = f2(p)?;
                d = f2(p + 2)?;
                p += 4;
            } else if flags & 0x80 != 0 {
                a = f2(p)?;
                bb = f2(p + 2)?;
                c = f2(p + 4)?;
                d = f2(p + 6)?;
                p += 8;
            }
            // Composition : enfant (a b c d dx dy) puis parent `m`.
            let child_m = [
                a * m[0] + bb * m[2],
                a * m[1] + bb * m[3],
                c * m[0] + d * m[2],
                c * m[1] + d * m[3],
                dx * m[0] + dy * m[2] + m[4],
                dx * m[1] + dy * m[3] + m[5],
            ];
            self.outline_with(child, &child_m, sink, depth + 1)?;
            if flags & 0x20 == 0 {
                return Some(());
            }
        }
    }
}

fn pick_cmap(b: &[u8], cmap: usize) -> (Option<(usize, u16)>, bool) {
    let Some(n) = u16_at(b, cmap + 2) else {
        return (None, false);
    };
    let mut best: Option<(u32, usize, u16, bool)> = None;
    for i in 0..n as usize {
        let rec = cmap + 4 + i * 8;
        let (Some(pid), Some(eid), Some(off)) =
            (u16_at(b, rec), u16_at(b, rec + 2), u32_at(b, rec + 4))
        else {
            break;
        };
        let at = cmap + off as usize;
        let Some(format) = u16_at(b, at) else {
            continue;
        };
        let rank = match (pid, eid, format) {
            (3, 10, 12) | (0, 4, 12) | (0, 6, 12) => 4,
            (3, 1, 4) | (0, 3, 4) => 3,
            (0, _, 4) => 2,
            (3, 0, 4) => 1,
            _ => 0,
        };
        if rank > 0 && best.is_none_or(|b| rank > b.0) {
            best = Some((rank, at, format, pid == 3 && eid == 0));
        }
    }
    match best {
        Some((_, at, f, sym)) => (Some((at, f)), sym),
        None => (None, false),
    }
}

fn cmap4(b: &[u8], at: usize, ch: u32) -> Option<u16> {
    if ch > 0xFFFF {
        return None;
    }
    let ch = ch as u16;
    let segs = u16_at(b, at + 6)? as usize / 2;
    let ends = at + 14;
    let starts = ends + segs * 2 + 2;
    let deltas = starts + segs * 2;
    let ranges = deltas + segs * 2;
    // Recherche dichotomique du premier segment dont la fin >= ch.
    let (mut lo, mut hi) = (0, segs);
    while lo < hi {
        let mid = (lo + hi) / 2;
        if u16_at(b, ends + mid * 2)? < ch {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if lo >= segs {
        return None;
    }
    let i = lo;
    let start = u16_at(b, starts + i * 2)?;
    if ch < start {
        return None;
    }
    let delta = u16_at(b, deltas + i * 2)?;
    let ro = u16_at(b, ranges + i * 2)? as usize;
    if ro == 0 {
        return Some(ch.wrapping_add(delta));
    }
    let gi = u16_at(b, ranges + i * 2 + ro + (ch - start) as usize * 2)?;
    if gi == 0 {
        return Some(0);
    }
    Some(gi.wrapping_add(delta))
}

fn cmap12(b: &[u8], at: usize, ch: u32) -> Option<u16> {
    let n = u32_at(b, at + 12)? as usize;
    let (mut lo, mut hi) = (0, n);
    while lo < hi {
        let mid = (lo + hi) / 2;
        let g = at + 16 + mid * 12;
        let (s, e) = (u32_at(b, g)?, u32_at(b, g + 4)?);
        if ch < s {
            hi = mid;
        } else if ch > e {
            lo = mid + 1;
        } else {
            return u16::try_from(u32_at(b, g + 8)? + (ch - s)).ok();
        }
    }
    None
}

fn simple_glyph(
    b: &[u8],
    at: usize,
    contours: usize,
    m: &[f32; 6],
    sink: &mut dyn OutlineSink,
) -> Option<()> {
    let mut ends = Vec::with_capacity(contours);
    for i in 0..contours {
        ends.push(u16_at(b, at + 10 + i * 2)? as usize);
    }
    let n = ends.last().map_or(0, |e| e + 1);
    let ins = u16_at(b, at + 10 + contours * 2)? as usize;
    let mut p = at + 12 + contours * 2 + ins;
    let mut flags = Vec::with_capacity(n);
    while flags.len() < n {
        let f = *b.get(p)?;
        p += 1;
        flags.push(f);
        if f & 8 != 0 {
            let r = *b.get(p)?;
            p += 1;
            for _ in 0..r {
                flags.push(f);
            }
        }
    }
    flags.truncate(n);
    let mut coords = |short: u8, same: u8| -> Option<Vec<i32>> {
        let mut v = 0i32;
        let mut out = Vec::with_capacity(n);
        for &f in &flags {
            if f & short != 0 {
                let d = *b.get(p)? as i32;
                p += 1;
                v += if f & same != 0 { d } else { -d };
            } else if f & same == 0 {
                v += i16_at(b, p)? as i32;
                p += 2;
            }
            out.push(v);
        }
        Some(out)
    };
    let xs = coords(2, 0x10)?;
    let ys = coords(4, 0x20)?;
    let tr = |i: usize| -> Pt {
        let (x, y) = (xs[i] as f32, ys[i] as f32);
        (x * m[0] + y * m[2] + m[4], x * m[1] + y * m[3] + m[5])
    };
    let mid = |a: Pt, c: Pt| ((a.0 + c.0) / 2.0, (a.1 + c.1) / 2.0);
    let mut first = 0;
    for &end in &ends {
        if end < first || end >= n {
            return None;
        }
        let pts: Vec<(Pt, bool)> = (first..=end).map(|i| (tr(i), flags[i] & 1 != 0)).collect();
        first = end + 1;
        if pts.len() < 2 {
            continue;
        }
        // Point de départ : un point sur la courbe, ou le milieu de deux hors courbe.
        let k = pts.len();
        let (start, from) = match pts.iter().position(|p| p.1) {
            Some(i) => (pts[i].0, i),
            None => (mid(pts[0].0, pts[k - 1].0), k - 1),
        };
        sink.move_to(start);
        let mut ctrl: Option<Pt> = None;
        for j in 1..=k {
            let (pt, on) = pts[(from + j) % k];
            match (on, ctrl) {
                (true, None) => sink.line_to(pt),
                (true, Some(c)) => {
                    sink.quad_to(c, pt);
                    ctrl = None;
                }
                (false, None) => ctrl = Some(pt),
                (false, Some(c)) => {
                    sink.quad_to(c, mid(c, pt));
                    ctrl = Some(pt);
                }
            }
        }
        if let Some(c) = ctrl {
            sink.quad_to(c, start);
        }
        sink.close();
    }
    Some(())
}

type Cache = Mutex<Vec<(PathBuf, u32, Option<Arc<Font>>)>>;

fn cache() -> &'static Cache {
    static C: OnceLock<Cache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(Vec::new()))
}

/// Charge (une seule fois par processus) la face `index` du fichier `path`.
pub fn load(path: &Path, index: u32) -> Option<Arc<Font>> {
    let mut c = super::lock(cache());
    if let Some((_, _, f)) = c.iter().find(|(p, i, _)| p == path && *i == index) {
        return f.clone();
    }
    let f = std::fs::read(path)
        .ok()
        .and_then(|d| Font::parse(d, index))
        .map(Arc::new);
    c.push((path.to_path_buf(), index, f.clone()));
    f
}

/// Nombre de faces d'un fichier (1 hors collection).
pub fn face_count(data: &[u8]) -> u32 {
    if data.get(0..4) == Some(b"ttcf") {
        u32_at(data, 8).unwrap_or(1)
    } else {
        1
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Police TrueType minimale construite en mémoire : glyphe 0 vide,
    /// glyphe 1 = carré plein (100..600) pour « A », glyphe 2 = losange en
    /// courbes (tous les points hors courbe) pour « B », glyphe 3 composite
    /// (le carré décalé de 1000) pour « C ».
    pub fn tiny_font() -> Vec<u8> {
        let be16 = |v: i32| (v as u16).to_be_bytes();
        let mut head = vec![0u8; 54];
        head[18..20].copy_from_slice(&be16(1000));
        head[50..52].copy_from_slice(&be16(0)); // loca courte
        let mut hhea = vec![0u8; 36];
        hhea[4..6].copy_from_slice(&be16(800));
        hhea[6..8].copy_from_slice(&be16(-200));
        hhea[8..10].copy_from_slice(&be16(90));
        hhea[34..36].copy_from_slice(&be16(4));
        let mut maxp = vec![0u8; 6];
        maxp[0..4].copy_from_slice(&0x0000_5000u32.to_be_bytes());
        maxp[4..6].copy_from_slice(&be16(4));
        let mut hmtx = Vec::new();
        for (adv, lsb) in [(500, 0), (700, 100), (700, 100), (1700, 1100)] {
            hmtx.extend_from_slice(&be16(adv));
            hmtx.extend_from_slice(&be16(lsb));
        }
        // Glyphe simple : un contour, points en coordonnées absolues (deltas i16).
        let simple = |pts: &[(i32, i32, bool)], bbox: [i32; 4]| {
            let mut g = Vec::new();
            g.extend_from_slice(&be16(1));
            for v in bbox {
                g.extend_from_slice(&be16(v));
            }
            g.extend_from_slice(&be16(pts.len() as i32 - 1));
            g.extend_from_slice(&be16(0));
            for p in pts {
                g.push(p.2 as u8);
            }
            let mut last = 0;
            for p in pts {
                g.extend_from_slice(&be16(p.0 - last));
                last = p.0;
            }
            last = 0;
            for p in pts {
                g.extend_from_slice(&be16(p.1 - last));
                last = p.1;
            }
            if g.len() % 2 == 1 {
                g.push(0);
            }
            g
        };
        let square = simple(
            &[
                (100, 0, true),
                (100, 600, true),
                (600, 600, true),
                (600, 0, true),
            ],
            [100, 0, 600, 600],
        );
        let diamond = simple(
            &[
                (100, 300, false),
                (350, 600, false),
                (600, 300, false),
                (350, 0, false),
            ],
            [100, 0, 600, 600],
        );
        let mut comp = Vec::new();
        comp.extend_from_slice(&be16(-1));
        for v in [1100, 0, 1600, 600] {
            comp.extend_from_slice(&be16(v));
        }
        comp.extend_from_slice(&be16(0x0003)); // mots + valeurs xy
        comp.extend_from_slice(&be16(1));
        comp.extend_from_slice(&be16(1000));
        comp.extend_from_slice(&be16(0));
        let mut glyf = Vec::new();
        let mut loca = vec![be16(0)];
        for g in [&square, &diamond, &comp] {
            glyf.extend_from_slice(g);
            loca.push(be16(glyf.len() as i32 / 2));
        }
        let loca: Vec<u8> = std::iter::once(be16(0)).chain(loca).flatten().collect();
        // cmap format 4 : un segment 'A'..'C' -> 1..3, plus le segment final.
        let mut sub = Vec::new();
        for v in [
            4,
            32,
            0,
            4,
            4,
            1,
            0,
            b'C' as i32,
            0xFFFF,
            0,
            b'A' as i32,
            0xFFFF,
            1 - b'A' as i32,
            1,
            0,
            0,
        ] {
            sub.extend_from_slice(&be16(v));
        }
        let mut cmap = Vec::new();
        for v in [0, 1, 3, 1] {
            cmap.extend_from_slice(&be16(v));
        }
        cmap.extend_from_slice(&12u32.to_be_bytes());
        cmap.extend_from_slice(&sub);
        let tables: [(&[u8; 4], Vec<u8>); 7] = [
            (b"cmap", cmap),
            (b"glyf", glyf),
            (b"head", head),
            (b"hhea", hhea),
            (b"hmtx", hmtx),
            (b"loca", loca),
            (b"maxp", maxp),
        ];
        let mut out = vec![0, 1, 0, 0];
        out.extend_from_slice(&be16(tables.len() as i32));
        out.extend_from_slice(&[0; 6]);
        let mut off = 12 + tables.len() * 16;
        let mut body = Vec::new();
        for (tag, data) in &tables {
            out.extend_from_slice(*tag);
            out.extend_from_slice(&[0; 4]);
            out.extend_from_slice(&(off as u32).to_be_bytes());
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            body.extend_from_slice(data);
            while body.len() % 4 != 0 {
                body.push(0);
            }
            off = 12 + tables.len() * 16 + body.len();
        }
        out.extend_from_slice(&body);
        out
    }

    #[derive(Default)]
    struct Rec(Vec<String>);
    impl OutlineSink for Rec {
        fn move_to(&mut self, p: Pt) {
            self.0.push(format!("M{} {}", p.0, p.1));
        }
        fn line_to(&mut self, p: Pt) {
            self.0.push(format!("L{} {}", p.0, p.1));
        }
        fn quad_to(&mut self, c: Pt, p: Pt) {
            self.0.push(format!("Q{} {} {} {}", c.0, c.1, p.0, p.1));
        }
        fn close(&mut self) {
            self.0.push("Z".into());
        }
    }

    #[test]
    fn lit_metriques_cmap_et_avances() {
        let f = Font::parse(tiny_font(), 0).expect("police");
        assert_eq!(f.metrics.units_per_em, 1000);
        assert_eq!(
            (f.metrics.ascent, f.metrics.descent, f.metrics.line_gap),
            (800, 200, 90)
        );
        assert_eq!(f.glyph_count(), 4);
        assert_eq!(f.glyph_index('A' as u32), 1);
        assert_eq!(f.glyph_index('C' as u32), 3);
        assert_eq!(f.glyph_index('Z' as u32), 0);
        assert_eq!(f.glyph_index(0x1F600), 0);
        assert_eq!(f.advance(1), 700);
        assert_eq!(f.left_bearing(1), 100);
        assert_eq!(f.bbox(1), Some([100, 0, 600, 600]));
        assert_eq!(f.bbox(0), None);
    }

    #[test]
    fn contours_simples_courbes_et_composites() {
        let f = Font::parse(tiny_font(), 0).unwrap();
        let mut r = Rec::default();
        assert!(f.outline(1, &mut r));
        assert_eq!(
            r.0,
            ["M100 0", "L100 600", "L600 600", "L600 0", "L100 0", "Z"]
        );
        let mut r = Rec::default();
        assert!(f.outline(2, &mut r));
        // Tous hors courbe : départ au milieu du dernier et du premier point.
        assert_eq!(r.0[0], "M225 150");
        assert_eq!(r.0.iter().filter(|s| s.starts_with('Q')).count(), 4);
        assert_eq!(r.0.last().unwrap(), "Z");
        let mut r = Rec::default();
        assert!(f.outline(3, &mut r));
        assert_eq!(r.0[0], "M1100 0");
        assert_eq!(r.0[1], "L1100 600");
        let mut r = Rec::default();
        assert!(f.outline(0, &mut r));
        assert!(r.0.is_empty());
    }

    #[test]
    fn police_systeme_si_presente() {
        let path = Path::new("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf");
        let Some(f) = load(path, 0) else { return };
        let g = f.glyph_index('é' as u32);
        assert_ne!(g, 0);
        let mut r = Rec::default();
        assert!(f.outline(g, &mut r)); // « é » est composite dans DejaVu
        assert!(r.0.len() > 10);
    }
}
