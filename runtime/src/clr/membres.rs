//! Réflexion sur les membres (#2008) : méthodes, constructeurs, champs,
//! propriétés, paramètres, attributs personnalisés et appel par réflexion.
//!
//! Écrit d'après ECMA-335 (§II.22 pour les tables, §II.23.2 pour les
//! signatures, §II.23.3 pour les valeurs d'attributs personnalisés) et la
//! documentation publique de System.Reflection ; aucun code de Microsoft,
//! de Mono ou de Wine.
//!
//! Le moteur ne sert que des primitives à la classe interne
//! `System.Reflection.Rtti` de notre mscorlib ; MethodInfo, PropertyInfo,
//! FieldInfo et le filtrage par BindingFlags sont écrits en C#. Un membre y
//! est désigné par une « poignée » : (indice de l'assembly << 32) | ligne de
//! sa table (MethodDef, Field ou Property), plus le Type qui le porte pour
//! les génériques.

use super::bcl::{alloc_managed_string, read_managed_string};
use super::exceptions::ClrException;
use super::gc;
use super::il::{load, store, Gen, Interpreter, Value};
use super::metadata::{self, ElementType};
use super::reflection::type_id_of;
use super::tables::{CodedIndex, TableId, Token};
use super::types::{self, Kind, TypeId};

fn handle(asm: usize, row: u32) -> i64 {
    ((asm as i64) << 32) | i64::from(row)
}

fn split(h: i64) -> (usize, u32) {
    ((h >> 32) as usize, h as u32)
}

fn arg_ref(args: &[Value], i: usize) -> *mut u8 {
    match args.get(i) {
        Some(Value::Ref(p)) => *p,
        _ => std::ptr::null_mut(),
    }
}

fn arg_i64(args: &[Value], i: usize) -> i64 {
    args.get(i).map_or(0, |v| v.as_i64())
}

fn argument_exception(class: &str, msg: &str) -> ClrException {
    ClrException::new("System", class, msg)
}

/// Éléments d'un tableau de références (longueur à +0, données à +8) ;
/// vide pour tout autre objet, que la réflexion peut passer ici (#2037).
fn array_refs(arr: *mut u8) -> Vec<Value> {
    if gc::type_of(arr) != Some(types::TYPE_ARRAY) || gc::array_elem_type(arr) != 0 {
        return Vec::new();
    }
    let len = unsafe { *(arr as *const u64) } as usize;
    (0..len)
        .map(
            |i| match unsafe { *(arr.add(8 + i * 8) as *const *mut u8) } {
                p if p.is_null() => Value::Null,
                p => Value::Ref(p),
            },
        )
        .collect()
}

fn long_array(items: &[i64]) -> Value {
    let arr = gc::with_gc(|gc| gc.alloc_array_of(types::TYPE_INT64, 8, items.len()));
    for (i, v) in items.iter().enumerate() {
        unsafe { *(arr.add(8 + i * 8) as *mut i64) = *v };
    }
    Value::Ref(arr)
}

fn ref_array(items: &[Value]) -> Value {
    let arr = gc::with_gc(|gc| gc.alloc_array(types::TYPE_ARRAY, 8, items.len()));
    for (i, v) in items.iter().enumerate() {
        let p = match v {
            Value::Ref(p) => *p,
            _ => std::ptr::null_mut(),
        };
        unsafe { *(arr.add(8 + i * 8) as *mut *mut u8) = p };
    }
    Value::Ref(arr)
}

/// Lecture d'un blob d'attribut personnalisé (ECMA-335 §II.23.3).
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn bytes(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.data.get(self.pos..self.pos + n)?;
        self.pos += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        self.bytes(1).map(|b| b[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.bytes(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        self.bytes(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u64(&mut self) -> Option<u64> {
        self.bytes(8)
            .map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
    }
    /// SerString : 0xFF pour null, sinon longueur compressée puis UTF-8.
    fn ser_string(&mut self) -> Option<Option<String>> {
        if self.data.get(self.pos) == Some(&0xFF) {
            self.pos += 1;
            return Some(None);
        }
        let len = metadata::decode_compressed_u32(self.data, &mut self.pos) as usize;
        let b = self.bytes(len)?;
        Some(Some(String::from_utf8_lossy(b).into_owned()))
    }
}

/// Type d'une valeur d'attribut, tel que le blob le décrit.
#[derive(Clone, Debug)]
enum AttrType {
    Prim(TypeId, Kind),
    String,
    Type,
    Object,
    Enum(TypeId, Kind),
    Array(Box<AttrType>),
}

impl Interpreter {
    /// Appelle une méthode statique interne de System.AppDomain de notre
    /// mscorlib (LeverSortie, LeverExceptionNonGeree, #2058). Une exception
    /// levée par un gestionnaire est journalisée, puis la sortie continue.
    pub(super) fn raise_domain_event(&mut self, method: &str, args: Vec<Value>) {
        let Some(corlib) = self
            .domain
            .iter()
            .position(|i| i.name == super::domain::CORLIB)
        else {
            return;
        };
        let Some((asm, type_row)) = self
            .find_type_in(corlib, "System.AppDomain", false)
            .and_then(|id| self.def_of(id))
        else {
            return;
        };
        let a = self.assembly_at(asm);
        let Some(row) = a.type_methods(type_row).find(|&m| {
            let md = a.get_method_def(m);
            md.is_static() && a.get_string(md.name) == method
        }) else {
            return;
        };
        let pinned = self.pins.len();
        self.pins.extend(args.iter().copied());
        if let super::il::ExecResult::Exception(e) = self.execute_in(asm, row, args, Gen::default())
        {
            crate::log_error!("clr", "exception dans AppDomain.{method} : {e}");
        }
        self.pins.truncate(pinned);
    }

    /// (assembly, ligne TypeDef) d'un type, y compris d'un type intégré au
    /// moteur que notre mscorlib déclare (String, Int32…).
    pub(super) fn def_of(&self, id: TypeId) -> Option<(usize, u32)> {
        if let Some(def) = types::with_types(|reg| reg.get(id).and_then(|t| t.def)) {
            return Some(def);
        }
        let (ns, name) =
            types::with_types(|reg| reg.get(id).map(|t| (t.namespace.clone(), t.name.clone())))?;
        let asm = self
            .domain
            .iter()
            .position(|i| i.name == super::domain::CORLIB)?;
        let a = self.assembly_at(asm);
        (1..=a.row_count(TableId::TypeDef))
            .find(|&r| {
                let td = a.get_type_def(r);
                a.get_string(td.namespace) == ns && a.get_string(td.name) == name
            })
            .map(|r| (asm, r))
    }

    pub(super) fn gen_of(&self, owner: TypeId) -> Gen {
        Gen {
            t: types::with_types(|reg| reg.get(owner).map(|t| t.type_args.clone()))
                .unwrap_or_default(),
            m: Vec::new(),
        }
    }

    fn owner_arg(&self, args: &[Value], i: usize) -> TypeId {
        let p = arg_ref(args, i);
        if p.is_null() {
            return types::TYPE_OBJECT;
        }
        type_id_of(p).unwrap_or(types::TYPE_OBJECT)
    }

    fn type_or_null(&self, id: Option<TypeId>) -> Value {
        match id {
            Some(id) => self.type_object(id),
            None => Value::Null,
        }
    }

    /// Valeur encadrée d'un `v` de type `id` (objet pour un type valeur).
    pub(super) fn box_as(&self, id: TypeId, v: Value) -> Value {
        match self.kind_of_type(id) {
            Kind::Ref | Kind::Struct(_) => v,
            k => {
                let b = gc::with_gc(|gc| gc.alloc(id, 8));
                store(b, k, v);
                Value::Ref(b)
            }
        }
    }

    /// L'argument `v`, passé par MethodInfo.Invoke ou ConstructorInfo.Invoke,
    /// convient-il au paramètre de type `et` (#2037) ? Null convient toujours
    /// (valeur nulle pour un type valeur). Un type valeur veut un objet
    /// encadré de même représentation, un tableau des éléments compatibles,
    /// une classe un objet qui en dérive. Sinon le message de .NET :
    /// « Object of type 'X' cannot be converted to type 'Y'. »
    fn argument_fits(
        &self,
        asm: usize,
        et: &ElementType,
        gen: &Gen,
        v: Value,
    ) -> Result<(), ClrException> {
        let Value::Ref(p) = v else { return Ok(()) };
        if p.is_null() {
            return Ok(());
        }
        let id = self.resolve_sig(asm, et, gen);
        let actual = gc::type_of(p);
        let fits = match (actual, et) {
            // Un objet hors du tas n'est pas un argument de réflexion.
            (None, _) => false,
            (Some(a), ElementType::SzArray(inner)) => {
                let elem = self.resolve_sig(asm, inner, gen);
                a == types::TYPE_ARRAY && self.array_elements_fit(elem, p)
            }
            (Some(a), _) => self.value_fits(id, a),
        };
        if fits {
            return Ok(());
        }
        let name = |t: TypeId| {
            types::with_types(|reg| reg.get(t).map_or_else(|| "?".into(), |i| i.full_name()))
        };
        Err(argument_exception(
            "Reflection.ArgumentInconvertibleException",
            &format!(
                "Object of type '{}' cannot be converted to type '{}'.",
                actual.map_or_else(|| "?".into(), name),
                name(id)
            ),
        ))
    }

    /// Un objet de type `actual` convient-il à un paramètre de type `id` ?
    fn value_fits(&self, id: TypeId, actual: TypeId) -> bool {
        if id == 0 || id == actual || id == types::TYPE_OBJECT {
            return true;
        }
        let ak = self.kind_of_type(actual);
        match self.kind_of_type(id) {
            Kind::Ref => {
                let mut t = actual;
                while t != 0 {
                    if t == id {
                        return true;
                    }
                    t = types::with_types(|reg| reg.get(t).map_or(0, |i| i.parent));
                }
                let iface = types::with_types(|reg| reg.get(id).is_some_and(|i| i.is_interface));
                // Une interface dont la définition manque ne refuse rien.
                iface && self.implements(actual, id).unwrap_or(true)
            }
            // Nullable<T> reçoit un T encadré.
            Kind::Struct(_) => types::with_types(|reg| {
                reg.get(id)
                    .is_some_and(|t| t.namespace == "System" && t.name == "Nullable`1")
            }),
            // Entier, flottant, booléen, enum : un objet encadré primitif
            // (Binder élargit int en long, enum en son entier).
            _ => !matches!(ak, Kind::Ref | Kind::Struct(_)),
        }
    }

    /// Valeur de type `id` tirée d'un objet encadré (null : valeur nulle).
    fn unbox_as(&self, id: TypeId, v: Value) -> Value {
        match self.kind_of_type(id) {
            Kind::Ref => v,
            k => {
                let p = match v {
                    Value::Ref(p) if !p.is_null() => p,
                    _ => {
                        return match k {
                            Kind::R4 => Value::F32(0.0),
                            Kind::R8 => Value::F64(0.0),
                            Kind::I8 | Kind::U8 | Kind::I => Value::I64(0),
                            Kind::Struct(_) => Value::Ref(self.new_struct(id)),
                            _ => Value::I32(0),
                        }
                    }
                };
                // Un entier encadré passé pour un autre type primitif garde
                // sa valeur (int pour long…), comme le fait Binder.
                let actual = gc::type_of(p).unwrap_or(id);
                let ak = self.kind_of_type(actual);
                let raw = load(p, if matches!(ak, Kind::Ref) { k } else { ak });
                match k {
                    Kind::R4 | Kind::R8 if !matches!(raw, Value::F32(_) | Value::F64(_)) => {
                        let f = raw.as_i64() as f64;
                        if k == Kind::R4 {
                            Value::F32(f as f32)
                        } else {
                            Value::F64(f)
                        }
                    }
                    Kind::R4 => Value::F32(raw.as_f64() as f32),
                    Kind::R8 => Value::F64(raw.as_f64()),
                    Kind::I8 | Kind::U8 | Kind::I => Value::I64(raw.as_i64()),
                    Kind::Struct(_) => load(p, k),
                    _ => Value::I32(raw.as_i64() as i32),
                }
            }
        }
    }

    fn method_sig_of(&self, asm: usize, row: u32) -> metadata::MethodSig {
        let a = self.assembly_at(asm);
        metadata::decode_method_sig(a.get_blob(a.get_method_def(row).signature))
    }

    fn methods_of(&self, id: TypeId) -> Vec<i64> {
        let Some((asm, row)) = self.def_of(id) else {
            return Vec::new();
        };
        self.assembly_at(asm)
            .type_methods(row)
            .map(|m| handle(asm, m))
            .collect()
    }

    fn fields_of(&self, id: TypeId) -> Vec<i64> {
        let Some((asm, row)) = self.def_of(id) else {
            return Vec::new();
        };
        self.assembly_at(asm)
            .type_fields(row)
            .map(|f| handle(asm, f))
            .collect()
    }

    fn properties_of(&self, id: TypeId) -> Vec<i64> {
        let Some((asm, row)) = self.def_of(id) else {
            return Vec::new();
        };
        self.assembly_at(asm)
            .type_properties(row)
            .map(|p| handle(asm, p))
            .collect()
    }

    /// Accesseur `get` (0x02) ou `set` (0x01) d'une propriété (§II.22.28).
    fn accessor(&self, asm: usize, prop: u32, setter: bool) -> i64 {
        let a = self.assembly_at(asm);
        let Some(coded) = CodedIndex::HasSemantics.encode(TableId::Property, prop) else {
            return 0;
        };
        let wanted = if setter { 0x01 } else { 0x02 };
        (1..=a.row_count(TableId::MethodSemantics))
            .map(|r| a.get_method_semantics(r))
            .find(|s| s.association == coded && s.semantics & wanted != 0)
            .map_or(0, |s| handle(asm, s.method))
    }

    fn property_type(&self, asm: usize, prop: u32, owner: TypeId) -> TypeId {
        let a = self.assembly_at(asm);
        let blob = a.get_blob(a.get_property(prop).typ);
        // PROPERTY [HASTHIS], nombre de paramètres, type (§II.23.2.5).
        let mut off = 1;
        let _ = metadata::decode_compressed_u32(blob, &mut off);
        let et = metadata::decode_type(blob, &mut off);
        self.resolve_sig(asm, &et, &self.gen_of(owner))
    }

    fn field_type(&self, asm: usize, field: u32, owner: TypeId) -> TypeId {
        self.resolve_sig(asm, &self.field_sig(asm, field), &self.gen_of(owner))
    }

    fn field_sig(&self, asm: usize, field: u32) -> ElementType {
        let a = self.assembly_at(asm);
        let blob = a.get_blob(a.get_field(field).signature);
        let mut off = 1;
        metadata::decode_type(blob, &mut off)
    }

    /// La cible d'un membre d'instance est-elle un objet du type `owner`
    /// (#2037) ? Sinon les décalages des champs tomberaient hors de l'objet.
    fn target_fits(&self, owner: TypeId, target: *mut u8) -> bool {
        let Some(actual) = gc::type_of(target) else {
            return false;
        };
        actual == owner || self.kind_of_type(owner) == Kind::Ref && self.value_fits(owner, actual)
    }

    /// Valeur de la table Constant d'un champ (énumérations, const).
    fn constant_of(&self, asm: usize, field: u32, owner: TypeId) -> Value {
        let a = self.assembly_at(asm);
        let Some(coded) = CodedIndex::HasConstant.encode(TableId::Field, field) else {
            return Value::Null;
        };
        let Some(c) = (1..=a.row_count(TableId::Constant))
            .map(|r| a.get_constant(r))
            .find(|c| c.parent == coded)
        else {
            return Value::Null;
        };
        let blob = a.get_blob(c.value).to_vec();
        let mut r = Reader {
            data: &blob,
            pos: 0,
        };
        let raw = match c.typ {
            0x02 | 0x04 | 0x05 => Value::I32(r.u8().unwrap_or(0) as i8 as i32),
            0x03 | 0x06 | 0x07 => Value::I32(r.u16().unwrap_or(0) as i16 as i32),
            0x08 | 0x09 => Value::I32(r.u32().unwrap_or(0) as i32),
            0x0A | 0x0B => Value::I64(r.u64().unwrap_or(0) as i64),
            0x0C => Value::F32(f32::from_bits(r.u32().unwrap_or(0))),
            0x0D => Value::F64(f64::from_bits(r.u64().unwrap_or(0))),
            0x0E => {
                let units: Vec<u16> = blob
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_le_bytes(*c))
                    .collect();
                return super::bcl::alloc_utf16(&units);
            }
            _ => return Value::Null,
        };
        let id = self.field_type(asm, field, owner);
        self.box_as(id, raw)
    }

    /// Adresse et genre d'un champ, pour `target` (ignoré s'il est statique).
    fn field_slot(
        &mut self,
        asm: usize,
        field: u32,
        owner: TypeId,
        target: *mut u8,
    ) -> Result<(*mut u8, Kind), ClrException> {
        let is_static = self.assembly_at(asm).get_field(field).is_static();
        self.ensure_layout(owner);
        let info = types::with_types(|reg| {
            let t = reg.get(owner)?;
            let list = if is_static {
                &t.static_fields
            } else {
                &t.fields
            };
            list.iter().find(|f| f.metadata_row == field).cloned()
        });
        let Some(info) = info else {
            return Err(argument_exception(
                "NotSupportedException",
                "Field not laid out by the engine.",
            ));
        };
        if is_static {
            let size = info.kind.slot_size();
            let ptr = types::with_types(|reg| reg.static_field_ptr(owner, info.offset, size));
            Ok((ptr, info.kind))
        } else {
            if target.is_null() {
                return Err(argument_exception(
                    "Reflection.TargetException",
                    "Non-static field requires a target.",
                ));
            }
            if !self.target_fits(owner, target) {
                return Err(argument_exception(
                    "ArgumentException",
                    "Field is not a field on the target object.",
                ));
            }
            Ok((unsafe { target.add(info.offset) }, info.kind))
        }
    }

    /// MethodBase.Invoke : arguments encadrés convertis selon la signature,
    /// paramètres `ref`/`out` recopiés dans le tableau, résultat encadré.
    fn invoke_member(
        &mut self,
        asm: usize,
        row: u32,
        owner: TypeId,
        target: Value,
        args_array: *mut u8,
    ) -> Result<Value, ClrException> {
        let sig = self.method_sig_of(asm, row);
        let gen = self.gen_of(owner);
        let boxed = array_refs(args_array);
        if boxed.len() != sig.param_types.len() {
            return Err(argument_exception(
                "Reflection.TargetParameterCountException",
                "Parameter count mismatch.",
            ));
        }
        let (is_static, is_virtual, is_ctor) = {
            let a = self.assembly_at(asm);
            let md = a.get_method_def(row);
            (
                md.is_static(),
                md.is_virtual(),
                a.get_string(md.name) == ".ctor",
            )
        };
        let pinned = self.pins.len();
        let mut call_args = Vec::new();
        let mut this = Value::Null;
        if is_ctor {
            // ConstructorInfo.Invoke(object[]) : nouvel objet.
            let kind = self.kind_of_type(owner);
            let obj = if kind == Kind::Ref {
                self.ensure_layout(owner);
                let size = types::with_types(|reg| {
                    reg.get(owner)
                        .map_or(types::EXTERNAL_OBJECT_SIZE, |t| t.instance_size)
                });
                gc::with_gc(|gc| gc.alloc(owner, size.max(16)))
            } else {
                self.new_struct(owner)
            };
            this = Value::Ref(obj);
            self.pins.push(this);
            call_args.push(this);
        } else if !is_static {
            match target {
                Value::Ref(p) if !p.is_null() && !self.target_fits(owner, p) => {
                    return Err(argument_exception(
                        "Reflection.TargetException",
                        "Object does not match target type.",
                    ))
                }
                Value::Ref(p) if !p.is_null() => call_args.push(target),
                _ => {
                    return Err(argument_exception(
                        "Reflection.TargetException",
                        "Non-static method requires a target.",
                    ))
                }
            }
        }
        for (i, et) in sig.param_types.iter().enumerate() {
            let et = match et {
                ElementType::ByRef(inner) => inner,
                et => et,
            };
            if let Err(e) = self.argument_fits(asm, et, &gen, boxed[i]) {
                self.pins.truncate(pinned);
                return Err(e);
            }
        }
        let mut byrefs = Vec::new();
        for (i, et) in sig.param_types.iter().enumerate() {
            if let ElementType::ByRef(inner) = et {
                let id = self.resolve_sig(asm, inner, &gen);
                let kind = self.kind_of_type(id);
                let cell = gc::with_gc(|gc| gc.alloc(types::TYPE_OBJECT, kind.slot_size().max(16)));
                self.pins.push(Value::Ref(cell));
                store(cell, kind, self.unbox_as(id, boxed[i]));
                byrefs.push((i, cell, id, kind));
                call_args.push(Value::Ref(cell));
            } else {
                let id = self.resolve_sig(asm, et, &gen);
                let v = self.unbox_as(id, boxed[i]);
                self.pins.push(v);
                call_args.push(v);
            }
        }
        let token = Token::new(TableId::MethodDef, row).0;
        let mut target_fn = super::resolve::Target::Managed(asm, row);
        let mut call_gen = gen;
        if is_virtual && !is_static {
            if let Some(Value::Ref(p)) = call_args.first().copied() {
                let previous = self.enter(asm);
                self.devirtualize(token, p, &mut target_fn, &mut call_gen);
                self.enter(previous);
            }
        }
        let result = match target_fn {
            super::resolve::Target::Managed(a2, r2) => {
                match self.execute_in(a2, r2, call_args, call_gen) {
                    super::il::ExecResult::Return(v) => Ok(v),
                    super::il::ExecResult::Exception(e) => Err(e),
                }
            }
            super::resolve::Target::External { .. } => Err(argument_exception(
                "NotSupportedException",
                "Method served by the engine cannot be invoked by reflection.",
            )),
        };
        let result = result.map(|v| {
            for (i, cell, id, kind) in &byrefs {
                let out = self.box_as(*id, load(*cell, *kind));
                unsafe { *(args_array.add(8 + i * 8) as *mut *mut u8) = out.as_ref() };
            }
            if is_ctor {
                return this;
            }
            if matches!(sig.ret_type, ElementType::Void) {
                return Value::Null;
            }
            let ret = self.resolve_sig(asm, &sig.ret_type, &self.gen_of(owner));
            self.box_as(ret, v)
        });
        self.pins.truncate(pinned);
        result
    }

    /// Type désigné par un nom sérialisé dans un attribut (« A.B, Asm »).
    fn type_from_name(&self, name: &str) -> Option<TypeId> {
        match self.type_by_name_value(name) {
            Value::Ref(p) => type_id_of(p),
            _ => None,
        }
    }

    fn attr_type_of_sig(&self, asm: usize, et: &ElementType) -> AttrType {
        use ElementType as E;
        match et {
            E::String => AttrType::String,
            E::Object => AttrType::Object,
            E::SzArray(inner) => AttrType::Array(Box::new(self.attr_type_of_sig(asm, inner))),
            _ => {
                let id = self.resolve_sig(asm, et, &Gen::default());
                if id == types::TYPE_STRING {
                    return AttrType::String;
                }
                let full = types::with_types(|reg| reg.get(id).map(|t| t.full_name()));
                if full.as_deref() == Some("System.Type") {
                    return AttrType::Type;
                }
                let is_enum = types::with_types(|reg| reg.get(id).is_some_and(|t| t.is_enum));
                let kind = self.kind_of_type(id);
                if is_enum {
                    AttrType::Enum(id, kind)
                } else if kind == Kind::Ref {
                    AttrType::Object
                } else {
                    AttrType::Prim(id, kind)
                }
            }
        }
    }

    /// FieldOrPropType d'un argument nommé ou d'une valeur `object`.
    fn attr_type_of_tag(&mut self, r: &mut Reader) -> Option<AttrType> {
        let tag = r.u8()?;
        Some(match tag {
            0x0E => AttrType::String,
            0x50 => AttrType::Type,
            0x51 => AttrType::Object,
            0x1D => AttrType::Array(Box::new(self.attr_type_of_tag(r)?)),
            0x55 => {
                let name = r.ser_string()?.unwrap_or_default();
                let id = self.type_from_name(&name).unwrap_or(types::TYPE_INT32);
                AttrType::Enum(id, self.kind_of_type(id))
            }
            t => {
                let (id, kind) = match t {
                    0x02 => (types::TYPE_BOOLEAN, Kind::Bool),
                    0x03 => (types::TYPE_CHAR, Kind::Char),
                    0x04 => (types::TYPE_SBYTE, Kind::I1),
                    0x05 => (types::TYPE_BYTE, Kind::U1),
                    0x06 => (types::TYPE_INT16, Kind::I2),
                    0x07 => (types::TYPE_UINT16, Kind::U2),
                    0x08 => (types::TYPE_INT32, Kind::I4),
                    0x09 => (types::TYPE_UINT32, Kind::U4),
                    0x0A => (types::TYPE_INT64, Kind::I8),
                    0x0B => (types::TYPE_UINT64, Kind::U8),
                    0x0C => (types::TYPE_FLOAT, Kind::R4),
                    0x0D => (types::TYPE_DOUBLE, Kind::R8),
                    _ => return None,
                };
                AttrType::Prim(id, kind)
            }
        })
    }

    fn read_prim(r: &mut Reader, kind: Kind) -> Option<Value> {
        Some(match kind {
            Kind::Bool | Kind::U1 => Value::I32(r.u8()? as i32),
            Kind::I1 => Value::I32(r.u8()? as i8 as i32),
            Kind::I2 => Value::I32(r.u16()? as i16 as i32),
            Kind::U2 | Kind::Char => Value::I32(r.u16()? as i32),
            Kind::I4 | Kind::U4 => Value::I32(r.u32()? as i32),
            Kind::I8 | Kind::U8 | Kind::I => Value::I64(r.u64()? as i64),
            Kind::R4 => Value::F32(f32::from_bits(r.u32()?)),
            Kind::R8 => Value::F64(f64::from_bits(r.u64()?)),
            _ => return None,
        })
    }

    /// Une valeur d'attribut : brute pour un paramètre de type valeur,
    /// encadrée quand `boxed` (paramètre object, tableau d'objets).
    fn read_attr_value(&mut self, r: &mut Reader, t: &AttrType, boxed: bool) -> Option<Value> {
        Some(match t {
            AttrType::Prim(id, kind) | AttrType::Enum(id, kind) => {
                let v = Self::read_prim(r, *kind)?;
                if boxed {
                    let b = self.box_as(*id, v);
                    self.pins.push(b);
                    b
                } else {
                    v
                }
            }
            AttrType::String => match r.ser_string()? {
                Some(s) => {
                    let v = alloc_managed_string(&s);
                    self.pins.push(v);
                    v
                }
                None => Value::Null,
            },
            AttrType::Type => match r.ser_string()? {
                Some(s) => self.type_or_null(self.type_from_name(&s)),
                None => Value::Null,
            },
            AttrType::Object => {
                let inner = self.attr_type_of_tag(r)?;
                self.read_attr_value(r, &inner, true)?
            }
            AttrType::Array(elem) => {
                let n = r.u32()?;
                if n == u32::MAX {
                    return Some(Value::Null);
                }
                let (elem_id, kind) = match &**elem {
                    AttrType::Prim(id, k) | AttrType::Enum(id, k) => (*id, *k),
                    AttrType::String => (types::TYPE_STRING, Kind::Ref),
                    _ => (types::TYPE_OBJECT, Kind::Ref),
                };
                let size = kind.elem_size();
                let arr = gc::with_gc(|gc| gc.alloc_array_of(elem_id, size, n as usize));
                self.pins.push(Value::Ref(arr));
                for i in 0..n as usize {
                    let boxed_elem = matches!(**elem, AttrType::Object);
                    let v = self.read_attr_value(r, elem, boxed_elem)?;
                    store(unsafe { arr.add(8 + i * size) }, kind, v);
                }
                Value::Ref(arr)
            }
        })
    }

    /// Objets des attributs personnalisés posés sur `parent` (index
    /// HasCustomAttribute codé) dans l'assembly `asm`.
    fn custom_attributes(&mut self, asm: usize, parent: u32) -> Result<Vec<Value>, ClrException> {
        let rows: Vec<(u32, Vec<u8>)> = {
            let a = self.assembly_at(asm);
            (1..=a.row_count(TableId::CustomAttribute))
                .map(|r| a.get_custom_attribute(r))
                .filter(|c| c.parent == parent)
                .map(|c| (c.typ, a.get_blob(c.value).to_vec()))
                .collect()
        };
        let mut out = Vec::new();
        for (ctor, blob) in rows {
            let (table, row) = CodedIndex::CustomAttributeType.decode(ctor);
            let token = match table {
                Some(TableId::MethodDef) => Token::new(TableId::MethodDef, row).0,
                Some(TableId::MemberRef) => Token::new(TableId::MemberRef, row).0,
                _ => continue,
            };
            let previous = self.enter(asm);
            let callee = self.resolve_callee(token);
            let sig = self.get_method_sig(token);
            self.enter(previous);
            let super::resolve::Target::Managed(casm, crow) = callee.target else {
                // Attribut d'une assembly absente : ignoré, comme un type
                // qu'on ne peut charger.
                continue;
            };
            let owner_row = self
                .domain
                .get(casm)
                .and_then(|i| i.method_owner.get(crow as usize).copied())
                .unwrap_or(0);
            let Some(owner) = self
                .domain
                .get(casm)
                .and_then(|i| i.type_ids.get(owner_row as usize).copied().flatten())
            else {
                continue;
            };
            let pinned = self.pins.len();
            let mut r = Reader {
                data: &blob,
                pos: 0,
            };
            if r.u16() != Some(1) {
                continue;
            }
            self.ensure_layout(owner);
            let size = types::with_types(|reg| {
                reg.get(owner)
                    .map_or(types::EXTERNAL_OBJECT_SIZE, |t| t.instance_size)
            });
            let obj = gc::with_gc(|gc| gc.alloc(owner, size.max(16)));
            self.pins.push(Value::Ref(obj));
            let mut args = vec![Value::Ref(obj)];
            let mut ok = true;
            for et in &sig.param_types {
                let t = self.attr_type_of_sig(asm, et);
                match self.read_attr_value(&mut r, &t, false) {
                    Some(v) => args.push(v),
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                self.pins.truncate(pinned);
                continue;
            }
            if let super::il::ExecResult::Exception(e) =
                self.execute_in(casm, crow, args, callee.gen.clone())
            {
                self.pins.truncate(pinned);
                return Err(e);
            }
            let named = r.u16().unwrap_or(0);
            for _ in 0..named {
                let Some(kind) = r.u8() else { break };
                let Some(t) = self.attr_type_of_tag(&mut r) else {
                    break;
                };
                let Some(Some(name)) = r.ser_string() else {
                    break;
                };
                let Some(v) = self.read_attr_value(&mut r, &t, false) else {
                    break;
                };
                self.set_named(obj, owner, kind == 0x53, &name, v)?;
            }
            out.push(Value::Ref(obj));
            self.pins.truncate(pinned);
            self.pins.push(Value::Ref(obj));
        }
        Ok(out)
    }

    /// Argument nommé d'un attribut : champ (0x53) ou propriété (0x54),
    /// cherché dans le type puis ses parents.
    fn set_named(
        &mut self,
        obj: *mut u8,
        owner: TypeId,
        field: bool,
        name: &str,
        v: Value,
    ) -> Result<(), ClrException> {
        let mut t = owner;
        while t != 0 {
            if let Some((asm, row)) = self.def_of(t) {
                if field {
                    let found = {
                        let a = self.assembly_at(asm);
                        a.type_fields(row)
                            .find(|&f| a.get_string(a.get_field(f).name) == name)
                    };
                    if let Some(f) = found {
                        let (ptr, kind) = self.field_slot(asm, f, t, obj)?;
                        store(ptr, kind, v);
                        return Ok(());
                    }
                } else {
                    let setter = format!("set_{name}");
                    let found = {
                        let a = self.assembly_at(asm);
                        a.type_methods(row)
                            .find(|&m| a.get_string(a.get_method_def(m).name) == setter)
                    };
                    if let Some(m) = found {
                        let gen = self.gen_of(t);
                        if let super::il::ExecResult::Exception(e) =
                            self.execute_in(asm, m, vec![Value::Ref(obj), v], gen)
                        {
                            return Err(e);
                        }
                        return Ok(());
                    }
                }
            }
            t = types::with_types(|reg| reg.get(t).map_or(0, |i| i.parent));
        }
        Ok(())
    }

    /// Tous les types de `t` vers la racine, interfaces comprises : pour
    /// IsAssignableFrom.
    fn assignable(&self, target: TypeId, from: TypeId) -> bool {
        if target == from || target == types::TYPE_OBJECT {
            return true;
        }
        let mut t = from;
        while t != 0 {
            if t == target {
                return true;
            }
            t = types::with_types(|reg| reg.get(t).map_or(0, |i| i.parent));
        }
        let is_iface = types::with_types(|reg| reg.get(target).is_some_and(|i| i.is_interface));
        is_iface && self.implements(from, target).unwrap_or(false)
    }

    fn interfaces_of(&self, id: TypeId) -> Vec<Value> {
        let mut out: Vec<TypeId> = Vec::new();
        let mut t = id;
        while t != 0 {
            if let Some((asm, row)) = self.def_of(t) {
                let gen = self.gen_of(t);
                let a = self.assembly_at(asm);
                let list: Vec<u32> = (1..=a.row_count(TableId::InterfaceImpl))
                    .map(|r| a.get_interface_impl(r))
                    .filter(|i| i.class == row)
                    .map(|i| i.interface)
                    .collect();
                for coded in list {
                    let iface = self.resolve_coded(asm, coded, &gen);
                    if !out.contains(&iface) {
                        out.push(iface);
                    }
                }
            }
            t = types::with_types(|reg| reg.get(t).map_or(0, |i| i.parent));
        }
        out.into_iter().map(|i| self.type_object(i)).collect()
    }

    /// Primitives de System.Reflection.Rtti (notre mscorlib).
    pub(super) fn member_call(
        &mut self,
        namespace: &str,
        class: &str,
        name: &str,
        args: &[Value],
    ) -> Option<Result<Value, ClrException>> {
        if (namespace, class) != ("System.Reflection", "Rtti") {
            return None;
        }
        let pinned = self.pins.len();
        self.pins.extend(args.iter().copied());
        let r = self.member_call_inner(name, args);
        self.pins.truncate(pinned);
        Some(r)
    }

    /// Une poignée de membre vient du code managé (un champ privé de
    /// MethodInfo peut être réécrit par réflexion) : elle doit désigner une
    /// assembly du domaine et une ligne existante de la bonne table, sinon
    /// `assembly_at` ou la lecture de la ligne paniqueraient.
    fn member_handle_ok(&self, h: i64, table: TableId) -> bool {
        let (asm, row) = split(h);
        h >= 0
            && self
                .try_assembly_at(asm)
                .is_some_and(|a| row >= 1 && row <= a.row_count(table))
    }

    fn member_call_inner(&mut self, name: &str, args: &[Value]) -> Result<Value, ClrException> {
        let table = match name {
            "NomMethode"
            | "DrapeauxMethode"
            | "Retour"
            | "TypesParametres"
            | "ParametresParReference"
            | "NomsParametres"
            | "AriteMethode"
            | "Invoquer"
            | "AttributsMethode" => Some(TableId::MethodDef),
            "NomChamp" | "DrapeauxChamp" | "TypeChamp" | "Constante" | "LireChamp"
            | "EcrireChamp" | "AttributsChamp" => Some(TableId::Field),
            "NomPropriete" | "TypePropriete" | "Accesseur" | "AttributsPropriete" => {
                Some(TableId::Property)
            }
            _ => None,
        };
        if let Some(table) = table {
            if !self.member_handle_ok(arg_i64(args, 0), table) {
                return Err(argument_exception(
                    "ArgumentException",
                    "Invalid member handle.",
                ));
            }
        }
        let h = || split(arg_i64(args, 0));
        Ok(match name {
            // --- Type ---
            "Methodes" => long_array(&self.methods_of(self.owner_arg(args, 0))),
            "Champs" => long_array(&self.fields_of(self.owner_arg(args, 0))),
            "Proprietes" => long_array(&self.properties_of(self.owner_arg(args, 0))),
            "DrapeauxType" => {
                let id = self.owner_arg(args, 0);
                Value::I32(self.def_of(id).map_or(0x0001, |(asm, row)| {
                    self.assembly_at(asm).get_type_def(row).flags as i32
                }))
            }
            "ArgumentsGeneriques" => {
                let id = self.owner_arg(args, 0);
                let list = types::with_types(|reg| reg.get(id).map(|t| t.type_args.clone()))
                    .unwrap_or_default();
                let v: Vec<Value> = list.into_iter().map(|t| self.type_object(t)).collect();
                ref_array(&v)
            }
            "AriteGenerique" => {
                let id = self.owner_arg(args, 0);
                Value::I32(types::with_types(|reg| {
                    reg.get(id).map_or(0, |t| t.generic_params as i32)
                }))
            }
            "DefinitionGenerique" => {
                let id = self.owner_arg(args, 0);
                let def = types::with_types(|reg| {
                    let t = reg.get(id)?;
                    if t.type_args.is_empty() {
                        return None;
                    }
                    reg.find_definition(id)
                });
                self.type_or_null(def)
            }
            "Instancier" => {
                let def = self.owner_arg(args, 0);
                let targs: Vec<TypeId> = array_refs(arg_ref(args, 1))
                    .into_iter()
                    .map(|v| match v {
                        Value::Ref(p) => type_id_of(p).unwrap_or(types::TYPE_OBJECT),
                        _ => types::TYPE_OBJECT,
                    })
                    .collect();
                let id = self.instantiate(def, targs);
                self.type_object(id)
            }
            "Assignable" => {
                let target = self.owner_arg(args, 0);
                let from = self.owner_arg(args, 1);
                Value::I32(self.assignable(target, from) as i32)
            }
            "Interfaces" => {
                let v = self.interfaces_of(self.owner_arg(args, 0));
                ref_array(&v)
            }
            "AttributsType" => {
                let id = self.owner_arg(args, 0);
                let Some((asm, row)) = self.def_of(id) else {
                    return Ok(ref_array(&[]));
                };
                let coded = CodedIndex::HasCustomAttribute
                    .encode(TableId::TypeDef, row)
                    .unwrap_or(0);
                let v = self.custom_attributes(asm, coded)?;
                ref_array(&v)
            }
            "AttributsAssembly" => {
                // Poignée : indice de l'assembly.
                let asm = arg_i64(args, 0) as usize;
                if asm >= self.domain.len() {
                    return Ok(ref_array(&[]));
                }
                let coded = CodedIndex::HasCustomAttribute
                    .encode(TableId::Assembly, 1)
                    .unwrap_or(0);
                let v = self.custom_attributes(asm, coded)?;
                ref_array(&v)
            }
            "AssemblyDe" => {
                let p = arg_ref(args, 0);
                Value::I64(super::reflection::assembly_index_of(p).map_or(-1, |i| i as i64))
            }
            // --- Méthodes ---
            "NomMethode" => {
                let (asm, row) = h();
                let a = self.assembly_at(asm);
                alloc_managed_string(a.get_string(a.get_method_def(row).name))
            }
            "DrapeauxMethode" => {
                let (asm, row) = h();
                Value::I32(i32::from(self.assembly_at(asm).get_method_def(row).flags))
            }
            "Retour" => {
                let (asm, row) = h();
                let sig = self.method_sig_of(asm, row);
                if matches!(sig.ret_type, ElementType::Void) {
                    let void = self.find_void();
                    self.type_or_null(void)
                } else {
                    let id =
                        self.resolve_sig(asm, &sig.ret_type, &self.gen_of(self.owner_arg(args, 1)));
                    self.type_object(id)
                }
            }
            "TypesParametres" => {
                let (asm, row) = h();
                let sig = self.method_sig_of(asm, row);
                let gen = self.gen_of(self.owner_arg(args, 1));
                let v: Vec<Value> = sig
                    .param_types
                    .iter()
                    .map(|et| {
                        let inner = match et {
                            ElementType::ByRef(i) => i,
                            e => e,
                        };
                        self.type_object(self.resolve_sig(asm, inner, &gen))
                    })
                    .collect();
                ref_array(&v)
            }
            "ParametresParReference" => {
                let (asm, row) = h();
                let sig = self.method_sig_of(asm, row);
                let mut mask = 0i64;
                for (i, et) in sig.param_types.iter().enumerate().take(63) {
                    if matches!(et, ElementType::ByRef(_)) {
                        mask |= 1 << i;
                    }
                }
                Value::I64(mask)
            }
            "NomsParametres" => {
                let (asm, row) = h();
                let a = self.assembly_at(asm);
                let count = self.method_sig_of(asm, row).param_types.len();
                let mut names = vec![String::new(); count];
                for p in a.method_params(row) {
                    let pr = a.get_param(p);
                    if pr.sequence >= 1 && (pr.sequence as usize) <= count {
                        names[pr.sequence as usize - 1] = a.get_string(pr.name).to_string();
                    }
                }
                super::bcl::alloc_string_array(&names)
            }
            "AriteMethode" => {
                let (asm, row) = h();
                Value::I32(self.method_sig_of(asm, row).generic_param_count as i32)
            }
            "Invoquer" => {
                let (asm, row) = h();
                let owner = self.owner_arg(args, 1);
                let target = args.get(2).copied().unwrap_or(Value::Null);
                self.invoke_member(asm, row, owner, target, arg_ref(args, 3))?
            }
            "AttributsMethode" => {
                let (asm, row) = h();
                let coded = CodedIndex::HasCustomAttribute
                    .encode(TableId::MethodDef, row)
                    .unwrap_or(0);
                let v = self.custom_attributes(asm, coded)?;
                ref_array(&v)
            }
            // --- Champs ---
            "NomChamp" => {
                let (asm, row) = h();
                let a = self.assembly_at(asm);
                alloc_managed_string(a.get_string(a.get_field(row).name))
            }
            "DrapeauxChamp" => {
                let (asm, row) = h();
                Value::I32(i32::from(self.assembly_at(asm).get_field(row).flags))
            }
            "TypeChamp" => {
                let (asm, row) = h();
                let id = self.field_type(asm, row, self.owner_arg(args, 1));
                self.type_object(id)
            }
            "Constante" => {
                let (asm, row) = h();
                self.constant_of(asm, row, self.owner_arg(args, 1))
            }
            "LireChamp" => {
                let (asm, row) = h();
                let owner = self.owner_arg(args, 1);
                let (ptr, kind) = self.field_slot(asm, row, owner, arg_ref(args, 2))?;
                let id = self.field_type(asm, row, owner);
                let v = load(ptr, kind);
                self.box_as(id, v)
            }
            "EcrireChamp" => {
                let (asm, row) = h();
                let owner = self.owner_arg(args, 1);
                let id = self.field_type(asm, row, owner);
                let value = args.get(3).copied().unwrap_or(Value::Null);
                self.argument_fits(asm, &self.field_sig(asm, row), &self.gen_of(owner), value)?;
                let v = self.unbox_as(id, value);
                let (ptr, kind) = self.field_slot(asm, row, owner, arg_ref(args, 2))?;
                store(ptr, kind, v);
                Value::Null
            }
            "AttributsChamp" => {
                let (asm, row) = h();
                let coded = CodedIndex::HasCustomAttribute
                    .encode(TableId::Field, row)
                    .unwrap_or(0);
                let v = self.custom_attributes(asm, coded)?;
                ref_array(&v)
            }
            // --- Propriétés ---
            "NomPropriete" => {
                let (asm, row) = h();
                let a = self.assembly_at(asm);
                alloc_managed_string(a.get_string(a.get_property(row).name))
            }
            "TypePropriete" => {
                let (asm, row) = h();
                let id = self.property_type(asm, row, self.owner_arg(args, 1));
                self.type_object(id)
            }
            "Accesseur" => {
                let (asm, row) = h();
                Value::I64(self.accessor(asm, row, args.get(1).is_some_and(|v| v.as_i32() != 0)))
            }
            "AttributsPropriete" => {
                let (asm, row) = h();
                let coded = CodedIndex::HasCustomAttribute
                    .encode(TableId::Property, row)
                    .unwrap_or(0);
                let v = self.custom_attributes(asm, coded)?;
                ref_array(&v)
            }
            // --- Objets ---
            "Encadrer" => {
                // Enum.ToObject : valeur entière rangée dans le type donné.
                let id = self.owner_arg(args, 0);
                let raw = args.get(1).copied().unwrap_or(Value::I64(0));
                let v = match self.kind_of_type(id) {
                    Kind::I8 | Kind::U8 | Kind::I => Value::I64(raw.as_i64()),
                    _ => Value::I32(raw.as_i64() as i32),
                };
                self.box_as(id, v)
            }
            "ValeurEntiere" => {
                // Valeur d'une énumération (ou d'un entier) encadrée.
                let p = arg_ref(args, 0);
                if p.is_null() {
                    return Ok(Value::I64(0));
                }
                let Some(id) = gc::type_of(p) else {
                    return Ok(Value::I64(0));
                };
                let v = match self.kind_of_type(id) {
                    Kind::Ref | Kind::Struct(_) => Value::I64(0),
                    k => load(p, k),
                };
                Value::I64(match v {
                    Value::I32(x) => match self.kind_of_type(id) {
                        Kind::U4 => i64::from(x as u32),
                        Kind::U2 | Kind::Char => i64::from(x as u16),
                        Kind::U1 | Kind::Bool => i64::from(x as u8),
                        _ => i64::from(x),
                    },
                    other => other.as_i64(),
                })
            }
            "AssemblyParNom" => {
                let s = arg_ref(args, 0);
                if s.is_null() {
                    return Ok(Value::Null);
                }
                let text = read_managed_string(s);
                let simple = text
                    .split(',')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase();
                match self.domain.iter().position(|i| i.name == simple) {
                    Some(i) => self.assembly_object(i),
                    None if super::domain::is_framework_assembly(&simple) => {
                        match self
                            .domain
                            .iter()
                            .position(|i| i.name == super::domain::CORLIB)
                        {
                            Some(i) => self.assembly_object(i),
                            None => Value::Null,
                        }
                    }
                    None => Value::Null,
                }
            }
            // AppDomain.GetAssemblies (#2054) : toutes les assemblies chargées.
            "Assemblies" => {
                let v: Vec<Value> = (0..self.domain.len())
                    .map(|i| self.assembly_object(i))
                    .collect();
                ref_array(&v)
            }
            "TypeDansAssembly" => {
                let asm = arg_i64(args, 0);
                let s = arg_ref(args, 1);
                if asm < 0 || s.is_null() {
                    return Ok(Value::Null);
                }
                let text = read_managed_string(s);
                let ignore = args.get(2).is_some_and(|v| v.as_i32() != 0);
                let id = self.find_type_in(asm as usize, &text, ignore);
                self.type_or_null(id)
            }
            "TypesAssembly" => {
                let asm = arg_i64(args, 0);
                let ids: Vec<TypeId> = match self.domain.get(asm.max(0) as usize) {
                    Some(info) if asm >= 0 => {
                        info.type_ids.iter().skip(2).flatten().copied().collect()
                    }
                    _ => Vec::new(),
                };
                let v: Vec<Value> = ids.into_iter().map(|t| self.type_object(t)).collect();
                ref_array(&v)
            }
            "NouvelleValeur" => {
                let id = self.owner_arg(args, 0);
                match self.kind_of_type(id) {
                    Kind::Ref => Value::Null,
                    Kind::Struct(_) => Value::Ref(self.new_struct(id)),
                    k => {
                        let zero = match k {
                            Kind::R4 => Value::F32(0.0),
                            Kind::R8 => Value::F64(0.0),
                            Kind::I8 | Kind::U8 | Kind::I => Value::I64(0),
                            _ => Value::I32(0),
                        };
                        self.box_as(id, zero)
                    }
                }
            }
            "TableauType" => {
                // Tableau d'éléments `id` rempli depuis des valeurs encadrées
                // (Enum.GetValues rend un MonEnum[]).
                let id = self.owner_arg(args, 0);
                let items = array_refs(arg_ref(args, 1));
                let kind = self.kind_of_type(id);
                let size = kind.elem_size();
                let arr = gc::with_gc(|gc| gc.alloc_array_of(id, size, items.len()));
                for (i, v) in items.iter().enumerate() {
                    let raw = self.unbox_as(id, *v);
                    store(unsafe { arr.add(8 + i * size) }, kind, raw);
                }
                Value::Ref(arr)
            }
            "NomType" => {
                let s = arg_ref(args, 0);
                if s.is_null() {
                    Value::Null
                } else {
                    let text = read_managed_string(s);
                    self.type_by_name_value(&text)
                }
            }
            _ => {
                return Err(ClrException::new(
                    "System",
                    "MissingMethodException",
                    &format!("Method not found: System.Reflection.Rtti::{name}"),
                ))
            }
        })
    }
}
