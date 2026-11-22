//! Classe Windows Runtime `Windows.Foundation.PropertyValue` (#2192) :
//! emballage d'une valeur scalaire dans un objet IPropertyValue, que
//! C++/WinRT utilise pour `box_value` et relit par IReference<T>
//! (`unbox_value`). Teams demande la fabrique au démarrage ; sans elle,
//! winrt::hresult_class_not_registered n'est pas rattrapée et Teams s'arrête.
//!
//! Sources : documentation publique de Microsoft (PropertyValue,
//! IPropertyValue, PropertyType, IReference<T> ; TYPE_E_TYPEMISMATCH et
//! DISP_E_OVERFLOW de winerror.h du SDK Windows) ; identifiants d'interface
//! et ordre des méthodes tels que les publie windows-rs (licence MIT,
//! crates/libs/windows/src/Windows/Foundation/mod.rs) ; identifiants des
//! IReference<T> calculés selon le système de types Windows Runtime
//! (`comobj::pinterface_iid`).
//!
//! Servis : CreateEmpty (rend nul, comme Windows), les scalaires de
//! CreateUInt8 à CreateRect, CreateInspectable (rend l'objet lui-même), et
//! les lectures d'IPropertyValue avec les conversions numériques. Pas
//! encore servis : les Create…Array (E_NOTIMPL, journalisé une fois) ; les
//! Get…Array rendent TYPE_E_TYPEMISMATCH, aucun tableau n'étant emballé.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use super::comobj::{
    guid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, IID_IUNKNOWN, S_OK,
};

const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
pub const TYPE_E_TYPEMISMATCH: i32 = 0x8002_8CA0u32 as i32;
pub const DISP_E_OVERFLOW: i32 = 0x8002_000Au32 as i32;

const fn g(d1: u32, d2: u16, d3: u16, d4: u64) -> Guid {
    guid(d1, d2, d3, d4.to_be_bytes())
}

const IID_IINSPECTABLE: Guid = g(0xAF86_E2E0, 0xB12D, 0x4C6A, 0x9C5A_D7AA_6510_1E90);
const IID_IAGILEOBJECT: Guid = g(0x94EA_2B94, 0xE9CC, 0x49E0, 0xC0FF_EE64_CA8F_5B90);
const IID_PROPERTY_VALUE: Guid = g(0x4bd6_82dd, 0x7554, 0x40e9, 0x9a9b_8265_4ede_7e62);
const IID_PROPERTY_VALUE_STATICS: Guid = g(0x629b_dbc8, 0xd932, 0x4ff4, 0x96b9_8d96_c5c1_e858);

const CLASS: &str = "Windows.Foundation.PropertyValue";

fn not_implemented(method: &str) -> i32 {
    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if !seen.iter().any(|m| m == method) {
        seen.push(method.to_string());
        crate::log_warn!(
            "com",
            "PropertyValue : {method} pas encore servie (E_NOTIMPL)"
        );
    }
    E_NOTIMPL
}

unsafe fn hstring(text: &[u16], out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    super::electron_win32::WindowsCreateString(text.as_ptr(), text.len() as u32, out)
}

// ============================================================
//  Valeurs
// ============================================================

#[derive(Clone, Debug, PartialEq)]
enum Val {
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F32(f32),
    F64(f64),
    Char16(u16),
    Bool(bool),
    Str(Vec<u16>),
    DateTime(i64),
    TimeSpan(i64),
    Guid(Guid),
    Point([f32; 2]),
    Size([f32; 2]),
    Rect([f32; 4]),
}

impl Val {
    /// PropertyType.
    fn property_type(&self) -> i32 {
        match self {
            Val::U8(_) => 1,
            Val::I16(_) => 2,
            Val::U16(_) => 3,
            Val::I32(_) => 4,
            Val::U32(_) => 5,
            Val::I64(_) => 6,
            Val::U64(_) => 7,
            Val::F32(_) => 8,
            Val::F64(_) => 9,
            Val::Char16(_) => 10,
            Val::Bool(_) => 11,
            Val::Str(_) => 12,
            Val::DateTime(_) => 14,
            Val::TimeSpan(_) => 15,
            Val::Guid(_) => 16,
            Val::Point(_) => 17,
            Val::Size(_) => 18,
            Val::Rect(_) => 19,
        }
    }

    /// Signature Windows Runtime du type, pour IReference<T>.
    fn signature(&self) -> &'static str {
        match self {
            Val::U8(_) => "u1",
            Val::I16(_) => "i2",
            Val::U16(_) => "u2",
            Val::I32(_) => "i4",
            Val::U32(_) => "u4",
            Val::I64(_) => "i8",
            Val::U64(_) => "u8",
            Val::F32(_) => "f4",
            Val::F64(_) => "f8",
            Val::Char16(_) => "c2",
            Val::Bool(_) => "b1",
            Val::Str(_) => "string",
            Val::DateTime(_) => "struct(Windows.Foundation.DateTime;i8)",
            Val::TimeSpan(_) => "struct(Windows.Foundation.TimeSpan;i8)",
            Val::Guid(_) => "g16",
            Val::Point(_) => "struct(Windows.Foundation.Point;f4;f4)",
            Val::Size(_) => "struct(Windows.Foundation.Size;f4;f4)",
            Val::Rect(_) => "struct(Windows.Foundation.Rect;f4;f4;f4;f4)",
        }
    }

    /// Nom du type dans GetRuntimeClassName (`IReference`1<Int32>`).
    fn type_name(&self) -> &'static str {
        match self {
            Val::U8(_) => "UInt8",
            Val::I16(_) => "Int16",
            Val::U16(_) => "UInt16",
            Val::I32(_) => "Int32",
            Val::U32(_) => "UInt32",
            Val::I64(_) => "Int64",
            Val::U64(_) => "UInt64",
            Val::F32(_) => "Single",
            Val::F64(_) => "Double",
            Val::Char16(_) => "Char16",
            Val::Bool(_) => "Boolean",
            Val::Str(_) => "String",
            Val::DateTime(_) => "Windows.Foundation.DateTime",
            Val::TimeSpan(_) => "Windows.Foundation.TimeSpan",
            Val::Guid(_) => "Guid",
            Val::Point(_) => "Windows.Foundation.Point",
            Val::Size(_) => "Windows.Foundation.Size",
            Val::Rect(_) => "Windows.Foundation.Rect",
        }
    }

    /// Valeur numérique, exacte pour les entiers.
    fn numeric(&self) -> Option<Num> {
        Some(match *self {
            Val::U8(v) => Num::Int(v as i128),
            Val::I16(v) => Num::Int(v as i128),
            Val::U16(v) => Num::Int(v as i128),
            Val::I32(v) => Num::Int(v as i128),
            Val::U32(v) => Num::Int(v as i128),
            Val::I64(v) => Num::Int(v as i128),
            Val::U64(v) => Num::Int(v as i128),
            Val::F32(v) => Num::Float(v as f64),
            Val::F64(v) => Num::Float(v),
            _ => return None,
        })
    }
}

#[derive(Clone, Copy)]
enum Num {
    Int(i128),
    Float(f64),
}

/// Conversion vers un entier de bornes [min, max] : TYPE_E_TYPEMISMATCH
/// si la valeur n'est pas numérique ou pas entière, DISP_E_OVERFLOW hors
/// bornes.
fn to_int(v: &Val, min: i128, max: i128) -> Result<i128, i32> {
    let n = match v.numeric().ok_or(TYPE_E_TYPEMISMATCH)? {
        Num::Int(i) => i,
        Num::Float(f) => {
            if !f.is_finite() || f.fract() != 0.0 {
                return Err(TYPE_E_TYPEMISMATCH);
            }
            if f < min as f64 || f > max as f64 {
                return Err(DISP_E_OVERFLOW);
            }
            f as i128
        }
    };
    if n < min || n > max {
        Err(DISP_E_OVERFLOW)
    } else {
        Ok(n)
    }
}

fn to_float(v: &Val) -> Result<f64, i32> {
    match v.numeric().ok_or(TYPE_E_TYPEMISMATCH)? {
        Num::Int(i) => Ok(i as f64),
        Num::Float(f) => Ok(f),
    }
}

// ============================================================
//  Objet emballé : IPropertyValue à +0, IReference<T> à +8
// ============================================================

#[repr(C)]
struct Boxed {
    vt_value: &'static VTable<45>,
    vt_ref: &'static VTable<7>,
    refs: AtomicU32,
    val: Val,
}

fn new_boxed(val: Val) -> usize {
    Box::into_raw(Box::new(Boxed {
        vt_value: &VALUE_VTBL,
        vt_ref: &REFERENCE_VTBL,
        refs: AtomicU32::new(1),
        val,
    })) as usize
}

unsafe fn boxed<'a>(base: usize) -> &'a Boxed {
    &*(base as *const Boxed)
}

/// Identifiant d'IReference<T> pour une signature de type.
fn reference_iid(sig: &'static str) -> Guid {
    static CACHE: OnceLock<Mutex<Vec<(&'static str, Guid)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(Vec::new()));
    let mut c = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, g)) = c.iter().find(|(s, _)| *s == sig) {
        return *g;
    }
    let g = super::comobj::pinterface_iid(&format!(
        "pinterface({{61c17706-2d65-11e0-9ae8-d48564015472}};{sig})"
    ));
    c.push((sig, g));
    g
}

unsafe fn boxed_qi(base: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    let b = boxed(base);
    let p = if [
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
        IID_PROPERTY_VALUE,
    ]
    .contains(&iid)
    {
        base
    } else if iid == reference_iid(b.val.signature()) {
        base + 8
    } else {
        *out = 0;
        return E_NOINTERFACE;
    };
    b.refs.fetch_add(1, Ordering::SeqCst);
    *out = p;
    S_OK
}

unsafe fn boxed_release(base: usize) -> u32 {
    let n = boxed(base).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        drop(Box::from_raw(base as *mut Boxed));
    }
    n
}

unsafe extern "win64" fn get_iids(_this: usize, count: *mut u32, iids: *mut usize) -> i32 {
    if !count.is_null() {
        *count = 0;
    }
    if !iids.is_null() {
        *iids = 0;
    }
    S_OK
}
unsafe extern "win64" fn trust_level(_this: usize, level: *mut u32) -> i32 {
    if level.is_null() {
        return E_POINTER;
    }
    *level = 0;
    S_OK
}

unsafe extern "win64" fn b_qi<const OFF: usize>(
    this: usize,
    iid: *const u8,
    out: *mut usize,
) -> i32 {
    boxed_qi(this - OFF, iid, out)
}
unsafe extern "win64" fn b_add_ref<const OFF: usize>(this: usize) -> u32 {
    boxed(this - OFF).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn b_release<const OFF: usize>(this: usize) -> u32 {
    boxed_release(this - OFF)
}
unsafe extern "win64" fn b_class<const OFF: usize>(this: usize, out: *mut usize) -> i32 {
    let name = format!(
        "Windows.Foundation.IReference`1<{}>",
        boxed(this - OFF).val.type_name()
    );
    let w: Vec<u16> = name.encode_utf16().collect();
    hstring(&w, out)
}

unsafe fn val<'a>(this: usize) -> &'a Val {
    &boxed(this).val
}

unsafe extern "win64" fn pv_type(this: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = val(this).property_type();
    S_OK
}
unsafe extern "win64" fn pv_is_numeric(this: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = u8::from(val(this).numeric().is_some());
    S_OK
}

macro_rules! int_getter {
    ($name:ident, $t:ty) => {
        unsafe extern "win64" fn $name(this: usize, out: *mut $t) -> i32 {
            if out.is_null() {
                return E_POINTER;
            }
            match to_int(val(this), <$t>::MIN as i128, <$t>::MAX as i128) {
                Ok(v) => {
                    *out = v as $t;
                    S_OK
                }
                Err(e) => e,
            }
        }
    };
}
int_getter!(pv_get_u8, u8);
int_getter!(pv_get_i16, i16);
int_getter!(pv_get_u16, u16);
int_getter!(pv_get_i32, i32);
int_getter!(pv_get_u32, u32);
int_getter!(pv_get_i64, i64);
int_getter!(pv_get_u64, u64);

unsafe extern "win64" fn pv_get_single(this: usize, out: *mut f32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match to_float(val(this)) {
        Ok(v) => {
            *out = v as f32;
            S_OK
        }
        Err(e) => e,
    }
}
unsafe extern "win64" fn pv_get_double(this: usize, out: *mut f64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match to_float(val(this)) {
        Ok(v) => {
            *out = v;
            S_OK
        }
        Err(e) => e,
    }
}

/// Lecture d'un type non numérique : le même type, sinon TYPE_E_TYPEMISMATCH.
macro_rules! exact_getter {
    ($name:ident, $t:ty, $pat:pat => $v:expr) => {
        unsafe extern "win64" fn $name(this: usize, out: *mut $t) -> i32 {
            if out.is_null() {
                return E_POINTER;
            }
            match val(this) {
                $pat => {
                    *out = $v;
                    S_OK
                }
                _ => TYPE_E_TYPEMISMATCH,
            }
        }
    };
}
exact_getter!(pv_get_char16, u16, Val::Char16(c) => *c);
exact_getter!(pv_get_boolean, u8, Val::Bool(b) => u8::from(*b));
exact_getter!(pv_get_guid, Guid, Val::Guid(g) => *g);
exact_getter!(pv_get_datetime, i64, Val::DateTime(t) => *t);
exact_getter!(pv_get_timespan, i64, Val::TimeSpan(t) => *t);
exact_getter!(pv_get_point, [f32; 2], Val::Point(p) => *p);
exact_getter!(pv_get_size, [f32; 2], Val::Size(p) => *p);
exact_getter!(pv_get_rect, [f32; 4], Val::Rect(r) => *r);

unsafe extern "win64" fn pv_get_string(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match val(this) {
        Val::Str(s) => hstring(s, out),
        _ => {
            *out = 0;
            TYPE_E_TYPEMISMATCH
        }
    }
}

/// Get…Array : aucun tableau n'est emballé ici.
unsafe extern "win64" fn pv_get_array(_this: usize, len: *mut u32, out: *mut usize) -> i32 {
    if len.is_null() || out.is_null() {
        return E_POINTER;
    }
    *len = 0;
    *out = 0;
    TYPE_E_TYPEMISMATCH
}

/// IReference<T>.Value : la valeur sous son propre type.
unsafe extern "win64" fn ref_value(this: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    unsafe fn put<T: Copy>(out: *mut u8, v: T) -> i32 {
        std::ptr::write_unaligned(out as *mut T, v);
        S_OK
    }
    match val(this - 8) {
        Val::U8(v) => put(out, *v),
        Val::I16(v) => put(out, *v),
        Val::U16(v) => put(out, *v),
        Val::I32(v) => put(out, *v),
        Val::U32(v) => put(out, *v),
        Val::I64(v) => put(out, *v),
        Val::U64(v) => put(out, *v),
        Val::F32(v) => put(out, *v),
        Val::F64(v) => put(out, *v),
        Val::Char16(v) => put(out, *v),
        Val::Bool(v) => put(out, u8::from(*v)),
        Val::Str(s) => hstring(s, out as *mut usize),
        Val::DateTime(v) | Val::TimeSpan(v) => put(out, *v),
        Val::Guid(g) => put(out, *g),
        Val::Point(p) | Val::Size(p) => put(out, *p),
        Val::Rect(r) => put(out, *r),
    }
}

static VALUE_VTBL: VTable<45> = VTable([
    b_qi::<0> as *const (),
    b_add_ref::<0> as *const (),
    b_release::<0> as *const (),
    get_iids as *const (),
    b_class::<0> as *const (),
    trust_level as *const (),
    pv_type as *const (),
    pv_is_numeric as *const (),
    pv_get_u8 as *const (),
    pv_get_i16 as *const (),
    pv_get_u16 as *const (),
    pv_get_i32 as *const (),
    pv_get_u32 as *const (),
    pv_get_i64 as *const (),
    pv_get_u64 as *const (),
    pv_get_single as *const (),
    pv_get_double as *const (),
    pv_get_char16 as *const (),
    pv_get_boolean as *const (),
    pv_get_string as *const (),
    pv_get_guid as *const (),
    pv_get_datetime as *const (),
    pv_get_timespan as *const (),
    pv_get_point as *const (),
    pv_get_size as *const (),
    pv_get_rect as *const (),
    // GetUInt8Array … GetRectArray (19 tableaux).
    pv_get_array as *const (),
    pv_get_array as *const (),
    pv_get_array as *const (),
    pv_get_array as *const (),
    pv_get_array as *const (),
    pv_get_array as *const (),
    pv_get_array as *const (),
    pv_get_array as *const (),
    pv_get_array as *const (),
    pv_get_array as *const (),
    pv_get_array as *const (),
    pv_get_array as *const (),
    pv_get_array as *const (),
    pv_get_array as *const (),
    pv_get_array as *const (),
    pv_get_array as *const (),
    pv_get_array as *const (),
    pv_get_array as *const (),
    pv_get_array as *const (),
]);

static REFERENCE_VTBL: VTable<7> = VTable([
    b_qi::<8> as *const (),
    b_add_ref::<8> as *const (),
    b_release::<8> as *const (),
    get_iids as *const (),
    b_class::<8> as *const (),
    trust_level as *const (),
    ref_value as *const (),
]);

// ============================================================
//  Fabrique : IPropertyValueStatics
// ============================================================

unsafe fn create(v: Val, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_boxed(v);
    S_OK
}

/// CreateEmpty : un objet nul, comme Windows.
unsafe extern "win64" fn st_create_empty(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}
unsafe extern "win64" fn st_u8(_this: usize, v: u8, out: *mut usize) -> i32 {
    create(Val::U8(v), out)
}
unsafe extern "win64" fn st_i16(_this: usize, v: i16, out: *mut usize) -> i32 {
    create(Val::I16(v), out)
}
unsafe extern "win64" fn st_u16(_this: usize, v: u16, out: *mut usize) -> i32 {
    create(Val::U16(v), out)
}
unsafe extern "win64" fn st_i32(_this: usize, v: i32, out: *mut usize) -> i32 {
    create(Val::I32(v), out)
}
unsafe extern "win64" fn st_u32(_this: usize, v: u32, out: *mut usize) -> i32 {
    create(Val::U32(v), out)
}
unsafe extern "win64" fn st_i64(_this: usize, v: i64, out: *mut usize) -> i32 {
    create(Val::I64(v), out)
}
unsafe extern "win64" fn st_u64(_this: usize, v: u64, out: *mut usize) -> i32 {
    create(Val::U64(v), out)
}
unsafe extern "win64" fn st_single(_this: usize, v: f32, out: *mut usize) -> i32 {
    create(Val::F32(v), out)
}
unsafe extern "win64" fn st_double(_this: usize, v: f64, out: *mut usize) -> i32 {
    create(Val::F64(v), out)
}
unsafe extern "win64" fn st_char16(_this: usize, v: u16, out: *mut usize) -> i32 {
    create(Val::Char16(v), out)
}
unsafe extern "win64" fn st_boolean(_this: usize, v: u8, out: *mut usize) -> i32 {
    create(Val::Bool(v != 0), out)
}
unsafe extern "win64" fn st_string(_this: usize, h: usize, out: *mut usize) -> i32 {
    create(
        Val::Str(super::electron_win32::hstring_slice(h).to_vec()),
        out,
    )
}
/// CreateInspectable : l'objet lui-même, avec une référence de plus.
unsafe extern "win64" fn st_inspectable(_this: usize, obj: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if obj != 0 {
        let add_ref: unsafe extern "win64" fn(usize) -> u32 =
            std::mem::transmute(*(*(obj as *const *const usize)).add(1));
        add_ref(obj);
    }
    *out = obj;
    S_OK
}
/// GUID de 16 octets : passé par adresse (convention x64).
unsafe extern "win64" fn st_guid(_this: usize, g: *const u8, out: *mut usize) -> i32 {
    match read_guid(g) {
        Some(g) => create(Val::Guid(g), out),
        None => E_POINTER,
    }
}
unsafe extern "win64" fn st_datetime(_this: usize, t: i64, out: *mut usize) -> i32 {
    create(Val::DateTime(t), out)
}
unsafe extern "win64" fn st_timespan(_this: usize, t: i64, out: *mut usize) -> i32 {
    create(Val::TimeSpan(t), out)
}
/// Point et Size, 8 octets : passés par valeur dans un registre entier.
unsafe extern "win64" fn st_point(_this: usize, p: u64, out: *mut usize) -> i32 {
    create(Val::Point(pair(p)), out)
}
unsafe extern "win64" fn st_size(_this: usize, p: u64, out: *mut usize) -> i32 {
    create(Val::Size(pair(p)), out)
}
fn pair(p: u64) -> [f32; 2] {
    [f32::from_bits(p as u32), f32::from_bits((p >> 32) as u32)]
}
/// Rect, 16 octets : passé par adresse.
unsafe extern "win64" fn st_rect(_this: usize, r: *const [f32; 4], out: *mut usize) -> i32 {
    if r.is_null() {
        return E_POINTER;
    }
    create(Val::Rect(std::ptr::read_unaligned(r)), out)
}
unsafe extern "win64" fn st_array(_this: usize, _n: u32, _items: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    not_implemented("Create…Array")
}

#[repr(C)]
struct Statics {
    vtbl: &'static VTable<45>,
}
static STATICS: Statics = Statics {
    vtbl: &STATICS_VTBL,
};

unsafe extern "win64" fn st_qi(_this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    if [
        IID_PROPERTY_VALUE_STATICS,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        *out = std::ptr::addr_of!(STATICS) as usize;
        S_OK
    } else {
        *out = 0;
        E_NOINTERFACE
    }
}
extern "win64" fn st_add_ref(_this: usize) -> u32 {
    1
}
extern "win64" fn st_release(_this: usize) -> u32 {
    1
}
unsafe extern "win64" fn st_class(_this: usize, out: *mut usize) -> i32 {
    let w: Vec<u16> = CLASS.encode_utf16().collect();
    hstring(&w, out)
}

static STATICS_VTBL: VTable<45> = VTable([
    st_qi as *const (),
    st_add_ref as *const (),
    st_release as *const (),
    get_iids as *const (),
    st_class as *const (),
    trust_level as *const (),
    st_create_empty as *const (),
    st_u8 as *const (),
    st_i16 as *const (),
    st_u16 as *const (),
    st_i32 as *const (),
    st_u32 as *const (),
    st_i64 as *const (),
    st_u64 as *const (),
    st_single as *const (),
    st_double as *const (),
    st_char16 as *const (),
    st_boolean as *const (),
    st_string as *const (),
    st_inspectable as *const (),
    st_guid as *const (),
    st_datetime as *const (),
    st_timespan as *const (),
    st_point as *const (),
    st_size as *const (),
    st_rect as *const (),
    // CreateUInt8Array … CreateRectArray (19 tableaux).
    st_array as *const (),
    st_array as *const (),
    st_array as *const (),
    st_array as *const (),
    st_array as *const (),
    st_array as *const (),
    st_array as *const (),
    st_array as *const (),
    st_array as *const (),
    st_array as *const (),
    st_array as *const (),
    st_array as *const (),
    st_array as *const (),
    st_array as *const (),
    st_array as *const (),
    st_array as *const (),
    st_array as *const (),
    st_array as *const (),
    st_array as *const (),
]);

// ============================================================
//  Écriture et relecture d'une valeur emballée (#2194)
// ============================================================

/// Base d'un objet emballé par Winlux, depuis son IPropertyValue (+0) ou
/// son IReference<T> (+8) ; `None` pour tout autre objet.
unsafe fn own_boxed(obj: usize) -> Option<usize> {
    if obj == 0 {
        return None;
    }
    let vt = *(obj as *const usize);
    if vt == &VALUE_VTBL as *const _ as usize {
        Some(obj)
    } else if vt == &REFERENCE_VTBL as *const _ as usize {
        Some(obj - 8)
    } else {
        None
    }
}

fn hex_units(units: &[u16]) -> String {
    units.iter().map(|u| format!("{u:04x}")).collect()
}

fn units_of_hex(s: &str) -> Option<Vec<u16>> {
    if !s.len().is_multiple_of(4) || !s.is_ascii() {
        return None;
    }
    (0..s.len())
        .step_by(4)
        .map(|i| u16::from_str_radix(&s[i..i + 4], 16).ok())
        .collect()
}

fn f32s(v: &[f32]) -> String {
    v.iter()
        .map(|f| format!("{:08x}", f.to_bits()))
        .collect::<Vec<_>>()
        .join(",")
}

fn f32s_of(s: &str, n: usize) -> Option<Vec<f32>> {
    let v: Option<Vec<f32>> = s
        .split(',')
        .map(|p| u32::from_str_radix(p, 16).ok().map(f32::from_bits))
        .collect();
    v.filter(|v| v.len() == n)
}

/// Texte d'une valeur emballée par Winlux : `<PropertyType> <contenu>`, sur
/// une ligne, sans espace dans le contenu. `None` pour un objet qui n'est
/// pas une valeur emballée par Winlux (tableau, objet de l'application…).
pub(crate) unsafe fn encode(obj: usize) -> Option<String> {
    let val = &boxed(own_boxed(obj)?).val;
    let body = match val {
        Val::U8(v) => v.to_string(),
        Val::I16(v) => v.to_string(),
        Val::U16(v) => v.to_string(),
        Val::I32(v) => v.to_string(),
        Val::U32(v) => v.to_string(),
        Val::I64(v) => v.to_string(),
        Val::U64(v) => v.to_string(),
        Val::F32(v) => format!("{:08x}", v.to_bits()),
        Val::F64(v) => format!("{:016x}", v.to_bits()),
        Val::Char16(v) => v.to_string(),
        Val::Bool(v) => (*v as u8).to_string(),
        // Une chaîne vide s'écrit « - » pour que la ligne garde trois champs.
        Val::Str(v) if v.is_empty() => "-".to_string(),
        Val::Str(v) => hex_units(v),
        Val::DateTime(v) | Val::TimeSpan(v) => v.to_string(),
        Val::Guid(g) => g.iter().map(|b| format!("{b:02x}")).collect(),
        Val::Point(p) | Val::Size(p) => f32s(p),
        Val::Rect(r) => f32s(r),
    };
    Some(format!("{} {body}", val.property_type()))
}

/// Objet IPropertyValue (une référence) relu depuis le texte d'`encode`.
pub(crate) fn decode(text: &str) -> Option<usize> {
    let (ty, body) = text.split_once(' ')?;
    let val = match ty.parse::<i32>().ok()? {
        1 => Val::U8(body.parse().ok()?),
        2 => Val::I16(body.parse().ok()?),
        3 => Val::U16(body.parse().ok()?),
        4 => Val::I32(body.parse().ok()?),
        5 => Val::U32(body.parse().ok()?),
        6 => Val::I64(body.parse().ok()?),
        7 => Val::U64(body.parse().ok()?),
        8 => Val::F32(f32::from_bits(u32::from_str_radix(body, 16).ok()?)),
        9 => Val::F64(f64::from_bits(u64::from_str_radix(body, 16).ok()?)),
        10 => Val::Char16(body.parse().ok()?),
        11 => Val::Bool(match body {
            "0" => false,
            "1" => true,
            _ => return None,
        }),
        12 if body == "-" => Val::Str(Vec::new()),
        12 => Val::Str(units_of_hex(body)?),
        14 => Val::DateTime(body.parse().ok()?),
        15 => Val::TimeSpan(body.parse().ok()?),
        16 => {
            let b = units_of_hex(body).filter(|u| u.len() == 8)?;
            let mut g = [0u8; 16];
            for (i, u) in b.iter().enumerate() {
                g[2 * i..2 * i + 2].copy_from_slice(&u.to_be_bytes());
            }
            Val::Guid(g)
        }
        17 => {
            let v = f32s_of(body, 2)?;
            Val::Point([v[0], v[1]])
        }
        18 => {
            let v = f32s_of(body, 2)?;
            Val::Size([v[0], v[1]])
        }
        19 => {
            let v = f32s_of(body, 4)?;
            Val::Rect([v[0], v[1], v[2], v[3]])
        }
        _ => return None,
    };
    Some(new_boxed(val))
}

/// `RoGetActivationFactory` pour `Windows.Foundation.PropertyValue`, `None`
/// pour une autre classe. La classe n'a pas de constructeur : sous
/// IActivationFactory, la fabrique n'est pas servie (E_NOINTERFACE).
pub unsafe fn activation_factory(class: &str, iid: *const u8, out: *mut usize) -> Option<i32> {
    if class != CLASS {
        return None;
    }
    let hr = st_qi(0, iid, out);
    if hr == E_NOINTERFACE {
        super::comobj::log_no_interface(class, iid);
    }
    Some(hr)
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn slot<F: Copy>(obj: usize, i: usize) -> F {
        let p = *(*(obj as *const *const usize)).add(i);
        std::mem::transmute_copy(&p)
    }
    unsafe fn release(obj: usize) -> u32 {
        let f: unsafe extern "win64" fn(usize) -> u32 = slot(obj, 2);
        f(obj)
    }

    #[test]
    fn identifiants_d_ireference() {
        // Valeurs publiées : IReference<Int32>, IReference<Boolean>.
        assert_eq!(
            super::super::comobj::guid_str(&reference_iid("i4")),
            "{548CEFBD-BC8A-5FA0-8DF2-957440FC8BF4}"
        );
        assert_eq!(
            super::super::comobj::guid_str(&reference_iid("b1")),
            "{3C00FD60-2950-5939-A21A-2D12C5A01B8A}"
        );
    }

    #[test]
    fn fabrique_et_valeurs() {
        let _teb = crate::process::test_teb();
        unsafe {
            let mut f = 0usize;
            assert_eq!(
                activation_factory(CLASS, IID_PROPERTY_VALUE_STATICS.as_ptr(), &mut f),
                Some(S_OK)
            );
            assert!(
                activation_factory("Windows.Foundation.Uri", IID_IUNKNOWN.as_ptr(), &mut f)
                    .is_none()
            );

            // CreateInt32 puis lecture par IPropertyValue et IReference<Int32>.
            let mk_i32: unsafe extern "win64" fn(usize, i32, *mut usize) -> i32 = slot(f, 10);
            let mut v = 0usize;
            assert_eq!(mk_i32(f, 300, &mut v), S_OK);
            let ty: unsafe extern "win64" fn(usize, *mut i32) -> i32 = slot(v, 6);
            let mut t = 0;
            assert_eq!(ty(v, &mut t), S_OK);
            assert_eq!(t, 4);
            let get_i64: unsafe extern "win64" fn(usize, *mut i64) -> i32 = slot(v, 13);
            let mut x = 0i64;
            assert_eq!(get_i64(v, &mut x), S_OK);
            assert_eq!(x, 300);
            let get_u8: unsafe extern "win64" fn(usize, *mut u8) -> i32 = slot(v, 8);
            let mut b = 0u8;
            assert_eq!(get_u8(v, &mut b), DISP_E_OVERFLOW);
            let get_bool: unsafe extern "win64" fn(usize, *mut u8) -> i32 = slot(v, 18);
            assert_eq!(get_bool(v, &mut b), TYPE_E_TYPEMISMATCH);
            let qi: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32 = slot(v, 0);
            let mut r = 0usize;
            assert_eq!(qi(v, reference_iid("i4").as_ptr(), &mut r), S_OK);
            assert_eq!(r, v + 8);
            let value: unsafe extern "win64" fn(usize, *mut i32) -> i32 = slot(r, 6);
            let mut y = 0i32;
            assert_eq!(value(r, &mut y), S_OK);
            assert_eq!(y, 300);
            assert_eq!(qi(v, reference_iid("b1").as_ptr(), &mut r), E_NOINTERFACE);
            assert_eq!(release(v + 8), 1);
            assert_eq!(release(v), 0);

            // CreateString.
            let mk_str: unsafe extern "win64" fn(usize, usize, *mut usize) -> i32 = slot(f, 18);
            let w: Vec<u16> = "Teams".encode_utf16().collect();
            let mut h = 0usize;
            assert_eq!(hstring(&w, &mut h), S_OK);
            assert_eq!(mk_str(f, h, &mut v), S_OK);
            let get_str: unsafe extern "win64" fn(usize, *mut usize) -> i32 = slot(v, 19);
            let mut h2 = 0usize;
            assert_eq!(get_str(v, &mut h2), S_OK);
            assert_eq!(super::super::electron_win32::hstring_slice(h2), &w[..]);
            assert_eq!(release(v), 0);

            // CreateDouble lu en Int32 (entier exact) puis 2.5 refusé.
            let mk_f64: unsafe extern "win64" fn(usize, f64, *mut usize) -> i32 = slot(f, 15);
            assert_eq!(mk_f64(f, 2.0, &mut v), S_OK);
            let get_i32: unsafe extern "win64" fn(usize, *mut i32) -> i32 = slot(v, 11);
            assert_eq!(get_i32(v, &mut y), S_OK);
            assert_eq!(y, 2);
            release(v);
            assert_eq!(mk_f64(f, 2.5, &mut v), S_OK);
            assert_eq!(get_i32(v, &mut y), TYPE_E_TYPEMISMATCH);
            release(v);

            // Point par valeur, CreateEmpty nul.
            let mk_point: unsafe extern "win64" fn(usize, u64, *mut usize) -> i32 = slot(f, 23);
            let p = (1.5f32.to_bits() as u64) | ((2.5f32.to_bits() as u64) << 32);
            assert_eq!(mk_point(f, p, &mut v), S_OK);
            let get_point: unsafe extern "win64" fn(usize, *mut [f32; 2]) -> i32 = slot(v, 23);
            let mut pt = [0f32; 2];
            assert_eq!(get_point(v, &mut pt), S_OK);
            assert_eq!(pt, [1.5, 2.5]);
            release(v);
            let empty: unsafe extern "win64" fn(usize, *mut usize) -> i32 = slot(f, 6);
            v = 1;
            assert_eq!(empty(f, &mut v), S_OK);
            assert_eq!(v, 0);
        }
    }
}
