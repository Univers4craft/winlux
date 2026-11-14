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
        ("System", "Number", m) => number_dispatch(m, args),
        ("System", "Char", m) => char_dispatch(m, args),
        // Horloge de DateTime (#1963) : graduations de 100 ns depuis le
        // 1er janvier de l'an 1 (UTC), et décalage de l'heure locale.
        ("System", "Horloge", "UtcNowTicks") => {
            let since_unix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            Some(Value::I64(
                UNIX_EPOCH_TICKS + (since_unix.as_nanos() / 100) as i64,
            ))
        }
        ("System", "Horloge", "LocalOffsetTicks") => {
            let utc = args.first()?.as_i64();
            Some(Value::I64(local_offset_ticks(utc)))
        }
        // Octets aléatoires de Guid.NewGuid (#1962).
        ("System", "Guid", "FillRandom") => {
            let buf = args.first()?.as_ref();
            if !buf.is_null() {
                let len = unsafe { *(buf as *const u64) } as usize;
                let mut filled = 0;
                while filled < len {
                    let n = unsafe { libc::getrandom(buf.add(8 + filled).cast(), len - filled, 0) };
                    if n <= 0 {
                        return None;
                    }
                    filled += n as usize;
                }
            }
            Some(Value::Null)
        }
        ("System", "Object", m) => object_dispatch(m, args),
        ("System", "Array", m) => array_dispatch(m, args),
        ("System", "GC", m) => gc_dispatch(m, args),
        ("System", "Convert", m) => convert_dispatch(m, args),
        ("System", "Type", m) => type_dispatch(m, args),
        ("System", "Activator", m) => activator_dispatch(m, args),
        ("System.Runtime.InteropServices", "Marshal", m) => marshal_dispatch(m, args),
        // Horloge monotone de Stopwatch, en graduations de 100 ns (#1922).
        ("System.Diagnostics", "Stopwatch", "GetTimestamp") => {
            static DEBUT: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
            let ecoule = DEBUT.get_or_init(std::time::Instant::now).elapsed();
            Some(Value::I64((ecoule.as_nanos() / 100) as i64))
        }
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

/// 1970-01-01 en graduations de 100 ns depuis 0001-01-01.
const UNIX_EPOCH_TICKS: i64 = 621_355_968_000_000_000;

/// Décalage de l'heure locale (fuseau du système, heure d'été comprise) à
/// l'instant UTC `utc_ticks`, en graduations de 100 ns.
fn local_offset_ticks(utc_ticks: i64) -> i64 {
    let t = ((utc_ticks - UNIX_EPOCH_TICKS) / 10_000_000) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return 0;
    }
    tm.tm_gmtoff * 10_000_000
}

/// Méthode connue de la classe mais pas encore écrite : l'appel lève une
/// `MissingMethodException` qui la nomme, au lieu de rendre une valeur vide
/// qui finirait en `NullReferenceException` trompeuse (#1190).
fn missing(class: &str, method: &str) -> Option<Value> {
    crate::log_warn!("clr-bcl", "{}.{} non implémenté", class, method);
    None
}

fn is_null(v: &Value) -> bool {
    matches!(v, Value::Null) || matches!(v, Value::Ref(p) if p.is_null())
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

/// Chaîne managée faite d'unités UTF-16.
pub fn alloc_utf16(units: &[u16]) -> Value {
    let ptr = gc::with_gc(|gc| gc.alloc(TYPE_STRING, 8 + units.len() * 2));
    unsafe {
        *(ptr as *mut u64) = units.len() as u64;
        std::ptr::copy_nonoverlapping(units.as_ptr(), ptr.add(8) as *mut u16, units.len());
    }
    Value::Ref(ptr)
}

/// Unités UTF-16 d'une chaîne managée (vide pour null).
/// Un objet qui n'est pas une `System.String` du tas (un tableau passé par
/// réflexion à une primitive, #2037) se lit aussi comme la chaîne vide : sa
/// première case n'est pas une longueur.
pub fn managed_utf16(ptr: *mut u8) -> Vec<u16> {
    string_units(ptr).to_vec()
}

/// Les unités d'une vraie chaîne managée, ou rien (#2037).
fn string_units<'a>(ptr: *mut u8) -> &'a [u16] {
    if gc::type_of(ptr) != Some(TYPE_STRING) {
        return &[];
    }
    unsafe {
        let len = *(ptr as *const u64) as usize;
        std::slice::from_raw_parts(ptr.add(8) as *const u16, len)
    }
}

/// `newobj System.String::.ctor` (#1956) : (char[]), (char[], int, int),
/// (char, int), (char*), (char*, int, int), (sbyte*). Rend la chaîne, ou
/// `None` pour une surcharge inconnue.
pub fn new_string(args: &[Value], params: &[ElementType]) -> Result<Value, &'static str> {
    use ElementType as E;
    let int = |v: &Value| v.as_i64();
    match (params, args) {
        ([E::SzArray(_)], [a]) | ([E::SzArray(_), E::I4, E::I4], [a, _, _]) => {
            let arr = a.as_ref();
            if arr.is_null() {
                return Ok(alloc_utf16(&[]));
            }
            let len = unsafe { *(arr as *const u64) } as i64;
            let (start, count) = if args.len() == 3 {
                (int(&args[1]), int(&args[2]))
            } else {
                (0, len)
            };
            if start < 0 || count < 0 || start + count > len {
                return Err("ArgumentOutOfRangeException");
            }
            let units = unsafe {
                std::slice::from_raw_parts(
                    (arr.add(8) as *const u16).add(start as usize),
                    count as usize,
                )
            };
            Ok(alloc_utf16(units))
        }
        ([E::Char, E::I4], [c, n]) => {
            if int(n) < 0 {
                return Err("ArgumentOutOfRangeException");
            }
            Ok(alloc_utf16(&vec![c.as_i32() as u16; int(n) as usize]))
        }
        ([E::Ptr(inner)], [p]) | ([E::Ptr(inner), E::I4, E::I4], [p, _, _]) => {
            let base = p.as_ref();
            if base.is_null() {
                return Ok(alloc_utf16(&[]));
            }
            let wide = matches!(**inner, E::Char);
            let (start, count) = if args.len() == 3 {
                (int(&args[1]), Some(int(&args[2])))
            } else {
                (0, None)
            };
            let mut units = Vec::new();
            let mut i = start as usize;
            loop {
                if count.is_some_and(|c| units.len() as i64 >= c) {
                    break;
                }
                let u = unsafe {
                    if wide {
                        *(base as *const u16).add(i)
                    } else {
                        u16::from(*base.add(i))
                    }
                };
                if count.is_none() && u == 0 {
                    break;
                }
                units.push(u);
                i += 1;
            }
            Ok(alloc_utf16(&units))
        }
        _ => Err("MissingMethodException"),
    }
}

// ── System.Char : casse et catégories Unicode (#1956) ──

fn char_dispatch(method: &str, args: &[Value]) -> Option<Value> {
    let unit = args.first()?.as_i32() as u16;
    // Une demi-paire de substitution n'a ni casse ni catégorie de lettre.
    let c = char::from_u32(u32::from(unit));
    let map = |up: bool| -> u16 {
        let Some(c) = c else { return unit };
        let mut it: Box<dyn Iterator<Item = char>> = if up {
            Box::new(c.to_uppercase())
        } else {
            Box::new(c.to_lowercase())
        };
        match (it.next(), it.next()) {
            (Some(m), None) if (m as u32) < 0x10000 => m as u32 as u16,
            _ => unit,
        }
    };
    let test = |f: fn(char) -> bool| Value::I32(c.is_some_and(f) as i32);
    Some(match method {
        "ToUpper" | "ToUpperInvariant" => Value::I32(i32::from(map(true))),
        "ToLower" | "ToLowerInvariant" => Value::I32(i32::from(map(false))),
        "IsLetter" => test(char::is_alphabetic),
        "IsUpper" => test(char::is_uppercase),
        "IsLower" => test(char::is_lowercase),
        "IsNumber" => test(char::is_numeric),
        "IsControl" => test(char::is_control),
        "IsPunctuation" => test(is_punctuation),
        _ => return missing("Char", method),
    })
}

/// Catégories P* d'Unicode pour les blocs courants : ASCII sans les
/// symboles `$+<=>^`|~`, Latin-1, ponctuation générale et CJK.
fn is_punctuation(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_punctuation() && !"$+<=>^`|~".contains(c);
    }
    matches!(c as u32,
        0xA1 | 0xA7 | 0xAB | 0xB6 | 0xB7 | 0xBB | 0xBF
        | 0x2010..=0x2027 | 0x2030..=0x2043 | 0x2045..=0x2051 | 0x2053..=0x205E
        | 0x3001..=0x3003 | 0x3008..=0x3011 | 0x3014..=0x301F)
}

// ── System.Number (interne à notre mscorlib, #1956) ──

fn number_dispatch(method: &str, args: &[Value]) -> Option<Value> {
    let text = |v: &Value| match v {
        Value::Ref(p) if !p.is_null() => read_managed_string(*p),
        _ => String::new(),
    };
    match (method, args) {
        ("FormatInteger", [v, unsigned, size, fmt]) => {
            Some(alloc_managed_string(&super::nombres::format_integer(
                v.as_i64(),
                unsigned.as_i32() != 0,
                size.as_i32() as usize,
                &text(fmt),
            )))
        }
        ("FormatDouble", [v, single, fmt]) => Some(alloc_managed_string(
            &super::nombres::format_double(v.as_f64(), single.as_i32() != 0, &text(fmt)),
        )),
        ("TryParseDouble", [s, out]) => {
            let parsed = super::nombres::parse_double(&text(s));
            let p = out.as_ref();
            if !p.is_null() {
                unsafe { (p as *mut f64).write_unaligned(parsed.unwrap_or(0.0)) };
            }
            Some(Value::I32(parsed.is_some() as i32))
        }
        _ => missing("Number", method),
    }
}

/// Texte d'une chaîne managée ; vide pour null ou pour un objet qui n'est
/// pas une chaîne (#2037).
pub fn read_managed_string(ptr: *mut u8) -> String {
    String::from_utf16_lossy(string_units(ptr))
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

/// Contenu d'une valeur encadrée (type valeur) ou d'une chaîne : ce que
/// comparent ValueType.Equals et String.Equals (#1896, clés de
/// Dictionary<int, …>).
fn boxed_bytes(p: *mut u8) -> Option<(types::TypeId, Vec<u8>)> {
    let id = gc::type_of(p)?;
    if id == TYPE_STRING {
        let s = read_managed_string(p);
        return Some((id, s.encode_utf16().flat_map(u16::to_le_bytes).collect()));
    }
    let size = types::with_types(|reg| {
        let t = reg.get(id)?;
        t.is_value.then_some(t.instance_size)
    })?;
    let size = if is_boxed_primitive(id) { 8 } else { size };
    Some((id, unsafe { std::slice::from_raw_parts(p, size) }.to_vec()))
}

pub(super) fn boxed_equal(a: *mut u8, b: *mut u8) -> bool {
    match (boxed_bytes(a), boxed_bytes(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

fn boxed_hash(p: *mut u8) -> Option<i32> {
    let (_, bytes) = boxed_bytes(p)?;
    let mut h: u32 = 0x811C_9DC5;
    for b in bytes {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    Some(h as i32)
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
        // Console.CursorLeft (#2233) : X de dwCursorPosition, ou moins
        // l'erreur Win32 quand la sortie n'est pas une console.
        "Colonne" => {
            let mut info = [0u16; 11];
            let ok = unsafe {
                crate::dlls::kernel32_misc::get_console_screen_buffer_info(
                    (-11isize) as usize,
                    info.as_mut_ptr(),
                )
            };
            Some(Value::I32(if ok != 0 {
                info[2] as i32
            } else {
                -(crate::process::last_error() as i32)
            }))
        }
        _ => missing("Console", method),
    }
}

// ── System.Environment ──

fn environment_dispatch(method: &str, args: &[Value]) -> Option<Value> {
    match method {
        "Exit" | "Quitter" => {
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
        "get_ProcessorCount" => Some(Value::I32(
            std::thread::available_parallelism().map_or(1, |n| n.get() as i32),
        )),
        // Faux dans un service Windows : le gestionnaire de services de
        // Winlux le lance avec WINLUX_SERVICE_NAME (#1991).
        "get_UserInteractive" => Some(Value::I32(
            std::env::var_os("WINLUX_SERVICE_NAME").is_none_or(|n| n.is_empty()) as i32,
        )),
        // Environment.GetFolderPath : chemin du CSIDL, comme SHGetFolderPath.
        "CheminSpecial" => {
            let csidl = args.first().map_or(-1, |v| v.as_i32());
            Some(match crate::dlls::knownfolders::csidl_path(csidl) {
                Some(p) => alloc_managed_string(&p),
                None => Value::Null,
            })
        }
        "get_Is64BitProcess" => Some(Value::I32(1)),
        "get_Is64BitOperatingSystem" => Some(Value::I32(1)),
        "GetCommandLineArgs" => Some(Value::Null),
        // Environment.GetEnvironmentVariables (#2233) : le bloc du processus,
        // le même que GetEnvironmentStringsW.
        "BlocEnvironnement" => {
            let vars: Vec<String> = std::env::vars_os()
                .map(|(k, v)| format!("{}={}", k.to_string_lossy(), v.to_string_lossy()))
                .collect();
            Some(alloc_string_array(&vars))
        }
        _ => missing("Environment", method),
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
        "Sinh" => Some(Value::F64(to_f64(args.last()?).sinh())),
        "Cosh" => Some(Value::F64(to_f64(args.last()?).cosh())),
        "Tanh" => Some(Value::F64(to_f64(args.last()?).tanh())),
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
        // Arrondi bancaire, comme Math.Round(double) (#2077).
        "Round" => Some(Value::F64(to_f64(args.last()?).round_ties_even())),
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
        _ => missing("Math", method),
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
                let (x, y) = (&args[args.len() - 2], &args[args.len() - 1]);
                // null n'est égal qu'à null, pas à "" (String.op_Equality).
                let eq = match (is_null(x), is_null(y)) {
                    (true, true) => true,
                    (false, false) => value_to_string(x) == value_to_string(y),
                    _ => false,
                };
                Some(Value::I32(eq as i32))
            } else {
                Some(Value::I32(0))
            }
        }
        // String.Equals(object) : vrai pour une chaîne de même contenu.
        "Equals" => match args {
            [Value::Ref(a), Value::Ref(b)] if gc::type_of(*b) == Some(TYPE_STRING) => Some(
                Value::I32((read_managed_string(*a) == read_managed_string(*b)) as i32),
            ),
            [_, _] => Some(Value::I32(0)),
            _ => None,
        },
        // Empreinte du contenu (FNV-1a sur l'UTF-16) : deux chaînes égales
        // ont la même, quel que soit l'objet.
        "GetHashCode" => match args.first() {
            Some(Value::Ref(p)) if !p.is_null() => {
                let mut h: u32 = 0x811C_9DC5;
                for u in read_managed_string(*p).encode_utf16() {
                    h = (h ^ u32::from(u)).wrapping_mul(0x0100_0193);
                }
                Some(Value::I32(h as i32))
            }
            _ => None,
        },
        "op_Inequality" => match string_dispatch("op_Equality", args) {
            Some(Value::I32(eq)) => Some(Value::I32((eq == 0) as i32)),
            other => other,
        },
        // Indices en unités UTF-16, comme .NET.
        "Substring" => {
            let units = match args.first() {
                Some(Value::Ref(p)) if !p.is_null() => managed_utf16(*p),
                _ => return Some(Value::Null),
            };
            let start = args.get(1).map_or(0, |v| v.as_i64());
            let len = match args.get(2) {
                Some(v) => v.as_i64(),
                None => units.len() as i64 - start,
            };
            if start < 0 || len < 0 || start + len > units.len() as i64 {
                return None;
            }
            Some(alloc_utf16(&units[start as usize..(start + len) as usize]))
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
        "get_Chars" => match args {
            [Value::Ref(p), i] if !p.is_null() => {
                let units = managed_utf16(*p);
                let i = i.as_i64();
                (i >= 0 && (i as usize) < units.len())
                    .then(|| Value::I32(i32::from(units[i as usize])))
            }
            _ => None,
        },
        "ToString" => match args.first() {
            Some(v) => Some(alloc_managed_string(&value_to_string(v))),
            None => Some(alloc_managed_string("")),
        },
        _ => missing("String", method),
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
                    (Value::Ref(a), Value::Ref(b)) => std::ptr::eq(*a, *b) || boxed_equal(*a, *b),
                    (Value::Null, Value::Null) => true,
                    _ => false,
                };
                Some(Value::I32(if eq { 1 } else { 0 }))
            } else {
                Some(Value::I32(0))
            }
        }
        "GetHashCode" => match args.first() {
            Some(Value::Ref(p)) => Some(Value::I32(boxed_hash(*p).unwrap_or(*p as i32))),
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
        _ => missing("Object", method),
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
        _ => missing("Array", method),
    }
}

// ── System.GC ──

fn gc_dispatch(method: &str, _args: &[Value]) -> Option<Value> {
    match method {
        "SuppressFinalize" | "ReRegisterForFinalize" | "WaitForPendingFinalizers" => {
            Some(Value::I32(0))
        }
        "GetTotalMemory" | "Alloue" => {
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
        _ => missing("Convert", method),
    }
}

// ── System.Type ──

fn type_dispatch(method: &str, args: &[Value]) -> Option<Value> {
    match method {
        "GetTypeFromHandle" => args.first().cloned().or(Some(Value::Null)),
        "get_Name" | "get_FullName" => Some(alloc_managed_string("System.Object")),
        "get_IsValueType" => Some(Value::I32(0)),
        "get_IsClass" => Some(Value::I32(1)),
        _ => missing("Type", method),
    }
}

// ── System.Activator ──

fn activator_dispatch(method: &str, _args: &[Value]) -> Option<Value> {
    match method {
        "CreateInstance" => Some(Value::Null),
        _ => Some(Value::Null),
    }
}

// System.IO (File, Path, Directory, flux) est écrit en C# dans notre
// mscorlib, au-dessus des primitives de fichiers.rs (#2012, #2013).

// ── System.Runtime.InteropServices.Marshal ──

fn marshal_dispatch(method: &str, args: &[Value]) -> Option<Value> {
    match method {
        // SizeOf est servi par marshal.rs (#2026).
        "GetLastWin32Error" | "GetLastPInvokeError" => {
            Some(Value::I32(crate::process::last_error() as i32))
        }
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
        _ => missing("Marshal", method),
    }
}
