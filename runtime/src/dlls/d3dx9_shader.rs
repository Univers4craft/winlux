//! D3DX9 : assemblage de shaders, table des constantes et sémantiques (#834).
//!
//! D3DXAssembleShader* passe par l'assembleur de `d3dx9_asm` ;
//! D3DXGetShaderConstantTable(Ex) rend un ID3DXConstantTable construit sur
//! le commentaire CTAB que les compilateurs placent dans le bytecode, ce qui
//! sert aux jeux livrés avec des shaders déjà compilés.

use super::d3dx9_asm::{self, ConstantTable};
use super::legacy_com::{get_ptr, ptr_size, put_ptr, rd32, vtable_sig, wr32};
use crate::com_call;
use std::sync::atomic::{AtomicU32, Ordering};

const S_OK: i32 = 0;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const D3DERR_INVALIDCALL: i32 = 0x8876_086Cu32 as i32;
const D3DXERR_INVALIDDATA: i32 = 0x8876_0B59u32 as i32;

// Registres de constantes du périphérique (IDirect3DDevice9).
const DEV_SET_VS_F: usize = 94;
const DEV_SET_VS_I: usize = 96;
const DEV_SET_VS_B: usize = 98;
const DEV_SET_PS_F: usize = 109;
const DEV_SET_PS_I: usize = 111;
const DEV_SET_PS_B: usize = 113;

const SET_INT4: u16 = 1;
const SET_FLOAT4: u16 = 2;
const SET_SAMPLER: u16 = 3;
const CLASS_MATRIX_ROWS: u16 = 2;
const CLASS_MATRIX_COLUMNS: u16 = 3;
const TYPE_BOOL: u16 = 1;
const TYPE_INT: u16 = 2;

// ═══════════════════════════════════════════════════════════════════════
// Assemblage
// ═══════════════════════════════════════════════════════════════════════

/// D3DXMACRO[] { Name, Definition } terminé par un nom nul.
unsafe fn read_defines(p: usize) -> Vec<(String, String)> {
    let mut v = Vec::new();
    if p == 0 {
        return v;
    }
    let ps = ptr_size();
    for i in 0..256 {
        let name = get_ptr(p + i * 2 * ps);
        if name == 0 {
            break;
        }
        let def = get_ptr(p + i * 2 * ps + ps);
        v.push((
            super::cstr(name as *const u8),
            super::cstr(def as *const u8),
        ));
    }
    v
}

unsafe fn assemble_bytes(
    src: &[u8],
    defines: usize,
    include: usize,
    shader: usize,
    errors: usize,
) -> i32 {
    put_ptr(shader, 0);
    put_ptr(errors, 0);
    if include != 0 {
        crate::log_warn!("d3dx9", "D3DXAssembleShader : #include du programme ignoré");
    }
    let text = String::from_utf8_lossy(src);
    let text = text.trim_end_matches('\0');
    match d3dx9_asm::assemble(text, &read_defines(defines)) {
        Ok(tokens) => {
            let bytes: Vec<u8> = tokens.iter().flat_map(|t| t.to_le_bytes()).collect();
            put_ptr(shader, super::d3dx9::new_buffer(bytes));
            S_OK
        }
        Err(e) => {
            crate::log_warn!("d3dx9", "D3DXAssembleShader : {e}");
            if errors != 0 {
                let mut m = format!("shader{e}\n").into_bytes();
                m.push(0);
                put_ptr(errors, super::d3dx9::new_buffer(m));
            }
            D3DXERR_INVALIDDATA
        }
    }
}

/// D3DXAssembleShader(pSrcData, SrcDataLen, pDefines, pInclude, Flags,
/// ppShader, ppErrorMsgs).
pub unsafe extern "win64" fn assemble_shader(
    src: usize,
    len: u32,
    defines: usize,
    include: usize,
    _flags: u32,
    shader: usize,
    errors: usize,
) -> i32 {
    if src == 0 || shader == 0 {
        return D3DERR_INVALIDCALL;
    }
    let d = std::slice::from_raw_parts(src as *const u8, len as usize);
    assemble_bytes(d, defines, include, shader, errors)
}

unsafe fn assemble_path(
    p: Option<std::path::PathBuf>,
    defines: usize,
    include: usize,
    shader: usize,
    errors: usize,
) -> i32 {
    let Some(p) = p else {
        return D3DERR_INVALIDCALL;
    };
    match std::fs::read(&p) {
        Ok(d) => assemble_bytes(&d, defines, include, shader, errors),
        Err(_) => {
            crate::log_warn!("d3dx9", "shader introuvable : {}", p.display());
            put_ptr(shader, 0);
            0x8007_0002u32 as i32
        }
    }
}
pub unsafe extern "win64" fn assemble_shader_from_file_a(
    path: *const u8,
    defines: usize,
    include: usize,
    _flags: u32,
    shader: usize,
    errors: usize,
) -> i32 {
    let p = (!path.is_null()).then(|| crate::paths::to_linux(&super::cstr(path)));
    assemble_path(p, defines, include, shader, errors)
}
pub unsafe extern "win64" fn assemble_shader_from_file_w(
    path: *const u16,
    defines: usize,
    include: usize,
    _flags: u32,
    shader: usize,
    errors: usize,
) -> i32 {
    let p = (!path.is_null()).then(|| crate::paths::to_linux(&super::wstr(path)));
    assemble_path(p, defines, include, shader, errors)
}

unsafe fn resource(module: usize, name: usize, wide: bool) -> Option<Vec<u8>> {
    use super::resource::*;
    let h = if wide {
        find_resource_w(module, name as *const u16, 10usize as *const u16)
    } else {
        find_resource_a(module, name as *const u8, 10usize as *const u8)
    };
    if h == 0 {
        return None;
    }
    let size = sizeof_resource(module, h) as usize;
    let p = lock_resource(load_resource(module, h));
    (p != 0).then(|| std::slice::from_raw_parts(p as *const u8, size).to_vec())
}
pub unsafe extern "win64" fn assemble_shader_from_resource_a(
    module: usize,
    name: usize,
    defines: usize,
    include: usize,
    _flags: u32,
    shader: usize,
    errors: usize,
) -> i32 {
    match resource(module, name, false) {
        Some(d) => assemble_bytes(&d, defines, include, shader, errors),
        None => D3DXERR_INVALIDDATA,
    }
}
pub unsafe extern "win64" fn assemble_shader_from_resource_w(
    module: usize,
    name: usize,
    defines: usize,
    include: usize,
    _flags: u32,
    shader: usize,
    errors: usize,
) -> i32 {
    match resource(module, name, true) {
        Some(d) => assemble_bytes(&d, defines, include, shader, errors),
        None => D3DXERR_INVALIDDATA,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Sémantiques et échantillonneurs
// ═══════════════════════════════════════════════════════════════════════

unsafe fn tokens_of(f: *const u32) -> Option<Vec<u32>> {
    if f.is_null() {
        return None;
    }
    let n = super::d3d9_shader::token_len(f, 1 << 20);
    Some(std::slice::from_raw_parts(f, n).to_vec())
}

fn reg_of(p: u32) -> (u32, u32) {
    (((p >> 28) & 7) | ((p >> 8) & 0x18), p & 0x7FF)
}

/// (usage, index) des entrées (`input`) ou des sorties d'un shader.
fn semantics(t: &[u32], input: bool) -> Vec<(u32, u32)> {
    let pixel = t[0] >> 16 == 0xFFFF;
    let major = (t[0] >> 8) & 0xFF;
    let mut v: Vec<(u32, u32)> = Vec::new();
    let mut add = |s: (u32, u32)| {
        if !v.contains(&s) {
            v.push(s);
        }
    };
    for (i, n) in d3dx9_asm::walk(t) {
        let op = t[i] & 0xFFFF;
        if op == 31 && n >= 3 {
            let (usage, idx) = (t[i + 1] & 0x1F, (t[i + 1] >> 16) & 0xF);
            let (ty, num) = reg_of(t[i + 2]);
            let is_input = ty == 1 || (pixel && ty == 3);
            let is_output = ty == 6 && major >= 3 && !pixel;
            if (input && is_input) || (!input && is_output) {
                if pixel && major < 3 {
                    // ps_2_x : t# = coordonnées de texture, v# = couleurs.
                    add(if ty == 3 { (5, num) } else { (10, num) });
                } else {
                    add((usage, idx));
                }
            }
        } else if !input && op != 0xFFFE && (major < 3 || pixel) && n >= 2 {
            // Sorties implicites : registres de destination.
            let (ty, num) = reg_of(t[i + 1]);
            match (pixel, ty) {
                (false, 4) => add(match num {
                    0 => (0, 0),
                    1 => (11, 0),
                    _ => (4, 0),
                }),
                (false, 5) => add((10, num)),
                (false, 6) => add((5, num)),
                (true, 8) => add((10, num)),
                (true, 9) => add((12, 0)),
                _ => {}
            }
        }
    }
    v
}

unsafe fn give_semantics(list: Vec<(u32, u32)>, out: usize, count: usize) -> i32 {
    if out != 0 {
        for (k, (u, i)) in list.iter().enumerate() {
            wr32(out + k * 8, *u);
            wr32(out + k * 8 + 4, *i);
        }
    }
    if count != 0 {
        wr32(count, list.len() as u32);
    }
    S_OK
}
pub unsafe extern "win64" fn get_shader_input_semantics(
    f: *const u32,
    out: usize,
    count: usize,
) -> i32 {
    let Some(t) = tokens_of(f) else {
        return D3DERR_INVALIDCALL;
    };
    give_semantics(semantics(&t, true), out, count)
}
pub unsafe extern "win64" fn get_shader_output_semantics(
    f: *const u32,
    out: usize,
    count: usize,
) -> i32 {
    let Some(t) = tokens_of(f) else {
        return D3DERR_INVALIDCALL;
    };
    give_semantics(semantics(&t, false), out, count)
}

/// D3DXGetShaderSamplers(pFunction, ppSamplers, pCount) : noms des
/// échantillonneurs de la table CTAB.
pub unsafe extern "win64" fn get_shader_samplers(f: *const u32, out: usize, count: usize) -> i32 {
    let Some(t) = tokens_of(f) else {
        return D3DERR_INVALIDCALL;
    };
    let names: Vec<String> = d3dx9_asm::constant_table(&t)
        .map(|c| {
            c.constants
                .into_iter()
                .filter(|k| k.set == SET_SAMPLER)
                .map(|k| k.name)
                .collect()
        })
        .unwrap_or_default();
    if out != 0 {
        // Chaînes gardées pour la durée du programme (comme la table D3DX).
        for (k, n) in names.iter().enumerate() {
            let mut b = n.clone().into_bytes();
            b.push(0);
            let s = Box::leak(b.into_boxed_slice());
            put_ptr(out + k * ptr_size(), s.as_ptr() as usize);
        }
    }
    if count != 0 {
        wr32(count, names.len() as u32);
    }
    S_OK
}

// ═══════════════════════════════════════════════════════════════════════
// ID3DXConstantTable
// ═══════════════════════════════════════════════════════════════════════

#[repr(C)]
struct Ct {
    vtbl: usize,
    refs: AtomicU32,
    table: ConstantTable,
    pixel: bool,
    /// Une case par constante : son adresse sert de D3DXHANDLE.
    handles: Vec<u8>,
    /// Chaînes C (noms, créateur) pour les descriptions.
    names: Vec<Vec<u8>>,
    creator: Vec<u8>,
}

unsafe fn ct<'a>(this: usize) -> &'a Ct {
    &*(this as *const Ct)
}

/// Constante désignée par un D3DXHANDLE : notre case, ou un nom (chaîne).
unsafe fn find(c: &Ct, h: usize) -> Option<usize> {
    if h == 0 {
        return None;
    }
    let base = c.handles.as_ptr() as usize;
    if h >= base && h < base + c.handles.len() {
        return Some(h - base);
    }
    if h < 0x10000 {
        return None;
    }
    let name = super::cstr(h as *const u8);
    let name = name.strip_prefix('$').unwrap_or(&name);
    c.table.constants.iter().position(|k| k.name == name)
}

unsafe extern "win64" fn ct_qi(this: usize, _iid: usize, out: usize) -> i32 {
    if out == 0 {
        return E_NOINTERFACE;
    }
    ct_addref(this);
    put_ptr(out, this);
    S_OK
}
unsafe extern "win64" fn ct_addref(this: usize) -> u32 {
    ct(this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn ct_release(this: usize) -> u32 {
    let n = ct(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this as *mut Ct));
    }
    n
}
unsafe extern "win64" fn ct_pointer(this: usize) -> usize {
    ct(this).table.raw.as_ptr() as usize
}
unsafe extern "win64" fn ct_size(this: usize) -> u32 {
    ct(this).table.raw.len() as u32
}
/// D3DXCONSTANTTABLE_DESC { LPCSTR Creator; DWORD Version; UINT Constants; }
unsafe extern "win64" fn ct_get_desc(this: usize, desc: usize) -> i32 {
    if desc == 0 {
        return D3DERR_INVALIDCALL;
    }
    let c = ct(this);
    let ps = ptr_size();
    put_ptr(desc, c.creator.as_ptr() as usize);
    wr32(desc + ps, c.table.version);
    wr32(desc + ps + 4, c.table.constants.len() as u32);
    S_OK
}
/// D3DXCONSTANT_DESC : Name, puis dix UINT, puis DefaultValue.
unsafe extern "win64" fn ct_get_constant_desc(
    this: usize,
    h: usize,
    desc: usize,
    count: usize,
) -> i32 {
    let c = ct(this);
    let Some(i) = find(c, h) else {
        return D3DERR_INVALIDCALL;
    };
    if desc != 0 {
        if count != 0 && rd32(count) == 0 {
            return D3DERR_INVALIDCALL;
        }
        let k = &c.table.constants[i];
        let ps = ptr_size();
        put_ptr(desc, c.names[i].as_ptr() as usize);
        let v = [
            k.set as u32,
            k.index as u32,
            k.count as u32,
            k.class as u32,
            k.ty as u32,
            k.rows as u32,
            k.columns as u32,
            k.elements as u32,
            k.struct_members as u32,
            k.bytes,
        ];
        for (j, x) in v.iter().enumerate() {
            wr32(desc + ps + 4 * j, *x);
        }
        let dv = match &k.default {
            Some(d) => d.as_ptr() as usize,
            None => 0,
        };
        put_ptr(desc + ps + 40, dv);
    }
    if count != 0 {
        wr32(count, 1);
    }
    S_OK
}
unsafe extern "win64" fn ct_get_sampler_index(this: usize, h: usize) -> u32 {
    let c = ct(this);
    match find(c, h).map(|i| &c.table.constants[i]) {
        Some(k) if k.set == SET_SAMPLER => k.index as u32,
        _ => u32::MAX,
    }
}
unsafe extern "win64" fn ct_get_constant(this: usize, parent: usize, index: u32) -> usize {
    let c = ct(this);
    if parent != 0 || index as usize >= c.table.constants.len() {
        return 0;
    }
    c.handles.as_ptr() as usize + index as usize
}
unsafe extern "win64" fn ct_get_constant_by_name(this: usize, parent: usize, name: usize) -> usize {
    let c = ct(this);
    if parent != 0 || name == 0 {
        return 0;
    }
    let n = super::cstr(name as *const u8);
    let n = n.strip_prefix('$').unwrap_or(&n);
    match c.table.constants.iter().position(|k| k.name == n) {
        Some(i) => c.handles.as_ptr() as usize + i,
        None => 0,
    }
}
unsafe extern "win64" fn ct_get_constant_element(this: usize, h: usize, index: u32) -> usize {
    let c = ct(this);
    match find(c, h) {
        Some(i) if index == 0 => c.handles.as_ptr() as usize + i,
        _ => 0,
    }
}

/// Valeurs passées par le programme, avant rangement en registres.
enum Vals {
    F(Vec<f32>),
    I(Vec<i32>),
    B(Vec<bool>),
}

impl Vals {
    fn f(&self, i: usize) -> f32 {
        match self {
            Vals::F(v) => v.get(i).copied().unwrap_or(0.0),
            Vals::I(v) => v.get(i).copied().unwrap_or(0) as f32,
            Vals::B(v) => v.get(i).copied().unwrap_or(false) as u32 as f32,
        }
    }
    fn i(&self, i: usize) -> i32 {
        match self {
            Vals::F(v) => v.get(i).copied().unwrap_or(0.0) as i32,
            Vals::I(v) => v.get(i).copied().unwrap_or(0),
            Vals::B(v) => v.get(i).copied().unwrap_or(false) as i32,
        }
    }
    fn b(&self, i: usize) -> bool {
        match self {
            Vals::F(v) => v.get(i).copied().unwrap_or(0.0) != 0.0,
            Vals::I(v) => v.get(i).copied().unwrap_or(0) != 0,
            Vals::B(v) => v.get(i).copied().unwrap_or(false),
        }
    }
    fn len(&self) -> usize {
        match self {
            Vals::F(v) => v.len(),
            Vals::I(v) => v.len(),
            Vals::B(v) => v.len(),
        }
    }
}

/// Range `vals` (ordre HLSL : éléments, puis lignes, puis colonnes) dans les
/// registres de la constante et les envoie au périphérique.
unsafe fn store(c: &Ct, dev: usize, i: usize, vals: &Vals) -> i32 {
    if dev == 0 {
        return D3DERR_INVALIDCALL;
    }
    let k = &c.table.constants[i];
    let rows = k.rows.max(1) as usize;
    let cols = k.columns.max(1) as usize;
    let per_elem = rows * cols;
    let elems = k.elements.max(1) as usize;
    let regs = k.count as usize;
    if k.set == SET_SAMPLER || regs == 0 {
        return S_OK;
    }
    // Registres par élément selon la classe.
    let (reg_per_elem, lanes) = match k.class {
        CLASS_MATRIX_ROWS => (rows, cols),
        CLASS_MATRIX_COLUMNS => (cols, rows),
        _ => (1, cols),
    };
    let mut out = vec![[0u32; 4]; regs];
    let avail = vals.len();
    for e in 0..elems {
        for r in 0..reg_per_elem {
            let reg = e * reg_per_elem + r;
            if reg >= regs {
                break;
            }
            for (l, slot) in out[reg].iter_mut().enumerate().take(lanes.min(4)) {
                // Indice dans les valeurs (ligne-major).
                let src = e * per_elem
                    + match k.class {
                        CLASS_MATRIX_COLUMNS => l * cols + r,
                        CLASS_MATRIX_ROWS => r * cols + l,
                        _ => l,
                    };
                if src >= avail {
                    continue;
                }
                *slot = match k.set {
                    SET_FLOAT4 => vals.f(src).to_bits(),
                    SET_INT4 => vals.i(src) as u32,
                    _ => vals.b(src) as u32,
                };
            }
        }
    }
    let flat: Vec<u32> = out.iter().flat_map(|r| *r).collect();
    let start = k.index as u32;
    let n = regs as u32;
    let p = flat.as_ptr();
    match (k.set, c.pixel) {
        (SET_FLOAT4, false) => com_call!(
            dev,
            DEV_SET_VS_F,
            fn(u32, *const u32, u32) -> i32,
            start,
            p,
            n
        ),
        (SET_FLOAT4, true) => com_call!(
            dev,
            DEV_SET_PS_F,
            fn(u32, *const u32, u32) -> i32,
            start,
            p,
            n
        ),
        (SET_INT4, false) => com_call!(
            dev,
            DEV_SET_VS_I,
            fn(u32, *const u32, u32) -> i32,
            start,
            p,
            n
        ),
        (SET_INT4, true) => com_call!(
            dev,
            DEV_SET_PS_I,
            fn(u32, *const u32, u32) -> i32,
            start,
            p,
            n
        ),
        (_, pixel) => {
            // Booléens : un BOOL par registre.
            let b: Vec<u32> = out.iter().map(|r| r[0]).collect();
            let idx = if pixel { DEV_SET_PS_B } else { DEV_SET_VS_B };
            com_call!(
                dev,
                idx,
                fn(u32, *const u32, u32) -> i32,
                start,
                b.as_ptr(),
                n
            )
        }
    }
}

unsafe fn set(this: usize, dev: usize, h: usize, vals: Vals) -> i32 {
    let c = ct(this);
    match find(c, h) {
        Some(i) => store(c, dev, i, &vals),
        None => D3DERR_INVALIDCALL,
    }
}
unsafe fn floats(p: usize, n: usize) -> Vec<f32> {
    if p == 0 {
        return Vec::new();
    }
    std::slice::from_raw_parts(p as *const f32, n).to_vec()
}
unsafe fn ints(p: usize, n: usize) -> Vec<i32> {
    if p == 0 {
        return Vec::new();
    }
    std::slice::from_raw_parts(p as *const i32, n).to_vec()
}

unsafe extern "win64" fn ct_set_defaults(this: usize, dev: usize) -> i32 {
    let c = ct(this);
    for (i, k) in c.table.constants.iter().enumerate() {
        if let Some(d) = &k.default {
            let n = d.len() / 4;
            let p = d.as_ptr() as usize;
            let v = match k.ty {
                TYPE_BOOL => Vals::B(ints(p, n).iter().map(|&x| x != 0).collect()),
                TYPE_INT => Vals::I(ints(p, n)),
                _ => Vals::F(floats(p, n)),
            };
            store(c, dev, i, &v);
        }
    }
    S_OK
}
unsafe extern "win64" fn ct_set_value(
    this: usize,
    dev: usize,
    h: usize,
    data: usize,
    bytes: u32,
) -> i32 {
    let c = ct(this);
    let Some(i) = find(c, h) else {
        return D3DERR_INVALIDCALL;
    };
    let n = bytes as usize / 4;
    let v = match c.table.constants[i].ty {
        TYPE_BOOL => Vals::B(ints(data, n).iter().map(|&x| x != 0).collect()),
        TYPE_INT => Vals::I(ints(data, n)),
        _ => Vals::F(floats(data, n)),
    };
    store(c, dev, i, &v)
}
unsafe extern "win64" fn ct_set_bool(this: usize, dev: usize, h: usize, b: u32) -> i32 {
    set(this, dev, h, Vals::B(vec![b != 0]))
}
unsafe extern "win64" fn ct_set_bool_array(
    this: usize,
    dev: usize,
    h: usize,
    p: usize,
    n: u32,
) -> i32 {
    set(
        this,
        dev,
        h,
        Vals::B(ints(p, n as usize).iter().map(|&x| x != 0).collect()),
    )
}
unsafe extern "win64" fn ct_set_int(this: usize, dev: usize, h: usize, v: i32) -> i32 {
    set(this, dev, h, Vals::I(vec![v]))
}
unsafe extern "win64" fn ct_set_int_array(
    this: usize,
    dev: usize,
    h: usize,
    p: usize,
    n: u32,
) -> i32 {
    set(this, dev, h, Vals::I(ints(p, n as usize)))
}
unsafe extern "win64" fn ct_set_float(this: usize, dev: usize, h: usize, v: f32) -> i32 {
    set(this, dev, h, Vals::F(vec![v]))
}
unsafe extern "win64" fn ct_set_float_array(
    this: usize,
    dev: usize,
    h: usize,
    p: usize,
    n: u32,
) -> i32 {
    set(this, dev, h, Vals::F(floats(p, n as usize)))
}
/// Les vecteurs remplissent un registre chacun (4 composantes).
unsafe fn set_vectors(this: usize, dev: usize, h: usize, p: usize, n: usize) -> i32 {
    let c = ct(this);
    let Some(i) = find(c, h) else {
        return D3DERR_INVALIDCALL;
    };
    let k = &c.table.constants[i];
    let cols = k.columns.max(1) as usize;
    let all = floats(p, n * 4);
    // Ne garder que les composantes utiles de chaque vecteur.
    let v: Vec<f32> = all
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|q| q[..cols.min(4)].to_vec())
        .collect();
    store(c, dev, i, &Vals::F(v))
}
unsafe extern "win64" fn ct_set_vector(this: usize, dev: usize, h: usize, p: usize) -> i32 {
    set_vectors(this, dev, h, p, 1)
}
unsafe extern "win64" fn ct_set_vector_array(
    this: usize,
    dev: usize,
    h: usize,
    p: usize,
    n: u32,
) -> i32 {
    set_vectors(this, dev, h, p, n as usize)
}
/// Matrices D3DXMATRIX (4×4 ligne-major) ramenées à la taille de la constante.
unsafe fn set_matrices(
    this: usize,
    dev: usize,
    h: usize,
    mats: Vec<[f32; 16]>,
    transpose: bool,
) -> i32 {
    let c = ct(this);
    let Some(i) = find(c, h) else {
        return D3DERR_INVALIDCALL;
    };
    let k = &c.table.constants[i];
    let rows = k.rows.max(1) as usize;
    let cols = k.columns.max(1) as usize;
    let mut v = Vec::with_capacity(mats.len() * rows * cols);
    for m in mats {
        for r in 0..rows.min(4) {
            for cc in 0..cols.min(4) {
                v.push(if transpose {
                    m[cc * 4 + r]
                } else {
                    m[r * 4 + cc]
                });
            }
        }
    }
    store(c, dev, i, &Vals::F(v))
}
unsafe fn mats(p: usize, n: usize) -> Vec<[f32; 16]> {
    let f = floats(p, n * 16);
    f.as_chunks::<16>().0.to_vec()
}
unsafe fn mat_ptrs(p: usize, n: usize) -> Vec<[f32; 16]> {
    let ps = ptr_size();
    (0..n)
        .map(|i| {
            let m = get_ptr(p + i * ps);
            mats(m, 1).first().copied().unwrap_or([0.0; 16])
        })
        .collect()
}
unsafe extern "win64" fn ct_set_matrix(this: usize, dev: usize, h: usize, p: usize) -> i32 {
    set_matrices(this, dev, h, mats(p, 1), false)
}
unsafe extern "win64" fn ct_set_matrix_array(
    this: usize,
    dev: usize,
    h: usize,
    p: usize,
    n: u32,
) -> i32 {
    set_matrices(this, dev, h, mats(p, n as usize), false)
}
unsafe extern "win64" fn ct_set_matrix_pointer_array(
    this: usize,
    dev: usize,
    h: usize,
    p: usize,
    n: u32,
) -> i32 {
    set_matrices(this, dev, h, mat_ptrs(p, n as usize), false)
}
unsafe extern "win64" fn ct_set_matrix_transpose(
    this: usize,
    dev: usize,
    h: usize,
    p: usize,
) -> i32 {
    set_matrices(this, dev, h, mats(p, 1), true)
}
unsafe extern "win64" fn ct_set_matrix_transpose_array(
    this: usize,
    dev: usize,
    h: usize,
    p: usize,
    n: u32,
) -> i32 {
    set_matrices(this, dev, h, mats(p, n as usize), true)
}
unsafe extern "win64" fn ct_set_matrix_transpose_pointer_array(
    this: usize,
    dev: usize,
    h: usize,
    p: usize,
    n: u32,
) -> i32 {
    set_matrices(this, dev, h, mat_ptrs(p, n as usize), true)
}

fn f(p: *const ()) -> usize {
    p as usize
}

fn ct_vtable() -> usize {
    vtable_sig(
        "ID3DXConstantTable",
        &[
            (f(ct_qi as *const ()), "ii"),
            (f(ct_addref as *const ()), ""),
            (f(ct_release as *const ()), ""),
            (f(ct_pointer as *const ()), ""),
            (f(ct_size as *const ()), ""),
            (f(ct_get_desc as *const ()), "i"),
            (f(ct_get_constant_desc as *const ()), "iii"),
            (f(ct_get_sampler_index as *const ()), "i"),
            (f(ct_get_constant as *const ()), "ii"),
            (f(ct_get_constant_by_name as *const ()), "ii"),
            (f(ct_get_constant_element as *const ()), "ii"),
            (f(ct_set_defaults as *const ()), "i"),
            (f(ct_set_value as *const ()), "iiii"),
            (f(ct_set_bool as *const ()), "iii"),
            (f(ct_set_bool_array as *const ()), "iiii"),
            (f(ct_set_int as *const ()), "iii"),
            (f(ct_set_int_array as *const ()), "iiii"),
            (f(ct_set_float as *const ()), "iii"),
            (f(ct_set_float_array as *const ()), "iiii"),
            (f(ct_set_vector as *const ()), "iii"),
            (f(ct_set_vector_array as *const ()), "iiii"),
            (f(ct_set_matrix as *const ()), "iii"),
            (f(ct_set_matrix_array as *const ()), "iiii"),
            (f(ct_set_matrix_pointer_array as *const ()), "iiii"),
            (f(ct_set_matrix_transpose as *const ()), "iii"),
            (f(ct_set_matrix_transpose_array as *const ()), "iiii"),
            (
                f(ct_set_matrix_transpose_pointer_array as *const ()),
                "iiii",
            ),
        ],
    )
}

pub(crate) fn new_constant_table(table: ConstantTable, pixel: bool) -> usize {
    let cstr = |s: &str| {
        let mut b = s.as_bytes().to_vec();
        b.push(0);
        b
    };
    let names = table.constants.iter().map(|k| cstr(&k.name)).collect();
    let creator = cstr(&table.creator);
    let n = table.constants.len().max(1);
    Box::into_raw(Box::new(Ct {
        vtbl: ct_vtable(),
        refs: AtomicU32::new(1),
        table,
        pixel,
        handles: vec![0; n],
        names,
        creator,
    })) as usize
}

pub unsafe extern "win64" fn get_shader_constant_table_ex(
    f: *const u32,
    _flags: u32,
    out: usize,
) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    put_ptr(out, 0);
    let Some(t) = tokens_of(f) else {
        return D3DERR_INVALIDCALL;
    };
    let Some(table) = d3dx9_asm::constant_table(&t) else {
        return E_FAIL;
    };
    put_ptr(out, new_constant_table(table, t[0] >> 16 == 0xFFFF));
    S_OK
}
pub unsafe extern "win64" fn get_shader_constant_table(f: *const u32, out: usize) -> i32 {
    get_shader_constant_table_ex(f, 0, out)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "D3DXAssembleShader" => fn assemble_shader,
        "D3DXAssembleShaderFromFileA" => fn assemble_shader_from_file_a,
        "D3DXAssembleShaderFromFileW" => fn assemble_shader_from_file_w,
        "D3DXAssembleShaderFromResourceA" => fn assemble_shader_from_resource_a,
        "D3DXAssembleShaderFromResourceW" => fn assemble_shader_from_resource_w,
        "D3DXGetShaderConstantTable" => fn get_shader_constant_table,
        "D3DXGetShaderConstantTableEx" => fn get_shader_constant_table_ex,
        "D3DXGetShaderInputSemantics" => fn get_shader_input_semantics,
        "D3DXGetShaderOutputSemantics" => fn get_shader_output_semantics,
        "D3DXGetShaderSamplers" => fn get_shader_samplers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantiques() {
        let t = d3dx9_asm::assemble(
            "vs_1_1\ndcl_position v0\ndcl_normal v1\ndcl_texcoord2 v2\n\
             m4x4 oPos, v0, c0\nmov oD0, v1\nmov oT2.xy, v2\n",
            &[],
        )
        .unwrap();
        assert_eq!(semantics(&t, true), vec![(0, 0), (3, 0), (5, 2)]);
        assert_eq!(semantics(&t, false), vec![(0, 0), (10, 0), (5, 2)]);
        let p = d3dx9_asm::assemble(
            "ps_2_0\ndcl t0.xy\ndcl v0\ndcl_2d s0\ntexld r0, t0, s0\nmul r0, r0, v0\nmov oC0, r0\n",
            &[],
        )
        .unwrap();
        assert_eq!(semantics(&p, true), vec![(5, 0), (10, 0)]);
        assert_eq!(semantics(&p, false), vec![(10, 0)]);
    }
}
