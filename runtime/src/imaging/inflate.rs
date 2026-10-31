//! Décompresseur DEFLATE maison (RFC 1951) avec enveloppes zlib (RFC 1950)
//! et gzip (RFC 1952). Utilisé par PNG, TIFF, PSD, EXR, SVGZ, FBX et XCF.

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u32,
    nbits: u32,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Bits {
            data,
            pos: 0,
            bit: 0,
            nbits: 0,
        }
    }
    fn need(&mut self, n: u32) -> Option<()> {
        while self.nbits < n {
            let b = *self.data.get(self.pos)?;
            self.pos += 1;
            self.bit |= (b as u32) << self.nbits;
            self.nbits += 8;
        }
        Some(())
    }
    fn get(&mut self, n: u32) -> Option<u32> {
        if n == 0 {
            return Some(0);
        }
        self.need(n)?;
        let v = self.bit & ((1u32 << n) - 1);
        self.bit >>= n;
        self.nbits -= n;
        Some(v)
    }
    fn align(&mut self) {
        let r = self.nbits % 8;
        self.bit >>= r;
        self.nbits -= r;
    }
}

/// Table de Huffman canonique (codes jusqu'à 15 bits) avec table rapide
/// sur 9 bits.
struct Huff {
    counts: [u16; 16],
    symbols: Vec<u16>,
    fast: Vec<(u8, u16)>,
}

const FAST: u32 = 9;

impl Huff {
    fn new(lengths: &[u8]) -> Option<Huff> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut offs = [0u16; 16];
        for i in 1..16 {
            offs[i] = offs[i - 1] + counts[i - 1];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (s, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
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
        Some(Huff { counts, symbols, fast })
    }
    fn decode(&self, b: &mut Bits) -> Option<u16> {
        // Lecture rapide si assez de bits sont disponibles.
        let _ = b.need(FAST);
        if b.nbits >= FAST {
            let (l, s) = self.fast[(b.bit & ((1 << FAST) - 1)) as usize];
            if l > 0 {
                b.bit >>= l;
                b.nbits -= l as u32;
                return Some(s);
            }
        }
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for len in 1..16 {
            code |= b.get(1)? as i32;
            let count = self.counts[len] as i32;
            if code - count < first {
                return self.symbols.get((index + (code - first)) as usize).copied();
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        None
    }
}

const LBASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
    131, 163, 195, 227, 258,
];
const LEXT: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DBASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DEXT: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// Décompresse un flux DEFLATE brut. `limit` borne la taille de sortie.
pub fn inflate_raw(data: &[u8], limit: usize) -> Option<Vec<u8>> {
    inflate_inner(data, limit).map(|(v, _)| v)
}

fn inflate_inner(data: &[u8], limit: usize) -> Option<(Vec<u8>, usize)> {
    let mut b = Bits::new(data);
    let mut out: Vec<u8> = Vec::new();
    loop {
        let last = b.get(1)?;
        let kind = b.get(2)?;
        match kind {
            0 => {
                b.align();
                // rend les octets déjà chargés dans le tampon de bits
                let p = b.pos - (b.nbits / 8) as usize;
                b.bit = 0;
                b.nbits = 0;
                let len = u16::from_le_bytes([*data.get(p)?, *data.get(p + 1)?]) as usize;
                let src = data.get(p + 4..p + 4 + len)?;
                out.extend_from_slice(src);
                b.pos = p + 4 + len;
            }
            1 | 2 => {
                let (lit, dist) = if kind == 1 {
                    let mut l = [0u8; 288];
                    for (i, v) in l.iter_mut().enumerate() {
                        *v = match i {
                            0..=143 => 8,
                            144..=255 => 9,
                            256..=279 => 7,
                            _ => 8,
                        };
                    }
                    (Huff::new(&l)?, Huff::new(&[5u8; 30])?)
                } else {
                    let hlit = b.get(5)? as usize + 257;
                    let hdist = b.get(5)? as usize + 1;
                    let hclen = b.get(4)? as usize + 4;
                    const ORD: [usize; 19] =
                        [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                    let mut cl = [0u8; 19];
                    for &o in ORD.iter().take(hclen) {
                        cl[o] = b.get(3)? as u8;
                    }
                    let ch = Huff::new(&cl)?;
                    let mut lens = vec![0u8; hlit + hdist];
                    let mut i = 0;
                    while i < hlit + hdist {
                        let sym = ch.decode(&mut b)?;
                        match sym {
                            0..=15 => {
                                lens[i] = sym as u8;
                                i += 1;
                            }
                            16 => {
                                let prev = *lens.get(i.checked_sub(1)?)?;
                                for _ in 0..3 + b.get(2)? {
                                    *lens.get_mut(i)? = prev;
                                    i += 1;
                                }
                            }
                            17 => i += 3 + b.get(3)? as usize,
                            _ => i += 11 + b.get(7)? as usize,
                        }
                    }
                    if i > hlit + hdist {
                        return None;
                    }
                    (Huff::new(&lens[..hlit])?, Huff::new(&lens[hlit..])?)
                };
                loop {
                    let sym = lit.decode(&mut b)?;
                    if sym < 256 {
                        out.push(sym as u8);
                    } else if sym == 256 {
                        break;
                    } else {
                        let s = (sym - 257) as usize;
                        let len = *LBASE.get(s)? as usize + b.get(*LEXT.get(s)? as u32)? as usize;
                        let d = dist.decode(&mut b)? as usize;
                        let dd = *DBASE.get(d)? as usize + b.get(*DEXT.get(d)? as u32)? as usize;
                        if dd > out.len() {
                            return None;
                        }
                        let start = out.len() - dd;
                        for k in 0..len {
                            let v = out[start + k];
                            out.push(v);
                        }
                    }
                    if out.len() > limit {
                        return None;
                    }
                }
            }
            _ => return None,
        }
        if out.len() > limit {
            return None;
        }
        if last == 1 {
            break;
        }
    }
    // Octets consommés (arrondis à l'octet).
    let used = b.pos - (b.nbits / 8) as usize;
    Some((out, used))
}

/// Décompresse un flux zlib (en-tête 2 octets + DEFLATE + Adler-32).
pub fn inflate_zlib(data: &[u8], limit: usize) -> Option<Vec<u8>> {
    if data.len() < 2 || data[0] & 0x0F != 8 || (((data[0] as u16) << 8) | data[1] as u16) % 31 != 0
    {
        // Certains fichiers (TIFF, PSD) écrivent parfois du DEFLATE brut.
        return inflate_raw(data, limit);
    }
    inflate_raw(&data[2..], limit)
}

/// Décompresse un fichier gzip (un ou plusieurs membres).
pub fn gunzip(data: &[u8], limit: usize) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut p = 0;
    while p + 18 <= data.len() && data[p] == 0x1F && data[p + 1] == 0x8B {
        let flags = data[p + 3];
        let mut q = p + 10;
        if flags & 4 != 0 {
            let x = u16::from_le_bytes([*data.get(q)?, *data.get(q + 1)?]) as usize;
            q += 2 + x;
        }
        for bit in [8u8, 16] {
            if flags & bit != 0 {
                while *data.get(q)? != 0 {
                    q += 1;
                }
                q += 1;
            }
        }
        if flags & 2 != 0 {
            q += 2;
        }
        let (chunk, used) = inflate_inner(data.get(q..)?, limit.saturating_sub(out.len()))?;
        out.extend_from_slice(&chunk);
        p = q + used + 8;
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompresse_bloc_stocke_et_fixe() {
        // zlib.compress(b"hello hello hello", 9)
        let z = [
            0x78, 0xda, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x90, 0x00, 0x3a, 0x2e,
            0x06, 0x7d,
        ];
        assert_eq!(inflate_zlib(&z, 1 << 20).unwrap(), b"hello hello hello");
        // Bloc stocké : BFINAL=1, BTYPE=00
        let stored = [0x01, 0x03, 0x00, 0xfc, 0xff, b'a', b'b', b'c'];
        assert_eq!(inflate_raw(&stored, 100).unwrap(), b"abc");
    }
}
