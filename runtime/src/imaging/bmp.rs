//! BMP/DIB (1 à 32 bits, RLE4/RLE8, masques), ICO/CUR (multi-tailles,
//! entrées PNG) et ICNS macOS (entrées PNG et ARGB/RLE).

use super::{be32, le16, le32, Error, Image, Page, Raster};

pub fn decode_bmp(d: &[u8]) -> Result<Image, Error> {
    if d.len() < 26 || &d[..2] != b"BM" {
        return Err(Error::Corrupt);
    }
    let off = le32(d, 10) as usize;
    let r = dib(&d[14..], Some(off.saturating_sub(14)), false)?;
    let mut img = Image::new("BMP");
    img.pages.push(Page::raster(String::new(), r));
    Ok(img)
}

/// Décode un DIB (BITMAPINFOHEADER et suivants). `icon` : hauteur doublée
/// avec masque AND.
pub fn dib(d: &[u8], pixel_off: Option<usize>, icon: bool) -> Result<Raster, Error> {
    let hs = le32(d, 0) as usize;
    let (w, mut h, bpp, comp, ncol);
    if hs == 12 {
        w = le16(d, 4) as i32;
        h = le16(d, 6) as i16 as i32;
        bpp = le16(d, 10) as u32;
        comp = 0;
        ncol = 0;
    } else {
        if d.len() < 40 {
            return Err(Error::Corrupt);
        }
        w = le32(d, 4) as i32;
        h = le32(d, 8) as i32;
        bpp = le16(d, 14) as u32;
        comp = le32(d, 16);
        ncol = le32(d, 32) as usize;
    }
    if icon {
        h /= 2;
    }
    let top_down = h < 0;
    let (w, h) = (w.unsigned_abs(), h.unsigned_abs());
    super::check_size(w, h)?;
    let mut masks = [0u32; 4];
    let mut pal_off = hs;
    if comp == 3 || comp == 6 {
        if hs >= 52 {
            for (i, m) in masks.iter_mut().enumerate().take(if hs >= 56 { 4 } else { 3 }) {
                *m = le32(d, 40 + i * 4);
            }
        } else {
            for (i, m) in masks.iter_mut().enumerate().take(3) {
                *m = le32(d, hs + i * 4);
            }
            pal_off += if comp == 6 { 16 } else { 12 };
        }
    } else if bpp == 16 {
        masks = [0x7C00, 0x03E0, 0x001F, 0];
    } else if bpp == 32 {
        // BI_RGB : l'octet haut est inutilisé, sauf dans les icônes.
        masks = [0xFF0000, 0xFF00, 0xFF, if icon { 0xFF000000 } else { 0 }];
    }
    let entry = if hs == 12 { 3 } else { 4 };
    let ncol = if bpp <= 8 {
        if ncol == 0 {
            1 << bpp
        } else {
            ncol.min(256)
        }
    } else {
        0
    };
    let pal: Vec<[u8; 4]> = (0..ncol)
        .map(|i| {
            let o = pal_off + i * entry;
            match d.get(o..o + 3) {
                Some(c) => [c[2], c[1], c[0], 255],
                None => [0, 0, 0, 255],
            }
        })
        .collect();
    let po = pixel_off.unwrap_or(pal_off + ncol * entry);
    let px = d.get(po..).ok_or(Error::Corrupt)?;
    if comp == 4 || comp == 5 {
        // JPEG/PNG intégré
        if px.starts_with(b"\x89PNG") {
            return super::png::decode(px).map(|mut i| i.pages.remove(0).into_raster());
        }
        return super::jpeg::decode_raster(px, None);
    }
    let mut r = Raster::new(w, h);
    let stride = ((w as usize * bpp as usize).div_ceil(32)) * 4;
    let row_of = |y: u32| if top_down { y } else { h - 1 - y };
    if comp == 1 || comp == 2 {
        // RLE8 / RLE4
        let (mut x, mut y) = (0u32, 0u32);
        let mut i = 0;
        let put = |r: &mut Raster, x: u32, y: u32, k: usize| {
            if x < w && y < h {
                r.set(x, row_of(y), *pal.get(k).unwrap_or(&[0, 0, 0, 255]));
            }
        };
        while i + 1 < px.len() {
            let (n, c) = (px[i], px[i + 1]);
            i += 2;
            if n > 0 {
                for k in 0..n as u32 {
                    let v = if comp == 1 {
                        c
                    } else if k % 2 == 0 {
                        c >> 4
                    } else {
                        c & 15
                    };
                    put(&mut r, x + k, y, v as usize);
                }
                x += n as u32;
            } else {
                match c {
                    0 => {
                        x = 0;
                        y += 1;
                    }
                    1 => break,
                    2 => {
                        x += *px.get(i).unwrap_or(&0) as u32;
                        y += *px.get(i + 1).unwrap_or(&0) as u32;
                        i += 2;
                    }
                    n => {
                        for k in 0..n as usize {
                            let v = if comp == 1 {
                                *px.get(i + k).unwrap_or(&0)
                            } else {
                                let b = *px.get(i + k / 2).unwrap_or(&0);
                                if k % 2 == 0 {
                                    b >> 4
                                } else {
                                    b & 15
                                }
                            };
                            put(&mut r, x + k as u32, y, v as usize);
                        }
                        x += n as u32;
                        let used = if comp == 1 { n as usize } else { (n as usize).div_ceil(2) };
                        i += used + (used & 1);
                    }
                }
            }
        }
        return Ok(r);
    }
    let mut any_alpha = false;
    for y in 0..h {
        let row = px.get(y as usize * stride..(y as usize + 1) * stride).unwrap_or(&[]);
        for x in 0..w {
            let c = match bpp {
                1 | 2 | 4 | 8 => {
                    let bit = x as usize * bpp as usize;
                    let b = *row.get(bit / 8).unwrap_or(&0);
                    let k = (b >> (8 - bpp as usize - bit % 8)) & ((1u16 << bpp) - 1) as u8;
                    *pal.get(k as usize).unwrap_or(&[0, 0, 0, 255])
                }
                24 => {
                    let o = x as usize * 3;
                    match row.get(o..o + 3) {
                        Some(c) => [c[2], c[1], c[0], 255],
                        None => [0, 0, 0, 255],
                    }
                }
                16 | 32 => {
                    let v = if bpp == 16 {
                        le16(row, x as usize * 2) as u32
                    } else {
                        le32(row, x as usize * 4)
                    };
                    let ch = |m: u32| -> u8 {
                        if m == 0 {
                            return 255;
                        }
                        let s = m.trailing_zeros();
                        let bits = (m >> s).count_ones();
                        let v = (v & m) >> s;
                        (v * 255 / ((1u64 << bits) - 1) as u32) as u8
                    };
                    let a = if masks[3] != 0 { ch(masks[3]) } else { 255 };
                    if a != 0 {
                        any_alpha = true;
                    }
                    [ch(masks[0]), ch(masks[1]), ch(masks[2]), a]
                }
                _ => return Err(Error::Variant("BMP profondeur")),
            };
            r.set(x, row_of(y), c);
        }
    }
    if bpp == 32 && !any_alpha {
        // Alpha à zéro partout : le canal n'est pas utilisé.
        for p in r.px.chunks_mut(4) {
            p[3] = 255;
        }
    }
    if icon && bpp < 32 {
        // masque AND 1 bit après les pixels
        let mstride = (w as usize).div_ceil(32) * 4;
        let mo = stride * h as usize;
        for y in 0..h {
            for x in 0..w {
                let b = px.get(mo + y as usize * mstride + x as usize / 8).copied().unwrap_or(0);
                if (b >> (7 - x % 8)) & 1 == 1 {
                    let ry = row_of(y);
                    let mut c = r.get(x, ry);
                    c[3] = 0;
                    r.set(x, ry, c);
                }
            }
        }
    }
    Ok(r)
}

pub fn decode_ico(d: &[u8]) -> Result<Image, Error> {
    let kind = le16(d, 2);
    let n = le16(d, 4) as usize;
    let mut img = Image::new(if kind == 2 { "CUR" } else { "ICO" });
    let mut entries: Vec<Raster> = Vec::new();
    for i in 0..n.min(64) {
        let e = 6 + i * 16;
        if e + 16 > d.len() {
            break;
        }
        let size = le32(d, e + 8) as usize;
        let off = le32(d, e + 12) as usize;
        let Some(b) = d.get(off..off.saturating_add(size).min(d.len())) else {
            continue;
        };
        let r = if b.starts_with(b"\x89PNG") {
            super::png::decode(b).map(|mut i| i.pages.remove(0).into_raster())
        } else {
            dib(b, None, true)
        };
        if let Ok(r) = r {
            entries.push(r);
        }
    }
    entries.sort_by_key(|r| std::cmp::Reverse(r.w * r.h));
    for r in entries {
        let name = format!("{}×{}", r.w, r.h);
        img.pages.push(Page::raster(name, r));
    }
    if img.pages.is_empty() {
        return Err(Error::Corrupt);
    }
    Ok(img)
}

pub fn decode_icns(d: &[u8]) -> Result<Image, Error> {
    if d.len() < 8 || &d[..4] != b"icns" {
        return Err(Error::Corrupt);
    }
    let mut img = Image::new("ICNS");
    let mut entries: Vec<Raster> = Vec::new();
    let mut p = 8;
    while p + 8 <= d.len() {
        let typ = &d[p..p + 4];
        let len = be32(d, p + 4) as usize;
        if len < 8 {
            break;
        }
        let body = d.get(p + 8..(p + len).min(d.len())).unwrap_or(&[]);
        p += len;
        if body.starts_with(b"\x89PNG") {
            if let Ok(mut i) = super::png::decode(body) {
                entries.push(i.pages.remove(0).into_raster());
            }
            continue;
        }
        let side = match typ {
            b"is32" => 16,
            b"il32" => 32,
            b"ih32" => 48,
            b"it32" => 128,
            b"ic04" | b"ic05" if body.starts_with(b"ARGB") => {
                let s = if typ == b"ic04" { 16 } else { 32 };
                if let Some(r) = icns_rle(&body[4..], s, 4) {
                    entries.push(r);
                }
                continue;
            }
            _ => continue,
        };
        let data = if typ == b"it32" { body.get(4..).unwrap_or(&[]) } else { body };
        if let Some(mut r) = icns_rle(data, side, 3) {
            // masque alpha séparé (s8mk, l8mk, h8mk, t8mk) : appliqué si présent
            let mk: &[u8] = match side {
                16 => b"s8mk",
                32 => b"l8mk",
                48 => b"h8mk",
                _ => b"t8mk",
            };
            if let Some(m) = find_chunk(d, mk) {
                for (i, a) in m.iter().take((side * side) as usize).enumerate() {
                    r.px[i * 4 + 3] = *a;
                }
            }
            entries.push(r);
        }
    }
    entries.sort_by_key(|r| std::cmp::Reverse(r.w * r.h));
    for r in entries {
        let name = format!("{}×{}", r.w, r.h);
        img.pages.push(Page::raster(name, r));
    }
    if img.pages.is_empty() {
        return Err(Error::Variant("ICNS JPEG 2000"));
    }
    Ok(img)
}

fn find_chunk<'a>(d: &'a [u8], typ: &[u8]) -> Option<&'a [u8]> {
    let mut p = 8;
    while p + 8 <= d.len() {
        let len = be32(d, p + 4) as usize;
        if len < 8 {
            return None;
        }
        if &d[p..p + 4] == typ {
            return d.get(p + 8..p + len);
        }
        p += len;
    }
    None
}

fn icns_rle(d: &[u8], side: u32, planes: usize) -> Option<Raster> {
    let n = (side * side) as usize;
    let mut chan = vec![0u8; n * planes];
    if d.len() == n * planes {
        chan.copy_from_slice(d);
    } else {
        let mut o = 0;
        let mut i = 0;
        while o < chan.len() && i < d.len() {
            let c = d[i];
            i += 1;
            if c & 0x80 == 0 {
                let k = c as usize + 1;
                for _ in 0..k {
                    *chan.get_mut(o)? = *d.get(i)?;
                    o += 1;
                    i += 1;
                }
            } else {
                let k = c as usize - 125;
                let v = *d.get(i)?;
                i += 1;
                for _ in 0..k {
                    if let Some(s) = chan.get_mut(o) {
                        *s = v;
                    }
                    o += 1;
                }
            }
        }
    }
    let mut r = Raster::new(side, side);
    for i in 0..n {
        let px = if planes == 4 {
            [chan[n + i], chan[2 * n + i], chan[3 * n + i], chan[i]]
        } else {
            [chan[i], chan[n + i], chan[2 * n + i], 255]
        };
        r.px[i * 4..i * 4 + 4].copy_from_slice(&px);
    }
    Some(r)
}
