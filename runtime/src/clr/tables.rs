//! Définitions des tables de métadonnées CLI (ECMA-335 §II.22).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TableId {
    Module = 0x00,
    TypeRef = 0x01,
    TypeDef = 0x02,
    Field = 0x04,
    MethodDef = 0x06,
    Param = 0x08,
    InterfaceImpl = 0x09,
    MemberRef = 0x0A,
    Constant = 0x0B,
    CustomAttribute = 0x0C,
    FieldMarshal = 0x0D,
    DeclSecurity = 0x0E,
    ClassLayout = 0x0F,
    FieldLayout = 0x10,
    StandAloneSig = 0x11,
    EventMap = 0x12,
    Event = 0x14,
    PropertyMap = 0x15,
    Property = 0x17,
    MethodSemantics = 0x18,
    MethodImpl = 0x19,
    ModuleRef = 0x1A,
    TypeSpec = 0x1B,
    ImplMap = 0x1C,
    FieldRva = 0x1D,
    Assembly = 0x20,
    AssemblyRef = 0x23,
    File = 0x26,
    ExportedType = 0x27,
    ManifestResource = 0x28,
    NestedClass = 0x29,
    GenericParam = 0x2A,
    MethodSpec = 0x2B,
    GenericParamConstraint = 0x2C,
}

impl TableId {
    pub const ALL: &[TableId] = &[
        TableId::Module,
        TableId::TypeRef,
        TableId::TypeDef,
        TableId::Field,
        TableId::MethodDef,
        TableId::Param,
        TableId::InterfaceImpl,
        TableId::MemberRef,
        TableId::Constant,
        TableId::CustomAttribute,
        TableId::FieldMarshal,
        TableId::DeclSecurity,
        TableId::ClassLayout,
        TableId::FieldLayout,
        TableId::StandAloneSig,
        TableId::EventMap,
        TableId::Event,
        TableId::PropertyMap,
        TableId::Property,
        TableId::MethodSemantics,
        TableId::MethodImpl,
        TableId::ModuleRef,
        TableId::TypeSpec,
        TableId::ImplMap,
        TableId::FieldRva,
        TableId::Assembly,
        TableId::AssemblyRef,
        TableId::File,
        TableId::ExportedType,
        TableId::ManifestResource,
        TableId::NestedClass,
        TableId::GenericParam,
        TableId::MethodSpec,
        TableId::GenericParamConstraint,
    ];

    pub fn from_u8(v: u8) -> Option<TableId> {
        Self::ALL.iter().find(|&&t| t as u8 == v).copied()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Token(pub u32);

impl Token {
    pub fn new(table: TableId, row: u32) -> Self {
        Self(((table as u32) << 24) | (row & 0x00FF_FFFF))
    }
    pub fn table(self) -> u8 {
        (self.0 >> 24) as u8
    }
    pub fn rid(self) -> u32 {
        self.0 & 0x00FF_FFFF
    }
    pub fn is_nil(self) -> bool {
        self.rid() == 0
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModuleRow {
    pub generation: u16,
    pub name: u32,
    pub mvid: u32,
}

#[derive(Debug, Clone, Default)]
pub struct TypeRefRow {
    pub resolution_scope: u32,
    pub name: u32,
    pub namespace: u32,
}

#[derive(Debug, Clone, Default)]
pub struct TypeDefRow {
    pub flags: u32,
    pub name: u32,
    pub namespace: u32,
    pub extends: u32,
    pub field_list: u32,
    pub method_list: u32,
}

impl TypeDefRow {
    pub fn is_interface(&self) -> bool {
        self.flags & 0x20 != 0
    }
    pub fn is_abstract(&self) -> bool {
        self.flags & 0x80 != 0
    }
    pub fn is_sealed(&self) -> bool {
        self.flags & 0x100 != 0
    }
    pub fn is_value_type(&self) -> bool {
        false
    }
    pub fn visibility(&self) -> u32 {
        self.flags & 0x07
    }
}

#[derive(Debug, Clone, Default)]
pub struct FieldRow {
    pub flags: u16,
    pub name: u32,
    pub signature: u32,
}

impl FieldRow {
    pub fn is_static(&self) -> bool {
        self.flags & 0x10 != 0
    }
}

#[derive(Debug, Clone, Default)]
pub struct MethodDefRow {
    pub rva: u32,
    pub impl_flags: u16,
    pub flags: u16,
    pub name: u32,
    pub signature: u32,
    pub param_list: u32,
}

impl MethodDefRow {
    pub fn is_static(&self) -> bool {
        self.flags & 0x10 != 0
    }
    pub fn is_virtual(&self) -> bool {
        self.flags & 0x40 != 0
    }
    pub fn is_abstract(&self) -> bool {
        self.flags & 0x0400 != 0
    }
    pub fn is_pinvoke(&self) -> bool {
        self.flags & 0x2000 != 0
    }
    pub fn is_cctor(&self) -> bool {
        self.flags & 0x10 != 0 && self.flags & 0x1800 == 0x1800
    }
}

#[derive(Debug, Clone, Default)]
pub struct ParamRow {
    pub flags: u16,
    pub sequence: u16,
    pub name: u32,
}

#[derive(Debug, Clone, Default)]
pub struct MemberRefRow {
    pub class: u32,
    pub name: u32,
    pub signature: u32,
}

#[derive(Debug, Clone, Default)]
pub struct TypeSpecRow {
    pub signature: u32,
}

#[derive(Debug, Clone, Default)]
pub struct StandAloneSigRow {
    pub signature: u32,
}

#[derive(Debug, Clone, Default)]
pub struct InterfaceImplRow {
    pub class: u32,
    pub interface: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ConstantRow {
    pub typ: u8,
    pub parent: u32,
    pub value: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ImplMapRow {
    pub mapping_flags: u16,
    pub member_forwarded: u32,
    pub import_name: u32,
    pub import_scope: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ModuleRefRow {
    pub name: u32,
}

#[derive(Debug, Clone, Default)]
pub struct AssemblyRow {
    pub hash_alg: u32,
    pub major: u16,
    pub minor: u16,
    pub build: u16,
    pub revision: u16,
    pub flags: u32,
    pub public_key: u32,
    pub name: u32,
    pub culture: u32,
}

#[derive(Debug, Clone, Default)]
pub struct AssemblyRefRow {
    pub major: u16,
    pub minor: u16,
    pub build: u16,
    pub revision: u16,
    pub flags: u32,
    pub public_key_or_token: u32,
    pub name: u32,
    pub culture: u32,
    pub hash_value: u32,
}

#[derive(Debug, Clone, Default)]
pub struct NestedClassRow {
    pub nested_class: u32,
    pub enclosing_class: u32,
}

#[derive(Debug, Clone, Default)]
pub struct GenericParamRow {
    pub number: u16,
    pub flags: u16,
    pub owner: u32,
    pub name: u32,
}

#[derive(Debug, Clone, Default)]
pub struct MethodSpecRow {
    pub method: u32,
    pub instantiation: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ClassLayoutRow {
    pub packing_size: u16,
    pub class_size: u32,
    pub parent: u32,
}

#[derive(Debug, Clone, Default)]
pub struct FieldRvaRow {
    pub rva: u32,
    pub field: u32,
}

#[derive(Debug, Clone, Default)]
pub struct CustomAttributeRow {
    pub parent: u32,
    pub typ: u32,
    pub value: u32,
}

#[derive(Debug, Clone, Default)]
pub struct MethodSemanticsRow {
    pub semantics: u16,
    pub method: u32,
    pub association: u32,
}

#[derive(Debug, Clone, Default)]
pub struct MethodImplRow {
    pub class: u32,
    pub method_body: u32,
    pub method_declaration: u32,
}

#[derive(Debug, Clone, Default)]
pub struct PropertyRow {
    pub flags: u16,
    pub name: u32,
    pub typ: u32,
}

#[derive(Debug, Clone, Default)]
pub struct PropertyMapRow {
    pub parent: u32,
    pub property_list: u32,
}

#[derive(Debug, Clone, Default)]
pub struct EventRow {
    pub event_flags: u16,
    pub name: u32,
    pub event_type: u32,
}

#[derive(Debug, Clone, Default)]
pub struct EventMapRow {
    pub parent: u32,
    pub event_list: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ManifestResourceRow {
    pub offset: u32,
    pub flags: u32,
    pub name: u32,
    pub implementation: u32,
}

#[derive(Debug, Clone, Default)]
pub struct GenericParamConstraintRow {
    pub owner: u32,
    pub constraint: u32,
}

/// Index codés (ECMA-335 §II.24.2.6)
#[derive(Debug, Clone, Copy)]
pub enum CodedIndex {
    TypeDefOrRef,
    HasConstant,
    HasCustomAttribute,
    HasFieldMarshal,
    HasDeclSecurity,
    MemberRefParent,
    HasSemantics,
    MethodDefOrRef,
    MemberForwarded,
    Implementation,
    CustomAttributeType,
    ResolutionScope,
    TypeOrMethodDef,
}

impl CodedIndex {
    pub fn tag_bits(self) -> u32 {
        match self {
            Self::TypeDefOrRef => 2,
            Self::HasConstant => 2,
            Self::HasCustomAttribute => 5,
            Self::HasFieldMarshal => 1,
            Self::HasDeclSecurity => 2,
            Self::MemberRefParent => 3,
            Self::HasSemantics => 1,
            Self::MethodDefOrRef => 1,
            Self::MemberForwarded => 1,
            Self::Implementation => 2,
            Self::CustomAttributeType => 3,
            Self::ResolutionScope => 2,
            Self::TypeOrMethodDef => 1,
        }
    }

    pub fn tables(self) -> &'static [Option<TableId>] {
        match self {
            Self::TypeDefOrRef => &[
                Some(TableId::TypeDef),
                Some(TableId::TypeRef),
                Some(TableId::TypeSpec),
            ],
            Self::HasConstant => &[
                Some(TableId::Field),
                Some(TableId::Param),
                Some(TableId::Property),
            ],
            Self::HasCustomAttribute => &[
                Some(TableId::MethodDef),
                Some(TableId::Field),
                Some(TableId::TypeRef),
                Some(TableId::TypeDef),
                Some(TableId::Param),
                Some(TableId::InterfaceImpl),
                Some(TableId::MemberRef),
                Some(TableId::Module),
                None, /*Permission*/
                Some(TableId::Property),
                Some(TableId::Event),
                Some(TableId::StandAloneSig),
                Some(TableId::ModuleRef),
                Some(TableId::TypeSpec),
                Some(TableId::Assembly),
                Some(TableId::AssemblyRef),
                Some(TableId::File),
                Some(TableId::ExportedType),
                Some(TableId::ManifestResource),
                Some(TableId::GenericParam),
                Some(TableId::GenericParamConstraint),
                Some(TableId::MethodSpec),
            ],
            Self::HasFieldMarshal => &[Some(TableId::Field), Some(TableId::Param)],
            Self::HasDeclSecurity => &[
                Some(TableId::TypeDef),
                Some(TableId::MethodDef),
                Some(TableId::Assembly),
            ],
            Self::MemberRefParent => &[
                Some(TableId::TypeDef),
                Some(TableId::TypeRef),
                Some(TableId::ModuleRef),
                Some(TableId::MethodDef),
                Some(TableId::TypeSpec),
            ],
            Self::HasSemantics => &[Some(TableId::Event), Some(TableId::Property)],
            Self::MethodDefOrRef => &[Some(TableId::MethodDef), Some(TableId::MemberRef)],
            Self::MemberForwarded => &[Some(TableId::Field), Some(TableId::MethodDef)],
            Self::Implementation => &[
                Some(TableId::File),
                Some(TableId::AssemblyRef),
                Some(TableId::ExportedType),
            ],
            Self::CustomAttributeType => &[
                None,
                None,
                Some(TableId::MethodDef),
                Some(TableId::MemberRef),
                None,
            ],
            Self::ResolutionScope => &[
                Some(TableId::Module),
                Some(TableId::ModuleRef),
                Some(TableId::AssemblyRef),
                Some(TableId::TypeRef),
            ],
            Self::TypeOrMethodDef => &[Some(TableId::TypeDef), Some(TableId::MethodDef)],
        }
    }

    pub fn decode(self, value: u32) -> (Option<TableId>, u32) {
        let bits = self.tag_bits();
        let tag = value & ((1 << bits) - 1);
        let row = value >> bits;
        let tables = self.tables();
        let table = tables.get(tag as usize).copied().flatten();
        (table, row)
    }
}
