//! d3d8.dll : Direct3D 8 (#237), traduit sur le Direct3D 9 de Winlux.
//!
//! Chaque objet Direct3D 8 (IDirect3D8, IDirect3DDevice8, textures,
//! surfaces, tampons de sommets et d'indices) enveloppe l'objet Direct3D 9
//! correspondant et lui passe ses appels, en convertissant ce qui diffère :
//! D3DPRESENT_PARAMETERS, D3DCAPS8, identifiant d'adaptateur, états de
//! texture devenus états d'échantillonneur, ZBIAS devenu DEPTHBIAS,
//! SetStreamSource/SetIndices (index de base), blocs d'états désignés par
//! un numéro, shaders de sommets désignés par un numéro ou un FVF, avec
//! leur déclaration à jetons Direct3D 8 (registres v0…v16) traduite en
//! D3DVERTEXELEMENT9 et des instructions `dcl` ajoutées au bytecode vs.1.1.
//!
//! Comme DirectDraw (`legacy_com`), la table de méthodes est celle de
//! l'ABI du programme : un jeu 32 bits appelle directement ces objets.
//! Les textures volumiques, chaînes d'échange supplémentaires, patchs et
//! ProcessVertices ne sont pas fournis (D3DERR_INVALIDCALL).

#![allow(clippy::too_many_arguments)]

use super::comobj::{guid, Guid};
use super::legacy_com::{put_ptr, rd32, vtable, wr32, x86};
use crate::dlls::lock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

const S_OK: i32 = 0;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const D3DERR_INVALIDCALL: i32 = 0x8876_086Cu32 as i32;
const D3DERR_NOTAVAILABLE: i32 = 0x8876_086Au32 as i32;

/// Appelle la méthode `idx` d'un objet Direct3D 9 (table win64 ou x86).
macro_rules! c9 {
    ($($t:tt)*) => { $crate::com_call!($($t)*) };
}

const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_D3D8: Guid = guid(
    0x1DD9E8DA,
    0x1C77,
    0x4D40,
    [0xB0, 0xCF, 0x98, 0xFE, 0xFD, 0xFF, 0x95, 0x12],
);
const IID_DEVICE8: Guid = guid(
    0x7385E5DF,
    0x8FE8,
    0x41D5,
    [0x86, 0xB6, 0xD7, 0xB4, 0x85, 0x47, 0xB6, 0xCF],
);
const IID_RESOURCE8: Guid = guid(
    0x1B36BB7B,
    0x09B7,
    0x410A,
    [0xB4, 0x45, 0x7D, 0x14, 0x30, 0xD7, 0xB3, 0x3F],
);
const IID_BASETEX8: Guid = guid(
    0xB4211CFA,
    0x51B9,
    0x4A9F,
    [0xAB, 0x78, 0xDB, 0x99, 0xB2, 0xBB, 0x67, 0x8E],
);
const IID_TEXTURE8: Guid = guid(
    0xE4CDD575,
    0x2866,
    0x4F01,
    [0xB1, 0x2E, 0x7E, 0xEC, 0xE1, 0xEC, 0x93, 0x58],
);
const IID_CUBE8: Guid = guid(
    0x3EE5B968,
    0x2ACA,
    0x4C34,
    [0x8B, 0xB5, 0x7E, 0x0C, 0x3D, 0x19, 0xB7, 0x50],
);
const IID_SURFACE8: Guid = guid(
    0xB96EEBCA,
    0xB326,
    0x4EA5,
    [0x88, 0x2F, 0x2F, 0xF5, 0xBA, 0xE0, 0x21, 0xDD],
);
const IID_VB8: Guid = guid(
    0x8AEEEAC7,
    0x05F9,
    0x44D4,
    [0xB5, 0x91, 0x00, 0x0B, 0x0D, 0xF1, 0xCB, 0x95],
);
const IID_IB8: Guid = guid(
    0x0E689C9A,
    0x053D,
    0x44A0,
    [0x9D, 0x92, 0xDB, 0x0E, 0x3D, 0x75, 0x0F, 0x86],
);

unsafe fn read_guid(p: usize) -> Option<Guid> {
    if p == 0 {
        None
    } else {
        Some(std::ptr::read_unaligned(p as *const Guid))
    }
}

// ---------------------------------------------------------------------
// IDirect3D8
// ---------------------------------------------------------------------

#[repr(C)]
struct D3d8 {
    vtbl: usize,
    refs: AtomicU32,
    d9: usize,
}

unsafe fn d3d<'a>(this: usize) -> &'a D3d8 {
    &*(this as *const D3d8)
}

unsafe extern "win64" fn d3d_qi(this: usize, iid: usize, out: usize) -> i32 {
    match read_guid(iid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_D3D8 => {
            d3d(this).refs.fetch_add(1, Ordering::AcqRel);
            put_ptr(out, this);
            S_OK
        }
        _ => {
            put_ptr(out, 0);
            E_NOINTERFACE
        }
    }
}
unsafe extern "win64" fn d3d_addref(this: usize) -> u32 {
    d3d(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn d3d_release(this: usize) -> u32 {
    let o = d3d(this);
    let n = o.refs.fetch_sub(1, Ordering::AcqRel).saturating_sub(1);
    if n == 0 {
        c9!(o.d9, 2, fn() -> u32);
        drop(Box::from_raw(this as *mut D3d8));
    }
    n
}
unsafe extern "win64" fn d3d_register_sw(_this: usize, _f: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn d3d_adapter_count(this: usize) -> u32 {
    c9!(d3d(this).d9, 4, fn() -> u32)
}

/// D3DADAPTER_IDENTIFIER8 : celui de Direct3D 9 sans DeviceName[32].
unsafe extern "win64" fn d3d_adapter_identifier(
    this: usize,
    adapter: u32,
    flags: u32,
    out: usize,
) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut id9 = vec![0u8; 1200];
    let hr = c9!(
        d3d(this).d9,
        5,
        fn(u32, u32, *mut u8) -> i32,
        adapter,
        flags,
        id9.as_mut_ptr()
    );
    if hr < 0 {
        return hr;
    }
    std::ptr::copy_nonoverlapping(id9.as_ptr(), out as *mut u8, 1024);
    std::ptr::copy_nonoverlapping(id9.as_ptr().add(1056), (out + 1024) as *mut u8, 44);
    S_OK
}

/// Formats d'écran énumérés par Direct3D 8 (tous à la suite).
const MODE_FORMATS: [u32; 2] = [22, 23]; // X8R8G8B8, R5G6B5

unsafe extern "win64" fn d3d_mode_count(this: usize, adapter: u32) -> u32 {
    MODE_FORMATS
        .iter()
        .map(|&f| c9!(d3d(this).d9, 6, fn(u32, u32) -> u32, adapter, f))
        .sum()
}

unsafe extern "win64" fn d3d_enum_modes(
    this: usize,
    adapter: u32,
    mut mode: u32,
    out: usize,
) -> i32 {
    for f in MODE_FORMATS {
        let n = c9!(d3d(this).d9, 6, fn(u32, u32) -> u32, adapter, f);
        if mode < n {
            return c9!(
                d3d(this).d9,
                7,
                fn(u32, u32, u32, usize) -> i32,
                adapter,
                f,
                mode,
                out
            );
        }
        mode -= n;
    }
    D3DERR_INVALIDCALL
}

unsafe extern "win64" fn d3d_display_mode(this: usize, adapter: u32, out: usize) -> i32 {
    c9!(d3d(this).d9, 8, fn(u32, usize) -> i32, adapter, out)
}
unsafe extern "win64" fn d3d_check_type(
    this: usize,
    a: u32,
    t: u32,
    f1: u32,
    f2: u32,
    w: i32,
) -> i32 {
    c9!(
        d3d(this).d9,
        9,
        fn(u32, u32, u32, u32, i32) -> i32,
        a,
        t,
        f1,
        f2,
        w
    )
}
unsafe extern "win64" fn d3d_check_format(
    this: usize,
    a: u32,
    t: u32,
    af: u32,
    usage: u32,
    rt: u32,
    f: u32,
) -> i32 {
    c9!(
        d3d(this).d9,
        10,
        fn(u32, u32, u32, u32, u32, u32) -> i32,
        a,
        t,
        af,
        usage,
        rt,
        f
    )
}
unsafe extern "win64" fn d3d_check_ms(this: usize, a: u32, t: u32, f: u32, w: i32, ms: u32) -> i32 {
    c9!(
        d3d(this).d9,
        11,
        fn(u32, u32, u32, i32, u32, usize) -> i32,
        a,
        t,
        f,
        w,
        ms,
        0
    )
}
unsafe extern "win64" fn d3d_check_ds(
    this: usize,
    a: u32,
    t: u32,
    af: u32,
    rt: u32,
    ds: u32,
) -> i32 {
    c9!(
        d3d(this).d9,
        12,
        fn(u32, u32, u32, u32, u32) -> i32,
        a,
        t,
        af,
        rt,
        ds
    )
}

/// D3DCAPS8 = les 53 premiers champs de D3DCAPS9 ; versions de shaders
/// ramenées à vs.1.1 et ps.1.4, les plus hautes de Direct3D 8.
unsafe fn caps8(caps9: &[u8], out: usize) {
    std::ptr::copy_nonoverlapping(caps9.as_ptr(), out as *mut u8, 212);
    let vs = rd32(out + 49 * 4);
    if vs & 0xFFFF > 0x0101 {
        wr32(out + 49 * 4, 0xFFFE_0101);
    }
    let ps = rd32(out + 51 * 4);
    if ps & 0xFFFF > 0x0104 {
        wr32(out + 51 * 4, 0xFFFF_0104);
    }
}

unsafe extern "win64" fn d3d_caps(this: usize, a: u32, t: u32, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut c = vec![0u8; 512];
    let hr = c9!(
        d3d(this).d9,
        14,
        fn(u32, u32, *mut u8) -> i32,
        a,
        t,
        c.as_mut_ptr()
    );
    if hr >= 0 {
        caps8(&c, out);
    }
    hr
}
unsafe extern "win64" fn d3d_monitor(this: usize, a: u32) -> usize {
    c9!(d3d(this).d9, 15, fn(u32) -> usize, a)
}

/// D3DPRESENT_PARAMETERS de Direct3D 8 (13 champs, hDeviceWindow en 7e
/// position) → Direct3D 9.
unsafe fn present_params9(pp: usize) -> super::d3d9::PresentParams {
    let ptr = super::legacy_com::ptr_size();
    let hwnd_at = 24;
    let after = hwnd_at + ptr;
    let d = |o: usize| rd32(pp + o);
    let swap = match d(20) {
        4 => 3, // D3DSWAPEFFECT_COPY_VSYNC → COPY
        s => s,
    };
    let windowed = d(after) as i32;
    let interval = if windowed != 0 {
        // En fenêtre, Direct3D 8 synchronise sauf COPY ; Direct3D 9 suit
        // PresentationInterval.
        if d(20) == 3 {
            0x8000_0000
        } else {
            0
        }
    } else {
        d(after + 16)
    };
    super::d3d9::PresentParams {
        width: d(0),
        height: d(4),
        format: d(8),
        count: d(12),
        ms_type: d(16),
        ms_quality: 0,
        swap_effect: swap,
        device_window: super::legacy_com::get_ptr(pp + hwnd_at),
        windowed,
        auto_depth: d(after + 4) as i32,
        auto_depth_format: d(after + 8),
        flags: d(after + 12) & 0x3, // LOCKABLE_BACKBUFFER | DISCARD_DEPTHSTENCIL
        refresh: d(after + 16 - 4),
        interval,
    }
}

/// Valeurs choisies par Direct3D 9 (taille, format du tampon arrière)
/// recopiées dans la structure du programme.
unsafe fn write_back_params(pp: usize, p9: &super::d3d9::PresentParams) {
    wr32(pp, p9.width);
    wr32(pp + 4, p9.height);
    wr32(pp + 8, p9.format);
    wr32(pp + 12, p9.count);
}

unsafe extern "win64" fn d3d_create_device(
    this: usize,
    adapter: u32,
    ty: u32,
    hwnd: usize,
    flags: u32,
    pp: usize,
    out: usize,
) -> i32 {
    if pp == 0 || out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut p9 = present_params9(pp);
    let mut dev9: usize = 0;
    let hr = c9!(
        d3d(this).d9,
        16,
        fn(u32, u32, usize, u32, *mut super::d3d9::PresentParams, *mut usize) -> i32,
        adapter,
        ty,
        hwnd,
        flags,
        &mut p9,
        &mut dev9
    );
    if hr < 0 || dev9 == 0 {
        put_ptr(out, 0);
        return if hr < 0 { hr } else { D3DERR_NOTAVAILABLE };
    }
    write_back_params(pp, &p9);
    d3d(this).refs.fetch_add(1, Ordering::AcqRel);
    let dev = Box::leak(Box::new(Device {
        vtbl: device_vtable(),
        refs: AtomicU32::new(1),
        d9: dev9,
        d3d8: this,
        st: Mutex::new(DevState {
            adapter,
            ty,
            focus: hwnd,
            flags,
            ..DevState::default()
        }),
    }));
    put_ptr(out, dev as *const Device as usize);
    S_OK
}

fn d3d_vtable() -> usize {
    let a = |f: *const (), n: u8| (f as usize, n);
    vtable(
        "IDirect3D8",
        &[
            a(d3d_qi as *const (), 2),
            a(d3d_addref as *const (), 0),
            a(d3d_release as *const (), 0),
            a(d3d_register_sw as *const (), 1),
            a(d3d_adapter_count as *const (), 0),
            a(d3d_adapter_identifier as *const (), 3),
            a(d3d_mode_count as *const (), 1),
            a(d3d_enum_modes as *const (), 3),
            a(d3d_display_mode as *const (), 2),
            a(d3d_check_type as *const (), 5),
            a(d3d_check_format as *const (), 6),
            a(d3d_check_ms as *const (), 5),
            a(d3d_check_ds as *const (), 5),
            a(d3d_caps as *const (), 3),
            a(d3d_monitor as *const (), 1),
            a(d3d_create_device as *const (), 6),
        ],
    )
}

/// Direct3DCreate8(SDKVersion).
pub unsafe extern "win64" fn direct3d_create8(_sdk: u32) -> usize {
    let Some(create) = crate::dlls::lookup("d3d9", "Direct3DCreate9") else {
        return 0;
    };
    let f: unsafe extern "win64" fn(u32) -> usize = std::mem::transmute(create);
    let d9 = f(32);
    if d9 == 0 {
        return 0;
    }
    Box::leak(Box::new(D3d8 {
        vtbl: d3d_vtable(),
        refs: AtomicU32::new(1),
        d9,
    })) as *const D3d8 as usize
}

// ---------------------------------------------------------------------
// Ressources : surfaces, textures, tampons
// ---------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Surface,
    Texture,
    Cube,
    Vertices,
    Indices,
}

#[repr(C)]
struct Res {
    vtbl: usize,
    refs: AtomicU32,
    d9: usize,
    dev: usize,
    kind: Kind,
}

unsafe fn res<'a>(this: usize) -> &'a Res {
    &*(this as *const Res)
}

/// Objet Direct3D 9 enveloppé par `p` (0 → 0).
unsafe fn inner(p: usize) -> usize {
    if p == 0 {
        0
    } else {
        res(p).d9
    }
}

/// Enveloppe de l'objet Direct3D 9 `d9` (dont on reçoit une référence) :
/// une seule enveloppe par objet, gardée par l'appareil.
unsafe fn wrap(dev: usize, d9: usize, kind: Kind) -> usize {
    if d9 == 0 {
        return 0;
    }
    let dv = device(dev);
    let mut st = lock(&dv.st);
    if let Some(&w) = st.wrappers.get(&d9) {
        res(w).refs.fetch_add(1, Ordering::AcqRel);
        c9!(d9, 2, fn() -> u32);
        return w;
    }
    let vt = match kind {
        Kind::Surface => surface_vtable(),
        Kind::Texture => texture_vtable(),
        Kind::Cube => cube_vtable(),
        Kind::Vertices | Kind::Indices => buffer_vtable(),
    };
    dv.refs.fetch_add(1, Ordering::AcqRel);
    let w = Box::leak(Box::new(Res {
        vtbl: vt,
        refs: AtomicU32::new(1),
        d9,
        dev,
        kind,
    })) as *const Res as usize;
    st.wrappers.insert(d9, w);
    w
}

unsafe extern "win64" fn r_qi(this: usize, iid: usize, out: usize) -> i32 {
    let r = res(this);
    let ok = match read_guid(iid) {
        Some(g) => {
            g == IID_IUNKNOWN
                || (g == IID_RESOURCE8)
                || match r.kind {
                    Kind::Surface => g == IID_SURFACE8,
                    Kind::Texture => g == IID_BASETEX8 || g == IID_TEXTURE8,
                    Kind::Cube => g == IID_BASETEX8 || g == IID_CUBE8,
                    Kind::Vertices => g == IID_VB8,
                    Kind::Indices => g == IID_IB8,
                }
        }
        None => false,
    };
    if ok {
        r.refs.fetch_add(1, Ordering::AcqRel);
        put_ptr(out, this);
        S_OK
    } else {
        put_ptr(out, 0);
        E_NOINTERFACE
    }
}
unsafe extern "win64" fn r_addref(this: usize) -> u32 {
    res(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn r_release(this: usize) -> u32 {
    let r = res(this);
    let n = r.refs.fetch_sub(1, Ordering::AcqRel).saturating_sub(1);
    if n == 0 {
        let dev = r.dev;
        lock(&device(dev).st).wrappers.remove(&r.d9);
        c9!(r.d9, 2, fn() -> u32);
        drop(Box::from_raw(this as *mut Res));
        dev_release(dev);
    }
    n
}
unsafe extern "win64" fn r_get_device(this: usize, out: usize) -> i32 {
    let dev = res(this).dev;
    device(dev).refs.fetch_add(1, Ordering::AcqRel);
    put_ptr(out, dev);
    S_OK
}
unsafe extern "win64" fn r_set_private(
    this: usize,
    g: usize,
    data: usize,
    size: u32,
    flags: u32,
) -> i32 {
    c9!(
        inner(this),
        4,
        fn(usize, usize, u32, u32) -> i32,
        g,
        data,
        size,
        flags
    )
}
unsafe extern "win64" fn r_get_private(this: usize, g: usize, data: usize, size: usize) -> i32 {
    c9!(
        inner(this),
        5,
        fn(usize, usize, usize) -> i32,
        g,
        data,
        size
    )
}
unsafe extern "win64" fn r_free_private(this: usize, g: usize) -> i32 {
    c9!(inner(this), 6, fn(usize) -> i32, g)
}
unsafe extern "win64" fn r_set_priority(this: usize, p: u32) -> u32 {
    c9!(inner(this), 7, fn(u32) -> u32, p)
}
unsafe extern "win64" fn r_get_priority(this: usize) -> u32 {
    c9!(inner(this), 8, fn() -> u32)
}
unsafe extern "win64" fn r_preload(this: usize) -> i32 {
    c9!(inner(this), 9, fn() -> i32)
}
unsafe extern "win64" fn r_get_type(this: usize) -> u32 {
    c9!(inner(this), 10, fn() -> u32)
}
unsafe extern "win64" fn r_set_lod(this: usize, l: u32) -> u32 {
    c9!(inner(this), 11, fn(u32) -> u32, l)
}
unsafe extern "win64" fn r_get_lod(this: usize) -> u32 {
    c9!(inner(this), 12, fn() -> u32)
}
unsafe extern "win64" fn r_level_count(this: usize) -> u32 {
    c9!(inner(this), 13, fn() -> u32)
}

/// D3DSURFACE_DESC 9 → 8 : Size remplace MultiSampleQuality, avant
/// MultiSampleType.
unsafe fn surface_desc8(d9: &[u32; 8], out: usize, bpp_hint: u32) {
    let (fmt, ty, usage, pool, ms, _q, w, h) =
        (d9[0], d9[1], d9[2], d9[3], d9[4], d9[5], d9[6], d9[7]);
    wr32(out, fmt);
    wr32(out + 4, ty);
    wr32(out + 8, usage);
    wr32(out + 12, pool);
    wr32(out + 16, w * h * bpp_hint / 8);
    wr32(out + 20, ms);
    wr32(out + 24, w);
    wr32(out + 28, h);
}

/// Bits par pixel d'un D3DFORMAT courant (taille indicative de
/// D3DSURFACE_DESC8.Size).
fn format_bits(f: u32) -> u32 {
    match f {
        20 => 24,                                    // R8G8B8
        21 | 22 | 31 | 32 | 35 | 77 | 71 | 75 => 32, // A8R8G8B8, X8R8G8B8…
        23..=26 | 29 | 30 | 70 | 73 | 80 | 81 => 16, // R5G6B5, X1R5G5B5…
        27 | 28 | 41 | 50 | 51 => 8,                 // R3G3B2, A8, P8, L8, A8L8
        0x3154_5844 => 4,                            // DXT1
        0x3254_5844..=0x3554_5844 => 8,              // DXT2..5
        _ => 32,
    }
}

unsafe fn get_desc(obj9: usize, idx: usize, level: Option<u32>, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut d = [0u32; 8];
    let hr = match level {
        Some(l) => c9!(obj9, idx, fn(u32, *mut u32) -> i32, l, d.as_mut_ptr()),
        None => c9!(obj9, idx, fn(*mut u32) -> i32, d.as_mut_ptr()),
    };
    if hr >= 0 {
        surface_desc8(&d, out, format_bits(d[0]));
    }
    hr
}

/// D3DLOCKED_RECT { INT Pitch; void* pBits } à la disposition du programme.
/// `lr` est rempli par Direct3D 9, lui aussi à la disposition du programme.
unsafe fn write_locked(out: usize, lr: &[u64; 2]) {
    let (pitch, bits) = super::legacy_com::locked_rect(lr.as_ptr() as usize);
    wr32(out, pitch as u32);
    put_ptr(out + super::legacy_com::ptr_size(), bits);
}

// --- Surface8 ---

unsafe extern "win64" fn s_get_container(_this: usize, _iid: usize, out: usize) -> i32 {
    put_ptr(out, 0);
    E_NOINTERFACE
}
unsafe extern "win64" fn s_get_desc(this: usize, out: usize) -> i32 {
    get_desc(inner(this), 12, None, out)
}
unsafe extern "win64" fn s_lock(this: usize, out: usize, rect: usize, flags: u32) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut lr = [0u64; 2];
    let hr = c9!(
        inner(this),
        13,
        fn(*mut u64, usize, u32) -> i32,
        lr.as_mut_ptr(),
        rect,
        flags
    );
    if hr >= 0 {
        write_locked(out, &lr);
    }
    hr
}
unsafe extern "win64" fn s_unlock(this: usize) -> i32 {
    c9!(inner(this), 14, fn() -> i32)
}

fn surface_vtable() -> usize {
    let a = |f: *const (), n: u8| (f as usize, n);
    vtable(
        "IDirect3DSurface8",
        &[
            a(r_qi as *const (), 2),
            a(r_addref as *const (), 0),
            a(r_release as *const (), 0),
            a(r_get_device as *const (), 1),
            a(r_set_private as *const (), 4),
            a(r_get_private as *const (), 3),
            a(r_free_private as *const (), 1),
            a(s_get_container as *const (), 2),
            a(s_get_desc as *const (), 1),
            a(s_lock as *const (), 3),
            a(s_unlock as *const (), 0),
        ],
    )
}

// --- Texture8 / CubeTexture8 ---

unsafe extern "win64" fn t_level_desc(this: usize, level: u32, out: usize) -> i32 {
    get_desc(inner(this), 17, Some(level), out)
}
unsafe extern "win64" fn t_surface_level(this: usize, level: u32, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut s: usize = 0;
    let hr = c9!(inner(this), 18, fn(u32, *mut usize) -> i32, level, &mut s);
    put_ptr(
        out,
        if hr >= 0 {
            wrap(res(this).dev, s, Kind::Surface)
        } else {
            0
        },
    );
    hr
}
unsafe extern "win64" fn t_lock(
    this: usize,
    level: u32,
    out: usize,
    rect: usize,
    flags: u32,
) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut lr = [0u64; 2];
    let hr = c9!(
        inner(this),
        19,
        fn(u32, *mut u64, usize, u32) -> i32,
        level,
        lr.as_mut_ptr(),
        rect,
        flags
    );
    if hr >= 0 {
        write_locked(out, &lr);
    }
    hr
}
unsafe extern "win64" fn t_unlock(this: usize, level: u32) -> i32 {
    c9!(inner(this), 20, fn(u32) -> i32, level)
}
unsafe extern "win64" fn t_dirty(this: usize, rect: usize) -> i32 {
    c9!(inner(this), 21, fn(usize) -> i32, rect)
}

unsafe extern "win64" fn c_cube_surface(this: usize, face: u32, level: u32, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut s: usize = 0;
    let hr = c9!(
        inner(this),
        18,
        fn(u32, u32, *mut usize) -> i32,
        face,
        level,
        &mut s
    );
    put_ptr(
        out,
        if hr >= 0 {
            wrap(res(this).dev, s, Kind::Surface)
        } else {
            0
        },
    );
    hr
}
unsafe extern "win64" fn c_lock(
    this: usize,
    face: u32,
    level: u32,
    out: usize,
    rect: usize,
    flags: u32,
) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut lr = [0u64; 2];
    let hr = c9!(
        inner(this),
        19,
        fn(u32, u32, *mut u64, usize, u32) -> i32,
        face,
        level,
        lr.as_mut_ptr(),
        rect,
        flags
    );
    if hr >= 0 {
        write_locked(out, &lr);
    }
    hr
}
unsafe extern "win64" fn c_unlock(this: usize, face: u32, level: u32) -> i32 {
    c9!(inner(this), 20, fn(u32, u32) -> i32, face, level)
}
unsafe extern "win64" fn c_dirty(this: usize, face: u32, rect: usize) -> i32 {
    c9!(inner(this), 21, fn(u32, usize) -> i32, face, rect)
}

fn base_texture_methods() -> Vec<(usize, u8)> {
    let a = |f: *const (), n: u8| (f as usize, n);
    vec![
        a(r_qi as *const (), 2),
        a(r_addref as *const (), 0),
        a(r_release as *const (), 0),
        a(r_get_device as *const (), 1),
        a(r_set_private as *const (), 4),
        a(r_get_private as *const (), 3),
        a(r_free_private as *const (), 1),
        a(r_set_priority as *const (), 1),
        a(r_get_priority as *const (), 0),
        a(r_preload as *const (), 0),
        a(r_get_type as *const (), 0),
        a(r_set_lod as *const (), 1),
        a(r_get_lod as *const (), 0),
        a(r_level_count as *const (), 0),
    ]
}

fn texture_vtable() -> usize {
    let a = |f: *const (), n: u8| (f as usize, n);
    let mut m = base_texture_methods();
    m.extend([
        a(t_level_desc as *const (), 2),
        a(t_surface_level as *const (), 2),
        a(t_lock as *const (), 4),
        a(t_unlock as *const (), 1),
        a(t_dirty as *const (), 1),
    ]);
    vtable("IDirect3DTexture8", &m)
}

fn cube_vtable() -> usize {
    let a = |f: *const (), n: u8| (f as usize, n);
    let mut m = base_texture_methods();
    m.extend([
        a(t_level_desc as *const (), 2),
        a(c_cube_surface as *const (), 3),
        a(c_lock as *const (), 5),
        a(c_unlock as *const (), 2),
        a(c_dirty as *const (), 2),
    ]);
    vtable("IDirect3DCubeTexture8", &m)
}

// --- VertexBuffer8 / IndexBuffer8 (mêmes méthodes, mêmes descriptions) ---

unsafe extern "win64" fn b_lock(
    this: usize,
    offset: u32,
    size: u32,
    out: usize,
    flags: u32,
) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut p: usize = 0;
    let hr = c9!(
        inner(this),
        11,
        fn(u32, u32, *mut usize, u32) -> i32,
        offset,
        size,
        &mut p,
        flags
    );
    put_ptr(out, if hr >= 0 { p } else { 0 });
    hr
}
unsafe extern "win64" fn b_unlock(this: usize) -> i32 {
    c9!(inner(this), 12, fn() -> i32)
}
unsafe extern "win64" fn b_desc(this: usize, out: usize) -> i32 {
    c9!(inner(this), 13, fn(usize) -> i32, out)
}

fn buffer_vtable() -> usize {
    let a = |f: *const (), n: u8| (f as usize, n);
    vtable(
        "IDirect3DBuffer8",
        &[
            a(r_qi as *const (), 2),
            a(r_addref as *const (), 0),
            a(r_release as *const (), 0),
            a(r_get_device as *const (), 1),
            a(r_set_private as *const (), 4),
            a(r_get_private as *const (), 3),
            a(r_free_private as *const (), 1),
            a(r_set_priority as *const (), 1),
            a(r_get_priority as *const (), 0),
            a(r_preload as *const (), 0),
            a(r_get_type as *const (), 0),
            a(b_lock as *const (), 4),
            a(b_unlock as *const (), 0),
            a(b_desc as *const (), 1),
        ],
    )
}

// ---------------------------------------------------------------------
// IDirect3DDevice8
// ---------------------------------------------------------------------

/// Shader de sommets Direct3D 8 : déclaration et shader Direct3D 9, et
/// constantes données par la déclaration.
struct VShader {
    decl9: usize,
    vs9: usize,
    consts: Vec<(u32, Vec<f32>)>,
    decl_tokens: Vec<u32>,
    function: Vec<u32>,
}

#[derive(Default)]
struct DevState {
    adapter: u32,
    ty: u32,
    focus: usize,
    flags: u32,
    /// Objet Direct3D 9 → son enveloppe.
    wrappers: HashMap<usize, usize>,
    textures: [usize; 8],
    streams: [(usize, u32); 16],
    indices: (usize, u32),
    vs: u32,
    vshaders: Vec<Option<VShader>>,
    pshaders: Vec<Option<(usize, Vec<u32>)>>,
    ps: u32,
    blocks: HashMap<u32, usize>,
    next_block: u32,
}

#[repr(C)]
struct Device {
    vtbl: usize,
    refs: AtomicU32,
    d9: usize,
    d3d8: usize,
    st: Mutex<DevState>,
}

unsafe fn device<'a>(this: usize) -> &'a Device {
    &*(this as *const Device)
}

fn d9(this: usize) -> usize {
    unsafe { device(this).d9 }
}

unsafe fn dev_release(this: usize) -> u32 {
    let d = device(this);
    let n = d.refs.fetch_sub(1, Ordering::AcqRel).saturating_sub(1);
    if n == 0 {
        let st = std::mem::take(&mut *lock(&d.st));
        for v in st.vshaders.into_iter().flatten() {
            if v.vs9 != 0 {
                c9!(v.vs9, 2, fn() -> u32);
            }
            if v.decl9 != 0 {
                c9!(v.decl9, 2, fn() -> u32);
            }
        }
        for (p, _) in st.pshaders.into_iter().flatten() {
            c9!(p, 2, fn() -> u32);
        }
        for b in st.blocks.into_values() {
            c9!(b, 2, fn() -> u32);
        }
        c9!(d.d9, 2, fn() -> u32);
        let d3d8 = d.d3d8;
        drop(Box::from_raw(this as *mut Device));
        d3d_release(d3d8);
    }
    n
}

unsafe extern "win64" fn d_qi(this: usize, iid: usize, out: usize) -> i32 {
    match read_guid(iid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_DEVICE8 => {
            device(this).refs.fetch_add(1, Ordering::AcqRel);
            put_ptr(out, this);
            S_OK
        }
        _ => {
            put_ptr(out, 0);
            E_NOINTERFACE
        }
    }
}
unsafe extern "win64" fn d_addref(this: usize) -> u32 {
    device(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn d_release(this: usize) -> u32 {
    dev_release(this)
}
unsafe extern "win64" fn d_test_coop(this: usize) -> i32 {
    c9!(d9(this), 3, fn() -> i32)
}
unsafe extern "win64" fn d_tex_mem(this: usize) -> u32 {
    c9!(d9(this), 4, fn() -> u32)
}
unsafe extern "win64" fn d_discard_bytes(this: usize, _bytes: u32) -> i32 {
    c9!(d9(this), 5, fn() -> i32)
}
unsafe extern "win64" fn d_get_d3d(this: usize, out: usize) -> i32 {
    let o = device(this).d3d8;
    d3d(o).refs.fetch_add(1, Ordering::AcqRel);
    put_ptr(out, o);
    S_OK
}
unsafe extern "win64" fn d_caps(this: usize, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut c = vec![0u8; 512];
    let hr = c9!(d9(this), 7, fn(*mut u8) -> i32, c.as_mut_ptr());
    if hr >= 0 {
        caps8(&c, out);
    }
    hr
}
unsafe extern "win64" fn d_display_mode(this: usize, out: usize) -> i32 {
    c9!(d9(this), 8, fn(u32, usize) -> i32, 0, out)
}
/// D3DDEVICE_CREATION_PARAMETERS { UINT; D3DDEVTYPE; HWND; DWORD }.
unsafe extern "win64" fn d_creation_params(this: usize, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let st = lock(&device(this).st);
    let p = super::legacy_com::ptr_size();
    wr32(out, st.adapter);
    wr32(out + 4, st.ty);
    put_ptr(out + 8, st.focus);
    wr32(out + 8 + p, st.flags);
    S_OK
}
unsafe extern "win64" fn d_cursor_props(this: usize, x: u32, y: u32, surf: usize) -> i32 {
    c9!(d9(this), 10, fn(u32, u32, usize) -> i32, x, y, inner(surf))
}
unsafe extern "win64" fn d_cursor_pos(this: usize, x: i32, y: i32, flags: u32) -> i32 {
    c9!(d9(this), 11, fn(i32, i32, u32) -> i32, x, y, flags);
    S_OK
}
unsafe extern "win64" fn d_show_cursor(this: usize, show: i32) -> i32 {
    c9!(d9(this), 12, fn(i32) -> i32, show)
}
unsafe extern "win64" fn d_reset(this: usize, pp: usize) -> i32 {
    if pp == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut p9 = present_params9(pp);
    let hr = c9!(
        d9(this),
        16,
        fn(*mut super::d3d9::PresentParams) -> i32,
        &mut p9
    );
    if hr >= 0 {
        write_back_params(pp, &p9);
    }
    hr
}
unsafe extern "win64" fn d_present(
    this: usize,
    src: usize,
    dst: usize,
    hwnd: usize,
    dirty: usize,
) -> i32 {
    c9!(
        d9(this),
        17,
        fn(usize, usize, usize, usize) -> i32,
        src,
        dst,
        hwnd,
        dirty
    )
}
unsafe extern "win64" fn d_back_buffer(this: usize, idx: u32, ty: u32, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut s: usize = 0;
    let hr = c9!(
        d9(this),
        18,
        fn(u32, u32, u32, *mut usize) -> i32,
        0,
        idx,
        ty,
        &mut s
    );
    put_ptr(
        out,
        if hr >= 0 {
            wrap(this, s, Kind::Surface)
        } else {
            0
        },
    );
    hr
}
unsafe extern "win64" fn d_raster_status(this: usize, out: usize) -> i32 {
    c9!(d9(this), 19, fn(u32, usize) -> i32, 0, out)
}
unsafe extern "win64" fn d_set_gamma(this: usize, flags: u32, ramp: usize) -> i32 {
    c9!(d9(this), 21, fn(u32, u32, usize) -> i32, 0, flags, ramp);
    S_OK
}
unsafe extern "win64" fn d_get_gamma(this: usize, ramp: usize) -> i32 {
    c9!(d9(this), 22, fn(u32, usize) -> i32, 0, ramp);
    S_OK
}

unsafe fn created(this: usize, hr: i32, obj: usize, kind: Kind, out: usize) -> i32 {
    put_ptr(out, if hr >= 0 { wrap(this, obj, kind) } else { 0 });
    hr
}

unsafe extern "win64" fn d_create_texture(
    this: usize,
    w: u32,
    h: u32,
    levels: u32,
    usage: u32,
    fmt: u32,
    pool: u32,
    out: usize,
) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut t: usize = 0;
    let hr = c9!(
        d9(this),
        23,
        fn(u32, u32, u32, u32, u32, u32, *mut usize, usize) -> i32,
        w,
        h,
        levels,
        usage,
        fmt,
        pool,
        &mut t,
        0
    );
    created(this, hr, t, Kind::Texture, out)
}
unsafe extern "win64" fn d_create_volume(
    _this: usize,
    _w: u32,
    _h: u32,
    _d: u32,
    _l: u32,
    _u: u32,
    _f: u32,
    _p: u32,
    out: usize,
) -> i32 {
    put_ptr(out, 0);
    D3DERR_NOTAVAILABLE
}
unsafe extern "win64" fn d_create_cube(
    this: usize,
    edge: u32,
    levels: u32,
    usage: u32,
    fmt: u32,
    pool: u32,
    out: usize,
) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut t: usize = 0;
    let hr = c9!(
        d9(this),
        25,
        fn(u32, u32, u32, u32, u32, *mut usize, usize) -> i32,
        edge,
        levels,
        usage,
        fmt,
        pool,
        &mut t,
        0
    );
    created(this, hr, t, Kind::Cube, out)
}
unsafe extern "win64" fn d_create_vb(
    this: usize,
    len: u32,
    usage: u32,
    fvf: u32,
    pool: u32,
    out: usize,
) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut b: usize = 0;
    let hr = c9!(
        d9(this),
        26,
        fn(u32, u32, u32, u32, *mut usize, usize) -> i32,
        len,
        usage,
        fvf,
        pool,
        &mut b,
        0
    );
    created(this, hr, b, Kind::Vertices, out)
}
unsafe extern "win64" fn d_create_ib(
    this: usize,
    len: u32,
    usage: u32,
    fmt: u32,
    pool: u32,
    out: usize,
) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut b: usize = 0;
    let hr = c9!(
        d9(this),
        27,
        fn(u32, u32, u32, u32, *mut usize, usize) -> i32,
        len,
        usage,
        fmt,
        pool,
        &mut b,
        0
    );
    created(this, hr, b, Kind::Indices, out)
}
unsafe extern "win64" fn d_create_rt(
    this: usize,
    w: u32,
    h: u32,
    fmt: u32,
    ms: u32,
    lockable: i32,
    out: usize,
) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut s: usize = 0;
    let hr = c9!(
        d9(this),
        28,
        fn(u32, u32, u32, u32, u32, i32, *mut usize, usize) -> i32,
        w,
        h,
        fmt,
        ms,
        0,
        lockable,
        &mut s,
        0
    );
    created(this, hr, s, Kind::Surface, out)
}
unsafe extern "win64" fn d_create_ds(
    this: usize,
    w: u32,
    h: u32,
    fmt: u32,
    ms: u32,
    out: usize,
) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut s: usize = 0;
    let hr = c9!(
        d9(this),
        29,
        fn(u32, u32, u32, u32, u32, i32, *mut usize, usize) -> i32,
        w,
        h,
        fmt,
        ms,
        0,
        0,
        &mut s,
        0
    );
    created(this, hr, s, Kind::Surface, out)
}
/// CreateImageSurface : surface hors écran en mémoire système.
unsafe extern "win64" fn d_create_image(this: usize, w: u32, h: u32, fmt: u32, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut s: usize = 0;
    let hr = c9!(
        d9(this),
        36,
        fn(u32, u32, u32, u32, *mut usize, usize) -> i32,
        w,
        h,
        fmt,
        2, // D3DPOOL_SYSTEMMEM
        &mut s,
        0
    );
    created(this, hr, s, Kind::Surface, out)
}

/// CopyRects : copie octet à octet entre deux surfaces de même format
/// (verrouillées), ou par StretchRect quand une surface ne se verrouille
/// pas (cible de rendu).
unsafe extern "win64" fn d_copy_rects(
    this: usize,
    src: usize,
    rects: usize,
    n: u32,
    dst: usize,
    points: usize,
) -> i32 {
    let (s9, t9) = (inner(src), inner(dst));
    if s9 == 0 || t9 == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut sd = [0u32; 8];
    let mut td = [0u32; 8];
    c9!(s9, 12, fn(*mut u32) -> i32, sd.as_mut_ptr());
    c9!(t9, 12, fn(*mut u32) -> i32, td.as_mut_ptr());
    let full = [0i32, 0, sd[6] as i32, sd[7] as i32];
    let count = if rects == 0 { 1 } else { n.max(1) as usize };
    let bpp = (format_bits(sd[0]) / 8).max(1) as usize;
    for i in 0..count {
        let r: [i32; 4] = if rects == 0 {
            full
        } else {
            std::ptr::read_unaligned((rects + 16 * i) as *const [i32; 4])
        };
        let (px, py) = if points == 0 {
            (r[0], r[1])
        } else {
            std::ptr::read_unaligned((points + 8 * i) as *const (i32, i32))
        };
        let (w, h) = (r[2] - r[0], r[3] - r[1]);
        if w <= 0 || h <= 0 {
            continue;
        }
        let mut sl = [0u64; 2];
        let mut tl = [0u64; 2];
        let src_rect = r;
        let dst_rect = [px, py, px + w, py + h];
        let ok_s = c9!(
            s9,
            13,
            fn(*mut u64, *const i32, u32) -> i32,
            sl.as_mut_ptr(),
            src_rect.as_ptr(),
            0x10
        ) >= 0;
        let ok_t = ok_s
            && c9!(
                t9,
                13,
                fn(*mut u64, *const i32, u32) -> i32,
                tl.as_mut_ptr(),
                dst_rect.as_ptr(),
                0
            ) >= 0;
        if ok_s && ok_t && sd[0] == td[0] {
            let (sp, sb) = super::legacy_com::locked_rect(sl.as_ptr() as usize);
            let (tp, tb) = super::legacy_com::locked_rect(tl.as_ptr() as usize);
            for y in 0..h as usize {
                std::ptr::copy(
                    (sb + y * sp as usize) as *const u8,
                    (tb + y * tp as usize) as *mut u8,
                    w as usize * bpp,
                );
            }
        }
        if ok_t {
            c9!(t9, 14, fn() -> i32);
        }
        if ok_s {
            c9!(s9, 14, fn() -> i32);
        }
        if !(ok_s && ok_t && sd[0] == td[0]) {
            c9!(
                d9(this),
                34,
                fn(usize, *const i32, usize, *const i32, u32) -> i32,
                s9,
                src_rect.as_ptr(),
                t9,
                dst_rect.as_ptr(),
                0
            );
        }
    }
    S_OK
}
unsafe extern "win64" fn d_update_texture(this: usize, src: usize, dst: usize) -> i32 {
    c9!(
        d9(this),
        31,
        fn(usize, usize) -> i32,
        inner(src),
        inner(dst)
    )
}
unsafe extern "win64" fn d_front_buffer(_this: usize, _dst: usize) -> i32 {
    D3DERR_INVALIDCALL
}
unsafe extern "win64" fn d_set_rt(this: usize, rt: usize, ds: usize) -> i32 {
    if rt != 0 {
        let hr = c9!(d9(this), 37, fn(u32, usize) -> i32, 0, inner(rt));
        if hr < 0 {
            return hr;
        }
    }
    c9!(d9(this), 39, fn(usize) -> i32, inner(ds))
}
unsafe extern "win64" fn d_get_rt(this: usize, out: usize) -> i32 {
    let mut s: usize = 0;
    let hr = c9!(d9(this), 38, fn(u32, *mut usize) -> i32, 0, &mut s);
    created(this, hr, s, Kind::Surface, out)
}
unsafe extern "win64" fn d_get_ds(this: usize, out: usize) -> i32 {
    let mut s: usize = 0;
    let hr = c9!(d9(this), 40, fn(*mut usize) -> i32, &mut s);
    if hr < 0 || s == 0 {
        put_ptr(out, 0);
        return if hr < 0 { hr } else { 0x8876_087Cu32 as i32 }; // D3DERR_NOTFOUND
    }
    created(this, hr, s, Kind::Surface, out)
}
unsafe extern "win64" fn d_begin_scene(this: usize) -> i32 {
    c9!(d9(this), 41, fn() -> i32)
}
unsafe extern "win64" fn d_end_scene(this: usize) -> i32 {
    c9!(d9(this), 42, fn() -> i32)
}
unsafe extern "win64" fn d_clear(
    this: usize,
    n: u32,
    rects: usize,
    flags: u32,
    color: u32,
    z: f32,
    stencil: u32,
) -> i32 {
    c9!(
        d9(this),
        43,
        fn(u32, usize, u32, u32, f32, u32) -> i32,
        n,
        rects,
        flags,
        color,
        z,
        stencil
    )
}
unsafe extern "win64" fn d_set_transform(this: usize, s: u32, m: usize) -> i32 {
    c9!(d9(this), 44, fn(u32, usize) -> i32, s, m)
}
unsafe extern "win64" fn d_get_transform(this: usize, s: u32, m: usize) -> i32 {
    c9!(d9(this), 45, fn(u32, usize) -> i32, s, m)
}
unsafe extern "win64" fn d_mul_transform(this: usize, s: u32, m: usize) -> i32 {
    c9!(d9(this), 46, fn(u32, usize) -> i32, s, m)
}
unsafe extern "win64" fn d_set_viewport(this: usize, v: usize) -> i32 {
    c9!(d9(this), 47, fn(usize) -> i32, v)
}
unsafe extern "win64" fn d_get_viewport(this: usize, v: usize) -> i32 {
    c9!(d9(this), 48, fn(usize) -> i32, v)
}
unsafe extern "win64" fn d_set_material(this: usize, m: usize) -> i32 {
    c9!(d9(this), 49, fn(usize) -> i32, m)
}
unsafe extern "win64" fn d_get_material(this: usize, m: usize) -> i32 {
    c9!(d9(this), 50, fn(usize) -> i32, m)
}
unsafe extern "win64" fn d_set_light(this: usize, i: u32, l: usize) -> i32 {
    c9!(d9(this), 51, fn(u32, usize) -> i32, i, l)
}
unsafe extern "win64" fn d_get_light(this: usize, i: u32, l: usize) -> i32 {
    c9!(d9(this), 52, fn(u32, usize) -> i32, i, l)
}
unsafe extern "win64" fn d_light_enable(this: usize, i: u32, on: i32) -> i32 {
    c9!(d9(this), 53, fn(u32, i32) -> i32, i, on)
}
unsafe extern "win64" fn d_get_light_enable(this: usize, i: u32, out: usize) -> i32 {
    c9!(d9(this), 54, fn(u32, usize) -> i32, i, out)
}
unsafe extern "win64" fn d_set_clip_plane(this: usize, i: u32, p: usize) -> i32 {
    c9!(d9(this), 55, fn(u32, usize) -> i32, i, p)
}
unsafe extern "win64" fn d_get_clip_plane(this: usize, i: u32, p: usize) -> i32 {
    c9!(d9(this), 56, fn(u32, usize) -> i32, i, p)
}

const D3DRS_ZBIAS: u32 = 47;
const D3DRS_DEPTHBIAS: u32 = 195;
/// États de rendu Direct3D 8 sans équivalent en Direct3D 9.
const RS_GONE: [u32; 5] = [10, 30, 40, 153, 164]; // LINEPATTERN, ZVISIBLE, EDGEANTIALIAS, SOFTWAREVERTEXPROCESSING, PATCHSEGMENTS

unsafe extern "win64" fn d_set_rs(this: usize, state: u32, value: u32) -> i32 {
    if state == D3DRS_ZBIAS {
        // ZBIAS 0..16 (vers l'observateur) → DEPTHBIAS flottant négatif.
        let bias = -(value as f32) * 0.000_015;
        return c9!(
            d9(this),
            57,
            fn(u32, u32) -> i32,
            D3DRS_DEPTHBIAS,
            bias.to_bits()
        );
    }
    if state == 153 {
        return c9!(d9(this), 77, fn(i32) -> i32, value as i32);
    }
    if RS_GONE.contains(&state) {
        return S_OK;
    }
    c9!(d9(this), 57, fn(u32, u32) -> i32, state, value)
}
unsafe extern "win64" fn d_get_rs(this: usize, state: u32, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    if state == D3DRS_ZBIAS {
        let mut v = 0u32;
        c9!(
            d9(this),
            58,
            fn(u32, *mut u32) -> i32,
            D3DRS_DEPTHBIAS,
            &mut v
        );
        wr32(
            out,
            (-f32::from_bits(v) / 0.000_015).round().max(0.0) as u32,
        );
        return S_OK;
    }
    if RS_GONE.contains(&state) {
        wr32(out, 0);
        return S_OK;
    }
    c9!(d9(this), 58, fn(u32, usize) -> i32, state, out)
}

// Blocs d'états : numéros Direct3D 8 → IDirect3DStateBlock9.
unsafe extern "win64" fn d_begin_sb(this: usize) -> i32 {
    c9!(d9(this), 60, fn() -> i32)
}
unsafe fn store_block(this: usize, sb: usize, out: usize) -> i32 {
    let mut st = lock(&device(this).st);
    st.next_block += 1;
    let token = st.next_block;
    st.blocks.insert(token, sb);
    wr32(out, token);
    S_OK
}
unsafe extern "win64" fn d_end_sb(this: usize, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut sb: usize = 0;
    let hr = c9!(d9(this), 61, fn(*mut usize) -> i32, &mut sb);
    if hr < 0 || sb == 0 {
        return if hr < 0 { hr } else { D3DERR_INVALIDCALL };
    }
    store_block(this, sb, out)
}
unsafe extern "win64" fn d_apply_sb(this: usize, token: u32) -> i32 {
    let sb = lock(&device(this).st).blocks.get(&token).copied();
    match sb {
        Some(sb) => c9!(sb, 5, fn() -> i32),
        None => D3DERR_INVALIDCALL,
    }
}
unsafe extern "win64" fn d_capture_sb(this: usize, token: u32) -> i32 {
    let sb = lock(&device(this).st).blocks.get(&token).copied();
    match sb {
        Some(sb) => c9!(sb, 4, fn() -> i32),
        None => D3DERR_INVALIDCALL,
    }
}
unsafe extern "win64" fn d_delete_sb(this: usize, token: u32) -> i32 {
    let sb = lock(&device(this).st).blocks.remove(&token);
    match sb {
        Some(sb) => {
            c9!(sb, 2, fn() -> u32);
            S_OK
        }
        None => D3DERR_INVALIDCALL,
    }
}
unsafe extern "win64" fn d_create_sb(this: usize, ty: u32, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut sb: usize = 0;
    let hr = c9!(d9(this), 59, fn(u32, *mut usize) -> i32, ty, &mut sb);
    if hr < 0 || sb == 0 {
        return if hr < 0 { hr } else { D3DERR_INVALIDCALL };
    }
    store_block(this, sb, out)
}
unsafe extern "win64" fn d_set_clip_status(this: usize, s: usize) -> i32 {
    c9!(d9(this), 62, fn(usize) -> i32, s)
}
unsafe extern "win64" fn d_get_clip_status(this: usize, s: usize) -> i32 {
    c9!(d9(this), 63, fn(usize) -> i32, s)
}

unsafe extern "win64" fn d_get_texture(this: usize, stage: u32, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let st = lock(&device(this).st);
    let t = st.textures.get(stage as usize).copied().unwrap_or(0);
    if t != 0 {
        res(t).refs.fetch_add(1, Ordering::AcqRel);
    }
    put_ptr(out, t);
    S_OK
}
unsafe extern "win64" fn d_set_texture(this: usize, stage: u32, tex: usize) -> i32 {
    let hr = c9!(d9(this), 65, fn(u32, usize) -> i32, stage, inner(tex));
    if hr >= 0 {
        let old = {
            let mut st = lock(&device(this).st);
            match st.textures.get_mut(stage as usize) {
                Some(slot) => std::mem::replace(slot, tex),
                None => 0,
            }
        };
        if tex != 0 && (stage as usize) < 8 {
            res(tex).refs.fetch_add(1, Ordering::AcqRel);
        }
        if old != 0 {
            r_release(old);
        }
    }
    hr
}

/// États d'étage de texture Direct3D 8 devenus états d'échantillonneur.
fn sampler_state(tss: u32) -> Option<u32> {
    Some(match tss {
        13 => 1,  // ADDRESSU
        14 => 2,  // ADDRESSV
        25 => 3,  // ADDRESSW
        15 => 4,  // BORDERCOLOR
        16 => 5,  // MAGFILTER
        17 => 6,  // MINFILTER
        18 => 7,  // MIPFILTER
        19 => 8,  // MIPMAPLODBIAS
        20 => 9,  // MAXMIPLEVEL
        21 => 10, // MAXANISOTROPY
        _ => return None,
    })
}
unsafe extern "win64" fn d_get_tss(this: usize, stage: u32, ty: u32, out: usize) -> i32 {
    match sampler_state(ty) {
        Some(s) => c9!(d9(this), 68, fn(u32, u32, usize) -> i32, stage, s, out),
        None => c9!(d9(this), 66, fn(u32, u32, usize) -> i32, stage, ty, out),
    }
}
unsafe extern "win64" fn d_set_tss(this: usize, stage: u32, ty: u32, value: u32) -> i32 {
    match sampler_state(ty) {
        Some(s) => c9!(d9(this), 69, fn(u32, u32, u32) -> i32, stage, s, value),
        None => c9!(d9(this), 67, fn(u32, u32, u32) -> i32, stage, ty, value),
    }
}
unsafe extern "win64" fn d_validate(this: usize, out: usize) -> i32 {
    c9!(d9(this), 70, fn(usize) -> i32, out)
}
unsafe extern "win64" fn d_get_info(_this: usize, _id: u32, _data: usize, _size: u32) -> i32 {
    1 // S_FALSE : aucune information
}
unsafe extern "win64" fn d_set_pal(this: usize, n: u32, e: usize) -> i32 {
    c9!(d9(this), 71, fn(u32, usize) -> i32, n, e)
}
unsafe extern "win64" fn d_get_pal(this: usize, n: u32, e: usize) -> i32 {
    c9!(d9(this), 72, fn(u32, usize) -> i32, n, e)
}
unsafe extern "win64" fn d_set_cur_pal(this: usize, n: u32) -> i32 {
    c9!(d9(this), 73, fn(u32) -> i32, n)
}
unsafe extern "win64" fn d_get_cur_pal(this: usize, out: usize) -> i32 {
    c9!(d9(this), 74, fn(usize) -> i32, out)
}
unsafe extern "win64" fn d_draw(this: usize, ty: u32, start: u32, count: u32) -> i32 {
    c9!(d9(this), 81, fn(u32, u32, u32) -> i32, ty, start, count)
}
unsafe extern "win64" fn d_draw_indexed(
    this: usize,
    ty: u32,
    min: u32,
    n: u32,
    start: u32,
    count: u32,
) -> i32 {
    let base = lock(&device(this).st).indices.1 as i32;
    c9!(
        d9(this),
        82,
        fn(u32, i32, u32, u32, u32, u32) -> i32,
        ty,
        base,
        min,
        n,
        start,
        count
    )
}
unsafe extern "win64" fn d_draw_up(
    this: usize,
    ty: u32,
    count: u32,
    data: usize,
    stride: u32,
) -> i32 {
    let hr = c9!(
        d9(this),
        83,
        fn(u32, u32, usize, u32) -> i32,
        ty,
        count,
        data,
        stride
    );
    // Direct3D 9 délie le flux 0 après un appel UP, comme Direct3D 8.
    let old = std::mem::take(&mut lock(&device(this).st).streams[0]);
    if old.0 != 0 {
        r_release(old.0);
    }
    hr
}
unsafe extern "win64" fn d_draw_indexed_up(
    this: usize,
    ty: u32,
    min: u32,
    n: u32,
    count: u32,
    idx: usize,
    fmt: u32,
    data: usize,
    stride: u32,
) -> i32 {
    let hr = c9!(
        d9(this),
        84,
        fn(u32, u32, u32, u32, usize, u32, usize, u32) -> i32,
        ty,
        min,
        n,
        count,
        idx,
        fmt,
        data,
        stride
    );
    let (old_s, old_i) = {
        let mut st = lock(&device(this).st);
        (
            std::mem::take(&mut st.streams[0]),
            std::mem::take(&mut st.indices),
        )
    };
    for w in [old_s.0, old_i.0] {
        if w != 0 {
            r_release(w);
        }
    }
    hr
}
unsafe extern "win64" fn d_process_vertices(
    _this: usize,
    _a: u32,
    _b: u32,
    _c: u32,
    _d: usize,
    _f: u32,
) -> i32 {
    D3DERR_INVALIDCALL
}

// --- Shaders de sommets ---

/// Registre d'entrée v0..v16 de Direct3D 8 → (usage, index) Direct3D 9.
fn vs_register_usage(reg: u32) -> (u8, u8) {
    use super::d3d9_shader::*;
    match reg {
        0 => (USAGE_POSITION, 0),
        1 => (USAGE_BLENDWEIGHT, 0),
        2 => (USAGE_BLENDINDICES, 0),
        3 => (USAGE_NORMAL, 0),
        4 => (USAGE_PSIZE, 0),
        5 => (USAGE_COLOR, 0),
        6 => (USAGE_COLOR, 1),
        7..=14 => (USAGE_TEXCOORD, (reg - 7) as u8),
        15 => (USAGE_POSITION, 1),
        16 => (USAGE_NORMAL, 1),
        _ => (USAGE_TEXCOORD, (reg & 7) as u8),
    }
}

/// Taille en octets d'un D3DVSDT_*.
fn vsdt_size(t: u32) -> u16 {
    match t {
        0 => 4,
        1 => 8,
        2 => 12,
        3 => 16,
        4..=6 => 4,
        7 => 8,
        _ => 4,
    }
}

/// Déclaration à jetons Direct3D 8 → éléments D3DVERTEXELEMENT9 (8 octets
/// chacun, terminés par D3DDECL_END), registres utilisés et constantes.
#[allow(clippy::type_complexity)]
fn translate_decl(tokens: &[u32]) -> (Vec<[u8; 8]>, Vec<u32>, Vec<(u32, Vec<f32>)>) {
    let mut elems = Vec::new();
    let mut regs = Vec::new();
    let mut consts = Vec::new();
    let mut stream: u16 = 0;
    let mut offset: u16 = 0;
    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i];
        i += 1;
        if t == 0xFFFF_FFFF {
            break;
        }
        match t >> 29 {
            1 => {
                // D3DVSD_STREAM (ou STREAM_TESS si le bit 28 est posé).
                if t & (1 << 28) == 0 {
                    stream = (t & 0xF) as u16;
                    offset = 0;
                }
            }
            2 => {
                if t & (1 << 28) != 0 {
                    // D3DVSD_SKIP(n DWORD).
                    offset += (((t >> 16) & 0xF) * 4) as u16;
                } else {
                    let reg = t & 0x1F;
                    let ty = (t >> 16) & 0xF;
                    let (usage, index) = vs_register_usage(reg);
                    let o = offset.to_le_bytes();
                    let s = stream.to_le_bytes();
                    elems.push([s[0], s[1], o[0], o[1], ty as u8, 0, usage, index]);
                    regs.push(reg);
                    offset += vsdt_size(ty);
                }
            }
            4 => {
                // D3DVSD_CONST(adresse, nombre) suivi de nombre × 4 flottants.
                let addr = t & 0x7F;
                let count = (t >> 25) & 0xF;
                let n = (count * 4) as usize;
                let vals: Vec<f32> = tokens[i..(i + n).min(tokens.len())]
                    .iter()
                    .map(|v| f32::from_bits(*v))
                    .collect();
                i += n;
                consts.push((addr, vals));
            }
            5 => {
                // Extension : n DWORD à sauter.
                i += ((t >> 24) & 0x1F) as usize;
            }
            _ => {}
        }
    }
    elems.push([0xFF, 0, 0, 0, 17, 0, 0, 0]);
    (elems, regs, consts)
}

/// Bytecode vs.1.x de Direct3D 8 → Direct3D 9 : une instruction
/// `dcl_<usage> vN` par registre de la déclaration, après le jeton de version.
fn add_dcl(function: &[u32], regs: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(function.len() + regs.len() * 3);
    out.push(*function.first().unwrap_or(&0xFFFE_0101));
    for &r in regs {
        let (usage, index) = vs_register_usage(r);
        out.push(0x0200_001F);
        out.push(0x8000_0000 | usage as u32 | (index as u32) << 16);
        out.push(0x900F_0000 | r);
    }
    out.extend_from_slice(function.get(1..).unwrap_or(&[]));
    out
}

/// Longueur d'un flux de jetons terminé par `end`.
unsafe fn token_len(p: usize, end: u32, max: usize) -> usize {
    let mut n = 0;
    while n < max {
        let t = rd32(p + 4 * n);
        n += 1;
        if t == end {
            break;
        }
    }
    n
}

unsafe extern "win64" fn d_create_vs(
    this: usize,
    decl: usize,
    function: usize,
    out: usize,
    _usage: u32,
) -> i32 {
    if decl == 0 || out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let tokens: Vec<u32> = (0..token_len(decl, 0xFFFF_FFFF, 4096))
        .map(|i| rd32(decl + 4 * i))
        .collect();
    let (elems, regs, consts) = translate_decl(&tokens);
    let mut decl9: usize = 0;
    let hr = c9!(
        d9(this),
        86,
        fn(*const [u8; 8], *mut usize) -> i32,
        elems.as_ptr(),
        &mut decl9
    );
    if hr < 0 {
        return hr;
    }
    let mut vs9: usize = 0;
    let mut func = Vec::new();
    if function != 0 {
        func = (0..token_len(function, 0x0000_FFFF, 1 << 16))
            .map(|i| rd32(function + 4 * i))
            .collect();
        let code = add_dcl(&func, &regs);
        let hr = c9!(
            d9(this),
            91,
            fn(*const u32, *mut usize) -> i32,
            code.as_ptr(),
            &mut vs9
        );
        if hr < 0 {
            c9!(decl9, 2, fn() -> u32);
            return hr;
        }
    }
    let mut st = lock(&device(this).st);
    st.vshaders.push(Some(VShader {
        decl9,
        vs9,
        consts,
        decl_tokens: tokens,
        function: func,
    }));
    // Numéro impair : jamais confondu avec un FVF (bit 0 réservé).
    wr32(out, (st.vshaders.len() as u32) << 1 | 1);
    S_OK
}

unsafe extern "win64" fn d_set_vs(this: usize, handle: u32) -> i32 {
    let dv = d9(this);
    if handle & 1 == 0 {
        lock(&device(this).st).vs = handle;
        c9!(dv, 92, fn(usize) -> i32, 0);
        return c9!(dv, 89, fn(u32) -> i32, handle);
    }
    let (decl9, vs9, consts) = {
        let mut st = lock(&device(this).st);
        let Some(Some(v)) = st.vshaders.get((handle >> 1) as usize - 1) else {
            return D3DERR_INVALIDCALL;
        };
        let r = (v.decl9, v.vs9, v.consts.clone());
        st.vs = handle;
        r
    };
    c9!(dv, 87, fn(usize) -> i32, decl9);
    c9!(dv, 92, fn(usize) -> i32, vs9);
    for (reg, vals) in consts {
        c9!(
            dv,
            94,
            fn(u32, *const f32, u32) -> i32,
            reg,
            vals.as_ptr(),
            (vals.len() / 4) as u32
        );
    }
    S_OK
}
unsafe extern "win64" fn d_get_vs(this: usize, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    wr32(out, lock(&device(this).st).vs);
    S_OK
}
unsafe extern "win64" fn d_delete_vs(this: usize, handle: u32) -> i32 {
    if handle & 1 == 0 {
        return D3DERR_INVALIDCALL;
    }
    let v = {
        let mut st = lock(&device(this).st);
        match st.vshaders.get_mut((handle >> 1) as usize - 1) {
            Some(slot) => slot.take(),
            None => None,
        }
    };
    match v {
        Some(v) => {
            if v.vs9 != 0 {
                c9!(v.vs9, 2, fn() -> u32);
            }
            c9!(v.decl9, 2, fn() -> u32);
            S_OK
        }
        None => D3DERR_INVALIDCALL,
    }
}
unsafe extern "win64" fn d_set_vs_const(this: usize, reg: u32, data: usize, count: u32) -> i32 {
    c9!(d9(this), 94, fn(u32, usize, u32) -> i32, reg, data, count)
}
unsafe extern "win64" fn d_get_vs_const(this: usize, reg: u32, data: usize, count: u32) -> i32 {
    c9!(d9(this), 95, fn(u32, usize, u32) -> i32, reg, data, count)
}
/// Copie d'un tableau de jetons gardé (déclaration ou fonction).
unsafe fn give_tokens(v: &[u32], data: usize, size: usize) -> i32 {
    if size == 0 {
        return D3DERR_INVALIDCALL;
    }
    let need = (v.len() * 4) as u32;
    let cap = rd32(size);
    wr32(size, need);
    if data == 0 {
        return S_OK;
    }
    if cap < need {
        return 0x8876_0868u32 as i32; // D3DERR_MOREDATA
    }
    std::ptr::copy_nonoverlapping(v.as_ptr() as *const u8, data as *mut u8, need as usize);
    S_OK
}
unsafe extern "win64" fn d_vs_decl(this: usize, handle: u32, data: usize, size: usize) -> i32 {
    let st = lock(&device(this).st);
    match st.vshaders.get((handle >> 1) as usize - 1) {
        Some(Some(v)) if handle & 1 == 1 => give_tokens(&v.decl_tokens, data, size),
        _ => D3DERR_INVALIDCALL,
    }
}
unsafe extern "win64" fn d_vs_function(this: usize, handle: u32, data: usize, size: usize) -> i32 {
    let st = lock(&device(this).st);
    match st.vshaders.get((handle >> 1) as usize - 1) {
        Some(Some(v)) if handle & 1 == 1 => give_tokens(&v.function, data, size),
        _ => D3DERR_INVALIDCALL,
    }
}
unsafe extern "win64" fn d_set_stream(this: usize, stream: u32, vb: usize, stride: u32) -> i32 {
    if stream >= 16 {
        return D3DERR_INVALIDCALL;
    }
    let hr = c9!(
        d9(this),
        100,
        fn(u32, usize, u32, u32) -> i32,
        stream,
        inner(vb),
        0,
        stride
    );
    if hr >= 0 {
        if vb != 0 {
            res(vb).refs.fetch_add(1, Ordering::AcqRel);
        }
        let old = std::mem::replace(
            &mut lock(&device(this).st).streams[stream as usize],
            (vb, stride),
        );
        if old.0 != 0 {
            r_release(old.0);
        }
    }
    hr
}
unsafe extern "win64" fn d_get_stream(this: usize, stream: u32, out: usize, stride: usize) -> i32 {
    if stream >= 16 || out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let (vb, s) = lock(&device(this).st).streams[stream as usize];
    if vb != 0 {
        res(vb).refs.fetch_add(1, Ordering::AcqRel);
    }
    put_ptr(out, vb);
    if stride != 0 {
        wr32(stride, s);
    }
    S_OK
}
unsafe extern "win64" fn d_set_indices(this: usize, ib: usize, base: u32) -> i32 {
    let hr = c9!(d9(this), 104, fn(usize) -> i32, inner(ib));
    if hr >= 0 {
        if ib != 0 {
            res(ib).refs.fetch_add(1, Ordering::AcqRel);
        }
        let old = std::mem::replace(&mut lock(&device(this).st).indices, (ib, base));
        if old.0 != 0 {
            r_release(old.0);
        }
    }
    hr
}
unsafe extern "win64" fn d_get_indices(this: usize, out: usize, base: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let (ib, b) = lock(&device(this).st).indices;
    if ib != 0 {
        res(ib).refs.fetch_add(1, Ordering::AcqRel);
    }
    put_ptr(out, ib);
    if base != 0 {
        wr32(base, b);
    }
    S_OK
}

// --- Shaders de pixels (ps.1.x, même bytecode qu'en Direct3D 9) ---

unsafe extern "win64" fn d_create_ps(this: usize, function: usize, out: usize) -> i32 {
    if function == 0 || out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut ps9: usize = 0;
    let hr = c9!(
        d9(this),
        106,
        fn(usize, *mut usize) -> i32,
        function,
        &mut ps9
    );
    if hr < 0 {
        return hr;
    }
    let func: Vec<u32> = (0..token_len(function, 0x0000_FFFF, 1 << 16))
        .map(|i| rd32(function + 4 * i))
        .collect();
    let mut st = lock(&device(this).st);
    st.pshaders.push(Some((ps9, func)));
    wr32(out, st.pshaders.len() as u32);
    S_OK
}
unsafe extern "win64" fn d_set_ps(this: usize, handle: u32) -> i32 {
    let ps9 = {
        let mut st = lock(&device(this).st);
        let p = if handle == 0 {
            0
        } else {
            match st.pshaders.get(handle as usize - 1) {
                Some(Some((p, _))) => *p,
                _ => return D3DERR_INVALIDCALL,
            }
        };
        st.ps = handle;
        p
    };
    c9!(d9(this), 107, fn(usize) -> i32, ps9)
}
unsafe extern "win64" fn d_get_ps(this: usize, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    wr32(out, lock(&device(this).st).ps);
    S_OK
}
unsafe extern "win64" fn d_delete_ps(this: usize, handle: u32) -> i32 {
    let p = {
        let mut st = lock(&device(this).st);
        match st.pshaders.get_mut((handle as usize).wrapping_sub(1)) {
            Some(slot) => slot.take(),
            None => None,
        }
    };
    match p {
        Some((p, _)) => {
            c9!(p, 2, fn() -> u32);
            S_OK
        }
        None => D3DERR_INVALIDCALL,
    }
}
unsafe extern "win64" fn d_set_ps_const(this: usize, reg: u32, data: usize, count: u32) -> i32 {
    c9!(d9(this), 109, fn(u32, usize, u32) -> i32, reg, data, count)
}
unsafe extern "win64" fn d_get_ps_const(this: usize, reg: u32, data: usize, count: u32) -> i32 {
    c9!(d9(this), 110, fn(u32, usize, u32) -> i32, reg, data, count)
}
unsafe extern "win64" fn d_ps_function(this: usize, handle: u32, data: usize, size: usize) -> i32 {
    let st = lock(&device(this).st);
    match st.pshaders.get((handle as usize).wrapping_sub(1)) {
        Some(Some((_, f))) => give_tokens(f, data, size),
        _ => D3DERR_INVALIDCALL,
    }
}
unsafe extern "win64" fn d_invalid1(_this: usize, _a: usize) -> i32 {
    D3DERR_INVALIDCALL
}
unsafe extern "win64" fn d_invalid2(_this: usize, _a: usize, _b: usize) -> i32 {
    D3DERR_INVALIDCALL
}
unsafe extern "win64" fn d_invalid3(_this: usize, _a: usize, _b: usize, _c: usize) -> i32 {
    D3DERR_INVALIDCALL
}

fn device_vtable() -> usize {
    let a = |f: *const (), n: u8| (f as usize, n);
    vtable(
        "IDirect3DDevice8",
        &[
            a(d_qi as *const (), 2),
            a(d_addref as *const (), 0),
            a(d_release as *const (), 0),
            a(d_test_coop as *const (), 0),
            a(d_tex_mem as *const (), 0),
            a(d_discard_bytes as *const (), 1),
            a(d_get_d3d as *const (), 1),
            a(d_caps as *const (), 1),
            a(d_display_mode as *const (), 1),
            a(d_creation_params as *const (), 1),
            a(d_cursor_props as *const (), 3),
            a(d_cursor_pos as *const (), 3),
            a(d_show_cursor as *const (), 1),
            a(d_invalid2 as *const (), 2), // CreateAdditionalSwapChain
            a(d_reset as *const (), 1),
            a(d_present as *const (), 4),
            a(d_back_buffer as *const (), 3),
            a(d_raster_status as *const (), 1),
            a(d_set_gamma as *const (), 2),
            a(d_get_gamma as *const (), 1),
            a(d_create_texture as *const (), 7),
            a(d_create_volume as *const (), 8),
            a(d_create_cube as *const (), 6),
            a(d_create_vb as *const (), 5),
            a(d_create_ib as *const (), 5),
            a(d_create_rt as *const (), 6),
            a(d_create_ds as *const (), 5),
            a(d_create_image as *const (), 4),
            a(d_copy_rects as *const (), 5),
            a(d_update_texture as *const (), 2),
            a(d_front_buffer as *const (), 1),
            a(d_set_rt as *const (), 2),
            a(d_get_rt as *const (), 1),
            a(d_get_ds as *const (), 1),
            a(d_begin_scene as *const (), 0),
            a(d_end_scene as *const (), 0),
            a(d_clear as *const (), 6),
            a(d_set_transform as *const (), 2),
            a(d_get_transform as *const (), 2),
            a(d_mul_transform as *const (), 2),
            a(d_set_viewport as *const (), 1),
            a(d_get_viewport as *const (), 1),
            a(d_set_material as *const (), 1),
            a(d_get_material as *const (), 1),
            a(d_set_light as *const (), 2),
            a(d_get_light as *const (), 2),
            a(d_light_enable as *const (), 2),
            a(d_get_light_enable as *const (), 2),
            a(d_set_clip_plane as *const (), 2),
            a(d_get_clip_plane as *const (), 2),
            a(d_set_rs as *const (), 2),
            a(d_get_rs as *const (), 2),
            a(d_begin_sb as *const (), 0),
            a(d_end_sb as *const (), 1),
            a(d_apply_sb as *const (), 1),
            a(d_capture_sb as *const (), 1),
            a(d_delete_sb as *const (), 1),
            a(d_create_sb as *const (), 2),
            a(d_set_clip_status as *const (), 1),
            a(d_get_clip_status as *const (), 1),
            a(d_get_texture as *const (), 2),
            a(d_set_texture as *const (), 2),
            a(d_get_tss as *const (), 3),
            a(d_set_tss as *const (), 3),
            a(d_validate as *const (), 1),
            a(d_get_info as *const (), 3),
            a(d_set_pal as *const (), 2),
            a(d_get_pal as *const (), 2),
            a(d_set_cur_pal as *const (), 1),
            a(d_get_cur_pal as *const (), 1),
            a(d_draw as *const (), 3),
            a(d_draw_indexed as *const (), 5),
            a(d_draw_up as *const (), 4),
            a(d_draw_indexed_up as *const (), 8),
            a(d_process_vertices as *const (), 5),
            a(d_create_vs as *const (), 4),
            a(d_set_vs as *const (), 1),
            a(d_get_vs as *const (), 1),
            a(d_delete_vs as *const (), 1),
            a(d_set_vs_const as *const (), 3),
            a(d_get_vs_const as *const (), 3),
            a(d_vs_decl as *const (), 3),
            a(d_vs_function as *const (), 3),
            a(d_set_stream as *const (), 3),
            a(d_get_stream as *const (), 3),
            a(d_set_indices as *const (), 2),
            a(d_get_indices as *const (), 2),
            a(d_create_ps as *const (), 2),
            a(d_set_ps as *const (), 1),
            a(d_get_ps as *const (), 1),
            a(d_delete_ps as *const (), 1),
            a(d_set_ps_const as *const (), 3),
            a(d_get_ps_const as *const (), 3),
            a(d_ps_function as *const (), 3),
            a(d_invalid3 as *const (), 3), // DrawRectPatch
            a(d_invalid3 as *const (), 3), // DrawTriPatch
            a(d_invalid1 as *const (), 1), // DeletePatch
        ],
    )
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "Direct3DCreate8" => fn direct3d_create8,
    }
}

#[allow(dead_code)]
fn _x86_only() -> bool {
    x86()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declaration_d3d8_traduite() {
        // STREAM(0), REG(v0, FLOAT3), REG(v3, FLOAT3), SKIP(1),
        // REG(v5, D3DCOLOR), REG(v7, FLOAT2), CONST(4, 1) {1,2,3,4}, END.
        let tokens = [
            0x2000_0000,
            0x4002_0000,
            0x4002_0003,
            0x5001_0000,
            0x4004_0005,
            0x4001_0007,
            0x8000_0000 | 1 << 25 | 4,
            1.0f32.to_bits(),
            2.0f32.to_bits(),
            3.0f32.to_bits(),
            4.0f32.to_bits(),
            0xFFFF_FFFF,
        ];
        let (e, regs, consts) = translate_decl(&tokens);
        assert_eq!(regs, vec![0, 3, 5, 7]);
        assert_eq!(e.len(), 5);
        assert_eq!(e[0], [0, 0, 0, 0, 2, 0, 0, 0]); // POSITION à 0
        assert_eq!(e[1], [0, 0, 12, 0, 2, 0, 3, 0]); // NORMAL à 12
        assert_eq!(e[2], [0, 0, 28, 0, 4, 0, 10, 0]); // COLOR0 à 28 (après le saut)
        assert_eq!(e[3], [0, 0, 32, 0, 1, 0, 5, 0]); // TEXCOORD0 à 32
        assert_eq!(e[4][0], 0xFF);
        assert_eq!(consts, vec![(4, vec![1.0, 2.0, 3.0, 4.0])]);
    }

    #[test]
    fn dcl_ajoutes_au_shader() {
        let f = [
            0xFFFE_0101,
            0x0000_0001,
            0xC00F_0000,
            0x90E4_0000,
            0x0000_FFFF,
        ];
        let out = add_dcl(&f, &[0, 5]);
        assert_eq!(out[0], 0xFFFE_0101);
        assert_eq!(&out[1..4], &[0x0200_001F, 0x8000_0000, 0x900F_0000]);
        assert_eq!(&out[4..7], &[0x0200_001F, 0x8000_000A, 0x900F_0005]);
        assert_eq!(&out[7..], &f[1..]);
    }

    #[test]
    fn etats_de_texture_vers_echantillonneur() {
        assert_eq!(sampler_state(13), Some(1));
        assert_eq!(sampler_state(17), Some(6));
        assert_eq!(sampler_state(1), None); // COLOROP reste un état d'étage
    }
}
