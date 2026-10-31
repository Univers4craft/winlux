//! Système de types managés .NET pour le runtime CLR Winlux.

use std::collections::HashMap;
use std::sync::Mutex;

use super::metadata::ElementType;

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
pub const TYPE_VOID: TypeId = 16;
pub const TYPE_ARRAY: TypeId = 17;
pub const TYPE_TYPE: TypeId = 18;
pub const TYPE_EXCEPTION: TypeId = 19;
pub const FIRST_USER_TYPE: TypeId = 256;

#[derive(Debug, Clone)]
pub struct TypeInfo {
    pub id: TypeId,
    pub name: String,
    pub namespace: String,
    pub parent: TypeId,
    pub flags: u32,
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<MethodInfo>,
    pub static_fields: Vec<FieldInfo>,
    pub vtable: Vec<u32>,
    pub interfaces: Vec<TypeId>,
    pub instance_size: usize,
    pub is_value_type: bool,
    pub is_enum: bool,
    pub is_interface: bool,
    pub is_abstract: bool,
    pub generic_params: Vec<String>,
    pub metadata_row: u32,
    pub cctor_run: bool,
}

impl TypeInfo {
    pub fn full_name(&self) -> String {
        if self.namespace.is_empty() {
            self.name.clone()
        } else {
            format!("{}.{}", self.namespace, self.name)
        }
    }

    pub fn find_method(&self, name: &str) -> Option<&MethodInfo> {
        self.methods.iter().find(|m| m.name == name)
    }

    pub fn find_field(&self, name: &str) -> Option<(usize, &FieldInfo)> {
        self.fields.iter().enumerate().find(|(_, f)| f.name == name)
    }

    pub fn find_static_field(&self, name: &str) -> Option<(usize, &FieldInfo)> {
        self.static_fields.iter().enumerate().find(|(_, f)| f.name == name)
    }
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    pub field_type: ElementType,
    pub offset: usize,
    pub metadata_row: u32,
}

#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub name: String,
    pub token: u32,
    pub rva: u32,
    pub flags: u16,
    pub impl_flags: u16,
    pub is_static: bool,
    pub is_virtual: bool,
    pub is_abstract: bool,
    pub is_pinvoke: bool,
    pub signature_blob: u32,
    pub param_count: usize,
    pub vtable_slot: Option<usize>,
    pub metadata_row: u32,
}

pub struct TypeRegistry {
    types: HashMap<TypeId, TypeInfo>,
    name_map: HashMap<String, TypeId>,
    next_id: TypeId,
    pub static_data: HashMap<TypeId, Vec<u8>>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        let mut reg = TypeRegistry {
            types: HashMap::new(),
            name_map: HashMap::new(),
            next_id: FIRST_USER_TYPE,
            static_data: HashMap::new(),
        };
        reg.register_builtins();
        reg
    }

    fn register_builtins(&mut self) {
        let builtins = [
            (TYPE_OBJECT, "Object", "System", 0, false),
            (TYPE_STRING, "String", "System", TYPE_OBJECT, false),
            (TYPE_INT32, "Int32", "System", TYPE_OBJECT, true),
            (TYPE_INT64, "Int64", "System", TYPE_OBJECT, true),
            (TYPE_FLOAT, "Single", "System", TYPE_OBJECT, true),
            (TYPE_DOUBLE, "Double", "System", TYPE_OBJECT, true),
            (TYPE_BOOLEAN, "Boolean", "System", TYPE_OBJECT, true),
            (TYPE_CHAR, "Char", "System", TYPE_OBJECT, true),
            (TYPE_BYTE, "Byte", "System", TYPE_OBJECT, true),
            (TYPE_SBYTE, "SByte", "System", TYPE_OBJECT, true),
            (TYPE_INT16, "Int16", "System", TYPE_OBJECT, true),
            (TYPE_UINT16, "UInt16", "System", TYPE_OBJECT, true),
            (TYPE_UINT32, "UInt32", "System", TYPE_OBJECT, true),
            (TYPE_UINT64, "UInt64", "System", TYPE_OBJECT, true),
            (TYPE_INTPTR, "IntPtr", "System", TYPE_OBJECT, true),
            (TYPE_VOID, "Void", "System", 0, true),
            (TYPE_ARRAY, "Array", "System", TYPE_OBJECT, false),
            (TYPE_TYPE, "Type", "System", TYPE_OBJECT, false),
            (TYPE_EXCEPTION, "Exception", "System", TYPE_OBJECT, false),
        ];
        for (id, name, ns, parent, is_vt) in builtins {
            let ti = TypeInfo {
                id,
                name: name.to_string(),
                namespace: ns.to_string(),
                parent,
                flags: 0,
                fields: Vec::new(),
                methods: Vec::new(),
                static_fields: Vec::new(),
                vtable: Vec::new(),
                interfaces: Vec::new(),
                instance_size: if is_vt { element_size_of_builtin(id) } else { 8 },
                is_value_type: is_vt,
                is_enum: false,
                is_interface: false,
                is_abstract: false,
                generic_params: Vec::new(),
                metadata_row: 0,
                cctor_run: true,
            };
            let full = ti.full_name();
            self.types.insert(id, ti);
            self.name_map.insert(full, id);
        }
    }

    pub fn register(&mut self, mut info: TypeInfo) -> TypeId {
        let id = self.next_id;
        self.next_id += 1;
        info.id = id;
        let full = info.full_name();
        self.types.insert(id, info);
        self.name_map.insert(full, id);
        id
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
        if child == parent { return true; }
        let mut current = child;
        loop {
            let Some(ti) = self.get(current) else { return false; };
            if ti.parent == 0 { return false; }
            if ti.parent == parent { return true; }
            current = ti.parent;
        }
    }

    pub fn alloc_static_data(&mut self, type_id: TypeId, size: usize) {
        self.static_data.entry(type_id).or_insert_with(|| vec![0u8; size]);
    }

    pub fn static_field_ptr(&mut self, type_id: TypeId, offset: usize) -> *mut u8 {
        let data = self.static_data.entry(type_id).or_insert_with(|| vec![0u8; 256]);
        if offset < data.len() {
            unsafe { data.as_mut_ptr().add(offset) }
        } else {
            data.resize(offset + 8, 0);
            unsafe { data.as_mut_ptr().add(offset) }
        }
    }
}

pub fn element_size(et: &ElementType) -> usize {
    match et {
        ElementType::Boolean | ElementType::I1 | ElementType::U1 => 1,
        ElementType::Char | ElementType::I2 | ElementType::U2 => 2,
        ElementType::I4 | ElementType::U4 | ElementType::R4 => 4,
        ElementType::I8 | ElementType::U8 | ElementType::R8 => 8,
        ElementType::IntPtr | ElementType::UIntPtr => 8,
        ElementType::String | ElementType::Object | ElementType::Class(_)
        | ElementType::SzArray(_) | ElementType::Array(_) => 8,
        ElementType::ValueType(_) => 8,
        ElementType::GenericInst(_, _) => 8,
        ElementType::ByRef(_) | ElementType::Ptr(_) => 8,
        ElementType::Void => 0,
        _ => 8,
    }
}

fn element_size_of_builtin(id: TypeId) -> usize {
    match id {
        TYPE_BOOLEAN | TYPE_BYTE | TYPE_SBYTE => 1,
        TYPE_CHAR | TYPE_INT16 | TYPE_UINT16 => 2,
        TYPE_INT32 | TYPE_UINT32 | TYPE_FLOAT => 4,
        TYPE_INT64 | TYPE_UINT64 | TYPE_DOUBLE => 8,
        TYPE_INTPTR => 8,
        _ => 8,
    }
}

pub fn element_type_to_type_id(et: &ElementType) -> TypeId {
    match et {
        ElementType::Boolean => TYPE_BOOLEAN,
        ElementType::Char => TYPE_CHAR,
        ElementType::I1 => TYPE_SBYTE,
        ElementType::U1 => TYPE_BYTE,
        ElementType::I2 => TYPE_INT16,
        ElementType::U2 => TYPE_UINT16,
        ElementType::I4 => TYPE_INT32,
        ElementType::U4 => TYPE_UINT32,
        ElementType::I8 => TYPE_INT64,
        ElementType::U8 => TYPE_UINT64,
        ElementType::R4 => TYPE_FLOAT,
        ElementType::R8 => TYPE_DOUBLE,
        ElementType::IntPtr => TYPE_INTPTR,
        ElementType::String => TYPE_STRING,
        ElementType::Object => TYPE_OBJECT,
        ElementType::Void => TYPE_VOID,
        _ => TYPE_OBJECT,
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
