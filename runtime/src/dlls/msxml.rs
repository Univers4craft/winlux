//! MSXML intégré (#1084) : classes DOMDocument (sans version, 2.6, 3.0,
//! 4.0, 6.0 et leurs variantes « FreeThreaded ») et interfaces
//! IXMLDOMDocument(2, 3), IXMLDOMNode, IXMLDOMElement, IXMLDOMAttribute,
//! IXMLDOMCharacterData/Text/Comment/CDATASection,
//! IXMLDOMProcessingInstruction, IXMLDOMDocumentFragment, IXMLDOMNodeList,
//! IXMLDOMNamedNodeMap et IXMLDOMParseError.
//!
//! Ordre des méthodes et identifiants : en-têtes publics msxml2.h et
//! msxml6.h de mingw-w64 ; comportements : documentation publique MSXML.
//! L'arbre lui-même est dans msxml_dom.rs.
//!
//! Les méthodes de liaison tardive (IDispatch::Invoke) ne sont pas servies
//! : les programmes compilés appellent les tables de méthodes.

use super::msxml_dom::{self as dom, Doc, Kind, DOC};
use super::ole32::Guid;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000eu32 as i32;
const DISP_E_UNKNOWNNAME: i32 = 0x8002_0006u32 as i32;
const DISP_E_MEMBERNOTFOUND: i32 = 0x8002_0003u32 as i32;

const VT_NULL: u16 = 1;
const VT_I4: u16 = 3;
const VT_BSTR: u16 = 8;
const VT_DISPATCH: u16 = 9;
const VT_UNKNOWN: u16 = 13;
const VT_BYREF: u16 = 0x4000;
const VARIANT_TRUE: i16 = -1;
const VARIANT_FALSE: i16 = 0;

const fn g(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Guid {
    Guid {
        data1: d1,
        data2: d2,
        data3: d3,
        data4: d4,
    }
}

const DOM_TAIL: [u8; 8] = [0xb2, 0x0e, 0x00, 0xc0, 0x4f, 0x98, 0x3e, 0x60];
const FRAG_TAIL: [u8; 8] = [0x83, 0x6f, 0x00, 0x00, 0xf8, 0x7a, 0x77, 0x82];

const IID_IUNKNOWN: Guid = g(0, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IDISPATCH: Guid = g(0x0002_0400, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
pub const IID_IXMLDOMNODE: Guid = g(0x2933_bf80, 0x7b36, 0x11d2, DOM_TAIL);
pub const IID_IXMLDOMDOCUMENT: Guid = g(0x2933_bf81, 0x7b36, 0x11d2, DOM_TAIL);
pub const IID_IXMLDOMNODELIST: Guid = g(0x2933_bf82, 0x7b36, 0x11d2, DOM_TAIL);
pub const IID_IXMLDOMNAMEDNODEMAP: Guid = g(0x2933_bf83, 0x7b36, 0x11d2, DOM_TAIL);
pub const IID_IXMLDOMCHARACTERDATA: Guid = g(0x2933_bf84, 0x7b36, 0x11d2, DOM_TAIL);
pub const IID_IXMLDOMATTRIBUTE: Guid = g(0x2933_bf85, 0x7b36, 0x11d2, DOM_TAIL);
pub const IID_IXMLDOMELEMENT: Guid = g(0x2933_bf86, 0x7b36, 0x11d2, DOM_TAIL);
pub const IID_IXMLDOMTEXT: Guid = g(0x2933_bf87, 0x7b36, 0x11d2, DOM_TAIL);
pub const IID_IXMLDOMCOMMENT: Guid = g(0x2933_bf88, 0x7b36, 0x11d2, DOM_TAIL);
pub const IID_IXMLDOMPROCESSINGINSTRUCTION: Guid = g(0x2933_bf89, 0x7b36, 0x11d2, DOM_TAIL);
pub const IID_IXMLDOMCDATASECTION: Guid = g(0x2933_bf8a, 0x7b36, 0x11d2, DOM_TAIL);
pub const IID_IXMLDOMDOCUMENT2: Guid = g(0x2933_bf95, 0x7b36, 0x11d2, DOM_TAIL);
pub const IID_IXMLDOMDOCUMENT3: Guid = g(0x2933_bf96, 0x7b36, 0x11d2, DOM_TAIL);
pub const IID_IXMLDOMDOCUMENTFRAGMENT: Guid = g(0x3efa_a413, 0x272f, 0x11d2, FRAG_TAIL);
pub const IID_IXMLDOMPARSEERROR: Guid = g(0x3efa_a426, 0x272f, 0x11d2, FRAG_TAIL);
const IID_ISTREAM: Guid = g(0x0000_000c, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_ISEQUENTIALSTREAM: Guid = g(
    0x0c73_3a30,
    0x2a1c,
    0x11ce,
    [0xad, 0xe5, 0, 0xaa, 0, 0x44, 0x77, 0x3d],
);

const MS_TAIL_26: [u8; 8] = [0x89, 0xb9, 0x00, 0x00, 0xf8, 0x1f, 0xe2, 0x21];
const MS_TAIL_40: [u8; 8] = [0xa6, 0x5f, 0x00, 0x40, 0x96, 0x32, 0x51, 0xe5];
const MS_TAIL_DD: [u8; 8] = [0xb3, 0x2e, 0x00, 0xc0, 0x4f, 0x99, 0x0b, 0xb4];

/// (CLSID, version) des classes de document servies.
const CLASSES: &[(Guid, u8)] = &[
    (g(0x2933_bf90, 0x7b36, 0x11d2, DOM_TAIL), 2), // Microsoft.XMLDOM
    (g(0xf6d9_0f11, 0x9c73, 0x11d3, MS_TAIL_DD), 3), // Msxml2.DOMDocument
    (g(0xf6d9_0f12, 0x9c73, 0x11d3, MS_TAIL_DD), 3),
    (g(0xf507_8f1b, 0xc551, 0x11d3, MS_TAIL_26), 2), // 2.6
    (g(0xf507_8f1c, 0xc551, 0x11d3, MS_TAIL_26), 2),
    (g(0xf507_8f32, 0xc551, 0x11d3, MS_TAIL_26), 3), // 3.0
    (g(0xf507_8f33, 0xc551, 0x11d3, MS_TAIL_26), 3),
    (g(0x88d9_69c0, 0xf192, 0x11d4, MS_TAIL_40), 4), // 4.0
    (g(0x88d9_69c1, 0xf192, 0x11d4, MS_TAIL_40), 4),
    (g(0x88d9_6a05, 0xf192, 0x11d4, MS_TAIL_40), 6), // 6.0
    (g(0x88d9_6a06, 0xf192, 0x11d4, MS_TAIL_40), 6),
];

type DocRef = Arc<Mutex<Doc>>;

enum What {
    Node(usize),
    /// Liste vivante des enfants d'un nœud.
    Children(usize),
    /// Liste figée (selectNodes, getElementsByTagName).
    Snapshot(Vec<usize>),
    /// Attributs vivants d'un élément.
    Attrs(usize),
    Error(dom::ParseError),
}

#[repr(C)]
struct Obj {
    vtbl: *const usize,
    refs: AtomicU32,
    doc: DocRef,
    what: What,
    /// Curseur de nextNode.
    cursor: AtomicUsize,
}

fn lock(d: &DocRef) -> std::sync::MutexGuard<'_, Doc> {
    d.lock().unwrap_or_else(|e| e.into_inner())
}

// ── Tables de méthodes ─────────────────────────────────────────────

struct Tables {
    doc: Vec<usize>,
    element: Vec<usize>,
    attribute: Vec<usize>,
    chardata: Vec<usize>,
    pi: Vec<usize>,
    fragment: Vec<usize>,
    list: Vec<usize>,
    map: Vec<usize>,
    error: Vec<usize>,
}
static TABLES: OnceLock<Tables> = OnceLock::new();

macro_rules! table {
    ($($f:expr),* $(,)?) => { vec![$($f as *const () as usize),*] };
}

fn tables() -> &'static Tables {
    TABLES.get_or_init(|| {
        let dispatch = table![
            query_interface,
            add_ref,
            release,
            type_info_count,
            type_info,
            ids_of_names,
            invoke
        ];
        let mut node = dispatch.clone();
        node.extend(table![
            n_get_node_name,
            n_get_node_value,
            n_put_node_value,
            n_get_node_type,
            n_get_parent_node,
            n_get_child_nodes,
            n_get_first_child,
            n_get_last_child,
            n_get_previous_sibling,
            n_get_next_sibling,
            n_get_attributes,
            n_insert_before,
            n_replace_child,
            n_remove_child,
            n_append_child,
            n_has_child_nodes,
            n_get_owner_document,
            n_clone_node,
            n_get_node_type_string,
            n_get_text,
            n_put_text,
            n_get_specified,
            n_out_null,
            n_get_node_typed_value,
            n_put_node_value,
            n_get_data_type,
            n_put_data_type,
            n_get_xml,
            n_transform_node,
            n_select_nodes,
            n_select_single_node,
            n_get_parsed,
            n_get_namespace_uri,
            n_get_prefix,
            n_get_base_name,
            n_transform_node_to_object,
        ]);
        assert_eq!(node.len(), 43);
        let mut doc = node.clone();
        doc.extend(table![
            d_out_null, // get_doctype
            d_out_null, // get_implementation
            d_get_document_element,
            d_putref_document_element,
            d_create_element,
            d_create_document_fragment,
            d_create_text_node,
            d_create_comment,
            d_create_cdata_section,
            d_create_processing_instruction,
            d_create_attribute,
            d_create_entity_reference,
            d_get_elements_by_tag_name,
            d_create_node,
            d_node_from_id,
            d_load,
            d_get_ready_state,
            d_get_parse_error,
            d_get_url,
            d_get_async,
            d_put_async,
            d_abort,
            d_load_xml,
            d_save,
            d_get_validate_on_parse,
            d_put_validate_on_parse,
            d_get_resolve_externals,
            d_put_resolve_externals,
            d_get_preserve_white_space,
            d_put_preserve_white_space,
            d_put_handler,
            d_put_handler,
            d_put_handler,
            d_out_null, // get_namespaces
            d_get_schemas,
            d_put_handler, // putref_schemas
            d_validate,
            d_set_property,
            d_get_property,
            d_validate_node,
            d_import_node,
        ]);
        assert_eq!(doc.len(), 84);
        let mut element = node.clone();
        element.extend(table![
            n_get_node_name, // get_tagName
            e_get_attribute,
            e_set_attribute,
            e_remove_attribute,
            e_get_attribute_node,
            e_set_attribute_node,
            e_remove_attribute_node,
            e_get_elements_by_tag_name,
            e_normalize,
        ]);
        let mut attribute = node.clone();
        attribute.extend(table![n_get_node_name, n_get_node_value, n_put_node_value]);
        let mut chardata = node.clone();
        chardata.extend(table![
            n_get_text, // get_data
            n_put_text, // put_data
            c_get_length,
            c_substring_data,
            c_append_data,
            c_insert_data,
            c_delete_data,
            c_replace_data,
            c_split_text,
        ]);
        let mut pi = node.clone();
        pi.extend(table![n_get_node_name, n_get_text, n_put_text]);
        let fragment = node;
        let mut list = dispatch.clone();
        list.extend(table![
            l_get_item,
            l_get_length,
            l_next_node,
            l_reset,
            l_new_enum
        ]);
        let mut map = dispatch.clone();
        map.extend(table![
            m_get_named_item,
            m_set_named_item,
            m_remove_named_item,
            l_get_item,
            l_get_length,
            m_get_qualified_item,
            m_remove_qualified_item,
            l_next_node,
            l_reset,
            l_new_enum,
        ]);
        let mut error = dispatch;
        error.extend(table![
            pe_get_error_code,
            pe_get_url,
            pe_get_reason,
            pe_get_src_text,
            pe_get_line,
            pe_get_linepos,
            pe_get_filepos,
        ]);
        Tables {
            doc,
            element,
            attribute,
            chardata,
            pi,
            fragment,
            list,
            map,
            error,
        }
    })
}

fn is_ours(p: usize) -> bool {
    if p == 0 {
        return false;
    }
    let vt = unsafe { *(p as *const *const usize) } as usize;
    let t = tables();
    [
        &t.doc,
        &t.element,
        &t.attribute,
        &t.chardata,
        &t.pi,
        &t.fragment,
        &t.list,
        &t.map,
        &t.error,
    ]
    .iter()
    .any(|v| v.as_ptr() as usize == vt)
}

fn new_obj(doc: &DocRef, what: What) -> *mut Obj {
    let t = tables();
    let vt = match &what {
        What::Node(id) => match lock(doc).node(*id).kind {
            Kind::Document => &t.doc,
            Kind::Element => &t.element,
            Kind::Attribute => &t.attribute,
            Kind::Text | Kind::CData | Kind::Comment => &t.chardata,
            Kind::Pi => &t.pi,
            Kind::Fragment => &t.fragment,
        },
        What::Children(_) | What::Snapshot(_) => &t.list,
        What::Attrs(_) => &t.map,
        What::Error(_) => &t.error,
    };
    Box::into_raw(Box::new(Obj {
        vtbl: vt.as_ptr(),
        refs: AtomicU32::new(1),
        doc: Arc::clone(doc),
        what,
        cursor: AtomicUsize::new(0),
    }))
}

unsafe fn obj<'a>(this: *mut Obj) -> &'a Obj {
    &*this
}

unsafe fn put<T>(out: *mut T, v: T) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = v;
    S_OK
}

/// Rend un nœud (ou NULL et S_FALSE).
unsafe fn put_node(doc: &DocRef, out: *mut *mut Obj, id: Option<usize>) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    match id {
        Some(id) => {
            *out = new_obj(doc, What::Node(id));
            S_OK
        }
        None => {
            *out = std::ptr::null_mut();
            S_FALSE
        }
    }
}

fn bstr(s: &str) -> *mut u16 {
    let w: Vec<u16> = s.encode_utf16().collect();
    unsafe { super::oleaut32::sys_alloc_string_len(w.as_ptr(), w.len() as u32) }
}

unsafe fn put_bstr(out: *mut *mut u16, s: &str) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = bstr(s);
    if (*out).is_null() {
        E_OUTOFMEMORY
    } else {
        S_OK
    }
}

unsafe fn put_bool(out: *mut i16, v: bool) -> i32 {
    put(out, if v { VARIANT_TRUE } else { VARIANT_FALSE })
}

unsafe fn read_bstr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let len = super::oleaut32::sys_string_len(p) as usize;
    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
}

/// VARIANT de sortie : chaîne, ou VT_NULL.
unsafe fn put_variant_str(out: *mut u8, s: Option<&str>) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    std::ptr::write_bytes(out, 0, 24);
    match s {
        Some(s) => {
            *(out as *mut u16) = VT_BSTR;
            *(out.add(8) as *mut *mut u16) = bstr(s);
            S_OK
        }
        None => {
            *(out as *mut u16) = VT_NULL;
            S_FALSE
        }
    }
}

/// Texte d'un VARIANT d'entrée (converti en chaîne).
unsafe fn variant_text(v: *const u8) -> Option<String> {
    if v.is_null() {
        return None;
    }
    let mut tmp = [0u8; 24];
    if super::oleaut32_var::change_type(tmp.as_mut_ptr().cast(), v.cast(), 0x409, 0, VT_BSTR) < 0 {
        return None;
    }
    let s = read_bstr(*(tmp.as_ptr().add(8) as *const *const u16));
    super::oleaut32::variant_clear(tmp.as_mut_ptr().cast());
    Some(s)
}

/// Objet COM (VT_UNKNOWN, VT_DISPATCH, éventuellement par référence).
unsafe fn variant_object(v: *const u8) -> Option<usize> {
    if v.is_null() {
        return None;
    }
    let vt = *(v as *const u16);
    let p = match vt {
        VT_UNKNOWN | VT_DISPATCH => *(v.add(8) as *const usize),
        x if x == VT_BYREF | VT_UNKNOWN || x == VT_BYREF | VT_DISPATCH => {
            let r = *(v.add(8) as *const *const usize);
            if r.is_null() {
                0
            } else {
                *r
            }
        }
        _ => 0,
    };
    (p != 0).then_some(p)
}

/// Nœud d'un objet passé par le programme (un des nôtres seulement).
unsafe fn arg_node(p: *mut Obj) -> Option<(DocRef, usize)> {
    if !is_ours(p as usize) {
        return None;
    }
    match (*p).what {
        What::Node(id) => Some((Arc::clone(&(*p).doc), id)),
        _ => None,
    }
}

/// Amène le nœud `other` dans le document `doc` (copie s'il vient d'un
/// autre document).
fn adopt(doc: &DocRef, other: &(DocRef, usize)) -> usize {
    if Arc::ptr_eq(doc, &other.0) {
        return other.1;
    }
    let src = lock(&other.0);
    lock(doc).import(&src, other.1, true)
}

fn this_node(o: &Obj) -> usize {
    match o.what {
        What::Node(id) => id,
        _ => DOC,
    }
}

// ── IUnknown / IDispatch ───────────────────────────────────────────

unsafe extern "win64" fn query_interface(
    this: *mut Obj,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if iid.is_null() {
        return E_INVALIDARG;
    }
    let o = obj(this);
    let iid = *iid;
    let ok = iid == IID_IUNKNOWN
        || iid == IID_IDISPATCH
        || match &o.what {
            What::Node(id) => {
                let kind = lock(&o.doc).node(*id).kind;
                iid == IID_IXMLDOMNODE
                    || match kind {
                        Kind::Document => {
                            iid == IID_IXMLDOMDOCUMENT
                                || iid == IID_IXMLDOMDOCUMENT2
                                || iid == IID_IXMLDOMDOCUMENT3
                        }
                        Kind::Element => iid == IID_IXMLDOMELEMENT,
                        Kind::Attribute => iid == IID_IXMLDOMATTRIBUTE,
                        Kind::Text => iid == IID_IXMLDOMCHARACTERDATA || iid == IID_IXMLDOMTEXT,
                        Kind::CData => {
                            iid == IID_IXMLDOMCHARACTERDATA
                                || iid == IID_IXMLDOMTEXT
                                || iid == IID_IXMLDOMCDATASECTION
                        }
                        Kind::Comment => {
                            iid == IID_IXMLDOMCHARACTERDATA || iid == IID_IXMLDOMCOMMENT
                        }
                        Kind::Pi => iid == IID_IXMLDOMPROCESSINGINSTRUCTION,
                        Kind::Fragment => iid == IID_IXMLDOMDOCUMENTFRAGMENT,
                    }
            }
            What::Children(_) | What::Snapshot(_) => iid == IID_IXMLDOMNODELIST,
            What::Attrs(_) => iid == IID_IXMLDOMNAMEDNODEMAP,
            What::Error(_) => iid == IID_IXMLDOMPARSEERROR,
        };
    if !ok {
        return E_NOINTERFACE;
    }
    add_ref(this);
    *out = this.cast();
    S_OK
}
unsafe extern "win64" fn add_ref(this: *mut Obj) -> u32 {
    obj(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn release(this: *mut Obj) -> u32 {
    let left = obj(this).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if left == 0 {
        drop(Box::from_raw(this));
    }
    left
}
unsafe extern "win64" fn type_info_count(_this: *mut Obj, n: *mut u32) -> i32 {
    put(n, 0)
}
unsafe extern "win64" fn type_info(_this: *mut Obj, _i: u32, _lcid: u32, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}
unsafe extern "win64" fn ids_of_names(
    _this: *mut Obj,
    _riid: *const Guid,
    _names: usize,
    count: u32,
    _lcid: u32,
    ids: *mut i32,
) -> i32 {
    for i in 0..count as usize {
        if !ids.is_null() {
            *ids.add(i) = -1;
        }
    }
    DISP_E_UNKNOWNNAME
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn invoke(
    _this: *mut Obj,
    _id: i32,
    _riid: *const Guid,
    _lcid: u32,
    _flags: u16,
    _params: usize,
    _result: usize,
    _excep: usize,
    _arg_err: usize,
) -> i32 {
    DISP_E_MEMBERNOTFOUND
}

// ── IXMLDOMNode ────────────────────────────────────────────────────

unsafe extern "win64" fn n_get_node_name(this: *mut Obj, out: *mut *mut u16) -> i32 {
    let o = obj(this);
    let name = lock(&o.doc).node_name(this_node(o));
    put_bstr(out, &name)
}
unsafe extern "win64" fn n_get_node_value(this: *mut Obj, out: *mut u8) -> i32 {
    let o = obj(this);
    let d = lock(&o.doc);
    let id = this_node(o);
    match d.node(id).kind {
        Kind::Element | Kind::Document | Kind::Fragment => put_variant_str(out, None),
        _ => put_variant_str(out, Some(&d.node(id).value)),
    }
}
unsafe extern "win64" fn n_put_node_value(this: *mut Obj, v: *const u8) -> i32 {
    let o = obj(this);
    let id = this_node(o);
    if matches!(
        lock(&o.doc).node(id).kind,
        Kind::Element | Kind::Document | Kind::Fragment
    ) {
        return E_FAIL;
    }
    let Some(text) = variant_text(v) else {
        return E_INVALIDARG;
    };
    lock(&o.doc).set_text(id, &text);
    S_OK
}
unsafe extern "win64" fn n_get_node_type(this: *mut Obj, out: *mut i32) -> i32 {
    let o = obj(this);
    let k = lock(&o.doc).node(this_node(o)).kind;
    put(out, k as i32)
}
unsafe extern "win64" fn n_get_parent_node(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    let o = obj(this);
    let id = this_node(o);
    let p = {
        let d = lock(&o.doc);
        // Le parent d'un attribut est nul en DOM.
        if d.node(id).kind == Kind::Attribute {
            None
        } else {
            d.node(id).parent
        }
    };
    put_node(&o.doc, out, p)
}
unsafe extern "win64" fn n_get_child_nodes(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    let o = obj(this);
    put(out, new_obj(&o.doc, What::Children(this_node(o))))
}
unsafe extern "win64" fn n_get_first_child(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    let o = obj(this);
    let c = lock(&o.doc).node(this_node(o)).children.first().copied();
    put_node(&o.doc, out, c)
}
unsafe extern "win64" fn n_get_last_child(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    let o = obj(this);
    let c = lock(&o.doc).node(this_node(o)).children.last().copied();
    put_node(&o.doc, out, c)
}
fn sibling(d: &Doc, id: usize, delta: isize) -> Option<usize> {
    let n = d.node(id);
    if n.kind == Kind::Attribute {
        return None;
    }
    let p = n.parent?;
    let kids = &d.node(p).children;
    let at = kids.iter().position(|&c| c == id)? as isize + delta;
    (at >= 0).then(|| kids.get(at as usize).copied()).flatten()
}
unsafe extern "win64" fn n_get_previous_sibling(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    let o = obj(this);
    let s = sibling(&lock(&o.doc), this_node(o), -1);
    put_node(&o.doc, out, s)
}
unsafe extern "win64" fn n_get_next_sibling(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    let o = obj(this);
    let s = sibling(&lock(&o.doc), this_node(o), 1);
    put_node(&o.doc, out, s)
}
unsafe extern "win64" fn n_get_attributes(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    let o = obj(this);
    let id = this_node(o);
    if out.is_null() {
        return E_INVALIDARG;
    }
    if lock(&o.doc).node(id).kind != Kind::Element {
        *out = std::ptr::null_mut();
        return S_FALSE;
    }
    *out = new_obj(&o.doc, What::Attrs(id));
    S_OK
}
/// insertBefore, appendChild : rend le nœud inséré.
unsafe fn insert(this: *mut Obj, new: *mut Obj, before: Option<usize>, out: *mut *mut Obj) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    let o = obj(this);
    let Some(child) = arg_node(new) else {
        return E_INVALIDARG;
    };
    let id = adopt(&o.doc, &child);
    let parent = this_node(o);
    if !lock(&o.doc).insert(parent, id, before) {
        return E_FAIL;
    }
    if !out.is_null() {
        if Arc::ptr_eq(&o.doc, &child.0) {
            add_ref(new);
            *out = new;
        } else {
            *out = new_obj(&o.doc, What::Node(id));
        }
    }
    S_OK
}
unsafe extern "win64" fn n_insert_before(
    this: *mut Obj,
    new: *mut Obj,
    before: *const u8,
    out: *mut *mut Obj,
) -> i32 {
    let reference = match variant_object(before) {
        Some(p) => match arg_node(p as *mut Obj) {
            Some((d, id)) if Arc::ptr_eq(&d, &obj(this).doc) => Some(id),
            _ => return E_INVALIDARG,
        },
        None => None,
    };
    insert(this, new, reference, out)
}
unsafe extern "win64" fn n_append_child(this: *mut Obj, new: *mut Obj, out: *mut *mut Obj) -> i32 {
    insert(this, new, None, out)
}
unsafe extern "win64" fn n_remove_child(
    this: *mut Obj,
    child: *mut Obj,
    out: *mut *mut Obj,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    let o = obj(this);
    let Some((d, id)) = arg_node(child) else {
        return E_INVALIDARG;
    };
    if !Arc::ptr_eq(&d, &o.doc) || lock(&o.doc).node(id).parent != Some(this_node(o)) {
        return E_INVALIDARG;
    }
    lock(&o.doc).detach(id);
    if !out.is_null() {
        add_ref(child);
        *out = child;
    }
    S_OK
}
unsafe extern "win64" fn n_replace_child(
    this: *mut Obj,
    new: *mut Obj,
    old: *mut Obj,
    out: *mut *mut Obj,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    let o = obj(this);
    let Some((d, old_id)) = arg_node(old) else {
        return E_INVALIDARG;
    };
    if !Arc::ptr_eq(&d, &o.doc) || lock(&o.doc).node(old_id).parent != Some(this_node(o)) {
        return E_INVALIDARG;
    }
    let hr = insert(this, new, Some(old_id), std::ptr::null_mut());
    if hr < 0 {
        return hr;
    }
    lock(&o.doc).detach(old_id);
    if !out.is_null() {
        add_ref(old);
        *out = old;
    }
    S_OK
}
unsafe extern "win64" fn n_has_child_nodes(this: *mut Obj, out: *mut i16) -> i32 {
    let o = obj(this);
    let has = !lock(&o.doc).node(this_node(o)).children.is_empty();
    let hr = put_bool(out, has);
    if hr == S_OK && !has {
        S_FALSE
    } else {
        hr
    }
}
unsafe extern "win64" fn n_get_owner_document(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    let o = obj(this);
    if this_node(o) == DOC {
        return put_node(&o.doc, out, None);
    }
    put_node(&o.doc, out, Some(DOC))
}
unsafe extern "win64" fn n_clone_node(this: *mut Obj, deep: i16, out: *mut *mut Obj) -> i32 {
    let o = obj(this);
    let id = this_node(o);
    if id == DOC {
        // Copie d'un document : nouveau document.
        let copy: DocRef = Arc::new(Mutex::new(Doc::new()));
        {
            let src = lock(&o.doc);
            let mut dst = lock(&copy);
            dst.preserve_ws = src.preserve_ws;
            dst.properties = src.properties.clone();
            if deep != 0 {
                for &c in &src.node(DOC).children {
                    let n = dst.import(&src, c, true);
                    dst.insert(DOC, n, None);
                }
            }
        }
        return put(out, new_obj(&copy, What::Node(DOC)));
    }
    let c = lock(&o.doc).clone_node(id, deep != 0);
    put_node(&o.doc, out, Some(c))
}
unsafe extern "win64" fn n_get_node_type_string(this: *mut Obj, out: *mut *mut u16) -> i32 {
    let o = obj(this);
    let s = lock(&o.doc).type_string(this_node(o));
    put_bstr(out, s)
}
unsafe extern "win64" fn n_get_text(this: *mut Obj, out: *mut *mut u16) -> i32 {
    let o = obj(this);
    let s = lock(&o.doc).text(this_node(o));
    put_bstr(out, &s)
}
unsafe extern "win64" fn n_put_text(this: *mut Obj, text: *const u16) -> i32 {
    let o = obj(this);
    let id = this_node(o);
    if id == DOC {
        return E_FAIL;
    }
    lock(&o.doc).set_text(id, &read_bstr(text));
    S_OK
}
unsafe extern "win64" fn n_get_specified(_this: *mut Obj, out: *mut i16) -> i32 {
    put_bool(out, true)
}
unsafe extern "win64" fn n_out_null(_this: *mut Obj, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    S_FALSE
}
unsafe extern "win64" fn n_get_node_typed_value(this: *mut Obj, out: *mut u8) -> i32 {
    let o = obj(this);
    let id = this_node(o);
    let d = lock(&o.doc);
    match d.node(id).kind {
        Kind::Document | Kind::Fragment => put_variant_str(out, None),
        _ => put_variant_str(out, Some(&d.text(id))),
    }
}
unsafe extern "win64" fn n_get_data_type(_this: *mut Obj, out: *mut u8) -> i32 {
    put_variant_str(out, None)
}
unsafe extern "win64" fn n_put_data_type(_this: *mut Obj, _t: *const u16) -> i32 {
    E_FAIL
}
unsafe extern "win64" fn n_get_xml(this: *mut Obj, out: *mut *mut u16) -> i32 {
    let o = obj(this);
    let s = lock(&o.doc).xml(this_node(o));
    put_bstr(out, &s)
}
unsafe extern "win64" fn n_transform_node(
    _this: *mut Obj,
    _style: *mut Obj,
    out: *mut *mut u16,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    crate::log_warn!("com", "MSXML : transformNode (XSLT) non pris en charge");
    E_NOTIMPL
}
unsafe extern "win64" fn n_transform_node_to_object(
    _this: *mut Obj,
    _style: *mut Obj,
    _out: *const u8,
) -> i32 {
    crate::log_warn!(
        "com",
        "MSXML : transformNodeToObject (XSLT) non pris en charge"
    );
    E_NOTIMPL
}
unsafe fn select(this: *mut Obj, expr: *const u16) -> Result<Vec<usize>, i32> {
    let o = obj(this);
    let e = read_bstr(expr);
    match lock(&o.doc).select(this_node(o), &e) {
        Some(v) => Ok(v),
        None => {
            crate::log_warn!("com", "MSXML : expression « {e} » non comprise");
            Err(E_FAIL)
        }
    }
}
unsafe extern "win64" fn n_select_nodes(
    this: *mut Obj,
    expr: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    match select(this, expr) {
        Ok(v) => put(out, new_obj(&obj(this).doc, What::Snapshot(v))),
        Err(e) => e,
    }
}
unsafe extern "win64" fn n_select_single_node(
    this: *mut Obj,
    expr: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    match select(this, expr) {
        Ok(v) => put_node(&obj(this).doc, out, v.first().copied()),
        Err(e) => e,
    }
}
unsafe extern "win64" fn n_get_parsed(_this: *mut Obj, out: *mut i16) -> i32 {
    put_bool(out, true)
}
unsafe extern "win64" fn n_get_namespace_uri(this: *mut Obj, out: *mut *mut u16) -> i32 {
    let o = obj(this);
    let s = lock(&o.doc).namespace_uri(this_node(o));
    put_bstr(out, &s)
}
unsafe extern "win64" fn n_get_prefix(this: *mut Obj, out: *mut *mut u16) -> i32 {
    let o = obj(this);
    let (p, _) = lock(&o.doc).prefix_and_base(this_node(o));
    put_bstr(out, &p)
}
unsafe extern "win64" fn n_get_base_name(this: *mut Obj, out: *mut *mut u16) -> i32 {
    let o = obj(this);
    let (_, b) = lock(&o.doc).prefix_and_base(this_node(o));
    put_bstr(out, &b)
}

// ── IXMLDOMDocument ────────────────────────────────────────────────

unsafe extern "win64" fn d_out_null(this: *mut Obj, out: *mut usize) -> i32 {
    n_out_null(this, out)
}
unsafe extern "win64" fn d_get_document_element(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    let o = obj(this);
    let e = lock(&o.doc).document_element();
    put_node(&o.doc, out, e)
}
unsafe extern "win64" fn d_putref_document_element(this: *mut Obj, element: *mut Obj) -> i32 {
    let o = obj(this);
    let Some(child) = arg_node(element) else {
        return E_INVALIDARG;
    };
    let id = adopt(&o.doc, &child);
    let mut d = lock(&o.doc);
    if d.node(id).kind != Kind::Element {
        return E_INVALIDARG;
    }
    match d.document_element() {
        Some(old) if old == id => S_OK,
        Some(old) => {
            let before = d.node(DOC).children.iter().position(|&c| c == old);
            d.detach(old);
            let at = before.and_then(|i| d.node(DOC).children.get(i).copied());
            if d.insert(DOC, id, at) {
                S_OK
            } else {
                E_FAIL
            }
        }
        None => {
            if d.insert(DOC, id, None) {
                S_OK
            } else {
                E_FAIL
            }
        }
    }
}
unsafe fn create(this: *mut Obj, kind: Kind, name: &str, value: &str, out: *mut *mut Obj) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let o = obj(this);
    let id = lock(&o.doc).create(kind, name, value);
    put(out, new_obj(&o.doc, What::Node(id)))
}
unsafe extern "win64" fn d_create_element(
    this: *mut Obj,
    name: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    let n = read_bstr(name);
    if n.is_empty() {
        return E_FAIL;
    }
    create(this, Kind::Element, &n, "", out)
}
unsafe extern "win64" fn d_create_document_fragment(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    create(this, Kind::Fragment, "", "", out)
}
unsafe extern "win64" fn d_create_text_node(
    this: *mut Obj,
    text: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    create(this, Kind::Text, "", &read_bstr(text), out)
}
unsafe extern "win64" fn d_create_comment(
    this: *mut Obj,
    text: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    create(this, Kind::Comment, "", &read_bstr(text), out)
}
unsafe extern "win64" fn d_create_cdata_section(
    this: *mut Obj,
    text: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    create(this, Kind::CData, "", &read_bstr(text), out)
}
unsafe extern "win64" fn d_create_processing_instruction(
    this: *mut Obj,
    target: *const u16,
    data: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    create(this, Kind::Pi, &read_bstr(target), &read_bstr(data), out)
}
unsafe extern "win64" fn d_create_attribute(
    this: *mut Obj,
    name: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    create(this, Kind::Attribute, &read_bstr(name), "", out)
}
unsafe extern "win64" fn d_create_entity_reference(
    _this: *mut Obj,
    _name: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}
unsafe extern "win64" fn d_get_elements_by_tag_name(
    this: *mut Obj,
    name: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    elements_by_tag(this, DOC, name, out)
}
unsafe fn elements_by_tag(
    this: *mut Obj,
    from: usize,
    name: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let o = obj(this);
    let mut v = Vec::new();
    lock(&o.doc).elements_by_tag(from, &read_bstr(name), &mut v);
    put(out, new_obj(&o.doc, What::Snapshot(v)))
}
unsafe extern "win64" fn d_create_node(
    this: *mut Obj,
    kind: *const u8,
    name: *const u16,
    _uri: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    if kind.is_null() {
        return E_INVALIDARG;
    }
    // Type en nombre (NODE_ELEMENT…) ou en chaîne (« element »…).
    let mut tmp = [0u8; 24];
    let k =
        if super::oleaut32_var::change_type(tmp.as_mut_ptr().cast(), kind.cast(), 0x409, 0, VT_I4)
            >= 0
        {
            *(tmp.as_ptr().add(8) as *const i32)
        } else {
            match variant_text(kind).unwrap_or_default().as_str() {
                "element" => 1,
                "attribute" => 2,
                "text" => 3,
                "cdatasection" => 4,
                "processinginstruction" => 7,
                "comment" => 8,
                "documentfragment" => 11,
                _ => 0,
            }
        };
    let n = read_bstr(name);
    match k {
        1 => create(this, Kind::Element, &n, "", out),
        2 => create(this, Kind::Attribute, &n, "", out),
        3 => create(this, Kind::Text, "", "", out),
        4 => create(this, Kind::CData, "", "", out),
        7 => create(this, Kind::Pi, &n, "", out),
        8 => create(this, Kind::Comment, "", "", out),
        11 => create(this, Kind::Fragment, "", "", out),
        _ => E_INVALIDARG,
    }
}
unsafe extern "win64" fn d_node_from_id(_this: *mut Obj, _id: *const u16, out: *mut usize) -> i32 {
    n_out_null(_this, out)
}

/// Chemin Linux d'une source `load`/`save` (chemin Windows ou file:///).
fn local_path(source: &str) -> std::path::PathBuf {
    let s = source.trim();
    let s = s
        .strip_prefix("file:///")
        .or_else(|| s.strip_prefix("file://"))
        .map(|r| r.replace("%20", " "))
        .unwrap_or_else(|| s.to_string());
    if s.starts_with('/') {
        std::path::PathBuf::from(s)
    } else {
        crate::paths::to_linux(&s.replace('/', "\\"))
    }
}

/// Lit tout un IStream (ISequentialStream::Read).
unsafe fn read_stream(stream: usize) -> Option<Vec<u8>> {
    let read: unsafe extern "win64" fn(usize, *mut u8, u32, *mut u32) -> i32 =
        std::mem::transmute(*(*(stream as *const *const usize)).add(3));
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let mut n = 0u32;
        let hr = read(stream, buf.as_mut_ptr(), buf.len() as u32, &mut n);
        if hr < 0 {
            return None;
        }
        out.extend_from_slice(&buf[..n as usize]);
        if n == 0 || hr == S_FALSE {
            return Some(out);
        }
    }
}

/// IStream ou ISequentialStream d'un objet, référence comprise.
unsafe fn stream_of(p: usize) -> Option<usize> {
    let qi: unsafe extern "win64" fn(usize, *const Guid, *mut usize) -> i32 =
        std::mem::transmute(*(*(p as *const *const usize)));
    for iid in [IID_ISTREAM, IID_ISEQUENTIALSTREAM] {
        let mut s = 0usize;
        if qi(p, &iid, &mut s) >= 0 && s != 0 {
            return Some(s);
        }
    }
    None
}

unsafe fn release_raw(p: usize) {
    let f: unsafe extern "win64" fn(usize) -> u32 =
        std::mem::transmute(*(*(p as *const *const usize)).add(2));
    f(p);
}

unsafe extern "win64" fn d_load(this: *mut Obj, source: *const u8, ok: *mut i16) -> i32 {
    if !ok.is_null() {
        *ok = VARIANT_FALSE;
    }
    let o = obj(this);
    let text = if let Some(p) = variant_object(source) {
        if is_ours(p) {
            let other = &*(p as *const Obj);
            let x = lock(&other.doc).xml(DOC);
            x
        } else {
            let Some(s) = stream_of(p) else {
                return E_INVALIDARG;
            };
            let data = read_stream(s);
            release_raw(s);
            let Some(data) = data else {
                return E_FAIL;
            };
            dom::decode(&data)
        }
    } else {
        let Some(name) = variant_text(source) else {
            return E_INVALIDARG;
        };
        let path = local_path(&name);
        lock(&o.doc).url = name.clone();
        match std::fs::read(&path) {
            Ok(bytes) => dom::decode(&bytes),
            Err(_) => {
                let mut d = lock(&o.doc);
                d.load_str("");
                d.error = dom::ParseError {
                    code: dom::XML_E_FILENOTFOUND,
                    reason: "Le système ne trouve pas la ressource spécifiée.".into(),
                    url: name,
                    ..Default::default()
                };
                return S_FALSE;
            }
        }
    };
    if lock(&o.doc).load_str(&text) {
        put_bool(ok, true);
        S_OK
    } else {
        S_FALSE
    }
}
unsafe extern "win64" fn d_get_ready_state(_this: *mut Obj, out: *mut i32) -> i32 {
    put(out, 4) // READYSTATE_COMPLETE
}
unsafe extern "win64" fn d_get_parse_error(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    let o = obj(this);
    let e = lock(&o.doc).error.clone();
    put(out, new_obj(&o.doc, What::Error(e)))
}
unsafe extern "win64" fn d_get_url(this: *mut Obj, out: *mut *mut u16) -> i32 {
    let o = obj(this);
    let u = lock(&o.doc).url.clone();
    if u.is_empty() {
        if !out.is_null() {
            *out = std::ptr::null_mut();
        }
        return S_FALSE;
    }
    put_bstr(out, &u)
}
unsafe extern "win64" fn d_get_async(this: *mut Obj, out: *mut i16) -> i32 {
    put_bool(out, lock(&obj(this).doc).asynchronous)
}
unsafe extern "win64" fn d_put_async(this: *mut Obj, v: i16) -> i32 {
    // Le chargement est toujours fait d'un coup ; la valeur est gardée.
    lock(&obj(this).doc).asynchronous = v != 0;
    S_OK
}
unsafe extern "win64" fn d_abort(_this: *mut Obj) -> i32 {
    S_OK
}
unsafe extern "win64" fn d_load_xml(this: *mut Obj, text: *const u16, ok: *mut i16) -> i32 {
    let o = obj(this);
    let s = read_bstr(text);
    let good = lock(&o.doc).load_str(&s);
    if !ok.is_null() {
        *ok = if good { VARIANT_TRUE } else { VARIANT_FALSE };
    }
    if good {
        S_OK
    } else {
        S_FALSE
    }
}

/// Octets enregistrés : UTF-16 avec BOM si la déclaration XML le demande,
/// UTF-8 sinon.
fn encoded(d: &Doc) -> Vec<u8> {
    let text = d.xml(DOC);
    let utf16 = d
        .node(DOC)
        .children
        .first()
        .map(|&c| d.node(c))
        .is_some_and(|n| {
            n.kind == Kind::Pi && n.name == "xml" && n.value.to_ascii_lowercase().contains("utf-16")
        });
    if utf16 {
        let mut out = vec![0xFF, 0xFE];
        for u in text.encode_utf16() {
            out.extend_from_slice(&u.to_le_bytes());
        }
        out
    } else {
        text.into_bytes()
    }
}

unsafe extern "win64" fn d_save(this: *mut Obj, dest: *const u8) -> i32 {
    let o = obj(this);
    if let Some(p) = variant_object(dest) {
        if is_ours(p) {
            let x = lock(&o.doc).xml(DOC);
            let other = &*(p as *const Obj);
            return if lock(&other.doc).load_str(&x) {
                S_OK
            } else {
                E_FAIL
            };
        }
        let Some(s) = stream_of(p) else {
            return E_INVALIDARG;
        };
        let bytes = encoded(&lock(&o.doc));
        let write: unsafe extern "win64" fn(usize, *const u8, u32, *mut u32) -> i32 =
            std::mem::transmute(*(*(s as *const *const usize)).add(4));
        let mut n = 0u32;
        let hr = write(s, bytes.as_ptr(), bytes.len() as u32, &mut n);
        release_raw(s);
        return if hr < 0 { hr } else { S_OK };
    }
    let Some(name) = variant_text(dest) else {
        return E_INVALIDARG;
    };
    let bytes = encoded(&lock(&o.doc));
    match std::fs::write(local_path(&name), bytes) {
        Ok(()) => S_OK,
        Err(e) => {
            crate::log_warn!("com", "MSXML : save({name}) impossible ({e})");
            match e.kind() {
                std::io::ErrorKind::NotFound => 0x8007_0003u32 as i32,
                std::io::ErrorKind::PermissionDenied => 0x8007_0005u32 as i32,
                _ => E_FAIL,
            }
        }
    }
}
unsafe extern "win64" fn d_get_validate_on_parse(this: *mut Obj, out: *mut i16) -> i32 {
    put_bool(out, lock(&obj(this).doc).validate_on_parse)
}
unsafe extern "win64" fn d_put_validate_on_parse(this: *mut Obj, v: i16) -> i32 {
    lock(&obj(this).doc).validate_on_parse = v != 0;
    S_OK
}
unsafe extern "win64" fn d_get_resolve_externals(this: *mut Obj, out: *mut i16) -> i32 {
    put_bool(out, lock(&obj(this).doc).resolve_externals)
}
unsafe extern "win64" fn d_put_resolve_externals(this: *mut Obj, v: i16) -> i32 {
    lock(&obj(this).doc).resolve_externals = v != 0;
    S_OK
}
unsafe extern "win64" fn d_get_preserve_white_space(this: *mut Obj, out: *mut i16) -> i32 {
    put_bool(out, lock(&obj(this).doc).preserve_ws)
}
unsafe extern "win64" fn d_put_preserve_white_space(this: *mut Obj, v: i16) -> i32 {
    lock(&obj(this).doc).preserve_ws = v != 0;
    S_OK
}
/// put_onreadystatechange, put_ondataavailable, put_ontransformnode,
/// putref_schemas : acceptés, sans effet (chargement synchrone, pas de
/// validation par schéma).
unsafe extern "win64" fn d_put_handler(_this: *mut Obj, _v: *const u8) -> i32 {
    S_OK
}
unsafe extern "win64" fn d_get_schemas(_this: *mut Obj, out: *mut u8) -> i32 {
    put_variant_str(out, None)
}
unsafe extern "win64" fn d_validate(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    // Pas de DTD ni de schéma : un document bien formé est valide.
    if !out.is_null() {
        let o = obj(this);
        *out = new_obj(&o.doc, What::Error(dom::ParseError::default()));
    }
    S_OK
}
unsafe extern "win64" fn d_set_property(this: *mut Obj, name: *const u16, value: *const u8) -> i32 {
    let n = read_bstr(name);
    let v = variant_text(value).unwrap_or_default();
    lock(&obj(this).doc).set_property(&n, &v);
    S_OK
}
unsafe extern "win64" fn d_get_property(this: *mut Obj, name: *const u16, out: *mut u8) -> i32 {
    let n = read_bstr(name);
    let d = lock(&obj(this).doc);
    let v = d
        .property(&n)
        .map(str::to_string)
        .or_else(|| match n.as_str() {
            "SelectionLanguage" => Some("XPath".into()),
            "SelectionNamespaces" => Some(String::new()),
            _ => None,
        });
    match v {
        Some(v) => {
            put_variant_str(out, Some(&v));
            S_OK
        }
        None => {
            if !out.is_null() {
                std::ptr::write_bytes(out, 0, 24);
            }
            E_INVALIDARG
        }
    }
}
unsafe extern "win64" fn d_validate_node(
    this: *mut Obj,
    _node: *mut Obj,
    out: *mut *mut Obj,
) -> i32 {
    d_validate(this, out)
}
unsafe extern "win64" fn d_import_node(
    this: *mut Obj,
    node: *mut Obj,
    deep: i16,
    out: *mut *mut Obj,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    let o = obj(this);
    let Some((src, id)) = arg_node(node) else {
        return E_INVALIDARG;
    };
    let copy = if Arc::ptr_eq(&src, &o.doc) {
        lock(&o.doc).clone_node(id, deep != 0)
    } else {
        let s = lock(&src);
        lock(&o.doc).import(&s, id, deep != 0)
    };
    put(out, new_obj(&o.doc, What::Node(copy)))
}

// ── IXMLDOMElement ─────────────────────────────────────────────────

unsafe extern "win64" fn e_get_attribute(this: *mut Obj, name: *const u16, out: *mut u8) -> i32 {
    let o = obj(this);
    let d = lock(&o.doc);
    let v = d
        .attribute(this_node(o), &read_bstr(name))
        .map(|a| d.node(a).value.clone());
    put_variant_str(out, v.as_deref())
}
unsafe extern "win64" fn e_set_attribute(
    this: *mut Obj,
    name: *const u16,
    value: *const u8,
) -> i32 {
    let o = obj(this);
    let n = read_bstr(name);
    if n.is_empty() {
        return E_INVALIDARG;
    }
    let Some(v) = variant_text(value) else {
        return E_INVALIDARG;
    };
    lock(&o.doc).set_attribute(this_node(o), &n, &v);
    S_OK
}
unsafe extern "win64" fn e_remove_attribute(this: *mut Obj, name: *const u16) -> i32 {
    let o = obj(this);
    let mut d = lock(&o.doc);
    match d.attribute(this_node(o), &read_bstr(name)) {
        Some(a) => {
            d.detach(a);
            S_OK
        }
        None => S_FALSE,
    }
}
unsafe extern "win64" fn e_get_attribute_node(
    this: *mut Obj,
    name: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    let o = obj(this);
    let a = lock(&o.doc).attribute(this_node(o), &read_bstr(name));
    put_node(&o.doc, out, a)
}
unsafe extern "win64" fn e_set_attribute_node(
    this: *mut Obj,
    attr: *mut Obj,
    out: *mut *mut Obj,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    let o = obj(this);
    let Some(a) = arg_node(attr) else {
        return E_INVALIDARG;
    };
    let id = adopt(&o.doc, &a);
    let old = {
        let mut d = lock(&o.doc);
        if d.node(id).kind != Kind::Attribute {
            return E_INVALIDARG;
        }
        // Un attribut déjà posé sur un autre élément est refusé.
        if d.node(id).parent.is_some_and(|p| p != this_node(o)) {
            return E_FAIL;
        }
        d.set_attribute_node(this_node(o), id)
    };
    if out.is_null() {
        return S_OK;
    }
    put_node(&o.doc, out, old).max(S_OK)
}
unsafe extern "win64" fn e_remove_attribute_node(
    this: *mut Obj,
    attr: *mut Obj,
    out: *mut *mut Obj,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    let o = obj(this);
    let Some((d, id)) = arg_node(attr) else {
        return E_INVALIDARG;
    };
    if !Arc::ptr_eq(&d, &o.doc) || lock(&o.doc).node(id).parent != Some(this_node(o)) {
        return E_INVALIDARG;
    }
    lock(&o.doc).detach(id);
    if !out.is_null() {
        add_ref(attr);
        *out = attr;
    }
    S_OK
}
unsafe extern "win64" fn e_get_elements_by_tag_name(
    this: *mut Obj,
    name: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    elements_by_tag(this, this_node(obj(this)), name, out)
}
unsafe extern "win64" fn e_normalize(this: *mut Obj) -> i32 {
    let o = obj(this);
    lock(&o.doc).normalize(this_node(o));
    S_OK
}

// ── IXMLDOMCharacterData / IXMLDOMText ─────────────────────────────

/// Données en UTF-16, comme les décalages de l'API.
fn units(d: &Doc, id: usize) -> Vec<u16> {
    d.node(id).value.encode_utf16().collect()
}
fn set_units(d: &mut Doc, id: usize, u: &[u16]) {
    d.nodes[id].value = String::from_utf16_lossy(u);
}
unsafe extern "win64" fn c_get_length(this: *mut Obj, out: *mut i32) -> i32 {
    let o = obj(this);
    let n = units(&lock(&o.doc), this_node(o)).len();
    put(out, n as i32)
}
fn range(len: usize, offset: i32, count: i32) -> Option<(usize, usize)> {
    if offset < 0 || count < 0 || offset as usize > len {
        return None;
    }
    let start = offset as usize;
    Some((start, (start + count as usize).min(len)))
}
unsafe extern "win64" fn c_substring_data(
    this: *mut Obj,
    offset: i32,
    count: i32,
    out: *mut *mut u16,
) -> i32 {
    let o = obj(this);
    let u = units(&lock(&o.doc), this_node(o));
    let Some((a, b)) = range(u.len(), offset, count) else {
        return E_INVALIDARG;
    };
    put_bstr(out, &String::from_utf16_lossy(&u[a..b]))
}
unsafe extern "win64" fn c_append_data(this: *mut Obj, text: *const u16) -> i32 {
    let o = obj(this);
    let id = this_node(o);
    let add = read_bstr(text);
    lock(&o.doc).nodes[id].value.push_str(&add);
    S_OK
}
unsafe extern "win64" fn c_insert_data(this: *mut Obj, offset: i32, text: *const u16) -> i32 {
    c_replace_data(this, offset, 0, text)
}
unsafe extern "win64" fn c_delete_data(this: *mut Obj, offset: i32, count: i32) -> i32 {
    c_replace_data(this, offset, count, std::ptr::null())
}
unsafe extern "win64" fn c_replace_data(
    this: *mut Obj,
    offset: i32,
    count: i32,
    text: *const u16,
) -> i32 {
    let o = obj(this);
    let id = this_node(o);
    let mut d = lock(&o.doc);
    let mut u = units(&d, id);
    let Some((a, b)) = range(u.len(), offset, count) else {
        return E_INVALIDARG;
    };
    let new: Vec<u16> = read_bstr(text).encode_utf16().collect();
    u.splice(a..b, new);
    set_units(&mut d, id, &u);
    S_OK
}
unsafe extern "win64" fn c_split_text(this: *mut Obj, offset: i32, out: *mut *mut Obj) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    let o = obj(this);
    let id = this_node(o);
    let tail = {
        let mut d = lock(&o.doc);
        if !matches!(d.node(id).kind, Kind::Text | Kind::CData) {
            return E_FAIL;
        }
        let u = units(&d, id);
        if offset < 0 || offset as usize > u.len() {
            return E_INVALIDARG;
        }
        let (a, b) = u.split_at(offset as usize);
        let kind = d.node(id).kind;
        set_units(&mut d, id, a);
        let t = d.create(kind, "", &String::from_utf16_lossy(b));
        if let Some(p) = d.node(id).parent {
            let next = sibling(&d, id, 1);
            d.insert(p, t, next);
        }
        t
    };
    if out.is_null() {
        return S_OK;
    }
    put_node(&o.doc, out, Some(tail))
}

// ── IXMLDOMNodeList / IXMLDOMNamedNodeMap ──────────────────────────

fn items(o: &Obj) -> Vec<usize> {
    let d = lock(&o.doc);
    match &o.what {
        What::Children(id) => d.node(*id).children.clone(),
        What::Snapshot(v) => v.clone(),
        What::Attrs(id) => d.node(*id).attrs.clone(),
        _ => Vec::new(),
    }
}
unsafe extern "win64" fn l_get_item(this: *mut Obj, index: i32, out: *mut *mut Obj) -> i32 {
    let o = obj(this);
    let v = items(o);
    let id = (index >= 0)
        .then(|| v.get(index as usize).copied())
        .flatten();
    put_node(&o.doc, out, id)
}
unsafe extern "win64" fn l_get_length(this: *mut Obj, out: *mut i32) -> i32 {
    put(out, items(obj(this)).len() as i32)
}
unsafe extern "win64" fn l_next_node(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    let o = obj(this);
    let i = o.cursor.fetch_add(1, Ordering::SeqCst);
    let id = items(o).get(i).copied();
    put_node(&o.doc, out, id)
}
unsafe extern "win64" fn l_reset(this: *mut Obj) -> i32 {
    obj(this).cursor.store(0, Ordering::SeqCst);
    S_OK
}
unsafe extern "win64" fn l_new_enum(_this: *mut Obj, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}
fn map_element(o: &Obj) -> usize {
    match o.what {
        What::Attrs(id) => id,
        _ => DOC,
    }
}
unsafe extern "win64" fn m_get_named_item(
    this: *mut Obj,
    name: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    let o = obj(this);
    let a = lock(&o.doc).attribute(map_element(o), &read_bstr(name));
    put_node(&o.doc, out, a)
}
unsafe extern "win64" fn m_set_named_item(
    this: *mut Obj,
    attr: *mut Obj,
    out: *mut *mut Obj,
) -> i32 {
    let o = obj(this);
    // Même effet que setAttributeNode sur l'élément, qui rend le nouveau.
    let element = new_obj(&o.doc, What::Node(map_element(o)));
    let hr = e_set_attribute_node(element, attr, std::ptr::null_mut());
    release(element);
    if hr < 0 {
        if !out.is_null() {
            *out = std::ptr::null_mut();
        }
        return hr;
    }
    if !out.is_null() {
        add_ref(attr);
        *out = attr;
    }
    S_OK
}
unsafe extern "win64" fn m_remove_named_item(
    this: *mut Obj,
    name: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    let o = obj(this);
    let a = lock(&o.doc).attribute(map_element(o), &read_bstr(name));
    if let Some(a) = a {
        lock(&o.doc).detach(a);
    }
    put_node(&o.doc, out, a)
}
unsafe extern "win64" fn m_get_qualified_item(
    this: *mut Obj,
    name: *const u16,
    uri: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    let o = obj(this);
    let (base, ns) = (read_bstr(name), read_bstr(uri));
    let d = lock(&o.doc);
    let a = d.node(map_element(o)).attrs.iter().copied().find(|&a| {
        let (_, b) = d.prefix_and_base(a);
        b == base && d.namespace_uri(a) == ns
    });
    drop(d);
    put_node(&o.doc, out, a)
}
unsafe extern "win64" fn m_remove_qualified_item(
    this: *mut Obj,
    name: *const u16,
    uri: *const u16,
    out: *mut *mut Obj,
) -> i32 {
    let mut found: *mut Obj = std::ptr::null_mut();
    let hr = m_get_qualified_item(this, name, uri, &mut found);
    if found.is_null() {
        if !out.is_null() {
            *out = std::ptr::null_mut();
        }
        return hr;
    }
    let o = obj(this);
    lock(&o.doc).detach(this_node(&*found));
    if out.is_null() {
        release(found);
    } else {
        *out = found;
    }
    S_OK
}

// ── IXMLDOMParseError ──────────────────────────────────────────────

fn error_of(o: &Obj) -> &dom::ParseError {
    match &o.what {
        What::Error(e) => e,
        _ => unreachable!("table de parseError sur un autre objet"),
    }
}
unsafe extern "win64" fn pe_get_error_code(this: *mut Obj, out: *mut i32) -> i32 {
    put(out, error_of(obj(this)).code)
}
unsafe extern "win64" fn pe_get_url(this: *mut Obj, out: *mut *mut u16) -> i32 {
    put_bstr(out, &error_of(obj(this)).url)
}
unsafe extern "win64" fn pe_get_reason(this: *mut Obj, out: *mut *mut u16) -> i32 {
    put_bstr(out, &error_of(obj(this)).reason)
}
unsafe extern "win64" fn pe_get_src_text(this: *mut Obj, out: *mut *mut u16) -> i32 {
    put_bstr(out, &error_of(obj(this)).src_text)
}
unsafe extern "win64" fn pe_get_line(this: *mut Obj, out: *mut i32) -> i32 {
    put(out, error_of(obj(this)).line)
}
unsafe extern "win64" fn pe_get_linepos(this: *mut Obj, out: *mut i32) -> i32 {
    put(out, error_of(obj(this)).linepos)
}
unsafe extern "win64" fn pe_get_filepos(this: *mut Obj, out: *mut i32) -> i32 {
    put(out, error_of(obj(this)).filepos)
}

// ── Classes ────────────────────────────────────────────────────────

/// CoCreateInstance d'une classe DOMDocument ; None si ce n'en est pas une.
pub unsafe fn create_builtin(clsid: &Guid, iid: &Guid, out: *mut *mut c_void) -> Option<i32> {
    let (_, version) = CLASSES.iter().find(|(c, _)| c == clsid)?;
    let doc: DocRef = Arc::new(Mutex::new(Doc::new()));
    {
        let mut d = lock(&doc);
        // MSXML 6 : XPath et entrées externes coupées ; avant : XSLPattern.
        if *version >= 4 {
            d.set_property("SelectionLanguage", "XPath");
        } else {
            d.set_property("SelectionLanguage", "XSLPattern");
        }
        if *version >= 6 {
            d.validate_on_parse = false;
        }
    }
    let o = new_obj(&doc, What::Node(DOC));
    let hr = query_interface(o, iid, out);
    release(o);
    Some(hr)
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn call<T>(o: *mut Obj, slot: usize) -> T
    where
        T: Copy,
    {
        let vt = *(o as *const *const usize);
        std::mem::transmute_copy(&*vt.add(slot))
    }

    #[test]
    fn document_par_les_tables() {
        unsafe {
            let clsid = CLASSES[5].0; // 3.0
            let mut out: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                create_builtin(&clsid, &IID_IXMLDOMDOCUMENT2, &mut out),
                Some(S_OK)
            );
            let doc = out as *mut Obj;
            let text = bstr("<r a=\"1\"><e>é</e></r>");
            let mut ok = 0i16;
            let load_xml: unsafe extern "win64" fn(*mut Obj, *const u16, *mut i16) -> i32 =
                call(doc, 65);
            assert_eq!(load_xml(doc, text, &mut ok), S_OK);
            assert_eq!(ok, VARIANT_TRUE);
            super::super::oleaut32::sys_free_string(text);
            let get_root: unsafe extern "win64" fn(*mut Obj, *mut *mut Obj) -> i32 = call(doc, 45);
            let mut root = std::ptr::null_mut();
            assert_eq!(get_root(doc, &mut root), S_OK);
            let get_attr: unsafe extern "win64" fn(*mut Obj, *const u16, *mut u8) -> i32 =
                call(root, 44);
            let mut v = [0u8; 24];
            let a = bstr("a");
            assert_eq!(get_attr(root, a, v.as_mut_ptr()), S_OK);
            assert_eq!(variant_text(v.as_ptr()).as_deref(), Some("1"));
            super::super::oleaut32::variant_clear(v.as_mut_ptr().cast());
            let get_xml: unsafe extern "win64" fn(*mut Obj, *mut *mut u16) -> i32 = call(root, 34);
            let mut x = std::ptr::null_mut();
            assert_eq!(get_xml(root, &mut x), S_OK);
            assert_eq!(read_bstr(x), "<r a=\"1\"><e>é</e></r>");
            super::super::oleaut32::sys_free_string(x);
            super::super::oleaut32::sys_free_string(a);
            release(root);
            release(doc);
        }
    }
}
