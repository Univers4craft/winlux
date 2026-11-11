//! Système de types managés .NET pour le runtime CLR Winlux.
//!
//! Chaque type a un identifiant (`TypeId`). Une instanciation générique
//! (`Boite<int>`) est un type à part entière, avec sa disposition mémoire et
//! ses champs statiques, rattaché à sa définition ouverte (`generic_def`).

use std::collections::HashMap;
use std::sync::Mutex;

pub type TypeId = u32;

pub const TYPE_OBJECT: TypeId = 1;
pub const TYPE_STRING: TypeId = 2;
pub const TYPE_INT32: TypeId = 3;
pub const TYPE_INT64: TypeId = 4;
pub const TYPE_FLOAT: TypeId = 5;
pub const TYPE_DOUBLE: TypeId = 6;
pub const TYPE_BOOLEAN: TypeId = 7;
pub const TYPE_CHAR: TypeId = 8;
pub const TYPE_BYTE: TypeId = 9;
pub const TYPE_SBYTE: TypeId = 10;
pub const TYPE_INT16: TypeId = 11;
pub const TYPE_UINT16: TypeId = 12;
pub const TYPE_UINT32: TypeId = 13;
pub const TYPE_UINT64: TypeId = 14;
pub const TYPE_INTPTR: TypeId = 15;
pub const TYPE_UINTPTR: TypeId = 16;
pub const TYPE_ARRAY: TypeId = 17;
pub const TYPE_EXCEPTION: TypeId = 19;
pub const TYPE_VALUETYPE: TypeId = 20;
pub const TYPE_ENUM: TypeId = 21;
pub const FIRST_USER_TYPE: TypeId = 256;

/// Type intégré au moteur (Object, String, primitives…) : sa disposition
/// mémoire est fixée par le runtime, même quand notre mscorlib le définit.
pub fn is_builtin(id: TypeId) -> bool {
    id < FIRST_USER_TYPE
}

pub const EXTERNAL_OBJECT_SIZE: usize = 64;

/// Façon dont une valeur d'un type est rangée en mémoire (champ, variable
/// locale, élément de tableau) et chargée sur la pile d'évaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bool,
    I1,
    U1,
    I2,
    U2,
    Char,
    I4,
    U4,
    I8,
    U8,
    R4,
    R8,
    /// Entier natif ou pointeur (managé ou non).
    I,
    /// Référence d'objet.
    Ref,
    /// Type valeur défini par une assembly, rangé en place.
    Struct(TypeId),
}

impl Kind {
    /// Taille d'un élément de tableau de ce type.
    pub fn elem_size(self) -> usize {
        match self {
            Kind::Bool | Kind::I1 | Kind::U1 => 1,
            Kind::I2 | Kind::U2 | Kind::Char => 2,
            Kind::I4 | Kind::U4 | Kind::R4 => 4,
            Kind::I8 | Kind::U8 | Kind::R8 | Kind::I | Kind::Ref => 8,
            Kind::Struct(id) => with_types(|reg| reg.get(id).map_or(8, |t| t.instance_size)),
        }
    }

    /// Alignement naturel, comme en C (ECMA-335 §II.10.7).
    pub fn align(self) -> usize {
        match self {
            Kind::Struct(id) => with_types(|reg| {
                reg.get(id)
                    .map_or(8, |t| if t.align == 0 { 8 } else { t.align })
            }),
            k => k.elem_size(),
        }
    }

    /// Taille d'un emplacement (champ, variable) : 8 octets au moins, pour
    /// que toute référence reste alignée sur 8 octets (le ramasse-miettes
    /// parcourt la mémoire par mots de 8 octets).
    pub fn slot_size(self) -> usize {
        self.elem_size().div_ceil(8).max(1) * 8
    }

    /// Type valeur primitif d'un identifiant de type de base.
    pub fn of_builtin(id: TypeId) -> Option<Kind> {
        Some(match id {
            TYPE_INT32 => Kind::I4,
            TYPE_INT64 => Kind::I8,
            TYPE_FLOAT => Kind::R4,
            TYPE_DOUBLE => Kind::R8,
            TYPE_BOOLEAN => Kind::Bool,
            TYPE_CHAR => Kind::Char,
            TYPE_BYTE => Kind::U1,
            TYPE_SBYTE => Kind::I1,
            TYPE_INT16 => Kind::I2,
            TYPE_UINT16 => Kind::U2,
            TYPE_UINT32 => Kind::U4,
            TYPE_UINT64 => Kind::U8,
            TYPE_INTPTR | TYPE_UINTPTR => Kind::I,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct TypeInfo {
    pub name: String,
    pub namespace: String,
    pub parent: TypeId,
    pub fields: Vec<FieldInfo>,
    pub static_fields: Vec<FieldInfo>,
    pub methods: Vec<MethodInfo>,
    pub instance_size: usize,
    /// Définition dans une assembly chargée : (assembly, ligne TypeDef).
    pub def: Option<(usize, u32)>,
    /// Nombre de paramètres génériques de la définition.
    pub generic_params: u32,
    /// Arguments d'une instanciation générique.
    pub type_args: Vec<TypeId>,
    pub is_value: bool,
    pub is_enum: bool,
    pub is_interface: bool,
    /// Disposition des champs calculée.
    pub laid_out: bool,
    /// Alignement d'un type valeur (0 : 8 octets).
    pub align: usize,
}

impl TypeInfo {
    pub fn full_name(&self) -> String {
        if self.namespace.is_empty() {
            self.name.clone()
        } else {
            format!("{}.{}", self.namespace, self.name)
        }
    }
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub offset: usize,
    pub metadata_row: u32,
    pub kind: Kind,
}

#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub name: String,
    /// Assembly qui déclare la méthode (indice dans le domaine chargé).
    pub assembly: usize,
    pub token: u32,
    pub is_virtual: bool,
    pub param_count: usize,
    /// Types des paramètres, pour départager deux surcharges de même
    /// arité (Equals(object) et Equals(int), #1956).
    pub params: Vec<super::metadata::ElementType>,
}

/// Deux types de paramètre peuvent-ils désigner le même type ? Les types
/// élémentaires se comparent ; deux jetons (classe, structure) sont propres
/// à leur assembly et s'acceptent.
fn same_param(a: &super::metadata::ElementType, b: &super::metadata::ElementType) -> bool {
    use super::metadata::ElementType as E;
    let simple = |e: &E| {
        matches!(
            e,
            E::Boolean
                | E::Char
                | E::I1
                | E::U1
                | E::I2
                | E::U2
                | E::I4
                | E::U4
                | E::I8
                | E::U8
                | E::R4
                | E::R8
                | E::String
                | E::Object
                | E::IntPtr
                | E::UIntPtr
        )
    };
    let generic = |e: &E| matches!(e, E::Var(_) | E::MVar(_));
    match (simple(a), simple(b)) {
        (true, true) => a == b,
        // Object, String et les primitives ont toujours leur propre code
        // d'élément (ECMA-335 §II.23.2.12) : face à un jeton, seul un
        // paramètre générique peut encore les désigner.
        (true, false) => generic(b),
        (false, true) => generic(a),
        (false, false) => true,
    }
}

pub struct TypeRegistry {
    types: HashMap<TypeId, TypeInfo>,
    name_map: HashMap<String, TypeId>,
    instances: HashMap<(TypeId, Vec<TypeId>), TypeId>,
    next_id: TypeId,
    /// Champs statiques de chaque type, alloués une fois pour toutes : une
    /// adresse prise par `ldsflda` reste valable.
    pub static_data: HashMap<TypeId, Box<[u64]>>,
    /// Réponses de `Interpreter::implements` : (type réel, interface). Le
    /// cache vit avec le registre, car `init` en refait un dont les
    /// numéros de type ne désignent plus les mêmes types (#1978).
    pub implements: HashMap<(TypeId, TypeId), bool>,
}

impl TypeRegistry {
    fn new() -> Self {
        let mut reg = TypeRegistry {
            types: HashMap::new(),
            name_map: HashMap::new(),
            instances: HashMap::new(),
            next_id: FIRST_USER_TYPE,
            static_data: HashMap::new(),
            implements: HashMap::new(),
        };
        let builtins = [
            (TYPE_OBJECT, "Object", 0, 8),
            (TYPE_STRING, "String", TYPE_OBJECT, 8),
            (TYPE_VALUETYPE, "ValueType", TYPE_OBJECT, 8),
            (TYPE_ENUM, "Enum", TYPE_VALUETYPE, 8),
            (TYPE_INT32, "Int32", TYPE_VALUETYPE, 8),
            (TYPE_INT64, "Int64", TYPE_VALUETYPE, 8),
            (TYPE_FLOAT, "Single", TYPE_VALUETYPE, 8),
            (TYPE_DOUBLE, "Double", TYPE_VALUETYPE, 8),
            (TYPE_BOOLEAN, "Boolean", TYPE_VALUETYPE, 8),
            (TYPE_CHAR, "Char", TYPE_VALUETYPE, 8),
            (TYPE_BYTE, "Byte", TYPE_VALUETYPE, 8),
            (TYPE_SBYTE, "SByte", TYPE_VALUETYPE, 8),
            (TYPE_INT16, "Int16", TYPE_VALUETYPE, 8),
            (TYPE_UINT16, "UInt16", TYPE_VALUETYPE, 8),
            (TYPE_UINT32, "UInt32", TYPE_VALUETYPE, 8),
            (TYPE_UINT64, "UInt64", TYPE_VALUETYPE, 8),
            (TYPE_INTPTR, "IntPtr", TYPE_VALUETYPE, 8),
            (TYPE_UINTPTR, "UIntPtr", TYPE_VALUETYPE, 8),
            (TYPE_ARRAY, "Array", TYPE_OBJECT, 8),
            (
                TYPE_EXCEPTION,
                "Exception",
                TYPE_OBJECT,
                EXTERNAL_OBJECT_SIZE,
            ),
        ];
        for (id, name, parent, size) in builtins {
            reg.insert(
                id,
                TypeInfo {
                    name: name.into(),
                    namespace: "System".into(),
                    parent,
                    instance_size: size,
                    is_value: parent == TYPE_VALUETYPE,
                    laid_out: true,
                    ..TypeInfo::default()
                },
            );
        }
        reg
    }

    fn insert(&mut self, id: TypeId, info: TypeInfo) {
        self.name_map.insert(info.full_name(), id);
        self.types.insert(id, info);
    }

    pub fn register(&mut self, info: TypeInfo) -> TypeId {
        let id = self.next_id;
        self.next_id += 1;
        self.insert(id, info);
        id
    }

    /// Instanciation déjà créée de `def` avec `args`.
    pub fn find_instance(&self, def: TypeId, args: &[TypeId]) -> Option<TypeId> {
        self.instances.get(&(def, args.to_vec())).copied()
    }

    /// Enregistre une instanciation générique (sans l'inscrire par nom).
    pub fn register_instance(&mut self, def: TypeId, args: Vec<TypeId>, info: TypeInfo) -> TypeId {
        let id = self.next_id;
        self.next_id += 1;
        self.types.insert(id, info);
        self.instances.insert((def, args), id);
        id
    }

    pub fn ensure_external(&mut self, namespace: &str, name: &str) -> TypeId {
        if let Some(id) = self.find_by_name(namespace, name) {
            return id;
        }
        let parent = if name.ends_with("Exception") {
            TYPE_EXCEPTION
        } else {
            TYPE_OBJECT
        };
        self.register(TypeInfo {
            name: name.into(),
            namespace: namespace.into(),
            parent,
            instance_size: EXTERNAL_OBJECT_SIZE,
            laid_out: true,
            ..TypeInfo::default()
        })
    }

    pub fn get(&self, id: TypeId) -> Option<&TypeInfo> {
        self.types.get(&id)
    }

    pub fn get_mut(&mut self, id: TypeId) -> Option<&mut TypeInfo> {
        self.types.get_mut(&id)
    }

    pub fn find_by_name(&self, namespace: &str, name: &str) -> Option<TypeId> {
        let full = if namespace.is_empty() {
            name.to_string()
        } else {
            format!("{namespace}.{name}")
        };
        self.name_map.get(&full).copied()
    }

    /// `child` est `parent`, en dérive, ou est une instanciation de la
    /// définition générique `parent`.
    pub fn is_subclass_of(&self, child: TypeId, parent: TypeId) -> bool {
        let mut current = child;
        while current != 0 {
            if current == parent {
                return true;
            }
            current = self.get(current).map_or(0, |t| t.parent);
        }
        false
    }

    /// Redéfinition virtuelle la plus dérivée : (assembly, jeton MethodDef,
    /// type qui la déclare, éventuellement instancié).
    pub fn find_override(
        &self,
        type_id: TypeId,
        name: &str,
        param_count: usize,
    ) -> Option<(usize, u32, TypeId)> {
        self.find_override_of(type_id, name, param_count, None, &[])
    }

    /// Comme `find_override`, pour une méthode de l'interface `iface`
    /// (« Espace.Nom », sans arité) : une implémentation explicite
    /// (« Espace.Nom<T>.Méthode », ECMA-335 §II.10.3.2) passe avant la
    /// méthode publique de même nom, à chaque niveau (#1896).
    pub fn find_override_of(
        &self,
        type_id: TypeId,
        name: &str,
        param_count: usize,
        iface: Option<&str>,
        params: &[super::metadata::ElementType],
    ) -> Option<(usize, u32, TypeId)> {
        let explicit = iface.map(|i| format!("{i}.{name}"));
        let mut current = type_id;
        while let Some(ti) = self.get(current) {
            let found = explicit
                .as_deref()
                .and_then(|e| {
                    ti.methods.iter().find(|m| {
                        m.is_virtual
                            && m.param_count == param_count
                            && m.name.contains('.')
                            && strip_generic_args(&m.name) == e
                    })
                })
                .or_else(|| {
                    ti.methods.iter().find(|m| {
                        m.is_virtual
                            && m.name == name
                            && m.param_count == param_count
                            && (params.len() != m.params.len()
                                || params.iter().zip(&m.params).all(|(a, b)| same_param(a, b)))
                    })
                });
            if let Some(m) = found {
                return Some((m.assembly, m.token, current));
            }
            current = ti.parent;
        }
        None
    }

    /// Emplacement d'un champ statique ; la zone du type est créée à la
    /// première demande avec `size` octets, et ne bouge plus ensuite.
    pub fn static_field_ptr(&mut self, type_id: TypeId, offset: usize, size: usize) -> *mut u8 {
        let words = (offset + size.max(8)).div_ceil(8);
        let data = self
            .static_data
            .entry(type_id)
            .or_insert_with(|| vec![0u64; words.max(4)].into_boxed_slice());
        if data.len() < words {
            // Zone trop petite (champ de la bibliothèque intégrée découvert
            // après coup) : elle n'est agrandie qu'avant toute prise d'adresse.
            let mut bigger = vec![0u64; words * 2];
            bigger[..data.len()].copy_from_slice(data);
            *data = bigger.into_boxed_slice();
        }
        unsafe { (data.as_mut_ptr() as *mut u8).add(offset) }
    }
}

static TYPE_REGISTRY: Mutex<Option<TypeRegistry>> = Mutex::new(None);

pub fn init() {
    *TYPE_REGISTRY.lock().unwrap() = Some(TypeRegistry::new());
}

thread_local! {
    /// Vrai pendant qu'un `with_types` de ce fil tient le registre.
    static TYPES_HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Accès exclusif au registre de types. Un appel imbriqué sur le même fil
/// bloquerait le processus pour toujours (#1687) : il s'arrête plutôt avec
/// un message qui le dit.
pub fn with_types<T>(f: impl FnOnce(&mut TypeRegistry) -> T) -> T {
    struct Held;
    impl Drop for Held {
        fn drop(&mut self) {
            TYPES_HELD.with(|h| h.set(false));
        }
    }
    assert!(
        !TYPES_HELD.with(|h| h.replace(true)),
        "registre de types CLR repris par le fil qui le tient déjà (interblocage évité)"
    );
    let _held = Held;
    let mut lock = TYPE_REGISTRY.lock().unwrap();
    let reg = lock.as_mut().expect("registre de types CLR non initialisé");
    f(reg)
}

/// « A.B<T, C<D>>.M » → « A.B.M » : le nom d'une implémentation explicite
/// sans ses arguments génériques.
pub fn strip_generic_args(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut depth = 0u32;
    for c in name.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}
