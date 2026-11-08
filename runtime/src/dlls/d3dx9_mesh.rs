//! D3DX9 : maillages (#834).
//!
//! ID3DXMesh sur de vrais tampons de sommets et d'indices Direct3D 9,
//! D3DXCreateMesh(FVF), D3DXLoadMeshFromX (fichiers .x texte, binaires et
//! compressés MSZIP, d'après la description publique du format DirectX
//! File), D3DXComputeNormals et les formes D3DXCreateBox/Sphere/Cylinder/
//! Torus/Polygon.
#![allow(clippy::too_many_arguments)]

use super::legacy_com::{get_ptr, ptr_size, put_ptr, rd32, vtable_sig, wr32};
use crate::com_call;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

const S_OK: i32 = 0;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const D3DERR_INVALIDCALL: i32 = 0x8876_086Cu32 as i32;
const D3DXERR_INVALIDDATA: i32 = 0x8876_0B59u32 as i32;
const D3DXERR_INVALIDMESH: i32 = 0x8876_0B54u32 as i32;

const DEV_ADDREF: usize = 1;
const DEV_RELEASE: usize = 2;
const DEV_CREATE_VB: usize = 26;
const DEV_CREATE_IB: usize = 27;
const DEV_DRAW_INDEXED: usize = 82;
const DEV_SET_FVF: usize = 89;
const DEV_SET_STREAM: usize = 100;
const DEV_SET_INDICES: usize = 104;
const BUF_LOCK: usize = 11;
const BUF_UNLOCK: usize = 12;

const FVF_XYZ: u32 = 0x002;
const FVF_NORMAL: u32 = 0x010;
const FVF_DIFFUSE: u32 = 0x040;
const FVF_SPECULAR: u32 = 0x080;
const FVF_POSITION_MASK: u32 = 0x400E;

const MESH_32BIT: u32 = 0x001;
const FMT_INDEX16: u32 = 101;
const FMT_INDEX32: u32 = 102;

// ═══════════════════════════════════════════════════════════════════════
// Format de sommet (FVF)
// ═══════════════════════════════════════════════════════════════════════

/// Éléments d'un FVF : (usage, index, décalage, taille en octets, type décl).
fn fvf_layout(fvf: u32) -> Option<Vec<(u8, u8, u16, u16, u8)>> {
    let mut v = Vec::new();
    let mut off = 0u16;
    let mut push = |u: u8, i: u8, size: u16, ty: u8, v: &mut Vec<_>| {
        v.push((u, i, off, size, ty));
        off += size;
    };
    match fvf & FVF_POSITION_MASK {
        0 => {}
        0x002 => push(0, 0, 12, 2, &mut v),
        0x004 => push(9, 0, 16, 3, &mut v), // XYZRHW
        0x4002 => push(0, 0, 16, 3, &mut v),
        _ => return None, // poids de mélange : non pris en charge
    }
    if fvf & FVF_NORMAL != 0 {
        push(3, 0, 12, 2, &mut v);
    }
    if fvf & 0x020 != 0 {
        push(4, 0, 4, 0, &mut v);
    }
    if fvf & FVF_DIFFUSE != 0 {
        push(10, 0, 4, 4, &mut v);
    }
    if fvf & FVF_SPECULAR != 0 {
        push(10, 1, 4, 4, &mut v);
    }
    let ntex = (fvf >> 8) & 0xF;
    for t in 0..ntex.min(8) {
        // Nombre de coordonnées : bits 16+2t (0 = 2, 1 = 3, 2 = 4, 3 = 1).
        let (size, ty) = match (fvf >> (16 + 2 * t)) & 3 {
            0 => (8, 1),
            1 => (12, 2),
            2 => (16, 3),
            _ => (4, 0),
        };
        push(5, t as u8, size, ty, &mut v);
    }
    Some(v)
}

fn fvf_stride(fvf: u32) -> u32 {
    fvf_layout(fvf)
        .map(|l| l.iter().map(|e| e.3 as u32).sum())
        .unwrap_or(0)
}

/// D3DVERTEXELEMENT9[] terminé par D3DDECL_END → FVF équivalent.
unsafe fn decl_to_fvf(p: usize) -> Option<u32> {
    if p == 0 {
        return None;
    }
    let mut fvf = 0u32;
    let mut ntex = 0u32;
    for i in 0..64 {
        let e = p + i * 8;
        let stream = (e as *const u16).read_unaligned();
        if stream == 0xFF {
            break;
        }
        let ty = *((e + 4) as *const u8);
        let usage = *((e + 6) as *const u8);
        let idx = *((e + 7) as *const u8);
        match (usage, idx, ty) {
            (0, 0, 2) => fvf |= FVF_XYZ,
            (9, 0, 3) => fvf |= 0x004,
            (3, 0, 2) => fvf |= FVF_NORMAL,
            (4, 0, 0) => fvf |= 0x020,
            (10, 0, 4) => fvf |= FVF_DIFFUSE,
            (10, 1, 4) => fvf |= FVF_SPECULAR,
            (5, i, t) if i as u32 == ntex => {
                let code = match t {
                    1 => 0,
                    2 => 1,
                    3 => 2,
                    0 => 3,
                    _ => return None,
                };
                fvf |= code << (16 + 2 * ntex);
                ntex += 1;
            }
            _ => return None,
        }
    }
    Some(fvf | (ntex << 8))
}

/// Écrit la déclaration (65 éléments au plus) d'un FVF.
unsafe fn write_decl(fvf: u32, out: usize) -> i32 {
    let Some(l) = fvf_layout(fvf) else {
        return D3DERR_INVALIDCALL;
    };
    let mut at = out;
    for (u, i, off, _, ty) in l {
        (at as *mut u16).write_unaligned(0);
        ((at + 2) as *mut u16).write_unaligned(off);
        *((at + 4) as *mut u8) = ty;
        *((at + 5) as *mut u8) = 0;
        *((at + 6) as *mut u8) = u;
        *((at + 7) as *mut u8) = i;
        at += 8;
    }
    (at as *mut u16).write_unaligned(0xFF);
    ((at + 2) as *mut u16).write_unaligned(0);
    *((at + 4) as *mut u8) = 17;
    *((at + 5) as *mut u8) = 0;
    *((at + 6) as *mut u8) = 0;
    *((at + 7) as *mut u8) = 0;
    S_OK
}

// ═══════════════════════════════════════════════════════════════════════
// Objet ID3DXMesh
// ═══════════════════════════════════════════════════════════════════════

#[repr(C)]
struct Mesh {
    vtbl: usize,
    refs: AtomicU32,
    dev: usize,
    vb: usize,
    ib: usize,
    fvf: u32,
    stride: u32,
    nverts: u32,
    nfaces: u32,
    options: u32,
    attrs: Vec<u32>,
    /// Table d'attributs (AttribId, FaceStart, FaceCount, VertexStart, VertexCount).
    table: Vec<[u32; 5]>,
}

unsafe fn mesh<'a>(this: usize) -> &'a mut Mesh {
    &mut *(this as *mut Mesh)
}

fn pool_of(options: u32) -> u32 {
    if options & 0x110 == 0x110 {
        2 // D3DPOOL_SYSTEMMEM
    } else {
        1 // D3DPOOL_MANAGED : verrouillable, survit à Reset
    }
}

/// Crée un maillage vide (tampons alloués, contenu nul).
unsafe fn new_mesh(
    dev: usize,
    nfaces: u32,
    nverts: u32,
    options: u32,
    fvf: u32,
) -> Result<usize, i32> {
    if dev == 0 || nfaces == 0 || nverts == 0 {
        return Err(D3DERR_INVALIDCALL);
    }
    let stride = fvf_stride(fvf);
    if stride == 0 {
        return Err(D3DERR_INVALIDCALL);
    }
    let big = options & MESH_32BIT != 0;
    if !big && nverts > 0xFFFF {
        return Err(D3DXERR_INVALIDMESH);
    }
    let pool = pool_of(options);
    let mut vb: usize = 0;
    let hr = com_call!(
        dev,
        DEV_CREATE_VB,
        fn(u32, u32, u32, u32, *mut usize, usize) -> i32,
        nverts * stride,
        0,
        fvf,
        pool,
        &mut vb,
        0
    );
    if hr < 0 || vb == 0 {
        return Err(if hr < 0 { hr } else { E_FAIL });
    }
    let idx_size = if big { 4 } else { 2 };
    let mut ib: usize = 0;
    let hr = com_call!(
        dev,
        DEV_CREATE_IB,
        fn(u32, u32, u32, u32, *mut usize, usize) -> i32,
        nfaces * 3 * idx_size,
        0,
        if big { FMT_INDEX32 } else { FMT_INDEX16 },
        pool,
        &mut ib,
        0
    );
    if hr < 0 || ib == 0 {
        com_call!(vb, 2, fn() -> u32);
        return Err(if hr < 0 { hr } else { E_FAIL });
    }
    com_call!(dev, DEV_ADDREF, fn() -> u32);
    Ok(Box::into_raw(Box::new(Mesh {
        vtbl: mesh_vtable(),
        refs: AtomicU32::new(1),
        dev,
        vb,
        ib,
        fvf,
        stride,
        nverts,
        nfaces,
        options,
        attrs: vec![0; nfaces as usize],
        table: Vec::new(),
    })) as usize)
}

unsafe fn lock_buf(b: usize) -> Option<*mut u8> {
    let mut p: usize = 0;
    let hr = com_call!(
        b,
        BUF_LOCK,
        fn(u32, u32, *mut usize, u32) -> i32,
        0,
        0,
        &mut p,
        0
    );
    (hr >= 0 && p != 0).then_some(p as *mut u8)
}
unsafe fn unlock_buf(b: usize) {
    com_call!(b, BUF_UNLOCK, fn() -> i32);
}

impl Mesh {
    unsafe fn vertices(&self) -> Vec<u8> {
        let n = (self.nverts * self.stride) as usize;
        match lock_buf(self.vb) {
            Some(p) => {
                let v = std::slice::from_raw_parts(p, n).to_vec();
                unlock_buf(self.vb);
                v
            }
            None => vec![0; n],
        }
    }
    unsafe fn set_vertices(&self, d: &[u8]) {
        if let Some(p) = lock_buf(self.vb) {
            let n = d.len().min((self.nverts * self.stride) as usize);
            std::ptr::copy_nonoverlapping(d.as_ptr(), p, n);
            unlock_buf(self.vb);
        }
    }
    unsafe fn indices(&self) -> Vec<u32> {
        let n = self.nfaces as usize * 3;
        let big = self.options & MESH_32BIT != 0;
        match lock_buf(self.ib) {
            Some(p) => {
                let v = if big {
                    std::slice::from_raw_parts(p as *const u32, n).to_vec()
                } else {
                    std::slice::from_raw_parts(p as *const u16, n)
                        .iter()
                        .map(|&x| x as u32)
                        .collect()
                };
                unlock_buf(self.ib);
                v
            }
            None => vec![0; n],
        }
    }
    unsafe fn set_indices(&self, idx: &[u32]) {
        let big = self.options & MESH_32BIT != 0;
        if let Some(p) = lock_buf(self.ib) {
            let n = idx.len().min(self.nfaces as usize * 3);
            for (i, &x) in idx[..n].iter().enumerate() {
                if big {
                    (p as *mut u32).add(i).write_unaligned(x);
                } else {
                    (p as *mut u16).add(i).write_unaligned(x as u16);
                }
            }
            unlock_buf(self.ib);
        }
    }
    /// Plages contiguës de faces par attribut.
    fn ranges(&self) -> Vec<[u32; 5]> {
        if !self.table.is_empty() {
            return self.table.clone();
        }
        let mut v: Vec<[u32; 5]> = Vec::new();
        for (f, &a) in self.attrs.iter().enumerate() {
            match v.last_mut() {
                Some(r) if r[0] == a && r[1] + r[2] == f as u32 => r[2] += 1,
                _ => v.push([a, f as u32, 1, 0, self.nverts]),
            }
        }
        v
    }
}

unsafe extern "win64" fn m_qi(this: usize, _iid: usize, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    m_addref(this);
    put_ptr(out, this);
    S_OK
}
unsafe extern "win64" fn m_addref(this: usize) -> u32 {
    mesh(this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn m_release(this: usize) -> u32 {
    let m = mesh(this);
    let n = m.refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        com_call!(m.vb, 2, fn() -> u32);
        com_call!(m.ib, 2, fn() -> u32);
        com_call!(m.dev, DEV_RELEASE, fn() -> u32);
        drop(Box::from_raw(this as *mut Mesh));
    }
    n
}
unsafe extern "win64" fn m_draw_subset(this: usize, attrib: u32) -> i32 {
    let m = mesh(this);
    let dev = m.dev;
    com_call!(dev, DEV_SET_FVF, fn(u32) -> i32, m.fvf);
    com_call!(
        dev,
        DEV_SET_STREAM,
        fn(u32, usize, u32, u32) -> i32,
        0,
        m.vb,
        0,
        m.stride
    );
    com_call!(dev, DEV_SET_INDICES, fn(usize) -> i32, m.ib);
    for r in m
        .ranges()
        .into_iter()
        .filter(|r| r[0] == attrib && r[2] > 0)
    {
        let hr = com_call!(
            dev,
            DEV_DRAW_INDEXED,
            fn(u32, i32, u32, u32, u32, u32) -> i32,
            4,
            0,
            0,
            m.nverts,
            r[1] * 3,
            r[2]
        );
        if hr < 0 {
            return hr;
        }
    }
    S_OK
}
unsafe extern "win64" fn m_num_faces(this: usize) -> u32 {
    mesh(this).nfaces
}
unsafe extern "win64" fn m_num_vertices(this: usize) -> u32 {
    mesh(this).nverts
}
unsafe extern "win64" fn m_fvf(this: usize) -> u32 {
    mesh(this).fvf
}
unsafe extern "win64" fn m_declaration(this: usize, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    write_decl(mesh(this).fvf, out)
}
unsafe extern "win64" fn m_bytes_per_vertex(this: usize) -> u32 {
    mesh(this).stride
}
unsafe extern "win64" fn m_options(this: usize) -> u32 {
    mesh(this).options
}
unsafe extern "win64" fn m_device(this: usize, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let d = mesh(this).dev;
    com_call!(d, DEV_ADDREF, fn() -> u32);
    put_ptr(out, d);
    S_OK
}

/// Copie les sommets vers un autre FVF, élément par élément.
fn convert_vertices(src: &[u8], from: u32, to: u32, n: usize) -> Vec<u8> {
    let (Some(a), Some(b)) = (fvf_layout(from), fvf_layout(to)) else {
        return Vec::new();
    };
    let (sa, sb) = (fvf_stride(from) as usize, fvf_stride(to) as usize);
    let mut out = vec![0u8; sb * n];
    for v in 0..n {
        for &(u, i, ob, size_b, _) in &b {
            if let Some(&(_, _, oa, size_a, _)) = a.iter().find(|e| e.0 == u && e.1 == i) {
                let len = size_a.min(size_b) as usize;
                let s = v * sa + oa as usize;
                let d = v * sb + ob as usize;
                if s + len <= src.len() {
                    out[d..d + len].copy_from_slice(&src[s..s + len]);
                }
            } else if u == 10 {
                // Couleur absente : blanc opaque.
                let d = v * sb + ob as usize;
                out[d..d + 4].copy_from_slice(&[255; 4]);
            }
        }
    }
    out
}

unsafe fn clone_to(this: usize, options: u32, fvf: u32, dev: usize, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    put_ptr(out, 0);
    let m = mesh(this);
    let dev = if dev == 0 { m.dev } else { dev };
    let c = match new_mesh(dev, m.nfaces, m.nverts, options, fvf) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let n = mesh(c);
    let v = convert_vertices(&m.vertices(), m.fvf, fvf, m.nverts as usize);
    n.set_vertices(&v);
    n.set_indices(&m.indices());
    n.attrs = m.attrs.clone();
    n.table = m.table.clone();
    put_ptr(out, c);
    S_OK
}
unsafe extern "win64" fn m_clone_fvf(
    this: usize,
    options: u32,
    fvf: u32,
    dev: usize,
    out: usize,
) -> i32 {
    clone_to(this, options, fvf, dev, out)
}
unsafe extern "win64" fn m_clone(
    this: usize,
    options: u32,
    decl: usize,
    dev: usize,
    out: usize,
) -> i32 {
    match decl_to_fvf(decl) {
        Some(fvf) => clone_to(this, options, fvf, dev, out),
        None => {
            crate::log_warn!(
                "d3dx9",
                "CloneMesh : déclaration sans équivalent FVF (#834)"
            );
            put_ptr(out, 0);
            E_NOTIMPL
        }
    }
}
unsafe extern "win64" fn m_vb(this: usize, out: usize) -> i32 {
    let b = mesh(this).vb;
    com_call!(b, 1, fn() -> u32);
    put_ptr(out, b);
    S_OK
}
unsafe extern "win64" fn m_ib(this: usize, out: usize) -> i32 {
    let b = mesh(this).ib;
    com_call!(b, 1, fn() -> u32);
    put_ptr(out, b);
    S_OK
}
unsafe extern "win64" fn m_lock_vb(this: usize, flags: u32, out: usize) -> i32 {
    let mut p: usize = 0;
    let vb = mesh(this).vb;
    let hr = com_call!(
        vb,
        BUF_LOCK,
        fn(u32, u32, *mut usize, u32) -> i32,
        0,
        0,
        &mut p,
        flags
    );
    put_ptr(out, p);
    hr
}
unsafe extern "win64" fn m_unlock_vb(this: usize) -> i32 {
    unlock_buf(mesh(this).vb);
    S_OK
}
unsafe extern "win64" fn m_lock_ib(this: usize, flags: u32, out: usize) -> i32 {
    let mut p: usize = 0;
    let ib = mesh(this).ib;
    let hr = com_call!(
        ib,
        BUF_LOCK,
        fn(u32, u32, *mut usize, u32) -> i32,
        0,
        0,
        &mut p,
        flags
    );
    put_ptr(out, p);
    hr
}
unsafe extern "win64" fn m_unlock_ib(this: usize) -> i32 {
    unlock_buf(mesh(this).ib);
    S_OK
}
unsafe extern "win64" fn m_attribute_table(this: usize, out: usize, size: usize) -> i32 {
    let m = mesh(this);
    if out != 0 {
        for (i, r) in m.table.iter().enumerate() {
            for (j, x) in r.iter().enumerate() {
                wr32(out + i * 20 + j * 4, *x);
            }
        }
    }
    if size != 0 {
        wr32(size, m.table.len() as u32);
    }
    S_OK
}

/// Représentant de chaque sommet : premier sommet de même position.
unsafe fn point_reps(m: &Mesh, eps: f32) -> Vec<u32> {
    let v = m.vertices();
    let stride = m.stride as usize;
    let pos = |i: usize| -> [f32; 3] {
        let b = &v[i * stride..i * stride + 12];
        [
            f32::from_le_bytes(b[0..4].try_into().unwrap_or_default()),
            f32::from_le_bytes(b[4..8].try_into().unwrap_or_default()),
            f32::from_le_bytes(b[8..12].try_into().unwrap_or_default()),
        ]
    };
    let q = if eps > 0.0 { eps } else { 1e-6 };
    let mut seen: HashMap<[i64; 3], u32> = HashMap::new();
    (0..m.nverts as usize)
        .map(|i| {
            if v.len() < (i + 1) * stride || stride < 12 {
                return i as u32;
            }
            let p = pos(i);
            let k = [
                (p[0] / q).round() as i64,
                (p[1] / q).round() as i64,
                (p[2] / q).round() as i64,
            ];
            *seen.entry(k).or_insert(i as u32)
        })
        .collect()
}

/// Adjacence : pour chaque arête de chaque face, la face voisine ou -1.
fn adjacency(idx: &[u32], reps: &[u32]) -> Vec<u32> {
    let r = |i: u32| reps.get(i as usize).copied().unwrap_or(i);
    let nf = idx.len() / 3;
    let mut edges: HashMap<(u32, u32), Vec<(usize, usize)>> = HashMap::new();
    for f in 0..nf {
        for e in 0..3 {
            let a = r(idx[f * 3 + e]);
            let b = r(idx[f * 3 + (e + 1) % 3]);
            edges.entry((a.min(b), a.max(b))).or_default().push((f, e));
        }
    }
    let mut adj = vec![u32::MAX; nf * 3];
    for list in edges.values() {
        if list.len() == 2 {
            let (f0, e0) = list[0];
            let (f1, e1) = list[1];
            if f0 != f1 {
                adj[f0 * 3 + e0] = f1 as u32;
                adj[f1 * 3 + e1] = f0 as u32;
            }
        }
    }
    adj
}

unsafe extern "win64" fn m_reps_to_adjacency(this: usize, reps: usize, adj: usize) -> i32 {
    let m = mesh(this);
    if adj == 0 {
        return D3DERR_INVALIDCALL;
    }
    let r: Vec<u32> = if reps == 0 {
        (0..m.nverts).collect()
    } else {
        std::slice::from_raw_parts(reps as *const u32, m.nverts as usize).to_vec()
    };
    for (i, x) in adjacency(&m.indices(), &r).iter().enumerate() {
        wr32(adj + i * 4, *x);
    }
    S_OK
}
unsafe extern "win64" fn m_adjacency_to_reps(this: usize, _adj: usize, reps: usize) -> i32 {
    let m = mesh(this);
    if reps == 0 {
        return D3DERR_INVALIDCALL;
    }
    for (i, x) in point_reps(m, 0.0).iter().enumerate() {
        wr32(reps + i * 4, *x);
    }
    S_OK
}
unsafe extern "win64" fn m_generate_adjacency(this: usize, eps: f32, adj: usize) -> i32 {
    let m = mesh(this);
    if adj == 0 {
        return D3DERR_INVALIDCALL;
    }
    let reps = point_reps(m, eps);
    for (i, x) in adjacency(&m.indices(), &reps).iter().enumerate() {
        wr32(adj + i * 4, *x);
    }
    S_OK
}
unsafe extern "win64" fn m_update_semantics(_this: usize, _decl: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn m_lock_attrs(this: usize, _flags: u32, out: usize) -> i32 {
    let m = mesh(this);
    m.table.clear();
    put_ptr(out, m.attrs.as_mut_ptr() as usize);
    S_OK
}
unsafe extern "win64" fn m_unlock_attrs(_this: usize) -> i32 {
    S_OK
}

/// Tri des faces par attribut (D3DXMESHOPT_ATTRSORT) et table d'attributs.
unsafe fn optimize_inplace(m: &mut Mesh, adj_out: usize, face_remap: usize) {
    let idx = m.indices();
    let mut order: Vec<usize> = (0..m.nfaces as usize).collect();
    order.sort_by_key(|&f| m.attrs[f]);
    let mut nidx = Vec::with_capacity(idx.len());
    let mut nattr = Vec::with_capacity(order.len());
    for &f in &order {
        nidx.extend_from_slice(&idx[f * 3..f * 3 + 3]);
        nattr.push(m.attrs[f]);
    }
    m.set_indices(&nidx);
    m.attrs = nattr;
    m.table = build_table(&nidx, &m.attrs);
    if face_remap != 0 {
        for (i, &f) in order.iter().enumerate() {
            wr32(face_remap + i * 4, f as u32);
        }
    }
    if adj_out != 0 {
        let reps = point_reps(m, 0.0);
        for (i, x) in adjacency(&nidx, &reps).iter().enumerate() {
            wr32(adj_out + i * 4, *x);
        }
    }
}
unsafe extern "win64" fn m_optimize(
    this: usize,
    flags: u32,
    _adj_in: usize,
    adj_out: usize,
    face_remap: usize,
    vert_remap: usize,
    out: usize,
) -> i32 {
    put_ptr(vert_remap, 0);
    let m = mesh(this);
    // Options du nouveau maillage : les D3DXMESH_* de `flags` (sauf 32BIT et
    // WRITEONLY, repris du maillage source) ; les D3DXMESHOPT_* sont au-dessus.
    let keep = MESH_32BIT | 0x440;
    let options = (flags & 0x000F_FFFF & !keep) | (m.options & keep);
    let hr = clone_to(this, options & !0xFF00_0000, m.fvf, m.dev, out);
    if hr < 0 {
        return hr;
    }
    optimize_inplace(mesh(get_ptr(out)), adj_out, face_remap);
    S_OK
}
unsafe extern "win64" fn m_optimize_inplace(
    this: usize,
    _flags: u32,
    _adj_in: usize,
    adj_out: usize,
    face_remap: usize,
    vert_remap: usize,
) -> i32 {
    put_ptr(vert_remap, 0);
    optimize_inplace(mesh(this), adj_out, face_remap);
    S_OK
}
unsafe extern "win64" fn m_set_attribute_table(this: usize, table: usize, n: u32) -> i32 {
    let m = mesh(this);
    m.table = (0..n as usize)
        .map(|i| {
            let mut r = [0u32; 5];
            for (j, x) in r.iter_mut().enumerate() {
                *x = rd32(table + i * 20 + j * 4);
            }
            r
        })
        .collect();
    S_OK
}

fn f(p: *const ()) -> usize {
    p as usize
}

fn mesh_vtable() -> usize {
    vtable_sig(
        "ID3DXMesh",
        &[
            (f(m_qi as *const ()), "ii"),
            (f(m_addref as *const ()), ""),
            (f(m_release as *const ()), ""),
            (f(m_draw_subset as *const ()), "i"),
            (f(m_num_faces as *const ()), ""),
            (f(m_num_vertices as *const ()), ""),
            (f(m_fvf as *const ()), ""),
            (f(m_declaration as *const ()), "i"),
            (f(m_bytes_per_vertex as *const ()), ""),
            (f(m_options as *const ()), ""),
            (f(m_device as *const ()), "i"),
            (f(m_clone_fvf as *const ()), "iiii"),
            (f(m_clone as *const ()), "iiii"),
            (f(m_vb as *const ()), "i"),
            (f(m_ib as *const ()), "i"),
            (f(m_lock_vb as *const ()), "ii"),
            (f(m_unlock_vb as *const ()), ""),
            (f(m_lock_ib as *const ()), "ii"),
            (f(m_unlock_ib as *const ()), ""),
            (f(m_attribute_table as *const ()), "ii"),
            (f(m_reps_to_adjacency as *const ()), "ii"),
            (f(m_adjacency_to_reps as *const ()), "ii"),
            (f(m_generate_adjacency as *const ()), "ii"),
            (f(m_update_semantics as *const ()), "i"),
            (f(m_lock_attrs as *const ()), "ii"),
            (f(m_unlock_attrs as *const ()), ""),
            (f(m_optimize as *const ()), "iiiiii"),
            (f(m_optimize_inplace as *const ()), "iiiii"),
            (f(m_set_attribute_table as *const ()), "ii"),
        ],
    )
}

// ═══════════════════════════════════════════════════════════════════════
// Construction depuis une géométrie en mémoire
// ═══════════════════════════════════════════════════════════════════════

/// Géométrie indépendante du périphérique.
#[derive(Default, Debug, Clone)]
struct Geometry {
    pos: Vec<[f32; 3]>,
    normal: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    tri: Vec<[u32; 3]>,
    attr: Vec<u32>,
}

impl Geometry {
    fn fvf(&self) -> u32 {
        let mut f = FVF_XYZ;
        if !self.normal.is_empty() {
            f |= FVF_NORMAL;
        }
        if !self.uv.is_empty() {
            f |= 0x100;
        }
        f
    }
    fn vertex_bytes(&self) -> Vec<u8> {
        let mut v = Vec::new();
        for i in 0..self.pos.len() {
            for c in self.pos[i] {
                v.extend_from_slice(&c.to_le_bytes());
            }
            if !self.normal.is_empty() {
                for c in self.normal.get(i).copied().unwrap_or([0.0; 3]) {
                    v.extend_from_slice(&c.to_le_bytes());
                }
            }
            if !self.uv.is_empty() {
                for c in self.uv.get(i).copied().unwrap_or([0.0; 2]) {
                    v.extend_from_slice(&c.to_le_bytes());
                }
            }
        }
        v
    }
}

unsafe fn upload(dev: usize, g: &Geometry, options: u32) -> Result<usize, i32> {
    let mut options = options;
    if g.pos.len() > 0xFFFF {
        options |= MESH_32BIT;
    }
    let m = new_mesh(
        dev,
        g.tri.len() as u32,
        g.pos.len() as u32,
        options,
        g.fvf(),
    )?;
    let me = mesh(m);
    me.set_vertices(&g.vertex_bytes());
    let idx: Vec<u32> = g.tri.iter().flatten().copied().collect();
    me.set_indices(&idx);
    me.attrs = if g.attr.len() == g.tri.len() {
        g.attr.clone()
    } else {
        vec![0; g.tri.len()]
    };
    me.table = build_table(&idx, &me.attrs);
    Ok(m)
}

/// Table d'attributs : une entrée par suite de faces de même attribut
/// (AttribId, FaceStart, FaceCount, VertexStart, VertexCount).
fn build_table(idx: &[u32], attrs: &[u32]) -> Vec<[u32; 5]> {
    let mut table: Vec<[u32; 5]> = Vec::new();
    for (f, &a) in attrs.iter().enumerate() {
        let tri = &idx[f * 3..f * 3 + 3];
        let lo = *tri.iter().min().unwrap_or(&0);
        let hi = *tri.iter().max().unwrap_or(&0);
        match table.last_mut() {
            Some(r) if r[0] == a => {
                r[2] += 1;
                let end = (r[3] + r[4]).max(hi + 1);
                r[3] = r[3].min(lo);
                r[4] = end - r[3];
            }
            _ => table.push([a, f as u32, 1, lo, hi + 1 - lo]),
        }
    }
    table
}

/// Rend l'adjacence demandée dans un ID3DXBuffer.
unsafe fn give_adjacency(m: usize, out: usize) {
    if out == 0 {
        return;
    }
    let me = mesh(m);
    let reps = point_reps(me, 0.0);
    let adj = adjacency(&me.indices(), &reps);
    let bytes: Vec<u8> = adj.iter().flat_map(|x| x.to_le_bytes()).collect();
    put_ptr(out, super::d3dx9::new_buffer(bytes));
}

// ═══════════════════════════════════════════════════════════════════════
// Fichiers .x
// ═══════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Name(String),
    Str(String),
    Num(f64),
    Open,
    Close,
    Template,
}

fn text_tokens(s: &str) -> Vec<Tok> {
    let b = s.as_bytes();
    let mut v = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'#' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'{' => {
                v.push(Tok::Open);
                i += 1;
            }
            b'}' => {
                v.push(Tok::Close);
                i += 1;
            }
            b'<' => {
                while i < b.len() && b[i] != b'>' {
                    i += 1;
                }
                i += 1;
            }
            b'"' => {
                let start = i + 1;
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += 1;
                }
                v.push(Tok::Str(
                    String::from_utf8_lossy(&b[start..i.min(b.len())]).into_owned(),
                ));
                i += 1;
            }
            b'0'..=b'9' | b'-' | b'+' | b'.' => {
                let start = i;
                i += 1;
                while i < b.len()
                    && (b[i].is_ascii_digit()
                        || matches!(b[i], b'.' | b'e' | b'E')
                        || (matches!(b[i], b'-' | b'+') && matches!(b[i - 1], b'e' | b'E')))
                {
                    i += 1;
                }
                if let Ok(x) = std::str::from_utf8(&b[start..i])
                    .unwrap_or("")
                    .parse::<f64>()
                {
                    v.push(Tok::Num(x));
                }
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < b.len()
                    && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'_' | b'-' | b'.'))
                {
                    i += 1;
                }
                let w = String::from_utf8_lossy(&b[start..i]).into_owned();
                v.push(if w == "template" {
                    Tok::Template
                } else {
                    Tok::Name(w)
                });
            }
            _ => i += 1,
        }
    }
    v
}

fn binary_tokens(b: &[u8], double: bool) -> Vec<Tok> {
    let mut v = Vec::new();
    let mut i = 0;
    let u16_at = |i: usize| b.get(i..i + 2).map(|s| u16::from_le_bytes([s[0], s[1]]));
    let u32_at = |i: usize| {
        b.get(i..i + 4)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    };
    while let Some(t) = u16_at(i) {
        i += 2;
        match t {
            1 | 2 => {
                let Some(n) = u32_at(i) else { break };
                let n = n as usize;
                let s = b.get(i + 4..i + 4 + n).unwrap_or(&[]);
                let s = String::from_utf8_lossy(s).into_owned();
                i += 4 + n;
                if t == 1 {
                    v.push(Tok::Name(s));
                } else {
                    v.push(Tok::Str(s));
                    i += 4; // terminateur (; ou ,)
                }
            }
            3 => {
                let Some(x) = u32_at(i) else { break };
                v.push(Tok::Num(x as f64));
                i += 4;
            }
            5 => i += 16,
            6 => {
                let Some(n) = u32_at(i) else { break };
                i += 4;
                for k in 0..n as usize {
                    match u32_at(i + 4 * k) {
                        Some(x) => v.push(Tok::Num(x as f64)),
                        None => return v,
                    }
                }
                i += 4 * n as usize;
            }
            7 => {
                let Some(n) = u32_at(i) else { break };
                i += 4;
                let w = if double { 8 } else { 4 };
                for k in 0..n as usize {
                    let at = i + w * k;
                    let x = if double {
                        b.get(at..at + 8)
                            .map(|s| f64::from_le_bytes(s.try_into().unwrap_or_default()))
                    } else {
                        u32_at(at).map(|x| f32::from_bits(x) as f64)
                    };
                    match x {
                        Some(x) => v.push(Tok::Num(x)),
                        None => return v,
                    }
                }
                i += w * n as usize;
            }
            10 => v.push(Tok::Open),
            11 => v.push(Tok::Close),
            31 => v.push(Tok::Template),
            _ => {}
        }
    }
    v
}

/// Décompresse un corps MSZIP (« tzip », « bzip ») : blocs « CK » + DEFLATE.
fn mszip(b: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read;
    let total = u32::from_le_bytes(b.get(0..4)?.try_into().ok()?) as usize;
    let mut out: Vec<u8> = Vec::with_capacity(total);
    let mut i = 4;
    while i + 4 <= b.len() && out.len() < total {
        let usize_ = u16::from_le_bytes([b[i], b[i + 1]]) as usize;
        let csize = u16::from_le_bytes([b[i + 2], b[i + 3]]) as usize;
        i += 4;
        let mut chunk = b.get(i..i + csize)?;
        i += csize;
        if chunk.starts_with(b"CK") {
            chunk = &chunk[2..];
        }
        let mut inf = crate::archive::inflate::Inflater::with_history(chunk, &out);
        let mut part = Vec::with_capacity(usize_);
        inf.read_to_end(&mut part).ok()?;
        out.extend_from_slice(&part);
    }
    Some(out)
}

#[derive(Debug, Default, Clone)]
struct Node {
    ty: String,
    name: String,
    vals: Vec<f64>,
    strs: Vec<String>,
    kids: Vec<Node>,
    refs: Vec<String>,
}

fn parse_nodes(t: &[Tok]) -> Vec<Node> {
    let mut i = 0;
    let mut top = Vec::new();
    while i < t.len() {
        match &t[i] {
            Tok::Template => {
                // template Nom { … } : ignoré.
                while i < t.len() && t[i] != Tok::Open {
                    i += 1;
                }
                let mut depth = 0;
                while i < t.len() {
                    match t[i] {
                        Tok::Open => depth += 1,
                        Tok::Close => {
                            depth -= 1;
                            if depth == 0 {
                                i += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
            Tok::Name(_) => match object(t, &mut i) {
                Some(n) => top.push(n),
                None => i += 1,
            },
            _ => i += 1,
        }
    }
    top
}

/// `Type [Nom] { … }` à partir de `t[*i]`.
fn object(t: &[Tok], i: &mut usize) -> Option<Node> {
    let Tok::Name(ty) = &t[*i] else { return None };
    let mut n = Node {
        ty: ty.clone(),
        ..Default::default()
    };
    let mut j = *i + 1;
    if let Some(Tok::Name(nm)) = t.get(j) {
        n.name = nm.clone();
        j += 1;
    }
    if t.get(j) != Some(&Tok::Open) {
        return None;
    }
    j += 1;
    while j < t.len() {
        match &t[j] {
            Tok::Close => {
                *i = j + 1;
                return Some(n);
            }
            Tok::Num(x) => {
                n.vals.push(*x);
                j += 1;
            }
            Tok::Str(s) => {
                n.strs.push(s.clone());
                j += 1;
            }
            Tok::Open => {
                // { Référence }
                if let (Some(Tok::Name(r)), Some(Tok::Close)) = (t.get(j + 1), t.get(j + 2)) {
                    n.refs.push(r.clone());
                    j += 3;
                } else {
                    j += 1;
                }
            }
            Tok::Name(_) => {
                let mut k = j;
                match object(t, &mut k) {
                    Some(c) => {
                        n.kids.push(c);
                        j = k;
                    }
                    None => j += 1,
                }
            }
            Tok::Template => j += 1,
        }
    }
    *i = j;
    Some(n)
}

/// Matériau lu dans un fichier .x.
#[derive(Clone, Debug, Default)]
struct XMaterial {
    diffuse: [f32; 4],
    power: f32,
    specular: [f32; 3],
    emissive: [f32; 3],
    texture: Option<String>,
}

fn material(n: &Node) -> XMaterial {
    let g = |k: usize| n.vals.get(k).copied().unwrap_or(0.0) as f32;
    let texture = n
        .kids
        .iter()
        .find(|k| k.ty.eq_ignore_ascii_case("TextureFilename"))
        .and_then(|k| k.strs.first().cloned());
    XMaterial {
        diffuse: [g(0), g(1), g(2), g(3)],
        power: g(4),
        specular: [g(5), g(6), g(7)],
        emissive: [g(8), g(9), g(10)],
        texture,
    }
}

type M4 = [f32; 16];
const IDENT: M4 = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];
fn mul(a: &M4, b: &M4) -> M4 {
    let mut r = [0.0; 16];
    for i in 0..4 {
        for j in 0..4 {
            r[i * 4 + j] = (0..4).map(|k| a[i * 4 + k] * b[k * 4 + j]).sum();
        }
    }
    r
}

/// Ajoute un objet Mesh (transformé par `m`) à la géométrie.
fn add_mesh(
    n: &Node,
    m: &M4,
    g: &mut Geometry,
    mats: &mut Vec<XMaterial>,
    named: &HashMap<String, XMaterial>,
) {
    let v = &n.vals;
    let mut k = 0;
    let mut next = || {
        let x = v.get(k).copied().unwrap_or(0.0);
        k += 1;
        x
    };
    let nv = next() as usize;
    let mut pos = Vec::with_capacity(nv);
    for _ in 0..nv {
        let p = [next() as f32, next() as f32, next() as f32];
        pos.push([
            p[0] * m[0] + p[1] * m[4] + p[2] * m[8] + m[12],
            p[0] * m[1] + p[1] * m[5] + p[2] * m[9] + m[13],
            p[0] * m[2] + p[1] * m[6] + p[2] * m[10] + m[14],
        ]);
    }
    let nf = next() as usize;
    let mut faces: Vec<Vec<u32>> = Vec::with_capacity(nf);
    for _ in 0..nf {
        let c = next() as usize;
        faces.push((0..c.min(64)).map(|_| next() as u32).collect());
    }
    // Normales : tableau propre et indices par face.
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut nfaces: Vec<Vec<u32>> = Vec::new();
    if let Some(nn) = n.kids.iter().find(|c| c.ty == "MeshNormals") {
        let w = &nn.vals;
        let mut k = 0;
        let mut nx = || {
            let x = w.get(k).copied().unwrap_or(0.0);
            k += 1;
            x
        };
        let cnt = nx() as usize;
        for _ in 0..cnt {
            let p = [nx() as f32, nx() as f32, nx() as f32];
            let t = [
                p[0] * m[0] + p[1] * m[4] + p[2] * m[8],
                p[0] * m[1] + p[1] * m[5] + p[2] * m[9],
                p[0] * m[2] + p[1] * m[6] + p[2] * m[10],
            ];
            let l = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt().max(1e-12);
            normals.push([t[0] / l, t[1] / l, t[2] / l]);
        }
        let fc = nx() as usize;
        for _ in 0..fc {
            let c = nx() as usize;
            nfaces.push((0..c.min(64)).map(|_| nx() as u32).collect());
        }
    }
    let uv: Vec<[f32; 2]> = n
        .kids
        .iter()
        .find(|c| c.ty == "MeshTextureCoords")
        .map(|t| {
            let cnt = t.vals.first().copied().unwrap_or(0.0) as usize;
            (0..cnt)
                .map(|i| {
                    [
                        t.vals.get(1 + i * 2).copied().unwrap_or(0.0) as f32,
                        t.vals.get(2 + i * 2).copied().unwrap_or(0.0) as f32,
                    ]
                })
                .collect()
        })
        .unwrap_or_default();
    // Matériaux du maillage.
    let base_mat = mats.len() as u32;
    let mut face_mat = vec![0u32; nf];
    if let Some(ml) = n.kids.iter().find(|c| c.ty == "MeshMaterialList") {
        let nm = ml.vals.first().copied().unwrap_or(0.0) as usize;
        let ni = ml.vals.get(1).copied().unwrap_or(0.0) as usize;
        let list: Vec<u32> = ml.vals.iter().skip(2).take(ni).map(|&x| x as u32).collect();
        for (f, slot) in face_mat.iter_mut().enumerate() {
            *slot = list
                .get(f)
                .or(list.last())
                .copied()
                .unwrap_or(0)
                .min(nm.saturating_sub(1) as u32);
        }
        let mut found = Vec::new();
        for c in ml.kids.iter().filter(|c| c.ty == "Material") {
            found.push(material(c));
        }
        for r in &ml.refs {
            if let Some(x) = named.get(r) {
                found.push(x.clone());
            }
        }
        found.resize(nm.max(1), XMaterial::default());
        mats.extend(found);
    } else {
        mats.push(XMaterial {
            diffuse: [1.0, 1.0, 1.0, 1.0],
            ..Default::default()
        });
    }
    // Sommets uniques (position, normale) par coin de face.
    let want_n = !normals.is_empty() || !g.normal.is_empty();
    let want_uv = !uv.is_empty() || !g.uv.is_empty();
    if want_n && g.normal.len() < g.pos.len() {
        g.normal.resize(g.pos.len(), [0.0; 3]);
    }
    if want_uv && g.uv.len() < g.pos.len() {
        g.uv.resize(g.pos.len(), [0.0; 2]);
    }
    let mut map: HashMap<(u32, u32), u32> = HashMap::new();
    for (fi, face) in faces.iter().enumerate() {
        let corner = |c: usize, map: &mut HashMap<(u32, u32), u32>, g: &mut Geometry| -> u32 {
            let p = face[c];
            let nidx = nfaces
                .get(fi)
                .and_then(|nf| nf.get(c))
                .copied()
                .unwrap_or(u32::MAX);
            *map.entry((p, nidx)).or_insert_with(|| {
                g.pos.push(pos.get(p as usize).copied().unwrap_or([0.0; 3]));
                if want_n {
                    g.normal
                        .push(normals.get(nidx as usize).copied().unwrap_or([0.0; 3]));
                }
                if want_uv {
                    g.uv.push(uv.get(p as usize).copied().unwrap_or([0.0; 2]));
                }
                (g.pos.len() - 1) as u32
            })
        };
        if face.len() < 3 {
            continue;
        }
        for t in 1..face.len() - 1 {
            let a = corner(0, &mut map, g);
            let b = corner(t, &mut map, g);
            let c = corner(t + 1, &mut map, g);
            g.tri.push([a, b, c]);
            g.attr.push(base_mat + face_mat[fi]);
        }
    }
}

fn walk_frames(
    n: &Node,
    parent: &M4,
    g: &mut Geometry,
    mats: &mut Vec<XMaterial>,
    named: &HashMap<String, XMaterial>,
) {
    match n.ty.as_str() {
        "Mesh" => add_mesh(n, parent, g, mats, named),
        "Frame" => {
            let local = n
                .kids
                .iter()
                .find(|k| k.ty == "FrameTransformMatrix")
                .filter(|k| k.vals.len() >= 16)
                .map(|k| {
                    let mut m = [0.0f32; 16];
                    for (i, x) in m.iter_mut().enumerate() {
                        *x = k.vals[i] as f32;
                    }
                    m
                })
                .unwrap_or(IDENT);
            let world = mul(&local, parent);
            for k in &n.kids {
                walk_frames(k, &world, g, mats, named);
            }
        }
        _ => {}
    }
}

/// Lit un fichier .x : géométrie fusionnée et matériaux.
fn load_x(d: &[u8]) -> Result<(Geometry, Vec<XMaterial>), String> {
    if d.len() < 16 || &d[..4] != b"xof " {
        return Err("en-tête « xof » absent".into());
    }
    let format = &d[8..12];
    let double = &d[12..16] == b"0064";
    let body = &d[16..];
    let toks = match format {
        b"txt " => text_tokens(&String::from_utf8_lossy(body)),
        b"bin " => binary_tokens(body, double),
        b"tzip" => {
            let raw = mszip(body).ok_or("flux MSZIP illisible")?;
            text_tokens(&String::from_utf8_lossy(&raw))
        }
        b"bzip" => {
            let raw = mszip(body).ok_or("flux MSZIP illisible")?;
            binary_tokens(&raw, double)
        }
        _ => return Err("format .x inconnu".into()),
    };
    let nodes = parse_nodes(&toks);
    let named: HashMap<String, XMaterial> = nodes
        .iter()
        .filter(|n| n.ty == "Material" && !n.name.is_empty())
        .map(|n| (n.name.clone(), material(n)))
        .collect();
    let mut g = Geometry::default();
    let mut mats = Vec::new();
    for n in &nodes {
        walk_frames(n, &IDENT, &mut g, &mut mats, &named);
    }
    if g.tri.is_empty() {
        return Err("aucun maillage dans le fichier".into());
    }
    Ok((g, mats))
}

/// Tampon D3DXMATERIAL[] (pointeurs de noms de texture dans le tampon).
unsafe fn materials_buffer(mats: &[XMaterial]) -> usize {
    let ps = ptr_size();
    let rec = 68 + if ps == 8 { 4 } else { 0 } + ps; // D3DMATERIAL9 puis LPSTR aligné
    let head = rec * mats.len();
    let mut data = vec![0u8; head];
    let mut names: Vec<(usize, usize)> = Vec::new();
    for (i, m) in mats.iter().enumerate() {
        let at = i * rec;
        let floats: Vec<f32> = m
            .diffuse
            .iter()
            .copied()
            .chain([0.0, 0.0, 0.0, 0.0]) // Ambient
            .chain([m.specular[0], m.specular[1], m.specular[2], 1.0])
            .chain([m.emissive[0], m.emissive[1], m.emissive[2], 1.0])
            .chain([m.power])
            .collect();
        for (k, x) in floats.iter().enumerate() {
            data[at + 4 * k..at + 4 * k + 4].copy_from_slice(&x.to_le_bytes());
        }
        if let Some(t) = &m.texture {
            names.push((at + rec - ps, data.len()));
            data.extend_from_slice(t.as_bytes());
            data.push(0);
        }
    }
    let buf = super::d3dx9::new_buffer(data);
    let base = com_call!(buf, 3, fn() -> usize);
    for (slot, off) in names {
        put_ptr(base + slot, base + off);
    }
    buf
}

unsafe fn load_into(
    d: &[u8],
    options: u32,
    dev: usize,
    adj: usize,
    mats: usize,
    effects: usize,
    nmat: usize,
    out: usize,
) -> i32 {
    put_ptr(out, 0);
    put_ptr(adj, 0);
    put_ptr(mats, 0);
    put_ptr(effects, 0);
    if dev == 0 || out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let (g, m) = match load_x(d) {
        Ok(x) => x,
        Err(e) => {
            crate::log_warn!("d3dx9", "D3DXLoadMeshFromX : {e}");
            return D3DXERR_INVALIDDATA;
        }
    };
    let mesh = match upload(dev, &g, options) {
        Ok(x) => x,
        Err(e) => return e,
    };
    give_adjacency(mesh, adj);
    if mats != 0 {
        put_ptr(mats, materials_buffer(&m));
    }
    if effects != 0 {
        // D3DXEFFECTINSTANCE { LPSTR; DWORD; D3DXEFFECTDEFAULT* } vides.
        let ps = ptr_size();
        let rec = if ps == 8 { 24 } else { 12 };
        put_ptr(
            effects,
            super::d3dx9::new_buffer(vec![0; rec * m.len().max(1)]),
        );
    }
    if nmat != 0 {
        wr32(nmat, m.len() as u32);
    }
    put_ptr(out, mesh);
    S_OK
}

pub unsafe extern "win64" fn load_mesh_from_x_a(
    path: *const u8,
    options: u32,
    dev: usize,
    adj: usize,
    mats: usize,
    effects: usize,
    nmat: usize,
    out: usize,
) -> i32 {
    if path.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let p = crate::paths::to_linux(&super::cstr(path));
    match std::fs::read(&p) {
        Ok(d) => load_into(&d, options, dev, adj, mats, effects, nmat, out),
        Err(_) => {
            crate::log_warn!("d3dx9", "maillage introuvable : {}", p.display());
            put_ptr(out, 0);
            D3DXERR_INVALIDDATA
        }
    }
}
pub unsafe extern "win64" fn load_mesh_from_x_w(
    path: *const u16,
    options: u32,
    dev: usize,
    adj: usize,
    mats: usize,
    effects: usize,
    nmat: usize,
    out: usize,
) -> i32 {
    if path.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let p = crate::paths::to_linux(&super::wstr(path));
    match std::fs::read(&p) {
        Ok(d) => load_into(&d, options, dev, adj, mats, effects, nmat, out),
        Err(_) => {
            crate::log_warn!("d3dx9", "maillage introuvable : {}", p.display());
            put_ptr(out, 0);
            D3DXERR_INVALIDDATA
        }
    }
}
pub unsafe extern "win64" fn load_mesh_from_x_in_memory(
    data: *const u8,
    size: u32,
    options: u32,
    dev: usize,
    adj: usize,
    mats: usize,
    effects: usize,
    nmat: usize,
    out: usize,
) -> i32 {
    if data.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let d = std::slice::from_raw_parts(data, size as usize);
    load_into(d, options, dev, adj, mats, effects, nmat, out)
}
/// D3DXLoadMeshFromXResource(Module, Name, Type, Options, pDevice, …).
pub unsafe extern "win64" fn load_mesh_from_x_resource(
    module: usize,
    name: *const u8,
    ty: *const u8,
    options: u32,
    dev: usize,
    adj: usize,
    mats: usize,
    effects: usize,
    nmat: usize,
    out: usize,
) -> i32 {
    use super::resource::*;
    let h = find_resource_a(module, name, ty);
    if h == 0 {
        put_ptr(out, 0);
        return D3DXERR_INVALIDDATA;
    }
    let size = sizeof_resource(module, h) as usize;
    let p = lock_resource(load_resource(module, h));
    if p == 0 {
        return D3DXERR_INVALIDDATA;
    }
    let d = std::slice::from_raw_parts(p as *const u8, size);
    load_into(d, options, dev, adj, mats, effects, nmat, out)
}

pub unsafe extern "win64" fn create_mesh_fvf(
    nfaces: u32,
    nverts: u32,
    options: u32,
    fvf: u32,
    dev: usize,
    out: usize,
) -> i32 {
    put_ptr(out, 0);
    match new_mesh(dev, nfaces, nverts, options, fvf) {
        Ok(m) => {
            put_ptr(out, m);
            S_OK
        }
        Err(e) => e,
    }
}
pub unsafe extern "win64" fn create_mesh(
    nfaces: u32,
    nverts: u32,
    options: u32,
    decl: usize,
    dev: usize,
    out: usize,
) -> i32 {
    match decl_to_fvf(decl) {
        Some(fvf) => create_mesh_fvf(nfaces, nverts, options, fvf, dev, out),
        None => {
            put_ptr(out, 0);
            E_NOTIMPL
        }
    }
}

/// D3DXComputeNormals(pMesh, pAdjacency) : normales lissées par sommet.
pub unsafe extern "win64" fn compute_normals(m: usize, _adj: usize) -> i32 {
    if m == 0 {
        return D3DERR_INVALIDCALL;
    }
    let me = mesh(m);
    let Some(layout) = fvf_layout(me.fvf) else {
        return D3DERR_INVALIDCALL;
    };
    let Some(&(_, _, noff, _, _)) = layout.iter().find(|e| e.0 == 3) else {
        return D3DERR_INVALIDCALL;
    };
    let mut v = me.vertices();
    let stride = me.stride as usize;
    let idx = me.indices();
    let reps = point_reps(me, 0.0);
    let p = |v: &[u8], i: usize| -> [f32; 3] {
        let b = &v[i * stride..];
        [
            f32::from_le_bytes(b[0..4].try_into().unwrap_or_default()),
            f32::from_le_bytes(b[4..8].try_into().unwrap_or_default()),
            f32::from_le_bytes(b[8..12].try_into().unwrap_or_default()),
        ]
    };
    let mut acc = vec![[0.0f32; 3]; me.nverts as usize];
    for t in idx.chunks(3) {
        if t.len() < 3 {
            break;
        }
        let (a, b, c) = (
            p(&v, t[0] as usize),
            p(&v, t[1] as usize),
            p(&v, t[2] as usize),
        );
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let w = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            u[1] * w[2] - u[2] * w[1],
            u[2] * w[0] - u[0] * w[2],
            u[0] * w[1] - u[1] * w[0],
        ];
        for &k in t {
            let r = reps[k as usize] as usize;
            for j in 0..3 {
                acc[r][j] += n[j];
            }
        }
    }
    for i in 0..me.nverts as usize {
        let n = acc[reps[i] as usize];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-12);
        let at = i * stride + noff as usize;
        for j in 0..3 {
            v[at + 4 * j..at + 4 * j + 4].copy_from_slice(&(n[j] / l).to_le_bytes());
        }
    }
    me.set_vertices(&v);
    S_OK
}

// ---- Formes ----

fn finish(g: Geometry, dev: usize, out: usize, adj: usize) -> i32 {
    unsafe {
        put_ptr(out, 0);
        put_ptr(adj, 0);
        match upload(dev, &g, 0x220) {
            Ok(m) => {
                give_adjacency(m, adj);
                put_ptr(out, m);
                S_OK
            }
            Err(e) => e,
        }
    }
}

pub unsafe extern "win64" fn create_box(
    dev: usize,
    w: f32,
    h: f32,
    d: f32,
    out: usize,
    adj: usize,
) -> i32 {
    let (x, y, z) = (w / 2.0, h / 2.0, d / 2.0);
    let mut g = Geometry::default();
    // Six faces, quatre sommets chacune (normales franches).
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        (
            [-1.0, 0.0, 0.0],
            [[-x, -y, -z], [-x, -y, z], [-x, y, z], [-x, y, -z]],
        ),
        (
            [0.0, 1.0, 0.0],
            [[-x, y, -z], [-x, y, z], [x, y, z], [x, y, -z]],
        ),
        (
            [1.0, 0.0, 0.0],
            [[x, y, -z], [x, y, z], [x, -y, z], [x, -y, -z]],
        ),
        (
            [0.0, -1.0, 0.0],
            [[-x, -y, z], [-x, -y, -z], [x, -y, -z], [x, -y, z]],
        ),
        (
            [0.0, 0.0, 1.0],
            [[-x, -y, z], [x, -y, z], [x, y, z], [-x, y, z]],
        ),
        (
            [0.0, 0.0, -1.0],
            [[-x, -y, -z], [-x, y, -z], [x, y, -z], [x, -y, -z]],
        ),
    ];
    for (n, q) in faces {
        let b = g.pos.len() as u32;
        for p in q {
            g.pos.push(p);
            g.normal.push(n);
        }
        g.tri.push([b, b + 1, b + 2]);
        g.tri.push([b + 2, b + 3, b]);
    }
    g.attr = vec![0; g.tri.len()];
    finish(g, dev, out, adj)
}

/// Surface de révolution : anneaux (rayon, z, normale radiale, normale z).
fn revolve(rings: &[(f32, f32, f32, f32)], slices: u32) -> Geometry {
    let mut g = Geometry::default();
    let s = slices.max(3);
    for &(r, z, nr, nz) in rings {
        for k in 0..s {
            let a = k as f32 / s as f32 * std::f32::consts::TAU;
            let (sn, cs) = a.sin_cos();
            g.pos.push([r * cs, r * sn, z]);
            g.normal.push([nr * cs, nr * sn, nz]);
        }
    }
    for i in 0..rings.len().saturating_sub(1) as u32 {
        for k in 0..s {
            let a = i * s + k;
            let b = i * s + (k + 1) % s;
            let c = a + s;
            let d = b + s;
            g.tri.push([a, b, d]);
            g.tri.push([d, c, a]);
        }
    }
    g.attr = vec![0; g.tri.len()];
    g
}

pub unsafe extern "win64" fn create_sphere(
    dev: usize,
    radius: f32,
    slices: u32,
    stacks: u32,
    out: usize,
    adj: usize,
) -> i32 {
    if slices < 2 || stacks < 2 || radius < 0.0 {
        return D3DERR_INVALIDCALL;
    }
    let rings: Vec<_> = (0..=stacks)
        .map(|i| {
            let t = std::f32::consts::PI * i as f32 / stacks as f32;
            let (s, c) = t.sin_cos();
            (radius * s, radius * c, s, c)
        })
        .collect();
    finish(revolve(&rings, slices), dev, out, adj)
}
pub unsafe extern "win64" fn create_cylinder(
    dev: usize,
    r1: f32,
    r2: f32,
    len: f32,
    slices: u32,
    stacks: u32,
    out: usize,
    adj: usize,
) -> i32 {
    if slices < 2 || stacks < 1 || r1 < 0.0 || r2 < 0.0 || len < 0.0 {
        return D3DERR_INVALIDCALL;
    }
    let slope = (r1 - r2) / len.max(1e-6);
    let nl = (1.0 + slope * slope).sqrt();
    let rings: Vec<_> = (0..=stacks)
        .map(|i| {
            let t = i as f32 / stacks as f32;
            (
                r1 + (r2 - r1) * t,
                -len / 2.0 + len * t,
                1.0 / nl,
                slope / nl,
            )
        })
        .collect();
    finish(revolve(&rings, slices), dev, out, adj)
}
pub unsafe extern "win64" fn create_torus(
    dev: usize,
    inner: f32,
    outer: f32,
    sides: u32,
    rings: u32,
    out: usize,
    adj: usize,
) -> i32 {
    if sides < 3 || rings < 3 || inner < 0.0 || outer < 0.0 {
        return D3DERR_INVALIDCALL;
    }
    let mut g = Geometry::default();
    for i in 0..rings {
        let a = i as f32 / rings as f32 * std::f32::consts::TAU;
        let (sa, ca) = a.sin_cos();
        for j in 0..sides {
            let b = j as f32 / sides as f32 * std::f32::consts::TAU;
            let (sb, cb) = b.sin_cos();
            let r = outer + inner * cb;
            g.pos.push([r * ca, r * sa, inner * sb]);
            g.normal.push([cb * ca, cb * sa, sb]);
        }
    }
    for i in 0..rings {
        for j in 0..sides {
            let a = i * sides + j;
            let b = i * sides + (j + 1) % sides;
            let c = ((i + 1) % rings) * sides + j;
            let d = ((i + 1) % rings) * sides + (j + 1) % sides;
            g.tri.push([a, c, d]);
            g.tri.push([d, b, a]);
        }
    }
    g.attr = vec![0; g.tri.len()];
    finish(g, dev, out, adj)
}
pub unsafe extern "win64" fn create_polygon(
    dev: usize,
    length: f32,
    sides: u32,
    out: usize,
    adj: usize,
) -> i32 {
    if sides < 3 || length < 0.0 {
        return D3DERR_INVALIDCALL;
    }
    let r = length / (2.0 * (std::f32::consts::PI / sides as f32).sin());
    let mut g = Geometry::default();
    g.pos.push([0.0; 3]);
    g.normal.push([0.0, 0.0, 1.0]);
    for k in 0..sides {
        let a = k as f32 / sides as f32 * std::f32::consts::TAU;
        g.pos.push([r * a.cos(), r * a.sin(), 0.0]);
        g.normal.push([0.0, 0.0, 1.0]);
    }
    for k in 0..sides {
        g.tri.push([0, 1 + k, 1 + (k + 1) % sides]);
    }
    g.attr = vec![0; g.tri.len()];
    finish(g, dev, out, adj)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "D3DXLoadMeshFromXA" => fn load_mesh_from_x_a,
        "D3DXLoadMeshFromXW" => fn load_mesh_from_x_w,
        "D3DXLoadMeshFromXInMemory" => fn load_mesh_from_x_in_memory,
        "D3DXLoadMeshFromXResource" => fn load_mesh_from_x_resource,
        "D3DXCreateMeshFVF" => fn create_mesh_fvf,
        "D3DXCreateMesh" => fn create_mesh,
        "D3DXComputeNormals" => fn compute_normals,
        "D3DXCreateBox" => fn create_box,
        "D3DXCreateSphere" => fn create_sphere,
        "D3DXCreateCylinder" => fn create_cylinder,
        "D3DXCreateTorus" => fn create_torus,
        "D3DXCreatePolygon" => fn create_polygon,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUBE_TXT: &str = "xof 0303txt 0032\n\
        template Vector { <3D82AB5E-62DA-11cf-AB39-0020AF71E433> FLOAT x; FLOAT y; FLOAT z; }\n\
        Material Rouge { 1.0;0.0;0.0;1.0;; 10.0; 1.0;1.0;1.0;; 0.0;0.0;0.0;; TextureFilename { \"rouge.bmp\"; } }\n\
        Frame Racine {\n\
          FrameTransformMatrix { 1,0,0,0, 0,1,0,0, 0,0,1,0, 10,0,0,1;; }\n\
          Mesh Quad {\n\
            4; -1;-1;0;, 1;-1;0;, 1;1;0;, -1;1;0;;\n\
            1; 4;0,1,2,3;;\n\
            MeshNormals { 1; 0;0;1;; 1; 4;0,0,0,0;; }\n\
            MeshTextureCoords { 4; 0;1;, 1;1;, 1;0;, 0;0;; }\n\
            MeshMaterialList { 1; 1; 0;; { Rouge } }\n\
          }\n\
        }\n";

    #[test]
    fn fichier_x_texte() {
        let (g, m) = load_x(CUBE_TXT.as_bytes()).unwrap();
        assert_eq!(g.tri.len(), 2);
        assert_eq!(g.pos.len(), 4);
        assert_eq!(g.pos[0], [9.0, -1.0, 0.0]); // translation de la Frame
        assert_eq!(g.normal[2], [0.0, 0.0, 1.0]);
        assert_eq!(g.uv[1], [1.0, 1.0]);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].diffuse, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(m[0].texture.as_deref(), Some("rouge.bmp"));
        assert_eq!(g.fvf(), FVF_XYZ | FVF_NORMAL | 0x100);
    }

    #[test]
    fn fichier_x_binaire() {
        let mut b = b"xof 0303bin 0032".to_vec();
        let w16 = |b: &mut Vec<u8>, v: u16| b.extend_from_slice(&v.to_le_bytes());
        let w32 = |b: &mut Vec<u8>, v: u32| b.extend_from_slice(&v.to_le_bytes());
        w16(&mut b, 1);
        w32(&mut b, 4);
        b.extend_from_slice(b"Mesh");
        w16(&mut b, 10);
        // nVertices puis 3 sommets en liste de flottants.
        w16(&mut b, 6);
        w32(&mut b, 1);
        w32(&mut b, 3);
        w16(&mut b, 7);
        w32(&mut b, 9);
        for x in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            w32(&mut b, x.to_bits());
        }
        w16(&mut b, 6);
        w32(&mut b, 5);
        for x in [1, 3, 0, 1, 2] {
            w32(&mut b, x);
        }
        w16(&mut b, 11);
        let (g, m) = load_x(&b).unwrap();
        assert_eq!(g.tri, vec![[0, 1, 2]]);
        assert_eq!(g.pos[1], [1.0, 0.0, 0.0]);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn adjacence_et_fvf() {
        let adj = adjacency(&[0, 1, 2, 2, 3, 0], &[0, 1, 2, 3]);
        assert_eq!(adj, vec![u32::MAX, u32::MAX, 1, u32::MAX, u32::MAX, 0]);
        assert_eq!(fvf_stride(FVF_XYZ | FVF_NORMAL | 0x100), 32);
        assert_eq!(fvf_stride(FVF_XYZ | FVF_DIFFUSE), 16);
        let src: Vec<u8> = [1.0f32, 2.0, 3.0, 0.0, 0.0, 1.0, 0.5, 0.25]
            .iter()
            .flat_map(|x| x.to_le_bytes())
            .collect();
        let out = convert_vertices(&src, FVF_XYZ | FVF_NORMAL | 0x100, FVF_XYZ | 0x100, 1);
        assert_eq!(out.len(), 20);
        assert_eq!(f32::from_le_bytes(out[12..16].try_into().unwrap()), 0.5);
    }
}
