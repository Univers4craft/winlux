//! Aperçu des modèles 3D (STL, OBJ, PLY, OFF, glTF/GLB, FBX binaire) :
//! pas d'édition ni de manipulation de scène, seulement une vignette et
//! un aperçu qui tourne sur lui-même (rastérisation logicielle).

use super::raster::Canvas;
use super::{Error, Image, Page};

#[derive(Clone, Debug)]
pub struct Mesh {
    /// Triangles normalisés (centrés, rayon 1).
    pub tris: Vec<[f32; 9]>,
}

pub fn is_model_ext(ext: &str) -> bool {
    matches!(ext, "stl" | "obj" | "glb" | "gltf" | "fbx" | "ply" | "off")
}

pub fn decode(d: &[u8], ext: &str) -> Result<Image, Error> {
    let (name, tris) = if d.starts_with(b"glTF") {
        ("glTF", glb(d)?)
    } else if d.starts_with(b"Kaydara FBX Binary") {
        ("FBX", fbx(d)?)
    } else {
        match ext {
            "stl" => ("STL", stl(d)?),
            "obj" => ("OBJ", obj(d)?),
            "ply" => ("PLY", ply(d)?),
            "off" => ("OFF", off(d)?),
            "gltf" => ("glTF", gltf_json(d, &[])?),
            "fbx" => return Err(Error::Variant("FBX ASCII")),
            _ => return Err(Error::Corrupt),
        }
    };
    if tris.is_empty() {
        return Err(Error::Corrupt);
    }
    let mut img = Image::new(name);
    img.meta
        .push(("model.triangles".into(), tris.len().to_string()));
    img.pages.push(Page {
        name: String::new(),
        content: super::Content::Model(Mesh {
            tris: normalize(tris),
        }),
        delay_ms: 0,
    });
    img.animated = true;
    Ok(img)
}

fn normalize(mut t: Vec<[f32; 9]>) -> Vec<[f32; 9]> {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for tri in &t {
        for k in 0..9 {
            if tri[k].is_finite() {
                lo[k % 3] = lo[k % 3].min(tri[k]);
                hi[k % 3] = hi[k % 3].max(tri[k]);
            }
        }
    }
    let c = [
        (lo[0] + hi[0]) / 2.0,
        (lo[1] + hi[1]) / 2.0,
        (lo[2] + hi[2]) / 2.0,
    ];
    let mut r = 0f32;
    for tri in &t {
        for v in tri.as_chunks::<3>().0.iter() {
            let d = (v[0] - c[0]).powi(2) + (v[1] - c[1]).powi(2) + (v[2] - c[2]).powi(2);
            if d.is_finite() {
                r = r.max(d);
            }
        }
    }
    let s = if r > 0.0 { 1.0 / r.sqrt() } else { 1.0 };
    t.retain(|tri| tri.iter().all(|v| v.is_finite()));
    for tri in t.iter_mut() {
        for k in 0..9 {
            tri[k] = (tri[k] - c[k % 3]) * s;
        }
    }
    t
}

fn f32le(d: &[u8], p: usize) -> f32 {
    f32::from_bits(super::le32(d, p))
}

fn stl(d: &[u8]) -> Result<Vec<[f32; 9]>, Error> {
    if d.len() >= 84 {
        let n = super::le32(d, 80) as usize;
        if 84 + n * 50 == d.len() || (!d.starts_with(b"solid") && 84 + n * 50 <= d.len()) {
            let mut t = Vec::with_capacity(n);
            for i in 0..n {
                let o = 84 + i * 50 + 12;
                let mut tri = [0f32; 9];
                for (k, v) in tri.iter_mut().enumerate() {
                    *v = f32le(d, o + k * 4);
                }
                t.push(tri);
            }
            return Ok(t);
        }
    }
    let s = String::from_utf8_lossy(d);
    let mut verts = Vec::new();
    for l in s.lines() {
        let l = l.trim();
        if let Some(r) = l.strip_prefix("vertex") {
            let v: Vec<f32> = r
                .split_whitespace()
                .filter_map(|x| x.parse().ok())
                .collect();
            if v.len() >= 3 {
                verts.extend_from_slice(&v[..3]);
            }
        }
    }
    Ok(verts.as_chunks::<9>().0.to_vec())
}

fn fan(out: &mut Vec<[f32; 9]>, v: &[[f32; 3]], idx: &[usize]) {
    for k in 1..idx.len().saturating_sub(1) {
        let (Some(a), Some(b), Some(c)) = (v.get(idx[0]), v.get(idx[k]), v.get(idx[k + 1])) else {
            continue;
        };
        out.push([a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]]);
    }
}

fn obj(d: &[u8]) -> Result<Vec<[f32; 9]>, Error> {
    let s = String::from_utf8_lossy(d);
    let mut v: Vec<[f32; 3]> = Vec::new();
    let mut out = Vec::new();
    for l in s.lines() {
        let mut it = l.split_whitespace();
        match it.next() {
            Some("v") => {
                let p: Vec<f32> = it.take(3).filter_map(|x| x.parse().ok()).collect();
                if p.len() == 3 {
                    v.push([p[0], p[1], p[2]]);
                }
            }
            Some("f") => {
                let idx: Vec<usize> = it
                    .filter_map(|t| {
                        let i: i64 = t.split('/').next()?.parse().ok()?;
                        Some(if i < 0 {
                            (v.len() as i64 + i) as usize
                        } else {
                            (i - 1) as usize
                        })
                    })
                    .collect();
                fan(&mut out, &v, &idx);
            }
            _ => {}
        }
        if out.len() > 20_000_000 {
            break;
        }
    }
    Ok(out)
}

fn off(d: &[u8]) -> Result<Vec<[f32; 9]>, Error> {
    let s = String::from_utf8_lossy(d);
    let mut toks = s
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .flat_map(|l| l.split_whitespace());
    let first = toks.next().ok_or(Error::Corrupt)?;
    let nv: usize = if first.ends_with("OFF") {
        toks.next()
    } else {
        Some(first)
    }
    .and_then(|t| t.parse().ok())
    .ok_or(Error::Corrupt)?;
    let nf: usize = toks
        .next()
        .and_then(|t| t.parse().ok())
        .ok_or(Error::Corrupt)?;
    let _ = toks.next();
    let mut v = Vec::with_capacity(nv.min(1 << 24));
    for _ in 0..nv {
        let p: Vec<f32> = (0..3).filter_map(|_| toks.next()?.parse().ok()).collect();
        v.push([
            *p.first().unwrap_or(&0.0),
            *p.get(1).unwrap_or(&0.0),
            *p.get(2).unwrap_or(&0.0),
        ]);
    }
    let mut out = Vec::new();
    for _ in 0..nf {
        let n: usize = toks.next().and_then(|t| t.parse().ok()).unwrap_or(0);
        let idx: Vec<usize> = (0..n).filter_map(|_| toks.next()?.parse().ok()).collect();
        fan(&mut out, &v, &idx);
    }
    Ok(out)
}

fn ply(d: &[u8]) -> Result<Vec<[f32; 9]>, Error> {
    let end = d
        .windows(10)
        .position(|w| w == b"end_header")
        .ok_or(Error::Corrupt)?;
    let head = String::from_utf8_lossy(&d[..end]);
    let mut body = end + 10;
    while body < d.len() && (d[body] == b'\r' || d[body] == b'\n') {
        body += 1;
        if d[body - 1] == b'\n' {
            break;
        }
    }
    let mut format = "";
    let mut nv = 0usize;
    let mut nf = 0usize;
    let mut vprops: Vec<(String, String)> = Vec::new();
    let mut face_types = ("uchar".to_string(), "int".to_string());
    let mut cur = "";
    for l in head.lines() {
        let p: Vec<&str> = l.split_whitespace().collect();
        match p.as_slice() {
            ["format", f, ..] => {
                format = if f.starts_with("ascii") {
                    "a"
                } else if f.contains("little") {
                    "l"
                } else {
                    "b"
                }
            }
            ["element", "vertex", n] => {
                nv = n.parse().unwrap_or(0);
                cur = "v";
            }
            ["element", "face", n] => {
                nf = n.parse().unwrap_or(0);
                cur = "f";
            }
            ["element", ..] => cur = "",
            ["property", "list", a, b, _] if cur == "f" => {
                face_types = (a.to_string(), b.to_string())
            }
            ["property", t, name] if cur == "v" => vprops.push((t.to_string(), name.to_string())),
            _ => {}
        }
    }
    let size = |t: &str| match t {
        "char" | "uchar" | "int8" | "uint8" => 1,
        "short" | "ushort" | "int16" | "uint16" => 2,
        "double" | "float64" => 8,
        _ => 4,
    };
    let xi = vprops
        .iter()
        .position(|(_, n)| n == "x")
        .ok_or(Error::Corrupt)?;
    let mut v: Vec<[f32; 3]> = Vec::with_capacity(nv.min(1 << 24));
    let mut out = Vec::new();
    if format == "a" {
        let s = String::from_utf8_lossy(&d[body..]);
        let mut lines = s.lines();
        for _ in 0..nv {
            let p: Vec<f32> = lines
                .next()
                .unwrap_or("")
                .split_whitespace()
                .filter_map(|x| x.parse().ok())
                .collect();
            v.push([
                *p.get(xi).unwrap_or(&0.0),
                *p.get(xi + 1).unwrap_or(&0.0),
                *p.get(xi + 2).unwrap_or(&0.0),
            ]);
        }
        for _ in 0..nf {
            let p: Vec<usize> = lines
                .next()
                .unwrap_or("")
                .split_whitespace()
                .filter_map(|x| x.parse().ok())
                .collect();
            if let Some((_, idx)) = p.split_first() {
                fan(&mut out, &v, idx);
            }
        }
        return Ok(out);
    }
    let le = format == "l";
    let rd = |p: usize, t: &str| -> f64 {
        let n = size(t);
        let Some(b) = d.get(p..p + n) else { return 0.0 };
        let mut a = [0u8; 8];
        a[..n].copy_from_slice(b);
        if !le {
            a[..n].reverse();
        }
        match t {
            "char" | "int8" => a[0] as i8 as f64,
            "uchar" | "uint8" => a[0] as f64,
            "short" | "int16" => i16::from_le_bytes([a[0], a[1]]) as f64,
            "ushort" | "uint16" => u16::from_le_bytes([a[0], a[1]]) as f64,
            "int" | "int32" => i32::from_le_bytes([a[0], a[1], a[2], a[3]]) as f64,
            "uint" | "uint32" => u32::from_le_bytes([a[0], a[1], a[2], a[3]]) as f64,
            "double" | "float64" => f64::from_le_bytes(a),
            _ => f32::from_le_bytes([a[0], a[1], a[2], a[3]]) as f64,
        }
    };
    let mut p = body;
    for _ in 0..nv {
        let mut vals = [0f32; 3];
        for (k, (t, _)) in vprops.iter().enumerate() {
            if k >= xi && k < xi + 3 {
                vals[k - xi] = rd(p, t) as f32;
            }
            p += size(t);
        }
        v.push(vals);
    }
    for _ in 0..nf {
        let n = rd(p, &face_types.0) as usize;
        p += size(&face_types.0);
        let idx: Vec<usize> = (0..n)
            .map(|i| rd(p + i * size(&face_types.1), &face_types.1) as usize)
            .collect();
        p += n * size(&face_types.1);
        fan(&mut out, &v, &idx);
        if p > d.len() {
            break;
        }
    }
    Ok(out)
}

// --- JSON minimal pour glTF ---
#[derive(Clone, Debug)]
pub enum Json {
    Null,
    Bool(#[allow(dead_code)] bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, k: &str) -> Option<&Json> {
        match self {
            Json::Obj(v) => v.iter().find(|(n, _)| n == k).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn idx(&self, i: usize) -> Option<&Json> {
        match self {
            Json::Arr(v) => v.get(i),
            _ => None,
        }
    }
    pub fn num(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            _ => None,
        }
    }
    pub fn arr(&self) -> &[Json] {
        match self {
            Json::Arr(v) => v,
            _ => &[],
        }
    }
    pub fn str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
}

pub fn parse_json(s: &[u8]) -> Option<Json> {
    let mut p = 0;
    let v = jvalue(s, &mut p, 0)?;
    Some(v)
}

fn ws(s: &[u8], p: &mut usize) {
    while *p < s.len() && s[*p].is_ascii_whitespace() {
        *p += 1;
    }
}

fn jvalue(s: &[u8], p: &mut usize, depth: usize) -> Option<Json> {
    if depth > 64 {
        return None;
    }
    ws(s, p);
    match *s.get(*p)? {
        b'{' => {
            *p += 1;
            let mut v = Vec::new();
            loop {
                ws(s, p);
                if s.get(*p) == Some(&b'}') {
                    *p += 1;
                    return Some(Json::Obj(v));
                }
                let Json::Str(k) = jvalue(s, p, depth + 1)? else {
                    return None;
                };
                ws(s, p);
                if s.get(*p) != Some(&b':') {
                    return None;
                }
                *p += 1;
                let val = jvalue(s, p, depth + 1)?;
                v.push((k, val));
                ws(s, p);
                if s.get(*p) == Some(&b',') {
                    *p += 1;
                }
            }
        }
        b'[' => {
            *p += 1;
            let mut v = Vec::new();
            loop {
                ws(s, p);
                if s.get(*p) == Some(&b']') {
                    *p += 1;
                    return Some(Json::Arr(v));
                }
                v.push(jvalue(s, p, depth + 1)?);
                ws(s, p);
                if s.get(*p) == Some(&b',') {
                    *p += 1;
                }
            }
        }
        b'"' => {
            *p += 1;
            let mut out = Vec::new();
            while *p < s.len() && s[*p] != b'"' {
                if s[*p] == b'\\' {
                    *p += 1;
                    match *s.get(*p)? {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let h = std::str::from_utf8(s.get(*p + 1..*p + 5)?).ok()?;
                            let c = char::from_u32(u32::from_str_radix(h, 16).ok()?).unwrap_or('?');
                            let mut b = [0u8; 4];
                            out.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
                            *p += 4;
                        }
                        c => out.push(c),
                    }
                } else {
                    out.push(s[*p]);
                }
                *p += 1;
            }
            *p += 1;
            Some(Json::Str(String::from_utf8_lossy(&out).into_owned()))
        }
        b't' => {
            *p += 4;
            Some(Json::Bool(true))
        }
        b'f' => {
            *p += 5;
            Some(Json::Bool(false))
        }
        b'n' => {
            *p += 4;
            Some(Json::Null)
        }
        _ => {
            let st = *p;
            while *p < s.len() && matches!(s[*p], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                *p += 1;
            }
            std::str::from_utf8(&s[st..*p])
                .ok()?
                .parse()
                .ok()
                .map(Json::Num)
        }
    }
}

fn glb(d: &[u8]) -> Result<Vec<[f32; 9]>, Error> {
    let mut p = 12;
    let mut json: &[u8] = &[];
    let mut bin: &[u8] = &[];
    while p + 8 <= d.len() {
        let len = super::le32(d, p) as usize;
        let kind = &d[p + 4..p + 8];
        let body = d.get(p + 8..p + 8 + len).ok_or(Error::Corrupt)?;
        if kind == b"JSON" {
            json = body;
        } else if kind == b"BIN\0" {
            bin = body;
        }
        p += 8 + len;
    }
    gltf_json(json, bin)
}

type M4 = [f32; 16];

fn m4_mul(a: &M4, b: &M4) -> M4 {
    let mut r = [0f32; 16];
    for c in 0..4 {
        for row in 0..4 {
            r[c * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[c * 4 + k]).sum();
        }
    }
    r
}

fn node_matrix(n: &Json) -> M4 {
    if let Some(m) = n.get("matrix") {
        let v: Vec<f32> = m
            .arr()
            .iter()
            .filter_map(|x| x.num())
            .map(|x| x as f32)
            .collect();
        if v.len() == 16 {
            return v.try_into().unwrap_or([0.0; 16]);
        }
    }
    let g = |k: &str, d: &[f32]| -> Vec<f32> {
        n.get(k)
            .map(|a| {
                a.arr()
                    .iter()
                    .filter_map(|x| x.num())
                    .map(|x| x as f32)
                    .collect()
            })
            .filter(|v: &Vec<f32>| v.len() == d.len())
            .unwrap_or_else(|| d.to_vec())
    };
    let t = g("translation", &[0.0, 0.0, 0.0]);
    let q = g("rotation", &[0.0, 0.0, 0.0, 1.0]);
    let s = g("scale", &[1.0, 1.0, 1.0]);
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let r = [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + z * w),
        2.0 * (x * z - y * w),
        0.0,
        2.0 * (x * y - z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + x * w),
        0.0,
        2.0 * (x * z + y * w),
        2.0 * (y * z - x * w),
        1.0 - 2.0 * (x * x + y * y),
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ];
    let mut m = r;
    for c in 0..3 {
        for row in 0..3 {
            m[c * 4 + row] *= s[c];
        }
    }
    m[12] = t[0];
    m[13] = t[1];
    m[14] = t[2];
    m
}

fn gltf_json(json: &[u8], bin: &[u8]) -> Result<Vec<[f32; 9]>, Error> {
    let j = parse_json(json).ok_or(Error::Corrupt)?;
    // tampons : BIN de GLB ou URI data:
    let mut buffers: Vec<Vec<u8>> = Vec::new();
    for (i, b) in j
        .get("buffers")
        .map(|b| b.arr())
        .unwrap_or(&[])
        .iter()
        .enumerate()
    {
        match b.get("uri").and_then(|u| u.str()) {
            Some(u) if u.starts_with("data:") => {
                let data = u
                    .split_once(";base64,")
                    .and_then(|(_, b)| super::base64(b))
                    .unwrap_or_default();
                buffers.push(data);
            }
            Some(_) => return Err(Error::Variant("glTF tampon externe")),
            None => buffers.push(if i == 0 { bin.to_vec() } else { Vec::new() }),
        }
    }
    let views = j.get("bufferViews").map(|v| v.arr()).unwrap_or(&[]);
    let accs = j.get("accessors").map(|v| v.arr()).unwrap_or(&[]);
    let read = |ai: usize| -> Option<(Vec<f32>, usize)> {
        let a = accs.get(ai)?;
        let count = a.get("count")?.num()? as usize;
        let comps = match a.get("type")?.str()? {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            _ => return None,
        };
        let ct = a.get("componentType")?.num()? as u32;
        let csize = match ct {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            _ => 4,
        };
        let bv = views.get(a.get("bufferView")?.num()? as usize)?;
        let buf = buffers.get(bv.get("buffer").and_then(|b| b.num()).unwrap_or(0.0) as usize)?;
        let off = bv.get("byteOffset").and_then(|v| v.num()).unwrap_or(0.0) as usize
            + a.get("byteOffset").and_then(|v| v.num()).unwrap_or(0.0) as usize;
        let stride = bv
            .get("byteStride")
            .and_then(|v| v.num())
            .map(|v| v as usize)
            .unwrap_or(comps * csize);
        let mut out = Vec::with_capacity(count * comps);
        for i in 0..count {
            for c in 0..comps {
                let p = off + i * stride + c * csize;
                let v = match ct {
                    5126 => f32le(buf, p),
                    5125 => super::le32(buf, p) as f32,
                    5123 => super::le16(buf, p) as f32,
                    5121 => *buf.get(p)? as f32,
                    5122 => super::le16(buf, p) as i16 as f32,
                    _ => *buf.get(p)? as i8 as f32,
                };
                out.push(v);
            }
        }
        Some((out, comps))
    };
    let meshes = j.get("meshes").map(|m| m.arr()).unwrap_or(&[]);
    let nodes = j.get("nodes").map(|m| m.arr()).unwrap_or(&[]);
    let mut out = Vec::new();
    let emit = |mi: usize, m: &M4, out: &mut Vec<[f32; 9]>| {
        let Some(mesh) = meshes.get(mi) else { return };
        for prim in mesh.get("primitives").map(|p| p.arr()).unwrap_or(&[]) {
            let mode = prim.get("mode").and_then(|v| v.num()).unwrap_or(4.0) as u32;
            if mode != 4 {
                continue;
            }
            let Some(pa) = prim
                .get("attributes")
                .and_then(|a| a.get("POSITION"))
                .and_then(|v| v.num())
            else {
                continue;
            };
            let Some((pos, 3)) = read(pa as usize) else {
                continue;
            };
            let tv: Vec<[f32; 3]> = pos
                .as_chunks::<3>()
                .0
                .iter()
                .map(|p| {
                    [
                        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
                        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
                        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
                    ]
                })
                .collect();
            let idx: Vec<usize> = match prim.get("indices").and_then(|v| v.num()) {
                Some(ia) => read(ia as usize)
                    .map(|(v, _)| v.into_iter().map(|x| x as usize).collect())
                    .unwrap_or_default(),
                None => (0..tv.len()).collect(),
            };
            for t in idx.as_chunks::<3>().0.iter() {
                fan(out, &tv, t);
            }
        }
    };
    let scene = j.get("scene").and_then(|s| s.num()).unwrap_or(0.0) as usize;
    let roots: Vec<usize> = j
        .get("scenes")
        .and_then(|s| s.idx(scene))
        .and_then(|s| s.get("nodes"))
        .map(|n| {
            n.arr()
                .iter()
                .filter_map(|x| x.num())
                .map(|x| x as usize)
                .collect()
        })
        .unwrap_or_default();
    if roots.is_empty() || nodes.is_empty() {
        for mi in 0..meshes.len() {
            emit(
                mi,
                &[
                    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
                ],
                &mut out,
            );
        }
    } else {
        let id: M4 = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let mut stack: Vec<(usize, M4, usize)> = roots.into_iter().map(|r| (r, id, 0)).collect();
        while let Some((ni, parent, depth)) = stack.pop() {
            let Some(n) = nodes.get(ni) else { continue };
            if depth > 64 {
                continue;
            }
            let m = m4_mul(&parent, &node_matrix(n));
            if let Some(mi) = n.get("mesh").and_then(|v| v.num()) {
                emit(mi as usize, &m, &mut out);
            }
            for c in n.get("children").map(|c| c.arr()).unwrap_or(&[]) {
                if let Some(c) = c.num() {
                    stack.push((c as usize, m, depth + 1));
                }
            }
        }
    }
    Ok(out)
}

/// FBX binaire : géométries (Vertices + PolygonVertexIndex).
fn fbx(d: &[u8]) -> Result<Vec<[f32; 9]>, Error> {
    let ver = super::le32(d, 23);
    let wide = ver >= 7500;
    let mut out = Vec::new();
    let mut verts: Vec<f64> = Vec::new();
    fn walk(
        d: &[u8],
        mut p: usize,
        end: usize,
        wide: bool,
        verts: &mut Vec<f64>,
        out: &mut Vec<[f32; 9]>,
        depth: usize,
    ) -> Option<()> {
        if depth > 32 {
            return None;
        }
        while p < end {
            let (eo, np, _pl, hdr) = if wide {
                let r = |o: usize| {
                    u64::from_le_bytes(
                        d.get(p + o..p + o + 8)
                            .and_then(|b| b.try_into().ok())
                            .unwrap_or([0; 8]),
                    ) as usize
                };
                (r(0), r(8), r(16), 24)
            } else {
                (
                    super::le32(d, p) as usize,
                    super::le32(d, p + 4) as usize,
                    super::le32(d, p + 8) as usize,
                    12,
                )
            };
            if eo == 0 {
                return Some(());
            }
            let nl = *d.get(p + hdr)? as usize;
            let name = d.get(p + hdr + 1..p + hdr + 1 + nl)?;
            let mut q = p + hdr + 1 + nl;
            let mut props: Vec<Vec<f64>> = Vec::new();
            for _ in 0..np {
                let t = *d.get(q)?;
                q += 1;
                match t {
                    b'Y' => q += 2,
                    b'C' => q += 1,
                    b'I' | b'F' => q += 4,
                    b'D' | b'L' => q += 8,
                    b'S' | b'R' => q += 4 + super::le32(d, q) as usize,
                    b'f' | b'd' | b'l' | b'i' | b'b' => {
                        let n = super::le32(d, q) as usize;
                        let enc = super::le32(d, q + 4);
                        let cl = super::le32(d, q + 8) as usize;
                        let raw = d.get(q + 12..q + 12 + cl)?;
                        q += 12 + cl;
                        let es = match t {
                            b'f' | b'i' => 4,
                            b'b' => 1,
                            _ => 8,
                        };
                        let wanted = name == b"Vertices" || name == b"PolygonVertexIndex";
                        if !wanted {
                            props.push(Vec::new());
                            continue;
                        }
                        let data = if enc == 1 {
                            super::inflate::inflate_zlib(raw, n * es + 16)?
                        } else {
                            raw.to_vec()
                        };
                        let vals: Vec<f64> = (0..n)
                            .map(|i| match t {
                                b'f' => f32le(&data, i * 4) as f64,
                                b'd' => f64::from_le_bytes(
                                    data.get(i * 8..i * 8 + 8)
                                        .and_then(|b| b.try_into().ok())
                                        .unwrap_or([0; 8]),
                                ),
                                b'i' => super::le32(&data, i * 4) as i32 as f64,
                                b'l' => i64::from_le_bytes(
                                    data.get(i * 8..i * 8 + 8)
                                        .and_then(|b| b.try_into().ok())
                                        .unwrap_or([0; 8]),
                                ) as f64,
                                _ => *data.get(i).unwrap_or(&0) as f64,
                            })
                            .collect();
                        props.push(vals);
                    }
                    _ => return None,
                }
            }
            if name == b"Vertices" {
                *verts = props.into_iter().next().unwrap_or_default();
            } else if name == b"PolygonVertexIndex" {
                let v: Vec<[f32; 3]> = verts
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|c| [c[0] as f32, c[1] as f32, c[2] as f32])
                    .collect();
                let mut poly = Vec::new();
                for &i in props.first().map(|v| v.as_slice()).unwrap_or(&[]) {
                    let i = i as i64;
                    if i < 0 {
                        poly.push((!i) as usize);
                        fan(out, &v, &poly);
                        poly.clear();
                    } else {
                        poly.push(i as usize);
                    }
                }
            } else if q < eo {
                walk(d, q, eo, wide, verts, out, depth + 1);
            }
            p = eo;
        }
        Some(())
    }
    walk(d, 27, d.len(), wide, &mut verts, &mut out, 0);
    if out.is_empty() {
        return Err(Error::Variant("FBX sans géométrie"));
    }
    Ok(out)
}

/// Rendu ombré (z-buffer) centré en (cx,cy), le modèle occupe `side` px.
#[allow(clippy::too_many_arguments)]
pub fn render(
    m: &Mesh,
    cv: &mut Canvas,
    cx: f32,
    cy: f32,
    side: f32,
    yaw: f32,
    pitch: f32,
    bg: [u8; 3],
) {
    let (w, h) = (cv.w, cv.h);
    let mut zb = vec![f32::MIN; w * h];
    let (sy, cyw) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let scale = side * 0.47;
    let rot = |x: f32, y: f32, z: f32| {
        let x1 = cyw * x + sy * z;
        let z1 = -sy * x + cyw * z;
        let y2 = cp * y - sp * z1;
        let z2 = sp * y + cp * z1;
        (x1, y2, z2)
    };
    let light = {
        let (x, y, z) = (-0.4f32, 0.6f32, 0.7f32);
        let l = (x * x + y * y + z * z).sqrt();
        (x / l, y / l, z / l)
    };
    let dark = (bg[0] as u32 + bg[1] as u32 + bg[2] as u32) < 384;
    let base = if dark {
        [0.70, 0.78, 0.90]
    } else {
        [0.45, 0.58, 0.78]
    };
    for t in &m.tris {
        let a = rot(t[0], t[1], t[2]);
        let b = rot(t[3], t[4], t[5]);
        let c = rot(t[6], t[7], t[8]);
        let n = {
            let (ux, uy, uz) = (b.0 - a.0, b.1 - a.1, b.2 - a.2);
            let (vx, vy, vz) = (c.0 - a.0, c.1 - a.1, c.2 - a.2);
            let (nx, ny, nz) = (uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx);
            let l = (nx * nx + ny * ny + nz * nz).sqrt();
            if l <= 0.0 {
                continue;
            }
            (nx / l, ny / l, nz / l)
        };
        let lam = (n.0 * light.0 + n.1 * light.1 + n.2 * light.2).abs();
        let rim = 1.0 - n.2.abs();
        let k = 0.28 + 0.62 * lam + 0.10 * rim;
        let col = [base[0] * k, base[1] * k, base[2] * k];
        let p = |v: (f32, f32, f32)| (cx + v.0 * scale, cy - v.1 * scale, v.2);
        let (p0, p1, p2) = (p(a), p(b), p(c));
        let minx = p0.0.min(p1.0).min(p2.0).floor().max(0.0) as usize;
        let maxx = (p0.0.max(p1.0).max(p2.0).ceil().max(0.0) as usize).min(w);
        let miny = p0.1.min(p1.1).min(p2.1).floor().max(0.0) as usize;
        let maxy = (p0.1.max(p1.1).max(p2.1).ceil().max(0.0) as usize).min(h);
        let area = (p1.0 - p0.0) * (p2.1 - p0.1) - (p2.0 - p0.0) * (p1.1 - p0.1);
        if area.abs() < 1e-8 {
            continue;
        }
        for y in miny..maxy {
            let fy = y as f32 + 0.5;
            for x in minx..maxx {
                let fx = x as f32 + 0.5;
                let w0 = ((p1.0 - fx) * (p2.1 - fy) - (p2.0 - fx) * (p1.1 - fy)) / area;
                let w1 = ((p2.0 - fx) * (p0.1 - fy) - (p0.0 - fx) * (p2.1 - fy)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * p0.2 + w1 * p1.2 + w2 * p2.2;
                let i = y * w + x;
                if z > zb[i] {
                    zb[i] = z;
                    cv.px[i] = [col[0], col[1], col[2], 1.0];
                }
            }
        }
    }
}
