//! GIMP XCF (versions 0 à 11, précision 8 bits) : calques en tuiles
//! RLE/zlib, composition normale avec opacité et visibilité. Le composite
//! est la première page, puis chaque calque.

use super::{be32, Error, Image, Page, Raster};

struct X<'a> {
    d: &'a [u8],
    v: u32,
}

impl X<'_> {
    fn ptr(&self, p: &mut usize) -> usize {
        if self.v >= 11 {
            let hi = be32(self.d, *p) as u64;
            let lo = be32(self.d, *p + 4) as u64;
            *p += 8;
            ((hi << 32) | lo) as usize
        } else {
            let v = be32(self.d, *p) as usize;
            *p += 4;
            v
        }
    }
}

struct Props {
    opacity: f32,
    visible: bool,
    offx: i32,
    offy: i32,
    compression: u8,
    colormap: Vec<u8>,
    group: bool,
}

fn props(d: &[u8], p: &mut usize) -> Props {
    let mut pr = Props {
        opacity: 1.0,
        visible: true,
        offx: 0,
        offy: 0,
        compression: 0,
        colormap: Vec::new(),
        group: false,
    };
    for _ in 0..10_000 {
        let t = be32(d, *p);
        let len = be32(d, *p + 4) as usize;
        let body = d.get(*p + 8..*p + 8 + len).unwrap_or(&[]);
        *p += 8 + len;
        match t {
            0 => break,
            1 => {
                let n = be32(body, 0) as usize;
                pr.colormap = body.get(4..4 + n * 3).unwrap_or(&[]).to_vec();
            }
            6 => pr.opacity = (be32(body, 0) as f32 / 255.0).clamp(0.0, 1.0),
            33 => pr.opacity = f32::from_bits(be32(body, 0)).clamp(0.0, 1.0),
            8 => pr.visible = be32(body, 0) != 0,
            15 => {
                pr.offx = be32(body, 0) as i32;
                pr.offy = be32(body, 4) as i32;
            }
            17 => pr.compression = *body.first().unwrap_or(&0),
            29 => pr.group = true,
            _ => {}
        }
        if *p >= d.len() {
            break;
        }
    }
    pr
}

pub fn decode(d: &[u8]) -> Result<Image, Error> {
    if d.len() < 30 || !d.starts_with(b"gimp xcf ") {
        return Err(Error::Corrupt);
    }
    let v = if &d[9..13] == b"file" {
        0
    } else {
        std::str::from_utf8(&d[10..13])
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    };
    let x = X { d, v };
    let mut p = 14;
    let w = be32(d, p);
    let h = be32(d, p + 4);
    let base = be32(d, p + 8);
    p += 12;
    super::check_size(w, h)?;
    if v >= 4 {
        let prec = be32(d, p);
        p += 4;
        let ok = if v >= 7 {
            prec == 100 || prec == 150
        } else {
            prec == 0 || prec == 1
        };
        if !ok {
            return Err(Error::Variant("XCF haute précision"));
        }
    }
    let ip = props(d, &mut p);
    let mut layer_ptrs = Vec::new();
    loop {
        let lp = x.ptr(&mut p);
        if lp == 0 || layer_ptrs.len() > 4096 {
            break;
        }
        layer_ptrs.push(lp);
    }
    let mut img = Image::new("XCF");
    let mut composite = Raster::new(w, h);
    let mut layers: Vec<(String, Raster)> = Vec::new();
    // la liste va du calque du dessus vers le fond
    for &lp in layer_ptrs.iter().rev() {
        let mut q = lp;
        let lw = be32(d, q);
        let lh = be32(d, q + 4);
        let ltype = be32(d, q + 8);
        q += 12;
        let nlen = be32(d, q) as usize;
        let name =
            String::from_utf8_lossy(d.get(q + 4..q + 4 + nlen.saturating_sub(1)).unwrap_or(&[]))
                .into_owned();
        q += 4 + nlen;
        let lpr = props(d, &mut q);
        if lpr.group || lw == 0 || lh == 0 || lw > 65_535 || lh > 65_535 {
            continue;
        }
        let hp = x.ptr(&mut q);
        let Some(lr) = read_hierarchy(&x, hp, ltype, ip.compression, &ip.colormap) else {
            continue;
        };
        let mut canvas = Raster::new(w, h);
        for yy in 0..lr.h as i32 {
            for xx in 0..lr.w as i32 {
                let (cx, cy) = (lpr.offx + xx, lpr.offy + yy);
                if cx < 0 || cy < 0 || cx as u32 >= w || cy as u32 >= h {
                    continue;
                }
                let mut c = lr.get(xx as u32, yy as u32);
                c[3] = (c[3] as f32 * lpr.opacity) as u8;
                canvas.set(cx as u32, cy as u32, c);
                if lpr.visible {
                    let dst = composite.get(cx as u32, cy as u32);
                    composite.set(cx as u32, cy as u32, super::over(c, dst));
                }
            }
        }
        layers.push((name, canvas));
    }
    let _ = base;
    img.pages.push(Page::raster(String::new(), composite));
    if layers.len() > 1 {
        for (n, r) in layers.into_iter().rev() {
            img.pages.push(Page::raster(n, r));
        }
        img.layered = true;
    }
    Ok(img)
}

fn read_hierarchy(x: &X, hp: usize, ltype: u32, comp: u8, cmap: &[u8]) -> Option<Raster> {
    let d = x.d;
    let w = be32(d, hp);
    let h = be32(d, hp + 4);
    let bpp = be32(d, hp + 8) as usize;
    if bpp == 0 || bpp > 4 {
        return None;
    }
    let mut p = hp + 12;
    let lvl = x.ptr(&mut p);
    let mut q = lvl + 8;
    let tw = w.div_ceil(64);
    let th = h.div_ceil(64);
    let mut r = Raster::new(w, h);
    let mut ptrs = Vec::new();
    loop {
        let t = x.ptr(&mut q);
        if t == 0 || ptrs.len() as u32 > tw * th + 1 {
            break;
        }
        ptrs.push(t);
    }
    for (i, &tp) in ptrs.iter().enumerate().take((tw * th) as usize) {
        let (tx, ty) = (i as u32 % tw, i as u32 / tw);
        let cw = (w - tx * 64).min(64) as usize;
        let ch = (h - ty * 64).min(64) as usize;
        let n = cw * ch;
        let end = ptrs.get(i + 1).copied().unwrap_or(d.len()).max(tp);
        let src = d.get(tp..end.min(d.len()))?;
        let mut planar = vec![0u8; n * bpp];
        let interleaved = match comp {
            0 => src.get(..n * bpp)?.to_vec(),
            1 => {
                let mut s = 0;
                for c in 0..bpp {
                    let mut k = 0;
                    while k < n {
                        let op = *src.get(s)? as usize;
                        s += 1;
                        if op <= 126 {
                            let v = *src.get(s)?;
                            s += 1;
                            for _ in 0..=op {
                                if k < n {
                                    planar[c * n + k] = v;
                                    k += 1;
                                }
                            }
                        } else if op == 127 {
                            let cnt = ((*src.get(s)? as usize) << 8) | *src.get(s + 1)? as usize;
                            let v = *src.get(s + 2)?;
                            s += 3;
                            for _ in 0..cnt {
                                if k < n {
                                    planar[c * n + k] = v;
                                    k += 1;
                                }
                            }
                        } else {
                            let cnt = if op == 128 {
                                let c2 = ((*src.get(s)? as usize) << 8) | *src.get(s + 1)? as usize;
                                s += 2;
                                c2
                            } else {
                                256 - op
                            };
                            for _ in 0..cnt {
                                if k < n {
                                    planar[c * n + k] = *src.get(s)?;
                                    k += 1;
                                }
                                s += 1;
                            }
                        }
                    }
                }
                let mut out = vec![0u8; n * bpp];
                for k in 0..n {
                    for c in 0..bpp {
                        out[k * bpp + c] = planar[c * n + k];
                    }
                }
                out
            }
            2 => super::inflate::inflate_zlib(src, n * bpp + 64)?,
            _ => return None,
        };
        for k in 0..n {
            let px = interleaved.get(k * bpp..k * bpp + bpp)?;
            let c = match ltype {
                0 => [px[0], px[1], px[2], 255],
                1 => [px[0], px[1], px[2], px[3]],
                2 => [px[0], px[0], px[0], 255],
                3 => [px[0], px[0], px[0], px[1]],
                4 | 5 => {
                    let i = px[0] as usize;
                    let rgb = cmap.get(i * 3..i * 3 + 3).unwrap_or(&[0, 0, 0]);
                    [rgb[0], rgb[1], rgb[2], if ltype == 5 { px[1] } else { 255 }]
                }
                _ => return None,
            };
            r.set(tx * 64 + (k % cw) as u32, ty * 64 + (k / cw) as u32, c);
        }
    }
    Some(r)
}
