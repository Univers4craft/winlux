//! Décompresseur LZX (variante des cabinets Microsoft), écrit de zéro à
//! partir de la description publique du format : arbres de Huffman
//! canoniques lus poids fort d'abord dans des mots de 16 bits, trames de
//! 32 Ko, files de distances R0/R1/R2 et translation des appels x86 (E8).

const FRAME: usize = 32768;
const MAIN_CHARS: usize = 256;
const LENGTHS: usize = 249;
const PRETREE: usize = 20;
const ALIGNED: usize = 8;
const MAX_BITS: u32 = 16;

fn bad() -> String {
    "flux LZX endommagé".into()
}

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u64,
    left: u32,
    padding: usize,
}

impl Bits<'_> {
    fn word(&mut self) -> Result<(), String> {
        let w = match self.data.get(self.pos..self.pos + 2) {
            Some(b) => u16::from_le_bytes([b[0], b[1]]),
            None => {
                // Fin des données : quelques mots nuls, comme un décodeur
                // tolérant, puis erreur.
                self.padding += 1;
                if self.padding > 8 {
                    return Err(bad());
                }
                0
            }
        };
        self.pos += 2;
        self.buf |= (w as u64) << (48 - self.left);
        self.left += 16;
        Ok(())
    }

    fn ensure(&mut self, n: u32) -> Result<(), String> {
        while self.left < n {
            self.word()?;
        }
        Ok(())
    }

    fn peek(&self, n: u32) -> u32 {
        if n == 0 {
            0
        } else {
            (self.buf >> (64 - n)) as u32
        }
    }

    fn remove(&mut self, n: u32) {
        self.buf <<= n;
        self.left -= n;
    }

    fn read(&mut self, n: u32) -> Result<u32, String> {
        if n == 0 {
            return Ok(0);
        }
        self.ensure(n)?;
        let v = self.peek(n);
        self.remove(n);
        Ok(v)
    }
}

/// Arbre de Huffman canonique, table directe sur 16 bits.
struct Tree {
    table: Vec<(u16, u8)>,
    empty: bool,
}

impl Tree {
    fn new(lens: &[u8]) -> Result<Tree, String> {
        let mut table = vec![(0u16, 0u8); 1 << MAX_BITS];
        let mut code: u32 = 0;
        let mut any = false;
        for len in 1..=MAX_BITS as u8 {
            for (sym, &l) in lens.iter().enumerate() {
                if l != len {
                    continue;
                }
                any = true;
                let span = 1u32 << (MAX_BITS - len as u32);
                let start = code << (MAX_BITS - len as u32);
                if start + span > 1 << MAX_BITS {
                    return Err(bad()); // trop de codes pour leur longueur
                }
                for e in &mut table[start as usize..(start + span) as usize] {
                    *e = (sym as u16, len);
                }
                code += 1;
            }
            code <<= 1;
        }
        Ok(Tree { table, empty: !any })
    }

    fn decode(&self, bits: &mut Bits) -> Result<usize, String> {
        bits.ensure(MAX_BITS)?;
        let (sym, len) = self.table[bits.peek(MAX_BITS) as usize];
        if len == 0 {
            return Err(bad());
        }
        bits.remove(len as u32);
        Ok(sym as usize)
    }
}

fn position_tables() -> ([u32; 51], [u32; 51]) {
    let mut base = [0u32; 51];
    let mut extra = [0u32; 51];
    let mut b = 0u32;
    for i in 0..51 {
        extra[i] = if i < 4 { 0 } else { ((i as u32) - 2) / 2 }.min(17);
        base[i] = b;
        b += 1 << extra[i];
    }
    (base, extra)
}

pub struct Lzx {
    window_bits: u32,
    slots: usize,
}

impl Lzx {
    pub fn new(window_bits: u32) -> Result<Lzx, String> {
        let slots = match window_bits {
            15 => 30,
            16 => 32,
            17 => 34,
            18 => 36,
            19 => 38,
            20 => 42,
            21 => 50,
            _ => return Err("fenêtre LZX non prise en charge".into()),
        };
        Ok(Lzx { window_bits, slots })
    }

    fn read_lengths(
        bits: &mut Bits,
        lens: &mut [u8],
        first: usize,
        last: usize,
    ) -> Result<(), String> {
        let mut pre = [0u8; PRETREE];
        for p in pre.iter_mut() {
            *p = bits.read(4)? as u8;
        }
        let pre = Tree::new(&pre)?;
        let mut x = first;
        while x < last {
            let z = pre.decode(bits)?;
            match z {
                17 => {
                    let run = bits.read(4)? as usize + 4;
                    for _ in 0..run {
                        if x < last {
                            lens[x] = 0;
                            x += 1;
                        }
                    }
                }
                18 => {
                    let run = bits.read(5)? as usize + 20;
                    for _ in 0..run {
                        if x < last {
                            lens[x] = 0;
                            x += 1;
                        }
                    }
                }
                19 => {
                    let run = bits.read(1)? as usize + 4;
                    let z = pre.decode(bits)? as i32;
                    let mut v = lens[x] as i32 - z;
                    if v < 0 {
                        v += 17;
                    }
                    for _ in 0..run {
                        if x < last {
                            lens[x] = v as u8;
                            x += 1;
                        }
                    }
                }
                _ => {
                    let mut v = lens[x] as i32 - z as i32;
                    if v < 0 {
                        v += 17;
                    }
                    lens[x] = v as u8;
                    x += 1;
                }
            }
        }
        Ok(())
    }

    /// Décompresse un dossier complet (données des blocs mises bout à bout).
    pub fn decompress(&self, input: &[u8], total: usize) -> Result<Vec<u8>, String> {
        let (base, extra_bits) = position_tables();
        let window = 1usize << self.window_bits;
        let mut bits = Bits {
            data: input,
            pos: 0,
            buf: 0,
            left: 0,
            padding: 0,
        };
        let mut raw: Vec<u8> = Vec::with_capacity(total.min(window * 4 + FRAME));
        let mut out: Vec<u8> = Vec::with_capacity(total);
        let main_len = MAIN_CHARS + self.slots * 8;
        let mut main_lens = vec![0u8; main_len];
        let mut length_lens = vec![0u8; LENGTHS];
        let (mut r0, mut r1, mut r2) = (1u32, 1u32, 1u32);
        let mut block_type = 0u32;
        let mut block_len = 0usize;
        let mut remaining = 0usize;
        let mut main_tree: Option<Tree> = None;
        let mut length_tree: Option<Tree> = None;
        let mut aligned_tree: Option<Tree> = None;
        let mut intel_started = false;
        let e8 = bits.read(1)?;
        let intel_size = if e8 == 1 {
            let hi = bits.read(16)?;
            let lo = bits.read(16)?;
            ((hi << 16) | lo) as i32
        } else {
            0
        };
        let mut frame = 0usize;
        while out.len() < total {
            let frame_start = raw.len();
            let frame_size = (total - out.len()).min(FRAME);
            let frame_end = frame_start + frame_size;
            while raw.len() < frame_end {
                if remaining == 0 {
                    if block_type == 3 && block_len & 1 == 1 {
                        bits.pos += 1;
                    }
                    block_type = bits.read(3)?;
                    let hi = bits.read(16)?;
                    let lo = bits.read(8)?;
                    block_len = ((hi << 8) | lo) as usize;
                    remaining = block_len;
                    match block_type {
                        1 | 2 => {
                            if block_type == 2 {
                                let mut al = [0u8; ALIGNED];
                                for a in al.iter_mut() {
                                    *a = bits.read(3)? as u8;
                                }
                                aligned_tree = Some(Tree::new(&al)?);
                            }
                            Self::read_lengths(&mut bits, &mut main_lens, 0, MAIN_CHARS)?;
                            Self::read_lengths(&mut bits, &mut main_lens, MAIN_CHARS, main_len)?;
                            let t = Tree::new(&main_lens)?;
                            if t.empty {
                                return Err(bad());
                            }
                            main_tree = Some(t);
                            if main_lens[0xE8] != 0 {
                                intel_started = true;
                            }
                            Self::read_lengths(&mut bits, &mut length_lens, 0, LENGTHS)?;
                            length_tree = Some(Tree::new(&length_lens)?);
                        }
                        3 => {
                            intel_started = true;
                            if bits.left == 0 {
                                bits.ensure(16)?;
                            }
                            // Les mots déjà chargés sont abandonnés : on
                            // repart de la position d'octet courante.
                            bits.buf = 0;
                            bits.left = 0;
                            let mut r = [0u32; 3];
                            for v in r.iter_mut() {
                                let b = input.get(bits.pos..bits.pos + 4).ok_or_else(bad)?;
                                *v = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                                bits.pos += 4;
                            }
                            (r0, r1, r2) = (r[0], r[1], r[2]);
                        }
                        _ => return Err(bad()),
                    }
                }
                let todo = frame_end - raw.len();
                let mut run = remaining.min(todo) as isize;
                remaining -= run as usize;
                match block_type {
                    3 => {
                        let n = run as usize;
                        let b = input.get(bits.pos..bits.pos + n).ok_or_else(bad)?;
                        raw.extend_from_slice(b);
                        bits.pos += n;
                    }
                    _ => {
                        let main = main_tree.as_ref().ok_or_else(bad)?;
                        while run > 0 {
                            let sym = main.decode(&mut bits)?;
                            if sym < MAIN_CHARS {
                                raw.push(sym as u8);
                                run -= 1;
                                continue;
                            }
                            let m = sym - MAIN_CHARS;
                            let mut length = m & 7;
                            if length == 7 {
                                let lt = length_tree.as_ref().ok_or_else(bad)?;
                                if lt.empty {
                                    return Err(bad());
                                }
                                length += lt.decode(&mut bits)?;
                            }
                            length += 2;
                            let slot = m >> 3;
                            let offset = match slot {
                                0 => r0,
                                1 => {
                                    std::mem::swap(&mut r1, &mut r0);
                                    r0
                                }
                                2 => {
                                    std::mem::swap(&mut r2, &mut r0);
                                    r0
                                }
                                _ => {
                                    let extra = if slot >= 36 { 17 } else { extra_bits[slot] };
                                    let mut o = base.get(slot).copied().ok_or_else(bad)? - 2;
                                    if extra >= 3 && block_type == 2 {
                                        if extra > 3 {
                                            o += bits.read(extra - 3)? << 3;
                                        }
                                        let at = aligned_tree.as_ref().ok_or_else(bad)?;
                                        o += at.decode(&mut bits)? as u32;
                                    } else if extra > 0 {
                                        o += bits.read(extra)?;
                                    }
                                    r2 = r1;
                                    r1 = r0;
                                    r0 = o;
                                    o
                                }
                            } as usize;
                            if offset == 0 || offset > raw.len() || offset > window {
                                return Err(bad());
                            }
                            let start = raw.len() - offset;
                            for k in 0..length {
                                let b = raw[start + k];
                                raw.push(b);
                            }
                            run -= length as isize;
                        }
                        if run < 0 {
                            let over = (-run) as usize;
                            if over > remaining {
                                return Err(bad());
                            }
                            remaining -= over;
                        }
                    }
                }
            }
            if raw.len() != frame_end {
                return Err(bad());
            }
            // Réalignement du flux de bits sur 16 bits en fin de trame.
            if bits.left > 0 {
                bits.ensure(16)?;
            }
            let drop = bits.left & 15;
            bits.remove(drop);
            let mut chunk = raw[frame_start..frame_end].to_vec();
            if intel_started && intel_size != 0 && frame < 32768 && frame_size > 10 {
                let mut curpos = out.len() as i32;
                let mut i = 0;
                while i < frame_size - 10 {
                    if chunk[i] != 0xE8 {
                        i += 1;
                        curpos += 1;
                        continue;
                    }
                    let p = i + 1;
                    let abs =
                        i32::from_le_bytes([chunk[p], chunk[p + 1], chunk[p + 2], chunk[p + 3]]);
                    if abs >= -curpos && abs < intel_size {
                        let rel = if abs >= 0 {
                            abs - curpos
                        } else {
                            abs + intel_size
                        };
                        chunk[p..p + 4].copy_from_slice(&rel.to_le_bytes());
                    }
                    i += 5;
                    curpos += 5;
                }
            }
            out.extend_from_slice(&chunk);
            frame += 1;
            // La fenêtre n'a besoin que des derniers `window` octets.
            if raw.len() > window * 4 {
                let cut = raw.len() - window;
                raw.drain(..cut);
            }
        }
        Ok(out)
    }
}
