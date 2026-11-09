//! Direct3D 11 et DXGI pour les programmes 32 bits (#1090).
//!
//! Comme Direct3D 9 (`d3d9_x86.rs`), les objets gardent leurs fonctions
//! win64 ; un programme x86 reçoit à la place, dans le champ `vtable` de
//! chaque objet, une table de passerelles stdcall. La signature de chaque
//! méthode vient de son champ dans la structure de table (relevée par
//! `build.rs`) : les cases `*mut *mut T` rendues par la méthode deviennent
//! des cases de 4 octets côté x86.
//!
//! Ce module convertit ce que les passerelles ne savent pas faire seules :
//! - les tableaux de pointeurs d'objets (`*SetShaderResources`,
//!   `OMSetRenderTargets`, `IASetVertexBuffers`…) et leurs lectures
//!   (`*GetConstantBuffers`…), de 4 octets par case en x86 ;
//! - les structures qui portent un pointeur : D3D11_SUBRESOURCE_DATA,
//!   D3D11_MAPPED_SUBRESOURCE, D3D11_INPUT_ELEMENT_DESC,
//!   D3D11_SO_DECLARATION_ENTRY, DXGI_SWAP_CHAIN_DESC (HWND) et
//!   DXGI_ADAPTER_DESC* (SIZE_T).
//!
//! Dispositions x86 : en-têtes d3d11.h et dxgi.h de MinGW-w64 (domaine
//! public).

use crate::dlls::legacy_com::{method, rd32, wr32, x86, x86_table_sized};
use std::collections::HashMap;

const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
use std::sync::Mutex;

/// Table à donner à un objet : la table win64 dans un programme 64 bits, sa
/// table de passerelles x86 dans un programme 32 bits. `name` est le nom de
/// la structure de table (`DeviceContextVTable`…), `module` son module.
pub(crate) fn abi<T>(t: &'static T, module: &'static str, name: &'static str) -> *const T {
    abi_named(t, module, name, name)
}

/// Comme `abi`, pour deux tables d'une même structure (`label` les distingue).
pub(crate) fn abi_named<T>(
    t: &'static T,
    module: &'static str,
    name: &'static str,
    label: &'static str,
) -> *const T {
    if !x86() {
        // WINLUX_COMTRACE : chaque appel de méthode journalisé (#996).
        return crate::thunks::com_trace_table(t, module, label).unwrap_or(t);
    }
    static CACHE: Mutex<Option<HashMap<usize, usize>>> = Mutex::new(None);
    let key = t as *const T as usize;
    if let Some(&v) = CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .get(&key)
    {
        return v as *const T;
    }
    let n = std::mem::size_of::<T>() / 8;
    let fns = unsafe { std::slice::from_raw_parts(key as *const usize, n) };
    let sigs = crate::wow32::vtable_sigs(module, name).unwrap_or_default();
    if sigs.len() != n {
        crate::log_warn!(
            "d3d11",
            "{name} : {} signatures pour {n} méthodes, table x86 impossible",
            sigs.len()
        );
        return t;
    }
    let methods: Vec<(usize, usize, &str)> = (0..n)
        .map(|i| match override_for(name, i) {
            Some((f, sig)) => (f, fns[i], sig),
            None => (fns[i], fns[i], strip_this(sigs[i])),
        })
        .collect();
    let v = x86_table_sized(label, &methods, std::mem::size_of::<T>());
    CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(key, v);
    v as *const T
}

/// La signature relevée compte `this` ; la passerelle l'ajoute elle-même.
fn strip_this(sig: &'static str) -> &'static str {
    if sig.starts_with('>') {
        sig
    } else {
        &sig[1..]
    }
}

/// Fonction win64 d'origine de la méthode `idx` (la passerelle x86 a été
/// remplacée par une conversion de ce module).
unsafe fn orig(this: usize, idx: usize) -> usize {
    method(this, idx)
}

// --- Tableaux de pointeurs d'objets ----------------------------------------------

/// Plafond raisonnable pour un tableau d'objets (128 vues de ressources).
const MAX_ARRAY: u32 = 256;

/// Lit `n` pointeurs x86 consécutifs.
unsafe fn read_ptrs(pp: usize, n: u32) -> Vec<usize> {
    if pp == 0 {
        return Vec::new();
    }
    (0..n.min(MAX_ARRAY) as usize)
        .map(|i| rd32(pp + 4 * i) as usize)
        .collect()
}

/// Pointeur à passer à la fonction win64 : nul si le programme a passé nul.
fn arr(v: &[usize], pp: usize) -> *const usize {
    if pp == 0 {
        std::ptr::null()
    } else {
        v.as_ptr()
    }
}

/// Écrit `v` en pointeurs x86.
unsafe fn write_ptrs(pp: usize, v: &[usize]) {
    if pp == 0 {
        return;
    }
    for (i, p) in v.iter().enumerate() {
        wr32(pp + 4 * i, *p as u32);
    }
}

/// Case de pointeur x86 rendue par une fonction win64.
unsafe fn put32(at: usize, v: usize) {
    if at != 0 {
        wr32(at, v as u32);
    }
}

type Set3 = unsafe extern "win64" fn(usize, u32, u32, *const usize);
type Get3 = unsafe extern "win64" fn(usize, u32, u32, *mut usize);

/// `xxSetShaderResources/Samplers/ConstantBuffers(start, n, pp)`.
unsafe extern "win64" fn set3<const I: usize>(this: usize, start: u32, n: u32, pp: usize) {
    let v = read_ptrs(pp, n);
    let f: Set3 = std::mem::transmute(orig(this, I));
    f(this, start, n.min(MAX_ARRAY), arr(&v, pp));
}

/// `xxGetShaderResources/Samplers/ConstantBuffers(start, n, pp)`.
unsafe extern "win64" fn get3<const I: usize>(this: usize, start: u32, n: u32, pp: usize) {
    let n = n.min(MAX_ARRAY);
    let mut v = vec![0usize; n as usize];
    let f: Get3 = std::mem::transmute(orig(this, I));
    f(
        this,
        start,
        n,
        if pp == 0 {
            std::ptr::null_mut()
        } else {
            v.as_mut_ptr()
        },
    );
    write_ptrs(pp, &v);
}

/// `xxSetConstantBuffers1(start, n, pp, first, num)`.
unsafe extern "win64" fn set5<const I: usize>(
    this: usize,
    start: u32,
    n: u32,
    pp: usize,
    first: usize,
    num: usize,
) {
    let v = read_ptrs(pp, n);
    let f: unsafe extern "win64" fn(usize, u32, u32, *const usize, usize, usize) =
        std::mem::transmute(orig(this, I));
    f(this, start, n.min(MAX_ARRAY), arr(&v, pp), first, num);
}

/// `xxGetConstantBuffers1(start, n, pp, first, num)`.
unsafe extern "win64" fn get5<const I: usize>(
    this: usize,
    start: u32,
    n: u32,
    pp: usize,
    first: usize,
    num: usize,
) {
    let n = n.min(MAX_ARRAY);
    let mut v = vec![0usize; n as usize];
    let f: unsafe extern "win64" fn(usize, u32, u32, *mut usize, usize, usize) =
        std::mem::transmute(orig(this, I));
    f(
        this,
        start,
        n,
        if pp == 0 {
            std::ptr::null_mut()
        } else {
            v.as_mut_ptr()
        },
        first,
        num,
    );
    write_ptrs(pp, &v);
}

/// `xxSetShader(shader, ppClassInstances, n)`.
unsafe extern "win64" fn set_shader<const I: usize>(this: usize, shader: usize, pp: usize, n: u32) {
    let v = read_ptrs(pp, n);
    let f: unsafe extern "win64" fn(usize, usize, *const usize, u32) =
        std::mem::transmute(orig(this, I));
    f(this, shader, arr(&v, pp), n.min(MAX_ARRAY));
}

/// `xxGetShader(ppShader, ppClassInstances, pNumClassInstances)`.
unsafe extern "win64" fn get_shader<const I: usize>(
    this: usize,
    out: usize,
    pp: usize,
    num: usize,
) {
    let cap = if pp != 0 && num != 0 {
        rd32(num).min(MAX_ARRAY)
    } else {
        0
    };
    let mut v = vec![0usize; cap as usize];
    let mut sh = 0usize;
    let f: unsafe extern "win64" fn(usize, *mut usize, *mut usize, *mut u32) =
        std::mem::transmute(orig(this, I));
    f(
        this,
        if out == 0 {
            std::ptr::null_mut()
        } else {
            &mut sh
        },
        if pp == 0 {
            std::ptr::null_mut()
        } else {
            v.as_mut_ptr()
        },
        num as *mut u32,
    );
    put32(out, sh);
    if num != 0 {
        let got = rd32(num).min(cap) as usize;
        write_ptrs(pp, &v[..got]);
    }
}

/// IASetVertexBuffers(start, n, ppBuffers, pStrides, pOffsets).
unsafe extern "win64" fn ia_set_vbs(
    this: usize,
    start: u32,
    n: u32,
    pp: usize,
    st: usize,
    of: usize,
) {
    let v = read_ptrs(pp, n);
    let f: unsafe extern "win64" fn(usize, u32, u32, *const usize, usize, usize) =
        std::mem::transmute(orig(this, 18));
    f(this, start, n.min(MAX_ARRAY), arr(&v, pp), st, of);
}

/// IAGetVertexBuffers(start, n, ppBuffers, pStrides, pOffsets).
unsafe extern "win64" fn ia_get_vbs(
    this: usize,
    start: u32,
    n: u32,
    pp: usize,
    st: usize,
    of: usize,
) {
    let n = n.min(MAX_ARRAY);
    let mut v = vec![0usize; n as usize];
    let f: unsafe extern "win64" fn(usize, u32, u32, *mut usize, usize, usize) =
        std::mem::transmute(orig(this, 79));
    f(
        this,
        start,
        n,
        if pp == 0 {
            std::ptr::null_mut()
        } else {
            v.as_mut_ptr()
        },
        st,
        of,
    );
    write_ptrs(pp, &v);
}

/// OMSetRenderTargets(n, ppRTV, pDSV).
unsafe extern "win64" fn om_set_rts(this: usize, n: u32, pp: usize, dsv: usize) {
    let v = read_ptrs(pp, n);
    let f: unsafe extern "win64" fn(usize, u32, *const usize, usize) =
        std::mem::transmute(orig(this, 33));
    f(this, n.min(MAX_ARRAY), arr(&v, pp), dsv);
}

/// OMGetRenderTargets(n, ppRTV, ppDSV).
unsafe extern "win64" fn om_get_rts(this: usize, n: u32, pp: usize, dsv: usize) {
    let n = n.min(MAX_ARRAY);
    let mut v = vec![0usize; n as usize];
    let mut d = 0usize;
    let f: unsafe extern "win64" fn(usize, u32, *mut usize, *mut usize) =
        std::mem::transmute(orig(this, 89));
    f(
        this,
        n,
        if pp == 0 {
            std::ptr::null_mut()
        } else {
            v.as_mut_ptr()
        },
        if dsv == 0 {
            std::ptr::null_mut()
        } else {
            &mut d
        },
    );
    write_ptrs(pp, &v);
    put32(dsv, d);
}

/// D3D11_KEEP_RENDER_TARGETS_AND_DEPTH_STENCIL et D3D11_KEEP_UNORDERED_ACCESS_VIEWS.
const KEEP: u32 = 0xffff_ffff;

/// OMSetRenderTargetsAndUnorderedAccessViews(nRTV, ppRTV, pDSV, uavStart, nUAV, ppUAV, pCounts).
unsafe extern "win64" fn om_set_rts_uavs(
    this: usize,
    nrtv: u32,
    pprtv: usize,
    dsv: usize,
    start: u32,
    nuav: u32,
    ppuav: usize,
    counts: usize,
) {
    let r = if nrtv == KEEP {
        Vec::new()
    } else {
        read_ptrs(pprtv, nrtv)
    };
    let u = if nuav == KEEP {
        Vec::new()
    } else {
        read_ptrs(ppuav, nuav)
    };
    let f: unsafe extern "win64" fn(
        usize,
        u32,
        *const usize,
        usize,
        u32,
        u32,
        *const usize,
        usize,
    ) = std::mem::transmute(orig(this, 34));
    f(
        this,
        nrtv,
        arr(&r, pprtv),
        dsv,
        start,
        nuav,
        arr(&u, ppuav),
        counts,
    );
}

/// OMGetRenderTargetsAndUnorderedAccessViews(nRTV, ppRTV, ppDSV, uavStart, nUAV, ppUAV).
unsafe extern "win64" fn om_get_rts_uavs(
    this: usize,
    nrtv: u32,
    pprtv: usize,
    dsv: usize,
    start: u32,
    nuav: u32,
    ppuav: usize,
) {
    let nrtv = nrtv.min(MAX_ARRAY);
    let nuav = nuav.min(MAX_ARRAY);
    let mut r = vec![0usize; nrtv as usize];
    let mut u = vec![0usize; nuav as usize];
    let mut d = 0usize;
    let f: unsafe extern "win64" fn(usize, u32, *mut usize, *mut usize, u32, u32, *mut usize) =
        std::mem::transmute(orig(this, 90));
    f(
        this,
        nrtv,
        if pprtv == 0 {
            std::ptr::null_mut()
        } else {
            r.as_mut_ptr()
        },
        if dsv == 0 {
            std::ptr::null_mut()
        } else {
            &mut d
        },
        start,
        nuav,
        if ppuav == 0 {
            std::ptr::null_mut()
        } else {
            u.as_mut_ptr()
        },
    );
    write_ptrs(pprtv, &r);
    put32(dsv, d);
    write_ptrs(ppuav, &u);
}

/// SOSetTargets(n, ppTargets, pOffsets).
unsafe extern "win64" fn so_set(this: usize, n: u32, pp: usize, offsets: usize) {
    let v = read_ptrs(pp, n);
    let f: unsafe extern "win64" fn(usize, u32, *const usize, usize) =
        std::mem::transmute(orig(this, 37));
    f(this, n.min(MAX_ARRAY), arr(&v, pp), offsets);
}

/// SOGetTargets(n, ppTargets).
unsafe extern "win64" fn so_get(this: usize, n: u32, pp: usize) {
    let n = n.min(MAX_ARRAY);
    let mut v = vec![0usize; n as usize];
    let f: unsafe extern "win64" fn(usize, u32, *mut usize) = std::mem::transmute(orig(this, 93));
    f(
        this,
        n,
        if pp == 0 {
            std::ptr::null_mut()
        } else {
            v.as_mut_ptr()
        },
    );
    write_ptrs(pp, &v);
}

/// CSSetUnorderedAccessViews(start, n, ppUAV, pCounts).
unsafe extern "win64" fn cs_set_uavs(this: usize, start: u32, n: u32, pp: usize, counts: usize) {
    let v = read_ptrs(pp, n);
    let f: unsafe extern "win64" fn(usize, u32, u32, *const usize, usize) =
        std::mem::transmute(orig(this, 68));
    f(this, start, n.min(MAX_ARRAY), arr(&v, pp), counts);
}

// --- Structures qui portent un pointeur ------------------------------------------

/// Map(resource, sub, type, flags, D3D11_MAPPED_SUBRESOURCE*) : 12 octets en x86.
unsafe extern "win64" fn map(
    this: usize,
    res: usize,
    sub: u32,
    ty: u32,
    flags: u32,
    out: usize,
) -> i32 {
    let mut m = [0usize; 2];
    let f: unsafe extern "win64" fn(usize, usize, u32, u32, u32, *mut [usize; 2]) -> i32 =
        std::mem::transmute(orig(this, 14));
    let hr = f(
        this,
        res,
        sub,
        ty,
        flags,
        if out == 0 {
            std::ptr::null_mut()
        } else {
            &mut m
        },
    );
    if out != 0 {
        wr32(out, m[0] as u32);
        wr32(out + 4, m[1] as u32);
        wr32(out + 8, (m[1] >> 32) as u32);
    }
    hr
}

/// D3D11_SUBRESOURCE_DATA x86 { pSysMem, SysMemPitch, SysMemSlicePitch } → 64 bits.
unsafe fn init_data(p: usize, n: usize) -> Vec<[usize; 2]> {
    (0..n.min(4096))
        .map(|i| {
            let at = p + 12 * i;
            [
                rd32(at) as usize,
                rd32(at + 4) as usize | (rd32(at + 8) as usize) << 32,
            ]
        })
        .collect()
}

/// Niveaux de mipmap (0 : chaîne complète).
fn mips(levels: u32, w: u32, h: u32, d: u32) -> usize {
    if levels != 0 {
        return levels as usize;
    }
    let m = w.max(h).max(d).max(1);
    (32 - m.leading_zeros()) as usize
}

/// Nombre d'entrées D3D11_SUBRESOURCE_DATA d'une création de ressource.
unsafe fn subresources(kind: u32, desc: usize) -> usize {
    if desc == 0 {
        return 1;
    }
    let r = |o: usize| rd32(desc + o);
    match kind {
        // D3D11_TEXTURE1D_DESC : Width, MipLevels, ArraySize.
        1 => mips(r(4), r(0), 1, 1) * r(8).max(1) as usize,
        // D3D11_TEXTURE2D_DESC : Width, Height, MipLevels, ArraySize.
        2 => mips(r(8), r(0), r(4), 1) * r(12).max(1) as usize,
        // D3D11_TEXTURE3D_DESC : Width, Height, Depth, MipLevels.
        3 => mips(r(12), r(0), r(4), r(8)),
        _ => 1,
    }
}

type Create = unsafe extern "win64" fn(usize, usize, *const [usize; 2], *mut usize) -> i32;

/// CreateBuffer / CreateTexture1D, 2D, 3D (et 2D1, 3D1) : desc, données initiales, objet.
unsafe extern "win64" fn create_res<const I: usize, const KIND: u32>(
    this: usize,
    desc: usize,
    init: usize,
    out: usize,
) -> i32 {
    let data = if init == 0 {
        Vec::new()
    } else {
        init_data(init, subresources(KIND, desc))
    };
    let mut obj = 0usize;
    let f: Create = std::mem::transmute(orig(this, I));
    let hr = f(
        this,
        desc,
        if init == 0 {
            std::ptr::null()
        } else {
            data.as_ptr()
        },
        if out == 0 {
            std::ptr::null_mut()
        } else {
            &mut obj
        },
    );
    put32(out, obj);
    hr
}

/// D3D11_INPUT_ELEMENT_DESC : 28 octets en x86, 32 en 64 bits.
#[repr(C)]
struct InputElement {
    name: usize,
    rest: [u32; 6],
}

/// CreateInputLayout(descs, n, bytecode, len, out).
unsafe extern "win64" fn create_il(
    this: usize,
    descs: usize,
    n: u32,
    code: usize,
    len: usize,
    out: usize,
) -> i32 {
    let v: Vec<InputElement> = (0..n.min(64) as usize)
        .map(|i| {
            let at = descs + 28 * i;
            let mut rest = [0u32; 6];
            for (k, r) in rest.iter_mut().enumerate() {
                *r = rd32(at + 4 + 4 * k);
            }
            InputElement {
                name: rd32(at) as usize,
                rest,
            }
        })
        .collect();
    let mut obj = 0usize;
    let f: unsafe extern "win64" fn(
        usize,
        *const InputElement,
        u32,
        usize,
        usize,
        *mut usize,
    ) -> i32 = std::mem::transmute(orig(this, 11));
    let hr = f(
        this,
        if descs == 0 {
            std::ptr::null()
        } else {
            v.as_ptr()
        },
        n,
        code,
        len,
        if out == 0 {
            std::ptr::null_mut()
        } else {
            &mut obj
        },
    );
    put32(out, obj);
    hr
}

/// D3D11_SO_DECLARATION_ENTRY : 16 octets en x86, 24 en 64 bits.
#[repr(C)]
struct SoEntry {
    stream: u32,
    name: usize,
    index: u32,
    start: u8,
    count: u8,
    slot: u8,
}

/// CreateGeometryShaderWithStreamOutput(code, len, decl, n, strides, nstrides, rasterized, linkage, out).
unsafe extern "win64" fn create_gs_so(
    this: usize,
    code: usize,
    len: usize,
    decl: usize,
    n: u32,
    strides: usize,
    nstrides: u32,
    rast: u32,
    linkage: usize,
    out: usize,
) -> i32 {
    let v: Vec<SoEntry> = (0..n.min(64) as usize)
        .map(|i| {
            let at = decl + 16 * i;
            let b = rd32(at + 12);
            SoEntry {
                stream: rd32(at),
                name: rd32(at + 4) as usize,
                index: rd32(at + 8),
                start: b as u8,
                count: (b >> 8) as u8,
                slot: (b >> 16) as u8,
            }
        })
        .collect();
    let mut obj = 0usize;
    let f: unsafe extern "win64" fn(
        usize,
        usize,
        usize,
        *const SoEntry,
        u32,
        usize,
        u32,
        u32,
        usize,
        *mut usize,
    ) -> i32 = std::mem::transmute(orig(this, 14));
    let hr = f(
        this,
        code,
        len,
        if decl == 0 {
            std::ptr::null()
        } else {
            v.as_ptr()
        },
        n,
        strides,
        nstrides,
        rast,
        linkage,
        if out == 0 {
            std::ptr::null_mut()
        } else {
            &mut obj
        },
    );
    put32(out, obj);
    hr
}

/// IDXGIDevice::QueryResourceResidency(ppResources, pStatus, n).
unsafe extern "win64" fn residency(this: usize, pp: usize, status: usize, n: u32) -> i32 {
    let v = read_ptrs(pp, n);
    let f: unsafe extern "win64" fn(usize, *const usize, usize, u32) -> i32 =
        std::mem::transmute(orig(this, 9));
    f(this, arr(&v, pp), status, n.min(MAX_ARRAY))
}

// --- DXGI -------------------------------------------------------------------------

/// DXGI_SWAP_CHAIN_DESC x86 (60 octets) → 64 bits (72 octets, HWND aligné).
pub(crate) unsafe fn swap_desc_in(p: usize) -> crate::dlls::dxgi::DxgiSwapChainDesc {
    let mut d: crate::dlls::dxgi::DxgiSwapChainDesc = std::mem::zeroed();
    std::ptr::copy_nonoverlapping(p as *const u8, &mut d as *mut _ as *mut u8, 44);
    d.output_window = rd32(p + 44) as usize;
    d.windowed = rd32(p + 48) as i32;
    d.swap_effect = rd32(p + 52);
    d.flags = rd32(p + 56);
    d
}

unsafe fn swap_desc_out(p: usize, d: &crate::dlls::dxgi::DxgiSwapChainDesc) {
    std::ptr::copy_nonoverlapping(d as *const _ as *const u8, p as *mut u8, 44);
    wr32(p + 44, d.output_window as u32);
    wr32(p + 48, d.windowed as u32);
    wr32(p + 52, d.swap_effect);
    wr32(p + 56, d.flags);
}

/// IDXGIFactory::CreateSwapChain(device, desc, out).
unsafe extern "win64" fn fac_create_sc(this: usize, device: usize, desc: usize, out: usize) -> i32 {
    let d = if desc == 0 {
        None
    } else {
        Some(swap_desc_in(desc))
    };
    let mut obj = 0usize;
    let f: unsafe extern "win64" fn(
        usize,
        usize,
        *const crate::dlls::dxgi::DxgiSwapChainDesc,
        *mut usize,
    ) -> i32 = std::mem::transmute(orig(this, 10));
    let hr = f(
        this,
        device,
        d.as_ref().map_or(std::ptr::null(), |d| d),
        if out == 0 {
            std::ptr::null_mut()
        } else {
            &mut obj
        },
    );
    put32(out, obj);
    hr
}

/// IDXGISwapChain::GetDesc(desc).
unsafe extern "win64" fn sc_get_desc(this: usize, out: usize) -> i32 {
    if out == 0 {
        return E_INVALIDARG;
    }
    let mut d: crate::dlls::dxgi::DxgiSwapChainDesc = std::mem::zeroed();
    let f: unsafe extern "win64" fn(usize, *mut crate::dlls::dxgi::DxgiSwapChainDesc) -> i32 =
        std::mem::transmute(orig(this, 12));
    let hr = f(this, &mut d);
    if hr >= 0 {
        swap_desc_out(out, &d);
    }
    hr
}

/// IDXGISwapChain1::GetHwnd(hwnd) : un HWND fait 4 octets en x86 (#1244).
unsafe extern "win64" fn sc_get_hwnd(this: usize, out: usize) -> i32 {
    if out == 0 {
        return E_INVALIDARG;
    }
    let mut h = 0usize;
    let f: unsafe extern "win64" fn(usize, *mut usize) -> i32 = std::mem::transmute(orig(this, 20));
    let hr = f(this, &mut h);
    if hr >= 0 {
        wr32(out, h as u32);
    }
    hr
}

/// DXGI_ADAPTER_DESC à DESC3 : les trois SIZE_T font 4 octets en x86.
/// `x86_size` : 292 (DESC), 296 (DESC1), 304 (DESC2 et DESC3).
unsafe fn adapter_desc<const I: usize, const X86_SIZE: usize>(this: usize, out: usize) -> i32 {
    if out == 0 {
        return E_INVALIDARG;
    }
    let mut b = [0u8; 320];
    let f: unsafe extern "win64" fn(usize, *mut u8) -> i32 = std::mem::transmute(orig(this, I));
    let hr = f(this, b.as_mut_ptr());
    if hr < 0 {
        return hr;
    }
    let o = out as *mut u8;
    std::ptr::copy_nonoverlapping(b.as_ptr(), o, 272);
    for k in 0..3 {
        let v = u64::from_le_bytes(b[272 + 8 * k..280 + 8 * k].try_into().unwrap_or_default());
        wr32(out + 272 + 4 * k, v.min(u32::MAX as u64) as u32);
    }
    // LUID puis Flags et préemptions.
    std::ptr::copy_nonoverlapping(b.as_ptr().add(296), o.add(284), X86_SIZE - 284);
    hr
}
unsafe extern "win64" fn adapter_desc0(this: usize, out: usize) -> i32 {
    adapter_desc::<8, 292>(this, out)
}
unsafe extern "win64" fn adapter_desc1(this: usize, out: usize) -> i32 {
    adapter_desc::<10, 296>(this, out)
}
unsafe extern "win64" fn adapter_desc2(this: usize, out: usize) -> i32 {
    adapter_desc::<11, 304>(this, out)
}
unsafe extern "win64" fn adapter_desc3(this: usize, out: usize) -> i32 {
    adapter_desc::<18, 304>(this, out)
}

// --- Table des conversions --------------------------------------------------------

/// Méthode remplacée en x86 : (fonction, signature après `this`).
fn override_for(name: &str, i: usize) -> Option<(usize, &'static str)> {
    macro_rules! o {
        ($f:expr, $s:expr) => {
            Some(($f as *const () as usize, $s))
        };
    }
    match (name, i) {
        ("DeviceContextVTable", i) => context(i),
        ("DeviceVTable", 3) => o!(create_res::<3, 0>, "iii>i"),
        ("DeviceVTable", 4) => o!(create_res::<4, 1>, "iii>i"),
        ("DeviceVTable", 5) => o!(create_res::<5, 2>, "iii>i"),
        ("DeviceVTable", 6) => o!(create_res::<6, 3>, "iii>i"),
        ("DeviceVTable", 11) => o!(create_il, "iiiii>i"),
        ("DeviceVTable", 14) => o!(create_gs_so, "iiiiiiiii>i"),
        ("DeviceVTable", 54) => o!(create_res::<54, 2>, "iii>i"),
        ("DeviceVTable", 55) => o!(create_res::<55, 3>, "iii>i"),
        ("DxgiDeviceVTable", 9) => o!(residency, "iii>i"),
        ("DxgiFactoryVTable", 10) => o!(fac_create_sc, "iii>i"),
        ("DxgiSwapChainVTable", 12) => o!(sc_get_desc, "i>i"),
        ("DxgiSwapChainVTable", 20) => o!(sc_get_hwnd, "i>i"),
        ("DxgiAdapterVTable", 8) => o!(adapter_desc0, "i>i"),
        ("DxgiAdapterVTable", 10) => o!(adapter_desc1, "i>i"),
        ("DxgiAdapterVTable", 11) => o!(adapter_desc2, "i>i"),
        ("DxgiAdapterVTable", 18) => o!(adapter_desc3, "i>i"),
        _ => None,
    }
}

/// ID3D11DeviceContext à ID3D11DeviceContext4.
fn context(i: usize) -> Option<(usize, &'static str)> {
    macro_rules! o {
        ($f:expr, $s:expr) => {
            Some(($f as *const () as usize, $s))
        };
    }
    macro_rules! set3 {
        ($($n:literal)*) => { match i { $($n => return o!(set3::<$n>, "iii>v"),)* _ => {} } };
    }
    macro_rules! get3 {
        ($($n:literal)*) => { match i { $($n => return o!(get3::<$n>, "iii>v"),)* _ => {} } };
    }
    macro_rules! set_shader {
        ($($n:literal)*) => { match i { $($n => return o!(set_shader::<$n>, "iii>v"),)* _ => {} } };
    }
    macro_rules! get_shader {
        ($($n:literal)*) => { match i { $($n => return o!(get_shader::<$n>, "iii>v"),)* _ => {} } };
    }
    macro_rules! set5 {
        ($($n:literal)*) => { match i { $($n => return o!(set5::<$n>, "iiiii>v"),)* _ => {} } };
    }
    macro_rules! get5 {
        ($($n:literal)*) => { match i { $($n => return o!(get5::<$n>, "iiiii>v"),)* _ => {} } };
    }
    // Ressources, échantillonneurs et tampons constants des six étages.
    set3!(7 8 10 16 22 25 26 31 32 59 61 62 63 65 66 67 70 71);
    get3!(72 73 75 77 81 84 85 87 88 97 99 100 101 103 104 105 106 108 109);
    set_shader!(9 11 23 60 64 69);
    get_shader!(74 76 82 98 102 107);
    // ID3D11DeviceContext1 : tampons constants avec plages.
    set5!(119 120 121 122 123 124);
    get5!(125 126 127 128 129 130);
    match i {
        14 => o!(map, "iiiii>i"),
        18 => o!(ia_set_vbs, "iiiii>v"),
        33 => o!(om_set_rts, "iii>v"),
        34 => o!(om_set_rts_uavs, "iiiiiii>v"),
        37 => o!(so_set, "iii>v"),
        68 => o!(cs_set_uavs, "iiii>v"),
        79 => o!(ia_get_vbs, "iiiii>v"),
        89 => o!(om_get_rts, "iii>v"),
        90 => o!(om_get_rts_uavs, "iiiiii>v"),
        93 => o!(so_get, "ii>v"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_chain_desc_x86_has_60_bytes() {
        let mut b = [0u32; 15];
        for (i, v) in b.iter_mut().enumerate() {
            *v = i as u32 + 1;
        }
        unsafe {
            let d = swap_desc_in(b.as_ptr() as usize);
            assert_eq!(d.buffer_desc.width, 1);
            assert_eq!(d.buffer_count, 11);
            assert_eq!(d.output_window, 12);
            assert_eq!((d.windowed, d.swap_effect, d.flags), (13, 14, 15));
            let mut o = [0u32; 16];
            swap_desc_out(o.as_mut_ptr() as usize, &d);
            assert_eq!(&o[..15], &b[..]);
            assert_eq!(o[15], 0);
        }
    }

    #[test]
    fn subresource_counts_follow_the_texture_desc() {
        // 256×64, chaîne complète (9 niveaux), 2 tranches.
        let t2 = [256u32, 64, 0, 2, 28, 1, 0, 0, 8, 0];
        assert_eq!(unsafe { subresources(2, t2.as_ptr() as usize) }, 18);
        let t3 = [16u32, 16, 4, 3, 28, 0, 8, 0, 0];
        assert_eq!(unsafe { subresources(3, t3.as_ptr() as usize) }, 3);
        assert_eq!(unsafe { subresources(0, t3.as_ptr() as usize) }, 1);
    }

    #[test]
    fn x86_structures_have_their_64_bit_sizes() {
        assert_eq!(std::mem::size_of::<InputElement>(), 32);
        assert_eq!(std::mem::size_of::<SoEntry>(), 24);
        assert_eq!(
            std::mem::size_of::<crate::dlls::dxgi::DxgiSwapChainDesc>(),
            72
        );
    }

    #[test]
    fn every_direct3d11_table_has_its_signatures() {
        for (module, name, n) in [
            ("dlls::d3d11", "DeviceVTable", 69),
            ("dlls::d3d11", "DeviceContextVTable", 149),
            ("dlls::d3d11", "Texture2DVTable", 11),
            ("dlls::d3d11", "BufferVTable", 11),
            ("dlls::d3d11", "SimpleComVTable", 9),
            ("dlls::d3d11", "DxgiDeviceVTable", 20),
            ("dlls::dxgi", "DxgiAdapterVTable", 19),
            ("dlls::dxgi", "DxgiSwapChainVTable", 29),
            ("dlls::dxgi", "DxgiFactoryVTable", 32),
        ] {
            let s = crate::wow32::vtable_sigs(module, name).expect(name);
            assert_eq!(s.len(), n, "{name}");
            assert!(s.iter().all(|s| !s.contains('?')), "{name} : {s:?}");
        }
        // Getters à tableau : une case `p` côté win64, remplacée en x86.
        let ctx = crate::wow32::vtable_sigs("dlls::d3d11", "DeviceContextVTable").unwrap();
        assert_eq!(ctx[73], "iiip>v");
        assert!(context(73).is_some() && context(14).is_some());
    }
}
