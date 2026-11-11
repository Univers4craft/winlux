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

impl Interpreter {
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
        let asm = self.assembly_at(index - 1);
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
                        let id = gc::array_elem_type(src);
                        let boxed = if matches!(sk, Kind::Struct(_)) {
                            self.new_struct(id)
                        } else {
                            gc::with_gc(|gc| gc.alloc(id, 8))
                        };
                        store(boxed, sk, v);
                        Value::Ref(boxed)
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
