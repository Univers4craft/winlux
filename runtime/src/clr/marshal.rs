//! Taille d'un type marshalé vers le code natif (#2026) :
//! `Marshal.SizeOf(Type)`, `SizeOf(object)` et `SizeOf<T>()`. Ce n'est pas
//! la disposition gérée : un `bool` devient un BOOL Win32 de 4 octets, un
//! `char` un octet ANSI (deux avec `CharSet.Unicode` ou `Auto`), une chaîne
//! un pointeur, sauf `MarshalAs(ByValTStr | ByValArray, SizeConst = n)`
//! qui la range dans la structure. `StructLayout(Pack, Size)` et
//! `FieldOffset` sont suivis (ECMA-335 §II.10.7, §II.22.17, §II.23.4 ;
//! documentation publique « Marshal.SizeOf » et « Default Marshaling for
//! Value Types » sur MS Learn).

use super::exceptions::ClrException;
use super::gc;
use super::il::{Gen, Interpreter, Value};
use super::metadata::{self, ElementType};
use super::reflection::type_id_of;
use super::types::{self, TypeId};

const FIELD_STATIC: u16 = 0x10;
const LAYOUT_MASK: u32 = 0x18;
const LAYOUT_EXPLICIT: u32 = 0x10;
const STRING_FORMAT_MASK: u32 = 0x3_0000;
/// NATIVE_TYPE_* (ECMA-335 §II.23.4).
const NATIVE_BYVALSTR: u8 = 0x17;
const NATIVE_BYVALARRAY: u8 = 0x1E;

fn not_marshalable(name: &str) -> ClrException {
    ClrException::new(
        "System",
        "ArgumentException",
        &format!(
            "Type '{name}' cannot be marshaled as an unmanaged structure; no meaningful size or offset can be computed."
        ),
    )
}

impl Interpreter {
    /// `Marshal.SizeOf` : `Some` si l'appel est pour nous.
    pub(super) fn marshal_call(
        &mut self,
        namespace: &str,
        class: &str,
        name: &str,
        args: &[Value],
        gen: Option<&Gen>,
    ) -> Option<Result<Value, ClrException>> {
        if (namespace, class, name) != ("System.Runtime.InteropServices", "Marshal", "SizeOf") {
            return None;
        }
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
        self.marshal_size_in(id, 0)
    }

    fn marshal_size_in(&self, id: TypeId, depth: usize) -> Result<(usize, usize), ClrException> {
        let name = types::with_types(|reg| reg.get(id).map(|t| t.full_name())).unwrap_or_default();
        if depth > 32 {
            return Err(not_marshalable(&name));
        }
        if let Some(p) = primitive(id, false) {
            return Ok(p);
        }
        let (is_value, is_enum, has_args) = types::with_types(|reg| {
            reg.get(id)
                .map(|t| (t.is_value, t.is_enum, !t.type_args.is_empty()))
        })
        .unwrap_or((false, false, false));
        if is_enum {
            let size = self.kind_of_type(id).elem_size();
            return Ok((size, size));
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
        let fields: Vec<u32> = a.type_fields(row).collect();
        let (mut end, mut max_align) = (0usize, 1usize);
        for f in fields {
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
            let (size, align) = self.field_size(asm, &et, &gen, native.as_deref(), wide, depth)?;
            let align = if pack > 0 {
                align.min(pack as usize)
            } else {
                align
            }
            .max(1);
            max_align = max_align.max(align);
            let at = match at_declared {
                Some(o) => o as usize,
                None => end.div_ceil(align) * align,
            };
            end = end.max(at + size);
        }
        let size = end.max(class_size as usize).div_ceil(max_align).max(1) * max_align;
        Ok((size, max_align))
    }

    fn field_size(
        &self,
        asm: usize,
        et: &ElementType,
        gen: &Gen,
        native: Option<&[u8]>,
        wide: bool,
        depth: usize,
    ) -> Result<(usize, usize), ClrException> {
        let char_size = if wide { 2 } else { 1 };
        match (native.and_then(|n| n.first()).copied(), et) {
            (Some(NATIVE_BYVALSTR), _) => {
                let count = size_const(native.unwrap_or_default());
                return Ok((count * char_size, char_size));
            }
            (Some(NATIVE_BYVALARRAY), ElementType::SzArray(inner)) => {
                let count = size_const(native.unwrap_or_default());
                let elem = self.resolve_sig(asm, inner, gen);
                let (size, align) = self.field_size_of_type(elem, wide, depth)?;
                return Ok((size * count, align));
            }
            _ => {}
        }
        let id = self.resolve_sig(asm, et, gen);
        match et {
            // Pointeurs natifs, chaînes, tableaux, délégués, classes : une
            // adresse.
            ElementType::Ptr(_)
            | ElementType::FnPtr
            | ElementType::String
            | ElementType::SzArray(_)
            | ElementType::Object => Ok((8, 8)),
            _ => self.field_size_of_type(id, wide, depth),
        }
    }

    fn field_size_of_type(
        &self,
        id: TypeId,
        wide: bool,
        depth: usize,
    ) -> Result<(usize, usize), ClrException> {
        if let Some(p) = primitive(id, wide) {
            return Ok(p);
        }
        let is_value = types::with_types(|reg| reg.get(id).is_some_and(|t| t.is_value));
        if is_value {
            self.marshal_size_in(id, depth + 1)
        } else {
            Ok((8, 8))
        }
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
