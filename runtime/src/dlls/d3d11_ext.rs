//! Interfaces Direct3D 11.1 à 11.4 : ID3D11Device1 à 5, ID3D11DeviceContext1
//! à 4, ID3D10Multithread et ID3D11Fence (#561).
//!
//! Chaque version ajoute ses méthodes à la fin de la précédente : l'appareil
//! et le contexte gardent un seul objet, dont la table virtuelle se prolonge
//! par [`DeviceExt`] (méthodes 43 à 68) et [`ContextExt`] (115 à 148).
//! L'ordre des méthodes suit les en-têtes publics `d3d11_1.h` à `d3d11_4.h`
//! (MinGW-w64, domaine public).
//!
//! Le rendu Winlux est synchrone : une barrière (fence) est atteinte dès que
//! le contexte la signale, et les ressources « en tuiles » ne sont pas prises
//! en charge (le niveau de tuiles annoncé par CheckFeatureSupport est 0).

use super::pipeline;
use super::{
    as_simple, create_dummy_com_obj, ctx_clear_rtv, ctx_copy_subresource, ctx_flush,
    ctx_update_subresource, dev_check_ms, dev_create_rtv, dev_create_srv, dev_create_stub,
    dev_create_tex2d, dev_create_uav, dev_get_immediate_context, gl_here, Device, DeviceContext,
    E_INVALIDARG, E_NOINTERFACE, IID_IUNKNOWN, S_OK,
};
use crate::dlls::gl_bindings as gl;
use crate::dlls::ole32::Guid;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};

/// ID3D11Device1 (a04bfb29), 2 (9d06dffa), 3 (a05c8c37), 4 (8992ab71),
/// 5 (8ffde202), reconnus par leur premier mot.
pub(super) fn is_device_n_iid(riid: &Guid) -> bool {
    [
        0xa04b_fb29,
        0x9d06_dffa,
        0xa05c_8c37,
        0x8992_ab71,
        0x8ffd_e202,
    ]
    .contains(&riid.data1)
}

/// ID3D11DeviceContext1 (bb2c6faa), 2 (420d5b32), 3 (b4e3c01d), 4 (917600da).
pub(super) fn is_context_n_iid(riid: &Guid) -> bool {
    [0xbb2c_6faa, 0x420d_5b32, 0xb4e3_c01d, 0x9176_00da].contains(&riid.data1)
}

/// ID3D10Multithread et ID3D11Multithread partagent le même identifiant.
pub(super) const IID_MULTITHREAD: Guid = Guid {
    data1: 0x9b7e_4e00,
    data2: 0x342c,
    data3: 0x4106,
    data4: [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
};

/// ID3D11Fence et ID3D11DeviceChild.
const IID_FENCE: Guid = Guid {
    data1: 0xaffd_e9d1,
    data2: 0x1df7,
    data3: 0x4bb7,
    data4: [0x8a, 0x34, 0x0f, 0x46, 0x25, 0x1d, 0xab, 0x80],
};
const IID_DEVICE_CHILD: Guid = Guid {
    data1: 0x1841_e5c8,
    data2: 0x16b0,
    data3: 0x489b,
    data4: [0xbc, 0xc8, 0x44, 0xcf, 0xb0, 0xd5, 0xde, 0xae],
};

type Out = *mut *mut c_void;

// ────────────────────── ID3D11Device1 à 5 ──────────────────────

#[repr(C)]
pub(super) struct DeviceExt {
    // ID3D11Device1 (43-49)
    get_immediate_context1: unsafe extern "win64" fn(*mut Device, Out),
    create_deferred_context1: unsafe extern "win64" fn(*mut Device, u32, Out) -> i32,
    create_blend_state1: unsafe extern "win64" fn(*mut Device, *const c_void, Out) -> i32,
    create_rasterizer_state1: unsafe extern "win64" fn(*mut Device, *const c_void, Out) -> i32,
    create_device_context_state: unsafe extern "win64" fn(
        *mut Device,
        u32,
        *const u32,
        u32,
        u32,
        *const Guid,
        *mut u32,
        Out,
    ) -> i32,
    open_shared_resource1: unsafe extern "win64" fn(*mut Device, usize, *const Guid, Out) -> i32,
    open_shared_resource_by_name:
        unsafe extern "win64" fn(*mut Device, *const u16, u32, *const Guid, Out) -> i32,
    // ID3D11Device2 (50-53)
    get_immediate_context2: unsafe extern "win64" fn(*mut Device, Out),
    create_deferred_context2: unsafe extern "win64" fn(*mut Device, u32, Out) -> i32,
    get_resource_tiling: unsafe extern "win64" fn(
        *mut Device,
        *mut c_void,
        *mut u32,
        *mut c_void,
        *mut c_void,
        *mut u32,
        u32,
        *mut c_void,
    ),
    check_multisample_quality_levels1:
        unsafe extern "win64" fn(*mut Device, u32, u32, u32, *mut u32) -> i32,
    // ID3D11Device3 (54-64)
    create_texture2d1:
        unsafe extern "win64" fn(*mut Device, *const c_void, *const c_void, Out) -> i32,
    create_texture3d1:
        unsafe extern "win64" fn(*mut Device, *const c_void, *const c_void, Out) -> i32,
    create_rasterizer_state2: unsafe extern "win64" fn(*mut Device, *const c_void, Out) -> i32,
    create_shader_resource_view1:
        unsafe extern "win64" fn(*mut Device, *mut c_void, *const c_void, Out) -> i32,
    create_unordered_access_view1:
        unsafe extern "win64" fn(*mut Device, *mut c_void, *const c_void, Out) -> i32,
    create_render_target_view1:
        unsafe extern "win64" fn(*mut Device, *mut c_void, *const c_void, Out) -> i32,
    create_query1: unsafe extern "win64" fn(*mut Device, *const c_void, Out) -> i32,
    get_immediate_context3: unsafe extern "win64" fn(*mut Device, Out),
    create_deferred_context3: unsafe extern "win64" fn(*mut Device, u32, Out) -> i32,
    write_to_subresource: unsafe extern "win64" fn(
        *mut Device,
        *mut c_void,
        u32,
        *const c_void,
        *const c_void,
        u32,
        u32,
    ),
    read_from_subresource: unsafe extern "win64" fn(
        *mut Device,
        *mut c_void,
        u32,
        u32,
        *mut c_void,
        u32,
        *const c_void,
    ),
    // ID3D11Device4 (65-66)
    register_device_removed_event: unsafe extern "win64" fn(*mut Device, usize, *mut u32) -> i32,
    unregister_device_removed: unsafe extern "win64" fn(*mut Device, u32),
    // ID3D11Device5 (67-68)
    open_shared_fence: unsafe extern "win64" fn(*mut Device, usize, *const Guid, Out) -> i32,
    create_fence: unsafe extern "win64" fn(*mut Device, u64, u32, *const Guid, Out) -> i32,
}

unsafe extern "win64" fn dev_deferred(_: *mut Device, _: u32, out: Out) -> i32 {
    // Comme ID3D11Device::CreateDeferredContext : pas de liste de commandes.
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    0x887a_0004u32 as i32 // DXGI_ERROR_UNSUPPORTED
}

/// D3D11_RENDER_TARGET_BLEND_DESC1 : le BlendEnable de 11.0 suivi de
/// LogicOpEnable, puis LogicOp avant le masque d'écriture.
#[repr(C)]
#[derive(Clone, Copy)]
struct RtBlend1 {
    enable: i32,
    logic_enable: i32,
    src: u32,
    dst: u32,
    op: u32,
    src_alpha: u32,
    dst_alpha: u32,
    op_alpha: u32,
    logic_op: u32,
    write_mask: u32,
}

#[repr(C)]
struct BlendDesc1 {
    alpha_to_coverage: i32,
    independent: i32,
    rt: [RtBlend1; 8],
}

unsafe extern "win64" fn dev_blend1(dev: *mut Device, desc: *const c_void, out: Out) -> i32 {
    if desc.is_null() {
        if !out.is_null() {
            *out = std::ptr::null_mut();
        }
        return E_INVALIDARG;
    }
    let d = &*(desc as *const BlendDesc1);
    let mut d0 = pipeline::BlendDesc {
        alpha_to_coverage: d.alpha_to_coverage,
        independent: d.independent,
        ..Default::default()
    };
    for (o, i) in d0.rt.iter_mut().zip(d.rt.iter()) {
        *o = pipeline::RtBlend {
            enable: i.enable,
            src: i.src,
            dst: i.dst,
            op: i.op,
            src_alpha: i.src_alpha,
            dst_alpha: i.dst_alpha,
            op_alpha: i.op_alpha,
            write_mask: i.write_mask,
        };
    }
    pipeline::dev_create_blend(dev, (&d0 as *const pipeline::BlendDesc).cast(), out)
}

unsafe extern "win64" fn dev_context_state(
    dev: *mut Device,
    _flags: u32,
    levels: *const u32,
    n: u32,
    _sdk: u32,
    _iid: *const Guid,
    chosen: *mut u32,
    out: Out,
) -> i32 {
    let fl = if levels.is_null() || n == 0 {
        (*dev).feature_level
    } else {
        super::pick_feature_level(std::slice::from_raw_parts(levels, n as usize))
    };
    if !chosen.is_null() {
        *chosen = fl;
    }
    if !out.is_null() {
        *out = create_dummy_com_obj();
    }
    S_OK
}

unsafe extern "win64" fn dev_open_shared1(
    _: *mut Device,
    _: usize,
    _: *const Guid,
    out: Out,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    crate::dlls::d2d1::unsupported_call("ID3D11Device1::OpenSharedResource1")
}

unsafe extern "win64" fn dev_open_by_name(
    _: *mut Device,
    _: *const u16,
    _: u32,
    _: *const Guid,
    out: Out,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    crate::dlls::d2d1::unsupported_call("ID3D11Device1::OpenSharedResourceByName")
}

unsafe extern "win64" fn dev_resource_tiling(
    _: *mut Device,
    _: *mut c_void,
    tile_count: *mut u32,
    mip_desc: *mut c_void,
    tile_shape: *mut c_void,
    sub_count: *mut u32,
    _: u32,
    _: *mut c_void,
) {
    // Aucune ressource en tuiles : tout est vide.
    if !tile_count.is_null() {
        *tile_count = 0;
    }
    if !mip_desc.is_null() {
        std::ptr::write_bytes(mip_desc as *mut u8, 0, 16);
    }
    if !tile_shape.is_null() {
        std::ptr::write_bytes(tile_shape as *mut u8, 0, 12);
    }
    if !sub_count.is_null() {
        *sub_count = 0;
    }
}

unsafe extern "win64" fn dev_check_ms1(
    dev: *mut Device,
    format: u32,
    count: u32,
    _flags: u32,
    out: *mut u32,
) -> i32 {
    dev_check_ms(dev, format, count, out)
}

unsafe extern "win64" fn dev_tex2d1(
    dev: *mut Device,
    desc: *const c_void,
    init: *const c_void,
    out: Out,
) -> i32 {
    // D3D11_TEXTURE2D_DESC1 = D3D11_TEXTURE2D_DESC + TextureLayout.
    dev_create_tex2d(dev, desc.cast(), init.cast(), out)
}

unsafe extern "win64" fn dev_create_query1(_: *mut Device, desc: *const c_void, out: Out) -> i32 {
    // D3D11_QUERY_DESC1 : D3D11_QUERY_DESC suivi de ContextType (#1317).
    super::query::create(desc.cast(), true, out)
}

unsafe extern "win64" fn dev_write_sub(
    dev: *mut Device,
    res: *mut c_void,
    sub: u32,
    box_: *const c_void,
    src: *const c_void,
    row: u32,
    depth: u32,
) {
    ctx_update_subresource((*dev).context, res, sub, box_, src, row, depth);
}

unsafe extern "win64" fn dev_read_sub(
    _: *mut Device,
    _: *mut c_void,
    _: u32,
    _: u32,
    _: *mut c_void,
    _: u32,
    _: *const c_void,
) {
    // Réservé aux textures à disposition « standard swizzle » lisibles par le
    // processeur, que CreateTexture2D1 ne crée jamais ici.
}

unsafe extern "win64" fn dev_register_removed(_: *mut Device, _: usize, cookie: *mut u32) -> i32 {
    // L'appareil n'est jamais retiré : l'évènement ne sera jamais signalé.
    if cookie.is_null() {
        return E_INVALIDARG;
    }
    *cookie = 1;
    S_OK
}

unsafe extern "win64" fn dev_unregister_removed(_: *mut Device, _: u32) {}

unsafe extern "win64" fn dev_open_fence(_: *mut Device, _: usize, _: *const Guid, out: Out) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    crate::dlls::d2d1::unsupported_call("ID3D11Device5::OpenSharedFence")
}

unsafe extern "win64" fn dev_create_fence(
    _: *mut Device,
    initial: u64,
    _flags: u32,
    riid: *const Guid,
    out: Out,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let f = Box::into_raw(Box::new(Fence {
        vtable: crate::dlls::dx11_x86::abi(&FENCE_VTABLE, "dlls::d3d11_ext", "FenceVTable"),
        refs: AtomicU32::new(1),
        value: AtomicU64::new(initial),
        waiting: Mutex::new(Vec::new()),
    }));
    let hr = fence_qi(f, riid, out);
    fence_release(f);
    hr
}

pub(super) const DEVICE_EXT: DeviceExt = DeviceExt {
    get_immediate_context1: dev_get_immediate_context,
    create_deferred_context1: dev_deferred,
    create_blend_state1: dev_blend1,
    create_rasterizer_state1: pipeline::dev_create_raster,
    create_device_context_state: dev_context_state,
    open_shared_resource1: dev_open_shared1,
    open_shared_resource_by_name: dev_open_by_name,
    get_immediate_context2: dev_get_immediate_context,
    create_deferred_context2: dev_deferred,
    get_resource_tiling: dev_resource_tiling,
    check_multisample_quality_levels1: dev_check_ms1,
    create_texture2d1: dev_tex2d1,
    create_texture3d1: dev_create_stub,
    // D3D11_RASTERIZER_DESC1 et 2 prolongent la description 11.0.
    create_rasterizer_state2: pipeline::dev_create_raster,
    // Les descriptions de vue « 1 » ajoutent PlaneSlice après les champs 11.0.
    create_shader_resource_view1: dev_create_srv,
    create_unordered_access_view1: dev_create_uav,
    create_render_target_view1: dev_create_rtv,
    create_query1: dev_create_query1,
    get_immediate_context3: dev_get_immediate_context,
    create_deferred_context3: dev_deferred,
    write_to_subresource: dev_write_sub,
    read_from_subresource: dev_read_sub,
    register_device_removed_event: dev_register_removed,
    unregister_device_removed: dev_unregister_removed,
    open_shared_fence: dev_open_fence,
    create_fence: dev_create_fence,
};

// ────────────────────── ID3D11Fence ──────────────────────

#[repr(C)]
struct FenceVTable {
    query_interface: unsafe extern "win64" fn(*mut Fence, *const Guid, Out) -> i32,
    add_ref: unsafe extern "win64" fn(*mut Fence) -> u32,
    release: unsafe extern "win64" fn(*mut Fence) -> u32,
    get_device: unsafe extern "win64" fn(*mut Fence, Out),
    get_private_data:
        unsafe extern "win64" fn(*mut Fence, *const Guid, *mut u32, *mut c_void) -> i32,
    set_private_data: unsafe extern "win64" fn(*mut Fence, *const Guid, u32, *const c_void) -> i32,
    set_private_data_interface:
        unsafe extern "win64" fn(*mut Fence, *const Guid, *const c_void) -> i32,
    create_shared_handle:
        unsafe extern "win64" fn(*mut Fence, *const c_void, u32, *const u16, *mut usize) -> i32,
    get_completed_value: unsafe extern "win64" fn(*mut Fence) -> u64,
    set_event_on_completion: unsafe extern "win64" fn(*mut Fence, u64, usize) -> i32,
}

#[repr(C)]
struct Fence {
    vtable: *const FenceVTable,
    refs: AtomicU32,
    value: AtomicU64,
    /// Évènements à signaler quand la valeur atteindra le seuil.
    waiting: Mutex<Vec<(u64, usize)>>,
}

unsafe extern "win64" fn fence_qi(this: *mut Fence, riid: *const Guid, out: Out) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let iid = if riid.is_null() { IID_IUNKNOWN } else { *riid };
    if iid == IID_IUNKNOWN || iid == IID_FENCE || iid == IID_DEVICE_CHILD {
        (*this).refs.fetch_add(1, Ordering::Relaxed);
        *out = this.cast();
        return S_OK;
    }
    crate::dlls::comobj::log_no_interface("d3d11", &iid);
    *out = std::ptr::null_mut();
    E_NOINTERFACE
}
unsafe extern "win64" fn fence_addref(this: *mut Fence) -> u32 {
    (*this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn fence_release(this: *mut Fence) -> u32 {
    let prev = (*this).refs.fetch_sub(1, Ordering::AcqRel);
    if prev == 1 {
        drop(Box::from_raw(this));
        return 0;
    }
    prev - 1
}
unsafe extern "win64" fn fence_get_device(_: *mut Fence, out: Out) {
    super::child_get_device(std::ptr::null_mut(), out);
}
unsafe extern "win64" fn fence_get_priv(
    _: *mut Fence,
    _: *const Guid,
    size: *mut u32,
    _: *mut c_void,
) -> i32 {
    if !size.is_null() {
        *size = 0;
    }
    0x887a_0002u32 as i32 // DXGI_ERROR_NOT_FOUND
}
unsafe extern "win64" fn fence_set_priv(
    _: *mut Fence,
    _: *const Guid,
    _: u32,
    _: *const c_void,
) -> i32 {
    S_OK
}
unsafe extern "win64" fn fence_set_priv_if(_: *mut Fence, _: *const Guid, _: *const c_void) -> i32 {
    S_OK
}
unsafe extern "win64" fn fence_shared_handle(
    _: *mut Fence,
    _: *const c_void,
    _: u32,
    _: *const u16,
    out: *mut usize,
) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    crate::dlls::d2d1::unsupported_call("ID3D11Fence::CreateSharedHandle")
}
unsafe extern "win64" fn fence_completed(this: *mut Fence) -> u64 {
    (*this).value.load(Ordering::Acquire)
}
unsafe extern "win64" fn fence_on_completion(this: *mut Fence, value: u64, event: usize) -> i32 {
    if event == 0 {
        // Sans évènement, l'appel attend : la valeur est déjà atteinte ou le
        // sera par un Signal d'un autre fil.
        while fence_completed(this) < value {
            std::thread::yield_now();
        }
        return S_OK;
    }
    if fence_completed(this) >= value {
        crate::dlls::kernel32::set_event(event);
    } else {
        let mut w = (*this).waiting.lock().unwrap_or_else(|e| e.into_inner());
        w.push((value, event));
    }
    S_OK
}

/// ID3D11DeviceContext4::Signal : le travail précédent est déjà exécuté.
unsafe fn fence_signal(this: *mut Fence, value: u64) {
    (*this).value.store(value, Ordering::Release);
    let mut w = (*this).waiting.lock().unwrap_or_else(|e| e.into_inner());
    w.retain(|&(v, ev)| {
        if v <= value {
            crate::dlls::kernel32::set_event(ev);
            false
        } else {
            true
        }
    });
}

static FENCE_VTABLE: FenceVTable = FenceVTable {
    query_interface: fence_qi,
    add_ref: fence_addref,
    release: fence_release,
    get_device: fence_get_device,
    get_private_data: fence_get_priv,
    set_private_data: fence_set_priv,
    set_private_data_interface: fence_set_priv_if,
    create_shared_handle: fence_shared_handle,
    get_completed_value: fence_completed,
    set_event_on_completion: fence_on_completion,
};

// ────────────────────── ID3D11DeviceContext1 à 4 ──────────────────────

type Ctx = *mut DeviceContext;
type SetCb1 = unsafe extern "win64" fn(Ctx, u32, u32, *const *mut c_void, *const u32, *const u32);
type GetCb1 = unsafe extern "win64" fn(Ctx, u32, u32, *mut *mut c_void, *mut u32, *mut u32);

#[repr(C)]
pub(super) struct ContextExt {
    // ID3D11DeviceContext1 (115-133)
    copy_subresource_region1: unsafe extern "win64" fn(
        Ctx,
        *mut c_void,
        u32,
        u32,
        u32,
        u32,
        *mut c_void,
        u32,
        *const c_void,
        u32,
    ),
    update_subresource1: unsafe extern "win64" fn(
        Ctx,
        *mut c_void,
        u32,
        *const c_void,
        *const c_void,
        u32,
        u32,
        u32,
    ),
    discard_resource: unsafe extern "win64" fn(Ctx, *mut c_void),
    discard_view: unsafe extern "win64" fn(Ctx, *mut c_void),
    vs_set_cbs1: SetCb1,
    hs_set_cbs1: SetCb1,
    ds_set_cbs1: SetCb1,
    gs_set_cbs1: SetCb1,
    ps_set_cbs1: SetCb1,
    cs_set_cbs1: SetCb1,
    vs_get_cbs1: GetCb1,
    hs_get_cbs1: GetCb1,
    ds_get_cbs1: GetCb1,
    gs_get_cbs1: GetCb1,
    ps_get_cbs1: GetCb1,
    cs_get_cbs1: GetCb1,
    swap_device_context_state: unsafe extern "win64" fn(Ctx, *mut c_void, Out),
    clear_view: unsafe extern "win64" fn(Ctx, *mut c_void, *const [f32; 4], *const [i32; 4], u32),
    discard_view1: unsafe extern "win64" fn(Ctx, *mut c_void, *const c_void, u32),
    // ID3D11DeviceContext2 (134-143)
    update_tile_mappings: unsafe extern "win64" fn(
        Ctx,
        *mut c_void,
        u32,
        *const c_void,
        *const c_void,
        *mut c_void,
        u32,
        *const u32,
        *const u32,
        *const u32,
        u32,
    ) -> i32,
    copy_tile_mappings: unsafe extern "win64" fn(
        Ctx,
        *mut c_void,
        *const c_void,
        *mut c_void,
        *const c_void,
        *const c_void,
        u32,
    ) -> i32,
    copy_tiles: unsafe extern "win64" fn(
        Ctx,
        *mut c_void,
        *const c_void,
        *const c_void,
        *mut c_void,
        u64,
        u32,
    ),
    update_tiles: unsafe extern "win64" fn(
        Ctx,
        *mut c_void,
        *const c_void,
        *const c_void,
        *const c_void,
        u32,
    ),
    resize_tile_pool: unsafe extern "win64" fn(Ctx, *mut c_void, u64) -> i32,
    tiled_resource_barrier: unsafe extern "win64" fn(Ctx, *mut c_void, *mut c_void),
    is_annotation_enabled: unsafe extern "win64" fn(Ctx) -> i32,
    set_marker_int: unsafe extern "win64" fn(Ctx, *const u16, i32),
    begin_event_int: unsafe extern "win64" fn(Ctx, *const u16, i32),
    end_event: unsafe extern "win64" fn(Ctx),
    // ID3D11DeviceContext3 (144-146)
    flush1: unsafe extern "win64" fn(Ctx, u32, usize),
    set_hardware_protection_state: unsafe extern "win64" fn(Ctx, i32),
    get_hardware_protection_state: unsafe extern "win64" fn(Ctx, *mut i32),
    // ID3D11DeviceContext4 (147-148)
    signal: unsafe extern "win64" fn(Ctx, *mut c_void, u64) -> i32,
    wait: unsafe extern "win64" fn(Ctx, *mut c_void, u64) -> i32,
}

unsafe extern "win64" fn ctx_copy_region1(
    this: Ctx,
    dst: *mut c_void,
    dst_sub: u32,
    x: u32,
    y: u32,
    z: u32,
    src: *mut c_void,
    src_sub: u32,
    box_: *const c_void,
    _flags: u32,
) {
    // D3D11_COPY_NO_OVERWRITE / DISCARD ne sont que des indications.
    ctx_copy_subresource(this, dst, dst_sub, x, y, z, src, src_sub, box_);
}

unsafe extern "win64" fn ctx_update1(
    this: Ctx,
    res: *mut c_void,
    sub: u32,
    box_: *const c_void,
    data: *const c_void,
    row: u32,
    depth: u32,
    _flags: u32,
) {
    ctx_update_subresource(this, res, sub, box_, data, row, depth);
}

unsafe extern "win64" fn ctx_discard(_: Ctx, _: *mut c_void) {}

macro_rules! cbs1 {
    ($set:ident, $get:ident, $stage:expr) => {
        unsafe extern "win64" fn $set(
            this: Ctx,
            start: u32,
            n: u32,
            bufs: *const *mut c_void,
            first: *const u32,
            count: *const u32,
        ) {
            if let Some(stage) = $stage {
                pipeline::set_cbs1(this, stage, start, n, bufs, first, count);
            }
        }
        unsafe extern "win64" fn $get(
            _: Ctx,
            _: u32,
            n: u32,
            bufs: *mut *mut c_void,
            first: *mut u32,
            count: *mut u32,
        ) {
            for i in 0..n as usize {
                if !bufs.is_null() {
                    *bufs.add(i) = std::ptr::null_mut();
                }
                if !first.is_null() {
                    *first.add(i) = 0;
                }
                if !count.is_null() {
                    *count.add(i) = 0;
                }
            }
        }
    };
}
// Les étages de coque et de domaine ne sont pas exécutés (comme en 11.0).
cbs1!(vs_set_cbs1, vs_get_cbs1, Some(0usize));
cbs1!(hs_set_cbs1, hs_get_cbs1, None::<usize>);
cbs1!(ds_set_cbs1, ds_get_cbs1, None::<usize>);
cbs1!(gs_set_cbs1, gs_get_cbs1, Some(2usize));
cbs1!(ps_set_cbs1, ps_get_cbs1, Some(1usize));
cbs1!(cs_set_cbs1, cs_get_cbs1, Some(3usize));

unsafe extern "win64" fn ctx_swap_state(_: Ctx, _state: *mut c_void, prev: Out) {
    // Un seul état d'appareil existe : l'ancien est rendu sous forme d'objet
    // neutre que l'appelant peut relâcher ou réactiver.
    if !prev.is_null() {
        *prev = create_dummy_com_obj();
    }
}

/// ClearView : sans rectangle, toute la vue ; sinon chaque rectangle, à
/// l'aide des ciseaux (la ligne 0 de la cible est le haut de l'image).
unsafe extern "win64" fn ctx_clear_view(
    this: Ctx,
    view: *mut c_void,
    color: *const [f32; 4],
    rects: *const [i32; 4],
    n: u32,
) {
    if color.is_null() {
        return;
    }
    if n == 0 || rects.is_null() {
        ctx_clear_rtv(this, view, color);
        return;
    }
    let (Some(g), Some(obj)) = (gl_here(), as_simple(view)) else {
        return;
    };
    if (*obj).gl_id == 0 {
        return;
    }
    let c = &*color;
    (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, (*obj).gl_id);
    pipeline::clear(g, || {
        (g.ColorMask)(1, 1, 1, 1);
        (g.ClearColor)(c[0], c[1], c[2], c[3]);
        (g.Enable)(gl::GL_SCISSOR_TEST);
        for i in 0..n as usize {
            let [l, t, r, b] = *rects.add(i);
            if r > l && b > t {
                (g.Scissor)(l, t, r - l, b - t);
                (g.Clear)(gl::GL_COLOR_BUFFER_BIT);
            }
        }
        (g.Disable)(gl::GL_SCISSOR_TEST);
    });
    let st = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, super::target_fbo(&st));
}

unsafe extern "win64" fn ctx_discard_view1(_: Ctx, _: *mut c_void, _: *const c_void, _: u32) {}

unsafe extern "win64" fn ctx_update_tile_mappings(
    _: Ctx,
    _: *mut c_void,
    _: u32,
    _: *const c_void,
    _: *const c_void,
    _: *mut c_void,
    _: u32,
    _: *const u32,
    _: *const u32,
    _: *const u32,
    _: u32,
) -> i32 {
    E_INVALIDARG
}
unsafe extern "win64" fn ctx_copy_tile_mappings(
    _: Ctx,
    _: *mut c_void,
    _: *const c_void,
    _: *mut c_void,
    _: *const c_void,
    _: *const c_void,
    _: u32,
) -> i32 {
    E_INVALIDARG
}
unsafe extern "win64" fn ctx_copy_tiles(
    _: Ctx,
    _: *mut c_void,
    _: *const c_void,
    _: *const c_void,
    _: *mut c_void,
    _: u64,
    _: u32,
) {
}
unsafe extern "win64" fn ctx_update_tiles(
    _: Ctx,
    _: *mut c_void,
    _: *const c_void,
    _: *const c_void,
    _: *const c_void,
    _: u32,
) {
}
unsafe extern "win64" fn ctx_resize_tile_pool(_: Ctx, _: *mut c_void, _: u64) -> i32 {
    E_INVALIDARG
}
unsafe extern "win64" fn ctx_tiled_barrier(_: Ctx, _: *mut c_void, _: *mut c_void) {}
unsafe extern "win64" fn ctx_annotations(_: Ctx) -> i32 {
    0
}
unsafe extern "win64" fn ctx_marker(_: Ctx, _: *const u16, _: i32) {}
unsafe extern "win64" fn ctx_end_event(_: Ctx) {}
unsafe extern "win64" fn ctx_flush1(this: Ctx, _type: u32, event: usize) {
    ctx_flush(this);
    if event != 0 {
        crate::dlls::kernel32::set_event(event);
    }
}
unsafe extern "win64" fn ctx_set_protection(_: Ctx, _: i32) {}
unsafe extern "win64" fn ctx_get_protection(_: Ctx, out: *mut i32) {
    if !out.is_null() {
        *out = 0;
    }
}
unsafe extern "win64" fn ctx_signal(_: Ctx, fence: *mut c_void, value: u64) -> i32 {
    if fence.is_null() {
        return E_INVALIDARG;
    }
    fence_signal(fence.cast(), value);
    S_OK
}
unsafe extern "win64" fn ctx_wait(_: Ctx, fence: *mut c_void, _: u64) -> i32 {
    if fence.is_null() {
        return E_INVALIDARG;
    }
    S_OK
}

pub(super) const CONTEXT_EXT: ContextExt = ContextExt {
    copy_subresource_region1: ctx_copy_region1,
    update_subresource1: ctx_update1,
    discard_resource: ctx_discard,
    discard_view: ctx_discard,
    vs_set_cbs1,
    hs_set_cbs1,
    ds_set_cbs1,
    gs_set_cbs1,
    ps_set_cbs1,
    cs_set_cbs1,
    vs_get_cbs1,
    hs_get_cbs1,
    ds_get_cbs1,
    gs_get_cbs1,
    ps_get_cbs1,
    cs_get_cbs1,
    swap_device_context_state: ctx_swap_state,
    clear_view: ctx_clear_view,
    discard_view1: ctx_discard_view1,
    update_tile_mappings: ctx_update_tile_mappings,
    copy_tile_mappings: ctx_copy_tile_mappings,
    copy_tiles: ctx_copy_tiles,
    update_tiles: ctx_update_tiles,
    resize_tile_pool: ctx_resize_tile_pool,
    tiled_resource_barrier: ctx_tiled_barrier,
    is_annotation_enabled: ctx_annotations,
    set_marker_int: ctx_marker,
    begin_event_int: ctx_marker,
    end_event: ctx_end_event,
    flush1: ctx_flush1,
    set_hardware_protection_state: ctx_set_protection,
    get_hardware_protection_state: ctx_get_protection,
    signal: ctx_signal,
    wait: ctx_wait,
};

// ────────────────────── ID3D10Multithread ──────────────────────

#[repr(C)]
struct MultithreadVTable {
    query_interface: unsafe extern "win64" fn(*mut Multithread, *const Guid, Out) -> i32,
    add_ref: unsafe extern "win64" fn(*mut Multithread) -> u32,
    release: unsafe extern "win64" fn(*mut Multithread) -> u32,
    enter: unsafe extern "win64" fn(*mut Multithread),
    leave: unsafe extern "win64" fn(*mut Multithread),
    set_multithread_protected: unsafe extern "win64" fn(*mut Multithread, i32) -> i32,
    get_multithread_protected: unsafe extern "win64" fn(*mut Multithread) -> i32,
}

/// Verrou réentrant d'ID3D10Multithread : fil propriétaire et profondeur.
#[repr(C)]
pub(super) struct Multithread {
    vtable: *const MultithreadVTable,
    owner: *mut Device,
    protected: AtomicU32,
    lock: Mutex<(u64, u32)>,
    cv: Condvar,
}

impl Multithread {
    pub(super) fn new() -> Self {
        Multithread {
            vtable: crate::dlls::dx11_x86::abi(
                &MULTITHREAD_VTABLE,
                "dlls::d3d11_ext",
                "MultithreadVTable",
            ),
            owner: std::ptr::null_mut(),
            protected: AtomicU32::new(1),
            lock: Mutex::new((0, 0)),
            cv: Condvar::new(),
        }
    }
    pub(super) fn set_owner(&mut self, dev: *mut Device) {
        self.owner = dev;
    }
}

fn thread_key() -> u64 {
    // Identifiant stable et non nul du fil courant.
    thread_local!(static KEY: u8 = const { 0 });
    KEY.with(|k| k as *const u8 as u64)
}

unsafe extern "win64" fn mt_qi(this: *mut Multithread, riid: *const Guid, out: Out) -> i32 {
    super::dev_qi((*this).owner, riid, out)
}
unsafe extern "win64" fn mt_addref(this: *mut Multithread) -> u32 {
    super::dev_addref((*this).owner)
}
unsafe extern "win64" fn mt_release(this: *mut Multithread) -> u32 {
    super::dev_release((*this).owner)
}
unsafe extern "win64" fn mt_enter(this: *mut Multithread) {
    let me = thread_key();
    let mut g = (*this).lock.lock().unwrap_or_else(|e| e.into_inner());
    while g.1 > 0 && g.0 != me {
        g = (*this).cv.wait(g).unwrap_or_else(|e| e.into_inner());
    }
    g.0 = me;
    g.1 += 1;
}
unsafe extern "win64" fn mt_leave(this: *mut Multithread) {
    let me = thread_key();
    let mut g = (*this).lock.lock().unwrap_or_else(|e| e.into_inner());
    if g.1 > 0 && g.0 == me {
        g.1 -= 1;
        if g.1 == 0 {
            g.0 = 0;
            (*this).cv.notify_one();
        }
    }
}
unsafe extern "win64" fn mt_set(this: *mut Multithread, on: i32) -> i32 {
    (*this).protected.swap((on != 0) as u32, Ordering::AcqRel) as i32
}
unsafe extern "win64" fn mt_get(this: *mut Multithread) -> i32 {
    (*this).protected.load(Ordering::Acquire) as i32
}

static MULTITHREAD_VTABLE: MultithreadVTable = MultithreadVTable {
    query_interface: mt_qi,
    add_ref: mt_addref,
    release: mt_release,
    enter: mt_enter,
    leave: mt_leave,
    set_multithread_protected: mt_set,
    get_multithread_protected: mt_get,
};

// ────────────────────── ID3DUserDefinedAnnotation ──────────────────────
// Marqueurs d'événements pour les outils de profilage, demandés au contexte
// par ANGLE et les moteurs de rendu de Chromium. Sans outil de profilage,
// Windows répond -1 à BeginEvent et EndEvent et FALSE à GetStatus : c'est
// exactement ce que fait cet objet.

/// ID3DUserDefinedAnnotation {b2daad8b-03d4-4dbf-95eb-32ab4b63d0ab}.
pub(super) const IID_ANNOTATION: Guid = Guid {
    data1: 0xb2da_ad8b,
    data2: 0x03d4,
    data3: 0x4dbf,
    data4: [0x95, 0xeb, 0x32, 0xab, 0x4b, 0x63, 0xd0, 0xab],
};

#[repr(C)]
struct AnnotationVTable {
    query_interface: unsafe extern "win64" fn(*mut Annotation, *const Guid, Out) -> i32,
    add_ref: unsafe extern "win64" fn(*mut Annotation) -> u32,
    release: unsafe extern "win64" fn(*mut Annotation) -> u32,
    begin_event: unsafe extern "win64" fn(*mut Annotation, *const u16) -> i32,
    end_event: unsafe extern "win64" fn(*mut Annotation) -> i32,
    set_marker: unsafe extern "win64" fn(*mut Annotation, *const u16),
    get_status: unsafe extern "win64" fn(*mut Annotation) -> i32,
}

/// Objet d'annotation : garde une référence sur son contexte.
#[repr(C)]
struct Annotation {
    vtable: *const AnnotationVTable,
    refs: AtomicU32,
    context: *mut DeviceContext,
}

/// Répond à QueryInterface(ID3DUserDefinedAnnotation) sur un contexte.
pub(super) unsafe fn annotation_for(context: *mut DeviceContext, out: Out) -> i32 {
    super::ctx_addref(context);
    let obj = Box::new(Annotation {
        vtable: crate::dlls::dx11_x86::abi(
            &ANNOTATION_VTABLE,
            "dlls::d3d11_ext",
            "AnnotationVTable",
        ),
        refs: AtomicU32::new(1),
        context,
    });
    *out = Box::into_raw(obj).cast();
    S_OK
}

unsafe extern "win64" fn ann_qi(this: *mut Annotation, riid: *const Guid, out: Out) -> i32 {
    if out.is_null() || riid.is_null() {
        return E_INVALIDARG;
    }
    if *riid == IID_ANNOTATION {
        (*this).refs.fetch_add(1, Ordering::Relaxed);
        *out = this.cast();
        return S_OK;
    }
    super::ctx_qi((*this).context, riid, out)
}
unsafe extern "win64" fn ann_addref(this: *mut Annotation) -> u32 {
    (*this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn ann_release(this: *mut Annotation) -> u32 {
    let left = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if left == 0 {
        let obj = Box::from_raw(this);
        super::ctx_release(obj.context);
    }
    left
}
unsafe extern "win64" fn ann_begin(_: *mut Annotation, _name: *const u16) -> i32 {
    -1
}
unsafe extern "win64" fn ann_end(_: *mut Annotation) -> i32 {
    -1
}
unsafe extern "win64" fn ann_marker(_: *mut Annotation, _name: *const u16) {}
unsafe extern "win64" fn ann_status(_: *mut Annotation) -> i32 {
    0
}

static ANNOTATION_VTABLE: AnnotationVTable = AnnotationVTable {
    query_interface: ann_qi,
    add_ref: ann_addref,
    release: ann_release,
    begin_event: ann_begin,
    end_event: ann_end,
    set_marker: ann_marker,
    get_status: ann_status,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_tables_have_windows_sizes() {
        let ptr = std::mem::size_of::<usize>();
        // ID3D11Device5 : 69 méthodes, dont 43 avant Device1.
        assert_eq!(std::mem::size_of::<DeviceExt>(), (69 - 43) * ptr);
        // ID3D11DeviceContext4 : 149 méthodes, dont 115 avant Context1.
        assert_eq!(std::mem::size_of::<ContextExt>(), (149 - 115) * ptr);
        assert_eq!(std::mem::size_of::<MultithreadVTable>(), 7 * ptr);
        assert_eq!(std::mem::size_of::<FenceVTable>(), 10 * ptr);
        assert_eq!(std::mem::size_of::<AnnotationVTable>(), 7 * ptr);
    }

    #[test]
    fn fence_signals_waiting_events_in_order() {
        unsafe {
            let mut out = std::ptr::null_mut();
            assert_eq!(
                dev_create_fence(std::ptr::null_mut(), 5, 0, &IID_FENCE, &mut out),
                S_OK
            );
            let f: *mut Fence = out.cast();
            assert_eq!(fence_completed(f), 5);
            fence_signal(f, 9);
            assert_eq!(fence_completed(f), 9);
            assert_eq!(fence_release(f), 0);
        }
    }

    #[test]
    fn context_answers_user_defined_annotation_like_without_profiler() {
        unsafe {
            let ctx = super::super::create_device_context();
            let mut out = std::ptr::null_mut();
            assert_eq!(super::super::ctx_qi(ctx, &IID_ANNOTATION, &mut out), S_OK);
            let a: *mut Annotation = out.cast();
            let name: Vec<u16> = "image".encode_utf16().chain([0]).collect();
            assert_eq!(ann_begin(a, name.as_ptr()), -1);
            ann_marker(a, name.as_ptr());
            assert_eq!(ann_end(a), -1);
            assert_eq!(ann_status(a), 0);
            assert_eq!(ann_release(a), 0);
            assert_eq!(super::super::ctx_release(ctx), 0);
        }
    }

    #[test]
    fn multithread_lock_is_reentrant() {
        let mut mt = Multithread::new();
        let p: *mut Multithread = &mut mt;
        unsafe {
            mt_enter(p);
            mt_enter(p);
            mt_leave(p);
            mt_leave(p);
            assert_eq!((*p).lock.lock().unwrap().1, 0);
            assert_eq!(mt_set(p, 0), 1);
            assert_eq!(mt_get(p), 0);
        }
    }
}
