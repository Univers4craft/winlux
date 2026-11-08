//! Runtime .NET intégré de Winlux : chargement d'une assembly, interpréteur
//! CIL, ramasse-miettes, BCL minimale et pont P/Invoke vers les DLL Win32
//! intégrées. Aucun runtime .NET extérieur n'est utilisé.

pub mod bcl;
pub mod domain;
#[cfg(test)]
pub mod emit;
pub mod exceptions;
pub mod gc;
pub mod il;
pub mod metadata;
pub mod opcodes;
pub mod pinvoke;
pub mod resolve;
pub mod tables;
pub mod types;
pub mod typesys;

use il::{ExecResult, Interpreter, Value};
use metadata::Assembly;
use tables::{CodedIndex, TableId, Token};
use types::TypeId;

/// Exécute le point d'entrée d'une assembly .NET et renvoie le code de
/// sortie. Les assemblies privées référencées sont cherchées dans `app_dir`,
/// le dossier du programme.
pub fn execute_assembly_in(pe_data: &[u8], app_dir: Option<&std::path::Path>) -> i32 {
    types::init();
    gc::init();
    crate::dlls::kernel32::init();

    let assembly = match Assembly::parse(pe_data.to_vec()) {
        Ok(a) => a,
        Err(e) => {
            crate::log_error!("clr", "assembly .NET illisible : {}", e);
            return 1;
        }
    };
    crate::log_info!(
        "clr",
        "assembly chargée : {} types, {} méthodes",
        assembly.row_count(TableId::TypeDef),
        assembly.row_count(TableId::MethodDef)
    );

    let entry = assembly.entry_point_token;
    if Token(entry).table() != TableId::MethodDef as u8 || Token(entry).rid() == 0 {
        crate::log_error!("clr", "point d'entrée absent ou invalide ({entry:#010x})");
        return 1;
    }

    let (assemblies, mut infos) = domain::load(assembly, app_dir);
    // Les assemblies référencées d'abord : une classe du programme qui dérive
    // d'une classe d'une bibliothèque y trouve son parent déjà enregistré.
    for idx in (0..assemblies.len()).rev() {
        load_types(&assemblies[idx], idx, &mut infos[idx]);
    }
    let mut interp = Interpreter::new(assemblies, infos);
    interp.link_types();

    let row = Token(entry).rid();
    let sig = interp.assembly.get_method_def(row).signature;
    let takes_args = !metadata::decode_method_sig(interp.assembly.get_blob(sig))
        .param_types
        .is_empty();
    let args = if takes_args {
        vec![bcl::alloc_string_array(
            &std::env::args().skip(2).collect::<Vec<_>>(),
        )]
    } else {
        Vec::new()
    };

    let code = match interp.execute_method(row, args) {
        ExecResult::Return(Value::I32(code)) => code,
        ExecResult::Return(_) => 0,
        ExecResult::Exception(exc) => unhandled(&exc),
    };
    let _ = std::io::Write::flush(&mut std::io::stdout());
    code
}

fn unhandled(exc: &exceptions::ClrException) -> i32 {
    let _ = std::io::Write::flush(&mut std::io::stdout());
    eprintln!("Unhandled exception. {exc}");
    crate::log_error!("clr", "exception non gérée : {exc}");
    // Code de sortie de .NET pour une exception non gérée (0xE0434352).
    0xE0434352u32 as i32
}

/// Inscrit les types de l'assembly (noms, définition, méthodes). Parents,
/// disposition des champs et instanciations génériques sont calculés par
/// l'interpréteur, qui voit toutes les assemblies du domaine.
fn load_types(assembly: &Assembly, asm_index: usize, info: &mut domain::AsmInfo) {
    let count = assembly.row_count(TableId::TypeDef);
    let mut ids: Vec<Option<TypeId>> = vec![None; count as usize + 1];
    let mut generic_params = vec![0u32; count as usize + 1];
    for g in 1..=assembly.row_count(TableId::GenericParam) {
        let (table, owner) =
            CodedIndex::TypeOrMethodDef.decode(assembly.get_generic_param(g).owner);
        if table == Some(TableId::TypeDef) {
            if let Some(n) = generic_params.get_mut(owner as usize) {
                *n += 1;
            }
        }
    }

    for row in 1..=count {
        let td = assembly.get_type_def(row);
        let name = assembly.get_string(td.name);
        if name == "<Module>" {
            continue;
        }
        let methods = assembly
            .type_methods(row)
            .map(|m| {
                let md = assembly.get_method_def(m);
                types::MethodInfo {
                    name: assembly.get_string(md.name).to_string(),
                    assembly: asm_index,
                    token: Token::new(TableId::MethodDef, m).0,
                    is_virtual: md.is_virtual(),
                    param_count: metadata::decode_method_sig(assembly.get_blob(md.signature))
                        .param_types
                        .len(),
                }
            })
            .collect();
        let info = types::TypeInfo {
            name: name.to_string(),
            namespace: assembly.get_string(td.namespace).to_string(),
            parent: types::TYPE_OBJECT,
            methods,
            def: Some((asm_index, row)),
            generic_params: generic_params[row as usize],
            is_interface: td.is_interface(),
            ..types::TypeInfo::default()
        };
        ids[row as usize] = Some(types::with_types(|reg| reg.register(info)));
    }
    crate::log_debug!("clr", "{} types enregistrés", ids.iter().flatten().count());
    info.type_ids = ids;
}

#[cfg(test)]
mod tests;
