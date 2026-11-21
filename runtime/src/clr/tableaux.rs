//! Méthodes de `System.Array` servies par le moteur (#1962) : copie,
//! effacement, inversion, clonage et création d'un tableau d'un type donné.
//! Ces opérations travaillent sur les octets des éléments, que seul le
//! moteur connaît ; les méthodes génériques (`IndexOf<T>`, `Resize<T>`…)
//! sont écrites en C# dans bcl/mscorlib.
//!
//! Écrit d'après la documentation publique « Array Class » et ECMA-335 ;
//! aucun code de Microsoft, de Mono ou de Wine.

use super::exceptions::ClrException;
use super::gc;
use super::il::{load, store, Interpreter, Value};
use super::metadata::{self, ElementType};
use super::tables::{CodedIndex, TableId, Token};
use super::types::{self, Kind, TypeId};

/// Données d'un tableau à une dimension : longueur (u64) puis éléments.
const DATA: usize = 8;

fn length(arr: *mut u8) -> i64 {
    unsafe { *(arr as *const u64) as i64 }
}

/// Bornes (borne inférieure, longueur) de chaque dimension : une seule,
/// (0, longueur), pour un tableau à une dimension (#2848).
fn bounds_of(arr: *mut u8) -> Vec<(i32, i32)> {
    gc::array_bounds(arr).unwrap_or_else(|| vec![(0, length(arr) as i32)])
}

/// Rang d'élément, à plat, désigné par un indice par dimension (rangement
/// ligne par ligne) : ArgumentException si le nombre d'indices n'est pas
/// le rang, IndexOutOfRangeException hors des bornes (documentation
/// « Array.GetValue »).
pub(super) fn flat_index(arr: *mut u8, indices: &[i64]) -> Result<usize, ClrException> {
    let bounds = bounds_of(arr);
    if indices.len() != bounds.len() {
        return Err(ClrException::new(
            "System",
            "ArgumentException",
            "Indices length does not match the array rank.",
        ));
    }
    let mut flat = 0usize;
    for (&i, &(lo, n)) in indices.iter().zip(&bounds) {
        let r = i - lo as i64;
        if r < 0 || r >= n as i64 {
            return Err(ClrException::index_out_of_range());
        }
        flat = flat * n as usize + r as usize;
    }
    Ok(flat)
}

fn null_arg(name: &str) -> ClrException {
    ClrException::new(
        "System",
        "ArgumentNullException",
        &format!("Value cannot be null.\r\nParameter name: {name}"),
    )
}

fn out_of_range(name: &str) -> ClrException {
    ClrException::new(
        "System",
        "ArgumentOutOfRangeException",
        &format!(
            "Specified argument was out of the range of valid values.\r\nParameter name: {name}"
        ),
    )
}

fn array_arg(v: Option<&Value>, name: &str) -> Result<*mut u8, ClrException> {
    match v {
        Some(Value::Ref(p)) if !p.is_null() => Ok(*p),
        _ => Err(null_arg(name)),
    }
}

/// Poignée d'un champ (`ldtoken`) : le jeton FieldDef et, au-dessus,
/// l'indice + 1 de l'assembly qui le déclare.
pub(super) fn field_handle(assembly: usize, token: u32) -> i64 {
    (((assembly as u64) + 1) << 32 | token as u64) as i64
}

/// Interfaces de collection qu'un tableau à une dimension implémente
/// (ECMA-335 §II.14.2 et documentation « Array Class » : `T[]` est un
/// `IList<T>` et un `IReadOnlyList<T>`) (#1975).
pub(super) const ARRAY_INTERFACES: &[&str] = &[
    "IEnumerable`1",
    "IEnumerable",
    "ICollection`1",
    "ICollection",
    "IList`1",
    "IList",
    "IReadOnlyCollection`1",
    "IReadOnlyList`1",
];

/// Énumérateur d'un tableau : tableau à +8, indice courant à +16 (-1 avant
/// le premier MoveNext), 1 à +24 s'il rend les éléments sans les encadrer
/// (`IEnumerator<T>`).
const E_ARRAY: usize = 8;
const E_INDEX: usize = 16;
const E_GENERIC: usize = 24;

fn read_word(p: *mut u8, off: usize) -> u64 {
    unsafe { (p.add(off) as *const u64).read_unaligned() }
}

fn write_word(p: *mut u8, off: usize, v: u64) {
    unsafe { (p.add(off) as *mut u64).write_unaligned(v) }
}

fn fixed_size() -> ClrException {
    ClrException::new(
        "System",
        "NotSupportedException",
        "Collection was of a fixed size.",
    )
}

fn enumerator_type() -> u32 {
    super::types::with_types(|reg| reg.ensure_external("System", "SZArrayEnumerator"))
}

impl Interpreter {
    /// Encadre l'élément `v` d'un tableau de valeurs, pour le rendre comme
    /// `object`.
    fn box_elem(&mut self, arr: *mut u8, kind: Kind, v: Value) -> Value {
        if kind == Kind::Ref {
            return v;
        }
        let id = gc::array_elem_type(arr);
        let boxed = if matches!(kind, Kind::Struct(_)) {
            self.new_struct(id)
        } else {
            gc::with_gc(|gc| gc.alloc(id, 8))
        };
        store(boxed, kind, v);
        Value::Ref(boxed)
    }

    /// Égalité de deux éléments pour Contains/IndexOf : octets égaux pour
    /// une valeur ; même objet, ou même contenu pour une chaîne ou une
    /// valeur encadrée, pour une référence.
    fn elem_equal(kind: Kind, a: Value, b: Value) -> bool {
        match kind {
            Kind::Ref => {
                let (x, y) = (a.as_ref(), b.as_ref());
                x == y || (!x.is_null() && !y.is_null() && super::bcl::boxed_equal(x, y))
            }
            Kind::R4 | Kind::R8 => a.as_f64() == b.as_f64(),
            Kind::Struct(_) => {
                let n = kind.elem_size();
                let (x, y) = (a.as_ref(), b.as_ref());
                !x.is_null()
                    && !y.is_null()
                    && unsafe {
                        std::slice::from_raw_parts(x, n) == std::slice::from_raw_parts(y, n)
                    }
            }
            _ => a.as_i64() == b.as_i64(),
        }
    }

    /// Membres des interfaces de collection appelés sur un tableau ou sur
    /// son énumérateur : le tableau n'a pas de table de méthodes à lui.
    fn array_interface_call(
        &mut self,
        namespace: &str,
        class: &str,
        name: &str,
        args: &[Value],
    ) -> Option<Result<Value, ClrException>> {
        let this = match args.first() {
            Some(Value::Ref(p)) if !p.is_null() => *p,
            _ => return None,
        };
        let ty = gc::type_of(this)?;
        if ty == enumerator_type() {
            return self.array_enumerator_call(this, name);
        }
        if ty != super::types::TYPE_ARRAY
            || !(namespace == "System.Collections.Generic" || namespace == "System.Collections")
            || !ARRAY_INTERFACES.contains(&class)
        {
            return None;
        }
        let kind = self.elem_kind(this);
        let size = kind.elem_size();
        let n = length(this);
        let at = |i: i64| unsafe { this.add(DATA + i as usize * size) };
        let index_arg = |i: usize| args.get(i).map_or(-1, |v| v.as_i64());
        let generic = class.ends_with("`1");
        let r = match name {
            "GetEnumerator" => {
                let e = gc::with_gc(|gc| {
                    gc.alloc(enumerator_type(), super::types::EXTERNAL_OBJECT_SIZE)
                });
                write_word(e, E_ARRAY, this as u64);
                write_word(e, E_INDEX, u64::MAX);
                write_word(e, E_GENERIC, generic as u64);
                Ok(Value::Ref(e))
            }
            "get_Count" => Ok(Value::I32(n as i32)),
            "get_IsReadOnly" | "get_IsFixedSize" | "get_IsSynchronized" => {
                Ok(Value::I32((name == "get_IsFixedSize") as i32))
            }
            "get_SyncRoot" => Ok(Value::Ref(this)),
            "get_Item" => {
                let i = index_arg(1);
                if i < 0 || i >= n {
                    Err(out_of_range("index"))
                } else {
                    let v = load(at(i), kind);
                    Ok(if generic {
                        v
                    } else {
                        self.box_elem(this, kind, v)
                    })
                }
            }
            "set_Item" => {
                let i = index_arg(1);
                if i < 0 || i >= n {
                    Err(out_of_range("index"))
                } else {
                    let v = args.get(2).copied().unwrap_or(Value::Null);
                    // IList.this[] (non générique) reçoit un objet encadré.
                    match (generic || kind == Kind::Ref, self.unbox_elem(this, kind, v)) {
                        (true, _) => {
                            store(at(i), kind, v);
                            Ok(Value::Null)
                        }
                        (false, Ok(Some(raw))) => {
                            store(at(i), kind, raw);
                            Ok(Value::Null)
                        }
                        (false, Ok(None)) => Err(ClrException::null_reference()),
                        (false, Err(e)) => Err(e),
                    }
                }
            }
            "Contains" | "IndexOf" => {
                let item = args.get(1).copied().unwrap_or(Value::Null);
                // IList.Contains(object) sur un tableau de valeurs : la
                // valeur arrive encadrée.
                let item = if !generic && kind != Kind::Ref {
                    match self.unbox_elem(this, kind, item) {
                        Ok(Some(raw)) => raw,
                        // null ou un objet d'un autre type n'y est pas.
                        _ => return Some(Ok(Value::I32(if name == "Contains" { 0 } else { -1 }))),
                    }
                } else {
                    item
                };
                let found = (0..n).find(|&i| Self::elem_equal(kind, load(at(i), kind), item));
                Ok(Value::I32(match (name, found) {
                    ("Contains", f) => f.is_some() as i32,
                    (_, Some(i)) => i as i32,
                    (_, None) => -1,
                }))
            }
            "CopyTo" => match args {
                [_, dst, index] => {
                    self.array_copy(&[Value::Ref(this), Value::I32(0), *dst, *index, Value::I64(n)])
                }
                _ => return None,
            },
            "Add" | "Insert" | "Remove" | "RemoveAt" | "Clear" => Err(fixed_size()),
            _ => return None,
        };
        Some(r)
    }

    fn array_enumerator_call(
        &mut self,
        e: *mut u8,
        name: &str,
    ) -> Option<Result<Value, ClrException>> {
        let arr = read_word(e, E_ARRAY) as *mut u8;
        let index = read_word(e, E_INDEX) as i64;
        let r = match name {
            "MoveNext" => {
                let next = index + 1;
                if next < length(arr) {
                    write_word(e, E_INDEX, next as u64);
                    Ok(Value::I32(1))
                } else {
                    write_word(e, E_INDEX, length(arr) as u64);
                    Ok(Value::I32(0))
                }
            }
            "get_Current" => {
                if index < 0 || index >= length(arr) {
                    Err(ClrException::new(
                        "System",
                        "InvalidOperationException",
                        if index < 0 {
                            "Enumeration has not started. Call MoveNext."
                        } else {
                            "Enumeration already finished."
                        },
                    ))
                } else {
                    let kind = self.elem_kind(arr);
                    let v = load(
                        unsafe { arr.add(DATA + index as usize * kind.elem_size()) },
                        kind,
                    );
                    Ok(if read_word(e, E_GENERIC) != 0 {
                        v
                    } else {
                        self.box_elem(arr, kind, v)
                    })
                }
            }
            "Reset" => {
                write_word(e, E_INDEX, u64::MAX);
                Ok(Value::Null)
            }
            "Dispose" => Ok(Value::Null),
            _ => return None,
        };
        Some(r)
    }

    /// `RuntimeHelpers.InitializeArray(array, champ)` : copie dans le
    /// tableau les octets du champ à RVA que le compilateur a émis pour son
    /// initialiseur (ECMA-335 §II.16.3.2, table FieldRVA §II.22.18).
    fn initialize_array(&self, args: &[Value]) -> Result<Value, ClrException> {
        let arr = array_arg(args.first(), "array")?;
        let handle = args.get(1).map_or(0, |v| v.as_i64()) as u64;
        let (index, token) = ((handle >> 32) as usize, handle as u32);
        let bad = || {
            ClrException::new(
                "System",
                "ArgumentException",
                "The field handle does not point to an initialized static field.",
            )
        };
        if index == 0 || Token(token).table() != TableId::Field as u8 {
            return Err(bad());
        }
        // Poignée forgée possible : indice borné au domaine (#1968).
        let asm = self.try_assembly_at(index - 1).ok_or_else(bad)?;
        let field = Token(token).rid();
        let rva = (1..=asm.row_count(TableId::FieldRva))
            .map(|row| asm.get_field_rva(row))
            .find(|r| r.field == field)
            .map(|r| r.rva)
            .ok_or_else(bad)?;
        let kind = self.elem_kind(arr);
        if kind == Kind::Ref {
            return Err(ClrException::new(
                "System",
                "ArgumentException",
                "The array must be of a primitive or value type.",
            ));
        }
        let size = length(arr) as usize * kind.elem_size();
        let src = asm.rva_slice(rva, size).ok_or_else(bad)?;
        unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), arr.add(DATA), size) };
        Ok(Value::Null)
    }

    /// `isinst`/`castclass` vers un type tableau `T[]` ou `T[,]` (jeton TypeSpec) :
    /// l'objet doit être un tableau dont les éléments conviennent (#2023).
    /// Un tableau de références ne garde pas le type de ses éléments : tout
    /// `U[]` de références est accepté pour un `T[]` de références. Pour les
    /// types valeur, l'élément doit être le même, au signe près pour les
    /// entiers (int[] vers uint[], une énumération vers son type de base),
    /// comme le permet ECMA-335 §I.8.7.1. `None` : pas un type tableau.
    pub(super) fn array_cast(&self, token: u32, obj: *mut u8) -> Option<bool> {
        if obj.is_null() || Token(token).table() != TableId::TypeSpec as u8 {
            return None;
        }
        let blob = {
            let a = &self.assembly;
            a.get_blob(a.get_type_spec(Token(token).rid()).signature)
                .to_vec()
        };
        let mut off = 0;
        // `T[,]` demande le même rang ; un `T[]` n'est pas un tableau à
        // plusieurs dimensions (#2848).
        let (inner, rank) = match metadata::decode_type(&blob, &mut off) {
            ElementType::SzArray(inner) => (inner, None),
            ElementType::Array(inner, rank) => (inner, Some(rank as usize)),
            _ => return None,
        };
        let actual = gc::type_of(obj)?;
        if actual != types::TYPE_ARRAY || gc::array_bounds(obj).map(|b| b.len()) != rank {
            return Some(false);
        }
        let target = self.resolve_sig(self.current, &inner, &self.current_gen());
        Some(self.array_elements_fit(target, obj))
    }

    /// Vrai si les éléments du tableau `obj` conviennent à un `target[]`
    /// (#2023) : références pour un type référence, même représentation
    /// pour un type valeur (int[] pour uint[], enum pour son entier).
    pub(super) fn array_elements_fit(&self, target: TypeId, obj: *mut u8) -> bool {
        let target_kind = self.kind_of_type(target);
        let elem = gc::array_elem_type(obj);
        if target_kind == Kind::Ref {
            return elem == 0;
        }
        if elem == 0 {
            return false;
        }
        let kind = self.kind_of_type(elem);
        let integer = |k: Kind| match k {
            Kind::I1 | Kind::U1 => Some(1),
            Kind::I2 | Kind::U2 => Some(2),
            Kind::I4 | Kind::U4 => Some(4),
            Kind::I8 | Kind::U8 => Some(8),
            Kind::I => Some(16),
            _ => None,
        };
        elem == target
            || kind == target_kind && !matches!(kind, Kind::Struct(_))
            || integer(kind).is_some() && integer(kind) == integer(target_kind)
    }

    /// Valeur à ranger dans un tableau de valeurs `arr` depuis un objet
    /// encadré (Array.SetValue, IList.this[], IList.Contains) : l'objet doit
    /// être de même représentation que les éléments, sinon on lirait une
    /// structure dans un objet plus petit (#2037). `Ok(None)` : null.
    fn unbox_elem(
        &self,
        arr: *mut u8,
        kind: Kind,
        v: Value,
    ) -> Result<Option<Value>, ClrException> {
        let p = match v {
            Value::Ref(p) if !p.is_null() => p,
            _ => return Ok(None),
        };
        let fits = match (gc::type_of(p), kind) {
            (None, _) => false,
            (Some(actual), Kind::Struct(_)) => actual == gc::array_elem_type(arr),
            (Some(actual), _) => !matches!(self.kind_of_type(actual), Kind::Ref | Kind::Struct(_)),
        };
        if fits {
            Ok(Some(load(p, kind)))
        } else {
            Err(ClrException::new(
                "System",
                "InvalidCastException",
                "Object cannot be stored in an array of this type.",
            ))
        }
    }

    /// Façon de ranger un élément du tableau `arr`.
    fn elem_kind(&self, arr: *mut u8) -> Kind {
        match gc::array_elem_type(arr) {
            0 => Kind::Ref,
            id => self.kind_of_type(id),
        }
    }

    pub(super) fn array_call(
        &mut self,
        namespace: &str,
        class: &str,
        name: &str,
        args: &[Value],
    ) -> Option<Result<Value, ClrException>> {
        if (namespace, class, name)
            == (
                "System.Runtime.CompilerServices",
                "RuntimeHelpers",
                "InitializeArray",
            )
        {
            return Some(self.initialize_array(args));
        }
        if let Some(r) = self.array_interface_call(namespace, class, name, args) {
            return Some(r);
        }
        if namespace != "System" || class != "Array" {
            return None;
        }
        let r = match name {
            "Copy" | "ConstrainedCopy" => self.array_copy(args),
            "Clear" => self.array_clear(args),
            "Reverse" => self.array_reverse(args),
            "Clone" => array_arg(args.first(), "this")
                .map(|a| Value::Ref(gc::with_gc(|gc| gc.clone_object(a)))),
            "CopyTo" => match args {
                [src, dst, index] => {
                    let n = array_arg(Some(src), "this").map(length);
                    match n {
                        Ok(n) => {
                            self.array_copy(&[*src, Value::I32(0), *dst, *index, Value::I64(n)])
                        }
                        Err(e) => Err(e),
                    }
                }
                _ => return None,
            },
            "CreateInstance" => self.array_create(args),
            "CreerTableau" => self.array_create_md(args),
            "get_Length" => array_arg(args.first(), "this").map(|a| Value::I32(length(a) as i32)),
            "get_LongLength" | "GetLongLength" => {
                array_arg(args.first(), "this").map(|a| Value::I64(length(a)))
            }
            // Array.GetValue / SetValue (#2008) : élément encadré ; un
            // indice par dimension, en entiers ou en tableau (#2848).
            "GetValue" | "SetValue" | "LireElement" | "EcrireElement" => {
                let set = matches!(name, "SetValue" | "EcrireElement");
                match array_arg(args.first(), "this") {
                    Err(e) => Err(e),
                    Ok(a) => {
                        let idx = args.get(if set { 2 } else { 1 }).copied();
                        let indices = match (name, idx) {
                            ("LireElement" | "EcrireElement", Some(Value::Ref(t)))
                                if !t.is_null() =>
                            {
                                (0..length(t))
                                    .map(|i| unsafe {
                                        *(t.add(DATA + 4 * i as usize) as *const i32)
                                    } as i64)
                                    .collect()
                            }
                            ("LireElement" | "EcrireElement", _) => {
                                return Some(Err(null_arg("indices")))
                            }
                            (_, i) => vec![i.map_or(-1, |v| v.as_i64())],
                        };
                        match flat_index(a, &indices) {
                            Err(e) => Err(e),
                            Ok(i) => {
                                let kind = self.elem_kind(a);
                                let size = kind.elem_size();
                                let at = unsafe { a.add(DATA + i * size) };
                                if set {
                                    let v = args.get(1).copied().unwrap_or(Value::Null);
                                    let raw = if kind == Kind::Ref {
                                        v
                                    } else {
                                        match self.unbox_elem(a, kind, v) {
                                            Ok(Some(raw)) => raw,
                                            Ok(None) => {
                                                unsafe { std::ptr::write_bytes(at, 0, size) };
                                                return Some(Ok(Value::Null));
                                            }
                                            Err(e) => return Some(Err(e)),
                                        }
                                    };
                                    store(at, kind, raw);
                                    Ok(Value::Null)
                                } else {
                                    let v = load(at, kind);
                                    Ok(self.box_elem(a, kind, v))
                                }
                            }
                        }
                    }
                }
            }
            "get_Rank" => {
                array_arg(args.first(), "this").map(|a| Value::I32(bounds_of(a).len() as i32))
            }
            "GetLength" | "GetLowerBound" | "GetUpperBound" => {
                match array_arg(args.first(), "this") {
                    Err(e) => Err(e),
                    Ok(a) => {
                        let b = bounds_of(a);
                        let d = args.get(1).map_or(-1, |v| v.as_i64());
                        match usize::try_from(d).ok().and_then(|d| b.get(d)) {
                            None => Err(ClrException::index_out_of_range()),
                            Some(&(lo, n)) => Ok(Value::I32(match name {
                                "GetLength" => n,
                                "GetLowerBound" => lo,
                                _ => lo.wrapping_add(n).wrapping_sub(1),
                            })),
                        }
                    }
                }
            }
            _ => return None,
        };
        Some(r)
    }

    /// Copy(src, dst, n) ou Copy(src, si, dst, di, n), entiers 32 ou 64 bits.
    /// Les zones peuvent se recouvrir (memmove). Entre deux types
    /// d'éléments différents, chaque élément est relu puis rangé.
    fn array_copy(&mut self, args: &[Value]) -> Result<Value, ClrException> {
        let (src, si, dst, di, n) = match args {
            [s, d, n] => (*s, 0, *d, 0, n.as_i64()),
            [s, si, d, di, n] => (*s, si.as_i64(), *d, di.as_i64(), n.as_i64()),
            _ => return Err(out_of_range("length")),
        };
        let src = array_arg(Some(&src), "sourceArray")?;
        let dst = array_arg(Some(&dst), "destinationArray")?;
        if n < 0 {
            return Err(out_of_range("length"));
        }
        if si < 0 || di < 0 {
            return Err(out_of_range(if si < 0 { "srcIndex" } else { "dstIndex" }));
        }
        if si + n > length(src) || di + n > length(dst) {
            return Err(ClrException::new(
                "System",
                "ArgumentException",
                "Destination array was not long enough. Check destIndex and length, and the array's lower bounds.",
            ));
        }
        let (sk, dk) = (self.elem_kind(src), self.elem_kind(dst));
        let (ss, ds) = (sk.elem_size(), dk.elem_size());
        // Structures : seulement vers le même type, sinon des références
        // tomberaient dans des entiers ou l'inverse (#2037).
        let structs = matches!(sk, Kind::Struct(_)) || matches!(dk, Kind::Struct(_));
        if structs && !(dk == Kind::Ref || gc::array_elem_type(src) == gc::array_elem_type(dst)) {
            return Err(ClrException::new(
                "System",
                "ArrayTypeMismatchException",
                "Source array type cannot be assigned to destination array type.",
            ));
        }
        unsafe {
            let from = src.add(DATA + si as usize * ss);
            let to = dst.add(DATA + di as usize * ds);
            if sk == dk {
                std::ptr::copy(from, to, n as usize * ss);
            } else {
                // Types différents (int[] vers long[], valeurs vers object[]) :
                // élément par élément, encadré si la destination le demande.
                let mut items = Vec::with_capacity(n as usize);
                for i in 0..n as usize {
                    items.push(load(from.add(i * ss), sk));
                }
                for (i, v) in items.into_iter().enumerate() {
                    let v = if dk == Kind::Ref && sk != Kind::Ref {
                        self.box_elem(src, sk, v)
                    } else if sk == Kind::Ref {
                        // object[] vers int[] : chaque objet est désencadré.
                        match self.unbox_elem(dst, dk, v)? {
                            Some(raw) => raw,
                            None => return Err(ClrException::null_reference()),
                        }
                    } else {
                        v
                    };
                    store(to.add(i * ds), dk, v);
                }
            }
        }
        Ok(Value::Null)
    }

    /// Clear(tableau, index, nombre) ; Clear(tableau) (.NET 6) efface tout.
    fn array_clear(&mut self, args: &[Value]) -> Result<Value, ClrException> {
        let arr = array_arg(args.first(), "array")?;
        let (index, n) = match args {
            [_, i, n] => (i.as_i64(), n.as_i64()),
            _ => (0, length(arr)),
        };
        if index < 0 || n < 0 || index + n > length(arr) {
            return Err(out_of_range("length"));
        }
        let size = self.elem_kind(arr).elem_size();
        unsafe {
            std::ptr::write_bytes(arr.add(DATA + index as usize * size), 0, n as usize * size)
        };
        Ok(Value::Null)
    }

    fn array_reverse(&mut self, args: &[Value]) -> Result<Value, ClrException> {
        let arr = array_arg(args.first(), "array")?;
        let (index, n) = match args {
            [_, i, n] => (i.as_i64(), n.as_i64()),
            _ => (0, length(arr)),
        };
        if index < 0 || n < 0 || index + n > length(arr) {
            return Err(out_of_range("length"));
        }
        let size = self.elem_kind(arr).elem_size();
        let base = unsafe { arr.add(DATA + index as usize * size) };
        let n = n as usize;
        for i in 0..n / 2 {
            unsafe {
                std::ptr::swap_nonoverlapping(
                    base.add(i * size),
                    base.add((n - 1 - i) * size),
                    size,
                );
            }
        }
        Ok(Value::Null)
    }

    /// CreateInstance(Type, longueur).
    fn array_create(&mut self, args: &[Value]) -> Result<Value, ClrException> {
        let (ty, n) = match args {
            [t, n] => (*t, n.as_i64()),
            _ => return Err(ClrException::new(
                "System",
                "NotSupportedException",
                "Array.CreateInstance : seuls les tableaux à une dimension sont pris en charge.",
            )),
        };
        let Value::Ref(t) = ty else {
            return Err(null_arg("elementType"));
        };
        let Some(elem) = super::reflection::type_id_of(t) else {
            return Err(ClrException::new(
                "System",
                "ArgumentException",
                "Type must be a type provided by the runtime.\r\nParameter name: elementType",
            ));
        };
        if n < 0 {
            return Err(out_of_range("length"));
        }
        let kind = self.kind_of_type(elem);
        let elem = if kind == Kind::Ref { 0 } else { elem };
        Ok(Value::Ref(gc::with_gc(|gc| {
            gc.alloc_array_of(elem, kind.elem_size(), n as usize)
        })))
    }

    /// CreerTableau(Type, longueurs, bornes inférieures ou null) : tableau
    /// à plusieurs dimensions (Array.CreateInstance, #2848).
    fn array_create_md(&mut self, args: &[Value]) -> Result<Value, ClrException> {
        let [Value::Ref(t), lens, lows] = args else {
            return Err(null_arg("elementType"));
        };
        if t.is_null() {
            return Err(null_arg("elementType"));
        }
        let Some(elem) = super::reflection::type_id_of(*t) else {
            return Err(ClrException::new(
                "System",
                "ArgumentException",
                "Type must be a type provided by the runtime.\r\nParameter name: elementType",
            ));
        };
        let ints = |v: &Value| -> Option<Vec<i32>> {
            let p = v.as_ref();
            (!p.is_null()).then(|| {
                (0..length(p) as usize)
                    .map(|i| unsafe { *(p.add(DATA + 4 * i) as *const i32) })
                    .collect()
            })
        };
        let Some(lens) = ints(lens) else {
            return Err(null_arg("lengths"));
        };
        let lows = ints(lows).unwrap_or_else(|| vec![0; lens.len()]);
        if lens.is_empty() || lens.len() > gc::MAX_RANK {
            return Err(ClrException::new(
                "System",
                "ArgumentException",
                "Must provide at least one rank.\r\nParameter name: lengths",
            ));
        }
        if lows.len() != lens.len() {
            return Err(ClrException::new(
                "System",
                "ArgumentException",
                "The arrays must have the same number of dimensions.\r\nParameter name: lowerBounds",
            ));
        }
        if lens.iter().any(|&n| n < 0) {
            return Err(out_of_range("lengths"));
        }
        let kind = self.kind_of_type(elem);
        let elem = if kind == Kind::Ref { 0 } else { elem };
        let bounds: Vec<(i32, i32)> = lows.into_iter().zip(lens).collect();
        // Une dimension indexée depuis 0 reste un tableau ordinaire.
        if bounds.len() == 1 && bounds[0].0 == 0 {
            return Ok(Value::Ref(gc::with_gc(|gc| {
                gc.alloc_array_of(elem, kind.elem_size(), bounds[0].1 as usize)
            })));
        }
        Ok(Value::Ref(alloc_md(elem, kind, &bounds)?))
    }

    /// Méthodes que le moteur fournit à un type tableau à plusieurs
    /// dimensions (ECMA-335 §II.14.2) : constructeurs, Get, Set et Address,
    /// appelés par un MemberRef dont le parent est une TypeSpec ARRAY.
    /// `None` si `token` ne désigne pas une telle méthode.
    pub(super) fn md_array_method(&self, token: u32) -> Option<(TypeId, Kind, u32, String)> {
        let tok = Token(token);
        if tok.table() != TableId::MemberRef as u8 {
            return None;
        }
        let mr = self.assembly.get_member_ref(tok.rid());
        let (parent, prow) = CodedIndex::MemberRefParent.decode(mr.class);
        if parent != Some(TableId::TypeSpec) {
            return None;
        }
        let blob = self
            .assembly
            .get_blob(self.assembly.get_type_spec(prow).signature);
        let mut off = 0;
        let ElementType::Array(inner, rank) = metadata::decode_type(blob, &mut off) else {
            return None;
        };
        let elem = self.resolve_sig(self.current, &inner, &self.current_gen());
        let kind = self.kind_of_type(elem);
        let elem = if kind == Kind::Ref { 0 } else { elem };
        let name = self.assembly.get_string(mr.name).to_string();
        Some((elem, kind, rank, name))
    }

    /// `newobj T[,]::.ctor` : `rank` longueurs, ou `2 × rank` couples
    /// (borne inférieure, longueur).
    pub(super) fn md_array_new(
        &mut self,
        elem: TypeId,
        kind: Kind,
        rank: u32,
        args: &[Value],
    ) -> Result<Value, ClrException> {
        let rank = rank as usize;
        let bounds: Vec<(i32, i32)> = if args.len() == rank {
            args.iter().map(|v| (0, v.as_i64() as i32)).collect()
        } else if args.len() == 2 * rank {
            args.chunks(2)
                .map(|c| (c[0].as_i64() as i32, c[1].as_i64() as i32))
                .collect()
        } else {
            return Err(ClrException::new(
                "System",
                "MissingMethodException",
                "Constructeur de tableau à plusieurs dimensions inconnu.",
            ));
        };
        if bounds.iter().any(|&(_, n)| n < 0) {
            return Err(ClrException::new(
                "System",
                "OverflowException",
                "Arithmetic operation resulted in an overflow.",
            ));
        }
        Ok(Value::Ref(alloc_md(elem, kind, &bounds)?))
    }

    /// `T[,]::Get`, `Set` et `Address` : `this`, un indice par dimension,
    /// puis la valeur pour Set.
    pub(super) fn md_array_access(
        &mut self,
        kind: Kind,
        name: &str,
        args: &[Value],
    ) -> Result<Value, ClrException> {
        let arr = match args.first() {
            Some(Value::Ref(p)) if !p.is_null() => *p,
            _ => return Err(ClrException::null_reference()),
        };
        let n = args.len() - 1 - (name == "Set") as usize;
        let indices: Vec<i64> = args[1..1 + n].iter().map(|v| v.as_i64()).collect();
        let i = flat_index(arr, &indices)?;
        let at = unsafe { arr.add(DATA + i * kind.elem_size()) };
        match name {
            "Get" => Ok(load(at, kind)),
            "Set" => {
                store(at, kind, args[args.len() - 1]);
                Ok(Value::Null)
            }
            "Address" => Ok(Value::Ref(at)),
            _ => Err(ClrException::new(
                "System",
                "MissingMethodException",
                &format!("Méthode de tableau inconnue : {name}"),
            )),
        }
    }
}

/// Tableau à plusieurs dimensions ; OutOfMemoryException au-delà de ce
/// que la mémoire peut tenir.
fn alloc_md(elem: TypeId, kind: Kind, bounds: &[(i32, i32)]) -> Result<*mut u8, ClrException> {
    let total = bounds
        .iter()
        .try_fold(1usize, |acc, &(_, n)| acc.checked_mul(n as usize))
        .and_then(|t| t.checked_mul(kind.elem_size()))
        .filter(|&b| b < u32::MAX as usize / 2);
    if total.is_none() {
        return Err(ClrException::new(
            "System",
            "OutOfMemoryException",
            "Array dimensions exceeded supported range.",
        ));
    }
    Ok(gc::with_gc(|gc| {
        gc.alloc_array_md(elem, kind.elem_size(), bounds)
    }))
}
