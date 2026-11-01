//! Lecture des fichiers composés OLE (Compound File Binary), le conteneur
//! des paquets .msi : en-tête, FAT, mini-FAT et répertoire des flux.

use std::collections::HashMap;

const FREE: u32 = 0xFFFF_FFFF;
const END: u32 = 0xFFFF_FFFE;
const MAX_CHAIN: usize = 1 << 22;

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

pub struct Compound {
    /// Flux à la racine, par nom décodé (`!File`, `!_StringPool`, `Data.cab`…).
    pub streams: HashMap<String, Vec<u8>>,
}

struct Reader<'a> {
    data: &'a [u8],
    sector: usize,
    fat: Vec<u32>,
}

impl Reader<'_> {
    fn sector(&self, n: u32) -> Option<&[u8]> {
        let start = (n as usize + 1).checked_mul(self.sector)?;
        self.data.get(start..start + self.sector)
    }

    fn chain(&self, table: &[u32], first: u32) -> Option<Vec<u32>> {
        let mut out = Vec::new();
        let mut n = first;
        while n != END && n != FREE {
            if out.len() > MAX_CHAIN || n as usize >= table.len() {
                return None;
            }
            out.push(n);
            n = *table.get(n as usize)?;
            if out.len() > table.len() {
                return None; // boucle dans la chaîne
            }
        }
        Some(out)
    }

    fn read(&self, first: u32, size: usize) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(size.min(1 << 28));
        for n in self.chain(&self.fat, first)? {
            out.extend_from_slice(self.sector(n)?);
            if out.len() >= size {
                break;
            }
        }
        if out.len() < size {
            return None;
        }
        out.truncate(size);
        Some(out)
    }
}

/// Décode un nom de flux MSI (caractères compressés 0x3800-0x4840).
pub fn decode_name(raw: &[u16]) -> String {
    fn mime(x: u16) -> char {
        match x {
            0..=9 => (b'0' + x as u8) as char,
            10..=35 => (b'A' + (x - 10) as u8) as char,
            36..=61 => (b'a' + (x - 36) as u8) as char,
            62 => '.',
            _ => '_',
        }
    }
    let mut out = String::new();
    for &c in raw {
        match c {
            0x4840 => out.push('!'),
            0x4800..=0x483F => out.push(mime(c - 0x4800)),
            0x3800..=0x47FF => {
                let v = c - 0x3800;
                out.push(mime(v & 0x3F));
                out.push(mime((v >> 6) & 0x3F));
            }
            _ => out.push(char::from_u32(c as u32).unwrap_or('?')),
        }
    }
    out
}

impl Compound {
    pub fn parse(data: &[u8]) -> Result<Compound, String> {
        if data.len() < 512 || data[..8] != [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1] {
            return Err("ce fichier n'est pas un paquet Windows Installer".into());
        }
        let bad = || "paquet Windows Installer endommagé".to_string();
        let shift = u16_at(data, 0x1E).ok_or_else(bad)?;
        let mini_shift = u16_at(data, 0x20).ok_or_else(bad)?;
        if !(9..=12).contains(&shift) || mini_shift != 6 {
            return Err(bad());
        }
        let sector = 1usize << shift;
        let n_fat = u32_at(data, 0x2C).ok_or_else(bad)? as usize;
        let first_dir = u32_at(data, 0x30).ok_or_else(bad)?;
        let cutoff = u32_at(data, 0x38).ok_or_else(bad)? as usize;
        let first_mini = u32_at(data, 0x3C).ok_or_else(bad)?;
        let mut difat_next = u32_at(data, 0x44).ok_or_else(bad)?;
        let mut fat_sectors: Vec<u32> = (0..109)
            .filter_map(|i| u32_at(data, 0x4C + i * 4))
            .filter(|&s| s != FREE)
            .collect();
        let mut reader = Reader {
            data,
            sector,
            fat: Vec::new(),
        };
        let per = sector / 4;
        let mut guard = 0;
        while difat_next != END && difat_next != FREE && guard < 1 << 16 {
            let s = reader.sector(difat_next).ok_or_else(bad)?;
            for i in 0..per - 1 {
                let v = u32_at(s, i * 4).ok_or_else(bad)?;
                if v != FREE {
                    fat_sectors.push(v);
                }
            }
            difat_next = u32_at(s, (per - 1) * 4).ok_or_else(bad)?;
            guard += 1;
        }
        fat_sectors.truncate(n_fat.max(1));
        let mut fat = Vec::with_capacity(fat_sectors.len() * per);
        for s in fat_sectors {
            let bytes = reader.sector(s).ok_or_else(bad)?;
            fat.extend(
                bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| u32::from_le_bytes(*c)),
            );
        }
        reader.fat = fat;
        let dir_sectors = reader.chain(&reader.fat, first_dir).ok_or_else(bad)?;
        let mut dir = Vec::new();
        for s in dir_sectors {
            dir.extend_from_slice(reader.sector(s).ok_or_else(bad)?);
        }
        struct Entry {
            name: String,
            kind: u8,
            left: u32,
            right: u32,
            child: u32,
            start: u32,
            size: usize,
        }
        let entries: Vec<Entry> = dir
            .chunks(128)
            .filter(|e| e.len() == 128)
            .map(|e| {
                let len = (u16_at(e, 64).unwrap_or(0) as usize).min(64) / 2;
                let raw: Vec<u16> = e[..len * 2]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_le_bytes(*c))
                    .take_while(|&c| c != 0)
                    .collect();
                Entry {
                    name: decode_name(&raw),
                    kind: e[66],
                    left: u32_at(e, 68).unwrap_or(FREE),
                    right: u32_at(e, 72).unwrap_or(FREE),
                    child: u32_at(e, 76).unwrap_or(FREE),
                    start: u32_at(e, 116).unwrap_or(END),
                    size: u32_at(e, 120).unwrap_or(0) as usize,
                }
            })
            .collect();
        let root = entries.first().filter(|e| e.kind == 5).ok_or_else(bad)?;
        let mini_stream = reader.read(root.start, root.size).unwrap_or_default();
        let mut mini_fat = Vec::new();
        if let Some(chain) = reader.chain(&reader.fat, first_mini) {
            for s in chain {
                let bytes = reader.sector(s).ok_or_else(bad)?;
                mini_fat.extend(
                    bytes
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|c| u32::from_le_bytes(*c)),
                );
            }
        }
        let read_mini = |first: u32, size: usize| -> Option<Vec<u8>> {
            // La taille annoncée vient du répertoire : capacité plafonnée.
            let mut out = Vec::with_capacity(size.min(mini_stream.len()));
            for n in reader.chain(&mini_fat, first)? {
                let start = n as usize * 64;
                out.extend_from_slice(mini_stream.get(start..start + 64)?);
                if out.len() >= size {
                    break;
                }
            }
            (out.len() >= size).then(|| {
                out.truncate(size);
                out
            })
        };
        // Parcours de l'arbre des enfants de la racine (frères gauche/droite).
        let mut streams = HashMap::new();
        let mut stack = vec![root.child];
        let mut seen = vec![false; entries.len()];
        while let Some(i) = stack.pop() {
            let Some(e) = entries.get(i as usize) else {
                continue;
            };
            if std::mem::replace(&mut seen[i as usize], true) {
                continue;
            }
            stack.push(e.left);
            stack.push(e.right);
            if e.kind != 2 {
                continue;
            }
            let bytes = if e.size == 0 {
                Some(Vec::new())
            } else if e.size < cutoff {
                read_mini(e.start, e.size)
            } else {
                reader.read(e.start, e.size)
            };
            if let Some(bytes) = bytes {
                streams.insert(e.name.clone(), bytes);
            }
        }
        Ok(Compound { streams })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_de_flux_msi() {
        // « !File » : marqueur de table puis « Fi » « le » compressés.
        let pair = |a: u16, b: u16| 0x3800 + a + (b << 6);
        let raw = [0x4840, pair(15, 44), pair(47, 40)];
        assert_eq!(decode_name(&raw), "!File");
        assert_eq!(decode_name(&[0x0005, b'S' as u16]), "\u{5}S");
    }
}
