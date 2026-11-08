//! Direct3D 9 pour les programmes 32 bits (#778).
//!
//! Les objets Direct3D 9 gardent leur table win64 pour le code du runtime
//! (D3DX, Direct3D 8) ; un programme x86 reçoit à la place une table de
//! passerelles stdcall construite d'après la signature de chaque méthode.
//! Les cases de pointeur rendues (objets créés, `ppbData`…) passent par
//! les cases de 8 octets de la passerelle (`p`). Les structures qui portent
//! un pointeur ou un handle sont converties ici : D3DPRESENT_PARAMETERS
//! (hDeviceWindow), D3DDEVICE_CREATION_PARAMETERS (hFocusWindow) et
//! D3DLOCKED_RECT (pBits).

use super::*;
use crate::dlls::legacy_com::{rd32, wr32, x86, x86_table};

/// Signatures des méthodes (arguments après `this`), dans l'ordre des tables.
const UNKNOWN: [&str; 3] = ["ip", "", ""];

const D3D9: [&str; 22] = [
    "ip", "", "", "i", "", "iii", "ii", "iiii", "ii", "iiiii", "iiiiii", "iiiiii", "iiiii", "iiii",
    "iii", "i", "iiiiip", "ii", "iiii", "iii", "iiiiiip", "ii",
];

const DEVICE: [&str; 134] = [
    "ip",
    "",
    "",
    "",
    "",
    "",
    "p",
    "i",
    "ii",
    "i", // 0-9
    "iii",
    "iii",
    "i",
    "ip",
    "ip",
    "",
    "i",
    "iiii",
    "iiip",
    "ii", // 10-19
    "i",
    "iii",
    "ii",
    "iiiiiipp",
    "iiiiiiipp",
    "iiiiipp",
    "iiiipp",
    "iiiipp",
    "iiiiiipp",
    "iiiiiipp", // 20-29
    "iiii",
    "ii",
    "ii",
    "ii",
    "iiiii",
    "iii",
    "iiiipp",
    "ii",
    "ip",
    "i", // 30-39
    "p",
    "",
    "",
    "iiiiii",
    "ii",
    "ii",
    "ii",
    "i",
    "i",
    "i", // 40-49
    "i",
    "ii",
    "ii",
    "ii",
    "ii",
    "ii",
    "ii",
    "ii",
    "ii",
    "ip", // 50-59
    "",
    "p",
    "i",
    "i",
    "ip",
    "ii",
    "iii",
    "iii",
    "iii",
    "iii", // 60-69
    "i",
    "ii",
    "ii",
    "i",
    "i",
    "i",
    "i",
    "i",
    "",
    "i", // 70-79
    ">f",
    "iii",
    "iiiiii",
    "iiii",
    "iiiiiiii",
    "iiiiii",
    "ip",
    "i",
    "p",
    "i", // 80-89
    "i",
    "ip",
    "i",
    "p",
    "iii",
    "iii",
    "iii",
    "iii",
    "iii",
    "iii", // 90-99
    "iiii",
    "ipii",
    "ii",
    "ii",
    "i",
    "p",
    "ip",
    "i",
    "p",
    "iii", // 100-109
    "iii",
    "iii",
    "iii",
    "iii",
    "iii",
    "iii",
    "iii",
    "i",
    "ip",
    "iiii", // 110-119
    "iiiiiiii",
    "iiiii",
    "i",
    "i",
    "i",
    "ii",
    "i",
    "i",
    "i",
    "iiiiiippi", // 120-129
    "iiiippi",
    "iiiiiippi",
    "ii",
    "iii", // 130-133
];

const RESOURCE: [&str; 11] = ["ip", "", "", "p", "iiii", "iii", "i", "i", "", "", ""];

fn with(base: &[&'static str], rest: &[&'static str]) -> Vec<&'static str> {
    base.iter().chain(rest).copied().collect()
}

fn signatures(name: &str) -> Vec<&'static str> {
    match name {
        "IDirect3D9" => D3D9.to_vec(),
        "IDirect3DDevice9" => DEVICE.to_vec(),
        "IDirect3DSurface9" => with(&RESOURCE, &["ip", "i", "iii", "", "p", "i"]),
        "IDirect3DTexture9" => with(
            &RESOURCE,
            &["i", "", "", "i", "", "", "ii", "ip", "iiii", "i", "i"],
        ),
        "IDirect3DCubeTexture9" => with(
            &RESOURCE,
            &["i", "", "", "i", "", "", "ii", "iip", "iiiii", "ii", "ii"],
        ),
        "IDirect3DBuffer9" => with(&RESOURCE, &["iipi", "", "i"]),
        "IDirect3DVertexDeclaration9" | "IDirect3DShader9" => with(&UNKNOWN, &["p", "ii"]),
        "IDirect3DQuery9" => with(&UNKNOWN, &["p", "", "", "i", "iii"]),
        "IDirect3DStateBlock9" => with(&UNKNOWN, &["p", "", ""]),
        "IDirect3DSwapChain9" | "IDirect3DSwapChain9Ex" => with(
            &UNKNOWN,
            &["iiiii", "i", "iip", "i", "i", "p", "i", "i", "i", "ii"],
        ),
        _ => Vec::new(),
    }
}

// --- Conversions de structures ---

/// D3DPRESENT_PARAMETERS x86 (56 octets, hDeviceWindow sur 4 octets).
unsafe fn pp_in(p: usize) -> PresentParams {
    let d = |i: usize| rd32(p + 4 * i);
    PresentParams {
        width: d(0),
        height: d(1),
        format: d(2),
        count: d(3),
        ms_type: d(4),
        ms_quality: d(5),
        swap_effect: d(6),
        device_window: d(7) as usize,
        windowed: d(8) as i32,
        auto_depth: d(9) as i32,
        auto_depth_format: d(10),
        flags: d(11),
        refresh: d(12),
        interval: d(13),
    }
}
unsafe fn pp_out(p: usize, v: &PresentParams) {
    let vals = [
        v.width,
        v.height,
        v.format,
        v.count,
        v.ms_type,
        v.ms_quality,
        v.swap_effect,
        v.device_window as u32,
        v.windowed as u32,
        v.auto_depth as u32,
        v.auto_depth_format,
        v.flags,
        v.refresh,
        v.interval,
    ];
    for (i, x) in vals.iter().enumerate() {
        wr32(p + 4 * i, *x);
    }
}
/// D3DLOCKED_RECT x86 : { INT Pitch; void *pBits; } sur 8 octets.
unsafe fn lr_out(p: usize, lr: &LockedRect) {
    wr32(p, lr.pitch as u32);
    wr32(p + 4, lr.bits as usize as u32);
}

unsafe extern "win64" fn create_device(
    this: *mut Obj,
    adapter: u32,
    ty: u32,
    focus: usize,
    behavior: u32,
    pp: usize,
    out: *mut *mut Obj,
) -> i32 {
    if pp == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut v = pp_in(pp);
    let hr = d3d_create_device(this, adapter, ty, focus, behavior, &mut v, out);
    pp_out(pp, &v);
    hr
}
unsafe extern "win64" fn create_device_ex(
    this: *mut Obj,
    adapter: u32,
    ty: u32,
    focus: usize,
    behavior: u32,
    pp: usize,
    _mode: usize,
    out: *mut *mut Obj,
) -> i32 {
    create_device(this, adapter, ty, focus, behavior, pp, out)
}
unsafe extern "win64" fn reset(this: *mut Obj, pp: usize) -> i32 {
    if pp == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut v = pp_in(pp);
    let hr = dev_reset(this, &mut v);
    pp_out(pp, &v);
    hr
}
unsafe extern "win64" fn create_add_sc(this: *mut Obj, pp: usize, out: *mut *mut Obj) -> i32 {
    if pp == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut v = pp_in(pp);
    let hr = dev_create_add_sc(this, &mut v, out);
    pp_out(pp, &v);
    hr
}
unsafe extern "win64" fn asc_pp(this: *mut Obj, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut v = PresentParams::default();
    let hr = asc_present_params(this, &mut v);
    if hr >= 0 {
        pp_out(out, &v);
    }
    hr
}
unsafe extern "win64" fn reset_ex(this: *mut Obj, pp: usize, _mode: usize) -> i32 {
    reset(this, pp)
}
unsafe extern "win64" fn present_params(this: *mut Obj, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut v = PresentParams::default();
    let hr = sc_present_params(this, &mut v);
    if hr >= 0 {
        pp_out(out, &v);
    }
    hr
}
unsafe extern "win64" fn creation_params(this: *mut Obj, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut v = CreationParams {
        adapter: 0,
        ty: 0,
        focus: 0,
        behavior: 0,
    };
    let hr = dev_creation_params(this, &mut v);
    if hr >= 0 {
        for (i, x) in [v.adapter, v.ty, v.focus as u32, v.behavior]
            .iter()
            .enumerate()
        {
            wr32(out + 4 * i, *x);
        }
    }
    hr
}
fn empty_lr() -> LockedRect {
    LockedRect {
        pitch: 0,
        bits: std::ptr::null_mut(),
    }
}
unsafe extern "win64" fn surf_lock(this: *mut Obj, out: usize, rect: *const Rect32, f: u32) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut lr = empty_lr();
    let hr = surface_lock(this, &mut lr, rect, f);
    lr_out(out, &lr);
    hr
}
unsafe extern "win64" fn tex_lock(
    this: *mut Obj,
    level: u32,
    out: usize,
    rect: *const Rect32,
    f: u32,
) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut lr = empty_lr();
    let hr = tex_lock_rect(this, level, &mut lr, rect, f);
    lr_out(out, &lr);
    hr
}
unsafe extern "win64" fn cube_lock(
    this: *mut Obj,
    face: u32,
    level: u32,
    out: usize,
    rect: *const Rect32,
    f: u32,
) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut lr = empty_lr();
    let hr = cube_lock_rect(this, face, level, &mut lr, rect, f);
    lr_out(out, &lr);
    hr
}

/// Méthodes dont la version x86 convertit une structure.
fn override_for(name: &str, i: usize) -> Option<usize> {
    let f = |p: *const ()| Some(p as usize);
    match (name, i) {
        ("IDirect3D9", 16) => f(create_device as *const ()),
        ("IDirect3D9", 20) => f(create_device_ex as *const ()),
        ("IDirect3DDevice9", 9) => f(creation_params as *const ()),
        ("IDirect3DDevice9", 13) => f(create_add_sc as *const ()),
        ("IDirect3DDevice9", 16) => f(reset as *const ()),
        ("IDirect3DDevice9", 132) => f(reset_ex as *const ()),
        ("IDirect3DSwapChain9", 9) => f(present_params as *const ()),
        ("IDirect3DSwapChain9Ex", 9) => f(asc_pp as *const ()),
        ("IDirect3DSurface9", 13) => f(surf_lock as *const ()),
        ("IDirect3DTexture9", 19) => f(tex_lock as *const ()),
        ("IDirect3DCubeTexture9", 19) => f(cube_lock as *const ()),
        _ => None,
    }
}

/// Nom d'interface d'une table win64 de ce module.
fn name_of(t: *const usize) -> Option<&'static str> {
    let known: [(&'static [usize], &'static str); 12] = [
        (d3d9_vtable(), "IDirect3D9"),
        (device_vtable(), "IDirect3DDevice9"),
        (surface_vtable(), "IDirect3DSurface9"),
        (texture_vtable(false), "IDirect3DTexture9"),
        (texture_vtable(true), "IDirect3DCubeTexture9"),
        (buffer_vtable(), "IDirect3DBuffer9"),
        (decl_vtable(), "IDirect3DVertexDeclaration9"),
        (shader_vtable(), "IDirect3DShader9"),
        (query_vtable(), "IDirect3DQuery9"),
        (stateblock_vtable(), "IDirect3DStateBlock9"),
        (swapchain_vtable(), "IDirect3DSwapChain9"),
        (add_swapchain_vtable(), "IDirect3DSwapChain9Ex"),
    ];
    known.iter().find(|(v, _)| v.as_ptr() == t).map(|(_, n)| *n)
}

/// Table à donner à un objet : la table win64 dans un programme 64 bits,
/// sa table de passerelles x86 dans un programme 32 bits.
pub(super) fn abi(t: &'static [usize]) -> *const usize {
    if !x86() {
        return t.as_ptr();
    }
    let Some(name) = name_of(t.as_ptr()) else {
        return t.as_ptr();
    };
    let sig = signatures(name);
    let methods: Vec<(usize, usize, &str)> = t
        .iter()
        .enumerate()
        .map(|(i, &f)| {
            (
                override_for(name, i).unwrap_or(f),
                f,
                sig.get(i).copied().unwrap_or(""),
            )
        })
        .collect();
    x86_table(name, &methods) as *const usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_completes() {
        assert_eq!(signatures("IDirect3DDevice9").len(), device_vtable().len());
        assert_eq!(signatures("IDirect3D9").len(), d3d9_vtable().len());
        assert_eq!(
            signatures("IDirect3DSurface9").len(),
            surface_vtable().len()
        );
        assert_eq!(
            signatures("IDirect3DTexture9").len(),
            texture_vtable(false).len()
        );
        assert_eq!(
            signatures("IDirect3DCubeTexture9").len(),
            texture_vtable(true).len()
        );
        assert_eq!(signatures("IDirect3DBuffer9").len(), buffer_vtable().len());
        assert_eq!(signatures("IDirect3DShader9").len(), shader_vtable().len());
        assert_eq!(signatures("IDirect3DQuery9").len(), query_vtable().len());
        assert_eq!(
            signatures("IDirect3DStateBlock9").len(),
            stateblock_vtable().len()
        );
        assert_eq!(
            signatures("IDirect3DSwapChain9").len(),
            swapchain_vtable().len()
        );
        // Repères : CreateTexture, Clear, DrawIndexedPrimitiveUP, CreateQuery.
        assert_eq!(DEVICE[23], "iiiiiipp");
        assert_eq!(DEVICE[43], "iiiiii");
        assert_eq!(DEVICE[84].len(), 8);
        assert_eq!(DEVICE[118], "ip");
    }

    #[test]
    fn present_parameters_x86() {
        let mut b = [0u32; 14];
        for (i, v) in b.iter_mut().enumerate() {
            *v = i as u32 + 1;
        }
        unsafe {
            let p = pp_in(b.as_ptr() as usize);
            assert_eq!((p.swap_effect, p.device_window, p.windowed), (7, 8, 9));
            assert_eq!(p.interval, 14);
            let mut o = [0u32; 15];
            pp_out(o.as_mut_ptr() as usize, &p);
            assert_eq!(&o[..14], &b[..]);
            assert_eq!(o[14], 0);
        }
    }
}
