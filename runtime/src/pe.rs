//! Lecture du format PE32+ (exécutables Windows 64 bits).
//!
//! Écrit à partir de la spécification publique « PE Format » de Microsoft.
//! On ne lit que ce dont le chargeur a besoin ; tout est validé avant usage.

use std::fmt;

pub const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
pub const IMAGE_FILE_DLL: u16 = 0x2000;
pub const PE32PLUS_MAGIC: u16 = 0x20b;
pub const PE32_MAGIC: u16 = 0x10b;

pub const DIR_EXPORT: usize = 0;
pub const DIR_IMPORT: usize = 1;
pub const DIR_BASERELOC: usize = 5;
pub const DIR_TLS: usize = 9;
pub const DIR_DELAY_IMPORT: usize = 13;

pub const SCN_MEM_EXECUTE: u32 = 0x2000_0000;
pub const SCN_MEM_READ: u32 = 0x4000_0000;
pub const SCN_MEM_WRITE: u32 = 0x8000_0000;
pub const SCN_CNT_UNINITIALIZED: u32 = 0x0000_0080;

#[derive(Debug)]
pub enum PeError {
    TooSmall,
    NotMz,
    NotPe,
    Pe32(u16),
    Machine(u16),
    Bad(&'static str),
}

impl fmt::Display for PeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PeError::TooSmall => write!(f, "fichier trop petit pour être un exécutable Windows"),
            PeError::NotMz => write!(f, "pas d'en-tête MZ : ce n'est pas un exécutable Windows"),
            PeError::NotPe => write!(f, "signature PE absente (programme DOS 16 bits ?)"),
            PeError::Pe32(machine) => write!(f, "programme Windows 32 bits (machine 0x{machine:04x}) : seul le 64 bits est pris en charge pour l'instant"),
            PeError::Machine(m) => write!(f, "processeur cible non pris en charge (machine 0x{m:04x}, attendu x86-64)"),
            PeError::Bad(s) => write!(f, "en-tête PE invalide : {s}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DataDir {
    pub rva: u32,
    pub size: u32,
}

#[derive(Clone, Debug)]
pub struct Section {
    pub name: String,
    pub virtual_size: u32,
    pub rva: u32,
    pub raw_size: u32,
    pub raw_ptr: u32,
    pub characteristics: u32,
}

#[derive(Clone, Debug)]
pub struct PeImage {
    pub characteristics: u16,
    pub entry_rva: u32,
    pub image_base: u64,
    pub section_alignment: u32,
    pub size_of_image: u32,
    pub size_of_headers: u32,
    pub subsystem: u16,
    pub dll_characteristics: u16,
    pub stack_reserve: u64,
    pub stack_commit: u64,
    pub dirs: Vec<DataDir>,
    pub sections: Vec<Section>,
}

pub fn rd16(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}
pub fn rd32(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()))
}
pub fn rd64(b: &[u8], o: usize) -> Option<u64> {
    b.get(o..o + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap()))
}

impl PeImage {
    pub fn parse(file: &[u8]) -> Result<PeImage, PeError> {
        if file.len() < 0x40 {
            return Err(PeError::TooSmall);
        }
        if &file[0..2] != b"MZ" {
            return Err(PeError::NotMz);
        }
        let pe = rd32(file, 0x3c).ok_or(PeError::TooSmall)? as usize;
        if file.get(pe..pe + 4) != Some(b"PE\0\0") {
            return Err(PeError::NotPe);
        }
        let coff = pe + 4;
        let machine = rd16(file, coff).ok_or(PeError::TooSmall)?;
        let nsections = rd16(file, coff + 2).ok_or(PeError::TooSmall)? as usize;
        let opt_size = rd16(file, coff + 16).ok_or(PeError::TooSmall)? as usize;
        let characteristics = rd16(file, coff + 18).ok_or(PeError::TooSmall)?;
        let opt = coff + 20;
        let magic = rd16(file, opt).ok_or(PeError::TooSmall)?;
        if magic == PE32_MAGIC {
            return Err(PeError::Pe32(machine));
        }
        if magic != PE32PLUS_MAGIC {
            return Err(PeError::Bad("magic de l'en-tête optionnel inconnu"));
        }
        if machine != IMAGE_FILE_MACHINE_AMD64 {
            return Err(PeError::Machine(machine));
        }
        let g32 = |o: usize| rd32(file, opt + o).ok_or(PeError::Bad("en-tête optionnel tronqué"));
        let g64 = |o: usize| rd64(file, opt + o).ok_or(PeError::Bad("en-tête optionnel tronqué"));
        let g16 = |o: usize| rd16(file, opt + o).ok_or(PeError::Bad("en-tête optionnel tronqué"));
        let ndirs = (g32(108)? as usize).min(16);
        let mut dirs = vec![DataDir::default(); 16];
        for (i, d) in dirs.iter_mut().enumerate().take(ndirs) {
            d.rva = g32(112 + i * 8)?;
            d.size = g32(116 + i * 8)?;
        }
        let img = PeImage {
            characteristics,
            entry_rva: g32(16)?,
            image_base: g64(24)?,
            section_alignment: g32(32)?,
            size_of_image: g32(56)?,
            size_of_headers: g32(60)?,
            subsystem: g16(68)?,
            dll_characteristics: g16(70)?,
            stack_reserve: g64(72)?,
            stack_commit: g64(80)?,
            dirs,
            sections: {
                let mut v = Vec::with_capacity(nsections);
                let mut s = opt + opt_size;
                for _ in 0..nsections {
                    let h = file.get(s..s + 40).ok_or(PeError::Bad("table des sections tronquée"))?;
                    let name_end = h[..8].iter().position(|&c| c == 0).unwrap_or(8);
                    v.push(Section {
                        name: String::from_utf8_lossy(&h[..name_end]).into_owned(),
                        virtual_size: rd32(h, 8).unwrap(),
                        rva: rd32(h, 12).unwrap(),
                        raw_size: rd32(h, 16).unwrap(),
                        raw_ptr: rd32(h, 20).unwrap(),
                        characteristics: rd32(h, 36).unwrap(),
                    });
                    s += 40;
                }
                v
            },
        };
        if img.size_of_image == 0 || img.size_of_image > 0x8000_0000 {
            return Err(PeError::Bad("SizeOfImage incohérent"));
        }
        if img.size_of_headers as usize > file.len() || img.size_of_headers > img.size_of_image {
            return Err(PeError::Bad("SizeOfHeaders incohérent"));
        }
        for s in &img.sections {
            let vend = s.rva as u64 + s.virtual_size.max(s.raw_size) as u64;
            if vend > img.size_of_image as u64 {
                return Err(PeError::Bad("section hors de l'image"));
            }
            if s.raw_size > 0 && (s.raw_ptr as u64 + s.raw_size as u64) > file.len() as u64 {
                // Certains éditeurs de liens arrondissent la dernière section au-delà du fichier :
                // on tolère, la copie sera tronquée à la taille réelle.
            }
        }
        Ok(img)
    }

    pub fn dir(&self, i: usize) -> DataDir {
        self.dirs[i]
    }

    pub fn is_dll(&self) -> bool {
        self.characteristics & IMAGE_FILE_DLL != 0
    }

    pub fn subsystem_name(&self) -> &'static str {
        match self.subsystem {
            2 => "graphique (GUI)",
            3 => "console",
            _ => "autre",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejette_un_fichier_non_pe() {
        assert!(matches!(PeImage::parse(b"not an exe"), Err(PeError::TooSmall)));
        let mut b = vec![0u8; 0x80];
        b[..2].copy_from_slice(b"MZ");
        assert!(matches!(PeImage::parse(&b), Err(PeError::NotPe)));
    }

    #[test]
    fn lit_les_entiers_little_endian() {
        let b = [0x34, 0x12, 0x78, 0x56, 0xbc, 0x9a, 0x00, 0x00];
        assert_eq!(rd16(&b, 0), Some(0x1234));
        assert_eq!(rd32(&b, 0), Some(0x5678_1234));
        assert_eq!(rd64(&b, 0), Some(0x0000_9abc_5678_1234));
        assert_eq!(rd64(&b, 2), None);
    }
}
