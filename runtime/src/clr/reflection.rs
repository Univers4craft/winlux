//! Réflexion minimale : objets `System.Type` et `System.Reflection.Assembly`
//! (#1869).
//!
//! Écrit d'après la documentation publique de l'API (« Type Class »,
//! « Assembly Class », « Assembly.FullName », « Assembly.Location ») et
//! ECMA-335 §II.6.2.1 et §II.22.2 (table Assembly) ; aucun code de
//! Microsoft, de Mono ou de Wine.
//!
//! Un type n'a qu'un objet `Type` et une assembly qu'un objet `Assembly` :
//! `typeof(A) == typeof(A)` compare des références, comme sous .NET. Ces
//! objets vivent tant que le programme : ils sont des racines du ramasse-
//! miettes. Leur classe est `System.RuntimeType` (ou
//! `System.Reflection.RuntimeAssembly`) de notre mscorlib ; le moteur range
//! le numéro du type (ou l'indice de l'assembly + 1) dans un mot ajouté
//! après les champs de la classe, que le code managé ne voit pas.

use std::collections::HashMap;
use std::sync::Mutex;

use super::bcl::alloc_managed_string;
use super::domain;
use super::exceptions::ClrException;
use super::gc;
use super::il::{Interpreter, Value};
use super::tables::TableId;
use super::types::{self, TypeId};

/// Assembly sans fichier chargé : la mscorlib intégrée (bcl.rs).
const BUILTIN_CORLIB: usize = usize::MAX;

/// Emplacement de mscorlib sur un .NET Framework 4 64 bits.
const CORLIB_LOCATION: &str = r"C:\Windows\Microsoft.NET\Framework64\v4.0.30319\mscorlib.dll";

/// Jeton de clé publique de la mscorlib du .NET Framework, publié dans les
/// noms d'assembly de la documentation.
const CORLIB_TOKEN: &str = "b77a5c561934e089";

#[derive(Default)]
struct Cache {
    types: HashMap<TypeId, usize>,
    assemblies: HashMap<usize, usize>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

fn with_cache<T>(f: impl FnOnce(&mut Cache) -> T) -> T {
    let mut lock = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    f(lock.get_or_insert_with(Cache::default))
}

/// Objets de réflexion créés : racines du ramasse-miettes.
pub fn roots() -> Vec<usize> {
    with_cache(|c| {
        c.types
            .values()
            .chain(c.assemblies.values())
            .copied()
            .collect()
    })
}

/// Nom, version, culture et jeton d'une assembly (ECMA-335 §II.6.2.1).
struct Identity {
    name: String,
    version: [u16; 4],
    culture: String,
    token: Option<String>,
    location: String,
}

impl Identity {
    fn full_name(&self) -> String {
        let [a, b, c, d] = self.version;
        let culture = if self.culture.is_empty() {
            "neutral"
        } else {
            &self.culture
        };
        format!(
            "{}, Version={a}.{b}.{c}.{d}, Culture={culture}, PublicKeyToken={}",
            self.name,
            self.token.as_deref().unwrap_or("null")
        )
    }
}

/// Jeton d'une clé publique : les 8 derniers octets de son SHA-1, à
/// l'envers (ECMA-335 §II.6.2.1.3).
fn public_key_token(key: &[u8]) -> String {
    let hash = crate::dlls::bcrypt::crypto::sha1(&[key]);
    hash[12..]
        .iter()
        .rev()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Numéro porté par un objet de réflexion créé par `reflection_object`
/// (et seulement par lui : un Type dérivé écrit en C# n'en a pas).
fn this_handle(args: &[Value], namespace: &str, class: &str) -> Option<usize> {
    let Some(Value::Ref(p)) = args.first() else {
        return None;
    };
    let id = gc::type_of(*p)?;
    let size = types::with_types(|reg| {
        let t = reg.get(id)?;
        (t.namespace == namespace && t.name == class).then_some(t.instance_size)
    })?;
    with_cache(|c| {
        c.types
            .values()
            .chain(c.assemblies.values())
            .any(|&o| o == *p as usize)
    })
    .then(|| unsafe { *(p.add(size) as *const u64) } as usize)
}

fn bool_value(b: bool) -> Value {
    Value::I32(b as i32)
}

impl Interpreter {
    /// Objet unique d'une classe de réflexion, portant `handle`.
    fn reflection_object(&self, namespace: &str, class: &str, handle: usize) -> *mut u8 {
        let id = types::with_types(|reg| reg.ensure_external(namespace, class));
        self.ensure_layout(id);
        let size = types::with_types(|reg| reg.get(id).map_or(0, |t| t.instance_size));
        let obj = gc::with_gc(|gc| gc.alloc(id, size + 8));
        unsafe { *(obj.add(size) as *mut u64) = handle as u64 };
        obj
    }

    /// `System.Type` d'un type enregistré.
    pub(super) fn type_object(&self, id: TypeId) -> Value {
        if let Some(obj) = with_cache(|c| c.types.get(&id).copied()) {
            return Value::Ref(obj as *mut u8);
        }
        let obj = self.reflection_object("System", "RuntimeType", id as usize);
        Value::Ref(with_cache(|c| *c.types.entry(id).or_insert(obj as usize)) as *mut u8)
    }

    fn assembly_object(&self, index: usize) -> Value {
        if let Some(obj) = with_cache(|c| c.assemblies.get(&index).copied()) {
            return Value::Ref(obj as *mut u8);
        }
        let obj = self.reflection_object(
            "System.Reflection",
            "RuntimeAssembly",
            index.wrapping_add(1),
        );
        Value::Ref(with_cache(|c| *c.assemblies.entry(index).or_insert(obj as usize)) as *mut u8)
    }

    /// Assembly qui définit un type : la sienne, sinon mscorlib pour les
    /// types intégrés.
    fn assembly_of_type(&self, id: TypeId) -> usize {
        let def = types::with_types(|reg| {
            let t = reg.get(id)?;
            // Une instanciation générique appartient à sa définition.
            t.def.map(|(asm, _)| asm)
        });
        def.or_else(|| self.domain.iter().position(|i| i.name == domain::CORLIB))
            .unwrap_or(BUILTIN_CORLIB)
    }

    fn identity(&self, index: usize) -> Identity {
        let corlib = index == BUILTIN_CORLIB
            || self
                .domain
                .get(index)
                .is_some_and(|i| i.name == domain::CORLIB);
        if index == BUILTIN_CORLIB || index >= self.domain.len() {
            return Identity {
                name: "mscorlib".into(),
                version: [4, 0, 0, 0],
                culture: String::new(),
                token: Some(CORLIB_TOKEN.into()),
                location: CORLIB_LOCATION.into(),
            };
        }
        let a = self.assembly_at(index);
        let info = &self.domain[index];
        let (name, version, culture, key) = if a.row_count(TableId::Assembly) > 0 {
            let row = a.get_assembly(1);
            (
                a.get_string(row.name).to_string(),
                [row.major, row.minor, row.build, row.revision],
                a.get_string(row.culture).to_string(),
                a.get_blob(row.public_key).to_vec(),
            )
        } else {
            (info.name.clone(), [0; 4], String::new(), Vec::new())
        };
        // Notre mscorlib n'est pas signée : elle se présente comme celle du
        // .NET Framework, que les programmes nomment avec ce jeton.
        let token = if corlib {
            Some(CORLIB_TOKEN.into())
        } else {
            (!key.is_empty()).then(|| public_key_token(&key))
        };
        let location = if corlib {
            CORLIB_LOCATION.into()
        } else {
            // Assembly chargée depuis la mémoire : chaîne vide, comme .NET.
            info.path
                .as_deref()
                .map(crate::paths::to_windows)
                .unwrap_or_default()
        };
        Identity {
            name,
            version,
            culture,
            token,
            location,
        }
    }

    /// Méthodes de Type, Assembly et Object.GetType servies par le moteur.
    pub(super) fn reflection_call(
        &mut self,
        namespace: &str,
        class: &str,
        name: &str,
        args: &[Value],
    ) -> Option<Result<Value, ClrException>> {
        let value = match (namespace, class, name) {
            ("System", "Type", "GetTypeFromHandle") => match args.first() {
                // Jeton non résolu (type d'une assembly absente) : gardé tel
                // quel, comme avant.
                Some(Value::I64(h)) if (*h as u64) < 0x0100_0000 => self.type_object(*h as TypeId),
                Some(v) => *v,
                None => Value::Null,
            },
            ("System", "Object", "GetType") => match args.first() {
                Some(Value::Ref(p)) if !p.is_null() => match gc::type_of(*p) {
                    Some(id) if id != 0 => self.type_object(id),
                    _ => self.type_object(types::TYPE_OBJECT),
                },
                Some(Value::I32(_)) => self.type_object(types::TYPE_INT32),
                Some(Value::I64(_)) => self.type_object(types::TYPE_INT64),
                Some(Value::F64(_)) => self.type_object(types::TYPE_DOUBLE),
                Some(Value::F32(_)) => self.type_object(types::TYPE_FLOAT),
                _ => return Some(Err(ClrException::null_reference())),
            },
            ("System", "Type" | "RuntimeType", _) => {
                let Some(id) = this_handle(args, "System", "RuntimeType") else {
                    // Ancienne représentation (numéro brut) : laissée à bcl.rs.
                    return match args.first() {
                        Some(Value::Null) => Some(Err(ClrException::null_reference())),
                        Some(Value::Ref(p)) if p.is_null() => {
                            Some(Err(ClrException::null_reference()))
                        }
                        _ => None,
                    };
                };
                let id = id as TypeId;
                let info = types::with_types(|reg| reg.get(id).cloned());
                let full = info
                    .as_ref()
                    .map_or_else(|| "System.Object".to_string(), |t| t.full_name());
                match name {
                    "get_Assembly" => self.assembly_object(self.assembly_of_type(id)),
                    "get_Name" => {
                        alloc_managed_string(info.as_ref().map_or("Object", |t| t.name.as_str()))
                    }
                    "get_FullName" | "ToString" => alloc_managed_string(&full),
                    "get_Namespace" => alloc_managed_string(
                        info.as_ref().map_or("System", |t| t.namespace.as_str()),
                    ),
                    "get_AssemblyQualifiedName" => {
                        let asm = self.identity(self.assembly_of_type(id)).full_name();
                        alloc_managed_string(&format!("{full}, {asm}"))
                    }
                    "get_BaseType" => match info.as_ref().map_or(0, |t| t.parent) {
                        0 => Value::Null,
                        parent => self.type_object(parent),
                    },
                    "get_IsValueType" => bool_value(info.as_ref().is_some_and(|t| t.is_value)),
                    "get_IsEnum" => bool_value(info.as_ref().is_some_and(|t| t.is_enum)),
                    "get_IsInterface" => bool_value(info.as_ref().is_some_and(|t| t.is_interface)),
                    "get_IsClass" => bool_value(
                        info.as_ref()
                            .is_some_and(|t| !t.is_value && !t.is_interface),
                    ),
                    _ => return None,
                }
            }
            ("System.Reflection", "Assembly", "GetExecutingAssembly") => {
                self.assembly_object(self.frame_assembly(0))
            }
            ("System.Reflection", "Assembly", "GetCallingAssembly") => {
                self.assembly_object(self.frame_assembly(1))
            }
            ("System.Reflection", "Assembly", "GetEntryAssembly") => self.assembly_object(0),
            ("System.Reflection", "Assembly" | "RuntimeAssembly", _) => {
                let Some(handle) = this_handle(args, "System.Reflection", "RuntimeAssembly") else {
                    return match args.first() {
                        Some(Value::Null) => Some(Err(ClrException::null_reference())),
                        Some(Value::Ref(p)) if p.is_null() => {
                            Some(Err(ClrException::null_reference()))
                        }
                        _ => None,
                    };
                };
                let id = self.identity(handle.wrapping_sub(1));
                match name {
                    "get_FullName" | "ToString" => alloc_managed_string(&id.full_name()),
                    "get_Location" => alloc_managed_string(&id.location),
                    "get_CodeBase" => alloc_managed_string(&if id.location.is_empty() {
                        String::new()
                    } else {
                        format!("file:///{}", id.location.replace('\\', "/"))
                    }),
                    "get_ImageRuntimeVersion" => alloc_managed_string("v4.0.30319"),
                    "GetSimpleName" => alloc_managed_string(&id.name),
                    "GetCultureName" => alloc_managed_string(&id.culture),
                    "GetVersionPart" => match args.get(1) {
                        Some(Value::I32(i)) if (0..4).contains(i) => {
                            Value::I32(i32::from(id.version[*i as usize]))
                        }
                        _ => Value::I32(0),
                    },
                    "GetPublicKeyTokenText" => {
                        alloc_managed_string(id.token.as_deref().unwrap_or(""))
                    }
                    _ => return None,
                }
            }
            _ => return None,
        };
        Some(Ok(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nom_complet_d_assembly() {
        let id = Identity {
            name: "Corsair.Service".into(),
            version: [4, 18, 0, 7],
            culture: String::new(),
            token: None,
            location: String::new(),
        };
        assert_eq!(
            id.full_name(),
            "Corsair.Service, Version=4.18.0.7, Culture=neutral, PublicKeyToken=null"
        );
    }

    /// Clé ECMA (16 octets) : son jeton publié est b77a5c561934e089.
    #[test]
    fn jeton_de_la_cle_ecma() {
        let key = [0u8, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(public_key_token(&key), CORLIB_TOKEN);
    }
}
