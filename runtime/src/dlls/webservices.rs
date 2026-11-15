//! webservices.dll : API Windows Web Services (WWSAPI), partie lecture XML.
//!
//! L'outil de déploiement d'Office (32 bits) charge cette DLL en différé
//! juste après sa première réponse HTTP. Ses imports, relevés par le
//! diagnostic de #2627 au banc, sont exactement les quinze fonctions de ce
//! fichier : erreurs, tas, lecteur XML (entrée tampon ou flux, texte) et
//! désérialisation par `WsReadType`, plus la création d'un écrivain (#2623).
//!
//! Provenance : pages de Microsoft Learn de chaque fonction, structure et
//! énumération (dépôt public MicrosoftDocs/sdk-api : valeurs de WS_TYPE,
//! WS_TYPE_MAPPING, WS_READ_OPTION, WS_FIELD_MAPPING,
//! WS_XML_READER_INPUT_TYPE, WS_XML_READER_ENCODING_TYPE, WS_CHARSET,
//! WS_ERROR_PROPERTY_ID) ; codes WS_E_* de la page « COM Error Codes » ;
//! signatures, ordre des champs et constantes WS_FIELD_* / WS_STRUCT_* de
//! windows-rs (métadonnées Win32 de Microsoft, licence MIT). Les décalages
//! x86 et x64 sont **déduits** de cet ordre de champs par l'alignement
//! naturel (pointeur de 4 ou 8 octets, ULONG et énumérations de 4).
//!
//! Le document est analysé en entier par l'arbre DOM de msxml_dom.rs, puis
//! parcouru comme une suite d'événements (début d'élément, texte, fin
//! d'élément), ce que suffit aux fonctions importées : aucune ne lit
//! d'attribut isolément ni de nœud brut.

use super::msxml_dom::{Doc, Kind, DOC};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
/// WS_E_INVALID_FORMAT (« COM Error Codes », Microsoft Learn).
const WS_E_INVALID_FORMAT: i32 = 0x803D_0000u32 as i32;
/// WS_E_INVALID_OPERATION.
const WS_E_INVALID_OPERATION: i32 = 0x803D_0003u32 as i32;

// WS_TYPE (Microsoft Learn).
const WS_BOOL_TYPE: u32 = 0;
const WS_INT8_TYPE: u32 = 1;
const WS_INT16_TYPE: u32 = 2;
const WS_INT32_TYPE: u32 = 3;
const WS_INT64_TYPE: u32 = 4;
const WS_UINT8_TYPE: u32 = 5;
const WS_UINT16_TYPE: u32 = 6;
const WS_UINT32_TYPE: u32 = 7;
const WS_UINT64_TYPE: u32 = 8;
const WS_FLOAT_TYPE: u32 = 9;
const WS_DOUBLE_TYPE: u32 = 10;
const WS_STRING_TYPE: u32 = 16;
const WS_WSZ_TYPE: u32 = 17;
const WS_XML_STRING_TYPE: u32 = 19;
const WS_STRUCT_TYPE: u32 = 26;
const WS_ENUM_TYPE: u32 = 31;

// WS_TYPE_MAPPING.
const WS_ELEMENT_TYPE_MAPPING: u32 = 1;
const WS_ELEMENT_CONTENT_TYPE_MAPPING: u32 = 3;

// WS_READ_OPTION.
const WS_READ_REQUIRED_VALUE: u32 = 1;
const WS_READ_REQUIRED_POINTER: u32 = 2;
const WS_READ_OPTIONAL_POINTER: u32 = 3;
const WS_READ_NILLABLE_POINTER: u32 = 4;
const WS_READ_NILLABLE_VALUE: u32 = 5;

// WS_FIELD_MAPPING.
const WS_TYPE_ATTRIBUTE_FIELD_MAPPING: u32 = 0;
const WS_ATTRIBUTE_FIELD_MAPPING: u32 = 1;
const WS_ELEMENT_FIELD_MAPPING: u32 = 2;
const WS_REPEATING_ELEMENT_FIELD_MAPPING: u32 = 3;
const WS_TEXT_FIELD_MAPPING: u32 = 4;
const WS_NO_FIELD_MAPPING: u32 = 5;

// WS_FIELD_OPTIONS (windows-rs).
const WS_FIELD_POINTER: u32 = 1;
const WS_FIELD_OPTIONAL: u32 = 2;
const WS_FIELD_NILLABLE: u32 = 4;

// WS_XML_READER_INPUT_TYPE, WS_XML_READER_ENCODING_TYPE, WS_CHARSET.
const WS_XML_READER_INPUT_TYPE_BUFFER: u32 = 1;
const WS_XML_READER_INPUT_TYPE_STREAM: u32 = 2;
const WS_XML_READER_ENCODING_TYPE_TEXT: u32 = 1;
const WS_CHARSET_UTF16LE: u32 = 2;
const WS_CHARSET_UTF16BE: u32 = 3;

// WS_ERROR_PROPERTY_ID.
const WS_ERROR_PROPERTY_STRING_COUNT: u32 = 0;
const WS_ERROR_PROPERTY_ORIGINAL_ERROR_CODE: u32 = 1;
const WS_ERROR_PROPERTY_LANGID: u32 = 2;

/// Poignées opaques : petits entiers, valables aussi pour un programme x86.
const HANDLE_BASE: usize = 0x5753_0000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Event {
    Start(usize),
    Text(usize),
    End(usize),
}

enum Source {
    None,
    Buffer(Vec<u8>),
    Stream { callback: usize, state: usize },
}

struct Reader {
    source: Source,
    charset: u32,
    /// Octets reçus du flux par WsFillReader.
    filled: Vec<u8>,
    doc: Option<Doc>,
    events: Vec<Event>,
    /// Événement courant ; `events.len()` : fin du document.
    pos: usize,
}

struct ErrorObject {
    /// Chaînes UTF-16 (dernière ajoutée en tête), sans zéro final, et le bloc
    /// qui les porte pour l'appelant.
    strings: Vec<(Vec<u16>, usize)>,
    original: u32,
}

#[derive(Default)]
struct Heap {
    blocks: Vec<usize>,
}

enum Object {
    Error(ErrorObject),
    Heap(Heap),
    Reader(Box<Reader>),
    Writer,
}

struct Table {
    next: usize,
    objects: HashMap<usize, Object>,
}

fn table() -> &'static Mutex<Table> {
    static T: OnceLock<Mutex<Table>> = OnceLock::new();
    T.get_or_init(|| {
        Mutex::new(Table {
            next: HANDLE_BASE + 1,
            objects: HashMap::new(),
        })
    })
}

fn lock() -> std::sync::MutexGuard<'static, Table> {
    table().lock().unwrap_or_else(|e| e.into_inner())
}

fn insert(object: Object) -> usize {
    let mut t = lock();
    let h = t.next;
    t.next += 1;
    t.objects.insert(h, object);
    h
}

fn ptr_size() -> usize {
    crate::wow32::pointer_size()
}

unsafe fn rd32(at: usize) -> u32 {
    std::ptr::read_unaligned(at as *const u32)
}

unsafe fn rdp(at: usize, ps: usize) -> usize {
    if ps == 4 {
        rd32(at) as usize
    } else {
        std::ptr::read_unaligned(at as *const usize)
    }
}

unsafe fn wrp(at: usize, v: usize, ps: usize) {
    if ps == 4 {
        std::ptr::write_unaligned(at as *mut u32, v as u32)
    } else {
        std::ptr::write_unaligned(at as *mut usize, v)
    }
}

/// WS_XML_STRING { length, bytes, dictionary, id } : `None` pour NULL.
unsafe fn xml_string(at: usize, ps: usize) -> Option<String> {
    if at == 0 {
        return None;
    }
    let len = rd32(at) as usize;
    let bytes = rdp(at + ps, ps);
    if len == 0 || bytes == 0 {
        return Some(String::new());
    }
    let s = std::slice::from_raw_parts(bytes as *const u8, len);
    Some(String::from_utf8_lossy(s).into_owned())
}

/// Bloc mis à zéro, adressable par le programme (le tas du processus, comme
/// HeapAlloc : sous 4 Gio pour un programme x86).
unsafe fn alloc(size: usize) -> usize {
    super::kernel32::heap_alloc(0, 8, size.max(1)) as usize
}

unsafe fn free(p: usize) {
    if p != 0 {
        super::kernel32::heap_free(0, 0, p as *mut std::ffi::c_void);
    }
}

/// Ajoute une chaîne à l'objet d'erreur du programme (s'il en a passé un) et
/// rend le code.
fn fail(error: usize, hr: i32, message: &str) -> i32 {
    crate::log_info!("webservices", "{message} (0x{:08x})", hr as u32);
    if error == 0 {
        return hr;
    }
    let mut t = lock();
    if let Some(Object::Error(e)) = t.objects.get_mut(&error) {
        let wide: Vec<u16> = message.encode_utf16().collect();
        let block = unsafe { alloc((wide.len() + 1) * 2) };
        if block != 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), block as *mut u16, wide.len());
            }
        }
        e.strings.insert(0, (wide, block));
        if e.original == 0 {
            e.original = hr as u32;
        }
    }
    hr
}

// ---------- Erreurs ----------

pub unsafe extern "win64" fn ws_create_error(_properties: usize, _count: u32, out: usize) -> i32 {
    if out == 0 {
        return E_INVALIDARG;
    }
    let h = insert(Object::Error(ErrorObject {
        strings: Vec::new(),
        original: 0,
    }));
    crate::wow32::write_ptr(out, h);
    S_OK
}

pub unsafe extern "win64" fn ws_free_error(error: usize) {
    if let Some(Object::Error(e)) = lock().objects.remove(&error) {
        for (_, block) in e.strings {
            free(block);
        }
    }
}

pub unsafe extern "win64" fn ws_get_error_property(
    error: usize,
    id: u32,
    buffer: usize,
    size: u32,
) -> i32 {
    let t = lock();
    let Some(Object::Error(e)) = t.objects.get(&error) else {
        return E_INVALIDARG;
    };
    if buffer == 0 {
        return E_INVALIDARG;
    }
    match (id, size) {
        (WS_ERROR_PROPERTY_STRING_COUNT, 4) => {
            std::ptr::write_unaligned(buffer as *mut u32, e.strings.len() as u32)
        }
        (WS_ERROR_PROPERTY_ORIGINAL_ERROR_CODE, 4) => {
            std::ptr::write_unaligned(buffer as *mut u32, e.original)
        }
        // LANGID (USHORT) : celui de l'interface.
        (WS_ERROR_PROPERTY_LANGID, 2) => std::ptr::write_unaligned(
            buffer as *mut u16,
            crate::intl::api::get_user_default_ui_language(),
        ),
        _ => return E_INVALIDARG,
    }
    S_OK
}

/// WS_STRING { length, chars } : chaîne n° `index`, la plus récente en 0.
pub unsafe extern "win64" fn ws_get_error_string(error: usize, index: u32, out: usize) -> i32 {
    let t = lock();
    let Some(Object::Error(e)) = t.objects.get(&error) else {
        return E_INVALIDARG;
    };
    let Some((wide, block)) = e.strings.get(index as usize) else {
        return E_INVALIDARG;
    };
    if out == 0 {
        return E_INVALIDARG;
    }
    let ps = ptr_size();
    std::ptr::write_unaligned(out as *mut u32, wide.len() as u32);
    wrp(out + ps, *block, ps);
    S_OK
}

// ---------- Tas ----------

pub unsafe extern "win64" fn ws_create_heap(
    _max_size: usize,
    _trim_size: usize,
    _properties: usize,
    _count: u32,
    out: usize,
    _error: usize,
) -> i32 {
    if out == 0 {
        return E_INVALIDARG;
    }
    let h = insert(Object::Heap(Heap::default()));
    crate::wow32::write_ptr(out, h);
    S_OK
}

pub unsafe extern "win64" fn ws_free_heap(heap: usize) {
    if let Some(Object::Heap(h)) = lock().objects.remove(&heap) {
        for b in h.blocks {
            free(b);
        }
    }
}

/// Bloc pris sur le tas WS du programme (libéré avec lui), ou sur le tas du
/// processus si l'appel n'en donne pas.
fn heap_alloc(heap: usize, size: usize) -> usize {
    let p = unsafe { alloc(size) };
    if p != 0 {
        if let Some(Object::Heap(h)) = lock().objects.get_mut(&heap) {
            h.blocks.push(p);
        }
    }
    p
}

// ---------- Écrivain ----------

/// Aucune fonction d'écriture n'est importée par les programmes vus : l'objet
/// existe pour être créé et libéré.
pub unsafe extern "win64" fn ws_create_writer(
    _properties: usize,
    _count: u32,
    out: usize,
    _error: usize,
) -> i32 {
    if out == 0 {
        return E_INVALIDARG;
    }
    crate::wow32::write_ptr(out, insert(Object::Writer));
    S_OK
}

pub unsafe extern "win64" fn ws_free_writer(writer: usize) {
    lock().objects.remove(&writer);
}

// ---------- Lecteur ----------

pub unsafe extern "win64" fn ws_create_reader(
    _properties: usize,
    _count: u32,
    out: usize,
    _error: usize,
) -> i32 {
    if out == 0 {
        return E_INVALIDARG;
    }
    let h = insert(Object::Reader(Box::new(Reader {
        source: Source::None,
        charset: 0,
        filled: Vec::new(),
        doc: None,
        events: Vec::new(),
        pos: 0,
    })));
    crate::wow32::write_ptr(out, h);
    S_OK
}

pub unsafe extern "win64" fn ws_free_reader(reader: usize) {
    lock().objects.remove(&reader);
}

/// Exécute `f` sur le lecteur, hors du verrou de la table pendant les
/// rappels du programme (le lecteur est retiré puis remis).
fn with_reader<R>(reader: usize, f: impl FnOnce(&mut Reader) -> R) -> Option<R> {
    let mut boxed = match lock().objects.remove(&reader) {
        Some(Object::Reader(r)) => r,
        Some(other) => {
            lock().objects.insert(reader, other);
            return None;
        }
        None => return None,
    };
    let result = f(&mut boxed);
    lock().objects.insert(reader, Object::Reader(boxed));
    Some(result)
}

pub unsafe extern "win64" fn ws_set_input(
    reader: usize,
    encoding: usize,
    input: usize,
    _properties: usize,
    _count: u32,
    error: usize,
) -> i32 {
    let ps = ptr_size();
    // WS_XML_READER_TEXT_ENCODING { encoding { encodingType }, charSet }.
    let charset = if encoding == 0 {
        0
    } else {
        let kind = rd32(encoding);
        if kind != WS_XML_READER_ENCODING_TYPE_TEXT {
            crate::log_warn!(
                "webservices",
                "WsSetInput : encodage {kind} pas encore pris en charge (webservices.rs:{})",
                line!()
            );
            return fail(error, E_NOTIMPL, "encodage XML non textuel");
        }
        rd32(encoding + 4)
    };
    let source = if input == 0 {
        Source::None
    } else {
        // WS_XML_READER_BUFFER_INPUT { input { inputType }, encodedData, size }
        // WS_XML_READER_STREAM_INPUT { input, readCallback, readCallbackState }
        match rd32(input) {
            WS_XML_READER_INPUT_TYPE_BUFFER => {
                let data = rdp(input + ps, ps);
                let size = rd32(input + 2 * ps) as usize;
                let bytes = if data == 0 || size == 0 {
                    Vec::new()
                } else {
                    std::slice::from_raw_parts(data as *const u8, size).to_vec()
                };
                Source::Buffer(bytes)
            }
            WS_XML_READER_INPUT_TYPE_STREAM => Source::Stream {
                callback: rdp(input + ps, ps),
                state: rdp(input + 2 * ps, ps),
            },
            other => {
                return fail(error, E_INVALIDARG, &format!("type d'entrée XML {other}"));
            }
        }
    };
    let found = with_reader(reader, |r| {
        r.source = source;
        r.charset = charset;
        r.filled.clear();
        r.doc = None;
        r.events.clear();
        r.pos = 0;
    });
    if found.is_none() {
        return E_INVALIDARG;
    }
    S_OK
}

/// Appelle le WS_READ_CALLBACK du programme jusqu'à la fin du flux.
unsafe fn read_stream(callback: usize, state: usize, out: &mut Vec<u8>) -> i32 {
    const CHUNK: usize = 64 * 1024;
    let buffer = alloc(CHUNK + 4);
    if buffer == 0 {
        return E_OUTOFMEMORY;
    }
    let actual = buffer + CHUNK;
    let f = if crate::wow32::active() {
        crate::wow32::callback64(callback as u32)
    } else {
        callback
    };
    let f: unsafe extern "win64" fn(usize, usize, u32, usize, usize, usize) -> i32 =
        std::mem::transmute(f);
    let hr = loop {
        std::ptr::write_unaligned(actual as *mut u32, 0);
        let hr = f(state, buffer, CHUNK as u32, actual, 0, 0);
        if hr < 0 {
            break hr;
        }
        let n = (rd32(actual) as usize).min(CHUNK);
        if n == 0 {
            break S_OK;
        }
        out.extend_from_slice(std::slice::from_raw_parts(buffer as *const u8, n));
        if out.len() > 64 << 20 {
            break S_OK;
        }
    };
    free(buffer);
    hr
}

/// Analyse le document s'il ne l'est pas encore (flux : tout est lu).
unsafe fn ensure_parsed(r: &mut Reader) -> Result<(), (i32, String)> {
    if r.doc.is_some() {
        return Ok(());
    }
    let bytes = match &r.source {
        Source::None => return Err((WS_E_INVALID_OPERATION, "lecteur sans entrée".into())),
        Source::Buffer(b) => b.clone(),
        Source::Stream { callback, state } => {
            if r.filled.is_empty() {
                let mut data = Vec::new();
                let hr = read_stream(*callback, *state, &mut data);
                if hr < 0 {
                    return Err((hr, "le rappel de lecture du flux a échoué".into()));
                }
                r.filled = data;
            }
            r.filled.clone()
        }
    };
    let text = decode(&bytes, r.charset);
    let mut doc = Doc::new();
    doc.preserve_ws = true;
    if !doc.load_str(&text) {
        return Err((
            WS_E_INVALID_FORMAT,
            format!("XML invalide : {}", doc.error.reason),
        ));
    }
    let mut events = Vec::new();
    flatten(&doc, DOC, &mut events);
    r.events = events;
    r.doc = Some(doc);
    r.pos = 0;
    Ok(())
}

fn decode(bytes: &[u8], charset: u32) -> String {
    let utf16 = |be: bool| {
        let u: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| {
                if be {
                    u16::from_be_bytes(*c)
                } else {
                    u16::from_le_bytes(*c)
                }
            })
            .collect();
        String::from_utf16_lossy(&u)
            .trim_start_matches('\u{feff}')
            .to_string()
    };
    match charset {
        WS_CHARSET_UTF16LE => utf16(false),
        WS_CHARSET_UTF16BE => utf16(true),
        _ => super::msxml_dom::decode(bytes),
    }
}

fn flatten(doc: &Doc, node: usize, out: &mut Vec<Event>) {
    for &c in &doc.node(node).children {
        match doc.node(c).kind {
            Kind::Element => {
                out.push(Event::Start(c));
                flatten(doc, c, out);
                out.push(Event::End(c));
            }
            Kind::Text | Kind::CData => out.push(Event::Text(c)),
            _ => {}
        }
    }
}

pub unsafe extern "win64" fn ws_fill_reader(
    reader: usize,
    _min_size: u32,
    _async_context: usize,
    error: usize,
) -> i32 {
    match with_reader(reader, |r| ensure_parsed(r)) {
        None => E_INVALIDARG,
        Some(Ok(())) => S_OK,
        Some(Err((hr, message))) => fail(error, hr, &message),
    }
}

fn is_blank(doc: &Doc, id: usize) -> bool {
    doc.node(id).value.chars().all(char::is_whitespace)
}

/// Saute les textes blancs ; rend l'élément sur lequel le lecteur s'arrête.
fn skip_blank(r: &mut Reader) -> Option<usize> {
    let doc = r.doc.as_ref()?;
    while let Some(e) = r.events.get(r.pos) {
        match *e {
            Event::Text(t) if is_blank(doc, t) => r.pos += 1,
            Event::Start(id) => return Some(id),
            _ => return None,
        }
    }
    None
}

fn local_name(doc: &Doc, id: usize) -> String {
    doc.prefix_and_base(id).1
}

fn name_matches(doc: &Doc, id: usize, local: Option<&str>, ns: Option<&str>) -> bool {
    local.is_none_or(|l| local_name(doc, id) == l) && ns.is_none_or(|n| doc.namespace_uri(id) == n)
}

pub unsafe extern "win64" fn ws_read_to_start_element(
    reader: usize,
    local: usize,
    ns: usize,
    found: usize,
    error: usize,
) -> i32 {
    let ps = ptr_size();
    let local = xml_string(local, ps);
    let ns = xml_string(ns, ps);
    let result = with_reader(reader, |r| {
        ensure_parsed(r)?;
        let hit = match skip_blank(r) {
            Some(id) => name_matches(r.doc.as_ref().unwrap(), id, local.as_deref(), ns.as_deref()),
            None => false,
        };
        Ok(hit)
    });
    match result {
        None => E_INVALIDARG,
        Some(Err((hr, message))) => fail(error, hr, &message),
        Some(Ok(hit)) => {
            if found != 0 {
                std::ptr::write_unaligned(found as *mut i32, hit as i32);
                S_OK
            } else if hit {
                S_OK
            } else {
                fail(
                    error,
                    WS_E_INVALID_FORMAT,
                    &format!(
                        "élément « {} » attendu",
                        local.as_deref().unwrap_or("(quelconque)")
                    ),
                )
            }
        }
    }
}

pub unsafe extern "win64" fn ws_read_start_element(reader: usize, error: usize) -> i32 {
    let result = with_reader(reader, |r| {
        ensure_parsed(r)?;
        match skip_blank(r) {
            Some(_) => {
                r.pos += 1;
                Ok(())
            }
            None => Err((WS_E_INVALID_FORMAT, "début d'élément attendu".to_string())),
        }
    });
    match result {
        None => E_INVALIDARG,
        Some(Ok(())) => S_OK,
        Some(Err((hr, message))) => fail(error, hr, &message),
    }
}

// ---------- Désérialisation (WsReadType) ----------

/// Contexte d'une lecture : taille des pointeurs du programme et tas WS.
struct Ctx<'a> {
    doc: &'a Doc,
    ps: usize,
    heap: usize,
}

type Fail = (i32, String);

fn bad(message: String) -> Fail {
    (WS_E_INVALID_FORMAT, message)
}

/// Taille d'une valeur du type dans la mémoire du programme.
unsafe fn type_size(ty: u32, desc: usize, ps: usize) -> Result<usize, Fail> {
    Ok(match ty {
        WS_BOOL_TYPE | WS_INT32_TYPE | WS_UINT32_TYPE | WS_FLOAT_TYPE | WS_ENUM_TYPE => 4,
        WS_INT8_TYPE | WS_UINT8_TYPE => 1,
        WS_INT16_TYPE | WS_UINT16_TYPE => 2,
        WS_INT64_TYPE | WS_UINT64_TYPE | WS_DOUBLE_TYPE => 8,
        WS_WSZ_TYPE => ps,
        WS_STRING_TYPE => 2 * ps,
        WS_XML_STRING_TYPE => 4 * ps,
        WS_STRUCT_TYPE => {
            if desc == 0 {
                return Err((E_INVALIDARG, "WS_STRUCT_TYPE sans description".into()));
            }
            rd32(desc) as usize
        }
        other => return Err(unsupported(other)),
    })
}

fn unsupported(ty: u32) -> Fail {
    crate::log_warn!(
        "webservices",
        "WsReadType : WS_TYPE {ty} pas encore pris en charge (webservices.rs:{})",
        line!()
    );
    (E_NOTIMPL, format!("WS_TYPE {ty} non pris en charge"))
}

/// Texte d'un élément : ses nœuds texte et CDATA, descendants compris.
fn element_text(doc: &Doc, id: usize) -> String {
    doc.text(id)
}

/// Écrit dans `dst` (taille `type_size`) la valeur simple lue dans `text`.
unsafe fn write_simple(cx: &Ctx, ty: u32, desc: usize, text: &str, dst: usize) -> Result<(), Fail> {
    let t = text.trim();
    let num = |kind: &str| bad(format!("« {t} » n'est pas un {kind}"));
    match ty {
        WS_BOOL_TYPE => {
            let v = match t {
                "true" | "1" => 1i32,
                "false" | "0" => 0,
                _ => return Err(num("booléen")),
            };
            std::ptr::write_unaligned(dst as *mut i32, v);
        }
        WS_INT8_TYPE => *(dst as *mut i8) = t.parse().map_err(|_| num("INT8"))?,
        WS_UINT8_TYPE => *(dst as *mut u8) = t.parse().map_err(|_| num("UINT8"))?,
        WS_INT16_TYPE => {
            std::ptr::write_unaligned(dst as *mut i16, t.parse().map_err(|_| num("INT16"))?)
        }
        WS_UINT16_TYPE => {
            std::ptr::write_unaligned(dst as *mut u16, t.parse().map_err(|_| num("UINT16"))?)
        }
        WS_INT32_TYPE => {
            std::ptr::write_unaligned(dst as *mut i32, t.parse().map_err(|_| num("INT32"))?)
        }
        WS_UINT32_TYPE => {
            std::ptr::write_unaligned(dst as *mut u32, t.parse().map_err(|_| num("UINT32"))?)
        }
        WS_INT64_TYPE => {
            std::ptr::write_unaligned(dst as *mut i64, t.parse().map_err(|_| num("INT64"))?)
        }
        WS_UINT64_TYPE => {
            std::ptr::write_unaligned(dst as *mut u64, t.parse().map_err(|_| num("UINT64"))?)
        }
        WS_FLOAT_TYPE => {
            std::ptr::write_unaligned(dst as *mut f32, t.parse().map_err(|_| num("FLOAT"))?)
        }
        WS_DOUBLE_TYPE => {
            std::ptr::write_unaligned(dst as *mut f64, t.parse().map_err(|_| num("DOUBLE"))?)
        }
        WS_WSZ_TYPE => {
            let wide: Vec<u16> = text.encode_utf16().collect();
            let p = heap_alloc(cx.heap, (wide.len() + 1) * 2);
            if p == 0 {
                return Err((E_OUTOFMEMORY, "tas épuisé".into()));
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), p as *mut u16, wide.len());
            wrp(dst, p, cx.ps);
        }
        WS_STRING_TYPE => {
            let wide: Vec<u16> = text.encode_utf16().collect();
            let p = heap_alloc(cx.heap, (wide.len() + 1) * 2);
            if p == 0 {
                return Err((E_OUTOFMEMORY, "tas épuisé".into()));
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), p as *mut u16, wide.len());
            std::ptr::write_unaligned(dst as *mut u32, wide.len() as u32);
            wrp(dst + cx.ps, p, cx.ps);
        }
        WS_XML_STRING_TYPE => {
            let bytes = text.as_bytes();
            let p = heap_alloc(cx.heap, bytes.len() + 1);
            if p == 0 {
                return Err((E_OUTOFMEMORY, "tas épuisé".into()));
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p as *mut u8, bytes.len());
            std::ptr::write_unaligned(dst as *mut u32, bytes.len() as u32);
            wrp(dst + cx.ps, p, cx.ps);
        }
        WS_ENUM_TYPE => {
            // WS_ENUM_DESCRIPTION { values, valueCount, … } ;
            // WS_ENUM_VALUE { value (int), name (WS_XML_STRING*) }.
            if desc == 0 {
                return Err((E_INVALIDARG, "WS_ENUM_TYPE sans description".into()));
            }
            let values = rdp(desc, cx.ps);
            let count = rd32(desc + cx.ps) as usize;
            let entry = 2 * cx.ps;
            let hit = (0..count.min(4096)).find_map(|i| {
                let at = values + i * entry;
                (xml_string(rdp(at + cx.ps, cx.ps), cx.ps).as_deref() == Some(t))
                    .then(|| std::ptr::read_unaligned(at as *const i32))
            });
            let v = hit.ok_or_else(|| bad(format!("valeur d'énumération « {t} » inconnue")))?;
            std::ptr::write_unaligned(dst as *mut i32, v);
        }
        other => return Err(unsupported(other)),
    }
    Ok(())
}

/// Lit l'élément `id` comme une valeur du type, écrite dans `dst`.
unsafe fn read_element(cx: &Ctx, id: usize, ty: u32, desc: usize, dst: usize) -> Result<(), Fail> {
    if ty == WS_STRUCT_TYPE {
        let children: Vec<usize> = cx.doc.node(id).children.clone();
        read_struct(cx, id, &children, desc, dst)
    } else {
        write_simple(cx, ty, desc, &element_text(cx.doc, id), dst)
    }
}

/// Champs d'un WS_STRUCT_DESCRIPTION lus depuis l'élément `owner` (attributs,
/// texte) et ses enfants `children`, dans l'ordre des champs.
unsafe fn read_struct(
    cx: &Ctx,
    owner: usize,
    children: &[usize],
    desc: usize,
    dst: usize,
) -> Result<(), Fail> {
    let ps = cx.ps;
    // WS_STRUCT_DESCRIPTION { size, alignment, fields, fieldCount, … }.
    let fields = rdp(desc + 8, ps);
    let count = rd32(desc + 8 + ps) as usize;
    let elements: Vec<usize> = children
        .iter()
        .copied()
        .filter(|&c| cx.doc.node(c).kind == Kind::Element)
        .collect();
    let mut next = 0usize;
    for i in 0..count.min(1024) {
        let f = rdp(fields + i * ps, ps);
        if f == 0 {
            continue;
        }
        // WS_FIELD_DESCRIPTION { mapping, localName, ns, type, typeDescription,
        // offset, options, defaultValue, countOffset, itemLocalName, itemNs,
        // itemRange } : x86 0/4/8/12/16/20/24/28/32/36/40/44,
        // x64 0/8/16/24/32/40/44/48/56/64/72/80.
        let (
            o_local,
            o_ns,
            o_type,
            o_desc,
            o_offset,
            o_options,
            o_default,
            o_count,
            o_item,
            o_item_ns,
        ) = if ps == 4 {
            (4, 8, 12, 16, 20, 24, 28, 32, 36, 40)
        } else {
            (8, 16, 24, 32, 40, 44, 48, 56, 64, 72)
        };
        let mapping = rd32(f);
        let local = xml_string(rdp(f + o_local, ps), ps);
        let ns = xml_string(rdp(f + o_ns, ps), ps);
        let ty = rd32(f + o_type);
        let tdesc = rdp(f + o_desc, ps);
        let at = dst + rd32(f + o_offset) as usize;
        let options = rd32(f + o_options);
        let default = rdp(f + o_default, ps);
        let pointer = options & WS_FIELD_POINTER != 0;
        let optional = options & (WS_FIELD_OPTIONAL | WS_FIELD_NILLABLE) != 0;
        let name = local.clone().unwrap_or_default();
        // Valeur d'un type simple lue dans un texte (attribut ou contenu).
        let put_text = |text: &str| -> Result<(), Fail> {
            if pointer {
                let size = type_size(ty, tdesc, ps)?;
                let p = heap_alloc(cx.heap, size);
                write_simple(cx, ty, tdesc, text, p)?;
                wrp(at, p, ps);
                Ok(())
            } else {
                write_simple(cx, ty, tdesc, text, at)
            }
        };
        let put_default = || -> Result<(), Fail> {
            // WS_DEFAULT_VALUE { value, valueSize } ; sinon zéro, déjà là.
            if default != 0 && !pointer {
                let value = rdp(default, ps);
                let size = rd32(default + ps) as usize;
                if value != 0 && size != 0 {
                    std::ptr::copy_nonoverlapping(value as *const u8, at as *mut u8, size);
                }
            }
            Ok(())
        };
        match mapping {
            WS_TYPE_ATTRIBUTE_FIELD_MAPPING | WS_NO_FIELD_MAPPING => put_default()?,
            WS_ATTRIBUTE_FIELD_MAPPING => {
                let attr = cx.doc.node(owner).attrs.iter().copied().find(|&a| {
                    let (prefix, base) = cx.doc.prefix_and_base(a);
                    prefix != "xmlns"
                        && cx.doc.node(a).name != "xmlns"
                        && local.as_deref().is_none_or(|l| base == l)
                        && ns.as_deref().is_none_or(|n| cx.doc.namespace_uri(a) == n)
                });
                match attr {
                    Some(a) => put_text(&cx.doc.node(a).value.clone())?,
                    None if optional => put_default()?,
                    None => return Err(bad(format!("attribut « {name} » manquant"))),
                }
            }
            WS_TEXT_FIELD_MAPPING => put_text(&element_text(cx.doc, owner))?,
            WS_ELEMENT_FIELD_MAPPING => {
                let hit = elements[next..]
                    .iter()
                    .position(|&c| name_matches(cx.doc, c, local.as_deref(), ns.as_deref()));
                match hit {
                    Some(k) => {
                        let c = elements[next + k];
                        next += k + 1;
                        if pointer {
                            let size = type_size(ty, tdesc, ps)?;
                            let p = heap_alloc(cx.heap, size);
                            read_element(cx, c, ty, tdesc, p)?;
                            wrp(at, p, ps);
                        } else {
                            read_element(cx, c, ty, tdesc, at)?;
                        }
                    }
                    None if optional => put_default()?,
                    None => return Err(bad(format!("élément « {name} » manquant"))),
                }
            }
            WS_REPEATING_ELEMENT_FIELD_MAPPING => {
                let item = xml_string(rdp(f + o_item, ps), ps);
                let item_ns = xml_string(rdp(f + o_item_ns, ps), ps);
                // Avec localName : éléments enveloppés dans <localName> ;
                // sans : directement parmi les enfants.
                let items: Vec<usize> = if local.is_some() {
                    let wrapper = elements[next..]
                        .iter()
                        .position(|&c| name_matches(cx.doc, c, local.as_deref(), ns.as_deref()));
                    match wrapper {
                        Some(k) => {
                            let w = elements[next + k];
                            next += k + 1;
                            cx.doc
                                .node(w)
                                .children
                                .iter()
                                .copied()
                                .filter(|&c| {
                                    cx.doc.node(c).kind == Kind::Element
                                        && name_matches(
                                            cx.doc,
                                            c,
                                            item.as_deref(),
                                            item_ns.as_deref(),
                                        )
                                })
                                .collect()
                        }
                        None => Vec::new(),
                    }
                } else {
                    let mut v = Vec::new();
                    while let Some(&c) = elements.get(next) {
                        if !name_matches(cx.doc, c, item.as_deref(), item_ns.as_deref()) {
                            break;
                        }
                        v.push(c);
                        next += 1;
                    }
                    v
                };
                let size = type_size(ty, tdesc, ps)?;
                let stride = if pointer { ps } else { size };
                let array = if items.is_empty() {
                    0
                } else {
                    heap_alloc(cx.heap, stride * items.len())
                };
                for (k, &c) in items.iter().enumerate() {
                    let slot = array + k * stride;
                    if pointer {
                        let p = heap_alloc(cx.heap, size);
                        read_element(cx, c, ty, tdesc, p)?;
                        wrp(slot, p, ps);
                    } else {
                        read_element(cx, c, ty, tdesc, slot)?;
                    }
                }
                wrp(at, array, ps);
                std::ptr::write_unaligned(
                    (dst + rd32(f + o_count) as usize) as *mut u32,
                    items.len() as u32,
                );
            }
            other => {
                crate::log_warn!(
                    "webservices",
                    "WsReadType : WS_FIELD_MAPPING {other} pas encore pris en charge (webservices.rs:{})",
                    line!()
                );
                return Err((
                    E_NOTIMPL,
                    format!("WS_FIELD_MAPPING {other} non pris en charge"),
                ));
            }
        }
    }
    Ok(())
}

/// Destination d'une valeur selon WS_READ_OPTION : la valeur elle-même
/// (`value`, `valueSize` octets) ou un pointeur vers un bloc du tas.
unsafe fn store(
    cx: &Ctx,
    option: u32,
    ty: u32,
    desc: usize,
    value: usize,
    value_size: u32,
    fill: impl FnOnce(usize) -> Result<(), Fail>,
) -> Result<(), Fail> {
    let size = type_size(ty, desc, cx.ps)?;
    // WS_WSZ_TYPE est lui-même un pointeur : avec une option « pointeur », la
    // chaîne est rendue directement, et les options « valeur » sont refusées
    // (WS_READ_OPTION, Microsoft Learn).
    if ty == WS_WSZ_TYPE {
        return match option {
            WS_READ_REQUIRED_POINTER | WS_READ_OPTIONAL_POINTER | WS_READ_NILLABLE_POINTER
                if value_size as usize == cx.ps =>
            {
                fill(value)
            }
            _ => Err((E_INVALIDARG, "WS_WSZ_TYPE se lit par pointeur".into())),
        };
    }
    match option {
        WS_READ_REQUIRED_VALUE | WS_READ_NILLABLE_VALUE => {
            if (value_size as usize) != size {
                return Err((
                    E_INVALIDARG,
                    format!("taille {value_size} au lieu de {size}"),
                ));
            }
            fill(value)
        }
        WS_READ_REQUIRED_POINTER | WS_READ_OPTIONAL_POINTER | WS_READ_NILLABLE_POINTER => {
            if (value_size as usize) != cx.ps {
                return Err((
                    E_INVALIDARG,
                    format!("taille {value_size} pour un pointeur"),
                ));
            }
            let p = heap_alloc(cx.heap, size);
            if p == 0 {
                return Err((E_OUTOFMEMORY, "tas épuisé".into()));
            }
            fill(p)?;
            wrp(value, p, cx.ps);
            Ok(())
        }
        other => Err((E_INVALIDARG, format!("WS_READ_OPTION {other}"))),
    }
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn ws_read_type(
    reader: usize,
    mapping: u32,
    ty: u32,
    desc: usize,
    option: u32,
    heap: usize,
    value: usize,
    value_size: u32,
    error: usize,
) -> i32 {
    if value == 0 {
        return E_INVALIDARG;
    }
    let ps = ptr_size();
    let result = with_reader(reader, |r| -> Result<(), Fail> {
        ensure_parsed(r)?;
        match mapping {
            WS_ELEMENT_TYPE_MAPPING => {
                let id = match skip_blank(r) {
                    Some(id) => id,
                    None if option == WS_READ_OPTIONAL_POINTER => {
                        wrp(value, 0, ps);
                        return Ok(());
                    }
                    None => return Err(bad("élément attendu pour WsReadType".into())),
                };
                let doc = r.doc.as_ref().unwrap();
                let cx = Ctx { doc, ps, heap };
                store(&cx, option, ty, desc, value, value_size, |dst| {
                    read_element(&cx, id, ty, desc, dst)
                })?;
                // Au-delà de la fin de l'élément lu.
                let end = r.events[r.pos..]
                    .iter()
                    .position(|e| *e == Event::End(id))
                    .map_or(r.events.len(), |k| r.pos + k + 1);
                r.pos = end;
                Ok(())
            }
            WS_ELEMENT_CONTENT_TYPE_MAPPING => {
                // Contenu de l'élément ouvert par WsReadStartElement : du
                // lecteur jusqu'à sa fin, qui n'est pas consommée.
                let start = r.pos;
                let mut depth = 0usize;
                let mut end = r.events.len();
                for (k, e) in r.events[start..].iter().enumerate() {
                    match e {
                        Event::Start(_) => depth += 1,
                        Event::End(_) if depth == 0 => {
                            end = start + k;
                            break;
                        }
                        Event::End(_) => depth -= 1,
                        Event::Text(_) => {}
                    }
                }
                let doc = r.doc.as_ref().unwrap();
                let owner = match r.events.get(end) {
                    Some(Event::End(p)) => *p,
                    _ => DOC,
                };
                let span: Vec<usize> = r.events[start..end]
                    .iter()
                    .filter_map(|e| match e {
                        Event::Start(id) | Event::Text(id) => Some(*id),
                        Event::End(_) => None,
                    })
                    .filter(|&id| doc.node(id).parent == Some(owner))
                    .collect();
                let cx = Ctx { doc, ps, heap };
                store(&cx, option, ty, desc, value, value_size, |dst| {
                    if ty == WS_STRUCT_TYPE {
                        read_struct(&cx, owner, &span, desc, dst)
                    } else {
                        let text: String = span
                            .iter()
                            .filter(|&&id| doc.node(id).kind != Kind::Element)
                            .map(|&id| doc.node(id).value.as_str())
                            .collect();
                        write_simple(&cx, ty, desc, &text, dst)
                    }
                })?;
                r.pos = end;
                Ok(())
            }
            other => {
                crate::log_warn!(
                    "webservices",
                    "WsReadType : WS_TYPE_MAPPING {other} pas encore pris en charge (webservices.rs:{})",
                    line!()
                );
                Err((
                    E_NOTIMPL,
                    format!("WS_TYPE_MAPPING {other} non pris en charge"),
                ))
            }
        }
    });
    match result {
        None => E_INVALIDARG,
        Some(Ok(())) => S_OK,
        Some(Err((hr, message))) => fail(error, hr, &message),
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WsCreateError" => fn ws_create_error,
        "WsFreeError" => fn ws_free_error,
        "WsGetErrorProperty" => fn ws_get_error_property,
        "WsGetErrorString" => fn ws_get_error_string,
        "WsCreateHeap" => fn ws_create_heap,
        "WsFreeHeap" => fn ws_free_heap,
        "WsCreateWriter" => fn ws_create_writer,
        "WsFreeWriter" => fn ws_free_writer,
        "WsCreateReader" => fn ws_create_reader,
        "WsFreeReader" => fn ws_free_reader,
        "WsSetInput" => fn ws_set_input,
        "WsFillReader" => fn ws_fill_reader,
        "WsReadToStartElement" => fn ws_read_to_start_element,
        "WsReadStartElement" => fn ws_read_start_element,
        "WsReadType" => fn ws_read_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WS_XML_STRING x64 : { length, bytes, dictionary, id } sur 4 cases.
    fn xs(s: &'static str) -> Box<[usize; 4]> {
        Box::new([s.len(), s.as_ptr() as usize, 0, 0])
    }

    unsafe fn wide_at(p: usize) -> String {
        let mut n = 0;
        while *((p as *const u16).add(n)) != 0 {
            n += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p as *const u16, n))
    }

    unsafe fn reader_on(xml: &'static str) -> usize {
        let mut r = 0usize;
        assert_eq!(
            ws_create_reader(0, 0, &mut r as *mut usize as usize, 0),
            S_OK
        );
        // WS_XML_READER_TEXT_ENCODING { { TEXT }, UTF8 } ;
        // WS_XML_READER_BUFFER_INPUT { { BUFFER }, encodedData, size }.
        let encoding = [WS_XML_READER_ENCODING_TYPE_TEXT, 1];
        let input = [
            WS_XML_READER_INPUT_TYPE_BUFFER as usize,
            xml.as_ptr() as usize,
            xml.len(),
        ];
        let hr = ws_set_input(
            r,
            encoding.as_ptr() as usize,
            input.as_ptr() as usize,
            0,
            0,
            0,
        );
        assert_eq!(hr, S_OK);
        r
    }

    #[test]
    fn lecture_d_un_element_et_erreur_nommee() {
        unsafe {
            let r =
                reader_on("<?xml version=\"1.0\"?>\n<Racine>\n  <Nom>Office 16</Nom>\n</Racine>");
            let mut heap = 0usize;
            assert_eq!(
                ws_create_heap(1 << 20, 0, 0, 0, &mut heap as *mut usize as usize, 0),
                S_OK
            );
            let racine = xs("Racine");
            let nom = xs("Nom");
            let mut found = 0i32;
            let hr = ws_read_to_start_element(
                r,
                racine.as_ptr() as usize,
                0,
                &mut found as *mut i32 as usize,
                0,
            );
            assert_eq!((hr, found), (S_OK, 1));
            assert_eq!(ws_read_start_element(r, 0), S_OK);
            let hr = ws_read_to_start_element(r, nom.as_ptr() as usize, 0, 0, 0);
            assert_eq!(hr, S_OK);
            let mut text = 0usize;
            let hr = ws_read_type(
                r,
                WS_ELEMENT_TYPE_MAPPING,
                WS_WSZ_TYPE,
                0,
                WS_READ_REQUIRED_POINTER,
                heap,
                &mut text as *mut usize as usize,
                8,
                0,
            );
            assert_eq!(hr, S_OK);
            assert_eq!(wide_at(text), "Office 16");
            // Plus d'élément : WS_E_INVALID_FORMAT, et la raison dans l'erreur.
            let mut error = 0usize;
            assert_eq!(
                ws_create_error(0, 0, &mut error as *mut usize as usize),
                S_OK
            );
            let hr = ws_read_to_start_element(r, nom.as_ptr() as usize, 0, 0, error);
            assert_eq!(hr, WS_E_INVALID_FORMAT);
            let mut count = 0u32;
            let hr = ws_get_error_property(
                error,
                WS_ERROR_PROPERTY_STRING_COUNT,
                &mut count as *mut u32 as usize,
                4,
            );
            assert_eq!((hr, count), (S_OK, 1));
            let mut string = [0usize; 2];
            assert_eq!(
                ws_get_error_string(error, 0, string.as_mut_ptr() as usize),
                S_OK
            );
            let message = String::from_utf16_lossy(std::slice::from_raw_parts(
                string[1] as *const u16,
                string[0] as u32 as usize,
            ));
            assert!(message.contains("Nom"), "{message}");
            ws_free_error(error);
            ws_free_heap(heap);
            ws_free_reader(r);
        }
    }

    #[test]
    fn structure_attribut_element_et_repetition() {
        unsafe {
            let r = reader_on(
                "<Cfg xmlns=\"urn:x\" Version=\"16\"><Nom>Office</Nom><Url>a</Url><Url>b</Url><Mode>Pro</Mode></Cfg>",
            );
            let (version, nom, url, mode, cfg, ns) = (
                xs("Version"),
                xs("Nom"),
                xs("Url"),
                xs("Mode"),
                xs("Cfg"),
                xs("urn:x"),
            );
            // Structure lue : { u32 version ; WCHAR* nom ; WCHAR** urls ;
            // u32 n ; int mode } = 0, 8, 16, 24, 28 ; 32 octets.
            // WS_FIELD_DESCRIPTION x64 : 11 cases (dernière : itemRange).
            let field = |mapping: u32,
                         local: usize,
                         ns: usize,
                         ty: u32,
                         desc: usize,
                         offset: u32,
                         options: u32,
                         count: usize,
                         item: usize| {
                let mut f = Box::new([0usize; 11]);
                f[0] = mapping as usize;
                f[1] = local;
                f[2] = ns;
                f[3] = ty as usize;
                f[4] = desc;
                f[5] = offset as usize | (options as usize) << 32;
                f[7] = count;
                f[8] = item;
                f[9] = ns_item(ns);
                f
            };
            fn ns_item(ns: usize) -> usize {
                ns
            }
            let enum_values = [(0usize, xs("Std")), (1usize, xs("Pro"))];
            let values: Vec<usize> = enum_values
                .iter()
                .flat_map(|(v, n)| [*v, n.as_ptr() as usize])
                .collect();
            let enum_desc = [values.as_ptr() as usize, 2, 0];
            let f_version = field(
                WS_ATTRIBUTE_FIELD_MAPPING,
                version.as_ptr() as usize,
                0,
                WS_UINT32_TYPE,
                0,
                0,
                0,
                0,
                0,
            );
            let f_nom = field(
                WS_ELEMENT_FIELD_MAPPING,
                nom.as_ptr() as usize,
                ns.as_ptr() as usize,
                WS_WSZ_TYPE,
                0,
                8,
                0,
                0,
                0,
            );
            let f_urls = field(
                WS_REPEATING_ELEMENT_FIELD_MAPPING,
                0,
                ns.as_ptr() as usize,
                WS_WSZ_TYPE,
                0,
                16,
                0,
                24,
                url.as_ptr() as usize,
            );
            let f_mode = field(
                WS_ELEMENT_FIELD_MAPPING,
                mode.as_ptr() as usize,
                ns.as_ptr() as usize,
                WS_ENUM_TYPE,
                enum_desc.as_ptr() as usize,
                28,
                WS_FIELD_OPTIONAL,
                0,
                0,
            );
            let fields = [
                f_version.as_ptr() as usize,
                f_nom.as_ptr() as usize,
                f_urls.as_ptr() as usize,
                f_mode.as_ptr() as usize,
            ];
            // WS_STRUCT_DESCRIPTION x64 : size, alignment | fields | fieldCount…
            let desc = [
                32usize | 8 << 32,
                fields.as_ptr() as usize,
                4,
                0,
                0,
                0,
                0,
                0,
            ];
            // WS_ELEMENT_DESCRIPTION n'est pas utile ici : WsReadType reçoit
            // directement la description de la structure.
            let _ = (cfg, ns_item(0));
            let mut heap = 0usize;
            ws_create_heap(1 << 20, 0, 0, 0, &mut heap as *mut usize as usize, 0);
            let mut out = [0u8; 32];
            let hr = ws_read_type(
                r,
                WS_ELEMENT_TYPE_MAPPING,
                WS_STRUCT_TYPE,
                desc.as_ptr() as usize,
                WS_READ_REQUIRED_VALUE,
                heap,
                out.as_mut_ptr() as usize,
                32,
                0,
            );
            assert_eq!(hr, S_OK);
            let at = out.as_ptr() as usize;
            assert_eq!(rd32(at), 16);
            assert_eq!(wide_at(rdp(at + 8, 8)), "Office");
            let urls = rdp(at + 16, 8);
            assert_eq!(rd32(at + 24), 2);
            assert_eq!(wide_at(rdp(urls, 8)), "a");
            assert_eq!(wide_at(rdp(urls + 8, 8)), "b");
            assert_eq!(rd32(at + 28), 1);
            ws_free_heap(heap);
            ws_free_reader(r);
        }
    }

    #[test]
    fn entree_en_flux_par_le_rappel() {
        unsafe extern "win64" fn rappel(
            state: usize,
            bytes: usize,
            max: u32,
            actual: usize,
            _async: usize,
            _error: usize,
        ) -> i32 {
            // state : (texte, position) ; 7 octets au plus par appel.
            let st = &mut *(state as *mut (&'static str, usize));
            let rest = &st.0.as_bytes()[st.1..];
            let n = rest.len().min(7).min(max as usize);
            std::ptr::copy_nonoverlapping(rest.as_ptr(), bytes as *mut u8, n);
            st.1 += n;
            *(actual as *mut u32) = n as u32;
            S_OK
        }
        unsafe {
            let mut r = 0usize;
            ws_create_reader(0, 0, &mut r as *mut usize as usize, 0);
            let mut state: (&'static str, usize) = ("<v>42</v>", 0);
            let input = [
                WS_XML_READER_INPUT_TYPE_STREAM as usize,
                rappel as usize,
                &mut state as *mut _ as usize,
            ];
            assert_eq!(ws_set_input(r, 0, input.as_ptr() as usize, 0, 0, 0), S_OK);
            assert_eq!(ws_fill_reader(r, 1 << 16, 0, 0), S_OK);
            let mut v = 0i32;
            let hr = ws_read_type(
                r,
                WS_ELEMENT_TYPE_MAPPING,
                WS_INT32_TYPE,
                0,
                WS_READ_REQUIRED_VALUE,
                0,
                &mut v as *mut i32 as usize,
                4,
                0,
            );
            assert_eq!((hr, v), (S_OK, 42));
            ws_free_reader(r);
        }
    }
}
