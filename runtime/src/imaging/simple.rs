//! Formats matriciels « simples » : TGA, PCX, PBM/PGM/PPM/PAM/PFM, QOI,
//! farbfeld, SGI, Sun Raster, XBM, XPM, WBMP et Radiance HDR.
#![allow(clippy::needless_range_loop)]

use super::{be16, be32, le16, Error, Image, Page, Raster};

fn one(format: &'static str, r: Raster) -> Image {
    let mut img = Image::new(format);
    img.pages.push(Page::raster(String::new(), r));
    img
}

pub fn tga(d: &[u8]) -> Result<Image, Error> {
    if d.len() < 18 {
        return Err(Error::Corrupt);
    }
    let idlen = d[0] as usize;
    let cmtype = d[1];
    let kind = d[2];
    let cm_first = le16(d, 3) as usize;
    let cm_len = le16(d, 5) as usize;
    let cm_bits = d[7] as usize;
    let w = le16(d, 12) as u32;
    let h = le16(d, 14) as u32;
    let bpp = d[16] as usize;
    let desc = d[17];
    super::check_size(w, h)?;
    if !matches!(kind, 1 | 2 | 3 | 9 | 10 | 11) || cmtype > 1 {
        return Err(Error::Corrupt);
    }
    let mut p = 18 + idlen;
    let read_color = |b: &[u8], bits: usize| -> [u8; 4] {
        match bits {
            8 => [b[0], b[0], b[0], 255],
            15 | 16 => {
                let v = u16::from_le_bytes([b[0], b[1]]);
                let c = |s: u16| (((v >> s) & 31) as u32 * 255 / 31) as u8;
                [c(10), c(5), c(0), 255]
            }
            24 => [b[2], b[1], b[0], 255],
            32 => [b[2], b[1], b[0], b[3]],
            _ => [0, 0, 0, 255],
        }
    };
    let mut cmap = Vec::new();
    if cmtype == 1 {
        let eb = cm_bits.div_ceil(8);
        for i in 0..cm_len {
            let b = d.get(p + i * eb..p + (i + 1) * eb).ok_or(Error::Corrupt)?;
            cmap.push(read_color(b, cm_bits));
        }
        p += cm_len * eb;
    }
    let bytes = bpp.div_ceil(8).max(1);
    let n = (w * h) as usize;
    let mut pix: Vec<[u8; 4]> = Vec::with_capacity(n);
    let decode_px = |b: &[u8]| -> [u8; 4] {
        if kind == 1 || kind == 9 {
            let i = if bytes == 2 {
                le16(b, 0) as usize
            } else {
                b[0] as usize
            };
            *cmap
                .get(i.wrapping_sub(cm_first))
                .unwrap_or(&[0, 0, 0, 255])
        } else if kind == 3 || kind == 11 {
            [b[0], b[0], b[0], if bytes == 2 { b[1] } else { 255 }]
        } else {
            read_color(b, bpp)
        }
    };
    if kind >= 9 {
        while pix.len() < n {
            let c = *d.get(p).ok_or(Error::Corrupt)?;
            p += 1;
            let k = (c & 0x7F) as usize + 1;
            if c & 0x80 != 0 {
                let b = d.get(p..p + bytes).ok_or(Error::Corrupt)?;
                let px = decode_px(b);
                p += bytes;
                for _ in 0..k {
                    pix.push(px);
                }
            } else {
                for _ in 0..k {
                    let b = d.get(p..p + bytes).ok_or(Error::Corrupt)?;
                    pix.push(decode_px(b));
                    p += bytes;
                }
            }
        }
    } else {
        for _ in 0..n {
            let b = d.get(p..p + bytes).ok_or(Error::Corrupt)?;
            pix.push(decode_px(b));
            p += bytes;
        }
    }
    let alpha_bits = desc & 15;
    let mut r = Raster::new(w, h);
    let right = desc & 0x10 != 0;
    let top = desc & 0x20 != 0;
    for (i, c) in pix.into_iter().take(n).enumerate() {
        let (x, y) = (i as u32 % w, i as u32 / w);
        let x = if right { w - 1 - x } else { x };
        let y = if top { y } else { h - 1 - y };
        let mut c = c;
        if bpp == 32 && alpha_bits == 0 {
            c[3] = 255;
        }
        r.set(x, y, c);
    }
    Ok(one("TGA", r))
}

pub fn pcx(d: &[u8]) -> Result<Image, Error> {
    if d.len() < 128 || d[0] != 10 {
        return Err(Error::Corrupt);
    }
    let bpp = d[3] as usize;
    let (x0, y0, x1, y1) = (le16(d, 4), le16(d, 6), le16(d, 8), le16(d, 10));
    let w = (x1.wrapping_sub(x0) as u32) + 1;
    let h = (y1.wrapping_sub(y0) as u32) + 1;
    super::check_size(w, h)?;
    let planes = d[65] as usize;
    let bpl = le16(d, 66) as usize;
    let total = bpl * planes * h as usize;
    let mut raw = Vec::with_capacity(total);
    let mut p = 128;
    while raw.len() < total && p < d.len() {
        let c = d[p];
        p += 1;
        if c >= 0xC0 {
            let v = *d.get(p).unwrap_or(&0);
            p += 1;
            raw.extend(std::iter::repeat_n(v, (c & 0x3F) as usize));
        } else {
            raw.push(c);
        }
    }
    raw.resize(total, 0);
    let vga = if bpp == 8 && planes == 1 && d.len() >= 769 && d[d.len() - 769] == 12 {
        Some(&d[d.len() - 768..])
    } else {
        None
    };
    let ega = &d[16..64];
    let mut r = Raster::new(w, h);
    for y in 0..h as usize {
        let line = &raw[y * bpl * planes..(y + 1) * bpl * planes];
        for x in 0..w as usize {
            let c = if planes >= 3 && bpp == 8 {
                [
                    line[x],
                    line[bpl + x],
                    line[2 * bpl + x],
                    if planes == 4 { line[3 * bpl + x] } else { 255 },
                ]
            } else if bpp == 8 {
                let i = line[x] as usize;
                match vga {
                    Some(pl) => [pl[i * 3], pl[i * 3 + 1], pl[i * 3 + 2], 255],
                    None => [line[x], line[x], line[x], 255],
                }
            } else {
                // 1 bit x N plans (EGA 16 couleurs) ou 2/4 bits
                let mut idx = 0usize;
                for pl in 0..planes {
                    let bit = x * bpp;
                    let b = line[pl * bpl + bit / 8];
                    let v = (b >> (8 - bpp - bit % 8)) as usize & ((1 << bpp) - 1);
                    idx |= v << (pl * bpp);
                }
                if planes == 1 && bpp == 1 {
                    let g = if idx == 1 { 255 } else { 0 };
                    [g, g, g, 255]
                } else {
                    let i = idx % 16;
                    [ega[i * 3], ega[i * 3 + 1], ega[i * 3 + 2], 255]
                }
            };
            r.set(x as u32, y as u32, c);
        }
    }
    Ok(one("PCX", r))
}

struct Tok<'a> {
    d: &'a [u8],
    p: usize,
}
impl Tok<'_> {
    fn word(&mut self) -> Option<&[u8]> {
        loop {
            while self.p < self.d.len() && self.d[self.p].is_ascii_whitespace() {
                self.p += 1;
            }
            if self.d.get(self.p) == Some(&b'#') {
                while self.p < self.d.len() && self.d[self.p] != b'\n' {
                    self.p += 1;
                }
                continue;
            }
            break;
        }
        let s = self.p;
        while self.p < self.d.len() && !self.d[self.p].is_ascii_whitespace() {
            self.p += 1;
        }
        if s == self.p {
            None
        } else {
            Some(&self.d[s..self.p])
        }
    }
    fn num(&mut self) -> Option<u32> {
        std::str::from_utf8(self.word()?).ok()?.parse().ok()
    }
}

/// PBM, PGM, PPM (ASCII et binaires), PAM et PFM.
pub fn pnm(d: &[u8]) -> Result<Image, Error> {
    if d.len() < 3 || d[0] != b'P' {
        return Err(Error::Corrupt);
    }
    let kind = d[1];
    let mut t = Tok { d, p: 2 };
    if kind == b'7' {
        return pam(d);
    }
    if kind == b'F' || kind == b'f' {
        let w = t.num().ok_or(Error::Corrupt)?;
        let h = t.num().ok_or(Error::Corrupt)?;
        let scale: f32 = std::str::from_utf8(t.word().ok_or(Error::Corrupt)?)
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or(Error::Corrupt)?;
        super::check_size(w, h)?;
        let ch = if kind == b'F' { 3 } else { 1 };
        let p = t.p + 1;
        let le = scale < 0.0;
        let mut r = Raster::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let mut c = [0u8; 4];
                c[3] = 255;
                for k in 0..ch {
                    let o = p + ((y * w + x) as usize * ch + k) * 4;
                    let b = d.get(o..o + 4).ok_or(Error::Corrupt)?;
                    let bits = [b[0], b[1], b[2], b[3]];
                    let v = f32::from_bits(if le {
                        u32::from_le_bytes(bits)
                    } else {
                        u32::from_be_bytes(bits)
                    });
                    c[k] = (super::tonemap(v) * 255.0) as u8;
                }
                if ch == 1 {
                    c[1] = c[0];
                    c[2] = c[0];
                }
                // PFM : lignes de bas en haut
                r.set(x, h - 1 - y, c);
            }
        }
        return Ok(one("PFM", r));
    }
    let w = t.num().ok_or(Error::Corrupt)?;
    let h = t.num().ok_or(Error::Corrupt)?;
    super::check_size(w, h)?;
    let maxv = if kind == b'1' || kind == b'4' {
        1
    } else {
        t.num().ok_or(Error::Corrupt)?.max(1)
    };
    let mut r = Raster::new(w, h);
    let n = (w * h) as usize;
    let scale = |v: u32| (v.min(maxv) * 255 / maxv) as u8;
    match kind {
        b'1' | b'2' | b'3' => {
            for i in 0..n {
                let c = if kind == b'1' {
                    // les chiffres peuvent être collés
                    let mut v = None;
                    while t.p < d.len() {
                        let c = d[t.p];
                        t.p += 1;
                        if c == b'0' || c == b'1' {
                            v = Some(c == b'1');
                            break;
                        }
                    }
                    let g = if v.ok_or(Error::Corrupt)? { 0 } else { 255 };
                    [g, g, g, 255]
                } else if kind == b'2' {
                    let g = scale(t.num().ok_or(Error::Corrupt)?);
                    [g, g, g, 255]
                } else {
                    [
                        scale(t.num().ok_or(Error::Corrupt)?),
                        scale(t.num().ok_or(Error::Corrupt)?),
                        scale(t.num().ok_or(Error::Corrupt)?),
                        255,
                    ]
                };
                r.px[i * 4..i * 4 + 4].copy_from_slice(&c);
            }
        }
        b'4' => {
            let p = t.p + 1;
            let stride = (w as usize).div_ceil(8);
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let b = *d.get(p + y * stride + x / 8).ok_or(Error::Corrupt)?;
                    let g = if (b >> (7 - x % 8)) & 1 == 1 { 0 } else { 255 };
                    r.set(x as u32, y as u32, [g, g, g, 255]);
                }
            }
        }
        b'5' | b'6' => {
            let p = t.p + 1;
            let ch = if kind == b'5' { 1 } else { 3 };
            let bs = if maxv > 255 { 2 } else { 1 };
            for i in 0..n {
                let mut c = [0u8; 4];
                c[3] = 255;
                for k in 0..ch {
                    let o = p + (i * ch + k) * bs;
                    let v = if bs == 2 {
                        be16(d.get(o..o + 2).ok_or(Error::Corrupt)?, 0) as u32
                    } else {
                        *d.get(o).ok_or(Error::Corrupt)? as u32
                    };
                    c[k] = scale(v);
                }
                if ch == 1 {
                    c[1] = c[0];
                    c[2] = c[0];
                }
                r.px[i * 4..i * 4 + 4].copy_from_slice(&c);
            }
        }
        _ => return Err(Error::Corrupt),
    }
    let name = match kind {
        b'1' | b'4' => "PBM",
        b'2' | b'5' => "PGM",
        _ => "PPM",
    };
    Ok(one(name, r))
}

fn pam(d: &[u8]) -> Result<Image, Error> {
    let end = d
        .windows(7)
        .position(|w| w == b"ENDHDR\n")
        .ok_or(Error::Corrupt)?;
    let head = std::str::from_utf8(&d[..end]).map_err(|_| Error::Corrupt)?;
    let mut w = 0;
    let mut h = 0;
    let mut depth = 0;
    let mut maxv = 255;
    for line in head.lines() {
        let mut it = line.split_whitespace();
        let k = it.next().unwrap_or("");
        let v: u32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        match k {
            "WIDTH" => w = v,
            "HEIGHT" => h = v,
            "DEPTH" => depth = v as usize,
            "MAXVAL" => maxv = v.max(1),
            _ => {}
        }
    }
    super::check_size(w, h)?;
    let p = end + 7;
    let bs = if maxv > 255 { 2 } else { 1 };
    let mut r = Raster::new(w, h);
    for i in 0..(w * h) as usize {
        let mut s = [0u32; 4];
        for (k, v) in s.iter_mut().enumerate().take(depth.min(4)) {
            let o = p + (i * depth + k) * bs;
            *v = if bs == 2 {
                be16(d.get(o..o + 2).ok_or(Error::Corrupt)?, 0) as u32
            } else {
                *d.get(o).ok_or(Error::Corrupt)? as u32
            };
            *v = (*v).min(maxv) * 255 / maxv;
        }
        let c = match depth {
            1 => [s[0], s[0], s[0], 255],
            2 => [s[0], s[0], s[0], s[1]],
            3 => [s[0], s[1], s[2], 255],
            _ => [s[0], s[1], s[2], s[3]],
        };
        r.px[i * 4..i * 4 + 4].copy_from_slice(&c.map(|v| v as u8));
    }
    Ok(one("PAM", r))
}

pub fn qoi(d: &[u8]) -> Result<Image, Error> {
    if d.len() < 22 || &d[..4] != b"qoif" {
        return Err(Error::Corrupt);
    }
    let w = be32(d, 4);
    let h = be32(d, 8);
    super::check_size(w, h)?;
    let n = (w * h) as usize;
    let mut r = Raster::new(w, h);
    let mut idx = [[0u8; 4]; 64];
    let mut px = [0u8, 0, 0, 255];
    let mut p = 14;
    let mut i = 0;
    while i < n {
        let b = *d.get(p).ok_or(Error::Corrupt)?;
        p += 1;
        let mut run = 1;
        match b {
            0xFE => {
                px[..3].copy_from_slice(d.get(p..p + 3).ok_or(Error::Corrupt)?);
                p += 3;
            }
            0xFF => {
                px.copy_from_slice(d.get(p..p + 4).ok_or(Error::Corrupt)?);
                p += 4;
            }
            _ => match b >> 6 {
                0 => px = idx[(b & 63) as usize],
                1 => {
                    px[0] = px[0].wrapping_add(((b >> 4) & 3).wrapping_sub(2));
                    px[1] = px[1].wrapping_add(((b >> 2) & 3).wrapping_sub(2));
                    px[2] = px[2].wrapping_add((b & 3).wrapping_sub(2));
                }
                2 => {
                    let b2 = *d.get(p).ok_or(Error::Corrupt)?;
                    p += 1;
                    let dg = (b & 63).wrapping_sub(32);
                    px[0] = px[0].wrapping_add(dg.wrapping_sub(8).wrapping_add(b2 >> 4));
                    px[1] = px[1].wrapping_add(dg);
                    px[2] = px[2].wrapping_add(dg.wrapping_sub(8).wrapping_add(b2 & 15));
                }
                _ => run = (b & 63) as usize + 1,
            },
        }
        let h =
            (px[0] as usize * 3 + px[1] as usize * 5 + px[2] as usize * 7 + px[3] as usize * 11)
                % 64;
        idx[h] = px;
        for _ in 0..run {
            if i < n {
                r.px[i * 4..i * 4 + 4].copy_from_slice(&px);
                i += 1;
            }
        }
    }
    Ok(one("QOI", r))
}

pub fn farbfeld(d: &[u8]) -> Result<Image, Error> {
    let w = be32(d, 8);
    let h = be32(d, 12);
    super::check_size(w, h)?;
    let mut r = Raster::new(w, h);
    for i in 0..(w * h) as usize {
        for k in 0..4 {
            r.px[i * 4 + k] = *d.get(16 + i * 8 + k * 2).ok_or(Error::Corrupt)?;
        }
    }
    Ok(one("farbfeld", r))
}

pub fn sgi(d: &[u8]) -> Result<Image, Error> {
    if d.len() < 512 || be16(d, 0) != 474 {
        return Err(Error::Corrupt);
    }
    let rle = d[2] == 1;
    let bpc = d[3] as usize;
    let w = be16(d, 6) as u32;
    let h = be16(d, 8) as u32;
    let zs = (be16(d, 10) as usize).clamp(1, 4);
    super::check_size(w, h)?;
    let mut r = Raster::new(w, h);
    let mut chan = vec![vec![0u8; (w * h) as usize]; zs];
    for (z, plane) in chan.iter_mut().enumerate() {
        for y in 0..h as usize {
            let row = &mut plane[y * w as usize..(y + 1) * w as usize];
            if rle {
                let ti = y + z * h as usize;
                let start = be32(d, 512 + ti * 4) as usize;
                let mut p = start;
                let mut x = 0;
                while x < row.len() {
                    let c = *d.get(p).ok_or(Error::Corrupt)?;
                    p += bpc;
                    let n = (c & 0x7F) as usize;
                    if n == 0 {
                        break;
                    }
                    if c & 0x80 != 0 {
                        for _ in 0..n {
                            if x < row.len() {
                                row[x] = *d.get(p).ok_or(Error::Corrupt)?;
                                x += 1;
                            }
                            p += bpc;
                        }
                    } else {
                        let v = *d.get(p).ok_or(Error::Corrupt)?;
                        p += bpc;
                        for _ in 0..n {
                            if x < row.len() {
                                row[x] = v;
                                x += 1;
                            }
                        }
                    }
                }
            } else {
                for (x, v) in row.iter_mut().enumerate() {
                    let o = 512 + ((z * h as usize + y) * w as usize + x) * bpc;
                    *v = *d.get(o).ok_or(Error::Corrupt)?;
                }
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let c = match zs {
                1 => [chan[0][i], chan[0][i], chan[0][i], 255],
                2 => [chan[0][i], chan[0][i], chan[0][i], chan[1][i]],
                3 => [chan[0][i], chan[1][i], chan[2][i], 255],
                _ => [chan[0][i], chan[1][i], chan[2][i], chan[3][i]],
            };
            r.set(x, h - 1 - y, c);
        }
    }
    Ok(one("SGI", r))
}

pub fn sun(d: &[u8]) -> Result<Image, Error> {
    if d.len() < 32 || be32(d, 0) != 0x59A6_6A95 {
        return Err(Error::Corrupt);
    }
    let w = be32(d, 4);
    let h = be32(d, 8);
    let depth = be32(d, 12) as usize;
    let kind = be32(d, 20);
    let maplen = be32(d, 28) as usize;
    super::check_size(w, h)?;
    let map = d.get(32..32 + maplen).ok_or(Error::Corrupt)?;
    let mut src = d.get(32 + maplen..).ok_or(Error::Corrupt)?.to_vec();
    if kind == 2 {
        let mut out = Vec::new();
        let mut i = 0;
        while i < src.len() {
            if src[i] == 0x80 {
                let n = *src.get(i + 1).unwrap_or(&0) as usize;
                if n == 0 {
                    out.push(0x80);
                    i += 2;
                } else {
                    let v = *src.get(i + 2).unwrap_or(&0);
                    out.extend(std::iter::repeat_n(v, n + 1));
                    i += 3;
                }
            } else {
                out.push(src[i]);
                i += 1;
            }
        }
        src = out;
    }
    let stride = (w as usize * depth).div_ceil(16) * 2;
    let ncol = maplen / 3;
    let mut r = Raster::new(w, h);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let row = src.get(y * stride..(y + 1) * stride).unwrap_or(&[]);
            let c = match depth {
                1 => {
                    let b = *row.get(x / 8).unwrap_or(&0);
                    let g = if (b >> (7 - x % 8)) & 1 == 1 { 0 } else { 255 };
                    [g, g, g, 255]
                }
                8 => {
                    let i = *row.get(x).unwrap_or(&0) as usize;
                    if ncol > 0 && i < ncol {
                        [map[i], map[ncol + i], map[2 * ncol + i], 255]
                    } else {
                        [i as u8, i as u8, i as u8, 255]
                    }
                }
                24 | 32 => {
                    let bs = depth / 8;
                    let o = x * bs + if bs == 4 { 1 } else { 0 };
                    let c = row.get(o..o + 3).unwrap_or(&[0, 0, 0]);
                    // 32 bits : octet de tête = alpha (ARGB) s'il est utilisé
                    let a = if bs == 4 {
                        *row.get(x * 4).unwrap_or(&0)
                    } else {
                        255
                    };
                    if kind == 3 {
                        [c[0], c[1], c[2], a]
                    } else {
                        [c[2], c[1], c[0], a]
                    }
                }
                _ => return Err(Error::Variant("Sun profondeur")),
            };
            r.set(x as u32, y as u32, c);
        }
    }
    if depth == 32 && r.px.chunks(4).all(|p| p[3] == 0) {
        for p in r.px.chunks_mut(4) {
            p[3] = 255;
        }
    }
    Ok(one("Sun Raster", r))
}

pub fn xbm(d: &[u8]) -> Result<Image, Error> {
    let s = String::from_utf8_lossy(d);
    let mut w = 0;
    let mut h = 0;
    for line in s.lines() {
        let l = line.trim();
        if l.starts_with("#define") {
            let mut it = l.split_whitespace().skip(1);
            let name = it.next().unwrap_or("");
            let v: u32 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            if name.ends_with("_width") {
                w = v;
            } else if name.ends_with("_height") {
                h = v;
            }
        }
    }
    super::check_size(w, h)?;
    let body = &s[s.find('{').ok_or(Error::Corrupt)? + 1..];
    let bytes: Vec<u8> = body
        .split(|c: char| c == ',' || c == '}' || c.is_whitespace())
        .filter_map(|t| {
            let t = t.trim();
            let t = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X"))?;
            u16::from_str_radix(t, 16).ok().map(|v| v as u8)
        })
        .collect();
    let stride = (w as usize).div_ceil(8);
    let mut r = Raster::new(w, h);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let b = *bytes.get(y * stride + x / 8).unwrap_or(&0);
            let g = if (b >> (x % 8)) & 1 == 1 { 0 } else { 255 };
            r.set(x as u32, y as u32, [g, g, g, 255]);
        }
    }
    Ok(one("XBM", r))
}

pub fn xpm(d: &[u8]) -> Result<Image, Error> {
    let s = String::from_utf8_lossy(d);
    let strings: Vec<&str> = s
        .split('"')
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, t)| t)
        .collect();
    let head: Vec<u32> = strings
        .first()
        .ok_or(Error::Corrupt)?
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    if head.len() < 4 {
        return Err(Error::Corrupt);
    }
    let (w, h, nc, cpp) = (head[0], head[1], head[2] as usize, head[3] as usize);
    super::check_size(w, h)?;
    let mut colors = std::collections::HashMap::new();
    for line in strings.iter().skip(1).take(nc) {
        if line.len() < cpp {
            continue;
        }
        let key = &line[..cpp];
        let rest: Vec<&str> = line[cpp..].split_whitespace().collect();
        let mut c = [0u8, 0, 0, 255];
        let mut i = 0;
        while i + 1 < rest.len() {
            if rest[i] == "c" || (rest[i] == "g" && c == [0, 0, 0, 255]) {
                c = super::svg::parse_color(rest[i + 1]).unwrap_or([0, 0, 0, 0]);
            }
            i += 2;
        }
        colors.insert(key.to_string(), c);
    }
    let mut r = Raster::new(w, h);
    for (y, line) in strings.iter().skip(1 + nc).take(h as usize).enumerate() {
        for x in 0..w as usize {
            if let Some(k) = line.get(x * cpp..(x + 1) * cpp) {
                r.set(x as u32, y as u32, *colors.get(k).unwrap_or(&[0, 0, 0, 0]));
            }
        }
    }
    Ok(one("XPM", r))
}

pub fn wbmp(d: &[u8]) -> Result<Image, Error> {
    if d.len() < 4 || d[0] != 0 || d[1] != 0 {
        return Err(Error::Corrupt);
    }
    let mut p = 2;
    let mut mb = || -> Option<u32> {
        let mut v = 0u32;
        loop {
            let b = *d.get(p)?;
            p += 1;
            v = (v << 7) | (b & 0x7F) as u32;
            if b & 0x80 == 0 {
                return Some(v);
            }
        }
    };
    let w = mb().ok_or(Error::Corrupt)?;
    let h = mb().ok_or(Error::Corrupt)?;
    super::check_size(w, h)?;
    let stride = (w as usize).div_ceil(8);
    if d.len() < p + stride * h as usize {
        return Err(Error::Corrupt);
    }
    let mut r = Raster::new(w, h);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let b = d[p + y * stride + x / 8];
            let g = if (b >> (7 - x % 8)) & 1 == 1 { 255 } else { 0 };
            r.set(x as u32, y as u32, [g, g, g, 255]);
        }
    }
    Ok(one("WBMP", r))
}

/// Radiance RGBE (.hdr) avec RLE moderne, tone mapping pour l'affichage.
pub fn hdr(d: &[u8]) -> Result<Image, Error> {
    let mut p = 0;
    let mut line = || -> Option<String> {
        let s = p;
        while p < d.len() && d[p] != b'\n' {
            p += 1;
        }
        let l = String::from_utf8_lossy(d.get(s..p)?).into_owned();
        p += 1;
        Some(l)
    };
    let first = line().ok_or(Error::Corrupt)?;
    if !first.starts_with("#?") {
        return Err(Error::Corrupt);
    }
    let mut exposure = 1.0f32;
    loop {
        let l = line().ok_or(Error::Corrupt)?;
        if l.is_empty() {
            break;
        }
        if let Some(v) = l.strip_prefix("EXPOSURE=") {
            exposure *= v.trim().parse::<f32>().unwrap_or(1.0);
        }
    }
    let res = line().ok_or(Error::Corrupt)?;
    let parts: Vec<&str> = res.split_whitespace().collect();
    if parts.len() < 4 {
        return Err(Error::Corrupt);
    }
    let h: u32 = parts[1].parse().map_err(|_| Error::Corrupt)?;
    let w: u32 = parts[3].parse().map_err(|_| Error::Corrupt)?;
    let flip_y = parts[0] == "+Y";
    super::check_size(w, h)?;
    let mut rgbe = vec![0u8; (w * h * 4) as usize];
    let wu = w as usize;
    for y in 0..h as usize {
        let row = &mut rgbe[y * wu * 4..(y + 1) * wu * 4];
        if (8..0x8000).contains(&wu)
            && d.get(p) == Some(&2)
            && d.get(p + 1) == Some(&2)
            && d.get(p + 2).is_some_and(|b| b & 0x80 == 0)
        {
            p += 4;
            for c in 0..4 {
                let mut x = 0;
                while x < wu {
                    let n = *d.get(p).ok_or(Error::Corrupt)? as usize;
                    p += 1;
                    if n > 128 {
                        let v = *d.get(p).ok_or(Error::Corrupt)?;
                        p += 1;
                        for _ in 0..n - 128 {
                            if x < wu {
                                row[x * 4 + c] = v;
                                x += 1;
                            }
                        }
                    } else {
                        for _ in 0..n {
                            if x < wu {
                                row[x * 4 + c] = *d.get(p).ok_or(Error::Corrupt)?;
                                x += 1;
                            }
                            p += 1;
                        }
                    }
                }
            }
        } else {
            let src = d.get(p..p + wu * 4).ok_or(Error::Corrupt)?;
            row.copy_from_slice(src);
            p += wu * 4;
        }
    }
    let mut r = Raster::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let e = rgbe[i + 3];
            let c = if e == 0 {
                [0, 0, 0, 255]
            } else {
                let f = 2f32.powi(e as i32 - 136) / exposure.max(1e-6);
                let t = |v: u8| (super::tonemap(v as f32 * f) * 255.0) as u8;
                [t(rgbe[i]), t(rgbe[i + 1]), t(rgbe[i + 2]), 255]
            };
            r.set(x, if flip_y { h - 1 - y } else { y }, c);
        }
    }
    let mut img = one("HDR", r);
    img.hdr = true;
    Ok(img)
}
