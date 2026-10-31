//! Compresseur DEFLATE (RFC 1951) écrit de zéro : LZ77 à chaînes de
//! hachage, puis codes de Huffman dynamiques, fixes ou bloc stocké, selon ce
//! qui est le plus court. Fonctionne en flux par blocs de 256 Kio.

use super::inflate::{CL_ORDER, DIST_BASE, DIST_EXTRA, LEN_BASE, LEN_EXTRA};
use std::collections::BinaryHeap;
use std::io::{self, Write};

pub struct BitWriter<W: Write> {
    inner: W,
    buf: u64,
    count: u32,
    out: Vec<u8>,
}

impl<W: Write> BitWriter<W> {
    pub fn new(inner: W) -> Self {
        BitWriter {
            inner,
            buf: 0,
            count: 0,
            out: Vec::with_capacity(64 * 1024),
        }
    }

    fn put(&mut self, bits: u32, n: u32) {
        self.buf |= (bits as u64) << self.count;
        self.count += n;
        while self.count >= 8 {
            self.out.push(self.buf as u8);
            self.buf >>= 8;
            self.count -= 8;
        }
    }

    fn put_code(&mut self, code: u16, len: u8) {
        let rev = (code as u32).reverse_bits() >> (32 - len as u32);
        self.put(rev, len as u32);
    }

    fn align(&mut self) {
        if self.count > 0 {
            self.put(0, 8 - self.count);
        }
    }

    fn flush_out(&mut self) -> io::Result<()> {
        if !self.out.is_empty() {
            self.inner.write_all(&self.out)?;
            self.out.clear();
        }
        Ok(())
    }

    fn finish(mut self) -> io::Result<W> {
        self.align();
        self.flush_out()?;
        Ok(self.inner)
    }
}

const fn len_codes() -> [u8; 259] {
    let mut t = [0u8; 259];
    let mut s = 0;
    while s < 29 {
        let base = LEN_BASE[s] as usize;
        let end = if s == 28 {
            259
        } else {
            LEN_BASE[s + 1] as usize
        };
        let mut l = base;
        while l < end {
            t[l] = s as u8;
            l += 1;
        }
        s += 1;
    }
    // 258 a son propre code (285), et non « 227 + 31 ».
    t[258] = 28;
    t
}
static LEN_CODE: [u8; 259] = len_codes();

fn dist_code(d: u16) -> usize {
    let mut s = 29;
    while DIST_BASE[s] > d {
        s -= 1;
    }
    s
}

#[derive(Clone, Copy)]
struct Tok {
    len: u16, // 0 = littéral
    val: u16, // octet ou distance
}

const HASH_BITS: u32 = 15;
const WINDOW: usize = 32 * 1024;
const MAX_CHAIN: usize = 48;
const CHUNK: usize = 256 * 1024;

fn hash(d: &[u8], i: usize) -> usize {
    let v = (d[i] as u32) << 16 | (d[i + 1] as u32) << 8 | d[i + 2] as u32;
    (v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
}

fn lz77(data: &[u8]) -> Vec<Tok> {
    let n = data.len();
    let mut toks = Vec::with_capacity(n / 2 + 16);
    let mut head = vec![u32::MAX; 1 << HASH_BITS];
    let mut prev = vec![u32::MAX; n];
    let insert = |head: &mut [u32], prev: &mut [u32], i: usize| {
        if i + 3 <= n {
            let h = hash(data, i);
            prev[i] = head[h];
            head[h] = i as u32;
        }
    };
    let mut i = 0;
    while i < n {
        let mut best_len = 0usize;
        let mut best_dist = 0usize;
        if i + 3 <= n {
            let h = hash(data, i);
            let mut cand = head[h];
            let max_len = (n - i).min(258);
            let mut chain = 0;
            while cand != u32::MAX && chain < MAX_CHAIN {
                let c = cand as usize;
                if i - c > WINDOW {
                    break;
                }
                if data[c + best_len.min(max_len - 1)] == data[i + best_len.min(max_len - 1)] {
                    let mut l = 0;
                    while l < max_len && data[c + l] == data[i + l] {
                        l += 1;
                    }
                    if l > best_len {
                        best_len = l;
                        best_dist = i - c;
                        if l == max_len {
                            break;
                        }
                    }
                }
                cand = prev[c];
                chain += 1;
            }
        }
        if best_len >= 3 {
            toks.push(Tok {
                len: best_len as u16,
                val: best_dist as u16,
            });
            for k in i..i + best_len {
                insert(&mut head, &mut prev, k);
            }
            i += best_len;
        } else {
            toks.push(Tok {
                len: 0,
                val: data[i] as u16,
            });
            insert(&mut head, &mut prev, i);
            i += 1;
        }
    }
    toks
}

/// Longueurs de codes de Huffman limitées à `limit` bits.
fn code_lengths(freq: &[u32], limit: u8) -> Vec<u8> {
    let mut f: Vec<u64> = freq.iter().map(|&x| x as u64).collect();
    loop {
        let mut lens = vec![0u8; f.len()];
        let used: Vec<usize> = (0..f.len()).filter(|&i| f[i] > 0).collect();
        if used.len() == 1 {
            lens[used[0]] = 1;
            return lens;
        }
        if used.is_empty() {
            return lens;
        }
        // Arbre de Huffman : feuilles 0..n, nœuds internes ensuite.
        let mut parent = vec![usize::MAX; f.len() * 2];
        let mut heap = BinaryHeap::new();
        for &i in &used {
            heap.push(std::cmp::Reverse((f[i], i)));
        }
        let mut next = f.len();
        while heap.len() > 1 {
            let std::cmp::Reverse((fa, a)) = heap.pop().unwrap();
            let std::cmp::Reverse((fb, b)) = heap.pop().unwrap();
            parent[a] = next;
            parent[b] = next;
            heap.push(std::cmp::Reverse((fa + fb, next)));
            next += 1;
        }
        let mut max = 0u8;
        for &i in &used {
            let mut d = 0u8;
            let mut k = i;
            while parent[k] != usize::MAX {
                k = parent[k];
                d = d.saturating_add(1);
            }
            lens[i] = d;
            max = max.max(d);
        }
        if max <= limit {
            return lens;
        }
        for x in f.iter_mut() {
            if *x > 0 {
                *x = (*x >> 1).max(1);
            }
        }
    }
}

fn canonical(lens: &[u8]) -> Vec<u16> {
    let mut bl = [0u16; 16];
    for &l in lens {
        bl[l as usize] += 1;
    }
    bl[0] = 0;
    let mut next = [0u16; 16];
    let mut code = 0u16;
    for b in 1..16 {
        code = (code + bl[b - 1]) << 1;
        next[b] = code;
    }
    lens.iter()
        .map(|&l| {
            if l == 0 {
                0
            } else {
                let c = next[l as usize];
                next[l as usize] += 1;
                c
            }
        })
        .collect()
}

fn fixed_lengths() -> (Vec<u8>, Vec<u8>) {
    let mut l = vec![8u8; 288];
    l[144..256].fill(9);
    l[256..280].fill(7);
    (l, vec![5u8; 30])
}

/// Séquence RLE des longueurs (symbole, bits supplémentaires, valeur).
fn rle(lens: &[u8]) -> Vec<(u8, u8, u8)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lens.len() {
        let l = lens[i];
        let mut run = 1;
        while i + run < lens.len() && lens[i + run] == l {
            run += 1;
        }
        let mut left = run;
        if l == 0 {
            while left >= 11 {
                let n = left.min(138);
                out.push((18, 7, (n - 11) as u8));
                left -= n;
            }
            if left >= 3 {
                out.push((17, 3, (left - 3) as u8));
                left = 0;
            }
        } else {
            out.push((l, 0, 0));
            left -= 1;
            while left >= 3 {
                let n = left.min(6);
                out.push((16, 2, (n - 3) as u8));
                left -= n;
            }
        }
        for _ in 0..left {
            out.push((l, 0, 0));
        }
        i += run;
    }
    out
}

fn block_cost(toks: &[Tok], ll: &[u8], dl: &[u8]) -> u64 {
    let mut c = ll[256] as u64;
    for t in toks {
        if t.len == 0 {
            c += ll[t.val as usize] as u64;
        } else {
            let s = LEN_CODE[t.len as usize] as usize;
            let d = dist_code(t.val);
            c += ll[257 + s] as u64 + LEN_EXTRA[s] as u64 + dl[d] as u64 + DIST_EXTRA[d] as u64;
        }
    }
    c
}

fn write_tokens<W: Write>(
    bw: &mut BitWriter<W>,
    toks: &[Tok],
    ll: &[u8],
    lc: &[u16],
    dl: &[u8],
    dc: &[u16],
) -> io::Result<()> {
    for t in toks {
        if t.len == 0 {
            let v = t.val as usize;
            bw.put_code(lc[v], ll[v]);
        } else {
            let s = LEN_CODE[t.len as usize] as usize;
            bw.put_code(lc[257 + s], ll[257 + s]);
            bw.put((t.len - LEN_BASE[s]) as u32, LEN_EXTRA[s] as u32);
            let d = dist_code(t.val);
            bw.put_code(dc[d], dl[d]);
            bw.put((t.val - DIST_BASE[d]) as u32, DIST_EXTRA[d] as u32);
        }
        if bw.out.len() >= 64 * 1024 {
            bw.flush_out()?;
        }
    }
    bw.put_code(lc[256], ll[256]);
    Ok(())
}

fn compress_block<W: Write>(bw: &mut BitWriter<W>, data: &[u8], last: bool) -> io::Result<()> {
    let toks = lz77(data);
    let mut lf = [0u32; 286];
    let mut df = [0u32; 30];
    lf[256] = 1;
    for t in &toks {
        if t.len == 0 {
            lf[t.val as usize] += 1;
        } else {
            lf[257 + LEN_CODE[t.len as usize] as usize] += 1;
            df[dist_code(t.val)] += 1;
        }
    }
    // Au moins deux codes de distance : certains décodeurs anciens l'exigent.
    df[0] = df[0].max(1);
    df[1] = df[1].max(1);
    let ll = code_lengths(&lf, 15);
    let dl = code_lengths(&df, 15);
    let hlit = (257..=286).rev().find(|&n| ll[n - 1] != 0).unwrap_or(257);
    let hdist = (1..=30).rev().find(|&n| dl[n - 1] != 0).unwrap_or(1);
    let mut all = ll[..hlit].to_vec();
    all.extend_from_slice(&dl[..hdist]);
    let seq = rle(&all);
    let mut cf = [0u32; 19];
    for &(s, _, _) in &seq {
        cf[s as usize] += 1;
    }
    let cl = code_lengths(&cf, 7);
    let hclen = (4..=19)
        .rev()
        .find(|&n| cl[CL_ORDER[n - 1]] != 0)
        .unwrap_or(4);
    let mut dyn_cost = 3 + 14 + 3 * hclen as u64 + block_cost(&toks, &ll, &dl);
    for &(s, e, _) in &seq {
        dyn_cost += cl[s as usize] as u64 + e as u64;
    }
    let (fll, fdl) = fixed_lengths();
    let fixed_cost = 3 + block_cost(&toks, &fll, &fdl);
    let stored_cost = (data.len() as u64 + 5 * (data.len() as u64 / 65535 + 1)) * 8 + 8;

    if stored_cost <= dyn_cost.min(fixed_cost) {
        let mut chunks = data.chunks(65535).peekable();
        if data.is_empty() {
            bw.put(last as u32, 1);
            bw.put(0, 2);
            bw.align();
            bw.put(0, 16);
            bw.put(0xFFFF, 16);
        }
        while let Some(c) = chunks.next() {
            let final_chunk = last && chunks.peek().is_none();
            bw.put(final_chunk as u32, 1);
            bw.put(0, 2);
            bw.align();
            bw.put(c.len() as u32, 16);
            bw.put(!(c.len() as u16) as u32, 16);
            bw.out.extend_from_slice(c);
            bw.flush_out()?;
        }
        return Ok(());
    }
    bw.put(last as u32, 1);
    if fixed_cost <= dyn_cost {
        bw.put(1, 2);
        write_tokens(bw, &toks, &fll, &canonical(&fll), &fdl, &canonical(&fdl))?;
    } else {
        bw.put(2, 2);
        bw.put((hlit - 257) as u32, 5);
        bw.put((hdist - 1) as u32, 5);
        bw.put((hclen - 4) as u32, 4);
        for &i in CL_ORDER.iter().take(hclen) {
            bw.put(cl[i] as u32, 3);
        }
        let cc = canonical(&cl);
        for &(s, e, v) in &seq {
            bw.put_code(cc[s as usize], cl[s as usize]);
            bw.put(v as u32, e as u32);
        }
        write_tokens(bw, &toks, &ll, &canonical(&ll), &dl, &canonical(&dl))?;
    }
    bw.flush_out()
}

/// Écrivain qui compresse en DEFLATE brut.
pub struct Deflater<W: Write> {
    bw: BitWriter<W>,
    pending: Vec<u8>,
}

impl<W: Write> Deflater<W> {
    pub fn new(inner: W) -> Self {
        Deflater {
            bw: BitWriter::new(inner),
            pending: Vec::with_capacity(CHUNK),
        }
    }

    pub fn finish(mut self) -> io::Result<W> {
        let data = std::mem::take(&mut self.pending);
        compress_block(&mut self.bw, &data, true)?;
        self.bw.finish()
    }
}

impl<W: Write> Write for Deflater<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let room = CHUNK - self.pending.len();
        let n = room.min(buf.len());
        self.pending.extend_from_slice(&buf[..n]);
        if self.pending.len() == CHUNK {
            let data = std::mem::take(&mut self.pending);
            compress_block(&mut self.bw, &data, false)?;
            self.pending = data;
            self.pending.clear();
        }
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Compresse entièrement un tampon (DEFLATE brut).
#[cfg(test)]
pub fn deflate(data: &[u8]) -> io::Result<Vec<u8>> {
    let mut d = Deflater::new(Vec::new());
    d.write_all(data)?;
    d.finish()
}

#[cfg(test)]
mod tests {
    use super::super::inflate::inflate;
    use super::*;

    fn roundtrip(data: &[u8]) {
        let c = deflate(data).unwrap();
        assert_eq!(inflate(&c).unwrap(), data, "taille {}", data.len());
    }

    #[test]
    fn roundtrips() {
        roundtrip(b"");
        roundtrip(b"a");
        roundtrip(b"hello hello hello hello");
        let text: Vec<u8> = (0..300_000u32)
            .flat_map(|i| format!("ligne {} du fichier Winlux\n", i % 977).into_bytes())
            .collect();
        roundtrip(&text);
        let c = deflate(&text).unwrap();
        assert!(c.len() < text.len() / 5, "compression trop faible");
        // Données pseudo-aléatoires : bloc stocké.
        let mut x = 0x1234_5678u32;
        let noise: Vec<u8> = (0..600_000)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect();
        roundtrip(&noise);
        roundtrip(&vec![0u8; 1_000_000]);
    }

    #[test]
    fn length_limit_respected() {
        let freq: Vec<u32> = (0..286).map(|i| 1u32 << (i % 30).min(30)).collect();
        let l = code_lengths(&freq, 15);
        assert!(l.iter().all(|&x| x <= 15));
    }
}
