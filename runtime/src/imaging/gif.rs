//! GIF maison (87a/89a) : LZW, entrelacement, transparence, animations
//! avec méthodes de disposition.

use super::{le16, Error, Image, Page, Raster};

pub fn decode(d: &[u8]) -> Result<Image, Error> {
    if d.len() < 13 || !d.starts_with(b"GIF8") {
        return Err(Error::Corrupt);
    }
    let w = le16(d, 6) as u32;
    let h = le16(d, 8) as u32;
    super::check_size(w, h)?;
    let flags = d[10];
    let bg_index = d[11] as usize;
    let mut p = 13;
    let mut gct: &[u8] = &[];
    if flags & 0x80 != 0 {
        let n = 3 << ((flags & 7) + 1);
        gct = d.get(p..p + n).ok_or(Error::Corrupt)?;
        p += n;
    }
    let _ = bg_index;
    let mut img = Image::new("GIF");
    let mut canvas = Raster::new(w, h);
    let (mut delay, mut transp, mut dispose) = (100u32, None::<u8>, 0u8);
    while p < d.len() {
        match d[p] {
            0x21 => {
                let label = *d.get(p + 1).ok_or(Error::Corrupt)?;
                p += 2;
                if label == 0xF9 && d.get(p) == Some(&4) && p + 5 < d.len() {
                    let pf = d[p + 1];
                    delay = le16(d, p + 2) as u32 * 10;
                    transp = if pf & 1 != 0 { Some(d[p + 4]) } else { None };
                    dispose = (pf >> 2) & 7;
                }
                // saute les sous-blocs
                while let Some(&n) = d.get(p) {
                    p += 1 + n as usize;
                    if n == 0 {
                        break;
                    }
                }
            }
            0x2C => {
                let b = d.get(p..p + 10).ok_or(Error::Corrupt)?;
                let fx = le16(b, 1) as u32;
                let fy = le16(b, 3) as u32;
                let fw = le16(b, 5) as u32;
                let fh = le16(b, 7) as u32;
                let lf = b[9];
                p += 10;
                let mut ct = gct;
                if lf & 0x80 != 0 {
                    let n = 3 << ((lf & 7) + 1);
                    ct = d.get(p..p + n).ok_or(Error::Corrupt)?;
                    p += n;
                }
                let min_code = *d.get(p).ok_or(Error::Corrupt)?;
                p += 1;
                let mut data = Vec::new();
                while let Some(&n) = d.get(p) {
                    p += 1;
                    if n == 0 {
                        break;
                    }
                    data.extend_from_slice(
                        d.get(p..p + n as usize).unwrap_or(&d[p.min(d.len())..]),
                    );
                    p += n as usize;
                }
                // Une image vide ou démesurée est ignorée (mais ses données
                // ont été consommées) : jamais d'allocation sur foi du fichier.
                if super::check_size(fw, fh).is_err() {
                    continue;
                }
                let idx = lzw(&data, min_code, fw as usize * fh as usize);
                let before = if dispose == 3 {
                    Some(canvas.clone())
                } else {
                    None
                };
                let rows: Vec<u32> = if lf & 0x40 != 0 {
                    let mut r = Vec::new();
                    for (s, st) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
                        let mut y = s;
                        while y < fh {
                            r.push(y);
                            y += st;
                        }
                    }
                    r
                } else {
                    (0..fh).collect()
                };
                for (i, &k) in idx.iter().enumerate() {
                    let row = i as u32 / fw.max(1);
                    let Some(&y) = rows.get(row as usize) else {
                        break;
                    };
                    let x = i as u32 % fw.max(1);
                    if Some(k) == transp {
                        continue;
                    }
                    let (cx, cy) = (fx + x, fy + y);
                    if cx < w && cy < h {
                        let c = ct
                            .get(k as usize * 3..k as usize * 3 + 3)
                            .unwrap_or(&[0, 0, 0]);
                        canvas.set(cx, cy, [c[0], c[1], c[2], 255]);
                    }
                }
                img.pages.push(Page::frame(
                    canvas.clone(),
                    if delay < 20 { 100 } else { delay },
                ));
                match dispose {
                    2 => {
                        for y in fy..(fy + fh).min(h) {
                            for x in fx..(fx + fw).min(w) {
                                canvas.set(x, y, [0, 0, 0, 0]);
                            }
                        }
                    }
                    3 => canvas = before.unwrap_or(canvas),
                    _ => {}
                }
                delay = 100;
                transp = None;
                dispose = 0;
                if img.pages.len() >= 2000 {
                    break;
                }
            }
            0x3B => break,
            _ => break,
        }
    }
    if img.pages.is_empty() {
        return Err(Error::Corrupt);
    }
    img.animated = img.pages.len() > 1;
    Ok(img)
}

fn lzw(data: &[u8], min: u8, need: usize) -> Vec<u8> {
    let min = min.clamp(2, 11) as u32;
    let clear = 1u32 << min;
    let end = clear + 1;
    let mut prefix = vec![0u16; 4096];
    let mut suffix = vec![0u8; 4096];
    let mut length = vec![0u16; 4096];
    for i in 0..clear as usize {
        suffix[i] = i as u8;
        length[i] = 1;
    }
    let mut next = end + 1;
    let mut size = min + 1;
    // La capacité est plafonnée : `need` vient du fichier.
    let mut out = Vec::with_capacity(need.min(1 << 24));
    let mut prev: Option<u32> = None;
    let (mut acc, mut nbits, mut p) = (0u32, 0u32, 0usize);
    let mut tmp = Vec::new();
    loop {
        while nbits < size {
            let Some(&b) = data.get(p) else {
                return out;
            };
            acc |= (b as u32) << nbits;
            nbits += 8;
            p += 1;
        }
        let code = acc & ((1 << size) - 1);
        acc >>= size;
        nbits -= size;
        if code == clear {
            next = end + 1;
            size = min + 1;
            prev = None;
            continue;
        }
        if code == end {
            return out;
        }
        let Some(pv) = prev else {
            if code < clear {
                out.push(code as u8);
                prev = Some(code);
            }
            continue;
        };
        let cur = if code < next { code } else { pv };
        tmp.clear();
        let mut c = cur;
        while c >= clear {
            tmp.push(suffix[c as usize]);
            c = prefix[c as usize] as u32;
            if tmp.len() > 4096 {
                return out;
            }
        }
        tmp.push(c as u8);
        let first = c as u8;
        out.extend(tmp.iter().rev());
        if code >= next {
            out.push(first);
        }
        if next < 4096 {
            prefix[next as usize] = pv as u16;
            suffix[next as usize] = first;
            length[next as usize] = length[pv as usize] + 1;
            next += 1;
            if next == (1 << size) && size < 12 {
                size += 1;
            }
        }
        prev = Some(code);
        if out.len() >= need {
            out.truncate(need);
            return out;
        }
    }
}
