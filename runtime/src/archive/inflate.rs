//! Décompresseur DEFLATE (RFC 1951) écrit de zéro, en flux : la sortie est
//! produite au fil de la lecture, sans charger toute l'archive en mémoire.

use std::io::{self, BufRead, Read};

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("deflate : {msg}"))
}

/// Lecture de bits poids faible d'abord, comme l'exige DEFLATE.
pub struct BitReader<R: BufRead> {
    inner: R,
    buf: u64,
    count: u32,
}

impl<R: BufRead> BitReader<R> {
    pub fn new(inner: R) -> Self {
        BitReader {
            inner,
            buf: 0,
            count: 0,
        }
    }

    fn refill(&mut self) -> io::Result<()> {
        while self.count <= 56 {
            let avail = self.inner.fill_buf()?;
            if avail.is_empty() {
                break;
            }
            let take = (((64 - self.count) / 8) as usize).min(avail.len());
            for (i, &b) in avail[..take].iter().enumerate() {
                self.buf |= (b as u64) << (self.count + 8 * i as u32);
            }
            self.count += 8 * take as u32;
            self.inner.consume(take);
        }
        Ok(())
    }

    /// Les `n` prochains bits (complétés par des zéros en fin de flux).
    fn peek(&mut self, n: u32) -> io::Result<u32> {
        if self.count < n {
            self.refill()?;
        }
        Ok((self.buf & ((1u64 << n) - 1)) as u32)
    }

    fn consume(&mut self, n: u32) -> io::Result<()> {
        if n > self.count {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "deflate : flux tronqué",
            ));
        }
        self.buf >>= n;
        self.count -= n;
        Ok(())
    }

    pub fn bits(&mut self, n: u32) -> io::Result<u32> {
        if n == 0 {
            return Ok(0);
        }
        let v = self.peek(n)?;
        self.consume(n)?;
        Ok(v)
    }

    pub fn align(&mut self) {
        let drop = self.count % 8;
        self.buf >>= drop;
        self.count -= drop;
    }

    /// Lit des octets entiers après `align()` (en-têtes, blocs stockés, gzip).
    pub fn read_bytes(&mut self, out: &mut [u8]) -> io::Result<()> {
        let mut i = 0;
        while i < out.len() && self.count >= 8 {
            out[i] = self.buf as u8;
            self.buf >>= 8;
            self.count -= 8;
            i += 1;
        }
        if i < out.len() {
            self.inner.read_exact(&mut out[i..])?;
        }
        Ok(())
    }

    /// Indique s'il reste au moins un octet (sans le consommer).
    pub fn at_eof(&mut self) -> io::Result<bool> {
        if self.count >= 8 {
            return Ok(false);
        }
        Ok(self.inner.fill_buf()?.is_empty())
    }
}

/// Table de décodage canonique : indice = `bits` inversés, valeur =
/// symbole << 4 | longueur (0 = code invalide).
struct Table {
    entries: Vec<u16>,
    max: u32,
}

impl Table {
    fn new(lengths: &[u8]) -> io::Result<Table> {
        let max = lengths.iter().copied().max().unwrap_or(0) as u32;
        let mut entries = vec![0u16; 1 << max.max(1)];
        if max == 0 {
            return Ok(Table { entries, max: 1 });
        }
        let mut bl_count = [0u32; 16];
        for &l in lengths {
            bl_count[l as usize] += 1;
        }
        bl_count[0] = 0;
        let mut next = [0u32; 16];
        let mut code = 0u32;
        let mut left: i64 = 1;
        for bits in 1..16 {
            code = (code + bl_count[bits - 1]) << 1;
            next[bits] = code;
            left = (left << 1) - bl_count[bits] as i64;
            if left < 0 {
                return Err(bad("code de Huffman sursouscrit"));
            }
        }
        for (sym, &l) in lengths.iter().enumerate() {
            if l == 0 {
                continue;
            }
            let l = l as u32;
            let c = next[l as usize];
            next[l as usize] += 1;
            let rev = c.reverse_bits() >> (32 - l);
            let mut k = rev;
            while k < (1 << max) {
                entries[k as usize] = ((sym as u16) << 4) | l as u16;
                k += 1 << l;
            }
        }
        Ok(Table { entries, max })
    }

    fn decode<R: BufRead>(&self, br: &mut BitReader<R>) -> io::Result<u16> {
        let bits = br.peek(self.max)?;
        let e = self.entries[bits as usize];
        if e & 15 == 0 {
            return Err(bad("code de Huffman invalide"));
        }
        br.consume((e & 15) as u32)?;
        Ok(e >> 4)
    }
}

pub(crate) const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
pub(crate) const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
pub(crate) const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
pub(crate) const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
pub(crate) const CL_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

enum State {
    Header,
    Stored(u32),
    Codes(Box<(Table, Table)>),
    Done,
}

const WINDOW: usize = 32 * 1024;
const TARGET: usize = 64 * 1024;

/// Lecteur qui décompresse un flux DEFLATE brut.
pub struct Inflater<R: BufRead> {
    br: BitReader<R>,
    state: State,
    last: bool,
    out: Vec<u8>,
    pos: usize,
}

impl<R: BufRead> Inflater<R> {
    pub fn new(inner: R) -> Self {
        Self::from_bits(BitReader::new(inner))
    }

    pub fn from_bits(br: BitReader<R>) -> Self {
        Inflater {
            br,
            state: State::Header,
            last: false,
            out: Vec::with_capacity(WINDOW + TARGET + 512),
            pos: 0,
        }
    }

    /// Décompresseur dont la fenêtre contient déjà `history` : les blocs MSZIP
    /// des cabinets (.cab) peuvent référencer la sortie du bloc précédent.
    pub fn with_history(inner: R, history: &[u8]) -> Self {
        let mut me = Self::new(inner);
        let keep = history.len().min(WINDOW);
        me.out.extend_from_slice(&history[history.len() - keep..]);
        me.pos = me.out.len();
        me
    }

    /// Récupère le lecteur de bits (pour lire la fin d'un membre gzip).
    pub fn into_bits(self) -> BitReader<R> {
        self.br
    }

    fn header(&mut self) -> io::Result<()> {
        self.last = self.br.bits(1)? == 1;
        match self.br.bits(2)? {
            0 => {
                self.br.align();
                let mut h = [0u8; 4];
                self.br.read_bytes(&mut h)?;
                let len = u16::from_le_bytes([h[0], h[1]]);
                let nlen = u16::from_le_bytes([h[2], h[3]]);
                if len != !nlen {
                    return Err(bad("longueur de bloc stocké incohérente"));
                }
                self.state = State::Stored(len as u32);
            }
            1 => {
                let mut l = [0u8; 288];
                l[..144].fill(8);
                l[144..256].fill(9);
                l[256..280].fill(7);
                l[280..].fill(8);
                let d = [5u8; 30];
                self.state = State::Codes(Box::new((Table::new(&l)?, Table::new(&d)?)));
            }
            2 => {
                let hlit = self.br.bits(5)? as usize + 257;
                let hdist = self.br.bits(5)? as usize + 1;
                let hclen = self.br.bits(4)? as usize + 4;
                let mut cl = [0u8; 19];
                for &i in CL_ORDER.iter().take(hclen) {
                    cl[i] = self.br.bits(3)? as u8;
                }
                let clt = Table::new(&cl)?;
                let mut lens = [0u8; 320];
                let mut i = 0;
                while i < hlit + hdist {
                    let sym = clt.decode(&mut self.br)?;
                    match sym {
                        0..=15 => {
                            lens[i] = sym as u8;
                            i += 1;
                        }
                        16 => {
                            if i == 0 {
                                return Err(bad("répétition sans longueur précédente"));
                            }
                            let prev = lens[i - 1];
                            let n = 3 + self.br.bits(2)? as usize;
                            if i + n > hlit + hdist {
                                return Err(bad("répétition trop longue"));
                            }
                            lens[i..i + n].fill(prev);
                            i += n;
                        }
                        17 | 18 => {
                            let n = if sym == 17 {
                                3 + self.br.bits(3)? as usize
                            } else {
                                11 + self.br.bits(7)? as usize
                            };
                            if i + n > hlit + hdist {
                                return Err(bad("répétition trop longue"));
                            }
                            lens[i..i + n].fill(0);
                            i += n;
                        }
                        _ => return Err(bad("symbole de longueur invalide")),
                    }
                }
                if lens[256] == 0 {
                    return Err(bad("pas de code de fin de bloc"));
                }
                let lit = Table::new(&lens[..hlit])?;
                let dist = Table::new(&lens[hlit..hlit + hdist])?;
                self.state = State::Codes(Box::new((lit, dist)));
            }
            _ => return Err(bad("type de bloc réservé")),
        }
        Ok(())
    }

    fn end_block(&mut self) {
        self.state = if self.last {
            self.br.align();
            State::Done
        } else {
            State::Header
        };
    }

    fn fill(&mut self) -> io::Result<()> {
        if self.pos == self.out.len() && self.out.len() > WINDOW + TARGET {
            let cut = self.out.len() - WINDOW;
            self.out.drain(..cut);
            self.pos = self.out.len();
        }
        while self.out.len() - self.pos < TARGET {
            match &mut self.state {
                State::Done => break,
                State::Header => self.header()?,
                State::Stored(rem) => {
                    if *rem == 0 {
                        self.end_block();
                        continue;
                    }
                    let n = (*rem as usize).min(TARGET);
                    let start = self.out.len();
                    self.out.resize(start + n, 0);
                    self.br.read_bytes(&mut self.out[start..])?;
                    *rem -= n as u32;
                }
                State::Codes(tables) => {
                    let (lit, dist) = (&tables.0, &tables.1);
                    let mut ended = false;
                    while self.out.len() - self.pos < TARGET {
                        let sym = lit.decode(&mut self.br)?;
                        if sym < 256 {
                            self.out.push(sym as u8);
                        } else if sym == 256 {
                            ended = true;
                            break;
                        } else {
                            let s = (sym - 257) as usize;
                            if s >= 29 {
                                return Err(bad("longueur invalide"));
                            }
                            let len =
                                LEN_BASE[s] as usize + self.br.bits(LEN_EXTRA[s] as u32)? as usize;
                            let ds = dist.decode(&mut self.br)? as usize;
                            if ds >= 30 {
                                return Err(bad("distance invalide"));
                            }
                            let d = DIST_BASE[ds] as usize
                                + self.br.bits(DIST_EXTRA[ds] as u32)? as usize;
                            if d > self.out.len() {
                                return Err(bad("distance hors de la fenêtre"));
                            }
                            let start = self.out.len() - d;
                            if d >= len {
                                self.out.extend_from_within(start..start + len);
                            } else {
                                for k in 0..len {
                                    let b = self.out[start + k];
                                    self.out.push(b);
                                }
                            }
                        }
                    }
                    if ended {
                        self.end_block();
                    }
                }
            }
        }
        Ok(())
    }
}

impl<R: BufRead> Read for Inflater<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.pos == self.out.len() {
            self.fill()?;
        }
        let n = (self.out.len() - self.pos).min(buf.len());
        buf[..n].copy_from_slice(&self.out[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// Décompresse entièrement un tampon DEFLATE brut.
#[cfg(test)]
pub fn inflate(data: &[u8]) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    Inflater::new(data).read_to_end(&mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_block() {
        // Bloc final stocké contenant « abc ».
        let data = [0x01, 0x03, 0x00, 0xFC, 0xFF, b'a', b'b', b'c'];
        assert_eq!(inflate(&data).unwrap(), b"abc");
    }

    #[test]
    fn fixed_block_from_zlib() {
        // zlib.compress(b"hello hello hello hello")[2:-4]
        let data = [0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x27, 0x01];
        assert_eq!(inflate(&data).unwrap(), b"hello hello hello hello");
    }

    #[test]
    fn truncated_is_error() {
        let data = [0xcb, 0x48, 0xcd];
        assert!(inflate(&data).is_err());
    }
}
