//! Résolution des jetons de méthode et de champ de l'interpréteur, y compris
//! entre assemblies et pour les membres de types génériques instanciés.

use super::il::{Gen, Interpreter};
use super::metadata::{self, Assembly, ElementType};
use super::tables::{CodedIndex, TableId, Token};
use super::types::{self, FieldInfo, TypeId};

/// Méthode désignée par un jeton d'appel.
#[derive(Debug, Clone)]
pub enum Target {
    /// Méthode IL : (assembly, ligne MethodDef).
    Managed(usize, u32),
    /// Méthode de la bibliothèque intégrée, appelée par son nom.
    External {
        namespace: String,
        class: String,
        name: String,
    },
}

/// Méthode à appeler et son contexte générique.
#[derive(Debug, Clone)]
pub struct Callee {
    pub target: Target,
    pub gen: Gen,
}

/// Champ résolu : son type propriétaire (instancié) et sa disposition.
pub struct FieldTarget {
    pub owner: TypeId,
    pub info: FieldInfo,
}

impl Interpreter {
    /// Méthode appelée par `token` dans le contexte de la méthode en cours.
    pub fn resolve_callee(&self, token: u32) -> Callee {
        let row = Token(token).rid();
        let cur = self.current_gen();
        match Token(token).table() {
            t if t == TableId::MethodDef as u8 => Callee {
                target: Target::Managed(self.current, row),
                gen: self.inherited_gen(self.current, row, &cur),
            },
            t if t == TableId::MethodSpec as u8 => {
                let spec = self.assembly.get_method_spec(row);
                let (table, inner) = CodedIndex::MethodDefOrRef.decode(spec.method);
                let mut callee = self.resolve_callee((table.map_or(0, |t| t as u32) << 24) | inner);
                let blob = self.assembly.get_blob(spec.instantiation);
                callee.gen.m = self.method_instantiation(self.current, blob, &cur);
                callee
            }
            t if t == TableId::MemberRef as u8 => self.resolve_member_ref(row, &cur),
            _ => Callee {
                target: Target::External {
                    namespace: String::new(),
                    class: String::new(),
                    name: format!("jeton {token:#010x}"),
                },
                gen: Gen::default(),
            },
        }
    }

    /// Une méthode d'un type générique, appelée par MethodDef depuis ce même
    /// type, garde les arguments de type de l'appelant.
    fn inherited_gen(&self, asm: usize, method_row: u32, cur: &Gen) -> Gen {
        let owner = self.domain[asm]
            .method_owner
            .get(method_row as usize)
            .copied()
            .unwrap_or(0);
        let generic = self.domain[asm]
            .type_ids
            .get(owner as usize)
            .copied()
            .flatten()
            .is_some_and(|id| {
                types::with_types(|reg| reg.get(id).is_some_and(|t| t.generic_params > 0))
            });
        Gen {
            t: if generic { cur.t.clone() } else { Vec::new() },
            m: Vec::new(),
        }
    }

    fn resolve_member_ref(&self, row: u32, cur: &Gen) -> Callee {
        let mr = self.assembly.get_member_ref(row);
        let name = self.assembly.get_string(mr.name).to_string();
        let (parent, prow) = CodedIndex::MemberRefParent.decode(mr.class);
        let external = |namespace: String, class: String| Callee {
            target: Target::External {
                namespace,
                class,
                name: name.clone(),
            },
            gen: Gen::default(),
        };
        match parent {
            Some(TableId::TypeRef) => {
                if let Some((asm, type_row)) = self.resolve_type_ref_in(&self.assembly, prow, 0) {
                    if let Some(m) = self.find_method_in(asm, type_row, &name, mr.signature) {
                        return Callee {
                            target: Target::Managed(asm, m),
                            gen: Gen::default(),
                        };
                    }
                }
                let tr = self.assembly.get_type_ref(prow);
                external(
                    self.assembly.get_string(tr.namespace).to_string(),
                    self.assembly.get_string(tr.name).to_string(),
                )
            }
            Some(TableId::TypeDef) => {
                if let Some(m) = self.find_method_in(self.current, prow, &name, mr.signature) {
                    return Callee {
                        target: Target::Managed(self.current, m),
                        gen: self.inherited_gen(self.current, m, cur),
                    };
                }
                let td = self.assembly.get_type_def(prow);
                external(
                    self.assembly.get_string(td.namespace).to_string(),
                    self.assembly.get_string(td.name).to_string(),
                )
            }
            Some(TableId::TypeSpec) => {
                let coded = (prow << 2) | 2;
                let id = self.resolve_coded(self.current, coded, cur);
                let (def, args, ns, class) = types::with_types(|reg| {
                    let t = reg.get(id);
                    (
                        t.and_then(|t| t.def),
                        t.map(|t| t.type_args.clone()).unwrap_or_default(),
                        t.map(|t| t.namespace.clone()).unwrap_or_default(),
                        t.map(|t| t.name.clone()).unwrap_or_default(),
                    )
                });
                if let Some((asm, type_row)) = def {
                    if let Some(m) = self.find_method_in(asm, type_row, &name, mr.signature) {
                        return Callee {
                            target: Target::Managed(asm, m),
                            gen: Gen {
                                t: args,
                                m: Vec::new(),
                            },
                        };
                    }
                }
                external(ns, class)
            }
            Some(TableId::MethodDef) => Callee {
                target: Target::Managed(self.current, prow),
                gen: self.inherited_gen(self.current, prow, cur),
            },
            _ => external(String::new(), String::new()),
        }
    }

    /// Méthode `name` du type `type_row` de l'assembly `asm` dont la
    /// signature correspond à `sig_blob` (blob de l'assembly courante).
    pub fn find_method_in(
        &self,
        asm: usize,
        type_row: u32,
        name: &str,
        sig_blob: u32,
    ) -> Option<u32> {
        let wanted = sig_key(&self.assembly, self.assembly.get_blob(sig_blob));
        let target = self.assembly_at(asm);
        target.type_methods(type_row).find(|&m| {
            let md = target.get_method_def(m);
            target.get_string(md.name) == name
                && sig_key(target, target.get_blob(md.signature)) == wanted
        })
    }

    /// Champ désigné par un jeton Field ou MemberRef, avec son propriétaire
    /// instancié dans le contexte de la méthode en cours.
    pub fn resolve_field(&self, token: u32) -> Option<FieldTarget> {
        let row = Token(token).rid();
        let cur = self.current_gen();
        let (asm, field_row, owner) = match Token(token).table() {
            t if t == TableId::Field as u8 => {
                let info = &self.domain[self.current];
                let owner_row = info.field_owner.get(row as usize).copied().unwrap_or(0);
                let mut owner = info.type_ids.get(owner_row as usize).copied().flatten()?;
                let generic =
                    types::with_types(|reg| reg.get(owner).is_some_and(|t| t.generic_params > 0));
                if generic && !cur.t.is_empty() {
                    owner = self.instantiate(owner, cur.t.clone());
                }
                (self.current, row, owner)
            }
            t if t == TableId::MemberRef as u8 => {
                let mr = self.assembly.get_member_ref(row);
                let (parent, prow) = CodedIndex::MemberRefParent.decode(mr.class);
                let name = self.assembly.get_string(mr.name);
                let (asm, type_row, owner) = match parent {
                    Some(TableId::TypeRef) => {
                        let (asm, type_row) = self.resolve_type_ref_in(&self.assembly, prow, 0)?;
                        let owner = self.domain[asm]
                            .type_ids
                            .get(type_row as usize)
                            .copied()
                            .flatten()?;
                        (asm, type_row, owner)
                    }
                    Some(TableId::TypeDef) => {
                        let owner = self.domain[self.current]
                            .type_ids
                            .get(prow as usize)
                            .copied()
                            .flatten()?;
                        (self.current, prow, owner)
                    }
                    Some(TableId::TypeSpec) => {
                        let owner = self.resolve_coded(self.current, (prow << 2) | 2, &cur);
                        let (asm, type_row) =
                            types::with_types(|reg| reg.get(owner).and_then(|t| t.def))?;
                        (asm, type_row, owner)
                    }
                    _ => return None,
                };
                let a = self.assembly_at(asm);
                let f = a
                    .type_fields(type_row)
                    .find(|&f| a.get_string(a.get_field(f).name) == name)?;
                (asm, f, owner)
            }
            _ => return None,
        };
        let _ = asm;
        let (info, _) = self.field_info(owner, field_row)?;
        Some(FieldTarget { owner, info })
    }

    /// Type désigné par un jeton TypeDef / TypeRef / TypeSpec dans le
    /// contexte de la méthode en cours.
    pub fn resolve_type_token(&self, token: u32) -> Option<TypeId> {
        let row = Token(token).rid();
        let tag = match Token(token).table() {
            t if t == TableId::TypeDef as u8 => 0,
            t if t == TableId::TypeRef as u8 => 1,
            t if t == TableId::TypeSpec as u8 => 2,
            _ => return None,
        };
        Some(self.resolve_coded(self.current, (row << 2) | tag, &self.current_gen()))
    }

    pub fn get_method_sig(&self, token: u32) -> metadata::MethodSig {
        let row = Token(token).rid();
        let blob = match Token(token).table() {
            t if t == TableId::MethodDef as u8 => self.assembly.get_method_def(row).signature,
            t if t == TableId::MemberRef as u8 => self.assembly.get_member_ref(row).signature,
            t if t == TableId::MethodSpec as u8 => {
                let spec = self.assembly.get_method_spec(row);
                let (table, inner) = CodedIndex::MethodDefOrRef.decode(spec.method);
                return self.get_method_sig((table.map_or(0, |t| t as u32) << 24) | inner);
            }
            _ => 0,
        };
        metadata::decode_method_sig(self.assembly.get_blob(blob))
    }

    /// Nom d'une méthode désignée par un jeton d'appel.
    pub fn method_name(&self, token: u32) -> String {
        let row = Token(token).rid();
        let name = match Token(token).table() {
            t if t == TableId::MethodDef as u8 => self.assembly.get_method_def(row).name,
            t if t == TableId::MemberRef as u8 => self.assembly.get_member_ref(row).name,
            t if t == TableId::MethodSpec as u8 => {
                let spec = self.assembly.get_method_spec(row);
                let (table, inner) = CodedIndex::MethodDefOrRef.decode(spec.method);
                return self.method_name((table.map_or(0, |t| t as u32) << 24) | inner);
            }
            _ => 0,
        };
        self.assembly.get_string(name).to_string()
    }
}

/// Signature de méthode réduite aux noms complets de ses types, pour
/// comparer des signatures écrites dans deux assemblies différentes.
fn sig_key(a: &Assembly, blob: &[u8]) -> String {
    let sig = metadata::decode_method_sig(blob);
    let mut key = format!(
        "{}:{}({}",
        sig.calling_convention & 0x3F,
        sig.generic_param_count,
        type_key(a, &sig.ret_type)
    );
    for p in &sig.param_types {
        key.push(',');
        key += &type_key(a, p);
    }
    key
}

fn type_key(a: &Assembly, et: &ElementType) -> String {
    let named = |coded: u32| {
        let (table, row) = CodedIndex::TypeDefOrRef.decode(coded);
        let (ns, name) = match table {
            Some(TableId::TypeDef) => {
                let td = a.get_type_def(row);
                (td.namespace, td.name)
            }
            Some(TableId::TypeRef) => {
                let tr = a.get_type_ref(row);
                (tr.namespace, tr.name)
            }
            _ => return format!("spec{row}"),
        };
        format!("{}.{}", a.get_string(ns), a.get_string(name))
    };
    match et {
        ElementType::Class(c) | ElementType::ValueType(c) => named(*c),
        ElementType::Ptr(i) => format!("{}*", type_key(a, i)),
        ElementType::ByRef(i) => format!("{}&", type_key(a, i)),
        ElementType::SzArray(i) => format!("{}[]", type_key(a, i)),
        ElementType::Array(i) => format!("{}[,]", type_key(a, i)),
        ElementType::GenericInst(i, args) => {
            let args: Vec<String> = args.iter().map(|t| type_key(a, t)).collect();
            format!("{}<{}>", type_key(a, i), args.join(","))
        }
        other => format!("{other:?}"),
    }
}

/// Classes dont `bcl.rs` et le moteur servent eux-mêmes une partie des
/// méthodes : une référence vers elles peut marcher sans notre mscorlib.
const SERVIES_PAR_LE_MOTEUR: [&str; 15] = [
    "System.Console",
    "System.Environment",
    "System.Math",
    "System.String",
    "System.Object",
    "System.Array",
    "System.GC",
    "System.Convert",
    "System.Type",
    "System.Activator",
    "System.IO.File",
    "System.IO.Path",
    "System.Runtime.InteropServices.Marshal",
    "System.Threading.Monitor",
    "System.Threading.Interlocked",
];

impl Interpreter {
    /// WINLUX_CLR_MANQUANTS : résout d'avance chaque MemberRef de méthode
    /// des assemblies chargées et liste celles que ni notre mscorlib ni une
    /// assembly du programme ne définit (#1922). C'est l'équivalent de
    /// manquants.log pour .NET : le travail de résolution du moteur fait
    /// d'un coup, au lieu d'un mur par lancement. La liste va dans le
    /// journal et, si la variable nomme un fichier, dans ce fichier.
    pub(super) fn report_missing(&mut self, destination: &str) {
        let mut missing: std::collections::BTreeMap<String, (usize, bool)> = Default::default();
        for index in 0..self.domain.len() {
            if self.domain[index].name == super::domain::CORLIB {
                continue;
            }
            let previous = self.enter(index);
            for row in 1..=self.assembly.row_count(TableId::MemberRef) {
                let mr = self.assembly.get_member_ref(row);
                // Signature de champ (0x06) : pas une méthode.
                if self.assembly.get_blob(mr.signature).first() == Some(&0x06) {
                    continue;
                }
                let token = Token::new(TableId::MemberRef, row).0;
                if let Target::External {
                    namespace,
                    class,
                    name,
                } = self.resolve_callee(token).target
                {
                    let class_full = if namespace.is_empty() {
                        class
                    } else {
                        format!("{namespace}.{class}")
                    };
                    let served = SERVIES_PAR_LE_MOTEUR.contains(&class_full.as_str());
                    let entry = missing
                        .entry(format!("{class_full}::{name}"))
                        .or_insert((0, served));
                    entry.0 += 1;
                }
            }
            self.enter(previous);
        }
        let mut text = String::new();
        for (member, (count, served)) in &missing {
            let note = if *served {
                " (classe servie en partie par le moteur)"
            } else {
                ""
            };
            text.push_str(&format!("{member}\t{count}{note}\n"));
            crate::log_warn!("clr-manquants", "{member} ({count} référence(s)){note}");
        }
        crate::log_warn!(
            "clr-manquants",
            "{} méthode(s) .NET référencée(s) non définie(s) dans notre mscorlib ni dans le programme",
            missing.len()
        );
        if destination != "1" && !destination.is_empty() {
            if let Err(e) = std::fs::write(destination, text) {
                crate::log_warn!(
                    "clr-manquants",
                    "écriture de {destination} impossible : {e}"
                );
            }
        }
    }
}
