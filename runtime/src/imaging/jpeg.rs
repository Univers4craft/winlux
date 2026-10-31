//! Décodeur JPEG maison : baseline, étendu, progressif,
//! marqueurs de redémarrage, sous-échantillonnage quelconque, CMYK/YCCK
//! (Adobe), EXIF (APP1) et IPTC (APP13).

use super::{be16, Error, Image, Page, Raster};

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27,
    20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58,
    59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

#[derive(Clone, Default)]
struct Huff {
    // (longueur, code) -> symbole via tables par longueur
    maxcode: [i32; 18],
    valptr: [i32; 17],
    mincode: [i32; 17],
    vals: Vec<u8>,
    // table rapide 9 bits : (longueur, symbole)
    fast: Vec<(u8, u8)>,
}

impl Huff {
    fn new(counts: &[u8; 16], vals: &[u8]) -> Huff {
        let mut h = Huff {
            vals: vals.to_vec(),
            fast: vec![(0, 0); 512],
            ..Default::default()
        };
        let mut code = 0i32;
        let mut k = 0i32;
        for l in 1..=16 {
            let n = counts[l - 1] as i32;
            h.valptr[l] = k;
            h.mincode[l] = code;
            if n > 0 {
                for i in 0..n {
                    if l <= 9 {
                        let c = (code + i) << (9 - l);
                        for f in 0..(1 << (9 - l)) {
                            if let Some(slot) = h.fast.get_mut((c + f) as usize) {
                                *slot = (l as u8, vals.get((k + i) as usize).copied().unwrap_or(0));
                            }
                        }
                    }
                }
                code += n;
                k += n;
                h.maxcode[l] = code - 1;
            } else {
                h.maxcode[l] = -1;
            }
            code <<= 1;
        }
        h.maxcode[17] = i32::MAX;
        h
    }
}

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
    acc: u32,
    n: u32,
    marker_hit: bool,
}

impl<'a> Reader<'a> {
    fn fill(&mut self) {
        while self.n <= 24 {
            let mut b = 0u32;
            if !self.marker_hit && self.p < self.d.len() {
                let c = self.d[self.p];
                if c == 0xFF {
                    let nx = self.d.get(self.p + 1).copied().unwrap_or(0);
                    if nx == 0 {
                        self.p += 2;
                        b = 0xFF;
                    } else {
                        self.marker_hit = true;
                    }
                } else {
                    self.p += 1;
                    b = c as u32;
                }
            }
            self.acc |= b << (24 - self.n);
            self.n += 8;
        }
    }
    fn bits(&mut self, k: u32) -> u32 {
        if k == 0 {
            return 0;
        }
        self.fill();
        let v = self.acc >> (32 - k);
        self.acc <<= k;
        self.n -= k;
        v
    }
    fn bit(&mut self) -> u32 {
        self.bits(1)
    }
    fn decode(&mut self, h: &Huff) -> Option<u8> {
        self.fill();
        let peek = (self.acc >> 23) as usize;
        let (l, s) = h.fast[peek];
        if l > 0 {
            self.acc <<= l;
            self.n -= l as u32;
            return Some(s);
        }
        let mut code = 0i32;
        for l in 1..=16 {
            code = (code << 1) | self.bit() as i32;
            if code <= h.maxcode[l] {
                let idx = h.valptr[l] + code - h.mincode[l];
                return h.vals.get(idx as usize).copied();
            }
        }
        None
    }
    fn receive_extend(&mut self, s: u8) -> i32 {
        if s == 0 {
            return 0;
        }
        let v = self.bits(s as u32) as i32;
        if v < (1 << (s - 1)) {
            v - (1 << s) + 1
        } else {
            v
        }
    }
    fn reset(&mut self) {
        self.acc = 0;
        self.n = 0;
        self.marker_hit = false;
        // saute le marqueur RSTn
        while self.p + 1 < self.d.len() {
            if self.d[self.p] == 0xFF && (0xD0..=0xD7).contains(&self.d[self.p + 1]) {
                self.p += 2;
                return;
            }
            if self.d[self.p] == 0xFF && self.d[self.p + 1] != 0 && self.d[self.p + 1] != 0xFF {
                return;
            }
            self.p += 1;
        }
    }
}

#[derive(Clone, Default)]
struct Comp {
    id: u8,
    h: usize,
    v: usize,
    tq: usize,
    bw: usize, // blocs par ligne (alloués)
    bh: usize,
    coefs: Vec<i32>,
    dc_pred: i32,
    td: usize,
    ta: usize,
}

pub fn decode(data: &[u8]) -> Result<Image, Error> {
    let mut img = Image::new("JPEG");
    let r = decode_raster(data, Some(&mut img))?;
    img.pages.push(Page::raster(String::new(), r));
    Ok(img)
}

/// Décode seulement les pixels (utilisé par TIFF/RAW/PSD pour les aperçus).
pub fn decode_raster(data: &[u8], img: Option<&mut Image>) -> Result<Raster, Error> {
    decode_raster_ex(data, img, None)
}

/// `photometric` : interprétation TIFF des composantes (2 = RGB, 6 = YCbCr,
/// 5 = CMJN), prioritaire sur les marqueurs JFIF/Adobe.
pub fn decode_raster_ex(data: &[u8], mut img: Option<&mut Image>, photometric: Option<u32>) -> Result<Raster, Error> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return Err(Error::Corrupt);
    }
    let mut qt = [[0i32; 64]; 4];
    let mut dc: Vec<Huff> = vec![Huff::default(); 4];
    let mut ac: Vec<Huff> = vec![Huff::default(); 4];
    let mut comps: Vec<Comp> = Vec::new();
    let (mut width, mut height) = (0usize, 0usize);
    let mut progressive = false;
    let mut restart = 0usize;
    let mut adobe: Option<u8> = None;
    let mut jfif = false;
    let (mut hmax, mut vmax) = (1usize, 1usize);
    let (mut mcux, mut mcuy) = (0usize, 0usize);
    let mut eobrun = 0u32;
    let mut p = 2;
    let mut frame_seen = false;
    while p + 4 <= data.len() {
        if data[p] != 0xFF {
            p += 1;
            continue;
        }
        let m = data[p + 1];
        if m == 0xFF || m == 0x00 || (0xD0..=0xD7).contains(&m) || m == 0x01 {
            p += if m == 0xFF { 1 } else { 2 };
            continue;
        }
        if m == 0xD9 {
            break;
        }
        let len = be16(data, p + 2) as usize;
        let seg = data.get(p + 4..p + 2 + len).ok_or(Error::Corrupt)?;
        p += 2 + len;
        match m {
            0xDB => {
                let mut q = 0;
                while q < seg.len() {
                    let pq = seg[q] >> 4;
                    let tq = (seg[q] & 3) as usize;
                    q += 1;
                    for &z in ZIGZAG.iter() {
                        let v = if pq == 0 {
                            let v = *seg.get(q).ok_or(Error::Corrupt)? as i32;
                            q += 1;
                            v
                        } else {
                            let v = be16(seg, q) as i32;
                            q += 2;
                            v
                        };
                        qt[tq][z] = v;
                    }
                }
            }
            0xC4 => {
                let mut q = 0;
                while q + 17 <= seg.len() {
                    let tc = seg[q] >> 4;
                    let th = (seg[q] & 3) as usize;
                    let mut counts = [0u8; 16];
                    counts.copy_from_slice(&seg[q + 1..q + 17]);
                    let total: usize = counts.iter().map(|&c| c as usize).sum();
                    let vals = seg.get(q + 17..q + 17 + total).ok_or(Error::Corrupt)?;
                    let h = Huff::new(&counts, vals);
                    if tc == 0 {
                        dc[th] = h;
                    } else {
                        ac[th] = h;
                    }
                    q += 17 + total;
                }
            }
            0xDD => restart = be16(seg, 0) as usize,
            0xE0 if seg.starts_with(b"JFIF") => jfif = true,
            0xE1 => {
                if let Some(i) = img.as_deref_mut() {
                    if seg.starts_with(b"Exif\0\0") {
                        super::meta::exif(&seg[6..], i);
                    } else if seg.starts_with(b"http://ns.adobe.com/xap/1.0/") {
                        super::meta::xmp(seg, i);
                    }
                }
            }
            0xED => {
                if let Some(i) = img.as_deref_mut() {
                    super::meta::photoshop_irb(seg, i);
                }
            }
            0xEE if seg.starts_with(b"Adobe") && seg.len() >= 12 => adobe = Some(seg[11]),
            0xC0..=0xCF if m != 0xC4 && m != 0xC8 && m != 0xCC => {
                if frame_seen {
                    continue;
                }
                frame_seen = true;
                match m {
                    0xC0 | 0xC1 => {}
                    0xC2 => progressive = true,
                    0xC3 => return Err(Error::Variant("JPEG sans perte")),
                    _ => return Err(Error::Variant("JPEG arithmétique")),
                }
                if seg.len() < 6 {
                    return Err(Error::Corrupt);
                }
                if seg[0] != 8 {
                    return Err(Error::Variant("JPEG 12 bits"));
                }
                height = be16(seg, 1) as usize;
                width = be16(seg, 3) as usize;
                let n = seg[5] as usize;
                if height == 0 {
                    return Err(Error::Variant("JPEG DNL"));
                }
                super::check_size(width as u32, height as u32)?;
                for i in 0..n {
                    let c = seg.get(6 + i * 3..9 + i * 3).ok_or(Error::Corrupt)?;
                    comps.push(Comp {
                        id: c[0],
                        h: ((c[1] >> 4) as usize).clamp(1, 4),
                        v: ((c[1] & 15) as usize).clamp(1, 4),
                        tq: (c[2] & 3) as usize,
                        ..Default::default()
                    });
                }
                hmax = comps.iter().map(|c| c.h).max().unwrap_or(1);
                vmax = comps.iter().map(|c| c.v).max().unwrap_or(1);
                mcux = width.div_ceil(8 * hmax);
                mcuy = height.div_ceil(8 * vmax);
                for c in comps.iter_mut() {
                    c.bw = mcux * c.h;
                    c.bh = mcuy * c.v;
                    c.coefs = vec![0; c.bw * c.bh * 64];
                }
            }
            0xDA => {
                if comps.is_empty() {
                    return Err(Error::Corrupt);
                }
                let ns = seg[0] as usize;
                let mut sc: Vec<usize> = Vec::new();
                for i in 0..ns {
                    let id = seg[1 + i * 2];
                    let t = seg[2 + i * 2];
                    let ci = comps.iter().position(|c| c.id == id).ok_or(Error::Corrupt)?;
                    comps[ci].td = (t >> 4) as usize & 3;
                    comps[ci].ta = (t & 15) as usize & 3;
                    sc.push(ci);
                }
                let o = 1 + ns * 2;
                let ss = seg[o] as usize;
                let se = (seg[o + 1] as usize).min(63);
                let ah = seg[o + 2] >> 4;
                let al = seg[o + 2] & 15;
                let mut rd = Reader {
                    d: data,
                    p,
                    acc: 0,
                    n: 0,
                    marker_hit: false,
                };
                for c in comps.iter_mut() {
                    c.dc_pred = 0;
                }
                eobrun = 0;
                let single = sc.len() == 1;
                let (units_x, units_y) = if single {
                    let c = &comps[sc[0]];
                    (
                        (width * c.h).div_ceil(8 * hmax),
                        (height * c.v).div_ceil(8 * vmax),
                    )
                } else {
                    (mcux, mcuy)
                };
                let total = units_x * units_y;
                let mut todo = if restart > 0 { restart } else { usize::MAX };
                for u in 0..total {
                    if todo == 0 {
                        rd.reset();
                        for c in comps.iter_mut() {
                            c.dc_pred = 0;
                        }
                        eobrun = 0;
                        todo = restart;
                    }
                    todo = todo.saturating_sub(1);
                    let (ux, uy) = (u % units_x, u / units_x);
                    for &ci in sc.iter() {
                        let (bx0, by0, nbx, nby) = if single {
                            (ux, uy, 1, 1)
                        } else {
                            let c = &comps[ci];
                            (ux * c.h, uy * c.v, c.h, c.v)
                        };
                        for by in by0..by0 + nby {
                            for bx in bx0..bx0 + nbx {
                                let c = &mut comps[ci];
                                let off = (by * c.bw + bx) * 64;
                                let blk = &mut c.coefs[off..off + 64];
                                let dct = &dc[c.td];
                                let act = &ac[c.ta];
                                if !progressive {
                                    let t = rd.decode(dct).ok_or(Error::Corrupt)?;
                                    let diff = rd.receive_extend(t);
                                    c.dc_pred += diff;
                                    blk[0] = c.dc_pred;
                                    let mut k = 1;
                                    while k < 64 {
                                        let rs = rd.decode(act).ok_or(Error::Corrupt)?;
                                        let (r, s) = ((rs >> 4) as usize, rs & 15);
                                        if s == 0 {
                                            if r == 15 {
                                                k += 16;
                                                continue;
                                            }
                                            break;
                                        }
                                        k += r;
                                        if k > 63 {
                                            break;
                                        }
                                        blk[ZIGZAG[k]] = rd.receive_extend(s);
                                        k += 1;
                                    }
                                } else if ss == 0 {
                                    if ah == 0 {
                                        let t = rd.decode(dct).ok_or(Error::Corrupt)?;
                                        let diff = rd.receive_extend(t);
                                        c.dc_pred += diff;
                                        blk[0] = c.dc_pred << al;
                                    } else if rd.bit() == 1 {
                                        blk[0] |= 1 << al;
                                    }
                                } else if ah == 0 {
                                    if eobrun > 0 {
                                        eobrun -= 1;
                                        continue;
                                    }
                                    let mut k = ss;
                                    while k <= se {
                                        let rs = rd.decode(act).ok_or(Error::Corrupt)?;
                                        let (r, s) = ((rs >> 4) as u32, rs & 15);
                                        if s == 0 {
                                            if r < 15 {
                                                eobrun = (1 << r) - 1 + rd.bits(r);
                                                break;
                                            }
                                            k += 16;
                                            continue;
                                        }
                                        k += r as usize;
                                        if k > 63 {
                                            break;
                                        }
                                        blk[ZIGZAG[k]] = rd.receive_extend(s) * (1 << al);
                                        k += 1;
                                    }
                                } else {
                                    refine_ac(&mut rd, act, blk, ss, se, al, &mut eobrun)?;
                                }
                            }
                        }
                    }
                }
                // avance jusqu'au prochain marqueur
                p = rd.p;
                while p + 1 < data.len()
                    && !(data[p] == 0xFF && data[p + 1] != 0 && !(0xD0..=0xD7).contains(&data[p + 1]))
                {
                    p += 1;
                }
            }
            _ => {}
        }
    }
    if comps.is_empty() || width == 0 {
        return Err(Error::Corrupt);
    }
    // IDCT de tous les blocs -> plans
    let mut planes: Vec<(Vec<u8>, usize, usize)> = Vec::new();
    for c in comps.iter() {
        let pw = c.bw * 8;
        let ph = c.bh * 8;
        let mut plane = vec![0u8; pw * ph];
        let q = &qt[c.tq];
        let mut tmp = [0i32; 64];
        for by in 0..c.bh {
            for bx in 0..c.bw {
                let off = (by * c.bw + bx) * 64;
                let blk = &c.coefs[off..off + 64];
                let mut out = [0u8; 64];
                if blk[1..].iter().all(|&v| v == 0) {
                    let v = (blk[0] * q[0]) as f32 / 8.0 + 128.0;
                    out = [v.round().clamp(0.0, 255.0) as u8; 64];
                } else {
                    for i in 0..64 {
                        tmp[i] = blk[i] * q[i];
                    }
                    idct8x8(&tmp, &mut out);
                }
                for y in 0..8 {
                    let dst = (by * 8 + y) * pw + bx * 8;
                    plane[dst..dst + 8].copy_from_slice(&out[y * 8..y * 8 + 8]);
                }
            }
        }
        planes.push((plane, pw, ph));
    }
    let mut r = Raster::new(width as u32, height as u32);
    let n = comps.len();
    let transform = match photometric {
        Some(2) => 0,
        Some(6) => 1,
        _ => match adobe {
        Some(t) => t,
        None => {
            if n == 3 {
                // JFIF ou ids 1,2,3 => YCbCr ; ids 'R','G','B' => RGB
                if !jfif && comps[0].id == b'R' && comps[1].id == b'G' {
                    0
                } else {
                    1
                }
            } else {
                0
            }
        }
    },
    };
    let rgba_tiff = photometric == Some(2) || photometric == Some(6);
    // Plans pleine résolution (suréchantillonnage bilinéaire centré).
    let full: Vec<Vec<u8>> = comps
        .iter()
        .zip(planes.iter())
        .map(|(c, (pl, pw, ph))| upsample(pl, *pw, *ph, c.h, c.v, hmax, vmax, width, height))
        .collect();
    let sample = |ci: usize, x: usize, y: usize| -> f32 { full[ci][y * width + x] as f32 };
    let clamp = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    for y in 0..height {
        for x in 0..width {
            let px = match n {
                1 => {
                    let g = clamp(sample(0, x, y));
                    [g, g, g, 255]
                }
                3 => {
                    let (a, b, c) = (sample(0, x, y), sample(1, x, y), sample(2, x, y));
                    if transform == 0 {
                        [clamp(a), clamp(b), clamp(c), 255]
                    } else {
                        ycc(a, b, c)
                    }
                }
                4 if rgba_tiff => {
                    let (a, b, c) = (sample(0, x, y), sample(1, x, y), sample(2, x, y));
                    let mut p = if transform == 0 { [clamp(a), clamp(b), clamp(c), 255] } else { ycc(a, b, c) };
                    p[3] = clamp(sample(3, x, y));
                    p
                }
                4 => {
                    let (mut cc, mut mm, mut yy) = (sample(0, x, y), sample(1, x, y), sample(2, x, y));
                    let k = sample(3, x, y);
                    if transform == 2 {
                        let rgb = ycc(cc, mm, yy);
                        cc = 255.0 - rgb[0] as f32;
                        mm = 255.0 - rgb[1] as f32;
                        yy = 255.0 - rgb[2] as f32;
                    }
                    // Adobe stocke le CMYK inversé
                    let (cc, mm, yy, k) = if adobe.is_some() {
                        (cc, mm, yy, k)
                    } else {
                        (255.0 - cc, 255.0 - mm, 255.0 - yy, 255.0 - k)
                    };
                    [
                        clamp(cc * k / 255.0),
                        clamp(mm * k / 255.0),
                        clamp(yy * k / 255.0),
                        255,
                    ]
                }
                _ => {
                    let g = clamp(sample(0, x, y));
                    [g, g, g, 255]
                }
            };
            let o = (y * width + x) * 4;
            r.px[o..o + 4].copy_from_slice(&px);
        }
    }
    Ok(r)
}

#[allow(clippy::too_many_arguments)]
fn upsample(pl: &[u8], pw: usize, ph: usize, h: usize, v: usize, hmax: usize, vmax: usize, w: usize, hh: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * hh];
    if h == hmax && v == vmax {
        for y in 0..hh {
            out[y * w..(y + 1) * w].copy_from_slice(&pl[y * pw..y * pw + w]);
        }
        return out;
    }
    // positions horizontales précalculées (virgule fixe 8 bits)
    let xs: Vec<(usize, usize, u32)> = (0..w)
        .map(|x| {
            let f = ((x as f32 + 0.5) * h as f32 / hmax as f32 - 0.5).max(0.0);
            let x0 = (f as usize).min(pw - 1);
            (x0, (x0 + 1).min(pw - 1), ((f - x0 as f32) * 256.0) as u32)
        })
        .collect();
    for y in 0..hh {
        let f = ((y as f32 + 0.5) * v as f32 / vmax as f32 - 0.5).max(0.0);
        let y0 = (f as usize).min(ph - 1);
        let y1 = (y0 + 1).min(ph - 1);
        let ay = ((f - y0 as f32) * 256.0) as u32;
        let (r0, r1) = (&pl[y0 * pw..y0 * pw + pw], &pl[y1 * pw..y1 * pw + pw]);
        let dst = &mut out[y * w..(y + 1) * w];
        for (o, &(x0, x1, ax)) in dst.iter_mut().zip(xs.iter()) {
            let a = r0[x0] as u32 * (256 - ax) + r0[x1] as u32 * ax;
            let b = r1[x0] as u32 * (256 - ax) + r1[x1] as u32 * ax;
            *o = ((a * (256 - ay) + b * ay + 32768) >> 16) as u8;
        }
    }
    out
}

fn ycc(y: f32, cb: f32, cr: f32) -> [u8; 4] {
    let cb = cb - 128.0;
    let cr = cr - 128.0;
    let c = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    [
        c(y + 1.402 * cr),
        c(y - 0.344_136 * cb - 0.714_136 * cr),
        c(y + 1.772 * cb),
        255,
    ]
}

fn refine_ac(
    rd: &mut Reader,
    act: &Huff,
    blk: &mut [i32],
    ss: usize,
    se: usize,
    al: u8,
    eobrun: &mut u32,
) -> Result<(), Error> {
    let p1 = 1i32 << al;
    let m1 = -1i32 << al;
    let mut k = ss;
    if *eobrun == 0 {
        while k <= se {
            let rs = rd.decode(act).ok_or(Error::Corrupt)?;
            let mut r = (rs >> 4) as i32;
            let s = rs & 15;
            let mut val = 0;
            if s == 0 {
                if r < 15 {
                    *eobrun = (1 << r) + rd.bits(r as u32);
                    break;
                }
            } else {
                val = if rd.bit() == 1 { p1 } else { m1 };
            }
            while k <= se {
                let z = ZIGZAG[k];
                if blk[z] != 0 {
                    if rd.bit() == 1 && (blk[z] & p1) == 0 {
                        blk[z] += if blk[z] >= 0 { p1 } else { m1 };
                    }
                } else {
                    if r == 0 {
                        if val != 0 {
                            blk[z] = val;
                        }
                        k += 1;
                        break;
                    }
                    r -= 1;
                }
                k += 1;
            }
        }
    }
    if *eobrun > 0 {
        while k <= se {
            let z = ZIGZAG[k];
            if blk[z] != 0 && rd.bit() == 1 && (blk[z] & p1) == 0 {
                blk[z] += if blk[z] >= 0 { p1 } else { m1 };
            }
            k += 1;
        }
        *eobrun -= 1;
    }
    Ok(())
}

/// IDCT flottante séparable (précision suffisante pour l'affichage).
fn idct8x8(input: &[i32; 64], out: &mut [u8; 64]) {
    use std::sync::OnceLock;
    static COS: OnceLock<[[f32; 8]; 8]> = OnceLock::new();
    let cos = COS.get_or_init(|| {
        let mut t = [[0f32; 8]; 8];
        for (x, row) in t.iter_mut().enumerate() {
            for (u, v) in row.iter_mut().enumerate() {
                let cu = if u == 0 { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
                *v = cu * (((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI) / 16.0).cos() / 2.0;
            }
        }
        t
    });
    let mut tmp = [0f32; 64];
    for y in 0..8 {
        if input[y * 8..y * 8 + 8].iter().all(|&v| v == 0) {
            continue;
        }
        for x in 0..8 {
            let mut s = 0.0;
            for u in 0..8 {
                s += cos[x][u] * input[y * 8 + u] as f32;
            }
            tmp[y * 8 + x] = s;
        }
    }
    for x in 0..8 {
        for y in 0..8 {
            let mut s = 0.0;
            for v in 0..8 {
                s += cos[y][v] * tmp[v * 8 + x];
            }
            out[y * 8 + x] = (s + 128.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}
