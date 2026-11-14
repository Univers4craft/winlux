//! `System.Runtime.InteropServices.Marshal` côté moteur.
//!
//! - Taille d'un type marshalé (#2026) : `SizeOf(Type)`, `SizeOf(object)`,
//!   `SizeOf<T>()`. Ce n'est pas la disposition gérée : un `bool` devient
//!   un BOOL Win32 de 4 octets, un `char` un octet ANSI (deux avec
//!   `CharSet.Unicode` ou `Auto`), une chaîne un pointeur, sauf
//!   `MarshalAs(ByValTStr | ByValArray, SizeConst = n)` qui la range dans la
//!   structure. `StructLayout(Pack, Size)` et `FieldOffset` sont suivis.
//! - Copie d'une structure vers la mémoire native et retour (#2105) :
//!   `StructureToPtr`, `PtrToStructure`, `DestroyStructure`, `OffsetOf`,
//!   avec la même liste de champs natifs que `SizeOf`. Les chaînes pointées
//!   sont allouées par CoTaskMemAlloc, comme sous Windows.
//! - Mémoire native (`AllocHGlobal`, `AllocCoTaskMem`…) et chaînes natives
//!   (`PtrToStringAnsi`, `StringToHGlobalUni`, BSTR…).
//!
//! Sources : ECMA-335 §II.10.7, §II.22.17, §II.23.4 ; documentation
//! publique de .NET Framework 4.x sur MS Learn (« Marshal Class »,
//! « Marshal.SizeOf », « Marshal.StructureToPtr », « Default Marshaling for
//! Value Types », « Default Marshaling for Strings », « UnmanagedType »).
//! Aucun code de Microsoft, de Mono ou de Wine.

use super::bcl;
use super::exceptions::ClrException;
use super::gc;
use super::il::{self, Gen, Interpreter, Value};
use super::metadata::{self, ElementType};
use super::reflection::type_id_of;
use super::types::{self, Kind, TypeId};

const FIELD_STATIC: u16 = 0x10;
const LAYOUT_MASK: u32 = 0x18;
const LAYOUT_EXPLICIT: u32 = 0x10;
const STRING_FORMAT_MASK: u32 = 0x3_0000;
/// NATIVE_TYPE_* (ECMA-335 §II.23.4) et UnmanagedType (MS Learn).
const NATIVE_BOOLEAN: u8 = 0x02;
const NATIVE_I1: u8 = 0x03;
const NATIVE_U1: u8 = 0x04;
const NATIVE_I2: u8 = 0x05;
const NATIVE_U2: u8 = 0x06;
const NATIVE_BSTR: u8 = 0x13;
const NATIVE_LPSTR: u8 = 0x14;
const NATIVE_LPWSTR: u8 = 0x15;
const NATIVE_LPTSTR: u8 = 0x16;
const NATIVE_BYVALSTR: u8 = 0x17;
const NATIVE_BYVALARRAY: u8 = 0x1E;
const NATIVE_VARIANTBOOL: u8 = 0x25;
/// CP_ACP : la page de code ANSI du système.
const CP_ACP: u32 = 0;

/// Forme native d'une chaîne pointée.
#[derive(Debug, Clone, Copy, PartialEq)]
enum StrFormat {
    Ansi,
    Wide,
    Bstr,
}

/// Comment un champ passe de sa forme gérée à sa forme native.
#[derive(Debug, Clone)]
enum Native {
    /// Mêmes octets des deux côtés (entiers, flottants, IntPtr, énumérations).
    Plain,
    /// `bool` géré (1 octet) : BOOL (4), I1/U1 (1) ou VARIANT_BOOL (2, -1).
    Bool { variant: bool },
    /// `char` géré : un octet ANSI ou une unité UTF-16.
    Char { wide: bool },
    /// Chaîne : un pointeur vers une copie native.
    StrPtr(StrFormat),
    /// `ByValTStr` : `count` caractères dans la structure, zéro final compris.
    ByValStr { count: usize, wide: bool },
    /// `ByValArray` : `count` éléments dans la structure.
    ByValArray {
        count: usize,
        elem: TypeId,
        kind: Kind,
        item: Box<Item>,
    },
    /// Type valeur imbriqué.
    Struct(TypeId),
    /// Délégué : pointeur de fonction natif qui le rappelle (#2382).
    FnPtr { wide: bool },
    /// Autre référence (tableau, classe) : un pointeur nul.
    NullPtr,
}

#[derive(Debug, Clone)]
struct Item {
    how: Native,
    size: usize,
    align: usize,
}

#[derive(Debug, Clone)]
struct Field {
    /// Ligne Field dans l'assembly du type.
    row: u32,
    managed: usize,
    kind: Kind,
    native: usize,
    item: Item,
}

#[derive(Debug, Clone)]
struct Layout {
    asm: usize,
    fields: Vec<Field>,
    size: usize,
    align: usize,
}

fn not_marshalable(name: &str) -> ClrException {
    ClrException::new(
        "System",
        "ArgumentException",
        &format!(
            "Type '{name}' cannot be marshaled as an unmanaged structure; no meaningful size or offset can be computed."
        ),
    )
}

fn argument_null(param: &str) -> ClrException {
    ClrException::new(
        "System",
        "ArgumentNullException",
        &format!("Value cannot be null.\r\nParameter name: {param}"),
    )
}

fn out_of_memory() -> ClrException {
    ClrException::new(
        "System",
        "OutOfMemoryException",
        "Insufficient memory to continue the execution of the program.",
    )
}

fn type_name(id: TypeId) -> String {
    types::with_types(|reg| reg.get(id).map(|t| t.full_name())).unwrap_or_default()
}

fn int_ptr(v: Option<&Value>) -> usize {
    v.map_or(0, |v| v.as_i64() as usize)
}

impl Interpreter {
    /// Membres de `Marshal` servis par le moteur : `Some` si l'appel est
    /// pour nous.
    pub(super) fn marshal_call(
        &mut self,
        namespace: &str,
        class: &str,
        name: &str,
        args: &[Value],
        gen: Option<&Gen>,
    ) -> Option<Result<Value, ClrException>> {
        if (namespace, class) != ("System.Runtime.InteropServices", "Marshal") {
            return None;
        }
        Some(match name {
            "SizeOf" => self.size_of(args, gen)?,
            "Allouer" => allocate(args),
            "Liberer" => {
                free(
                    int_ptr(args.first()),
                    args.get(1).is_some_and(|v| v.as_i32() != 0),
                );
                Ok(Value::Null)
            }
            "Reallouer" => reallocate(args),
            "VersNatif" => self.structure_to_native(args),
            "DepuisNatif" => self.structure_from_native(args),
            "RemplirDepuisNatif" => self.fill_from_native(args),
            "Detruire" => self.destroy(args),
            "Decalage" => self.offset_of(args),
            "LireChaine" => Ok(read_string(args)),
            "EcrireChaine" => write_string(args),
            "LibererBstr" => {
                let p = int_ptr(args.first());
                if p != 0 {
                    unsafe { crate::dlls::oleaut32::sys_free_string(p as *mut u16) };
                }
                Ok(Value::Null)
            }
            "ExceptionSysteme" => Ok(system_exception(args)),
            _ => return None,
        })
    }

    fn size_of(
        &mut self,
        args: &[Value],
        gen: Option<&Gen>,
    ) -> Option<Result<Value, ClrException>> {
        // SizeOf<T>() et SizeOf<T>(T) : l'argument de type de la méthode.
        let generic = gen.and_then(|g| g.m.first().copied());
        let id = match (generic, args.first()) {
            (Some(t), _) => t,
            // SizeOf(Type t) ou SizeOf(object structure).
            (None, Some(Value::Ref(p))) if !p.is_null() => match type_id_of(*p) {
                Some(id) => id,
                None => gc::type_of(*p)?,
            },
            (None, Some(_)) => {
                return Some(Err(ClrException::new(
                    "System",
                    "ArgumentNullException",
                    "Value cannot be null.",
                )))
            }
            (None, None) => return None,
        };
        Some(
            self.marshal_size(id)
                .map(|(size, _)| Value::I32(size as i32)),
        )
    }

    /// (taille, alignement) du type `id` une fois marshalé.
    pub(super) fn marshal_size(&self, id: TypeId) -> Result<(usize, usize), ClrException> {
        if let Some(p) = primitive(id, false) {
            return Ok(p);
        }
        if types::with_types(|reg| reg.get(id).is_some_and(|t| t.is_enum)) {
            let size = self.kind_of_type(id).elem_size();
            return Ok((size, size));
        }
        self.native_layout(id, 0).map(|l| (l.size, l.align))
    }

    /// Champs natifs d'un type valeur ou d'une classe à disposition
    /// séquentielle ou explicite.
    fn native_layout(&self, id: TypeId, depth: usize) -> Result<Layout, ClrException> {
        let name = type_name(id);
        if depth > 32 {
            return Err(not_marshalable(&name));
        }
        let (is_value, is_enum, has_args) = types::with_types(|reg| {
            reg.get(id)
                .map(|t| (t.is_value, t.is_enum, !t.type_args.is_empty()))
        })
        .unwrap_or((false, false, false));
        if is_enum || primitive(id, false).is_some() {
            return Err(not_marshalable(&name));
        }
        // Les types génériques ne se marshalent pas (MS Learn).
        let Some((asm, row)) = self.def_of(id).filter(|_| !has_args) else {
            return Err(not_marshalable(&name));
        };
        let a = self.assembly_at(asm);
        let flags = a.get_type_def(row).flags;
        if !is_value && flags & LAYOUT_MASK == 0 {
            // Classe en disposition automatique : rien à marshaler.
            return Err(not_marshalable(&name));
        }
        let explicit = flags & LAYOUT_MASK == LAYOUT_EXPLICIT;
        let wide = flags & STRING_FORMAT_MASK != 0;
        let (pack, class_size) = a.class_layout_of(row).unwrap_or((0, 0));
        let gen = self.gen_of(id);
        self.ensure_layout(id);
        let managed =
            types::with_types(|reg| reg.get(id).map(|t| t.fields.clone())).unwrap_or_default();
        let rows: Vec<u32> = a.type_fields(row).collect();
        let (mut end, mut max_align) = (0usize, 1usize);
        let mut fields = Vec::new();
        for f in rows {
            let a = self.assembly_at(asm);
            let fd = a.get_field(f);
            if fd.flags & FIELD_STATIC != 0 {
                continue;
            }
            let blob = a.get_blob(fd.signature);
            let mut off = 1;
            let et = metadata::decode_type(blob, &mut off);
            let native = a.field_marshal_of(f).map(<[u8]>::to_vec);
            let at_declared = if explicit { a.field_offset_of(f) } else { None };
            let item = self.field_item(asm, &et, &gen, native.as_deref(), wide, depth)?;
            let align = if pack > 0 {
                item.align.min(pack as usize)
            } else {
                item.align
            }
            .max(1);
            max_align = max_align.max(align);
            let at = match at_declared {
                Some(o) => o as usize,
                None => end.div_ceil(align) * align,
            };
            end = end.max(at + item.size);
            if let Some(m) = managed.iter().find(|m| m.metadata_row == f) {
                fields.push(Field {
                    row: f,
                    managed: m.offset,
                    kind: m.kind,
                    native: at,
                    item,
                });
            }
        }
        let size = end.max(class_size as usize).div_ceil(max_align).max(1) * max_align;
        Ok(Layout {
            asm,
            fields,
            size,
            align: max_align,
        })
    }

    fn field_item(
        &self,
        asm: usize,
        et: &ElementType,
        gen: &Gen,
        native: Option<&[u8]>,
        wide: bool,
        depth: usize,
    ) -> Result<Item, ClrException> {
        let char_size = if wide { 2 } else { 1 };
        let item = |how, size, align| Ok(Item { how, size, align });
        match (native.and_then(|n| n.first()).copied(), et) {
            (Some(NATIVE_BYVALSTR), _) => {
                let count = size_const(native.unwrap_or_default());
                return item(
                    Native::ByValStr { count, wide },
                    count * char_size,
                    char_size,
                );
            }
            (Some(NATIVE_BYVALARRAY), ElementType::SzArray(inner)) => {
                let count = size_const(native.unwrap_or_default());
                let elem = self.resolve_sig(asm, inner, gen);
                let one = self.type_item(elem, wide, depth)?;
                let (size, align) = (one.size * count, one.align);
                let kind = self.kind_of_type(elem);
                return item(
                    Native::ByValArray {
                        count,
                        elem,
                        kind,
                        item: Box::new(one),
                    },
                    size,
                    align,
                );
            }
            (Some(NATIVE_BOOLEAN), ElementType::Boolean) => {
                return item(Native::Bool { variant: false }, 4, 4)
            }
            (Some(NATIVE_I1 | NATIVE_U1), ElementType::Boolean) => {
                return item(Native::Bool { variant: false }, 1, 1)
            }
            (Some(NATIVE_VARIANTBOOL), ElementType::Boolean) => {
                return item(Native::Bool { variant: true }, 2, 2)
            }
            (Some(NATIVE_I1 | NATIVE_U1), ElementType::Char) => {
                return item(Native::Char { wide: false }, 1, 1)
            }
            (Some(NATIVE_I2 | NATIVE_U2), ElementType::Char) => {
                return item(Native::Char { wide: true }, 2, 2)
            }
            (Some(NATIVE_LPSTR), ElementType::String) => {
                return item(Native::StrPtr(StrFormat::Ansi), 8, 8)
            }
            (Some(NATIVE_LPWSTR | NATIVE_LPTSTR), ElementType::String) => {
                return item(Native::StrPtr(StrFormat::Wide), 8, 8)
            }
            (Some(NATIVE_BSTR), ElementType::String) => {
                return item(Native::StrPtr(StrFormat::Bstr), 8, 8)
            }
            _ => {}
        }
        match et {
            ElementType::Ptr(_) | ElementType::FnPtr => item(Native::Plain, 8, 8),
            ElementType::SzArray(_) | ElementType::Object => item(Native::NullPtr, 8, 8),
            _ => self.type_item(self.resolve_sig(asm, et, gen), wide, depth),
        }
    }

    fn type_item(&self, id: TypeId, wide: bool, depth: usize) -> Result<Item, ClrException> {
        let how = match id {
            types::TYPE_BOOLEAN => Native::Bool { variant: false },
            types::TYPE_CHAR => Native::Char { wide },
            types::TYPE_STRING => {
                return Ok(Item {
                    how: Native::StrPtr(if wide {
                        StrFormat::Wide
                    } else {
                        StrFormat::Ansi
                    }),
                    size: 8,
                    align: 8,
                })
            }
            _ => Native::Plain,
        };
        if let Some((size, align)) = primitive(id, wide) {
            return Ok(Item { how, size, align });
        }
        let (is_value, is_enum) =
            types::with_types(|reg| reg.get(id).map(|t| (t.is_value, t.is_enum)))
                .unwrap_or((false, false));
        if is_enum {
            let size = self.kind_of_type(id).elem_size();
            return Ok(Item {
                how: Native::Plain,
                size,
                align: size,
            });
        }
        if is_delegate_type(id) {
            return Ok(Item {
                how: Native::FnPtr { wide },
                size: 8,
                align: 8,
            });
        }
        if is_value {
            let l = self.native_layout(id, depth + 1)?;
            return Ok(Item {
                how: Native::Struct(id),
                size: l.size,
                align: l.align,
            });
        }
        Ok(Item {
            how: Native::NullPtr,
            size: 8,
            align: 8,
        })
    }

    /// Type d'un objet `System.Type` passé en argument.
    fn type_arg(&self, v: Option<&Value>, param: &str) -> Result<TypeId, ClrException> {
        match v {
            Some(Value::Ref(p)) if !p.is_null() => type_id_of(*p).ok_or_else(|| {
                ClrException::new(
                    "System",
                    "ArgumentException",
                    "Type must be a runtime Type object.",
                )
            }),
            _ => Err(argument_null(param)),
        }
    }

    /// Objet du tas passé en argument, avec son type.
    fn object_arg(v: Option<&Value>, param: &str) -> Result<(*mut u8, TypeId), ClrException> {
        match v {
            Some(Value::Ref(p)) if !p.is_null() => match gc::type_of(*p) {
                Some(id) => Ok((*p, id)),
                None => Err(argument_null(param)),
            },
            _ => Err(argument_null(param)),
        }
    }

    /// VersNatif(object structure, IntPtr ptr, bool fDeleteOld).
    fn structure_to_native(&mut self, args: &[Value]) -> Result<Value, ClrException> {
        let (obj, id) = Self::object_arg(args.first(), "structure")?;
        let dst = int_ptr(args.get(1)) as *mut u8;
        if dst.is_null() {
            return Err(argument_null("ptr"));
        }
        let layout = self.native_layout(id, 0)?;
        unsafe {
            if args.get(2).is_some_and(|v| v.as_i32() != 0) {
                self.destroy_fields(&layout, dst);
            }
            self.write_fields(&layout, obj, dst);
        }
        Ok(Value::Null)
    }

    /// DepuisNatif(IntPtr ptr, Type structureType) : un nouvel objet. Notre
    /// mscorlib n'appelle cette primitive que pour un type valeur ; une
    /// classe passe par son constructeur puis par RemplirDepuisNatif.
    fn structure_from_native(&mut self, args: &[Value]) -> Result<Value, ClrException> {
        let src = int_ptr(args.first()) as *const u8;
        let id = self.type_arg(args.get(1), "structureType")?;
        if src.is_null() {
            return Ok(Value::Null);
        }
        let layout = self.native_layout(id, 0)?;
        let size = types::with_types(|reg| reg.get(id).map_or(8, |t| t.instance_size)).max(8);
        let obj = gc::with_gc(|gc| gc.alloc(id, size));
        self.pins.push(Value::Ref(obj));
        unsafe { self.read_fields(&layout, src, obj) };
        self.pins.pop();
        Ok(Value::Ref(obj))
    }

    /// RemplirDepuisNatif(IntPtr ptr, object structure).
    fn fill_from_native(&mut self, args: &[Value]) -> Result<Value, ClrException> {
        let src = int_ptr(args.first()) as *const u8;
        let (obj, id) = Self::object_arg(args.get(1), "structure")?;
        if src.is_null() {
            return Ok(Value::Null);
        }
        let layout = self.native_layout(id, 0)?;
        unsafe { self.read_fields(&layout, src, obj) };
        Ok(Value::Null)
    }

    /// Detruire(IntPtr ptr, Type structureType) : libère ce que les champs
    /// pointent (chaînes), pas le bloc lui-même.
    fn destroy(&mut self, args: &[Value]) -> Result<Value, ClrException> {
        let ptr = int_ptr(args.first()) as *mut u8;
        let id = self.type_arg(args.get(1), "structureType")?;
        if ptr.is_null() {
            return Err(argument_null("ptr"));
        }
        let layout = self.native_layout(id, 0)?;
        unsafe { self.destroy_fields(&layout, ptr) };
        Ok(Value::Null)
    }

    /// Decalage(Type t, string fieldName) : décalage natif du champ.
    fn offset_of(&mut self, args: &[Value]) -> Result<Value, ClrException> {
        let id = self.type_arg(args.first(), "t")?;
        let field = match args.get(1) {
            Some(Value::Ref(p)) if !p.is_null() => bcl::read_managed_string(*p),
            _ => return Err(argument_null("fieldName")),
        };
        let layout = self.native_layout(id, 0)?;
        let a = self.assembly_at(layout.asm);
        layout
            .fields
            .iter()
            .find(|f| a.get_string(a.get_field(f.row).name) == field)
            .map(|f| Value::I32(f.native as i32))
            .ok_or_else(|| {
                ClrException::new(
                    "System",
                    "ArgumentException",
                    &format!(
                        "Field passed in is not a marshaled member of the type '{}'.\r\nParameter name: fieldName",
                        type_name(id)
                    ),
                )
            })
    }

    unsafe fn write_fields(&mut self, layout: &Layout, src: *const u8, dst: *mut u8) {
        for f in &layout.fields {
            self.write_item(&f.item, f.kind, src.add(f.managed), dst.add(f.native));
        }
    }

    unsafe fn write_item(&mut self, item: &Item, kind: Kind, src: *const u8, dst: *mut u8) {
        match &item.how {
            Native::Plain => {
                std::ptr::copy(src, dst, item.size.min(kind.elem_size()));
            }
            Native::Bool { variant } => {
                let on = *src != 0;
                match (item.size, variant) {
                    (2, true) => (dst as *mut i16).write_unaligned(if on { -1 } else { 0 }),
                    (1, _) => *dst = on as u8,
                    _ => (dst as *mut i32).write_unaligned(on as i32),
                }
            }
            Native::Char { wide } => {
                let c = (src as *const u16).read_unaligned();
                if *wide {
                    (dst as *mut u16).write_unaligned(c);
                } else {
                    *dst = to_ansi(&[c]).first().copied().unwrap_or(b'?');
                }
            }
            Native::StrPtr(format) => {
                let s = (src as *const *mut u8).read_unaligned();
                let p = if s.is_null() {
                    0
                } else {
                    native_string(&bcl::managed_utf16(s), *format, true)
                };
                (dst as *mut usize).write_unaligned(p);
            }
            Native::ByValStr { count, wide } => {
                let width = if *wide { 2 } else { 1 };
                std::ptr::write_bytes(dst, 0, count * width);
                let s = (src as *const *mut u8).read_unaligned();
                if s.is_null() || *count == 0 {
                    return;
                }
                let units = bcl::managed_utf16(s);
                if *wide {
                    let n = units.len().min(count - 1);
                    std::ptr::copy_nonoverlapping(units.as_ptr(), dst as *mut u16, n);
                } else {
                    let bytes = to_ansi(&units);
                    let n = bytes.len().min(count - 1);
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, n);
                }
            }
            Native::ByValArray {
                count,
                kind,
                item: one,
                ..
            } => {
                std::ptr::write_bytes(dst, 0, count * one.size);
                let arr = (src as *const *mut u8).read_unaligned();
                if arr.is_null() || gc::type_of(arr) != Some(types::TYPE_ARRAY) {
                    return;
                }
                let len = (arr as *const u64).read() as usize;
                let step = kind.elem_size();
                for i in 0..len.min(*count) {
                    self.write_item(one, *kind, arr.add(8 + i * step), dst.add(i * one.size));
                }
            }
            Native::Struct(id) => {
                if let Ok(l) = self.native_layout(*id, 0) {
                    self.write_fields(&l, src, dst);
                }
            }
            Native::FnPtr { wide } => {
                let d = (src as *const *mut u8).read_unaligned();
                let p = if d.is_null() || !gc::is_delegate(d) {
                    0
                } else {
                    // Plus de porte libre : pointeur nul, comme un champ
                    // vide, et le journal le dit.
                    self.callback_for(d, *wide).unwrap_or_else(|| {
                        crate::log_warn!(
                            "clr-interop",
                            "StructureToPtr : plus de {} délégués confiés au code natif",
                            super::callbacks::CAPACITY
                        );
                        0
                    })
                };
                (dst as *mut usize).write_unaligned(p);
            }
            Native::NullPtr => (dst as *mut usize).write_unaligned(0),
        }
    }

    unsafe fn read_fields(&mut self, layout: &Layout, src: *const u8, dst: *mut u8) {
        for f in &layout.fields {
            self.read_item(&f.item, f.kind, src.add(f.native), dst.add(f.managed));
        }
    }

    unsafe fn read_item(&mut self, item: &Item, kind: Kind, src: *const u8, dst: *mut u8) {
        match &item.how {
            Native::Plain => {
                std::ptr::copy(src, dst, item.size.min(kind.elem_size()));
            }
            Native::Bool { .. } => {
                let on = match item.size {
                    1 => *src != 0,
                    2 => (src as *const i16).read_unaligned() != 0,
                    _ => (src as *const i32).read_unaligned() != 0,
                };
                *dst = on as u8;
            }
            Native::Char { wide } => {
                let c = if *wide {
                    (src as *const u16).read_unaligned()
                } else {
                    from_ansi(std::slice::from_raw_parts(src, 1))
                        .first()
                        .copied()
                        .unwrap_or(0)
                };
                (dst as *mut u16).write_unaligned(c);
            }
            Native::StrPtr(format) => {
                let p = (src as *const usize).read_unaligned();
                let v = if p == 0 {
                    Value::Null
                } else {
                    bcl::alloc_utf16(&read_native(p, None, *format))
                };
                il::store(dst, Kind::Ref, v);
            }
            Native::ByValStr { count, wide } => {
                let units: Vec<u16> = if *wide {
                    let s = std::slice::from_raw_parts(src as *const u16, *count);
                    s.iter().copied().take_while(|&c| c != 0).collect()
                } else {
                    let s = std::slice::from_raw_parts(src, *count);
                    let n = s.iter().position(|&b| b == 0).unwrap_or(s.len());
                    from_ansi(&s[..n])
                };
                il::store(dst, Kind::Ref, bcl::alloc_utf16(&units));
            }
            Native::ByValArray {
                count,
                elem,
                kind,
                item: one,
            } => {
                let step = kind.elem_size();
                let arr = gc::with_gc(|gc| gc.alloc_array_of(*elem, step, *count));
                self.pins.push(Value::Ref(arr));
                for i in 0..*count {
                    self.read_item(one, *kind, src.add(i * one.size), arr.add(8 + i * step));
                }
                self.pins.pop();
                il::store(dst, Kind::Ref, Value::Ref(arr));
            }
            Native::Struct(id) => {
                if let Ok(l) = self.native_layout(*id, 0) {
                    self.read_fields(&l, src, dst);
                }
            }
            Native::FnPtr { .. } => {
                // Un pointeur donné par nous rend son délégué ; un pointeur
                // natif quelconque demanderait GetDelegateForFunctionPointer.
                let p = (src as *const usize).read_unaligned();
                let v = super::callbacks::delegate_for_pointer(p)
                    .map_or(Value::Null, |d| Value::Ref(d as *mut u8));
                il::store(dst, Kind::Ref, v);
            }
            Native::NullPtr => il::store(dst, Kind::Ref, Value::Null),
        }
    }

    unsafe fn destroy_fields(&self, layout: &Layout, ptr: *mut u8) {
        for f in &layout.fields {
            self.destroy_item(&f.item, ptr.add(f.native));
        }
    }

    unsafe fn destroy_item(&self, item: &Item, ptr: *mut u8) {
        match &item.how {
            Native::StrPtr(format) => {
                let p = (ptr as *const usize).read_unaligned();
                if p != 0 {
                    free_native_string(p, *format);
                }
                (ptr as *mut usize).write_unaligned(0);
            }
            Native::Struct(id) => {
                if let Ok(l) = self.native_layout(*id, 0) {
                    self.destroy_fields(&l, ptr);
                }
            }
            Native::ByValArray {
                count, item: one, ..
            } => {
                for i in 0..*count {
                    self.destroy_item(one, ptr.add(i * one.size));
                }
            }
            _ => {}
        }
    }
}

/// Type délégué : descend de System.Delegate (MulticastDelegate compris).
fn is_delegate_type(id: TypeId) -> bool {
    let mut cur = id;
    for _ in 0..64 {
        let Some((ns, name, parent)) = types::with_types(|reg| {
            reg.get(cur)
                .map(|t| (t.namespace.clone(), t.name.clone(), t.parent))
        }) else {
            return false;
        };
        if cur != id && ns == "System" && (name == "MulticastDelegate" || name == "Delegate") {
            return true;
        }
        if parent == 0 || parent == cur {
            return false;
        }
        cur = parent;
    }
    false
}

/// Allouer(long cb, bool coTask) : LocalAlloc (AllocHGlobal) ou
/// CoTaskMemAlloc.
fn allocate(args: &[Value]) -> Result<Value, ClrException> {
    let size = args.first().map_or(0, |v| v.as_i64());
    let co_task = args.get(1).is_some_and(|v| v.as_i32() != 0);
    if size < 0 {
        return Err(out_of_memory());
    }
    let p = unsafe {
        if co_task {
            crate::dlls::ole32::co_task_mem_alloc(size as usize)
        } else {
            crate::dlls::kernel32_misc::local_alloc(0, size as usize)
        }
    };
    if p.is_null() {
        return Err(out_of_memory());
    }
    Ok(Value::I64(p as i64))
}

fn free(p: usize, co_task: bool) {
    if p == 0 {
        return;
    }
    unsafe {
        if co_task {
            crate::dlls::ole32::co_task_mem_free(p as *mut _);
        } else {
            crate::dlls::kernel32_misc::local_free(p as *mut _);
        }
    }
}

/// Reallouer(IntPtr pv, long cb, bool coTask).
fn reallocate(args: &[Value]) -> Result<Value, ClrException> {
    let p = int_ptr(args.first());
    let size = args.get(1).map_or(0, |v| v.as_i64());
    let co_task = args.get(2).is_some_and(|v| v.as_i32() != 0);
    if size < 0 {
        return Err(out_of_memory());
    }
    let q = unsafe {
        if co_task {
            crate::dlls::ole32::co_task_mem_realloc(p as *mut _, size as usize)
        } else {
            // LMEM_MOVEABLE : le bloc peut changer d'adresse.
            crate::dlls::kernel32_misc::local_realloc(p as *mut _, size as usize, 2)
        }
    };
    if q.is_null() {
        return Err(out_of_memory());
    }
    Ok(Value::I64(q as i64))
}

/// Format d'une chaîne native passé par notre mscorlib : 0 ANSI, 1 UTF-16,
/// 2 BSTR.
fn format_arg(v: Option<&Value>) -> StrFormat {
    match v.map_or(0, |v| v.as_i32()) {
        1 => StrFormat::Wide,
        2 => StrFormat::Bstr,
        _ => StrFormat::Ansi,
    }
}

/// LireChaine(IntPtr ptr, int len, int format) : `len` négatif lit
/// jusqu'au zéro final.
fn read_string(args: &[Value]) -> Value {
    let p = int_ptr(args.first());
    if p == 0 {
        return Value::Null;
    }
    let len = args.get(1).map_or(-1, |v| v.as_i32());
    let len = (len >= 0).then_some(len as usize);
    bcl::alloc_utf16(&unsafe { read_native(p, len, format_arg(args.get(2))) })
}

/// EcrireChaine(string s, int format, bool coTask) : copie native terminée
/// par un zéro ; IntPtr.Zero pour null.
fn write_string(args: &[Value]) -> Result<Value, ClrException> {
    let s = match args.first() {
        Some(Value::Ref(p)) if !p.is_null() => *p,
        _ => return Ok(Value::I64(0)),
    };
    let co_task = args.get(2).is_some_and(|v| v.as_i32() != 0);
    let p = unsafe { native_string(&bcl::managed_utf16(s), format_arg(args.get(1)), co_task) };
    if p == 0 {
        return Err(out_of_memory());
    }
    Ok(Value::I64(p as i64))
}

/// ExceptionSysteme(string name, string message) : une exception du moteur
/// pour les HRESULT dont notre mscorlib n'a pas le type.
fn system_exception(args: &[Value]) -> Value {
    let name = match args.first() {
        Some(Value::Ref(p)) if !p.is_null() => bcl::read_managed_string(*p),
        _ => String::new(),
    };
    let message = match args.get(1) {
        Some(Value::Ref(p)) if !p.is_null() => bcl::read_managed_string(*p),
        _ => String::new(),
    };
    let name = match name.as_str() {
        "NotImplementedException" | "NullReferenceException" | "OutOfMemoryException" => name,
        _ => "Exception".into(),
    };
    Value::Ref(ClrException::new("System", &name, &message).obj_ptr)
}

/// Copie native d'une chaîne ; CoTaskMemAlloc pour `co_task`, LocalAlloc
/// sinon (une BSTR passe toujours par SysAllocStringLen). 0 si la mémoire
/// manque.
unsafe fn native_string(units: &[u16], format: StrFormat, co_task: bool) -> usize {
    let alloc = |n: usize| -> *mut u8 {
        if co_task {
            crate::dlls::ole32::co_task_mem_alloc(n) as *mut u8
        } else {
            crate::dlls::kernel32_misc::local_alloc(0, n) as *mut u8
        }
    };
    match format {
        StrFormat::Bstr => {
            crate::dlls::oleaut32::sys_alloc_string_len(units.as_ptr(), units.len() as u32) as usize
        }
        StrFormat::Wide => {
            let p = alloc((units.len() + 1) * 2);
            if p.is_null() {
                return 0;
            }
            std::ptr::copy_nonoverlapping(units.as_ptr(), p as *mut u16, units.len());
            (p as *mut u16).add(units.len()).write_unaligned(0);
            p as usize
        }
        StrFormat::Ansi => {
            let bytes = to_ansi(units);
            let p = alloc(bytes.len() + 1);
            if p.is_null() {
                return 0;
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
            *p.add(bytes.len()) = 0;
            p as usize
        }
    }
}

/// Libère une chaîne écrite dans une structure (CoTaskMemAlloc ou BSTR).
unsafe fn free_native_string(p: usize, format: StrFormat) {
    match format {
        StrFormat::Bstr => crate::dlls::oleaut32::sys_free_string(p as *mut u16),
        _ => crate::dlls::ole32::co_task_mem_free(p as *mut _),
    }
}

/// Unités UTF-16 d'une chaîne native ; `len` en caractères (ANSI : en
/// octets), sinon jusqu'au zéro final (BSTR : sa longueur préfixée).
unsafe fn read_native(p: usize, len: Option<usize>, format: StrFormat) -> Vec<u16> {
    match format {
        StrFormat::Ansi => {
            let s = p as *const u8;
            let n = len.unwrap_or_else(|| (0..).take_while(|&i| *s.add(i) != 0).count());
            from_ansi(std::slice::from_raw_parts(s, n))
        }
        StrFormat::Wide => {
            let s = p as *const u16;
            let n = len.unwrap_or_else(|| (0..).take_while(|&i| *s.add(i) != 0).count());
            std::slice::from_raw_parts(s, n).to_vec()
        }
        StrFormat::Bstr => {
            let bytes = ((p - 4) as *const u32).read_unaligned() as usize;
            let n = len.unwrap_or(bytes / 2);
            std::slice::from_raw_parts(p as *const u16, n).to_vec()
        }
    }
}

/// UTF-16 → page de code ANSI du système (WideCharToMultiByte, CP_ACP).
fn to_ansi(units: &[u16]) -> Vec<u8> {
    if units.is_empty() {
        return Vec::new();
    }
    let n = units.len() as i32;
    unsafe {
        let need = crate::intl::api::wide_char_to_multi_byte(
            CP_ACP,
            0,
            units.as_ptr(),
            n,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
            std::ptr::null_mut(),
        );
        if need <= 0 {
            return units
                .iter()
                .map(|&c| if c < 0x80 { c as u8 } else { b'?' })
                .collect();
        }
        let mut out = vec![0u8; need as usize];
        let got = crate::intl::api::wide_char_to_multi_byte(
            CP_ACP,
            0,
            units.as_ptr(),
            n,
            out.as_mut_ptr(),
            need,
            std::ptr::null(),
            std::ptr::null_mut(),
        );
        out.truncate(got.max(0) as usize);
        out
    }
}

/// Page de code ANSI du système → UTF-16 (MultiByteToWideChar, CP_ACP).
fn from_ansi(bytes: &[u8]) -> Vec<u16> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let n = bytes.len() as i32;
    unsafe {
        let need = crate::intl::api::multi_byte_to_wide_char(
            CP_ACP,
            0,
            bytes.as_ptr(),
            n,
            std::ptr::null_mut(),
            0,
        );
        if need <= 0 {
            return bytes.iter().map(|&b| b as u16).collect();
        }
        let mut out = vec![0u16; need as usize];
        let got = crate::intl::api::multi_byte_to_wide_char(
            CP_ACP,
            0,
            bytes.as_ptr(),
            n,
            out.as_mut_ptr(),
            need,
        );
        out.truncate(got.max(0) as usize);
        out
    }
}

/// Types primitifs : (taille, alignement) marshalés.
fn primitive(id: TypeId, wide: bool) -> Option<(usize, usize)> {
    Some(match id {
        types::TYPE_BOOLEAN => (4, 4),
        types::TYPE_CHAR if wide => (2, 2),
        types::TYPE_CHAR => (1, 1),
        types::TYPE_BYTE | types::TYPE_SBYTE => (1, 1),
        types::TYPE_INT16 | types::TYPE_UINT16 => (2, 2),
        types::TYPE_INT32 | types::TYPE_UINT32 | types::TYPE_FLOAT => (4, 4),
        types::TYPE_INT64 | types::TYPE_UINT64 | types::TYPE_DOUBLE => (8, 8),
        types::TYPE_INTPTR | types::TYPE_UINTPTR => (8, 8),
        _ => return None,
    })
}

/// SizeConst d'un blob ByValTStr (FIXEDSYSSTRING) ou ByValArray
/// (FIXEDARRAY) : le nombre compressé qui suit le type natif (§II.23.4).
fn size_const(native: &[u8]) -> usize {
    if native.len() <= 1 {
        return 0;
    }
    let mut off = 1;
    metadata::decode_compressed_u32(native, &mut off) as usize
}
