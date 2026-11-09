//! Effets Direct2D personnalisés (#561) : un effet enregistré par
//! RegisterEffectFromString est une classe de l'application (ID2D1EffectImpl,
//! créée par sa fonction de fabrique) qui décrit son calcul à Direct2D par un
//! contexte d'effet (ID2D1EffectContext : chargement des pixel shaders) et un
//! graphe de transformations (ID2D1TransformGraph). Chaque transformation de
//! dessin (ID2D1DrawTransform) reçoit un ID2D1DrawInfo où elle choisit son
//! pixel shader et ses constantes.
//!
//! L'ordre des méthodes suit `d2d1effectauthor.h` (MinGW-w64, domaine public)
//! ; la description XML et les liaisons de propriétés suivent la
//! documentation publique de RegisterEffectFromString. Le calcul passe par
//! le module `d2d_shader` de la couche D3D11 (traduction DXBC → GLSL).

use super::{com_add_ref, com_release, Effect, EffectObj, Img};
use crate::dlls::audio_com::{guid, put, Com, VTable, E_INVALIDARG, E_POINTER, S_OK};
use crate::dlls::d2d1::{add_ref, method, nothing, qi_list, release};
use crate::dlls::ole32::Guid;
use std::ffi::c_void;
use std::sync::{Arc, Mutex};

const E_FAIL: i32 = 0x8000_4005u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
/// HRESULT_FROM_WIN32(ERROR_NOT_FOUND).
const E_NOT_FOUND: i32 = 0x8007_0490u32 as i32;
const D2DERR_INSUFFICIENT_BUFFER: i32 = 0x8007_007Au32 as i32;

const IID_EFFECT_IMPL: Guid = guid(
    0xa248fd3f,
    0x3e6c,
    0x4e63,
    [0x9f, 0x03, 0x7f, 0x68, 0xec, 0xc9, 0x1d, 0xb9],
);
const IID_EFFECT_CONTEXT: Guid = guid(
    0x3d9f916b,
    0x27dc,
    0x4ad7,
    [0xb4, 0xf1, 0x64, 0x94, 0x53, 0x40, 0xf5, 0x63],
);
const IID_EFFECT_CONTEXT1: Guid = guid(
    0x84ab595a,
    0xfc81,
    0x4546,
    [0xba, 0xcd, 0xe8, 0xef, 0x4d, 0x8a, 0xbe, 0x7a],
);
const IID_EFFECT_CONTEXT2: Guid = guid(
    0x577ad2a0,
    0x9fc7,
    0x4dda,
    [0x8b, 0x18, 0xda, 0xb8, 0x10, 0x14, 0x00, 0x52],
);
const IID_TRANSFORM_GRAPH: Guid = guid(
    0x13d29038,
    0xc3e6,
    0x4034,
    [0x90, 0x81, 0x13, 0xb5, 0x3a, 0x41, 0x79, 0x92],
);
const IID_RENDER_INFO: Guid = guid(
    0x519ae1bd,
    0xd19a,
    0x420d,
    [0xb8, 0x49, 0x36, 0x4f, 0x59, 0x47, 0x76, 0xb7],
);
const IID_DRAW_INFO: Guid = guid(
    0x693ce632,
    0x7f2f,
    0x45de,
    [0x93, 0xfe, 0x18, 0xd8, 0x8b, 0x37, 0xaa, 0x21],
);
const IID_TRANSFORM_NODE: Guid = guid(
    0xb2efe1e7,
    0x729f,
    0x4102,
    [0x94, 0x9f, 0x50, 0x5f, 0xa2, 0x1b, 0xf6, 0x66],
);
const IID_DRAW_TRANSFORM: Guid = guid(
    0x36bfdcb6,
    0x9739,
    0x435d,
    [0xa3, 0x0d, 0xa6, 0x53, 0xbe, 0xff, 0x6a, 0x6f],
);
const IID_OFFSET_TRANSFORM: Guid = guid(
    0x3fe6adea,
    0x7643,
    0x4f53,
    [0xbd, 0x14, 0xa0, 0xce, 0x63, 0xf2, 0x40, 0x42],
);
const IID_BOUNDS_TRANSFORM: Guid = guid(
    0x90f732e2,
    0x5092,
    0x4606,
    [0xa8, 0x19, 0x86, 0x51, 0x97, 0x0b, 0xac, 0xcd],
);
const IID_CONCRETE_TRANSFORM: Guid = guid(
    0x1a799d8a,
    0x69f7,
    0x4e4c,
    [0x9f, 0xed, 0x43, 0x7c, 0xcc, 0x66, 0x84, 0xcc],
);
const IID_BORDER_TRANSFORM: Guid = guid(
    0x4998735c,
    0x3a19,
    0x473c,
    [0x97, 0x81, 0x65, 0x68, 0x47, 0xe3, 0xa3, 0x47],
);

// --- Description XML ---------------------------------------------------------------

/// D2D1_PROPERTY_TYPE.
pub(super) mod ty {
    pub const STRING: u32 = 1;
    pub const BOOL: u32 = 2;
    pub const UINT32: u32 = 3;
    pub const INT32: u32 = 4;
    pub const FLOAT: u32 = 5;
    pub const VECTOR2: u32 = 6;
    pub const VECTOR3: u32 = 7;
    pub const VECTOR4: u32 = 8;
    pub const BLOB: u32 = 9;
    pub const IUNKNOWN: u32 = 10;
    pub const ENUM: u32 = 11;
    pub const ARRAY: u32 = 12;
    pub const CLSID: u32 = 13;
    pub const MATRIX_3X2: u32 = 14;
    pub const MATRIX_4X3: u32 = 15;
    pub const MATRIX_4X4: u32 = 16;
    pub const MATRIX_5X4: u32 = 17;
    pub const COLOR_CONTEXT: u32 = 18;
}

fn type_from_name(s: &str) -> u32 {
    match s.to_ascii_lowercase().as_str() {
        "string" => ty::STRING,
        "bool" => ty::BOOL,
        "uint32" => ty::UINT32,
        "int32" => ty::INT32,
        "float" => ty::FLOAT,
        "vector2" => ty::VECTOR2,
        "vector3" => ty::VECTOR3,
        "vector4" => ty::VECTOR4,
        "blob" => ty::BLOB,
        "iunknown" => ty::IUNKNOWN,
        "enum" => ty::ENUM,
        "array" => ty::ARRAY,
        "clsid" => ty::CLSID,
        "matrix3x2" => ty::MATRIX_3X2,
        "matrix4x3" => ty::MATRIX_4X3,
        "matrix4x4" => ty::MATRIX_4X4,
        "matrix5x4" => ty::MATRIX_5X4,
        "colorcontext" => ty::COLOR_CONTEXT,
        _ => 0,
    }
}

/// Valeur par défaut (attribut `value`) dans la forme binaire du type.
fn default_value(t: u32, s: &str) -> Vec<u8> {
    let nums = || -> Vec<f32> {
        s.trim_matches(|c| c == '(' || c == ')' || c == ' ')
            .split(',')
            .filter_map(|x| x.trim().parse::<f32>().ok())
            .collect()
    };
    let int = |x: &str| -> i64 {
        let x = x.trim();
        x.strip_prefix("0x")
            .or_else(|| x.strip_prefix("0X"))
            .map_or_else(
                || x.parse().unwrap_or(0),
                |h| i64::from_str_radix(h, 16).unwrap_or(0),
            )
    };
    match t {
        ty::STRING => s
            .encode_utf16()
            .chain([0])
            .flat_map(|c| c.to_le_bytes())
            .collect(),
        ty::BOOL => (s.trim().eq_ignore_ascii_case("true") as u32)
            .to_le_bytes()
            .to_vec(),
        ty::UINT32 | ty::ENUM => (int(s) as u32).to_le_bytes().to_vec(),
        ty::INT32 => (int(s) as i32).to_le_bytes().to_vec(),
        ty::FLOAT
        | ty::VECTOR2
        | ty::VECTOR3
        | ty::VECTOR4
        | ty::MATRIX_3X2
        | ty::MATRIX_4X3
        | ty::MATRIX_4X4
        | ty::MATRIX_5X4 => {
            let n = match t {
                ty::FLOAT => 1,
                ty::VECTOR2 => 2,
                ty::VECTOR3 => 3,
                ty::VECTOR4 => 4,
                ty::MATRIX_3X2 => 6,
                ty::MATRIX_4X3 => 12,
                ty::MATRIX_4X4 => 16,
                _ => 20,
            };
            let mut v = nums();
            v.resize(n, 0.0);
            v.iter().flat_map(|f| f.to_le_bytes()).collect()
        }
        _ => Vec::new(),
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct PropDef {
    pub name: String,
    pub ty: u32,
    pub default: Vec<u8>,
    /// Fonctions de la liaison (D2D1_PROPERTY_BINDING) : écriture, lecture.
    pub set: usize,
    pub get: usize,
}

/// Effet enregistré : description et code de l'application.
#[derive(Default, Debug)]
pub(super) struct Registration {
    /// Propriétés personnalisées, dans l'ordre de la description (index 0…).
    pub props: Vec<PropDef>,
    /// DisplayName, Author, Category, Description.
    pub system: [String; 4],
    pub inputs: usize,
    /// PD2D1_EFFECT_FACTORY.
    pub create: usize,
}

struct Tag<'a> {
    name: &'a str,
    attrs: Vec<(&'a str, &'a str)>,
    closing: bool,
    self_closing: bool,
}

fn tags(xml: &str) -> Vec<Tag<'_>> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find('<') {
        rest = &rest[i + 1..];
        let Some(end) = rest.find('>') else { break };
        let body = &rest[..end];
        rest = &rest[end + 1..];
        if body.starts_with('?') || body.starts_with('!') {
            continue;
        }
        let closing = body.starts_with('/');
        let self_closing = body.ends_with('/');
        let body = body.trim_start_matches('/').trim_end_matches('/').trim();
        let name_end = body.find(|c: char| c.is_whitespace()).unwrap_or(body.len());
        let name = &body[..name_end];
        let mut attrs = Vec::new();
        let mut a = &body[name_end..];
        while let Some(eq) = a.find('=') {
            let key = a[..eq].trim();
            let after = a[eq + 1..].trim_start();
            let Some(q) = after.chars().next().filter(|c| *c == '\'' || *c == '"') else {
                break;
            };
            let after = &after[1..];
            let Some(close) = after.find(q) else { break };
            attrs.push((key, &after[..close]));
            a = &after[close + 1..];
        }
        out.push(Tag {
            name,
            attrs,
            closing,
            self_closing,
        });
    }
    out
}

fn attr<'a>(t: &Tag<'a>, k: &str) -> Option<&'a str> {
    t.attrs
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(k))
        .map(|(_, v)| *v)
}

const SYSTEM_NAMES: [&str; 4] = ["DisplayName", "Author", "Category", "Description"];

/// Lit la description XML d'un effet.
pub(super) fn parse(xml: &str) -> Registration {
    let mut r = Registration::default();
    let mut depth = 0usize;
    let mut in_inputs = false;
    for t in tags(xml) {
        if t.closing {
            depth = depth.saturating_sub(1);
            if t.name == "Inputs" {
                in_inputs = false;
            }
            continue;
        }
        match t.name {
            "Inputs" => in_inputs = !t.self_closing,
            "Input" if in_inputs => r.inputs += 1,
            // Propriétés de premier niveau (dans <Effect>) ; les
            // sous-propriétés (profondeur 3 et plus) décrivent leur parent.
            "Property" if depth == 1 => {
                let name = attr(&t, "name").unwrap_or_default();
                let value = attr(&t, "value").unwrap_or_default();
                if let Some(i) = SYSTEM_NAMES.iter().position(|s| *s == name) {
                    r.system[i] = value.to_string();
                } else {
                    let ty = type_from_name(attr(&t, "type").unwrap_or_default());
                    r.props.push(PropDef {
                        name: name.to_string(),
                        ty,
                        default: default_value(ty, value),
                        set: 0,
                        get: 0,
                    });
                }
            }
            _ => {}
        }
        if !t.self_closing {
            depth += 1;
        }
    }
    r
}

/// Ajoute les liaisons (tableau de D2D1_PROPERTY_BINDING : nom, écriture,
/// lecture ; 24 octets chacune) aux propriétés de même nom.
pub(super) unsafe fn bind(r: &mut Registration, bindings: *const usize, n: u32) {
    if bindings.is_null() {
        return;
    }
    for i in 0..n as usize {
        let b = bindings.add(i * 3);
        let name = super::wide(*b as *const u16);
        if let Some(p) = r.props.iter_mut().find(|p| p.name == name) {
            p.set = *b.add(1);
            p.get = *b.add(2);
        } else {
            crate::log_debug!("d2d1", "liaison « {name} » sans propriété décrite");
        }
    }
}

// --- Effet vivant --------------------------------------------------------------------

/// Partie « personnalisée » d'un ID2D1Effect.
pub(super) struct Custom {
    pub reg: Arc<Registration>,
    /// IUnknown rendu par la fabrique de l'application (passé aux liaisons).
    unk: *mut c_void,
    /// Son ID2D1EffectImpl.
    imp: *mut c_void,
    graph: *mut GraphObj,
    context: *mut ContextObj,
    /// Changement à signaler par PrepareForRender (D2D1_CHANGE_TYPE + 1).
    pending: Mutex<u32>,
}

unsafe impl Send for Custom {}
unsafe impl Sync for Custom {}

impl Drop for Custom {
    fn drop(&mut self) {
        unsafe {
            Com::release(self.graph);
            Com::release(self.context);
            if !self.imp.is_null() {
                com_release(self.imp);
            }
            if !self.unk.is_null() {
                com_release(self.unk);
            }
        }
    }
}

/// Instancie le code de l'application et appelle Initialize.
pub(super) unsafe fn instantiate(reg: Arc<Registration>, effect: *mut EffectObj) -> i32 {
    if reg.create == 0 {
        return E_FAIL;
    }
    let factory: unsafe extern "win64" fn(*mut *mut c_void) -> i32 =
        std::mem::transmute(reg.create);
    let mut unk = std::ptr::null_mut();
    let hr = factory(&mut unk);
    if hr < 0 || unk.is_null() {
        return if hr < 0 { hr } else { E_FAIL };
    }
    let mut imp = std::ptr::null_mut();
    let qi = method::<unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32>(
        unk, 0,
    );
    let hr = qi(unk, &IID_EFFECT_IMPL, &mut imp);
    if hr < 0 || imp.is_null() {
        com_release(unk);
        return E_NOINTERFACE;
    }
    let graph = Com::create(
        GRAPH_VT.ptr(),
        Graph {
            effect,
            state: Mutex::new(GraphState::default()),
        },
    );
    let context = Com::create(CONTEXT_VT.ptr(), Context { effect });
    let e = Com::get(effect);
    // Les valeurs par défaut des propriétés sans liaison.
    {
        let mut props = crate::dlls::lock(&e.props);
        for (i, p) in reg.props.iter().enumerate() {
            if p.set == 0 && !p.default.is_empty() {
                props.insert(i as u32, p.default.clone());
            }
        }
    }
    let custom = Custom {
        reg: reg.clone(),
        unk,
        imp,
        graph,
        context,
        pending: Mutex::new(1),
    };
    let _ = e.custom.set(custom);
    let init =
        method::<unsafe extern "win64" fn(*mut c_void, *mut c_void, *mut c_void) -> i32>(imp, 3);
    let hr = init(imp, context.cast(), graph.cast());
    if hr < 0 {
        crate::log_warn!(
            "d2d1",
            "effet personnalisé : Initialize a échoué ({:#x})",
            hr
        );
        return hr;
    }
    // Valeurs par défaut des propriétés liées, passées par leur fonction
    // une fois l'effet initialisé : Paint.NET ne relie son objet .NET qu'à
    // Initialize et son setter lève NullReferenceException avant (#1268).
    for (i, p) in reg.props.iter().enumerate() {
        if p.set != 0 && !p.default.is_empty() {
            if let Some(h) = set_value(e, i as u32, &p.default) {
                if h < 0 {
                    crate::log_warn!(
                        "d2d1",
                        "effet personnalisé : valeur par défaut de « {} » refusée ({:#x})",
                        p.name,
                        h
                    );
                }
            }
        }
    }
    hr
}

/// Propriété personnalisée : écrit la valeur (liaison ou mémoire).
pub(super) unsafe fn set_value(e: &Effect, index: u32, data: &[u8]) -> Option<i32> {
    let c = e.custom.get()?;
    let p = c.reg.props.get(index as usize)?;
    *crate::dlls::lock(&c.pending) = 2; // D2D1_CHANGE_TYPE_PROPERTIES
    if p.set != 0 {
        let f: unsafe extern "win64" fn(*mut c_void, *const u8, u32) -> i32 =
            std::mem::transmute(p.set);
        return Some(f(c.unk, data.as_ptr(), data.len() as u32));
    }
    crate::dlls::lock(&e.props).insert(index, data.to_vec());
    Some(S_OK)
}

/// Propriété personnalisée : lit la valeur. `data` nul : taille seulement.
pub(super) unsafe fn get_value(
    e: &Effect,
    index: u32,
    data: *mut u8,
    size: u32,
) -> Option<(i32, u32)> {
    let c = e.custom.get()?;
    let p = c.reg.props.get(index as usize)?;
    if p.get != 0 {
        let f: unsafe extern "win64" fn(*mut c_void, *mut u8, u32, *mut u32) -> i32 =
            std::mem::transmute(p.get);
        let mut actual = 0u32;
        let hr = f(c.unk, data, size, &mut actual);
        return Some((hr, actual));
    }
    let props = crate::dlls::lock(&e.props);
    let v = props.get(&index).map_or(&[][..], |v| &v[..]);
    if !data.is_null() {
        if (size as usize) < v.len() {
            return Some((D2DERR_INSUFFICIENT_BUFFER, v.len() as u32));
        }
        std::ptr::write_bytes(data, 0, size as usize);
        std::ptr::copy_nonoverlapping(v.as_ptr(), data, v.len());
    }
    Some((S_OK, v.len() as u32))
}

/// Chaîne système (DisplayName…) d'un effet personnalisé, en UTF-16.
pub(super) fn system_string(e: &Effect, which: usize) -> Option<Vec<u8>> {
    let s = e.reg()?.system.get(which)?;
    Some(
        s.encode_utf16()
            .chain([0])
            .flat_map(|c| c.to_le_bytes())
            .collect(),
    )
}

/// Signale à l'effet un changement d'entrée.
pub(super) fn inputs_changed(e: &Effect) {
    if let Some(c) = e.custom.get() {
        let mut p = crate::dlls::lock(&c.pending);
        *p = (*p).max(1);
    }
}

// --- Graphe de transformations ---------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Source {
    None,
    EffectInput(u32),
    Node(usize),
}

struct NodeEntry {
    node: *mut c_void,
    /// ID2D1DrawInfo donné à une transformation de dessin.
    info: *mut DrawInfoObj,
    inputs: Vec<Source>,
}

#[derive(Default)]
struct GraphState {
    nodes: Vec<NodeEntry>,
    output: Option<usize>,
    passthrough: Option<u32>,
}

impl GraphState {
    fn find(&self, node: *mut c_void) -> Option<usize> {
        self.nodes.iter().position(|n| n.node == node)
    }
}

struct Graph {
    effect: *mut EffectObj,
    state: Mutex<GraphState>,
}
unsafe impl Send for Graph {}
unsafe impl Sync for Graph {}
type GraphObj = Com<Graph>;

impl Drop for Graph {
    fn drop(&mut self) {
        let nodes = std::mem::take(&mut crate::dlls::lock(&self.state).nodes);
        for n in nodes {
            unsafe { release_entry(n) };
        }
    }
}

unsafe fn release_entry(n: NodeEntry) {
    if !n.info.is_null() {
        Com::release(n.info);
    }
    com_release(n.node);
}

unsafe fn node_input_count(node: *mut c_void) -> u32 {
    method::<unsafe extern "win64" fn(*mut c_void) -> u32>(node, 3)(node)
}

unsafe fn com_qi(p: *mut c_void, iid: &Guid) -> *mut c_void {
    let mut out = std::ptr::null_mut();
    let qi =
        method::<unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32>(p, 0);
    if qi(p, iid, &mut out) < 0 {
        return std::ptr::null_mut();
    }
    out
}

unsafe fn add_node(g: &Graph, s: &mut GraphState, node: *mut c_void) -> i32 {
    if node.is_null() {
        return E_INVALIDARG;
    }
    if s.find(node).is_some() {
        return E_INVALIDARG;
    }
    com_add_ref(node);
    let n = node_input_count(node) as usize;
    let mut entry = NodeEntry {
        node,
        info: std::ptr::null_mut(),
        inputs: vec![Source::None; n],
    };
    // Une transformation de dessin reçoit son ID2D1DrawInfo à l'ajout.
    let draw = com_qi(node, &IID_DRAW_TRANSFORM);
    if !draw.is_null() {
        let info = Com::create(
            DRAW_INFO_VT.ptr(),
            DrawInfo {
                state: Mutex::new(DrawState {
                    linear: vec![true; n],
                    ..Default::default()
                }),
            },
        );
        entry.info = info;
        let set = method::<unsafe extern "win64" fn(*mut c_void, *mut c_void) -> i32>(draw, 7);
        let hr = set(draw, info.cast());
        com_release(draw);
        if hr < 0 {
            release_entry(entry);
            return hr;
        }
    }
    let _ = g;
    s.nodes.push(entry);
    S_OK
}

unsafe extern "win64" fn graph_qi(
    this: *mut GraphObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_TRANSFORM_GRAPH])
}
unsafe extern "win64" fn graph_input_count(this: *mut GraphObj) -> u32 {
    crate::dlls::lock(&Com::get(Com::get(this).effect).inputs).len() as u32
}
unsafe extern "win64" fn graph_single(this: *mut GraphObj, node: *mut c_void) -> i32 {
    let g = Com::get(this);
    let mut s = crate::dlls::lock(&g.state);
    for n in std::mem::take(&mut s.nodes) {
        release_entry(n);
    }
    s.output = None;
    s.passthrough = None;
    let hr = add_node(g, &mut s, node);
    if hr < 0 {
        return hr;
    }
    let inputs = s.nodes[0].inputs.len();
    for (i, src) in s.nodes[0].inputs.iter_mut().enumerate().take(inputs) {
        *src = Source::EffectInput(i as u32);
    }
    s.output = Some(0);
    S_OK
}
unsafe extern "win64" fn graph_add(this: *mut GraphObj, node: *mut c_void) -> i32 {
    let g = Com::get(this);
    let mut s = crate::dlls::lock(&g.state);
    add_node(g, &mut s, node)
}
unsafe extern "win64" fn graph_remove(this: *mut GraphObj, node: *mut c_void) -> i32 {
    let g = Com::get(this);
    let mut s = crate::dlls::lock(&g.state);
    let Some(i) = s.find(node) else {
        return E_NOT_FOUND;
    };
    let entry = s.nodes.remove(i);
    // Les index des nœuds suivants reculent d'un cran.
    for n in s.nodes.iter_mut() {
        for src in n.inputs.iter_mut() {
            *src = match *src {
                Source::Node(j) if j == i => Source::None,
                Source::Node(j) if j > i => Source::Node(j - 1),
                o => o,
            };
        }
    }
    s.output = match s.output {
        Some(o) if o == i => None,
        Some(o) if o > i => Some(o - 1),
        o => o,
    };
    drop(s);
    release_entry(entry);
    S_OK
}
unsafe extern "win64" fn graph_set_output(this: *mut GraphObj, node: *mut c_void) -> i32 {
    let mut s = crate::dlls::lock(&Com::get(this).state);
    let Some(i) = s.find(node) else {
        return E_NOT_FOUND;
    };
    s.output = Some(i);
    s.passthrough = None;
    S_OK
}
unsafe extern "win64" fn graph_connect(
    this: *mut GraphObj,
    from: *mut c_void,
    to: *mut c_void,
    index: u32,
) -> i32 {
    let mut s = crate::dlls::lock(&Com::get(this).state);
    let (Some(f), Some(t)) = (s.find(from), s.find(to)) else {
        return E_NOT_FOUND;
    };
    let Some(slot) = s.nodes[t].inputs.get_mut(index as usize) else {
        return E_INVALIDARG;
    };
    *slot = Source::Node(f);
    S_OK
}
unsafe extern "win64" fn graph_connect_input(
    this: *mut GraphObj,
    effect_input: u32,
    node: *mut c_void,
    index: u32,
) -> i32 {
    let mut s = crate::dlls::lock(&Com::get(this).state);
    let Some(t) = s.find(node) else {
        return E_NOT_FOUND;
    };
    let Some(slot) = s.nodes[t].inputs.get_mut(index as usize) else {
        return E_INVALIDARG;
    };
    *slot = Source::EffectInput(effect_input);
    S_OK
}
unsafe extern "win64" fn graph_clear(this: *mut GraphObj) {
    let mut s = crate::dlls::lock(&Com::get(this).state);
    let nodes = std::mem::take(&mut s.nodes);
    s.output = None;
    s.passthrough = None;
    drop(s);
    for n in nodes {
        release_entry(n);
    }
}
unsafe extern "win64" fn graph_passthrough(this: *mut GraphObj, input: u32) -> i32 {
    let mut s = crate::dlls::lock(&Com::get(this).state);
    s.passthrough = Some(input);
    s.output = None;
    S_OK
}

static GRAPH_VT: VTable<12> = VTable([
    graph_qi as *const (),
    add_ref::<Graph> as *const (),
    release::<Graph> as *const (),
    graph_input_count as *const (),
    graph_single as *const (),
    graph_add as *const (),
    graph_remove as *const (),
    graph_set_output as *const (),
    graph_connect as *const (),
    graph_connect_input as *const (),
    graph_clear as *const (),
    graph_passthrough as *const (),
]);

// --- ID2D1DrawInfo ----------------------------------------------------------------------

#[derive(Default)]
struct DrawState {
    shader: Option<Guid>,
    constants: Vec<u8>,
    /// Filtre de chaque entrée (SetInputDescription) : bilinéaire ou non.
    linear: Vec<bool>,
}

struct DrawInfo {
    state: Mutex<DrawState>,
}
type DrawInfoObj = Com<DrawInfo>;

unsafe extern "win64" fn info_qi(
    this: *mut DrawInfoObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_RENDER_INFO, IID_DRAW_INFO])
}
/// D2D1_INPUT_DESCRIPTION (filtre, niveaux de détail), passée par valeur.
unsafe extern "win64" fn info_input_desc(this: *mut DrawInfoObj, index: u32, desc: u64) -> i32 {
    let mut s = crate::dlls::lock(&Com::get(this).state);
    let filter = desc as u32;
    let i = index as usize;
    if i >= s.linear.len() {
        s.linear.resize(i + 1, true);
    }
    // D2D1_FILTER_MIN_MAG_MIP_POINT (0) : plus proche voisin.
    s.linear[i] = filter != 0;
    S_OK
}
unsafe extern "win64" fn info_ok2(_: *mut DrawInfoObj, _: u32, _: u32) -> i32 {
    S_OK
}
unsafe extern "win64" fn info_ps_constants(this: *mut DrawInfoObj, data: *const u8, n: u32) -> i32 {
    if data.is_null() && n > 0 {
        return E_INVALIDARG;
    }
    crate::dlls::lock(&Com::get(this).state).constants = if n > 0 {
        std::slice::from_raw_parts(data, n as usize).to_vec()
    } else {
        Vec::new()
    };
    S_OK
}
unsafe extern "win64" fn info_resource_texture(_: *mut DrawInfoObj, _: u32, _: *mut c_void) -> i32 {
    crate::dlls::d2d1::unsupported_call("ID2D1DrawInfo::SetResourceTexture")
}
unsafe extern "win64" fn info_vs_constants(_: *mut DrawInfoObj, _: *const u8, _: u32) -> i32 {
    S_OK
}
unsafe extern "win64" fn info_pixel_shader(
    this: *mut DrawInfoObj,
    id: *const Guid,
    _opts: u32,
) -> i32 {
    let Some(id) = id.as_ref() else {
        return E_INVALIDARG;
    };
    crate::dlls::lock(&Com::get(this).state).shader = Some(*id);
    S_OK
}
unsafe extern "win64" fn info_vertex_processing(
    _: *mut DrawInfoObj,
    _: *mut c_void,
    _: u32,
    _: *const c_void,
    _: *const c_void,
    _: *const Guid,
) -> i32 {
    crate::dlls::d2d1::unsupported_call("ID2D1DrawInfo::SetVertexProcessing")
}

static DRAW_INFO_VT: VTable<12> = VTable([
    info_qi as *const (),
    add_ref::<DrawInfo> as *const (),
    release::<DrawInfo> as *const (),
    info_input_desc as *const (),
    info_ok2 as *const (), // SetOutputBuffer
    nothing as *const (),  // SetCached
    nothing as *const (),  // SetInstructionCountHint
    info_ps_constants as *const (),
    info_resource_texture as *const (),
    info_vs_constants as *const (),
    info_pixel_shader as *const (),
    info_vertex_processing as *const (),
]);

// --- Transformations prêtes à l'emploi ---------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Builtin {
    Offset(i32, i32),
    Bounds([i32; 4]),
    Border,
    /// Nœud fait d'un autre effet (CreateTransformNodeFromEffect).
    Effect(*mut c_void),
}

struct BuiltinNode {
    kind: Mutex<Builtin>,
}
unsafe impl Send for BuiltinNode {}
unsafe impl Sync for BuiltinNode {}
type BuiltinObj = Com<BuiltinNode>;

impl Drop for BuiltinNode {
    fn drop(&mut self) {
        if let Builtin::Effect(e) = *crate::dlls::lock(&self.kind) {
            unsafe { com_release(e) };
        }
    }
}

unsafe fn new_builtin(vt: *const (), kind: Builtin, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(
        out,
        Com::create(
            vt,
            BuiltinNode {
                kind: Mutex::new(kind),
            },
        ),
    )
}

unsafe extern "win64" fn bn_qi(
    this: *mut BuiltinObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    let iid = match *crate::dlls::lock(&Com::get(this).kind) {
        Builtin::Offset(..) => IID_OFFSET_TRANSFORM,
        Builtin::Bounds(_) => IID_BOUNDS_TRANSFORM,
        Builtin::Border => IID_BORDER_TRANSFORM,
        Builtin::Effect(_) => IID_TRANSFORM_NODE,
    };
    qi_list(
        this,
        riid,
        out,
        &[IID_TRANSFORM_NODE, IID_CONCRETE_TRANSFORM, iid],
    )
}
unsafe extern "win64" fn bn_inputs(this: *mut BuiltinObj) -> u32 {
    match *crate::dlls::lock(&Com::get(this).kind) {
        Builtin::Effect(e) => method::<unsafe extern "win64" fn(*mut c_void) -> u32>(e, 17)(e),
        _ => 1,
    }
}
unsafe extern "win64" fn bn_set_offset(this: *mut BuiltinObj, p: u64) {
    *crate::dlls::lock(&Com::get(this).kind) =
        Builtin::Offset(p as u32 as i32, (p >> 32) as u32 as i32);
}
unsafe extern "win64" fn bn_get_offset(this: *mut BuiltinObj) -> u64 {
    match *crate::dlls::lock(&Com::get(this).kind) {
        Builtin::Offset(x, y) => (x as u32 as u64) | ((y as u32 as u64) << 32),
        _ => 0,
    }
}
unsafe extern "win64" fn bn_set_bounds(this: *mut BuiltinObj, r: *const [i32; 4]) {
    if let Some(r) = r.as_ref() {
        *crate::dlls::lock(&Com::get(this).kind) = Builtin::Bounds(*r);
    }
}
unsafe extern "win64" fn bn_get_bounds(this: *mut BuiltinObj, r: *mut [i32; 4]) {
    if let (Some(r), Builtin::Bounds(b)) = (r.as_mut(), *crate::dlls::lock(&Com::get(this).kind)) {
        *r = b;
    }
}
unsafe extern "win64" fn bn_ok2(_: *mut BuiltinObj, _: u32, _: u32) -> i32 {
    S_OK
}
unsafe extern "win64" fn bn_zero(_: *mut BuiltinObj) -> u32 {
    0
}

static OFFSET_VT: VTable<6> = VTable([
    bn_qi as *const (),
    add_ref::<BuiltinNode> as *const (),
    release::<BuiltinNode> as *const (),
    bn_inputs as *const (),
    bn_set_offset as *const (),
    bn_get_offset as *const (),
]);
static BOUNDS_VT: VTable<6> = VTable([
    bn_qi as *const (),
    add_ref::<BuiltinNode> as *const (),
    release::<BuiltinNode> as *const (),
    bn_inputs as *const (),
    bn_set_bounds as *const (),
    bn_get_bounds as *const (),
]);
static BORDER_VT: VTable<10> = VTable([
    bn_qi as *const (),
    add_ref::<BuiltinNode> as *const (),
    release::<BuiltinNode> as *const (),
    bn_inputs as *const (),
    bn_ok2 as *const (),  // SetOutputBuffer
    nothing as *const (), // SetCached
    nothing as *const (), // SetExtendModeX
    nothing as *const (), // SetExtendModeY
    bn_zero as *const (), // GetExtendModeX
    bn_zero as *const (), // GetExtendModeY
]);
static EFFECT_NODE_VT: VTable<4> = VTable([
    bn_qi as *const (),
    add_ref::<BuiltinNode> as *const (),
    release::<BuiltinNode> as *const (),
    bn_inputs as *const (),
]);

unsafe fn as_builtin(p: *mut c_void) -> Option<Builtin> {
    let vt = *(p as *const *const ());
    [
        OFFSET_VT.ptr(),
        BOUNDS_VT.ptr(),
        BORDER_VT.ptr(),
        EFFECT_NODE_VT.ptr(),
    ]
    .contains(&vt)
    .then(|| *crate::dlls::lock(&Com::get(p as *mut BuiltinObj).kind))
}

// --- ID2D1EffectContext -----------------------------------------------------------------

/// Pixel shaders chargés (communs à tous les effets du processus).
static SHADERS: Mutex<Vec<(Guid, Arc<Vec<u8>>)>> = Mutex::new(Vec::new());

struct Context {
    effect: *mut EffectObj,
}
unsafe impl Send for Context {}
unsafe impl Sync for Context {}
type ContextObj = Com<Context>;

unsafe extern "win64" fn ctx_qi(
    this: *mut ContextObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    // Paint.NET 5 demande ID2D1EffectContext2 dans Initialize (#1206).
    qi_list(
        this,
        riid,
        out,
        &[IID_EFFECT_CONTEXT, IID_EFFECT_CONTEXT1, IID_EFFECT_CONTEXT2],
    )
}
unsafe extern "win64" fn ctx_dpi(_: *mut ContextObj, x: *mut f32, y: *mut f32) {
    if !x.is_null() {
        *x = 96.0;
    }
    if !y.is_null() {
        *y = 96.0;
    }
}
unsafe extern "win64" fn ctx_create_effect(
    this: *mut ContextObj,
    id: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    let f = Com::get(Com::get(this).effect).factory;
    super::create_effect(f, id, out)
}
unsafe extern "win64" fn ctx_feature_level(
    _: *mut ContextObj,
    levels: *const u32,
    n: u32,
    out: *mut u32,
) -> i32 {
    if levels.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    for i in 0..n as usize {
        if *levels.add(i) <= 0xb100 {
            *out = *levels.add(i);
            return S_OK;
        }
    }
    0x887a_0004u32 as i32 // DXGI_ERROR_UNSUPPORTED
}
unsafe extern "win64" fn ctx_node_from_effect(
    _: *mut ContextObj,
    effect: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if effect.is_null() {
        return E_INVALIDARG;
    }
    com_add_ref(effect);
    new_builtin(EFFECT_NODE_VT.ptr(), Builtin::Effect(effect), out)
}
unsafe extern "win64" fn ctx_blend(
    _: *mut ContextObj,
    _: u32,
    _: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    crate::dlls::d2d1::unsupported_call("ID2D1EffectContext::CreateBlendTransform")
}
unsafe extern "win64" fn ctx_border(
    _: *mut ContextObj,
    _: u32,
    _: u32,
    out: *mut *mut c_void,
) -> i32 {
    new_builtin(BORDER_VT.ptr(), Builtin::Border, out)
}
unsafe extern "win64" fn ctx_offset(_: *mut ContextObj, p: u64, out: *mut *mut c_void) -> i32 {
    new_builtin(
        OFFSET_VT.ptr(),
        Builtin::Offset(p as u32 as i32, (p >> 32) as u32 as i32),
        out,
    )
}
unsafe extern "win64" fn ctx_bounds(
    _: *mut ContextObj,
    r: *const [i32; 4],
    out: *mut *mut c_void,
) -> i32 {
    let Some(r) = r.as_ref() else {
        return E_INVALIDARG;
    };
    new_builtin(BOUNDS_VT.ptr(), Builtin::Bounds(*r), out)
}
unsafe extern "win64" fn ctx_load_shader(
    _: *mut ContextObj,
    id: *const Guid,
    code: *const u8,
    n: u32,
) -> i32 {
    let Some(id) = id.as_ref() else {
        return E_INVALIDARG;
    };
    if code.is_null() || n == 0 {
        return E_INVALIDARG;
    }
    let mut s = crate::dlls::lock(&SHADERS);
    // Un identifiant déjà chargé garde son premier code, comme sous Windows.
    if !s.iter().any(|(g, _)| g == id) {
        s.push((
            *id,
            Arc::new(std::slice::from_raw_parts(code, n as usize).to_vec()),
        ));
    }
    S_OK
}
unsafe extern "win64" fn ctx_load_other(
    _: *mut ContextObj,
    _: *const Guid,
    _: *const u8,
    _: u32,
) -> i32 {
    S_OK
}
unsafe extern "win64" fn ctx_is_loaded(_: *mut ContextObj, id: *const Guid) -> i32 {
    let Some(id) = id.as_ref() else { return 0 };
    crate::dlls::lock(&SHADERS).iter().any(|(g, _)| g == id) as i32
}
unsafe extern "win64" fn ctx_resource_texture(
    _: *mut ContextObj,
    _: *const Guid,
    _: *const c_void,
    _: *const u8,
    _: *const u32,
    _: u32,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    crate::dlls::d2d1::unsupported_call("ID2D1EffectContext::CreateResourceTexture")
}
unsafe extern "win64" fn ctx_find(
    _: *mut ContextObj,
    _: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOT_FOUND
}
unsafe extern "win64" fn ctx_vertex_buffer(
    _: *mut ContextObj,
    _: *const c_void,
    _: *const Guid,
    _: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    crate::dlls::d2d1::unsupported_call("ID2D1EffectContext::CreateVertexBuffer")
}
unsafe fn ctx_factory(this: *mut ContextObj) -> *mut super::FactoryObj {
    Com::get(Com::get(this).effect).factory
}
unsafe extern "win64" fn ctx_color_context(
    this: *mut ContextObj,
    space: u32,
    profile: *const u8,
    size: u32,
    out: *mut *mut c_void,
) -> i32 {
    let profile = if profile.is_null() || size == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(profile, size as usize).to_vec()
    };
    let dxgi = if space == 2 { 10 } else { 0 };
    super::new_color_in(ctx_factory(this), space, profile, dxgi, out)
}
unsafe extern "win64" fn ctx_color_context_file(
    this: *mut ContextObj,
    path: *const u16,
    out: *mut *mut c_void,
) -> i32 {
    match super::read_profile(path) {
        Some(bytes) => super::new_color_in(ctx_factory(this), 0, bytes, 0, out),
        None => {
            if !out.is_null() {
                *out = std::ptr::null_mut();
            }
            0x8007_0002u32 as i32
        }
    }
}
unsafe extern "win64" fn ctx_color_context_wic(
    this: *mut ContextObj,
    _wic: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    // Profil WIC non lu : traité comme sRGB, comme le contexte de dessin.
    super::new_color_in(ctx_factory(this), 1, Vec::new(), 0, out)
}
/// ID2D1EffectContext2::CreateColorContextFromDxgiColorSpace.
unsafe extern "win64" fn ctx_color_context_dxgi(
    this: *mut ContextObj,
    space: u32,
    out: *mut *mut c_void,
) -> i32 {
    let d2d = if space == 1 { 2 } else { 1 };
    super::new_color_in(ctx_factory(this), d2d, Vec::new(), space, out)
}
/// ID2D1EffectContext2::CreateColorContextFromSimpleColorProfile.
unsafe extern "win64" fn ctx_color_context_simple(
    this: *mut ContextObj,
    _profile: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    super::new_color_in(ctx_factory(this), 0, Vec::new(), 0, out)
}
/// ID2D1EffectContext1::CreateLookupTable3D : pas de table 3D pour l'instant.
unsafe extern "win64" fn ctx_lookup_table(
    _: *mut ContextObj,
    _: u32,
    _: *const u32,
    _: *const u8,
    _: u32,
    _: *const u32,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    crate::dlls::d2d1::unsupported_call("ID2D1EffectContext1::CreateLookupTable3D")
}
unsafe extern "win64" fn ctx_feature(_: *mut ContextObj, _: u32, data: *mut u8, n: u32) -> i32 {
    if data.is_null() {
        return E_INVALIDARG;
    }
    // D2D1_FEATURE_DOUBLES / D3D10_X_HARDWARE_OPTIONS : non.
    std::ptr::write_bytes(data, 0, n as usize);
    S_OK
}
unsafe extern "win64" fn ctx_precision(_: *mut ContextObj, p: u32) -> i32 {
    // 8 bits (1, 2), 16 bits flottant (4), 32 bits flottant (6).
    matches!(p, 1 | 2 | 4 | 6) as i32
}

static CONTEXT_VT: VTable<27> = VTable([
    ctx_qi as *const (),
    add_ref::<Context> as *const (),
    release::<Context> as *const (),
    ctx_dpi as *const (),
    ctx_create_effect as *const (),
    ctx_feature_level as *const (),
    ctx_node_from_effect as *const (),
    ctx_blend as *const (),
    ctx_border as *const (),
    ctx_offset as *const (),
    ctx_bounds as *const (),
    ctx_load_shader as *const (),
    ctx_load_other as *const (), // LoadVertexShader
    ctx_load_other as *const (), // LoadComputeShader
    ctx_is_loaded as *const (),
    ctx_resource_texture as *const (),
    ctx_find as *const (), // FindResourceTexture
    ctx_vertex_buffer as *const (),
    ctx_find as *const (), // FindVertexBuffer
    ctx_color_context as *const (),
    ctx_color_context_file as *const (),
    ctx_color_context_wic as *const (),
    ctx_feature as *const (),
    ctx_precision as *const (),
    // ID2D1EffectContext1
    ctx_lookup_table as *const (),
    // ID2D1EffectContext2
    ctx_color_context_dxgi as *const (),
    ctx_color_context_simple as *const (),
]);

// --- Calcul ------------------------------------------------------------------------------

/// Rectangle « infini » de Direct2D : borné par la taille de la cible.
fn clamp_rect(r: [i32; 4], hint: (u32, u32)) -> [i32; 4] {
    const BIG: i32 = 1 << 24;
    let mut r = r;
    if r[0] < -BIG || r[2] > BIG || r[1] < -BIG || r[3] > BIG {
        r = [
            r[0].max(0),
            r[1].max(0),
            r[2].min(hint.0 as i32),
            r[3].min(hint.1 as i32),
        ];
    }
    r
}

/// Calcule la sortie d'un effet personnalisé. `hint` : taille de la cible,
/// pour les sorties sans limite (générateurs).
pub(super) unsafe fn render(effect: *mut EffectObj, depth: u32, hint: (u32, u32)) -> Option<Img> {
    let e = Com::get(effect);
    let c = e.custom.get()?;
    let change = std::mem::take(&mut *crate::dlls::lock(&c.pending));
    if change > 0 {
        let prepare = method::<unsafe extern "win64" fn(*mut c_void, u32) -> i32>(c.imp, 4);
        let hr = prepare(c.imp, change - 1);
        if hr < 0 {
            crate::log_warn!(
                "d2d1",
                "effet personnalisé : PrepareForRender a échoué ({:#x})",
                hr
            );
            return None;
        }
    }
    let g = Com::get(c.graph);
    let (passthrough, output) = {
        let s = crate::dlls::lock(&g.state);
        (s.passthrough, s.output)
    };
    if let Some(i) = passthrough {
        return effect_input(e, i, depth, hint);
    }
    eval_node(e, g, output?, depth, hint)
}

unsafe fn effect_input(e: &Effect, i: u32, depth: u32, hint: (u32, u32)) -> Option<Img> {
    let input = crate::dlls::lock(&e.inputs).get(i as usize).copied()?;
    super::image_img(input, depth + 1, hint)
}

unsafe fn eval_node(
    e: &Effect,
    g: &Graph,
    index: usize,
    depth: u32,
    hint: (u32, u32),
) -> Option<Img> {
    if depth > 32 {
        return None;
    }
    let (node, info, sources) = {
        let s = crate::dlls::lock(&g.state);
        let n = s.nodes.get(index)?;
        (n.node, n.info, n.inputs.clone())
    };
    let mut inputs = Vec::with_capacity(sources.len());
    for src in &sources {
        inputs.push(match *src {
            Source::None => None,
            Source::EffectInput(i) => effect_input(e, i, depth, hint),
            Source::Node(j) if j != index => eval_node(e, g, j, depth + 1, hint),
            Source::Node(_) => None,
        });
    }
    if let Some(b) = as_builtin(node) {
        let first = inputs.into_iter().next().flatten();
        return match b {
            Builtin::Offset(x, y) => first.map(|mut i| {
                i.x += x;
                i.y += y;
                i
            }),
            Builtin::Bounds(r) => first.map(|i| crop(i, r)),
            Builtin::Border => first,
            Builtin::Effect(fx) => super::image_of_effect(fx, depth + 1, hint),
        };
    }
    if info.is_null() {
        crate::log_debug!("d2d1", "nœud de graphe d'effet inconnu : entrée 0 recopiée");
        return inputs.into_iter().next().flatten();
    }
    // Rectangles des entrées → rectangle de sortie (ID2D1Transform).
    let rects: Vec<[i32; 4]> = inputs
        .iter()
        .map(|i| {
            i.as_ref()
                .map_or([0; 4], |i| [i.x, i.y, i.x + i.w as i32, i.y + i.h as i32])
        })
        .collect();
    let mut out = [0i32; 4];
    let mut opaque = [0i32; 4];
    let map_in = method::<
        unsafe extern "win64" fn(
            *mut c_void,
            *const [i32; 4],
            *const [i32; 4],
            u32,
            *mut [i32; 4],
            *mut [i32; 4],
        ) -> i32,
    >(node, 5);
    let zero = vec![[0i32; 4]; rects.len()];
    if map_in(
        node,
        rects.as_ptr(),
        zero.as_ptr(),
        rects.len() as u32,
        &mut out,
        &mut opaque,
    ) < 0
    {
        return None;
    }
    let out = clamp_rect(out, hint);
    let (w, h) = (
        (out[2] - out[0]).max(0) as u32,
        (out[3] - out[1]).max(0) as u32,
    );
    if w == 0 || h == 0 || w > 16384 || h > 16384 {
        return None;
    }
    let mut needed = vec![[0i32; 4]; rects.len()];
    let map_out = method::<
        unsafe extern "win64" fn(*mut c_void, *const [i32; 4], *mut [i32; 4], u32) -> i32,
    >(node, 4);
    let _ = map_out(node, &out, needed.as_mut_ptr(), needed.len() as u32);

    let (shader, constants, linear) = {
        let s = crate::dlls::lock(&Com::get(info).state);
        (s.shader, s.constants.clone(), s.linear.clone())
    };
    let code = shader.and_then(|id| {
        crate::dlls::lock(&SHADERS)
            .iter()
            .find(|(g, _)| *g == id)
            .map(|(_, c)| c.clone())
    });
    let Some(code) = code else {
        crate::log_debug!(
            "d2d1",
            "transformation sans pixel shader chargé : entrée 0 recopiée"
        );
        return inputs.into_iter().next().flatten();
    };
    let empty = Vec::new();
    let gl_inputs: Vec<crate::dlls::d3d11::d2d_shader::Input> = inputs
        .iter()
        .enumerate()
        .map(|(k, i)| match i {
            Some(i) => crate::dlls::d3d11::d2d_shader::Input {
                x: i.x,
                y: i.y,
                w: i.w,
                h: i.h,
                px: &i.px,
                linear: linear.get(k).copied().unwrap_or(true),
            },
            None => crate::dlls::d3d11::d2d_shader::Input {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
                px: &empty,
                linear: true,
            },
        })
        .collect();
    let px =
        crate::dlls::d3d11::d2d_shader::run(&code, &gl_inputs, &constants, out[0], out[1], w, h)?;
    Some(Img {
        x: out[0],
        y: out[1],
        w,
        h,
        px,
    })
}

fn crop(i: Img, r: [i32; 4]) -> Img {
    let (w, h) = ((r[2] - r[0]).max(0) as u32, (r[3] - r[1]).max(0) as u32);
    let mut px = vec![0u8; w as usize * h as usize * 4];
    for y in 0..h as i32 {
        let sy = r[1] + y - i.y;
        if sy < 0 || sy >= i.h as i32 {
            continue;
        }
        for x in 0..w as i32 {
            let sx = r[0] + x - i.x;
            if sx < 0 || sx >= i.w as i32 {
                continue;
            }
            let s = (sy as usize * i.w as usize + sx as usize) * 4;
            let d = (y as usize * w as usize + x as usize) * 4;
            px[d..d + 4].copy_from_slice(&i.px[s..s + 4]);
        }
    }
    Img {
        x: r[0],
        y: r[1],
        w,
        h,
        px,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_effect_description() {
        let xml = "<?xml version='1.0'?>\
            <Effect>\
              <Property name='DisplayName' type='string' value='Teinte'/>\
              <Property name='Author' type='string' value='Winlux'/>\
              <Property name='Category' type='string' value='Test'/>\
              <Property name='Description' type='string' value='Essai'/>\
              <Inputs><Input name='Source'/><Input name='Masque'/></Inputs>\
              <Property name='Force' type='float' value='0.5'>\
                <Property name='DisplayName' type='string' value='Force'/>\
              </Property>\
              <Property name='ConstantBuffer' type='blob'>\
                <Property name='DisplayName' type='string' value='ConstantBuffer'/>\
              </Property>\
              <Property name='Couleur' type='vector4' value='(1.0, 0.0, 0.0, 1.0)'/>\
            </Effect>";
        let r = parse(xml);
        assert_eq!(r.inputs, 2);
        assert_eq!(r.system[0], "Teinte");
        assert_eq!(r.system[3], "Essai");
        let names: Vec<_> = r.props.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Force", "ConstantBuffer", "Couleur"]);
        assert_eq!(r.props[0].ty, ty::FLOAT);
        assert_eq!(r.props[0].default, 0.5f32.to_le_bytes());
        assert_eq!(r.props[1].ty, ty::BLOB);
        assert_eq!(r.props[2].default.len(), 16);
    }

    #[test]
    fn vtables_have_windows_sizes() {
        assert_eq!(CONTEXT_VT.0.len(), 27);
        assert_eq!(GRAPH_VT.0.len(), 12);
        assert_eq!(DRAW_INFO_VT.0.len(), 12);
        assert_eq!(BORDER_VT.0.len(), 10);
    }

    /// Effet d'essai : son Initialize demande ID2D1EffectContext2 puis crée
    /// un contexte de couleur, comme le CheckerboardEffect de Paint.NET (#1206).
    mod essai {
        use super::*;
        pub static INIT_HR: Mutex<Option<i32>> = Mutex::new(None);
        #[repr(C)]
        pub struct Imp {
            vt: *const (),
        }
        unsafe extern "win64" fn qi(
            this: *mut Imp,
            riid: *const Guid,
            out: *mut *mut c_void,
        ) -> i32 {
            if *riid == IID_EFFECT_IMPL || *riid == crate::dlls::d2d1::IID_IUNKNOWN {
                *out = this.cast();
                return S_OK;
            }
            *out = std::ptr::null_mut();
            E_NOINTERFACE
        }
        unsafe extern "win64" fn one(_: *mut Imp) -> u32 {
            1
        }
        unsafe extern "win64" fn init(_: *mut Imp, ctx: *mut c_void, _graph: *mut c_void) -> i32 {
            let q = method::<
                unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
            >(ctx, 0);
            let mut ctx2 = std::ptr::null_mut();
            let mut hr = q(ctx, &IID_EFFECT_CONTEXT2, &mut ctx2);
            if hr >= 0 {
                // CreateColorContextFromDxgiColorSpace(DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709).
                let create = method::<
                    unsafe extern "win64" fn(*mut c_void, u32, *mut *mut c_void) -> i32,
                >(ctx2, 25);
                let mut color = std::ptr::null_mut();
                hr = create(ctx2, 0, &mut color);
                if hr >= 0 && color.is_null() {
                    hr = E_FAIL;
                }
                if !color.is_null() {
                    com_release(color);
                }
                com_release(ctx2);
            }
            *crate::dlls::lock(&INIT_HR) = Some(hr);
            hr
        }
        unsafe extern "win64" fn ok(_: *mut Imp, _: usize) -> i32 {
            S_OK
        }
        static VT: VTable<6> = VTable([
            qi as *const (),
            one as *const (),
            one as *const (),
            init as *const (),
            ok as *const (),
            ok as *const (),
        ]);
        static mut IMP: Imp = Imp {
            vt: std::ptr::null(),
        };
        pub unsafe extern "win64" fn create(out: *mut *mut c_void) -> i32 {
            let p = std::ptr::addr_of_mut!(IMP);
            (*p).vt = VT.ptr();
            *out = p.cast();
            S_OK
        }
    }

    #[test]
    fn initialize_obtient_le_contexte_d_effet_2() {
        unsafe {
            let f = Com::create(
                super::super::super::FACTORY_VT.ptr(),
                super::super::super::Factory,
            );
            let id = guid(0x1206_0001, 0, 0, [0, 0, 0, 0, 0, 0, 0, 1]);
            let xml: Vec<u16> = "<?xml version='1.0'?><Effect>\
                <Property name='DisplayName' type='string' value='Damier'/>\
                <Property name='Author' type='string' value=''/>\
                <Property name='Category' type='string' value=''/>\
                <Property name='Description' type='string' value=''/>\
                <Inputs/></Effect>"
                .encode_utf16()
                .chain([0])
                .collect();
            assert_eq!(
                super::super::factory_register_string(
                    f,
                    &id,
                    xml.as_ptr(),
                    std::ptr::null(),
                    0,
                    essai::create as *const () as usize
                ),
                S_OK
            );
            let mut e = std::ptr::null_mut();
            let hr = super::super::create_effect(f, &id, &mut e);
            assert_eq!(*crate::dlls::lock(&essai::INIT_HR), Some(S_OK));
            assert_eq!(hr, S_OK);
            assert!(!e.is_null());
            com_release(e);
            Com::release(f);
        }
    }

    /// Effet d'essai dont le setter exige qu'Initialize ait déjà eu lieu,
    /// comme l'effet personnalisé .NET de Paint.NET (#1268).
    mod ordre {
        use super::*;
        use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
        pub static INIT: AtomicBool = AtomicBool::new(false);
        pub static AVANT_INIT: AtomicU32 = AtomicU32::new(0);
        pub static VALEUR: AtomicU32 = AtomicU32::new(0);
        #[repr(C)]
        pub struct Imp {
            vt: *const (),
        }
        unsafe extern "win64" fn qi(
            this: *mut Imp,
            riid: *const Guid,
            out: *mut *mut c_void,
        ) -> i32 {
            if *riid == IID_EFFECT_IMPL || *riid == crate::dlls::d2d1::IID_IUNKNOWN {
                *out = this.cast();
                return S_OK;
            }
            *out = std::ptr::null_mut();
            E_NOINTERFACE
        }
        unsafe extern "win64" fn one(_: *mut Imp) -> u32 {
            1
        }
        unsafe extern "win64" fn init(_: *mut Imp, _: *mut c_void, _: *mut c_void) -> i32 {
            INIT.store(true, Ordering::SeqCst);
            S_OK
        }
        unsafe extern "win64" fn ok(_: *mut Imp, _: usize) -> i32 {
            S_OK
        }
        pub unsafe extern "win64" fn set(_: *mut c_void, data: *const u8, n: u32) -> i32 {
            if !INIT.load(Ordering::SeqCst) {
                AVANT_INIT.fetch_add(1, Ordering::SeqCst);
                return E_FAIL;
            }
            if n == 4 {
                VALEUR.store(
                    u32::from_le_bytes(*data.cast::<[u8; 4]>()),
                    Ordering::SeqCst,
                );
            }
            S_OK
        }
        pub unsafe extern "win64" fn get(_: *mut c_void, _: *mut u8, _: u32, n: *mut u32) -> i32 {
            if !n.is_null() {
                *n = 4;
            }
            S_OK
        }
        static VT: VTable<6> = VTable([
            qi as *const (),
            one as *const (),
            one as *const (),
            init as *const (),
            ok as *const (),
            ok as *const (),
        ]);
        static mut IMP: Imp = Imp {
            vt: std::ptr::null(),
        };
        pub unsafe extern "win64" fn create(out: *mut *mut c_void) -> i32 {
            let p = std::ptr::addr_of_mut!(IMP);
            (*p).vt = VT.ptr();
            *out = p.cast();
            S_OK
        }
    }

    #[test]
    fn valeurs_par_defaut_posees_apres_initialize() {
        use std::sync::atomic::Ordering;
        unsafe {
            let f = Com::create(
                super::super::super::FACTORY_VT.ptr(),
                super::super::super::Factory,
            );
            let id = guid(0x1268_0001, 0, 0, [0, 0, 0, 0, 0, 0, 0, 1]);
            let xml: Vec<u16> = "<?xml version='1.0'?><Effect>\
                <Property name='DisplayName' type='string' value='Damier'/>\
                <Property name='Author' type='string' value=''/>\
                <Property name='Category' type='string' value=''/>\
                <Property name='Description' type='string' value=''/>\
                <Inputs/>\
                <Property name='Taille' type='uint32' value='8'/>\
                </Effect>"
                .encode_utf16()
                .chain([0])
                .collect();
            let nom: Vec<u16> = "Taille".encode_utf16().chain([0]).collect();
            let liaisons = [
                nom.as_ptr() as usize,
                ordre::set as *const () as usize,
                ordre::get as *const () as usize,
            ];
            assert_eq!(
                super::super::factory_register_string(
                    f,
                    &id,
                    xml.as_ptr(),
                    liaisons.as_ptr().cast(),
                    1,
                    ordre::create as *const () as usize
                ),
                S_OK
            );
            let mut e = std::ptr::null_mut();
            let hr = super::super::create_effect(f, &id, &mut e);
            assert_eq!(hr, S_OK);
            assert_eq!(ordre::AVANT_INIT.load(Ordering::SeqCst), 0);
            assert_eq!(ordre::VALEUR.load(Ordering::SeqCst), 8);
            com_release(e);
            Com::release(f);
        }
    }

    #[test]
    fn effet_rend_sa_fabrique_par_id2d1resource() {
        unsafe {
            let f = Com::create(
                super::super::super::FACTORY_VT.ptr(),
                super::super::super::Factory,
            );
            let id = guid(0x1291_0001, 0, 0, [0, 0, 0, 0, 0, 0, 0, 1]);
            let xml: Vec<u16> = "<?xml version='1.0'?><Effect>\
                <Property name='DisplayName' type='string' value='Damier'/>\
                <Property name='Author' type='string' value=''/>\
                <Property name='Category' type='string' value=''/>\
                <Property name='Description' type='string' value=''/>\
                <Inputs/></Effect>"
                .encode_utf16()
                .chain([0])
                .collect();
            assert_eq!(
                super::super::factory_register_string(
                    f,
                    &id,
                    xml.as_ptr(),
                    std::ptr::null(),
                    0,
                    essai::create as *const () as usize
                ),
                S_OK
            );
            let mut e = std::ptr::null_mut();
            assert_eq!(super::super::create_effect(f, &id, &mut e), S_OK);
            // QueryInterface(ID2D1Resource) puis GetFactory (case 3), comme
            // Paint.NET (#1291).
            let qi = method::<
                unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
            >(e, 0);
            let mut res = std::ptr::null_mut();
            assert_eq!(qi(e, &super::super::IID_D2D1_RESOURCE, &mut res), S_OK);
            assert!(!res.is_null());
            let get_factory =
                method::<unsafe extern "win64" fn(*mut c_void, *mut *mut c_void)>(res, 3);
            let mut got = std::ptr::null_mut();
            get_factory(res, &mut got);
            assert_eq!(got, f.cast());
            com_release(got);
            // La facette rend l'effet pour ID2D1Effect.
            let qi2 = method::<
                unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
            >(res, 0);
            let mut back = std::ptr::null_mut();
            assert_eq!(qi2(res, &super::super::IID_D2D1_EFFECT, &mut back), S_OK);
            assert_eq!(back, e);
            com_release(back);
            com_release(res);
            // QueryInterface(ID2D1Image) rend la sortie de l'effet, comme
            // GetOutput : Paint.NET la passe à DrawImage (#1297).
            let mut img = std::ptr::null_mut();
            assert_eq!(qi(e, &super::super::IID_D2D1_IMAGE, &mut img), S_OK);
            assert!(!img.is_null());
            let get_factory =
                method::<unsafe extern "win64" fn(*mut c_void, *mut *mut c_void)>(img, 3);
            let mut f2 = std::ptr::null_mut();
            get_factory(img, &mut f2);
            assert_eq!(f2, f.cast());
            com_release(f2);
            com_release(img);
            com_release(e);
            Com::release(f);
        }
    }

    #[test]
    fn infinite_output_is_bounded_by_target() {
        assert_eq!(
            clamp_rect(
                [i32::MIN / 2, i32::MIN / 2, i32::MAX / 2, i32::MAX / 2],
                (800, 600)
            ),
            [0, 0, 800, 600]
        );
        assert_eq!(clamp_rect([-4, -4, 20, 20], (8, 8)), [-4, -4, 20, 20]);
    }
}
