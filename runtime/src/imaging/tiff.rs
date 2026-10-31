//! TIFF maison (bandes, tuiles, multi-pages, LZW, Deflate, PackBits, JPEG)
//! et aperçus des photos RAW (DNG, CR2, CR3, NEF, ARW, ORF, RW2, RAF, PEF,
//! SRW...) : on affiche la plus grande image intégrée par l'appareil.

use super::{Error, Image, Page, Raster};

#[derive(Clone, Copy)]
pub struct Tiff<'a> {
    pub d: &'a [u8],
    pub le: bool,
}

#[derive(Clone, Copy)]
pub struct Entry {
    pub tag: u16,
    pub typ: u16,
    pub count: u32,
    /// Position des données (dans l'entrée ou déportées).
    pub at: usize,
}

impl<'a> Tiff<'a> {
    pub fn parse(d: &'a [u8]) -> Option<(Tiff<'a>, usize)> {
        if d.len() < 8 {
            return None;
        }
        let le = match &d[..2] {
            b"II" => true,
            b"MM" => false,
            _ => return None,
        };
        let t = Tiff { d, le };
        let magic = t.u16(2)?;
        if !matches!(magic, 42 | 0x4F52 | 0x5352 | 0x55) {
            return None;
        }
        Some((t, t.u32(4)? as usize))
    }
    pub fn u16(&self, p: usize) -> Option<u16> {
        let b = self.d.get(p..p + 2)?;
        Some(if self.le {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        })
    }
    pub fn u32(&self, p: usize) -> Option<u32> {
        let b = self.d.get(p..p + 4)?;
        let a = [b[0], b[1], b[2], b[3]];
        Some(if self.le {
            u32::from_le_bytes(a)
        } else {
            u32::from_be_bytes(a)
        })
    }
    pub fn ifd(&self, off: usize) -> Option<(Vec<Entry>, usize)> {
        let n = self.u16(off)? as usize;
        if n == 0 || n > 4096 {
            return None;
        }
        let mut v = Vec::with_capacity(n);
        for i in 0..n {
            let e = off + 2 + i * 12;
            let tag = self.u16(e)?;
            let typ = self.u16(e + 2)?;
            let count = self.u32(e + 4)?;
            let size = type_size(typ) * count as usize;
            let at = if size <= 4 { e + 8 } else { self.u32(e + 8)? as usize };
            v.push(Entry {
                tag,
                typ,
                count,
                at,
            });
        }
        let next = self.u32(off + 2 + n * 12).unwrap_or(0) as usize;
        Some((v, next))
    }
    /// Valeur entière numéro `i` d'une entrée.
    pub fn val(&self, e: &Entry, i: usize) -> Option<u32> {
        if i >= e.count as usize {
            return None;
        }
        match e.typ {
            1 | 2 | 6 | 7 => self.d.get(e.at + i).map(|&b| b as u32),
            3 | 8 => self.u16(e.at + i * 2).map(|v| v as u32),
            4 | 9 | 13 => self.u32(e.at + i * 4),
            _ => None,
        }
    }
    pub fn vals(&self, e: &Entry) -> Vec<u32> {
        (0..(e.count as usize).min(1 << 20))
            .map_while(|i| self.val(e, i))
            .collect()
    }
    pub fn rational(&self, e: &Entry, i: usize) -> Option<f64> {
        if !matches!(e.typ, 5 | 10) {
            return self.val(e, i).map(|v| v as f64);
        }
        let n = self.u32(e.at + i * 8)?;
        let d = self.u32(e.at + i * 8 + 4)?;
        if d == 0 {
            return None;
        }
        Some(if e.typ == 10 {
            n as i32 as f64 / d as i32 as f64
        } else {
            n as f64 / d as f64
        })
    }
    pub fn text(&self, e: &Entry) -> Option<String> {
        let b = self.d.get(e.at..e.at + e.count as usize)?;
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        let s: String = String::from_utf8(b[..end].to_vec())
            .unwrap_or_else(|_| b[..end].iter().map(|&c| c as char).collect());
        let s = s.trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
}

fn type_size(t: u16) -> usize {
    match t {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 => 8,
        _ => 1,
    }
}

pub fn find<'e>(ifd: &'e [Entry], tag: u16) -> Option<&'e Entry> {
    ifd.iter().find(|e| e.tag == tag)
}

fn get(t: &Tiff, ifd: &[Entry], tag: u16, def: u32) -> u32 {
    find(ifd, tag).and_then(|e| t.val(e, 0)).unwrap_or(def)
}

/// Toutes les IFD (chaîne principale + SubIFDs), dans l'ordre.
fn all_ifds(t: &Tiff, first: usize) -> Vec<(Vec<Entry>, bool)> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![(first, true)];
    let mut chain = first;
    let _ = &mut chain;
    while let Some((off, top)) = stack.pop() {
        if off == 0 || !seen.insert(off) || out.len() > 64 {
            continue;
        }
        let Some((ifd, next)) = t.ifd(off) else {
            continue;
        };
        if let Some(e) = find(&ifd, 0x14A) {
            for v in t.vals(e).into_iter().rev() {
                stack.push((v as usize, false));
            }
        }
        if top && next != 0 {
            stack.insert(0, (next, true));
        }
        out.push((ifd, top));
    }
    out
}

pub fn decode(data: &[u8], format: &'static str) -> Result<Image, Error> {
    let (t, first) = Tiff::parse(data).ok_or(Error::Corrupt)?;
    let mut img = Image::new(format);
    if let Some((ifd0, _)) = t.ifd(first) {
        super::meta::exif_ifd(&t, &ifd0, &mut img);
    }
    let mut chain = first;
    let mut seen = std::collections::HashSet::new();
    let mut last_err = Error::Corrupt;
    while chain != 0 && seen.insert(chain) && img.pages.len() < 256 {
        let Some((ifd, next)) = t.ifd(chain) else {
            break;
        };
        // Ignore les vignettes réduites (NewSubfileType bit 0).
        if get(&t, &ifd, 0xFE, 0) & 1 == 0 || img.pages.is_empty() {
            match decode_ifd(&t, &ifd) {
                Ok(r) => {
                    let name = find(&ifd, 0x11D)
                        .and_then(|e| t.text(e))
                        .unwrap_or_default();
                    img.pages.push(Page::raster(name, r));
                }
                Err(e) => last_err = e,
            }
        }
        chain = next;
    }
    if img.pages.is_empty() {
        return Err(last_err);
    }
    Ok(img)
}

/// Décode une IFD TIFF en image RGBA.
pub fn decode_ifd(t: &Tiff, ifd: &[Entry]) -> Result<Raster, Error> {
    let w = get(t, ifd, 0x100, 0);
    let h = get(t, ifd, 0x101, 0);
    super::check_size(w, h)?;
    let comp = get(t, ifd, 0x103, 1);
    let photo = get(t, ifd, 0x106, 1);
    let spp = get(t, ifd, 0x115, 1).clamp(1, 16) as usize;
    let bps = get(t, ifd, 0x102, 1) as usize;
    let planar = get(t, ifd, 0x11C, 1);
    let pred = get(t, ifd, 0x13D, 1);
    let fmt = get(t, ifd, 0x153, 1);
    let extra = find(ifd, 0x152).and_then(|e| t.val(e, 0));
    if photo == 32803 || photo == 34892 {
        return Err(Error::Variant("RAW CFA"));
    }
    let (offs, counts, tw, th) = if let Some(e) = find(ifd, 0x144) {
        let tw = get(t, ifd, 0x142, w);
        let th = get(t, ifd, 0x143, h);
        (
            t.vals(e),
            find(ifd, 0x145).map(|e| t.vals(e)).unwrap_or_default(),
            tw,
            th,
        )
    } else {
        let rps = get(t, ifd, 0x116, h).min(h).max(1);
        (
            find(ifd, 0x111).map(|e| t.vals(e)).unwrap_or_default(),
            find(ifd, 0x117).map(|e| t.vals(e)).unwrap_or_default(),
            w,
            rps,
        )
    };
    if offs.is_empty() {
        return Err(Error::Corrupt);
    }
    if comp == 6 || comp == 7 {
        return decode_jpeg_tiles(t, ifd, &offs, &counts, w, h, tw, th);
    }
    let tiled = find(ifd, 0x144).is_some();
    let across = if tiled { w.div_ceil(tw) } else { 1 } as usize;
    let down = h.div_ceil(th) as usize;
    let planes = if planar == 2 { spp } else { 1 };
    let chunk_spp = if planar == 2 { 1 } else { spp };
    let row_bytes = (tw as usize * chunk_spp * bps).div_ceil(8);
    let palette = find(ifd, 0x140).map(|e| t.vals(e)).unwrap_or_default();
    // tampon de samples normalisés 0..=65535 [plane][y][x*chunk_spp]
    let mut samples = vec![0u16; w as usize * h as usize * spp];
    for plane in 0..planes {
        for ty in 0..down {
            for tx in 0..across {
                let idx = plane * across * down + ty * across + tx;
                let (Some(&o), c) = (offs.get(idx), counts.get(idx).copied()) else {
                    continue;
                };
                let o = o as usize;
                let c = c.map(|c| c as usize).unwrap_or(t.d.len().saturating_sub(o));
                let Some(src) = t.d.get(o..(o + c).min(t.d.len())) else {
                    continue;
                };
                let need = row_bytes * th as usize;
                let mut buf = match comp {
                    1 => src.to_vec(),
                    5 => lzw(src, need),
                    8 | 32946 => super::inflate::inflate_zlib(src, need * 2 + 64).unwrap_or_default(),
                    32773 => packbits(src, need),
                    2..=4 => return Err(Error::Variant("TIFF CCITT")),
                    _ => return Err(Error::Variant("TIFF compression")),
                };
                buf.resize(need, 0);
                if pred == 2 {
                    for row in buf.chunks_mut(row_bytes) {
                        if bps == 8 {
                            for i in chunk_spp..row.len() {
                                row[i] = row[i].wrapping_add(row[i - chunk_spp]);
                            }
                        } else if bps == 16 {
                            let n = row.len() / 2;
                            for i in chunk_spp..n {
                                let rd = |k: usize| {
                                    if t.le {
                                        u16::from_le_bytes([row[k * 2], row[k * 2 + 1]])
                                    } else {
                                        u16::from_be_bytes([row[k * 2], row[k * 2 + 1]])
                                    }
                                };
                                let v = rd(i).wrapping_add(rd(i - chunk_spp));
                                let b = if t.le { v.to_le_bytes() } else { v.to_be_bytes() };
                                row[i * 2] = b[0];
                                row[i * 2 + 1] = b[1];
                            }
                        }
                    }
                }
                for y in 0..th as usize {
                    let gy = ty * th as usize + y;
                    if gy >= h as usize {
                        break;
                    }
                    let row = &buf[y * row_bytes..(y + 1) * row_bytes];
                    for x in 0..tw as usize {
                        let gx = tx * tw as usize + x;
                        if gx >= w as usize {
                            break;
                        }
                        for s in 0..chunk_spp {
                            let v = read_sample(row, x * chunk_spp + s, bps, fmt, t.le);
                            let si = (gy * w as usize + gx) * spp + plane + s;
                            samples[si] = v;
                        }
                    }
                }
            }
        }
    }
    let mut r = Raster::new(w, h);
    let maxv = |v: u16| (v >> 8) as u8;
    let has_alpha = extra.is_some() || (photo == 2 && spp >= 4) || (photo <= 1 && spp >= 2);
    for i in 0..(w as usize * h as usize) {
        let s = &samples[i * spp..i * spp + spp];
        let px = match photo {
            0 | 1 => {
                let mut g = maxv(s[0]);
                if photo == 0 {
                    g = 255 - g;
                }
                let a = if has_alpha && spp >= 2 { maxv(s[1]) } else { 255 };
                [g, g, g, a]
            }
            2 if spp >= 3 => {
                let a = if has_alpha && spp >= 4 { maxv(s[3]) } else { 255 };
                [maxv(s[0]), maxv(s[1]), maxv(s[2]), a]
            }
            3 => {
                let n = palette.len() / 3;
                let k = (s[0] >> (16 - bps.min(16))) as usize;
                if k < n && n > 0 {
                    [
                        (palette[k] >> 8) as u8,
                        (palette[n + k] >> 8) as u8,
                        (palette[2 * n + k] >> 8) as u8,
                        255,
                    ]
                } else {
                    [0, 0, 0, 255]
                }
            }
            5 if spp >= 4 => {
                let k = 255 - maxv(s[3]) as u32;
                let f = |c: u16| ((255 - maxv(c) as u32) * k / 255) as u8;
                [f(s[0]), f(s[1]), f(s[2]), 255]
            }
            6 if spp >= 3 => {
                let (y, cb, cr) = (maxv(s[0]) as f32, maxv(s[1]) as f32 - 128.0, maxv(s[2]) as f32 - 128.0);
                let c = |v: f32| v.round().clamp(0.0, 255.0) as u8;
                [c(y + 1.402 * cr), c(y - 0.344 * cb - 0.714 * cr), c(y + 1.772 * cb), 255]
            }
            _ => return Err(Error::Variant("TIFF photométrie")),
        };
        r.px[i * 4..i * 4 + 4].copy_from_slice(&px);
    }
    Ok(r)
}

fn read_sample(row: &[u8], i: usize, bps: usize, fmt: u32, le: bool) -> u16 {
    match bps {
        8 => *row.get(i).unwrap_or(&0) as u16 * 257,
        16 => {
            let b = [*row.get(i * 2).unwrap_or(&0), *row.get(i * 2 + 1).unwrap_or(&0)];
            if le {
                u16::from_le_bytes(b)
            } else {
                u16::from_be_bytes(b)
            }
        }
        32 => {
            let b = [
                *row.get(i * 4).unwrap_or(&0),
                *row.get(i * 4 + 1).unwrap_or(&0),
                *row.get(i * 4 + 2).unwrap_or(&0),
                *row.get(i * 4 + 3).unwrap_or(&0),
            ];
            let u = if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) };
            if fmt == 3 {
                (super::tonemap(f32::from_bits(u)) * 65535.0) as u16
            } else {
                (u >> 16) as u16
            }
        }
        1..=7 => {
            let bit = i * bps;
            let byte = *row.get(bit / 8).unwrap_or(&0);
            let shift = 8 - bps - (bit % 8);
            let v = (byte >> shift) as u32 & ((1 << bps) - 1);
            (v * 65535 / ((1 << bps) - 1)) as u16
        }
        _ => 0,
    }
}

fn decode_jpeg_tiles(
    t: &Tiff,
    ifd: &[Entry],
    offs: &[u32],
    counts: &[u32],
    w: u32,
    h: u32,
    tw: u32,
    th: u32,
) -> Result<Raster, Error> {
    let tables = find(ifd, 0x15B).and_then(|e| t.d.get(e.at..e.at + e.count as usize));
    // Ancien JPEG (6) : flux complet indiqué par JPEGInterchangeFormat.
    if let (Some(o), Some(l)) = (find(ifd, 0x201), find(ifd, 0x202)) {
        let o = t.val(o, 0).unwrap_or(0) as usize;
        let l = t.val(l, 0).unwrap_or(0) as usize;
        if let Some(j) = t.d.get(o..o + l) {
            return super::jpeg::decode_raster(j, None);
        }
    }
    let tiled = find(ifd, 0x144).is_some();
    let across = if tiled { w.div_ceil(tw) } else { 1 };
    let mut r = Raster::new(w, h);
    for (i, &o) in offs.iter().enumerate() {
        let o = o as usize;
        let c = counts.get(i).copied().unwrap_or(0) as usize;
        let Some(src) = t.d.get(o..(o + c).min(t.d.len())) else {
            continue;
        };
        let mut j = Vec::with_capacity(src.len() + 1024);
        if let Some(tb) = tables {
            if tb.len() > 4 {
                j.extend_from_slice(&tb[..tb.len() - 2]);
                j.extend_from_slice(src.get(2..).unwrap_or(&[]));
            }
        }
        if j.is_empty() {
            j.extend_from_slice(src);
        }
        let tile = super::jpeg::decode_raster_ex(&j, None, Some(get(t, ifd, 0x106, 6)))?;
        let (bx, by) = ((i as u32 % across) * tw, (i as u32 / across) * th);
        for y in 0..tile.h {
            for x in 0..tile.w {
                if bx + x < w && by + y < h {
                    r.set(bx + x, by + y, tile.get(x, y));
                }
            }
        }
    }
    Ok(r)
}

pub fn packbits(src: &[u8], need: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(need);
    let mut i = 0;
    while i < src.len() && out.len() < need {
        let n = src[i] as i8;
        i += 1;
        if n >= 0 {
            let k = n as usize + 1;
            out.extend_from_slice(&src[i..(i + k).min(src.len())]);
            i += k;
        } else if n != -128 {
            let k = (-(n as i32)) as usize + 1;
            if let Some(&b) = src.get(i) {
                out.extend(std::iter::repeat_n(b, k));
            }
            i += 1;
        }
    }
    out
}

/// LZW TIFF (MSB d'abord, code early change).
pub fn lzw(src: &[u8], need: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(need);
    let mut dict: Vec<(u16, u8, u16)> = Vec::with_capacity(4096); // (préfixe, octet, longueur)
    let reset = |d: &mut Vec<(u16, u8, u16)>| {
        d.clear();
        for i in 0..256u16 {
            d.push((u16::MAX, i as u8, 1));
        }
        d.push((0, 0, 0));
        d.push((0, 0, 0));
    };
    reset(&mut dict);
    let mut bitpos = 0usize;
    let mut width = 9;
    let mut prev: Option<u16> = None;
    let emit = |d: &Vec<(u16, u8, u16)>, code: u16, out: &mut Vec<u8>| -> u8 {
        let len = d[code as usize].2 as usize;
        let start = out.len();
        out.resize(start + len, 0);
        let mut c = code;
        let mut k = len;
        while k > 0 {
            k -= 1;
            out[start + k] = d[c as usize].1;
            c = d[c as usize].0;
        }
        out[start]
    };
    while out.len() < need {
        if bitpos + width > src.len() * 8 {
            break;
        }
        let mut code = 0u32;
        for b in 0..width {
            let p = bitpos + b;
            code = (code << 1) | ((src[p / 8] >> (7 - p % 8)) & 1) as u32;
        }
        bitpos += width;
        let code = code as u16;
        if code == 256 {
            reset(&mut dict);
            width = 9;
            prev = None;
            continue;
        }
        if code == 257 {
            break;
        }
        match prev {
            None => {
                if code as usize >= dict.len() {
                    break;
                }
                emit(&dict, code, &mut out);
            }
            Some(p) => {
                if (code as usize) < dict.len() {
                    let first = emit(&dict, code, &mut out);
                    let l = dict[p as usize].2 + 1;
                    dict.push((p, first, l));
                } else if code as usize == dict.len() {
                    let first_of_prev = {
                        let mut c = p;
                        while dict[c as usize].0 != u16::MAX {
                            c = dict[c as usize].0;
                        }
                        dict[c as usize].1
                    };
                    let l = dict[p as usize].2 + 1;
                    dict.push((p, first_of_prev, l));
                    emit(&dict, code, &mut out);
                } else {
                    break;
                }
            }
        }
        prev = Some(code);
        if dict.len() + 1 >= (1 << width) && width < 12 {
            width += 1;
        }
        if dict.len() >= 4096 {
            // attend un code clear
        }
    }
    out
}

/// Extrait la meilleure image d'aperçu d'un fichier RAW.
pub fn raw_preview(data: &[u8], format: &'static str) -> Result<Image, Error> {
    let mut img = Image::new(format);
    img.raw = true;
    img.embedded_preview = true;
    let mut best: Option<Raster> = None;
    let mut consider = |r: Raster, best: &mut Option<Raster>| {
        if best
            .as_ref()
            .is_none_or(|b| (r.w as u64 * r.h as u64) > (b.w as u64 * b.h as u64))
        {
            *best = Some(r);
        }
    };
    // RAF : en-tête Fujifilm avec position du JPEG.
    if data.starts_with(b"FUJIFILMCCD-RAW") && data.len() > 92 {
        let o = super::be32(data, 84) as usize;
        let l = super::be32(data, 88) as usize;
        if let Some(j) = data.get(o..o + l) {
            if let Ok(r) = super::jpeg::decode_raster(j, Some(&mut img)) {
                consider(r, &mut best);
            }
        }
    }
    if let Some((t, first)) = Tiff::parse(data) {
        if let Some((ifd0, _)) = t.ifd(first) {
            super::meta::exif_ifd(&t, &ifd0, &mut img);
        }
        for (ifd, _) in all_ifds(&t, first) {
            if let (Some(o), Some(l)) = (find(&ifd, 0x201), find(&ifd, 0x202)) {
                let o = t.val(o, 0).unwrap_or(0) as usize;
                let l = t.val(l, 0).unwrap_or(0) as usize;
                if let Some(j) = data.get(o..o.saturating_add(l).min(data.len())) {
                    if let Ok(r) = super::jpeg::decode_raster(j, None) {
                        consider(r, &mut best);
                        continue;
                    }
                }
            }
            if let Ok(r) = decode_ifd(&t, &ifd) {
                consider(r, &mut best);
            }
        }
    }
    if best.as_ref().is_none_or(|b| b.w < 640) {
        // Recherche générique des JPEG intégrés (CR3, conteneurs inconnus).
        for r in scan_jpegs(data) {
            consider(r, &mut best);
        }
    }
    let r = best.ok_or(Error::Variant("RAW sans aperçu"))?;
    img.pages.push(Page::raster(String::new(), r));
    Ok(img)
}

/// Décode les JPEG baseline trouvés dans un flux binaire quelconque.
pub fn scan_jpegs(data: &[u8]) -> Vec<Raster> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut tries = 0;
    while i + 4 < data.len() && tries < 12 {
        if data[i] == 0xFF && data[i + 1] == 0xD8 && data[i + 2] == 0xFF {
            tries += 1;
            if let Ok(r) = super::jpeg::decode_raster(&data[i..], None) {
                if r.w >= 32 {
                    out.push(r);
                }
            }
            i += 4;
        } else {
            i += 1;
        }
    }
    out
}
