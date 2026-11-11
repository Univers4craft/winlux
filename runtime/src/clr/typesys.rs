//! Résolution des types pour l'interpréteur : jetons et signatures vers
//! `TypeId`, instanciations génériques, disposition mémoire des champs et
//! façon de ranger chaque valeur (`Kind`).
//!
//! Le code IL d'une méthode générique est partagé entre ses instanciations ;
//! ce qui dépend de `T` (taille, boxing, champs statiques) est résolu à
//! l'exécution grâce au contexte générique (`Gen`) de la méthode en cours.

use super::domain;
use super::il::{Gen, Interpreter};
use super::metadata::{self, Assembly, ElementType};
use super::tables::{CodedIndex, TableId};
use super::types::{self, FieldInfo, Kind, TypeId, TypeInfo};

/// Champ « littéral » (constante de compilation, sans emplacement).
const FIELD_LITERAL: u16 = 0x0040;

impl Interpreter {
    /// Parents et nature (type valeur, énumération) des types définis par
    /// les assemblies du domaine.
    pub fn link_types(&mut self) {
        for asm in 0..self.domain.len() {
            let a = self.assembly_at(asm);
            let rows: Vec<(TypeId, u32)> = (1..=a.row_count(TableId::TypeDef))
                .filter_map(|r| {
                    let id = self.domain[asm]
                        .type_ids
                        .get(r as usize)
                        .copied()
                        .flatten()?;
                    Some((id, a.get_type_def(r).extends))
                })
                .collect();
            for (id, extends) in rows {
                if types::is_builtin(id) {
                    continue;
                }
                let parent = if CodedIndex::TypeDefOrRef.decode(extends).1 == 0 {
                    0
                } else {
                    self.resolve_coded_open(asm, extends)
                };
                types::with_types(|reg| {
                    if let Some(t) = reg.get_mut(id) {
                        t.parent = parent;
                    }
                });
            }
        }
        types::with_types(|reg| {
            let ids: Vec<TypeId> = self
                .domain
                .iter()
                .flat_map(|d| d.type_ids.iter().flatten().copied())
                .filter(|&id| !types::is_builtin(id))
                .collect();
            for id in ids {
                let parent = reg.get(id).map_or(0, |t| t.parent);
                let is_value = parent == types::TYPE_VALUETYPE || parent == types::TYPE_ENUM;
                if let Some(t) = reg.get_mut(id) {
                    t.is_value = is_value;
                    t.is_enum = parent == types::TYPE_ENUM;
                }
            }
        });
    }

    /// Type désigné par un codage TypeDefOrRef, sans instancier un type
    /// générique ouvert (utilisé pour le parent d'une définition).
    fn resolve_coded_open(&self, asm: usize, coded: u32) -> TypeId {
        let (table, row) = CodedIndex::TypeDefOrRef.decode(coded);
        if table == Some(TableId::TypeSpec) {
            let a = self.assembly_at(asm);
            let blob = a.get_blob(a.get_type_spec(row).signature);
            let mut off = 0;
            if let ElementType::GenericInst(inner, _) = metadata::decode_type(blob, &mut off) {
                return self.resolve_sig(asm, &inner, &Gen::default());
            }
        }
        self.resolve_coded(asm, coded, &Gen::default())
    }

    /// Type désigné par un codage TypeDefOrRef de l'assembly `asm`.
    pub fn resolve_coded(&self, asm: usize, coded: u32, gen: &Gen) -> TypeId {
        let (table, row) = CodedIndex::TypeDefOrRef.decode(coded);
        match table {
            Some(TableId::TypeDef) => self.domain[asm]
                .type_ids
                .get(row as usize)
                .copied()
                .flatten()
                .unwrap_or(types::TYPE_OBJECT),
            Some(TableId::TypeRef) => self.type_ref_id(asm, row),
            Some(TableId::TypeSpec) => {
                let a = self.assembly_at(asm);
                let blob = a.get_blob(a.get_type_spec(row).signature);
                let mut off = 0;
                let et = metadata::decode_type(blob, &mut off);
                self.resolve_sig(asm, &et, gen)
            }
            _ => types::TYPE_OBJECT,
        }
    }

    /// Type d'une TypeRef : défini par une assembly du domaine, ou fourni
    /// par la bibliothèque intégrée.
    pub fn type_ref_id(&self, asm: usize, row: u32) -> TypeId {
        let a = self.assembly_at(asm);
        if let Some((target, t)) = self.resolve_type_ref_in(a, row, 0) {
            if let Some(id) = self.domain[target]
                .type_ids
                .get(t as usize)
                .copied()
                .flatten()
            {
                return id;
            }
        }
        let tr = a.get_type_ref(row);
        let ns = a.get_string(tr.namespace);
        let name = a.get_string(tr.name);
        types::with_types(|reg| reg.ensure_external(ns, name))
    }

    /// Type d'un élément de signature, dans le contexte générique `gen`.
    pub fn resolve_sig(&self, asm: usize, et: &ElementType, gen: &Gen) -> TypeId {
        use ElementType as E;
        match et {
            E::Void => types::TYPE_OBJECT,
            E::Boolean => types::TYPE_BOOLEAN,
            E::Char => types::TYPE_CHAR,
            E::I1 => types::TYPE_SBYTE,
            E::U1 => types::TYPE_BYTE,
            E::I2 => types::TYPE_INT16,
            E::U2 => types::TYPE_UINT16,
            E::I4 => types::TYPE_INT32,
            E::U4 => types::TYPE_UINT32,
            E::I8 => types::TYPE_INT64,
            E::U8 => types::TYPE_UINT64,
            E::R4 => types::TYPE_FLOAT,
            E::R8 => types::TYPE_DOUBLE,
            E::String => types::TYPE_STRING,
            E::Object | E::TypedByRef | E::Sentinel => types::TYPE_OBJECT,
            E::IntPtr | E::Ptr(_) | E::ByRef(_) | E::FnPtr => types::TYPE_INTPTR,
            E::UIntPtr => types::TYPE_UINTPTR,
            E::Class(c) | E::ValueType(c) => self.resolve_coded(asm, *c, gen),
            E::GenericInst(inner, args) => {
                let def = self.resolve_sig(asm, inner, gen);
                let args: Vec<TypeId> =
                    args.iter().map(|t| self.resolve_sig(asm, t, gen)).collect();
                self.instantiate(def, args)
            }
            E::Var(n) => gen
                .t
                .get(*n as usize)
                .copied()
                .unwrap_or(types::TYPE_OBJECT),
            E::MVar(n) => gen
                .m
                .get(*n as usize)
                .copied()
                .unwrap_or(types::TYPE_OBJECT),
            E::SzArray(_) | E::Array(_) => types::TYPE_ARRAY,
        }
    }

    /// Instanciation `def<args>` (créée à la première demande).
    pub fn instantiate(&self, def: TypeId, args: Vec<TypeId>) -> TypeId {
        if args.is_empty() {
            return def;
        }
        let existing = types::with_types(|reg| {
            if let Some(id) = reg.find_instance(def, &args) {
                return Ok(id);
            }
            let Some(open) = reg.get(def) else {
                return Ok(def);
            };
            if open.generic_params == 0 {
                // Type de la bibliothèque intégrée (List`1…) : l'instanciation
                // garde le nom du type ouvert, sans définition à disposer.
                let info = TypeInfo {
                    type_args: args.clone(),
                    laid_out: open.laid_out,
                    instance_size: open.instance_size,
                    ..open.clone()
                };
                return Ok(reg.register_instance(def, args.clone(), info));
            }
            let info = TypeInfo {
                name: open.name.clone(),
                namespace: open.namespace.clone(),
                parent: open.parent,
                methods: open.methods.clone(),
                def: open.def,
                generic_params: open.generic_params,
                type_args: args.clone(),
                is_value: open.is_value,
                is_enum: open.is_enum,
                is_interface: open.is_interface,
                ..TypeInfo::default()
            };
            let def_row = open.def;
            Err((reg.register_instance(def, args.clone(), info), def_row))
        });
        match existing {
            Ok(id) => id,
            Err((id, def_row)) => {
                // Parent substitué : `class B<T> : A<T>` donne B<int> : A<int>.
                if let Some((asm, row)) = def_row {
                    let extends = self.assembly_at(asm).get_type_def(row).extends;
                    if CodedIndex::TypeDefOrRef.decode(extends).1 != 0 {
                        let parent = self.resolve_coded(
                            asm,
                            extends,
                            &Gen {
                                t: args,
                                m: Vec::new(),
                            },
                        );
                        types::with_types(|reg| {
                            if let Some(t) = reg.get_mut(id) {
                                t.parent = parent;
                            }
                        });
                    }
                }
                id
            }
        }
    }

    /// Calcule une fois la disposition des champs d'un type : les champs
    /// d'instance suivent ceux du parent (un type valeur commence à 0, une
    /// classe garde son premier mot pour le runtime), chacun sur un
    /// emplacement multiple de 8 octets ; un type valeur est rangé en place.
    pub fn ensure_layout(&self, id: TypeId) {
        let Some((def, args, parent, is_value, is_delegate_root)) = types::with_types(|reg| {
            let t = reg.get_mut(id)?;
            if t.laid_out {
                return None;
            }
            t.laid_out = true;
            let root = t.namespace == "System" && t.name == "Delegate";
            Some((t.def, t.type_args.clone(), t.parent, t.is_value, root))
        }) else {
            return;
        };
        let Some((asm, row)) = def else { return };
        if parent != 0 {
            self.ensure_layout(parent);
        }
        let base = if is_value {
            0
        } else {
            types::with_types(|reg| reg.get(parent).map_or(8, |p| p.instance_size.max(8)))
        };
        let gen = Gen {
            t: args,
            m: Vec::new(),
        };
        let a = self.assembly_at(asm);
        // Types valeur : disposition des structures C (séquentielle par
        // défaut, ou explicite), avec `Pack` et `Size` éventuels, pour que
        // P/Invoke les passe telles quelles (ECMA-335 §II.10.7).
        let explicit = is_value && a.get_type_def(row).flags & 0x18 == 0x10;
        let (pack, class_size) = if is_value {
            a.class_layout_of(row).unwrap_or((0, 0))
        } else {
            (0, 0)
        };
        let mut fields = Vec::new();
        let mut statics = Vec::new();
        let mut offset = base;
        let mut end = base;
        let mut max_align = 1;
        let mut static_offset = 0;
        let field_rows: Vec<u32> = a.type_fields(row).collect();
        for f in field_rows {
            let a = self.assembly_at(asm);
            let fd = a.get_field(f);
            let blob = a.get_blob(fd.signature);
            let mut off = 1;
            let et = metadata::decode_type(blob, &mut off);
            let is_static = fd.is_static();
            let literal = fd.flags & FIELD_LITERAL != 0;
            let field_offset = if explicit { a.field_offset_of(f) } else { None };
            if is_static {
                if literal {
                    continue;
                }
                let kind = self.kind_of_sig(asm, &et, &gen);
                statics.push(FieldInfo {
                    offset: static_offset,
                    metadata_row: f,
                    kind,
                });
                static_offset += kind.slot_size();
            } else if is_value {
                let kind = self.kind_of_sig(asm, &et, &gen);
                let mut align = kind.align().max(1);
                if pack > 0 {
                    align = align.min(pack as usize);
                }
                max_align = max_align.max(align);
                let at = match field_offset {
                    Some(o) => o as usize,
                    None => end.div_ceil(align) * align,
                };
                fields.push(FieldInfo {
                    offset: at,
                    metadata_row: f,
                    kind,
                });
                end = end.max(at + kind.elem_size());
            } else {
                let kind = self.kind_of_sig(asm, &et, &gen);
                fields.push(FieldInfo {
                    offset,
                    metadata_row: f,
                    kind,
                });
                offset += kind.slot_size();
            }
        }
        let size = if is_value {
            end.max(class_size as usize).div_ceil(max_align).max(1) * max_align
        } else if is_delegate_root {
            // Le moteur range cible, méthode et liste d'invocation dans tout
            // délégué (delegates.rs) : sans cette réserve, un délégué de notre
            // mscorlib (Func<T>…) débordait de son objet de 16 octets et
            // corrompait le tas (#1702).
            offset.max(base).max(super::delegates::OBJECT_SIZE)
        } else {
            offset.max(base)
        };
        types::with_types(|reg| {
            if let Some(t) = reg.get_mut(id) {
                t.fields = fields;
                t.static_fields = statics;
                t.instance_size = size;
                if is_value {
                    t.align = max_align;
                }
            }
            if static_offset > 0 {
                reg.static_data
                    .insert(id, vec![0u64; static_offset / 8].into_boxed_slice());
            }
        });
    }

    /// Façon de ranger une valeur du type `id`.
    pub fn kind_of_type(&self, id: TypeId) -> Kind {
        if let Some(k) = Kind::of_builtin(id) {
            return k;
        }
        let Some((is_value, is_enum)) =
            types::with_types(|reg| reg.get(id).map(|t| (t.is_value, t.is_enum)))
        else {
            return Kind::Ref;
        };
        if !is_value {
            return Kind::Ref;
        }
        self.ensure_layout(id);
        if is_enum {
            return types::with_types(|reg| {
                reg.get(id)
                    .and_then(|t| t.fields.first())
                    .map_or(Kind::I4, |f| f.kind)
            });
        }
        Kind::Struct(id)
    }

    /// Façon de ranger une valeur d'un élément de signature.
    pub fn kind_of_sig(&self, asm: usize, et: &ElementType, gen: &Gen) -> Kind {
        use ElementType as E;
        match et {
            E::Boolean => Kind::Bool,
            E::Char => Kind::Char,
            E::I1 => Kind::I1,
            E::U1 => Kind::U1,
            E::I2 => Kind::I2,
            E::U2 => Kind::U2,
            E::I4 => Kind::I4,
            E::U4 => Kind::U4,
            E::I8 => Kind::I8,
            E::U8 => Kind::U8,
            E::R4 => Kind::R4,
            E::R8 => Kind::R8,
            E::IntPtr | E::UIntPtr | E::Ptr(_) | E::ByRef(_) | E::FnPtr => Kind::I,
            E::String | E::Object | E::Class(_) | E::SzArray(_) | E::Array(_) => Kind::Ref,
            E::GenericInst(inner, _) if matches!(**inner, E::Class(_)) => Kind::Ref,
            E::Void | E::TypedByRef | E::Sentinel => Kind::Ref,
            E::ValueType(_) | E::GenericInst(..) | E::Var(_) | E::MVar(_) => {
                self.kind_of_type(self.resolve_sig(asm, et, gen))
            }
        }
    }

    /// Champ (instance ou statique) d'un type, une fois sa disposition faite.
    pub fn field_info(&self, owner: TypeId, row: u32) -> Option<(FieldInfo, bool)> {
        self.ensure_layout(owner);
        types::with_types(|reg| {
            let t = reg.get(owner)?;
            if let Some(f) = t.fields.iter().find(|f| f.metadata_row == row) {
                return Some((f.clone(), false));
            }
            t.static_fields
                .iter()
                .find(|f| f.metadata_row == row)
                .map(|f| (f.clone(), true))
        })
    }

    /// Instanciation de méthode d'un blob MethodSpec (`0x0A`, nombre, types).
    pub fn method_instantiation(&self, asm: usize, blob: &[u8], gen: &Gen) -> Vec<TypeId> {
        let mut off = 1;
        let count = metadata::decode_compressed_u32(blob, &mut off);
        (0..count)
            .map(|_| {
                let et = metadata::decode_type(blob, &mut off);
                self.resolve_sig(asm, &et, gen)
            })
            .collect()
    }

    /// Assembly chargée qui définit le type d'une TypeRef de `a` :
    /// (assembly, ligne TypeDef).
    pub fn resolve_type_ref_in(&self, a: &Assembly, row: u32, depth: u32) -> Option<(usize, u32)> {
        if depth > 16 || row == 0 || row > a.row_count(TableId::TypeRef) {
            return None;
        }
        let tr = a.get_type_ref(row);
        let ns = a.get_string(tr.namespace);
        let name = a.get_string(tr.name);
        let (scope, srow) = CodedIndex::ResolutionScope.decode(tr.resolution_scope);
        match scope {
            Some(TableId::AssemblyRef) => {
                let wanted = a
                    .get_string(a.get_assembly_ref(srow).name)
                    .to_ascii_lowercase();
                // Une assembly du .NET Framework non chargée (System,
                // System.Core…) : ses types sont dans notre mscorlib, comme
                // si elle les y redirigeait (ECMA-335 §II.22.14, #1947 :
                // Stopwatch est dans System.dll).
                let asm = self
                    .domain
                    .iter()
                    .position(|i| i.name == wanted)
                    .or_else(|| {
                        domain::is_framework_assembly(&wanted)
                            .then(|| self.domain.iter().position(|i| i.name == domain::CORLIB))
                            .flatten()
                    })?;
                let target = self.assembly_at(asm);
                let t = (1..=target.row_count(TableId::TypeDef)).find(|&t| {
                    let td = target.get_type_def(t);
                    target.get_string(td.name) == name
                        && target.get_string(td.namespace) == ns
                        && !is_nested(target, t)
                })?;
                Some((asm, t))
            }
            Some(TableId::TypeRef) => {
                let (asm, outer) = self.resolve_type_ref_in(a, srow, depth + 1)?;
                let target = self.assembly_at(asm);
                let t = (1..=target.row_count(TableId::NestedClass)).find_map(|n| {
                    let nc = target.get_nested_class(n);
                    (nc.enclosing_class == outer
                        && target.get_string(target.get_type_def(nc.nested_class).name) == name)
                        .then_some(nc.nested_class)
                })?;
                Some((asm, t))
            }
            _ => None,
        }
    }
}

fn is_nested(a: &Assembly, type_row: u32) -> bool {
    (1..=a.row_count(TableId::NestedClass)).any(|n| a.get_nested_class(n).nested_class == type_row)
}
