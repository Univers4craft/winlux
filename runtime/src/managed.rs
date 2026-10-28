//! Pont optionnel vers les runtimes .NET natifs Linux.
//!
//! Un apphost Windows moderne est un PE natif accompagné de `nom.dll` et de
//! `nom.runtimeconfig.json`. L'assembly compagnon peut être lancé directement
//! avec le pilote `dotnet` Linux. Les anciens PE contenant un en-tête CLR sont
//! confiés à Mono quand celui-ci est installé. Ce pont ne prétend pas fournir
//! WPF/WinForms : ces piles restent dépendantes d'API Windows supplémentaires.
//!
//! Améliorations v2 :
//! - Détection de la version du Framework (.NET 2.0, 3.5, 4.x) via métadonnées CLR
//! - Support des assemblies mixtes (managed + native) avec détection IL-only
//! - Lecture du fichier app.config / exe.config pour Mono
//! - Résolution automatique des assemblies compagnons dans le même répertoire
//! - Variables d'environnement supplémentaires pour configurer Mono / dotnet

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

const PE32_MAGIC: u16 = 0x10b;
const PE32PLUS_MAGIC: u16 = 0x20b;
const COM_DESCRIPTOR: usize = 14;
/// Flag du CLR header : COMIMAGE_FLAGS_ILONLY
const COMIMAGE_FLAGS_ILONLY: u32 = 0x01;
/// Flag du CLR header : COMIMAGE_FLAGS_32BITREQUIRED
const COMIMAGE_FLAGS_32BITREQUIRED: u32 = 0x02;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManagedTarget {
    DotNet(PathBuf),
    Mono(PathBuf),
}

/// Informations extraites de l'en-tête CLR et des métadonnées.
#[derive(Clone, Debug, Default)]
pub struct ClrInfo {
    /// Version du runtime requise (ex: "v4.0.30319", "v2.0.50727")
    pub runtime_version: String,
    /// Flags CLR (IL-only, 32-bit required, etc.)
    pub flags: u32,
    /// Est-ce un assembly purement IL (pas de code natif mélangé)?
    pub il_only: bool,
    /// Est-ce un assembly qui exige le mode 32 bits?
    pub requires_32bit: bool,
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

/// Extrait les informations CLR détaillées d'un PE (version runtime, flags).
pub fn clr_info(bytes: &[u8]) -> Option<ClrInfo> {
    if bytes.get(..2) != Some(b"MZ") {
        return None;
    }
    let pe = rd32(bytes, 0x3c)? as usize;
    if bytes.get(pe..pe + 4) != Some(b"PE\0\0") {
        return None;
    }
    let coff = pe + 4;
    let optional = coff + 20;
    let magic = rd16(bytes, optional)?;
    let (count_off, dir_off) = match magic {
        PE32_MAGIC => (92, 96),
        PE32PLUS_MAGIC => (108, 112),
        _ => return None,
    };
    let count = rd32(bytes, optional + count_off)? as usize;
    if count <= COM_DESCRIPTOR {
        return None;
    }
    let entry = optional + dir_off + COM_DESCRIPTOR * 8;
    let clr_rva = rd32(bytes, entry)? as usize;
    let clr_size = rd32(bytes, entry + 4)? as usize;
    if clr_rva == 0 || clr_size == 0 {
        return None;
    }

    // Trouver l'offset fichier du CLR header via les sections
    let num_sections = rd16(bytes, coff + 2)? as usize;
    let optional_size = rd16(bytes, coff + 16)? as usize;
    let sections_start = optional + optional_size;
    let clr_file_offset = rva_to_file_offset(bytes, sections_start, num_sections, clr_rva)?;

    // Lire le CLR header (IMAGE_COR20_HEADER)
    if clr_file_offset + 16 > bytes.len() {
        return None;
    }
    let _cb = rd32(bytes, clr_file_offset)?;
    let major = rd16(bytes, clr_file_offset + 4)?;
    let minor = rd16(bytes, clr_file_offset + 6)?;
    let flags = rd32(bytes, clr_file_offset + 16)?;

    // Lire la version du runtime depuis le Metadata header
    let meta_rva = rd32(bytes, clr_file_offset + 8)? as usize;
    let runtime_version = if meta_rva != 0 {
        if let Some(meta_off) = rva_to_file_offset(bytes, sections_start, num_sections, meta_rva) {
            read_metadata_version(bytes, meta_off)
        } else {
            format!("v{major}.{minor}")
        }
    } else {
        format!("v{major}.{minor}")
    };

    Some(ClrInfo {
        runtime_version,
        flags,
        il_only: flags & COMIMAGE_FLAGS_ILONLY != 0,
        requires_32bit: flags & COMIMAGE_FLAGS_32BITREQUIRED != 0,
    })
}

/// Convertit une RVA en offset fichier via la table des sections PE.
fn rva_to_file_offset(
    bytes: &[u8],
    sections_start: usize,
    num_sections: usize,
    rva: usize,
) -> Option<usize> {
    for i in 0..num_sections {
        let sec = sections_start + i * 40;
        if sec + 40 > bytes.len() {
            break;
        }
        let virt_size = rd32(bytes, sec + 8)? as usize;
        let virt_addr = rd32(bytes, sec + 12)? as usize;
        let raw_offset = rd32(bytes, sec + 20)? as usize;
        if rva >= virt_addr && rva < virt_addr + virt_size {
            return Some(raw_offset + (rva - virt_addr));
        }
    }
    None
}

/// Lit la chaîne de version dans l'en-tête des métadonnées CLI (signature BSJB).
fn read_metadata_version(bytes: &[u8], offset: usize) -> String {
    // Le metadata header commence par la signature 0x424A5342 ("BSJB")
    if offset + 16 > bytes.len() {
        return String::new();
    }
    let sig = rd32(bytes, offset).unwrap_or(0);
    if sig != 0x424A5342 {
        return String::new();
    }
    let version_len = rd32(bytes, offset + 12).unwrap_or(0) as usize;
    if version_len == 0 || offset + 16 + version_len > bytes.len() {
        return String::new();
    }
    let version_bytes = &bytes[offset + 16..offset + 16 + version_len];
    let end = version_bytes
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(version_bytes.len());
    String::from_utf8_lossy(&version_bytes[..end]).into_owned()
}

/// Détermine si un Framework donné est "ancien" (2.0, 3.x) ou "moderne" (4.x+).
pub fn is_legacy_framework(version: &str) -> bool {
    // v2.0.50727, v1.1.4322, etc.
    if let Some(rest) = version.strip_prefix('v') {
        if let Some(major_str) = rest.split('.').next() {
            if let Ok(major) = major_str.parse::<u32>() {
                return major < 4;
            }
        }
    }
    false
}

fn sidecars(exe: &Path) -> (PathBuf, PathBuf) {
    let assembly = exe.with_extension("dll");
    let runtime_config = exe.with_extension("runtimeconfig.json");
    (assembly, runtime_config)
}

/// Cherche les assemblies compagnons (dll de même nom dans le même dossier).
#[allow(dead_code)]
fn find_companion_assemblies(exe: &Path) -> Vec<PathBuf> {
    let parent = match exe.parent() {
        Some(p) => p,
        None => return vec![],
    };
    let stem = match exe.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => return vec![],
    };
    let mut companions = Vec::new();
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("dll") {
                if let Some(fstem) = path.file_stem().and_then(|s| s.to_str()) {
                    // Collecter les DLL qui ont le même préfixe (assemblies dépendantes)
                    if fstem == stem || fstem.starts_with(&format!("{stem}.")) {
                        companions.push(path);
                    }
                }
            }
        }
    }
    companions
}

/// Choisit le runtime sans lancer de processus.
pub fn detect(exe: &Path, bytes: &[u8]) -> Option<ManagedTarget> {
    let (assembly, runtime_config) = sidecars(exe);

    // Cas 1 : apphost .NET moderne (assembly.dll + runtimeconfig.json)
    if assembly.is_file() && runtime_config.is_file() {
        return Some(ManagedTarget::DotNet(assembly));
    }

    // Cas 2 : PE avec en-tête CLR
    if has_clr_header(bytes) {
        if runtime_config.is_file() {
            // runtimeconfig.json présent → .NET moderne
            Some(ManagedTarget::DotNet(exe.to_path_buf()))
        } else {
            // Pas de runtimeconfig → c'est un assembly .NET Framework (Mono)
            // Vérifier si dotnet peut quand même le charger (Framework >= 4.0 avec compat)
            let info = clr_info(bytes);
            if let Some(ref info) = info {
                crate::trace!(
                    "CLR détecté : version={}, flags={:#x}, IL-only={}, 32bit={}",
                    info.runtime_version,
                    info.flags,
                    info.il_only,
                    info.requires_32bit
                );
                // Les assemblies mixtes (non IL-only) nécessitent Mono, car ils
                // contiennent du code natif x86/x64 Windows qu'on ne peut pas exécuter.
                if !info.il_only {
                    crate::trace!("assembly mixte (managed+native) : sera confié à Mono");
                }
            }
            Some(ManagedTarget::Mono(exe.to_path_buf()))
        }
    } else {
        None
    }
}

fn selected_runner(configured: Option<OsString>, fallback: &str) -> OsString {
    configured
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.into())
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

    match target {
        ManagedTarget::Mono(path) => {
            // Configurer Mono avec les bons chemins et options
            // --runtime=v4.0 si nécessaire (sinon Mono choisit automatiquement)
            if let Ok(bytes) = std::fs::read(path) {
                if let Some(info) = clr_info(&bytes) {
                    if !is_legacy_framework(&info.runtime_version) {
                        command.arg("--runtime=v4.0");
                    }
                }
            }

            // Chercher un fichier app.config / exe.config
            let config = path.with_extension("exe.config");
            if config.is_file() {
                command.env("MONO_CONFIG", &config);
                crate::trace!("configuration Mono : {}", config.display());
            }

            // Ajouter le répertoire de l'exe au MONO_PATH pour la résolution d'assemblies
            if let Some(parent) = path.parent() {
                let existing = std::env::var("MONO_PATH").unwrap_or_default();
                let new_path = if existing.is_empty() {
                    parent.to_string_lossy().into_owned()
                } else {
                    format!("{}:{existing}", parent.display())
                };
                command.env("MONO_PATH", new_path);
            }

            command.arg(image).args(arguments);
        }
        ManagedTarget::DotNet(path) => {
            // Pour dotnet, configurer DOTNET_ADDITIONAL_DEPS si nécessaire
            if let Some(parent) = path.parent() {
                // Chercher des runtimeconfig.dev.json pour les dépendances additionnelles
                let dev_config = path.with_extension("runtimeconfig.dev.json");
                if dev_config.is_file() {
                    crate::trace!("config dev trouvée : {}", dev_config.display());
                }
                // Ajouter le répertoire au probing path
                command.env("DOTNET_ADDITIONAL_PROBING_PATHS", parent);
            }
            command.arg(image).args(arguments);
        }
    }

    command.status().map_err(|error| {
        format!(
            "runtime {runtime_name} indisponible ({}) : {error}. Installez-le ou configurez {}",
            Path::new(&program).display(),
            if matches!(target, ManagedTarget::DotNet(_)) {
                "WINLUX_DOTNET"
            } else {
                "WINLUX_MONO"
            },
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
        assert_eq!(
            detect(path, &managed_pe(PE32_MAGIC)),
            Some(ManagedTarget::Mono(path.into()))
        );
    }

    #[test]
    fn la_surcharge_de_runtime_conserve_un_chemin_avec_espaces() {
        let configured = Some(OsString::from("/opt/dot net/dotnet"));
        assert_eq!(
            selected_runner(configured, "dotnet"),
            OsString::from("/opt/dot net/dotnet")
        );
    }
}
