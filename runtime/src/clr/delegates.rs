//! Délégués (ECMA-335 §II.14.6) : `ldftn`, `ldvirtftn`, construction,
//! `Invoke`, délégués multicast (`Delegate.Combine` / `Remove`) et
//! `Interlocked`, dont se servent les accesseurs d'événements générés par
//! les compilateurs.
//!
//! Un délégué est un objet ordinaire du tas, marqué comme tel par le
//! ramasse-miettes, dont trois mots suivent le mot réservé des classes :
//! la cible (`this` de la méthode, ou null), le pointeur de méthode rendu
//! par `ldftn`, et la liste d'invocation (tableau de délégués simples, ou
//! null pour un délégué simple).

use super::exceptions::ClrException;
use super::gc;
use super::il::{Interpreter, Value};
use super::metadata::ElementType;
use super::resolve::{Callee, Target};
use super::types::{self, Kind};

const TARGET: usize = 8;
const METHOD: usize = 16;
const LIST: usize = 24;

/// Méthode désignée par un pointeur de méthode (`ldftn`).
#[derive(Clone)]
pub struct FnHandle {
    /// Assembly et jeton d'origine, pour la signature d'une méthode de la
    /// bibliothèque intégrée.
    asm: usize,
    token: u32,
    callee: Callee,
}

#[derive(Default)]
pub struct FnTable {
    handles: Vec<FnHandle>,
    index: std::collections::HashMap<String, usize>,
}

fn read_ref(obj: *mut u8, offset: usize) -> *mut u8 {
    unsafe { *(obj.add(offset) as *const *mut u8) }
}

fn write_ref(obj: *mut u8, offset: usize, v: *mut u8) {
    unsafe { *(obj.add(offset) as *mut *mut u8) = v }
}

fn array_len(arr: *mut u8) -> usize {
    unsafe { *(arr as *const u64) as usize }
}

fn array_get(arr: *mut u8, i: usize) -> *mut u8 {
    read_ref(arr, 8 + i * 8)
}

impl Interpreter {
    /// Pointeur de méthode (valeur non nulle) pour `callee`, désigné par
    /// `token` dans l'assembly courante. Une même méthode donne toujours le
    /// même pointeur, ce qui permet de comparer deux délégués.
    pub(super) fn fn_handle(&mut self, token: u32, callee: Callee) -> i64 {
        let key = match &callee.target {
            Target::Managed(..) => {
                format!("{:?}:{:?}:{:?}", callee.target, callee.gen.t, callee.gen.m)
            }
            Target::External { .. } => format!("{}:{token}:{:?}", self.current, callee.gen.m),
        };
        let current = self.current;
        let mut fns = self.shared.fns.lock().unwrap();
        if let Some(&i) = fns.index.get(&key) {
            return i as i64 + 1;
        }
        let i = fns.handles.len();
        fns.handles.push(FnHandle {
            asm: current,
            token,
            callee,
        });
        fns.index.insert(key, i);
        i as i64 + 1
    }

    /// Appelle la méthode d'un pointeur de méthode.
    fn call_handle(&mut self, handle: i64, args: Vec<Value>) -> Result<Value, ClrException> {
        let Some(fh) = usize::try_from(handle - 1)
            .ok()
            .and_then(|i| self.shared.fns.lock().unwrap().handles.get(i).cloned())
        else {
            return Err(ClrException::new(
                "System",
                "ExecutionEngineException",
                "pointeur de méthode invalide",
            ));
        };
        let previous = self.enter(fh.asm);
        let result = self.invoke(fh.token, fh.callee, args, false);
        self.enter(previous);
        result
    }

    /// Méthodes fournies par le runtime pour les délégués et l'accès
    /// atomique. `None` : l'appel ne relève pas de ce module.
    pub(super) fn delegate_call(
        &mut self,
        namespace: &str,
        class: &str,
        name: &str,
        args: &[Value],
        params: &[ElementType],
    ) -> Option<Result<Value, ClrException>> {
        let this = match args.first() {
            Some(Value::Ref(p)) => *p,
            _ => std::ptr::null_mut(),
        };
        let delegate_arg = |i: usize| match args.get(i) {
            Some(Value::Ref(p)) if gc::is_delegate(*p) => Some(*p),
            _ => None,
        };
        match (namespace, class, name) {
            // Constructeur d'un délégué : .ctor(object, native int).
            (_, _, ".ctor")
                if args.len() == 3
                    && matches!(params, [ElementType::Object, ElementType::IntPtr]) =>
            {
                if this.is_null() {
                    return Some(Err(ClrException::null_reference()));
                }
                write_ref(this, TARGET, args[1].as_ref());
                unsafe { *(this.add(METHOD) as *mut i64) = args[2].as_i64() };
                write_ref(this, LIST, std::ptr::null_mut());
                gc::set_delegate(this);
                Some(Ok(Value::Null))
            }
            (_, _, "Invoke") if gc::is_delegate(this) => {
                Some(self.invoke_delegate(this, args[1..].to_vec()))
            }
            ("System", "Delegate", "Combine") => Some(Ok(match args {
                [a, b] => self.combine(a.as_ref(), b.as_ref()),
                [Value::Ref(arr)] => {
                    let mut acc = std::ptr::null_mut();
                    for i in 0..array_len(*arr) {
                        acc = self.combine(acc, array_get(*arr, i)).as_ref();
                    }
                    ref_value(acc)
                }
                _ => Value::Null,
            })),
            ("System", "Delegate", "Remove") => {
                Some(Ok(self.remove(args[0].as_ref(), args[1].as_ref(), false)))
            }
            ("System", "Delegate", "RemoveAll") => {
                Some(Ok(self.remove(args[0].as_ref(), args[1].as_ref(), true)))
            }
            ("System", "Delegate" | "MulticastDelegate", "op_Equality" | "op_Inequality") => {
                let eq = delegates_equal(args[0].as_ref(), args[1].as_ref());
                Some(Ok(Value::I32((eq == (name == "op_Equality")) as i32)))
            }
            (_, _, "Equals") if gc::is_delegate(this) && args.len() == 2 => Some(Ok(Value::I32(
                (delegate_arg(1).is_some() && delegates_equal(this, args[1].as_ref())) as i32,
            ))),
            (_, _, "GetHashCode") if gc::is_delegate(this) => {
                let h = unsafe { *(this.add(METHOD) as *const i64) };
                Some(Ok(Value::I32(h as i32)))
            }
            (_, _, "get_Target") if gc::is_delegate(this) => {
                let list = read_ref(this, LIST);
                let single = if list.is_null() {
                    this
                } else {
                    array_get(list, array_len(list) - 1)
                };
                Some(Ok(ref_value(read_ref(single, TARGET))))
            }
            (_, _, "GetInvocationList") if gc::is_delegate(this) => {
                let items = invocation_list(this);
                let id = gc::type_of(this).unwrap_or(types::TYPE_OBJECT);
                let arr = gc::with_gc(|gc| gc.alloc_array_of(id, 8, items.len()));
                for (i, d) in items.into_iter().enumerate() {
                    write_ref(arr, 8 + i * 8, d);
                }
                Some(Ok(Value::Ref(arr)))
            }
            ("System.Threading", "Interlocked", _) => interlocked(name, args, params),
            _ => None,
        }
    }

    /// `Invoke` : appelle chaque délégué de la liste dans l'ordre et rend
    /// le résultat du dernier.
    pub(super) fn invoke_delegate(
        &mut self,
        d: *mut u8,
        rest: Vec<Value>,
    ) -> Result<Value, ClrException> {
        let pinned = self.pins.len();
        self.pins.push(Value::Ref(d));
        self.pins.extend(rest.iter().copied());
        let mut result = Ok(Value::Null);
        for single in invocation_list(d) {
            let target = read_ref(single, TARGET);
            let handle = unsafe { *(single.add(METHOD) as *const i64) };
            // Une cible non nulle devient le premier argument : `this`
            // d'une méthode d'instance, ou premier paramètre d'une méthode
            // statique fermée sur lui (méthodes d'extension).
            let mut args = Vec::with_capacity(rest.len() + 1);
            if !target.is_null() {
                args.push(Value::Ref(target));
            }
            args.extend(rest.iter().copied());
            result = self.call_handle(handle, args);
            if result.is_err() {
                break;
            }
        }
        self.pins.truncate(pinned);
        result
    }

    /// Nouveau délégué du type de `like` dont la liste est `items`.
    fn make_multicast(&mut self, like: *mut u8, items: &[*mut u8]) -> Value {
        match items {
            [] => Value::Null,
            [one] => Value::Ref(*one),
            _ => {
                let id = gc::type_of(like).unwrap_or(types::TYPE_OBJECT);
                let (copy, arr) = gc::with_gc(|gc| {
                    let copy = gc.clone_object(like);
                    let arr = gc.alloc_array_of(id, 8, items.len());
                    (copy, arr)
                });
                for (i, d) in items.iter().enumerate() {
                    write_ref(arr, 8 + i * 8, *d);
                }
                let last = items[items.len() - 1];
                write_ref(copy, TARGET, read_ref(last, TARGET));
                unsafe { *(copy.add(METHOD) as *mut i64) = *(last.add(METHOD) as *const i64) };
                write_ref(copy, LIST, arr);
                Value::Ref(copy)
            }
        }
    }

    fn combine(&mut self, a: *mut u8, b: *mut u8) -> Value {
        if a.is_null() {
            return ref_value(b);
        }
        if b.is_null() {
            return Value::Ref(a);
        }
        let mut items = invocation_list(a);
        items.extend(invocation_list(b));
        self.make_multicast(a, &items)
    }

    /// Retire de `source` la dernière occurrence de la liste de `value`
    /// (toutes les occurrences avec `all`).
    fn remove(&mut self, source: *mut u8, value: *mut u8, all: bool) -> Value {
        if source.is_null() || value.is_null() {
            return ref_value(source);
        }
        let mut items = invocation_list(source);
        let needle = invocation_list(value);
        let mut changed = false;
        while items.len() >= needle.len() {
            let found = (0..=items.len() - needle.len()).rev().find(|&start| {
                needle
                    .iter()
                    .zip(&items[start..])
                    .all(|(x, y)| delegates_equal(*x, *y))
            });
            let Some(start) = found else { break };
            items.drain(start..start + needle.len());
            changed = true;
            if !all {
                break;
            }
        }
        if !changed {
            return Value::Ref(source);
        }
        self.make_multicast(source, &items)
    }
}

fn ref_value(p: *mut u8) -> Value {
    if p.is_null() {
        Value::Null
    } else {
        Value::Ref(p)
    }
}

/// Délégués simples qui composent `d`.
fn invocation_list(d: *mut u8) -> Vec<*mut u8> {
    let list = read_ref(d, LIST);
    if list.is_null() {
        vec![d]
    } else {
        (0..array_len(list)).map(|i| array_get(list, i)).collect()
    }
}

/// Égalité de délégués : même type, même suite de cibles et de méthodes.
fn delegates_equal(a: *mut u8, b: *mut u8) -> bool {
    if a.is_null() || b.is_null() {
        return a == b;
    }
    if gc::type_of(a) != gc::type_of(b) {
        return false;
    }
    let (la, lb) = (invocation_list(a), invocation_list(b));
    la.len() == lb.len()
        && la.iter().zip(&lb).all(|(x, y)| unsafe {
            read_ref(*x, TARGET) == read_ref(*y, TARGET)
                && *(x.add(METHOD) as *const i64) == *(y.add(METHOD) as *const i64)
        })
}

/// Opérations de `System.Threading.Interlocked`. Le moteur n'exécute
/// encore qu'un fil : chaque opération est faite d'un seul tenant.
fn interlocked(
    name: &str,
    args: &[Value],
    params: &[ElementType],
) -> Option<Result<Value, ClrException>> {
    let ptr = args.first()?.as_ref();
    if ptr.is_null() {
        return Some(Err(ClrException::null_reference()));
    }
    let kind = match params.first() {
        Some(ElementType::ByRef(inner)) => match **inner {
            ElementType::I4 | ElementType::U4 => Kind::I4,
            ElementType::I8 | ElementType::U8 => Kind::I8,
            ElementType::R4 => Kind::R4,
            ElementType::R8 => Kind::R8,
            ElementType::IntPtr | ElementType::UIntPtr => Kind::I,
            _ => Kind::Ref,
        },
        _ => return None,
    };
    let old = super::il::load(ptr, kind);
    let add = |old: Value, delta: i64| match kind {
        Kind::I4 => Value::I32((old.as_i32()).wrapping_add(delta as i32)),
        _ => Value::I64(old.as_i64().wrapping_add(delta)),
    };
    let same = |a: Value, b: Value| match kind {
        Kind::R4 | Kind::R8 => a.as_f64() == b.as_f64(),
        Kind::Ref => a.as_ref() == b.as_ref(),
        _ => a.as_i64() == b.as_i64(),
    };
    let (new, ret) = match name {
        "CompareExchange" => {
            let (value, comparand) = (*args.get(1)?, *args.get(2)?);
            (same(old, comparand).then_some(value), old)
        }
        "Exchange" => (Some(*args.get(1)?), old),
        "Increment" => {
            let v = add(old, 1);
            (Some(v), v)
        }
        "Decrement" => {
            let v = add(old, -1);
            (Some(v), v)
        }
        "Add" => {
            let v = add(old, args.get(1)?.as_i64());
            (Some(v), v)
        }
        "Read" => (None, old),
        _ => return None,
    };
    if let Some(v) = new {
        super::il::store(ptr, kind, v);
    }
    Some(Ok(ret))
}
