//! Classes Windows Runtime de `Windows.Data.Json` (#2171) : JsonObject,
//! JsonArray, JsonValue et JsonError. Spotify crée un JsonObject dès son
//! démarrage ; sans fabrique il lève une exception C++ et se ferme.
//!
//! Sources : documentation publique de Microsoft (espace de noms
//! Windows.Data.Json, JsonErrorStatus ; codes WEB_E_INVALID_JSON_STRING,
//! WEB_E_INVALID_JSON_NUMBER et WEB_E_JSON_VALUE_NOT_FOUND de winerror.h du
//! SDK Windows) ; identifiants d'interface et ordre des méthodes tels que
//! les publie windows-rs (licence MIT, crates/libs/windows/src/Windows/Data/
//! Json/mod.rs et crates/libs/collections/src/bindings.rs) ; calcul des
//! identifiants des interfaces paramétrées (IMap, IVector, IIterable…)
//! selon le système de types publié de Windows Runtime (UUID v5, SHA-1).
//!
//! Un même objet `Node` porte toutes les interfaces d'une valeur :
//! IJsonValue, IStringable et, pour un objet ou un tableau, IJsonObject ou
//! IJsonArray, IMap ou IVector, IIterable. Les enfants sont des `Node`
//! retenus par une référence. Une valeur IJsonValue venue d'ailleurs est
//! recopiée (Stringify puis analyse).
//!
//! Pas encore servis (E_NOTIMPL, journalisé une fois) : IMap.GetView et
//! IVector.GetView.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use super::comobj::{
    guid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, IID_IUNKNOWN, S_OK,
};

const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_BOUNDS: i32 = 0x8000_000Bu32 as i32;
const E_ILLEGAL_METHOD_CALL: i32 = 0x8000_000Eu32 as i32;
pub const WEB_E_INVALID_JSON_STRING: i32 = 0x8375_0007u32 as i32;
pub const WEB_E_INVALID_JSON_NUMBER: i32 = 0x8375_0008u32 as i32;
pub const WEB_E_JSON_VALUE_NOT_FOUND: i32 = 0x8375_0009u32 as i32;

const fn g(d1: u32, d2: u16, d3: u16, d4: u64) -> Guid {
    guid(d1, d2, d3, d4.to_be_bytes())
}

const IID_IINSPECTABLE: Guid = g(0xAF86_E2E0, 0xB12D, 0x4C6A, 0x9C5A_D7AA_6510_1E90);
const IID_IAGILEOBJECT: Guid = g(0x94EA_2B94, 0xE9CC, 0x49E0, 0xC0FF_EE64_CA8F_5B90);
const IID_IACTIVATION_FACTORY: Guid = g(0x0000_0035, 0, 0, 0xC000_0000_0000_0046);
const IID_ISTRINGABLE: Guid = g(0x9636_9f54, 0x8eb6, 0x48f0, 0xabce_c1b2_11e6_27c3);
const IID_JSON_VALUE: Guid = g(0xa321_9ecb, 0xf0b3, 0x4dcd, 0xbeee_19d4_8cd3_ed1e);
const IID_JSON_OBJECT: Guid = g(0x064e_24dd, 0x29c2, 0x4f83, 0x9ac1_9ee1_1578_beb3);
const IID_JSON_ARRAY: Guid = g(0x08c1_ddb6, 0x0cbd, 0x4a9a, 0xb5d3_2f85_2dc3_7e81);
const IID_JSON_OBJECT_DEFAULTS: Guid = g(0xd960_d2a2, 0xb7f0, 0x4f00, 0x8e44_d82c_f415_ea13);
const IID_JSON_OBJECT_STATICS: Guid = g(0x2289_f159, 0x54de, 0x45d8, 0xabcc_2260_3fa0_66a0);
const IID_JSON_ARRAY_STATICS: Guid = g(0xdb14_34a9, 0xe164, 0x499f, 0x93e2_8a8f_49bb_90ba);
const IID_JSON_VALUE_STATICS: Guid = g(0x5f6b_544a, 0x2f53, 0x48e1, 0x91a3_f78b_50a6_345c);
const IID_JSON_VALUE_STATICS2: Guid = g(0x1d9e_cbe4, 0x3fe8, 0x4335, 0x8392_93d8_e368_65f0);
const IID_JSON_ERROR_STATICS2: Guid = g(0x4040_30da, 0x87d0, 0x436c, 0x83ab_fc7b_12c0_cc26);

const CLASS_OBJECT: &str = "Windows.Data.Json.JsonObject";
const CLASS_ARRAY: &str = "Windows.Data.Json.JsonArray";
const CLASS_VALUE: &str = "Windows.Data.Json.JsonValue";
const CLASS_ERROR: &str = "Windows.Data.Json.JsonError";

/// Profondeur d'imbrication acceptée à l'analyse et à l'écriture.
const MAX_DEPTH: usize = 512;

// ============================================================
//  Identifiants des interfaces paramétrées
// ============================================================

/// Identifiant d'une interface paramétrée : UUID v5 (SHA-1) de sa
/// signature dans l'espace de noms {11f47ad5-7b73-42c0-abae-878b1e16adee}.
pub fn pinterface_iid(signature: &str) -> Guid {
    const NS: [u8; 16] = [
        0x11, 0xf4, 0x7a, 0xd5, 0x7b, 0x73, 0x42, 0xc0, 0xab, 0xae, 0x87, 0x8b, 0x1e, 0x16, 0xad,
        0xee,
    ];
    let h = super::bcrypt::crypto::sha1(&[&NS, signature.as_bytes()]);
    let d1 = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
    let d2 = u16::from_be_bytes([h[4], h[5]]);
    let d3 = (u16::from_be_bytes([h[6], h[7]]) & 0x0fff) | 0x5000;
    let mut d4 = [0u8; 8];
    d4.copy_from_slice(&h[8..16]);
    d4[0] = (d4[0] & 0x3f) | 0x80;
    guid(d1, d2, d3, d4)
}

struct Generic {
    map: Guid,
    iterable_kvp: Guid,
    iterator_kvp: Guid,
    kvp: Guid,
    vector: Guid,
    iterable_value: Guid,
    iterator_value: Guid,
}

fn generic() -> &'static Generic {
    static G: OnceLock<Generic> = OnceLock::new();
    G.get_or_init(|| {
        let value = "{a3219ecb-f0b3-4dcd-beee-19d48cd3ed1e}";
        let kvp = format!("pinterface({{02b51929-c1c4-4a7e-8940-0312b5c18500}};string;{value})");
        Generic {
            map: pinterface_iid(&format!(
                "pinterface({{3c2925fe-8519-45c1-aa79-197b6718c1c1}};string;{value})"
            )),
            iterable_kvp: pinterface_iid(&format!(
                "pinterface({{faa585ea-6214-4217-afda-7f46de5869b3}};{kvp})"
            )),
            iterator_kvp: pinterface_iid(&format!(
                "pinterface({{6a79e863-4300-459a-9966-cbb660963ee1}};{kvp})"
            )),
            kvp: pinterface_iid(&kvp),
            vector: pinterface_iid(&format!(
                "pinterface({{913337e9-11a1-4345-a3a2-4e7f956e222d}};{value})"
            )),
            iterable_value: pinterface_iid(&format!(
                "pinterface({{faa585ea-6214-4217-afda-7f46de5869b3}};{value})"
            )),
            iterator_value: pinterface_iid(&format!(
                "pinterface({{6a79e863-4300-459a-9966-cbb660963ee1}};{value})"
            )),
        }
    })
}

// ============================================================
//  Outils
// ============================================================

fn not_implemented(method: &str) -> i32 {
    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if !seen.iter().any(|m| m == method) {
        seen.push(method.to_string());
        crate::log_warn!(
            "com",
            "Windows.Data.Json : {method} pas encore servie (E_NOTIMPL)"
        );
    }
    E_NOTIMPL
}

unsafe fn hstring(text: &str, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = text.encode_utf16().collect();
    super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, out)
}

unsafe fn read_hstring(h: usize) -> String {
    String::from_utf16_lossy(super::electron_win32::hstring_slice(h))
}

unsafe fn com_call_release(p: usize) {
    let f: unsafe extern "win64" fn(usize) -> u32 =
        std::mem::transmute(*(*(p as *const *const usize)).add(2));
    f(p);
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
    *level = 0; // BaseTrust
    S_OK
}

// ============================================================
//  Valeurs
// ============================================================

/// JsonValueType.
const TYPE_NULL: i32 = 0;
const TYPE_BOOLEAN: i32 = 1;
const TYPE_NUMBER: i32 = 2;
const TYPE_STRING: i32 = 3;
const TYPE_ARRAY: i32 = 4;
const TYPE_OBJECT: i32 = 5;

enum Data {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    /// Enfants : adresses de `Node`, une référence chacun.
    Arr(Mutex<Vec<usize>>),
    Obj(Mutex<Vec<(String, usize)>>),
}

/// Positions des tables dans un `Node`.
const OFF_VALUE: usize = 0;
const OFF_KIND: usize = 8;
const OFF_COLL: usize = 16;
const OFF_ITER: usize = 24;
const OFF_STR: usize = 32;
const OFF_DFLT: usize = 40;

#[repr(C)]
struct Node {
    vt_value: *const (),
    vt_kind: *const (),
    vt_coll: *const (),
    vt_iter: *const (),
    vt_str: *const (),
    vt_dflt: *const (),
    refs: AtomicU32,
    data: Data,
}

impl Drop for Node {
    fn drop(&mut self) {
        let children: Vec<usize> = match &self.data {
            Data::Arr(v) => std::mem::take(&mut *v.lock().unwrap_or_else(|e| e.into_inner())),
            Data::Obj(v) => std::mem::take(&mut *v.lock().unwrap_or_else(|e| e.into_inner()))
                .into_iter()
                .map(|(_, c)| c)
                .collect(),
            _ => Vec::new(),
        };
        for c in children {
            unsafe { node_release(c) };
        }
    }
}

fn vt<const N: usize>(t: &'static VTable<N>) -> *const () {
    t as *const VTable<N> as *const ()
}

fn new_node(data: Data) -> usize {
    let (kind, coll, iter, dflt) = match data {
        Data::Obj(_) => (
            vt(&OBJECT_VTBL),
            vt(&MAP_VTBL),
            vt(&ITERABLE_KVP_VTBL),
            vt(&DEFAULTS_VTBL),
        ),
        Data::Arr(_) => (
            vt(&ARRAY_VTBL),
            vt(&VECTOR_VTBL),
            vt(&ITERABLE_VALUE_VTBL),
            std::ptr::null(),
        ),
        _ => (
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        ),
    };
    Box::into_raw(Box::new(Node {
        vt_value: vt(&VALUE_VTBL),
        vt_kind: kind,
        vt_coll: coll,
        vt_iter: iter,
        vt_str: vt(&STRINGABLE_VTBL),
        vt_dflt: dflt,
        refs: AtomicU32::new(1),
        data,
    })) as usize
}

unsafe fn node<'a>(base: usize) -> &'a Node {
    &*(base as *const Node)
}

unsafe fn node_add_ref(base: usize) -> u32 {
    node(base).refs.fetch_add(1, Ordering::SeqCst) + 1
}

unsafe fn node_release(base: usize) -> u32 {
    let n = node(base).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        drop(Box::from_raw(base as *mut Node));
    }
    n
}

fn value_type(base: usize) -> i32 {
    match unsafe { &node(base).data } {
        Data::Null => TYPE_NULL,
        Data::Bool(_) => TYPE_BOOLEAN,
        Data::Num(_) => TYPE_NUMBER,
        Data::Str(_) => TYPE_STRING,
        Data::Arr(_) => TYPE_ARRAY,
        Data::Obj(_) => TYPE_OBJECT,
    }
}

/// Interface par défaut d'une valeur : IJsonObject ou IJsonArray pour un
/// objet ou un tableau, IJsonValue sinon.
fn default_interface(base: usize) -> usize {
    match value_type(base) {
        TYPE_ARRAY | TYPE_OBJECT => base + OFF_KIND,
        _ => base + OFF_VALUE,
    }
}

unsafe fn node_qi(base: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    let t = value_type(base);
    let gen = generic();
    let obj = t == TYPE_OBJECT;
    let arr = t == TYPE_ARRAY;
    let p = if [IID_IUNKNOWN, IID_IINSPECTABLE, IID_IAGILEOBJECT].contains(&iid) {
        default_interface(base)
    } else if iid == IID_JSON_VALUE {
        base + OFF_VALUE
    } else if iid == IID_ISTRINGABLE {
        base + OFF_STR
    } else if (obj && iid == IID_JSON_OBJECT) || (arr && iid == IID_JSON_ARRAY) {
        base + OFF_KIND
    } else if (obj && iid == gen.map) || (arr && iid == gen.vector) {
        base + OFF_COLL
    } else if (obj && iid == gen.iterable_kvp) || (arr && iid == gen.iterable_value) {
        base + OFF_ITER
    } else if obj && iid == IID_JSON_OBJECT_DEFAULTS {
        base + OFF_DFLT
    } else {
        *out = 0;
        return E_NOINTERFACE;
    };
    node_add_ref(base);
    *out = p;
    S_OK
}

fn class_of(base: usize) -> &'static str {
    match value_type(base) {
        TYPE_OBJECT => CLASS_OBJECT,
        TYPE_ARRAY => CLASS_ARRAY,
        _ => CLASS_VALUE,
    }
}

unsafe extern "win64" fn n_qi<const OFF: usize>(
    this: usize,
    iid: *const u8,
    out: *mut usize,
) -> i32 {
    node_qi(this - OFF, iid, out)
}
unsafe extern "win64" fn n_add_ref<const OFF: usize>(this: usize) -> u32 {
    node_add_ref(this - OFF)
}
unsafe extern "win64" fn n_release<const OFF: usize>(this: usize) -> u32 {
    node_release(this - OFF)
}
unsafe extern "win64" fn n_class<const OFF: usize>(this: usize, out: *mut usize) -> i32 {
    hstring(class_of(this - OFF), out)
}

/// Table d'une interface d'un `Node` : les six méthodes d'IInspectable
/// décalées de `$off`, puis les méthodes propres.
macro_rules! node_vtbl {
    ($off:expr; $($f:expr),* $(,)?) => {
        VTable([
            n_qi::<{ $off }> as *const (),
            n_add_ref::<{ $off }> as *const (),
            n_release::<{ $off }> as *const (),
            get_iids as *const (),
            n_class::<{ $off }> as *const (),
            trust_level as *const (),
            $($f as *const ()),*
        ])
    };
}

/// Rend `base` avec une référence de plus, sous l'interface voulue.
unsafe fn give(base: usize, off: usize, out: *mut usize) -> i32 {
    node_add_ref(base);
    *out = base + off;
    S_OK
}

/// Prend une valeur IJsonValue reçue d'un appelant : notre `Node` avec une
/// référence de plus, ou une copie d'une valeur venue d'ailleurs.
unsafe fn adopt(p: usize) -> Result<usize, i32> {
    if p == 0 {
        return Err(E_INVALIDARG);
    }
    let qi: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32 =
        std::mem::transmute(**(p as *const *const usize));
    let mut v = 0usize;
    let hr = qi(p, IID_JSON_VALUE.as_ptr(), &mut v);
    if hr < 0 || v == 0 {
        return Err(if hr < 0 { hr } else { E_NOINTERFACE });
    }
    if *(v as *const *const ()) == vt(&VALUE_VTBL) {
        return Ok(v - OFF_VALUE);
    }
    // IJsonValue d'une autre implémentation : recopiée.
    let stringify: unsafe extern "win64" fn(usize, *mut usize) -> i32 =
        std::mem::transmute(*(*(v as *const *const usize)).add(7));
    let mut h = 0usize;
    let hr = stringify(v, &mut h);
    com_call_release(v);
    if hr < 0 {
        return Err(hr);
    }
    let text = super::electron_win32::hstring_slice(h).to_vec();
    super::electron_win32::WindowsDeleteString(h);
    parse(&text)
}

// ============================================================
//  Analyse et écriture
// ============================================================

struct Parser<'a> {
    s: &'a [u16],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u16> {
        self.s.get(self.i).copied()
    }
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(0x20 | 0x09 | 0x0a | 0x0d)) {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c as u16) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn literal(&mut self, word: &str) -> bool {
        let w: Vec<u16> = word.encode_utf16().collect();
        if self.s[self.i..].starts_with(&w) {
            self.i += w.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self, depth: usize) -> Result<Data, i32> {
        if depth > MAX_DEPTH {
            return Err(WEB_E_INVALID_JSON_STRING);
        }
        self.skip_ws();
        match self.peek() {
            Some(0x7b) => self.object(depth),
            Some(0x5b) => self.array(depth),
            Some(0x22) => self.string().map(Data::Str),
            Some(c) if c == b'-' as u16 || (b'0' as u16..=b'9' as u16).contains(&c) => {
                self.number().map(Data::Num)
            }
            _ if self.literal("true") => Ok(Data::Bool(true)),
            _ if self.literal("false") => Ok(Data::Bool(false)),
            _ if self.literal("null") => Ok(Data::Null),
            _ => Err(WEB_E_INVALID_JSON_STRING),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Data, i32> {
        self.i += 1;
        let mut members: Vec<(String, usize)> = Vec::new();
        let release_all = |m: Vec<(String, usize)>| {
            for (_, c) in m {
                unsafe { node_release(c) };
            }
        };
        self.skip_ws();
        if self.eat(b'}') {
            return Ok(Data::Obj(Mutex::new(members)));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"' as u16) {
                release_all(members);
                return Err(WEB_E_INVALID_JSON_STRING);
            }
            let key = match self.string() {
                Ok(k) => k,
                Err(e) => {
                    release_all(members);
                    return Err(e);
                }
            };
            self.skip_ws();
            if !self.eat(b':') {
                release_all(members);
                return Err(WEB_E_INVALID_JSON_STRING);
            }
            let v = match self.value(depth + 1) {
                Ok(d) => new_node(d),
                Err(e) => {
                    release_all(members);
                    return Err(e);
                }
            };
            // Une clé répétée : la dernière l'emporte.
            if let Some(slot) = members.iter_mut().find(|(k, _)| *k == key) {
                unsafe { node_release(slot.1) };
                slot.1 = v;
            } else {
                members.push((key, v));
            }
            self.skip_ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b'}') {
                return Ok(Data::Obj(Mutex::new(members)));
            }
            release_all(members);
            return Err(WEB_E_INVALID_JSON_STRING);
        }
    }

    fn array(&mut self, depth: usize) -> Result<Data, i32> {
        self.i += 1;
        let mut items: Vec<usize> = Vec::new();
        let release_all = |m: Vec<usize>| {
            for c in m {
                unsafe { node_release(c) };
            }
        };
        self.skip_ws();
        if self.eat(b']') {
            return Ok(Data::Arr(Mutex::new(items)));
        }
        loop {
            match self.value(depth + 1) {
                Ok(d) => items.push(new_node(d)),
                Err(e) => {
                    release_all(items);
                    return Err(e);
                }
            }
            self.skip_ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b']') {
                return Ok(Data::Arr(Mutex::new(items)));
            }
            release_all(items);
            return Err(WEB_E_INVALID_JSON_STRING);
        }
    }

    fn hex4(&mut self) -> Result<u16, i32> {
        let mut v = 0u16;
        for _ in 0..4 {
            let c = self.peek().ok_or(WEB_E_INVALID_JSON_STRING)?;
            let d = char::from_u32(c as u32)
                .and_then(|c| c.to_digit(16))
                .ok_or(WEB_E_INVALID_JSON_STRING)?;
            v = v * 16 + d as u16;
            self.i += 1;
        }
        Ok(v)
    }

    fn string(&mut self) -> Result<String, i32> {
        self.i += 1; // guillemet ouvrant
        let mut units: Vec<u16> = Vec::new();
        loop {
            let c = self.peek().ok_or(WEB_E_INVALID_JSON_STRING)?;
            self.i += 1;
            match c {
                0x22 => break,
                0x5c => {
                    let e = self.peek().ok_or(WEB_E_INVALID_JSON_STRING)?;
                    self.i += 1;
                    units.push(match e {
                        0x22 => 0x22,
                        0x5c => 0x5c,
                        0x2f => 0x2f,
                        0x62 => 0x08,
                        0x66 => 0x0c,
                        0x6e => 0x0a,
                        0x72 => 0x0d,
                        0x74 => 0x09,
                        0x75 => self.hex4()?,
                        _ => return Err(WEB_E_INVALID_JSON_STRING),
                    });
                }
                c if c < 0x20 => return Err(WEB_E_INVALID_JSON_STRING),
                c => units.push(c),
            }
        }
        Ok(String::from_utf16_lossy(&units))
    }

    fn number(&mut self) -> Result<f64, i32> {
        let start = self.i;
        let digit = |c: Option<u16>| matches!(c, Some(d) if (0x30..=0x39).contains(&d));
        self.eat(b'-');
        if self.eat(b'0') {
            if digit(self.peek()) {
                return Err(WEB_E_INVALID_JSON_NUMBER);
            }
        } else if digit(self.peek()) {
            while digit(self.peek()) {
                self.i += 1;
            }
        } else {
            return Err(WEB_E_INVALID_JSON_NUMBER);
        }
        if self.eat(b'.') {
            if !digit(self.peek()) {
                return Err(WEB_E_INVALID_JSON_NUMBER);
            }
            while digit(self.peek()) {
                self.i += 1;
            }
        }
        if self.eat(b'e') || self.eat(b'E') {
            if !self.eat(b'+') {
                self.eat(b'-');
            }
            if !digit(self.peek()) {
                return Err(WEB_E_INVALID_JSON_NUMBER);
            }
            while digit(self.peek()) {
                self.i += 1;
            }
        }
        let text = String::from_utf16_lossy(&self.s[start..self.i]);
        match text.parse::<f64>() {
            Ok(v) if v.is_finite() => Ok(v),
            _ => Err(WEB_E_INVALID_JSON_NUMBER),
        }
    }
}

/// Analyse un texte JSON complet en un nouveau `Node` (une référence).
fn parse(text: &[u16]) -> Result<usize, i32> {
    let mut p = Parser { s: text, i: 0 };
    let data = p.value(0)?;
    p.skip_ws();
    let base = new_node(data);
    if p.i != text.len() {
        unsafe { node_release(base) };
        return Err(WEB_E_INVALID_JSON_STRING);
    }
    Ok(base)
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Nombre écrit au plus court, comme Number.toString de JavaScript.
pub fn format_number(v: f64) -> String {
    if !v.is_finite() {
        return "null".into();
    }
    if v == 0.0 {
        return "0".into();
    }
    let a = v.abs();
    if !(1e-6..1e21).contains(&a) {
        let s = format!("{v:e}");
        match s.split_once('e') {
            Some((m, e)) if !e.starts_with('-') => format!("{m}e+{e}"),
            _ => s,
        }
    } else {
        format!("{v}")
    }
}

unsafe fn stringify(base: usize, out: &mut String, depth: usize) -> Result<(), i32> {
    if depth > MAX_DEPTH {
        return Err(E_ILLEGAL_METHOD_CALL);
    }
    match &node(base).data {
        Data::Null => out.push_str("null"),
        Data::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Data::Num(n) => out.push_str(&format_number(*n)),
        Data::Str(s) => write_string(s, out),
        Data::Arr(v) => {
            let items = v.lock().unwrap_or_else(|e| e.into_inner()).clone();
            out.push('[');
            for (i, c) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                stringify(*c, out, depth + 1)?;
            }
            out.push(']');
        }
        Data::Obj(v) => {
            let members: Vec<(String, usize)> = v.lock().unwrap_or_else(|e| e.into_inner()).clone();
            out.push('{');
            for (i, (k, c)) in members.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(k, out);
                out.push(':');
                stringify(*c, out, depth + 1)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

unsafe fn stringify_to(base: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let mut s = String::new();
    if let Err(e) = stringify(base, &mut s, 0) {
        *out = 0;
        return e;
    }
    hstring(&s, out)
}

// ============================================================
//  IJsonValue et IStringable
// ============================================================

unsafe extern "win64" fn v_value_type(this: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = value_type(this - OFF_VALUE);
    S_OK
}
unsafe extern "win64" fn v_stringify(this: usize, out: *mut usize) -> i32 {
    stringify_to(this - OFF_VALUE, out)
}
unsafe extern "win64" fn s_to_string(this: usize, out: *mut usize) -> i32 {
    stringify_to(this - OFF_STR, out)
}

/// Accès typés communs à IJsonValue, IJsonObject et IJsonArray.
unsafe fn get_string(base: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match &node(base).data {
        Data::Str(s) => hstring(s, out),
        _ => {
            *out = 0;
            E_ILLEGAL_METHOD_CALL
        }
    }
}
unsafe fn get_number(base: usize, out: *mut f64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match node(base).data {
        Data::Num(n) => {
            *out = n;
            S_OK
        }
        _ => E_ILLEGAL_METHOD_CALL,
    }
}
unsafe fn get_boolean(base: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match node(base).data {
        Data::Bool(b) => {
            *out = u8::from(b);
            S_OK
        }
        _ => E_ILLEGAL_METHOD_CALL,
    }
}
/// GetObject / GetArray : l'interface IJsonObject ou IJsonArray.
unsafe fn get_container(base: usize, want: i32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if value_type(base) != want {
        *out = 0;
        return E_ILLEGAL_METHOD_CALL;
    }
    give(base, OFF_KIND, out)
}

unsafe extern "win64" fn v_get_string(this: usize, out: *mut usize) -> i32 {
    get_string(this - OFF_VALUE, out)
}
unsafe extern "win64" fn v_get_number(this: usize, out: *mut f64) -> i32 {
    get_number(this - OFF_VALUE, out)
}
unsafe extern "win64" fn v_get_boolean(this: usize, out: *mut u8) -> i32 {
    get_boolean(this - OFF_VALUE, out)
}
unsafe extern "win64" fn v_get_array(this: usize, out: *mut usize) -> i32 {
    get_container(this - OFF_VALUE, TYPE_ARRAY, out)
}
unsafe extern "win64" fn v_get_object(this: usize, out: *mut usize) -> i32 {
    get_container(this - OFF_VALUE, TYPE_OBJECT, out)
}

static VALUE_VTBL: VTable<13> = node_vtbl!(OFF_VALUE;
    v_value_type,
    v_stringify,
    v_get_string,
    v_get_number,
    v_get_boolean,
    v_get_array,
    v_get_object,
);

static STRINGABLE_VTBL: VTable<7> = node_vtbl!(OFF_STR; s_to_string);

// ============================================================
//  JsonObject : IJsonObject, IMap, IIterable, valeurs par défaut
// ============================================================

unsafe fn members<'a>(base: usize) -> std::sync::MutexGuard<'a, Vec<(String, usize)>> {
    match &node(base).data {
        Data::Obj(m) => m.lock().unwrap_or_else(|e| e.into_inner()),
        _ => unreachable!("IJsonObject sur une valeur qui n'est pas un objet"),
    }
}

/// Enfant nommé, avec une référence de plus.
unsafe fn named(base: usize, key: usize) -> Option<usize> {
    let key = read_hstring(key);
    let m = members(base);
    let c = m.iter().find(|(k, _)| *k == key).map(|(_, c)| *c)?;
    node_add_ref(c);
    Some(c)
}

/// Remplace ou ajoute un membre ; rend vrai si la clé existait.
unsafe fn insert_member(base: usize, key: String, child: usize) -> bool {
    let old = {
        let mut m = members(base);
        if let Some(slot) = m.iter_mut().find(|(k, _)| *k == key) {
            Some(std::mem::replace(&mut slot.1, child))
        } else {
            m.push((key, child));
            None
        }
    };
    if let Some(o) = old {
        node_release(o);
        true
    } else {
        false
    }
}

/// GetNamedXxx : `f` lit l'enfant ; absent → WEB_E_JSON_VALUE_NOT_FOUND.
unsafe fn with_named(base: usize, key: usize, f: impl FnOnce(usize) -> i32) -> i32 {
    match named(base, key) {
        Some(c) => {
            let hr = f(c);
            node_release(c);
            hr
        }
        None => WEB_E_JSON_VALUE_NOT_FOUND,
    }
}

unsafe extern "win64" fn o_get_named_value(this: usize, key: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    with_named(this - OFF_KIND, key, |c| give(c, OFF_VALUE, out))
}
unsafe extern "win64" fn o_set_named_value(this: usize, key: usize, value: usize) -> i32 {
    match adopt(value) {
        Ok(c) => {
            insert_member(this - OFF_KIND, read_hstring(key), c);
            S_OK
        }
        Err(e) => e,
    }
}
unsafe extern "win64" fn o_get_named_object(this: usize, key: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    with_named(this - OFF_KIND, key, |c| get_container(c, TYPE_OBJECT, out))
}
unsafe extern "win64" fn o_get_named_array(this: usize, key: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    with_named(this - OFF_KIND, key, |c| get_container(c, TYPE_ARRAY, out))
}
unsafe extern "win64" fn o_get_named_string(this: usize, key: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    with_named(this - OFF_KIND, key, |c| get_string(c, out))
}
unsafe extern "win64" fn o_get_named_number(this: usize, key: usize, out: *mut f64) -> i32 {
    with_named(this - OFF_KIND, key, |c| get_number(c, out))
}
unsafe extern "win64" fn o_get_named_boolean(this: usize, key: usize, out: *mut u8) -> i32 {
    with_named(this - OFF_KIND, key, |c| get_boolean(c, out))
}

static OBJECT_VTBL: VTable<13> = node_vtbl!(OFF_KIND;
    o_get_named_value,
    o_set_named_value,
    o_get_named_object,
    o_get_named_array,
    o_get_named_string,
    o_get_named_number,
    o_get_named_boolean,
);

/// IJsonObjectWithDefaultValues : la valeur par défaut seulement si le nom
/// manque.
unsafe extern "win64" fn d_value(this: usize, key: usize, dflt: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match named(this - OFF_DFLT, key) {
        Some(c) => {
            *out = c + OFF_VALUE;
            S_OK
        }
        None => pass_through(dflt, out),
    }
}
unsafe extern "win64" fn d_object(this: usize, key: usize, dflt: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match named(this - OFF_DFLT, key) {
        Some(c) => {
            let hr = get_container(c, TYPE_OBJECT, out);
            node_release(c);
            hr
        }
        None => pass_through(dflt, out),
    }
}
unsafe extern "win64" fn d_string(this: usize, key: usize, dflt: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match named(this - OFF_DFLT, key) {
        Some(c) => {
            let hr = get_string(c, out);
            node_release(c);
            hr
        }
        None => super::electron_win32::WindowsDuplicateString(dflt, out),
    }
}
unsafe extern "win64" fn d_array(this: usize, key: usize, dflt: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match named(this - OFF_DFLT, key) {
        Some(c) => {
            let hr = get_container(c, TYPE_ARRAY, out);
            node_release(c);
            hr
        }
        None => pass_through(dflt, out),
    }
}
unsafe extern "win64" fn d_number(this: usize, key: usize, dflt: f64, out: *mut f64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match named(this - OFF_DFLT, key) {
        Some(c) => {
            let hr = get_number(c, out);
            node_release(c);
            hr
        }
        None => {
            *out = dflt;
            S_OK
        }
    }
}
unsafe extern "win64" fn d_boolean(this: usize, key: usize, dflt: u8, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match named(this - OFF_DFLT, key) {
        Some(c) => {
            let hr = get_boolean(c, out);
            node_release(c);
            hr
        }
        None => {
            *out = u8::from(dflt != 0);
            S_OK
        }
    }
}

/// Rend l'objet par défaut fourni par l'appelant (peut être nul).
unsafe fn pass_through(p: usize, out: *mut usize) -> i32 {
    if p != 0 {
        let add_ref: unsafe extern "win64" fn(usize) -> u32 =
            std::mem::transmute(*(*(p as *const *const usize)).add(1));
        add_ref(p);
    }
    *out = p;
    S_OK
}

static DEFAULTS_VTBL: VTable<12> = node_vtbl!(OFF_DFLT;
    d_value,
    d_object,
    d_string,
    d_array,
    d_number,
    d_boolean,
);

unsafe extern "win64" fn m_lookup(this: usize, key: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match named(this - OFF_COLL, key) {
        Some(c) => {
            *out = c + OFF_VALUE;
            S_OK
        }
        None => {
            *out = 0;
            E_BOUNDS
        }
    }
}
unsafe extern "win64" fn m_size(this: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = members(this - OFF_COLL).len() as u32;
    S_OK
}
unsafe extern "win64" fn m_has_key(this: usize, key: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let found = named(this - OFF_COLL, key);
    if let Some(c) = found {
        node_release(c);
    }
    *out = u8::from(found.is_some());
    S_OK
}
unsafe extern "win64" fn m_get_view(_this: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    not_implemented("JsonObject.GetView")
}
unsafe extern "win64" fn m_insert(this: usize, key: usize, value: usize, replaced: *mut u8) -> i32 {
    match adopt(value) {
        Ok(c) => {
            let r = insert_member(this - OFF_COLL, read_hstring(key), c);
            if !replaced.is_null() {
                *replaced = u8::from(r);
            }
            S_OK
        }
        Err(e) => e,
    }
}
unsafe extern "win64" fn m_remove(this: usize, key: usize) -> i32 {
    let key = read_hstring(key);
    let removed = {
        let mut m = members(this - OFF_COLL);
        m.iter().position(|(k, _)| *k == key).map(|i| m.remove(i).1)
    };
    match removed {
        Some(c) => {
            node_release(c);
            S_OK
        }
        None => E_BOUNDS,
    }
}
unsafe extern "win64" fn m_clear(this: usize) -> i32 {
    let old = std::mem::take(&mut *members(this - OFF_COLL));
    for (_, c) in old {
        node_release(c);
    }
    S_OK
}

static MAP_VTBL: VTable<13> = node_vtbl!(OFF_COLL;
    m_lookup,
    m_size,
    m_has_key,
    m_get_view,
    m_insert,
    m_remove,
    m_clear,
);

// ============================================================
//  JsonArray : IJsonArray, IVector, IIterable
// ============================================================

unsafe fn items<'a>(base: usize) -> std::sync::MutexGuard<'a, Vec<usize>> {
    match &node(base).data {
        Data::Arr(v) => v.lock().unwrap_or_else(|e| e.into_inner()),
        _ => unreachable!("IJsonArray sur une valeur qui n'est pas un tableau"),
    }
}

/// Élément `i`, avec une référence de plus.
unsafe fn item_at(base: usize, i: u32) -> Option<usize> {
    let v = items(base);
    let c = *v.get(i as usize)?;
    node_add_ref(c);
    Some(c)
}

unsafe fn with_item(base: usize, i: u32, f: impl FnOnce(usize) -> i32) -> i32 {
    match item_at(base, i) {
        Some(c) => {
            let hr = f(c);
            node_release(c);
            hr
        }
        None => E_BOUNDS,
    }
}

unsafe extern "win64" fn a_get_object_at(this: usize, i: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    with_item(this - OFF_KIND, i, |c| get_container(c, TYPE_OBJECT, out))
}
unsafe extern "win64" fn a_get_array_at(this: usize, i: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    with_item(this - OFF_KIND, i, |c| get_container(c, TYPE_ARRAY, out))
}
unsafe extern "win64" fn a_get_string_at(this: usize, i: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    with_item(this - OFF_KIND, i, |c| get_string(c, out))
}
unsafe extern "win64" fn a_get_number_at(this: usize, i: u32, out: *mut f64) -> i32 {
    with_item(this - OFF_KIND, i, |c| get_number(c, out))
}
unsafe extern "win64" fn a_get_boolean_at(this: usize, i: u32, out: *mut u8) -> i32 {
    with_item(this - OFF_KIND, i, |c| get_boolean(c, out))
}

static ARRAY_VTBL: VTable<11> = node_vtbl!(OFF_KIND;
    a_get_object_at,
    a_get_array_at,
    a_get_string_at,
    a_get_number_at,
    a_get_boolean_at,
);

unsafe extern "win64" fn vec_get_at(this: usize, i: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match item_at(this - OFF_COLL, i) {
        Some(c) => {
            *out = c + OFF_VALUE;
            S_OK
        }
        None => {
            *out = 0;
            E_BOUNDS
        }
    }
}
unsafe extern "win64" fn vec_size(this: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = items(this - OFF_COLL).len() as u32;
    S_OK
}
unsafe extern "win64" fn vec_get_view(_this: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    not_implemented("JsonArray.GetView")
}
unsafe extern "win64" fn vec_index_of(
    this: usize,
    value: usize,
    index: *mut u32,
    found: *mut u8,
) -> i32 {
    if index.is_null() || found.is_null() {
        return E_POINTER;
    }
    *index = 0;
    *found = 0;
    if value == 0 {
        return S_OK;
    }
    let qi: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32 =
        std::mem::transmute(**(value as *const *const usize));
    let mut v = 0usize;
    if qi(value, IID_JSON_VALUE.as_ptr(), &mut v) < 0 || v == 0 {
        return S_OK;
    }
    if let Some(i) = items(this - OFF_COLL)
        .iter()
        .position(|&c| c + OFF_VALUE == v)
    {
        *index = i as u32;
        *found = 1;
    }
    com_call_release(v);
    S_OK
}
unsafe extern "win64" fn vec_set_at(this: usize, i: u32, value: usize) -> i32 {
    let c = match adopt(value) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let old = {
        let mut v = items(this - OFF_COLL);
        match v.get_mut(i as usize) {
            Some(slot) => std::mem::replace(slot, c),
            None => {
                drop(v);
                node_release(c);
                return E_BOUNDS;
            }
        }
    };
    node_release(old);
    S_OK
}
unsafe extern "win64" fn vec_insert_at(this: usize, i: u32, value: usize) -> i32 {
    let c = match adopt(value) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let mut v = items(this - OFF_COLL);
    if i as usize > v.len() {
        drop(v);
        node_release(c);
        return E_BOUNDS;
    }
    v.insert(i as usize, c);
    S_OK
}
unsafe extern "win64" fn vec_remove_at(this: usize, i: u32) -> i32 {
    let old = {
        let mut v = items(this - OFF_COLL);
        if i as usize >= v.len() {
            return E_BOUNDS;
        }
        v.remove(i as usize)
    };
    node_release(old);
    S_OK
}
unsafe extern "win64" fn vec_append(this: usize, value: usize) -> i32 {
    match adopt(value) {
        Ok(c) => {
            items(this - OFF_COLL).push(c);
            S_OK
        }
        Err(e) => e,
    }
}
unsafe extern "win64" fn vec_remove_at_end(this: usize) -> i32 {
    let old = items(this - OFF_COLL).pop();
    match old {
        Some(c) => {
            node_release(c);
            S_OK
        }
        None => E_BOUNDS,
    }
}
unsafe extern "win64" fn vec_clear(this: usize) -> i32 {
    let old = std::mem::take(&mut *items(this - OFF_COLL));
    for c in old {
        node_release(c);
    }
    S_OK
}
unsafe extern "win64" fn vec_get_many(
    this: usize,
    start: u32,
    capacity: u32,
    out: *mut usize,
    actual: *mut u32,
) -> i32 {
    if actual.is_null() || (out.is_null() && capacity > 0) {
        return E_POINTER;
    }
    let v = items(this - OFF_COLL);
    if start as usize > v.len() {
        *actual = 0;
        return E_BOUNDS;
    }
    let n = (v.len() - start as usize).min(capacity as usize);
    for k in 0..n {
        let c = v[start as usize + k];
        node_add_ref(c);
        *out.add(k) = c + OFF_VALUE;
    }
    *actual = n as u32;
    S_OK
}
unsafe extern "win64" fn vec_replace_all(this: usize, count: u32, values: *const usize) -> i32 {
    if values.is_null() && count > 0 {
        return E_POINTER;
    }
    let mut fresh = Vec::with_capacity(count as usize);
    for k in 0..count as usize {
        match adopt(*values.add(k)) {
            Ok(c) => fresh.push(c),
            Err(e) => {
                for c in fresh {
                    node_release(c);
                }
                return e;
            }
        }
    }
    let old = std::mem::replace(&mut *items(this - OFF_COLL), fresh);
    for c in old {
        node_release(c);
    }
    S_OK
}

static VECTOR_VTBL: VTable<18> = node_vtbl!(OFF_COLL;
    vec_get_at,
    vec_size,
    vec_get_view,
    vec_index_of,
    vec_set_at,
    vec_insert_at,
    vec_remove_at,
    vec_append,
    vec_remove_at_end,
    vec_clear,
    vec_get_many,
    vec_replace_all,
);

// ============================================================
//  Itérateurs et paires clé-valeur
// ============================================================

#[repr(C)]
struct Iter {
    vtbl: *const (),
    refs: AtomicU32,
    /// Objet ou tableau parcouru, retenu par une référence.
    node: usize,
    pos: AtomicU32,
    kvp: bool,
}

unsafe fn iter<'a>(this: usize) -> &'a Iter {
    &*(this as *const Iter)
}

unsafe extern "win64" fn iterable_first(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let base = this - OFF_ITER;
    let kvp = value_type(base) == TYPE_OBJECT;
    node_add_ref(base);
    *out = Box::into_raw(Box::new(Iter {
        vtbl: if kvp {
            vt(&ITERATOR_KVP_VTBL)
        } else {
            vt(&ITERATOR_VALUE_VTBL)
        },
        refs: AtomicU32::new(1),
        node: base,
        pos: AtomicU32::new(0),
        kvp,
    })) as usize;
    S_OK
}

static ITERABLE_VALUE_VTBL: VTable<7> = node_vtbl!(OFF_ITER; iterable_first);
static ITERABLE_KVP_VTBL: VTable<7> = node_vtbl!(OFF_ITER; iterable_first);

unsafe extern "win64" fn it_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    let gen = generic();
    let own = if iter(this).kvp {
        gen.iterator_kvp
    } else {
        gen.iterator_value
    };
    if [IID_IUNKNOWN, IID_IINSPECTABLE, IID_IAGILEOBJECT, own].contains(&iid) {
        iter(this).refs.fetch_add(1, Ordering::SeqCst);
        *out = this;
        S_OK
    } else {
        *out = 0;
        E_NOINTERFACE
    }
}
unsafe extern "win64" fn it_add_ref(this: usize) -> u32 {
    iter(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn it_release(this: usize) -> u32 {
    let n = iter(this).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        let it = Box::from_raw(this as *mut Iter);
        node_release(it.node);
    }
    n
}
unsafe extern "win64" fn it_class(_this: usize, out: *mut usize) -> i32 {
    hstring("Windows.Foundation.Collections.IIterator`1", out)
}

fn len_of(base: usize) -> usize {
    unsafe {
        match &node(base).data {
            Data::Arr(_) => items(base).len(),
            Data::Obj(_) => members(base).len(),
            _ => 0,
        }
    }
}

/// Élément `i` du parcours : IJsonValue, ou une nouvelle IKeyValuePair.
unsafe fn element(it: &Iter, i: usize) -> Option<usize> {
    if it.kvp {
        let (key, c) = members(it.node).get(i).cloned()?;
        node_add_ref(c);
        Some(Box::into_raw(Box::new(Pair {
            vtbl: vt(&PAIR_VTBL),
            refs: AtomicU32::new(1),
            key,
            value: c,
        })) as usize)
    } else {
        item_at(it.node, i as u32).map(|c| c + OFF_VALUE)
    }
}

unsafe extern "win64" fn it_current(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let it = iter(this);
    match element(it, it.pos.load(Ordering::SeqCst) as usize) {
        Some(p) => {
            *out = p;
            S_OK
        }
        None => {
            *out = 0;
            E_BOUNDS
        }
    }
}
unsafe extern "win64" fn it_has_current(this: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let it = iter(this);
    *out = u8::from((it.pos.load(Ordering::SeqCst) as usize) < len_of(it.node));
    S_OK
}
unsafe extern "win64" fn it_move_next(this: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let it = iter(this);
    let len = len_of(it.node);
    let pos = it.pos.load(Ordering::SeqCst) as usize;
    if pos < len {
        it.pos.store(pos as u32 + 1, Ordering::SeqCst);
    }
    *out = u8::from(pos + 1 < len);
    S_OK
}
unsafe extern "win64" fn it_get_many(
    this: usize,
    capacity: u32,
    out: *mut usize,
    actual: *mut u32,
) -> i32 {
    if actual.is_null() || (out.is_null() && capacity > 0) {
        return E_POINTER;
    }
    let it = iter(this);
    let mut n = 0u32;
    while n < capacity {
        let pos = it.pos.load(Ordering::SeqCst) as usize;
        let Some(p) = element(it, pos) else { break };
        *out.add(n as usize) = p;
        it.pos.store(pos as u32 + 1, Ordering::SeqCst);
        n += 1;
    }
    *actual = n;
    S_OK
}

static ITERATOR_VALUE_VTBL: VTable<10> = VTable([
    it_qi as *const (),
    it_add_ref as *const (),
    it_release as *const (),
    get_iids as *const (),
    it_class as *const (),
    trust_level as *const (),
    it_current as *const (),
    it_has_current as *const (),
    it_move_next as *const (),
    it_get_many as *const (),
]);
static ITERATOR_KVP_VTBL: VTable<10> = VTable([
    it_qi as *const (),
    it_add_ref as *const (),
    it_release as *const (),
    get_iids as *const (),
    it_class as *const (),
    trust_level as *const (),
    it_current as *const (),
    it_has_current as *const (),
    it_move_next as *const (),
    it_get_many as *const (),
]);

#[repr(C)]
struct Pair {
    vtbl: *const (),
    refs: AtomicU32,
    key: String,
    value: usize,
}

unsafe fn pair<'a>(this: usize) -> &'a Pair {
    &*(this as *const Pair)
}

unsafe extern "win64" fn p_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    if [
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
        generic().kvp,
    ]
    .contains(&iid)
    {
        pair(this).refs.fetch_add(1, Ordering::SeqCst);
        *out = this;
        S_OK
    } else {
        *out = 0;
        E_NOINTERFACE
    }
}
unsafe extern "win64" fn p_add_ref(this: usize) -> u32 {
    pair(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn p_release(this: usize) -> u32 {
    let n = pair(this).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        let p = Box::from_raw(this as *mut Pair);
        node_release(p.value);
    }
    n
}
unsafe extern "win64" fn p_class(_this: usize, out: *mut usize) -> i32 {
    hstring("Windows.Foundation.Collections.IKeyValuePair`2", out)
}
unsafe extern "win64" fn p_key(this: usize, out: *mut usize) -> i32 {
    hstring(&pair(this).key, out)
}
unsafe extern "win64" fn p_value(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    give(pair(this).value, OFF_VALUE, out)
}

static PAIR_VTBL: VTable<8> = VTable([
    p_qi as *const (),
    p_add_ref as *const (),
    p_release as *const (),
    get_iids as *const (),
    p_class as *const (),
    trust_level as *const (),
    p_key as *const (),
    p_value as *const (),
]);

// ============================================================
//  Fabriques
// ============================================================

/// Fabrique statique : jusqu'à trois interfaces, aux décalages 0, 8 et 16.
#[repr(C)]
struct Factory {
    vt: [*const (); 3],
    class: &'static str,
    /// (IID, décalage) servis ; IUnknown, IInspectable et IAgileObject
    /// donnent le décalage 0.
    iids: &'static [(Guid, usize)],
}
unsafe impl Sync for Factory {}

unsafe fn factory<'a>(this: usize, off: usize) -> &'a Factory {
    &*((this - off) as *const Factory)
}

unsafe fn factory_qi(f: &Factory, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    let base = f as *const Factory as usize;
    let off = if [IID_IUNKNOWN, IID_IINSPECTABLE, IID_IAGILEOBJECT].contains(&iid) {
        Some(0)
    } else {
        f.iids.iter().find(|(g, _)| *g == iid).map(|(_, o)| *o)
    };
    match off {
        Some(o) => {
            *out = base + o;
            S_OK
        }
        None => {
            *out = 0;
            E_NOINTERFACE
        }
    }
}

unsafe extern "win64" fn f_qi<const OFF: usize>(
    this: usize,
    iid: *const u8,
    out: *mut usize,
) -> i32 {
    factory_qi(factory(this, OFF), iid, out)
}
extern "win64" fn f_add_ref(_this: usize) -> u32 {
    1
}
extern "win64" fn f_release(_this: usize) -> u32 {
    1
}
unsafe extern "win64" fn f_class<const OFF: usize>(this: usize, out: *mut usize) -> i32 {
    hstring(factory(this, OFF).class, out)
}

macro_rules! factory_vtbl {
    ($off:expr; $($f:expr),* $(,)?) => {
        VTable([
            f_qi::<{ $off }> as *const (),
            f_add_ref as *const (),
            f_release as *const (),
            get_iids as *const (),
            f_class::<{ $off }> as *const (),
            trust_level as *const (),
            $($f as *const ()),*
        ])
    };
}

/// Analyse pour Parse / TryParse ; `want` restreint le type de la racine.
unsafe fn parse_hstring(h: usize, want: Option<i32>) -> Result<usize, i32> {
    let base = parse(super::electron_win32::hstring_slice(h))?;
    if want.is_some_and(|t| t != value_type(base)) {
        node_release(base);
        return Err(WEB_E_INVALID_JSON_STRING);
    }
    Ok(base)
}

unsafe fn do_parse(h: usize, want: Option<i32>, off: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match parse_hstring(h, want) {
        Ok(b) => {
            *out = b + off;
            S_OK
        }
        Err(e) => {
            *out = 0;
            e
        }
    }
}

unsafe fn do_try_parse(
    h: usize,
    want: Option<i32>,
    off: usize,
    out: *mut usize,
    ok: *mut u8,
) -> i32 {
    if out.is_null() || ok.is_null() {
        return E_POINTER;
    }
    let hr = do_parse(h, want, off, out);
    *ok = u8::from(hr >= 0);
    S_OK
}

// --- JsonObject ---

unsafe extern "win64" fn obj_activate(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_node(Data::Obj(Mutex::new(Vec::new()))) + OFF_KIND;
    S_OK
}
unsafe extern "win64" fn obj_parse(_this: usize, h: usize, out: *mut usize) -> i32 {
    do_parse(h, Some(TYPE_OBJECT), OFF_KIND, out)
}
unsafe extern "win64" fn obj_try_parse(
    _this: usize,
    h: usize,
    out: *mut usize,
    ok: *mut u8,
) -> i32 {
    do_try_parse(h, Some(TYPE_OBJECT), OFF_KIND, out, ok)
}

static OBJ_FACTORY_ACT: VTable<7> = factory_vtbl!(0; obj_activate);
static OBJ_FACTORY_STATICS: VTable<8> = factory_vtbl!(8; obj_parse, obj_try_parse);
static OBJ_FACTORY: Factory = Factory {
    vt: [
        &OBJ_FACTORY_ACT as *const VTable<7> as *const (),
        &OBJ_FACTORY_STATICS as *const VTable<8> as *const (),
        std::ptr::null(),
    ],
    class: CLASS_OBJECT,
    iids: &[(IID_IACTIVATION_FACTORY, 0), (IID_JSON_OBJECT_STATICS, 8)],
};

// --- JsonArray ---

unsafe extern "win64" fn arr_activate(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_node(Data::Arr(Mutex::new(Vec::new()))) + OFF_KIND;
    S_OK
}
unsafe extern "win64" fn arr_parse(_this: usize, h: usize, out: *mut usize) -> i32 {
    do_parse(h, Some(TYPE_ARRAY), OFF_KIND, out)
}
unsafe extern "win64" fn arr_try_parse(
    _this: usize,
    h: usize,
    out: *mut usize,
    ok: *mut u8,
) -> i32 {
    do_try_parse(h, Some(TYPE_ARRAY), OFF_KIND, out, ok)
}

static ARR_FACTORY_ACT: VTable<7> = factory_vtbl!(0; arr_activate);
static ARR_FACTORY_STATICS: VTable<8> = factory_vtbl!(8; arr_parse, arr_try_parse);
static ARR_FACTORY: Factory = Factory {
    vt: [
        &ARR_FACTORY_ACT as *const VTable<7> as *const (),
        &ARR_FACTORY_STATICS as *const VTable<8> as *const (),
        std::ptr::null(),
    ],
    class: CLASS_ARRAY,
    iids: &[(IID_IACTIVATION_FACTORY, 0), (IID_JSON_ARRAY_STATICS, 8)],
};

// --- JsonValue (non activable) ---

/// ActivateInstance d'une classe qui n'a pas de constructeur par défaut.
unsafe extern "win64" fn no_activate(_this: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}
unsafe extern "win64" fn val_parse(_this: usize, h: usize, out: *mut usize) -> i32 {
    do_parse(h, None, OFF_VALUE, out)
}
unsafe extern "win64" fn val_try_parse(
    _this: usize,
    h: usize,
    out: *mut usize,
    ok: *mut u8,
) -> i32 {
    do_try_parse(h, None, OFF_VALUE, out, ok)
}
unsafe fn create(data: Data, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_node(data) + OFF_VALUE;
    S_OK
}
unsafe extern "win64" fn val_create_boolean(_this: usize, b: u8, out: *mut usize) -> i32 {
    create(Data::Bool(b != 0), out)
}
unsafe extern "win64" fn val_create_number(_this: usize, n: f64, out: *mut usize) -> i32 {
    create(Data::Num(n), out)
}
unsafe extern "win64" fn val_create_string(_this: usize, h: usize, out: *mut usize) -> i32 {
    create(Data::Str(read_hstring(h)), out)
}
unsafe extern "win64" fn val_create_null(_this: usize, out: *mut usize) -> i32 {
    create(Data::Null, out)
}

static VAL_FACTORY_STATICS: VTable<11> = factory_vtbl!(0;
    val_parse,
    val_try_parse,
    val_create_boolean,
    val_create_number,
    val_create_string,
);
static VAL_FACTORY_STATICS2: VTable<7> = factory_vtbl!(8; val_create_null);
static VAL_FACTORY_ACT: VTable<7> = factory_vtbl!(16; no_activate);
static VAL_FACTORY: Factory = Factory {
    vt: [
        &VAL_FACTORY_STATICS as *const VTable<11> as *const (),
        &VAL_FACTORY_STATICS2 as *const VTable<7> as *const (),
        &VAL_FACTORY_ACT as *const VTable<7> as *const (),
    ],
    class: CLASS_VALUE,
    iids: &[
        (IID_JSON_VALUE_STATICS, 0),
        (IID_JSON_VALUE_STATICS2, 8),
        (IID_IACTIVATION_FACTORY, 16),
    ],
};

// --- JsonError ---

/// JsonError.GetJsonStatus : JsonErrorStatus d'un HRESULT.
unsafe extern "win64" fn err_get_json_status(_this: usize, hr: i32, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = match hr {
        WEB_E_INVALID_JSON_STRING => 1,
        WEB_E_INVALID_JSON_NUMBER => 2,
        WEB_E_JSON_VALUE_NOT_FOUND => 3,
        _ => 0,
    };
    S_OK
}

static ERR_FACTORY_STATICS2: VTable<7> = factory_vtbl!(0; err_get_json_status);
static ERR_FACTORY_ACT: VTable<7> = factory_vtbl!(8; no_activate);
static ERR_FACTORY: Factory = Factory {
    vt: [
        &ERR_FACTORY_STATICS2 as *const VTable<7> as *const (),
        &ERR_FACTORY_ACT as *const VTable<7> as *const (),
        std::ptr::null(),
    ],
    class: CLASS_ERROR,
    iids: &[(IID_JSON_ERROR_STATICS2, 0), (IID_IACTIVATION_FACTORY, 8)],
};

/// `RoGetActivationFactory` pour les classes de Windows.Data.Json, `None`
/// pour une autre classe.
pub unsafe fn activation_factory(class: &str, iid: *const u8, out: *mut usize) -> Option<i32> {
    let f = match class {
        CLASS_OBJECT => &OBJ_FACTORY,
        CLASS_ARRAY => &ARR_FACTORY,
        CLASS_VALUE => &VAL_FACTORY,
        CLASS_ERROR => &ERR_FACTORY,
        _ => return None,
    };
    let hr = factory_qi(f, iid, out);
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
    unsafe fn hs(text: &str) -> usize {
        let mut h = 0usize;
        assert_eq!(hstring(text, &mut h), S_OK);
        h
    }
    unsafe fn text(h: usize) -> String {
        read_hstring(h)
    }
    unsafe fn qi(obj: usize, iid: &Guid) -> usize {
        let f: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32 = slot(obj, 0);
        let mut p = 0usize;
        assert_eq!(
            f(obj, iid.as_ptr(), &mut p),
            S_OK,
            "QI {}",
            super::super::comobj::guid_str(iid)
        );
        p
    }
    unsafe fn release(obj: usize) -> u32 {
        let f: unsafe extern "win64" fn(usize) -> u32 = slot(obj, 2);
        f(obj)
    }
    unsafe fn get_hs(obj: usize, i: usize) -> String {
        let f: unsafe extern "win64" fn(usize, *mut usize) -> i32 = slot(obj, i);
        let mut h = 0usize;
        assert_eq!(f(obj, &mut h), S_OK);
        text(h)
    }

    #[test]
    fn identifiants_parametres() {
        // Valeurs publiées : IIterable<String>, IVector<String>.
        assert_eq!(
            super::super::comobj::guid_str(&pinterface_iid(
                "pinterface({faa585ea-6214-4217-afda-7f46de5869b3};string)"
            )),
            "{E2FCC7C1-3BFC-5A0B-B2B0-72E769D1CB7E}"
        );
        assert_eq!(
            super::super::comobj::guid_str(&pinterface_iid(
                "pinterface({913337e9-11a1-4345-a3a2-4e7f956e222d};string)"
            )),
            "{98B9ACC1-4B56-532E-AC73-03D5291CCA90}"
        );
    }

    #[test]
    fn analyse_et_ecriture() {
        let w = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        unsafe {
            let src = r#" { "a" : 1, "b": [true, null, -2.5e3, "x\né😀"], "c": {}, "a": 3 } "#;
            let b = parse(&w(src)).unwrap();
            let mut s = String::new();
            stringify(b, &mut s, 0).unwrap();
            assert_eq!(s, "{\"a\":3,\"b\":[true,null,-2500,\"x\\né😀\"],\"c\":{}}");
            node_release(b);
            for bad in [
                "",
                "{",
                "[1,]",
                "{\"a\" 1}",
                "tru",
                "\"a",
                "{} x",
                "[\"\u{1}\"]",
            ] {
                assert_eq!(parse(&w(bad)), Err(WEB_E_INVALID_JSON_STRING), "{bad:?}");
            }
            for bad in ["01", "1.", "-", "1e", "1e999"] {
                assert_eq!(parse(&w(bad)), Err(WEB_E_INVALID_JSON_NUMBER), "{bad:?}");
            }
        }
        assert_eq!(format_number(1.0), "1");
        assert_eq!(format_number(0.1), "0.1");
        assert_eq!(format_number(1e21), "1e+21");
        assert_eq!(format_number(1.5e-7), "1.5e-7");
        assert_eq!(format_number(-0.0), "0");
    }

    #[test]
    fn json_object_par_la_fabrique() {
        let _teb = crate::process::test_teb();
        unsafe {
            // Comme Spotify : la fabrique sous IActivationFactory.
            let mut f = 0usize;
            assert_eq!(
                activation_factory(CLASS_OBJECT, IID_IACTIVATION_FACTORY.as_ptr(), &mut f),
                Some(S_OK)
            );
            let activate: unsafe extern "win64" fn(usize, *mut usize) -> i32 = slot(f, 6);
            let mut inst = 0usize;
            assert_eq!(activate(f, &mut inst), S_OK);
            let obj = qi(inst, &IID_JSON_OBJECT);
            assert_eq!(obj, inst);
            assert_eq!(get_hs(obj, 4), CLASS_OBJECT);

            // SetNamedValue avec des valeurs de JsonValue.
            let mut vf = 0usize;
            assert_eq!(
                activation_factory(CLASS_VALUE, IID_JSON_VALUE_STATICS.as_ptr(), &mut vf),
                Some(S_OK)
            );
            let mk_str: unsafe extern "win64" fn(usize, usize, *mut usize) -> i32 = slot(vf, 10);
            let mk_num: unsafe extern "win64" fn(usize, f64, *mut usize) -> i32 = slot(vf, 9);
            let mk_bool: unsafe extern "win64" fn(usize, u8, *mut usize) -> i32 = slot(vf, 8);
            let (mut s, mut n, mut t) = (0usize, 0usize, 0usize);
            assert_eq!(mk_str(vf, hs("Spotify \"x\""), &mut s), S_OK);
            assert_eq!(mk_num(vf, 42.0, &mut n), S_OK);
            assert_eq!(mk_bool(vf, 1, &mut t), S_OK);
            let set: unsafe extern "win64" fn(usize, usize, *mut usize) -> i32 = slot(obj, 7);
            let set: unsafe extern "win64" fn(usize, usize, usize) -> i32 =
                std::mem::transmute(set);
            assert_eq!(set(obj, hs("nom"), s), S_OK);
            assert_eq!(set(obj, hs("n"), n), S_OK);
            assert_eq!(set(obj, hs("ok"), t), S_OK);
            for v in [s, n, t] {
                release(v);
            }

            // Stringify par IJsonValue et IStringable.
            let val = qi(obj, &IID_JSON_VALUE);
            let expected = "{\"nom\":\"Spotify \\\"x\\\"\",\"n\":42,\"ok\":true}";
            assert_eq!(get_hs(val, 7), expected);
            let st = qi(obj, &IID_ISTRINGABLE);
            assert_eq!(get_hs(st, 6), expected);
            release(st);
            let ty: unsafe extern "win64" fn(usize, *mut i32) -> i32 = slot(val, 6);
            let mut k = -1;
            assert_eq!(ty(val, &mut k), S_OK);
            assert_eq!(k, TYPE_OBJECT);
            release(val);

            // Lectures nommées.
            let get_num: unsafe extern "win64" fn(usize, usize, *mut f64) -> i32 = slot(obj, 11);
            let mut x = 0.0;
            assert_eq!(get_num(obj, hs("n"), &mut x), S_OK);
            assert_eq!(x, 42.0);
            assert_eq!(
                get_num(obj, hs("absent"), &mut x),
                WEB_E_JSON_VALUE_NOT_FOUND
            );
            assert_eq!(get_num(obj, hs("nom"), &mut x), E_ILLEGAL_METHOD_CALL);
            let get_str: unsafe extern "win64" fn(usize, usize, *mut usize) -> i32 = slot(obj, 10);
            let mut h = 0usize;
            assert_eq!(get_str(obj, hs("nom"), &mut h), S_OK);
            assert_eq!(text(h), "Spotify \"x\"");

            // IMap et parcours par IIterable.
            let map = qi(obj, &generic().map);
            let size: unsafe extern "win64" fn(usize, *mut u32) -> i32 = slot(map, 7);
            let mut len = 0u32;
            assert_eq!(size(map, &mut len), S_OK);
            assert_eq!(len, 3);
            let remove: unsafe extern "win64" fn(usize, usize) -> i32 = slot(map, 11);
            assert_eq!(remove(map, hs("ok")), S_OK);
            assert_eq!(remove(map, hs("ok")), E_BOUNDS);
            release(map);
            let iterable = qi(obj, &generic().iterable_kvp);
            let first: unsafe extern "win64" fn(usize, *mut usize) -> i32 = slot(iterable, 6);
            let mut it = 0usize;
            assert_eq!(first(iterable, &mut it), S_OK);
            let mut keys = Vec::new();
            let has: unsafe extern "win64" fn(usize, *mut u8) -> i32 = slot(it, 7);
            let next: unsafe extern "win64" fn(usize, *mut u8) -> i32 = slot(it, 8);
            let cur: unsafe extern "win64" fn(usize, *mut usize) -> i32 = slot(it, 6);
            let mut b = 0u8;
            has(it, &mut b);
            while b != 0 {
                let mut p = 0usize;
                assert_eq!(cur(it, &mut p), S_OK);
                keys.push(get_hs(p, 6));
                release(p);
                next(it, &mut b);
            }
            assert_eq!(keys, ["nom", "n"]);
            release(it);
            release(iterable);

            // Valeur par défaut.
            let d = qi(obj, &IID_JSON_OBJECT_DEFAULTS);
            let dnum: unsafe extern "win64" fn(usize, usize, f64, *mut f64) -> i32 = slot(d, 10);
            assert_eq!(dnum(d, hs("absent"), 7.5, &mut x), S_OK);
            assert_eq!(x, 7.5);
            release(d);

            // Références : l'instance et son QueryInterface.
            assert_eq!(release(inst), 1);
            assert_eq!(release(obj), 0);
        }
    }

    #[test]
    fn json_array_et_parse() {
        let _teb = crate::process::test_teb();
        unsafe {
            let mut f = 0usize;
            assert_eq!(
                activation_factory(CLASS_ARRAY, IID_JSON_ARRAY_STATICS.as_ptr(), &mut f),
                Some(S_OK)
            );
            let parse_fn: unsafe extern "win64" fn(usize, usize, *mut usize) -> i32 = slot(f, 6);
            let mut a = 0usize;
            assert_eq!(parse_fn(f, hs("[1, {\"k\": \"v\"}, [2]]"), &mut a), S_OK);
            assert_eq!(parse_fn(f, hs("{}"), &mut a), WEB_E_INVALID_JSON_STRING);
            assert_eq!(a, 0);
            let try_parse: unsafe extern "win64" fn(usize, usize, *mut usize, *mut u8) -> i32 =
                slot(f, 7);
            let mut ok = 9u8;
            assert_eq!(try_parse(f, hs("[1,"), &mut a, &mut ok), S_OK);
            assert_eq!((a, ok), (0, 0));
            assert_eq!(
                try_parse(f, hs("[1, {\"k\": \"v\"}, [2]]"), &mut a, &mut ok),
                S_OK
            );
            assert_eq!(ok, 1);
            let get_obj: unsafe extern "win64" fn(usize, u32, *mut usize) -> i32 = slot(a, 6);
            let mut o = 0usize;
            assert_eq!(get_obj(a, 1, &mut o), S_OK);
            let get_str: unsafe extern "win64" fn(usize, usize, *mut usize) -> i32 = slot(o, 10);
            let mut h = 0usize;
            assert_eq!(get_str(o, hs("k"), &mut h), S_OK);
            assert_eq!(text(h), "v");
            let mut other = 0usize;
            assert_eq!(get_obj(a, 0, &mut other), E_ILLEGAL_METHOD_CALL);
            assert_eq!(get_obj(a, 9, &mut other), E_BOUNDS);

            // IVector : ajout d'un élément venu de l'objet, puis écriture.
            let vec = qi(a, &generic().vector);
            let get_at: unsafe extern "win64" fn(usize, u32, *mut usize) -> i32 = slot(vec, 6);
            let mut first = 0usize;
            assert_eq!(get_at(vec, 0, &mut first), S_OK);
            let append: unsafe extern "win64" fn(usize, usize) -> i32 = slot(vec, 13);
            assert_eq!(append(vec, first), S_OK);
            let index_of: unsafe extern "win64" fn(usize, usize, *mut u32, *mut u8) -> i32 =
                slot(vec, 9);
            let (mut idx, mut found) = (9u32, 0u8);
            assert_eq!(index_of(vec, first, &mut idx, &mut found), S_OK);
            assert_eq!((idx, found), (0, 1));
            release(first);
            let val = qi(a, &IID_JSON_VALUE);
            assert_eq!(get_hs(val, 7), "[1,{\"k\":\"v\"},[2],1]");
            release(val);
            release(vec);
            release(o);

            // JsonError.GetJsonStatus.
            let mut e = 0usize;
            assert_eq!(
                activation_factory(CLASS_ERROR, IID_JSON_ERROR_STATICS2.as_ptr(), &mut e),
                Some(S_OK)
            );
            let status: unsafe extern "win64" fn(usize, i32, *mut i32) -> i32 = slot(e, 6);
            let mut st = -1;
            assert_eq!(status(e, WEB_E_JSON_VALUE_NOT_FOUND, &mut st), S_OK);
            assert_eq!(st, 3);

            assert_eq!(release(a), 0);
            assert!(activation_factory(
                "Windows.Data.Xml.Dom.XmlDocument",
                IID_IUNKNOWN.as_ptr(),
                &mut e
            )
            .is_none());
        }
    }
}
