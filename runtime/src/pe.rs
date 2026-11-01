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
pub const DIR_COM_DESCRIPTOR: usize = 14;
const COMIMAGE_FLAGS_ILONLY: u32 = 1;

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
    /// Assembly .NET « IL seul » au format PE32 (AnyCPU) : comme sous
    /// Windows 64 bits, elle se charge en tant qu'image mais sans imports,
    /// relocations ni point d'entrée ; c'est le CLR qui la lit.
    pub il_only32: bool,
}

pub fn rd16(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}
pub fn rd32(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
}
pub fn rd64(b: &[u8], o: usize) -> Option<u64> {
    b.get(o..o + 8)
        .map(|s| u64::from_le_bytes(s.try_into().unwrap()))
}

/// Lit le champ Subsystem de l'en-tête optionnel sans valider l'image.
/// Le champ est au même décalage en PE32 et PE32+, ce qui permet de classer
/// aussi les assemblies .NET AnyCPU avant de choisir comment les lancer.
pub fn peek_subsystem(file: &[u8]) -> Option<u16> {
    if file.get(..2) != Some(b"MZ") {
        return None;
    }
    let pe = rd32(file, 0x3c)? as usize;
    if file.get(pe..pe.checked_add(4)?) != Some(b"PE\0\0") {
        return None;
    }
    rd16(file, pe.checked_add(24 + 68)?)
}

impl PeImage {
    pub fn parse(file: &[u8]) -> Result<PeImage, PeError> {
        Self::parse_with(file, false)
    }

    /// Comme `parse`, en acceptant aussi une assembly .NET IL seul au
    /// format PE32, que `LoadLibraryEx` charge dans un processus 64 bits.
    pub fn parse_il_only(file: &[u8]) -> Result<PeImage, PeError> {
        Self::parse_with(file, true)
    }

    fn parse_with(file: &[u8], il_only32: bool) -> Result<PeImage, PeError> {
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
        let g32 = |o: usize| rd32(file, opt + o).ok_or(PeError::Bad("en-tête optionnel tronqué"));
        let g64 = |o: usize| rd64(file, opt + o).ok_or(PeError::Bad("en-tête optionnel tronqué"));
        let g16 = |o: usize| rd16(file, opt + o).ok_or(PeError::Bad("en-tête optionnel tronqué"));
        let pe32 = magic == PE32_MAGIC;
        if pe32 && !il_only32 {
            return Err(PeError::Pe32(machine));
        }
        if !pe32 && magic != PE32PLUS_MAGIC {
            return Err(PeError::Bad("magic de l'en-tête optionnel inconnu"));
        }
        if !pe32 && machine != IMAGE_FILE_MACHINE_AMD64 {
            return Err(PeError::Machine(machine));
        }
        // PE32 : champs de 32 bits pour ImageBase et la pile, répertoires 16 octets plus tôt.
        let (dirs_at, wide) = if pe32 { (92, 0) } else { (108, 8) };
        let ndirs = (g32(dirs_at)? as usize).min(16);
        let mut dirs = vec![DataDir::default(); 16];
        for (i, d) in dirs.iter_mut().enumerate().take(ndirs) {
            d.rva = g32(dirs_at + 4 + i * 8)?;
            d.size = g32(dirs_at + 8 + i * 8)?;
        }
        if pe32 {
            let cor = dirs[DIR_COM_DESCRIPTOR];
            if cor.rva == 0 {
                return Err(PeError::Pe32(machine));
            }
        }
        let img = PeImage {
            characteristics,
            entry_rva: g32(16)?,
            image_base: if pe32 { g32(28)? as u64 } else { g64(24)? },
            section_alignment: g32(32)?,
            size_of_image: g32(56)?,
            size_of_headers: g32(60)?,
            subsystem: g16(68)?,
            dll_characteristics: g16(70)?,
            stack_reserve: if pe32 { g32(72)? as u64 } else { g64(72)? },
            stack_commit: if pe32 {
                g32(76)? as u64
            } else {
                g64(72 + wide)?
            },
            dirs,
            il_only32: pe32,
            sections: {
                let mut v = Vec::with_capacity(nsections);
                let mut s = opt + opt_size;
                for _ in 0..nsections {
                    let h = file
                        .get(s..s + 40)
                        .ok_or(PeError::Bad("table des sections tronquée"))?;
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
        if img.il_only32
            && !img
                .cor_flags(file)
                .is_some_and(|f| f & COMIMAGE_FLAGS_ILONLY != 0)
        {
            return Err(PeError::Pe32(machine));
        }
        Ok(img)
    }

    /// Drapeaux de l'en-tête CLR (IMAGE_COR20_HEADER.Flags), lus dans le fichier.
    fn cor_flags(&self, file: &[u8]) -> Option<u32> {
        let cor = self.dirs[DIR_COM_DESCRIPTOR];
        let s = self
            .sections
            .iter()
            .find(|s| cor.rva >= s.rva && cor.rva < s.rva + s.virtual_size.max(s.raw_size))?;
        let off = (s.raw_ptr + (cor.rva - s.rva)) as usize;
        rd32(file, off.checked_add(16)?)
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
        assert!(matches!(
            PeImage::parse(b"not an exe"),
            Err(PeError::TooSmall)
        ));
        let mut b = vec![0u8; 0x80];
        b[..2].copy_from_slice(b"MZ");
        assert!(matches!(PeImage::parse(&b), Err(PeError::NotPe)));
    }

    #[test]
    fn lit_le_sous_systeme_sans_charger() {
        let mut b = vec![0u8; 0x200];
        b[..2].copy_from_slice(b"MZ");
        b[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        b[0x80 + 24 + 68] = 2;
        assert_eq!(peek_subsystem(&b), Some(2));
        assert_eq!(peek_subsystem(&b[..0x90]), None);
        assert_eq!(peek_subsystem(b"MZ"), None);
    }

    /// Assembly .NET AnyCPU minimale : PE32, une section, en-tête CLR.
    fn il_only_image(cor_flags: u32) -> Vec<u8> {
        let mut b = vec![0u8; 0x400];
        b[..2].copy_from_slice(b"MZ");
        b[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        let coff = 0x84;
        b[coff..coff + 2].copy_from_slice(&0x14cu16.to_le_bytes());
        b[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes());
        b[coff + 16..coff + 18].copy_from_slice(&224u16.to_le_bytes());
        b[coff + 18..coff + 20].copy_from_slice(&0x2102u16.to_le_bytes());
        let opt = coff + 20;
        let put = |b: &mut Vec<u8>, o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        b[opt..opt + 2].copy_from_slice(&PE32_MAGIC.to_le_bytes());
        put(&mut b, opt + 16, 0x2000); // point d'entrée (stub mscoree)
        put(&mut b, opt + 28, 0x40_0000); // ImageBase 32 bits
        put(&mut b, opt + 32, 0x2000);
        put(&mut b, opt + 56, 0x4000);
        put(&mut b, opt + 60, 0x200);
        put(&mut b, opt + 72, 0x10_0000);
        put(&mut b, opt + 76, 0x1000);
        put(&mut b, opt + 92, 16);
        put(&mut b, opt + 96 + 14 * 8, 0x2008); // en-tête CLR
        put(&mut b, opt + 100 + 14 * 8, 72);
        let sec = opt + 224;
        b[sec..sec + 5].copy_from_slice(b".text");
        put(&mut b, sec + 8, 0x200);
        put(&mut b, sec + 12, 0x2000);
        put(&mut b, sec + 16, 0x200);
        put(&mut b, sec + 20, 0x200);
        put(&mut b, sec + 36, 0x6000_0020);
        put(&mut b, 0x200 + 8, 72);
        put(&mut b, 0x200 + 8 + 16, cor_flags);
        b
    }

    #[test]
    fn accepte_une_assembly_il_seul_32_bits_pour_le_clr() {
        let b = il_only_image(1);
        // Lancée comme programme : refusée (32 bits) ; chargée par LoadLibrary : acceptée.
        assert!(matches!(PeImage::parse(&b), Err(PeError::Pe32(0x14c))));
        let img = PeImage::parse_il_only(&b).unwrap();
        assert!(img.il_only32);
        assert_eq!(img.image_base, 0x40_0000);
        assert_eq!(img.stack_reserve, 0x10_0000);
        assert_eq!(img.stack_commit, 0x1000);
        assert_eq!(img.dir(DIR_COM_DESCRIPTOR).rva, 0x2008);
        assert_eq!(img.sections.len(), 1);
        // Une image PE32 avec du code natif (pas « IL seul ») reste refusée.
        assert!(matches!(
            PeImage::parse_il_only(&il_only_image(0)),
            Err(PeError::Pe32(0x14c))
        ));
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
