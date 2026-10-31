//! Module CLR — runtime .NET managé intégré pour Winlux.
//!
//! Fournit un interpréteur CIL complet avec GC, système de types,
//! BCL minimale et pont P/Invoke vers les DLL Win32 natives.

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

pub fn init() {
    types::init();
    gc::init();
    crate::log_info!("clr", "runtime CLR Winlux initialisé");
}

pub fn execute_assembly(pe_data: &[u8]) -> i32 {
    init();

    let assembly = match Assembly::parse(pe_data.to_vec()) {
        Ok(a) => a,
        Err(e) => {
            crate::log_error!("clr", "échec du parsing de l'assembly .NET: {}", e);
            return 1;
        }
    };

    crate::log_info!(
        "clr",
        "assembly chargée: {} types, {} méthodes",
        assembly.row_count(tables::TableId::TypeDef),
        assembly.row_count(tables::TableId::MethodDef)
    );

    load_types(&assembly);

    let entry_token = assembly.entry_point_token;
    if entry_token == 0 {
        crate::log_error!("clr", "pas de point d'entrée (entry point token = 0)");
        return 1;
    }

    crate::log_info!("clr", "point d'entrée: token 0x{:08X}", entry_token);

    let mut interp = Interpreter::new(assembly);
    run_cctors(&mut interp);

    match interp.execute_token(entry_token, vec![]) {
        ExecResult::Return(val) => match val {
            Value::I32(code) => {
                crate::log_info!("clr", "programme terminé avec code {}", code);
                code
            }
            _ => {
                crate::log_info!("clr", "programme terminé normalement");
                0
            }
        },
        ExecResult::Exit(code) => {
            crate::log_info!("clr", "Environment.Exit({})", code);
            code
        }
        ExecResult::Exception(msg) => {
            eprintln!("Unhandled Exception: {}", msg);
            crate::log_error!("clr", "exception non gérée: {}", msg);
            -1
        }
        ExecResult::PInvoke { module, name, args } => {
            crate::log_warn!(
                "clr",
                "P/Invoke non résolu au niveau racine: {}.{}",
                module,
                name
            );
            -1
        }
        ExecResult::InternalCall {
            namespace,
            class,
            method,
            args,
        } => {
            crate::log_warn!(
                "clr",
                "appel interne non résolu: {}.{}::{}",
                namespace,
                class,
                method
            );
            -1
        }
    }
}

fn load_types(assembly: &Assembly) {
    let type_count = assembly.row_count(tables::TableId::TypeDef);
    let method_count = assembly.row_count(tables::TableId::MethodDef);
    let field_count = assembly.row_count(tables::TableId::Field);

    for row in 1..=type_count {
        let td = assembly.get_type_def(row);
        let name = assembly.get_string(td.name);
        let ns = assembly.get_string(td.namespace);

        if name == "<Module>" {
            continue;
        }

        let extends_type_id = resolve_extends(assembly, td.extends);

        let is_value_type = extends_type_id
            == types::with_types(|reg| reg.find_by_name("System", "ValueType").unwrap_or(0))
            || extends_type_id
                == types::with_types(|reg| reg.find_by_name("System", "Enum").unwrap_or(0));

        let is_enum = extends_type_id
            == types::with_types(|reg| reg.find_by_name("System", "Enum").unwrap_or(0));

        let method_range = assembly.type_methods(row);
        let field_range = assembly.type_fields(row);

        let mut fields = Vec::new();
        let mut static_fields = Vec::new();
        let mut offset = 0usize;
        let mut static_offset = 0usize;

        for f_row in field_range {
            if f_row > field_count {
                break;
            }
            let fr = assembly.get_field(f_row);
            let f_name = assembly.get_string(fr.name);
            let sig_blob = assembly.get_blob(fr.signature);
            let field_type = if sig_blob.len() > 1 {
                let mut pos = 1;
                metadata::decode_type(sig_blob, &mut pos)
            } else {
                metadata::ElementType::Object
            };

            let size = types::element_size(&field_type);
            let fi = types::FieldInfo {
                name: f_name.to_string(),
                field_type,
                offset: if fr.is_static() {
                    static_offset
                } else {
                    offset
                },
                metadata_row: f_row,
            };

            if fr.is_static() {
                static_fields.push(fi);
                static_offset += size;
            } else {
                fields.push(fi);
                offset += size;
            }
        }

        let mut methods = Vec::new();
        let mut vtable_slot = 0usize;
        for m_row in method_range {
            if m_row > method_count {
                break;
            }
            let md = assembly.get_method_def(m_row);
            let m_name = assembly.get_string(md.name);

            let sig_blob = assembly.get_blob(md.signature);
            let param_c = if !sig_blob.is_empty() {
                let msig = metadata::decode_method_sig(sig_blob);
                msig.param_types.len()
            } else {
                0
            };

            let vt_slot = if md.is_virtual() {
                let s = vtable_slot;
                vtable_slot += 1;
                Some(s)
            } else {
                None
            };

            let token = tables::Token::new(tables::TableId::MethodDef, m_row).0;
            methods.push(types::MethodInfo {
                name: m_name.to_string(),
                token,
                rva: md.rva,
                flags: md.flags,
                impl_flags: md.impl_flags,
                is_static: md.is_static(),
                is_virtual: md.is_virtual(),
                is_abstract: md.is_abstract(),
                is_pinvoke: md.is_pinvoke(),
                signature_blob: md.signature,
                param_count: param_c,
                vtable_slot: vt_slot,
                metadata_row: m_row,
            });
        }

        let vtable: Vec<u32> = methods
            .iter()
            .filter(|m| m.is_virtual)
            .map(|m| m.token)
            .collect();

        let instance_size = if is_value_type {
            offset.max(1)
        } else {
            offset + 8
        };

        let ti = types::TypeInfo {
            id: 0,
            name: name.to_string(),
            namespace: ns.to_string(),
            parent: extends_type_id,
            flags: td.flags,
            fields,
            methods,
            static_fields: static_fields.clone(),
            vtable,
            interfaces: Vec::new(),
            instance_size,
            is_value_type,
            is_enum,
            is_interface: td.is_interface(),
            is_abstract: td.is_abstract(),
            generic_params: Vec::new(),
            metadata_row: row,
            cctor_run: false,
        };

        let type_id = types::with_types(|reg| reg.register(ti));

        if static_offset > 0 {
            types::with_types(|reg| reg.alloc_static_data(type_id, static_offset));
        }
    }

    crate::log_info!(
        "clr",
        "{} types utilisateur chargés",
        type_count.saturating_sub(1)
    );
}

fn resolve_extends(assembly: &Assembly, coded: u32) -> types::TypeId {
    if coded == 0 {
        return 0;
    }
    let (table, row) = tables::CodedIndex::TypeDefOrRef.decode(coded);
    match table {
        Some(tables::TableId::TypeDef) => {
            if row == 0 {
                return types::TYPE_OBJECT;
            }
            let td = assembly.get_type_def(row);
            let name = assembly.get_string(td.name);
            let ns = assembly.get_string(td.namespace);
            types::with_types(|reg| reg.find_by_name(ns, name).unwrap_or(types::TYPE_OBJECT))
        }
        Some(tables::TableId::TypeRef) => {
            let tr = assembly.get_type_ref(row);
            let name = assembly.get_string(tr.name);
            let ns = assembly.get_string(tr.namespace);
            types::with_types(|reg| reg.find_by_name(ns, name).unwrap_or(types::TYPE_OBJECT))
        }
        Some(tables::TableId::TypeSpec) => types::TYPE_OBJECT,
        _ => types::TYPE_OBJECT,
    }
}

fn run_cctors(interp: &mut Interpreter) {
    let method_count = interp.assembly.row_count(tables::TableId::MethodDef);
    let mut cctors = Vec::new();

    for row in 1..=method_count {
        let md = interp.assembly.get_method_def(row);
        if md.is_cctor() && md.rva != 0 {
            let token = tables::Token::new(tables::TableId::MethodDef, row).0;
            cctors.push(token);
        }
    }

    for token in cctors {
        crate::log_debug!("clr", "exécution du .cctor token 0x{:08X}", token);
        let _ = interp.execute_token(token, vec![]);
    }
}

pub fn is_dotnet_assembly(data: &[u8]) -> bool {
    Assembly::parse(data.to_vec()).is_ok()
}
