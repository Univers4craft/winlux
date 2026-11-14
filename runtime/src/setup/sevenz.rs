//! Archives 7z, lues par Winlux lui-même, et installateurs « 7z
//! auto-extractibles » (celui de 7-Zip, et ceux faits avec le module SFX
//! de 7-Zip) : un petit lanceur Windows 32 bits suivi d'une archive 7z.
//! Winlux n'exécute pas le lanceur, il lit l'archive et installe son
//! contenu.
//!
//! Format suivi : description publique du format 7z (7zFormat.txt du SDK
//! LZMA, domaine public). Méthodes : copie, LZMA, LZMA2 (décodeurs de
//! `lzma.rs`), filtres x86 BCJ et BCJ2. Les archives chiffrées ne sont
//! pas prises en charge.

use super::lzma::{Lzma2Reader, LzmaReader};
use super::{Installer, Options};
use std::io::Read;
use std::path::Path;

const SIGNATURE: &[u8; 6] = b"7z\xBC\xAF\x27\x1C";

fn bad(msg: &str) -> String {
    format!("archive 7z : {msg}")
}

/// Lecteur des en-têtes (nombres 7z à longueur variable).
struct Cur<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cur<'a> {
    fn byte(&mut self) -> Result<u8, String> {
        let v = *self.b.get(self.i).ok_or_else(|| bad("en-tête tronqué"))?;
        self.i += 1;
        Ok(v)
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.i.checked_add(n).ok_or_else(|| bad("taille"))?;
        let s = self
            .b
            .get(self.i..end)
            .ok_or_else(|| bad("en-tête tronqué"))?;
        self.i = end;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    /// Nombre 7z : les bits forts du premier octet disent combien d'octets suivent.
    fn num(&mut self) -> Result<u64, String> {
        let first = self.byte()?;
        let mut mask = 0x80u8;
        let mut val = 0u64;
        for k in 0..8 {
            if first & mask == 0 {
                let high = (first & mask.wrapping_sub(1)) as u64;
                return Ok(val | (high << (8 * k)));
            }
            val |= (self.byte()? as u64) << (8 * k);
            mask >>= 1;
        }
        Ok(val)
    }
    fn small(&mut self) -> Result<usize, String> {
        let v = self.num()?;
        if v > 1 << 24 {
            return Err(bad("nombre trop grand"));
        }
        Ok(v as usize)
    }
    fn bits(&mut self, n: usize) -> Result<Vec<bool>, String> {
        let mut out = Vec::with_capacity(n);
        let mut cur = 0u8;
        for i in 0..n {
            if i % 8 == 0 {
                cur = self.byte()?;
            }
            out.push(cur & (0x80 >> (i % 8)) != 0);
        }
        Ok(out)
    }
    /// Bits « tous définis » : un octet 1, sinon un champ de bits.
    fn defined(&mut self, n: usize) -> Result<Vec<bool>, String> {
        if self.byte()? != 0 {
            Ok(vec![true; n])
        } else {
            self.bits(n)
        }
    }
    fn digests(&mut self, n: usize) -> Result<Vec<Option<u32>>, String> {
        let def = self.defined(n)?;
        def.into_iter()
            .map(|d| if d { self.u32().map(Some) } else { Ok(None) })
            .collect()
    }
}

#[derive(Clone, Debug)]
struct Coder {
    id: Vec<u8>,
    ins: usize,
    outs: usize,
    props: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
struct Folder {
    coders: Vec<Coder>,
    /// (flux d'entrée, flux de sortie) reliés.
    binds: Vec<(usize, usize)>,
    /// Flux d'entrée alimentés par un flux compressé, dans l'ordre.
    packed: Vec<usize>,
    unpack_sizes: Vec<u64>,
    crc: Option<u32>,
}

impl Folder {
    fn main_out(&self) -> Option<usize> {
        let total: usize = self.coders.iter().map(|c| c.outs).sum();
        (0..total).find(|o| !self.binds.iter().any(|b| b.1 == *o))
    }
    fn size(&self) -> u64 {
        self.main_out()
            .and_then(|o| self.unpack_sizes.get(o).copied())
            .unwrap_or(0)
    }
}

#[derive(Default, Debug)]
struct Streams {
    pack_pos: u64,
    pack_sizes: Vec<u64>,
    folders: Vec<Folder>,
    /// Par dossier : tailles de ses flux (fichiers) décompressés.
    sub_sizes: Vec<Vec<u64>>,
    sub_crcs: Vec<Option<u32>>,
}

fn read_folder(c: &mut Cur) -> Result<Folder, String> {
    let n = c.small()?;
    if n == 0 || n > 64 {
        return Err(bad("nombre de codeurs"));
    }
    let mut f = Folder::default();
    for _ in 0..n {
        let flags = c.byte()?;
        let id = c.take((flags & 0x0F) as usize)?.to_vec();
        let (ins, outs) = if flags & 0x10 != 0 {
            (c.small()?, c.small()?)
        } else {
            (1, 1)
        };
        let props = if flags & 0x20 != 0 {
            let len = c.small()?;
            c.take(len)?.to_vec()
        } else {
            Vec::new()
        };
        if flags & 0x80 != 0 {
            return Err(bad("méthodes alternatives non prises en charge"));
        }
        f.coders.push(Coder {
            id,
            ins,
            outs,
            props,
        });
    }
    let total_out: usize = f.coders.iter().map(|c| c.outs).sum();
    let total_in: usize = f.coders.iter().map(|c| c.ins).sum();
    for _ in 0..total_out.saturating_sub(1) {
        let i = c.small()?;
        let o = c.small()?;
        f.binds.push((i, o));
    }
    let n_packed = total_in
        .checked_sub(f.binds.len())
        .ok_or_else(|| bad("liaisons"))?;
    if n_packed == 1 {
        let free = (0..total_in)
            .find(|i| !f.binds.iter().any(|b| b.0 == *i))
            .ok_or_else(|| bad("flux compressé"))?;
        f.packed.push(free);
    } else {
        for _ in 0..n_packed {
            f.packed.push(c.small()?);
        }
    }
    Ok(f)
}

fn read_streams(c: &mut Cur) -> Result<Streams, String> {
    let mut s = Streams::default();
    loop {
        match c.byte()? {
            0x00 => break,
            0x06 => {
                s.pack_pos = c.num()?;
                let n = c.small()?;
                loop {
                    match c.byte()? {
                        0x00 => break,
                        0x09 => {
                            s.pack_sizes = (0..n).map(|_| c.num()).collect::<Result<_, _>>()?;
                        }
                        0x0A => {
                            c.digests(n)?;
                        }
                        _ => return Err(bad("PackInfo")),
                    }
                }
            }
            0x07 => {
                if c.byte()? != 0x0B {
                    return Err(bad("UnPackInfo"));
                }
                let n = c.small()?;
                if c.byte()? != 0 {
                    return Err(bad("dossiers externes"));
                }
                for _ in 0..n {
                    s.folders.push(read_folder(c)?);
                }
                if c.byte()? != 0x0C {
                    return Err(bad("tailles décompressées"));
                }
                for f in &mut s.folders {
                    let outs: usize = f.coders.iter().map(|c| c.outs).sum();
                    f.unpack_sizes = (0..outs).map(|_| c.num()).collect::<Result<_, _>>()?;
                }
                loop {
                    match c.byte()? {
                        0x00 => break,
                        0x0A => {
                            let d = c.digests(n)?;
                            for (f, crc) in s.folders.iter_mut().zip(d) {
                                f.crc = crc;
                            }
                        }
                        _ => return Err(bad("UnPackInfo")),
                    }
                }
            }
            0x08 => {
                let mut counts = vec![1usize; s.folders.len()];
                let mut t = c.byte()?;
                if t == 0x0D {
                    for v in &mut counts {
                        *v = c.small()?;
                    }
                    t = c.byte()?;
                }
                let mut sizes = Vec::new();
                for (k, f) in s.folders.iter().enumerate() {
                    let n = counts[k];
                    if n == 0 {
                        sizes.push(Vec::new());
                        continue;
                    }
                    let mut v = Vec::new();
                    let mut sum = 0u64;
                    if t == 0x09 {
                        for _ in 1..n {
                            let x = c.num()?;
                            sum += x;
                            v.push(x);
                        }
                    }
                    v.push(f.size().saturating_sub(sum));
                    sizes.push(v);
                }
                if t == 0x09 {
                    t = c.byte()?;
                }
                // CRC des flux dont le dossier n'en donne pas.
                let mut unknown = 0;
                for (k, f) in s.folders.iter().enumerate() {
                    if !(counts[k] == 1 && f.crc.is_some()) {
                        unknown += counts[k];
                    }
                }
                let mut extra = Vec::new();
                while t != 0x00 {
                    if t == 0x0A {
                        extra = c.digests(unknown)?;
                    } else {
                        return Err(bad("SubStreamsInfo"));
                    }
                    t = c.byte()?;
                }
                let mut it = extra.into_iter();
                for (k, f) in s.folders.iter().enumerate() {
                    if counts[k] == 1 && f.crc.is_some() {
                        s.sub_crcs.push(f.crc);
                    } else {
                        for _ in 0..counts[k] {
                            s.sub_crcs.push(it.next().flatten());
                        }
                    }
                }
                s.sub_sizes = sizes;
            }
            _ => return Err(bad("StreamsInfo")),
        }
    }
    if s.sub_sizes.is_empty() {
        s.sub_sizes = s.folders.iter().map(|f| vec![f.size()]).collect();
        s.sub_crcs = s.folders.iter().map(|f| f.crc).collect();
    }
    Ok(s)
}

// ── Décodeurs ───────────────────────────────────────────────────────────

/// Filtre x86 BCJ (décodage), position de départ 0.
pub fn bcj_x86_decode(data: &mut [u8]) {
    const ALLOWED: [bool; 8] = [true, true, true, false, true, false, false, false];
    const BIT: [u32; 8] = [0, 1, 2, 2, 3, 3, 3, 3];
    let ms = |b: u8| b == 0 || b == 0xFF;
    if data.len() < 5 {
        return;
    }
    let ip: u32 = 5;
    let mut prev_mask: u32 = 0;
    let mut prev_pos: usize = usize::MAX;
    let mut pos = 0usize;
    let limit = data.len() - 4;
    while pos < limit {
        if data[pos] & 0xFE != 0xE8 {
            pos += 1;
            continue;
        }
        let d = pos.wrapping_sub(prev_pos);
        if d > 3 {
            prev_mask = 0;
        } else {
            prev_mask = (prev_mask << (d as u32 - 1)) & 7;
            if prev_mask != 0 {
                let b = data[pos + 4 - BIT[prev_mask as usize] as usize];
                if !ALLOWED[prev_mask as usize] || ms(b) {
                    prev_pos = pos;
                    prev_mask = ((prev_mask << 1) & 7) | 1;
                    pos += 1;
                    continue;
                }
            }
        }
        prev_pos = pos;
        if ms(data[pos + 4]) {
            let mut src =
                u32::from_le_bytes([data[pos + 1], data[pos + 2], data[pos + 3], data[pos + 4]]);
            let mut dest;
            loop {
                dest = src.wrapping_sub(ip.wrapping_add(pos as u32));
                if prev_mask == 0 {
                    break;
                }
                let index = BIT[prev_mask as usize] * 8;
                let b = (dest >> (24 - index)) as u8;
                if !ms(b) {
                    break;
                }
                src = dest ^ ((1u32 << (32 - index)) - 1);
            }
            data[pos + 4] = !((((dest >> 24) & 1) as u8).wrapping_sub(1));
            data[pos + 3] = (dest >> 16) as u8;
            data[pos + 2] = (dest >> 8) as u8;
            data[pos + 1] = dest as u8;
            pos += 5;
        } else {
            prev_mask = ((prev_mask << 1) & 7) | 1;
            pos += 1;
        }
    }
}

/// Filtre BCJ2 (décodage) : flux principal, appels (E8), sauts (E9, Jcc)
/// et flux de décision codé par intervalles.
pub fn bcj2_decode(
    main: &[u8],
    call: &[u8],
    jump: &[u8],
    rc: &[u8],
    out_size: usize,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(out_size);
    let mut probs = [1024u16; 258];
    let mut ri = 0usize;
    let next = |ri: &mut usize| -> u8 {
        let b = rc.get(*ri).copied().unwrap_or(0);
        *ri += 1;
        b
    };
    let mut code: u32 = 0;
    let mut range: u32 = 0xFFFF_FFFF;
    for _ in 0..5 {
        code = (code << 8) | next(&mut ri) as u32;
    }
    let (mut mi, mut ci, mut ji) = (0usize, 0usize, 0usize);
    let mut prev: u8 = 0;
    let is_j = |b0: u8, b1: u8| (b1 & 0xFE) == 0xE8 || (b0 == 0x0F && (b1 & 0xF0) == 0x80);
    while out.len() < out_size {
        // Octets ordinaires jusqu'à un saut possible.
        let mut b;
        loop {
            if out.len() >= out_size {
                return Ok(out);
            }
            b = *main
                .get(mi)
                .ok_or_else(|| bad("BCJ2 : flux principal tronqué"))?;
            mi += 1;
            out.push(b);
            if is_j(prev, b) {
                break;
            }
            prev = b;
        }
        if out.len() >= out_size {
            break;
        }
        let p = match b {
            0xE8 => prev as usize,
            0xE9 => 256,
            _ => 257,
        };
        let bound = (range >> 11) * probs[p] as u32;
        let bit = if code < bound {
            range = bound;
            probs[p] += (2048 - probs[p]) >> 5;
            false
        } else {
            range -= bound;
            code -= bound;
            probs[p] -= probs[p] >> 5;
            true
        };
        if range < 1 << 24 {
            range <<= 8;
            code = (code << 8) | next(&mut ri) as u32;
        }
        if !bit {
            prev = b;
            continue;
        }
        let (src, at) = if b == 0xE8 {
            (call, &mut ci)
        } else {
            (jump, &mut ji)
        };
        let four = src
            .get(*at..*at + 4)
            .ok_or_else(|| bad("BCJ2 : flux d'adresses tronqué"))?;
        *at += 4;
        let v = u32::from_be_bytes(four.try_into().unwrap());
        let dest = v.wrapping_sub(out.len() as u32 + 4);
        for k in 0..4 {
            if out.len() < out_size {
                out.push((dest >> (8 * k)) as u8);
            }
        }
        prev = (dest >> 24) as u8;
    }
    Ok(out)
}

fn read_exact_n(mut r: impl Read, n: u64) -> Result<Vec<u8>, String> {
    let mut v = Vec::with_capacity(n.min(1 << 30) as usize);
    r.by_ref()
        .take(n)
        .read_to_end(&mut v)
        .map_err(|e| bad(&e.to_string()))?;
    if (v.len() as u64) < n {
        return Err(bad("données tronquées"));
    }
    Ok(v)
}

fn run_coder(c: &Coder, inputs: Vec<Vec<u8>>, size: u64) -> Result<Vec<u8>, String> {
    if size > 4 << 30 {
        return Err(bad("flux trop grand"));
    }
    let first = || inputs.first().cloned().unwrap_or_default();
    match c.id.as_slice() {
        [0x00] => Ok(first()),
        [0x03, 0x01, 0x01] => {
            if c.props.len() < 5 {
                return Err(bad("propriétés LZMA"));
            }
            let data = first();
            let src = std::io::Cursor::new(c.props[..5].to_vec()).chain(std::io::Cursor::new(data));
            read_exact_n(LzmaReader::new(src).map_err(|e| bad(&e.to_string()))?, size)
        }
        [0x21] => {
            let p = *c.props.first().ok_or_else(|| bad("propriétés LZMA2"))?;
            let src = std::io::Cursor::new(vec![p]).chain(std::io::Cursor::new(first()));
            read_exact_n(
                Lzma2Reader::new(src).map_err(|e| bad(&e.to_string()))?,
                size,
            )
        }
        [0x03, 0x03, 0x01, 0x03] | [0x04] if c.ins == 1 => {
            let mut d = first();
            bcj_x86_decode(&mut d);
            Ok(d)
        }
        [0x03, 0x03, 0x01, 0x1B] if inputs.len() == 4 => bcj2_decode(
            &inputs[0],
            &inputs[1],
            &inputs[2],
            &inputs[3],
            size as usize,
        ),
        [0x06, 0xF1, 0x07, 0x01] => Err(bad("archive chiffrée (AES) : non prise en charge")),
        id => Err(bad(&format!("méthode {id:02X?} non prise en charge"))),
    }
}

/// Décompresse un dossier (graphe de codeurs) en mémoire.
fn decode_folder(f: &Folder, packs: &[&[u8]]) -> Result<Vec<u8>, String> {
    // Premier flux d'entrée et de sortie de chaque codeur.
    let mut in_base = Vec::new();
    let mut out_base = Vec::new();
    let (mut i, mut o) = (0, 0);
    for c in &f.coders {
        in_base.push(i);
        out_base.push(o);
        i += c.ins;
        o += c.outs;
    }
    fn out_stream(
        f: &Folder,
        packs: &[&[u8]],
        in_base: &[usize],
        out_base: &[usize],
        out: usize,
        depth: u32,
    ) -> Result<Vec<u8>, String> {
        if depth > 32 {
            return Err(bad("graphe de codeurs"));
        }
        let k = (0..f.coders.len())
            .rev()
            .find(|&k| out_base[k] <= out)
            .ok_or_else(|| bad("flux de sortie"))?;
        let c = &f.coders[k];
        if c.outs != 1 {
            return Err(bad("codeur à plusieurs sorties"));
        }
        let mut inputs = Vec::new();
        for j in 0..c.ins {
            let idx = in_base[k] + j;
            if let Some(&(_, src)) = f.binds.iter().find(|b| b.0 == idx) {
                inputs.push(out_stream(f, packs, in_base, out_base, src, depth + 1)?);
            } else {
                let p = f
                    .packed
                    .iter()
                    .position(|&x| x == idx)
                    .ok_or_else(|| bad("flux compressé manquant"))?;
                inputs.push(packs.get(p).ok_or_else(|| bad("flux compressé"))?.to_vec());
            }
        }
        let size = *f.unpack_sizes.get(out).ok_or_else(|| bad("taille"))?;
        run_coder(c, inputs, size)
    }
    let main = f.main_out().ok_or_else(|| bad("sortie principale"))?;
    let data = out_stream(f, packs, &in_base, &out_base, main, 0)?;
    if let Some(crc) = f.crc {
        if crate::archive::crc32::crc32(&data) != crc {
            return Err(bad("somme de contrôle du dossier"));
        }
    }
    Ok(data)
}

/// Les flux compressés de chaque dossier, découpés dans l'archive.
fn folder_packs<'a>(archive: &'a [u8], s: &Streams) -> Result<Vec<Vec<&'a [u8]>>, String> {
    let mut pos = 32u64 + s.pack_pos;
    let mut sizes = s.pack_sizes.iter();
    let mut out = Vec::new();
    for f in &s.folders {
        let mut v = Vec::new();
        for _ in 0..f.packed.len() {
            let n = *sizes.next().ok_or_else(|| bad("tailles compressées"))?;
            let end = pos.checked_add(n).ok_or_else(|| bad("taille"))?;
            v.push(
                archive
                    .get(pos as usize..end as usize)
                    .ok_or_else(|| bad("archive tronquée"))?,
            );
            pos = end;
        }
        out.push(v);
    }
    Ok(out)
}

/// Une entrée de l'archive.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Chemin relatif, séparateurs « \ ».
    pub name: String,
    pub dir: bool,
    pub data: Vec<u8>,
}

/// Lit toute l'archive commençant à `archive[0]` (signature 7z).
pub fn read_all(archive: &[u8]) -> Result<Vec<Entry>, String> {
    if archive.len() < 32 || &archive[..6] != SIGNATURE {
        return Err(bad("signature absente"));
    }
    let mut c = Cur { b: archive, i: 8 };
    let start_crc = c.u32()?;
    if crate::archive::crc32::crc32(&archive[12..32]) != start_crc {
        return Err(bad("en-tête de départ abîmé"));
    }
    let off = c.u64()?;
    let len = c.u64()?;
    let crc = c.u32()?;
    let at = 32u64.checked_add(off).ok_or_else(|| bad("position"))?;
    let end = at.checked_add(len).ok_or_else(|| bad("taille"))?;
    let mut header = archive
        .get(at as usize..end as usize)
        .ok_or_else(|| bad("archive tronquée"))?
        .to_vec();
    if crate::archive::crc32::crc32(&header) != crc {
        return Err(bad("en-tête abîmé"));
    }
    // En-tête compressé : il décrit un dossier qui contient le vrai.
    for _ in 0..4 {
        if header.first() != Some(&0x17) {
            break;
        }
        let mut c = Cur { b: &header, i: 1 };
        let s = read_streams(&mut c)?;
        let packs = folder_packs(archive, &s)?;
        let f = s
            .folders
            .first()
            .ok_or_else(|| bad("en-tête compressé vide"))?;
        header = decode_folder(f, &packs[0])?;
    }
    let mut c = Cur { b: &header, i: 0 };
    if c.byte()? != 0x01 {
        return Err(bad("en-tête inconnu"));
    }
    let mut streams = Streams::default();
    let mut names: Vec<String> = Vec::new();
    let mut empty_stream: Vec<bool> = Vec::new();
    let mut empty_file: Vec<bool> = Vec::new();
    let mut attrs: Vec<Option<u32>> = Vec::new();
    loop {
        match c.byte()? {
            0x00 => break,
            0x02 => {
                // Propriétés de l'archive : ignorées.
                loop {
                    if c.byte()? == 0 {
                        break;
                    }
                    let n = c.small()?;
                    c.take(n)?;
                }
            }
            0x03 => {
                read_streams(&mut c)?;
            }
            0x04 => streams = read_streams(&mut c)?,
            0x05 => {
                let n = c.small()?;
                loop {
                    let t = c.byte()?;
                    if t == 0 {
                        break;
                    }
                    let size = c.small()?;
                    let body = c.take(size)?;
                    let mut p = Cur { b: body, i: 0 };
                    match t {
                        0x0E => empty_stream = p.bits(n)?,
                        0x0F => {
                            let k = empty_stream.iter().filter(|&&e| e).count();
                            empty_file = p.bits(k)?;
                        }
                        0x11 => {
                            if p.byte()? != 0 {
                                return Err(bad("noms externes"));
                            }
                            let words: Vec<u16> = body[1..]
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .map(|w| u16::from_le_bytes(*w))
                                .collect();
                            names = words
                                .split(|&w| w == 0)
                                .take(n)
                                .map(String::from_utf16_lossy)
                                .collect();
                        }
                        0x15 => {
                            let def = p.defined(n)?;
                            if p.byte()? != 0 {
                                return Err(bad("attributs externes"));
                            }
                            attrs = def
                                .into_iter()
                                .map(|d| if d { p.u32().map(Some) } else { Ok(None) })
                                .collect::<Result<_, _>>()?;
                        }
                        _ => {} // dates, factice… : sans effet ici
                    }
                }
                while names.len() < n {
                    names.push(format!("fichier{}", names.len()));
                }
            }
            _ => return Err(bad("en-tête")),
        }
    }
    let packs = folder_packs(archive, &streams)?;
    // Flux décompressés, dossier par dossier, découpés en fichiers.
    let mut files_data: Vec<Vec<u8>> = Vec::new();
    for (k, f) in streams.folders.iter().enumerate() {
        let data = decode_folder(f, &packs[k])?;
        let mut pos = 0usize;
        for &n in &streams.sub_sizes[k] {
            let end = pos + n as usize;
            files_data.push(data.get(pos..end).ok_or_else(|| bad("flux"))?.to_vec());
            pos = end;
        }
    }
    for (i, d) in files_data.iter().enumerate() {
        if let Some(Some(crc)) = streams.sub_crcs.get(i) {
            if crate::archive::crc32::crc32(d) != *crc {
                return Err(bad("somme de contrôle d'un fichier"));
            }
        }
    }
    let mut out = Vec::new();
    let mut data_it = files_data.into_iter();
    let mut empty_idx = 0;
    for (i, name) in names.into_iter().enumerate() {
        let empty = empty_stream.get(i).copied().unwrap_or(false);
        let name = name.replace('/', "\\");
        if name.split('\\').any(|p| p == "..") || name.starts_with('\\') || name.contains(':') {
            return Err(bad(&format!("chemin refusé : {name}")));
        }
        if empty {
            let is_file = empty_file.get(empty_idx).copied().unwrap_or(false);
            empty_idx += 1;
            let dir_attr = attrs
                .get(i)
                .copied()
                .flatten()
                .is_some_and(|a| a & 0x10 != 0);
            out.push(Entry {
                name,
                dir: !is_file || dir_attr,
                data: Vec::new(),
            });
        } else {
            out.push(Entry {
                name,
                dir: false,
                data: data_it.next().ok_or_else(|| bad("flux manquant"))?,
            });
        }
    }
    Ok(out)
}

/// Fin de l'image PE (dernière section), là où commence l'archive d'un SFX.
fn pe_end(data: &[u8]) -> Option<usize> {
    let pe = u32::from_le_bytes(data.get(0x3C..0x40)?.try_into().ok()?) as usize;
    if data.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let n = u16::from_le_bytes(data.get(pe + 6..pe + 8)?.try_into().ok()?) as usize;
    let opt = u16::from_le_bytes(data.get(pe + 20..pe + 22)?.try_into().ok()?) as usize;
    let sec = pe + 24 + opt;
    let mut end = 0;
    for i in 0..n {
        let s = sec + i * 40;
        let raw = u32::from_le_bytes(data.get(s + 16..s + 20)?.try_into().ok()?) as usize;
        let ptr = u32::from_le_bytes(data.get(s + 20..s + 24)?.try_into().ok()?) as usize;
        end = end.max(ptr + raw);
    }
    Some(end)
}

/// Position de l'archive 7z d'un exécutable auto-extractible : après
/// l'image PE, avec un en-tête de départ valide.
pub fn find_archive(data: &[u8]) -> Option<usize> {
    if data.len() < 64 || &data[..2] != b"MZ" {
        return None;
    }
    let from = pe_end(data)?;
    let mut i = from;
    while i + 32 <= data.len() {
        let rest = data.get(i..)?;
        let k = rest.windows(6).position(|w| w == SIGNATURE)?;
        let at = i + k;
        let h = data.get(at..at + 32)?;
        let crc = u32::from_le_bytes(h[8..12].try_into().ok()?);
        if crate::archive::crc32::crc32(&h[12..32]) == crc {
            return Some(at);
        }
        i = at + 1;
    }
    None
}

/// Bloc de configuration du module SFX de 7-Zip (`;!@Install@!UTF-8!`).
pub fn sfx_config(data: &[u8], archive_at: usize) -> Option<Vec<(String, String)>> {
    let head = &data[..archive_at];
    let start = head.windows(18).rposition(|w| w == b";!@Install@!UTF-8!")?;
    let body = &head[start + 18..];
    let end = body.windows(15).position(|w| w == b";!@InstallEnd@!")?;
    let text = String::from_utf8_lossy(&body[..end]);
    Some(
        text.lines()
            .filter_map(|l| {
                let (k, v) = l.split_once('=')?;
                // Une seule paire de guillemets autour de la valeur : un
                // `\"` final (RunProgram="\"%%T\\setup.exe\"") reste entier.
                let v = v.trim();
                let v = v
                    .strip_prefix('"')
                    .and_then(|x| x.strip_suffix('"'))
                    .unwrap_or(v);
                Some((k.trim().to_string(), v.to_string()))
            })
            .collect(),
    )
}

/// Machine de l'en-tête PE (0x14c x86, 0x8664 x64, 0xAA64 ARM64).
pub(super) fn pe_machine(data: &[u8]) -> Option<u16> {
    let pe = u32::from_le_bytes(data.get(0x3C..0x40)?.try_into().ok()?) as usize;
    if data.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    Some(u16::from_le_bytes(
        data.get(pe + 4..pe + 6)?.try_into().ok()?,
    ))
}

pub(super) fn is_gui_pe64(data: &[u8]) -> Option<(bool, bool)> {
    let pe = u32::from_le_bytes(data.get(0x3C..0x40)?.try_into().ok()?) as usize;
    if data.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let machine = u16::from_le_bytes(data.get(pe + 4..pe + 6)?.try_into().ok()?);
    let subsystem = u16::from_le_bytes(data.get(pe + 24 + 68..pe + 24 + 70)?.try_into().ok()?);
    Some((machine == 0x8664 || machine == 0xAA64, subsystem == 2))
}

/// Nom, version et éditeur tirés des ressources de version du lanceur.
pub fn names(data: &[u8], path: &Path) -> (String, String, String) {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Application".into());
    let name = crate::netports::version_string(data, "ProductName")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(stem);
    let version = crate::netports::version_string(data, "ProductVersion").unwrap_or_default();
    let publisher = crate::netports::version_string(data, "CompanyName").unwrap_or_default();
    (
        name.trim().to_string(),
        version.trim().to_string(),
        publisher.trim().to_string(),
    )
}

pub fn info(data: &[u8], path: &Path) -> Result<super::Info, String> {
    let at = find_archive(data).ok_or_else(|| bad("absente"))?;
    let (name, version, publisher) = names(data, path);
    let x64 = read_all(&data[at..])
        .map(|e| {
            e.iter()
                .any(|f| is_gui_pe64(&f.data).is_some_and(|(x, _)| x))
        })
        .unwrap_or(false);
    let pf = super::Folders { x64 }.program_files();
    Ok(super::Info {
        kind: super::Kind::SevenZip,
        name: name.clone(),
        version,
        publisher,
        default_dir: format!("{pf}\\{name}"),
    })
}

/// Programme que le module SFX lance après extraction (`RunProgram` ou
/// `ExecuteFile`), s'il y en a un.
fn runs_own_setup(data: &[u8], at: usize) -> bool {
    sfx_config(data, at).is_some_and(|c| {
        c.iter()
            .any(|(k, _)| k == "RunProgram" || k == "ExecuteFile")
    })
}

/// Paquet MSI porté par un auto-extractible qui lance son propre
/// programme d'installation : Paint.NET (SetupShim.exe, puis une interface
/// .NET, puis `PaintDotNet_x64.msi`). Winlux ne suit pas ce programme ; il
/// installe le paquet MSI lui-même, comme le ferait ce programme. Un seul
/// MSI par architecture est retenu : le x64 s'il y en a plusieurs.
pub fn embedded_msi(entries: &[Entry]) -> Option<&Entry> {
    let msis: Vec<&Entry> = entries
        .iter()
        .filter(|e| !e.dir && e.name.to_ascii_lowercase().ends_with(".msi"))
        .collect();
    match msis.len() {
        0 => None,
        1 => Some(msis[0]),
        _ => {
            let x64: Vec<&Entry> = msis
                .iter()
                .copied()
                .filter(|e| {
                    let n = e.name.to_ascii_lowercase();
                    n.contains("x64") || n.contains("amd64")
                })
                .collect();
            (x64.len() == 1).then(|| x64[0])
        }
    }
}

/// Archive d'installateur sans configuration d'auto-extractible reconnue
/// (auto-extracteur d'Adobe : `setup.exe`, `setup.ini`, `AcroPro.msi`…) :
/// `setup.exe` ou `setup.ini` à la racine. Ce n'est pas une application à
/// copier dans Program Files, mais un installateur à suivre (#1738).
pub fn is_setup_bundle(entries: &[Entry]) -> bool {
    entries.iter().any(|e| {
        !e.dir && !e.name.contains('\\') && {
            let n = e.name.to_ascii_lowercase();
            n == "setup.exe" || n == "setup.ini"
        }
    })
}

/// Section `[Product]` de `setup.ini` (clés en minuscules).
fn setup_ini_product(entries: &[Entry]) -> Vec<(String, String)> {
    let Some(ini) = entries
        .iter()
        .find(|e| !e.dir && e.name.eq_ignore_ascii_case("setup.ini"))
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&ini.data);
    let mut in_product = false;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_product = line.eq_ignore_ascii_case("[product]");
            continue;
        }
        if in_product {
            if let Some((k, v)) = line.split_once('=') {
                out.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
            }
        }
    }
    out
}

/// Lancement d'un auto-extractible 7z qui lance son propre installateur.
/// `None` : ce n'est pas ce cas, la suite habituelle s'applique.
/// - Il porte un paquet MSI (voir [`embedded_msi`]) : le contenu est extrait
///   dans un dossier temporaire (un MSI peut lire ses .cab à côté de lui),
///   le paquet est installé par le msiexec de Winlux, puis le dossier est
///   effacé.
/// - Il ne porte qu'un téléchargeur : voir [`super::web`].
/// - Archive d'installateur sans configuration reconnue ([`is_setup_bundle`],
///   Adobe) : le MSI nommé par `setup.ini` est installé de même, ou à
///   défaut le `setup.exe` de l'archive est lancé depuis le dossier
///   temporaire. Rien n'est jamais annoncé comme installé à sa place (#1738).
pub fn chain(exe: &Path, args: &[String], allow_download: bool) -> Option<i32> {
    let data = std::fs::read(exe).ok()?;
    let at = find_archive(&data)?;
    let own_setup = runs_own_setup(&data, at);
    let entries = match read_all(&data[at..]) {
        Ok(e) => e,
        Err(e) if own_setup => {
            eprintln!("setup : {} : {e}", exe.display());
            return Some(1);
        }
        Err(_) => return None,
    };
    let bundle = !own_setup && is_setup_bundle(&entries);
    if !own_setup && !bundle {
        return None;
    }
    let product = setup_ini_product(&entries);
    let named_msi = product.iter().find(|(k, _)| k == "msi").and_then(|(_, v)| {
        entries
            .iter()
            .find(|e| !e.dir && e.name.eq_ignore_ascii_case(v.trim_matches('"')))
    });
    let Some(msi) = named_msi.or_else(|| embedded_msi(&entries)) else {
        if bundle {
            return Some(run_bundle_setup(&entries, args));
        }
        if super::web::is_downloader(&entries) {
            return Some(super::web::run(&data, exe, args, allow_download));
        }
        // Programme nommé par la configuration du SFX (Zoom, #2390).
        let config = sfx_config(&data, at).unwrap_or_default();
        drop(data);
        return Some(run_sfx_program(&entries, &config, args));
    };
    let quiet = args.iter().any(|a| {
        let w = a.trim_start_matches(['/', '-']).to_ascii_lowercase();
        matches!(
            w.as_str(),
            "s" | "q" | "qn" | "quiet" | "silent" | "verysilent"
        )
    });
    let work = crate::paths::setup_work_dir("sfx");
    let _ = std::fs::remove_dir_all(&work);
    let mut msi_path = None;
    for e in &entries {
        let rel: std::path::PathBuf = e.name.split('\\').collect();
        let path = work.join(rel);
        let written = if e.dir {
            std::fs::create_dir_all(&path)
        } else {
            path.parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&path, &e.data))
        };
        if let Err(err) = written {
            eprintln!("setup : {} : {err}", path.display());
            let _ = std::fs::remove_dir_all(&work);
            return Some(1);
        }
        if std::ptr::eq(e, msi) {
            msi_path = Some(path);
        }
    }
    drop(entries);
    let msi_path = msi_path?;
    // Contrat de licence du MSI porté (Acrobat Reader, #2330) : la même
    // page que pour msiexec, avant toute installation. Refus = 1602, le
    // contenu extrait est effacé et rien n'est installé. Une option
    // silencieuse vaut acceptation, comme sous Windows.
    if let Some(code) = super::license::require_acceptance(&msi_path, args) {
        let _ = std::fs::remove_dir_all(&work);
        return Some(code);
    }
    let mut log = crate::msi::install::Logger { file: None, quiet };
    // Correctif nommé par setup.ini (Acrobat) : pas encore appliqué (#1715).
    if let Some((_, patch)) = product.iter().find(|(k, _)| k == "patch") {
        log.log(&format!(
            "correctif .msp non pris en charge, installation de la version de base : {patch} (#1715)"
        ));
    }
    let opts = crate::msi::install::Options {
        props: Vec::new(),
        quiet,
        admin: false,
    };
    let rc = crate::msi::install::install(&msi_path, &opts, &mut log);
    let _ = std::fs::remove_dir_all(&work);
    Some(rc)
}

/// Programme et paramètres que le module SFX de 7-Zip lance après
/// extraction, d'après sa configuration (aide de 7-Zip, commutateur `-sfx`) :
/// - `RunProgram` : « Command for executing », `setup.exe` par défaut,
///   précédée du préfixe `Directory` (`.\` par défaut, c'est-à-dire le
///   dossier d'extraction) ; `%%T` y désigne le dossier d'extraction ;
/// - sinon `ExecuteFile` (« Name of file for executing ») avec
///   `ExecuteParameters`.
///
/// Rend le programme (relatif au dossier d'extraction, ou chemin Windows
/// tel quel) et ses paramètres, `%%T` remplacé par `temp`.
pub(crate) fn sfx_program(config: &[(String, String)], temp: &str) -> (String, String) {
    let get = |k: &str| {
        config
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| unescape(v).replace("%%T", temp))
    };
    let (program, params) = match (get("RunProgram"), get("ExecuteFile")) {
        (Some(cmd), _) => split_command(&cmd),
        (None, Some(file)) => (file, get("ExecuteParameters").unwrap_or_default()),
        (None, None) => ("setup.exe".to_string(), String::new()),
    };
    let program = match get("Directory") {
        Some(dir) if !dir.is_empty() && !is_absolute_windows(&program) => {
            format!("{}\\{}", dir.trim_end_matches('\\'), program)
        }
        _ => program,
    };
    let program = program
        .strip_prefix(".\\")
        .map(str::to_string)
        .unwrap_or(program);
    (program, params)
}

/// `\\` et `\"` dans une valeur : l'aide de 7-Zip écrit le préfixe par
/// défaut de `Directory` `".\\"`, soit `.\` une fois l'échappement retiré.
fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        match (c, chars.clone().next()) {
            ('\\', Some(n @ ('\\' | '"'))) => {
                out.push(n);
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

fn is_absolute_windows(p: &str) -> bool {
    let b = p.as_bytes();
    p.starts_with("\\\\") || (b.len() > 2 && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
}

/// Premier mot d'une ligne de commande (entre guillemets ou jusqu'à
/// l'espace) et le reste.
fn split_command(cmd: &str) -> (String, String) {
    let cmd = cmd.trim();
    if let Some(rest) = cmd.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            return (rest[..end].to_string(), rest[end + 1..].trim().to_string());
        }
    }
    match cmd.split_once(' ') {
        Some((p, r)) => (p.to_string(), r.trim().to_string()),
        None => (cmd.to_string(), String::new()),
    }
}

/// Auto-extractible 7-Zip qui lance son propre programme (Zoom, #2390),
/// comme le module SFX : archive extraite dans le dossier temporaire du
/// profil (sur le disque, #2326), programme de la configuration lancé et
/// attendu, son code rendu tel quel (un contrat refusé dans ce programme
/// rend son propre code, 1602 pour une annulation), dossier effacé ensuite.
fn run_sfx_program(entries: &[Entry], config: &[(String, String)], args: &[String]) -> i32 {
    let work = crate::paths::setup_work_dir("sfx");
    let _ = std::fs::remove_dir_all(&work);
    if let Err(code) = extract_entries(entries, &work) {
        return code;
    }
    let dir = crate::paths::to_windows(&work);
    let (program, mut params) = sfx_program(config, &dir);
    let inside = work.join(program.split('\\').collect::<std::path::PathBuf>());
    let win = if !is_absolute_windows(&program)
        && !program.split('\\').any(|p| p == "..")
        && inside.is_file()
    {
        crate::paths::to_windows(&inside)
    } else {
        program
    };
    if !args.is_empty() {
        if !params.is_empty() {
            params.push(' ');
        }
        params.push_str(&args.join(" "));
    }
    let outcome = super::run::program(&win, &params, &dir, true, false);
    eprintln!("setup : {}", super::run::describe(&win, &outcome));
    if !matches!(outcome, super::run::Outcome::TimedOut) {
        let _ = std::fs::remove_dir_all(&work);
    }
    match outcome {
        super::run::Outcome::Exited(c) => c,
        super::run::Outcome::Started | super::run::Outcome::TimedOut => 0,
        _ => 1,
    }
}

/// Écrit les entrées de l'archive sous `work` ; en cas d'erreur, le
/// dossier est effacé et le code 1 rendu.
fn extract_entries(entries: &[Entry], work: &Path) -> Result<(), i32> {
    for e in entries {
        let path = work.join(e.name.split('\\').collect::<std::path::PathBuf>());
        let written = if e.dir {
            std::fs::create_dir_all(&path)
        } else {
            path.parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&path, &e.data))
        };
        if let Err(err) = written {
            eprintln!("setup : {} : {err}", path.display());
            let _ = std::fs::remove_dir_all(work);
            return Err(1);
        }
    }
    Ok(())
}

/// Archive d'installateur sans MSI : extraite dans un dossier temporaire,
/// puis son `setup.exe` est lancé et attendu ; son code est rendu (#1738).
fn run_bundle_setup(entries: &[Entry], args: &[String]) -> i32 {
    let work = crate::paths::setup_work_dir("sfx");
    let _ = std::fs::remove_dir_all(&work);
    let mut setup = None;
    for e in entries {
        let path = work.join(e.name.split('\\').collect::<std::path::PathBuf>());
        let written = if e.dir {
            std::fs::create_dir_all(&path)
        } else {
            path.parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&path, &e.data))
        };
        if let Err(err) = written {
            eprintln!("setup : {} : {err}", path.display());
            let _ = std::fs::remove_dir_all(&work);
            return 1;
        }
        if !e.dir && e.name.eq_ignore_ascii_case("setup.exe") {
            setup = Some(path);
        }
    }
    let Some(setup) = setup else {
        let _ = std::fs::remove_dir_all(&work);
        eprintln!("setup : archive d'installateur sans setup.exe ni paquet MSI");
        return 1;
    };
    let win = crate::paths::to_windows(&setup);
    let dir = crate::paths::to_windows(&work);
    let params = args.join(" ");
    let outcome = super::run::program(&win, &params, &dir, true, false);
    eprintln!("setup : {}", super::run::describe(&win, &outcome));
    // Toujours en cours : son dossier reste, il en a encore besoin.
    if !matches!(outcome, super::run::Outcome::TimedOut) {
        let _ = std::fs::remove_dir_all(&work);
    }
    match outcome {
        super::run::Outcome::Exited(c) => c,
        super::run::Outcome::Started | super::run::Outcome::TimedOut => 0,
        _ => 1,
    }
}

/// Installe : les fichiers vont dans Program Files\<produit>, un
/// raccourci du menu Démarrer pointe sur le programme principal.
pub fn install(
    data: &[u8],
    path: &Path,
    opts: &Options,
    inst: &mut Installer,
) -> Result<(), String> {
    let at = find_archive(data).ok_or_else(|| bad("absente"))?;
    if runs_own_setup(data, at) {
        return Err(
            "cet auto-extractible lance son propre programme d'installation ; Winlux ne sait pas encore le suivre"
                .into(),
        );
    }
    let entries = read_all(&data[at..])?;
    if is_setup_bundle(&entries) {
        // Jamais copié dans Program Files ni annoncé comme installé (#1738).
        return Err(
            "archive d'installateur (setup.exe, setup.ini) : à lancer, pas à copier".into(),
        );
    }
    let (name, version, publisher) = names(data, path);
    let x64 = entries
        .iter()
        .any(|f| is_gui_pe64(&f.data).is_some_and(|(x, _)| x));
    let dir = opts
        .dir
        .clone()
        .unwrap_or_else(|| format!("{}\\{}", super::Folders { x64 }.program_files(), name));
    inst.log.log(&format!("{name} {version} → {dir}"));
    inst.mkdirs_win(&dir).map_err(|e| format!("{dir} : {e}"))?;
    let mut guis = Vec::new();
    for e in &entries {
        let win = format!("{dir}\\{}", e.name);
        if e.dir {
            inst.mkdirs_win(&win)
                .map_err(|er| format!("{win} : {er}"))?;
            continue;
        }
        inst.write_file(&win, &e.data)
            .map_err(|er| format!("{win} : {er}"))?;
        let top = !e.name.contains('\\');
        let lower = e.name.to_lowercase();
        if top
            && lower.ends_with(".exe")
            && !lower.starts_with("unins")
            && is_gui_pe64(&e.data).is_some_and(|(_, gui)| gui)
        {
            let desc = crate::netports::version_string(&e.data, "FileDescription")
                .filter(|d| !d.trim().is_empty())
                .unwrap_or_else(|| e.name.trim_end_matches(".exe").to_string());
            guis.push((e.name.clone(), desc.trim().to_string()));
        }
    }
    // 7-Zip : le programme principal est le gestionnaire de fichiers, les
    // autres (7zG) ne s'ouvrent qu'avec des arguments.
    if guis.iter().any(|(n, _)| n.eq_ignore_ascii_case("7zFM.exe")) {
        guis.retain(|(n, _)| n.eq_ignore_ascii_case("7zFM.exe"));
    }
    let menu = super::Folders::programs();
    for (exe, desc) in &guis {
        let link = if guis.len() == 1 {
            format!("{menu}\\{desc}.lnk")
        } else {
            format!("{menu}\\{name}\\{desc}.lnk")
        };
        let target = format!("{dir}\\{exe}");
        inst.shortcut(&link, &target, "", &dir, desc)
            .map_err(|e| format!("{link} : {e}"))?;
    }
    inst.manifest.id = name.clone();
    inst.manifest.name = name;
    inst.manifest.version = version;
    inst.manifest.publisher = publisher;
    inst.manifest.location = dir;
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Encodeur minimal d'un nombre 7z.
    fn num(v: u64) -> Vec<u8> {
        if v < 0x80 {
            vec![v as u8]
        } else if v < 0x4000 {
            vec![0x80 | (v >> 8) as u8, v as u8]
        } else {
            let mut o = vec![0xFF];
            o.extend(v.to_le_bytes());
            o
        }
    }

    /// Archive 7z « copie » (méthode 00) : un dossier, plusieurs fichiers
    /// et un dossier vide.
    pub fn build_copy_archive(files: &[(&str, &[u8])], empty_dir: &str) -> Vec<u8> {
        let mut packed = Vec::new();
        for (_, d) in files {
            packed.extend_from_slice(d);
        }
        let mut h = vec![0x01, 0x04];
        // PackInfo
        h.push(0x06);
        h.extend(num(0));
        h.extend(num(1));
        h.push(0x09);
        h.extend(num(packed.len() as u64));
        h.push(0x00);
        // UnPackInfo : un dossier, un codeur « copie ».
        h.extend([0x07, 0x0B]);
        h.extend(num(1));
        h.push(0x00);
        h.extend([0x01, 0x01, 0x00]);
        h.push(0x0C);
        h.extend(num(packed.len() as u64));
        h.push(0x00);
        // SubStreamsInfo
        h.push(0x08);
        h.push(0x0D);
        h.extend(num(files.len() as u64));
        h.push(0x09);
        for (_, d) in &files[..files.len() - 1] {
            h.extend(num(d.len() as u64));
        }
        h.push(0x0A);
        h.push(0x01);
        for (_, d) in files {
            h.extend(crate::archive::crc32::crc32(d).to_le_bytes());
        }
        h.push(0x00);
        h.push(0x00); // fin des flux
                      // FilesInfo
        let n = files.len() + 1;
        h.push(0x05);
        h.extend(num(n as u64));
        // EmptyStream : le dernier (le dossier).
        let mut bits = vec![0u8; n.div_ceil(8)];
        bits[(n - 1) / 8] |= 0x80 >> ((n - 1) % 8);
        h.push(0x0E);
        h.extend(num(bits.len() as u64));
        h.extend(&bits);
        let mut names = vec![0u8];
        for (name, _) in files {
            for c in name.encode_utf16().chain([0]) {
                names.extend(c.to_le_bytes());
            }
        }
        for c in empty_dir.encode_utf16().chain([0]) {
            names.extend(c.to_le_bytes());
        }
        h.push(0x11);
        h.extend(num(names.len() as u64));
        h.extend(names);
        h.push(0x00);
        h.push(0x00);
        let mut a = SIGNATURE.to_vec();
        a.extend([0, 4]);
        let mut start = Vec::new();
        start.extend((packed.len() as u64).to_le_bytes());
        start.extend((h.len() as u64).to_le_bytes());
        start.extend(crate::archive::crc32::crc32(&h).to_le_bytes());
        a.extend(crate::archive::crc32::crc32(&start).to_le_bytes());
        a.extend(start);
        a.extend(packed);
        a.extend(h);
        a
    }

    #[test]
    fn archive_copie_lue_avec_ses_dossiers() {
        let a = build_copy_archive(
            &[("lisezmoi.txt", b"bonjour"), ("Lang/fr.txt", b"francais")],
            "vide",
        );
        let e = read_all(&a).unwrap();
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].name, "lisezmoi.txt");
        assert_eq!(e[0].data, b"bonjour");
        assert_eq!(e[1].name, "Lang\\fr.txt");
        assert_eq!(e[1].data, b"francais");
        assert!(e[2].dir);
    }

    #[test]
    fn archive_abimee_refusee() {
        let mut a = build_copy_archive(&[("a", b"x"), ("b", b"y")], "d");
        let n = a.len();
        a[n - 3] ^= 0xFF;
        assert!(read_all(&a).is_err());
    }

    #[test]
    fn chemins_qui_sortent_du_dossier_refuses() {
        let a = build_copy_archive(&[("..\\x", b"1"), ("b", b"2")], "d");
        assert!(read_all(&a).unwrap_err().contains("refusé"));
    }

    #[test]
    fn filtre_bcj_x86_restaure_les_appels() {
        // Encodage BCJ d'un CALL relatif à la position 0 : cible absolue
        // = relatif + (position + 5).
        let mut d = vec![0xE8, 0x10, 0x00, 0x00, 0x00, 0x90, 0x90, 0x90, 0x90];
        let rel: u32 = 0x10;
        let abs = rel.wrapping_add(5);
        d[1..5].copy_from_slice(&abs.to_le_bytes());
        bcj_x86_decode(&mut d);
        assert_eq!(&d[1..5], &rel.to_le_bytes());
    }

    #[test]
    fn bcj2_sans_saut_recopie_le_flux_principal() {
        // Flux de décision : 5 octets nuls → tous les bits à 0.
        let main = b"\x01\x02\xE8\x03\x04";
        let out = bcj2_decode(main, &[], &[], &[0; 8], main.len()).unwrap();
        assert_eq!(out, main);
    }

    #[test]
    fn sfx_trouve_l_archive_apres_le_pe() {
        // PE minimal sans section : l'archive suit l'en-tête.
        let mut pe = vec![0u8; 0x200];
        pe[0..2].copy_from_slice(b"MZ");
        pe[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        pe[0x80 + 4..0x80 + 6].copy_from_slice(&0x14Cu16.to_le_bytes());
        // Une signature au hasard dans le lanceur ne compte pas.
        pe[0x150..0x156].copy_from_slice(SIGNATURE);
        let at = pe.len();
        pe.extend(build_copy_archive(&[("a.txt", b"a"), ("b.txt", b"b")], "d"));
        assert_eq!(find_archive(&pe), Some(at));
        assert_eq!(read_all(&pe[at..]).unwrap().len(), 3);
    }

    #[test]
    fn sfx_qui_lance_son_installateur_est_reconnu() {
        // Bloc de configuration tel que l'écrit le module SFX de 7-Zip
        // (Paint.NET) : la balise de fin fait 15 octets.
        let mut d = b"MZ....".to_vec();
        d.extend(b";!@Install@!UTF-8!\r\nTitle=\"Paint.NET Setup\"\r\n");
        d.extend(b"ExecuteFile=\"SetupShim.exe\"\r\n;!@InstallEnd@!");
        let at = d.len();
        d.extend(SIGNATURE);
        let c = sfx_config(&d, at).unwrap();
        assert!(c.contains(&("ExecuteFile".into(), "SetupShim.exe".into())));
        assert!(c.contains(&("Title".into(), "Paint.NET Setup".into())));
        assert!(runs_own_setup(&d, at));
        // Sans bloc : une archive simple (7-Zip lui-même).
        let plain = [b"MZ....".as_slice(), SIGNATURE].concat();
        assert!(!runs_own_setup(&plain, 6));
    }

    /// #2390 : programme lancé par le module SFX, d'après sa configuration
    /// (aide de 7-Zip, commutateur -sfx).
    #[test]
    fn programme_du_module_sfx() {
        let c = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        let t = "C:\\Users\\a\\AppData\\Local\\Temp\\winlux-sfx-1";
        // Rien : setup.exe par défaut.
        assert_eq!(sfx_program(&[], t), ("setup.exe".into(), String::new()));
        // ExecuteFile et ses paramètres.
        assert_eq!(
            sfx_program(
                &c(&[
                    ("ExecuteFile", "msiexec.exe"),
                    ("ExecuteParameters", "/i 7zip.msi REINSTALL=ALL")
                ]),
                t
            ),
            ("msiexec.exe".into(), "/i 7zip.msi REINSTALL=ALL".into())
        );
        // RunProgram : %%T, guillemets échappés, paramètres.
        let (p, a) = sfx_program(
            &c(&[("RunProgram", "\\\"%%T\\\\bin\\\\setup.exe\\\" /x")]),
            t,
        );
        assert_eq!(p, format!("{t}\\bin\\setup.exe"));
        assert_eq!(a, "/x");
        // Directory : préfixe du programme ; « .\\ » par défaut.
        assert_eq!(
            sfx_program(&c(&[("Directory", "bin"), ("RunProgram", "go.exe")]), t).0,
            "bin\\go.exe"
        );
        assert_eq!(
            sfx_program(&c(&[("Directory", ".\\\\"), ("RunProgram", "go.exe")]), t).0,
            "go.exe"
        );
        // Bloc de configuration : une seule paire de guillemets retirée.
        let mut d = b"MZ....".to_vec();
        d.extend(b";!@Install@!UTF-8!\r\nRunProgram=\"\\\"%%T\\\\a.exe\\\"\"\r\n;!@InstallEnd@!");
        let at = d.len();
        d.extend(SIGNATURE);
        let cfg = sfx_config(&d, at).unwrap();
        assert_eq!(cfg[0].1, "\\\"%%T\\\\a.exe\\\"");
        assert_eq!(sfx_program(&cfg, t).0, format!("{t}\\a.exe"));
    }

    #[test]
    fn installeur_web_inconnu_refuse_sans_rien_telecharger() {
        // Auto-extractible qui lance SetupShim.exe et ne porte qu'un
        // téléchargeur, sans ressources de version : aucun paquet connu.
        let mut d = vec![0u8; 0x200];
        d[0..2].copy_from_slice(b"MZ");
        d[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        d[0x80..0x84].copy_from_slice(b"PE\0\0");
        d[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
        d.extend(b";!@Install@!UTF-8!\r\nExecuteFile=\"SetupShim.exe\"\r\n;!@InstallEnd@!");
        d.extend(build_copy_archive(
            &[
                ("SetupShim.exe", b"MZ"),
                ("x64\\SetupDownloader\\SetupDownloader.exe", b"MZ"),
            ],
            "x64",
        ));
        let dir = std::env::temp_dir().join(format!("winlux-web-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("app.web.exe");
        std::fs::write(&exe, &d).unwrap();
        assert_eq!(chain(&exe, &[], false), Some(1));
        assert_eq!(chain(&exe, &[], true), Some(1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// #2330 : un auto-extractible qui porte un MSI à contrat (Acrobat
    /// Reader) montre la page du contrat avant d'installer ; refus ou pas
    /// d'affichage = 1602, rien d'installé, le contenu extrait effacé.
    #[test]
    fn contrat_du_msi_porte_par_l_auto_extractible() {
        let _g = crate::mounts::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("winlux-sfx-licence-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let home = dir.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let mut d = vec![0u8; 0x200];
        d[0..2].copy_from_slice(b"MZ");
        d[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        d[0x80..0x84].copy_from_slice(b"PE\0\0");
        d[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
        d.extend(b";!@Install@!UTF-8!\r\nExecuteFile=\"setup.exe\"\r\n;!@InstallEnd@!");
        d.extend(build_copy_archive(
            &[
                ("setup.exe", b"MZ"),
                (
                    "x64\\Licence.msi",
                    include_bytes!("../msi/testdata/licence.msi"),
                ),
            ],
            "x64",
        ));
        let exe = dir.join("Reader.exe");
        std::fs::write(&exe, &d).unwrap();
        let refuse = dir.join("refuse.sh");
        std::fs::write(&refuse, "#!/bin/sh\nexit 66\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&refuse, std::fs::Permissions::from_mode(0o755)).unwrap();
        let keys = [
            "WINLUX_HOME",
            "WINLUX_DRIVE_C",
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "WINLUX_SETUP_WIZARD",
            "WINLUX_SETUP_DEPTH",
        ];
        let saved: Vec<_> = keys.iter().map(|k| (*k, std::env::var_os(k))).collect();
        std::env::set_var("WINLUX_HOME", &home);
        std::env::set_var("WINLUX_DRIVE_C", &home);
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("WINLUX_SETUP_DEPTH");
        // Page du contrat, refusée.
        std::env::set_var("DISPLAY", ":99");
        std::env::set_var("WINLUX_SETUP_WIZARD", &refuse);
        let refused = chain(&exe, &[], false);
        // Sans affichage : la page ne peut pas s'ouvrir.
        std::env::remove_var("DISPLAY");
        let headless = chain(&exe, &[], false);
        for (k, v) in saved {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        assert_eq!(refused, Some(1602));
        assert_eq!(headless, Some(1602));
        assert!(!home.join("Program Files").exists(), "rien d'installé");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn msi_porte_par_l_auto_extractible() {
        let e = |name: &str| Entry {
            name: name.into(),
            dir: false,
            data: Vec::new(),
        };
        let one = [e("SetupShim.exe"), e("x64\\PaintDotNet_x64.msi")];
        assert_eq!(embedded_msi(&one).unwrap().name, "x64\\PaintDotNet_x64.msi");
        let two = [e("x86\\App_x86.msi"), e("x64\\App_x64.msi")];
        assert_eq!(embedded_msi(&two).unwrap().name, "x64\\App_x64.msi");
        assert!(embedded_msi(&[e("setup.exe")]).is_none());
        let ambigu = [e("a.msi"), e("b.msi")];
        assert!(embedded_msi(&ambigu).is_none());
    }

    /// Auto-extracteur d'Adobe : setup.exe et setup.ini à la racine, le MSI
    /// et le correctif nommés par `[Product]` (#1738).
    #[test]
    fn archive_d_installateur_reconnue() {
        let f = |n: &str, d: &[u8]| Entry {
            name: n.into(),
            dir: false,
            data: d.to_vec(),
        };
        let ini =
            b"[Startup]\r\nRequireOS64=1\r\n[Product]\r\nmsi=AcroPro.msi\r\nPATCH=Upd.msp\r\n";
        let bundle = [
            f("setup.exe", b"MZ"),
            f("setup.ini", ini),
            f("AcroPro.msi", b""),
            f("Transforms\\1036.mst", b""),
        ];
        assert!(is_setup_bundle(&bundle));
        let p = setup_ini_product(&bundle);
        assert!(p.contains(&("msi".into(), "AcroPro.msi".into())));
        assert!(p.contains(&("patch".into(), "Upd.msp".into())));
        assert!(!p.iter().any(|(k, _)| k == "requireos64"));
        // Application portable : setup.exe dans un sous-dossier ne compte pas.
        assert!(!is_setup_bundle(&[
            f("App\\setup.exe", b"MZ"),
            f("app.exe", b"MZ")
        ]));
    }
}
