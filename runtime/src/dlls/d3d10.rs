//! Direct3D 10 et 10.1 (d3d10.dll, d3d10_1.dll) au-dessus de Direct3D 11
//! (#1090, étape 2).
//!
//! ID3D10Device réunit l'appareil et le contexte immédiat de Direct3D 11 :
//! chaque méthode est relayée vers l'appareil ou le contexte Direct3D 11
//! de Winlux, en convertissant ce qui diffère (fenêtres d'affichage en
//! entiers, description de blend, vue de profondeur sans `Flags`, tampon
//! sans `StructureByteStride`, déclaration de flux de sortie sans `Stream`).
//! Les objets rendus au programme sont des enveloppes Direct3D 10
//! (`Obj`) autour des objets Direct3D 11 : elles ajoutent Map et Unmap aux
//! ressources, Begin, End et GetData aux requêtes, et rendent l'appareil
//! Direct3D 10 par GetDevice. Une même ressource Direct3D 11 a une seule
//! enveloppe, retrouvée quand le programme la relit (GetResource,
//! PSGetShaderResources…).
//!
//! Les méthodes lisent elles-mêmes la disposition x86 ou x64 de leurs
//! arguments (pointeurs de 4 ou 8 octets) : leurs tables x86 n'ont que des
//! arguments de 4 octets (`legacy_com::vtable_sig`).
//!
//! Interfaces et structures : en-têtes d3d10.h et d3d10_1.h de MinGW-w64
//! (domaine public).

#![allow(non_snake_case)]

use crate::dlls::legacy_com::{method, rd32, vtable_sig, wr32, x86};
use crate::dlls::ole32::Guid;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;

const fn d3d10_iid(d1: u32) -> Guid {
    Guid {
        data1: d1,
        data2: 0x342c,
        data3: 0x4106,
        data4: [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
    }
}

const IID_IUNKNOWN: Guid = Guid {
    data1: 0,
    data2: 0,
    data3: 0,
    data4: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};
const IID_DEVICE_CHILD: Guid = d3d10_iid(0x9b7e4c00);
const IID_RESOURCE: Guid = d3d10_iid(0x9b7e4c01);
const IID_BUFFER: Guid = d3d10_iid(0x9b7e4c02);
const IID_TEXTURE1D: Guid = d3d10_iid(0x9b7e4c03);
pub(crate) const IID_TEXTURE2D: Guid = d3d10_iid(0x9b7e4c04);
const IID_TEXTURE3D: Guid = d3d10_iid(0x9b7e4c05);
const IID_SRV: Guid = d3d10_iid(0x9b7e4c07);
const IID_RTV: Guid = d3d10_iid(0x9b7e4c08);
const IID_DSV: Guid = d3d10_iid(0x9b7e4c09);
const IID_VERTEX_SHADER: Guid = d3d10_iid(0x9b7e4c0a);
const IID_INPUT_LAYOUT: Guid = d3d10_iid(0x9b7e4c0b);
const IID_SAMPLER: Guid = d3d10_iid(0x9b7e4c0c);
const IID_ASYNCHRONOUS: Guid = d3d10_iid(0x9b7e4c0d);
const IID_QUERY: Guid = d3d10_iid(0x9b7e4c0e);
const IID_DEVICE: Guid = d3d10_iid(0x9b7e4c0f);
const IID_PREDICATE: Guid = d3d10_iid(0x9b7e4c10);
const IID_COUNTER: Guid = d3d10_iid(0x9b7e4c11);
const IID_SRV1: Guid = d3d10_iid(0x9b7e4c87);
const IID_DEVICE1: Guid = d3d10_iid(0x9b7e4c8f);
const IID_MULTITHREAD: Guid = d3d10_iid(0x9b7e4e00);
const IID_VIEW: Guid = Guid {
    data1: 0xc902b03f,
    data2: 0x60a7,
    data3: 0x49ba,
    data4: [0x99, 0x36, 0x2a, 0x3a, 0xb3, 0x7a, 0x7e, 0x33],
};
const IID_BLEND: Guid = Guid {
    data1: 0xedad8d19,
    data2: 0x8a35,
    data3: 0x4d6d,
    data4: [0x85, 0x66, 0x2e, 0xa2, 0x76, 0xcd, 0xe1, 0x61],
};
const IID_BLEND1: Guid = Guid {
    data1: 0xedad8d99,
    data2: 0x8a35,
    data3: 0x4d6d,
    data4: [0x85, 0x66, 0x2e, 0xa2, 0x76, 0xcd, 0xe1, 0x61],
};
const IID_DEPTH_STENCIL: Guid = Guid {
    data1: 0x2b4b1cc8,
    data2: 0xa4ad,
    data3: 0x41f8,
    data4: [0x83, 0x22, 0xca, 0x86, 0xfc, 0x3e, 0xc6, 0x75],
};
const IID_GEOMETRY_SHADER: Guid = Guid {
    data1: 0x6316be88,
    data2: 0x54cd,
    data3: 0x4040,
    data4: [0xab, 0x44, 0x20, 0x46, 0x1b, 0xc8, 0x1f, 0x68],
};
const IID_PIXEL_SHADER: Guid = Guid {
    data1: 0x4968b601,
    data2: 0x9d00,
    data3: 0x4cde,
    data4: [0x83, 0x46, 0x8e, 0x7f, 0x67, 0x58, 0x19, 0xb6],
};
const IID_RASTERIZER: Guid = Guid {
    data1: 0xa2a07292,
    data2: 0x89af,
    data3: 0x4345,
    data4: [0xbe, 0x2e, 0xc5, 0x3d, 0x9f, 0xbb, 0x6e, 0x9f],
};

/// Niveaux de fonctionnalités.
const LEVEL_10_0: u32 = 0xa000;
const LEVEL_10_1: u32 = 0xa100;

/// Plafond d'un tableau d'objets passé par le programme.
const MAX_ARRAY: u32 = 128;

// --- Appels et pointeurs selon l'ABI du programme ------------------------------------

/// Appelle la méthode `idx` (fonction win64) d'un objet COM du runtime.
macro_rules! call {
    ($obj:expr, $idx:expr, fn($($t:ty),*) $(-> $r:ty)?, $($a:expr),* $(,)?) => {{
        let o = $obj as usize;
        let f: unsafe extern "win64" fn(usize $(, $t)*) $(-> $r)? =
            std::mem::transmute(method(o, $idx));
        f(o $(, $a)*)
    }};
}

/// Taille d'un pointeur du programme.
fn ps() -> usize {
    if x86() {
        4
    } else {
        8
    }
}

/// Lit un pointeur du programme.
unsafe fn rdp(at: usize) -> usize {
    if x86() {
        rd32(at) as usize
    } else {
        (at as *const usize).read_unaligned()
    }
}

/// Écrit un pointeur du programme (rien si `at` est nul).
unsafe fn wrp(at: usize, v: usize) {
    if at == 0 {
        return;
    }
    if x86() {
        wr32(at, v as u32)
    } else {
        (at as *mut usize).write_unaligned(v)
    }
}

unsafe fn rdu(at: usize) -> u32 {
    rd32(at)
}

// --- Enveloppes d'objets --------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Buffer,
    Texture1D,
    Texture2D,
    Texture3D,
    Srv,
    Rtv,
    Dsv,
    InputLayout,
    VertexShader,
    GeometryShader,
    PixelShader,
    Blend,
    DepthStencil,
    Rasterizer,
    Sampler,
    Query,
    Predicate,
    Counter,
}

impl Kind {
    fn iids(self) -> &'static [Guid] {
        match self {
            Kind::Buffer => &[IID_RESOURCE, IID_BUFFER],
            Kind::Texture1D => &[IID_RESOURCE, IID_TEXTURE1D],
            Kind::Texture2D => &[IID_RESOURCE, IID_TEXTURE2D],
            Kind::Texture3D => &[IID_RESOURCE, IID_TEXTURE3D],
            Kind::Srv => &[IID_VIEW, IID_SRV, IID_SRV1],
            Kind::Rtv => &[IID_VIEW, IID_RTV],
            Kind::Dsv => &[IID_VIEW, IID_DSV],
            Kind::InputLayout => &[IID_INPUT_LAYOUT],
            Kind::VertexShader => &[IID_VERTEX_SHADER],
            Kind::GeometryShader => &[IID_GEOMETRY_SHADER],
            Kind::PixelShader => &[IID_PIXEL_SHADER],
            Kind::Blend => &[IID_BLEND, IID_BLEND1],
            Kind::DepthStencil => &[IID_DEPTH_STENCIL],
            Kind::Rasterizer => &[IID_RASTERIZER],
            Kind::Sampler => &[IID_SAMPLER],
            Kind::Query => &[IID_ASYNCHRONOUS, IID_QUERY],
            Kind::Predicate => &[IID_ASYNCHRONOUS, IID_QUERY, IID_PREDICATE],
            Kind::Counter => &[IID_ASYNCHRONOUS, IID_COUNTER],
        }
    }

    fn is_resource(self) -> bool {
        matches!(
            self,
            Kind::Buffer | Kind::Texture1D | Kind::Texture2D | Kind::Texture3D
        )
    }

    /// Ressource Direct3D 11 d'après D3D11_RESOURCE_DIMENSION.
    fn of_dimension(dim: u32) -> Kind {
        match dim {
            1 => Kind::Buffer,
            2 => Kind::Texture1D,
            4 => Kind::Texture3D,
            _ => Kind::Texture2D,
        }
    }
}

/// Enveloppe Direct3D 10 d'un objet Direct3D 11 (une référence tenue).
#[repr(C)]
struct Obj {
    vtbl: usize,
    refs: AtomicU32,
    inner: usize,
    kind: Kind,
    dev: *mut Dev,
    /// Description Direct3D 10 gardée pour GetDesc (états, requêtes).
    desc: Vec<u8>,
}

/// Objet Direct3D 11 → son enveloppe.
static WRAPS: Mutex<Option<HashMap<usize, usize>>> = Mutex::new(None);
/// Enveloppes vivantes.
static LIVE: Mutex<Option<HashSet<usize>>> = Mutex::new(None);

fn wraps<R>(f: impl FnOnce(&mut HashMap<usize, usize>, &mut HashSet<usize>) -> R) -> R {
    let mut w = WRAPS.lock().unwrap_or_else(|e| e.into_inner());
    let mut l = LIVE.lock().unwrap_or_else(|e| e.into_inner());
    f(
        w.get_or_insert_with(HashMap::new),
        l.get_or_insert_with(HashSet::new),
    )
}

/// Objet Direct3D 11 visé par un objet passé par le programme : ses
/// enveloppes sont ouvertes, tout autre objet passe tel quel.
unsafe fn un(p: usize) -> usize {
    if p == 0 {
        return 0;
    }
    if wraps(|_, live| live.contains(&p)) {
        (*(p as *const Obj)).inner
    } else {
        p
    }
}

/// Enveloppe d'un objet Direct3D 11 rendu avec une référence pour
/// l'appelant (la référence passe à l'enveloppe).
unsafe fn wrap(inner: usize, kind: Kind, dev: *mut Dev, desc: Vec<u8>) -> usize {
    if inner == 0 {
        return 0;
    }
    if let Some(w) = wraps(|map, _| map.get(&inner).copied()) {
        (*(w as *const Obj)).refs.fetch_add(1, Ordering::AcqRel);
        call!(inner, 2, fn() -> u32,);
        return w;
    }
    let obj = Box::into_raw(Box::new(Obj {
        vtbl: table(kind),
        refs: AtomicU32::new(1),
        inner,
        kind,
        dev,
        desc,
    })) as usize;
    wraps(|map, live| {
        map.insert(inner, obj);
        live.insert(obj);
    });
    obj
}

/// Ressource rendue par Direct3D 11 (sa sorte d'après GetType).
unsafe fn wrap_resource(inner: usize, dev: *mut Dev) -> usize {
    if inner == 0 {
        return 0;
    }
    let mut dim = 0u32;
    call!(inner, 7, fn(*mut u32), &mut dim);
    wrap(inner, Kind::of_dimension(dim), dev, Vec::new())
}

/// Tampon arrière d'une chaîne d'échange demandé en ID3D10Texture2D.
pub(crate) unsafe fn wrap_backbuffer(tex: usize, device: usize) -> usize {
    let dev = if is_device(device) {
        device as *mut Dev
    } else {
        std::ptr::null_mut()
    };
    wrap(tex, Kind::Texture2D, dev, Vec::new())
}

unsafe fn obj<'a>(p: usize) -> &'a Obj {
    &*(p as *const Obj)
}

unsafe extern "win64" fn obj_qi(this: usize, riid: *const Guid, out: usize) -> i32 {
    if out == 0 {
        return E_INVALIDARG;
    }
    wrp(out, 0);
    let Some(r) = riid.as_ref() else {
        return E_INVALIDARG;
    };
    let o = obj(this);
    if *r == IID_IUNKNOWN || *r == IID_DEVICE_CHILD || o.kind.iids().contains(r) {
        o.refs.fetch_add(1, Ordering::AcqRel);
        wrp(out, this);
        return S_OK;
    }
    // Interfaces DXGI d'une ressource (IDXGISurface, IDXGIResource).
    if o.kind.is_resource() {
        let mut p = 0usize;
        let hr = call!(o.inner, 0, fn(*const Guid, *mut usize) -> i32, riid, &mut p);
        if hr >= 0 {
            wrp(out, p);
        }
        return hr;
    }
    E_NOINTERFACE
}
unsafe extern "win64" fn obj_addref(this: usize) -> u32 {
    obj(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn obj_release(this: usize) -> u32 {
    let o = obj(this);
    let prev = o.refs.fetch_sub(1, Ordering::AcqRel);
    if prev != 1 {
        return prev - 1;
    }
    let inner = o.inner;
    wraps(|map, live| {
        map.remove(&inner);
        live.remove(&this);
    });
    drop(Box::from_raw(this as *mut Obj));
    call!(inner, 2, fn() -> u32,);
    0
}
unsafe extern "win64" fn obj_get_device(this: usize, out: usize) {
    let d = obj(this).dev;
    if !d.is_null() {
        (*d).refs.fetch_add(1, Ordering::AcqRel);
    }
    wrp(out, d as usize);
}
unsafe extern "win64" fn obj_get_private(this: usize, g: usize, size: usize, data: usize) -> i32 {
    call!(
        obj(this).inner,
        4,
        fn(usize, usize, usize) -> i32,
        g,
        size,
        data
    )
}
unsafe extern "win64" fn obj_set_private(this: usize, g: usize, size: u32, data: usize) -> i32 {
    call!(
        obj(this).inner,
        5,
        fn(usize, u32, usize) -> i32,
        g,
        size,
        data
    )
}
unsafe extern "win64" fn obj_set_private_iface(this: usize, g: usize, iface: usize) -> i32 {
    call!(obj(this).inner, 6, fn(usize, usize) -> i32, g, iface)
}

// Ressources : GetType (7), priorité d'éviction (8, 9), Map (10), Unmap
// (11), GetDesc (12).
unsafe extern "win64" fn res_get_type(this: usize, out: *mut u32) {
    if !out.is_null() {
        *out = match obj(this).kind {
            Kind::Buffer => 1,
            Kind::Texture1D => 2,
            Kind::Texture3D => 4,
            _ => 3,
        };
    }
}
unsafe extern "win64" fn res_set_eviction(this: usize, p: u32) {
    call!(obj(this).inner, 8, fn(u32), p);
}
unsafe extern "win64" fn res_get_eviction(this: usize) -> u32 {
    call!(obj(this).inner, 9, fn() -> u32,)
}

/// D3D11_MAPPED_SUBRESOURCE.
#[repr(C)]
#[derive(Default)]
struct Mapped {
    data: usize,
    row: u32,
    depth: u32,
}

unsafe fn map(this: usize, sub: u32, kind: u32, flags: u32) -> Result<Mapped, i32> {
    let o = obj(this);
    if o.dev.is_null() {
        return Err(E_INVALIDARG);
    }
    let mut m = Mapped::default();
    let hr = call!(
        (*o.dev).ctx11,
        14,
        fn(usize, u32, u32, u32, *mut Mapped) -> i32,
        o.inner,
        sub,
        kind,
        flags,
        &mut m
    );
    if hr < 0 {
        Err(hr)
    } else {
        Ok(m)
    }
}
unsafe fn unmap(this: usize, sub: u32) {
    let o = obj(this);
    if !o.dev.is_null() {
        call!((*o.dev).ctx11, 15, fn(usize, u32), o.inner, sub);
    }
}

/// ID3D10Buffer::Map(type, flags, void **) et ID3D10Texture1D::Map(sub, …).
unsafe extern "win64" fn buffer_map(this: usize, kind: u32, flags: u32, out: usize) -> i32 {
    match map(this, 0, kind, flags) {
        Ok(m) => {
            wrp(out, m.data);
            S_OK
        }
        Err(hr) => hr,
    }
}
unsafe extern "win64" fn buffer_unmap(this: usize) {
    unmap(this, 0)
}
unsafe extern "win64" fn tex1d_map(
    this: usize,
    sub: u32,
    kind: u32,
    flags: u32,
    out: usize,
) -> i32 {
    match map(this, sub, kind, flags) {
        Ok(m) => {
            wrp(out, m.data);
            S_OK
        }
        Err(hr) => hr,
    }
}
/// D3D10_MAPPED_TEXTURE2D : pData, RowPitch.
unsafe extern "win64" fn tex2d_map(
    this: usize,
    sub: u32,
    kind: u32,
    flags: u32,
    out: usize,
) -> i32 {
    if out == 0 {
        return E_INVALIDARG;
    }
    match map(this, sub, kind, flags) {
        Ok(m) => {
            wrp(out, m.data);
            wr32(out + ps(), m.row);
            S_OK
        }
        Err(hr) => hr,
    }
}
/// D3D10_MAPPED_TEXTURE3D : pData, RowPitch, DepthPitch.
unsafe extern "win64" fn tex3d_map(
    this: usize,
    sub: u32,
    kind: u32,
    flags: u32,
    out: usize,
) -> i32 {
    if out == 0 {
        return E_INVALIDARG;
    }
    match map(this, sub, kind, flags) {
        Ok(m) => {
            wrp(out, m.data);
            wr32(out + ps(), m.row);
            wr32(out + ps() + 4, m.depth);
            S_OK
        }
        Err(hr) => hr,
    }
}
unsafe extern "win64" fn tex_unmap(this: usize, sub: u32) {
    unmap(this, sub)
}
/// D3D10_BUFFER_DESC : les cinq premiers champs de D3D11_BUFFER_DESC.
unsafe extern "win64" fn buffer_get_desc(this: usize, out: *mut [u32; 5]) {
    if out.is_null() {
        return;
    }
    let mut d = [0u32; 6];
    call!(obj(this).inner, 10, fn(*mut [u32; 6]), &mut d);
    (*out).copy_from_slice(&d[..5]);
}
/// Descriptions de textures : mêmes dispositions en Direct3D 10 et 11.
unsafe extern "win64" fn tex_get_desc(this: usize, out: usize) {
    if out != 0 {
        call!(obj(this).inner, 10, fn(usize), out);
    }
}

// Vues : GetResource (7), GetDesc (8), GetDesc1 (9, SRV 10.1).
unsafe extern "win64" fn view_get_resource(this: usize, out: usize) {
    let o = obj(this);
    let mut r = 0usize;
    call!(o.inner, 7, fn(*mut usize), &mut r);
    wrp(out, wrap_resource(r, o.dev));
}
unsafe extern "win64" fn view_get_desc(this: usize, out: usize) {
    if out == 0 {
        return;
    }
    let o = obj(this);
    if o.kind == Kind::Dsv {
        // D3D11 : Format, ViewDimension, Flags, union ; D3D10 sans Flags.
        let mut d = [0u32; 6];
        call!(o.inner, 8, fn(*mut [u32; 6]), &mut d);
        let d10 = [d[0], d[1], d[3], d[4], d[5]];
        std::ptr::copy_nonoverlapping(d10.as_ptr(), out as *mut u32, 5);
    } else {
        call!(o.inner, 8, fn(usize), out);
    }
}

// États : description Direct3D 10 gardée à la création.
unsafe extern "win64" fn state_get_desc(this: usize, out: usize) {
    let o = obj(this);
    if out == 0 || o.desc.is_empty() {
        return;
    }
    let n = if o.kind == Kind::Blend {
        BLEND_DESC10
    } else {
        o.desc.len()
    };
    std::ptr::copy_nonoverlapping(o.desc.as_ptr(), out as *mut u8, n.min(o.desc.len()));
}
/// ID3D10BlendState1::GetDesc1 : D3D10_BLEND_DESC1, gardée après la
/// D3D10_BLEND_DESC.
unsafe extern "win64" fn blend_get_desc1(this: usize, out: usize) {
    let o = obj(this);
    if out != 0 && o.desc.len() == BLEND_DESC10 + BLEND_DESC1 {
        std::ptr::copy_nonoverlapping(o.desc[BLEND_DESC10..].as_ptr(), out as *mut u8, BLEND_DESC1);
    }
}

// Requêtes, prédicats, compteurs : Begin (7), End (8), GetData (9),
// GetDataSize (10), GetDesc (11).
unsafe extern "win64" fn async_begin(this: usize) {
    let o = obj(this);
    if !o.dev.is_null() {
        call!((*o.dev).ctx11, 27, fn(usize), o.inner);
    }
}
unsafe extern "win64" fn async_end(this: usize) {
    let o = obj(this);
    if !o.dev.is_null() {
        call!((*o.dev).ctx11, 28, fn(usize), o.inner);
    }
}
/// D3D10_QUERY de la requête (le compteur rend 4 octets).
fn query_type(o: &Obj) -> Option<u32> {
    if o.kind == Kind::Counter || o.desc.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes([
        o.desc[0], o.desc[1], o.desc[2], o.desc[3],
    ]))
}
unsafe extern "win64" fn async_get_data_size(this: usize) -> u32 {
    match query_type(obj(this)) {
        Some(0) | Some(5) | Some(7) => 4, // événement, prédicats : BOOL
        Some(1) | Some(2) => 8,           // occlusion, horodatage : UINT64
        Some(3) | Some(6) => 16,          // fréquence + discontinuité, flux de sortie
        Some(4) => 64,                    // D3D10_QUERY_DATA_PIPELINE_STATISTICS
        _ => 4,
    }
}
/// GetData : le rendu OpenGL est synchrone vu du programme ; une requête
/// finie rend des valeurs neutres (événement atteint, échantillons
/// visibles, horodatage en nanosecondes).
unsafe extern "win64" fn async_get_data(this: usize, data: usize, size: u32, _flags: u32) -> i32 {
    let o = obj(this);
    if !o.dev.is_null() {
        call!((*o.dev).ctx11, 111, fn(),);
    }
    if data == 0 || size == 0 {
        return S_OK;
    }
    let need = async_get_data_size(this);
    if size < need {
        return E_INVALIDARG;
    }
    let mut v = vec![0u8; need as usize];
    match query_type(o) {
        Some(0) | Some(5) => v[..4].copy_from_slice(&1u32.to_le_bytes()),
        Some(1) => v[..8].copy_from_slice(&1u64.to_le_bytes()),
        Some(2) => {
            let ns = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos() as u64);
            v[..8].copy_from_slice(&ns.to_le_bytes());
        }
        Some(3) => v[..8].copy_from_slice(&1_000_000_000u64.to_le_bytes()),
        _ => {}
    }
    std::ptr::copy_nonoverlapping(v.as_ptr(), data as *mut u8, v.len());
    S_OK
}

/// Table de la sorte `kind` pour l'ABI du programme.
fn table(kind: Kind) -> usize {
    let mut m: Vec<(usize, &str)> = vec![
        (obj_qi as *const () as usize, "ii"),
        (obj_addref as *const () as usize, ""),
        (obj_release as *const () as usize, ""),
        (obj_get_device as *const () as usize, "i"),
        (obj_get_private as *const () as usize, "iii"),
        (obj_set_private as *const () as usize, "iii"),
        (obj_set_private_iface as *const () as usize, "ii"),
    ];
    let resource = [
        (res_get_type as *const () as usize, "i"),
        (res_set_eviction as *const () as usize, "i"),
        (res_get_eviction as *const () as usize, ""),
    ];
    let name = match kind {
        Kind::Buffer => {
            m.extend(resource);
            m.extend([
                (buffer_map as *const () as usize, "iii"),
                (buffer_unmap as *const () as usize, ""),
                (buffer_get_desc as *const () as usize, "i"),
            ]);
            "ID3D10Buffer"
        }
        Kind::Texture1D | Kind::Texture2D | Kind::Texture3D => {
            m.extend(resource);
            let map = match kind {
                Kind::Texture1D => tex1d_map as *const () as usize,
                Kind::Texture2D => tex2d_map as *const () as usize,
                _ => tex3d_map as *const () as usize,
            };
            m.extend([
                (map, "iiii"),
                (tex_unmap as *const () as usize, "i"),
                (tex_get_desc as *const () as usize, "i"),
            ]);
            match kind {
                Kind::Texture1D => "ID3D10Texture1D",
                Kind::Texture2D => "ID3D10Texture2D",
                _ => "ID3D10Texture3D",
            }
        }
        Kind::Srv | Kind::Rtv | Kind::Dsv => {
            m.extend([
                (view_get_resource as *const () as usize, "i"),
                (view_get_desc as *const () as usize, "i"),
                // ID3D10ShaderResourceView1::GetDesc1 : même description.
                (view_get_desc as *const () as usize, "i"),
            ]);
            match kind {
                Kind::Srv => "ID3D10ShaderResourceView1",
                Kind::Rtv => "ID3D10RenderTargetView",
                _ => "ID3D10DepthStencilView",
            }
        }
        Kind::Blend => {
            m.extend([
                (state_get_desc as *const () as usize, "i"),
                (blend_get_desc1 as *const () as usize, "i"),
            ]);
            "ID3D10BlendState1"
        }
        Kind::DepthStencil | Kind::Rasterizer | Kind::Sampler => {
            m.push((state_get_desc as *const () as usize, "i"));
            match kind {
                Kind::DepthStencil => "ID3D10DepthStencilState",
                Kind::Rasterizer => "ID3D10RasterizerState",
                _ => "ID3D10SamplerState",
            }
        }
        Kind::Query | Kind::Predicate | Kind::Counter => {
            m.extend([
                (async_begin as *const () as usize, ""),
                (async_end as *const () as usize, ""),
                (async_get_data as *const () as usize, "iii"),
                (async_get_data_size as *const () as usize, ""),
                (state_get_desc as *const () as usize, "i"),
            ]);
            match kind {
                Kind::Counter => "ID3D10Counter",
                _ => "ID3D10Predicate",
            }
        }
        Kind::InputLayout => "ID3D10InputLayout",
        Kind::VertexShader => "ID3D10VertexShader",
        Kind::GeometryShader => "ID3D10GeometryShader",
        Kind::PixelShader => "ID3D10PixelShader",
    };
    vtable_sig(name, &m)
}

// --- Appareil -------------------------------------------------------------------------

/// ID3D10Device1 : appareil et contexte immédiat Direct3D 11 (références
/// tenues).
#[repr(C)]
struct Dev {
    vtbl: usize,
    refs: AtomicU32,
    dev11: usize,
    ctx11: usize,
    level: u32,
    flags: u32,
    filter: Mutex<(u32, u32)>,
    /// Fenêtres et rectangles de découpe tels que donnés (D3D10_VIEWPORT de
    /// 24 octets, D3D10_RECT de 16), rendus par RSGetViewports et
    /// RSGetScissorRects.
    raster: Mutex<(Vec<[u8; 24]>, Vec<[u8; 16]>)>,
}

static DEVICES: Mutex<Option<HashSet<usize>>> = Mutex::new(None);

fn is_device(p: usize) -> bool {
    DEVICES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|s| s.contains(&p))
}

unsafe fn dev<'a>(p: *mut Dev) -> &'a Dev {
    &*p
}

unsafe extern "win64" fn dev_qi(this: *mut Dev, riid: *const Guid, out: usize) -> i32 {
    if out == 0 {
        return E_INVALIDARG;
    }
    wrp(out, 0);
    let Some(r) = riid.as_ref() else {
        return E_INVALIDARG;
    };
    if *r == IID_IUNKNOWN || *r == IID_DEVICE || *r == IID_DEVICE1 {
        dev(this).refs.fetch_add(1, Ordering::AcqRel);
        wrp(out, this as usize);
        return S_OK;
    }
    if *r == IID_MULTITHREAD {
        return E_NOINTERFACE;
    }
    // IDXGIDevice et le reste : l'appareil Direct3D 11.
    let mut p = 0usize;
    let hr = call!(
        dev(this).dev11,
        0,
        fn(*const Guid, *mut usize) -> i32,
        riid,
        &mut p
    );
    if hr >= 0 {
        wrp(out, p);
    }
    hr
}
unsafe extern "win64" fn dev_addref(this: *mut Dev) -> u32 {
    dev(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn dev_release(this: *mut Dev) -> u32 {
    let prev = dev(this).refs.fetch_sub(1, Ordering::AcqRel);
    if prev != 1 {
        return prev - 1;
    }
    if let Some(s) = DEVICES.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        s.remove(&(this as usize));
    }
    let d = Box::from_raw(this);
    call!(d.ctx11, 2, fn() -> u32,);
    call!(d.dev11, 2, fn() -> u32,);
    0
}

/// Lit `n` objets du programme et les ouvre.
unsafe fn read_objs(pp: usize, n: u32) -> Vec<usize> {
    if pp == 0 {
        return Vec::new();
    }
    (0..n.min(MAX_ARRAY) as usize)
        .map(|i| un(rdp(pp + i * ps())))
        .collect()
}

fn arr(v: &[usize], pp: usize) -> *const usize {
    if pp == 0 {
        std::ptr::null()
    } else {
        v.as_ptr()
    }
}

/// Rend `v` (références Direct3D 11) au programme, enveloppés.
unsafe fn write_objs(pp: usize, v: &[usize], kind: Kind, d: *mut Dev) {
    for (i, &o) in v.iter().enumerate() {
        let w = if kind.is_resource() {
            wrap_resource(o, d)
        } else {
            wrap(o, kind, d, Vec::new())
        };
        wrp(pp + i * ps(), w);
    }
}

macro_rules! set_objs {
    ($name:ident, $idx:expr) => {
        unsafe extern "win64" fn $name(d: *mut Dev, start: u32, n: u32, pp: usize) {
            let v = read_objs(pp, n);
            let n = n.min(MAX_ARRAY);
            call!(
                dev(d).ctx11,
                $idx,
                fn(u32, u32, *const usize),
                start,
                n,
                arr(&v, pp)
            );
        }
    };
}
macro_rules! get_objs {
    ($name:ident, $idx:expr, $kind:expr) => {
        unsafe extern "win64" fn $name(d: *mut Dev, start: u32, n: u32, pp: usize) {
            if pp == 0 {
                return;
            }
            let n = n.min(MAX_ARRAY);
            let mut v = vec![0usize; n as usize];
            call!(
                dev(d).ctx11,
                $idx,
                fn(u32, u32, *mut usize),
                start,
                n,
                v.as_mut_ptr()
            );
            write_objs(pp, &v, $kind, d);
        }
    };
}
macro_rules! set_shader {
    ($name:ident, $idx:expr) => {
        unsafe extern "win64" fn $name(d: *mut Dev, s: usize) {
            call!(dev(d).ctx11, $idx, fn(usize, usize, u32), un(s), 0, 0);
        }
    };
}
macro_rules! get_shader {
    ($name:ident, $idx:expr, $kind:expr) => {
        unsafe extern "win64" fn $name(d: *mut Dev, out: usize) {
            let mut s = 0usize;
            let mut n = 0u32;
            call!(
                dev(d).ctx11,
                $idx,
                fn(*mut usize, usize, *mut u32),
                &mut s,
                0,
                &mut n
            );
            wrp(out, wrap(s, $kind, d, Vec::new()));
        }
    };
}

set_objs!(vs_set_cb, 7);
set_objs!(ps_set_srv, 8);
set_shader!(ps_set_shader, 9);
set_objs!(ps_set_samplers, 10);
set_shader!(vs_set_shader, 11);
set_objs!(ps_set_cb, 16);
set_objs!(gs_set_cb, 22);
set_shader!(gs_set_shader, 23);
set_objs!(vs_set_srv, 25);
set_objs!(vs_set_samplers, 26);
set_objs!(gs_set_srv, 31);
set_objs!(gs_set_samplers, 32);
get_objs!(vs_get_cb, 72, Kind::Buffer);
get_objs!(ps_get_srv, 73, Kind::Srv);
get_shader!(ps_get_shader, 74, Kind::PixelShader);
get_objs!(ps_get_samplers, 75, Kind::Sampler);
get_shader!(vs_get_shader, 76, Kind::VertexShader);
get_objs!(ps_get_cb, 77, Kind::Buffer);
get_objs!(gs_get_cb, 81, Kind::Buffer);
get_shader!(gs_get_shader, 82, Kind::GeometryShader);
get_objs!(vs_get_srv, 84, Kind::Srv);
get_objs!(vs_get_samplers, 85, Kind::Sampler);
get_objs!(gs_get_srv, 87, Kind::Srv);
get_objs!(gs_get_samplers, 88, Kind::Sampler);

unsafe extern "win64" fn draw_indexed(d: *mut Dev, count: u32, start: u32, base: i32) {
    call!(dev(d).ctx11, 12, fn(u32, u32, i32), count, start, base);
}
unsafe extern "win64" fn draw(d: *mut Dev, count: u32, start: u32) {
    call!(dev(d).ctx11, 13, fn(u32, u32), count, start);
}
unsafe extern "win64" fn ia_set_layout(d: *mut Dev, il: usize) {
    call!(dev(d).ctx11, 17, fn(usize), un(il));
}
unsafe extern "win64" fn ia_set_vbs(
    d: *mut Dev,
    start: u32,
    n: u32,
    pp: usize,
    strides: usize,
    offsets: usize,
) {
    let v = read_objs(pp, n);
    call!(
        dev(d).ctx11,
        18,
        fn(u32, u32, *const usize, usize, usize),
        start,
        n.min(MAX_ARRAY),
        arr(&v, pp),
        strides,
        offsets
    );
}
unsafe extern "win64" fn ia_set_ib(d: *mut Dev, b: usize, fmt: u32, off: u32) {
    call!(dev(d).ctx11, 19, fn(usize, u32, u32), un(b), fmt, off);
}
unsafe extern "win64" fn draw_indexed_instanced(
    d: *mut Dev,
    count: u32,
    inst: u32,
    start: u32,
    base: i32,
    start_inst: u32,
) {
    call!(
        dev(d).ctx11,
        20,
        fn(u32, u32, u32, i32, u32),
        count,
        inst,
        start,
        base,
        start_inst
    );
}
unsafe extern "win64" fn draw_instanced(d: *mut Dev, count: u32, inst: u32, start: u32, si: u32) {
    call!(
        dev(d).ctx11,
        21,
        fn(u32, u32, u32, u32),
        count,
        inst,
        start,
        si
    );
}
unsafe extern "win64" fn ia_set_topology(d: *mut Dev, t: u32) {
    call!(dev(d).ctx11, 24, fn(u32), t);
}
unsafe extern "win64" fn set_predication(d: *mut Dev, p: usize, value: i32) {
    call!(dev(d).ctx11, 30, fn(usize, i32), un(p), value);
}
unsafe extern "win64" fn om_set_rts(d: *mut Dev, n: u32, pp: usize, dsv: usize) {
    let v = read_objs(pp, n);
    call!(
        dev(d).ctx11,
        33,
        fn(u32, *const usize, usize),
        n.min(MAX_ARRAY),
        arr(&v, pp),
        un(dsv)
    );
}
unsafe extern "win64" fn om_set_blend(d: *mut Dev, s: usize, factor: usize, mask: u32) {
    call!(dev(d).ctx11, 35, fn(usize, usize, u32), un(s), factor, mask);
}
unsafe extern "win64" fn om_set_depth(d: *mut Dev, s: usize, r: u32) {
    call!(dev(d).ctx11, 36, fn(usize, u32), un(s), r);
}
unsafe extern "win64" fn so_set_targets(d: *mut Dev, n: u32, pp: usize, offsets: usize) {
    let v = read_objs(pp, n);
    call!(
        dev(d).ctx11,
        37,
        fn(u32, *const usize, usize),
        n.min(MAX_ARRAY),
        arr(&v, pp),
        offsets
    );
}
unsafe extern "win64" fn draw_auto(d: *mut Dev) {
    call!(dev(d).ctx11, 38, fn(),);
}
unsafe extern "win64" fn rs_set_state(d: *mut Dev, s: usize) {
    call!(dev(d).ctx11, 43, fn(usize), un(s));
}

/// D3D10_VIEWPORT (entiers) → D3D11_VIEWPORT (flottants).
unsafe extern "win64" fn rs_set_viewports(d: *mut Dev, n: u32, vps: usize) {
    let n = n.min(16);
    let mut v = vec![[0f32; 6]; n as usize];
    if vps != 0 {
        for (i, o) in v.iter_mut().enumerate() {
            let p = vps + i * 24;
            *o = [
                rdu(p) as i32 as f32,
                rdu(p + 4) as i32 as f32,
                rdu(p + 8) as f32,
                rdu(p + 12) as f32,
                f32::from_bits(rdu(p + 16)),
                f32::from_bits(rdu(p + 20)),
            ];
        }
    }
    call!(dev(d).ctx11, 44, fn(u32, *const [f32; 6]), n, v.as_ptr());
    dev(d).raster.lock().unwrap_or_else(|e| e.into_inner()).0 = if vps == 0 {
        Vec::new()
    } else {
        (0..n as usize)
            .map(|i| std::ptr::read_unaligned((vps + i * 24) as *const [u8; 24]))
            .collect()
    };
}
unsafe extern "win64" fn rs_set_scissors(d: *mut Dev, n: u32, rects: usize) {
    let n = n.min(16);
    call!(dev(d).ctx11, 45, fn(u32, usize), n, rects);
    dev(d).raster.lock().unwrap_or_else(|e| e.into_inner()).1 = if rects == 0 {
        Vec::new()
    } else {
        (0..n as usize)
            .map(|i| std::ptr::read_unaligned((rects + i * 16) as *const [u8; 16]))
            .collect()
    };
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn copy_region(
    d: *mut Dev,
    dst: usize,
    dsub: u32,
    x: u32,
    y: u32,
    z: u32,
    src: usize,
    ssub: u32,
    b: usize,
) {
    call!(
        dev(d).ctx11,
        46,
        fn(usize, u32, u32, u32, u32, usize, u32, usize),
        un(dst),
        dsub,
        x,
        y,
        z,
        un(src),
        ssub,
        b
    );
}
unsafe extern "win64" fn copy_resource(d: *mut Dev, dst: usize, src: usize) {
    call!(dev(d).ctx11, 47, fn(usize, usize), un(dst), un(src));
}
unsafe extern "win64" fn update_subresource(
    d: *mut Dev,
    dst: usize,
    sub: u32,
    b: usize,
    data: usize,
    row: u32,
    depth: u32,
) {
    call!(
        dev(d).ctx11,
        48,
        fn(usize, u32, usize, usize, u32, u32),
        un(dst),
        sub,
        b,
        data,
        row,
        depth
    );
}
unsafe extern "win64" fn clear_rtv(d: *mut Dev, v: usize, color: usize) {
    call!(dev(d).ctx11, 50, fn(usize, usize), un(v), color);
}
unsafe extern "win64" fn clear_dsv(d: *mut Dev, v: usize, flags: u32, depth: f32, stencil: u32) {
    call!(
        dev(d).ctx11,
        53,
        fn(usize, u32, f32, u32),
        un(v),
        flags,
        depth,
        stencil & 0xff
    );
}
unsafe extern "win64" fn generate_mips(d: *mut Dev, v: usize) {
    call!(dev(d).ctx11, 54, fn(usize), un(v));
}
unsafe extern "win64" fn resolve(
    d: *mut Dev,
    dst: usize,
    dsub: u32,
    src: usize,
    ssub: u32,
    f: u32,
) {
    call!(
        dev(d).ctx11,
        57,
        fn(usize, u32, usize, u32, u32),
        un(dst),
        dsub,
        un(src),
        ssub,
        f
    );
}
unsafe extern "win64" fn ia_get_layout(d: *mut Dev, out: usize) {
    let mut p = 0usize;
    call!(dev(d).ctx11, 78, fn(*mut usize), &mut p);
    wrp(out, wrap(p, Kind::InputLayout, d, Vec::new()));
}
unsafe extern "win64" fn ia_get_vbs(
    d: *mut Dev,
    start: u32,
    n: u32,
    pp: usize,
    strides: usize,
    offsets: usize,
) {
    let n = n.min(MAX_ARRAY);
    let mut v = vec![0usize; n as usize];
    call!(
        dev(d).ctx11,
        79,
        fn(u32, u32, *mut usize, usize, usize),
        start,
        n,
        v.as_mut_ptr(),
        strides,
        offsets
    );
    if pp != 0 {
        write_objs(pp, &v, Kind::Buffer, d);
    } else {
        for o in v.into_iter().filter(|&o| o != 0) {
            call!(o, 2, fn() -> u32,);
        }
    }
}
unsafe extern "win64" fn ia_get_ib(d: *mut Dev, out: usize, fmt: usize, off: usize) {
    let mut p = 0usize;
    call!(
        dev(d).ctx11,
        80,
        fn(*mut usize, usize, usize),
        &mut p,
        fmt,
        off
    );
    if out != 0 {
        wrp(out, wrap(p, Kind::Buffer, d, Vec::new()));
    } else if p != 0 {
        call!(p, 2, fn() -> u32,);
    }
}
unsafe extern "win64" fn ia_get_topology(d: *mut Dev, out: usize) {
    call!(dev(d).ctx11, 83, fn(usize), out);
}
unsafe extern "win64" fn get_predication(d: *mut Dev, out: usize, value: usize) {
    let mut p = 0usize;
    call!(dev(d).ctx11, 86, fn(*mut usize, usize), &mut p, value);
    if out != 0 {
        wrp(out, wrap(p, Kind::Predicate, d, Vec::new()));
    } else if p != 0 {
        call!(p, 2, fn() -> u32,);
    }
}
unsafe extern "win64" fn om_get_rts(d: *mut Dev, n: u32, pp: usize, dsv: usize) {
    let n = n.min(MAX_ARRAY);
    let mut v = vec![0usize; n as usize];
    let mut ds = 0usize;
    call!(
        dev(d).ctx11,
        89,
        fn(u32, *mut usize, *mut usize),
        n,
        if pp == 0 {
            std::ptr::null_mut()
        } else {
            v.as_mut_ptr()
        },
        if dsv == 0 {
            std::ptr::null_mut()
        } else {
            &mut ds
        }
    );
    if pp != 0 {
        write_objs(pp, &v, Kind::Rtv, d);
    }
    if dsv != 0 {
        wrp(dsv, wrap(ds, Kind::Dsv, d, Vec::new()));
    }
}
unsafe extern "win64" fn om_get_blend(d: *mut Dev, out: usize, factor: usize, mask: usize) {
    let mut p = 0usize;
    call!(
        dev(d).ctx11,
        91,
        fn(*mut usize, usize, usize),
        &mut p,
        factor,
        mask
    );
    if out != 0 {
        wrp(out, wrap(p, Kind::Blend, d, Vec::new()));
    } else if p != 0 {
        call!(p, 2, fn() -> u32,);
    }
}
unsafe extern "win64" fn om_get_depth(d: *mut Dev, out: usize, r: usize) {
    let mut p = 0usize;
    call!(dev(d).ctx11, 92, fn(*mut usize, usize), &mut p, r);
    if out != 0 {
        wrp(out, wrap(p, Kind::DepthStencil, d, Vec::new()));
    } else if p != 0 {
        call!(p, 2, fn() -> u32,);
    }
}
/// SOGetTargets : Direct3D 11 ne rend pas les décalages (mis à 0).
unsafe extern "win64" fn so_get_targets(d: *mut Dev, n: u32, pp: usize, offsets: usize) {
    let n = n.min(MAX_ARRAY);
    let mut v = vec![0usize; n as usize];
    call!(dev(d).ctx11, 93, fn(u32, *mut usize), n, v.as_mut_ptr());
    if pp != 0 {
        write_objs(pp, &v, Kind::Buffer, d);
    } else {
        for o in v.into_iter().filter(|&o| o != 0) {
            call!(o, 2, fn() -> u32,);
        }
    }
    if offsets != 0 {
        for i in 0..n as usize {
            wr32(offsets + 4 * i, 0);
        }
    }
}
unsafe extern "win64" fn rs_get_state(d: *mut Dev, out: usize) {
    let mut p = 0usize;
    call!(dev(d).ctx11, 94, fn(*mut usize), &mut p);
    if out != 0 {
        wrp(out, wrap(p, Kind::Rasterizer, d, Vec::new()));
    } else if p != 0 {
        call!(p, 2, fn() -> u32,);
    }
}
/// RSGetViewports : sans tableau, rend le nombre de fenêtres en place.
unsafe extern "win64" fn rs_get_viewports(d: *mut Dev, n: *mut u32, vps: usize) {
    if n.is_null() {
        return;
    }
    let r = dev(d).raster.lock().unwrap_or_else(|e| e.into_inner());
    *n = read_back(&r.0, *n, vps);
}
/// RSGetScissorRects : même convention.
unsafe extern "win64" fn rs_get_scissors(d: *mut Dev, n: *mut u32, rects: usize) {
    if n.is_null() {
        return;
    }
    let r = dev(d).raster.lock().unwrap_or_else(|e| e.into_inner());
    *n = read_back(&r.1, *n, rects);
}
/// Recopie au plus `room` entrées de `kept` dans `out` (cases au-delà mises
/// à zéro), ou rend seulement leur nombre quand `out` est nul.
unsafe fn read_back<const N: usize>(kept: &[[u8; N]], room: u32, out: usize) -> u32 {
    if out == 0 {
        return kept.len() as u32;
    }
    for i in 0..room as usize {
        let e = kept.get(i).copied().unwrap_or([0; N]);
        std::ptr::write_unaligned((out + i * N) as *mut [u8; N], e);
    }
    kept.len().min(room as usize) as u32
}
unsafe extern "win64" fn get_removed_reason(d: *mut Dev) -> i32 {
    call!(dev(d).dev11, 39, fn() -> i32,)
}
unsafe extern "win64" fn set_exception_mode(d: *mut Dev, m: u32) -> i32 {
    call!(dev(d).dev11, 41, fn(u32) -> i32, m)
}
unsafe extern "win64" fn get_exception_mode(d: *mut Dev) -> u32 {
    call!(dev(d).dev11, 42, fn() -> u32,)
}
unsafe extern "win64" fn dev_get_private(d: *mut Dev, g: usize, size: usize, data: usize) -> i32 {
    call!(
        dev(d).dev11,
        34,
        fn(usize, usize, usize) -> i32,
        g,
        size,
        data
    )
}
unsafe extern "win64" fn dev_set_private(d: *mut Dev, g: usize, size: u32, data: usize) -> i32 {
    call!(
        dev(d).dev11,
        35,
        fn(usize, u32, usize) -> i32,
        g,
        size,
        data
    )
}
unsafe extern "win64" fn dev_set_private_iface(d: *mut Dev, g: usize, iface: usize) -> i32 {
    call!(dev(d).dev11, 36, fn(usize, usize) -> i32, g, iface)
}
unsafe extern "win64" fn clear_state(d: *mut Dev) {
    call!(dev(d).ctx11, 110, fn(),);
    *dev(d).raster.lock().unwrap_or_else(|e| e.into_inner()) = (Vec::new(), Vec::new());
}
unsafe extern "win64" fn flush(d: *mut Dev) {
    call!(dev(d).ctx11, 111, fn(),);
}

// --- Création -------------------------------------------------------------------------

/// D3D10_SUBRESOURCE_DATA / D3D11_SUBRESOURCE_DATA (x64).
#[repr(C)]
#[derive(Clone, Copy)]
struct SubData {
    mem: usize,
    pitch: u32,
    slice: u32,
}

/// Données initiales dans la disposition x64 (`count` entrées).
unsafe fn init_data(p: usize, count: usize, keep: &mut Vec<SubData>) -> usize {
    if p == 0 || !x86() {
        return p;
    }
    *keep = (0..count.min(4096))
        .map(|i| {
            let q = p + 12 * i;
            SubData {
                mem: rd32(q) as usize,
                pitch: rd32(q + 4),
                slice: rd32(q + 8),
            }
        })
        .collect();
    keep.as_ptr() as usize
}

/// Nombre de niveaux de mip (0 : chaîne complète jusqu'à 1×1).
fn mips(levels: u32, dims: &[u32]) -> usize {
    if levels != 0 {
        return levels as usize;
    }
    let m = dims.iter().copied().max().unwrap_or(1).max(1);
    (32 - m.leading_zeros()) as usize
}

unsafe fn out_obj(
    out: usize,
    hr: i32,
    inner: usize,
    kind: Kind,
    d: *mut Dev,
    desc: Vec<u8>,
) -> i32 {
    if hr < 0 {
        return hr;
    }
    if out == 0 {
        if inner != 0 {
            call!(inner, 2, fn() -> u32,);
        }
        return S_FALSE;
    }
    wrp(out, wrap(inner, kind, d, desc));
    hr
}

/// D3D10_BUFFER_DESC (5 champs) → D3D11_BUFFER_DESC.
unsafe extern "win64" fn create_buffer(
    d: *mut Dev,
    desc: *const [u32; 5],
    init: usize,
    out: usize,
) -> i32 {
    let Some(b) = desc.as_ref() else {
        return E_INVALIDARG;
    };
    let d11 = [b[0], b[1], b[2], b[3], b[4] & 0x7, 0];
    let mut keep = Vec::new();
    let init = init_data(init, 1, &mut keep);
    let mut p = 0usize;
    let hr = call!(
        dev(d).dev11,
        3,
        fn(*const [u32; 6], usize, *mut usize) -> i32,
        &d11,
        init,
        if out == 0 {
            std::ptr::null_mut()
        } else {
            &mut p
        }
    );
    out_obj(out, hr, p, Kind::Buffer, d, Vec::new())
}
unsafe fn create_texture(
    d: *mut Dev,
    idx: usize,
    kind: Kind,
    desc: usize,
    init: usize,
    out: usize,
) -> i32 {
    if desc == 0 {
        return E_INVALIDARG;
    }
    let u = |i: usize| rdu(desc + 4 * i);
    let count = match kind {
        Kind::Texture1D => mips(u(1), &[u(0)]) * u(2).max(1) as usize,
        Kind::Texture2D => mips(u(2), &[u(0), u(1)]) * u(3).max(1) as usize,
        _ => mips(u(3), &[u(0), u(1), u(2)]),
    };
    let mut keep = Vec::new();
    let init = init_data(init, count, &mut keep);
    let mut p = 0usize;
    let hr = call!(
        dev(d).dev11,
        idx,
        fn(usize, usize, *mut usize) -> i32,
        desc,
        init,
        if out == 0 {
            std::ptr::null_mut()
        } else {
            &mut p
        }
    );
    out_obj(out, hr, p, kind, d, Vec::new())
}
unsafe extern "win64" fn create_tex1d(d: *mut Dev, desc: usize, init: usize, out: usize) -> i32 {
    create_texture(d, 4, Kind::Texture1D, desc, init, out)
}
unsafe extern "win64" fn create_tex2d(d: *mut Dev, desc: usize, init: usize, out: usize) -> i32 {
    create_texture(d, 5, Kind::Texture2D, desc, init, out)
}
unsafe extern "win64" fn create_tex3d(d: *mut Dev, desc: usize, init: usize, out: usize) -> i32 {
    create_texture(d, 6, Kind::Texture3D, desc, init, out)
}
unsafe fn create_view(
    d: *mut Dev,
    idx: usize,
    kind: Kind,
    res: usize,
    desc: usize,
    out: usize,
) -> i32 {
    let mut p = 0usize;
    let hr = call!(
        dev(d).dev11,
        idx,
        fn(usize, usize, *mut usize) -> i32,
        un(res),
        desc,
        if out == 0 {
            std::ptr::null_mut()
        } else {
            &mut p
        }
    );
    out_obj(out, hr, p, kind, d, Vec::new())
}
unsafe extern "win64" fn create_srv(d: *mut Dev, res: usize, desc: usize, out: usize) -> i32 {
    create_view(d, 7, Kind::Srv, res, desc, out)
}
unsafe extern "win64" fn create_rtv(d: *mut Dev, res: usize, desc: usize, out: usize) -> i32 {
    create_view(d, 9, Kind::Rtv, res, desc, out)
}
/// D3D10_DEPTH_STENCIL_VIEW_DESC : pas de `Flags` (D3D11, après la dimension).
unsafe extern "win64" fn create_dsv(d: *mut Dev, res: usize, desc: usize, out: usize) -> i32 {
    let d11;
    let p = if desc == 0 {
        0
    } else {
        let u = |i: usize| rdu(desc + 4 * i);
        d11 = [u(0), u(1), 0, u(2), u(3), u(4)];
        d11.as_ptr() as usize
    };
    create_view(d, 10, Kind::Dsv, res, p, out)
}

/// D3D10_INPUT_ELEMENT_DESC : même disposition que D3D11 ; en x86, nom de
/// sémantique sur 4 octets (28 octets par élément).
unsafe extern "win64" fn create_input_layout(
    d: *mut Dev,
    elems: usize,
    n: u32,
    code: usize,
    len: usize,
    out: usize,
) -> i32 {
    let keep: Vec<[u32; 8]>;
    let elems = if x86() && elems != 0 {
        keep = (0..n.min(64) as usize)
            .map(|i| {
                let p = elems + 28 * i;
                let name = rd32(p) as u64;
                [
                    name as u32,
                    (name >> 32) as u32,
                    rd32(p + 4),
                    rd32(p + 8),
                    rd32(p + 12),
                    rd32(p + 16),
                    rd32(p + 20),
                    rd32(p + 24),
                ]
            })
            .collect();
        keep.as_ptr() as usize
    } else {
        elems
    };
    let mut p = 0usize;
    let hr = call!(
        dev(d).dev11,
        11,
        fn(usize, u32, usize, usize, *mut usize) -> i32,
        elems,
        n.min(64),
        code,
        len,
        if out == 0 {
            std::ptr::null_mut()
        } else {
            &mut p
        }
    );
    out_obj(out, hr, p, Kind::InputLayout, d, Vec::new())
}
unsafe fn create_shader(
    d: *mut Dev,
    idx: usize,
    kind: Kind,
    code: usize,
    len: usize,
    out: usize,
) -> i32 {
    let mut p = 0usize;
    let hr = call!(
        dev(d).dev11,
        idx,
        fn(usize, usize, usize, *mut usize) -> i32,
        code,
        len,
        0,
        if out == 0 {
            std::ptr::null_mut()
        } else {
            &mut p
        }
    );
    out_obj(out, hr, p, kind, d, Vec::new())
}
unsafe extern "win64" fn create_vs(d: *mut Dev, code: usize, len: usize, out: usize) -> i32 {
    create_shader(d, 12, Kind::VertexShader, code, len, out)
}
unsafe extern "win64" fn create_gs(d: *mut Dev, code: usize, len: usize, out: usize) -> i32 {
    create_shader(d, 13, Kind::GeometryShader, code, len, out)
}
unsafe extern "win64" fn create_ps(d: *mut Dev, code: usize, len: usize, out: usize) -> i32 {
    create_shader(d, 15, Kind::PixelShader, code, len, out)
}

/// D3D11_SO_DECLARATION_ENTRY (x64).
#[repr(C)]
#[derive(Clone, Copy)]
struct SoEntry11 {
    stream: u32,
    name: usize,
    index: u32,
    start: u8,
    count: u8,
    slot: u8,
}

/// D3D10_SO_DECLARATION_ENTRY (sans `Stream`) → D3D11, flux 0, un seul
/// pas de tampon.
unsafe extern "win64" fn create_gs_so(
    d: *mut Dev,
    code: usize,
    len: usize,
    decl: usize,
    n: u32,
    stride: u32,
    out: usize,
) -> i32 {
    let size = if x86() { 12 } else { 16 };
    let entries: Vec<SoEntry11> = (0..n.min(64) as usize)
        .map(|i| {
            let p = decl + size * i;
            let q = p + ps();
            SoEntry11 {
                stream: 0,
                name: rdp(p),
                index: rd32(q),
                start: *((q + 4) as *const u8),
                count: *((q + 5) as *const u8),
                slot: *((q + 6) as *const u8),
            }
        })
        .collect();
    let strides = [stride];
    let mut p = 0usize;
    let hr = call!(
        dev(d).dev11,
        14,
        fn(usize, usize, *const SoEntry11, u32, *const u32, u32, u32, usize, *mut usize) -> i32,
        code,
        len,
        if decl == 0 {
            std::ptr::null()
        } else {
            entries.as_ptr()
        },
        entries.len() as u32,
        strides.as_ptr(),
        1,
        0,
        0,
        if out == 0 {
            std::ptr::null_mut()
        } else {
            &mut p
        }
    );
    out_obj(out, hr, p, Kind::GeometryShader, d, Vec::new())
}

/// Taille de D3D10_BLEND_DESC et de D3D10_BLEND_DESC1 (= D3D11_BLEND_DESC).
const BLEND_DESC10: usize = 68;
const BLEND_DESC1: usize = 264;

/// D3D10_BLEND_DESC → D3D10_BLEND_DESC1 (une cible par entrée, mêmes
/// fonctions de mélange).
fn blend_desc1_of(b: &[u8; BLEND_DESC10]) -> [u8; BLEND_DESC1] {
    let u = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let mut out = [0u8; BLEND_DESC1];
    out[..4].copy_from_slice(&u(0).to_le_bytes()); // AlphaToCoverageEnable
    out[4..8].copy_from_slice(&1u32.to_le_bytes()); // IndependentBlendEnable
    for i in 0..8 {
        let rt = 8 + 32 * i;
        let fields = [u(4 + 4 * i), u(36), u(40), u(44), u(48), u(52), u(56)];
        for (k, f) in fields.iter().enumerate() {
            out[rt + 4 * k..rt + 4 * k + 4].copy_from_slice(&f.to_le_bytes());
        }
        out[rt + 28] = b[60 + i];
    }
    out
}

/// D3D10_BLEND_DESC1 → D3D10_BLEND_DESC (fonctions de la cible 0).
fn blend_desc_of1(b: &[u8; BLEND_DESC1]) -> [u8; BLEND_DESC10] {
    let u = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let independent = u(4) != 0;
    let mut out = [0u8; BLEND_DESC10];
    out[..4].copy_from_slice(&u(0).to_le_bytes());
    for i in 0..8 {
        let rt = 8 + 32 * if independent { i } else { 0 };
        out[4 + 4 * i..8 + 4 * i].copy_from_slice(&u(rt).to_le_bytes());
        out[60 + i] = b[rt + 28];
    }
    for k in 0..6 {
        out[36 + 4 * k..40 + 4 * k].copy_from_slice(&u(12 + 4 * k).to_le_bytes());
    }
    out
}

unsafe fn create_blend_from1(
    d: *mut Dev,
    d1: [u8; BLEND_DESC1],
    d10: [u8; BLEND_DESC10],
    out: usize,
) -> i32 {
    let mut p = 0usize;
    let hr = call!(
        dev(d).dev11,
        20,
        fn(*const u8, *mut usize) -> i32,
        d1.as_ptr(),
        if out == 0 {
            std::ptr::null_mut()
        } else {
            &mut p
        }
    );
    let mut keep = d10.to_vec();
    keep.extend_from_slice(&d1);
    out_obj(out, hr, p, Kind::Blend, d, keep)
}
unsafe extern "win64" fn create_blend(d: *mut Dev, desc: usize, out: usize) -> i32 {
    if desc == 0 {
        return E_INVALIDARG;
    }
    let mut b = [0u8; BLEND_DESC10];
    std::ptr::copy_nonoverlapping(desc as *const u8, b.as_mut_ptr(), BLEND_DESC10);
    create_blend_from1(d, blend_desc1_of(&b), b, out)
}
unsafe extern "win64" fn create_blend1(d: *mut Dev, desc: usize, out: usize) -> i32 {
    if desc == 0 {
        return E_INVALIDARG;
    }
    let mut b = [0u8; BLEND_DESC1];
    std::ptr::copy_nonoverlapping(desc as *const u8, b.as_mut_ptr(), BLEND_DESC1);
    create_blend_from1(d, b, blend_desc_of1(&b), out)
}

/// États de même disposition en Direct3D 10 et 11 : la description est
/// gardée pour GetDesc.
unsafe fn create_state(
    d: *mut Dev,
    idx: usize,
    kind: Kind,
    desc: usize,
    size: usize,
    out: usize,
) -> i32 {
    if desc == 0 {
        return E_INVALIDARG;
    }
    let keep = std::slice::from_raw_parts(desc as *const u8, size).to_vec();
    let mut p = 0usize;
    let hr = call!(
        dev(d).dev11,
        idx,
        fn(usize, *mut usize) -> i32,
        desc,
        if out == 0 {
            std::ptr::null_mut()
        } else {
            &mut p
        }
    );
    out_obj(out, hr, p, kind, d, keep)
}
unsafe extern "win64" fn create_depth(d: *mut Dev, desc: usize, out: usize) -> i32 {
    create_state(d, 21, Kind::DepthStencil, desc, 52, out)
}
unsafe extern "win64" fn create_raster(d: *mut Dev, desc: usize, out: usize) -> i32 {
    create_state(d, 22, Kind::Rasterizer, desc, 40, out)
}
unsafe extern "win64" fn create_sampler(d: *mut Dev, desc: usize, out: usize) -> i32 {
    create_state(d, 23, Kind::Sampler, desc, 52, out)
}
unsafe extern "win64" fn create_query(d: *mut Dev, desc: usize, out: usize) -> i32 {
    create_state(d, 24, Kind::Query, desc, 8, out)
}
unsafe extern "win64" fn create_predicate(d: *mut Dev, desc: usize, out: usize) -> i32 {
    create_state(d, 25, Kind::Predicate, desc, 8, out)
}
unsafe extern "win64" fn create_counter(d: *mut Dev, desc: usize, out: usize) -> i32 {
    create_state(d, 26, Kind::Counter, desc, 8, out)
}
unsafe extern "win64" fn check_format(d: *mut Dev, f: u32, out: usize) -> i32 {
    call!(dev(d).dev11, 29, fn(u32, usize) -> i32, f, out)
}
unsafe extern "win64" fn check_ms(d: *mut Dev, f: u32, n: u32, out: usize) -> i32 {
    call!(dev(d).dev11, 30, fn(u32, u32, usize) -> i32, f, n, out)
}
unsafe extern "win64" fn check_counter_info(d: *mut Dev, out: usize) {
    call!(dev(d).dev11, 31, fn(usize), out);
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn check_counter(
    d: *mut Dev,
    desc: usize,
    ty: usize,
    active: usize,
    name: usize,
    name_len: usize,
    units: usize,
    units_len: usize,
    descr: usize,
    descr_len: usize,
) -> i32 {
    call!(
        dev(d).dev11,
        32,
        fn(usize, usize, usize, usize, usize, usize, usize, usize, usize) -> i32,
        desc,
        ty,
        active,
        name,
        name_len,
        units,
        units_len,
        descr,
        descr_len
    )
}
unsafe extern "win64" fn get_creation_flags(d: *mut Dev) -> u32 {
    dev(d).flags
}
unsafe extern "win64" fn open_shared(_: *mut Dev, _: usize, _: usize, out: usize) -> i32 {
    wrp(out, 0);
    E_NOTIMPL
}
unsafe extern "win64" fn set_filter_size(d: *mut Dev, w: u32, h: u32) {
    *dev(d).filter.lock().unwrap_or_else(|e| e.into_inner()) = (w, h);
}
unsafe extern "win64" fn get_filter_size(d: *mut Dev, w: *mut u32, h: *mut u32) {
    let (fw, fh) = *dev(d).filter.lock().unwrap_or_else(|e| e.into_inner());
    if !w.is_null() {
        *w = fw;
    }
    if !h.is_null() {
        *h = fh;
    }
}
/// ID3D10Device1::CreateShaderResourceView1 : D3D10_SHADER_RESOURCE_VIEW_DESC1
/// a la disposition de D3D11_SHADER_RESOURCE_VIEW_DESC.
unsafe extern "win64" fn create_srv1(d: *mut Dev, res: usize, desc: usize, out: usize) -> i32 {
    create_view(d, 7, Kind::Srv, res, desc, out)
}
unsafe extern "win64" fn get_feature_level(d: *mut Dev) -> u32 {
    dev(d).level
}

/// ID3D10Device1 : 98 méthodes de ID3D10Device, puis 3.
fn device_table() -> usize {
    let m: [(usize, &str); 101] = [
        (dev_qi as *const () as usize, "ii"),
        (dev_addref as *const () as usize, ""),
        (dev_release as *const () as usize, ""),
        (vs_set_cb as *const () as usize, "iii"),
        (ps_set_srv as *const () as usize, "iii"),
        (ps_set_shader as *const () as usize, "i"),
        (ps_set_samplers as *const () as usize, "iii"),
        (vs_set_shader as *const () as usize, "i"),
        (draw_indexed as *const () as usize, "iii"),
        (draw as *const () as usize, "ii"),
        (ps_set_cb as *const () as usize, "iii"),
        (ia_set_layout as *const () as usize, "i"),
        (ia_set_vbs as *const () as usize, "iiiii"),
        (ia_set_ib as *const () as usize, "iii"),
        (draw_indexed_instanced as *const () as usize, "iiiii"),
        (draw_instanced as *const () as usize, "iiii"),
        (gs_set_cb as *const () as usize, "iii"),
        (gs_set_shader as *const () as usize, "i"),
        (ia_set_topology as *const () as usize, "i"),
        (vs_set_srv as *const () as usize, "iii"),
        (vs_set_samplers as *const () as usize, "iii"),
        (set_predication as *const () as usize, "ii"),
        (gs_set_srv as *const () as usize, "iii"),
        (gs_set_samplers as *const () as usize, "iii"),
        (om_set_rts as *const () as usize, "iii"),
        (om_set_blend as *const () as usize, "iii"),
        (om_set_depth as *const () as usize, "ii"),
        (so_set_targets as *const () as usize, "iii"),
        (draw_auto as *const () as usize, ""),
        (rs_set_state as *const () as usize, "i"),
        (rs_set_viewports as *const () as usize, "ii"),
        (rs_set_scissors as *const () as usize, "ii"),
        (copy_region as *const () as usize, "iiiiiiii"),
        (copy_resource as *const () as usize, "ii"),
        (update_subresource as *const () as usize, "iiiiii"),
        (clear_rtv as *const () as usize, "ii"),
        (clear_dsv as *const () as usize, "iifi"),
        (generate_mips as *const () as usize, "i"),
        (resolve as *const () as usize, "iiiii"),
        (vs_get_cb as *const () as usize, "iii"),
        (ps_get_srv as *const () as usize, "iii"),
        (ps_get_shader as *const () as usize, "i"),
        (ps_get_samplers as *const () as usize, "iii"),
        (vs_get_shader as *const () as usize, "i"),
        (ps_get_cb as *const () as usize, "iii"),
        (ia_get_layout as *const () as usize, "i"),
        (ia_get_vbs as *const () as usize, "iiiii"),
        (ia_get_ib as *const () as usize, "iii"),
        (gs_get_cb as *const () as usize, "iii"),
        (gs_get_shader as *const () as usize, "i"),
        (ia_get_topology as *const () as usize, "i"),
        (vs_get_srv as *const () as usize, "iii"),
        (vs_get_samplers as *const () as usize, "iii"),
        (get_predication as *const () as usize, "ii"),
        (gs_get_srv as *const () as usize, "iii"),
        (gs_get_samplers as *const () as usize, "iii"),
        (om_get_rts as *const () as usize, "iii"),
        (om_get_blend as *const () as usize, "iii"),
        (om_get_depth as *const () as usize, "ii"),
        (so_get_targets as *const () as usize, "iii"),
        (rs_get_state as *const () as usize, "i"),
        (rs_get_viewports as *const () as usize, "ii"),
        (rs_get_scissors as *const () as usize, "ii"),
        (get_removed_reason as *const () as usize, ""),
        (set_exception_mode as *const () as usize, "i"),
        (get_exception_mode as *const () as usize, ""),
        (dev_get_private as *const () as usize, "iii"),
        (dev_set_private as *const () as usize, "iii"),
        (dev_set_private_iface as *const () as usize, "ii"),
        (clear_state as *const () as usize, ""),
        (flush as *const () as usize, ""),
        (create_buffer as *const () as usize, "iii"),
        (create_tex1d as *const () as usize, "iii"),
        (create_tex2d as *const () as usize, "iii"),
        (create_tex3d as *const () as usize, "iii"),
        (create_srv as *const () as usize, "iii"),
        (create_rtv as *const () as usize, "iii"),
        (create_dsv as *const () as usize, "iii"),
        (create_input_layout as *const () as usize, "iiiii"),
        (create_vs as *const () as usize, "iii"),
        (create_gs as *const () as usize, "iii"),
        (create_gs_so as *const () as usize, "iiiiii"),
        (create_ps as *const () as usize, "iii"),
        (create_blend as *const () as usize, "ii"),
        (create_depth as *const () as usize, "ii"),
        (create_raster as *const () as usize, "ii"),
        (create_sampler as *const () as usize, "ii"),
        (create_query as *const () as usize, "ii"),
        (create_predicate as *const () as usize, "ii"),
        (create_counter as *const () as usize, "ii"),
        (check_format as *const () as usize, "ii"),
        (check_ms as *const () as usize, "iii"),
        (check_counter_info as *const () as usize, "i"),
        (check_counter as *const () as usize, "iiiiiiiii"),
        (get_creation_flags as *const () as usize, ""),
        (open_shared as *const () as usize, "iii"),
        (set_filter_size as *const () as usize, "ii"),
        (get_filter_size as *const () as usize, "ii"),
        (create_srv1 as *const () as usize, "iii"),
        (create_blend1 as *const () as usize, "ii"),
        (get_feature_level as *const () as usize, ""),
    ];
    vtable_sig("ID3D10Device1", &m)
}

/// Crée l'appareil Direct3D 10 sur un appareil Direct3D 11 de niveau
/// `level`.
unsafe fn create_device(flags: u32, level: u32) -> Result<*mut Dev, i32> {
    let levels = [level];
    let (mut d11, mut ctx) = (0usize, 0usize);
    let hr = crate::dlls::d3d11::D3D11CreateDevice(
        0,
        1,
        0,
        flags,
        levels.as_ptr(),
        1,
        7,
        &mut d11,
        std::ptr::null_mut(),
        &mut ctx,
    );
    if hr < 0 || d11 == 0 || ctx == 0 {
        return Err(if hr < 0 { hr } else { E_INVALIDARG });
    }
    let d = Box::into_raw(Box::new(Dev {
        vtbl: device_table(),
        refs: AtomicU32::new(1),
        dev11: d11,
        ctx11: ctx,
        level,
        flags,
        filter: Mutex::new((0, 0)),
        raster: Mutex::new((Vec::new(), Vec::new())),
    }));
    DEVICES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashSet::new)
        .insert(d as usize);
    crate::log_info!(
        "d3d10",
        "appareil Direct3D 10 (niveau 0x{level:x}) sur Direct3D 11"
    );
    Ok(d)
}

unsafe fn make_swap_chain(d: *mut Dev, desc: usize, out: *mut usize) {
    if desc == 0 || out.is_null() {
        return;
    }
    let x86_desc;
    let sd = if x86() {
        x86_desc = crate::dlls::dx11_x86::swap_desc_in(desc);
        &x86_desc
    } else {
        &*(desc as *const crate::dlls::dxgi::DxgiSwapChainDesc)
    };
    *out = crate::dlls::dxgi::create_swap_chain(sd, d as *mut std::ffi::c_void) as usize;
}

// --- Exports --------------------------------------------------------------------------

pub unsafe extern "win64" fn D3D10CreateDevice(
    _adapter: usize,
    _driver: u32,
    _software: usize,
    flags: u32,
    _sdk: u32,
    device: *mut usize,
) -> i32 {
    match create_device(flags, LEVEL_10_0) {
        Ok(d) => {
            if device.is_null() {
                dev_release(d);
            } else {
                *device = d as usize;
            }
            S_OK
        }
        Err(hr) => hr,
    }
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn D3D10CreateDeviceAndSwapChain(
    _adapter: usize,
    _driver: u32,
    _software: usize,
    flags: u32,
    _sdk: u32,
    desc: usize,
    swap_chain: *mut usize,
    device: *mut usize,
) -> i32 {
    let d = match create_device(flags, LEVEL_10_0) {
        Ok(d) => d,
        Err(hr) => return hr,
    };
    make_swap_chain(d, desc, swap_chain);
    if device.is_null() {
        dev_release(d);
    } else {
        *device = d as usize;
    }
    S_OK
}

/// Niveau 10.1 ou 10.0 (9.x est servi en 10.0).
fn level1(asked: u32) -> u32 {
    if asked >= LEVEL_10_1 {
        LEVEL_10_1
    } else {
        LEVEL_10_0
    }
}

pub unsafe extern "win64" fn D3D10CreateDevice1(
    _adapter: usize,
    _driver: u32,
    _software: usize,
    flags: u32,
    level: u32,
    _sdk: u32,
    device: *mut usize,
) -> i32 {
    match create_device(flags, level1(level)) {
        Ok(d) => {
            if device.is_null() {
                dev_release(d);
            } else {
                *device = d as usize;
            }
            S_OK
        }
        Err(hr) => hr,
    }
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn D3D10CreateDeviceAndSwapChain1(
    _adapter: usize,
    _driver: u32,
    _software: usize,
    flags: u32,
    level: u32,
    _sdk: u32,
    desc: usize,
    swap_chain: *mut usize,
    device: *mut usize,
) -> i32 {
    let d = match create_device(flags, level1(level)) {
        Ok(d) => d,
        Err(hr) => return hr,
    };
    make_swap_chain(d, desc, swap_chain);
    if device.is_null() {
        dev_release(d);
    } else {
        *device = d as usize;
    }
    S_OK
}

/// Profils de shaders du niveau 10.0.
static VS_PROFILE: &[u8] = b"vs_4_0\0";
static PS_PROFILE: &[u8] = b"ps_4_0\0";
static GS_PROFILE: &[u8] = b"gs_4_0\0";

pub unsafe extern "win64" fn D3D10GetVertexShaderProfile(_device: usize) -> *const u8 {
    VS_PROFILE.as_ptr()
}
pub unsafe extern "win64" fn D3D10GetPixelShaderProfile(_device: usize) -> *const u8 {
    PS_PROFILE.as_ptr()
}
pub unsafe extern "win64" fn D3D10GetGeometryShaderProfile(_device: usize) -> *const u8 {
    GS_PROFILE.as_ptr()
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "D3D10CreateDevice" => fn D3D10CreateDevice,
        "D3D10CreateDeviceAndSwapChain" => fn D3D10CreateDeviceAndSwapChain,
        "D3D10GetVertexShaderProfile" => fn D3D10GetVertexShaderProfile,
        "D3D10GetPixelShaderProfile" => fn D3D10GetPixelShaderProfile,
        "D3D10GetGeometryShaderProfile" => fn D3D10GetGeometryShaderProfile,
    }
}

pub fn exports_1() -> crate::dlls::Exports {
    crate::exports! {
        "D3D10CreateDevice1" => fn D3D10CreateDevice1,
        "D3D10CreateDeviceAndSwapChain1" => fn D3D10CreateDeviceAndSwapChain1,
        "D3D10GetVertexShaderProfile" => fn D3D10GetVertexShaderProfile,
        "D3D10GetPixelShaderProfile" => fn D3D10GetPixelShaderProfile,
        "D3D10GetGeometryShaderProfile" => fn D3D10GetGeometryShaderProfile,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_descs_round_trip() {
        let mut b = [0u8; BLEND_DESC10];
        b[0] = 1; // AlphaToCoverage
        b[4 + 4] = 1; // cible 1 : mélange
        for (k, v) in [5u32, 6, 1, 2, 1, 1].iter().enumerate() {
            b[36 + 4 * k..40 + 4 * k].copy_from_slice(&v.to_le_bytes());
        }
        b[60] = 0xf;
        b[61] = 0x3;
        let d1 = blend_desc1_of(&b);
        assert_eq!(&d1[8 + 32..8 + 32 + 4], &1u32.to_le_bytes());
        assert_eq!(&d1[8 + 4..8 + 8], &5u32.to_le_bytes());
        assert_eq!(d1[8 + 28], 0xf);
        assert_eq!(d1[8 + 32 + 28], 0x3);
        assert_eq!(blend_desc_of1(&d1), b);
    }

    #[test]
    fn mip_chain_counts() {
        assert_eq!(mips(0, &[256, 64]), 9);
        assert_eq!(mips(0, &[1]), 1);
        assert_eq!(mips(3, &[256]), 3);
    }

    #[test]
    fn so_entry_has_the_d3d11_layout() {
        assert_eq!(std::mem::size_of::<SoEntry11>(), 24);
        assert_eq!(std::mem::size_of::<SubData>(), 16);
        assert_eq!(std::mem::size_of::<Mapped>(), 16);
    }

    #[test]
    fn device_table_has_101_methods() {
        // La table win64 est un tableau de 101 fonctions. On vérifie la
        // dernière par son effet, pas par son adresse : en --release, deux
        // références à une même fonction peuvent avoir des adresses
        // différentes (#1218).
        let t = device_table() as *const usize;
        let entries = unsafe { std::slice::from_raw_parts(t, 101) };
        assert!(entries.iter().all(|&f| f != 0));
        let mut d = Dev {
            vtbl: t as usize,
            refs: AtomicU32::new(1),
            dev11: 0,
            ctx11: 0,
            level: 0xa100,
            flags: 0,
            filter: Mutex::new((0, 0)),
            raster: Mutex::new((Vec::new(), Vec::new())),
        };
        let level: unsafe extern "win64" fn(*mut Dev) -> u32 =
            unsafe { std::mem::transmute(entries[100]) };
        assert_eq!(unsafe { level(&mut d) }, 0xa100);
    }
}
