//! Système de types managés .NET pour le runtime CLR Winlux.

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
pub const TYPE_ARRAY: TypeId = 17;
pub const TYPE_EXCEPTION: TypeId = 19;
pub const TYPE_VALUETYPE: TypeId = 20;
pub const TYPE_ENUM: TypeId = 21;
const FIRST_USER_TYPE: TypeId = 256;

/// Taille de l'objet d'un type extérieur à l'assembly (BCL) : assez pour le
/// message d'une exception et quelques champs internes.
pub const EXTERNAL_OBJECT_SIZE: usize = 64;

#[derive(Debug, Clone)]
pub struct TypeInfo {
    pub name: String,
    pub namespace: String,
    pub parent: TypeId,
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<MethodInfo>,
    pub instance_size: usize,
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
}

#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub name: String,
    pub token: u32,
    pub is_virtual: bool,
    pub param_count: usize,
}

pub struct TypeRegistry {
    types: HashMap<TypeId, TypeInfo>,
    name_map: HashMap<String, TypeId>,
    next_id: TypeId,
    pub static_data: HashMap<TypeId, Vec<u8>>,
}

impl TypeRegistry {
    fn new() -> Self {
        let mut reg = TypeRegistry {
            types: HashMap::new(),
            name_map: HashMap::new(),
            next_id: FIRST_USER_TYPE,
            static_data: HashMap::new(),
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
                    fields: Vec::new(),
                    methods: Vec::new(),
                    instance_size: size,
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

    /// Type d'une autre assembly (BCL), créé à la demande. Les noms en
    /// « …Exception » dérivent de System.Exception pour que `catch` les filtre.
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
            fields: Vec::new(),
            methods: Vec::new(),
            instance_size: EXTERNAL_OBJECT_SIZE,
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

    /// Redéfinition la plus dérivée d'une méthode virtuelle, recherchée par nom
    /// et nombre de paramètres en remontant la hiérarchie depuis `type_id`.
    pub fn find_override(&self, type_id: TypeId, name: &str, param_count: usize) -> Option<u32> {
        let mut current = type_id;
        while let Some(ti) = self.get(current) {
            let found = ti
                .methods
                .iter()
                .find(|m| m.is_virtual && m.name == name && m.param_count == param_count);
            if let Some(m) = found {
                return Some(m.token);
            }
            current = ti.parent;
        }
        None
    }

    pub fn static_field_ptr(&mut self, type_id: TypeId, offset: usize) -> *mut u8 {
        let data = self.static_data.entry(type_id).or_default();
        if data.len() < offset + 8 {
            data.resize(offset + 8, 0);
        }
        unsafe { data.as_mut_ptr().add(offset) }
    }
}

static TYPE_REGISTRY: Mutex<Option<TypeRegistry>> = Mutex::new(None);

pub fn init() {
    *TYPE_REGISTRY.lock().unwrap() = Some(TypeRegistry::new());
}

pub fn with_types<T>(f: impl FnOnce(&mut TypeRegistry) -> T) -> T {
    let mut lock = TYPE_REGISTRY.lock().unwrap();
    let reg = lock.as_mut().expect("registre de types CLR non initialisé");
    f(reg)
}
