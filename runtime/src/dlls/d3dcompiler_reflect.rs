//! `D3DReflect` de d3dcompiler_47 : `ID3D11ShaderReflection` sur un
//! conteneur DXBC (#1247). ComputeSharp.D2D1, et donc Paint.NET, le lit pour
//! chaque effet Direct2D personnalisé (`D2D1ReflectionServices.GetShaderInfo`).
//!
//! Sources lues dans le conteneur :
//! - `RDEF` : nom du compilateur, tampons de constantes et liaisons ;
//! - `ISGN`/`OSGN` (et `ISG1`/`OSG1`/`OSG5`) : signatures ;
//! - `SHDR`/`SHEX` : version, déclarations et compteurs d'instructions ;
//! - `SFI0` : options requises (`GetRequiresFlags`) ;
//! - `Aon9` : shader `*_level_9_*`, niveau minimal 9_1.
//!
//! Les dispositions des structures rendues (D3D11_SHADER_DESC,
//! D3D11_SIGNATURE_PARAMETER_DESC, D3D11_SHADER_INPUT_BIND_DESC,
//! D3D11_SHADER_BUFFER_DESC) suivent d3d11shader.h de MinGW-w64 (domaine
//! public). Elles sont écrites par décalage, en 64 bits.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::dlls::ole32::Guid;

const S_OK: i32 = 0;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;

/// IID_ID3D11ShaderReflection de d3dcompiler_47 (8d536ca1-…) et des
/// versions 43 et antérieures (0a233719-…).
const IID_REFLECTION_47: u32 = 0x8d53_6ca1;
const IID_REFLECTION_43: u32 = 0x0a23_3719;

/// Table de méthodes COM (pointeurs de fonctions win64).
#[repr(transparent)]
struct Vt<const N: usize>([*const (); N]);
unsafe impl<const N: usize> Sync for Vt<N> {}

fn rd(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4)
        .map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
        .unwrap_or(0)
}

/// Pointeur vers une chaîne terminée par NUL dans `data`, ou "" si le
/// décalage sort du bloc.
fn str_at(data: &[u8], off: u32) -> *const u8 {
    match data.get(off as usize..) {
        Some(rest) if rest.contains(&0) => rest.as_ptr(),
        _ => c"".as_ptr().cast(),
    }
}

/// Élément de signature (D3D11_SIGNATURE_PARAMETER_DESC).
#[derive(Clone, Copy)]
struct Param {
    name: *const u8,
    index: u32,
    register: u32,
    system_value: u32,
    component_type: u32,
    mask: u8,
    rw_mask: u8,
    stream: u32,
    min_precision: u32,
}

/// Liaison de ressource (D3D11_SHADER_INPUT_BIND_DESC).
#[derive(Clone, Copy)]
struct Binding {
    name: *const u8,
    ty: u32,
    bind_point: u32,
    bind_count: u32,
    flags: u32,
    return_type: u32,
    dimension: u32,
    samples: u32,
}

/// Tampon de constantes (D3D11_SHADER_BUFFER_DESC).
#[derive(Clone, Copy)]
struct CBuffer {
    name: *const u8,
    ty: u32,
    variables: u32,
    size: u32,
    flags: u32,
}

#[derive(Default)]
struct Counts {
    instructions: u32,
    declarations: u32,
    temps: u32,
    temp_arrays: u32,
    mov: u32,
    movc: u32,
    conversion: u32,
    bitwise: u32,
    float: u32,
    int: u32,
    uint: u32,
    texture_normal: u32,
    texture_load: u32,
    texture_store: u32,
    static_flow: u32,
    dynamic_flow: u32,
    emit: u32,
    cut: u32,
    barrier: u32,
    interlocked: u32,
    sample_frequency: bool,
    thread_group: [u32; 3],
}

#[repr(C)]
struct Reflection {
    vtable: *const Vt<22>,
    refs: AtomicU32,
    /// Copie du conteneur : les chaînes rendues pointent dedans.
    blob: Vec<u8>,
    version: u32,
    creator: *const u8,
    flags: u32,
    inputs: Vec<Param>,
    outputs: Vec<Param>,
    patch: Vec<Param>,
    bindings: Vec<Binding>,
    cbuffers: Vec<CBuffer>,
    counts: Counts,
    level_9: bool,
    requires: u64,
}

unsafe impl Send for Reflection {}
unsafe impl Sync for Reflection {}

fn parse_signature(data: &[u8], tag: &[u8; 4]) -> Vec<Param> {
    let count = rd(data, 0) as usize;
    let (stride, stream, precision) = match tag {
        b"OSG5" => (28, true, false),
        b"ISG1" | b"OSG1" | b"PSG1" => (32, true, true),
        _ => (24, false, false),
    };
    let mut out = Vec::new();
    for i in 0..count.min(64) {
        let mut e = 8 + i * stride;
        let st = if stream { rd(data, e) } else { 0 };
        if stream {
            e += 4;
        }
        if data.len() < e + 24 {
            break;
        }
        out.push(Param {
            name: str_at(data, rd(data, e)),
            index: rd(data, e + 4),
            system_value: rd(data, e + 8),
            component_type: rd(data, e + 12),
            register: rd(data, e + 16),
            mask: data[e + 20],
            rw_mask: data[e + 21],
            stream: st,
            min_precision: if precision { rd(data, e + 24) } else { 0 },
        });
    }
    out
}

/// Chunk RDEF : tampons de constantes, liaisons, compilateur, options.
fn parse_rdef(data: &[u8], r: &mut Reflection) {
    let (n_cb, cb_off) = (rd(data, 0) as usize, rd(data, 4) as usize);
    let (n_bind, bind_off) = (rd(data, 8) as usize, rd(data, 12) as usize);
    r.flags = rd(data, 20);
    r.creator = str_at(data, rd(data, 24));
    for i in 0..n_bind.min(256) {
        let e = bind_off + i * 32;
        if data.len() < e + 32 {
            break;
        }
        r.bindings.push(Binding {
            name: str_at(data, rd(data, e)),
            ty: rd(data, e + 4),
            return_type: rd(data, e + 8),
            dimension: rd(data, e + 12),
            samples: rd(data, e + 16),
            bind_point: rd(data, e + 20),
            bind_count: rd(data, e + 24),
            flags: rd(data, e + 28),
        });
    }
    for i in 0..n_cb.min(256) {
        let e = cb_off + i * 24;
        if data.len() < e + 24 {
            break;
        }
        r.cbuffers.push(CBuffer {
            name: str_at(data, rd(data, e)),
            variables: rd(data, e + 4),
            size: rd(data, e + 12),
            flags: rd(data, e + 16),
            ty: rd(data, e + 20),
        });
    }
}

/// Compte déclarations et instructions du programme (SHDR/SHEX).
fn count_program(tokens: &[u32], c: &mut Counts) {
    let len = (tokens.get(1).copied().unwrap_or(0) as usize).min(tokens.len());
    let mut p = 2;
    while p < len {
        let tok = tokens[p];
        let op = tok & 0x7FF;
        let n = if op == 0x35 {
            tokens.get(p + 1).copied().unwrap_or(0) as usize
        } else {
            ((tok >> 24) & 0x7F) as usize
        };
        if n == 0 {
            break;
        }
        if op != 0x35 {
            match op {
                // dcl_temps
                104 => {
                    c.declarations += 1;
                    c.temps = tokens.get(p + 1).copied().unwrap_or(0);
                }
                // dcl_indexableTemp
                105 => {
                    c.declarations += 1;
                    c.temp_arrays += tokens.get(p + 3).copied().unwrap_or(0);
                }
                // dcl_input_ps avec interpolation par échantillon.
                98 | 99 => {
                    c.declarations += 1;
                    let mode = (tok >> 11) & 0xF;
                    if mode == 5 || mode == 7 {
                        c.sample_frequency = true;
                    }
                }
                // dcl_thread_group
                155 => {
                    c.declarations += 1;
                    for (k, v) in c.thread_group.iter_mut().enumerate() {
                        *v = tokens.get(p + 1 + k).copied().unwrap_or(0);
                    }
                }
                88..=106 | 143..=162 | 206 => c.declarations += 1,
                _ => {
                    c.instructions += 1;
                    classify(op, c);
                }
            }
        }
        p += n;
    }
}

/// Catégories des compteurs de D3D11_SHADER_DESC et des Get*Count, d'après
/// les numéros d'opcode du format à jetons D3D10/D3D11.
fn classify(op: u32, c: &mut Counts) {
    match op {
        54 => c.mov += 1,
        55 => c.movc += 1,
        27 | 28 | 43 | 86 | 130 | 131 => c.conversion += 1,
        1 | 41 | 42 | 59 | 60 | 85 | 87 | 134..=142 => c.bitwise += 1,
        0 | 11 | 12 | 14..=17 | 24..=26 | 29 | 47 | 49..=52 | 56 | 57 | 64..=68 | 75 | 77 | 129 => {
            c.float += 1
        }
        30 | 32..=40 => c.int += 1,
        78..=84 | 132 | 133 => c.uint += 1,
        69..=74 | 107 | 108 | 126..=128 => c.texture_normal += 1,
        45 | 46 | 163 | 165 | 167 => c.texture_load += 1,
        164 | 166 | 168 => c.texture_store += 1,
        2 | 4 | 7 | 18 | 21..=23 | 48 | 62 | 76 => c.static_flow += 1,
        3 | 5 | 8 | 13 | 31 | 63 => c.dynamic_flow += 1,
        169..=189 => c.interlocked += 1,
        190 => c.barrier += 1,
        _ => {}
    }
    if matches!(op, 19 | 20 | 116 | 118) {
        c.emit += 1;
    }
    if matches!(op, 9 | 20 | 117 | 118) {
        c.cut += 1;
    }
}

fn build(blob: &[u8]) -> Option<Reflection> {
    let chunks = crate::dlls::dxbc::chunks(blob);
    if chunks.is_empty() {
        return None;
    }
    let mut r = Reflection {
        vtable: &REFLECTION_VTBL,
        refs: AtomicU32::new(1),
        blob: Vec::new(),
        version: 0,
        creator: c"".as_ptr().cast(),
        flags: 0,
        inputs: Vec::new(),
        outputs: Vec::new(),
        patch: Vec::new(),
        bindings: Vec::new(),
        cbuffers: Vec::new(),
        counts: Counts::default(),
        level_9: false,
        requires: 0,
    };
    for (tag, data) in chunks {
        match &tag {
            b"RDEF" => parse_rdef(data, &mut r),
            b"ISGN" | b"ISG1" => r.inputs = parse_signature(data, &tag),
            b"OSGN" | b"OSG1" | b"OSG5" => r.outputs = parse_signature(data, &tag),
            b"PCSG" | b"PSG1" => r.patch = parse_signature(data, &tag),
            b"SHDR" | b"SHEX" => {
                let tokens: Vec<u32> = data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|x| u32::from_le_bytes(*x))
                    .collect();
                r.version = tokens.first().copied().unwrap_or(0);
                count_program(&tokens, &mut r.counts);
            }
            b"SFI0" => {
                r.requires = u64::from_le_bytes(
                    data.get(..8)
                        .and_then(|b| b.try_into().ok())
                        .unwrap_or([0; 8]),
                )
            }
            b"Aon9" => r.level_9 = true,
            _ => {}
        }
    }
    Some(r)
}

/// Re-pointe les chaînes vers la copie gardée par l'objet.
fn rebase(r: &mut Reflection, src: &[u8]) {
    r.blob = src.to_vec();
    let (lo, hi) = (src.as_ptr() as usize, src.as_ptr() as usize + src.len());
    let base = r.blob.as_ptr() as usize;
    let fix = |p: &mut *const u8| {
        let a = *p as usize;
        if (lo..hi).contains(&a) {
            *p = (base + (a - lo)) as *const u8;
        }
    };
    fix(&mut r.creator);
    for p in r
        .inputs
        .iter_mut()
        .chain(r.outputs.iter_mut())
        .chain(r.patch.iter_mut())
    {
        fix(&mut p.name);
    }
    for b in r.bindings.iter_mut() {
        fix(&mut b.name);
    }
    for c in r.cbuffers.iter_mut() {
        fix(&mut c.name);
    }
}

/// Niveau minimal (D3D_FEATURE_LEVEL) d'après le modèle de shader.
fn min_feature_level(r: &Reflection) -> u32 {
    if r.level_9 {
        return 0x9100;
    }
    let major = (r.version >> 4) & 0xF;
    let minor = r.version & 0xF;
    match (major, minor) {
        (5, _) => 0xb000,
        (4, 1) => 0xa100,
        _ => 0xa000,
    }
}

// ── Méthodes ──────────────────────────────────────────────────────────────

unsafe fn me<'a>(this: *mut Reflection) -> &'a Reflection {
    &*this
}

unsafe extern "win64" fn qi(
    this: *mut Reflection,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || riid.is_null() {
        return E_INVALIDARG;
    }
    let g = &*riid;
    let unknown = g.data1 == 0 && g.data4 == [0xc0, 0, 0, 0, 0, 0, 0, 0x46];
    if unknown || g.data1 == IID_REFLECTION_47 || g.data1 == IID_REFLECTION_43 {
        (*this).refs.fetch_add(1, Ordering::Relaxed);
        *out = this.cast();
        return S_OK;
    }
    *out = std::ptr::null_mut();
    E_NOINTERFACE
}
unsafe extern "win64" fn add_ref(this: *mut Reflection) -> u32 {
    (*this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn release(this: *mut Reflection) -> u32 {
    let prev = (*this).refs.fetch_sub(1, Ordering::AcqRel);
    if prev == 1 {
        drop(Box::from_raw(this));
        return 0;
    }
    prev - 1
}

/// Taille de D3D11_SHADER_DESC en 64 bits.
pub(crate) const SHADER_DESC_SIZE: usize = 160;

unsafe extern "win64" fn get_desc(this: *mut Reflection, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_FAIL;
    }
    let r = me(this);
    let c = &r.counts;
    std::ptr::write_bytes(out, 0, SHADER_DESC_SIZE);
    let w = |o: usize, v: u32| std::ptr::write_unaligned(out.add(o) as *mut u32, v);
    w(0, r.version);
    std::ptr::write_unaligned(out.add(8) as *mut *const u8, r.creator);
    w(16, r.flags);
    w(20, r.cbuffers.len() as u32);
    w(24, r.bindings.len() as u32);
    w(28, r.inputs.len() as u32);
    w(32, r.outputs.len() as u32);
    w(36, c.instructions);
    w(40, c.temps);
    w(44, c.temp_arrays);
    w(52, c.declarations);
    w(56, c.texture_normal);
    w(60, c.texture_load);
    w(76, c.float);
    w(80, c.int);
    w(84, c.uint);
    w(88, c.static_flow);
    w(92, c.dynamic_flow);
    w(104, c.cut);
    w(108, c.emit);
    w(124, r.patch.len() as u32);
    w(148, c.barrier);
    w(152, c.interlocked);
    w(156, c.texture_store);
    S_OK
}

// Objets « constant buffer » et « variable » : ni IUnknown ni compteur,
// ils vivent autant que la réflexion (comme sous Windows).
#[repr(C)]
struct CbObj {
    vtable: *const Vt<3>,
    desc: Option<CBuffer>,
}
unsafe impl Send for CbObj {}
unsafe impl Sync for CbObj {}

static NULL_CB: CbObj = CbObj {
    vtable: &CB_VTBL,
    desc: None,
};

unsafe extern "win64" fn cb_desc(this: *const CbObj, out: *mut u8) -> i32 {
    let Some(d) = (*this).desc else { return E_FAIL };
    if out.is_null() {
        return E_FAIL;
    }
    std::ptr::write_unaligned(out as *mut *const u8, d.name);
    for (k, v) in [d.ty, d.variables, d.size, d.flags].into_iter().enumerate() {
        std::ptr::write_unaligned(out.add(8 + k * 4) as *mut u32, v);
    }
    S_OK
}
/// Variables : la description détaillée n'est pas lue, l'objet rendu est
/// l'objet « invalide » dont GetDesc échoue, comme pour un nom inconnu.
unsafe extern "win64" fn cb_variable(_: *const CbObj, _: usize) -> *const c_void {
    std::ptr::addr_of!(NULL_VAR).cast()
}

#[repr(C)]
struct VarObj {
    vtable: *const Vt<5>,
}
unsafe impl Sync for VarObj {}
static NULL_VAR: VarObj = VarObj { vtable: &VAR_VTBL };

unsafe extern "win64" fn var_desc(_: *const VarObj, _: *mut u8) -> i32 {
    E_FAIL
}
unsafe extern "win64" fn var_null(_: *const VarObj) -> *const c_void {
    std::ptr::addr_of!(NULL_CB).cast()
}
unsafe extern "win64" fn var_slot(_: *const VarObj, _: u32) -> u32 {
    0
}

static CB_VTBL: Vt<3> = Vt([
    cb_desc as *const (),
    cb_variable as *const (),
    cb_variable as *const (),
]);
static VAR_VTBL: Vt<5> = Vt([
    var_desc as *const (),
    var_null as *const (),
    var_null as *const (),
    var_slot as *const (),
    var_null as *const (),
]);

/// Objets tampon de constantes de chaque réflexion, créés à la demande ;
/// chaque liste est remplie d'un coup, ses éléments ne bougent plus.
static CB_OBJECTS: std::sync::Mutex<Vec<(usize, Vec<CbObj>)>> = std::sync::Mutex::new(Vec::new());

unsafe fn cb_object(this: *mut Reflection, i: Option<usize>) -> *const c_void {
    let r = me(this);
    let Some(i) = i.filter(|&i| i < r.cbuffers.len()) else {
        return std::ptr::addr_of!(NULL_CB).cast();
    };
    let mut all = CB_OBJECTS.lock().unwrap_or_else(|e| e.into_inner());
    let key = this as usize;
    let pos = match all.iter().position(|e| e.0 == key) {
        Some(p) => p,
        None => {
            let objs = r
                .cbuffers
                .iter()
                .map(|c| CbObj {
                    vtable: &CB_VTBL,
                    desc: Some(*c),
                })
                .collect();
            all.push((key, objs));
            all.len() - 1
        }
    };
    &all[pos].1[i] as *const CbObj as *const c_void
}

unsafe fn name_eq(p: *const u8, name: *const u8) -> bool {
    !p.is_null()
        && !name.is_null()
        && std::ffi::CStr::from_ptr(p.cast()) == std::ffi::CStr::from_ptr(name.cast())
}

unsafe extern "win64" fn cb_by_index(this: *mut Reflection, i: u32) -> *const c_void {
    cb_object(this, Some(i as usize))
}
unsafe extern "win64" fn cb_by_name(this: *mut Reflection, name: *const u8) -> *const c_void {
    let i = me(this).cbuffers.iter().position(|c| name_eq(c.name, name));
    cb_object(this, i)
}

unsafe fn write_binding(b: &Binding, out: *mut u8) {
    std::ptr::write_unaligned(out as *mut *const u8, b.name);
    let v = [
        b.ty,
        b.bind_point,
        b.bind_count,
        b.flags,
        b.return_type,
        b.dimension,
        b.samples,
    ];
    for (k, x) in v.into_iter().enumerate() {
        std::ptr::write_unaligned(out.add(8 + k * 4) as *mut u32, x);
    }
}
unsafe extern "win64" fn binding_desc(this: *mut Reflection, i: u32, out: *mut u8) -> i32 {
    match me(this).bindings.get(i as usize) {
        Some(b) if !out.is_null() => {
            write_binding(b, out);
            S_OK
        }
        _ => E_INVALIDARG,
    }
}
unsafe extern "win64" fn binding_by_name(
    this: *mut Reflection,
    name: *const u8,
    out: *mut u8,
) -> i32 {
    match me(this).bindings.iter().find(|b| name_eq(b.name, name)) {
        Some(b) if !out.is_null() => {
            write_binding(b, out);
            S_OK
        }
        _ => E_INVALIDARG,
    }
}

unsafe fn write_param(p: Option<&Param>, out: *mut u8) -> i32 {
    let Some(p) = p else { return E_INVALIDARG };
    if out.is_null() {
        return E_INVALIDARG;
    }
    std::ptr::write_bytes(out, 0, 40);
    std::ptr::write_unaligned(out as *mut *const u8, p.name);
    let w = |o: usize, v: u32| std::ptr::write_unaligned(out.add(o) as *mut u32, v);
    w(8, p.index);
    w(12, p.register);
    w(16, p.system_value);
    w(20, p.component_type);
    *out.add(24) = p.mask;
    *out.add(25) = p.rw_mask;
    w(28, p.stream);
    w(32, p.min_precision);
    S_OK
}
unsafe extern "win64" fn input_desc(this: *mut Reflection, i: u32, out: *mut u8) -> i32 {
    write_param(me(this).inputs.get(i as usize), out)
}
unsafe extern "win64" fn output_desc(this: *mut Reflection, i: u32, out: *mut u8) -> i32 {
    write_param(me(this).outputs.get(i as usize), out)
}
unsafe extern "win64" fn patch_desc(this: *mut Reflection, i: u32, out: *mut u8) -> i32 {
    write_param(me(this).patch.get(i as usize), out)
}
unsafe extern "win64" fn variable_by_name(_: *mut Reflection, _: *const u8) -> *const c_void {
    std::ptr::addr_of!(NULL_VAR).cast()
}
unsafe extern "win64" fn mov_count(this: *mut Reflection) -> u32 {
    me(this).counts.mov
}
unsafe extern "win64" fn movc_count(this: *mut Reflection) -> u32 {
    me(this).counts.movc
}
unsafe extern "win64" fn conversion_count(this: *mut Reflection) -> u32 {
    me(this).counts.conversion
}
unsafe extern "win64" fn bitwise_count(this: *mut Reflection) -> u32 {
    me(this).counts.bitwise
}
unsafe extern "win64" fn gs_input_primitive(_: *mut Reflection) -> u32 {
    0
}
unsafe extern "win64" fn is_sample_frequency(this: *mut Reflection) -> i32 {
    me(this).counts.sample_frequency as i32
}
unsafe extern "win64" fn interface_slots(_: *mut Reflection) -> u32 {
    0
}
unsafe extern "win64" fn get_min_feature_level(this: *mut Reflection, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = min_feature_level(me(this));
    S_OK
}
unsafe extern "win64" fn thread_group_size(
    this: *mut Reflection,
    x: *mut u32,
    y: *mut u32,
    z: *mut u32,
) -> u32 {
    let g = me(this).counts.thread_group;
    for (p, v) in [(x, g[0]), (y, g[1]), (z, g[2])] {
        if !p.is_null() {
            *p = v;
        }
    }
    g[0].saturating_mul(g[1]).saturating_mul(g[2])
}
unsafe extern "win64" fn requires_flags(this: *mut Reflection) -> u64 {
    me(this).requires
}

static REFLECTION_VTBL: Vt<22> = Vt([
    qi as *const (),
    add_ref as *const (),
    release as *const (),
    get_desc as *const (),
    cb_by_index as *const (),
    cb_by_name as *const (),
    binding_desc as *const (),
    input_desc as *const (),
    output_desc as *const (),
    patch_desc as *const (),
    variable_by_name as *const (),
    binding_by_name as *const (),
    mov_count as *const (),
    movc_count as *const (),
    conversion_count as *const (),
    bitwise_count as *const (),
    gs_input_primitive as *const (),
    is_sample_frequency as *const (),
    interface_slots as *const (),
    get_min_feature_level as *const (),
    thread_group_size as *const (),
    requires_flags as *const (),
]);

impl Drop for Reflection {
    fn drop(&mut self) {
        let key = self as *mut Reflection as usize;
        CB_OBJECTS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|e| e.0 != key);
    }
}

/// D3DReflect(pSrcData, SrcDataSize, pInterface, ppReflector).
pub(crate) unsafe extern "win64" fn d3d_reflect(
    src: *const c_void,
    size: usize,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if src.is_null() || out.is_null() || iid.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    let d1 = (*iid).data1;
    if d1 != IID_REFLECTION_47 && d1 != IID_REFLECTION_43 {
        crate::dlls::comobj::log_no_interface("d3dcompiler (D3DReflect)", &*iid);
        return E_NOINTERFACE;
    }
    let bytes = std::slice::from_raw_parts(src.cast::<u8>(), size.min(64 << 20));
    let Some(mut r) = build(bytes) else {
        return E_INVALIDARG;
    };
    rebase(&mut r, bytes);
    *out = Box::into_raw(Box::new(r)).cast();
    S_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dlls::dxbc::tests::{container, sample_ps};

    /// RDEF minimal : un tampon « Couleurs » (16 octets), trois liaisons
    /// (échantillonneur s0, texture t0, tampon b0) et le nom du compilateur.
    fn rdef() -> Vec<u8> {
        let mut d = vec![0u8; 28];
        let w = |d: &mut Vec<u8>, o: usize, v: u32| d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        let cb_off = d.len();
        d.extend([0u8; 24]);
        let bind_off = d.len();
        d.extend([0u8; 96]);
        let name = |d: &mut Vec<u8>, s: &str| {
            let o = d.len() as u32;
            d.extend(s.as_bytes());
            d.push(0);
            o
        };
        let creator = name(&mut d, "compilateur d'essai 1.0");
        let couleurs = name(&mut d, "Couleurs");
        let echant = name(&mut d, "echant");
        let image = name(&mut d, "image");
        w(&mut d, 0, 1);
        w(&mut d, 4, cb_off as u32);
        w(&mut d, 8, 3);
        w(&mut d, 12, bind_off as u32);
        w(&mut d, 16, 0xFFFF_0400);
        w(&mut d, 20, 0x100);
        w(&mut d, 24, creator);
        // Tampon : nom, variables, offset des variables, taille, drapeaux, type.
        for (k, v) in [couleurs, 1, 0, 16, 0, 0].into_iter().enumerate() {
            w(&mut d, cb_off + k * 4, v);
        }
        // Liaisons : nom, type, retour, dimension, échantillons, point, nombre, drapeaux.
        let rows = [
            [echant, 3, 0, 0, 0, 0, 1, 0],
            [image, 2, 5, 4, 0xFFFF_FFFF, 0, 1, 0xC],
            [couleurs, 0, 0, 0, 0, 0, 1, 0],
        ];
        for (i, row) in rows.iter().enumerate() {
            for (k, v) in row.iter().enumerate() {
                w(&mut d, bind_off + i * 32 + k * 4, *v);
            }
        }
        d
    }

    /// Le pixel shader d'essai de dxbc.rs, avec RDEF, SFI0 et Aon9.
    fn shader() -> Vec<u8> {
        let base = sample_ps();
        let mut parts: Vec<([u8; 4], Vec<u8>)> = crate::dlls::dxbc::chunks(&base)
            .into_iter()
            .map(|(t, d)| (t, d.to_vec()))
            .collect();
        parts.insert(0, (*b"RDEF", rdef()));
        parts.push((*b"SFI0", 1u64.to_le_bytes().to_vec()));
        parts.push((*b"Aon9", vec![0; 8]));
        let refs: Vec<(&[u8; 4], Vec<u8>)> = parts.iter().map(|(t, d)| (t, d.clone())).collect();
        container(&refs)
    }

    unsafe fn call<F: Copy>(obj: *mut c_void, slot: usize) -> F {
        let vt = *(obj as *const *const usize);
        std::mem::transmute_copy(&*vt.add(slot))
    }

    unsafe fn cstr(p: *const u8) -> String {
        std::ffi::CStr::from_ptr(p.cast())
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn d3dreflect_decrit_un_pixel_shader_d2d() {
        let blob = shader();
        let iid = Guid {
            data1: IID_REFLECTION_47,
            data2: 0x0cca,
            data3: 0x4956,
            data4: [0xa8, 0x37, 0x78, 0x69, 0x63, 0x75, 0x55, 0x84],
        };
        let mut r = std::ptr::null_mut();
        unsafe {
            assert_eq!(
                d3d_reflect(blob.as_ptr().cast(), blob.len(), &iid, &mut r),
                S_OK
            );
            // La copie de l'appelant peut disparaître : les chaînes restent.
            drop(blob);
            // GetDesc (3).
            let get_desc: unsafe extern "win64" fn(*mut c_void, *mut u8) -> i32 = call(r, 3);
            let mut d = [0u8; SHADER_DESC_SIZE];
            assert_eq!(get_desc(r, d.as_mut_ptr()), S_OK);
            let u = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
            assert_eq!(u(0), 0x50, "ps_5_0");
            let creator = std::ptr::read_unaligned(d.as_ptr().add(8) as *const *const u8);
            assert_eq!(cstr(creator), "compilateur d'essai 1.0");
            assert_eq!((u(20), u(24), u(28), u(32)), (1, 3, 2, 1));
            assert_eq!(u(36), 3, "sample, mul, ret");
            assert_eq!(u(40), 1, "dcl_temps 1");
            assert_eq!(u(52), 6, "déclarations");
            assert_eq!(u(56), 1, "un échantillonnage");
            // GetMinFeatureLevel (19) : Aon9 → 9_1.
            let fl: unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32 = call(r, 19);
            let mut level = 0;
            assert_eq!(fl(r, &mut level), S_OK);
            assert_eq!(level, 0x9100);
            // GetRequiresFlags (21) : SFI0.
            let req: unsafe extern "win64" fn(*mut c_void) -> u64 = call(r, 21);
            assert_eq!(req(r), 1);
            // Aucun mov dans ce shader.
            let mov: unsafe extern "win64" fn(*mut c_void) -> u32 = call(r, 12);
            assert_eq!(mov(r), 0);
            // GetInputParameterDesc (7).
            let input: unsafe extern "win64" fn(*mut c_void, u32, *mut u8) -> i32 = call(r, 7);
            let mut p = [0u8; 40];
            assert_eq!(input(r, 1, p.as_mut_ptr()), S_OK);
            let name = std::ptr::read_unaligned(p.as_ptr() as *const *const u8);
            assert_eq!(cstr(name), "TEXCOORD");
            assert_eq!(p[24], 0x3);
            assert_eq!(input(r, 2, p.as_mut_ptr()), E_INVALIDARG);
            // GetResourceBindingDescByName (11).
            let by_name: unsafe extern "win64" fn(*mut c_void, *const u8, *mut u8) -> i32 =
                call(r, 11);
            let mut b = [0u8; 40];
            assert_eq!(by_name(r, c"image".as_ptr().cast(), b.as_mut_ptr()), S_OK);
            assert_eq!(
                u32::from_le_bytes(b[8..12].try_into().unwrap()),
                2,
                "texture"
            );
            // GetConstantBufferByIndex (4) puis GetDesc du tampon.
            let cb_at: unsafe extern "win64" fn(*mut c_void, u32) -> *mut c_void = call(r, 4);
            let cb = cb_at(r, 0);
            let cb_get: unsafe extern "win64" fn(*mut c_void, *mut u8) -> i32 = call(cb, 0);
            let mut cd = [0u8; 24];
            assert_eq!(cb_get(cb, cd.as_mut_ptr()), S_OK);
            assert_eq!(u32::from_le_bytes(cd[16..20].try_into().unwrap()), 16);
            // Un index hors limites rend l'objet invalide, pas NULL.
            let bad = cb_at(r, 9);
            let bad_get: unsafe extern "win64" fn(*mut c_void, *mut u8) -> i32 = call(bad, 0);
            assert_eq!(bad_get(bad, cd.as_mut_ptr()), E_FAIL);
            let rel: unsafe extern "win64" fn(*mut c_void) -> u32 = call(r, 2);
            assert_eq!(rel(r), 0);
        }
    }

    #[test]
    fn d3dreflect_refuse_un_bloc_qui_n_est_pas_dxbc() {
        let iid = Guid {
            data1: IID_REFLECTION_47,
            data2: 0,
            data3: 0,
            data4: [0; 8],
        };
        let mut r = std::ptr::null_mut();
        let junk = [0u8; 64];
        unsafe {
            assert_eq!(
                d3d_reflect(junk.as_ptr().cast(), junk.len(), &iid, &mut r),
                E_INVALIDARG
            );
        }
        assert!(r.is_null());
    }
}
