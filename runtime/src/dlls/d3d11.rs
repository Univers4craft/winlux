//! Implémentation D3D11 → OpenGL pour Winlux.
//! Traduit les interfaces ID3D11Device, ID3D11DeviceContext et les
//! types de ressources vers des appels OpenGL natifs.
#![allow(non_snake_case, non_upper_case_globals, dead_code)]

use crate::dlls::d3d_shader::{self, ShaderType};
use crate::dlls::dxgi::{self, DxgiSwapChainDesc};
use crate::dlls::gl_bindings as gl;
use crate::dlls::ole32::Guid;
use crate::{log_info, log_warn};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000eu32 as i32;

const IID_IUNKNOWN: Guid = Guid {
    data1: 0,
    data2: 0,
    data3: 0,
    data4: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};
const IID_ID3D11_DEVICE: Guid = Guid {
    data1: 0xdb6f6ddb,
    data2: 0xac77,
    data3: 0x4e88,
    data4: [0x82, 0x53, 0x81, 0x9d, 0xf9, 0xbb, 0xf1, 0x40],
};
const IID_ID3D11_DEVICE_CONTEXT: Guid = Guid {
    data1: 0xc0bfa96c,
    data2: 0xe089,
    data3: 0x44fb,
    data4: [0x8e, 0xaf, 0x26, 0xf8, 0x79, 0x61, 0x90, 0xda],
};
const IID_ID3D11_TEXTURE2D: Guid = Guid {
    data1: 0x6f15aaf2,
    data2: 0xd208,
    data3: 0x4e89,
    data4: [0x9a, 0xb4, 0x48, 0x95, 0x35, 0xd3, 0x4f, 0x9c],
};

// D3D11 Feature levels
const D3D_FEATURE_LEVEL_11_0: u32 = 0xb000;
const D3D_FEATURE_LEVEL_11_1: u32 = 0xb100;
const D3D_FEATURE_LEVEL_10_1: u32 = 0xa100;
const D3D_FEATURE_LEVEL_10_0: u32 = 0xa000;

// D3D11 resource formats
const DXGI_FORMAT_R8G8B8A8_UNORM: u32 = 28;
const DXGI_FORMAT_B8G8R8A8_UNORM: u32 = 87;
const DXGI_FORMAT_R32G32B32A32_FLOAT: u32 = 2;
const DXGI_FORMAT_R32G32B32_FLOAT: u32 = 6;
const DXGI_FORMAT_R32G32_FLOAT: u32 = 16;
const DXGI_FORMAT_R32_FLOAT: u32 = 41;
const DXGI_FORMAT_R16_UINT: u32 = 57;
const DXGI_FORMAT_R32_UINT: u32 = 42;
const DXGI_FORMAT_D24_UNORM_S8_UINT: u32 = 45;
const DXGI_FORMAT_D32_FLOAT: u32 = 40;

// D3D11 bind flags
const D3D11_BIND_VERTEX_BUFFER: u32 = 0x1;
const D3D11_BIND_INDEX_BUFFER: u32 = 0x2;
const D3D11_BIND_CONSTANT_BUFFER: u32 = 0x4;
const D3D11_BIND_SHADER_RESOURCE: u32 = 0x8;
const D3D11_BIND_RENDER_TARGET: u32 = 0x20;
const D3D11_BIND_DEPTH_STENCIL: u32 = 0x40;

// D3D11 usage
const D3D11_USAGE_DEFAULT: u32 = 0;
const D3D11_USAGE_IMMUTABLE: u32 = 1;
const D3D11_USAGE_DYNAMIC: u32 = 2;
const D3D11_USAGE_STAGING: u32 = 3;

// D3D11 Map types
const D3D11_MAP_READ: u32 = 1;
const D3D11_MAP_WRITE: u32 = 2;
const D3D11_MAP_READ_WRITE: u32 = 3;
const D3D11_MAP_WRITE_DISCARD: u32 = 4;
const D3D11_MAP_WRITE_NO_OVERWRITE: u32 = 5;

// D3D11 primitive topology
const D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST: u32 = 4;
const D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP: u32 = 5;
const D3D11_PRIMITIVE_TOPOLOGY_LINELIST: u32 = 2;
const D3D11_PRIMITIVE_TOPOLOGY_LINESTRIP: u32 = 3;
const D3D11_PRIMITIVE_TOPOLOGY_POINTLIST: u32 = 1;

// ────────────────────── D3D11 descriptors ──────────────────────

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct D3D11Texture2DDesc {
    pub width: u32,
    pub height: u32,
    pub mip_levels: u32,
    pub array_size: u32,
    pub format: u32,
    pub sample_desc_count: u32,
    pub sample_desc_quality: u32,
    pub usage: u32,
    pub bind_flags: u32,
    pub cpu_access_flags: u32,
    pub misc_flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct D3D11BufferDesc {
    pub byte_width: u32,
    pub usage: u32,
    pub bind_flags: u32,
    pub cpu_access_flags: u32,
    pub misc_flags: u32,
    pub structure_byte_stride: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct D3D11SubresourceData {
    pub sys_mem: *const c_void,
    pub sys_mem_pitch: u32,
    pub sys_mem_slice_pitch: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct D3D11RenderTargetViewDesc {
    pub format: u32,
    pub view_dimension: u32,
    pub union_data: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct D3D11DepthStencilViewDesc {
    pub format: u32,
    pub view_dimension: u32,
    pub flags: u32,
    pub union_data: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct D3D11ShaderResourceViewDesc {
    pub format: u32,
    pub view_dimension: u32,
    pub union_data: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct D3D11SamplerDesc {
    pub filter: u32,
    pub address_u: u32,
    pub address_v: u32,
    pub address_w: u32,
    pub mip_lod_bias: f32,
    pub max_anisotropy: u32,
    pub comparison_func: u32,
    pub border_color: [f32; 4],
    pub min_lod: f32,
    pub max_lod: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct D3D11InputElementDesc {
    pub semantic_name: *const u8,
    pub semantic_index: u32,
    pub format: u32,
    pub input_slot: u32,
    pub aligned_byte_offset: u32,
    pub input_slot_class: u32,
    pub instance_data_step_rate: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct D3D11MappedSubresource {
    pub data: *mut c_void,
    pub row_pitch: u32,
    pub depth_pitch: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct D3D11Viewport {
    pub top_left_x: f32,
    pub top_left_y: f32,
    pub width: f32,
    pub height: f32,
    pub min_depth: f32,
    pub max_depth: f32,
}

// ────────────────────── Resource objects ──────────────────────

#[repr(C)]
struct Texture2DVTable {
    query_interface: unsafe extern "win64" fn(*mut Texture2D, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut Texture2D) -> u32,
    release: unsafe extern "win64" fn(*mut Texture2D) -> u32,
    get_device: unsafe extern "win64" fn(*mut Texture2D, *mut *mut c_void),
    get_private_data:
        unsafe extern "win64" fn(*mut Texture2D, *const Guid, *mut u32, *mut c_void) -> i32,
    set_private_data:
        unsafe extern "win64" fn(*mut Texture2D, *const Guid, u32, *const c_void) -> i32,
    set_private_data_interface:
        unsafe extern "win64" fn(*mut Texture2D, *const Guid, *const c_void) -> i32,
    get_type: unsafe extern "win64" fn(*mut Texture2D, *mut u32),
    set_eviction_priority: unsafe extern "win64" fn(*mut Texture2D, u32),
    get_eviction_priority: unsafe extern "win64" fn(*mut Texture2D) -> u32,
    get_desc: unsafe extern "win64" fn(*mut Texture2D, *mut D3D11Texture2DDesc),
}

#[repr(C)]
pub(crate) struct Texture2D {
    vtable: *const Texture2DVTable,
    ref_count: AtomicU32,
    pub desc: D3D11Texture2DDesc,
    pub gl_texture: gl::GLuint,
    owns_texture: bool,
    staging_buf: Mutex<Vec<u8>>,
}

unsafe impl Send for Texture2D {}
unsafe impl Sync for Texture2D {}

impl Texture2D {
    unsafe extern "win64" fn query_interface(
        this: *mut Self,
        riid: *const Guid,
        out: *mut *mut c_void,
    ) -> i32 {
        if out.is_null() {
            return E_INVALIDARG;
        }
        let riid = &*riid;
        if *riid == IID_IUNKNOWN || *riid == IID_ID3D11_TEXTURE2D {
            (*this).ref_count.fetch_add(1, Ordering::Relaxed);
            *out = this as *mut c_void;
            return S_OK;
        }
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
    unsafe extern "win64" fn add_ref(this: *mut Self) -> u32 {
        (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
    }
    unsafe extern "win64" fn release(this: *mut Self) -> u32 {
        let prev = (*this).ref_count.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            let t = Box::from_raw(this);
            if t.owns_texture && t.gl_texture != 0 {
                if let Some(g) = gl::gl() {
                    (g.DeleteTextures)(1, &t.gl_texture);
                }
            }
            return 0;
        }
        prev - 1
    }
    unsafe extern "win64" fn get_device(_this: *mut Self, _out: *mut *mut c_void) {}
    unsafe extern "win64" fn get_priv(
        _: *mut Self,
        _: *const Guid,
        _: *mut u32,
        _: *mut c_void,
    ) -> i32 {
        E_NOTIMPL
    }
    unsafe extern "win64" fn set_priv(
        _: *mut Self,
        _: *const Guid,
        _: u32,
        _: *const c_void,
    ) -> i32 {
        S_OK
    }
    unsafe extern "win64" fn set_priv_iface(_: *mut Self, _: *const Guid, _: *const c_void) -> i32 {
        S_OK
    }
    unsafe extern "win64" fn get_type(_: *mut Self, out: *mut u32) {
        if !out.is_null() {
            *out = 3;
        }
    } // D3D11_RESOURCE_DIMENSION_TEXTURE2D
    unsafe extern "win64" fn set_eviction(_: *mut Self, _: u32) {}
    unsafe extern "win64" fn get_eviction(_: *mut Self) -> u32 {
        0
    }
    unsafe extern "win64" fn get_desc(this: *mut Self, out: *mut D3D11Texture2DDesc) {
        if !out.is_null() {
            std::ptr::write(out, (*this).desc);
        }
    }
}

static TEXTURE2D_VTABLE: Texture2DVTable = Texture2DVTable {
    query_interface: Texture2D::query_interface,
    add_ref: Texture2D::add_ref,
    release: Texture2D::release,
    get_device: Texture2D::get_device,
    get_private_data: Texture2D::get_priv,
    set_private_data: Texture2D::set_priv,
    set_private_data_interface: Texture2D::set_priv_iface,
    get_type: Texture2D::get_type,
    set_eviction_priority: Texture2D::set_eviction,
    get_eviction_priority: Texture2D::get_eviction,
    get_desc: Texture2D::get_desc,
};

pub(crate) fn create_backbuffer_texture(gl_tex: gl::GLuint, w: u32, h: u32) -> *mut Texture2D {
    Box::into_raw(Box::new(Texture2D {
        vtable: &TEXTURE2D_VTABLE,
        ref_count: AtomicU32::new(1),
        desc: D3D11Texture2DDesc {
            width: w,
            height: h,
            mip_levels: 1,
            array_size: 1,
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            sample_desc_count: 1,
            sample_desc_quality: 0,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_RENDER_TARGET,
            ..Default::default()
        },
        gl_texture: gl_tex,
        owns_texture: false,
        staging_buf: Mutex::new(Vec::new()),
    }))
}

fn create_texture2d(
    desc: &D3D11Texture2DDesc,
    initial: *const D3D11SubresourceData,
) -> *mut Texture2D {
    let g = match gl::gl() {
        Some(g) => g,
        None => return std::ptr::null_mut(),
    };

    let (gl_internal, gl_format, gl_type) = dxgi_format_to_gl(desc.format);

    let mut tex = 0u32;
    unsafe {
        (g.GenTextures)(1, &mut tex);
        (g.BindTexture)(gl::GL_TEXTURE_2D, tex);

        let data = if !initial.is_null() {
            (*initial).sys_mem
        } else {
            std::ptr::null()
        };
        (g.TexImage2D)(
            gl::GL_TEXTURE_2D,
            0,
            gl_internal as gl::GLint,
            desc.width as i32,
            desc.height as i32,
            0,
            gl_format,
            gl_type,
            data,
        );
        (g.TexParameteri)(gl::GL_TEXTURE_2D, gl::GL_TEXTURE_MIN_FILTER, gl::GL_LINEAR);
        (g.TexParameteri)(gl::GL_TEXTURE_2D, gl::GL_TEXTURE_MAG_FILTER, gl::GL_LINEAR);
        (g.TexParameteri)(
            gl::GL_TEXTURE_2D,
            gl::GL_TEXTURE_WRAP_S,
            gl::GL_CLAMP_TO_EDGE,
        );
        (g.TexParameteri)(
            gl::GL_TEXTURE_2D,
            gl::GL_TEXTURE_WRAP_T,
            gl::GL_CLAMP_TO_EDGE,
        );
        (g.BindTexture)(gl::GL_TEXTURE_2D, 0);
    }

    Box::into_raw(Box::new(Texture2D {
        vtable: &TEXTURE2D_VTABLE,
        ref_count: AtomicU32::new(1),
        desc: *desc,
        gl_texture: tex,
        owns_texture: true,
        staging_buf: Mutex::new(Vec::new()),
    }))
}

fn dxgi_format_to_gl(format: u32) -> (gl::GLenum, gl::GLenum, gl::GLenum) {
    match format {
        DXGI_FORMAT_R8G8B8A8_UNORM => (gl::GL_RGBA8, gl::GL_RGBA, gl::GL_UNSIGNED_BYTE),
        DXGI_FORMAT_B8G8R8A8_UNORM => (gl::GL_RGBA8, gl::GL_BGRA, gl::GL_UNSIGNED_BYTE),
        DXGI_FORMAT_R32G32B32A32_FLOAT => (gl::GL_RGBA32F, gl::GL_RGBA, gl::GL_FLOAT),
        DXGI_FORMAT_R32G32B32_FLOAT => (gl::GL_RGB32F, gl::GL_RGB, gl::GL_FLOAT),
        DXGI_FORMAT_R32G32_FLOAT => (gl::GL_RG32F, gl::GL_RG, gl::GL_FLOAT),
        DXGI_FORMAT_R32_FLOAT => (gl::GL_R32F, gl::GL_RED, gl::GL_FLOAT),
        DXGI_FORMAT_D24_UNORM_S8_UINT => (
            gl::GL_DEPTH24_STENCIL8,
            gl::GL_DEPTH_STENCIL,
            gl::GL_UNSIGNED_INT_24_8,
        ),
        DXGI_FORMAT_D32_FLOAT => (
            gl::GL_DEPTH_COMPONENT32F,
            gl::GL_DEPTH_COMPONENT,
            gl::GL_FLOAT,
        ),
        _ => (gl::GL_RGBA8, gl::GL_RGBA, gl::GL_UNSIGNED_BYTE),
    }
}

fn dxgi_format_element_size(format: u32) -> u32 {
    match format {
        DXGI_FORMAT_R32G32B32A32_FLOAT => 16,
        DXGI_FORMAT_R32G32B32_FLOAT => 12,
        DXGI_FORMAT_R32G32_FLOAT => 8,
        DXGI_FORMAT_R32_FLOAT | DXGI_FORMAT_R32_UINT => 4,
        DXGI_FORMAT_R16_UINT => 2,
        DXGI_FORMAT_R8G8B8A8_UNORM | DXGI_FORMAT_B8G8R8A8_UNORM => 4,
        _ => 4,
    }
}

fn dxgi_format_to_gl_type(format: u32) -> (gl::GLint, gl::GLenum, u8) {
    match format {
        DXGI_FORMAT_R32G32B32A32_FLOAT => (4, gl::GL_FLOAT, 0),
        DXGI_FORMAT_R32G32B32_FLOAT => (3, gl::GL_FLOAT, 0),
        DXGI_FORMAT_R32G32_FLOAT => (2, gl::GL_FLOAT, 0),
        DXGI_FORMAT_R32_FLOAT => (1, gl::GL_FLOAT, 0),
        DXGI_FORMAT_R8G8B8A8_UNORM => (4, gl::GL_UNSIGNED_BYTE, 1),
        _ => (4, gl::GL_FLOAT, 0),
    }
}

// ────────────────────── Buffer ──────────────────────

#[repr(C)]
struct BufferVTable {
    query_interface: unsafe extern "win64" fn(*mut Buffer, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut Buffer) -> u32,
    release: unsafe extern "win64" fn(*mut Buffer) -> u32,
    get_device: unsafe extern "win64" fn(*mut Buffer, *mut *mut c_void),
    get_private_data:
        unsafe extern "win64" fn(*mut Buffer, *const Guid, *mut u32, *mut c_void) -> i32,
    set_private_data: unsafe extern "win64" fn(*mut Buffer, *const Guid, u32, *const c_void) -> i32,
    set_private_data_interface:
        unsafe extern "win64" fn(*mut Buffer, *const Guid, *const c_void) -> i32,
    get_type: unsafe extern "win64" fn(*mut Buffer, *mut u32),
    set_eviction_priority: unsafe extern "win64" fn(*mut Buffer, u32),
    get_eviction_priority: unsafe extern "win64" fn(*mut Buffer) -> u32,
    get_desc: unsafe extern "win64" fn(*mut Buffer, *mut D3D11BufferDesc),
}

#[repr(C)]
struct Buffer {
    vtable: *const BufferVTable,
    ref_count: AtomicU32,
    desc: D3D11BufferDesc,
    gl_buffer: gl::GLuint,
    staging_data: Mutex<Vec<u8>>,
}

unsafe impl Send for Buffer {}
unsafe impl Sync for Buffer {}

impl Buffer {
    unsafe extern "win64" fn query_interface(
        this: *mut Self,
        riid: *const Guid,
        out: *mut *mut c_void,
    ) -> i32 {
        if out.is_null() {
            return E_INVALIDARG;
        }
        let riid = &*riid;
        if *riid == IID_IUNKNOWN {
            (*this).ref_count.fetch_add(1, Ordering::Relaxed);
            *out = this as *mut c_void;
            return S_OK;
        }
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
    unsafe extern "win64" fn add_ref(this: *mut Self) -> u32 {
        (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
    }
    unsafe extern "win64" fn release(this: *mut Self) -> u32 {
        let prev = (*this).ref_count.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            let b = Box::from_raw(this);
            if b.gl_buffer != 0 {
                if let Some(g) = gl::gl() {
                    (g.DeleteBuffers)(1, &b.gl_buffer);
                }
            }
            return 0;
        }
        prev - 1
    }
    unsafe extern "win64" fn get_device(_: *mut Self, _: *mut *mut c_void) {}
    unsafe extern "win64" fn get_priv(
        _: *mut Self,
        _: *const Guid,
        _: *mut u32,
        _: *mut c_void,
    ) -> i32 {
        E_NOTIMPL
    }
    unsafe extern "win64" fn set_priv(
        _: *mut Self,
        _: *const Guid,
        _: u32,
        _: *const c_void,
    ) -> i32 {
        S_OK
    }
    unsafe extern "win64" fn set_priv_iface(_: *mut Self, _: *const Guid, _: *const c_void) -> i32 {
        S_OK
    }
    unsafe extern "win64" fn get_type(_: *mut Self, out: *mut u32) {
        if !out.is_null() {
            *out = 1;
        }
    } // D3D11_RESOURCE_DIMENSION_BUFFER
    unsafe extern "win64" fn set_eviction(_: *mut Self, _: u32) {}
    unsafe extern "win64" fn get_eviction(_: *mut Self) -> u32 {
        0
    }
    unsafe extern "win64" fn get_desc(this: *mut Self, out: *mut D3D11BufferDesc) {
        if !out.is_null() {
            std::ptr::write(out, (*this).desc);
        }
    }
}

static BUFFER_VTABLE: BufferVTable = BufferVTable {
    query_interface: Buffer::query_interface,
    add_ref: Buffer::add_ref,
    release: Buffer::release,
    get_device: Buffer::get_device,
    get_private_data: Buffer::get_priv,
    set_private_data: Buffer::set_priv,
    set_private_data_interface: Buffer::set_priv_iface,
    get_type: Buffer::get_type,
    set_eviction_priority: Buffer::set_eviction,
    get_eviction_priority: Buffer::get_eviction,
    get_desc: Buffer::get_desc,
};

fn bind_flags_to_gl_target(flags: u32) -> gl::GLenum {
    if flags & D3D11_BIND_VERTEX_BUFFER != 0 {
        gl::GL_ARRAY_BUFFER
    } else if flags & D3D11_BIND_INDEX_BUFFER != 0 {
        gl::GL_ELEMENT_ARRAY_BUFFER
    } else if flags & D3D11_BIND_CONSTANT_BUFFER != 0 {
        gl::GL_UNIFORM_BUFFER
    } else {
        gl::GL_ARRAY_BUFFER
    }
}

fn usage_to_gl_usage(usage: u32) -> gl::GLenum {
    match usage {
        D3D11_USAGE_IMMUTABLE => gl::GL_STATIC_DRAW,
        D3D11_USAGE_DYNAMIC => gl::GL_DYNAMIC_DRAW,
        D3D11_USAGE_STAGING => gl::GL_STREAM_READ,
        _ => gl::GL_STATIC_DRAW,
    }
}

fn create_buffer(desc: &D3D11BufferDesc, initial: *const D3D11SubresourceData) -> *mut Buffer {
    let g = match gl::gl() {
        Some(g) => g,
        None => return std::ptr::null_mut(),
    };

    let target = bind_flags_to_gl_target(desc.bind_flags);
    let usage = usage_to_gl_usage(desc.usage);

    let mut buf = 0u32;
    unsafe {
        (g.GenBuffers)(1, &mut buf);
        (g.BindBuffer)(target, buf);
        let data = if !initial.is_null() {
            (*initial).sys_mem
        } else {
            std::ptr::null()
        };
        (g.BufferData)(target, desc.byte_width as isize, data, usage);
        (g.BindBuffer)(target, 0);
    }

    Box::into_raw(Box::new(Buffer {
        vtable: &BUFFER_VTABLE,
        ref_count: AtomicU32::new(1),
        desc: *desc,
        gl_buffer: buf,
        staging_data: Mutex::new(vec![0u8; desc.byte_width as usize]),
    }))
}

// ────────────────────── Views (RenderTargetView, DepthStencilView, ShaderResourceView) ──────

struct SimpleComVTable {
    query_interface:
        unsafe extern "win64" fn(*mut SimpleComObj, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut SimpleComObj) -> u32,
    release: unsafe extern "win64" fn(*mut SimpleComObj) -> u32,
}

struct SimpleComObj {
    vtable: *const SimpleComVTable,
    ref_count: AtomicU32,
    gl_id: gl::GLuint,
    resource_tex: gl::GLuint,
}

impl SimpleComObj {
    unsafe extern "win64" fn query_interface(
        this: *mut Self,
        _riid: *const Guid,
        out: *mut *mut c_void,
    ) -> i32 {
        if out.is_null() {
            return E_INVALIDARG;
        }
        (*this).ref_count.fetch_add(1, Ordering::Relaxed);
        *out = this as *mut c_void;
        S_OK
    }
    unsafe extern "win64" fn add_ref(this: *mut Self) -> u32 {
        (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
    }
    unsafe extern "win64" fn release(this: *mut Self) -> u32 {
        let prev = (*this).ref_count.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            let obj = Box::from_raw(this);
            if obj.gl_id != 0 {
                if let Some(g) = gl::gl() {
                    (g.DeleteFramebuffers)(1, &obj.gl_id);
                }
            }
            return 0;
        }
        prev - 1
    }
}

static SIMPLE_COM_VTABLE: SimpleComVTable = SimpleComVTable {
    query_interface: SimpleComObj::query_interface,
    add_ref: SimpleComObj::add_ref,
    release: SimpleComObj::release,
};

// ────────────────────── Typage des objets COM maison ──────────────────────
//
// Les méthodes du contexte reçoivent des `ID3D11Resource*` / vues sans type :
// on reconnaît nos objets à leur vtable (premier mot) avant tout cast, au
// lieu de supposer qu'un pointeur quelconque est un `Buffer`.

unsafe fn vtable_ptr(p: *const c_void) -> *const c_void {
    if p.is_null() {
        return std::ptr::null();
    }
    *(p as *const *const c_void)
}

unsafe fn as_buffer(p: *mut c_void) -> Option<*mut Buffer> {
    (vtable_ptr(p) == &BUFFER_VTABLE as *const BufferVTable as *const c_void)
        .then_some(p as *mut Buffer)
}

unsafe fn as_texture2d(p: *mut c_void) -> Option<*mut Texture2D> {
    (vtable_ptr(p) == &TEXTURE2D_VTABLE as *const Texture2DVTable as *const c_void)
        .then_some(p as *mut Texture2D)
}

unsafe fn as_simple(p: *mut c_void) -> Option<*mut SimpleComObj> {
    (vtable_ptr(p) == &SIMPLE_COM_VTABLE as *const SimpleComVTable as *const c_void)
        .then_some(p as *mut SimpleComObj)
}

/// Taille (octets) du niveau 0 d'une texture 2D, sans débordement.
fn texture_bytes(desc: &D3D11Texture2DDesc) -> Option<usize> {
    let elem = dxgi_format_element_size(desc.format) as usize;
    (desc.width as usize)
        .checked_mul(desc.height as usize)?
        .checked_mul(elem)
        .filter(|&n| n <= 1 << 30)
}

fn create_render_target_view(resource: *mut c_void) -> *mut SimpleComObj {
    let g = match gl::gl() {
        Some(g) => g,
        None => return std::ptr::null_mut(),
    };

    let tex_gl = match unsafe { as_texture2d(resource) } {
        Some(t) => unsafe { (*t).gl_texture },
        None => 0,
    };

    let mut fbo = 0u32;
    unsafe {
        (g.GenFramebuffers)(1, &mut fbo);
        (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, fbo);
        if tex_gl != 0 {
            (g.FramebufferTexture2D)(
                gl::GL_FRAMEBUFFER,
                gl::GL_COLOR_ATTACHMENT0,
                gl::GL_TEXTURE_2D,
                tex_gl,
                0,
            );
        }
        (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, 0);
    }

    Box::into_raw(Box::new(SimpleComObj {
        vtable: &SIMPLE_COM_VTABLE,
        ref_count: AtomicU32::new(1),
        gl_id: fbo,
        resource_tex: tex_gl,
    }))
}

// ────────────────────── Shader objects ──────────────────────

struct ShaderObj {
    vtable: *const SimpleComVTable,
    ref_count: AtomicU32,
    gl_id: gl::GLuint,
    resource_tex: gl::GLuint,
}

fn create_shader_object(
    bytecode: *const c_void,
    size: usize,
    shader_type: ShaderType,
) -> *mut SimpleComObj {
    let g = match gl::gl() {
        Some(g) => g,
        None => return std::ptr::null_mut(),
    };

    let gl_type = match shader_type {
        ShaderType::Vertex => gl::GL_VERTEX_SHADER,
        ShaderType::Pixel => gl::GL_FRAGMENT_SHADER,
        ShaderType::Geometry => gl::GL_GEOMETRY_SHADER,
        _ => gl::GL_VERTEX_SHADER,
    };

    let bytes = if !bytecode.is_null() && size > 0 {
        unsafe { std::slice::from_raw_parts(bytecode as *const u8, size) }
    } else {
        &[]
    };

    let translated = d3d_shader::translate_dxbc(bytes);
    let glsl = match &translated {
        Some(t) => &t.glsl_source,
        None => match shader_type {
            ShaderType::Vertex => d3d_shader::passthrough_vertex_shader(),
            _ => d3d_shader::passthrough_pixel_shader(),
        },
    };

    let shader = unsafe { (g.CreateShader)(gl_type) };
    if shader == 0 {
        return std::ptr::null_mut();
    }

    unsafe {
        let src = glsl.as_ptr() as *const gl::GLchar;
        let len = glsl.len() as gl::GLint;
        (g.ShaderSource)(shader, 1, &src, &len);
        (g.CompileShader)(shader);

        let mut ok = 0i32;
        (g.GetShaderiv)(shader, gl::GL_COMPILE_STATUS, &mut ok);
        if ok == 0 {
            let mut log_len = 0i32;
            (g.GetShaderiv)(shader, gl::GL_INFO_LOG_LENGTH, &mut log_len);
            if log_len > 0 {
                let mut buf = vec![0u8; log_len as usize];
                (g.GetShaderInfoLog)(
                    shader,
                    log_len,
                    std::ptr::null_mut(),
                    buf.as_mut_ptr() as *mut gl::GLchar,
                );
                let msg = String::from_utf8_lossy(&buf);
                log_warn!("d3d11", "erreur de compilation shader : {}", msg);
            }
        }
    }

    Box::into_raw(Box::new(SimpleComObj {
        vtable: &SIMPLE_COM_VTABLE,
        ref_count: AtomicU32::new(1),
        gl_id: shader,
        resource_tex: 0,
    }))
}

// ────────────────────── InputLayout ──────────────────────

struct InputLayout {
    vtable: *const SimpleComVTable,
    ref_count: AtomicU32,
    gl_id: gl::GLuint,
    resource_tex: gl::GLuint,
}

#[derive(Clone)]
struct InputElement {
    slot: u32,
    offset: u32,
    components: gl::GLint,
    gl_type: gl::GLenum,
    normalized: u8,
    stride: u32,
}

struct InputLayoutData {
    vtable: *const SimpleComVTable,
    ref_count: AtomicU32,
    gl_id: gl::GLuint,
    resource_tex: gl::GLuint,
    elements: Vec<InputElement>,
}

fn create_input_layout(descs: *const D3D11InputElementDesc, count: u32) -> *mut SimpleComObj {
    let mut elements = Vec::new();
    for i in 0..count {
        let d = unsafe { &*descs.add(i as usize) };
        let (components, gl_type, norm) = dxgi_format_to_gl_type(d.format);
        elements.push(InputElement {
            slot: d.input_slot,
            offset: d.aligned_byte_offset,
            components,
            gl_type,
            normalized: norm,
            stride: 0,
        });
    }

    // We store element info but use a SimpleComObj wrapper
    // Elements are stored in a leaked vec for simplicity
    let elems = Box::leak(Box::new(elements));
    LAYOUT_STORE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(elems as *const Vec<InputElement> as usize);

    Box::into_raw(Box::new(SimpleComObj {
        vtable: &SIMPLE_COM_VTABLE,
        ref_count: AtomicU32::new(1),
        gl_id: elems as *const Vec<InputElement> as u32,
        resource_tex: 0,
    }))
}

static LAYOUT_STORE: Mutex<Vec<usize>> = Mutex::new(Vec::new());

// ────────────────────── ID3D11DeviceContext ──────────────────────

struct DeviceContextState {
    current_rtv: *mut SimpleComObj,
    current_dsv: *mut SimpleComObj,
    current_vs: gl::GLuint,
    current_ps: gl::GLuint,
    current_program: gl::GLuint,
    current_vao: gl::GLuint,
    topology: u32,
    /// Format du tampon d'index courant (DXGI_FORMAT_R16_UINT ou R32_UINT).
    index_format: u32,
    viewports: [D3D11Viewport; 16],
    num_viewports: u32,
}

impl Default for DeviceContextState {
    fn default() -> Self {
        Self {
            current_rtv: std::ptr::null_mut(),
            current_dsv: std::ptr::null_mut(),
            current_vs: 0,
            current_ps: 0,
            current_program: 0,
            current_vao: 0,
            topology: D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            index_format: DXGI_FORMAT_R16_UINT,
            viewports: [D3D11Viewport::default(); 16],
            num_viewports: 0,
        }
    }
}

unsafe impl Send for DeviceContextState {}

#[repr(C)]
struct DeviceContextVTable {
    // IUnknown (0-2)
    query_interface:
        unsafe extern "win64" fn(*mut DeviceContext, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut DeviceContext) -> u32,
    release: unsafe extern "win64" fn(*mut DeviceContext) -> u32,
    // ID3D11DeviceChild (3-6)
    get_device: unsafe extern "win64" fn(*mut DeviceContext, *mut *mut c_void),
    get_private_data:
        unsafe extern "win64" fn(*mut DeviceContext, *const Guid, *mut u32, *mut c_void) -> i32,
    set_private_data:
        unsafe extern "win64" fn(*mut DeviceContext, *const Guid, u32, *const c_void) -> i32,
    set_private_data_interface:
        unsafe extern "win64" fn(*mut DeviceContext, *const Guid, *const c_void) -> i32,
    // ID3D11DeviceContext (7+) — subset of the most commonly called methods
    vs_set_constant_buffers:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 7
    ps_set_shader_resources:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 8
    ps_set_shader:
        unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, *const *mut c_void, u32), // 9
    ps_set_samplers: unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 10
    vs_set_shader:
        unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, *const *mut c_void, u32), // 11
    draw_indexed: unsafe extern "win64" fn(*mut DeviceContext, u32, u32, i32), // 12
    draw: unsafe extern "win64" fn(*mut DeviceContext, u32, u32),              // 13
    map: unsafe extern "win64" fn(
        *mut DeviceContext,
        *mut c_void,
        u32,
        u32,
        u32,
        *mut D3D11MappedSubresource,
    ) -> i32, // 14
    unmap: unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, u32),     // 15
    ps_set_constant_buffers:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 16
    ia_set_input_layout: unsafe extern "win64" fn(*mut DeviceContext, *mut c_void), // 17
    ia_set_vertex_buffers: unsafe extern "win64" fn(
        *mut DeviceContext,
        u32,
        u32,
        *const *mut c_void,
        *const u32,
        *const u32,
    ), // 18
    ia_set_index_buffer: unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, u32, u32), // 19
    draw_indexed_instanced: unsafe extern "win64" fn(*mut DeviceContext, u32, u32, u32, i32, u32), // 20
    draw_instanced: unsafe extern "win64" fn(*mut DeviceContext, u32, u32, u32, u32), // 21
    gs_set_constant_buffers:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 22
    gs_set_shader:
        unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, *const *mut c_void, u32), // 23
    ia_set_primitive_topology: unsafe extern "win64" fn(*mut DeviceContext, u32),     // 24
    vs_set_shader_resources:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 25
    vs_set_samplers: unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 26
    begin: unsafe extern "win64" fn(*mut DeviceContext, *mut c_void), // 27
    end: unsafe extern "win64" fn(*mut DeviceContext, *mut c_void),   // 28
    get_data:
        unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, *mut c_void, u32, u32) -> i32, // 29
    set_predication: unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, i32), // 30
    gs_set_shader_resources:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 31
    gs_set_samplers: unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 32
    om_set_render_targets:
        unsafe extern "win64" fn(*mut DeviceContext, u32, *const *mut c_void, *mut c_void), // 33
    om_set_render_targets_and_unordered: unsafe extern "win64" fn(
        *mut DeviceContext,
        u32,
        *const *mut c_void,
        *mut c_void,
        u32,
        u32,
        *const *mut c_void,
        *const u32,
    ), // 34
    om_set_blend_state: unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, *const f32, u32), // 35
    om_set_depth_stencil_state: unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, u32), // 36
    so_set_targets:
        unsafe extern "win64" fn(*mut DeviceContext, u32, *const *mut c_void, *const u32), // 37
    draw_auto: unsafe extern "win64" fn(*mut DeviceContext), // 38
    draw_indexed_instanced_indirect: unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, u32), // 39
    draw_instanced_indirect: unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, u32), // 40
    dispatch: unsafe extern "win64" fn(*mut DeviceContext, u32, u32, u32),                   // 41
    dispatch_indirect: unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, u32),       // 42
    rs_set_state: unsafe extern "win64" fn(*mut DeviceContext, *mut c_void),                 // 43
    rs_set_viewports: unsafe extern "win64" fn(*mut DeviceContext, u32, *const D3D11Viewport), // 44
    rs_set_scissor_rects: unsafe extern "win64" fn(*mut DeviceContext, u32, *const c_void),  // 45
    copy_subresource_region: unsafe extern "win64" fn(
        *mut DeviceContext,
        *mut c_void,
        u32,
        u32,
        u32,
        u32,
        *mut c_void,
        u32,
        *const c_void,
    ), // 46
    copy_resource: unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, *mut c_void),   // 47
    update_subresource: unsafe extern "win64" fn(
        *mut DeviceContext,
        *mut c_void,
        u32,
        *const c_void,
        *const c_void,
        u32,
        u32,
    ), // 48
    copy_structure_count:
        unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, u32, *mut c_void), // 49
    clear_render_target_view:
        unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, *const [f32; 4]), // 50
    clear_unordered_access_view_uint:
        unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, *const [u32; 4]), // 51
    clear_unordered_access_view_float:
        unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, *const [f32; 4]), // 52
    clear_depth_stencil_view:
        unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, u32, f32, u8), // 53
    generate_mips: unsafe extern "win64" fn(*mut DeviceContext, *mut c_void),                // 54
    set_resource_min_lod: unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, f32),    // 55
    get_resource_min_lod: unsafe extern "win64" fn(*mut DeviceContext, *mut c_void) -> f32,  // 56
    resolve_subresource:
        unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, u32, *mut c_void, u32, u32), // 57
    execute_command_list: unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, i32),    // 58
    hs_set_shader_resources:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 59
    hs_set_shader:
        unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, *const *mut c_void, u32), // 60
    hs_set_samplers: unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 61
    hs_set_constant_buffers:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 62
    ds_set_shader_resources:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 63
    ds_set_shader:
        unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, *const *mut c_void, u32), // 64
    ds_set_samplers: unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 65
    ds_set_constant_buffers:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 66
    cs_set_shader_resources:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 67
    cs_set_unordered_access_views:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void, *const u32), // 68
    cs_set_shader:
        unsafe extern "win64" fn(*mut DeviceContext, *mut c_void, *const *mut c_void, u32), // 69
    cs_set_samplers: unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 70
    cs_set_constant_buffers:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *const *mut c_void), // 71
    clear_state: unsafe extern "win64" fn(*mut DeviceContext), // 72
    flush: unsafe extern "win64" fn(*mut DeviceContext),       // 73
    get_type_ctx: unsafe extern "win64" fn(*mut DeviceContext) -> u32, // 74
    get_context_flags: unsafe extern "win64" fn(*mut DeviceContext) -> u32, // 75
    finish_command_list: unsafe extern "win64" fn(*mut DeviceContext, i32, *mut *mut c_void) -> i32, // 76
}

#[repr(C)]
pub(crate) struct DeviceContext {
    vtable: *const DeviceContextVTable,
    ref_count: AtomicU32,
    state: Mutex<DeviceContextState>,
}

unsafe impl Send for DeviceContext {}
unsafe impl Sync for DeviceContext {}

// Stub implementations for DeviceContext methods
unsafe extern "win64" fn ctx_qi(
    this: *mut DeviceContext,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let riid = &*riid;
    if *riid == IID_IUNKNOWN || *riid == IID_ID3D11_DEVICE_CONTEXT {
        (*this).ref_count.fetch_add(1, Ordering::Relaxed);
        *out = this as *mut c_void;
        return S_OK;
    }
    *out = std::ptr::null_mut();
    E_NOINTERFACE
}
unsafe extern "win64" fn ctx_addref(this: *mut DeviceContext) -> u32 {
    (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn ctx_release(this: *mut DeviceContext) -> u32 {
    let prev = (*this).ref_count.fetch_sub(1, Ordering::AcqRel);
    if prev == 1 {
        let _ = Box::from_raw(this);
        return 0;
    }
    prev - 1
}
unsafe extern "win64" fn ctx_get_device(_: *mut DeviceContext, _: *mut *mut c_void) {}
unsafe extern "win64" fn ctx_get_priv(
    _: *mut DeviceContext,
    _: *const Guid,
    _: *mut u32,
    _: *mut c_void,
) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn ctx_set_priv(
    _: *mut DeviceContext,
    _: *const Guid,
    _: u32,
    _: *const c_void,
) -> i32 {
    S_OK
}
unsafe extern "win64" fn ctx_set_priv_iface(
    _: *mut DeviceContext,
    _: *const Guid,
    _: *const c_void,
) -> i32 {
    S_OK
}

// Stubs for unimplemented methods
unsafe extern "win64" fn ctx_noop_3ptr(
    _: *mut DeviceContext,
    _: u32,
    _: u32,
    _: *const *mut c_void,
) {
}
unsafe extern "win64" fn ctx_noop_set_shader(
    _: *mut DeviceContext,
    _: *mut c_void,
    _: *const *mut c_void,
    _: u32,
) {
}
unsafe extern "win64" fn ctx_noop_1ptr(_: *mut DeviceContext, _: *mut c_void) {}
unsafe extern "win64" fn ctx_noop_void(_: *mut DeviceContext) {}
unsafe extern "win64" fn ctx_noop_2ptr(_: *mut DeviceContext, _: *mut c_void, _: *mut c_void) {}
unsafe extern "win64" fn ctx_noop_pred(_: *mut DeviceContext, _: *mut c_void, _: i32) {}
unsafe extern "win64" fn ctx_noop_so(
    _: *mut DeviceContext,
    _: u32,
    _: *const *mut c_void,
    _: *const u32,
) {
}
unsafe extern "win64" fn ctx_noop_blend(
    _: *mut DeviceContext,
    _: *mut c_void,
    _: *const f32,
    _: u32,
) {
}
unsafe extern "win64" fn ctx_noop_ds(_: *mut DeviceContext, _: *mut c_void, _: u32) {}
unsafe extern "win64" fn ctx_noop_rminlod(_: *mut DeviceContext, _: *mut c_void, _: f32) {}
unsafe extern "win64" fn ctx_noop_grminlod(_: *mut DeviceContext, _: *mut c_void) -> f32 {
    0.0
}
unsafe extern "win64" fn ctx_noop_resolve(
    _: *mut DeviceContext,
    _: *mut c_void,
    _: u32,
    _: *mut c_void,
    _: u32,
    _: u32,
) {
}
unsafe extern "win64" fn ctx_noop_exec(_: *mut DeviceContext, _: *mut c_void, _: i32) {}
unsafe extern "win64" fn ctx_noop_uav(
    _: *mut DeviceContext,
    _: u32,
    _: u32,
    _: *const *mut c_void,
    _: *const u32,
) {
}
unsafe extern "win64" fn ctx_get_data(
    _: *mut DeviceContext,
    _: *mut c_void,
    _: *mut c_void,
    _: u32,
    _: u32,
) -> i32 {
    S_OK
}
unsafe extern "win64" fn ctx_get_type(_: *mut DeviceContext) -> u32 {
    0
} // D3D11_DEVICE_CONTEXT_IMMEDIATE
unsafe extern "win64" fn ctx_get_flags(_: *mut DeviceContext) -> u32 {
    0
}
unsafe extern "win64" fn ctx_finish_cl(_: *mut DeviceContext, _: i32, _: *mut *mut c_void) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn ctx_noop_copy_struct(
    _: *mut DeviceContext,
    _: *mut c_void,
    _: u32,
    _: *mut c_void,
) {
}
unsafe extern "win64" fn ctx_noop_clear_uav_u(
    _: *mut DeviceContext,
    _: *mut c_void,
    _: *const [u32; 4],
) {
}
unsafe extern "win64" fn ctx_noop_clear_uav_f(
    _: *mut DeviceContext,
    _: *mut c_void,
    _: *const [f32; 4],
) {
}
unsafe extern "win64" fn ctx_noop_draw_indirect(_: *mut DeviceContext, _: *mut c_void, _: u32) {}

// Implemented methods
unsafe extern "win64" fn ctx_vs_set_shader(
    this: *mut DeviceContext,
    shader: *mut c_void,
    _class: *const *mut c_void,
    _n: u32,
) {
    let mut s = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    s.current_vs = match as_simple(shader) {
        Some(o) => (*o).gl_id,
        None => 0,
    };
    update_program(&mut s);
}

unsafe extern "win64" fn ctx_ps_set_shader(
    this: *mut DeviceContext,
    shader: *mut c_void,
    _class: *const *mut c_void,
    _n: u32,
) {
    let mut s = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    s.current_ps = match as_simple(shader) {
        Some(o) => (*o).gl_id,
        None => 0,
    };
    update_program(&mut s);
}

fn topology_to_gl(t: u32) -> gl::GLenum {
    match t {
        D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST => gl::GL_TRIANGLES,
        D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP => gl::GL_TRIANGLE_STRIP,
        D3D11_PRIMITIVE_TOPOLOGY_LINELIST => gl::GL_LINES,
        D3D11_PRIMITIVE_TOPOLOGY_LINESTRIP => gl::GL_LINE_STRIP,
        D3D11_PRIMITIVE_TOPOLOGY_POINTLIST => gl::GL_POINTS,
        _ => gl::GL_TRIANGLES,
    }
}

unsafe extern "win64" fn ctx_ia_set_topology(this: *mut DeviceContext, topology: u32) {
    (*this)
        .state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .topology = topology;
}

unsafe extern "win64" fn ctx_ia_set_input_layout(_this: *mut DeviceContext, _layout: *mut c_void) {
    // Layout is applied when vertex buffers are bound
}

unsafe extern "win64" fn ctx_ia_set_vertex_buffers(
    _this: *mut DeviceContext,
    _start: u32,
    count: u32,
    buffers: *const *mut c_void,
    strides: *const u32,
    offsets: *const u32,
) {
    let Some(g) = gl::gl() else { return };
    for i in 0..count {
        let buf_ptr = *buffers.add(i as usize);
        let Some(buf) = as_buffer(buf_ptr) else {
            continue;
        };
        let buf = &*buf;
        let _stride = *strides.add(i as usize);
        let _offset = *offsets.add(i as usize);
        (g.BindBuffer)(gl::GL_ARRAY_BUFFER, buf.gl_buffer);
    }
}

unsafe extern "win64" fn ctx_ia_set_index_buffer(
    this: *mut DeviceContext,
    buffer: *mut c_void,
    format: u32,
    _offset: u32,
) {
    let Some(g) = gl::gl() else { return };
    // Le format d'index (16 ou 32 bits) est mémorisé pour les tirages.
    let mut s = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    s.index_format = format;
    if let Some(buf) = as_buffer(buffer) {
        (g.BindBuffer)(gl::GL_ELEMENT_ARRAY_BUFFER, (*buf).gl_buffer);
    }
}

/// (type GL, taille) des indices selon le format mémorisé.
fn index_type(format: u32) -> (gl::GLenum, usize) {
    if format == DXGI_FORMAT_R32_UINT {
        (gl::GL_UNSIGNED_INT, 4)
    } else {
        (gl::GL_UNSIGNED_SHORT, 2)
    }
}

unsafe extern "win64" fn ctx_draw(this: *mut DeviceContext, vertex_count: u32, start: u32) {
    let Some(g) = gl::gl() else { return };
    let s = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    let mode = topology_to_gl(s.topology);
    (g.DrawArrays)(mode, start as i32, vertex_count as i32);
}

unsafe extern "win64" fn ctx_draw_indexed(
    this: *mut DeviceContext,
    index_count: u32,
    start_index: u32,
    _base_vertex: i32,
) {
    let Some(g) = gl::gl() else { return };
    let s = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    let mode = topology_to_gl(s.topology);
    let (ty, size) = index_type(s.index_format);
    let offset = (start_index as usize * size) as *const c_void;
    (g.DrawElements)(mode, index_count as i32, ty, offset);
}

unsafe extern "win64" fn ctx_draw_instanced(
    _this: *mut DeviceContext,
    vpc: u32,
    ic: u32,
    start: u32,
    _base: u32,
) {
    let Some(g) = gl::gl() else { return };
    (g.DrawArraysInstanced)(gl::GL_TRIANGLES, start as i32, vpc as i32, ic as i32);
}

unsafe extern "win64" fn ctx_draw_indexed_instanced(
    this: *mut DeviceContext,
    ipc: u32,
    ic: u32,
    start: u32,
    _base: i32,
    _ibase: u32,
) {
    let Some(g) = gl::gl() else { return };
    let s = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    let (ty, size) = index_type(s.index_format);
    let offset = (start as usize * size) as *const c_void;
    (g.DrawElementsInstanced)(gl::GL_TRIANGLES, ipc as i32, ty, offset, ic as i32);
}

unsafe extern "win64" fn ctx_map(
    _this: *mut DeviceContext,
    resource: *mut c_void,
    _sub: u32,
    _map_type: u32,
    _flags: u32,
    mapped: *mut D3D11MappedSubresource,
) -> i32 {
    if resource.is_null() || mapped.is_null() {
        return E_INVALIDARG;
    }
    if let Some(buf) = as_buffer(resource) {
        let mut guard = (*buf)
            .staging_data
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let ptr = guard.as_mut_ptr();
        (*mapped).data = ptr as *mut c_void;
        (*mapped).row_pitch = (*buf).desc.byte_width;
        (*mapped).depth_pitch = 0;
        return S_OK;
    }
    if let Some(tex) = as_texture2d(resource) {
        // Texture : tampon CPU dimensionné au niveau 0, renvoyé à l'appli
        // (téléversé au Unmap).
        let desc = (*tex).desc;
        let Some(bytes) = texture_bytes(&desc) else {
            return E_INVALIDARG;
        };
        let mut guard = (*tex).staging_buf.lock().unwrap_or_else(|e| e.into_inner());
        if guard.len() != bytes {
            guard.resize(bytes, 0);
        }
        (*mapped).data = guard.as_mut_ptr() as *mut c_void;
        (*mapped).row_pitch = desc.width * dxgi_format_element_size(desc.format);
        (*mapped).depth_pitch = bytes as u32;
        return S_OK;
    }
    E_INVALIDARG
}

unsafe extern "win64" fn ctx_unmap(_this: *mut DeviceContext, resource: *mut c_void, _sub: u32) {
    let Some(g) = gl::gl() else { return };
    if let Some(buf) = as_buffer(resource) {
        let guard = (*buf)
            .staging_data
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let target = bind_flags_to_gl_target((*buf).desc.bind_flags);
        (g.BindBuffer)(target, (*buf).gl_buffer);
        (g.BufferSubData)(
            target,
            0,
            guard.len() as isize,
            guard.as_ptr() as *const c_void,
        );
        (g.BindBuffer)(target, 0);
    } else if let Some(tex) = as_texture2d(resource) {
        let guard = (*tex).staging_buf.lock().unwrap_or_else(|e| e.into_inner());
        if (*tex).gl_texture != 0 && texture_bytes(&(*tex).desc) == Some(guard.len()) {
            upload_texture_level0(g, &*tex, guard.as_ptr() as *const c_void);
        }
    }
}

/// Téléverse le niveau 0 d'une texture 2D depuis un tampon CPU.
unsafe fn upload_texture_level0(g: &gl::GlFunctions, tex: &Texture2D, data: *const c_void) {
    let (_, gl_format, gl_type) = dxgi_format_to_gl(tex.desc.format);
    (g.BindTexture)(gl::GL_TEXTURE_2D, tex.gl_texture);
    (g.TexSubImage2D)(
        gl::GL_TEXTURE_2D,
        0,
        0,
        0,
        tex.desc.width as i32,
        tex.desc.height as i32,
        gl_format,
        gl_type,
        data,
    );
    (g.BindTexture)(gl::GL_TEXTURE_2D, 0);
}

unsafe extern "win64" fn ctx_om_set_render_targets(
    this: *mut DeviceContext,
    count: u32,
    rtvs: *const *mut c_void,
    dsv: *mut c_void,
) {
    let Some(g) = gl::gl() else { return };
    let mut s = (*this).state.lock().unwrap_or_else(|e| e.into_inner());

    if count > 0 && !rtvs.is_null() {
        let rtv = *rtvs;
        if let Some(obj) = as_simple(rtv) {
            (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, (*obj).gl_id);
            s.current_rtv = obj;
        }
    } else {
        (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, 0);
        s.current_rtv = std::ptr::null_mut();
    }
    s.current_dsv = as_simple(dsv).unwrap_or(std::ptr::null_mut());
}

unsafe extern "win64" fn ctx_om_set_rt_uav(
    this: *mut DeviceContext,
    count: u32,
    rtvs: *const *mut c_void,
    dsv: *mut c_void,
    _start: u32,
    _num_uav: u32,
    _uavs: *const *mut c_void,
    _init: *const u32,
) {
    ctx_om_set_render_targets(this, count, rtvs, dsv);
}

unsafe extern "win64" fn ctx_clear_rtv(
    _this: *mut DeviceContext,
    view: *mut c_void,
    color: *const [f32; 4],
) {
    let Some(g) = gl::gl() else { return };
    if color.is_null() {
        return;
    }
    let Some(obj) = as_simple(view) else { return };
    let obj = &*obj;
    let c = &*color;

    (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, obj.gl_id);
    (g.ClearColor)(c[0], c[1], c[2], c[3]);
    (g.Clear)(gl::GL_COLOR_BUFFER_BIT);
}

unsafe extern "win64" fn ctx_clear_dsv(
    _this: *mut DeviceContext,
    _view: *mut c_void,
    flags: u32,
    _depth: f32,
    _stencil: u8,
) {
    let Some(g) = gl::gl() else { return };
    let mut mask = 0u32;
    if flags & 1 != 0 {
        mask |= gl::GL_DEPTH_BUFFER_BIT;
    }
    if flags & 2 != 0 {
        mask |= gl::GL_STENCIL_BUFFER_BIT;
    }
    (g.Clear)(mask);
}

unsafe extern "win64" fn ctx_rs_set_viewports(
    _this: *mut DeviceContext,
    count: u32,
    viewports: *const D3D11Viewport,
) {
    let Some(g) = gl::gl() else { return };
    if count > 0 && !viewports.is_null() {
        let vp = &*viewports;
        (g.Viewport)(
            vp.top_left_x as i32,
            vp.top_left_y as i32,
            vp.width as i32,
            vp.height as i32,
        );
    }
}

unsafe extern "win64" fn ctx_rs_set_scissors(_: *mut DeviceContext, _: u32, _: *const c_void) {}
unsafe extern "win64" fn ctx_rs_set_state(_: *mut DeviceContext, _: *mut c_void) {}

unsafe extern "win64" fn ctx_copy_subresource(
    _this: *mut DeviceContext,
    _dst: *mut c_void,
    _sub: u32,
    _x: u32,
    _y: u32,
    _z: u32,
    _src: *mut c_void,
    _sub2: u32,
    _box_: *const c_void,
) {
}

unsafe extern "win64" fn ctx_copy_resource(
    _: *mut DeviceContext,
    _dst: *mut c_void,
    _src: *mut c_void,
) {
}

unsafe extern "win64" fn ctx_update_subresource(
    _this: *mut DeviceContext,
    resource: *mut c_void,
    _sub: u32,
    _box_: *const c_void,
    data: *const c_void,
    _row_pitch: u32,
    _depth_pitch: u32,
) {
    if resource.is_null() || data.is_null() {
        return;
    }
    let Some(g) = gl::gl() else { return };
    if let Some(buf) = as_buffer(resource) {
        let target = bind_flags_to_gl_target((*buf).desc.bind_flags);
        let size = (*buf).desc.byte_width;
        (g.BindBuffer)(target, (*buf).gl_buffer);
        (g.BufferSubData)(target, 0, size as isize, data);
        (g.BindBuffer)(target, 0);
    } else if let Some(tex) = as_texture2d(resource) {
        // Sous-ressource 0 entière seulement (pas de boîte partielle).
        if (*tex).gl_texture != 0 && texture_bytes(&(*tex).desc).is_some() {
            upload_texture_level0(g, &*tex, data);
        }
    }
}

unsafe extern "win64" fn ctx_generate_mips(_: *mut DeviceContext, srv: *mut c_void) {
    let Some(g) = gl::gl() else { return };
    if let Some(obj) = as_simple(srv) {
        let obj = &*obj;
        if obj.resource_tex != 0 {
            (g.BindTexture)(gl::GL_TEXTURE_2D, obj.resource_tex);
            (g.GenerateMipmap)(gl::GL_TEXTURE_2D);
        }
    }
}

unsafe extern "win64" fn ctx_clear_state(this: *mut DeviceContext) {
    let Some(g) = gl::gl() else { return };
    let mut s = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    *s = DeviceContextState::default();
    (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, 0);
    (g.UseProgram)(0);
}

unsafe extern "win64" fn ctx_flush(_this: *mut DeviceContext) {
    if let Some(g) = gl::gl() {
        (g.Flush)();
    }
}

unsafe extern "win64" fn ctx_dispatch(_: *mut DeviceContext, _x: u32, _y: u32, _z: u32) {}

fn update_program(s: &mut DeviceContextState) {
    let Some(g) = gl::gl() else { return };

    if s.current_vs == 0 && s.current_ps == 0 {
        unsafe {
            (g.UseProgram)(0);
        }
        s.current_program = 0;
        return;
    }

    unsafe {
        if s.current_program != 0 {
            (g.DeleteProgram)(s.current_program);
        }
        let prog = (g.CreateProgram)();
        if s.current_vs != 0 {
            (g.AttachShader)(prog, s.current_vs);
        }
        if s.current_ps != 0 {
            (g.AttachShader)(prog, s.current_ps);
        }
        (g.LinkProgram)(prog);

        let mut ok = 0i32;
        (g.GetProgramiv)(prog, gl::GL_LINK_STATUS, &mut ok);
        if ok == 0 {
            let mut len = 0i32;
            (g.GetProgramiv)(prog, gl::GL_INFO_LOG_LENGTH, &mut len);
            if len > 0 {
                let mut buf = vec![0u8; len as usize];
                (g.GetProgramInfoLog)(
                    prog,
                    len,
                    std::ptr::null_mut(),
                    buf.as_mut_ptr() as *mut gl::GLchar,
                );
                log_warn!(
                    "d3d11",
                    "erreur de link programme : {}",
                    String::from_utf8_lossy(&buf)
                );
            }
        }

        (g.UseProgram)(prog);
        s.current_program = prog;
    }
}

static DEVICE_CONTEXT_VTABLE: DeviceContextVTable = DeviceContextVTable {
    query_interface: ctx_qi,
    add_ref: ctx_addref,
    release: ctx_release,
    get_device: ctx_get_device,
    get_private_data: ctx_get_priv,
    set_private_data: ctx_set_priv,
    set_private_data_interface: ctx_set_priv_iface,
    vs_set_constant_buffers: ctx_noop_3ptr,
    ps_set_shader_resources: ctx_noop_3ptr,
    ps_set_shader: ctx_ps_set_shader,
    ps_set_samplers: ctx_noop_3ptr,
    vs_set_shader: ctx_vs_set_shader,
    draw_indexed: ctx_draw_indexed,
    draw: ctx_draw,
    map: ctx_map,
    unmap: ctx_unmap,
    ps_set_constant_buffers: ctx_noop_3ptr,
    ia_set_input_layout: ctx_ia_set_input_layout,
    ia_set_vertex_buffers: ctx_ia_set_vertex_buffers,
    ia_set_index_buffer: ctx_ia_set_index_buffer,
    draw_indexed_instanced: ctx_draw_indexed_instanced,
    draw_instanced: ctx_draw_instanced,
    gs_set_constant_buffers: ctx_noop_3ptr,
    gs_set_shader: ctx_noop_set_shader,
    ia_set_primitive_topology: ctx_ia_set_topology,
    vs_set_shader_resources: ctx_noop_3ptr,
    vs_set_samplers: ctx_noop_3ptr,
    begin: ctx_noop_1ptr,
    end: ctx_noop_1ptr,
    get_data: ctx_get_data,
    set_predication: ctx_noop_pred,
    gs_set_shader_resources: ctx_noop_3ptr,
    gs_set_samplers: ctx_noop_3ptr,
    om_set_render_targets: ctx_om_set_render_targets,
    om_set_render_targets_and_unordered: ctx_om_set_rt_uav,
    om_set_blend_state: ctx_noop_blend,
    om_set_depth_stencil_state: ctx_noop_ds,
    so_set_targets: ctx_noop_so,
    draw_auto: ctx_noop_void,
    draw_indexed_instanced_indirect: ctx_noop_draw_indirect,
    draw_instanced_indirect: ctx_noop_draw_indirect,
    dispatch: ctx_dispatch,
    dispatch_indirect: ctx_noop_draw_indirect,
    rs_set_state: ctx_rs_set_state,
    rs_set_viewports: ctx_rs_set_viewports,
    rs_set_scissor_rects: ctx_rs_set_scissors,
    copy_subresource_region: ctx_copy_subresource,
    copy_resource: ctx_copy_resource,
    update_subresource: ctx_update_subresource,
    copy_structure_count: ctx_noop_copy_struct,
    clear_render_target_view: ctx_clear_rtv,
    clear_unordered_access_view_uint: ctx_noop_clear_uav_u,
    clear_unordered_access_view_float: ctx_noop_clear_uav_f,
    clear_depth_stencil_view: ctx_clear_dsv,
    generate_mips: ctx_generate_mips,
    set_resource_min_lod: ctx_noop_rminlod,
    get_resource_min_lod: ctx_noop_grminlod,
    resolve_subresource: ctx_noop_resolve,
    execute_command_list: ctx_noop_exec,
    hs_set_shader_resources: ctx_noop_3ptr,
    hs_set_shader: ctx_noop_set_shader,
    hs_set_samplers: ctx_noop_3ptr,
    hs_set_constant_buffers: ctx_noop_3ptr,
    ds_set_shader_resources: ctx_noop_3ptr,
    ds_set_shader: ctx_noop_set_shader,
    ds_set_samplers: ctx_noop_3ptr,
    ds_set_constant_buffers: ctx_noop_3ptr,
    cs_set_shader_resources: ctx_noop_3ptr,
    cs_set_unordered_access_views: ctx_noop_uav,
    cs_set_shader: ctx_noop_set_shader,
    cs_set_samplers: ctx_noop_3ptr,
    cs_set_constant_buffers: ctx_noop_3ptr,
    clear_state: ctx_clear_state,
    flush: ctx_flush,
    get_type_ctx: ctx_get_type,
    get_context_flags: ctx_get_flags,
    finish_command_list: ctx_finish_cl,
};

fn create_device_context() -> *mut DeviceContext {
    Box::into_raw(Box::new(DeviceContext {
        vtable: &DEVICE_CONTEXT_VTABLE,
        ref_count: AtomicU32::new(1),
        state: Mutex::new(DeviceContextState::default()),
    }))
}

// ────────────────────── ID3D11Device ──────────────────────

#[repr(C)]
struct DeviceVTable {
    // IUnknown (0-2)
    query_interface: unsafe extern "win64" fn(*mut Device, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut Device) -> u32,
    release: unsafe extern "win64" fn(*mut Device) -> u32,
    // ID3D11Device (3+)
    create_buffer: unsafe extern "win64" fn(
        *mut Device,
        *const D3D11BufferDesc,
        *const D3D11SubresourceData,
        *mut *mut c_void,
    ) -> i32,
    create_texture_1d: unsafe extern "win64" fn(
        *mut Device,
        *const c_void,
        *const c_void,
        *mut *mut c_void,
    ) -> i32,
    create_texture_2d: unsafe extern "win64" fn(
        *mut Device,
        *const D3D11Texture2DDesc,
        *const D3D11SubresourceData,
        *mut *mut c_void,
    ) -> i32,
    create_texture_3d: unsafe extern "win64" fn(
        *mut Device,
        *const c_void,
        *const c_void,
        *mut *mut c_void,
    ) -> i32,
    create_shader_resource_view:
        unsafe extern "win64" fn(*mut Device, *mut c_void, *const c_void, *mut *mut c_void) -> i32,
    create_unordered_access_view:
        unsafe extern "win64" fn(*mut Device, *mut c_void, *const c_void, *mut *mut c_void) -> i32,
    create_render_target_view:
        unsafe extern "win64" fn(*mut Device, *mut c_void, *const c_void, *mut *mut c_void) -> i32,
    create_depth_stencil_view:
        unsafe extern "win64" fn(*mut Device, *mut c_void, *const c_void, *mut *mut c_void) -> i32,
    create_input_layout: unsafe extern "win64" fn(
        *mut Device,
        *const D3D11InputElementDesc,
        u32,
        *const c_void,
        usize,
        *mut *mut c_void,
    ) -> i32,
    create_vertex_shader: unsafe extern "win64" fn(
        *mut Device,
        *const c_void,
        usize,
        *mut c_void,
        *mut *mut c_void,
    ) -> i32,
    create_geometry_shader: unsafe extern "win64" fn(
        *mut Device,
        *const c_void,
        usize,
        *mut c_void,
        *mut *mut c_void,
    ) -> i32,
    create_geometry_shader_with_so: unsafe extern "win64" fn(
        *mut Device,
        *const c_void,
        usize,
        *const c_void,
        u32,
        *const u32,
        u32,
        u32,
        *mut c_void,
        *mut *mut c_void,
    ) -> i32,
    create_pixel_shader: unsafe extern "win64" fn(
        *mut Device,
        *const c_void,
        usize,
        *mut c_void,
        *mut *mut c_void,
    ) -> i32,
    create_hull_shader: unsafe extern "win64" fn(
        *mut Device,
        *const c_void,
        usize,
        *mut c_void,
        *mut *mut c_void,
    ) -> i32,
    create_domain_shader: unsafe extern "win64" fn(
        *mut Device,
        *const c_void,
        usize,
        *mut c_void,
        *mut *mut c_void,
    ) -> i32,
    create_compute_shader: unsafe extern "win64" fn(
        *mut Device,
        *const c_void,
        usize,
        *mut c_void,
        *mut *mut c_void,
    ) -> i32,
    create_class_linkage: unsafe extern "win64" fn(*mut Device, *mut *mut c_void) -> i32,
    create_blend_state:
        unsafe extern "win64" fn(*mut Device, *const c_void, *mut *mut c_void) -> i32,
    create_depth_stencil_state:
        unsafe extern "win64" fn(*mut Device, *const c_void, *mut *mut c_void) -> i32,
    create_rasterizer_state:
        unsafe extern "win64" fn(*mut Device, *const c_void, *mut *mut c_void) -> i32,
    create_sampler_state:
        unsafe extern "win64" fn(*mut Device, *const c_void, *mut *mut c_void) -> i32,
    create_query: unsafe extern "win64" fn(*mut Device, *const c_void, *mut *mut c_void) -> i32,
    create_predicate: unsafe extern "win64" fn(*mut Device, *const c_void, *mut *mut c_void) -> i32,
    create_counter: unsafe extern "win64" fn(*mut Device, *const c_void, *mut *mut c_void) -> i32,
    create_deferred_context: unsafe extern "win64" fn(*mut Device, u32, *mut *mut c_void) -> i32,
    open_shared_resource:
        unsafe extern "win64" fn(*mut Device, usize, *const Guid, *mut *mut c_void) -> i32,
    check_format_support: unsafe extern "win64" fn(*mut Device, u32, *mut u32) -> i32,
    check_multisample_quality: unsafe extern "win64" fn(*mut Device, u32, u32, *mut u32) -> i32,
    check_counter_info: unsafe extern "win64" fn(*mut Device, *mut c_void),
    check_counter: unsafe extern "win64" fn(
        *mut Device,
        *const c_void,
        *mut u32,
        *mut u32,
        *mut u32,
        *mut u8,
        *mut u32,
        *mut u8,
        *mut u32,
        *mut u8,
        *mut u32,
    ) -> i32,
    check_feature_support: unsafe extern "win64" fn(*mut Device, u32, *mut c_void, u32) -> i32,
    get_private_data:
        unsafe extern "win64" fn(*mut Device, *const Guid, *mut u32, *mut c_void) -> i32,
    set_private_data: unsafe extern "win64" fn(*mut Device, *const Guid, u32, *const c_void) -> i32,
    set_private_data_interface:
        unsafe extern "win64" fn(*mut Device, *const Guid, *const c_void) -> i32,
    get_feature_level: unsafe extern "win64" fn(*mut Device) -> u32,
    get_creation_flags: unsafe extern "win64" fn(*mut Device) -> u32,
    get_device_removed_reason: unsafe extern "win64" fn(*mut Device) -> i32,
    get_immediate_context: unsafe extern "win64" fn(*mut Device, *mut *mut c_void),
    set_exception_mode: unsafe extern "win64" fn(*mut Device, u32) -> i32,
    get_exception_mode: unsafe extern "win64" fn(*mut Device) -> u32,
}

#[repr(C)]
struct Device {
    vtable: *const DeviceVTable,
    ref_count: AtomicU32,
    context: *mut DeviceContext,
    feature_level: u32,
}

unsafe impl Send for Device {}
unsafe impl Sync for Device {}

// Device method implementations
unsafe extern "win64" fn dev_qi(
    this: *mut Device,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let riid = &*riid;
    if *riid == IID_IUNKNOWN || *riid == IID_ID3D11_DEVICE {
        (*this).ref_count.fetch_add(1, Ordering::Relaxed);
        *out = this as *mut c_void;
        return S_OK;
    }
    *out = std::ptr::null_mut();
    E_NOINTERFACE
}
unsafe extern "win64" fn dev_addref(this: *mut Device) -> u32 {
    (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn dev_release(this: *mut Device) -> u32 {
    let prev = (*this).ref_count.fetch_sub(1, Ordering::AcqRel);
    if prev == 1 {
        let dev = Box::from_raw(this);
        // L'appareil relâche sa propre référence sur le contexte immédiat ;
        // celui-ci survit tant que l'application en détient une.
        if !dev.context.is_null() {
            ctx_release(dev.context);
        }
        return 0;
    }
    prev - 1
}

unsafe extern "win64" fn dev_create_buffer(
    _this: *mut Device,
    desc: *const D3D11BufferDesc,
    init: *const D3D11SubresourceData,
    out: *mut *mut c_void,
) -> i32 {
    if desc.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let buf = create_buffer(&*desc, init);
    if buf.is_null() {
        return E_OUTOFMEMORY;
    }
    *out = buf as *mut c_void;
    S_OK
}

unsafe extern "win64" fn dev_create_tex2d(
    _this: *mut Device,
    desc: *const D3D11Texture2DDesc,
    init: *const D3D11SubresourceData,
    out: *mut *mut c_void,
) -> i32 {
    if desc.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let tex = create_texture2d(&*desc, init);
    if tex.is_null() {
        return E_OUTOFMEMORY;
    }
    *out = tex as *mut c_void;
    S_OK
}

unsafe extern "win64" fn dev_create_stub(
    _: *mut Device,
    _: *const c_void,
    _: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

unsafe extern "win64" fn dev_create_srv(
    _this: *mut Device,
    resource: *mut c_void,
    _desc: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let tex_gl = match as_texture2d(resource) {
        Some(t) => (*t).gl_texture,
        None => 0,
    };
    *out = Box::into_raw(Box::new(SimpleComObj {
        vtable: &SIMPLE_COM_VTABLE,
        ref_count: AtomicU32::new(1),
        gl_id: 0,
        resource_tex: tex_gl,
    })) as *mut c_void;
    S_OK
}

unsafe extern "win64" fn dev_create_uav(
    _: *mut Device,
    _: *mut c_void,
    _: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

unsafe extern "win64" fn dev_create_rtv(
    _this: *mut Device,
    resource: *mut c_void,
    _desc: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let rtv = create_render_target_view(resource);
    if rtv.is_null() {
        return E_FAIL;
    }
    *out = rtv as *mut c_void;
    S_OK
}

unsafe extern "win64" fn dev_create_dsv(
    _this: *mut Device,
    _resource: *mut c_void,
    _desc: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = Box::into_raw(Box::new(SimpleComObj {
        vtable: &SIMPLE_COM_VTABLE,
        ref_count: AtomicU32::new(1),
        gl_id: 0,
        resource_tex: 0,
    })) as *mut c_void;
    S_OK
}

unsafe extern "win64" fn dev_create_il(
    _this: *mut Device,
    descs: *const D3D11InputElementDesc,
    count: u32,
    _bytecode: *const c_void,
    _bc_len: usize,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let layout = create_input_layout(descs, count);
    *out = layout as *mut c_void;
    S_OK
}

unsafe extern "win64" fn dev_create_vs(
    _this: *mut Device,
    bytecode: *const c_void,
    size: usize,
    _linkage: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let s = create_shader_object(bytecode, size, ShaderType::Vertex);
    if s.is_null() {
        return E_FAIL;
    }
    *out = s as *mut c_void;
    S_OK
}

unsafe extern "win64" fn dev_create_gs(
    _this: *mut Device,
    bytecode: *const c_void,
    size: usize,
    _linkage: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let s = create_shader_object(bytecode, size, ShaderType::Geometry);
    if s.is_null() {
        return E_FAIL;
    }
    *out = s as *mut c_void;
    S_OK
}

unsafe extern "win64" fn dev_create_gs_so(
    _: *mut Device,
    _: *const c_void,
    _: usize,
    _: *const c_void,
    _: u32,
    _: *const u32,
    _: u32,
    _: u32,
    _: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

unsafe extern "win64" fn dev_create_ps(
    _this: *mut Device,
    bytecode: *const c_void,
    size: usize,
    _linkage: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let s = create_shader_object(bytecode, size, ShaderType::Pixel);
    if s.is_null() {
        return E_FAIL;
    }
    *out = s as *mut c_void;
    S_OK
}

unsafe extern "win64" fn dev_create_hs(
    _: *mut Device,
    _: *const c_void,
    _: usize,
    _: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}
unsafe extern "win64" fn dev_create_ds(
    _: *mut Device,
    _: *const c_void,
    _: usize,
    _: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}
unsafe extern "win64" fn dev_create_cs(
    _: *mut Device,
    _: *const c_void,
    _: usize,
    _: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}
unsafe extern "win64" fn dev_create_cl(_: *mut Device, out: *mut *mut c_void) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

fn create_dummy_com_obj() -> *mut c_void {
    Box::into_raw(Box::new(SimpleComObj {
        vtable: &SIMPLE_COM_VTABLE,
        ref_count: AtomicU32::new(1),
        gl_id: 0,
        resource_tex: 0,
    })) as *mut c_void
}

unsafe extern "win64" fn dev_create_state(
    _: *mut Device,
    _: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = create_dummy_com_obj();
    S_OK
}

unsafe extern "win64" fn dev_create_deferred(_: *mut Device, _: u32, _: *mut *mut c_void) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn dev_open_shared(
    _: *mut Device,
    _: usize,
    _: *const Guid,
    _: *mut *mut c_void,
) -> i32 {
    E_NOTIMPL
}

unsafe extern "win64" fn dev_check_format(_: *mut Device, _format: u32, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0x1 | 0x2 | 0x4 | 0x10 | 0x20; // basic support flags
    S_OK
}

unsafe extern "win64" fn dev_check_ms(_: *mut Device, _: u32, _: u32, out: *mut u32) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    S_OK
}

unsafe extern "win64" fn dev_check_counter_info(_: *mut Device, _: *mut c_void) {}
unsafe extern "win64" fn dev_check_counter(
    _: *mut Device,
    _: *const c_void,
    _: *mut u32,
    _: *mut u32,
    _: *mut u32,
    _: *mut u8,
    _: *mut u32,
    _: *mut u8,
    _: *mut u32,
    _: *mut u8,
    _: *mut u32,
) -> i32 {
    E_NOTIMPL
}

unsafe extern "win64" fn dev_check_feature(
    _: *mut Device,
    _feature: u32,
    _data: *mut c_void,
    _size: u32,
) -> i32 {
    S_OK
}

unsafe extern "win64" fn dev_get_priv(
    _: *mut Device,
    _: *const Guid,
    _: *mut u32,
    _: *mut c_void,
) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn dev_set_priv(
    _: *mut Device,
    _: *const Guid,
    _: u32,
    _: *const c_void,
) -> i32 {
    S_OK
}
unsafe extern "win64" fn dev_set_priv_iface(
    _: *mut Device,
    _: *const Guid,
    _: *const c_void,
) -> i32 {
    S_OK
}

unsafe extern "win64" fn dev_get_feature_level(this: *mut Device) -> u32 {
    (*this).feature_level
}
unsafe extern "win64" fn dev_get_creation_flags(_: *mut Device) -> u32 {
    0
}
unsafe extern "win64" fn dev_get_removed_reason(_: *mut Device) -> i32 {
    S_OK
}

unsafe extern "win64" fn dev_get_immediate_context(this: *mut Device, out: *mut *mut c_void) {
    if !out.is_null() {
        (*(*this).context).ref_count.fetch_add(1, Ordering::Relaxed);
        *out = (*this).context as *mut c_void;
    }
}

unsafe extern "win64" fn dev_set_exception_mode(_: *mut Device, _: u32) -> i32 {
    S_OK
}
unsafe extern "win64" fn dev_get_exception_mode(_: *mut Device) -> u32 {
    0
}

static DEVICE_VTABLE: DeviceVTable = DeviceVTable {
    query_interface: dev_qi,
    add_ref: dev_addref,
    release: dev_release,
    create_buffer: dev_create_buffer,
    create_texture_1d: dev_create_stub,
    create_texture_2d: dev_create_tex2d,
    create_texture_3d: dev_create_stub,
    create_shader_resource_view: dev_create_srv,
    create_unordered_access_view: dev_create_uav,
    create_render_target_view: dev_create_rtv,
    create_depth_stencil_view: dev_create_dsv,
    create_input_layout: dev_create_il,
    create_vertex_shader: dev_create_vs,
    create_geometry_shader: dev_create_gs,
    create_geometry_shader_with_so: dev_create_gs_so,
    create_pixel_shader: dev_create_ps,
    create_hull_shader: dev_create_hs,
    create_domain_shader: dev_create_ds,
    create_compute_shader: dev_create_cs,
    create_class_linkage: dev_create_cl,
    create_blend_state: dev_create_state,
    create_depth_stencil_state: dev_create_state,
    create_rasterizer_state: dev_create_state,
    create_sampler_state: dev_create_state,
    create_query: dev_create_state,
    create_predicate: dev_create_state,
    create_counter: dev_create_state,
    create_deferred_context: dev_create_deferred,
    open_shared_resource: dev_open_shared,
    check_format_support: dev_check_format,
    check_multisample_quality: dev_check_ms,
    check_counter_info: dev_check_counter_info,
    check_counter: dev_check_counter,
    check_feature_support: dev_check_feature,
    get_private_data: dev_get_priv,
    set_private_data: dev_set_priv,
    set_private_data_interface: dev_set_priv_iface,
    get_feature_level: dev_get_feature_level,
    get_creation_flags: dev_get_creation_flags,
    get_device_removed_reason: dev_get_removed_reason,
    get_immediate_context: dev_get_immediate_context,
    set_exception_mode: dev_set_exception_mode,
    get_exception_mode: dev_get_exception_mode,
};

// ────────────────────── Entry points ──────────────────────

pub unsafe extern "win64" fn D3D11CreateDevice(
    _adapter: usize,
    _driver_type: u32,
    _software: usize,
    _flags: u32,
    feature_levels: *const u32,
    num_levels: u32,
    _sdk_version: u32,
    device: *mut usize,
    feature_level: *mut u32,
    context: *mut usize,
) -> i32 {
    let fl = if !feature_levels.is_null() && num_levels > 0 {
        *feature_levels
    } else {
        D3D_FEATURE_LEVEL_11_0
    };

    let ctx = create_device_context();
    let dev = Box::into_raw(Box::new(Device {
        vtable: &DEVICE_VTABLE,
        ref_count: AtomicU32::new(1),
        context: ctx,
        feature_level: fl,
    }));

    if !device.is_null() {
        *device = dev as usize;
    }
    if !feature_level.is_null() {
        *feature_level = fl;
    }
    if !context.is_null() {
        // Référence remise à l'application, distincte de celle de l'appareil.
        ctx_addref(ctx);
        *context = ctx as usize;
    }

    log_info!(
        "d3d11",
        "D3D11CreateDevice → device Winlux OpenGL (FL 0x{:x})",
        fl
    );
    S_OK
}

pub unsafe extern "win64" fn D3D11CreateDeviceAndSwapChain(
    adapter: usize,
    driver_type: u32,
    software: usize,
    flags: u32,
    feature_levels: *const u32,
    num_levels: u32,
    sdk_version: u32,
    swap_chain_desc: usize,
    swap_chain: *mut usize,
    device: *mut usize,
    feature_level: *mut u32,
    context: *mut usize,
) -> i32 {
    let hr = D3D11CreateDevice(
        adapter,
        driver_type,
        software,
        flags,
        feature_levels,
        num_levels,
        sdk_version,
        device,
        feature_level,
        context,
    );
    if hr != S_OK {
        return hr;
    }

    if swap_chain_desc != 0 && !swap_chain.is_null() {
        let desc = &*(swap_chain_desc as *const DxgiSwapChainDesc);
        let dev_ptr = if !device.is_null() {
            *device as *mut c_void
        } else {
            std::ptr::null_mut()
        };
        let sc = dxgi::create_swap_chain(desc, dev_ptr);
        *swap_chain = sc as usize;
        log_info!("d3d11", "SwapChain créée via D3D11CreateDeviceAndSwapChain");
    }

    S_OK
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "D3D11CreateDevice" => fn D3D11CreateDevice,
        "D3D11CreateDeviceAndSwapChain" => fn D3D11CreateDeviceAndSwapChain,
    }
}
