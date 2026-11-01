//! OpenEXR (lignes de balayage) : compressions NONE, RLE, ZIPS et ZIP,
//! canaux HALF/FLOAT/UINT, tone mapping pour l'affichage.

use super::{le32, Error, Image, Page, Raster};

fn half(h: u16) -> f32 {
    let s = ((h >> 15) & 1) as u32;
    let e = ((h >> 10) & 31) as i32;
    let m = (h & 1023) as u32;
    let v = if e == 0 {
        m as f32 * 2f32.powi(-24)
    } else if e == 31 {
        if m == 0 {
            f32::INFINITY
        } else {
            0.0
        }
    } else {
        f32::from_bits(((e - 15 + 127) as u32) << 23 | (m << 13))
    };
    if s == 1 {
        -v
    } else {
        v
    }
}

pub fn decode(d: &[u8]) -> Result<Image, Error> {
    if d.len() < 8 || le32(d, 0) != 20000630 {
        return Err(Error::Corrupt);
    }
    let ver = le32(d, 4);
    if ver & 0x200 != 0 {
        return Err(Error::Variant("EXR tuilé"));
    }
    if ver & 0x1000 != 0 || ver & 0x800 != 0 {
        return Err(Error::Variant("EXR multi-parties"));
    }
    let mut p = 8;
    let mut chans: Vec<(String, u32)> = Vec::new();
    let mut comp = 0u8;
    let mut win = (0i32, 0i32, 0i32, 0i32);
    let cstr = |p: &mut usize| -> Option<String> {
        let s = *p;
        while *d.get(*p)? != 0 {
            *p += 1;
        }
        let r = String::from_utf8_lossy(&d[s..*p]).into_owned();
        *p += 1;
        Some(r)
    };
    loop {
        let name = cstr(&mut p).ok_or(Error::Corrupt)?;
        if name.is_empty() {
            break;
        }
        let typ = cstr(&mut p).ok_or(Error::Corrupt)?;
        let size = le32(d, p) as usize;
        p += 4;
        let v = d.get(p..p + size).ok_or(Error::Corrupt)?;
        match (name.as_str(), typ.as_str()) {
            ("channels", "chlist") => {
                let mut q = 0;
                while q < v.len() && v[q] != 0 {
                    let s = q;
                    while q < v.len() && v[q] != 0 {
                        q += 1;
                    }
                    let n = String::from_utf8_lossy(&v[s..q]).into_owned();
                    q += 1;
                    let pt = le32(v, q);
                    q += 16;
                    chans.push((n, pt));
                }
            }
            ("compression", _) => comp = *v.first().ok_or(Error::Corrupt)?,
            ("dataWindow", "box2i") => {
                win = (
                    le32(v, 0) as i32,
                    le32(v, 4) as i32,
                    le32(v, 8) as i32,
                    le32(v, 12) as i32,
                );
            }
            _ => {}
        }
        p += size;
    }
    let w = win.2.saturating_sub(win.0).saturating_add(1).max(0) as u32;
    let h = win.3.saturating_sub(win.1).saturating_add(1).max(0) as u32;
    super::check_size(w, h)?;
    // Tampon flottant de 16 octets par pixel : 64 mégapixels au plus (1 Gio).
    if w as u64 * h as u64 > 1 << 26 || chans.len() > 256 {
        return Err(Error::TooLarge);
    }
    if chans.is_empty() {
        return Err(Error::Corrupt);
    }
    let lines = match comp {
        0..=2 => 1,
        3 => 16,
        4 => return Err(Error::Variant("EXR PIZ")),
        5 => return Err(Error::Variant("EXR PXR24")),
        6 | 7 => return Err(Error::Variant("EXR B44")),
        _ => return Err(Error::Variant("EXR DWA")),
    };
    let chunks = h.div_ceil(lines) as usize;
    let table = p;
    let line_bytes: usize = chans
        .iter()
        .map(|(_, t)| if *t == 1 { 2 } else { 4 } * w as usize)
        .sum();
    let mut fl = vec![[0f32, 0.0, 0.0, 1.0]; (w * h) as usize];
    let idx = |n: &str| -> Option<usize> {
        match n.rsplit('.').next().unwrap_or(n) {
            "R" | "r" => Some(0),
            "G" | "g" => Some(1),
            "B" | "b" => Some(2),
            "A" | "a" => Some(3),
            "Y" | "y" => Some(4),
            _ => None,
        }
    };
    let has_rgb = chans.iter().any(|(n, _)| idx(n) == Some(0));
    for c in 0..chunks {
        let o = u64::from_le_bytes(
            d.get(table + c * 8..table + c * 8 + 8)
                .ok_or(Error::Corrupt)?
                .try_into()
                .unwrap_or([0; 8]),
        ) as usize;
        let y0 = le32(d, o) as i32 - win.1;
        let size = le32(d, o + 4) as usize;
        let src = d.get(o + 8..o + 8 + size).ok_or(Error::Corrupt)?;
        let n_lines = (lines as i32).min(h as i32 - y0).max(0) as usize;
        let expect = line_bytes * n_lines;
        if expect > 256 << 20 {
            return Err(Error::TooLarge);
        }
        let raw = if size == expect || comp == 0 {
            src.to_vec()
        } else {
            let mut t = if comp == 1 {
                let mut out = Vec::with_capacity(expect);
                let mut i = 0;
                while i < src.len() && out.len() < expect {
                    let n = src[i] as i8;
                    i += 1;
                    if n < 0 {
                        let k = (-(n as i32)) as usize;
                        out.extend_from_slice(src.get(i..i + k).ok_or(Error::Corrupt)?);
                        i += k;
                    } else {
                        let v = *src.get(i).ok_or(Error::Corrupt)?;
                        out.extend(std::iter::repeat_n(v, n as usize + 1));
                        i += 1;
                    }
                }
                out
            } else {
                super::inflate::inflate_zlib(src, expect + 16).ok_or(Error::Corrupt)?
            };
            for i in 1..t.len() {
                t[i] = (t[i - 1] as i32 + t[i] as i32 - 128) as u8;
            }
            let half_n = t.len().div_ceil(2);
            let mut out = vec![0u8; t.len()];
            for (k, v) in out.iter_mut().enumerate() {
                *v = if k % 2 == 0 {
                    t[k / 2]
                } else {
                    t[half_n + k / 2]
                };
            }
            out
        };
        let mut q = 0;
        for l in 0..n_lines {
            let y = y0 as usize + l;
            for (n, t) in chans.iter() {
                let slot = idx(n);
                for x in 0..w as usize {
                    let v = match t {
                        1 => {
                            let v = half(u16::from_le_bytes([
                                *raw.get(q).unwrap_or(&0),
                                *raw.get(q + 1).unwrap_or(&0),
                            ]));
                            q += 2;
                            v
                        }
                        2 => {
                            let v = f32::from_bits(le32(&raw, q));
                            q += 4;
                            v
                        }
                        _ => {
                            let v = le32(&raw, q) as f32 / u32::MAX as f32;
                            q += 4;
                            v
                        }
                    };
                    if y < h as usize {
                        let px = &mut fl[y * w as usize + x];
                        match slot {
                            Some(4) if !has_rgb => {
                                px[0] = v;
                                px[1] = v;
                                px[2] = v;
                            }
                            Some(k) if k < 4 => px[k] = v,
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    let mut r = Raster::new(w, h);
    for (i, px) in fl.iter().enumerate() {
        let c = [
            (super::tonemap(px[0]) * 255.0) as u8,
            (super::tonemap(px[1]) * 255.0) as u8,
            (super::tonemap(px[2]) * 255.0) as u8,
            (px[3].clamp(0.0, 1.0) * 255.0) as u8,
        ];
        r.px[i * 4..i * 4 + 4].copy_from_slice(&c);
    }
    let mut img = Image::new("OpenEXR");
    img.hdr = true;
    img.pages.push(Page::raster(String::new(), r));
    Ok(img)
}
