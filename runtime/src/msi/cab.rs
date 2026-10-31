//! Cabinets Microsoft (.cab) tels qu'ils sont intégrés aux paquets .msi :
//! dossiers non compressés, MSZIP (DEFLATE par blocs) et LZX.

use crate::archive::inflate::Inflater;
use std::io::Read;

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

fn cstr(d: &[u8], o: usize) -> Option<(&[u8], usize)> {
    let rest = d.get(o..)?;
    let end = rest.iter().position(|&b| b == 0)?;
    Some((&rest[..end], o + end + 1))
}

pub struct CabFile {
    pub name: String,
    folder: usize,
    offset: usize,
    size: usize,
}

struct Folder {
    start: usize,
    blocks: usize,
    compression: u16,
}

pub struct Cabinet<'a> {
    data: &'a [u8],
    folders: Vec<Folder>,
    pub files: Vec<CabFile>,
    data_reserve: usize,
}

const MAX_FOLDER: usize = 4 << 30;

impl<'a> Cabinet<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Cabinet<'a>, String> {
        let bad = || "cabinet endommagé".to_string();
        if data.get(..4) != Some(b"MSCF") {
            return Err("cabinet invalide (signature MSCF absente)".into());
        }
        let files_at = u32_at(data, 16).ok_or_else(bad)? as usize;
        let n_folders = u16_at(data, 26).ok_or_else(bad)? as usize;
        let n_files = u16_at(data, 28).ok_or_else(bad)? as usize;
        let flags = u16_at(data, 30).ok_or_else(bad)?;
        let mut at = 36;
        let (mut folder_reserve, mut data_reserve) = (0, 0);
        if flags & 4 != 0 {
            let header = u16_at(data, 36).ok_or_else(bad)? as usize;
            folder_reserve = *data.get(38).ok_or_else(bad)? as usize;
            data_reserve = *data.get(39).ok_or_else(bad)? as usize;
            at = 40 + header;
        }
        for bit in [1u16, 2] {
            if flags & bit != 0 {
                at = cstr(data, at).ok_or_else(bad)?.1;
                at = cstr(data, at).ok_or_else(bad)?.1;
            }
        }
        let mut folders = Vec::with_capacity(n_folders);
        for _ in 0..n_folders {
            folders.push(Folder {
                start: u32_at(data, at).ok_or_else(bad)? as usize,
                blocks: u16_at(data, at + 4).ok_or_else(bad)? as usize,
                compression: u16_at(data, at + 6).ok_or_else(bad)?,
            });
            at += 8 + folder_reserve;
        }
        let mut files = Vec::with_capacity(n_files);
        at = files_at;
        for _ in 0..n_files {
            let size = u32_at(data, at).ok_or_else(bad)? as usize;
            let offset = u32_at(data, at + 4).ok_or_else(bad)? as usize;
            let folder = u16_at(data, at + 8).ok_or_else(bad)? as usize;
            let attribs = u16_at(data, at + 14).ok_or_else(bad)?;
            let (raw, next) = cstr(data, at + 16).ok_or_else(bad)?;
            let name = if attribs & 0x80 != 0 {
                String::from_utf8_lossy(raw).into_owned()
            } else {
                crate::dlls::decode_ansi(raw)
            };
            at = next;
            if folder >= 0xFFFD {
                return Err("cabinet réparti sur plusieurs disques : non pris en charge".into());
            }
            files.push(CabFile {
                name,
                folder,
                offset,
                size,
            });
        }
        Ok(Cabinet {
            data,
            folders,
            files,
            data_reserve,
        })
    }

    /// Décompresse un dossier entier du cabinet.
    fn folder(&self, index: usize, needed: usize) -> Result<Vec<u8>, String> {
        let f = self.folders.get(index).ok_or("dossier de cabinet absent")?;
        let bad = || "bloc de cabinet endommagé".to_string();
        let mut out: Vec<u8> = Vec::new();
        let mut at = f.start;
        let kind = f.compression & 0x0F;
        if kind == 2 {
            return Err("compression Quantum non prise en charge".into());
        }
        if kind > 3 {
            return Err("compression de cabinet inconnue".into());
        }
        let mut packed_all = Vec::new();
        let mut plain_total = 0usize;
        for _ in 0..f.blocks {
            if out.len() >= needed || plain_total >= needed {
                break;
            }
            let packed = u16_at(self.data, at + 4).ok_or_else(bad)? as usize;
            let plain = u16_at(self.data, at + 6).ok_or_else(bad)? as usize;
            let start = at + 8 + self.data_reserve;
            let block = self.data.get(start..start + packed).ok_or_else(bad)?;
            at = start + packed;
            match kind {
                0 => out.extend_from_slice(block),
                1 => {
                    if block.get(..2) != Some(b"CK") {
                        return Err(bad());
                    }
                    let history_from = out.len().saturating_sub(32 * 1024);
                    let mut inf = Inflater::with_history(&block[2..], &out[history_from..]);
                    let mut chunk = Vec::with_capacity(plain);
                    inf.read_to_end(&mut chunk).map_err(|e| e.to_string())?;
                    if chunk.len() != plain {
                        return Err(bad());
                    }
                    out.extend_from_slice(&chunk);
                }
                _ => {
                    packed_all.extend_from_slice(block);
                    plain_total += plain;
                }
            }
            if out.len() > MAX_FOLDER || plain_total > MAX_FOLDER {
                return Err("cabinet trop volumineux".into());
            }
        }
        if kind == 3 {
            let lzx = super::lzx::Lzx::new(((f.compression >> 8) & 0x1F) as u32)?;
            out = lzx.decompress(&packed_all, plain_total)?;
        }
        Ok(out)
    }

    /// Extrait chaque fichier et appelle `f(nom, contenu)`.
    pub fn extract(
        &self,
        f: &mut dyn FnMut(&str, &[u8]) -> Result<(), String>,
    ) -> Result<(), String> {
        for index in 0..self.folders.len() {
            let needed = self
                .files
                .iter()
                .filter(|c| c.folder == index)
                .map(|c| c.offset + c.size)
                .max()
                .unwrap_or(0);
            if needed == 0 {
                continue;
            }
            let data = self.folder(index, needed)?;
            for c in self.files.iter().filter(|c| c.folder == index) {
                let bytes = data
                    .get(c.offset..c.offset + c.size)
                    .ok_or("fichier de cabinet tronqué")?;
                f(&c.name, bytes)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Construit un cabinet MSZIP d'un seul dossier (pour les tests).
    pub fn build_mszip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut plain = Vec::new();
        let mut entries = Vec::new();
        for (name, data) in files {
            entries.push((name.to_string(), plain.len(), data.len()));
            plain.extend_from_slice(data);
        }
        // Blocs de 32 Ko compressés indépendamment, en blocs « stockés »
        // DEFLATE (valide en MSZIP) pour garder le constructeur simple.
        let mut blocks = Vec::new();
        for chunk in plain.chunks(32768) {
            let mut b = b"CK".to_vec();
            b.push(1);
            b.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
            b.extend_from_slice(&(!(chunk.len() as u16)).to_le_bytes());
            b.extend_from_slice(chunk);
            blocks.push((b, chunk.len()));
        }
        let mut file_table = Vec::new();
        for (name, offset, size) in &entries {
            file_table.extend_from_slice(&(*size as u32).to_le_bytes());
            file_table.extend_from_slice(&(*offset as u32).to_le_bytes());
            file_table.extend_from_slice(&0u16.to_le_bytes());
            file_table.extend_from_slice(&[0; 4]);
            file_table.extend_from_slice(&0x20u16.to_le_bytes());
            file_table.extend_from_slice(name.as_bytes());
            file_table.push(0);
        }
        let files_at = 36 + 8;
        let data_at = files_at + file_table.len();
        let mut out = b"MSCF".to_vec();
        out.extend_from_slice(&[0; 4]);
        let total = data_at + blocks.iter().map(|b| 8 + b.0.len()).sum::<usize>();
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(files_at as u32).to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&[3, 1]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&[0; 6]);
        out.extend_from_slice(&(data_at as u32).to_le_bytes());
        out.extend_from_slice(&(blocks.len() as u16).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&file_table);
        for (b, n) in blocks {
            out.extend_from_slice(&[0; 4]);
            out.extend_from_slice(&(b.len() as u16).to_le_bytes());
            out.extend_from_slice(&(n as u16).to_le_bytes());
            out.extend_from_slice(&b);
        }
        out
    }

    #[test]
    fn cabinet_mszip() {
        let big: Vec<u8> = (0..70000u32).map(|i| (i % 251) as u8).collect();
        let cab = build_mszip(&[("a.txt", b"bonjour"), ("big.bin", &big)]);
        let parsed = Cabinet::parse(&cab).unwrap();
        let mut got = Vec::new();
        parsed
            .extract(&mut |n, d| {
                got.push((n.to_string(), d.to_vec()));
                Ok(())
            })
            .unwrap();
        assert_eq!(got[0], ("a.txt".into(), b"bonjour".to_vec()));
        assert_eq!(got[1].1, big);
    }
}
