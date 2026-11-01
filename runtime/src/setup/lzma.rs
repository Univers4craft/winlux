//! Décompresseurs LZMA et LZMA2 écrits de zéro, en flux, pour les
//! installateurs NSIS et Inno Setup. Le filtre x86 (BCJ) de NSIS est en bas.

use std::io::{self, Read};

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("lzma : {msg}"))
}

const NUM_STATES: usize = 12;
const POS_STATES_MAX: usize = 1 << 4;
const PROB_INIT: u16 = 1024;

/// Source d'octets compressés (lecture par petits blocs).
struct Input<R: Read> {
    inner: R,
    buf: Box<[u8; 65536]>,
    pos: usize,
    len: usize,
    eof: bool,
    /// Octets consommés depuis le début du flux.
    consumed: u64,
}

impl<R: Read> Input<R> {
    fn new(inner: R) -> Self {
        Input {
            inner,
            buf: Box::new([0; 65536]),
            pos: 0,
            len: 0,
            eof: false,
            consumed: 0,
        }
    }

    fn byte(&mut self) -> io::Result<u8> {
        if self.pos == self.len {
            if self.eof {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "lzma : flux tronqué",
                ));
            }
            self.len = loop {
                match self.inner.read(&mut self.buf[..]) {
                    Ok(n) => break n,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                }
            };
            self.pos = 0;
            if self.len == 0 {
                self.eof = true;
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "lzma : flux tronqué",
                ));
            }
        }
        let b = self.buf[self.pos];
        self.pos += 1;
        self.consumed += 1;
        Ok(b)
    }
}

/// Décodeur d'intervalle (range coder).
struct Range {
    range: u32,
    code: u32,
}

impl Range {
    fn init<R: Read>(input: &mut Input<R>) -> io::Result<Range> {
        if input.byte()? != 0 {
            return Err(bad("début de flux invalide"));
        }
        let mut code = 0u32;
        for _ in 0..4 {
            code = (code << 8) | input.byte()? as u32;
        }
        if code == u32::MAX {
            return Err(bad("début de flux invalide"));
        }
        Ok(Range {
            range: u32::MAX,
            code,
        })
    }

    #[inline]
    fn normalize<R: Read>(&mut self, input: &mut Input<R>) -> io::Result<()> {
        if self.range < (1 << 24) {
            self.range <<= 8;
            self.code = (self.code << 8) | input.byte()? as u32;
        }
        Ok(())
    }

    #[inline]
    fn bit<R: Read>(&mut self, prob: &mut u16, input: &mut Input<R>) -> io::Result<u32> {
        let bound = (self.range >> 11) * *prob as u32;
        let bit = if self.code < bound {
            self.range = bound;
            *prob += (2048 - *prob) >> 5;
            0
        } else {
            self.range -= bound;
            self.code -= bound;
            *prob -= *prob >> 5;
            1
        };
        self.normalize(input)?;
        Ok(bit)
    }

    fn direct<R: Read>(&mut self, count: u32, input: &mut Input<R>) -> io::Result<u32> {
        let mut res = 0u32;
        for _ in 0..count {
            self.range >>= 1;
            self.code = self.code.wrapping_sub(self.range);
            let t = 0u32.wrapping_sub(self.code >> 31);
            self.code = self.code.wrapping_add(self.range & t);
            if self.code == self.range {
                return Err(bad("données corrompues"));
            }
            self.normalize(input)?;
            res = (res << 1).wrapping_add(t.wrapping_add(1));
        }
        Ok(res)
    }

    fn tree<R: Read>(
        &mut self,
        probs: &mut [u16],
        bits: u32,
        input: &mut Input<R>,
    ) -> io::Result<u32> {
        let mut m = 1usize;
        for _ in 0..bits {
            m = (m << 1) + self.bit(&mut probs[m], input)? as usize;
        }
        Ok(m as u32 - (1 << bits))
    }

    fn tree_reverse<R: Read>(
        &mut self,
        probs: &mut [u16],
        bits: u32,
        input: &mut Input<R>,
    ) -> io::Result<u32> {
        let mut m = 1usize;
        let mut sym = 0u32;
        for i in 0..bits {
            let bit = self.bit(&mut probs[m], input)?;
            m = (m << 1) + bit as usize;
            sym |= bit << i;
        }
        Ok(sym)
    }
}

struct LenDecoder {
    choice: u16,
    choice2: u16,
    low: [[u16; 8]; POS_STATES_MAX],
    mid: [[u16; 8]; POS_STATES_MAX],
    high: [u16; 256],
}

impl LenDecoder {
    fn new() -> Self {
        LenDecoder {
            choice: PROB_INIT,
            choice2: PROB_INIT,
            low: [[PROB_INIT; 8]; POS_STATES_MAX],
            mid: [[PROB_INIT; 8]; POS_STATES_MAX],
            high: [PROB_INIT; 256],
        }
    }

    fn decode<R: Read>(
        &mut self,
        rc: &mut Range,
        pos_state: usize,
        input: &mut Input<R>,
    ) -> io::Result<u32> {
        if rc.bit(&mut self.choice, input)? == 0 {
            return rc.tree(&mut self.low[pos_state], 3, input);
        }
        if rc.bit(&mut self.choice2, input)? == 0 {
            return Ok(8 + rc.tree(&mut self.mid[pos_state], 3, input)?);
        }
        Ok(16 + rc.tree(&mut self.high, 8, input)?)
    }
}

/// Fenêtre glissante (dictionnaire).
struct Window {
    buf: Vec<u8>,
    pos: usize,
    full: bool,
    total: u64,
}

impl Window {
    fn new(size: u32) -> Window {
        Window {
            buf: vec![0; (size as usize).max(4096)],
            pos: 0,
            full: false,
            total: 0,
        }
    }

    fn reset(&mut self) {
        self.pos = 0;
        self.full = false;
        self.total = 0;
    }

    #[inline]
    fn put(&mut self, b: u8) {
        self.buf[self.pos] = b;
        self.pos += 1;
        self.total += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
            self.full = true;
        }
    }

    /// Octet situé `dist` positions en arrière (1 = le dernier écrit).
    #[inline]
    fn get(&self, dist: usize) -> u8 {
        let i = if dist <= self.pos {
            self.pos - dist
        } else {
            self.buf.len() + self.pos - dist
        };
        self.buf[i]
    }

    fn has(&self, dist: usize) -> bool {
        dist <= self.buf.len() && (self.full || dist <= self.pos)
    }
}

/// État du décodeur LZMA (conservé d'un bloc LZMA2 à l'autre).
struct Lzma {
    lc: u32,
    lp: u32,
    pb: u32,
    literal: Vec<u16>,
    pos_slot: [[u16; 64]; 4],
    pos_decoders: [u16; 115],
    align: [u16; 16],
    is_match: [u16; NUM_STATES << 4],
    is_rep: [u16; NUM_STATES],
    is_rep_g0: [u16; NUM_STATES],
    is_rep_g1: [u16; NUM_STATES],
    is_rep_g2: [u16; NUM_STATES],
    is_rep0_long: [u16; NUM_STATES << 4],
    len: LenDecoder,
    rep_len: LenDecoder,
    state: usize,
    reps: [u32; 4],
    /// Reste d'une copie interrompue faute de place en sortie.
    pending: u32,
    finished: bool,
}

impl Lzma {
    fn new(props: u8) -> io::Result<Lzma> {
        let mut me = Lzma {
            lc: 0,
            lp: 0,
            pb: 0,
            literal: Vec::new(),
            pos_slot: [[PROB_INIT; 64]; 4],
            pos_decoders: [PROB_INIT; 115],
            align: [PROB_INIT; 16],
            is_match: [PROB_INIT; NUM_STATES << 4],
            is_rep: [PROB_INIT; NUM_STATES],
            is_rep_g0: [PROB_INIT; NUM_STATES],
            is_rep_g1: [PROB_INIT; NUM_STATES],
            is_rep_g2: [PROB_INIT; NUM_STATES],
            is_rep0_long: [PROB_INIT; NUM_STATES << 4],
            len: LenDecoder::new(),
            rep_len: LenDecoder::new(),
            state: 0,
            reps: [0; 4],
            pending: 0,
            finished: false,
        };
        me.set_props(props)?;
        Ok(me)
    }

    fn set_props(&mut self, props: u8) -> io::Result<()> {
        if props >= 9 * 5 * 5 {
            return Err(bad("propriétés invalides"));
        }
        let p = props as u32;
        self.lc = p % 9;
        self.lp = (p / 9) % 5;
        self.pb = p / 45;
        self.reset_state();
        Ok(())
    }

    fn reset_state(&mut self) {
        self.literal = vec![PROB_INIT; 0x300 << (self.lc + self.lp)];
        self.pos_slot = [[PROB_INIT; 64]; 4];
        self.pos_decoders = [PROB_INIT; 115];
        self.align = [PROB_INIT; 16];
        self.is_match = [PROB_INIT; NUM_STATES << 4];
        self.is_rep = [PROB_INIT; NUM_STATES];
        self.is_rep_g0 = [PROB_INIT; NUM_STATES];
        self.is_rep_g1 = [PROB_INIT; NUM_STATES];
        self.is_rep_g2 = [PROB_INIT; NUM_STATES];
        self.is_rep0_long = [PROB_INIT; NUM_STATES << 4];
        self.len = LenDecoder::new();
        self.rep_len = LenDecoder::new();
        self.state = 0;
        self.reps = [0; 4];
        self.pending = 0;
        self.finished = false;
    }

    /// Décode jusqu'à `limit` octets dans `out` (et dans la fenêtre).
    fn decode<R: Read>(
        &mut self,
        rc: &mut Range,
        win: &mut Window,
        input: &mut Input<R>,
        out: &mut [u8],
    ) -> io::Result<usize> {
        let mut n = 0;
        while self.pending > 0 && n < out.len() {
            let b = win.get(self.reps[0] as usize + 1);
            win.put(b);
            out[n] = b;
            n += 1;
            self.pending -= 1;
        }
        let pb_mask = (1u64 << self.pb) - 1;
        let lp_mask = (1u64 << self.lp) - 1;
        while n < out.len() && !self.finished {
            let pos_state = (win.total & pb_mask) as usize;
            let st = self.state;
            if rc.bit(&mut self.is_match[(st << 4) + pos_state], input)? == 0 {
                let prev = if win.total > 0 { win.get(1) } else { 0 };
                let lit_state = (((win.total & lp_mask) as usize) << self.lc)
                    + (prev as usize >> (8 - self.lc));
                let probs = &mut self.literal[0x300 * lit_state..0x300 * (lit_state + 1)];
                let mut sym = 1usize;
                if st >= 7 {
                    let mut match_byte = win.get(self.reps[0] as usize + 1) as usize;
                    while sym < 0x100 {
                        let match_bit = (match_byte >> 7) & 1;
                        match_byte <<= 1;
                        let bit = rc.bit(&mut probs[((1 + match_bit) << 8) + sym], input)? as usize;
                        sym = (sym << 1) | bit;
                        if match_bit != bit {
                            break;
                        }
                    }
                }
                while sym < 0x100 {
                    sym = (sym << 1) | rc.bit(&mut probs[sym], input)? as usize;
                }
                let b = (sym - 0x100) as u8;
                win.put(b);
                out[n] = b;
                n += 1;
                self.state = if st < 4 {
                    0
                } else if st < 10 {
                    st - 3
                } else {
                    st - 6
                };
                continue;
            }
            let len;
            if rc.bit(&mut self.is_rep[st], input)? != 0 {
                if win.total == 0 {
                    return Err(bad("répétition sans données"));
                }
                if rc.bit(&mut self.is_rep_g0[st], input)? == 0 {
                    if rc.bit(&mut self.is_rep0_long[(st << 4) + pos_state], input)? == 0 {
                        self.state = if st < 7 { 9 } else { 11 };
                        let b = win.get(self.reps[0] as usize + 1);
                        win.put(b);
                        out[n] = b;
                        n += 1;
                        continue;
                    }
                } else {
                    let dist;
                    if rc.bit(&mut self.is_rep_g1[st], input)? == 0 {
                        dist = self.reps[1];
                    } else {
                        if rc.bit(&mut self.is_rep_g2[st], input)? == 0 {
                            dist = self.reps[2];
                        } else {
                            dist = self.reps[3];
                            self.reps[3] = self.reps[2];
                        }
                        self.reps[2] = self.reps[1];
                    }
                    self.reps[1] = self.reps[0];
                    self.reps[0] = dist;
                }
                len = self.rep_len.decode(rc, pos_state, input)?;
                self.state = if st < 7 { 8 } else { 11 };
            } else {
                self.reps[3] = self.reps[2];
                self.reps[2] = self.reps[1];
                self.reps[1] = self.reps[0];
                len = self.len.decode(rc, pos_state, input)?;
                self.state = if st < 7 { 7 } else { 10 };
                let dist = self.distance(rc, len, input)?;
                if dist == u32::MAX {
                    self.finished = true;
                    break;
                }
                self.reps[0] = dist;
            }
            let dist = self.reps[0] as usize + 1;
            if !win.has(dist) {
                return Err(bad("distance hors du dictionnaire"));
            }
            let mut left = len + 2;
            while left > 0 && n < out.len() {
                let b = win.get(dist);
                win.put(b);
                out[n] = b;
                n += 1;
                left -= 1;
            }
            self.pending = left;
        }
        Ok(n)
    }

    fn distance<R: Read>(
        &mut self,
        rc: &mut Range,
        len: u32,
        input: &mut Input<R>,
    ) -> io::Result<u32> {
        let len_state = len.min(3) as usize;
        let slot = rc.tree(&mut self.pos_slot[len_state], 6, input)?;
        if slot < 4 {
            return Ok(slot);
        }
        let direct = (slot >> 1) - 1;
        let mut dist = (2 | (slot & 1)) << direct;
        if slot < 14 {
            let base = (dist - slot) as usize;
            dist += rc.tree_reverse(&mut self.pos_decoders[base..], direct, input)?;
        } else {
            dist = dist.wrapping_add(rc.direct(direct - 4, input)? << 4);
            dist = dist.wrapping_add(rc.tree_reverse(&mut self.align, 4, input)?);
        }
        Ok(dist)
    }
}

/// Flux LZMA « brut » : 5 octets de propriétés puis les données, sans taille
/// (format d'Inno Setup et de NSIS).
pub struct LzmaReader<R: Read> {
    input: Input<R>,
    rc: Range,
    win: Window,
    lzma: Lzma,
}

impl<R: Read> LzmaReader<R> {
    pub fn new(inner: R) -> io::Result<Self> {
        let mut input = Input::new(inner);
        let props = input.byte()?;
        let mut dict = 0u32;
        for i in 0..4 {
            dict |= (input.byte()? as u32) << (8 * i);
        }
        let lzma = Lzma::new(props)?;
        // Les dictionnaires annoncés dépassent parfois la taille utile.
        let win = Window::new(dict.clamp(4096, 1 << 30));
        let rc = Range::init(&mut input)?;
        Ok(LzmaReader {
            input,
            rc,
            win,
            lzma,
        })
    }
}

impl<R: Read> Read for LzmaReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        self.lzma
            .decode(&mut self.rc, &mut self.win, &mut self.input, out)
    }
}

/// Flux LZMA2 (Inno Setup « lzma2 ») : un octet de dictionnaire puis des blocs.
pub struct Lzma2Reader<R: Read> {
    input: Input<R>,
    rc: Range,
    win: Window,
    lzma: Option<Lzma>,
    /// Octets restant à produire dans le bloc courant.
    chunk_left: u32,
    /// Bloc courant non compressé.
    stored: bool,
    /// Fin des données compressées du bloc courant (en octets consommés).
    chunk_end: u64,
    need_dict_reset: bool,
    done: bool,
}

impl<R: Read> Lzma2Reader<R> {
    pub fn new(inner: R) -> io::Result<Self> {
        let mut input = Input::new(inner);
        let p = input.byte()? as u32;
        if p > 40 {
            return Err(bad("dictionnaire LZMA2 invalide"));
        }
        let dict = if p == 40 {
            u32::MAX
        } else {
            (2 | (p & 1)) << (p / 2 + 11)
        };
        Ok(Lzma2Reader {
            input,
            rc: Range { range: 0, code: 0 },
            win: Window::new(dict.clamp(4096, 1 << 30)),
            lzma: None,
            chunk_left: 0,
            stored: false,
            chunk_end: 0,
            need_dict_reset: true,
            done: false,
        })
    }

    fn next_chunk(&mut self) -> io::Result<()> {
        let control = self.input.byte()?;
        if control == 0 {
            self.done = true;
            return Ok(());
        }
        let be16 = |inp: &mut Input<R>| -> io::Result<u32> {
            Ok(((inp.byte()? as u32) << 8) | inp.byte()? as u32)
        };
        if control == 1 || control == 2 {
            if control == 1 {
                self.win.reset();
                self.need_dict_reset = false;
            } else if self.need_dict_reset {
                return Err(bad("LZMA2 sans dictionnaire initial"));
            }
            self.chunk_left = be16(&mut self.input)? + 1;
            self.stored = true;
            return Ok(());
        }
        if control < 0x80 {
            return Err(bad("bloc LZMA2 invalide"));
        }
        let unpacked = (((control as u32) & 0x1F) << 16) + be16(&mut self.input)? + 1;
        let packed = be16(&mut self.input)? + 1;
        let mode = (control >> 5) & 3;
        if mode == 3 {
            self.win.reset();
            self.need_dict_reset = false;
        } else if self.need_dict_reset {
            return Err(bad("LZMA2 sans dictionnaire initial"));
        }
        if mode >= 2 {
            let props = self.input.byte()?;
            match &mut self.lzma {
                Some(l) => l.set_props(props)?,
                None => self.lzma = Some(Lzma::new(props)?),
            }
        } else if mode == 1 {
            match &mut self.lzma {
                Some(l) => l.reset_state(),
                None => return Err(bad("LZMA2 sans propriétés")),
            }
        } else if self.lzma.is_none() {
            return Err(bad("LZMA2 sans propriétés"));
        }
        let start = self.input.consumed;
        self.rc = Range::init(&mut self.input)?;
        self.chunk_end = start + packed as u64;
        self.chunk_left = unpacked;
        self.stored = false;
        Ok(())
    }
}

impl<R: Read> Read for Lzma2Reader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        while self.chunk_left == 0 {
            if self.done {
                return Ok(0);
            }
            self.next_chunk()?;
        }
        let want = out.len().min(self.chunk_left as usize);
        let n = if self.stored {
            for slot in out.iter_mut().take(want) {
                let b = self.input.byte()?;
                self.win.put(b);
                *slot = b;
            }
            want
        } else {
            let lzma = self.lzma.as_mut().ok_or_else(|| bad("état LZMA2"))?;
            let n = lzma.decode(
                &mut self.rc,
                &mut self.win,
                &mut self.input,
                &mut out[..want],
            )?;
            if n == 0 {
                return Err(bad("bloc LZMA2 trop court"));
            }
            n
        };
        self.chunk_left -= n as u32;
        if self.chunk_left == 0 && !self.stored {
            // Le décodeur doit avoir consommé exactement le bloc compressé.
            while self.input.consumed < self.chunk_end {
                self.input.byte()?;
            }
            if let Some(l) = &mut self.lzma {
                l.pending = 0;
            }
        }
        Ok(n)
    }
}

/// Filtre x86 BCJ (décodage) utilisé par NSIS devant LZMA : les adresses
/// absolues des CALL/JMP redeviennent relatives. `state` garde le contexte
/// entre deux appels, `pos` est la position du tampon dans le flux.
pub struct Bcj {
    pos: u32,
    prev_mask: u32,
    /// Octets reçus mais pas encore filtrés (instruction coupée).
    carry: Vec<u8>,
}

impl Bcj {
    pub fn new() -> Bcj {
        Bcj {
            pos: 5,
            prev_mask: 0,
            carry: Vec::new(),
        }
    }

    /// Filtre `data` sur place et renvoie le nombre d'octets définitifs
    /// (les derniers peuvent attendre la suite).
    fn convert(&mut self, data: &mut [u8]) -> usize {
        const MASK_TO_ALLOWED: [bool; 8] = [true, true, true, false, true, false, false, false];
        const MASK_TO_BIT: [u32; 8] = [0, 1, 2, 2, 3, 3, 3, 3];
        let size = data.len();
        if size < 5 {
            return 0;
        }
        let mut i = 0usize;
        let mut prev_pos = 0usize.wrapping_sub(5);
        let limit = size - 5;
        while i <= limit {
            let b = data[i];
            if b != 0xE8 && b != 0xE9 {
                i += 1;
                continue;
            }
            let offset = i.wrapping_sub(prev_pos);
            prev_pos = i;
            if offset > 5 {
                self.prev_mask = 0;
            } else {
                for _ in 0..offset {
                    self.prev_mask &= 0x77;
                    self.prev_mask <<= 1;
                }
            }
            let mut b4 = data[i + 4];
            if (b4 == 0 || b4 == 0xFF)
                && MASK_TO_ALLOWED[((self.prev_mask >> 1) & 7) as usize]
                && (self.prev_mask >> 1) < 0x10
            {
                let mut src = u32::from_le_bytes([data[i + 1], data[i + 2], data[i + 3], b4]);
                let mut dest;
                loop {
                    dest = src.wrapping_sub(self.pos.wrapping_add(i as u32));
                    if self.prev_mask == 0 {
                        break;
                    }
                    let idx = MASK_TO_BIT[(self.prev_mask >> 1) as usize] * 8;
                    let bb = (dest >> (24 - idx)) as u8;
                    if !(bb == 0 || bb == 0xFF) {
                        break;
                    }
                    src = dest ^ ((1u32 << (32 - idx)) - 1);
                }
                b4 = !(((dest >> 24) & 1) as u8).wrapping_sub(1);
                let d = dest.to_le_bytes();
                data[i + 1] = d[0];
                data[i + 2] = d[1];
                data[i + 3] = d[2];
                data[i + 4] = b4;
                i += 5;
                self.prev_mask = 0;
            } else {
                i += 1;
                self.prev_mask |= 1;
                if b4 == 0 || b4 == 0xFF {
                    self.prev_mask |= 0x10;
                }
            }
        }
        let offset = i.wrapping_sub(prev_pos);
        if offset > 5 {
            self.prev_mask = 0;
        } else {
            for _ in 0..offset {
                self.prev_mask &= 0x77;
                self.prev_mask <<= 1;
            }
        }
        self.pos = self.pos.wrapping_add(i as u32);
        i
    }
}

/// Lecteur appliquant le filtre BCJ x86 à un flux décompressé.
pub struct BcjReader<R: Read> {
    inner: R,
    bcj: Bcj,
    /// Données filtrées prêtes à rendre.
    ready: Vec<u8>,
    ready_pos: usize,
    eof: bool,
}

impl<R: Read> BcjReader<R> {
    pub fn new(inner: R) -> Self {
        BcjReader {
            inner,
            bcj: Bcj::new(),
            ready: Vec::new(),
            ready_pos: 0,
            eof: false,
        }
    }
}

impl<R: Read> Read for BcjReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.ready_pos == self.ready.len() {
            if self.eof && self.bcj.carry.is_empty() {
                return Ok(0);
            }
            let mut buf = std::mem::take(&mut self.bcj.carry);
            let start = buf.len();
            buf.resize(start + 65536, 0);
            let mut got = 0;
            if !self.eof {
                got = self.inner.read(&mut buf[start..])?;
                if got == 0 {
                    self.eof = true;
                }
            }
            buf.truncate(start + got);
            let done = if self.eof {
                buf.len()
            } else {
                self.bcj.convert(&mut buf)
            };
            // En fin de flux, les derniers octets passent sans conversion.
            self.bcj.carry = buf.split_off(done);
            if self.eof {
                let tail = std::mem::take(&mut self.bcj.carry);
                buf.extend(tail);
            }
            self.ready = buf;
            self.ready_pos = 0;
        }
        let n = out.len().min(self.ready.len() - self.ready_pos);
        out[..n].copy_from_slice(&self.ready[self.ready_pos..self.ready_pos + n]);
        self.ready_pos += n;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `printf 'hello hello hello hello\n' | xz --format=lzma` puis on retire
    /// la taille (8 octets après les propriétés) : flux « brut » 5+données.
    #[test]
    fn lzma_raw_stream() {
        let data: [u8; 24] = [
            0x5d, 0x00, 0x00, 0x80, 0x00, 0x00, 0x34, 0x19, 0x49, 0xee, 0x8d, 0xe9, 0x56, 0x0a,
            0xc1, 0xb6, 0x20, 0xb7, 0xff, 0xff, 0xba, 0x34, 0x00, 0x00,
        ];
        let mut out = vec![0u8; 24];
        let mut r = LzmaReader::new(&data[..]).unwrap();
        r.read_exact(&mut out).unwrap();
        assert_eq!(&out, b"hello hello hello hello\n");
    }

    /// Données pseudo-x86 (avec des CALL) identiques à celles des vecteurs.
    fn x86_sample() -> Vec<u8> {
        let mut out = Vec::new();
        let mut x: u32 = 12345;
        while out.len() < 4000 {
            x = (x.wrapping_mul(1103515245).wrapping_add(12345)) & 0x7fff_ffff;
            let r = (x >> 16) & 0xff;
            if r < 40 {
                out.push(0xE8);
                let rel = ((x >> 3) & 0xfff) as i32 - 0x800;
                out.extend_from_slice(&rel.to_le_bytes());
            } else {
                out.push((r & 0x1f) as u8);
            }
        }
        out.truncate(4000);
        out
    }

    #[test]
    fn lzma_vector() {
        let data = include_bytes!("testdata/x86.lzma");
        let mut out = vec![0u8; 4000];
        LzmaReader::new(&data[..])
            .unwrap()
            .read_exact(&mut out)
            .unwrap();
        assert_eq!(out, x86_sample());
    }

    #[test]
    fn lzma2_vector() {
        let mut data = vec![8u8]; // dictionnaire de 64 Kio
        data.extend_from_slice(include_bytes!("testdata/x86.lzma2"));
        let mut out = Vec::new();
        Lzma2Reader::new(&data[..])
            .unwrap()
            .read_to_end(&mut out)
            .unwrap();
        assert_eq!(out, x86_sample());
    }

    #[test]
    fn bcj_x86_vector() {
        let mut data = vec![8u8];
        data.extend_from_slice(include_bytes!("testdata/x86.bcj.lzma2"));
        let mut out = Vec::new();
        BcjReader::new(Lzma2Reader::new(&data[..]).unwrap())
            .read_to_end(&mut out)
            .unwrap();
        assert_eq!(out, x86_sample());
    }
}
