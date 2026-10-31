//! WebP maison : conteneur RIFF (simple, étendu, animé), VP8L sans perte,
//! VP8 avec perte (RFC 6386) et canal alpha ALPH.
#![allow(clippy::needless_range_loop)]

use super::vp8_tables::{AC_TABLE, BMODES_PROBA, COEFFS_PROBA0, COEFFS_UPDATE, DC_TABLE};
use super::{le32, Error, Image, Page, Raster};

fn le24(d: &[u8], p: usize) -> u32 {
    d.get(p..p + 3)
        .map(|b| b[0] as u32 | (b[1] as u32) << 8 | (b[2] as u32) << 16)
        .unwrap_or(0)
}

pub fn decode(d: &[u8]) -> Result<Image, Error> {
    let mut img = Image::new("WebP");
    let mut p = 12;
    let end = (le32(d, 4) as usize + 8).min(d.len());
    let (mut cw, mut ch) = (0u32, 0u32);
    let mut alph: Option<&[u8]> = None;
    let mut frames: Vec<(u32, u32, u32, u8, Raster)> = Vec::new();
    let mut single: Option<Raster> = None;
    while p + 8 <= end {
        let id = &d[p..p + 4];
        let len = le32(d, p + 4) as usize;
        let body = d.get(p + 8..(p + 8 + len).min(end)).ok_or(Error::Corrupt)?;
        p += 8 + len + (len & 1);
        match id {
            b"VP8X" if body.len() >= 10 => {
                cw = le24(body, 4) + 1;
                ch = le24(body, 7) + 1;
            }
            b"ALPH" => alph = Some(body),
            b"VP8 " => {
                let mut r = vp8(body)?;
                if let Some(a) = alph.take() {
                    apply_alpha(&mut r, a)?;
                }
                single = Some(r);
            }
            b"VP8L" => single = Some(vp8l(body)?),
            b"ANMF" if body.len() >= 16 => {
                let fx = le24(body, 0) * 2;
                let fy = le24(body, 3) * 2;
                let dur = le24(body, 12);
                let flags = body[15];
                let mut q = 16;
                let mut fa: Option<&[u8]> = None;
                while q + 8 <= body.len() {
                    let sid = &body[q..q + 4];
                    let sl = le32(body, q + 4) as usize;
                    let sb = body
                        .get(q + 8..(q + 8 + sl).min(body.len()))
                        .ok_or(Error::Corrupt)?;
                    q += 8 + sl + (sl & 1);
                    match sid {
                        b"ALPH" => fa = Some(sb),
                        b"VP8 " => {
                            let mut r = vp8(sb)?;
                            if let Some(a) = fa.take() {
                                apply_alpha(&mut r, a)?;
                            }
                            frames.push((fx, fy, dur, flags, r));
                        }
                        b"VP8L" => frames.push((fx, fy, dur, flags, vp8l(sb)?)),
                        _ => {}
                    }
                }
            }
            b"EXIF" => super::meta::exif(body.strip_prefix(b"Exif\0\0").unwrap_or(body), &mut img),
            b"XMP " => super::meta::xmp(body, &mut img),
            _ => {}
        }
    }
    if !frames.is_empty() {
        super::check_size(cw, ch)?;
        let mut canvas = Raster::new(cw, ch);
        for (fx, fy, dur, flags, r) in frames {
            for y in 0..r.h {
                for x in 0..r.w {
                    let (cx, cy) = (fx + x, fy + y);
                    if cx < cw && cy < ch {
                        let s = r.get(x, y);
                        let v = if flags & 2 != 0 {
                            s
                        } else {
                            super::over(s, canvas.get(cx, cy))
                        };
                        canvas.set(cx, cy, v);
                    }
                }
            }
            img.pages.push(Page::frame(canvas.clone(), dur.max(20)));
            if flags & 1 != 0 {
                for y in fy..(fy + r.h).min(ch) {
                    for x in fx..(fx + r.w).min(cw) {
                        canvas.set(x, y, [0; 4]);
                    }
                }
            }
        }
        img.animated = img.pages.len() > 1;
        return Ok(img);
    }
    let r = single.ok_or(Error::Corrupt)?;
    img.pages.push(Page::raster(String::new(), r));
    Ok(img)
}

// ---------------------------------------------------------------- VP8L

struct LBits<'a> {
    d: &'a [u8],
    pos: usize,
    acc: u64,
    n: u32,
}

impl<'a> LBits<'a> {
    fn new(d: &'a [u8]) -> Self {
        LBits {
            d,
            pos: 0,
            acc: 0,
            n: 0,
        }
    }
    fn fill(&mut self) {
        while self.n <= 56 {
            let b = self.d.get(self.pos).copied().unwrap_or(0) as u64;
            self.pos += 1;
            self.acc |= b << self.n;
            self.n += 8;
        }
    }
    fn get(&mut self, k: u32) -> u32 {
        if k == 0 {
            return 0;
        }
        self.fill();
        let v = (self.acc & ((1u64 << k) - 1)) as u32;
        self.acc >>= k;
        self.n -= k;
        v
    }
    fn eof(&self) -> bool {
        self.pos > self.d.len() + 8
    }
}

#[derive(Default, Clone)]
struct Code {
    /// table rapide : (longueur, symbole) indexée par 10 bits inversés
    fast: Vec<(u8, u16)>,
    counts: [u16; 16],
    symbols: Vec<u16>,
    single: Option<u16>,
}

const FAST: u32 = 10;

impl Code {
    fn from_lengths(lengths: &[u8]) -> Option<Code> {
        let nz: Vec<usize> = (0..lengths.len()).filter(|&i| lengths[i] > 0).collect();
        if nz.is_empty() {
            return Some(Code {
                single: Some(0),
                ..Default::default()
            });
        }
        if nz.len() == 1 {
            return Some(Code {
                single: Some(nz[0] as u16),
                ..Default::default()
            });
        }
        let mut counts = [0u16; 16];
        for &l in lengths {
            if l as usize >= 16 {
                return None;
            }
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut offs = [0u16; 16];
        for i in 1..16 {
            offs[i] = offs[i - 1] + counts[i - 1];
        }
        let mut symbols = vec![0u16; nz.len()];
        for (s, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
        // table rapide
        let mut fast = vec![(0u8, 0u16); 1 << FAST];
        let mut code = 0u32;
        let mut k = 0usize;
        for len in 1..16u32 {
            for _ in 0..counts[len as usize] {
                if len <= FAST {
                    let rev = code.reverse_bits() >> (32 - len);
                    let mut i = rev;
                    while i < (1 << FAST) {
                        fast[i as usize] = (len as u8, symbols[k]);
                        i += 1 << len;
                    }
                }
                code += 1;
                k += 1;
            }
            code <<= 1;
        }
        Some(Code {
            fast,
            counts,
            symbols,
            single: None,
        })
    }
    fn read(&self, b: &mut LBits) -> u16 {
        if let Some(s) = self.single {
            return s;
        }
        b.fill();
        let (l, s) = self.fast[(b.acc & ((1 << FAST) - 1)) as usize];
        if l > 0 {
            b.acc >>= l;
            b.n -= l as u32;
            return s;
        }
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for len in 1..16 {
            code |= b.get(1) as i32;
            let count = self.counts[len] as i32;
            if code - count < first {
                return self
                    .symbols
                    .get((index + (code - first)) as usize)
                    .copied()
                    .unwrap_or(0);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        0
    }
}

fn read_code(b: &mut LBits, alphabet: usize) -> Option<Code> {
    let mut lengths = vec![0u8; alphabet];
    if b.get(1) == 1 {
        let n = b.get(1) + 1;
        let first8 = b.get(1);
        let s0 = b.get(if first8 == 1 { 8 } else { 1 }) as usize;
        *lengths.get_mut(s0)? = 1;
        if n == 2 {
            let s1 = b.get(8) as usize;
            *lengths.get_mut(s1)? = 1;
        }
        return Code::from_lengths(&lengths);
    }
    const ORDER: [usize; 19] = [
        17, 18, 0, 1, 2, 3, 4, 5, 16, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    ];
    let ncl = b.get(4) as usize + 4;
    let mut cl = [0u8; 19];
    for &o in ORDER.iter().take(ncl) {
        cl[o] = b.get(3) as u8;
    }
    let clc = Code::from_lengths(&cl)?;
    let mut max_symbol = if b.get(1) == 1 {
        let nb = 2 + 2 * b.get(3);
        2 + b.get(nb) as usize
    } else {
        alphabet
    };
    if max_symbol > alphabet {
        return None;
    }
    let mut prev = 8u8;
    let mut s = 0;
    while s < alphabet {
        if max_symbol == 0 {
            break;
        }
        max_symbol -= 1;
        let c = clc.read(b);
        match c {
            0..=15 => {
                lengths[s] = c as u8;
                s += 1;
                if c != 0 {
                    prev = c as u8;
                }
            }
            16 => {
                let r = 3 + b.get(2) as usize;
                for _ in 0..r {
                    *lengths.get_mut(s)? = prev;
                    s += 1;
                }
            }
            17 => s += 3 + b.get(3) as usize,
            _ => s += 11 + b.get(7) as usize,
        }
        if s > alphabet || b.eof() {
            return None;
        }
    }
    Code::from_lengths(&lengths)
}

fn prefix_value(b: &mut LBits, sym: u32) -> u32 {
    if sym < 4 {
        return sym + 1;
    }
    let extra = (sym - 2) >> 1;
    let off = (2 + (sym & 1)) << extra;
    off + b.get(extra) + 1
}

const DIST_MAP: [(i8, i8); 120] = [
    (0, 1),
    (1, 0),
    (1, 1),
    (-1, 1),
    (0, 2),
    (2, 0),
    (1, 2),
    (-1, 2),
    (2, 1),
    (-2, 1),
    (2, 2),
    (-2, 2),
    (0, 3),
    (3, 0),
    (1, 3),
    (-1, 3),
    (3, 1),
    (-3, 1),
    (2, 3),
    (-2, 3),
    (3, 2),
    (-3, 2),
    (0, 4),
    (4, 0),
    (1, 4),
    (-1, 4),
    (4, 1),
    (-4, 1),
    (3, 3),
    (-3, 3),
    (2, 4),
    (-2, 4),
    (4, 2),
    (-4, 2),
    (0, 5),
    (3, 4),
    (-3, 4),
    (4, 3),
    (-4, 3),
    (5, 0),
    (1, 5),
    (-1, 5),
    (5, 1),
    (-5, 1),
    (2, 5),
    (-2, 5),
    (5, 2),
    (-5, 2),
    (4, 4),
    (-4, 4),
    (3, 5),
    (-3, 5),
    (5, 3),
    (-5, 3),
    (0, 6),
    (6, 0),
    (1, 6),
    (-1, 6),
    (6, 1),
    (-6, 1),
    (2, 6),
    (-2, 6),
    (6, 2),
    (-6, 2),
    (4, 5),
    (-4, 5),
    (5, 4),
    (-5, 4),
    (3, 6),
    (-3, 6),
    (6, 3),
    (-6, 3),
    (0, 7),
    (7, 0),
    (1, 7),
    (-1, 7),
    (5, 5),
    (-5, 5),
    (7, 1),
    (-7, 1),
    (4, 6),
    (-4, 6),
    (6, 4),
    (-6, 4),
    (2, 7),
    (-2, 7),
    (7, 2),
    (-7, 2),
    (3, 7),
    (-3, 7),
    (7, 3),
    (-7, 3),
    (5, 6),
    (-5, 6),
    (6, 5),
    (-6, 5),
    (8, 0),
    (4, 7),
    (-4, 7),
    (7, 4),
    (-7, 4),
    (8, 1),
    (8, 2),
    (6, 6),
    (-6, 6),
    (8, 3),
    (5, 7),
    (-5, 7),
    (7, 5),
    (-7, 5),
    (8, 4),
    (6, 7),
    (-6, 7),
    (7, 6),
    (-7, 6),
    (8, 5),
    (7, 7),
    (-7, 7),
    (8, 6),
    (8, 7),
];

enum Transform {
    Predictor(u32, Vec<u32>),
    Color(u32, Vec<u32>),
    SubGreen,
    Index(u32, Vec<u32>),
}

fn sub(size: u32, bits: u32) -> u32 {
    (size + (1 << bits) - 1) >> bits
}

fn decode_stream(b: &mut LBits, w: u32, h: u32, top: bool) -> Option<Vec<u32>> {
    let mut xsize = w;
    let mut transforms = Vec::new();
    if top {
        while b.get(1) == 1 {
            if transforms.len() >= 4 {
                return None;
            }
            match b.get(2) {
                0 => {
                    let bits = b.get(3) + 2;
                    let data = decode_stream(b, sub(xsize, bits), sub(h, bits), false)?;
                    transforms.push(Transform::Predictor(bits, data));
                }
                1 => {
                    let bits = b.get(3) + 2;
                    let data = decode_stream(b, sub(xsize, bits), sub(h, bits), false)?;
                    transforms.push(Transform::Color(bits, data));
                }
                2 => transforms.push(Transform::SubGreen),
                _ => {
                    let n = b.get(8) + 1;
                    let mut pal = decode_stream(b, n, 1, false)?;
                    for i in 1..pal.len() {
                        pal[i] = add_px(pal[i], pal[i - 1]);
                    }
                    let bits = if n <= 2 {
                        3
                    } else if n <= 4 {
                        2
                    } else if n <= 16 {
                        1
                    } else {
                        0
                    };
                    xsize = sub(xsize, bits);
                    transforms.push(Transform::Index(bits, pal));
                }
            }
        }
    }
    let cache_bits = if b.get(1) == 1 { b.get(4) } else { 0 };
    if cache_bits > 11 {
        return None;
    }
    let cache_size = if cache_bits > 0 {
        1usize << cache_bits
    } else {
        0
    };
    let (meta_bits, meta) = if top && b.get(1) == 1 {
        let bits = b.get(3) + 2;
        let m = decode_stream(b, sub(xsize, bits), sub(h, bits), false)?;
        (bits, Some(m))
    } else {
        (0, None)
    };
    let ngroups = meta
        .as_ref()
        .map(|m| m.iter().map(|&p| (p >> 8) & 0xFFFF).max().unwrap_or(0) as usize + 1)
        .unwrap_or(1);
    if ngroups > 4096 {
        return None;
    }
    let mut groups = Vec::with_capacity(ngroups);
    for _ in 0..ngroups {
        let g = [
            read_code(b, 256 + 24 + cache_size)?,
            read_code(b, 256)?,
            read_code(b, 256)?,
            read_code(b, 256)?,
            read_code(b, 40)?,
        ];
        groups.push(g);
    }
    let total = xsize as usize * h as usize;
    let mut px = vec![0u32; total];
    let mut cache = vec![0u32; cache_size];
    let mut i = 0usize;
    let mut cached_upto = 0usize;
    let mw = if meta.is_some() {
        sub(xsize, meta_bits)
    } else {
        1
    };
    while i < total {
        let x = (i % xsize as usize) as u32;
        let y = (i / xsize as usize) as u32;
        let g = match &meta {
            Some(m) => {
                &groups[((m[((y >> meta_bits) * mw + (x >> meta_bits)) as usize] >> 8) & 0xFFFF)
                    as usize]
            }
            None => &groups[0],
        };
        let s = g[0].read(b) as u32;
        if s < 256 {
            let r = g[1].read(b) as u32;
            let bl = g[2].read(b) as u32;
            let a = g[3].read(b) as u32;
            px[i] = (a << 24) | (r << 16) | (s << 8) | bl;
            i += 1;
        } else if s < 256 + 24 {
            let len = prefix_value(b, s - 256) as usize;
            let ds = g[4].read(b) as u32;
            let dc = prefix_value(b, ds) as usize;
            let dist = if dc > 120 {
                dc - 120
            } else {
                let (dx, dy) = DIST_MAP[dc - 1];
                let v = dx as isize + dy as isize * xsize as isize;
                if v < 1 {
                    1
                } else {
                    v as usize
                }
            };
            if dist > i || i + len > total {
                return None;
            }
            for k in 0..len {
                px[i + k] = px[i + k - dist];
            }
            i += len;
        } else {
            let k = (s - 280) as usize;
            px[i] = *cache.get(k)?;
            i += 1;
        }
        if cache_size > 0 {
            while cached_upto < i {
                let v = px[cached_upto];
                cache[(0x1e35_a7bd_u32.wrapping_mul(v) >> (32 - cache_bits)) as usize] = v;
                cached_upto += 1;
            }
        } else {
            cached_upto = i;
        }
        if b.eof() {
            return None;
        }
    }
    // transformations inverses, dans l'ordre inverse
    let mut cur_w = xsize;
    for t in transforms.iter().rev() {
        match t {
            Transform::Index(bits, pal) => {
                let out_w = w;
                let mut out = vec![0u32; out_w as usize * h as usize];
                let bpp = 8 >> bits;
                let mask = (1u32 << bpp) - 1;
                for y in 0..h as usize {
                    for x in 0..out_w as usize {
                        let packed = px[y * cur_w as usize + (x >> bits)];
                        let idx = if *bits == 0 {
                            (packed >> 8) & 0xFF
                        } else {
                            ((packed >> 8) >> ((x as u32 & ((1 << bits) - 1)) * bpp)) & mask
                        };
                        out[y * out_w as usize + x] = pal.get(idx as usize).copied().unwrap_or(0);
                    }
                }
                px = out;
                cur_w = out_w;
            }
            Transform::SubGreen => {
                for p in px.iter_mut() {
                    let g = (*p >> 8) & 0xFF;
                    let r = ((*p >> 16) + g) & 0xFF;
                    let bl = (*p + g) & 0xFF;
                    *p = (*p & 0xFF00_FF00) | (r << 16) | bl;
                }
            }
            Transform::Color(bits, data) => {
                let bw = sub(cur_w, *bits);
                for y in 0..h {
                    for x in 0..cur_w {
                        let e = data[((y >> bits) * bw + (x >> bits)) as usize];
                        let g2r = (e & 0xFF) as u8 as i8 as i32;
                        let g2b = ((e >> 8) & 0xFF) as u8 as i8 as i32;
                        let r2b = ((e >> 16) & 0xFF) as u8 as i8 as i32;
                        let i = (y * cur_w + x) as usize;
                        let p = px[i];
                        let g = ((p >> 8) & 0xFF) as u8 as i8 as i32;
                        let mut r = ((p >> 16) & 0xFF) as i32;
                        let mut bl = (p & 0xFF) as i32;
                        r = (r + ((g2r * g) >> 5)) & 0xFF;
                        bl = (bl + ((g2b * g) >> 5)) & 0xFF;
                        bl = (bl + ((r2b * (r as u8 as i8 as i32)) >> 5)) & 0xFF;
                        px[i] = (p & 0xFF00_FF00) | ((r as u32) << 16) | bl as u32;
                    }
                }
            }
            Transform::Predictor(bits, data) => {
                let bw = sub(cur_w, *bits);
                let wu = cur_w as usize;
                for y in 0..h as usize {
                    for x in 0..wu {
                        let i = y * wu + x;
                        let pred = if y == 0 && x == 0 {
                            0xFF00_0000
                        } else if y == 0 {
                            px[i - 1]
                        } else if x == 0 {
                            px[i - wu]
                        } else {
                            let mode = (data
                                [((y as u32 >> bits) * bw + (x as u32 >> bits)) as usize]
                                >> 8)
                                & 0xF;
                            let l = px[i - 1];
                            let t = px[i - wu];
                            let tl = px[i - wu - 1];
                            let tr = px[i - wu + 1];
                            predict(mode, l, t, tl, tr)
                        };
                        px[i] = add_px(px[i], pred);
                    }
                }
            }
        }
    }
    Some(px)
}

fn add_px(a: u32, b: u32) -> u32 {
    let ag = (a & 0xFF00_FF00).wrapping_add(b & 0xFF00_FF00) & 0xFF00_FF00;
    let rb = (a & 0x00FF_00FF).wrapping_add(b & 0x00FF_00FF) & 0x00FF_00FF;
    ag | rb
}

fn avg2(a: u32, b: u32) -> u32 {
    (((a ^ b) & 0xFEFE_FEFE) >> 1) + (a & b)
}

fn ch(p: u32, s: u32) -> i32 {
    ((p >> s) & 0xFF) as i32
}

fn predict(mode: u32, l: u32, t: u32, tl: u32, tr: u32) -> u32 {
    match mode {
        0 => 0xFF00_0000,
        1 => l,
        2 => t,
        3 => tr,
        4 => tl,
        5 => avg2(avg2(l, tr), t),
        6 => avg2(l, tl),
        7 => avg2(l, t),
        8 => avg2(tl, t),
        9 => avg2(t, tr),
        10 => avg2(avg2(l, tl), avg2(t, tr)),
        11 => {
            let mut pl = 0;
            let mut pt = 0;
            for s in [24, 16, 8, 0] {
                let pr = ch(l, s) + ch(t, s) - ch(tl, s);
                pl += (pr - ch(l, s)).abs();
                pt += (pr - ch(t, s)).abs();
            }
            if pl < pt {
                l
            } else {
                t
            }
        }
        12 => {
            let mut out = 0u32;
            for s in [24, 16, 8, 0] {
                let v = (ch(l, s) + ch(t, s) - ch(tl, s)).clamp(0, 255) as u32;
                out |= v << s;
            }
            out
        }
        13 => {
            let a = avg2(l, t);
            let mut out = 0u32;
            for s in [24, 16, 8, 0] {
                let v = ch(a, s);
                let v = (v + (v - ch(tl, s)) / 2).clamp(0, 255) as u32;
                out |= v << s;
            }
            out
        }
        _ => 0,
    }
}

fn argb_raster(px: &[u32], w: u32, h: u32) -> Raster {
    let mut r = Raster::new(w, h);
    for (i, &p) in px.iter().enumerate().take((w * h) as usize) {
        r.px[i * 4..i * 4 + 4].copy_from_slice(&[
            (p >> 16) as u8,
            (p >> 8) as u8,
            p as u8,
            (p >> 24) as u8,
        ]);
    }
    r
}

pub fn vp8l(d: &[u8]) -> Result<Raster, Error> {
    if d.len() < 5 || d[0] != 0x2F {
        return Err(Error::Corrupt);
    }
    let mut b = LBits::new(&d[1..]);
    let w = b.get(14) + 1;
    let h = b.get(14) + 1;
    let _alpha = b.get(1);
    if b.get(3) != 0 {
        return Err(Error::Corrupt);
    }
    super::check_size(w, h)?;
    let px = decode_stream(&mut b, w, h, true).ok_or(Error::Corrupt)?;
    Ok(argb_raster(&px, w, h))
}

fn apply_alpha(r: &mut Raster, a: &[u8]) -> Result<(), Error> {
    let hd = *a.first().ok_or(Error::Corrupt)?;
    let comp = hd & 3;
    let filt = (hd >> 2) & 3;
    let (w, h) = (r.w as usize, r.h as usize);
    let mut al: Vec<u8> = match comp {
        0 => a.get(1..1 + w * h).ok_or(Error::Corrupt)?.to_vec(),
        1 => {
            let mut b = LBits::new(&a[1..]);
            let px = decode_stream(&mut b, w as u32, h as u32, true).ok_or(Error::Corrupt)?;
            px.iter().map(|p| (p >> 8) as u8).collect()
        }
        _ => return Err(Error::Corrupt),
    };
    if filt != 0 {
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let pred = if x == 0 && y == 0 {
                    0
                } else if y == 0 {
                    al[i - 1] as i32
                } else if x == 0 {
                    al[i - w] as i32
                } else {
                    match filt {
                        1 => al[i - 1] as i32,
                        2 => al[i - w] as i32,
                        _ => (al[i - 1] as i32 + al[i - w] as i32 - al[i - w - 1] as i32)
                            .clamp(0, 255),
                    }
                };
                al[i] = al[i].wrapping_add(pred as u8);
            }
        }
    }
    for (i, v) in al.iter().enumerate().take(w * h) {
        r.px[i * 4 + 3] = *v;
    }
    Ok(())
}

// ---------------------------------------------------------------- VP8

struct Bool<'a> {
    d: &'a [u8],
    p: usize,
    value: u32,
    range: u32,
    count: u32,
}

impl<'a> Bool<'a> {
    fn new(d: &'a [u8]) -> Self {
        let b0 = d.first().copied().unwrap_or(0) as u32;
        let b1 = d.get(1).copied().unwrap_or(0) as u32;
        Bool {
            d,
            p: 2,
            value: (b0 << 8) | b1,
            range: 255,
            count: 0,
        }
    }
    fn bit(&mut self, prob: u8) -> bool {
        let split = 1 + (((self.range - 1) * prob as u32) >> 8);
        let big = split << 8;
        let r = if self.value >= big {
            self.range -= split;
            self.value -= big;
            true
        } else {
            self.range = split;
            false
        };
        while self.range < 128 {
            self.value <<= 1;
            self.range <<= 1;
            self.count += 1;
            if self.count == 8 {
                self.count = 0;
                self.value |= self.d.get(self.p).copied().unwrap_or(0) as u32;
                self.p += 1;
            }
        }
        r
    }
    fn lit(&mut self, n: u32) -> u32 {
        let mut v = 0;
        for _ in 0..n {
            v = (v << 1) | self.bit(128) as u32;
        }
        v
    }
    fn signed(&mut self, n: u32) -> i32 {
        let v = self.lit(n) as i32;
        if self.bit(128) {
            -v
        } else {
            v
        }
    }
    fn flag(&mut self) -> bool {
        self.bit(128)
    }
}

const ZIGZAG: [usize; 16] = [0, 1, 4, 8, 5, 2, 3, 6, 9, 12, 13, 10, 7, 11, 14, 15];
const BANDS: [usize; 17] = [0, 1, 2, 3, 6, 4, 5, 6, 6, 6, 6, 6, 6, 6, 6, 7, 0];
const CAT3: &[u8] = &[173, 148, 140];
const CAT4: &[u8] = &[176, 155, 140, 135];
const CAT5: &[u8] = &[180, 157, 141, 134, 130];
const CAT6: &[u8] = &[254, 254, 243, 230, 196, 177, 153, 140, 133, 130, 129];

type Probas = [[[[u8; 11]; 3]; 8]; 4];

fn large(b: &mut Bool, p: &[u8; 11]) -> i32 {
    if !b.bit(p[3]) {
        if !b.bit(p[4]) {
            2
        } else {
            3 + b.bit(p[5]) as i32
        }
    } else if !b.bit(p[6]) {
        if !b.bit(p[7]) {
            5 + b.bit(159) as i32
        } else {
            7 + 2 * b.bit(165) as i32 + b.bit(145) as i32
        }
    } else {
        let b1 = b.bit(p[8]) as usize;
        let b0 = b.bit(p[9 + b1]) as usize;
        let cat = 2 * b1 + b0;
        let tab = [CAT3, CAT4, CAT5, CAT6][cat];
        let mut v = 0i32;
        for &t in tab {
            v = 2 * v + b.bit(t) as i32;
        }
        v + 3 + (8 << cat)
    }
}

/// Lit les coefficients d'un bloc ; renvoie l'indice du dernier non nul + 1.
fn coeffs(
    b: &mut Bool,
    pr: &Probas,
    t: usize,
    ctx: usize,
    dq: (i32, i32),
    first: usize,
    out: &mut [i32; 16],
) -> usize {
    let mut n = first;
    let mut p = &pr[t][BANDS[n]][ctx];
    while n < 16 {
        if !b.bit(p[0]) {
            return n;
        }
        while !b.bit(p[1]) {
            n += 1;
            if n == 16 {
                return 16;
            }
            p = &pr[t][BANDS[n]][0];
        }
        let v;
        let next = BANDS[n + 1];
        if !b.bit(p[2]) {
            v = 1;
            p = &pr[t][next][1];
        } else {
            v = large(b, p);
            p = &pr[t][next][2];
        }
        let s = if b.flag() { -v } else { v };
        out[ZIGZAG[n]] = s * if n > 0 { dq.1 } else { dq.0 };
        n += 1;
    }
    16
}

fn mul1(a: i32) -> i32 {
    ((a * 20091) >> 16) + a
}
fn mul2(a: i32) -> i32 {
    (a * 35468) >> 16
}

/// IDCT 4x4 ajoutée à `dst` (pas `stride`).
fn idct_add(inp: &[i32; 16], dst: &mut [u8], off: usize, stride: usize) {
    let mut tmp = [0i32; 16];
    for i in 0..4 {
        let a = inp[i] + inp[8 + i];
        let b = inp[i] - inp[8 + i];
        let c = mul2(inp[4 + i]) - mul1(inp[12 + i]);
        let d = mul1(inp[4 + i]) + mul2(inp[12 + i]);
        tmp[i * 4] = a + d;
        tmp[i * 4 + 1] = b + c;
        tmp[i * 4 + 2] = b - c;
        tmp[i * 4 + 3] = a - d;
    }
    for i in 0..4 {
        let dc = tmp[i] + 4;
        let a = dc + tmp[8 + i];
        let b = dc - tmp[8 + i];
        let c = mul2(tmp[4 + i]) - mul1(tmp[12 + i]);
        let d = mul1(tmp[4 + i]) + mul2(tmp[12 + i]);
        let row = off + i * stride;
        for (k, v) in [a + d, b + c, b - c, a - d].into_iter().enumerate() {
            let o = row + k;
            dst[o] = (dst[o] as i32 + (v >> 3)).clamp(0, 255) as u8;
        }
    }
}

fn iwht(inp: &[i32; 16]) -> [i32; 16] {
    let mut tmp = [0i32; 16];
    for i in 0..4 {
        let a0 = inp[i] + inp[12 + i];
        let a1 = inp[4 + i] + inp[8 + i];
        let a2 = inp[4 + i] - inp[8 + i];
        let a3 = inp[i] - inp[12 + i];
        tmp[i] = a0 + a1;
        tmp[8 + i] = a0 - a1;
        tmp[4 + i] = a3 + a2;
        tmp[12 + i] = a3 - a2;
    }
    let mut out = [0i32; 16];
    for i in 0..4 {
        let dc = tmp[i * 4] + 3;
        let a0 = dc + tmp[3 + i * 4];
        let a1 = tmp[1 + i * 4] + tmp[2 + i * 4];
        let a2 = tmp[1 + i * 4] - tmp[2 + i * 4];
        let a3 = dc - tmp[3 + i * 4];
        out[i * 4] = (a0 + a1) >> 3;
        out[i * 4 + 1] = (a3 + a2) >> 3;
        out[i * 4 + 2] = (a0 - a1) >> 3;
        out[i * 4 + 3] = (a3 - a2) >> 3;
    }
    out
}

#[allow(dead_code)]
struct Mb {
    i4: bool,
    ymode: u8,
    bmodes: [u8; 16],
    uvmode: u8,
    segment: usize,
    skip: bool,
    coefs: Vec<[i32; 16]>, // 16 Y + 4 U + 4 V
    has: [bool; 24],
}

/// Plan avec bord : lecture hors image selon les règles VP8.
struct Plane {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

fn avg3(a: u8, b: u8, c: u8) -> u8 {
    ((a as u32 + 2 * b as u32 + c as u32 + 2) >> 2) as u8
}
fn avg2b(a: u8, b: u8) -> u8 {
    ((a as u32 + b as u32 + 1) >> 1) as u8
}

/// Prédiction d'un bloc carré (16 ou 8) à (x0,y0).
fn predict_block(pl: &mut Plane, x0: usize, y0: usize, size: usize, mode: u8) {
    let has_top = y0 > 0;
    let has_left = x0 > 0;
    let top: Vec<u8> = (0..size)
        .map(|i| {
            if has_top {
                pl.px[(y0 - 1) * pl.w + x0 + i]
            } else {
                127
            }
        })
        .collect();
    let left: Vec<u8> = (0..size)
        .map(|i| {
            if has_left {
                pl.px[(y0 + i) * pl.w + x0 - 1]
            } else {
                129
            }
        })
        .collect();
    let tl = if !has_top {
        127
    } else if !has_left {
        129
    } else {
        pl.px[(y0 - 1) * pl.w + x0 - 1]
    };
    let shift = if size == 16 { 4 } else { 3 };
    for y in 0..size {
        for x in 0..size {
            let v = match mode {
                0 => {
                    // DC
                    match (has_top, has_left) {
                        (true, true) => {
                            ((top.iter().map(|&v| v as u32).sum::<u32>()
                                + left.iter().map(|&v| v as u32).sum::<u32>()
                                + size as u32)
                                >> (shift + 1)) as u8
                        }
                        (true, false) => {
                            ((top.iter().map(|&v| v as u32).sum::<u32>() + (size as u32 / 2))
                                >> shift) as u8
                        }
                        (false, true) => {
                            ((left.iter().map(|&v| v as u32).sum::<u32>() + (size as u32 / 2))
                                >> shift) as u8
                        }
                        _ => 128,
                    }
                }
                2 => top[x],
                3 => left[y],
                _ => (top[x] as i32 + left[y] as i32 - tl as i32).clamp(0, 255) as u8,
            };
            pl.px[(y0 + y) * pl.w + x0 + x] = v;
        }
    }
}

/// Prédiction 4x4 ; `tr` : 4 pixels au-dessus à droite.
fn predict4(pl: &mut Plane, x0: usize, y0: usize, mode: u8, tr: [u8; 4]) {
    let w = pl.w;
    let at = |pl: &Plane, x: isize, y: isize| -> u8 {
        if y < 0 {
            127
        } else if x < 0 {
            129
        } else {
            pl.px[y as usize * w + x as usize]
        }
    };
    let (xi, yi) = (x0 as isize, y0 as isize);
    let mut top = [0u8; 8];
    for (i, t) in top.iter_mut().enumerate().take(4) {
        *t = at(pl, xi + i as isize, yi - 1);
    }
    top[4..8].copy_from_slice(&tr);
    let x = if yi == 0 { 127 } else { at(pl, xi - 1, yi - 1) };
    let l: Vec<u8> = (0..4).map(|i| at(pl, xi - 1, yi + i)).collect();
    let (i, j, k, ll) = (l[0], l[1], l[2], l[3]);
    let (a, b, c, d, e, f, g, h) = (
        top[0], top[1], top[2], top[3], top[4], top[5], top[6], top[7],
    );
    let mut o = [[0u8; 4]; 4]; // o[y][x]
    match mode {
        0 => {
            let dc = ((top[..4].iter().map(|&v| v as u32).sum::<u32>()
                + l.iter().map(|&v| v as u32).sum::<u32>()
                + 4)
                >> 3) as u8;
            o = [[dc; 4]; 4];
        }
        1 => {
            for (yy, row) in o.iter_mut().enumerate() {
                for (xx, v) in row.iter_mut().enumerate() {
                    *v = (top[xx] as i32 + l[yy] as i32 - x as i32).clamp(0, 255) as u8;
                }
            }
        }
        2 => {
            let v = [avg3(x, a, b), avg3(a, b, c), avg3(b, c, d), avg3(c, d, e)];
            o = [v; 4];
        }
        3 => {
            let v = [
                avg3(x, i, j),
                avg3(i, j, k),
                avg3(j, k, ll),
                avg3(k, ll, ll),
            ];
            for (yy, row) in o.iter_mut().enumerate() {
                *row = [v[yy]; 4];
            }
        }
        6 => {
            // LD
            let s = [
                avg3(a, b, c),
                avg3(b, c, d),
                avg3(c, d, e),
                avg3(d, e, f),
                avg3(e, f, g),
                avg3(f, g, h),
                avg3(g, h, h),
            ];
            for (yy, row) in o.iter_mut().enumerate() {
                for (xx, v) in row.iter_mut().enumerate() {
                    *v = s[xx + yy];
                }
            }
        }
        4 => {
            // RD
            let s = [
                avg3(j, k, ll),
                avg3(i, j, k),
                avg3(x, i, j),
                avg3(a, x, i),
                avg3(b, a, x),
                avg3(c, b, a),
                avg3(d, c, b),
            ];
            for (yy, row) in o.iter_mut().enumerate() {
                for (xx, v) in row.iter_mut().enumerate() {
                    *v = s[3 - yy + xx];
                }
            }
        }
        5 => {
            // VR
            o[0][0] = avg2b(x, a);
            o[2][1] = o[0][0];
            o[0][1] = avg2b(a, b);
            o[2][2] = o[0][1];
            o[0][2] = avg2b(b, c);
            o[2][3] = o[0][2];
            o[0][3] = avg2b(c, d);
            o[3][0] = avg3(k, j, i);
            o[2][0] = avg3(j, i, x);
            o[1][0] = avg3(i, x, a);
            o[3][1] = o[1][0];
            o[1][1] = avg3(x, a, b);
            o[3][2] = o[1][1];
            o[1][2] = avg3(a, b, c);
            o[3][3] = o[1][2];
            o[1][3] = avg3(b, c, d);
        }
        7 => {
            // VL
            o[0][0] = avg2b(a, b);
            o[0][1] = avg2b(b, c);
            o[2][0] = o[0][1];
            o[0][2] = avg2b(c, d);
            o[2][1] = o[0][2];
            o[0][3] = avg2b(d, e);
            o[2][2] = o[0][3];
            o[1][0] = avg3(a, b, c);
            o[1][1] = avg3(b, c, d);
            o[3][0] = o[1][1];
            o[1][2] = avg3(c, d, e);
            o[3][1] = o[1][2];
            o[1][3] = avg3(d, e, f);
            o[3][2] = o[1][3];
            o[2][3] = avg3(e, f, g);
            o[3][3] = avg3(f, g, h);
        }
        8 => {
            // HD
            o[0][0] = avg2b(i, x);
            o[1][2] = o[0][0];
            o[1][0] = avg2b(j, i);
            o[2][2] = o[1][0];
            o[2][0] = avg2b(k, j);
            o[3][2] = o[2][0];
            o[3][0] = avg2b(ll, k);
            o[0][3] = avg3(a, b, c);
            o[0][2] = avg3(x, a, b);
            o[0][1] = avg3(i, x, a);
            o[1][3] = o[0][1];
            o[1][1] = avg3(j, i, x);
            o[2][3] = o[1][1];
            o[2][1] = avg3(k, j, i);
            o[3][3] = o[2][1];
            o[3][1] = avg3(ll, k, j);
        }
        _ => {
            // HU
            o[0][0] = avg2b(i, j);
            o[0][2] = avg2b(j, k);
            o[1][0] = o[0][2];
            o[1][2] = avg2b(k, ll);
            o[2][0] = o[1][2];
            o[0][1] = avg3(i, j, k);
            o[0][3] = avg3(j, k, ll);
            o[1][1] = o[0][3];
            o[1][3] = avg3(k, ll, ll);
            o[2][1] = o[1][3];
            o[2][2] = ll;
            o[2][3] = ll;
            o[3] = [ll; 4];
        }
    }
    for (yy, row) in o.iter().enumerate() {
        let p = (y0 + yy) * w + x0;
        pl.px[p..p + 4].copy_from_slice(row);
    }
}

pub fn vp8(d: &[u8]) -> Result<Raster, Error> {
    if d.len() < 10 {
        return Err(Error::Corrupt);
    }
    let tag = d[0] as u32 | (d[1] as u32) << 8 | (d[2] as u32) << 16;
    if tag & 1 != 0 {
        return Err(Error::Variant("VP8 interframe"));
    }
    let part0 = (tag >> 5) as usize;
    if d[3..6] != [0x9D, 0x01, 0x2A] {
        return Err(Error::Corrupt);
    }
    let w = (super::le16(d, 6) & 0x3FFF) as u32;
    let h = (super::le16(d, 8) & 0x3FFF) as u32;
    super::check_size(w, h)?;
    let first = d.get(10..10 + part0).ok_or(Error::Corrupt)?;
    let mut b = Bool::new(first);
    let _cs = b.flag();
    let _clamp = b.flag();
    // segmentation
    let use_seg = b.flag();
    let mut update_map = false;
    let mut abs_delta = false;
    let mut seg_q = [0i32; 4];
    let mut seg_lf = [0i32; 4];
    let mut seg_probs = [255u8; 3];
    if use_seg {
        update_map = b.flag();
        if b.flag() {
            abs_delta = b.flag();
            for q in seg_q.iter_mut() {
                *q = if b.flag() { b.signed(7) } else { 0 };
            }
            for l in seg_lf.iter_mut() {
                *l = if b.flag() { b.signed(6) } else { 0 };
            }
        }
        if update_map {
            for p in seg_probs.iter_mut() {
                *p = if b.flag() { b.lit(8) as u8 } else { 255 };
            }
        }
    }
    // filtre
    let simple = b.flag();
    let level = b.lit(6) as i32;
    let sharp = b.lit(3) as i32;
    let mut ref_delta = [0i32; 4];
    let mut mode_delta = [0i32; 4];
    let use_lf_delta = b.flag();
    if use_lf_delta && b.flag() {
        for r in ref_delta.iter_mut() {
            if b.flag() {
                *r = b.signed(6);
            }
        }
        for m in mode_delta.iter_mut() {
            if b.flag() {
                *m = b.signed(6);
            }
        }
    }
    let nparts = 1usize << b.lit(2);
    // partitions DCT
    let mut parts: Vec<Bool> = Vec::with_capacity(nparts);
    let mut ps = 10 + part0;
    let sizes_at = ps;
    ps += 3 * (nparts - 1);
    for i in 0..nparts {
        let size = if i + 1 < nparts {
            le24(d, sizes_at + i * 3) as usize
        } else {
            d.len().saturating_sub(ps)
        };
        let slice = d.get(ps..(ps + size).min(d.len())).unwrap_or(&[]);
        parts.push(Bool::new(slice));
        ps += size;
    }
    // quantification
    let base_q = b.lit(7) as i32;
    let mut dq = [0i32; 5];
    for v in dq.iter_mut() {
        *v = if b.flag() { b.signed(4) } else { 0 };
    }
    let clip = |v: i32, m: i32| v.clamp(0, m) as usize;
    let mut quant = [[(0i32, 0i32); 3]; 4]; // [seg][y1,y2,uv]
    for (s, qm) in quant.iter_mut().enumerate() {
        let q = if use_seg {
            seg_q[s] + if abs_delta { 0 } else { base_q }
        } else {
            base_q
        };
        let y2ac = ((AC_TABLE[clip(q + dq[2], 127)] as i32 * 101_581) >> 16).max(8);
        *qm = [
            (
                DC_TABLE[clip(q + dq[0], 127)] as i32,
                AC_TABLE[clip(q, 127)] as i32,
            ),
            (DC_TABLE[clip(q + dq[1], 127)] as i32 * 2, y2ac),
            (
                DC_TABLE[clip(q + dq[3], 117)] as i32,
                AC_TABLE[clip(q + dq[4], 127)] as i32,
            ),
        ];
    }
    let _refresh = b.flag();
    let mut pr: Probas = [[[[0; 11]; 3]; 8]; 4];
    let mut k = 0;
    for t in pr.iter_mut() {
        for band in t.iter_mut() {
            for c in band.iter_mut() {
                for p in c.iter_mut() {
                    *p = if b.bit(COEFFS_UPDATE[k]) {
                        b.lit(8) as u8
                    } else {
                        COEFFS_PROBA0[k]
                    };
                    k += 1;
                }
            }
        }
    }
    let use_skip = b.flag();
    let skip_p = if use_skip { b.lit(8) as u8 } else { 0 };
    let mbw = w.div_ceil(16) as usize;
    let mbh = h.div_ceil(16) as usize;
    let mut yp = Plane {
        w: mbw * 16,
        h: mbh * 16,
        px: vec![0; mbw * mbh * 256],
    };
    let mut up = Plane {
        w: mbw * 8,
        h: mbh * 8,
        px: vec![0; mbw * mbh * 64],
    };
    let mut vp = Plane {
        w: mbw * 8,
        h: mbh * 8,
        px: vec![0; mbw * mbh * 64],
    };
    let mut intra_t = vec![0u8; mbw * 4];
    // contextes non nuls : [y 4 bits, u 2 bits, v 2 bits, dc]
    let mut nz_top = vec![[false; 9]; mbw];
    let mut finfo: Vec<(u8, bool)> = vec![(0, false); mbw * mbh]; // (segment, i4|has coefs)
    let mut mbinfo: Vec<(usize, bool, bool)> = vec![(0, false, false); mbw * mbh];
    for my in 0..mbh {
        let mut intra_l = [0u8; 4];
        let mut nz_left = [false; 9];
        let part = &mut parts[my & (nparts - 1)];
        for mx in 0..mbw {
            // en-tête du macrobloc
            let segment = if update_map {
                if !b.bit(seg_probs[0]) {
                    b.bit(seg_probs[1]) as usize
                } else {
                    2 + b.bit(seg_probs[2]) as usize
                }
            } else {
                0
            };
            let skip = use_skip && b.bit(skip_p);
            let i4 = !b.bit(145);
            let mut mb = Mb {
                i4,
                ymode: 0,
                bmodes: [0; 16],
                uvmode: 0,
                segment,
                skip,
                coefs: vec![[0; 16]; 24],
                has: [false; 24],
            };
            if !i4 {
                // numérotation commune : 0 DC, 1 TM, 2 V, 3 H
                let ym = if b.bit(156) {
                    if b.bit(128) {
                        1
                    } else {
                        3
                    }
                } else if b.bit(163) {
                    2
                } else {
                    0
                };
                mb.ymode = ym;
                intra_t[mx * 4..mx * 4 + 4].fill(ym);
                intra_l = [ym; 4];
            } else {
                for y in 0..4 {
                    let mut ym = intra_l[y];
                    for x in 0..4 {
                        let top = intra_t[mx * 4 + x] as usize;
                        let base = (top * 10 + ym as usize) * 9;
                        let p = &BMODES_PROBA[base..base + 9];
                        ym = if !b.bit(p[0]) {
                            0
                        } else if !b.bit(p[1]) {
                            1
                        } else if !b.bit(p[2]) {
                            2
                        } else if !b.bit(p[3]) {
                            if !b.bit(p[4]) {
                                3
                            } else if !b.bit(p[5]) {
                                4
                            } else {
                                5
                            }
                        } else if !b.bit(p[6]) {
                            6
                        } else if !b.bit(p[7]) {
                            7
                        } else if !b.bit(p[8]) {
                            8
                        } else {
                            9
                        };
                        intra_t[mx * 4 + x] = ym;
                        mb.bmodes[y * 4 + x] = ym;
                    }
                    intra_l[y] = ym;
                }
            }
            mb.uvmode = if !b.bit(142) {
                0
            } else if !b.bit(114) {
                2
            } else if b.bit(183) {
                1
            } else {
                3
            };
            // résidus
            let q = quant[mb.segment];
            let mut any = false;
            if !skip {
                let first;
                let ytype;
                if !i4 {
                    let mut dc = [0i32; 16];
                    let ctx = nz_top[mx][8] as usize + nz_left[8] as usize;
                    let n = coeffs(part, &pr, 1, ctx, q[1], 0, &mut dc);
                    nz_top[mx][8] = n > 0;
                    nz_left[8] = n > 0;
                    let wht = if n > 1 {
                        iwht(&dc)
                    } else {
                        [(dc[0] + 3) >> 3; 16]
                    };
                    for (i, v) in wht.iter().enumerate() {
                        mb.coefs[i][0] = *v;
                        if *v != 0 {
                            mb.has[i] = true;
                        }
                    }
                    first = 1;
                    ytype = 0;
                } else {
                    first = 0;
                    ytype = 3;
                }
                for y in 0..4 {
                    for x in 0..4 {
                        let ctx = nz_top[mx][x] as usize + nz_left[y] as usize;
                        let i = y * 4 + x;
                        let mut c = mb.coefs[i];
                        let n = coeffs(part, &pr, ytype, ctx, q[0], first, &mut c);
                        mb.coefs[i] = c;
                        let nz = n > first;
                        nz_top[mx][x] = nz;
                        nz_left[y] = nz;
                        if n > first || c[0] != 0 {
                            mb.has[i] = true;
                        }
                    }
                }
                for (plane, base) in [(0usize, 16usize), (1, 20)] {
                    for y in 0..2 {
                        for x in 0..2 {
                            let ti = 4 + plane * 2 + x;
                            let li = 4 + plane * 2 + y;
                            let ctx = nz_top[mx][ti] as usize + nz_left[li] as usize;
                            let i = base + y * 2 + x;
                            let mut c = [0i32; 16];
                            let n = coeffs(part, &pr, 2, ctx, q[2], 0, &mut c);
                            mb.coefs[i] = c;
                            nz_top[mx][ti] = n > 0;
                            nz_left[li] = n > 0;
                            mb.has[i] = n > 0;
                        }
                    }
                }
                any = mb.has.iter().any(|&h| h);
            } else {
                for v in nz_top[mx][..8].iter_mut() {
                    *v = false;
                }
                for v in nz_left[..8].iter_mut() {
                    *v = false;
                }
                if !i4 {
                    nz_top[mx][8] = false;
                    nz_left[8] = false;
                }
            }
            finfo[my * mbw + mx] = (mb.segment as u8, i4 || any);
            mbinfo[my * mbw + mx] = (mb.segment, i4, i4 || any);
            // reconstruction
            let (x0, y0) = (mx * 16, my * 16);
            if !i4 {
                predict_block(&mut yp, x0, y0, 16, mb.ymode);
                for i in 0..16 {
                    if mb.has[i] {
                        let off = (y0 + (i / 4) * 4) * yp.w + x0 + (i % 4) * 4;
                        let stride = yp.w;
                        idct_add(&mb.coefs[i], &mut yp.px, off, stride);
                    }
                }
            } else {
                // pixels au-dessus à droite du macrobloc
                let tr_mb: [u8; 4] = if my == 0 {
                    [127; 4]
                } else if mx + 1 >= mbw {
                    [yp.px[(y0 - 1) * yp.w + x0 + 15]; 4]
                } else {
                    let p = (y0 - 1) * yp.w + x0 + 16;
                    [yp.px[p], yp.px[p + 1], yp.px[p + 2], yp.px[p + 3]]
                };
                for i in 0..16 {
                    let (bx, by) = (i % 4, i / 4);
                    let (px0, py0) = (x0 + bx * 4, y0 + by * 4);
                    let tr = if bx == 3 {
                        tr_mb
                    } else if py0 == 0 {
                        [127; 4]
                    } else {
                        let p = (py0 - 1) * yp.w + px0 + 4;
                        [yp.px[p], yp.px[p + 1], yp.px[p + 2], yp.px[p + 3]]
                    };
                    predict4(&mut yp, px0, py0, mb.bmodes[i], tr);
                    if mb.has[i] {
                        let off = py0 * yp.w + px0;
                        let stride = yp.w;
                        idct_add(&mb.coefs[i], &mut yp.px, off, stride);
                    }
                }
            }
            for (pl, base) in [(&mut up, 16usize), (&mut vp, 20)] {
                let (cx, cy) = (mx * 8, my * 8);
                predict_block(pl, cx, cy, 8, mb.uvmode);
                for k in 0..4 {
                    if mb.has[base + k] {
                        let off = (cy + (k / 2) * 4) * pl.w + cx + (k % 2) * 4;
                        let stride = pl.w;
                        idct_add(&mb.coefs[base + k], &mut pl.px, off, stride);
                    }
                }
            }
        }
    }
    // filtre de boucle
    if level > 0 {
        let mut strengths = [[(0i32, 0i32, 0i32); 2]; 4];
        for (s, st) in strengths.iter_mut().enumerate() {
            let mut base = if use_seg {
                seg_lf[s] + if abs_delta { 0 } else { level }
            } else {
                level
            };
            let _ = &mut base;
            for (i4, slot) in st.iter_mut().enumerate() {
                let mut lv = base;
                if use_lf_delta {
                    lv += ref_delta[0];
                    if i4 == 1 {
                        lv += mode_delta[0];
                    }
                }
                let lv = lv.clamp(0, 63);
                if lv > 0 {
                    let mut il = lv;
                    if sharp > 0 {
                        il >>= if sharp > 4 { 2 } else { 1 };
                        il = il.min(9 - sharp);
                    }
                    let il = il.max(1);
                    let hev = if lv >= 40 {
                        2
                    } else if lv >= 15 {
                        1
                    } else {
                        0
                    };
                    *slot = (2 * lv + il, il, hev);
                }
            }
        }
        for my in 0..mbh {
            for mx in 0..mbw {
                let (seg, i4, inner) = mbinfo[my * mbw + mx];
                let (limit, il, hev) = strengths[seg][i4 as usize];
                if limit == 0 {
                    continue;
                }
                filter_mb(
                    &mut yp, &mut up, &mut vp, mx, my, limit, il, hev, inner, simple,
                );
            }
        }
    }
    let _ = finfo;
    // YUV -> RGB (suréchantillonnage bilinéaire de la chrominance)
    let mut r = Raster::new(w, h);
    let cw = up.w as i32;
    let chh = up.h as i32;
    let csample = |pl: &Plane, x: i32, y: i32| -> i32 {
        pl.px[(y.clamp(0, chh - 1) * cw + x.clamp(0, cw - 1)) as usize] as i32
    };
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let yy = yp.px[(y as usize) * yp.w + x as usize] as i32;
            // centre chroma : (x - 0.5)/2
            let cx = (x - 1) >> 1;
            let cy = (y - 1) >> 1;
            let fx = if x & 1 == 0 { 3 } else { 1 };
            let fy = if y & 1 == 0 { 3 } else { 1 };
            let sample = |pl: &Plane| -> i32 {
                let a = csample(pl, cx, cy);
                let b = csample(pl, cx + 1, cy);
                let c = csample(pl, cx, cy + 1);
                let dd = csample(pl, cx + 1, cy + 1);
                let (wx0, wx1) = (4 - fx, fx);
                let (wy0, wy1) = (4 - fy, fy);
                (a * wx0 * wy0 + b * wx1 * wy0 + c * wx0 * wy1 + dd * wx1 * wy1 + 8) >> 4
            };
            let u = sample(&up);
            let v = sample(&vp);
            let mh = |a: i32, c: i32| (a * c) >> 8;
            let clip8 = |v: i32| -> u8 {
                if v & !((256 << 6) - 1) == 0 {
                    (v >> 6) as u8
                } else if v < 0 {
                    0
                } else {
                    255
                }
            };
            let rr = clip8(mh(yy, 19077) + mh(v, 26149) - 14234);
            let gg = clip8(mh(yy, 19077) - mh(u, 6419) - mh(v, 13320) + 8708);
            let bb = clip8(mh(yy, 19077) + mh(u, 33050) - 17685);
            r.set(x as u32, y as u32, [rr, gg, bb, 255]);
        }
    }
    Ok(r)
}

fn sclip1(v: i32) -> i32 {
    v.clamp(-128, 127)
}
fn sclip2(v: i32) -> i32 {
    v.clamp(-16, 15)
}
fn clip1(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

fn filter2(p: &mut [u8], i: usize, s: usize) {
    let (p1, p0, q0, q1) = (
        p[i - 2 * s] as i32,
        p[i - s] as i32,
        p[i] as i32,
        p[i + s] as i32,
    );
    let a = 3 * (q0 - p0) + sclip1(p1 - q1);
    let a1 = sclip2((a + 4) >> 3);
    let a2 = sclip2((a + 3) >> 3);
    p[i - s] = clip1(p0 + a2);
    p[i] = clip1(q0 - a1);
}
fn filter4(p: &mut [u8], i: usize, s: usize) {
    let (p1, p0, q0, q1) = (
        p[i - 2 * s] as i32,
        p[i - s] as i32,
        p[i] as i32,
        p[i + s] as i32,
    );
    let a = 3 * (q0 - p0);
    let a1 = sclip2((a + 4) >> 3);
    let a2 = sclip2((a + 3) >> 3);
    let a3 = (a1 + 1) >> 1;
    p[i - 2 * s] = clip1(p1 + a3);
    p[i - s] = clip1(p0 + a2);
    p[i] = clip1(q0 - a1);
    p[i + s] = clip1(q1 - a3);
}
fn filter6(p: &mut [u8], i: usize, s: usize) {
    let (p2, p1, p0) = (p[i - 3 * s] as i32, p[i - 2 * s] as i32, p[i - s] as i32);
    let (q0, q1, q2) = (p[i] as i32, p[i + s] as i32, p[i + 2 * s] as i32);
    let a = sclip1(3 * (q0 - p0) + sclip1(p1 - q1));
    let a1 = (27 * a + 63) >> 7;
    let a2 = (18 * a + 63) >> 7;
    let a3 = (9 * a + 63) >> 7;
    p[i - 3 * s] = clip1(p2 + a3);
    p[i - 2 * s] = clip1(p1 + a2);
    p[i - s] = clip1(p0 + a1);
    p[i] = clip1(q0 - a1);
    p[i + s] = clip1(q1 - a2);
    p[i + 2 * s] = clip1(q2 - a3);
}
fn hev(p: &[u8], i: usize, s: usize, t: i32) -> bool {
    let (p1, p0, q0, q1) = (
        p[i - 2 * s] as i32,
        p[i - s] as i32,
        p[i] as i32,
        p[i + s] as i32,
    );
    (p1 - p0).abs() > t || (q1 - q0).abs() > t
}
fn needs(p: &[u8], i: usize, s: usize, t: i32) -> bool {
    let (p1, p0, q0, q1) = (
        p[i - 2 * s] as i32,
        p[i - s] as i32,
        p[i] as i32,
        p[i + s] as i32,
    );
    4 * (p0 - q0).abs() + (p1 - q1).abs() <= t
}
fn needs2(p: &[u8], i: usize, s: usize, t: i32, it: i32) -> bool {
    let g = |k: isize| p[(i as isize + k * s as isize) as usize] as i32;
    let (p3, p2, p1, p0, q0, q1, q2, q3) = (g(-4), g(-3), g(-2), g(-1), g(0), g(1), g(2), g(3));
    if 4 * (p0 - q0).abs() + (p1 - q1).abs() > t {
        return false;
    }
    (p3 - p2).abs() <= it
        && (p2 - p1).abs() <= it
        && (p1 - p0).abs() <= it
        && (q3 - q2).abs() <= it
        && (q2 - q1).abs() <= it
        && (q1 - q0).abs() <= it
}

/// Filtre une arête : `start` premier pixel côté q, `step` à travers
/// l'arête, `along` le long de l'arête, `n` pixels.
#[allow(clippy::too_many_arguments)]
fn edge(
    p: &mut [u8],
    start: usize,
    step: usize,
    along: usize,
    n: usize,
    thresh: i32,
    it: i32,
    hevt: i32,
    mb_edge: bool,
) {
    let t2 = 2 * thresh + 1;
    for k in 0..n {
        let i = start + k * along;
        if needs2(p, i, step, t2, it) {
            if hev(p, i, step, hevt) {
                filter2(p, i, step);
            } else if mb_edge {
                filter6(p, i, step);
            } else {
                filter4(p, i, step);
            }
        }
    }
}

fn simple_edge(p: &mut [u8], start: usize, step: usize, along: usize, n: usize, thresh: i32) {
    let t2 = 2 * thresh + 1;
    for k in 0..n {
        let i = start + k * along;
        if needs(p, i, step, t2) {
            filter2(p, i, step);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn filter_mb(
    y: &mut Plane,
    u: &mut Plane,
    v: &mut Plane,
    mx: usize,
    my: usize,
    limit: i32,
    il: i32,
    hevt: i32,
    inner: bool,
    simple: bool,
) {
    let yw = y.w;
    let yo = my * 16 * yw + mx * 16;
    if simple {
        if mx > 0 {
            simple_edge(&mut y.px, yo, 1, yw, 16, limit + 4);
        }
        if inner {
            for k in 1..4 {
                simple_edge(&mut y.px, yo + 4 * k, 1, yw, 16, limit);
            }
        }
        if my > 0 {
            simple_edge(&mut y.px, yo, yw, 1, 16, limit + 4);
        }
        if inner {
            for k in 1..4 {
                simple_edge(&mut y.px, yo + 4 * k * yw, yw, 1, 16, limit);
            }
        }
        return;
    }
    let cw = u.w;
    let co = my * 8 * cw + mx * 8;
    if mx > 0 {
        edge(&mut y.px, yo, 1, yw, 16, limit + 4, il, hevt, true);
        edge(&mut u.px, co, 1, cw, 8, limit + 4, il, hevt, true);
        edge(&mut v.px, co, 1, cw, 8, limit + 4, il, hevt, true);
    }
    if inner {
        for k in 1..4 {
            edge(&mut y.px, yo + 4 * k, 1, yw, 16, limit, il, hevt, false);
        }
        edge(&mut u.px, co + 4, 1, cw, 8, limit, il, hevt, false);
        edge(&mut v.px, co + 4, 1, cw, 8, limit, il, hevt, false);
    }
    if my > 0 {
        edge(&mut y.px, yo, yw, 1, 16, limit + 4, il, hevt, true);
        edge(&mut u.px, co, cw, 1, 8, limit + 4, il, hevt, true);
        edge(&mut v.px, co, cw, 1, 8, limit + 4, il, hevt, true);
    }
    if inner {
        for k in 1..4 {
            edge(
                &mut y.px,
                yo + 4 * k * yw,
                yw,
                1,
                16,
                limit,
                il,
                hevt,
                false,
            );
        }
        edge(&mut u.px, co + 4 * cw, cw, 1, 8, limit, il, hevt, false);
        edge(&mut v.px, co + 4 * cw, cw, 1, 8, limit, il, hevt, false);
    }
}
