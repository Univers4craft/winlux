//! Détection des programmes .NET, exécutés par le CLR intégré (`crate::clr`).
//!
//! Deux formes existent : un PE contenant un en-tête CLR (.NET Framework,
//! AnyCPU), et l'« apphost » d'un .NET moderne, un petit PE natif accompagné de
//! `nom.dll` (l'assembly) et de `nom.runtimeconfig.json`.

use std::path::Path;

const PE32_MAGIC: u16 = 0x10b;
const PE32PLUS_MAGIC: u16 = 0x20b;
const COM_DESCRIPTOR: usize = 14;

use crate::pe::{rd16, rd32};

/// Détecte le répertoire IMAGE_DIRECTORY_ENTRY_COM_DESCRIPTOR dans un PE32
/// ou PE32+. Cette lecture précède volontairement le chargeur x86-64 afin de
/// reconnaître aussi les assemblages AnyCPU généralement marqués PE32.
pub fn has_clr_header(bytes: &[u8]) -> bool {
    if bytes.get(..2) != Some(b"MZ") {
        return false;
    }
    let pe = match rd32(bytes, 0x3c).and_then(|v| usize::try_from(v).ok()) {
        Some(value) => value,
        None => return false,
    };
    if bytes.get(pe..pe.saturating_add(4)) != Some(b"PE\0\0") {
        return false;
    }
    let coff = match pe.checked_add(4) {
        Some(value) => value,
        None => return false,
    };
    let optional_size = match rd16(bytes, coff + 16) {
        Some(value) => value as usize,
        None => return false,
    };
    let optional = match coff.checked_add(20) {
        Some(value) => value,
        None => return false,
    };
    let magic = match rd16(bytes, optional) {
        Some(value) => value,
        None => return false,
    };
    let (count_offset, directories_offset) = match magic {
        PE32_MAGIC => (92usize, 96usize),
        PE32PLUS_MAGIC => (108usize, 112usize),
        _ => return false,
    };
    let required = match directories_offset.checked_add((COM_DESCRIPTOR + 1) * 8) {
        Some(value) => value,
        None => return false,
    };
    if optional_size < required {
        return false;
    }
    let count = match rd32(bytes, optional + count_offset) {
        Some(value) => value as usize,
        None => return false,
    };
    if count <= COM_DESCRIPTOR {
        return false;
    }
    let entry = optional + directories_offset + COM_DESCRIPTOR * 8;
    matches!((rd32(bytes, entry), rd32(bytes, entry + 4)), (Some(rva), Some(size)) if rva != 0 && size != 0)
}

/// Image .NET à exécuter pour `exe` : le fichier lui-même s'il porte un
/// en-tête CLR, sinon l'assembly compagnon d'un apphost.
pub fn managed_image(exe: &Path, bytes: &[u8]) -> Option<Vec<u8>> {
    if has_clr_header(bytes) {
        return Some(bytes.to_vec());
    }
    if !exe.with_extension("runtimeconfig.json").is_file() {
        return None;
    }
    let assembly = std::fs::read(exe.with_extension("dll")).ok()?;
    has_clr_header(&assembly).then_some(assembly)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn managed_pe(magic: u16) -> Vec<u8> {
        let pe = 0x80usize;
        let optional = pe + 24;
        let directories = if magic == PE32_MAGIC { 96 } else { 112 };
        let count = if magic == PE32_MAGIC { 92 } else { 108 };
        let mut bytes = vec![0u8; optional + directories + 16 * 8];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&(pe as u32).to_le_bytes());
        bytes[pe..pe + 4].copy_from_slice(b"PE\0\0");
        bytes[pe + 20..pe + 22].copy_from_slice(&((directories + 16 * 8) as u16).to_le_bytes());
        bytes[optional..optional + 2].copy_from_slice(&magic.to_le_bytes());
        bytes[optional + count..optional + count + 4].copy_from_slice(&16u32.to_le_bytes());
        let clr = optional + directories + COM_DESCRIPTOR * 8;
        bytes[clr..clr + 4].copy_from_slice(&0x2000u32.to_le_bytes());
        bytes[clr + 4..clr + 8].copy_from_slice(&0x48u32.to_le_bytes());
        bytes
    }

    #[test]
    fn detecte_les_assemblages_clr_pe32_et_pe32plus() {
        assert!(has_clr_header(&managed_pe(PE32_MAGIC)));
        assert!(has_clr_header(&managed_pe(PE32PLUS_MAGIC)));
        assert!(!has_clr_header(b"pas un PE"));
        let mut native = managed_pe(PE32PLUS_MAGIC);
        let optional = 0x80 + 24;
        let clr = optional + 112 + COM_DESCRIPTOR * 8;
        native[clr..clr + 8].fill(0);
        assert!(!has_clr_header(&native));
    }

    #[test]
    fn un_apphost_execute_son_assembly_compagnon() {
        let dir = std::env::temp_dir().join(format!("winlux-apphost-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("app.exe");
        std::fs::write(dir.join("app.dll"), managed_pe(PE32_MAGIC)).unwrap();
        assert!(managed_image(&exe, b"MZ natif").is_none());
        std::fs::write(dir.join("app.runtimeconfig.json"), "{}").unwrap();
        assert!(managed_image(&exe, b"MZ natif").is_some());
        assert!(managed_image(&exe, &managed_pe(PE32PLUS_MAGIC)).is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
