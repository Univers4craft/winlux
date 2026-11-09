//! IDXGIResource et IDXGIResource1 d'une texture Direct3D 11.
//!
//! ANGLE (processus GPU de Chromium) demande IDXGIResource à la texture
//! que Chromium lui confie et déréférence le résultat sans tester le
//! HRESULT : le refus faisait tomber libGLESv2 sur une lecture de
//! l'adresse 0. La texture répond par une facette séparée, qui tient une
//! référence sur elle.
//!
//! Dispositions : dxgi.h de MinGW-w64 (domaine public).

use super::*;

/// IDXGIResource {035f3ab4-482e-4e50-b41f-8a7f8bd8960b}.
pub(super) const IID_DXGI_RESOURCE: Guid = Guid {
    data1: 0x035f_3ab4,
    data2: 0x482e,
    data3: 0x4e50,
    data4: [0xb4, 0x1f, 0x8a, 0x7f, 0x8b, 0xd8, 0x96, 0x0b],
};
/// IDXGIResource1 {30961379-4609-4a41-998e-54fe567ee0c1}.
pub(super) const IID_DXGI_RESOURCE1: Guid = Guid {
    data1: 0x3096_1379,
    data2: 0x4609,
    data3: 0x4a41,
    data4: [0x99, 0x8e, 0x54, 0xfe, 0x56, 0x7e, 0xe0, 0xc1],
};
/// IDXGIObject {aec22fb8-76f3-4639-9be0-28eb43a67a2e}.
const IID_DXGI_OBJECT: Guid = Guid {
    data1: 0xaec2_2fb8,
    data2: 0x76f3,
    data3: 0x4639,
    data4: [0x9b, 0xe0, 0x28, 0xeb, 0x43, 0xa6, 0x7a, 0x2e],
};
/// IDXGIDeviceSubObject {3d3e0379-f9de-4d58-bb6c-18d62992f1a6}.
const IID_DXGI_DEVICE_SUB_OBJECT: Guid = Guid {
    data1: 0x3d3e_0379,
    data2: 0xf9de,
    data3: 0x4d58,
    data4: [0xbb, 0x6c, 0x18, 0xd6, 0x29, 0x92, 0xf1, 0xa6],
};

const DXGI_USAGE_SHADER_INPUT: u32 = 0x10;
const DXGI_USAGE_RENDER_TARGET_OUTPUT: u32 = 0x20;
const DXGI_USAGE_UNORDERED_ACCESS: u32 = 0x400;
const D3D11_BIND_UNORDERED_ACCESS: u32 = 0x80;
/// D3D11_RESOURCE_MISC_SHARED et SHARED_KEYEDMUTEX.
const MISC_SHARED_ANY: u32 = 0x2 | 0x100;
const DXGI_RESOURCE_PRIORITY_NORMAL: u32 = 0x7800_0000;

/// Table d'IDXGIResource1 (14 méthodes).
#[repr(C)]
pub(super) struct DxgiResourceVTable {
    query_interface:
        unsafe extern "win64" fn(*mut DxgiResource, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut DxgiResource) -> u32,
    release: unsafe extern "win64" fn(*mut DxgiResource) -> u32,
    set_private_data:
        unsafe extern "win64" fn(*mut DxgiResource, *const Guid, u32, *const c_void) -> i32,
    set_private_data_interface:
        unsafe extern "win64" fn(*mut DxgiResource, *const Guid, *mut c_void) -> i32,
    get_private_data:
        unsafe extern "win64" fn(*mut DxgiResource, *const Guid, *mut u32, *mut c_void) -> i32,
    get_parent: unsafe extern "win64" fn(*mut DxgiResource, *const Guid, *mut *mut c_void) -> i32,
    get_device: unsafe extern "win64" fn(*mut DxgiResource, *const Guid, *mut *mut c_void) -> i32,
    get_shared_handle: unsafe extern "win64" fn(*mut DxgiResource, *mut usize) -> i32,
    get_usage: unsafe extern "win64" fn(*mut DxgiResource, *mut u32) -> i32,
    set_eviction_priority: unsafe extern "win64" fn(*mut DxgiResource, u32) -> i32,
    get_eviction_priority: unsafe extern "win64" fn(*mut DxgiResource, *mut u32) -> i32,
    create_subresource_surface:
        unsafe extern "win64" fn(*mut DxgiResource, u32, *mut *mut c_void) -> i32,
    create_shared_handle: unsafe extern "win64" fn(
        *mut DxgiResource,
        *const c_void,
        u32,
        *const u16,
        *mut usize,
    ) -> i32,
}

/// Facette IDXGIResource1 d'une texture : tient une référence sur elle.
#[repr(C)]
pub(super) struct DxgiResource {
    vtable: *const DxgiResourceVTable,
    ref_count: AtomicU32,
    tex: *mut Texture2D,
}

fn is_resource_iid(g: &Guid) -> bool {
    *g == IID_DXGI_RESOURCE
        || *g == IID_DXGI_RESOURCE1
        || *g == IID_DXGI_OBJECT
        || *g == IID_DXGI_DEVICE_SUB_OBJECT
}

fn resource_vt() -> *const DxgiResourceVTable {
    crate::dlls::dx11_x86::abi(&RESOURCE_VTABLE, "dlls::d3d11", "DxgiResourceVTable")
}

/// Réponse d'une texture à QueryInterface pour une interface DXGI de
/// ressource : une nouvelle facette, ou `None` pour un autre IID.
pub(super) unsafe fn texture_query(
    tex: *mut Texture2D,
    riid: &Guid,
    out: *mut *mut c_void,
) -> Option<i32> {
    if !is_resource_iid(riid) {
        return None;
    }
    Texture2D::add_ref(tex);
    let r = Box::into_raw(Box::new(DxgiResource {
        vtable: resource_vt(),
        ref_count: AtomicU32::new(1),
        tex,
    }));
    *out = r.cast();
    Some(S_OK)
}

unsafe extern "win64" fn res_qi(
    this: *mut DxgiResource,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || riid.is_null() {
        return E_INVALIDARG;
    }
    if is_resource_iid(&*riid) {
        res_add_ref(this);
        *out = this.cast();
        return S_OK;
    }
    Texture2D::query_interface((*this).tex, riid, out)
}
unsafe extern "win64" fn res_add_ref(this: *mut DxgiResource) -> u32 {
    (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn res_release(this: *mut DxgiResource) -> u32 {
    let prev = (*this).ref_count.fetch_sub(1, Ordering::AcqRel);
    if prev == 1 {
        let r = Box::from_raw(this);
        Texture2D::release(r.tex);
        return 0;
    }
    prev - 1
}
// Les données privées sont celles de la texture (même clé).
unsafe extern "win64" fn res_set_priv(
    this: *mut DxgiResource,
    g: *const Guid,
    size: u32,
    data: *const c_void,
) -> i32 {
    crate::dlls::dxgi_private::set((*this).tex as *const c_void, g, size, data)
}
unsafe extern "win64" fn res_set_priv_iface(
    this: *mut DxgiResource,
    g: *const Guid,
    unk: *mut c_void,
) -> i32 {
    Texture2D::set_priv_iface((*this).tex, g, unk)
}
unsafe extern "win64" fn res_get_priv(
    this: *mut DxgiResource,
    g: *const Guid,
    size: *mut u32,
    data: *mut c_void,
) -> i32 {
    crate::dlls::dxgi_private::get((*this).tex as *const c_void, g, size, data)
}
/// GetParent et GetDevice : l'appareil, sous l'interface demandée.
unsafe extern "win64" fn res_get_device(
    _: *mut DxgiResource,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    let mut dev = std::ptr::null_mut();
    child_get_device(std::ptr::null_mut(), &mut dev);
    if dev.is_null() {
        return E_FAIL;
    }
    let hr = dev_qi(dev.cast(), riid, out);
    dev_release(dev.cast());
    hr
}

/// Poignées partagées données dans ce processus (valeur → texture).
static SHARED: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

unsafe extern "win64" fn res_get_shared_handle(this: *mut DxgiResource, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let tex = (*this).tex;
    if (*tex).desc.misc_flags & MISC_SHARED_ANY == 0 {
        // Ressource non partagée : pas de poignée.
        *out = 0;
        return S_OK;
    }
    let mut s = crate::dlls::lock(&SHARED);
    let h = match s.iter().find(|(_, t)| *t == tex as usize) {
        Some(&(h, _)) => h,
        None => {
            // Valeur multiple de 4 hors des poignées du noyau Winlux.
            let h = 0x4000_0000 + 4 * (s.len() + 1);
            s.push((h, tex as usize));
            h
        }
    };
    *out = h;
    S_OK
}

/// Texture d'une poignée partagée donnée par [`res_get_shared_handle`].
pub(super) fn shared_texture(handle: usize) -> Option<*mut Texture2D> {
    crate::dlls::lock(&SHARED)
        .iter()
        .find(|(h, _)| *h == handle)
        .map(|&(_, t)| t as *mut Texture2D)
}

/// Oublie les poignées d'une texture détruite.
pub(super) fn forget_texture(tex: *const Texture2D) {
    crate::dlls::lock(&SHARED).retain(|(_, t)| *t != tex as usize);
}

unsafe extern "win64" fn res_get_usage(this: *mut DxgiResource, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let bind = (*(*this).tex).desc.bind_flags;
    let mut u = 0;
    if bind & D3D11_BIND_SHADER_RESOURCE != 0 {
        u |= DXGI_USAGE_SHADER_INPUT;
    }
    if bind & D3D11_BIND_RENDER_TARGET != 0 {
        u |= DXGI_USAGE_RENDER_TARGET_OUTPUT;
    }
    if bind & D3D11_BIND_UNORDERED_ACCESS != 0 {
        u |= DXGI_USAGE_UNORDERED_ACCESS;
    }
    *out = u;
    S_OK
}
unsafe extern "win64" fn res_set_eviction(_: *mut DxgiResource, _: u32) -> i32 {
    S_OK
}
unsafe extern "win64" fn res_get_eviction(_: *mut DxgiResource, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = DXGI_RESOURCE_PRIORITY_NORMAL;
    S_OK
}
unsafe extern "win64" fn res_create_subresource_surface(
    _: *mut DxgiResource,
    _: u32,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    log_warn!(
        "d3d11",
        "IDXGIResource1::CreateSubresourceSurface non prise en charge"
    );
    E_NOTIMPL
}
unsafe extern "win64" fn res_create_shared_handle(
    _: *mut DxgiResource,
    _: *const c_void,
    _: u32,
    _: *const u16,
    out: *mut usize,
) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    log_warn!(
        "d3d11",
        "IDXGIResource1::CreateSharedHandle (poignée NT) non prise en charge"
    );
    E_NOTIMPL
}

static RESOURCE_VTABLE: DxgiResourceVTable = DxgiResourceVTable {
    query_interface: res_qi,
    add_ref: res_add_ref,
    release: res_release,
    set_private_data: res_set_priv,
    set_private_data_interface: res_set_priv_iface,
    get_private_data: res_get_priv,
    get_parent: res_get_device,
    get_device: res_get_device,
    get_shared_handle: res_get_shared_handle,
    get_usage: res_get_usage,
    set_eviction_priority: res_set_eviction,
    get_eviction_priority: res_get_eviction,
    create_subresource_surface: res_create_subresource_surface,
    create_shared_handle: res_create_shared_handle,
};
