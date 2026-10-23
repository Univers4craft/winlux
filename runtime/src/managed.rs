//! Pont optionnel vers les runtimes .NET natifs Linux.
//!
//! Un apphost Windows moderne est un PE natif accompagné de `nom.dll` et de
//! `nom.runtimeconfig.json`. L'assembly compagnon peut être lancé directement
//! avec le pilote `dotnet` Linux. Les anciens PE contenant un en-tête CLR sont
//! confiés à Mono quand celui-ci est installé. Ce pont ne prétend pas fournir
//! WPF/WinForms : ces piles restent dépendantes d'API Windows supplémentaires.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

const PE32_MAGIC: u16 = 0x10b;
const PE32PLUS_MAGIC: u16 = 0x20b;
const COM_DESCRIPTOR: usize = 14;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManagedTarget {
    DotNet(PathBuf),
    Mono(PathBuf),
}

fn rd16(bytes: &[u8], offset: usize) -> Option<u16> {
    let value = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([value[0], value[1]]))
}

fn rd32(bytes: &[u8], offset: usize) -> Option<u32> {
    let value = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes(value.try_into().ok()?))
}

/// Détecte le répertoire IMAGE_DIRECTORY_ENTRY_COM_DESCRIPTOR dans un PE32
/// ou PE32+. Cette lecture précède volontairement le chargeur x86-64 afin de
/// reconnaître aussi les assemblages AnyCPU généralement marqués PE32.
pub fn has_clr_header(bytes: &[u8]) -> bool {
    if bytes.get(..2) != Some(b"MZ") { return false; }
    let pe = match rd32(bytes, 0x3c).and_then(|v| usize::try_from(v).ok()) {
        Some(value) => value,
        None => return false,
    };
    if bytes.get(pe..pe.saturating_add(4)) != Some(b"PE\0\0") { return false; }
    let coff = match pe.checked_add(4) { Some(value) => value, None => return false };
    let optional_size = match rd16(bytes, coff + 16) { Some(value) => value as usize, None => return false };
    let optional = match coff.checked_add(20) { Some(value) => value, None => return false };
    let magic = match rd16(bytes, optional) { Some(value) => value, None => return false };
    let (count_offset, directories_offset) = match magic {
        PE32_MAGIC => (92usize, 96usize),
        PE32PLUS_MAGIC => (108usize, 112usize),
        _ => return false,
    };
    let required = match directories_offset.checked_add((COM_DESCRIPTOR + 1) * 8) {
        Some(value) => value,
        None => return false,
    };
    if optional_size < required { return false; }
    let count = match rd32(bytes, optional + count_offset) { Some(value) => value as usize, None => return false };
    if count <= COM_DESCRIPTOR { return false; }
    let entry = optional + directories_offset + COM_DESCRIPTOR * 8;
    matches!((rd32(bytes, entry), rd32(bytes, entry + 4)), (Some(rva), Some(size)) if rva != 0 && size != 0)
}

fn sidecars(exe: &Path) -> (PathBuf, PathBuf) {
    let assembly = exe.with_extension("dll");
    let runtime_config = exe.with_extension("runtimeconfig.json");
    (assembly, runtime_config)
}

/// Choisit le runtime sans lancer de processus.
pub fn detect(exe: &Path, bytes: &[u8]) -> Option<ManagedTarget> {
    let (assembly, runtime_config) = sidecars(exe);
    if assembly.is_file() && runtime_config.is_file() {
        return Some(ManagedTarget::DotNet(assembly));
    }
    if has_clr_header(bytes) {
        if runtime_config.is_file() {
            Some(ManagedTarget::DotNet(exe.to_path_buf()))
        } else {
            Some(ManagedTarget::Mono(exe.to_path_buf()))
        }
    } else {
        None
    }
}

fn selected_runner(configured: Option<OsString>, fallback: &str) -> OsString {
    configured.filter(|value| !value.is_empty()).unwrap_or_else(|| fallback.into())
}

fn runner(variable: &str, fallback: &str) -> OsString {
    selected_runner(std::env::var_os(variable), fallback)
}

pub fn run(target: &ManagedTarget, arguments: &[String]) -> Result<ExitStatus, String> {
    let (program, image, runtime_name): (OsString, &Path, &str) = match target {
        ManagedTarget::DotNet(path) => (runner("WINLUX_DOTNET", "dotnet"), path, ".NET"),
        ManagedTarget::Mono(path) => (runner("WINLUX_MONO", "mono"), path, "Mono/.NET Framework"),
    };
    crate::trace!("pont géré {runtime_name} : {}", image.display());
    let mut command = Command::new(&program);
    command.arg(image).args(arguments);
    command.status().map_err(|error| {
        format!(
            "runtime {runtime_name} indisponible ({}) : {error}. Installez-le ou configurez {}",
            Path::new(&program).display(),
            if matches!(target, ManagedTarget::DotNet(_)) { "WINLUX_DOTNET" } else { "WINLUX_MONO" },
        )
    })
}

pub fn description(target: &ManagedTarget) -> &'static str {
    match target {
        ManagedTarget::DotNet(_) => "application .NET moderne",
        ManagedTarget::Mono(_) => "assembly .NET Framework/AnyCPU",
    }
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
    fn choisit_mono_pour_un_ancien_assembly() {
        let path = Path::new("ancien.exe");
        assert_eq!(detect(path, &managed_pe(PE32_MAGIC)), Some(ManagedTarget::Mono(path.into())));
    }

    #[test]
    fn la_surcharge_de_runtime_conserve_un_chemin_avec_espaces() {
        let configured = Some(OsString::from("/opt/dot net/dotnet"));
        assert_eq!(selected_runner(configured, "dotnet"), OsString::from("/opt/dot net/dotnet"));
    }
}
