//! Documents SVG de Direct2D (#2719) : `ID2D1DeviceContext5::CreateSvgDocument`
//! et `DrawSvgDocument`, `ID2D1SvgDocument`, `ID2D1SvgElement` et les objets
//! d'attribut (`ID2D1SvgPaint`, `ID2D1SvgStrokeDashArray`,
//! `ID2D1SvgPointCollection`, `ID2D1SvgPathData`).
//!
//! Le flux XML est lu par l'analyseur SVG maison (`imaging::svg`) ; le
//! document est un arbre d'éléments dont les attributs sont gardés en texte
//! SVG, ou en objet d'attribut vivant une fois demandé sous cette forme.
//! `DrawSvgDocument` sérialise l'arbre et le rastérise par
//! `imaging::svg::render_bgra`.
//!
//! Interfaces, identifiants, ordre des méthodes et valeurs des énumérations :
//! windows-rs 0.58 (MIT/Apache), projection de d2d1svg.h. Comportement :
//! documentation publique de Direct2D sur Learn.

use super::*;
use crate::imaging::svg as svgfmt;

const ERROR_INSUFFICIENT_BUFFER: i32 = 0x8007_007Au32 as i32;
const S_FALSE: i32 = 1;

const IID_SVG_ATTRIBUTE: Guid = guid(
    0xc9cdb0dd,
    0xf8c9,
    0x4e70,
    [0xb7, 0xc2, 0x30, 0x1c, 0x80, 0x29, 0x2c, 0x5e],
);
const IID_SVG_PAINT: Guid = guid(
    0xd59bab0a,
    0x68a2,
    0x455b,
    [0xa5, 0xdc, 0x9e, 0xb2, 0x85, 0x4e, 0x24, 0x90],
);
const IID_SVG_DASHES: Guid = guid(
    0xf1c0ca52,
    0x92a3,
    0x4f00,
    [0xb4, 0xce, 0xf3, 0x56, 0x91, 0xef, 0xd9, 0xd9],
);
const IID_SVG_POINTS: Guid = guid(
    0x9dbe4c0d,
    0x3572,
    0x4dd9,
    [0x98, 0x25, 0x55, 0x30, 0x81, 0x3b, 0xb7, 0x12],
);
const IID_SVG_PATH_DATA: Guid = guid(
    0xc095e4f4,
    0xbb98,
    0x43d6,
    [0x97, 0x45, 0x4d, 0x1b, 0x84, 0xec, 0x98, 0x88],
);
const IID_SVG_DOCUMENT: Guid = guid(
    0x86b88e4d,
    0xafa4,
    0x4d7b,
    [0x88, 0xe4, 0x68, 0xa5, 0x1c, 0x4a, 0x0a, 0xec],
);
const IID_SVG_ELEMENT: Guid = guid(
    0xac7b67a6,
    0x183e,
    0x49c1,
    [0xa8, 0x23, 0x0e, 0xbe, 0x40, 0xb0, 0xdb, 0x29],
);

// ---------------------------------------------------------------------
// Objets
// ---------------------------------------------------------------------

pub(super) struct Doc {
    st: Mutex<DocSt>,
}

struct DocSt {
    viewport: (f32, f32),
    /// Racine (tenue).
    root: *mut ElemObj,
}

type DocObj = Com<Res<Doc>>;

struct Elem {
    st: Mutex<ElemSt>,
}

struct ElemSt {
    tag: String,
    /// Contenu texte : `Some` pour un nœud de texte.
    text: Option<String>,
    attrs: Vec<(String, AttrVal)>,
    /// Enfants (tenus).
    kids: Vec<*mut ElemObj>,
    /// Parent et document (non tenus ; remis à nul quand ils meurent).
    parent: *mut ElemObj,
    doc: *mut DocObj,
}

enum AttrVal {
    Str(String),
    /// Objet d'attribut vivant (tenu).
    Obj(*mut AttrObj),
}

type ElemObj = Com<Res<Elem>>;

#[derive(Clone)]
enum AttrData {
    Paint {
        kind: u32,
        color: [f32; 4],
        id: String,
    },
    /// (valeur, unités : 0 nombre, 1 pourcentage).
    Dashes(Vec<(f32, u32)>),
    Points(Vec<(f32, f32)>),
    Path {
        data: Vec<f32>,
        commands: Vec<u32>,
    },
}

struct Attr {
    st: Mutex<AttrSt>,
}

struct AttrSt {
    data: AttrData,
    /// Élément qui le porte (non tenu).
    elem: *mut ElemObj,
}

type AttrObj = Com<Res<Attr>>;

impl Drop for Elem {
    fn drop(&mut self) {
        let st = self.st.get_mut().unwrap_or_else(|e| e.into_inner());
        unsafe {
            for k in std::mem::take(&mut st.kids) {
                lock(&elem(k).st).parent = std::ptr::null_mut();
                Com::release(k);
            }
            for (_, v) in std::mem::take(&mut st.attrs) {
                if let AttrVal::Obj(a) = v {
                    lock(&Com::get(a).data.st).elem = std::ptr::null_mut();
                    Com::release(a);
                }
            }
        }
    }
}

impl Drop for Doc {
    fn drop(&mut self) {
        let st = self.st.get_mut().unwrap_or_else(|e| e.into_inner());
        unsafe {
            if !st.root.is_null() {
                set_doc(st.root, std::ptr::null_mut());
                Com::release(st.root);
            }
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    crate::dlls::lock(m)
}

unsafe fn elem<'a>(p: *mut ElemObj) -> &'a Elem {
    &Com::get(p).data
}

unsafe fn is_elem(p: *mut c_void) -> bool {
    !p.is_null() && *(p as *const *const ()) == ELEM_VT.ptr()
}

unsafe fn attr_of(p: *mut c_void) -> Option<*mut AttrObj> {
    if p.is_null() {
        return None;
    }
    let vt = *(p as *const *const ());
    [
        PAINT_VT.ptr(),
        DASHES_VT.ptr(),
        POINTS_VT.ptr(),
        PATH_VT.ptr(),
    ]
    .contains(&vt)
    .then_some(p as *mut AttrObj)
}

/// Pose le document de tout un sous-arbre.
unsafe fn set_doc(e: *mut ElemObj, d: *mut DocObj) {
    let kids = {
        let mut st = lock(&elem(e).st);
        st.doc = d;
        st.kids.clone()
    };
    for k in kids {
        set_doc(k, d);
    }
}

unsafe fn new_elem(f: *mut FactoryObj, tag: &str, text: Option<String>) -> *mut ElemObj {
    new_res(
        f,
        ELEM_VT.ptr(),
        Elem {
            st: Mutex::new(ElemSt {
                tag: tag.to_string(),
                text,
                attrs: Vec::new(),
                kids: Vec::new(),
                parent: std::ptr::null_mut(),
                doc: std::ptr::null_mut(),
            }),
        },
    )
}

unsafe fn new_attr(f: *mut FactoryObj, data: AttrData) -> *mut AttrObj {
    let vt = match data {
        AttrData::Paint { .. } => PAINT_VT.ptr(),
        AttrData::Dashes(_) => DASHES_VT.ptr(),
        AttrData::Points(_) => POINTS_VT.ptr(),
        AttrData::Path { .. } => PATH_VT.ptr(),
    };
    new_res(
        f,
        vt,
        Attr {
            st: Mutex::new(AttrSt {
                data,
                elem: std::ptr::null_mut(),
            }),
        },
    )
}

/// Élément (tenu) construit depuis un nœud XML, enfants compris.
unsafe fn build(f: *mut FactoryObj, n: &svgfmt::Node) -> *mut ElemObj {
    let e = new_elem(f, &n.name, None);
    let mut kids = Vec::new();
    if !n.text.trim().is_empty() {
        kids.push(new_elem(f, "", Some(n.text.clone())));
    }
    for k in &n.kids {
        kids.push(build(f, k));
    }
    for &k in &kids {
        lock(&elem(k).st).parent = e;
    }
    let mut st = lock(&elem(e).st);
    st.attrs = n
        .attrs
        .iter()
        .map(|(k, v)| (k.clone(), AttrVal::Str(v.clone())))
        .collect();
    st.kids = kids;
    drop(st);
    e
}

// ---------------------------------------------------------------------
// Texte SVG des attributs
// ---------------------------------------------------------------------

fn fmt_num(v: f32) -> String {
    let s = format!("{v}");
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

fn color_hex(c: [f32; 4]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

fn color_parse(s: &str) -> Option<[f32; 4]> {
    let c = svgfmt::parse_color(s)?;
    Some([
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        c[3] as f32 / 255.0,
    ])
}

fn nums(s: &str) -> Vec<f32> {
    s.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.trim_end_matches('%').parse().ok())
        .collect()
}

/// Nombre de valeurs de chaque commande de chemin (D2D1_SVG_PATH_COMMAND).
fn arity(cmd: u32) -> Option<usize> {
    Some(match cmd {
        0 => 0,
        1..=4 => 2,
        5 | 6 => 6,
        7 | 8 => 4,
        9 | 10 => 7,
        11..=14 => 1,
        15 | 16 => 4,
        17 | 18 => 2,
        _ => return None,
    })
}

const PATH_LETTERS: &[u8; 19] = b"ZMmLlCcQqAaHhVvSsTt";

fn path_command(letter: u8) -> Option<u32> {
    if letter == b'z' {
        return Some(0);
    }
    PATH_LETTERS
        .iter()
        .position(|&c| c == letter)
        .map(|i| i as u32)
}

/// Analyse un attribut `d` en commandes et valeurs.
fn parse_d(d: &str) -> (Vec<f32>, Vec<u32>) {
    let b = d.as_bytes();
    let (mut data, mut cmds) = (Vec::new(), Vec::new());
    let mut cur: Option<u32> = None;
    let mut pending: Vec<f32> = Vec::new();
    let mut i = 0;
    let flush = |cur: &mut Option<u32>,
                 pending: &mut Vec<f32>,
                 data: &mut Vec<f32>,
                 cmds: &mut Vec<u32>| {
        let Some(mut c) = *cur else {
            pending.clear();
            return;
        };
        let n = arity(c).unwrap_or(0);
        if n == 0 {
            cmds.push(c);
            pending.clear();
            return;
        }
        let mut first = true;
        for chunk in pending.chunks(n) {
            if chunk.len() < n {
                break;
            }
            if !first && (c == 1 || c == 2) {
                c += 2; // un moveto répété est un lineto
            }
            cmds.push(c);
            data.extend_from_slice(chunk);
            first = false;
        }
        *cur = Some(c);
        pending.clear();
    };
    while i < b.len() {
        let ch = b[i];
        if ch.is_ascii_alphabetic() && ch != b'e' && ch != b'E' {
            flush(&mut cur, &mut pending, &mut data, &mut cmds);
            cur = path_command(ch);
            if cur == Some(0) {
                cmds.push(0);
                cur = None;
            }
            i += 1;
            continue;
        }
        if ch.is_ascii_digit() || ch == b'-' || ch == b'+' || ch == b'.' {
            let st = i;
            i += 1;
            let mut dot = ch == b'.';
            while i < b.len() {
                let c = b[i];
                if c.is_ascii_digit() {
                    i += 1;
                } else if c == b'.' && !dot {
                    dot = true;
                    i += 1;
                } else if (c == b'e' || c == b'E') && i + 1 < b.len() {
                    i += 1;
                    if b[i] == b'-' || b[i] == b'+' {
                        i += 1;
                    }
                } else {
                    break;
                }
            }
            if let Ok(v) = d[st..i].parse::<f32>() {
                pending.push(v);
            }
            continue;
        }
        i += 1;
    }
    flush(&mut cur, &mut pending, &mut data, &mut cmds);
    (data, cmds)
}

fn path_string(data: &[f32], cmds: &[u32]) -> String {
    let mut out = String::new();
    let mut k = 0;
    for &c in cmds {
        let Some(n) = arity(c) else { continue };
        if !out.is_empty() {
            out.push(' ');
        }
        out.push(PATH_LETTERS[c as usize] as char);
        for _ in 0..n {
            let v = data.get(k).copied().unwrap_or(0.0);
            out.push(' ');
            out.push_str(&fmt_num(v));
            k += 1;
        }
    }
    out
}

fn attr_string(a: &AttrData) -> String {
    match a {
        AttrData::Paint { kind, color, id } => {
            let url = format!("url(#{id})");
            match kind {
                0 => "none".into(),
                1 => color_hex(*color),
                2 => "currentColor".into(),
                3 => url,
                4 => format!("{url} none"),
                5 => format!("{url} {}", color_hex(*color)),
                6 => format!("{url} currentColor"),
                _ => "none".into(),
            }
        }
        AttrData::Dashes(v) => v
            .iter()
            .map(|(x, u)| format!("{}{}", fmt_num(*x), if *u == 1 { "%" } else { "" }))
            .collect::<Vec<_>>()
            .join(" "),
        AttrData::Points(v) => v
            .iter()
            .map(|(x, y)| format!("{},{}", fmt_num(*x), fmt_num(*y)))
            .collect::<Vec<_>>()
            .join(" "),
        AttrData::Path { data, commands } => path_string(data, commands),
    }
}

/// Objet d'attribut du type `iid` lu depuis un texte SVG.
fn attr_from_string(iid: &Guid, s: &str) -> Option<AttrData> {
    let s = s.trim();
    if *iid == IID_SVG_PAINT {
        let (url, rest) = match s.strip_prefix("url(") {
            Some(r) => {
                let (u, rest) = r.split_once(')')?;
                (
                    Some(u.trim().trim_start_matches('#').to_string()),
                    rest.trim(),
                )
            }
            None => (None, s),
        };
        let black = [0.0, 0.0, 0.0, 1.0];
        let (kind, color) = match (url.is_some(), rest) {
            (false, "none") => (0, black),
            (false, "currentColor") => (2, black),
            (false, c) => (1, color_parse(c)?),
            (true, "") => (3, black),
            (true, "none") => (4, black),
            (true, "currentColor") => (6, black),
            (true, c) => (5, color_parse(c)?),
        };
        return Some(AttrData::Paint {
            kind,
            color,
            id: url.unwrap_or_default(),
        });
    }
    if *iid == IID_SVG_DASHES {
        let v = s
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|t| !t.is_empty())
            .filter_map(|t| match t.strip_suffix('%') {
                Some(p) => p.parse().ok().map(|x| (x, 1)),
                None => t.parse().ok().map(|x| (x, 0)),
            })
            .collect();
        return Some(AttrData::Dashes(v));
    }
    if *iid == IID_SVG_POINTS {
        let n = nums(s);
        return Some(AttrData::Points(
            n.as_chunks::<2>().0.iter().map(|p| (p[0], p[1])).collect(),
        ));
    }
    if *iid == IID_SVG_PATH_DATA {
        let (data, commands) = parse_d(s);
        return Some(AttrData::Path { data, commands });
    }
    None
}

fn attr_iid(a: &AttrData) -> Guid {
    match a {
        AttrData::Paint { .. } => IID_SVG_PAINT,
        AttrData::Dashes(_) => IID_SVG_DASHES,
        AttrData::Points(_) => IID_SVG_POINTS,
        AttrData::Path { .. } => IID_SVG_PATH_DATA,
    }
}

const ALIGNS: [&str; 10] = [
    "none", "xMinYMin", "xMidYMin", "xMaxYMin", "xMinYMid", "xMidYMid", "xMaxYMid", "xMinYMax",
    "xMidYMax", "xMaxYMax",
];

/// Taille attendue d'une valeur D2D1_SVG_ATTRIBUTE_POD_TYPE.
fn pod_size(ty: u32) -> Option<usize> {
    Some(match ty {
        0 | 2..=7 | 9 | 10 => 4,
        1 => 16,
        8 => 24,
        11 => 12,
        12 => 16,
        13 => 8,
        _ => return None,
    })
}

fn f32_at(b: &[u8], i: usize) -> f32 {
    f32::from_le_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]])
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]])
}

const FILL_MODES: [&str; 2] = ["evenodd", "nonzero"];
const DISPLAYS: [&str; 2] = ["inline", "none"];
const OVERFLOWS: [&str; 2] = ["visible", "hidden"];
const CAPS: [&str; 3] = ["butt", "square", "round"];
const JOINS: [&str; 4] = ["", "bevel", "round", "miter"];
const VISIBILITIES: [&str; 2] = ["visible", "hidden"];
const UNITS: [&str; 2] = ["userSpaceOnUse", "objectBoundingBox"];
const EXTENDS: [&str; 3] = ["pad", "repeat", "reflect"];

fn pick(list: &[&str], v: u32) -> Option<String> {
    list.get(v as usize)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn pod_to_string(ty: u32, b: &[u8]) -> Option<String> {
    Some(match ty {
        0 => fmt_num(f32_at(b, 0)),
        1 => color_hex([f32_at(b, 0), f32_at(b, 1), f32_at(b, 2), f32_at(b, 3)]),
        2 => pick(&FILL_MODES, u32_at(b, 0))?,
        3 => pick(&DISPLAYS, u32_at(b, 0))?,
        4 => pick(&OVERFLOWS, u32_at(b, 0))?,
        5 => pick(&CAPS, u32_at(b, 0))?,
        6 => pick(&JOINS, u32_at(b, 0))?,
        7 => pick(&VISIBILITIES, u32_at(b, 0))?,
        8 => format!(
            "matrix({})",
            (0..6)
                .map(|i| fmt_num(f32_at(b, i)))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        9 => pick(&UNITS, u32_at(b, 0))?,
        10 => pick(&EXTENDS, u32_at(b, 0))?,
        11 => {
            let defer = if u32_at(b, 0) != 0 { "defer " } else { "" };
            let align = pick(&ALIGNS, u32_at(b, 1))?;
            let scale = if u32_at(b, 2) == 1 { " slice" } else { "" };
            format!("{defer}{align}{scale}")
        }
        12 => (0..4)
            .map(|i| fmt_num(f32_at(b, i)))
            .collect::<Vec<_>>()
            .join(" "),
        13 => format!(
            "{}{}",
            fmt_num(f32_at(b, 0)),
            if u32_at(b, 1) == 1 { "%" } else { "" }
        ),
        _ => return None,
    })
}

/// Liste de transformations SVG (matrix, translate, scale, rotate, skewX,
/// skewY) en matrice m11 m12 m21 m22 dx dy.
fn parse_transform(s: &str) -> Option<[f32; 6]> {
    let mut m = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut rest = s.trim();
    while !rest.is_empty() {
        let (name, r) = rest.split_once('(')?;
        let (args, r) = r.split_once(')')?;
        let a = nums(args);
        let get = |i: usize, d: f32| a.get(i).copied().unwrap_or(d);
        let t: [f32; 6] = match name.trim().trim_start_matches(',').trim() {
            "matrix" if a.len() == 6 => [a[0], a[1], a[2], a[3], a[4], a[5]],
            "translate" => [1.0, 0.0, 0.0, 1.0, get(0, 0.0), get(1, 0.0)],
            "scale" => [get(0, 1.0), 0.0, 0.0, get(1, get(0, 1.0)), 0.0, 0.0],
            "rotate" => {
                let (s, c) = get(0, 0.0).to_radians().sin_cos();
                let (cx, cy) = (get(1, 0.0), get(2, 0.0));
                [c, s, -s, c, cx - c * cx + s * cy, cy - s * cx - c * cy]
            }
            "skewX" => [1.0, 0.0, get(0, 0.0).to_radians().tan(), 1.0, 0.0, 0.0],
            "skewY" => [1.0, get(0, 0.0).to_radians().tan(), 0.0, 1.0, 0.0, 0.0],
            _ => return None,
        };
        // m ← m · t (t appliquée d'abord, comme en SVG)
        m = [
            t[0] * m[0] + t[1] * m[2],
            t[0] * m[1] + t[1] * m[3],
            t[2] * m[0] + t[3] * m[2],
            t[2] * m[1] + t[3] * m[3],
            t[4] * m[0] + t[5] * m[2] + m[4],
            t[4] * m[1] + t[5] * m[3] + m[5],
        ];
        rest = r.trim();
    }
    Some(m)
}

fn find(list: &[&str], s: &str) -> Option<u32> {
    list.iter()
        .position(|x| !x.is_empty() && *x == s)
        .map(|i| i as u32)
}

fn string_to_pod(ty: u32, s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    let f = |v: &[f32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
    let u = |v: u32| v.to_le_bytes().to_vec();
    Some(match ty {
        0 => f(&[s.trim_end_matches("px").parse().ok()?]),
        1 => f(&color_parse(s)?),
        2 => u(find(&FILL_MODES, s)?),
        3 => u(find(&DISPLAYS, s)?),
        4 => u(find(&OVERFLOWS, s)?),
        5 => u(find(&CAPS, s)?),
        6 => u(find(&JOINS, s)?),
        7 => u(find(&VISIBILITIES, s)?),
        8 => f(&parse_transform(s)?),
        9 => u(find(&UNITS, s)?),
        10 => u(find(&EXTENDS, s)?),
        11 => {
            let mut w = s.split_whitespace().peekable();
            let defer = w.next_if_eq(&"defer").is_some() as u32;
            let align = find(&ALIGNS, w.next()?)?;
            let scale = (w.next() == Some("slice")) as u32;
            [defer, align, scale]
                .iter()
                .flat_map(|x| x.to_le_bytes())
                .collect()
        }
        12 => {
            let v = nums(s);
            if v.len() != 4 {
                return None;
            }
            f(&v)
        }
        13 => {
            let (v, unit) = match s.strip_suffix('%') {
                Some(p) => (p.parse().ok()?, 1u32),
                None => (s.trim_end_matches("px").parse().ok()?, 0),
            };
            let mut out = f(&[v]);
            out.extend_from_slice(&unit.to_le_bytes());
            out
        }
        _ => return None,
    })
}

// ---------------------------------------------------------------------
// Sérialisation
// ---------------------------------------------------------------------

fn escape(s: &str, quote: bool) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' if quote => o.push_str("&quot;"),
            c => o.push(c),
        }
    }
    o
}

unsafe fn attr_text(v: &AttrVal) -> String {
    match v {
        AttrVal::Str(s) => s.clone(),
        AttrVal::Obj(a) => attr_string(&lock(&Com::get(*a).data.st).data),
    }
}

unsafe fn to_xml(e: *mut ElemObj, out: &mut String) {
    let (tag, text, attrs, kids) = {
        let st = lock(&elem(e).st);
        (
            st.tag.clone(),
            st.text.clone(),
            st.attrs
                .iter()
                .map(|(k, v)| (k.clone(), attr_text(v)))
                .collect::<Vec<_>>(),
            st.kids.clone(),
        )
    };
    if let Some(t) = text {
        out.push_str(&escape(&t, false));
        return;
    }
    out.push('<');
    out.push_str(&tag);
    for (k, v) in attrs {
        out.push_str(&format!(" {k}=\"{}\"", escape(&v, true)));
    }
    if kids.is_empty() {
        out.push_str("/>");
        return;
    }
    out.push('>');
    for k in kids {
        to_xml(k, out);
    }
    out.push_str(&format!("</{tag}>"));
}

// ---------------------------------------------------------------------
// Chaînes larges
// ---------------------------------------------------------------------

unsafe fn wide_in(p: *const u16) -> Option<String> {
    (!p.is_null()).then(|| crate::dlls::wstr(p))
}

unsafe fn wide_in_n(p: *const u16, n: u32) -> String {
    if p.is_null() {
        return String::new();
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, n as usize))
}

/// Copie `s` terminée par un nul dans `buf` de `len` caractères.
unsafe fn wide_out(s: &str, buf: *mut u16, len: u32) -> i32 {
    let w: Vec<u16> = s.encode_utf16().collect();
    if buf.is_null() {
        return E_POINTER;
    }
    if w.len() + 1 > len as usize {
        return ERROR_INSUFFICIENT_BUFFER;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    *buf.add(w.len()) = 0;
    S_OK
}

fn wlen(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

// ---------------------------------------------------------------------
// Flux
// ---------------------------------------------------------------------

unsafe fn read_stream(stream: *mut c_void) -> Result<Vec<u8>, i32> {
    type Read = unsafe extern "win64" fn(*mut c_void, *mut u8, u32, *mut u32) -> i32;
    let read: Read = std::mem::transmute(*(*(stream as *const *const usize)).add(3));
    let mut out = Vec::new();
    let mut buf = vec![0u8; 65536];
    loop {
        let mut got = 0u32;
        let hr = read(stream, buf.as_mut_ptr(), buf.len() as u32, &mut got);
        if hr < 0 {
            return Err(hr);
        }
        out.extend_from_slice(&buf[..got as usize]);
        if got == 0 || hr == S_FALSE || out.len() > 64 << 20 {
            return Ok(out);
        }
    }
}

unsafe fn write_stream(stream: *mut c_void, data: &[u8]) -> i32 {
    type Write = unsafe extern "win64" fn(*mut c_void, *const u8, u32, *mut u32) -> i32;
    let write: Write = std::mem::transmute(*(*(stream as *const *const usize)).add(4));
    let mut done = 0usize;
    while done < data.len() {
        let mut n = 0u32;
        let hr = write(
            stream,
            data[done..].as_ptr(),
            (data.len() - done).min(u32::MAX as usize) as u32,
            &mut n,
        );
        if hr < 0 {
            return hr;
        }
        if n == 0 {
            return E_FAIL_STREAM;
        }
        done += n as usize;
    }
    S_OK
}

const E_FAIL_STREAM: i32 = 0x8003_001Du32 as i32; // STG_E_WRITEFAULT

/// Arbre lu depuis un flux XML (nul : vide).
unsafe fn parse_stream(f: *mut FactoryObj, stream: *mut c_void) -> Result<*mut ElemObj, i32> {
    let bytes = read_stream(stream)?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}');
    match svgfmt::parse_xml(text) {
        Some(n) if !n.name.is_empty() => Ok(build(f, &n)),
        _ => {
            crate::log_warn!("d2d1", "SVG : flux XML illisible ({} octets)", bytes.len());
            Err(E_INVALIDARG)
        }
    }
}

// ---------------------------------------------------------------------
// ID2D1DeviceContext5
// ---------------------------------------------------------------------

/// CreateSvgDocument(inputXmlStream, viewportSize, svgDocument). Un flux nul
/// donne un document vide (élément `svg` seul).
pub(super) unsafe extern "win64" fn create_svg_document(
    this: *mut TargetObj,
    stream: *mut c_void,
    viewport: u64,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let f = Com::get(this).factory;
    let root = if stream.is_null() {
        new_elem(f, "svg", None)
    } else {
        match parse_stream(f, stream) {
            Ok(r) => r,
            Err(hr) => return hr,
        }
    };
    let vp = (
        f32::from_bits(viewport as u32),
        f32::from_bits((viewport >> 32) as u32),
    );
    let d = new_res(
        f,
        DOC_VT.ptr(),
        Doc {
            st: Mutex::new(DocSt { viewport: vp, root }),
        },
    );
    set_doc(root, d);
    put(out, d)
}

/// DrawSvgDocument(svgDocument) : rastérise le document à la taille de sa
/// zone d'affichage.
pub(super) unsafe extern "win64" fn draw_svg_document(this: *mut TargetObj, doc: *mut c_void) {
    if doc.is_null() || *(doc as *const *const ()) != DOC_VT.ptr() {
        return;
    }
    let (vp, root) = {
        let st = lock(&Com::get(doc as *mut DocObj).data.st);
        (st.viewport, st.root)
    };
    if root.is_null() {
        return;
    }
    let mut xml = String::new();
    to_xml(root, &mut xml);
    let absolute = {
        let st = lock(&elem(root).st);
        let fixed = |n: &str| {
            st.attrs
                .iter()
                .any(|(k, v)| k == n && !attr_text(v).trim().ends_with('%'))
        };
        fixed("width") && fixed("height")
    };
    let (w, h) = (vp.0.ceil().max(0.0) as usize, vp.1.ceil().max(0.0) as usize);
    if w == 0 || h == 0 || w * h > 1 << 26 {
        return;
    }
    let Some(px) = svgfmt::render_bgra(xml.as_bytes(), w, h, !absolute) else {
        crate::log_warn!(
            "d2d1",
            "DrawSvgDocument : document non rendu ({} octets)",
            xml.len()
        );
        return;
    };
    let r = RectF {
        l: 0.0,
        t: 0.0,
        r: w as f32,
        b: h as f32,
    };
    draw_bitmap_pixels(this, &px, w as u32, h as u32, &r, 1.0, 1, &r);
}

// ---------------------------------------------------------------------
// ID2D1SvgDocument
// ---------------------------------------------------------------------

unsafe fn doc_st<'a>(p: *mut DocObj) -> std::sync::MutexGuard<'a, DocSt> {
    lock(&Com::get(p).data.st)
}

unsafe extern "win64" fn doc_qi(
    this: *mut DocObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_D2D1_RESOURCE, IID_SVG_DOCUMENT])
}

unsafe extern "win64" fn doc_set_viewport(this: *mut DocObj, size: u64) -> i32 {
    doc_st(this).viewport = (
        f32::from_bits(size as u32),
        f32::from_bits((size >> 32) as u32),
    );
    S_OK
}

unsafe extern "win64" fn doc_get_viewport(this: *mut DocObj, out: *mut [f32; 2]) {
    if !out.is_null() {
        let v = doc_st(this).viewport;
        *out = [v.0, v.1];
    }
}

/// SetRoot : la racine doit être un élément `svg` sans parent.
unsafe extern "win64" fn doc_set_root(this: *mut DocObj, root: *mut c_void) -> i32 {
    if !is_elem(root) {
        return E_INVALIDARG;
    }
    let r = root as *mut ElemObj;
    {
        let st = lock(&elem(r).st);
        if st.tag != "svg" || !st.parent.is_null() {
            return E_INVALIDARG;
        }
    }
    Com::add_ref(r);
    let old = std::mem::replace(&mut doc_st(this).root, r);
    if !old.is_null() {
        set_doc(old, std::ptr::null_mut());
        Com::release(old);
    }
    set_doc(r, this);
    S_OK
}

unsafe extern "win64" fn doc_get_root(this: *mut DocObj, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    let r = doc_st(this).root;
    if !r.is_null() {
        Com::add_ref(r);
    }
    *out = r as *mut c_void;
}

unsafe fn find_id(e: *mut ElemObj, id: &str) -> Option<*mut ElemObj> {
    let (hit, kids) = {
        let st = lock(&elem(e).st);
        let hit = st
            .attrs
            .iter()
            .any(|(k, v)| k == "id" && attr_text(v) == id);
        (hit, st.kids.clone())
    };
    if hit {
        return Some(e);
    }
    kids.into_iter().find_map(|k| find_id(k, id))
}

/// FindElementById : élément nul (S_OK) si aucun ne porte cet id.
unsafe extern "win64" fn doc_find(this: *mut DocObj, id: *const u16, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(id) = wide_in(id) else {
        return E_INVALIDARG;
    };
    let root = doc_st(this).root;
    if root.is_null() {
        return S_OK;
    }
    if let Some(e) = find_id(root, &id) {
        Com::add_ref(e);
        *out = e as *mut c_void;
    }
    S_OK
}

unsafe extern "win64" fn doc_serialize(
    this: *mut DocObj,
    stream: *mut c_void,
    subtree: *mut c_void,
) -> i32 {
    if stream.is_null() {
        return E_POINTER;
    }
    let e = if subtree.is_null() {
        doc_st(this).root
    } else if is_elem(subtree) {
        subtree as *mut ElemObj
    } else {
        return E_INVALIDARG;
    };
    if e.is_null() {
        return S_OK;
    }
    let mut xml = String::new();
    to_xml(e, &mut xml);
    write_stream(stream, xml.as_bytes())
}

/// Deserialize : sous-arbre détaché, lu depuis le flux.
unsafe extern "win64" fn doc_deserialize(
    this: *mut DocObj,
    stream: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || stream.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    match parse_stream(Com::get(this).factory, stream) {
        Ok(e) => {
            set_doc(e, this);
            put(out, e)
        }
        Err(hr) => hr,
    }
}

unsafe extern "win64" fn doc_create_paint(
    this: *mut DocObj,
    kind: u32,
    color: *const [f32; 4],
    id: *const u16,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if kind > 6 {
        return E_INVALIDARG;
    }
    let color = color.as_ref().copied().unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let id = wide_in(id).unwrap_or_default();
    put(
        out,
        new_attr(Com::get(this).factory, AttrData::Paint { kind, color, id }),
    )
}

unsafe extern "win64" fn doc_create_dashes(
    this: *mut DocObj,
    dashes: *const [u32; 2],
    count: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || (dashes.is_null() && count > 0) {
        return E_POINTER;
    }
    let v = (0..count as usize)
        .map(|i| {
            let d = *dashes.add(i);
            (f32::from_bits(d[0]), d[1])
        })
        .collect();
    put(out, new_attr(Com::get(this).factory, AttrData::Dashes(v)))
}

unsafe extern "win64" fn doc_create_points(
    this: *mut DocObj,
    points: *const [f32; 2],
    count: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || (points.is_null() && count > 0) {
        return E_POINTER;
    }
    let v = (0..count as usize)
        .map(|i| {
            let p = *points.add(i);
            (p[0], p[1])
        })
        .collect();
    put(out, new_attr(Com::get(this).factory, AttrData::Points(v)))
}

unsafe extern "win64" fn doc_create_path(
    this: *mut DocObj,
    data: *const f32,
    data_count: u32,
    commands: *const u32,
    command_count: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null()
        || (data.is_null() && data_count > 0)
        || (commands.is_null() && command_count > 0)
    {
        return E_POINTER;
    }
    let data = slice(data, data_count).to_vec();
    let commands = slice(commands, command_count).to_vec();
    put(
        out,
        new_attr(Com::get(this).factory, AttrData::Path { data, commands }),
    )
}

unsafe fn slice<'a, T>(p: *const T, n: u32) -> &'a [T] {
    if p.is_null() || n == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(p, n as usize)
    }
}

pub(super) static DOC_VT: VTable<15> = VTable([
    doc_qi as *const (),
    super::add_ref::<Res<Doc>> as *const (),
    super::release::<Res<Doc>> as *const (),
    get_factory::<Doc> as *const (),
    doc_set_viewport as *const (),
    doc_get_viewport as *const (),
    doc_set_root as *const (),
    doc_get_root as *const (),
    doc_find as *const (),
    doc_serialize as *const (),
    doc_deserialize as *const (),
    doc_create_paint as *const (),
    doc_create_dashes as *const (),
    doc_create_points as *const (),
    doc_create_path as *const (),
]);

// ---------------------------------------------------------------------
// ID2D1SvgElement
// ---------------------------------------------------------------------

unsafe fn est<'a>(p: *mut ElemObj) -> std::sync::MutexGuard<'a, ElemSt> {
    lock(&elem(p).st)
}

unsafe fn put_held(out: *mut *mut c_void, p: *mut ElemObj) {
    if out.is_null() {
        return;
    }
    if !p.is_null() {
        Com::add_ref(p);
    }
    *out = p as *mut c_void;
}

unsafe extern "win64" fn el_qi(
    this: *mut ElemObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_D2D1_RESOURCE, IID_SVG_ELEMENT])
}

unsafe extern "win64" fn el_document(this: *mut ElemObj, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    let d = est(this).doc;
    if !d.is_null() {
        Com::add_ref(d);
    }
    *out = d as *mut c_void;
}

unsafe extern "win64" fn el_tag(this: *mut ElemObj, buf: *mut u16, len: u32) -> i32 {
    let t = est(this).tag.clone();
    wide_out(&t, buf, len)
}

unsafe extern "win64" fn el_tag_len(this: *mut ElemObj) -> u32 {
    wlen(&est(this).tag)
}

unsafe extern "win64" fn el_is_text(this: *mut ElemObj) -> i32 {
    est(this).text.is_some() as i32
}

unsafe extern "win64" fn el_parent(this: *mut ElemObj, out: *mut *mut c_void) {
    let p = est(this).parent;
    put_held(out, p);
}

unsafe extern "win64" fn el_has_children(this: *mut ElemObj) -> i32 {
    !est(this).kids.is_empty() as i32
}

unsafe extern "win64" fn el_first(this: *mut ElemObj, out: *mut *mut c_void) {
    let k = est(this)
        .kids
        .first()
        .copied()
        .unwrap_or(std::ptr::null_mut());
    put_held(out, k);
}

unsafe extern "win64" fn el_last(this: *mut ElemObj, out: *mut *mut c_void) {
    let k = est(this)
        .kids
        .last()
        .copied()
        .unwrap_or(std::ptr::null_mut());
    put_held(out, k);
}

unsafe fn sibling(this: *mut ElemObj, r: *mut c_void, out: *mut *mut c_void, step: isize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let k = {
        let st = est(this);
        let Some(i) = st.kids.iter().position(|&k| k as *mut c_void == r) else {
            return E_INVALIDARG;
        };
        let j = i as isize + step;
        if j < 0 {
            None
        } else {
            st.kids.get(j as usize).copied()
        }
    };
    put_held(out, k.unwrap_or(std::ptr::null_mut()));
    S_OK
}

unsafe extern "win64" fn el_previous(
    this: *mut ElemObj,
    r: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    sibling(this, r, out, -1)
}

unsafe extern "win64" fn el_next(this: *mut ElemObj, r: *mut c_void, out: *mut *mut c_void) -> i32 {
    sibling(this, r, out, 1)
}

/// Détache `child` de son parent actuel (sans le relâcher : la référence
/// passe au nouveau parent).
unsafe fn detach(child: *mut ElemObj) -> bool {
    let p = est(child).parent;
    if p.is_null() {
        return false;
    }
    let mut ps = est(p);
    if let Some(i) = ps.kids.iter().position(|&k| k == child) {
        ps.kids.remove(i);
        drop(ps);
        est(child).parent = std::ptr::null_mut();
        return true;
    }
    false
}

/// Ancêtre ou soi-même ?
unsafe fn is_ancestor(a: *mut ElemObj, mut e: *mut ElemObj) -> bool {
    while !e.is_null() {
        if e == a {
            return true;
        }
        e = est(e).parent;
    }
    false
}

unsafe fn insert(this: *mut ElemObj, new: *mut c_void, before: *mut c_void) -> i32 {
    if !is_elem(new) {
        return E_INVALIDARG;
    }
    let n = new as *mut ElemObj;
    if is_ancestor(n, this) {
        return E_INVALIDARG;
    }
    if !before.is_null() && !est(this).kids.iter().any(|&k| k as *mut c_void == before) {
        return E_INVALIDARG;
    }
    if !detach(n) {
        Com::add_ref(n);
    }
    let doc = {
        let mut st = est(this);
        let at = st
            .kids
            .iter()
            .position(|&k| k as *mut c_void == before)
            .unwrap_or(st.kids.len());
        st.kids.insert(at, n);
        st.doc
    };
    est(n).parent = this;
    set_doc(n, doc);
    S_OK
}

unsafe extern "win64" fn el_insert_before(
    this: *mut ElemObj,
    new: *mut c_void,
    before: *mut c_void,
) -> i32 {
    insert(this, new, before)
}

unsafe extern "win64" fn el_append(this: *mut ElemObj, new: *mut c_void) -> i32 {
    insert(this, new, std::ptr::null_mut())
}

unsafe extern "win64" fn el_replace(this: *mut ElemObj, new: *mut c_void, old: *mut c_void) -> i32 {
    if !is_elem(new) || !is_elem(old) {
        return E_INVALIDARG;
    }
    if new == old {
        return S_OK;
    }
    let hr = insert(this, new, old);
    if hr != S_OK {
        return hr;
    }
    el_remove(this, old)
}

unsafe extern "win64" fn el_remove(this: *mut ElemObj, old: *mut c_void) -> i32 {
    let o = old as *mut ElemObj;
    let found = {
        let mut st = est(this);
        match st.kids.iter().position(|&k| k == o) {
            Some(i) => {
                st.kids.remove(i);
                true
            }
            None => false,
        }
    };
    if !found {
        return E_INVALIDARG;
    }
    est(o).parent = std::ptr::null_mut();
    set_doc(o, std::ptr::null_mut());
    Com::release(o);
    S_OK
}

/// CreateChild(tagName, newChild) : ajouté en dernier enfant.
unsafe extern "win64" fn el_create_child(
    this: *mut ElemObj,
    tag: *const u16,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(tag) = wide_in(tag).filter(|t| !t.is_empty()) else {
        return E_INVALIDARG;
    };
    let c = new_elem(Com::get(this).factory, &tag, None);
    let hr = insert(this, c as *mut c_void, std::ptr::null_mut());
    if hr != S_OK {
        Com::release(c);
        return hr;
    }
    // insert a pris sa propre référence ; celle de création revient à l'appelant.
    put(out, c)
}

unsafe extern "win64" fn el_is_specified(
    this: *mut ElemObj,
    name: *const u16,
    inherited: *mut i32,
) -> i32 {
    if !inherited.is_null() {
        *inherited = 0;
    }
    let Some(n) = wide_in(name) else { return 0 };
    est(this).attrs.iter().any(|(k, _)| *k == n) as i32
}

unsafe extern "win64" fn el_attr_count(this: *mut ElemObj) -> u32 {
    est(this).attrs.len() as u32
}

unsafe extern "win64" fn el_attr_name(
    this: *mut ElemObj,
    index: u32,
    buf: *mut u16,
    len: u32,
    inherited: *mut i32,
) -> i32 {
    if !inherited.is_null() {
        *inherited = 0;
    }
    let Some(n) = est(this).attrs.get(index as usize).map(|(k, _)| k.clone()) else {
        return E_INVALIDARG;
    };
    wide_out(&n, buf, len)
}

unsafe extern "win64" fn el_attr_name_len(
    this: *mut ElemObj,
    index: u32,
    len: *mut u32,
    inherited: *mut i32,
) -> i32 {
    if len.is_null() {
        return E_POINTER;
    }
    if !inherited.is_null() {
        *inherited = 0;
    }
    let Some(n) = est(this).attrs.get(index as usize).map(|(k, _)| wlen(k)) else {
        return E_INVALIDARG;
    };
    *len = n;
    S_OK
}

unsafe fn release_val(v: AttrVal) {
    if let AttrVal::Obj(a) = v {
        lock(&Com::get(a).data.st).elem = std::ptr::null_mut();
        Com::release(a);
    }
}

/// Remplace (ou ajoute) un attribut ; relâche l'ancienne valeur.
unsafe fn set_val(this: *mut ElemObj, name: String, v: AttrVal) {
    let old = {
        let mut st = est(this);
        match st.attrs.iter_mut().find(|(k, _)| *k == name) {
            Some(e) => Some(std::mem::replace(&mut e.1, v)),
            None => {
                st.attrs.push((name, v));
                None
            }
        }
    };
    if let Some(o) = old {
        release_val(o);
    }
}

unsafe extern "win64" fn el_remove_attr(this: *mut ElemObj, name: *const u16) -> i32 {
    let Some(n) = wide_in(name) else {
        return E_INVALIDARG;
    };
    let old = {
        let mut st = est(this);
        st.attrs
            .iter()
            .position(|(k, _)| *k == n)
            .map(|i| st.attrs.remove(i).1)
    };
    match old {
        Some(v) => {
            release_val(v);
            S_OK
        }
        None => E_INVALIDARG,
    }
}

/// SetTextValue : le contenu texte d'un nœud de texte, ou l'unique enfant
/// texte d'un élément.
unsafe extern "win64" fn el_set_text(this: *mut ElemObj, text: *const u16, len: u32) -> i32 {
    let t = wide_in_n(text, len);
    let old_kids = {
        let mut st = est(this);
        if st.text.is_some() {
            st.text = Some(t);
            return S_OK;
        }
        std::mem::take(&mut st.kids)
    };
    let mut keep = Vec::new();
    for k in old_kids {
        if est(k).text.is_some() {
            est(k).parent = std::ptr::null_mut();
            Com::release(k);
        } else {
            keep.push(k);
        }
    }
    let node = new_elem(Com::get(this).factory, "", Some(t));
    {
        let mut st = est(this);
        st.kids = keep;
        st.kids.insert(0, node);
    }
    let doc = est(this).doc;
    let mut ns = est(node);
    ns.parent = this;
    ns.doc = doc;
    S_OK
}

unsafe fn text_of(this: *mut ElemObj) -> String {
    let (text, kids) = {
        let st = est(this);
        (st.text.clone(), st.kids.clone())
    };
    if let Some(t) = text {
        return t;
    }
    kids.iter().filter_map(|&k| est(k).text.clone()).collect()
}

unsafe extern "win64" fn el_get_text(this: *mut ElemObj, buf: *mut u16, len: u32) -> i32 {
    wide_out(&text_of(this), buf, len)
}

unsafe extern "win64" fn el_text_len(this: *mut ElemObj) -> u32 {
    wlen(&text_of(this))
}

/// SetAttributeValue(name, ID2D1SvgAttribute) : l'objet est lié à l'élément.
unsafe extern "win64" fn el_set_attr_obj(
    this: *mut ElemObj,
    name: *const u16,
    v: *mut c_void,
) -> i32 {
    let Some(n) = wide_in(name) else {
        return E_INVALIDARG;
    };
    let Some(a) = attr_of(v) else {
        return E_INVALIDARG;
    };
    {
        let mut st = lock(&Com::get(a).data.st);
        if !st.elem.is_null() && st.elem != this {
            return E_INVALIDARG;
        }
        st.elem = this;
    }
    // Déjà lié sous ce nom : rien à faire.
    if est(this)
        .attrs
        .iter()
        .any(|(k, x)| *k == n && matches!(x, AttrVal::Obj(o) if *o == a))
    {
        return S_OK;
    }
    Com::add_ref(a);
    set_val(this, n, AttrVal::Obj(a));
    S_OK
}

unsafe extern "win64" fn el_set_attr_pod(
    this: *mut ElemObj,
    name: *const u16,
    ty: u32,
    value: *const u8,
    size: u32,
) -> i32 {
    let Some(n) = wide_in(name) else {
        return E_INVALIDARG;
    };
    if value.is_null() || pod_size(ty) != Some(size as usize) {
        return E_INVALIDARG;
    }
    let Some(s) = pod_to_string(ty, slice(value, size)) else {
        return E_INVALIDARG;
    };
    set_val(this, n, AttrVal::Str(s));
    S_OK
}

unsafe extern "win64" fn el_set_attr_str(
    this: *mut ElemObj,
    name: *const u16,
    ty: u32,
    value: *const u16,
) -> i32 {
    let (Some(n), Some(v)) = (wide_in(name), wide_in(value)) else {
        return E_INVALIDARG;
    };
    if ty > 1 {
        return E_INVALIDARG;
    }
    set_val(this, n, AttrVal::Str(v));
    S_OK
}

/// GetAttributeValue(name, riid, value) : l'objet d'attribut vivant ; un
/// attribut gardé en texte est converti et devient cet objet.
unsafe extern "win64" fn el_get_attr_obj(
    this: *mut ElemObj,
    name: *const u16,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let (Some(n), Some(iid)) = (wide_in(name), riid.as_ref()) else {
        return E_INVALIDARG;
    };
    let cur = {
        let st = est(this);
        match st.attrs.iter().find(|(k, _)| *k == n) {
            None => return E_INVALIDARG,
            Some((_, AttrVal::Obj(a))) => Ok(*a),
            Some((_, AttrVal::Str(s))) => Err(s.clone()),
        }
    };
    let a = match cur {
        Ok(a) => a,
        Err(s) => {
            let Some(data) = attr_from_string(iid, &s) else {
                return E_INVALIDARG;
            };
            // La référence de création est celle que tient l'élément.
            let a = new_attr(Com::get(this).factory, data);
            lock(&Com::get(a).data.st).elem = this;
            set_val(this, n, AttrVal::Obj(a));
            a
        }
    };
    let kind = attr_iid(&lock(&Com::get(a).data.st).data);
    if *iid != kind
        && *iid != IID_SVG_ATTRIBUTE
        && *iid != IID_D2D1_RESOURCE
        && *iid != IID_IUNKNOWN
    {
        return E_NOINTERFACE;
    }
    // `a` est tenu par l'élément ; on rend une référence à l'appelant.
    Com::add_ref(a);
    *out = a as *mut c_void;
    S_OK
}

unsafe fn attr_text_of(this: *mut ElemObj, name: *const u16) -> Option<String> {
    let n = wide_in(name)?;
    let st = est(this);
    st.attrs
        .iter()
        .find(|(k, _)| *k == n)
        .map(|(_, v)| attr_text(v))
}

unsafe extern "win64" fn el_get_attr_pod(
    this: *mut ElemObj,
    name: *const u16,
    ty: u32,
    value: *mut u8,
    size: u32,
) -> i32 {
    if value.is_null() {
        return E_POINTER;
    }
    if pod_size(ty) != Some(size as usize) {
        return E_INVALIDARG;
    }
    let Some(s) = attr_text_of(this, name) else {
        return E_INVALIDARG;
    };
    let Some(b) = string_to_pod(ty, &s) else {
        return E_INVALIDARG;
    };
    std::ptr::copy_nonoverlapping(b.as_ptr(), value, size as usize);
    S_OK
}

unsafe extern "win64" fn el_get_attr_str(
    this: *mut ElemObj,
    name: *const u16,
    ty: u32,
    buf: *mut u16,
    len: u32,
) -> i32 {
    if ty > 1 {
        return E_INVALIDARG;
    }
    let Some(s) = attr_text_of(this, name) else {
        return E_INVALIDARG;
    };
    wide_out(&s, buf, len)
}

unsafe extern "win64" fn el_get_attr_len(
    this: *mut ElemObj,
    name: *const u16,
    ty: u32,
    len: *mut u32,
) -> i32 {
    if len.is_null() {
        return E_POINTER;
    }
    if ty > 1 {
        return E_INVALIDARG;
    }
    let Some(s) = attr_text_of(this, name) else {
        return E_INVALIDARG;
    };
    *len = wlen(&s);
    S_OK
}

pub(super) static ELEM_VT: VTable<34> = VTable([
    el_qi as *const (),
    super::add_ref::<Res<Elem>> as *const (),
    super::release::<Res<Elem>> as *const (),
    get_factory::<Elem> as *const (),
    el_document as *const (),
    el_tag as *const (),
    el_tag_len as *const (),
    el_is_text as *const (),
    el_parent as *const (),
    el_has_children as *const (),
    el_first as *const (),
    el_last as *const (),
    el_previous as *const (),
    el_next as *const (),
    el_insert_before as *const (),
    el_append as *const (),
    el_replace as *const (),
    el_remove as *const (),
    el_create_child as *const (),
    el_is_specified as *const (),
    el_attr_count as *const (),
    el_attr_name as *const (),
    el_attr_name_len as *const (),
    el_remove_attr as *const (),
    el_set_text as *const (),
    el_get_text as *const (),
    el_text_len as *const (),
    el_set_attr_obj as *const (),
    el_set_attr_pod as *const (),
    el_set_attr_str as *const (),
    el_get_attr_obj as *const (),
    el_get_attr_pod as *const (),
    el_get_attr_str as *const (),
    el_get_attr_len as *const (),
]);

// ---------------------------------------------------------------------
// Objets d'attribut
// ---------------------------------------------------------------------

unsafe fn ast<'a>(p: *mut AttrObj) -> std::sync::MutexGuard<'a, AttrSt> {
    lock(&Com::get(p).data.st)
}

unsafe extern "win64" fn attr_qi(
    this: *mut AttrObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    let own = attr_iid(&ast(this).data);
    qi_list(
        this,
        riid,
        out,
        &[IID_D2D1_RESOURCE, IID_SVG_ATTRIBUTE, own],
    )
}

unsafe extern "win64" fn attr_element(this: *mut AttrObj, out: *mut *mut c_void) {
    let e = ast(this).elem;
    put_held(out, e);
}

/// Clone : copie non liée.
unsafe extern "win64" fn attr_clone(this: *mut AttrObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let data = ast(this).data.clone();
    put(out, new_attr(Com::get(this).factory, data))
}

macro_rules! attr_base {
    () => {
        [
            attr_qi as *const (),
            super::add_ref::<Res<Attr>> as *const (),
            super::release::<Res<Attr>> as *const (),
            get_factory::<Attr> as *const (),
            attr_element as *const (),
            attr_clone as *const (),
        ]
    };
}

const fn with_base<const N: usize>(base: [*const (); 6], extra: &[*const ()]) -> [*const (); N] {
    assert!(6 + extra.len() == N, "taille de table");
    let mut out = [std::ptr::null::<()>(); N];
    let mut i = 0;
    while i < N {
        out[i] = if i < 6 { base[i] } else { extra[i - 6] };
        i += 1;
    }
    out
}

// ID2D1SvgPaint -------------------------------------------------------

unsafe fn paint<R>(
    this: *mut AttrObj,
    f: impl FnOnce(&mut u32, &mut [f32; 4], &mut String) -> R,
) -> Option<R> {
    match &mut ast(this).data {
        AttrData::Paint { kind, color, id } => Some(f(kind, color, id)),
        _ => None,
    }
}

unsafe extern "win64" fn paint_set_type(this: *mut AttrObj, t: u32) -> i32 {
    if t > 6 {
        return E_INVALIDARG;
    }
    paint(this, |k, _, _| *k = t).map_or(E_INVALIDARG, |_| S_OK)
}

unsafe extern "win64" fn paint_get_type(this: *mut AttrObj) -> u32 {
    paint(this, |k, _, _| *k).unwrap_or(0)
}

/// SetColor : le type devient COLOR (ou URI_COLOR s'il portait une URI).
unsafe extern "win64" fn paint_set_color(this: *mut AttrObj, c: *const [f32; 4]) -> i32 {
    let Some(c) = c.as_ref().copied() else {
        return E_POINTER;
    };
    paint(this, |k, col, _| {
        *col = c;
        *k = if (3..=6).contains(k) { 5 } else { 1 };
    })
    .map_or(E_INVALIDARG, |_| S_OK)
}

unsafe extern "win64" fn paint_get_color(this: *mut AttrObj, out: *mut [f32; 4]) {
    if let Some(c) = paint(this, |_, c, _| *c) {
        if !out.is_null() {
            *out = c;
        }
    }
}

unsafe extern "win64" fn paint_set_id(this: *mut AttrObj, id: *const u16) -> i32 {
    let Some(s) = wide_in(id) else {
        return E_INVALIDARG;
    };
    paint(this, |_, _, i| *i = s).map_or(E_INVALIDARG, |_| S_OK)
}

unsafe extern "win64" fn paint_get_id(this: *mut AttrObj, buf: *mut u16, len: u32) -> i32 {
    let id = paint(this, |_, _, i| i.clone()).unwrap_or_default();
    wide_out(&id, buf, len)
}

unsafe extern "win64" fn paint_id_len(this: *mut AttrObj) -> u32 {
    paint(this, |_, _, i| wlen(i)).unwrap_or(0)
}

static PAINT_VT: VTable<13> = VTable(with_base::<13>(
    attr_base!(),
    &[
        paint_set_type as *const (),
        paint_get_type as *const (),
        paint_set_color as *const (),
        paint_get_color as *const (),
        paint_set_id as *const (),
        paint_get_id as *const (),
        paint_id_len as *const (),
    ],
));

// Tableaux (dashes, points, segments, commandes) ------------------------

/// Remplace à partir de `start` (au plus à la fin), en étendant au besoin.
fn update<T: Copy>(v: &mut Vec<T>, items: &[T], start: u32) -> i32 {
    let s = start as usize;
    if s > v.len() {
        return E_INVALIDARG;
    }
    for (k, x) in items.iter().enumerate() {
        if s + k < v.len() {
            v[s + k] = *x;
        } else {
            v.push(*x);
        }
    }
    S_OK
}

/// Copie `count` éléments à partir de `start`.
unsafe fn copy_out<T: Copy>(v: &[T], out: *mut T, count: u32, start: u32) -> i32 {
    if out.is_null() && count > 0 {
        return E_POINTER;
    }
    let s = start as usize;
    if s + count as usize > v.len() {
        return E_INVALIDARG;
    }
    std::ptr::copy_nonoverlapping(v[s..].as_ptr(), out, count as usize);
    S_OK
}

fn remove_end<T>(v: &mut Vec<T>, n: u32) -> i32 {
    if n as usize > v.len() {
        return E_INVALIDARG;
    }
    v.truncate(v.len() - n as usize);
    S_OK
}

unsafe fn dashes<R>(this: *mut AttrObj, f: impl FnOnce(&mut Vec<(f32, u32)>) -> R) -> Option<R> {
    match &mut ast(this).data {
        AttrData::Dashes(v) => Some(f(v)),
        _ => None,
    }
}

unsafe extern "win64" fn dash_remove_end(this: *mut AttrObj, n: u32) -> i32 {
    dashes(this, |v| remove_end(v, n)).unwrap_or(E_INVALIDARG)
}

unsafe extern "win64" fn dash_update(
    this: *mut AttrObj,
    d: *const [u32; 2],
    count: u32,
    start: u32,
) -> i32 {
    if d.is_null() && count > 0 {
        return E_POINTER;
    }
    let items: Vec<(f32, u32)> = slice(d, count)
        .iter()
        .map(|x| (f32::from_bits(x[0]), x[1]))
        .collect();
    dashes(this, |v| update(v, &items, start)).unwrap_or(E_INVALIDARG)
}

unsafe extern "win64" fn dash_update_f(
    this: *mut AttrObj,
    d: *const f32,
    count: u32,
    start: u32,
) -> i32 {
    if d.is_null() && count > 0 {
        return E_POINTER;
    }
    let items: Vec<(f32, u32)> = slice(d, count).iter().map(|x| (*x, 0)).collect();
    dashes(this, |v| update(v, &items, start)).unwrap_or(E_INVALIDARG)
}

unsafe extern "win64" fn dash_get(
    this: *mut AttrObj,
    out: *mut [u32; 2],
    count: u32,
    start: u32,
) -> i32 {
    let v: Vec<[u32; 2]> =
        dashes(this, |v| v.iter().map(|(x, u)| [x.to_bits(), *u]).collect()).unwrap_or_default();
    copy_out(&v, out, count, start)
}

unsafe extern "win64" fn dash_get_f(
    this: *mut AttrObj,
    out: *mut f32,
    count: u32,
    start: u32,
) -> i32 {
    let v: Vec<f32> = dashes(this, |v| v.iter().map(|(x, _)| *x).collect()).unwrap_or_default();
    copy_out(&v, out, count, start)
}

unsafe extern "win64" fn dash_count(this: *mut AttrObj) -> u32 {
    dashes(this, |v| v.len() as u32).unwrap_or(0)
}

static DASHES_VT: VTable<12> = VTable(with_base::<12>(
    attr_base!(),
    &[
        dash_remove_end as *const (),
        dash_update as *const (),
        dash_update_f as *const (),
        dash_get as *const (),
        dash_get_f as *const (),
        dash_count as *const (),
    ],
));

unsafe fn points<R>(this: *mut AttrObj, f: impl FnOnce(&mut Vec<(f32, f32)>) -> R) -> Option<R> {
    match &mut ast(this).data {
        AttrData::Points(v) => Some(f(v)),
        _ => None,
    }
}

unsafe extern "win64" fn pts_remove_end(this: *mut AttrObj, n: u32) -> i32 {
    points(this, |v| remove_end(v, n)).unwrap_or(E_INVALIDARG)
}

unsafe extern "win64" fn pts_update(
    this: *mut AttrObj,
    p: *const (f32, f32),
    count: u32,
    start: u32,
) -> i32 {
    if p.is_null() && count > 0 {
        return E_POINTER;
    }
    let items = slice(p, count).to_vec();
    points(this, |v| update(v, &items, start)).unwrap_or(E_INVALIDARG)
}

unsafe extern "win64" fn pts_get(
    this: *mut AttrObj,
    out: *mut (f32, f32),
    count: u32,
    start: u32,
) -> i32 {
    let v = points(this, |v| v.clone()).unwrap_or_default();
    copy_out(&v, out, count, start)
}

unsafe extern "win64" fn pts_count(this: *mut AttrObj) -> u32 {
    points(this, |v| v.len() as u32).unwrap_or(0)
}

static POINTS_VT: VTable<10> = VTable(with_base::<10>(
    attr_base!(),
    &[
        pts_remove_end as *const (),
        pts_update as *const (),
        pts_get as *const (),
        pts_count as *const (),
    ],
));

unsafe fn path<R>(
    this: *mut AttrObj,
    f: impl FnOnce(&mut Vec<f32>, &mut Vec<u32>) -> R,
) -> Option<R> {
    match &mut ast(this).data {
        AttrData::Path { data, commands } => Some(f(data, commands)),
        _ => None,
    }
}

unsafe extern "win64" fn path_remove_data(this: *mut AttrObj, n: u32) -> i32 {
    path(this, |d, _| remove_end(d, n)).unwrap_or(E_INVALIDARG)
}

unsafe extern "win64" fn path_update_data(
    this: *mut AttrObj,
    p: *const f32,
    count: u32,
    start: u32,
) -> i32 {
    if p.is_null() && count > 0 {
        return E_POINTER;
    }
    let items = slice(p, count).to_vec();
    path(this, |d, _| update(d, &items, start)).unwrap_or(E_INVALIDARG)
}

unsafe extern "win64" fn path_get_data(
    this: *mut AttrObj,
    out: *mut f32,
    count: u32,
    start: u32,
) -> i32 {
    let v = path(this, |d, _| d.clone()).unwrap_or_default();
    copy_out(&v, out, count, start)
}

unsafe extern "win64" fn path_data_count(this: *mut AttrObj) -> u32 {
    path(this, |d, _| d.len() as u32).unwrap_or(0)
}

unsafe extern "win64" fn path_remove_cmds(this: *mut AttrObj, n: u32) -> i32 {
    path(this, |_, c| remove_end(c, n)).unwrap_or(E_INVALIDARG)
}

unsafe extern "win64" fn path_update_cmds(
    this: *mut AttrObj,
    p: *const u32,
    count: u32,
    start: u32,
) -> i32 {
    if p.is_null() && count > 0 {
        return E_POINTER;
    }
    let items = slice(p, count).to_vec();
    if items.iter().any(|c| arity(*c).is_none()) {
        return E_INVALIDARG;
    }
    path(this, |_, c| update(c, &items, start)).unwrap_or(E_INVALIDARG)
}

unsafe extern "win64" fn path_get_cmds(
    this: *mut AttrObj,
    out: *mut u32,
    count: u32,
    start: u32,
) -> i32 {
    let v = path(this, |_, c| c.clone()).unwrap_or_default();
    copy_out(&v, out, count, start)
}

unsafe extern "win64" fn path_cmd_count(this: *mut AttrObj) -> u32 {
    path(this, |_, c| c.len() as u32).unwrap_or(0)
}

/// CreatePathGeometry(fillMode, pathGeometry) : la géométrie du chemin, par
/// le même analyseur que le rendu SVG.
unsafe extern "win64" fn path_geometry(
    this: *mut AttrObj,
    mode: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(d) = path(this, |d, c| path_string(d, c)) else {
        return E_INVALIDARG;
    };
    let mut p = Path::default();
    for seg in svgfmt::parse_path(&d) {
        match seg {
            svgfmt::Seg::Move(pt) => p.begin(pt),
            svgfmt::Seg::Line(pt) => p.line(pt),
            svgfmt::Seg::Cubic(a, b, c) => p.cubic(a, b, c, 1.0),
            svgfmt::Seg::Close => p.close(),
        }
    }
    let hr = new_geometry(Com::get(this).factory, GeomKind::Path, p, out);
    if hr == S_OK {
        let g = &Com::get(*out as *mut Com<Res<Geometry>>).data;
        *crate::dlls::lock(&g.rule) = if mode == 1 {
            FillRule::NonZero
        } else {
            FillRule::EvenOdd
        };
    }
    hr
}

static PATH_VT: VTable<15> = VTable(with_base::<15>(
    attr_base!(),
    &[
        path_remove_data as *const (),
        path_update_data as *const (),
        path_get_data as *const (),
        path_data_count as *const (),
        path_remove_cmds as *const (),
        path_update_cmds as *const (),
        path_get_cmds as *const (),
        path_cmd_count as *const (),
        path_geometry as *const (),
    ],
));

#[cfg(test)]
mod tests {
    use super::*;

    /// IStream minimal : Read (3) et Write (4) seulement.
    #[repr(C)]
    struct FakeStream {
        vt: *const (),
        data: Vec<u8>,
        pos: usize,
    }

    unsafe extern "win64" fn fs_read(
        this: *mut FakeStream,
        buf: *mut u8,
        n: u32,
        got: *mut u32,
    ) -> i32 {
        let s = &mut *this;
        let k = (n as usize).min(s.data.len() - s.pos);
        std::ptr::copy_nonoverlapping(s.data[s.pos..].as_ptr(), buf, k);
        s.pos += k;
        *got = k as u32;
        if k < n as usize {
            S_FALSE
        } else {
            S_OK
        }
    }

    unsafe extern "win64" fn fs_write(
        this: *mut FakeStream,
        buf: *const u8,
        n: u32,
        done: *mut u32,
    ) -> i32 {
        let s = &mut *this;
        s.data
            .extend_from_slice(std::slice::from_raw_parts(buf, n as usize));
        *done = n;
        S_OK
    }

    static FS_VT: VTable<5> = VTable([
        not_implemented as *const (),
        not_implemented as *const (),
        not_implemented as *const (),
        fs_read as *const (),
        fs_write as *const (),
    ]);

    fn stream(text: &str) -> FakeStream {
        FakeStream {
            vt: FS_VT.ptr(),
            data: text.as_bytes().to_vec(),
            pos: 0,
        }
    }

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn tag_of(e: *mut c_void) -> String {
        let mut b = [0u16; 32];
        assert_eq!(el_tag(e.cast(), b.as_mut_ptr(), 32), S_OK);
        String::from_utf16_lossy(&b[..b.iter().position(|&c| c == 0).unwrap()])
    }

    unsafe fn rel(p: *mut c_void) {
        type Rel = unsafe extern "win64" fn(*mut c_void) -> u32;
        method::<Rel>(p, 2)(p);
    }

    /// #2719 : Teams (Win2D CanvasSvgDocument) appelle
    /// ID2D1DeviceContext5::CreateSvgDocument puis DrawSvgDocument. Le
    /// document lu d'un flux se parcourt, ses attributs se lisent en texte et
    /// en objets vivants (peinture, données de chemin, géométrie), il se
    /// sérialise et se dessine dans la cible.
    #[test]
    fn document_svg_2719() {
        unsafe {
            let f = Com::create(super::super::FACTORY_VT.ptr(), super::super::Factory);
            let fake = Com::create(super::super::FACTORY_VT.ptr(), super::super::Factory);
            let mut dev = std::ptr::null_mut();
            assert_eq!(
                super::super::v11::factory_create_device(f, fake.cast(), &mut dev),
                S_OK
            );
            Com::release(fake);
            type CreateDc = unsafe extern "win64" fn(*mut c_void, u32, *mut *mut c_void) -> i32;
            let mut dc = std::ptr::null_mut();
            assert_eq!(method::<CreateDc>(dev, 4)(dev, 0, &mut dc), S_OK);
            type CreateBmp = unsafe extern "win64" fn(
                *mut c_void,
                u64,
                *const c_void,
                u32,
                *const c_void,
                *mut *mut c_void,
            ) -> i32;
            let size = 8u64 | (8u64 << 32);
            let mut bmp = std::ptr::null_mut();
            assert_eq!(
                method::<CreateBmp>(dc, 57)(
                    dc,
                    size,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &mut bmp
                ),
                S_OK
            );
            type SetTarget = unsafe extern "win64" fn(*mut c_void, *mut c_void);
            method::<SetTarget>(dc, 74)(dc, bmp);
            let dc_t: *mut TargetObj = dc.cast();

            // Flux illisible : E_INVALIDARG, rien en sortie.
            let mut bad = stream("pas du xml");
            let mut doc = 1 as *mut c_void;
            let vp = 8f32.to_bits() as u64 | ((8f32.to_bits() as u64) << 32);
            assert_eq!(
                create_svg_document(dc_t, (&mut bad as *mut FakeStream).cast(), vp, &mut doc),
                E_INVALIDARG
            );
            assert!(doc.is_null());

            // Flux nul : document vide, racine « svg ».
            assert_eq!(
                create_svg_document(dc_t, std::ptr::null_mut(), vp, &mut doc),
                S_OK
            );
            let mut root = std::ptr::null_mut();
            doc_get_root(doc.cast(), &mut root);
            assert_eq!(tag_of(root), "svg");
            rel(root);
            rel(doc);

            let mut src = stream(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\">\
                 <rect id=\"r\" x=\"0\" y=\"0\" width=\"8\" height=\"8\" fill=\"#00ff00\"/>\
                 <path id=\"p\" d=\"M0 0 L4 0 L4 4 Z\" fill=\"none\"/></svg>",
            );
            assert_eq!(
                create_svg_document(dc_t, (&mut src as *mut FakeStream).cast(), vp, &mut doc),
                S_OK
            );
            let mut iface = std::ptr::null_mut();
            assert_eq!(doc_qi(doc.cast(), &IID_SVG_DOCUMENT, &mut iface), S_OK);
            rel(iface);
            doc_get_root(doc.cast(), &mut root);
            let mut rect = std::ptr::null_mut();
            el_first(root.cast(), &mut rect);
            assert_eq!(tag_of(rect), "rect");
            let mut p = std::ptr::null_mut();
            assert_eq!(doc_find(doc.cast(), w("p").as_ptr(), &mut p), S_OK);
            assert_eq!(tag_of(p), "path");
            let mut next = std::ptr::null_mut();
            assert_eq!(el_next(root.cast(), rect, &mut next), S_OK);
            assert_eq!(next, p);
            rel(next);
            assert_eq!(el_attr_count(rect.cast()), 6);

            // Attribut en texte, puis en float.
            let mut buf = [0u16; 16];
            assert_eq!(
                el_get_attr_str(rect.cast(), w("id").as_ptr(), 1, buf.as_mut_ptr(), 16),
                S_OK
            );
            assert_eq!(buf[0], 'r' as u16);
            let mut wd = 0f32;
            assert_eq!(
                el_get_attr_pod(
                    rect.cast(),
                    w("width").as_ptr(),
                    0,
                    (&mut wd as *mut f32).cast(),
                    4
                ),
                S_OK
            );
            assert_eq!(wd, 8.0);

            // Peinture vivante : la changer change le dessin.
            let mut paint = std::ptr::null_mut();
            assert_eq!(
                el_get_attr_obj(rect.cast(), w("fill").as_ptr(), &IID_SVG_PAINT, &mut paint),
                S_OK
            );
            assert_eq!(paint_get_type(paint.cast()), 1);
            let mut c = [0f32; 4];
            paint_get_color(paint.cast(), &mut c);
            assert_eq!(c, [0.0, 1.0, 0.0, 1.0]);
            assert_eq!(paint_set_color(paint.cast(), &[0.0, 0.0, 1.0, 1.0]), S_OK);

            // Données de chemin et géométrie.
            let mut pd = std::ptr::null_mut();
            assert_eq!(
                el_get_attr_obj(p.cast(), w("d").as_ptr(), &IID_SVG_PATH_DATA, &mut pd),
                S_OK
            );
            assert_eq!(path_cmd_count(pd.cast()), 4);
            assert_eq!(path_data_count(pd.cast()), 6);
            let mut geo = std::ptr::null_mut();
            assert_eq!(path_geometry(pd.cast(), 0, &mut geo), S_OK);
            let mut r = RectF {
                l: 0.0,
                t: 0.0,
                r: 0.0,
                b: 0.0,
            };
            assert_eq!(geometry_bounds(geo.cast(), std::ptr::null(), &mut r), S_OK);
            assert_eq!((r.l, r.t, r.r, r.b), (0.0, 0.0, 4.0, 4.0));
            rel(geo);
            rel(pd);
            // Objet déjà vivant : redonné tel quel ; mauvaise interface
            // refusée sans toucher à la référence de l'élément.
            let mut again = std::ptr::null_mut();
            assert_eq!(
                el_get_attr_obj(
                    rect.cast(),
                    w("fill").as_ptr(),
                    &IID_SVG_PATH_DATA,
                    &mut again
                ),
                E_NOINTERFACE
            );
            assert!(again.is_null());
            assert_eq!(
                el_get_attr_obj(rect.cast(), w("fill").as_ptr(), &IID_SVG_PAINT, &mut again),
                S_OK
            );
            assert_eq!(again, paint);
            rel(again);
            rel(paint);

            // Nouvel enfant, puis sérialisation.
            let mut circle = std::ptr::null_mut();
            assert_eq!(
                el_create_child(root.cast(), w("circle").as_ptr(), &mut circle),
                S_OK
            );
            assert_eq!(tag_of(circle), "circle");
            assert_eq!(
                el_set_attr_str(circle.cast(), w("r").as_ptr(), 0, w("1").as_ptr()),
                S_OK
            );
            let mut out = stream("");
            assert_eq!(
                doc_serialize(
                    doc.cast(),
                    (&mut out as *mut FakeStream).cast(),
                    std::ptr::null_mut()
                ),
                S_OK
            );
            let xml = String::from_utf8(out.data.clone()).unwrap();
            assert!(xml.contains("<rect"), "{xml}");
            assert!(xml.contains("id=\"p\""), "{xml}");
            assert!(xml.contains("<circle r=\"1\""), "{xml}");
            rel(circle);

            // Dessin : le rectangle couvre la cible en bleu (BGRA), le
            // cercle sans couleur est noir.
            super::super::begin_draw(dc_t);
            draw_svg_document(dc_t, doc);
            assert_eq!(
                super::super::end_draw(dc_t, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            {
                let b = &Com::get(bmp as *mut BitmapObj).data;
                let px = crate::dlls::lock(&b.px);
                // (6, 6) : loin du cercle noir ajouté à l'origine.
                let i = (6 * 8 + 6) * 4;
                assert_eq!(&px[i..i + 4], &[255, 0, 0, 255]);
                assert!(px[2] < 8 && px[0] < 255, "cercle noir à l'origine");
            }

            rel(p);
            rel(rect);
            rel(root);
            rel(doc);
            rel(bmp);
            rel(dc);
            rel(dev);
            Com::release(f);
        }
    }
}
