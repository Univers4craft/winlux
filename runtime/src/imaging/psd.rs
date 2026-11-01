//! Photoshop PSD/PSB : image composite (brut, RLE, ZIP) et calques
//! affichés comme pages. Modes RGB, niveaux de gris, CMJN, indexé, bitmap.

use super::{be16, be32, Error, Image, Page, Raster};

struct Ctx<'a> {
    d: &'a [u8],
    psb: bool,
    depth: usize,
    mode: u16,
    palette: &'a [u8],
}

impl Ctx<'_> {
    fn len(&self, p: usize) -> usize {
        if self.psb {
            let hi = be32(self.d, p) as u64;
            let lo = be32(self.d, p + 4) as u64;
            ((hi << 32) | lo) as usize
        } else {
            be32(self.d, p) as usize
        }
    }
    fn lsize(&self) -> usize {
        if self.psb {
            8
        } else {
            4
        }
    }
}

pub fn decode(d: &[u8]) -> Result<Image, Error> {
    if d.len() < 26 || &d[..4] != b"8BPS" {
        return Err(Error::Corrupt);
    }
    let ver = be16(d, 4);
    let nch = be16(d, 12) as usize;
    let h = be32(d, 14);
    let w = be32(d, 18);
    let depth = be16(d, 22) as usize;
    let mode = be16(d, 24);
    super::check_size(w, h)?;
    // Limites du format : 1 à 56 canaux, profondeur 1, 8, 16 ou 32 bits.
    if nch == 0 || nch > 56 || !matches!(depth, 1 | 8 | 16 | 32) {
        return Err(Error::Corrupt);
    }
    let mut p = 26;
    let cm_len = be32(d, p) as usize;
    let palette = d.get(p + 4..p + 4 + cm_len).unwrap_or(&[]);
    p += 4 + cm_len;
    let ctx = Ctx {
        d,
        psb: ver == 2,
        depth,
        mode,
        palette,
    };
    let mut img = Image::new(if ver == 2 { "PSB" } else { "PSD" });
    let ir_len = be32(d, p) as usize;
    if let Some(ir) = d.get(p + 4..p + 4 + ir_len) {
        super::meta::irb(ir, &mut img);
    }
    p += 4 + ir_len;
    let lm_len = ctx.len(p);
    let lm_start = p + ctx.lsize();
    p = lm_start + lm_len;
    let mut layers: Vec<(String, Raster)> = Vec::new();
    if lm_len > 0 {
        if let Some(l) = read_layers(&ctx, lm_start, w, h) {
            layers = l;
        }
    }
    // Image composite
    match read_planes(&ctx, p, w, h, nch) {
        Ok(planes) => {
            let r = compose(&ctx, &planes, w, h, nch, None);
            img.pages.push(Page::raster(String::new(), r));
        }
        Err(e) => {
            if layers.is_empty() {
                return Err(e);
            }
        }
    }
    let composite_missing = img.pages.is_empty();
    for (name, r) in layers.into_iter().rev() {
        img.pages.push(Page::raster(name, r));
    }
    if composite_missing && !img.pages.is_empty() {
        let r = flatten(&img.pages, w, h);
        img.pages.insert(0, Page::raster(String::new(), r));
    }
    img.layered = img.pages.len() > 1;
    Ok(img)
}

fn flatten(pages: &[Page], w: u32, h: u32) -> Raster {
    let mut out = Raster::new(w, h);
    for pg in pages.iter().rev() {
        if let super::Content::Raster(r) = &pg.content {
            for i in 0..out.px.len() / 4 {
                let s = [
                    r.px[i * 4],
                    r.px[i * 4 + 1],
                    r.px[i * 4 + 2],
                    r.px[i * 4 + 3],
                ];
                let d = [
                    out.px[i * 4],
                    out.px[i * 4 + 1],
                    out.px[i * 4 + 2],
                    out.px[i * 4 + 3],
                ];
                out.px[i * 4..i * 4 + 4].copy_from_slice(&super::over(s, d));
            }
        }
    }
    out
}

/// Lit `nch` plans compressés (section des données d'image).
fn read_planes(ctx: &Ctx, p: usize, w: u32, h: u32, nch: usize) -> Result<Vec<Vec<u16>>, Error> {
    let d = ctx.d;
    let comp = be16(d, p);
    let mut q = p + 2;
    let row = (w as usize * ctx.depth).div_ceil(8);
    let mut planes = Vec::with_capacity(nch);
    match comp {
        0 => {
            for _ in 0..nch {
                let src = d.get(q..q + row * h as usize).ok_or(Error::Corrupt)?;
                planes.push(unpack(ctx, src, w, h));
                q += row * h as usize;
            }
        }
        1 => {
            let cs = if ctx.psb { 4 } else { 2 };
            let counts_at = q;
            q += cs * h as usize * nch;
            for c in 0..nch {
                let mut buf = Vec::with_capacity(row * h as usize);
                for y in 0..h as usize {
                    let k = counts_at + (c * h as usize + y) * cs;
                    let n = if cs == 4 {
                        be32(d, k) as usize
                    } else {
                        be16(d, k) as usize
                    };
                    let src = d.get(q..q + n).ok_or(Error::Corrupt)?;
                    let mut line = super::tiff::packbits(src, row);
                    line.resize(row, 0);
                    buf.extend_from_slice(&line);
                    q += n;
                }
                planes.push(unpack(ctx, &buf, w, h));
            }
        }
        _ => return Err(Error::Variant("PSD ZIP composite")),
    }
    Ok(planes)
}

fn unpack(ctx: &Ctx, src: &[u8], w: u32, h: u32) -> Vec<u16> {
    let n = (w * h) as usize;
    let mut out = vec![0u16; n];
    match ctx.depth {
        1 => {
            let row = (w as usize).div_ceil(8);
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let b = *src.get(y * row + x / 8).unwrap_or(&0);
                    out[y * w as usize + x] = if (b >> (7 - x % 8)) & 1 == 1 {
                        0
                    } else {
                        65535
                    };
                }
            }
        }
        8 => {
            for (i, v) in out.iter_mut().enumerate() {
                *v = *src.get(i).unwrap_or(&0) as u16 * 257;
            }
        }
        16 => {
            for (i, v) in out.iter_mut().enumerate() {
                *v = if i * 2 + 1 < src.len() {
                    be16(src, i * 2)
                } else {
                    0
                };
            }
        }
        _ => {
            for (i, v) in out.iter_mut().enumerate() {
                let f = if i * 4 + 3 < src.len() {
                    f32::from_bits(be32(src, i * 4))
                } else {
                    0.0
                };
                *v = (super::tonemap(f) * 65535.0) as u16;
            }
        }
    }
    out
}

fn compose(
    ctx: &Ctx,
    planes: &[Vec<u16>],
    w: u32,
    h: u32,
    nch: usize,
    alpha: Option<&[u16]>,
) -> Raster {
    let mut r = Raster::new(w, h);
    let g = |c: usize, i: usize| -> u8 { planes.get(c).map(|p| (p[i] >> 8) as u8).unwrap_or(0) };
    // Le 4e canal du composite RGB est l'alpha fusionné, sauf s'il est vide.
    let alpha_valid = planes.get(3).is_some_and(|p| p.iter().any(|&v| v != 0));
    for i in 0..(w * h) as usize {
        let mut px = match ctx.mode {
            3 => [
                g(0, i),
                g(1, i),
                g(2, i),
                if nch > 3 { g(3, i) } else { 255 },
            ],
            4 => {
                let k = g(3, i) as u32;
                let f = |c: u8| (c as u32 * k / 255) as u8;
                [f(g(0, i)), f(g(1, i)), f(g(2, i)), 255]
            }
            2 => {
                let k = g(0, i) as usize;
                let n = ctx.palette.len() / 3;
                if k < n {
                    [
                        ctx.palette[k],
                        ctx.palette[n + k],
                        ctx.palette[2 * n + k],
                        255,
                    ]
                } else {
                    [0, 0, 0, 255]
                }
            }
            9 => {
                // Lab approximatif -> sRGB
                let l = g(0, i) as f32 * 100.0 / 255.0;
                let a = g(1, i) as f32 - 128.0;
                let b = g(2, i) as f32 - 128.0;
                lab(l, a, b)
            }
            _ => {
                let v = g(0, i);
                [
                    v,
                    v,
                    v,
                    if nch > 1 && ctx.mode == 1 {
                        g(1, i)
                    } else {
                        255
                    },
                ]
            }
        };
        if ctx.mode == 3 && nch > 3 && !alpha_valid {
            px[3] = 255;
        }
        if let Some(a) = alpha {
            px[3] = (a[i] >> 8) as u8;
        }
        r.px[i * 4..i * 4 + 4].copy_from_slice(&px);
    }
    r
}

fn lab(l: f32, a: f32, b: f32) -> [u8; 4] {
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let f = |t: f32| {
        if t > 0.206_893 {
            t * t * t
        } else {
            (t - 16.0 / 116.0) / 7.787
        }
    };
    let (x, y, z) = (0.950_47 * f(fx), f(fy), 1.088_83 * f(fz));
    let r = 3.2406 * x - 1.5372 * y - 0.4986 * z;
    let g = -0.9689 * x + 1.8758 * y + 0.0415 * z;
    let bb = 0.0557 * x - 0.2040 * y + 1.0570 * z;
    let s = |v: f32| (super::srgb(v) * 255.0) as u8;
    [s(r), s(g), s(bb), 255]
}

fn read_layers(ctx: &Ctx, start: usize, w: u32, h: u32) -> Option<Vec<(String, Raster)>> {
    let d = ctx.d;
    let li_len = ctx.len(start);
    if li_len == 0 {
        return None;
    }
    let mut p = start + ctx.lsize();
    let count = (be16(d, p) as i16).unsigned_abs() as usize;
    p += 2;
    struct L {
        top: i32,
        left: i32,
        bottom: i32,
        right: i32,
        chans: Vec<(i16, usize)>,
        opacity: u8,
        hidden: bool,
        name: String,
    }
    let mut ls = Vec::new();
    for _ in 0..count.min(4096) {
        let top = be32(d, p) as i32;
        let left = be32(d, p + 4) as i32;
        let bottom = be32(d, p + 8) as i32;
        let right = be32(d, p + 12) as i32;
        let nc = be16(d, p + 16) as usize;
        p += 18;
        let mut chans = Vec::new();
        for _ in 0..nc {
            let id = be16(d, p) as i16;
            let len = ctx.len(p + 2);
            chans.push((id, len));
            p += 2 + ctx.lsize();
        }
        if d.get(p..p + 4)? != b"8BIM" {
            return None;
        }
        let opacity = *d.get(p + 8)?;
        let flags = *d.get(p + 10)?;
        let extra = be32(d, p + 12) as usize;
        let e = p + 16;
        p = e + extra;
        // nom pascal après masque et plages de fusion
        let mut q = e;
        let mlen = be32(d, q) as usize;
        q += 4 + mlen;
        let blen = be32(d, q) as usize;
        q += 4 + blen;
        let nlen = *d.get(q)? as usize;
        let mut name: String = d
            .get(q + 1..q + 1 + nlen)?
            .iter()
            .map(|&c| c as char)
            .collect();
        // nom Unicode (luni) s'il est présent
        let mut k = q + ((1 + nlen + 3) & !3);
        while k + 12 <= p {
            if d.get(k..k + 4)? != b"8BIM" && d.get(k..k + 4)? != b"8B64" {
                break;
            }
            let key = d.get(k + 4..k + 8)?;
            let len = be32(d, k + 8) as usize;
            if key == b"luni" && len >= 4 {
                // Nombre d'unités UTF-16 borné par le bloc et par la section.
                let n = (be32(d, k + 12) as usize)
                    .min((len - 4) / 2)
                    .min(p.saturating_sub(k + 16) / 2);
                let u: Vec<u16> = (0..n).map(|i| be16(d, k + 16 + i * 2)).collect();
                name = String::from_utf16_lossy(&u);
            }
            k += 12 + ((len + 1) & !1);
        }
        ls.push(L {
            top,
            left,
            bottom,
            right,
            chans,
            opacity,
            hidden: flags & 2 != 0,
            name,
        });
    }
    let mut out = Vec::new();
    for l in ls {
        let lw = l.right.saturating_sub(l.left).max(0) as u32;
        let lh = l.bottom.saturating_sub(l.top).max(0) as u32;
        // Dimensions vérifiées AVANT de lire les plans (allocation lw*lh).
        let dims_ok = lw > 0
            && lh > 0
            && lw <= w.saturating_mul(4)
            && lh <= h.saturating_mul(4)
            && super::check_size(lw, lh).is_ok();
        let mut planes: Vec<(i16, Vec<u16>)> = Vec::new();
        for &(id, len) in l.chans.iter() {
            let q = p;
            p = p.saturating_add(len);
            if !dims_ok || id < -1 {
                continue;
            }
            if let Ok(mut v) = read_planes(ctx, q, lw, lh, 1) {
                planes.push((id, v.remove(0)));
            }
        }
        if !dims_ok {
            continue;
        }
        let get = |id: i16| {
            planes
                .iter()
                .find(|(i, _)| *i == id)
                .map(|(_, v)| v.clone())
        };
        let nch = if ctx.mode == 4 {
            4
        } else if ctx.mode == 3 {
            3
        } else {
            1
        };
        let pl: Vec<Vec<u16>> = (0..nch as i16)
            .map(|i| get(i).unwrap_or_else(|| vec![0; (lw * lh) as usize]))
            .collect();
        let alpha = get(-1);
        let lr = compose(
            ctx,
            &pl,
            lw,
            lh,
            nch,
            Some(alpha.as_deref().unwrap_or(&vec![65535; (lw * lh) as usize])),
        );
        let mut canvas = Raster::new(w, h);
        for y in 0..lh as i32 {
            for x in 0..lw as i32 {
                let (cx, cy) = (l.left + x, l.top + y);
                if cx >= 0 && cy >= 0 && (cx as u32) < w && (cy as u32) < h {
                    let mut c = lr.get(x as u32, y as u32);
                    c[3] = (c[3] as u32 * l.opacity as u32 / 255) as u8;
                    canvas.set(cx as u32, cy as u32, c);
                }
            }
        }
        let _ = l.hidden;
        out.push((l.name, canvas));
    }
    Some(out)
}
