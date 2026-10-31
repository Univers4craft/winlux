//! Bibliothèque de classes de base (BCL) minimale pour le runtime CLR Winlux.

use super::gc;
use super::il::Value;
use super::metadata::ElementType;
use super::types::{self, TYPE_STRING};

/// Appelle une méthode de la BCL intégrée. `params` donne les types déclarés
/// des arguments (hors `this`) pour afficher booléens et caractères comme .NET.
/// `None` signifie que la méthode n'existe pas dans la BCL Winlux.
pub fn dispatch_internal_call(
    namespace: &str,
    class: &str,
    method: &str,
    args: &[Value],
    params: &[ElementType],
) -> Option<Value> {
    match (namespace, class, method) {
        ("System", "Console", m) => console_dispatch(m, args, params),
        ("System", "Environment", m) => environment_dispatch(m, args),
        ("System", "Math", m) => math_dispatch(m, args),
        ("System", "String", m) => string_dispatch(m, args),
        ("System", "Object", m) => object_dispatch(m, args),
        ("System", "Array", m) => array_dispatch(m, args),
        ("System", "GC", m) => gc_dispatch(m, args),
        ("System", "Convert", m) => convert_dispatch(m, args),
        ("System", "Type", m) => type_dispatch(m, args),
        ("System", "Activator", m) => activator_dispatch(m, args),
        ("System.IO", "File", m) => file_dispatch(m, args),
        ("System.IO", "Path", m) => path_dispatch(m, args),
        ("System.Runtime.InteropServices", "Marshal", m) => marshal_dispatch(m, args),
        _ => {
            crate::log_warn!(
                "clr-bcl",
                "appel interne inconnu: {}.{}::{}",
                namespace,
                class,
                method
            );
            None
        }
    }
}

/// Chaîne managée : longueur (u64) suivie des unités UTF-16.
pub fn alloc_managed_string(s: &str) -> Value {
    let utf16: Vec<u16> = s.encode_utf16().collect();
    let ptr = gc::with_gc(|gc| gc.alloc(TYPE_STRING, 8 + utf16.len() * 2));
    unsafe {
        *(ptr as *mut u64) = utf16.len() as u64;
        std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr.add(8) as *mut u16, utf16.len());
    }
    Value::Ref(ptr)
}

pub fn read_managed_string(ptr: *mut u8) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe {
        let len = *(ptr as *const u64) as usize;
        let slice = std::slice::from_raw_parts(ptr.add(8) as *const u16, len);
        String::from_utf16_lossy(slice)
    }
}

pub fn alloc_string_array(items: &[String]) -> Value {
    let arr = gc::with_gc(|gc| gc.alloc_array(types::TYPE_ARRAY, 8, items.len()));
    for (i, s) in items.iter().enumerate() {
        let r = alloc_managed_string(s).as_ref();
        unsafe { *(arr.add(8 + i * 8) as *mut *mut u8) = r };
    }
    Value::Ref(arr)
}

/// Lit la valeur d'un objet boxé selon son type.
pub fn unbox(ptr: *mut u8, type_id: types::TypeId) -> Value {
    unsafe {
        match type_id {
            types::TYPE_INT64 | types::TYPE_UINT64 | types::TYPE_INTPTR => {
                Value::I64(*(ptr as *const i64))
            }
            types::TYPE_FLOAT => Value::F32(*(ptr as *const f32)),
            types::TYPE_DOUBLE => Value::F64(*(ptr as *const f64)),
            _ => Value::I32(*(ptr as *const i32)),
        }
    }
}

fn to_string_typed(v: &Value, et: Option<&ElementType>) -> String {
    match (v, et) {
        (Value::I32(n), Some(ElementType::Boolean)) => {
            if *n != 0 { "True" } else { "False" }.to_string()
        }
        (Value::I32(n), Some(ElementType::Char)) => {
            char::from_u32(*n as u32).unwrap_or('\u{fffd}').to_string()
        }
        (Value::I32(n), Some(ElementType::U4)) => (*n as u32).to_string(),
        (Value::I64(n), Some(ElementType::U8)) => (*n as u64).to_string(),
        _ => value_to_string(v),
    }
}

pub fn value_to_string(v: &Value) -> String {
    match v {
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::F32(f) => f.to_string(),
        Value::F64(f) => f.to_string(),
        Value::Null => String::new(),
        Value::Ref(ptr) => match gc::type_of(*ptr) {
            Some(TYPE_STRING) => read_managed_string(*ptr),
            Some(types::TYPE_BOOLEAN) => to_string_typed(
                &Value::I32(unsafe { *(*ptr as *const i32) }),
                Some(&ElementType::Boolean),
            ),
            Some(types::TYPE_CHAR) => to_string_typed(
                &Value::I32(unsafe { *(*ptr as *const i32) }),
                Some(&ElementType::Char),
            ),
            Some(id) if is_boxed_primitive(id) => value_to_string(&unbox(*ptr, id)),
            Some(id) => types::with_types(|reg| {
                reg.get(id)
                    .map(|t| t.full_name())
                    .unwrap_or_else(|| "System.Object".into())
            }),
            None => String::new(),
        },
    }
}

fn is_boxed_primitive(id: types::TypeId) -> bool {
    (types::TYPE_INT32..=types::TYPE_INTPTR).contains(&id)
}

/// Remplace `{0}`, `{1}`… comme `String.Format` (sans spécificateurs de format).
fn format_composite(fmt: &str, args: &[Value]) -> String {
    let mut out = fmt.to_string();
    for (i, a) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), &value_to_string(a));
    }
    out
}

/// Texte écrit par Console.Write/WriteLine pour une surcharge donnée.
fn console_text(args: &[Value], params: &[ElementType]) -> String {
    match args {
        [] => String::new(),
        [single] => to_string_typed(single, params.first()),
        [fmt, rest @ ..] => format_composite(&value_to_string(fmt), rest),
    }
}

// ── System.Console ──

fn console_dispatch(method: &str, args: &[Value], params: &[ElementType]) -> Option<Value> {
    match method {
        "WriteLine" => {
            println!("{}", console_text(args, params));
            Some(Value::I32(0))
        }
        "Write" => {
            print!("{}", console_text(args, params));
            Some(Value::I32(0))
        }
        "ReadLine" => {
            let mut buf = String::new();
            let _ = std::io::stdin().read_line(&mut buf);
            let trimmed = buf.trim_end_matches('\n').trim_end_matches('\r');
            Some(alloc_managed_string(trimmed))
        }
        "ReadKey" => {
            let mut buf = [0u8; 1];
            let _ = std::io::Read::read(&mut std::io::stdin(), &mut buf);
            Some(Value::I32(buf[0] as i32))
        }
        "get_Out" | "get_Error" | "get_In" => Some(Value::Null),
        "SetOut" | "SetError" | "SetIn" => Some(Value::I32(0)),
        "Beep" => Some(Value::I32(0)),
        _ => {
            crate::log_debug!("clr-bcl", "Console.{} non implémenté", method);
            Some(Value::I32(0))
        }
    }
}

// ── System.Environment ──

fn environment_dispatch(method: &str, args: &[Value]) -> Option<Value> {
    match method {
        "Exit" => {
            let code = match args.last() {
                Some(Value::I32(c)) => *c,
                _ => 0,
            };
            crate::dlls::kernel32::flush_and_exit(code)
        }
        "get_ExitCode" => Some(Value::I32(0)),
        "set_ExitCode" => Some(Value::I32(0)),
        "get_CurrentDirectory" => {
            let cwd = std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            Some(alloc_managed_string(&cwd))
        }
        "GetEnvironmentVariable" => {
            let name = match args.last() {
                Some(Value::Ref(p)) if !p.is_null() => read_managed_string(*p),
                _ => return Some(Value::Null),
            };
            match std::env::var(&name) {
                Ok(val) => Some(alloc_managed_string(&val)),
                Err(_) => Some(Value::Null),
            }
        }
        "get_OSVersion" => Some(alloc_managed_string("Winlux 1.0")),
        "get_MachineName" => {
            let name = std::env::var("HOSTNAME").unwrap_or_else(|_| "winlux".into());
            Some(alloc_managed_string(&name))
        }
        "get_UserName" => {
            let name = std::env::var("USER").unwrap_or_else(|_| "user".into());
            Some(alloc_managed_string(&name))
        }
        "get_NewLine" => Some(alloc_managed_string("\r\n")),
        "get_TickCount" | "get_TickCount64" => {
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            if method == "get_TickCount" {
                Some(Value::I32(t as i32))
            } else {
                Some(Value::I64(t))
            }
        }
        "get_ProcessorCount" => Some(Value::I32(1)),
        "get_Is64BitProcess" => Some(Value::I32(1)),
        "get_Is64BitOperatingSystem" => Some(Value::I32(1)),
        "GetCommandLineArgs" => Some(Value::Null),
        _ => {
            crate::log_debug!("clr-bcl", "Environment.{} non implémenté", method);
            Some(Value::I32(0))
        }
    }
}

// ── System.Math ──

fn math_dispatch(method: &str, args: &[Value]) -> Option<Value> {
    let to_f64 = |v: &Value| -> f64 {
        match v {
            Value::F64(f) => *f,
            Value::F32(f) => *f as f64,
            Value::I32(i) => *i as f64,
            Value::I64(i) => *i as f64,
            _ => 0.0,
        }
    };

    match method {
        "Abs" => match args.last()? {
            Value::I32(v) => Some(Value::I32(v.wrapping_abs())),
            Value::I64(v) => Some(Value::I64(v.wrapping_abs())),
            Value::F64(v) => Some(Value::F64(v.abs())),
            Value::F32(v) => Some(Value::F32(v.abs())),
            _ => Some(Value::I32(0)),
        },
        "Sqrt" => Some(Value::F64(to_f64(args.last()?).sqrt())),
        "Sin" => Some(Value::F64(to_f64(args.last()?).sin())),
        "Cos" => Some(Value::F64(to_f64(args.last()?).cos())),
        "Tan" => Some(Value::F64(to_f64(args.last()?).tan())),
        "Asin" => Some(Value::F64(to_f64(args.last()?).asin())),
        "Acos" => Some(Value::F64(to_f64(args.last()?).acos())),
        "Atan" => Some(Value::F64(to_f64(args.last()?).atan())),
        "Atan2" => {
            if args.len() >= 2 {
                Some(Value::F64(
                    to_f64(&args[args.len() - 2]).atan2(to_f64(&args[args.len() - 1])),
                ))
            } else {
                Some(Value::F64(0.0))
            }
        }
        "Pow" => {
            if args.len() >= 2 {
                Some(Value::F64(
                    to_f64(&args[args.len() - 2]).powf(to_f64(&args[args.len() - 1])),
                ))
            } else {
                Some(Value::F64(0.0))
            }
        }
        "Log" => Some(Value::F64(to_f64(args.last()?).ln())),
        "Log10" => Some(Value::F64(to_f64(args.last()?).log10())),
        "Log2" => Some(Value::F64(to_f64(args.last()?).log2())),
        "Exp" => Some(Value::F64(to_f64(args.last()?).exp())),
        "Floor" => Some(Value::F64(to_f64(args.last()?).floor())),
        "Ceiling" => Some(Value::F64(to_f64(args.last()?).ceil())),
        "Round" => Some(Value::F64(to_f64(args.last()?).round())),
        "Truncate" => Some(Value::F64(to_f64(args.last()?).trunc())),
        "Min" => {
            if args.len() >= 2 {
                match (&args[args.len() - 2], &args[args.len() - 1]) {
                    (Value::I32(a), Value::I32(b)) => Some(Value::I32(*a.min(b))),
                    (Value::I64(a), Value::I64(b)) => Some(Value::I64(*a.min(b))),
                    _ => Some(Value::F64(
                        to_f64(&args[args.len() - 2]).min(to_f64(&args[args.len() - 1])),
                    )),
                }
            } else {
                Some(Value::I32(0))
            }
        }
        "Max" => {
            if args.len() >= 2 {
                match (&args[args.len() - 2], &args[args.len() - 1]) {
                    (Value::I32(a), Value::I32(b)) => Some(Value::I32(*a.max(b))),
                    (Value::I64(a), Value::I64(b)) => Some(Value::I64(*a.max(b))),
                    _ => Some(Value::F64(
                        to_f64(&args[args.len() - 2]).max(to_f64(&args[args.len() - 1])),
                    )),
                }
            } else {
                Some(Value::I32(0))
            }
        }
        "Sign" => match args.last()? {
            Value::I32(v) => Some(Value::I32(v.signum())),
            Value::I64(v) => Some(Value::I32(v.signum() as i32)),
            Value::F64(v) => Some(Value::I32(if *v > 0.0 {
                1
            } else if *v < 0.0 {
                -1
            } else {
                0
            })),
            _ => Some(Value::I32(0)),
        },
        "Clamp" => {
            if args.len() >= 3 {
                let val = to_f64(&args[args.len() - 3]);
                let min = to_f64(&args[args.len() - 2]);
                let max = to_f64(&args[args.len() - 1]);
                Some(Value::F64(val.clamp(min, max)))
            } else {
                Some(Value::F64(0.0))
            }
        }
        "get_PI" | "PI" => Some(Value::F64(std::f64::consts::PI)),
        "get_E" | "E" => Some(Value::F64(std::f64::consts::E)),
        _ => {
            crate::log_debug!("clr-bcl", "Math.{} non implémenté", method);
            Some(Value::F64(0.0))
        }
    }
}

// ── System.String ──

fn string_dispatch(method: &str, args: &[Value]) -> Option<Value> {
    match method {
        "Concat" => {
            let mut result = String::new();
            for a in args {
                result.push_str(&value_to_string(a));
            }
            Some(alloc_managed_string(&result))
        }
        "get_Length" => match args.first() {
            Some(Value::Ref(p)) if !p.is_null() => {
                let len = unsafe { *((*p) as *const u32) } as i32;
                Some(Value::I32(len))
            }
            _ => Some(Value::I32(0)),
        },
        "IsNullOrEmpty" => match args.last() {
            Some(Value::Null) => Some(Value::I32(1)),
            Some(Value::Ref(p)) if p.is_null() => Some(Value::I32(1)),
            Some(Value::Ref(p)) => {
                let len = unsafe { *(*p as *const u32) };
                Some(Value::I32(if len == 0 { 1 } else { 0 }))
            }
            _ => Some(Value::I32(1)),
        },
        "op_Equality" => {
            if args.len() >= 2 {
                let a = value_to_string(&args[args.len() - 2]);
                let b = value_to_string(&args[args.len() - 1]);
                Some(Value::I32(if a == b { 1 } else { 0 }))
            } else {
                Some(Value::I32(0))
            }
        }
        "op_Inequality" => {
            if args.len() >= 2 {
                let a = value_to_string(&args[args.len() - 2]);
                let b = value_to_string(&args[args.len() - 1]);
                Some(Value::I32(if a != b { 1 } else { 0 }))
            } else {
                Some(Value::I32(1))
            }
        }
        "Substring" => {
            let this_str = match args.first() {
                Some(Value::Ref(p)) if !p.is_null() => read_managed_string(*p),
                _ => return Some(Value::Null),
            };
            let start = match args.get(1) {
                Some(Value::I32(s)) => *s as usize,
                _ => 0,
            };
            let sub = if let Some(Value::I32(len)) = args.get(2) {
                let end = (start + *len as usize).min(this_str.len());
                &this_str[start..end]
            } else {
                &this_str[start.min(this_str.len())..]
            };
            Some(alloc_managed_string(sub))
        }
        "ToUpper" => {
            let s = match args.first() {
                Some(v) => value_to_string(v).to_uppercase(),
                None => String::new(),
            };
            Some(alloc_managed_string(&s))
        }
        "ToLower" => {
            let s = match args.first() {
                Some(v) => value_to_string(v).to_lowercase(),
                None => String::new(),
            };
            Some(alloc_managed_string(&s))
        }
        "Trim" => {
            let s = match args.first() {
                Some(v) => value_to_string(v).trim().to_string(),
                None => String::new(),
            };
            Some(alloc_managed_string(&s))
        }
        "Contains" => {
            if args.len() >= 2 {
                let haystack = value_to_string(&args[0]);
                let needle = value_to_string(&args[1]);
                Some(Value::I32(if haystack.contains(&needle) { 1 } else { 0 }))
            } else {
                Some(Value::I32(0))
            }
        }
        "StartsWith" => {
            if args.len() >= 2 {
                let s = value_to_string(&args[0]);
                let prefix = value_to_string(&args[1]);
                Some(Value::I32(if s.starts_with(&prefix) { 1 } else { 0 }))
            } else {
                Some(Value::I32(0))
            }
        }
        "EndsWith" => {
            if args.len() >= 2 {
                let s = value_to_string(&args[0]);
                let suffix = value_to_string(&args[1]);
                Some(Value::I32(if s.ends_with(&suffix) { 1 } else { 0 }))
            } else {
                Some(Value::I32(0))
            }
        }
        "Replace" => {
            if args.len() >= 3 {
                let s = value_to_string(&args[0]);
                let from = value_to_string(&args[1]);
                let to = value_to_string(&args[2]);
                Some(alloc_managed_string(&s.replace(&from, &to)))
            } else {
                Some(Value::Null)
            }
        }
        "IndexOf" => {
            if args.len() >= 2 {
                let s = value_to_string(&args[0]);
                let needle = value_to_string(&args[1]);
                let idx = s.find(&needle).map(|i| i as i32).unwrap_or(-1);
                Some(Value::I32(idx))
            } else {
                Some(Value::I32(-1))
            }
        }
        "Format" => {
            if args.is_empty() {
                return Some(alloc_managed_string(""));
            }
            let text = format_composite(&value_to_string(&args[0]), &args[1..]);
            Some(alloc_managed_string(&text))
        }
        "get_Chars" => {
            if args.len() >= 2 {
                let s = value_to_string(&args[0]);
                let idx = match &args[1] {
                    Value::I32(i) => *i as usize,
                    _ => 0,
                };
                let ch = s.chars().nth(idx).unwrap_or('\0') as i32;
                Some(Value::I32(ch))
            } else {
                Some(Value::I32(0))
            }
        }
        "ToString" => match args.first() {
            Some(v) => Some(alloc_managed_string(&value_to_string(v))),
            None => Some(alloc_managed_string("")),
        },
        _ => {
            crate::log_debug!("clr-bcl", "String.{} non implémenté", method);
            Some(Value::Null)
        }
    }
}

// ── System.Object ──

fn object_dispatch(method: &str, args: &[Value]) -> Option<Value> {
    match method {
        "ToString" => Some(alloc_managed_string(&value_to_string(args.first()?))),
        "GetType" => Some(Value::Null),
        "Equals" => {
            if args.len() >= 2 {
                let eq = match (&args[0], &args[1]) {
                    (Value::I32(a), Value::I32(b)) => a == b,
                    (Value::I64(a), Value::I64(b)) => a == b,
                    (Value::Ref(a), Value::Ref(b)) => std::ptr::eq(*a, *b),
                    (Value::Null, Value::Null) => true,
                    _ => false,
                };
                Some(Value::I32(if eq { 1 } else { 0 }))
            } else {
                Some(Value::I32(0))
            }
        }
        "GetHashCode" => match args.first() {
            Some(Value::Ref(p)) => Some(Value::I32(*p as i32)),
            Some(Value::I32(v)) => Some(Value::I32(*v)),
            Some(Value::I64(v)) => Some(Value::I32(*v as i32)),
            _ => Some(Value::I32(0)),
        },
        "ReferenceEquals" => {
            if args.len() >= 2 {
                let eq = match (&args[0], &args[1]) {
                    (Value::Ref(a), Value::Ref(b)) => std::ptr::eq(*a, *b),
                    (Value::Null, Value::Null) => true,
                    _ => false,
                };
                Some(Value::I32(if eq { 1 } else { 0 }))
            } else {
                Some(Value::I32(0))
            }
        }
        ".ctor" => Some(Value::I32(0)),
        "MemberwiseClone" => args.first().cloned().or(Some(Value::Null)),
        "Finalize" => Some(Value::I32(0)),
        _ => {
            crate::log_debug!("clr-bcl", "Object.{} non implémenté", method);
            Some(Value::I32(0))
        }
    }
}

// ── System.Array ──

fn array_dispatch(method: &str, args: &[Value]) -> Option<Value> {
    match method {
        "get_Length" | "GetLength" => match args.first() {
            Some(Value::Ref(p)) if !p.is_null() => {
                let len = unsafe { *(*p as *const u64) } as i32;
                Some(Value::I32(len))
            }
            _ => Some(Value::I32(0)),
        },
        "Copy" => Some(Value::I32(0)),
        "Clear" => Some(Value::I32(0)),
        "Reverse" => Some(Value::I32(0)),
        "Sort" => Some(Value::I32(0)),
        "get_Rank" => Some(Value::I32(1)),
        _ => {
            crate::log_debug!("clr-bcl", "Array.{} non implémenté", method);
            Some(Value::I32(0))
        }
    }
}

// ── System.GC ──

fn gc_dispatch(method: &str, _args: &[Value]) -> Option<Value> {
    match method {
        "SuppressFinalize" | "ReRegisterForFinalize" | "WaitForPendingFinalizers" => {
            Some(Value::I32(0))
        }
        "GetTotalMemory" => {
            let total = gc::with_gc(|gc| gc.total_allocated());
            Some(Value::I64(total as i64))
        }
        _ => Some(Value::I32(0)),
    }
}

// ── System.Convert ──

fn convert_dispatch(method: &str, args: &[Value]) -> Option<Value> {
    let val = args.last()?;
    match method {
        "ToInt32" => match val {
            Value::I32(v) => Some(Value::I32(*v)),
            Value::I64(v) => Some(Value::I32(*v as i32)),
            Value::F64(v) => Some(Value::I32(*v as i32)),
            Value::F32(v) => Some(Value::I32(*v as i32)),
            Value::Ref(p) if !p.is_null() => {
                let s = read_managed_string(*p);
                Some(Value::I32(s.parse::<i32>().unwrap_or(0)))
            }
            _ => Some(Value::I32(0)),
        },
        "ToInt64" => match val {
            Value::I32(v) => Some(Value::I64(*v as i64)),
            Value::I64(v) => Some(Value::I64(*v)),
            Value::F64(v) => Some(Value::I64(*v as i64)),
            Value::Ref(p) if !p.is_null() => {
                let s = read_managed_string(*p);
                Some(Value::I64(s.parse::<i64>().unwrap_or(0)))
            }
            _ => Some(Value::I64(0)),
        },
        "ToDouble" => match val {
            Value::I32(v) => Some(Value::F64(*v as f64)),
            Value::I64(v) => Some(Value::F64(*v as f64)),
            Value::F64(v) => Some(Value::F64(*v)),
            Value::F32(v) => Some(Value::F64(*v as f64)),
            Value::Ref(p) if !p.is_null() => {
                let s = read_managed_string(*p);
                Some(Value::F64(s.parse::<f64>().unwrap_or(0.0)))
            }
            _ => Some(Value::F64(0.0)),
        },
        "ToString" => Some(alloc_managed_string(&value_to_string(val))),
        "ToBoolean" => {
            let b = match val {
                Value::I32(v) => *v != 0,
                Value::I64(v) => *v != 0,
                Value::Null => false,
                _ => true,
            };
            Some(Value::I32(if b { 1 } else { 0 }))
        }
        "ToByte" => match val {
            Value::I32(v) => Some(Value::I32((*v as u8) as i32)),
            _ => Some(Value::I32(0)),
        },
        "ToChar" => match val {
            Value::I32(v) => Some(Value::I32((*v as u16) as i32)),
            _ => Some(Value::I32(0)),
        },
        _ => {
            crate::log_debug!("clr-bcl", "Convert.{} non implémenté", method);
            Some(Value::I32(0))
        }
    }
}

// ── System.Type ──

fn type_dispatch(method: &str, args: &[Value]) -> Option<Value> {
    match method {
        "GetTypeFromHandle" => args.first().cloned().or(Some(Value::Null)),
        "get_Name" | "get_FullName" => Some(alloc_managed_string("System.Object")),
        "get_IsValueType" => Some(Value::I32(0)),
        "get_IsClass" => Some(Value::I32(1)),
        _ => {
            crate::log_debug!("clr-bcl", "Type.{} non implémenté", method);
            Some(Value::Null)
        }
    }
}

// ── System.Activator ──

fn activator_dispatch(method: &str, _args: &[Value]) -> Option<Value> {
    match method {
        "CreateInstance" => Some(Value::Null),
        _ => Some(Value::Null),
    }
}

// ── System.IO.File ──

fn file_dispatch(method: &str, args: &[Value]) -> Option<Value> {
    match method {
        "Exists" => {
            let path = match args.last() {
                Some(Value::Ref(p)) if !p.is_null() => read_managed_string(*p),
                _ => return Some(Value::I32(0)),
            };
            Some(Value::I32(if std::path::Path::new(&path).exists() {
                1
            } else {
                0
            }))
        }
        "ReadAllText" => {
            let path = match args.last() {
                Some(Value::Ref(p)) if !p.is_null() => read_managed_string(*p),
                _ => return Some(Value::Null),
            };
            match std::fs::read_to_string(&path) {
                Ok(content) => Some(alloc_managed_string(&content)),
                Err(_) => Some(Value::Null),
            }
        }
        "WriteAllText" => {
            if args.len() >= 2 {
                let path = value_to_string(&args[args.len() - 2]);
                let content = value_to_string(&args[args.len() - 1]);
                let _ = std::fs::write(&path, &content);
            }
            Some(Value::I32(0))
        }
        "Delete" => {
            let path = match args.last() {
                Some(Value::Ref(p)) if !p.is_null() => read_managed_string(*p),
                _ => return Some(Value::I32(0)),
            };
            let _ = std::fs::remove_file(&path);
            Some(Value::I32(0))
        }
        _ => {
            crate::log_debug!("clr-bcl", "File.{} non implémenté", method);
            Some(Value::Null)
        }
    }
}

// ── System.IO.Path ──

fn path_dispatch(method: &str, args: &[Value]) -> Option<Value> {
    match method {
        "Combine" => {
            let parts: Vec<String> = args.iter().map(value_to_string).collect();
            let mut path = std::path::PathBuf::new();
            for p in &parts {
                path.push(p);
            }
            Some(alloc_managed_string(&path.to_string_lossy()))
        }
        "GetFileName" => {
            let s = value_to_string(args.last()?);
            let name = std::path::Path::new(&s)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            Some(alloc_managed_string(&name))
        }
        "GetExtension" => {
            let s = value_to_string(args.last()?);
            let ext = std::path::Path::new(&s)
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            Some(alloc_managed_string(&ext))
        }
        "GetDirectoryName" => {
            let s = value_to_string(args.last()?);
            let dir = std::path::Path::new(&s)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            Some(alloc_managed_string(&dir))
        }
        _ => {
            crate::log_debug!("clr-bcl", "Path.{} non implémenté", method);
            Some(Value::Null)
        }
    }
}

// ── System.Runtime.InteropServices.Marshal ──

fn marshal_dispatch(method: &str, args: &[Value]) -> Option<Value> {
    match method {
        "SizeOf" => Some(Value::I32(8)),
        "GetLastWin32Error" | "GetLastPInvokeError" => Some(Value::I32(0)),
        "PtrToStringAnsi" | "PtrToStringUni" => match args.last() {
            Some(Value::Ref(p)) if !p.is_null() => {
                let s = unsafe { std::ffi::CStr::from_ptr(*p as *const std::ffi::c_char) };
                Some(alloc_managed_string(&s.to_string_lossy()))
            }
            _ => Some(Value::Null),
        },
        "StringToHGlobalAnsi" | "StringToHGlobalUni" => match args.last() {
            Some(Value::Ref(p)) if !p.is_null() => {
                let s = read_managed_string(*p);
                let c = std::ffi::CString::new(s).unwrap_or_default();
                let ptr = c.into_raw();
                Some(Value::Ref(ptr as *mut u8))
            }
            _ => Some(Value::Ref(std::ptr::null_mut())),
        },
        "FreeHGlobal" => Some(Value::I32(0)),
        "AllocHGlobal" => {
            let size = match args.last() {
                Some(Value::I32(s)) => *s as usize,
                _ => 0,
            };
            if size > 0 {
                let layout = std::alloc::Layout::from_size_align(size, 8).unwrap();
                let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
                Some(Value::Ref(ptr))
            } else {
                Some(Value::Ref(std::ptr::null_mut()))
            }
        }
        _ => {
            crate::log_debug!("clr-bcl", "Marshal.{} non implémenté", method);
            Some(Value::I32(0))
        }
    }
}
