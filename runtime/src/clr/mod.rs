//! Runtime .NET intégré de Winlux : chargement d'une assembly, interpréteur
//! CIL, ramasse-miettes, BCL minimale et pont P/Invoke vers les DLL Win32
//! intégrées. Aucun runtime .NET extérieur n'est utilisé.

pub mod bcl;
pub mod exceptions;
pub mod gc;
pub mod il;
pub mod metadata;
pub mod opcodes;
pub mod pinvoke;
pub mod tables;
pub mod types;

use il::{ExecResult, Interpreter, Value};
use metadata::Assembly;
use tables::{CodedIndex, TableId, Token};
use types::TypeId;

/// Exécute le point d'entrée d'une assembly .NET et renvoie le code de sortie.
pub fn execute_assembly(pe_data: &[u8]) -> i32 {
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

    load_types(&assembly);
    let mut interp = Interpreter::new(assembly);
    if let Err(exc) = run_cctors(&mut interp) {
        return unhandled(&exc);
    }

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

/// Enregistre les types de l'assembly : parents, méthodes (pour la
/// répartition virtuelle) et disposition des champs d'instance.
fn load_types(assembly: &Assembly) {
    let count = assembly.row_count(TableId::TypeDef);
    let mut ids: Vec<Option<TypeId>> = vec![None; count as usize + 1];

    for row in 1..=count {
        let td = assembly.get_type_def(row);
        let name = assembly.get_string(td.name);
        if name == "<Module>" {
            continue;
        }
        let info = types::TypeInfo {
            name: name.to_string(),
            namespace: assembly.get_string(td.namespace).to_string(),
            parent: types::TYPE_OBJECT,
            fields: Vec::new(),
            methods: Vec::new(),
            instance_size: 0,
        };
        ids[row as usize] = Some(types::with_types(|reg| reg.register(info)));
    }

    for row in 1..=count {
        let Some(id) = ids[row as usize] else {
            continue;
        };
        let td = assembly.get_type_def(row);
        let parent = resolve_type(assembly, td.extends, &ids).unwrap_or(0);
        let methods = assembly
            .type_methods(row)
            .map(|m| {
                let md = assembly.get_method_def(m);
                types::MethodInfo {
                    name: assembly.get_string(md.name).to_string(),
                    token: Token::new(TableId::MethodDef, m).0,
                    is_virtual: md.is_virtual(),
                    param_count: metadata::decode_method_sig(assembly.get_blob(md.signature))
                        .param_types
                        .len(),
                }
            })
            .collect();
        types::with_types(|reg| {
            let ti = reg.get_mut(id).unwrap();
            ti.parent = parent;
            ti.methods = methods;
        });
    }

    let mut done = vec![false; count as usize + 1];
    for row in 1..=count {
        layout(assembly, row, &ids, &mut done);
    }
    crate::log_debug!("clr", "{} types enregistrés", ids.iter().flatten().count());
}

/// Chaque champ d'instance occupe 8 octets à la suite de ceux du parent ; le
/// premier emplacement de tout objet est réservé au runtime (message d'exception).
fn layout(assembly: &Assembly, row: u32, ids: &[Option<TypeId>], done: &mut [bool]) {
    let Some(id) = ids[row as usize] else { return };
    if done[row as usize] {
        return;
    }
    done[row as usize] = true;
    let parent = types::with_types(|reg| reg.get(id).unwrap().parent);
    if let Some(parent_row) = ids.iter().position(|&i| i == Some(parent)) {
        layout(assembly, parent_row as u32, ids, done);
    }
    let base = types::with_types(|reg| reg.get(parent).map_or(8, |p| p.instance_size.max(8)));

    let mut fields = Vec::new();
    let mut offset = base;
    for f in assembly.type_fields(row) {
        if !assembly.get_field(f).is_static() {
            fields.push(types::FieldInfo {
                offset,
                metadata_row: f,
            });
            offset += 8;
        }
    }
    types::with_types(|reg| {
        let ti = reg.get_mut(id).unwrap();
        ti.fields = fields;
        ti.instance_size = offset;
    });
}

fn resolve_type(assembly: &Assembly, coded: u32, ids: &[Option<TypeId>]) -> Option<TypeId> {
    let (table, row) = CodedIndex::TypeDefOrRef.decode(coded);
    match table {
        Some(TableId::TypeDef) => ids.get(row as usize).copied().flatten(),
        Some(TableId::TypeRef) => {
            let tr = assembly.get_type_ref(row);
            let ns = assembly.get_string(tr.namespace);
            let name = assembly.get_string(tr.name);
            Some(types::with_types(|reg| reg.ensure_external(ns, name)))
        }
        _ => None,
    }
}

fn run_cctors(interp: &mut Interpreter) -> Result<(), exceptions::ClrException> {
    for row in 1..=interp.assembly.row_count(TableId::MethodDef) {
        let md = interp.assembly.get_method_def(row);
        if md.is_cctor() && interp.assembly.get_string(md.name) == ".cctor" && md.rva != 0 {
            if let ExecResult::Exception(exc) = interp.execute_method(row, Vec::new()) {
                return Err(exc);
            }
        }
    }
    Ok(())
}
