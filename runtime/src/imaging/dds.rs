//! DirectDraw Surface : non compressé (masques), BC1 à BC5 (DXT1-5, ATI1/2)
//! et en-tête DX10. Seule la première surface (niveau 0) est affichée.

use super::{le32, Error, Image, Page, Raster};

pub fn decode(d: &[u8]) -> Result<Image, Error> {
    if d.len() < 128 || &d[..4] != b"DDS " {
        return Err(Error::Corrupt);
    }
    let h = le32(d, 12);
    let w = le32(d, 16);
    super::check_size(w, h)?;
    let pf_flags = le32(d, 80);
    let four = &d[84..88];
    let bits = le32(d, 88);
    let masks = [le32(d, 92), le32(d, 96), le32(d, 100), le32(d, 104)];
    let mut off = 128;
    enum K {
        Bc(u8),
        Masks,
        Rgba8,
        Bgra8,
    }
    let kind = if pf_flags & 4 != 0 {
        match four {
            b"DXT1" => K::Bc(1),
            b"DXT2" | b"DXT3" => K::Bc(2),
            b"DXT4" | b"DXT5" => K::Bc(3),
            b"ATI1" | b"BC4U" => K::Bc(4),
            b"ATI2" | b"BC5U" => K::Bc(5),
            b"DX10" => {
                off = 148;
                match le32(d, 128) {
                    70..=72 => K::Bc(1),
                    73..=75 => K::Bc(2),
                    76..=78 => K::Bc(3),
                    79..=81 => K::Bc(4),
                    82..=84 => K::Bc(5),
                    27..=29 => K::Rgba8,
                    87 | 88 | 90..=93 => K::Bgra8,
                    94..=96 => return Err(Error::Variant("DDS BC6H")),
                    97..=99 => return Err(Error::Variant("DDS BC7")),
                    _ => return Err(Error::Variant("DDS format DXGI")),
                }
            }
            _ => return Err(Error::Variant("DDS FourCC")),
        }
    } else {
        K::Masks
    };
    let src = d.get(off..).ok_or(Error::Corrupt)?;
    let mut r = Raster::new(w, h);
    match kind {
        K::Bc(n) => {
            let bs = if n == 1 || n == 4 { 8 } else { 16 };
            let bw = w.div_ceil(4) as usize;
            let bh = h.div_ceil(4) as usize;
            if src.len() < bw * bh * bs {
                return Err(Error::Corrupt);
            }
            for by in 0..bh {
                for bx in 0..bw {
                    let b = &src[(by * bw + bx) * bs..][..bs];
                    let mut px = [[0u8; 4]; 16];
                    match n {
                        1 => bc1(b, &mut px, true),
                        2 => {
                            bc1(&b[8..], &mut px, false);
                            for (i, p) in px.iter_mut().enumerate() {
                                let a = (b[i / 2] >> ((i % 2) * 4)) & 15;
                                p[3] = a * 17;
                            }
                        }
                        3 => {
                            bc1(&b[8..], &mut px, false);
                            let a = bc4(b);
                            for (i, p) in px.iter_mut().enumerate() {
                                p[3] = a[i];
                            }
                        }
                        4 => {
                            let a = bc4(b);
                            for (i, p) in px.iter_mut().enumerate() {
                                *p = [a[i], a[i], a[i], 255];
                            }
                        }
                        _ => {
                            let rr = bc4(b);
                            let gg = bc4(&b[8..]);
                            for (i, p) in px.iter_mut().enumerate() {
                                // reconstruit Z pour les cartes normales
                                let x = rr[i] as f32 / 127.5 - 1.0;
                                let y = gg[i] as f32 / 127.5 - 1.0;
                                let z = (1.0 - x * x - y * y).max(0.0).sqrt();
                                *p = [rr[i], gg[i], ((z + 1.0) * 127.5) as u8, 255];
                            }
                        }
                    }
                    for (i, p) in px.iter().enumerate() {
                        let (x, y) = (
                            bx as u32 * 4 + (i % 4) as u32,
                            by as u32 * 4 + (i / 4) as u32,
                        );
                        if x < w && y < h {
                            r.set(x, y, *p);
                        }
                    }
                }
            }
        }
        K::Rgba8 | K::Bgra8 => {
            let bgra = matches!(kind, K::Bgra8);
            for i in 0..(w * h) as usize {
                let c = src.get(i * 4..i * 4 + 4).ok_or(Error::Corrupt)?;
                let px = if bgra {
                    [c[2], c[1], c[0], c[3]]
                } else {
                    [c[0], c[1], c[2], c[3]]
                };
                r.px[i * 4..i * 4 + 4].copy_from_slice(&px);
            }
        }
        K::Masks => {
            let bpp = (bits as usize).div_ceil(8).clamp(1, 4);
            let alpha = pf_flags & 1 != 0;
            let lum = pf_flags & 0x20000 != 0;
            for i in 0..(w * h) as usize {
                let b = src.get(i * bpp..i * bpp + bpp).ok_or(Error::Corrupt)?;
                let mut v = 0u32;
                for (k, &x) in b.iter().enumerate() {
                    v |= (x as u32) << (8 * k);
                }
                let ch = |m: u32| -> u8 {
                    if m == 0 {
                        return 0;
                    }
                    let s = m.trailing_zeros();
                    let n = (m >> s).count_ones();
                    (((v & m) >> s) as u64 * 255 / ((1u64 << n) - 1)) as u8
                };
                let a = if alpha { ch(masks[3]) } else { 255 };
                let px = if lum {
                    let g = ch(masks[0]);
                    [g, g, g, a]
                } else {
                    [ch(masks[0]), ch(masks[1]), ch(masks[2]), a]
                };
                r.px[i * 4..i * 4 + 4].copy_from_slice(&px);
            }
        }
    }
    let mut img = Image::new("DDS");
    img.pages.push(Page::raster(String::new(), r));
    Ok(img)
}

fn c565(v: u16) -> [u8; 4] {
    let r = ((v >> 11) & 31) as u32;
    let g = ((v >> 5) & 63) as u32;
    let b = (v & 31) as u32;
    [
        (r * 255 / 31) as u8,
        (g * 255 / 63) as u8,
        (b * 255 / 31) as u8,
        255,
    ]
}

fn bc1(b: &[u8], out: &mut [[u8; 4]; 16], punch: bool) {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let (a, bb) = (c565(c0), c565(c1));
    let mix = |x: u8, y: u8, wx: u32, wy: u32| ((x as u32 * wx + y as u32 * wy) / (wx + wy)) as u8;
    let mut pal = [a, bb, [0; 4], [0; 4]];
    if c0 > c1 || !punch {
        pal[2] = [
            mix(a[0], bb[0], 2, 1),
            mix(a[1], bb[1], 2, 1),
            mix(a[2], bb[2], 2, 1),
            255,
        ];
        pal[3] = [
            mix(a[0], bb[0], 1, 2),
            mix(a[1], bb[1], 1, 2),
            mix(a[2], bb[2], 1, 2),
            255,
        ];
    } else {
        pal[2] = [
            mix(a[0], bb[0], 1, 1),
            mix(a[1], bb[1], 1, 1),
            mix(a[2], bb[2], 1, 1),
            255,
        ];
        pal[3] = [0, 0, 0, 0];
    }
    let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    for (i, o) in out.iter_mut().enumerate() {
        *o = pal[((bits >> (i * 2)) & 3) as usize];
    }
}

fn bc4(b: &[u8]) -> [u8; 16] {
    let (a0, a1) = (b[0] as u32, b[1] as u32);
    let mut pal = [0u32; 8];
    pal[0] = a0;
    pal[1] = a1;
    if a0 > a1 {
        for i in 1..7 {
            pal[i + 1] = ((7 - i) as u32 * a0 + i as u32 * a1) / 7;
        }
    } else {
        for i in 1..5 {
            pal[i + 1] = ((5 - i) as u32 * a0 + i as u32 * a1) / 5;
        }
        pal[6] = 0;
        pal[7] = 255;
    }
    let mut bits = 0u64;
    for k in 0..6 {
        bits |= (b[2 + k] as u64) << (8 * k);
    }
    let mut out = [0u8; 16];
    for (i, o) in out.iter_mut().enumerate() {
        *o = pal[((bits >> (i * 3)) & 7) as usize] as u8;
    }
    out
}
