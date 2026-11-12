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
use super::tables::{TableId, Token};
use super::types::Kind;

/// Données d'un tableau à une dimension : longueur (u64) puis éléments.
const DATA: usize = 8;

fn length(arr: *mut u8) -> i64 {
    unsafe { *(arr as *const u64) as i64 }
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
                    store(at(i), kind, args.get(2).copied().unwrap_or(Value::Null));
                    Ok(Value::Null)
                }
            }
            "Contains" | "IndexOf" => {
                let item = args.get(1).copied().unwrap_or(Value::Null);
                // IList.Contains(object) sur un tableau de valeurs : la
                // valeur arrive encadrée.
                let item = if !generic && kind != Kind::Ref {
                    match item {
                        Value::Ref(p) if !p.is_null() => load(p, kind),
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
            "GetLength" | "get_Length" => {
                array_arg(args.first(), "this").map(|a| Value::I32(length(a) as i32))
            }
            "get_LongLength" | "GetLongLength" => {
                array_arg(args.first(), "this").map(|a| Value::I64(length(a)))
            }
            // Array.GetValue / SetValue (#2008) : élément encadré.
            "GetValue" | "SetValue" => {
                let set = name == "SetValue";
                match array_arg(args.first(), "this") {
                    Err(e) => Err(e),
                    Ok(a) => {
                        let i = args.get(if set { 2 } else { 1 }).map_or(-1, |v| v.as_i64());
                        if i < 0 || i >= length(a) {
                            Err(out_of_range("index"))
                        } else {
                            let kind = self.elem_kind(a);
                            let size = kind.elem_size();
                            let at = unsafe { a.add(8 + i as usize * size) };
                            if set {
                                let v = args.get(1).copied().unwrap_or(Value::Null);
                                let raw = if kind == Kind::Ref {
                                    v
                                } else {
                                    match v {
                                        Value::Ref(p) if !p.is_null() => load(p, kind),
                                        _ => {
                                            unsafe { std::ptr::write_bytes(at, 0, size) };
                                            return Some(Ok(Value::Null));
                                        }
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
            "get_Rank" => Ok(Value::I32(1)),
            "GetLowerBound" => Ok(Value::I32(0)),
            "GetUpperBound" => {
                array_arg(args.first(), "this").map(|a| Value::I32(length(a) as i32 - 1))
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
}
