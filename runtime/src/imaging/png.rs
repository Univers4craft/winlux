//! Décodeur PNG maison (toutes profondeurs, palette, transparence, Adam7)
//! et APNG (images animées).

use super::inflate::inflate_zlib;
use super::{be16, be32, Error, Image, Page, Raster};

struct Header {
    w: u32,
    h: u32,
    depth: u8,
    color: u8,
    interlace: u8,
}

fn channels(color: u8) -> Option<usize> {
    Some(match color {
        0 => 1,
        2 => 3,
        3 => 1,
        4 => 2,
        6 => 4,
        _ => return None,
    })
}

struct Ctx<'a> {
    hd: &'a Header,
    plte: &'a [u8],
    trns: &'a [u8],
}

pub fn decode(data: &[u8]) -> Result<Image, Error> {
    if data.len() < 33 || &data[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(Error::Corrupt);
    }
    let mut p = 8;
    let mut hd = None;
    let mut plte: &[u8] = &[];
    let mut trns: &[u8] = &[];
    let mut idat = Vec::new();
    let mut exif: Option<Vec<u8>> = None;
    let mut texts: Vec<(String, String)> = Vec::new();
    // APNG
    let mut frames: Vec<(u32, u32, u32, u32, u32, u8, u8, Vec<u8>)> = Vec::new();
    let mut cur_fctl: Option<(u32, u32, u32, u32, u32, u8, u8)> = None;
    let mut cur_data: Vec<u8> = Vec::new();
    let mut default_is_frame = false;
    let mut animated = false;
    while p + 8 <= data.len() {
        let len = be32(data, p) as usize;
        let kind = &data[p + 4..p + 8];
        let body = data.get(p + 8..p + 8 + len).ok_or(Error::Corrupt)?;
        p += 12 + len;
        match kind {
            b"IHDR" if len >= 13 => {
                hd = Some(Header {
                    w: be32(body, 0),
                    h: be32(body, 4),
                    depth: body[8],
                    color: body[9],
                    interlace: body[12],
                });
            }
            b"PLTE" => plte = body,
            b"tRNS" => trns = body,
            b"IDAT" => {
                idat.extend_from_slice(body);
                if cur_fctl.is_some() && frames.is_empty() && cur_data.is_empty() {
                    default_is_frame = true;
                }
            }
            b"acTL" => animated = true,
            b"fcTL" if len >= 26 => {
                if let Some(f) = cur_fctl.take() {
                    frames.push((
                        f.0,
                        f.1,
                        f.2,
                        f.3,
                        f.4,
                        f.5,
                        f.6,
                        std::mem::take(&mut cur_data),
                    ));
                }
                let dn = be16(body, 20) as u32;
                let dd = be16(body, 22) as u32;
                let delay = (dn * 1000).checked_div(dd).unwrap_or(dn * 10);
                cur_fctl = Some((
                    be32(body, 4),
                    be32(body, 8),
                    be32(body, 12),
                    be32(body, 16),
                    delay,
                    body[24],
                    body[25],
                ));
            }
            b"fdAT" if len >= 4 => cur_data.extend_from_slice(&body[4..]),
            b"eXIf" => exif = Some(body.to_vec()),
            b"tEXt" => {
                if let Some(z) = body.iter().position(|&c| c == 0) {
                    let k = String::from_utf8_lossy(&body[..z]).into_owned();
                    let v: String = body[z + 1..].iter().map(|&c| c as char).collect();
                    texts.push((k, v));
                }
            }
            b"IEND" => break,
            _ => {}
        }
    }
    let hd = hd.ok_or(Error::Corrupt)?;
    super::check_size(hd.w, hd.h)?;
    let ctx = Ctx {
        hd: &hd,
        plte,
        trns,
    };
    let main = decode_frame(&ctx, hd.w, hd.h, &idat)?;
    let mut img = Image::new("PNG");
    if let Some(e) = exif {
        super::meta::exif(&e, &mut img);
    }
    for (k, v) in texts {
        if !v.is_empty() && v.len() < 200 {
            img.meta.push((k, v));
        }
    }
    if animated {
        if let Some(f) = cur_fctl.take() {
            frames.push((f.0, f.1, f.2, f.3, f.4, f.5, f.6, cur_data));
        }
    }
    if !animated || frames.len() < 2 {
        img.pages.push(Page::raster(String::new(), main));
        return Ok(img);
    }
    img.format = "APNG";
    img.animated = true;
    let mut canvas = Raster::new(hd.w, hd.h);
    for (i, (fw, fh, fx, fy, delay, dispose, blend, fdata)) in frames.into_iter().enumerate() {
        let frame = if i == 0 && default_is_frame {
            main.clone()
        } else {
            match decode_frame(&ctx, fw, fh, &fdata) {
                Ok(f) => f,
                Err(_) => break,
            }
        };
        let before = if dispose == 2 {
            Some(canvas.clone())
        } else {
            None
        };
        for y in 0..fh.min(frame.h) {
            for x in 0..fw.min(frame.w) {
                let (cx, cy) = (fx + x, fy + y);
                if cx >= hd.w || cy >= hd.h {
                    continue;
                }
                let s = frame.get(x, y);
                if blend == 0 {
                    canvas.set(cx, cy, s);
                } else {
                    let d = canvas.get(cx, cy);
                    canvas.set(cx, cy, super::over(s, d));
                }
            }
        }
        img.pages
            .push(Page::frame(canvas.clone(), delay.clamp(20, 10_000)));
        match dispose {
            1 => {
                for y in fy..(fy + fh).min(hd.h) {
                    for x in fx..(fx + fw).min(hd.w) {
                        canvas.set(x, y, [0, 0, 0, 0]);
                    }
                }
            }
            2 => canvas = before.unwrap_or(canvas),
            _ => {}
        }
    }
    if img.pages.is_empty() {
        img.pages.push(Page::raster(String::new(), main));
    }
    Ok(img)
}

fn decode_frame(ctx: &Ctx, w: u32, h: u32, z: &[u8]) -> Result<Raster, Error> {
    let hd = ctx.hd;
    let ch = channels(hd.color).ok_or(Error::Corrupt)?;
    let bpp_bits = ch * hd.depth as usize;
    let row_bytes = |pw: u32| (pw as usize * bpp_bits).div_ceil(8);
    let limit = (row_bytes(w) + 1) * h as usize * 2 + 1024;
    let raw = inflate_zlib(z, limit).ok_or(Error::Corrupt)?;
    let mut out = Raster::new(w, h);
    let bpp = bpp_bits.div_ceil(8).max(1);
    let passes: &[(u32, u32, u32, u32)] = if hd.interlace == 1 {
        &[
            (0, 0, 8, 8),
            (4, 0, 8, 8),
            (0, 4, 4, 8),
            (2, 0, 4, 4),
            (0, 2, 2, 4),
            (1, 0, 2, 2),
            (0, 1, 1, 2),
        ]
    } else {
        &[(0, 0, 1, 1)]
    };
    let mut pos = 0;
    for &(x0, y0, dx, dy) in passes {
        if x0 >= w || y0 >= h {
            continue;
        }
        let pw = (w - x0).div_ceil(dx);
        let ph = (h - y0).div_ceil(dy);
        let rb = row_bytes(pw);
        let mut prev = vec![0u8; rb];
        let mut cur = vec![0u8; rb];
        for py in 0..ph {
            let f = *raw.get(pos).ok_or(Error::Corrupt)?;
            let src = raw.get(pos + 1..pos + 1 + rb).ok_or(Error::Corrupt)?;
            pos += 1 + rb;
            unfilter(f, src, &prev, &mut cur, bpp)?;
            for px in 0..pw {
                let c = pixel(ctx, &cur, px as usize, ch);
                out.set(x0 + px * dx, y0 + (py * dy), c);
            }
            std::mem::swap(&mut prev, &mut cur);
        }
    }
    Ok(out)
}

fn unfilter(f: u8, src: &[u8], prev: &[u8], cur: &mut [u8], bpp: usize) -> Result<(), Error> {
    for i in 0..src.len() {
        let a = if i >= bpp { cur[i - bpp] as i16 } else { 0 };
        let b = prev[i] as i16;
        let c = if i >= bpp { prev[i - bpp] as i16 } else { 0 };
        let x = src[i];
        cur[i] = match f {
            0 => x,
            1 => x.wrapping_add(a as u8),
            2 => x.wrapping_add(b as u8),
            3 => x.wrapping_add(((a + b) / 2) as u8),
            4 => {
                let p = a + b - c;
                let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                let pr = if pa <= pb && pa <= pc {
                    a
                } else if pb <= pc {
                    b
                } else {
                    c
                };
                x.wrapping_add(pr as u8)
            }
            _ => return Err(Error::Corrupt),
        };
    }
    Ok(())
}

fn sample(row: &[u8], depth: u8, idx: usize) -> u16 {
    match depth {
        16 => ((row[idx * 2] as u16) << 8) | row[idx * 2 + 1] as u16,
        8 => row[idx] as u16,
        d => {
            let bit = idx * d as usize;
            let byte = row[bit / 8];
            let shift = 8 - d as usize - (bit % 8);
            ((byte >> shift) & ((1u8 << d) - 1)) as u16
        }
    }
}

fn pixel(ctx: &Ctx, row: &[u8], x: usize, ch: usize) -> [u8; 4] {
    let hd = ctx.hd;
    let d = hd.depth;
    let to8 = |v: u16| -> u8 {
        match d {
            16 => (v >> 8) as u8,
            8 => v as u8,
            _ => (v as u32 * 255 / ((1u32 << d) - 1)) as u8,
        }
    };
    match hd.color {
        0 => {
            let v = sample(row, d, x);
            let a = if ctx.trns.len() >= 2 && v == be16(ctx.trns, 0) {
                0
            } else {
                255
            };
            let g = to8(v);
            [g, g, g, a]
        }
        2 => {
            let r = sample(row, d, x * 3);
            let g = sample(row, d, x * 3 + 1);
            let b = sample(row, d, x * 3 + 2);
            let a = if ctx.trns.len() >= 6
                && r == be16(ctx.trns, 0)
                && g == be16(ctx.trns, 2)
                && b == be16(ctx.trns, 4)
            {
                0
            } else {
                255
            };
            [to8(r), to8(g), to8(b), a]
        }
        3 => {
            let i = sample(row, d, x) as usize;
            let c = ctx.plte.get(i * 3..i * 3 + 3).unwrap_or(&[0, 0, 0]);
            let a = ctx.trns.get(i).copied().unwrap_or(255);
            [c[0], c[1], c[2], a]
        }
        4 => {
            let g = to8(sample(row, d, x * 2));
            [g, g, g, to8(sample(row, d, x * 2 + 1))]
        }
        _ => {
            let _ = ch;
            [
                to8(sample(row, d, x * 4)),
                to8(sample(row, d, x * 4 + 1)),
                to8(sample(row, d, x * 4 + 2)),
                to8(sample(row, d, x * 4 + 3)),
            ]
        }
    }
}
