//! Suite auto-vérifiante Direct3D 9, Direct3D 12, DXGI et DXVA2, sans
//! fenêtre : vérifie que les appareils se créent et fonctionnent quel que
//! soit le matériel (Vulkan, OpenGL ou rien), sans planter.
//! Code de sortie = nombre d'échecs.
#![no_std]
#![no_main]

use core::ffi::{c_char, c_void};
use core::ptr::{null, null_mut};
use winlux_testexe::*;

#[repr(C)]
struct Guid(u32, u16, u16, [u8; 8]);

const IID_D3D12_DEVICE: Guid = Guid(
    0x1898_19f1,
    0x1db6,
    0x4b57,
    [0xbe, 0x54, 0x18, 0x21, 0x33, 0x9b, 0x85, 0xf7],
);
const IID_QUEUE: Guid = Guid(
    0x0ec8_70a6,
    0x5d7e,
    0x4c22,
    [0x8c, 0xfc, 0x5b, 0xaa, 0xe0, 0x76, 0x16, 0xed],
);
const IID_ALLOCATOR: Guid = Guid(
    0x6102_dee4,
    0xaf59,
    0x4b09,
    [0xb9, 0x99, 0xb4, 0x4d, 0x73, 0xf0, 0x9b, 0x24],
);
const IID_LIST: Guid = Guid(
    0x5b16_0d0f,
    0xac1b,
    0x4185,
    [0x8b, 0xa8, 0xb3, 0xae, 0x42, 0xa5, 0xa4, 0x55],
);
const IID_HEAP: Guid = Guid(
    0x8efb_471d,
    0x616c,
    0x4f49,
    [0x90, 0xf7, 0x12, 0x7b, 0xb7, 0x63, 0xfa, 0x51],
);
const IID_RESOURCE: Guid = Guid(
    0x6964_42be,
    0xa72e,
    0x4059,
    [0xbc, 0x79, 0x5b, 0x5c, 0x98, 0x04, 0x0f, 0xad],
);
const IID_FENCE: Guid = Guid(
    0x0a75_3dcf,
    0xc4d8,
    0x4b91,
    [0xad, 0xf6, 0xbe, 0x5a, 0x60, 0xd9, 0x5a, 0x76],
);
const IID_FACTORY4: Guid = Guid(
    0x1bc6_ea02,
    0xef36,
    0x464f,
    [0xbf, 0x0c, 0x21, 0xca, 0x39, 0xe5, 0x16, 0x8a],
);
const IID_DECODER_SERVICE: Guid = Guid(
    0xfc51_a551,
    0xd5e7,
    0x11d9,
    [0xaf, 0x55, 0x00, 0x05, 0x4e, 0x43, 0xff, 0x02],
);

/// Entrée `i` de la vtable d'un objet COM.
unsafe fn vt<F: Copy>(obj: *mut c_void, i: usize) -> F {
    let table = *(obj as *const *const usize);
    core::mem::transmute_copy::<usize, F>(&*table.add(i))
}

unsafe fn release(obj: *mut c_void) {
    if !obj.is_null() {
        vt::<unsafe extern "win64" fn(*mut c_void) -> u32>(obj, 2)(obj);
    }
}

unsafe fn proc(dll: &core::ffi::CStr, name: &core::ffi::CStr) -> *const c_void {
    let m = LoadLibraryA(dll.as_ptr());
    if m.is_null() {
        return null();
    }
    GetProcAddress(m, name.as_ptr())
}

type Create3 =
    unsafe extern "win64" fn(*mut c_void, *const c_void, *const Guid, *mut *mut c_void) -> i32;

#[repr(C)]
#[derive(Default)]
struct PresentParams {
    width: u32,
    height: u32,
    format: u32,
    count: u32,
    ms_type: u32,
    ms_quality: u32,
    swap_effect: u32,
    window: usize,
    windowed: i32,
    auto_depth: i32,
    depth_format: u32,
    flags: u32,
    refresh: u32,
    interval: u32,
}

unsafe fn graphics_info() {
    out(b"\n-- Detection materielle --\n");
    let p = proc(c"kernel32.dll", c"WinluxGraphicsInfo");
    check!(!p.is_null(), "WinluxGraphicsInfo exporte");
    if p.is_null() {
        return;
    }
    let f: unsafe extern "win64" fn(*mut u8, u32) -> u32 = core::mem::transmute(p);
    let mut buf = [0u8; 4096];
    let n = f(buf.as_mut_ptr(), buf.len() as u32) as usize;
    let text = &buf[..n.min(buf.len() - 1)];
    check!(text.starts_with(b"opengl="), "rapport graphique lisible");
    check!(
        text.windows(14).any(|w| w == b"backend_d3d12="),
        "moteur D3D12 annonce"
    );
}

unsafe fn d3d9_suite() -> *mut c_void {
    out(b"\n-- Direct3D 9 --\n");
    let p = proc(c"d3d9.dll", c"Direct3DCreate9");
    check!(!p.is_null(), "Direct3DCreate9 exporte");
    if p.is_null() {
        return null_mut();
    }
    let create: unsafe extern "win64" fn(u32) -> *mut c_void = core::mem::transmute(p);
    let d3d = create(32);
    check!(!d3d.is_null(), "IDirect3D9 cree");
    let mut pp = PresentParams {
        windowed: 1,
        swap_effect: 1,
        ..Default::default()
    };
    let mut dev = null_mut();
    let hr = vt::<
        unsafe extern "win64" fn(
            *mut c_void,
            u32,
            u32,
            usize,
            u32,
            *mut PresentParams,
            *mut *mut c_void,
        ) -> i32,
    >(d3d, 16)(d3d, 0, 1, 0, 0x40, &mut pp, &mut dev);
    check!(
        hr == 0 && !dev.is_null(),
        "IDirect3DDevice9 cree (meme sans affichage)"
    );
    release(d3d);
    if dev.is_null() {
        return null_mut();
    }
    // Surface hors écran : ColorFill puis LockRect.
    let mut surf = null_mut();
    let hr = vt::<
        unsafe extern "win64" fn(
            *mut c_void,
            u32,
            u32,
            u32,
            u32,
            *mut *mut c_void,
            *mut c_void,
        ) -> i32,
    >(dev, 36)(
        dev,
        16,
        16,
        21, // A8R8G8B8
        0,
        &mut surf,
        null_mut(),
    );
    check!(hr == 0 && !surf.is_null(), "CreateOffscreenPlainSurface");
    if !surf.is_null() {
        let hr = vt::<unsafe extern "win64" fn(*mut c_void, *mut c_void, *const c_void, u32) -> i32>(
            dev, 35,
        )(dev, surf, null(), 0xff20_4080);
        check!(hr == 0, "ColorFill");
        let mut locked: [usize; 2] = [0; 2]; // { Pitch (INT, aligné), pBits }
        let hr = vt::<
            unsafe extern "win64" fn(*mut c_void, *mut [usize; 2], *const c_void, u32) -> i32,
        >(surf, 13)(surf, &mut locked, null(), 0);
        let px = if hr == 0 && locked[1] != 0 {
            *(locked[1] as *const u32)
        } else {
            0
        };
        check!(px == 0xff20_4080, "LockRect relit la couleur");
        vt::<unsafe extern "win64" fn(*mut c_void) -> i32>(surf, 14)(surf);
        release(surf);
    }
    let mut vb = null_mut();
    let hr = vt::<
        unsafe extern "win64" fn(
            *mut c_void,
            u32,
            u32,
            u32,
            u32,
            *mut *mut c_void,
            *mut c_void,
        ) -> i32,
    >(dev, 26)(
        dev,
        96,
        0,
        0x42, // XYZ | DIFFUSE
        1,
        &mut vb,
        null_mut(),
    );
    check!(hr == 0 && !vb.is_null(), "CreateVertexBuffer");
    release(vb);
    let ok = vt::<unsafe extern "win64" fn(*mut c_void) -> i32>(dev, 41)(dev) == 0
        && vt::<unsafe extern "win64" fn(*mut c_void, u32, *const c_void, u32, u32, f32, u32) -> i32>(
            dev, 43,
        )(dev, 0, null(), 1, 0xff00_0000, 1.0, 0)
            == 0
        && vt::<unsafe extern "win64" fn(*mut c_void) -> i32>(dev, 42)(dev) == 0
        && vt::<
            unsafe extern "win64" fn(
                *mut c_void,
                *const c_void,
                *const c_void,
                usize,
                *const c_void,
            ) -> i32,
        >(dev, 17)(dev, null(), null(), 0, null())
            == 0;
    check!(ok, "BeginScene / Clear / EndScene / Present");
    dev
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ResourceDesc {
    dimension: u32,
    alignment: u64,
    width: u64,
    height: u32,
    depth: u16,
    mips: u16,
    format: u32,
    samples: u32,
    quality: u32,
    layout: u32,
    flags: u32,
}

unsafe fn committed(dev: *mut c_void, heap: u32, desc: &ResourceDesc) -> *mut c_void {
    let props = [heap, 0, 0, 0, 0];
    let mut res = null_mut();
    let hr = vt::<
        unsafe extern "win64" fn(
            *mut c_void,
            *const [u32; 5],
            u32,
            *const ResourceDesc,
            u32,
            *const c_void,
            *const Guid,
            *mut *mut c_void,
        ) -> i32,
    >(dev, 27)(dev, &props, 0, desc, 0, null(), &IID_RESOURCE, &mut res);
    if hr == 0 {
        res
    } else {
        null_mut()
    }
}

unsafe fn d3d12_suite() {
    out(b"\n-- Direct3D 12 --\n");
    let p = proc(c"d3d12.dll", c"D3D12CreateDevice");
    check!(!p.is_null(), "D3D12CreateDevice exporte");
    if p.is_null() {
        return;
    }
    let create: unsafe extern "win64" fn(*mut c_void, u32, *const Guid, *mut *mut c_void) -> i32 =
        core::mem::transmute(p);
    check!(
        create(null_mut(), 0xb000, &IID_D3D12_DEVICE, null_mut()) == 1,
        "sondage FL 11_0 (S_FALSE)"
    );
    let mut dev = null_mut();
    check!(
        create(null_mut(), 0xb000, &IID_D3D12_DEVICE, &mut dev) == 0 && !dev.is_null(),
        "ID3D12Device cree"
    );
    if dev.is_null() {
        return;
    }
    let qdesc = [0u32; 4];
    let mut q = null_mut();
    vt::<Create3>(dev, 8)(dev, qdesc.as_ptr().cast(), &IID_QUEUE, &mut q);
    let mut alloc = null_mut();
    vt::<unsafe extern "win64" fn(*mut c_void, u32, *const Guid, *mut *mut c_void) -> i32>(dev, 9)(
        dev,
        0,
        &IID_ALLOCATOR,
        &mut alloc,
    );
    let mut list = null_mut();
    vt::<
        unsafe extern "win64" fn(
            *mut c_void,
            u32,
            u32,
            *mut c_void,
            *mut c_void,
            *const Guid,
            *mut *mut c_void,
        ) -> i32,
    >(dev, 12)(dev, 0, 0, alloc, null_mut(), &IID_LIST, &mut list);
    let hdesc = [2u32, 2, 0, 0]; // RTV, 2 descripteurs
    let mut heap = null_mut();
    vt::<Create3>(dev, 14)(dev, hdesc.as_ptr().cast(), &IID_HEAP, &mut heap);
    check!(
        !q.is_null() && !alloc.is_null() && !list.is_null() && !heap.is_null(),
        "file, allocateur, liste, tas"
    );
    if q.is_null() || list.is_null() || heap.is_null() {
        return;
    }
    let mut rtv = 0usize;
    vt::<unsafe extern "win64" fn(*mut c_void, *mut usize) -> *mut usize>(heap, 9)(heap, &mut rtv);
    let inc = vt::<unsafe extern "win64" fn(*mut c_void, u32) -> u32>(dev, 15)(dev, 2);
    check!(
        rtv != 0 && inc > 0,
        "poignee CPU de tas (retour par pointeur cache)"
    );

    let rt_desc = ResourceDesc {
        dimension: 3,
        width: 8,
        height: 8,
        depth: 1,
        mips: 1,
        format: 28,
        samples: 1,
        flags: 1,
        ..Default::default()
    };
    let rt = committed(dev, 1, &rt_desc);
    let rb_desc = ResourceDesc {
        dimension: 1,
        width: 256 * 8,
        height: 1,
        depth: 1,
        mips: 1,
        samples: 1,
        layout: 1,
        ..Default::default()
    };
    let rb = committed(dev, 3, &rb_desc);
    check!(!rt.is_null() && !rb.is_null(), "ressources committed");
    if rt.is_null() || rb.is_null() {
        return;
    }
    vt::<unsafe extern "win64" fn(*mut c_void, *mut c_void, *const c_void, usize)>(dev, 20)(
        dev,
        rt,
        null(),
        rtv,
    );
    vt::<unsafe extern "win64" fn(*mut c_void, u32, *const usize, i32, *const usize)>(list, 46)(
        list,
        1,
        &rtv,
        0,
        null(),
    );
    let green = [0.0f32, 1.0, 0.0, 1.0];
    vt::<unsafe extern "win64" fn(*mut c_void, usize, *const [f32; 4], u32, *const c_void)>(
        list, 48,
    )(list, rtv, &green, 0, null());
    // Copie vers le tampon de relecture (empreinte : pas de ligne 256).
    let dst: [u64; 6] = [rb as u64, 1, 0, 28 | (8u64 << 32), 8 | (1u64 << 32), 256];
    let src: [u64; 6] = [rt as u64, 0, 0, 0, 0, 0];
    vt::<
        unsafe extern "win64" fn(
            *mut c_void,
            *const [u64; 6],
            u32,
            u32,
            u32,
            *const [u64; 6],
            *const c_void,
        ),
    >(list, 16)(list, &dst, 0, 0, 0, &src, null());
    check!(
        vt::<unsafe extern "win64" fn(*mut c_void) -> i32>(list, 9)(list) == 0,
        "Close"
    );
    vt::<unsafe extern "win64" fn(*mut c_void, u32, *const *mut c_void)>(q, 10)(q, 1, &list);

    let mut fence = null_mut();
    vt::<unsafe extern "win64" fn(*mut c_void, u64, u32, *const Guid, *mut *mut c_void) -> i32>(
        dev, 36,
    )(dev, 0, 0, &IID_FENCE, &mut fence);
    check!(!fence.is_null(), "ID3D12Fence");
    if !fence.is_null() {
        let ev = CreateEventA(null_mut(), 0, 0, null::<c_char>());
        vt::<unsafe extern "win64" fn(*mut c_void, u64, Handle) -> i32>(fence, 9)(fence, 1, ev);
        vt::<unsafe extern "win64" fn(*mut c_void, *mut c_void, u64) -> i32>(q, 14)(q, fence, 1);
        check!(
            WaitForSingleObject(ev, 1000) == 0,
            "Signal + SetEventOnCompletion"
        );
        CloseHandle(ev);
        release(fence);
    }
    let mut mapped: *mut u8 = null_mut();
    vt::<unsafe extern "win64" fn(*mut c_void, u32, *const c_void, *mut *mut u8) -> i32>(rb, 8)(
        rb,
        0,
        null(),
        &mut mapped,
    );
    let ok = !mapped.is_null()
        && *mapped == 0
        && *mapped.add(1) == 255
        && *mapped.add(256 * 7 + 4 * 7 + 1) == 255;
    check!(
        ok,
        "effacement relu apres copie (Vulkan, OpenGL ou processeur)"
    );

    // Swapchain DXGI 1.4 sur la file D3D12.
    let fp = proc(c"dxgi.dll", c"CreateDXGIFactory2");
    let mut factory = null_mut();
    if !fp.is_null() {
        let cf: unsafe extern "win64" fn(u32, *const Guid, *mut *mut c_void) -> i32 =
            core::mem::transmute(fp);
        cf(0, &IID_FACTORY4, &mut factory);
    }
    check!(!factory.is_null(), "IDXGIFactory4");
    if !factory.is_null() {
        let sd: [u32; 12] = [64, 64, 28, 0, 1, 0, 0x20, 2, 0, 4, 0, 0];
        let mut sc = null_mut();
        let hr = vt::<
            unsafe extern "win64" fn(
                *mut c_void,
                *mut c_void,
                usize,
                *const [u32; 12],
                *const c_void,
                *mut c_void,
                *mut *mut c_void,
            ) -> i32,
        >(factory, 15)(factory, q, 0, &sd, null(), null_mut(), &mut sc);
        check!(
            hr == 0 && !sc.is_null(),
            "CreateSwapChainForHwnd sur une file D3D12"
        );
        if !sc.is_null() {
            let idx0 = vt::<unsafe extern "win64" fn(*mut c_void) -> u32>(sc, 36)(sc);
            vt::<unsafe extern "win64" fn(*mut c_void, u32, u32) -> i32>(sc, 8)(sc, 1, 0);
            let idx1 = vt::<unsafe extern "win64" fn(*mut c_void) -> u32>(sc, 36)(sc);
            check!(
                idx0 == 0 && idx1 == 1,
                "Present et GetCurrentBackBufferIndex"
            );
            release(sc);
        }
        release(factory);
    }
    for o in [rb, rt, heap, list, alloc, q, dev] {
        release(o);
    }
}

unsafe fn dxva2_suite(d3d9_dev: *mut c_void) {
    out(b"\n-- DXVA2 --\n");
    let p = proc(c"dxva2.dll", c"DXVA2CreateDirect3DDeviceManager9");
    check!(!p.is_null(), "DXVA2CreateDirect3DDeviceManager9 exporte");
    if p.is_null() || d3d9_dev.is_null() {
        return;
    }
    let create: unsafe extern "win64" fn(*mut u32, *mut *mut c_void) -> i32 =
        core::mem::transmute(p);
    let mut token = 0;
    let mut mgr = null_mut();
    check!(
        create(&mut token, &mut mgr) == 0 && !mgr.is_null(),
        "IDirect3DDeviceManager9"
    );
    if mgr.is_null() {
        return;
    }
    let reset = vt::<unsafe extern "win64" fn(*mut c_void, *mut c_void, u32) -> i32>(mgr, 3)(
        mgr, d3d9_dev, token,
    );
    let mut h = 0usize;
    let open = vt::<unsafe extern "win64" fn(*mut c_void, *mut usize) -> i32>(mgr, 4)(mgr, &mut h);
    let mut svc = null_mut();
    let get = vt::<
        unsafe extern "win64" fn(*mut c_void, usize, *const Guid, *mut *mut c_void) -> i32,
    >(mgr, 9)(mgr, h, &IID_DECODER_SERVICE, &mut svc);
    check!(
        reset == 0 && open == 0 && get == 0 && !svc.is_null(),
        "ResetDevice / OpenDeviceHandle / GetVideoService"
    );
    if !svc.is_null() {
        let mut n = 0u32;
        let mut guids: *mut c_void = null_mut();
        let hr = vt::<unsafe extern "win64" fn(*mut c_void, *mut u32, *mut *mut c_void) -> i32>(
            svc, 4,
        )(svc, &mut n, &mut guids);
        // Sans décodage matériel, la liste est vide : l'application décode
        // alors en logiciel. Avec VA-API, au moins H.264.
        check!(
            hr == 0 && n <= 8,
            "GetDecoderDeviceGuids (liste du materiel reel)"
        );
        CoTaskMemFree(guids);
        let mut surfs = [null_mut::<c_void>(); 2];
        let nv12 = u32::from_le_bytes(*b"NV12");
        let hr = vt::<
            unsafe extern "win64" fn(
                *mut c_void,
                u32,
                u32,
                u32,
                u32,
                u32,
                u32,
                u32,
                *mut *mut c_void,
                *mut c_void,
            ) -> i32,
        >(svc, 3)(
            svc,
            64,
            32,
            1,
            nv12,
            0,
            0,
            1,
            surfs.as_mut_ptr(),
            null_mut(),
        );
        check!(
            hr == 0 && !surfs[0].is_null() && !surfs[1].is_null(),
            "surfaces video NV12"
        );
        for s in surfs {
            release(s);
        }
        release(svc);
    }
    release(mgr);
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    out(b"Winlux - suite Direct3D / DXVA2\n");
    unsafe {
        graphics_info();
        let dev9 = d3d9_suite();
        d3d12_suite();
        dxva2_suite(dev9);
        release(dev9);
        if FAILURES == 0 {
            out(b"\nTous les tests Direct3D passent.\n");
        }
    }
    finish()
}
