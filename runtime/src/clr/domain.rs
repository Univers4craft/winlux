//! Domaine d'exécution : l'assembly du programme et celles qu'elle référence.
//!
//! Chaque `AssemblyRef` est cherchée dans le dossier de l'appli (`Nom.dll`,
//! puis `Nom.exe`), comme le fait le sondage d'un .NET Framework pour les
//! assemblies privées (dossier de base de l'application). Une référence non
//! trouvée reste servie par la bibliothèque intégrée (`bcl.rs`).
//!
//! Les assemblies de base (`mscorlib`…) viennent d'abord de la BCL Winlux,
//! écrite en C# et compilée à la construction (`bcl/`, étape 7). Elle est
//! cherchée dans `WINLUX_BCL_DIR`, sinon dans `/opt/winlux/bcl`. Ce qu'elle
//! ne définit pas encore reste servi par `bcl.rs`.

use std::path::{Path, PathBuf};

/// Nom (en minuscules) de l'assembly de base.
pub const CORLIB: &str = "mscorlib";

/// Assemblies fournies par la BCL Winlux plutôt que par le dossier de l'appli.
const BCL_ASSEMBLIES: [&str; 1] = [CORLIB];

/// Dossier de la BCL Winlux compilée, s'il existe.
fn bcl_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("WINLUX_BCL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/opt/winlux/bcl"));
    dir.is_dir().then_some(dir)
}

use super::metadata::Assembly;
use super::tables::TableId;
use super::types::TypeId;

/// Ce que le runtime retient d'une assembly chargée.
#[derive(Default, Clone)]
pub struct AsmInfo {
    /// Nom simple, en minuscules (comparaison sans casse, comme Windows).
    pub name: String,
    /// TypeDef → type enregistré.
    pub type_ids: Vec<Option<TypeId>>,
    /// MethodDef → TypeDef qui la déclare.
    pub method_owner: Vec<u32>,
    /// Field → TypeDef qui le déclare.
    pub field_owner: Vec<u32>,
}

impl AsmInfo {
    pub fn new(assembly: &Assembly, fallback: &str) -> AsmInfo {
        let name = if assembly.row_count(TableId::Assembly) > 0 {
            assembly
                .get_string(assembly.get_assembly(1).name)
                .to_string()
        } else {
            fallback.to_string()
        };
        let types = assembly.row_count(TableId::TypeDef);
        let mut method_owner = vec![0; assembly.row_count(TableId::MethodDef) as usize + 1];
        let mut field_owner = vec![0; assembly.row_count(TableId::Field) as usize + 1];
        for t in 1..=types {
            for m in assembly.type_methods(t) {
                if let Some(slot) = method_owner.get_mut(m as usize) {
                    *slot = t;
                }
            }
            for f in assembly.type_fields(t) {
                if let Some(slot) = field_owner.get_mut(f as usize) {
                    *slot = t;
                }
            }
        }
        AsmInfo {
            name: name.to_ascii_lowercase(),
            type_ids: vec![None; types as usize + 1],
            method_owner,
            field_owner,
        }
    }
}

/// Charge `main` puis, de proche en proche, les assemblies privées qu'elle
/// référence. L'indice 0 est toujours le programme.
pub fn load(main: Assembly, app_dir: Option<&Path>) -> (Vec<Assembly>, Vec<AsmInfo>) {
    let info = AsmInfo::new(&main, "programme");
    let mut assemblies = vec![main];
    let mut infos = vec![info];
    let mut next = 0;
    while next < assemblies.len() {
        let refs: Vec<String> = {
            let a = &assemblies[next];
            (1..=a.row_count(TableId::AssemblyRef))
                .map(|r| a.get_string(a.get_assembly_ref(r).name).to_string())
                .collect()
        };
        next += 1;
        for name in refs {
            let key = name.to_ascii_lowercase();
            if infos.iter().any(|i| i.name == key) {
                continue;
            }
            let from_bcl = BCL_ASSEMBLIES
                .contains(&key.as_str())
                .then(bcl_dir)
                .flatten()
                .and_then(|d| probe(&d, &name));
            let Some(path) = from_bcl.or_else(|| app_dir.and_then(|d| probe(d, &name))) else {
                continue;
            };
            match std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|data| Assembly::parse(data).map_err(|e| e.to_string()))
            {
                Ok(a) => {
                    crate::log_info!(
                        "clr",
                        "assembly « {name} » chargée depuis {}",
                        path.display()
                    );
                    let mut info = AsmInfo::new(&a, &name);
                    info.name = key;
                    assemblies.push(a);
                    infos.push(info);
                }
                Err(e) => crate::log_warn!("clr", "assembly « {name} » illisible : {e}"),
            }
        }
    }
    (assemblies, infos)
}

/// Fichier d'une assembly privée dans le dossier de l'appli, sans tenir
/// compte de la casse (les noms viennent d'un système de fichiers Windows).
fn probe(dir: &Path, name: &str) -> Option<std::path::PathBuf> {
    let wanted = [
        format!("{name}.dll").to_ascii_lowercase(),
        format!("{name}.exe").to_ascii_lowercase(),
    ];
    let entries: Vec<_> = std::fs::read_dir(dir).ok()?.flatten().collect();
    wanted.iter().find_map(|w| {
        entries
            .iter()
            .find(|e| e.file_name().to_string_lossy().to_ascii_lowercase() == *w)
            .map(|e| e.path())
            .filter(|p| p.is_file())
    })
}
