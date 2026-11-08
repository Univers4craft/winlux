//! Implémentation D3D11 → OpenGL pour Winlux.
//! Traduit les interfaces ID3D11Device, ID3D11DeviceContext et les
//! types de ressources vers des appels OpenGL natifs.
#![allow(non_snake_case, non_upper_case_globals, dead_code)]

use crate::dlls::d3d_shader::ShaderType;
use crate::dlls::dxbc;
use crate::dlls::dxgi::{self, DxgiSwapChainDesc};
use crate::dlls::gl_bindings as gl;
use crate::dlls::ole32::Guid;
use crate::{log_info, log_warn};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

#[path = "d2d1_shader.rs"]
pub(crate) mod d2d_shader;
#[path = "d3d11_ext.rs"]
mod ext;
#[path = "d3d11_pipeline.rs"]
mod pipeline;

/// Images présentées (toutes chaînes d'échange), pour WINLUX_D3D11_TRACE.
pub(crate) static FRAMES: AtomicU32 = AtomicU32::new(0);

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
const IID_ID3D11_DEVICE_CHILD: Guid = Guid {
    data1: 0x1841e5c8,
    data2: 0x16b0,
    data3: 0x489b,
    data4: [0xbc, 0xc8, 0x44, 0xcf, 0xb0, 0xd5, 0xde, 0xae],
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
    /// GL_TEXTURE_2D, ou GL_TEXTURE_2D_ARRAY pour un tableau ou un cube.
    pub gl_target: gl::GLenum,
    owns_texture: bool,
    /// Sous-ressource ouverte par Map : son contenu côté processeur.
    staging_buf: Mutex<MappedTexture>,
}

/// État d'un Map de texture : sous-ressource, type d'accès, octets.
#[derive(Default)]
struct MappedTexture {
    level: u32,
    slice: u32,
    map_type: u32,
    data: Vec<u8>,
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
        // ID3D11Resource (dc8e63f3) et ID3D11DeviceChild en sont la base.
        if *riid == IID_IUNKNOWN
            || *riid == IID_ID3D11_TEXTURE2D
            || *riid == IID_ID3D11_DEVICE_CHILD
            || riid.data1 == 0xdc8e_63f3
        {
            (*this).ref_count.fetch_add(1, Ordering::Relaxed);
            *out = this as *mut c_void;
            return S_OK;
        }
        crate::dlls::comobj::log_no_interface("d3d11 (texture 2D)", riid);
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
    unsafe extern "win64" fn add_ref(this: *mut Self) -> u32 {
        (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
    }
    unsafe extern "win64" fn release(this: *mut Self) -> u32 {
        let prev = (*this).ref_count.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            crate::dlls::dxgi_private::forget(this as *const c_void);
            let t = Box::from_raw(this);
            if t.owns_texture && t.gl_texture != 0 {
                if let Some(g) = gl_here() {
                    (g.DeleteTextures)(1, &t.gl_texture);
                }
            }
            return 0;
        }
        prev - 1
    }
    unsafe extern "win64" fn get_device(_this: *mut Self, _out: *mut *mut c_void) {}
    unsafe extern "win64" fn get_priv(
        this: *mut Self,
        g: *const Guid,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32 {
        unsafe { crate::dlls::dxgi_private::get(this as *const c_void, g, size, data) }
    }
    unsafe extern "win64" fn set_priv(
        this: *mut Self,
        g: *const Guid,
        size: u32,
        data: *const c_void,
    ) -> i32 {
        unsafe { crate::dlls::dxgi_private::set(this as *const c_void, g, size, data) }
    }
    unsafe extern "win64" fn set_priv_iface(
        this: *mut Self,
        g: *const Guid,
        iface: *const c_void,
    ) -> i32 {
        unsafe { crate::dlls::dxgi_private::set_iface(this as *const c_void, g, iface) }
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
        gl_target: gl::GL_TEXTURE_2D,
        owns_texture: false,
        staging_buf: Mutex::new(MappedTexture::default()),
    }))
}

fn create_texture2d(
    desc: &D3D11Texture2DDesc,
    initial: *const D3D11SubresourceData,
) -> *mut Texture2D {
    let g = match gl_here() {
        Some(g) => g,
        None => return std::ptr::null_mut(),
    };

    let (gl_internal, _, _) = dxgi_format_to_gl_bind(desc.format, desc.bind_flags);
    // Toute la chaîne de mips est allouée (0 = chaîne complète), et les
    // tableaux (ou cubes) sont des GL_TEXTURE_2D_ARRAY (#519). Le stockage
    // est immuable : les vues de ressource en sont des glTextureView.
    let mut desc = *desc;
    desc.mip_levels = mip_count(&desc);
    desc.array_size = desc.array_size.max(1);
    let target = if is_array_texture(&desc) {
        GL_TEXTURE_2D_ARRAY
    } else {
        gl::GL_TEXTURE_2D
    };

    let mut tex = 0u32;
    unsafe {
        (g.GenTextures)(1, &mut tex);
        (g.BindTexture)(target, tex);
        if target == GL_TEXTURE_2D_ARRAY {
            (g.TexStorage3D)(
                target,
                desc.mip_levels as i32,
                gl_internal,
                desc.width as i32,
                desc.height as i32,
                desc.array_size as i32,
            );
        } else {
            (g.TexStorage2D)(
                target,
                desc.mip_levels as i32,
                gl_internal,
                desc.width as i32,
                desc.height as i32,
            );
        }
        (g.TexParameteri)(target, gl::GL_TEXTURE_MIN_FILTER, gl::GL_LINEAR);
        (g.TexParameteri)(target, gl::GL_TEXTURE_MAG_FILTER, gl::GL_LINEAR);
        (g.TexParameteri)(target, gl::GL_TEXTURE_WRAP_S, gl::GL_CLAMP_TO_EDGE);
        (g.TexParameteri)(target, gl::GL_TEXTURE_WRAP_T, gl::GL_CLAMP_TO_EDGE);
        (g.BindTexture)(target, 0);
    }

    let t = Box::into_raw(Box::new(Texture2D {
        vtable: &TEXTURE2D_VTABLE,
        ref_count: AtomicU32::new(1),
        desc,
        gl_texture: tex,
        gl_target: target,
        owns_texture: true,
        staging_buf: Mutex::new(MappedTexture::default()),
    }));
    // Données initiales : une entrée par sous-ressource, tranche par
    // tranche, niveau par niveau (indice = tranche × niveaux + niveau).
    if !initial.is_null() {
        for slice in 0..desc.array_size {
            for level in 0..desc.mip_levels {
                let d = unsafe { &*initial.add((slice * desc.mip_levels + level) as usize) };
                if d.sys_mem.is_null() {
                    continue;
                }
                let (w, h) = level_size(&desc, level);
                unsafe {
                    upload_region(
                        g,
                        &*t,
                        level,
                        slice,
                        [0, 0, w, h],
                        d.sys_mem_pitch,
                        d.sys_mem,
                    )
                };
            }
        }
    }
    t
}

/// Format DXGI → (format interne, format, type) OpenGL.
///
/// Les formats « TYPELESS » prennent le format typé de leur famille ; ceux
/// des tampons de profondeur (R24G8, R32G8X24) restent des formats de
/// profondeur, sinon le cadre de rendu est incomplet et rien ne se dessine.
fn dxgi_format_to_gl(format: u32) -> (gl::GLenum, gl::GLenum, gl::GLenum) {
    dxgi_format_to_gl_bind(format, 0)
}

/// D3D11_BIND_DEPTH_STENCIL.
const BIND_DEPTH_STENCIL: u32 = 0x40;

/// Formats compressés BC1–BC7 : (format interne GL, octets par bloc 4x4).
fn bc_format(format: u32) -> Option<(gl::GLenum, u32)> {
    Some(match format {
        70 | 71 => (0x83F1, 8),  // BC1 → DXT1 RGBA
        72 => (0x8C4D, 8),       // BC1 sRGB
        73 | 74 => (0x83F2, 16), // BC2 → DXT3
        75 => (0x8C4E, 16),
        76 | 77 => (0x83F3, 16), // BC3 → DXT5
        78 => (0x8C4F, 16),
        79 | 80 => (0x8DBB, 8), // BC4 → RGTC1
        81 => (0x8DBC, 8),
        82 | 83 => (0x8DBD, 16), // BC5 → RGTC2
        84 => (0x8DBE, 16),
        94 | 95 => (0x8E8F, 16), // BC6H → BPTC flottant
        96 => (0x8E8E, 16),
        97 | 98 => (0x8E8C, 16), // BC7 → BPTC
        99 => (0x8E8D, 16),
        _ => return None,
    })
}

fn dxgi_format_to_gl_bind(format: u32, bind: u32) -> (gl::GLenum, gl::GLenum, gl::GLenum) {
    if let Some((internal, _)) = bc_format(format) {
        return (internal, 0, 0);
    }
    const RGBA: u32 = 0x1908;
    const RGB: u32 = 0x1907;
    const RG: u32 = 0x8227;
    const RED: u32 = 0x1903;
    const BGRA: u32 = 0x80E1;
    const RGBA_INT: u32 = 0x8D99;
    const RG_INT: u32 = 0x8228;
    const RED_INT: u32 = 0x8D94;
    const DEPTH: u32 = 0x1902;
    const DEPTH_STENCIL: u32 = 0x84F9;
    const UBYTE: u32 = 0x1401;
    const BYTE: u32 = 0x1400;
    const USHORT: u32 = 0x1403;
    const SHORT: u32 = 0x1402;
    const UINT: u32 = 0x1405;
    const INT: u32 = 0x1404;
    const FLOAT: u32 = 0x1406;
    const HALF: u32 = 0x140B;
    let depth = bind & BIND_DEPTH_STENCIL != 0;
    match format {
        // R32G32B32A32
        1 | 2 => (0x8814, RGBA, FLOAT),
        3 => (0x8D70, RGBA_INT, UINT),
        4 => (0x8D82, RGBA_INT, INT),
        // R32G32B32
        5 | 6 => (0x8815, RGB, FLOAT),
        7 => (0x8D71, 0x8D98, UINT),
        8 => (0x8D83, 0x8D98, INT),
        // R16G16B16A16
        9 | 10 => (0x881A, RGBA, HALF),
        11 => (0x805B, RGBA, USHORT),
        12 => (0x8D76, RGBA_INT, USHORT),
        13 => (0x8F9B, RGBA, SHORT),
        14 => (0x8D88, RGBA_INT, SHORT),
        // R32G32
        15 | 16 => (0x8230, RG, FLOAT),
        17 => (0x823C, RG_INT, UINT),
        18 => (0x823B, RG_INT, INT),
        // R32G8X24 : profondeur 32 bits flottante + gabarit.
        19..=22 => (0x8CAD, DEPTH_STENCIL, 0x8DAD),
        // R10G10B10A2, R11G11B10
        23 | 24 => (0x8059, RGBA, 0x8368),
        25 => (0x906F, RGBA_INT, 0x8368),
        26 => (0x8C3A, RGB, 0x8C3B),
        // R8G8B8A8
        27 | 28 => (gl::GL_RGBA8, RGBA, UBYTE),
        29 => (0x8C43, RGBA, UBYTE),
        30 => (0x8D7C, RGBA_INT, UBYTE),
        31 => (0x8F97, RGBA, BYTE),
        32 => (0x8D8E, RGBA_INT, BYTE),
        // R16G16
        33 | 34 => (0x822F, RG, HALF),
        35 => (0x822C, RG, USHORT),
        36 => (0x823A, RG_INT, USHORT),
        // R32 (profondeur 32 bits flottante si liée comme telle)
        39 if depth => (gl::GL_DEPTH_COMPONENT32F, DEPTH, FLOAT),
        40 => (gl::GL_DEPTH_COMPONENT32F, DEPTH, FLOAT),
        39 | 41 => (0x822E, RED, FLOAT),
        42 => (0x8236, RED_INT, UINT),
        43 => (0x8235, RED_INT, INT),
        // R24G8 : profondeur 24 bits + gabarit.
        44..=47 => (gl::GL_DEPTH24_STENCIL8, DEPTH_STENCIL, 0x84FA),
        // R8G8
        48 | 49 => (0x822B, RG, UBYTE),
        50 => (0x8238, RG_INT, UBYTE),
        // R16 (profondeur 16 bits si liée comme telle)
        53 if depth => (0x81A5, DEPTH, USHORT),
        55 => (0x81A5, DEPTH, USHORT),
        53 | 54 => (0x822D, RED, HALF),
        56 => (0x822A, RED, USHORT),
        57 => (0x8234, RED_INT, USHORT),
        // R8, A8
        60 | 61 | 65 => (0x8229, RED, UBYTE),
        62 => (0x8232, RED_INT, UBYTE),
        // B8G8R8A8 / B8G8R8X8
        // B5G6R5, B5G5R5A1 (bleu dans les bits de poids faible).
        85 => (0x8D62, RGB, 0x8363),
        86 => (0x8057, BGRA, 0x8366),
        87 | 88 | 90 | 92 => (gl::GL_RGBA8, BGRA, UBYTE),
        91 | 93 => (0x8C43, BGRA, UBYTE),
        _ => (gl::GL_RGBA8, RGBA, UBYTE),
    }
}

/// Format de profondeur avec gabarit (point d'attache DEPTH_STENCIL).
pub(super) fn dxgi_has_stencil(format: u32) -> bool {
    matches!(format, 19..=22 | 44..=47)
}

fn dxgi_format_element_size(format: u32) -> u32 {
    match format {
        1..=4 => 16,
        5..=8 => 12,
        9..=22 => 8,
        60..=65 => 1,
        48..=59 | 85 | 86 => 2,
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
        crate::dlls::comobj::log_no_interface("d3d11 (tampon)", riid);
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
    unsafe extern "win64" fn add_ref(this: *mut Self) -> u32 {
        (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
    }
    unsafe extern "win64" fn release(this: *mut Self) -> u32 {
        let prev = (*this).ref_count.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            crate::dlls::dxgi_private::forget(this as *const c_void);
            let b = Box::from_raw(this);
            if b.gl_buffer != 0 {
                if let Some(g) = gl_here() {
                    (g.DeleteBuffers)(1, &b.gl_buffer);
                }
            }
            return 0;
        }
        prev - 1
    }
    unsafe extern "win64" fn get_device(_: *mut Self, _: *mut *mut c_void) {}
    unsafe extern "win64" fn get_priv(
        this: *mut Self,
        g: *const Guid,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32 {
        unsafe { crate::dlls::dxgi_private::get(this as *const c_void, g, size, data) }
    }
    unsafe extern "win64" fn set_priv(
        this: *mut Self,
        g: *const Guid,
        size: u32,
        data: *const c_void,
    ) -> i32 {
        unsafe { crate::dlls::dxgi_private::set(this as *const c_void, g, size, data) }
    }
    unsafe extern "win64" fn set_priv_iface(
        this: *mut Self,
        g: *const Guid,
        iface: *const c_void,
    ) -> i32 {
        unsafe { crate::dlls::dxgi_private::set_iface(this as *const c_void, g, iface) }
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
    let g = match gl_here() {
        Some(g) => g,
        None => return std::ptr::null_mut(),
    };
    // Copie côté processeur, initialisée avec les données de départ : un Map
    // WRITE_NO_OVERWRITE suivi d'Unmap ne doit pas les effacer.
    let mut shadow = vec![0u8; desc.byte_width as usize];
    unsafe {
        if !initial.is_null() && !(*initial).sys_mem.is_null() {
            let src = std::slice::from_raw_parts((*initial).sys_mem as *const u8, shadow.len());
            shadow.copy_from_slice(src);
        }
    }

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
        staging_data: Mutex::new(shadow),
    }))
}

// ────────────────────── Views (RenderTargetView, DepthStencilView, ShaderResourceView) ──────

struct SimpleComVTable {
    query_interface:
        unsafe extern "win64" fn(*mut SimpleComObj, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut SimpleComObj) -> u32,
    release: unsafe extern "win64" fn(*mut SimpleComObj) -> u32,
    // ID3D11DeviceChild (3-6) : ANGLE nomme chaque objet par SetPrivateData.
    get_device: unsafe extern "win64" fn(*mut SimpleComObj, *mut *mut c_void),
    get_private_data:
        unsafe extern "win64" fn(*mut SimpleComObj, *const Guid, *mut u32, *mut c_void) -> i32,
    set_private_data:
        unsafe extern "win64" fn(*mut SimpleComObj, *const Guid, u32, *const c_void) -> i32,
    set_private_data_interface:
        unsafe extern "win64" fn(*mut SimpleComObj, *const Guid, *const c_void) -> i32,
    // Vues : GetResource (7) puis GetDesc (8) ; états : GetDesc (7).
    get_resource_or_desc: unsafe extern "win64" fn(*mut SimpleComObj, *mut *mut c_void),
    get_view_desc: unsafe extern "win64" fn(*mut SimpleComObj, *mut c_void),
}

/// Dernier appareil créé : parent rendu par ID3D11DeviceChild::GetDevice.
static LAST_DEVICE: std::sync::atomic::AtomicPtr<Device> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

unsafe extern "win64" fn child_get_device(_: *mut SimpleComObj, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    let dev = LAST_DEVICE.load(Ordering::Acquire);
    unsafe {
        if !dev.is_null() {
            dev_addref(dev);
        }
        *out = dev.cast();
    }
}
unsafe extern "win64" fn child_get_private_data(
    this: *mut SimpleComObj,
    g: *const Guid,
    size: *mut u32,
    data: *mut c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::get(this as *const c_void, g, size, data) }
}
unsafe extern "win64" fn child_set_private_data(
    this: *mut SimpleComObj,
    g: *const Guid,
    size: u32,
    data: *const c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::set(this as *const c_void, g, size, data) }
}
unsafe extern "win64" fn child_set_private_data_interface(
    this: *mut SimpleComObj,
    g: *const Guid,
    iface: *const c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::set_iface(this as *const c_void, g, iface) }
}
/// États et shaders : GetDesc (7) n'est pas rempli (taille inconnue ici).
unsafe extern "win64" fn child_get_resource_or_desc(_: *mut SimpleComObj, _: *mut *mut c_void) {}
unsafe extern "win64" fn child_get_view_desc(_: *mut SimpleComObj, _: *mut c_void) {}

/// ID3D11View::GetResource : jamais nul pour une vue valide (ANGLE fait
/// Release dessus sans vérifier).
unsafe extern "win64" fn view_get_resource(this: *mut SimpleComObj, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    unsafe {
        let res = (*this).view_resource;
        if !res.is_null() {
            com_addref(res);
        }
        *out = res;
    }
}
unsafe extern "win64" fn view_get_desc(this: *mut SimpleComObj, out: *mut c_void) {
    if out.is_null() {
        return;
    }
    unsafe {
        let len = (*this).view_desc_len.min(24);
        std::ptr::copy_nonoverlapping(
            (*this).view_desc.as_ptr().cast::<u8>(),
            out.cast::<u8>(),
            len,
        );
    }
}

unsafe fn com_addref(p: *mut c_void) -> u32 {
    unsafe {
        let vt = *(p as *const *const usize);
        let f: unsafe extern "win64" fn(*mut c_void) -> u32 = std::mem::transmute(*vt.add(1));
        f(p)
    }
}
unsafe fn com_release(p: *mut c_void) -> u32 {
    unsafe {
        let vt = *(p as *const *const usize);
        let f: unsafe extern "win64" fn(*mut c_void) -> u32 = std::mem::transmute(*vt.add(2));
        f(p)
    }
}

#[derive(Clone, Copy)]
enum ViewKind {
    RenderTarget,
    ShaderResource,
    DepthStencil,
    UnorderedAccess,
}

/// Rattache une vue à sa ressource (AddRef) et retient son descripteur :
/// celui de l'appel s'il est donné, sinon celui d'une vue 2D par défaut.
unsafe fn attach_view(
    view: *mut SimpleComObj,
    kind: ViewKind,
    resource: *mut c_void,
    desc: *const c_void,
) {
    unsafe {
        (*view).vtable = &VIEW_COM_VTABLE;
        if !resource.is_null() {
            com_addref(resource);
            (*view).view_resource = resource;
        }
        // D3D11_RENDER_TARGET_VIEW_DESC et D3D11_UNORDERED_ACCESS_VIEW_DESC
        // font 20 octets, les autres 24.
        let len = if matches!(kind, ViewKind::RenderTarget | ViewKind::UnorderedAccess) {
            20
        } else {
            24
        };
        (*view).view_desc_len = len;
        let d = &mut (*view).view_desc;
        if !desc.is_null() {
            std::ptr::copy_nonoverlapping(desc.cast::<u8>(), d.as_mut_ptr().cast::<u8>(), len);
            return;
        }
        let tex = as_texture2d(resource).map(|t| (*t).desc);
        d[0] = tex.map_or(0, |t| t.format);
        match kind {
            // D3D11_RTV_DIMENSION_TEXTURE2D = 4, MipSlice 0.
            ViewKind::RenderTarget => d[1] = 4,
            // D3D11_SRV_DIMENSION_TEXTURE2D = 4, MostDetailedMip 0, MipLevels.
            // Tableau : TEXTURE2DARRAY (5) sur toutes les tranches ; cube :
            // TEXTURECUBE (9) ou TEXTURECUBEARRAY (10).
            ViewKind::ShaderResource => {
                d[1] = match tex {
                    None => 1,
                    Some(t) if t.misc_flags & MISC_TEXTURECUBE != 0 && t.array_size > 6 => {
                        d[4] = 0;
                        d[5] = t.array_size / 6;
                        10
                    }
                    Some(t) if t.misc_flags & MISC_TEXTURECUBE != 0 => 9,
                    Some(t) if t.array_size > 1 => {
                        d[4] = 0;
                        d[5] = t.array_size;
                        5
                    }
                    Some(_) => 4,
                };
                d[3] = tex.map_or(0, |t| t.mip_levels.max(1));
            }
            // D3D11_DSV_DIMENSION_TEXTURE2D = 3, Flags 0, MipSlice 0.
            ViewKind::DepthStencil => d[1] = 3,
            // Tampon entier (1), Texture2D (4) ou toutes les tranches (5).
            ViewKind::UnorderedAccess => {
                d[1] = match tex {
                    None => 1,
                    Some(t) if t.array_size > 1 => {
                        d[4] = t.array_size;
                        5
                    }
                    Some(_) => 4,
                };
            }
        }
    }
}

struct SimpleComObj {
    vtable: *const SimpleComVTable,
    ref_count: AtomicU32,
    gl_id: gl::GLuint,
    resource_tex: gl::GLuint,
    /// Vues : ressource visée (référence tenue) et descripteur D3D11 rendu
    /// par GetDesc (`view_desc_len` octets : 20 pour une RTV, 24 sinon).
    view_resource: *mut c_void,
    view_desc: [u32; 6],
    view_desc_len: usize,
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
            crate::dlls::dxgi_private::forget(this as *const c_void);
            pipeline::forget(this.cast(), (*this).gl_id);
            let obj = Box::from_raw(this);
            // Vue de ressource : sa glTextureView lui appartient.
            if let Some(t) = as_texture2d(obj.view_resource) {
                if obj.view_desc_len == 24
                    && obj.resource_tex != 0
                    && obj.resource_tex != (*t).gl_texture
                {
                    if let Some(g) = gl_here() {
                        (g.DeleteTextures)(1, &obj.resource_tex);
                    }
                }
            }
            if !obj.view_resource.is_null() {
                com_release(obj.view_resource);
            }
            // Seule une vue de rendu (descripteur de 20 octets) porte un cadre.
            if obj.gl_id != 0 && obj.view_desc_len == 20 {
                if let Some(g) = gl_here() {
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
    get_device: child_get_device,
    get_private_data: child_get_private_data,
    set_private_data: child_set_private_data,
    set_private_data_interface: child_set_private_data_interface,
    get_resource_or_desc: child_get_resource_or_desc,
    get_view_desc: child_get_view_desc,
};

/// Vues (RTV, SRV, DSV) : même objet, avec GetResource et GetDesc remplis.
static VIEW_COM_VTABLE: SimpleComVTable = SimpleComVTable {
    query_interface: SimpleComObj::query_interface,
    add_ref: SimpleComObj::add_ref,
    release: SimpleComObj::release,
    get_device: child_get_device,
    get_private_data: child_get_private_data,
    set_private_data: child_set_private_data,
    set_private_data_interface: child_set_private_data_interface,
    get_resource_or_desc: view_get_resource,
    get_view_desc: view_get_desc,
};

/// Fonctions GL, avec un contexte courant sur ce thread (voir
/// `dxgi::ensure_thread_context`).
fn gl_here() -> Option<&'static gl::GlFunctions> {
    crate::dlls::dxgi::ensure_thread_context();
    gl::gl()
}

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
    let vt = vtable_ptr(p);
    (vt == &SIMPLE_COM_VTABLE as *const SimpleComVTable as *const c_void
        || vt == &VIEW_COM_VTABLE as *const SimpleComVTable as *const c_void)
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

/// Nombre de niveaux de mip (0 dans la description = chaîne complète).
fn mip_count(desc: &D3D11Texture2DDesc) -> u32 {
    let full = 32 - desc.width.max(desc.height).max(1).leading_zeros();
    match desc.mip_levels {
        0 => full,
        n => n.min(full),
    }
}

/// Taille d'un niveau de mip.
fn level_size(desc: &D3D11Texture2DDesc, level: u32) -> (u32, u32) {
    (
        (desc.width >> level.min(31)).max(1),
        (desc.height >> level.min(31)).max(1),
    )
}

/// GL_TEXTURE_2D_ARRAY, GL_TEXTURE_CUBE_MAP, GL_TEXTURE_CUBE_MAP_ARRAY.
pub(super) const GL_TEXTURE_2D_ARRAY: gl::GLenum = 0x8C1A;
const GL_TEXTURE_CUBE_MAP: gl::GLenum = 0x8513;
const GL_TEXTURE_CUBE_MAP_ARRAY: gl::GLenum = 0x9009;
/// D3D11_RESOURCE_MISC_TEXTURECUBE.
const MISC_TEXTURECUBE: u32 = 0x4;

/// Tableau ou cube : stocké en GL_TEXTURE_2D_ARRAY.
fn is_array_texture(desc: &D3D11Texture2DDesc) -> bool {
    desc.array_size > 1 || desc.misc_flags & MISC_TEXTURECUBE != 0
}

/// Sous-ressource D3D → (niveau de mip, tranche).
fn subresource(desc: &D3D11Texture2DDesc, sub: u32) -> (u32, u32) {
    let levels = mip_count(desc).max(1);
    (sub % levels, (sub / levels).min(desc.array_size.max(1) - 1))
}

/// Pas d'une ligne (octets) : rangée de blocs 4x4 pour un format BC.
fn row_pitch(format: u32, width: u32) -> u32 {
    match bc_format(format) {
        Some((_, block)) => width.div_ceil(4) * block,
        None => width * dxgi_format_element_size(format),
    }
}

/// Lignes stockées (rangées de blocs pour un format BC).
fn row_count(format: u32, height: u32) -> u32 {
    if bc_format(format).is_some() {
        height.div_ceil(4)
    } else {
        height
    }
}

/// Octets d'un niveau de mip (une tranche).
fn level_bytes(desc: &D3D11Texture2DDesc, level: u32) -> Option<usize> {
    let (w, h) = level_size(desc, level);
    (row_pitch(desc.format, w) as usize)
        .checked_mul(row_count(desc.format, h) as usize)
        .filter(|&n| n <= 1 << 30)
}

/// Exécute `f` avec un pas de ligne source (octets, 0 = lignes jointives)
/// et un alignement de 1 octet, puis rend les réglages par défaut.
unsafe fn with_unpack_pitch(g: &gl::GlFunctions, format: u32, pitch: u32, f: impl FnOnce()) {
    const UNPACK_ROW_LENGTH: u32 = 0x0CF2;
    const UNPACK_ALIGNMENT: u32 = 0x0CF5;
    let elem = dxgi_format_element_size(format).max(1);
    let row = if pitch != 0 && pitch.is_multiple_of(elem) {
        (pitch / elem) as i32
    } else {
        0
    };
    (g.PixelStorei)(UNPACK_ALIGNMENT, 1);
    (g.PixelStorei)(UNPACK_ROW_LENGTH, row);
    f();
    (g.PixelStorei)(UNPACK_ROW_LENGTH, 0);
    (g.PixelStorei)(UNPACK_ALIGNMENT, 4);
}

/// Boîte D3D11_BOX : left, top, front, right, bottom, back.
#[repr(C)]
#[derive(Clone, Copy)]
struct D3D11Box {
    left: u32,
    top: u32,
    front: u32,
    right: u32,
    bottom: u32,
    back: u32,
}

fn create_render_target_view(resource: *mut c_void, desc: *const c_void) -> *mut SimpleComObj {
    let g = match gl_here() {
        Some(g) => g,
        None => return std::ptr::null_mut(),
    };

    let tex = unsafe { as_texture2d(resource) };
    let tex_gl = tex.map_or(0, |t| unsafe { (*t).gl_texture });
    // D3D11_RENDER_TARGET_VIEW_DESC : Format, ViewDimension, puis MipSlice
    // et, pour un tableau (5), FirstArraySlice.
    let (mip, layer) = if desc.is_null() {
        (0, 0)
    } else {
        let d = desc as *const u32;
        unsafe {
            match *d.add(1) {
                4 => (*d.add(2), 0),
                5 => (*d.add(2), *d.add(3)),
                _ => (0, 0),
            }
        }
    };

    let mut fbo = 0u32;
    unsafe {
        (g.GenFramebuffers)(1, &mut fbo);
        (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, fbo);
        if let Some(t) = tex {
            attach_texture(g, gl::GL_COLOR_ATTACHMENT0, &*t, mip, layer);
        }
        (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, 0);
    }

    Box::into_raw(Box::new(SimpleComObj {
        vtable: &SIMPLE_COM_VTABLE,
        ref_count: AtomicU32::new(1),
        gl_id: fbo,
        resource_tex: tex_gl,
        view_resource: std::ptr::null_mut(),
        view_desc: [0; 6],
        view_desc_len: 0,
    }))
}

/// Rattache un niveau (et une tranche pour un tableau) d'une texture au
/// cadre lié.
pub(super) unsafe fn attach_texture(
    g: &gl::GlFunctions,
    point: gl::GLenum,
    tex: &Texture2D,
    mip: u32,
    layer: u32,
) {
    if tex.gl_target == GL_TEXTURE_2D_ARRAY {
        (g.FramebufferTextureLayer)(
            gl::GL_FRAMEBUFFER,
            point,
            tex.gl_texture,
            mip as i32,
            layer as i32,
        );
    } else {
        (g.FramebufferTexture2D)(
            gl::GL_FRAMEBUFFER,
            point,
            gl::GL_TEXTURE_2D,
            tex.gl_texture,
            mip as i32,
        );
    }
}

/// Cible GL d'une vue de ressource selon sa dimension D3D11.
pub(super) fn srv_target(dim: u32) -> gl::GLenum {
    match dim {
        5 => GL_TEXTURE_2D_ARRAY,
        9 => GL_TEXTURE_CUBE_MAP,
        10 => GL_TEXTURE_CUBE_MAP_ARRAY,
        _ => gl::GL_TEXTURE_2D,
    }
}

/// Vue GL (glTextureView) d'une vue de ressource : cible, niveaux et
/// tranches qu'elle désigne. 0 si la vue ne peut être créée (texture de
/// stockage modifiable, comme le tampon arrière) : la texture est alors
/// liée telle quelle.
unsafe fn create_texture_view(g: &gl::GlFunctions, tex: &Texture2D, d: &[u32; 6]) -> gl::GLuint {
    if !tex.owns_texture || tex.gl_texture == 0 {
        return 0;
    }
    let levels = tex.desc.mip_levels.max(1);
    let layers = tex.desc.array_size.max(1);
    let (first_mip, mips) = (d[2].min(levels - 1), d[3]);
    let mips = if mips == 0 || mips == u32::MAX {
        levels - first_mip
    } else {
        mips.min(levels - first_mip)
    };
    let (first_layer, count) = match d[1] {
        5 => (d[4], d[5].max(1)),
        9 => (0, 6),
        10 => (d[4], d[5].max(1) * 6),
        _ => (0, 1),
    };
    if first_layer + count > layers {
        return 0;
    }
    let target = srv_target(d[1]);
    let (internal, _, _) = dxgi_format_to_gl_bind(tex.desc.format, tex.desc.bind_flags);
    let mut view = 0;
    (g.GenTextures)(1, &mut view);
    while (g.GetError)() != 0 {}
    (g.TextureView)(
        view,
        target,
        tex.gl_texture,
        internal,
        first_mip,
        mips,
        first_layer,
        count,
    );
    if (g.GetError)() != 0 {
        (g.DeleteTextures)(1, &view);
        return 0;
    }
    view
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
    let g = match gl_here() {
        Some(g) => g,
        None => return std::ptr::null_mut(),
    };

    let gl_type = match shader_type {
        ShaderType::Vertex => gl::GL_VERTEX_SHADER,
        ShaderType::Pixel => gl::GL_FRAGMENT_SHADER,
        ShaderType::Geometry => gl::GL_GEOMETRY_SHADER,
        ShaderType::Compute => 0x91B9, // GL_COMPUTE_SHADER
    };

    let bytes = if !bytecode.is_null() && size > 0 {
        unsafe { std::slice::from_raw_parts(bytecode as *const u8, size) }
    } else {
        &[]
    };

    // Traduction DXBC → GLSL (#137).
    let stage = match shader_type {
        ShaderType::Vertex => dxbc::Stage::Vertex,
        ShaderType::Pixel => dxbc::Stage::Pixel,
        ShaderType::Geometry => dxbc::Stage::Geometry,
        _ => dxbc::Stage::Compute,
    };
    let translated = pipeline::translate(bytes, stage);
    let shader = translated
        .as_ref()
        .and_then(|t| compile_glsl(g, gl_type, &t.glsl));
    let meta = if shader.is_some() {
        translated.map(|t| pipeline::Meta::Shader {
            stage,
            inputs: t.inputs,
            pairs: t.sampler_pairs,
        })
    } else {
        None
    };
    // Traduction impossible : aucun shader GL. Les tirages qui l'utilisent
    // sont sautés ; un shader « de passage » aux entrées et sorties
    // différentes donnait des programmes invalides, et Mesa a planté dans
    // glLinkProgram sur un pixel shader compilé en étage géométrique (#517).
    if shader.is_none() {
        log_warn!(
            "d3d11",
            "shader {:?} non traduit : les tirages qui l'utilisent sont sautés",
            stage
        );
    }
    let shader = shader.unwrap_or(0);

    let obj = Box::into_raw(Box::new(SimpleComObj {
        vtable: &SIMPLE_COM_VTABLE,
        ref_count: AtomicU32::new(1),
        gl_id: shader,
        resource_tex: 0,
        view_resource: std::ptr::null_mut(),
        view_desc: [0; 6],
        view_desc_len: 0,
    }));
    if let Some(m) = meta {
        pipeline::register(obj.cast(), m);
    }
    obj
}

/// Compile un shader GLSL ; `None` (et le journal du pilote) en cas d'échec.
fn compile_glsl(g: &gl::GlFunctions, gl_type: gl::GLenum, glsl: &str) -> Option<gl::GLuint> {
    unsafe {
        let shader = (g.CreateShader)(gl_type);
        if shader == 0 {
            return None;
        }
        let src = glsl.as_ptr() as *const gl::GLchar;
        let len = glsl.len() as gl::GLint;
        (g.ShaderSource)(shader, 1, &src, &len);
        (g.CompileShader)(shader);
        let mut ok = 0i32;
        (g.GetShaderiv)(shader, gl::GL_COMPILE_STATUS, &mut ok);
        if ok != 0 {
            return Some(shader);
        }
        let mut log_len = 0i32;
        (g.GetShaderiv)(shader, gl::GL_INFO_LOG_LENGTH, &mut log_len);
        let mut buf = vec![0u8; log_len.max(1) as usize];
        (g.GetShaderInfoLog)(
            shader,
            log_len,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut gl::GLchar,
        );
        log_warn!(
            "d3d11",
            "erreur de compilation shader : {}",
            String::from_utf8_lossy(&buf)
        );
        crate::log_debug!("d3d11", "source refusée :\n{glsl}");
        (g.DeleteShader)(shader);
        None
    }
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
    let elements = if descs.is_null() {
        Vec::new()
    } else {
        unsafe { pipeline::layout_from(descs, count) }
    };
    let obj = create_dummy_com_obj() as *mut SimpleComObj;
    pipeline::register(obj.cast(), pipeline::Meta::Layout(elements));
    obj
}

static LAYOUT_STORE: Mutex<Vec<usize>> = Mutex::new(Vec::new());

// ────────────────────── ID3D11DeviceContext ──────────────────────

struct DeviceContextState {
    current_rtv: *mut SimpleComObj,
    current_dsv: *mut SimpleComObj,
    /// Cadre propre aux passes « profondeur seule » (OMSetRenderTargets sans
    /// cible couleur, ex. cascades d'ombre), créé à la demande (#530).
    depth_fbo: gl::GLuint,
    current_vs: gl::GLuint,
    current_ps: gl::GLuint,
    current_program: gl::GLuint,
    current_vao: gl::GLuint,
    topology: u32,
    /// Format du tampon d'index courant (DXGI_FORMAT_R16_UINT ou R32_UINT).
    index_format: u32,
    viewports: [D3D11Viewport; 16],
    num_viewports: u32,
    /// Pipeline programmable et états fixes (d3d11_pipeline.rs).
    pipe: pipeline::State,
}

impl Default for DeviceContextState {
    fn default() -> Self {
        Self {
            current_rtv: std::ptr::null_mut(),
            current_dsv: std::ptr::null_mut(),
            depth_fbo: 0,
            current_vs: 0,
            current_ps: 0,
            current_program: 0,
            current_vao: 0,
            topology: D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            index_format: DXGI_FORMAT_R16_UINT,
            viewports: [D3D11Viewport::default(); 16],
            num_viewports: 0,
            pipe: pipeline::State::default(),
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
    vs_get_constant_buffers:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 72
    ps_get_shader_resources:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 73
    ps_get_shader:
        unsafe extern "win64" fn(*mut DeviceContext, *mut *mut c_void, *mut *mut c_void, *mut u32), // 74
    ps_get_samplers: unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 75
    vs_get_shader:
        unsafe extern "win64" fn(*mut DeviceContext, *mut *mut c_void, *mut *mut c_void, *mut u32), // 76
    ps_get_constant_buffers:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 77
    ia_get_input_layout: unsafe extern "win64" fn(*mut DeviceContext, *mut *mut c_void), // 78
    ia_get_vertex_buffers: unsafe extern "win64" fn(
        *mut DeviceContext,
        u32,
        u32,
        *mut *mut c_void,
        *mut u32,
        *mut u32,
    ), // 79
    ia_get_index_buffer:
        unsafe extern "win64" fn(*mut DeviceContext, *mut *mut c_void, *mut u32, *mut u32), // 80
    gs_get_constant_buffers:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 81
    gs_get_shader:
        unsafe extern "win64" fn(*mut DeviceContext, *mut *mut c_void, *mut *mut c_void, *mut u32), // 82
    ia_get_primitive_topology: unsafe extern "win64" fn(*mut DeviceContext, *mut u32), // 83
    vs_get_shader_resources:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 84
    vs_get_samplers: unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 85
    get_predication: unsafe extern "win64" fn(*mut DeviceContext, *mut *mut c_void, *mut i32), // 86
    gs_get_shader_resources:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 87
    gs_get_samplers: unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 88
    om_get_render_targets:
        unsafe extern "win64" fn(*mut DeviceContext, u32, *mut *mut c_void, *mut *mut c_void), // 89
    om_get_render_targets_and_uavs: unsafe extern "win64" fn(
        *mut DeviceContext,
        u32,
        *mut *mut c_void,
        *mut *mut c_void,
        u32,
        u32,
        *mut *mut c_void,
    ), // 90
    om_get_blend_state:
        unsafe extern "win64" fn(*mut DeviceContext, *mut *mut c_void, *mut f32, *mut u32), // 91
    om_get_depth_stencil_state:
        unsafe extern "win64" fn(*mut DeviceContext, *mut *mut c_void, *mut u32), // 92
    so_get_targets: unsafe extern "win64" fn(*mut DeviceContext, u32, *mut *mut c_void), // 93
    rs_get_state: unsafe extern "win64" fn(*mut DeviceContext, *mut *mut c_void),      // 94
    rs_get_viewports: unsafe extern "win64" fn(*mut DeviceContext, *mut u32, *mut c_void), // 95
    rs_get_scissor_rects: unsafe extern "win64" fn(*mut DeviceContext, *mut u32, *mut c_void), // 96
    hs_get_shader_resources:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 97
    hs_get_shader:
        unsafe extern "win64" fn(*mut DeviceContext, *mut *mut c_void, *mut *mut c_void, *mut u32), // 98
    hs_get_samplers: unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 99
    hs_get_constant_buffers:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 100
    ds_get_shader_resources:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 101
    ds_get_shader:
        unsafe extern "win64" fn(*mut DeviceContext, *mut *mut c_void, *mut *mut c_void, *mut u32), // 102
    ds_get_samplers: unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 103
    ds_get_constant_buffers:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 104
    cs_get_shader_resources:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 105
    cs_get_unordered_access_views:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 106
    cs_get_shader:
        unsafe extern "win64" fn(*mut DeviceContext, *mut *mut c_void, *mut *mut c_void, *mut u32), // 107
    cs_get_samplers: unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 108
    cs_get_constant_buffers:
        unsafe extern "win64" fn(*mut DeviceContext, u32, u32, *mut *mut c_void), // 109
    clear_state: unsafe extern "win64" fn(*mut DeviceContext), // 110
    flush: unsafe extern "win64" fn(*mut DeviceContext),       // 111
    get_type_ctx: unsafe extern "win64" fn(*mut DeviceContext) -> u32, // 112
    get_context_flags: unsafe extern "win64" fn(*mut DeviceContext) -> u32, // 113
    finish_command_list: unsafe extern "win64" fn(*mut DeviceContext, i32, *mut *mut c_void) -> i32, // 114
    /// ID3D11DeviceContext1 à 4 (115-148).
    ext: ext::ContextExt,
}

#[repr(C)]
pub(crate) struct DeviceContext {
    vtable: *const DeviceContextVTable,
    ref_count: AtomicU32,
    state: Mutex<DeviceContextState>,
    /// Appareil qui a créé ce contexte (sans référence : il vit plus longtemps).
    device: std::sync::atomic::AtomicPtr<Device>,
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
    if *riid == IID_IUNKNOWN
        || *riid == IID_ID3D11_DEVICE_CONTEXT
        || *riid == IID_ID3D11_DEVICE_CHILD
        || ext::is_context_n_iid(riid)
    {
        (*this).ref_count.fetch_add(1, Ordering::Relaxed);
        *out = this as *mut c_void;
        return S_OK;
    }
    // ID3D10Multithread se demande aussi au contexte immédiat.
    let dev = (*this).device.load(Ordering::Acquire);
    if *riid == ext::IID_MULTITHREAD && !dev.is_null() {
        return dev_qi(dev, riid, out);
    }
    crate::dlls::comobj::log_no_interface("d3d11 (contexte)", riid);
    *out = std::ptr::null_mut();
    E_NOINTERFACE
}
unsafe extern "win64" fn ctx_addref(this: *mut DeviceContext) -> u32 {
    (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn ctx_release(this: *mut DeviceContext) -> u32 {
    let prev = (*this).ref_count.fetch_sub(1, Ordering::AcqRel);
    if prev == 1 {
        crate::dlls::dxgi_private::forget(this as *const c_void);
        let _ = Box::from_raw(this);
        return 0;
    }
    prev - 1
}
unsafe extern "win64" fn ctx_get_device(this: *mut DeviceContext, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    let dev = (*this).device.load(Ordering::Acquire);
    if !dev.is_null() {
        dev_addref(dev);
    }
    *out = dev.cast();
}
unsafe extern "win64" fn ctx_get_priv(
    this: *mut DeviceContext,
    g: *const Guid,
    size: *mut u32,
    data: *mut c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::get(this as *const c_void, g, size, data) }
}
unsafe extern "win64" fn ctx_set_priv(
    this: *mut DeviceContext,
    g: *const Guid,
    size: u32,
    data: *const c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::set(this as *const c_void, g, size, data) }
}
unsafe extern "win64" fn ctx_set_priv_iface(
    this: *mut DeviceContext,
    g: *const Guid,
    iface: *const c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::set_iface(this as *const c_void, g, iface) }
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
// Méthodes Get* d'ID3D11DeviceContext (72 à 109) : l'état n'est pas relu, les
// sorties sont mises à zéro (aucun objet renvoyé), comme un contexte neuf.
unsafe fn zero_ptrs(out: *mut *mut c_void, n: u32) {
    if !out.is_null() {
        for i in 0..n.min(128) as usize {
            unsafe { *out.add(i) = core::ptr::null_mut() };
        }
    }
}
unsafe fn zero_u32(out: *mut u32) {
    if !out.is_null() {
        unsafe { *out = 0 };
    }
}
unsafe extern "win64" fn ctx_get_array(
    _: *mut DeviceContext,
    _: u32,
    n: u32,
    out: *mut *mut c_void,
) {
    unsafe { zero_ptrs(out, n) };
}
unsafe extern "win64" fn ctx_get_shader(
    _: *mut DeviceContext,
    shader: *mut *mut c_void,
    classes: *mut *mut c_void,
    count: *mut u32,
) {
    unsafe {
        zero_ptrs(shader, 1);
        if !count.is_null() {
            zero_ptrs(classes, *count);
        }
        zero_u32(count);
    }
}
unsafe extern "win64" fn ctx_get_one(_: *mut DeviceContext, out: *mut *mut c_void) {
    unsafe { zero_ptrs(out, 1) };
}
unsafe extern "win64" fn ctx_get_vertex_buffers(
    _: *mut DeviceContext,
    _: u32,
    n: u32,
    buffers: *mut *mut c_void,
    strides: *mut u32,
    offsets: *mut u32,
) {
    unsafe {
        zero_ptrs(buffers, n);
        for i in 0..n.min(128) as usize {
            zero_u32(if strides.is_null() {
                strides
            } else {
                strides.add(i)
            });
            zero_u32(if offsets.is_null() {
                offsets
            } else {
                offsets.add(i)
            });
        }
    }
}
unsafe extern "win64" fn ctx_get_index_buffer(
    _: *mut DeviceContext,
    buf: *mut *mut c_void,
    fmt: *mut u32,
    off: *mut u32,
) {
    unsafe {
        zero_ptrs(buf, 1);
        zero_u32(fmt);
        zero_u32(off);
    }
}
unsafe extern "win64" fn ctx_get_u32(_: *mut DeviceContext, out: *mut u32) {
    unsafe { zero_u32(out) };
}
unsafe extern "win64" fn ctx_get_predication(
    _: *mut DeviceContext,
    pred: *mut *mut c_void,
    value: *mut i32,
) {
    unsafe {
        zero_ptrs(pred, 1);
        zero_u32(value.cast());
    }
}
unsafe extern "win64" fn ctx_get_render_targets(
    _: *mut DeviceContext,
    n: u32,
    rtv: *mut *mut c_void,
    dsv: *mut *mut c_void,
) {
    unsafe {
        zero_ptrs(rtv, n);
        zero_ptrs(dsv, 1);
    }
}
unsafe extern "win64" fn ctx_get_rt_uav(
    _: *mut DeviceContext,
    n: u32,
    rtv: *mut *mut c_void,
    dsv: *mut *mut c_void,
    _: u32,
    n_uav: u32,
    uav: *mut *mut c_void,
) {
    unsafe {
        zero_ptrs(rtv, n);
        zero_ptrs(dsv, 1);
        zero_ptrs(uav, n_uav);
    }
}
unsafe extern "win64" fn ctx_get_blend(
    _: *mut DeviceContext,
    state: *mut *mut c_void,
    factor: *mut f32,
    mask: *mut u32,
) {
    unsafe {
        zero_ptrs(state, 1);
        if !factor.is_null() {
            for i in 0..4 {
                *factor.add(i) = 1.0;
            }
        }
        if !mask.is_null() {
            *mask = u32::MAX;
        }
    }
}
unsafe extern "win64" fn ctx_get_ptr_u32(
    _: *mut DeviceContext,
    state: *mut *mut c_void,
    value: *mut u32,
) {
    unsafe {
        zero_ptrs(state, 1);
        zero_u32(value);
    }
}
unsafe extern "win64" fn ctx_get_so(_: *mut DeviceContext, n: u32, out: *mut *mut c_void) {
    unsafe { zero_ptrs(out, n) };
}
unsafe extern "win64" fn ctx_get_count(_: *mut DeviceContext, count: *mut u32, _: *mut c_void) {
    unsafe { zero_u32(count) };
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
    crate::dlls::d2d1::unsupported_call("ID3D11DeviceContext::FinishCommandList")
}
unsafe extern "win64" fn ctx_noop_copy_struct(
    _: *mut DeviceContext,
    _: *mut c_void,
    _: u32,
    _: *mut c_void,
) {
}
/// ClearUnorderedAccessView* : tampon rempli mot par mot (valeur 0 de
/// D3D), texture effacée au niveau de la vue.
unsafe fn clear_uav(view: *mut c_void, words: [u32; 4], float: bool) {
    let Some(g) = gl_here() else { return };
    let Some(v) = as_simple(view) else { return };
    let b = pipeline::UavBinding::of_view(v);
    if b.buf != 0 {
        const GL_COPY_WRITE_BUFFER: u32 = 0x8F37;
        const GL_R32UI: u32 = 0x8236;
        const GL_RED_INTEGER: u32 = 0x8D94;
        (g.BindBuffer)(GL_COPY_WRITE_BUFFER, b.buf);
        (g.ClearBufferSubData)(
            GL_COPY_WRITE_BUFFER,
            GL_R32UI,
            b.off,
            b.len,
            GL_RED_INTEGER,
            gl::GL_UNSIGNED_INT,
            words.as_ptr().cast(),
        );
        (g.BindBuffer)(GL_COPY_WRITE_BUFFER, 0);
        if let Some(buf) = as_buffer((*v).view_resource) {
            // Copie processeur tenue à jour (Map en lecture la relit aussi).
            let mut data = (*buf)
                .staging_data
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let (off, len) = (b.off as usize, b.len as usize);
            if off + len <= data.len() {
                for w in data[off..off + len].chunks_mut(4) {
                    let bytes = words[0].to_le_bytes();
                    w.copy_from_slice(&bytes[..w.len()]);
                }
            }
        }
    } else if b.tex != 0 {
        let integer = matches!(
            b.format,
            0x8D70
                | 0x8D82
                | 0x8D71
                | 0x8D83
                | 0x8D76
                | 0x8D88
                | 0x906F
                | 0x8D7C
                | 0x8D8E
                | 0x823C
                | 0x823B
                | 0x823A
                | 0x8238
                | 0x8236
                | 0x8235
                | 0x8234
                | 0x8232
                | 0x8239
                | 0x8231
                | 0x8233
                | 0x8237
        );
        const GL_RGBA_INTEGER: u32 = 0x8D99;
        if float || !integer {
            let f: [f32; 4] = if float {
                words.map(f32::from_bits)
            } else {
                words.map(|w| w as f32)
            };
            (g.ClearTexImage)(b.tex, b.level, gl::GL_RGBA, gl::GL_FLOAT, f.as_ptr().cast());
        } else {
            (g.ClearTexImage)(
                b.tex,
                b.level,
                GL_RGBA_INTEGER,
                gl::GL_UNSIGNED_INT,
                words.as_ptr().cast(),
            );
        }
    }
}

unsafe extern "win64" fn ctx_clear_uav_uint(
    _: *mut DeviceContext,
    view: *mut c_void,
    values: *const [u32; 4],
) {
    if !values.is_null() {
        clear_uav(view, *values, false);
    }
}

unsafe extern "win64" fn ctx_clear_uav_float(
    _: *mut DeviceContext,
    view: *mut c_void,
    values: *const [f32; 4],
) {
    if !values.is_null() {
        clear_uav(view, (*values).map(f32::to_bits), true);
    }
}
unsafe extern "win64" fn ctx_noop_draw_indirect(_: *mut DeviceContext, _: *mut c_void, _: u32) {}

// Implemented methods
unsafe extern "win64" fn ctx_vs_set_shader(
    this: *mut DeviceContext,
    shader: *mut c_void,
    _class: *const *mut c_void,
    _n: u32,
) {
    pipeline::set_shader(this, 0, shader);
    let mut s = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    s.current_vs = as_simple(shader).map_or(0, |o| (*o).gl_id);
}

unsafe extern "win64" fn ctx_ps_set_shader(
    this: *mut DeviceContext,
    shader: *mut c_void,
    _class: *const *mut c_void,
    _n: u32,
) {
    pipeline::set_shader(this, 1, shader);
    let mut s = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    s.current_ps = as_simple(shader).map_or(0, |o| (*o).gl_id);
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
    let Some(g) = gl_here() else { return };
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
    let Some(g) = gl_here() else { return };
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
    let Some(g) = gl_here() else { return };
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
    let Some(g) = gl_here() else { return };
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
    let Some(g) = gl_here() else { return };
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
    let Some(g) = gl_here() else { return };
    let s = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    let (ty, size) = index_type(s.index_format);
    let offset = (start as usize * size) as *const c_void;
    (g.DrawElementsInstanced)(gl::GL_TRIANGLES, ipc as i32, ty, offset, ic as i32);
}

/// D3D11_MAP_READ et D3D11_MAP_WRITE_DISCARD.
const MAP_READ: u32 = 1;
const MAP_WRITE_DISCARD: u32 = 4;

unsafe extern "win64" fn ctx_map(
    _this: *mut DeviceContext,
    resource: *mut c_void,
    sub: u32,
    map_type: u32,
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
        // Lecture (READ, READ_WRITE) : un compute shader ou une copie GPU a
        // pu écrire le tampon depuis ; la copie processeur est relue.
        if matches!(map_type, 1 | 3) && (*buf).gl_buffer != 0 {
            if let Some(g) = gl_here() {
                const GL_COPY_READ_BUFFER: u32 = 0x8F36;
                (g.BindBuffer)(GL_COPY_READ_BUFFER, (*buf).gl_buffer);
                (g.GetBufferSubData)(
                    GL_COPY_READ_BUFFER,
                    0,
                    guard.len() as isize,
                    guard.as_mut_ptr().cast(),
                );
                (g.BindBuffer)(GL_COPY_READ_BUFFER, 0);
            }
        }
        let ptr = guard.as_mut_ptr();
        (*mapped).data = ptr as *mut c_void;
        (*mapped).row_pitch = (*buf).desc.byte_width;
        (*mapped).depth_pitch = 0;
        return S_OK;
    }
    if let Some(tex) = as_texture2d(resource) {
        // Texture : la sous-ressource est copiée côté processeur (relue
        // sauf WRITE_DISCARD), puis téléversée au Unmap si elle est écrite.
        let desc = (*tex).desc;
        let (level, slice) = subresource(&desc, sub);
        let Some(bytes) = level_bytes(&desc, level) else {
            return E_INVALIDARG;
        };
        let mut guard = (*tex).staging_buf.lock().unwrap_or_else(|e| e.into_inner());
        guard.level = level;
        guard.slice = slice;
        guard.map_type = map_type;
        guard.data.resize(bytes, 0);
        if map_type != MAP_WRITE_DISCARD && (*tex).gl_texture != 0 {
            if let Some(g) = gl_here() {
                read_level(g, &*tex, level, slice, &mut guard.data);
            }
        }
        let (w, _) = level_size(&desc, level);
        (*mapped).data = guard.data.as_mut_ptr() as *mut c_void;
        (*mapped).row_pitch = row_pitch(desc.format, w);
        (*mapped).depth_pitch = bytes as u32;
        return S_OK;
    }
    E_INVALIDARG
}

unsafe extern "win64" fn ctx_unmap(_this: *mut DeviceContext, resource: *mut c_void, _sub: u32) {
    let Some(g) = gl_here() else { return };
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
        let (level, slice) = (guard.level, guard.slice);
        if guard.map_type != MAP_READ
            && (*tex).gl_texture != 0
            && level_bytes(&(*tex).desc, level) == Some(guard.data.len())
        {
            let (w, h) = level_size(&(*tex).desc, level);
            upload_region(
                g,
                &*tex,
                level,
                slice,
                [0, 0, w, h],
                0,
                guard.data.as_ptr().cast(),
            );
        }
    }
}

/// Téléverse un rectangle [x, y, largeur, hauteur] d'un niveau de mip et
/// d'une tranche ; `pitch` = octets par ligne de la source (rangée de blocs
/// pour un format BC ; 0 = lignes jointives).
unsafe fn upload_region(
    g: &gl::GlFunctions,
    tex: &Texture2D,
    level: u32,
    slice: u32,
    rect: [u32; 4],
    pitch: u32,
    data: *const c_void,
) {
    let target = tex.gl_target;
    let array = target == GL_TEXTURE_2D_ARRAY;
    let format = tex.desc.format;
    (g.BindTexture)(target, tex.gl_texture);
    if let Some((internal, _)) = bc_format(format) {
        // Blocs 4x4 : les lignes de blocs sont recopiées jointives si
        // l'appelant en donne un pas plus grand.
        let tight = row_pitch(format, rect[2]) as usize;
        let rows = row_count(format, rect[3]) as usize;
        let pitch = if pitch == 0 { tight } else { pitch as usize };
        let packed: Vec<u8>;
        let src = if pitch == tight {
            data
        } else {
            let mut v = Vec::with_capacity(tight * rows);
            for r in 0..rows {
                let line = std::slice::from_raw_parts((data as *const u8).add(r * pitch), tight);
                v.extend_from_slice(line);
            }
            packed = v;
            packed.as_ptr().cast()
        };
        let size = (tight * rows) as i32;
        if array {
            (g.CompressedTexSubImage3D)(
                target,
                level as i32,
                rect[0] as i32,
                rect[1] as i32,
                slice as i32,
                rect[2] as i32,
                rect[3] as i32,
                1,
                internal,
                size,
                src,
            );
        } else {
            (g.CompressedTexSubImage2D)(
                target,
                level as i32,
                rect[0] as i32,
                rect[1] as i32,
                rect[2] as i32,
                rect[3] as i32,
                internal,
                size,
                src,
            );
        }
    } else {
        let (_, gl_format, gl_type) = dxgi_format_to_gl_bind(format, tex.desc.bind_flags);
        with_unpack_pitch(g, format, pitch, || {
            if array {
                (g.TexSubImage3D)(
                    target,
                    level as i32,
                    rect[0] as i32,
                    rect[1] as i32,
                    slice as i32,
                    rect[2] as i32,
                    rect[3] as i32,
                    1,
                    gl_format,
                    gl_type,
                    data,
                )
            } else {
                (g.TexSubImage2D)(
                    target,
                    level as i32,
                    rect[0] as i32,
                    rect[1] as i32,
                    rect[2] as i32,
                    rect[3] as i32,
                    gl_format,
                    gl_type,
                    data,
                )
            }
        });
    }
    (g.BindTexture)(target, 0);
}

/// Relit un niveau de mip d'une tranche dans `out` (lignes jointives).
unsafe fn read_level(g: &gl::GlFunctions, tex: &Texture2D, level: u32, slice: u32, out: &mut [u8]) {
    const PACK_ALIGNMENT: u32 = 0x0D05;
    if level_bytes(&tex.desc, level) != Some(out.len()) {
        return;
    }
    let (w, h) = level_size(&tex.desc, level);
    if bc_format(tex.desc.format).is_some() {
        (g.GetCompressedTextureSubImage)(
            tex.gl_texture,
            level as i32,
            0,
            0,
            slice as i32,
            w as i32,
            h as i32,
            1,
            out.len() as i32,
            out.as_mut_ptr().cast(),
        );
        return;
    }
    let (_, gl_format, gl_type) = dxgi_format_to_gl_bind(tex.desc.format, tex.desc.bind_flags);
    (g.PixelStorei)(PACK_ALIGNMENT, 1);
    (g.GetTextureSubImage)(
        tex.gl_texture,
        level as i32,
        0,
        0,
        slice as i32,
        w as i32,
        h as i32,
        1,
        gl_format,
        gl_type,
        out.len() as i32,
        out.as_mut_ptr().cast(),
    );
    (g.PixelStorei)(PACK_ALIGNMENT, 4);
}

unsafe extern "win64" fn ctx_om_set_render_targets(
    this: *mut DeviceContext,
    count: u32,
    rtvs: *const *mut c_void,
    dsv: *mut c_void,
) {
    let Some(g) = gl_here() else { return };
    let mut s = (*this).state.lock().unwrap_or_else(|e| e.into_inner());

    s.current_rtv = if count > 0 && !rtvs.is_null() {
        as_simple(*rtvs).unwrap_or(std::ptr::null_mut())
    } else {
        std::ptr::null_mut()
    };
    s.current_dsv = as_simple(dsv).unwrap_or(std::ptr::null_mut());
    if s.current_rtv.is_null() && !s.current_dsv.is_null() && s.depth_fbo == 0 {
        let mut fbo = 0;
        (g.GenFramebuffers)(1, &mut fbo);
        (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, fbo);
        // Aucun tampon couleur : seule la profondeur est écrite.
        let none = 0u32;
        (g.DrawBuffers)(1, &none);
        s.depth_fbo = fbo;
    }
    let fbo = target_fbo(&s);
    (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, fbo);
    if fbo != 0 {
        pipeline::attach_depth(g, s.current_dsv);
    }
}

/// Cadre GL des cibles courantes : celui de la vue couleur, sinon le cadre
/// « profondeur seule » quand seule une vue de profondeur est liée (une
/// passe d'ombre partait dans la fenêtre, #530), sinon la fenêtre.
unsafe fn target_fbo(s: &DeviceContextState) -> gl::GLuint {
    if !s.current_rtv.is_null() {
        (*s.current_rtv).gl_id
    } else if !s.current_dsv.is_null() {
        s.depth_fbo
    } else {
        0
    }
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
    this: *mut DeviceContext,
    view: *mut c_void,
    color: *const [f32; 4],
) {
    let Some(g) = gl_here() else { return };
    if color.is_null() {
        return;
    }
    let Some(obj) = as_simple(view) else { return };
    let c = &*color;
    // Effacement entier : ni ciseaux ni masque d'écriture (règle D3D).
    // La profondeur rattachée au cadre de la vue réduirait l'effacement à
    // l'intersection des tailles : elle est détachée le temps de l'effacer.
    (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, (*obj).gl_id);
    pipeline::attach_depth(g, std::ptr::null_mut());
    pipeline::clear(g, || {
        (g.ColorMask)(1, 1, 1, 1);
        (g.ClearColor)(c[0], c[1], c[2], c[3]);
        (g.Clear)(gl::GL_COLOR_BUFFER_BIT);
    });
    let st = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    if st.current_rtv == obj {
        pipeline::attach_depth(g, st.current_dsv);
    }
    (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, target_fbo(&st));
}

unsafe extern "win64" fn ctx_clear_dsv(
    this: *mut DeviceContext,
    view: *mut c_void,
    flags: u32,
    depth: f32,
    stencil: u8,
) {
    let Some(g) = gl_here() else { return };
    let Some(view) = as_simple(view) else { return };
    // Toujours dans un cadre temporaire sans couleur : dans celui de la cible
    // courante, l'effacement était limité à sa taille (cascade d'ombre
    // 2048x2048 effacée sur 1024x768 seulement, #537).
    let mut temp = 0;
    (g.GenFramebuffers)(1, &mut temp);
    (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, temp);
    let none = 0u32;
    (g.DrawBuffers)(1, &none);
    pipeline::attach_depth(g, view);
    let mut mask = 0u32;
    if flags & 1 != 0 {
        mask |= gl::GL_DEPTH_BUFFER_BIT;
    }
    if flags & 2 != 0 {
        mask |= gl::GL_STENCIL_BUFFER_BIT;
    }
    pipeline::clear(g, || {
        (g.DepthMask)(1);
        (g.StencilMask)(0xFF);
        (g.ClearDepth)(depth as f64);
        (g.ClearStencil)(stencil as i32);
        (g.Clear)(mask);
    });
    let st = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, target_fbo(&st));
    (g.DeleteFramebuffers)(1, &temp);
}

unsafe extern "win64" fn ctx_rs_set_viewports(
    _this: *mut DeviceContext,
    count: u32,
    viewports: *const D3D11Viewport,
) {
    let Some(g) = gl_here() else { return };
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
    dst: *mut c_void,
    dst_sub: u32,
    x: u32,
    y: u32,
    _z: u32,
    src: *mut c_void,
    src_sub: u32,
    box_: *const c_void,
) {
    let box_ = box_ as *const D3D11Box;
    let Some(g) = gl_here() else { return };
    // ANGLE téléverse dans une texture intermédiaire puis la copie ici dans
    // la texture échantillonnée : sans cette copie tout reste noir (#514).
    if let (Some(d), Some(s)) = (as_texture2d(dst), as_texture2d(src)) {
        let (src_level, src_slice) = subresource(&(*s).desc, src_sub);
        let (dst_level, dst_slice) = subresource(&(*d).desc, dst_sub);
        let (sw, sh) = level_size(&(*s).desc, src_level);
        let (dw, dh) = level_size(&(*d).desc, dst_level);
        let (left, top, right, bottom) = if box_.is_null() {
            (0, 0, sw, sh)
        } else {
            let b = *box_;
            (b.left, b.top, b.right.min(sw), b.bottom.min(sh))
        };
        if right <= left || bottom <= top || x >= dw || y >= dh {
            return;
        }
        let w = (right - left).min(dw - x);
        let h = (bottom - top).min(dh - y);
        copy_texture_region(
            g,
            &*s,
            [src_level, left, top, src_slice],
            &*d,
            [dst_level, x, y, dst_slice],
            [w, h, 1],
        );
    } else if let (Some(d), Some(s)) = (as_buffer(dst), as_buffer(src)) {
        let size = (*s).desc.byte_width;
        let (left, right) = if box_.is_null() {
            (0, size)
        } else {
            ((*box_).left, (*box_).right.min(size))
        };
        let room = (*d).desc.byte_width.saturating_sub(x);
        let len = right.saturating_sub(left).min(room);
        if len > 0 {
            copy_buffer_range(g, &*s, left, &*d, x, len);
        }
    }
}

/// Copie GPU entre deux textures (glCopyImageSubData) : source et
/// destination en [niveau, x, y, tranche], taille en [largeur, hauteur,
/// tranches].
unsafe fn copy_texture_region(
    g: &gl::GlFunctions,
    s: &Texture2D,
    from: [u32; 4],
    d: &Texture2D,
    to: [u32; 4],
    size: [u32; 3],
) {
    if s.gl_texture == 0 || d.gl_texture == 0 {
        return;
    }
    (g.CopyImageSubData)(
        s.gl_texture,
        s.gl_target,
        from[0] as i32,
        from[1] as i32,
        from[2] as i32,
        from[3] as i32,
        d.gl_texture,
        d.gl_target,
        to[0] as i32,
        to[1] as i32,
        to[2] as i32,
        to[3] as i32,
        size[0] as i32,
        size[1] as i32,
        size[2] as i32,
    );
}

/// Copie d'une plage d'octets entre deux tampons : sur la carte, et dans
/// leurs copies côté processeur (que Map renvoie).
unsafe fn copy_buffer_range(
    g: &gl::GlFunctions,
    s: &Buffer,
    src_offset: u32,
    d: &Buffer,
    dst_offset: u32,
    len: u32,
) {
    const COPY_READ_BUFFER: u32 = 0x8F36;
    const COPY_WRITE_BUFFER: u32 = 0x8F37;
    (g.BindBuffer)(COPY_READ_BUFFER, s.gl_buffer);
    (g.BindBuffer)(COPY_WRITE_BUFFER, d.gl_buffer);
    (g.CopyBufferSubData)(
        COPY_READ_BUFFER,
        COPY_WRITE_BUFFER,
        src_offset as isize,
        dst_offset as isize,
        len as isize,
    );
    (g.BindBuffer)(COPY_READ_BUFFER, 0);
    (g.BindBuffer)(COPY_WRITE_BUFFER, 0);
    let (so, dof, n) = (src_offset as usize, dst_offset as usize, len as usize);
    if std::ptr::eq(s, d) {
        let mut data = d.staging_data.lock().unwrap_or_else(|e| e.into_inner());
        if so + n <= data.len() && dof + n <= data.len() {
            data.copy_within(so..so + n, dof);
        }
        return;
    }
    let from = s.staging_data.lock().unwrap_or_else(|e| e.into_inner());
    let mut to = d.staging_data.lock().unwrap_or_else(|e| e.into_inner());
    if so + n <= from.len() && dof + n <= to.len() {
        to[dof..dof + n].copy_from_slice(&from[so..so + n]);
    }
}

unsafe extern "win64" fn ctx_copy_resource(
    _: *mut DeviceContext,
    dst: *mut c_void,
    src: *mut c_void,
) {
    let Some(g) = gl_here() else { return };
    // Mêmes dimensions exigées par D3D : chaque niveau commun est copié.
    if let (Some(d), Some(s)) = (as_texture2d(dst), as_texture2d(src)) {
        if (*d).desc.width != (*s).desc.width || (*d).desc.height != (*s).desc.height {
            return;
        }
        let levels = mip_count(&(*s).desc).min(mip_count(&(*d).desc));
        let layers = (*s).desc.array_size.min((*d).desc.array_size).max(1);
        for level in 0..levels {
            let (w, h) = level_size(&(*s).desc, level);
            if (*s).gl_target == (*d).gl_target {
                copy_texture_region(
                    g,
                    &*s,
                    [level, 0, 0, 0],
                    &*d,
                    [level, 0, 0, 0],
                    [w, h, layers],
                );
            } else {
                for slice in 0..layers {
                    copy_texture_region(
                        g,
                        &*s,
                        [level, 0, 0, slice],
                        &*d,
                        [level, 0, 0, slice],
                        [w, h, 1],
                    );
                }
            }
        }
    } else if let (Some(d), Some(s)) = (as_buffer(dst), as_buffer(src)) {
        let len = (*s).desc.byte_width.min((*d).desc.byte_width);
        if len > 0 {
            copy_buffer_range(g, &*s, 0, &*d, 0, len);
        }
    }
}

unsafe extern "win64" fn ctx_update_subresource(
    _this: *mut DeviceContext,
    resource: *mut c_void,
    sub: u32,
    box_: *const c_void,
    data: *const c_void,
    row_pitch: u32,
    _depth_pitch: u32,
) {
    if resource.is_null() || data.is_null() {
        return;
    }
    let box_ = box_ as *const D3D11Box;
    let Some(g) = gl_here() else { return };
    if let Some(buf) = as_buffer(resource) {
        let size = (*buf).desc.byte_width;
        let (left, right) = if box_.is_null() {
            (0, size)
        } else {
            ((*box_).left, (*box_).right.min(size))
        };
        if right <= left {
            return;
        }
        let len = (right - left) as usize;
        let target = bind_flags_to_gl_target((*buf).desc.bind_flags);
        (g.BindBuffer)(target, (*buf).gl_buffer);
        (g.BufferSubData)(target, left as isize, len as isize, data);
        (g.BindBuffer)(target, 0);
        let mut shadow = (*buf)
            .staging_data
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if left as usize + len <= shadow.len() {
            let src = std::slice::from_raw_parts(data as *const u8, len);
            shadow[left as usize..left as usize + len].copy_from_slice(src);
        }
    } else if let Some(tex) = as_texture2d(resource) {
        // Boîte et pas de ligne de l'appelant : seule la zone visée change.
        let (level, slice) = subresource(&(*tex).desc, sub);
        let (w, h) = level_size(&(*tex).desc, level);
        let rect = if box_.is_null() {
            [0, 0, w, h]
        } else {
            let b = *box_;
            let (r, bt) = (b.right.min(w), b.bottom.min(h));
            if r <= b.left || bt <= b.top {
                return;
            }
            [b.left, b.top, r - b.left, bt - b.top]
        };
        if (*tex).gl_texture != 0 {
            upload_region(g, &*tex, level, slice, rect, row_pitch, data);
        }
    }
}

unsafe extern "win64" fn ctx_generate_mips(_: *mut DeviceContext, srv: *mut c_void) {
    let Some(g) = gl_here() else { return };
    let Some(obj) = as_simple(srv) else { return };
    // Sur la texture elle-même (toute sa chaîne), avec sa cible.
    if let Some(t) = as_texture2d((*obj).view_resource) {
        if (*t).gl_texture != 0 {
            (g.BindTexture)((*t).gl_target, (*t).gl_texture);
            (g.GenerateMipmap)((*t).gl_target);
            (g.BindTexture)((*t).gl_target, 0);
        }
    }
}

unsafe extern "win64" fn ctx_clear_state(this: *mut DeviceContext) {
    let Some(g) = gl_here() else { return };
    let mut s = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    *s = DeviceContextState::default();
    (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, 0);
    (g.UseProgram)(0);
}

unsafe extern "win64" fn ctx_flush(_this: *mut DeviceContext) {
    if let Some(g) = gl_here() {
        (g.Flush)();
    }
}

fn update_program(s: &mut DeviceContextState) {
    let Some(g) = gl_here() else { return };

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
    vs_set_constant_buffers: pipeline::vs_set_cbs,
    ps_set_shader_resources: pipeline::ps_set_srvs,
    ps_set_shader: ctx_ps_set_shader,
    ps_set_samplers: pipeline::ps_set_samplers,
    vs_set_shader: ctx_vs_set_shader,
    draw_indexed: pipeline::ctx_draw_indexed,
    draw: pipeline::ctx_draw,
    map: ctx_map,
    unmap: ctx_unmap,
    ps_set_constant_buffers: pipeline::ps_set_cbs,
    ia_set_input_layout: pipeline::ia_set_input_layout,
    ia_set_vertex_buffers: pipeline::ia_set_vertex_buffers,
    ia_set_index_buffer: pipeline::ia_set_index_buffer,
    draw_indexed_instanced: pipeline::ctx_draw_indexed_instanced,
    draw_instanced: pipeline::ctx_draw_instanced,
    gs_set_constant_buffers: pipeline::gs_set_cbs,
    gs_set_shader: pipeline::gs_set_shader,
    ia_set_primitive_topology: ctx_ia_set_topology,
    vs_set_shader_resources: pipeline::vs_set_srvs,
    vs_set_samplers: pipeline::vs_set_samplers,
    begin: ctx_noop_1ptr,
    end: ctx_noop_1ptr,
    get_data: ctx_get_data,
    set_predication: ctx_noop_pred,
    gs_set_shader_resources: pipeline::gs_set_srvs,
    gs_set_samplers: pipeline::gs_set_samplers,
    om_set_render_targets: ctx_om_set_render_targets,
    om_set_render_targets_and_unordered: ctx_om_set_rt_uav,
    om_set_blend_state: pipeline::om_set_blend_state,
    om_set_depth_stencil_state: pipeline::om_set_depth_stencil_state,
    so_set_targets: ctx_noop_so,
    draw_auto: ctx_noop_void,
    draw_indexed_instanced_indirect: ctx_noop_draw_indirect,
    draw_instanced_indirect: ctx_noop_draw_indirect,
    dispatch: pipeline::ctx_dispatch,
    dispatch_indirect: pipeline::ctx_dispatch_indirect,
    rs_set_state: pipeline::rs_set_state,
    rs_set_viewports: pipeline::rs_set_viewports,
    rs_set_scissor_rects: pipeline::rs_set_scissor_rects,
    copy_subresource_region: ctx_copy_subresource,
    copy_resource: ctx_copy_resource,
    update_subresource: ctx_update_subresource,
    copy_structure_count: ctx_noop_copy_struct,
    clear_render_target_view: ctx_clear_rtv,
    clear_unordered_access_view_uint: ctx_clear_uav_uint,
    clear_unordered_access_view_float: ctx_clear_uav_float,
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
    cs_set_shader_resources: pipeline::cs_set_srvs,
    cs_set_unordered_access_views: pipeline::cs_set_uavs,
    cs_set_shader: pipeline::cs_set_shader,
    cs_set_samplers: pipeline::cs_set_samplers,
    cs_set_constant_buffers: pipeline::cs_set_cbs,
    vs_get_constant_buffers: ctx_get_array,
    ps_get_shader_resources: ctx_get_array,
    ps_get_shader: ctx_get_shader,
    ps_get_samplers: ctx_get_array,
    vs_get_shader: ctx_get_shader,
    ps_get_constant_buffers: ctx_get_array,
    ia_get_input_layout: ctx_get_one,
    ia_get_vertex_buffers: ctx_get_vertex_buffers,
    ia_get_index_buffer: ctx_get_index_buffer,
    gs_get_constant_buffers: ctx_get_array,
    gs_get_shader: ctx_get_shader,
    ia_get_primitive_topology: ctx_get_u32,
    vs_get_shader_resources: ctx_get_array,
    vs_get_samplers: ctx_get_array,
    get_predication: ctx_get_predication,
    gs_get_shader_resources: ctx_get_array,
    gs_get_samplers: ctx_get_array,
    om_get_render_targets: ctx_get_render_targets,
    om_get_render_targets_and_uavs: ctx_get_rt_uav,
    om_get_blend_state: ctx_get_blend,
    om_get_depth_stencil_state: ctx_get_ptr_u32,
    so_get_targets: ctx_get_so,
    rs_get_state: ctx_get_one,
    rs_get_viewports: ctx_get_count,
    rs_get_scissor_rects: ctx_get_count,
    hs_get_shader_resources: ctx_get_array,
    hs_get_shader: ctx_get_shader,
    hs_get_samplers: ctx_get_array,
    hs_get_constant_buffers: ctx_get_array,
    ds_get_shader_resources: ctx_get_array,
    ds_get_shader: ctx_get_shader,
    ds_get_samplers: ctx_get_array,
    ds_get_constant_buffers: ctx_get_array,
    cs_get_shader_resources: ctx_get_array,
    cs_get_unordered_access_views: ctx_get_array,
    cs_get_shader: ctx_get_shader,
    cs_get_samplers: ctx_get_array,
    cs_get_constant_buffers: ctx_get_array,
    clear_state: ctx_clear_state,
    flush: ctx_flush,
    get_type_ctx: ctx_get_type,
    get_context_flags: ctx_get_flags,
    finish_command_list: ctx_finish_cl,
    ext: ext::CONTEXT_EXT,
};

fn create_device_context() -> *mut DeviceContext {
    Box::into_raw(Box::new(DeviceContext {
        vtable: &DEVICE_CONTEXT_VTABLE,
        ref_count: AtomicU32::new(1),
        state: Mutex::new(DeviceContextState::default()),
        device: std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()),
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
    /// ID3D11Device1 à 5 (43-68).
    ext: ext::DeviceExt,
}

#[repr(C)]
struct Device {
    vtable: *const DeviceVTable,
    ref_count: AtomicU32,
    context: *mut DeviceContext,
    feature_level: u32,
    /// D3D11_CREATE_DEVICE_FLAG passés à la création (GetCreationFlags).
    flags: u32,
    dxgi: DxgiDevice,
    multithread: ext::Multithread,
}

unsafe impl Send for Device {}

// ────────────────────── IDXGIDevice (vue DXGI de l'appareil) ──────────────────────
//
// ANGLE (Direct3D 11 de Warzone 2100) demande IDXGIDevice2 à l'appareil avant
// de présenter dans une fenêtre, puis remonte à l'adaptateur et à la fabrique.
// Interface détachée logée dans l'appareil : même compteur de références.

#[repr(C)]
struct DxgiDeviceVTable {
    query_interface:
        unsafe extern "win64" fn(*mut DxgiDevice, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut DxgiDevice) -> u32,
    release: unsafe extern "win64" fn(*mut DxgiDevice) -> u32,
    set_private_data:
        unsafe extern "win64" fn(*mut DxgiDevice, *const Guid, u32, *const c_void) -> i32,
    set_private_data_interface:
        unsafe extern "win64" fn(*mut DxgiDevice, *const Guid, *const c_void) -> i32,
    get_private_data:
        unsafe extern "win64" fn(*mut DxgiDevice, *const Guid, *mut u32, *mut c_void) -> i32,
    get_parent: unsafe extern "win64" fn(*mut DxgiDevice, *const Guid, *mut *mut c_void) -> i32,
    get_adapter: unsafe extern "win64" fn(*mut DxgiDevice, *mut *mut c_void) -> i32,
    create_surface: unsafe extern "win64" fn(
        *mut DxgiDevice,
        *const c_void,
        u32,
        u32,
        *const c_void,
        *mut *mut c_void,
    ) -> i32,
    query_resource_residency:
        unsafe extern "win64" fn(*mut DxgiDevice, *const *mut c_void, *mut u32, u32) -> i32,
    set_gpu_thread_priority: unsafe extern "win64" fn(*mut DxgiDevice, i32) -> i32,
    get_gpu_thread_priority: unsafe extern "win64" fn(*mut DxgiDevice, *mut i32) -> i32,
    set_maximum_frame_latency: unsafe extern "win64" fn(*mut DxgiDevice, u32) -> i32,
    get_maximum_frame_latency: unsafe extern "win64" fn(*mut DxgiDevice, *mut u32) -> i32,
    offer_resources: unsafe extern "win64" fn(*mut DxgiDevice, u32, *const *mut c_void, u32) -> i32,
    reclaim_resources:
        unsafe extern "win64" fn(*mut DxgiDevice, u32, *const *mut c_void, *mut i32) -> i32,
    enqueue_set_event: unsafe extern "win64" fn(*mut DxgiDevice, usize) -> i32,
    trim: unsafe extern "win64" fn(*mut DxgiDevice),
    // IDXGIDevice4
    offer_resources1:
        unsafe extern "win64" fn(*mut DxgiDevice, u32, *const *mut c_void, u32, u32) -> i32,
    reclaim_resources1:
        unsafe extern "win64" fn(*mut DxgiDevice, u32, *const *mut c_void, *mut u32) -> i32,
}

#[repr(C)]
struct DxgiDevice {
    vtable: *const DxgiDeviceVTable,
    owner: *mut Device,
    frame_latency: AtomicU32,
}

/// IDXGIDevice, 1, 2, 3, 4 et IDXGIObject, reconnus par leur premier mot.
fn is_dxgi_device_iid(riid: &Guid) -> bool {
    [
        0x95b4_f95f,
        0x54ec_77fa,
        0x77db_970f,
        0x0500_8617,
        0x6007_896c,
        0xaec2_2fb8,
    ]
    .contains(&riid.data1)
}

unsafe extern "win64" fn dxdev_qi(
    this: *mut DxgiDevice,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    unsafe { dev_qi((*this).owner, riid, out) }
}
unsafe extern "win64" fn dxdev_addref(this: *mut DxgiDevice) -> u32 {
    unsafe { dev_addref((*this).owner) }
}
unsafe extern "win64" fn dxdev_release(this: *mut DxgiDevice) -> u32 {
    unsafe { dev_release((*this).owner) }
}
// Données privées partagées avec l'appareil D3D11 : c'est le même objet.
unsafe extern "win64" fn dxdev_set_priv(
    this: *mut DxgiDevice,
    g: *const Guid,
    size: u32,
    data: *const c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::set((*this).owner as *const c_void, g, size, data) }
}
unsafe extern "win64" fn dxdev_set_priv_if(
    this: *mut DxgiDevice,
    g: *const Guid,
    iface: *const c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::set_iface((*this).owner as *const c_void, g, iface) }
}
unsafe extern "win64" fn dxdev_get_priv(
    this: *mut DxgiDevice,
    g: *const Guid,
    size: *mut u32,
    data: *mut c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::get((*this).owner as *const c_void, g, size, data) }
}
unsafe extern "win64" fn dxdev_get_adapter(_: *mut DxgiDevice, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    unsafe { *out = dxgi::new_default_adapter() };
    S_OK
}
unsafe extern "win64" fn dxdev_get_parent(
    this: *mut DxgiDevice,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let mut adapter = std::ptr::null_mut();
    unsafe {
        dxdev_get_adapter(this, &mut adapter);
        let hr = dxgi::adapter_query_interface(adapter, riid, out);
        dxgi::adapter_release(adapter);
        hr
    }
}
unsafe extern "win64" fn dxdev_create_surface(
    _: *mut DxgiDevice,
    _: *const c_void,
    _: u32,
    _: u32,
    _: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        unsafe { *out = std::ptr::null_mut() };
    }
    crate::dlls::d2d1::unsupported_call("IDXGIDevice::CreateSurface")
}
unsafe extern "win64" fn dxdev_residency(
    _: *mut DxgiDevice,
    _: *const *mut c_void,
    status: *mut u32,
    n: u32,
) -> i32 {
    // DXGI_RESIDENCY_FULLY_RESIDENT = 1 : tout est en mémoire vidéo.
    if !status.is_null() {
        for i in 0..n as usize {
            unsafe { *status.add(i) = 1 };
        }
    }
    S_OK
}
unsafe extern "win64" fn dxdev_set_prio(_: *mut DxgiDevice, prio: i32) -> i32 {
    if (-7..=7).contains(&prio) {
        S_OK
    } else {
        E_INVALIDARG
    }
}
unsafe extern "win64" fn dxdev_get_prio(_: *mut DxgiDevice, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    unsafe { *out = 0 };
    S_OK
}
unsafe extern "win64" fn dxdev_set_latency(this: *mut DxgiDevice, n: u32) -> i32 {
    if n > 16 {
        return E_INVALIDARG;
    }
    // 0 rétablit la valeur par défaut de Windows (3 images).
    unsafe {
        (*this)
            .frame_latency
            .store(if n == 0 { 3 } else { n }, Ordering::Relaxed)
    };
    S_OK
}
unsafe extern "win64" fn dxdev_get_latency(this: *mut DxgiDevice, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    unsafe { *out = (*this).frame_latency.load(Ordering::Relaxed) };
    S_OK
}
unsafe extern "win64" fn dxdev_offer(
    _: *mut DxgiDevice,
    _: u32,
    _: *const *mut c_void,
    _: u32,
) -> i32 {
    S_OK
}
unsafe extern "win64" fn dxdev_reclaim(
    _: *mut DxgiDevice,
    n: u32,
    _: *const *mut c_void,
    discarded: *mut i32,
) -> i32 {
    // Rien n'est jamais rendu au système : aucune ressource n'est perdue.
    if !discarded.is_null() {
        for i in 0..n as usize {
            unsafe { *discarded.add(i) = 0 };
        }
    }
    S_OK
}
unsafe extern "win64" fn dxdev_enqueue_set_event(_: *mut DxgiDevice, event: usize) -> i32 {
    // Le rendu est synchrone : le travail en file est déjà terminé.
    if event != 0 {
        crate::dlls::kernel32::set_event(event);
    }
    S_OK
}
unsafe extern "win64" fn dxdev_trim(_: *mut DxgiDevice) {}
unsafe extern "win64" fn dxdev_offer1(
    _: *mut DxgiDevice,
    _: u32,
    _: *const *mut c_void,
    _: u32,
    _: u32,
) -> i32 {
    S_OK
}
unsafe extern "win64" fn dxdev_reclaim1(
    _: *mut DxgiDevice,
    n: u32,
    _: *const *mut c_void,
    results: *mut u32,
) -> i32 {
    // DXGI_RECLAIM_RESOURCE_RESULT_OK (0) : rien n'a été rendu au système.
    if !results.is_null() {
        for i in 0..n as usize {
            unsafe { *results.add(i) = 0 };
        }
    }
    S_OK
}

static DXGI_DEVICE_VTABLE: DxgiDeviceVTable = DxgiDeviceVTable {
    query_interface: dxdev_qi,
    add_ref: dxdev_addref,
    release: dxdev_release,
    set_private_data: dxdev_set_priv,
    set_private_data_interface: dxdev_set_priv_if,
    get_private_data: dxdev_get_priv,
    get_parent: dxdev_get_parent,
    get_adapter: dxdev_get_adapter,
    create_surface: dxdev_create_surface,
    query_resource_residency: dxdev_residency,
    set_gpu_thread_priority: dxdev_set_prio,
    get_gpu_thread_priority: dxdev_get_prio,
    set_maximum_frame_latency: dxdev_set_latency,
    get_maximum_frame_latency: dxdev_get_latency,
    offer_resources: dxdev_offer,
    reclaim_resources: dxdev_reclaim,
    enqueue_set_event: dxdev_enqueue_set_event,
    trim: dxdev_trim,
    offer_resources1: dxdev_offer1,
    reclaim_resources1: dxdev_reclaim1,
};
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
    if *riid == IID_IUNKNOWN || *riid == IID_ID3D11_DEVICE || ext::is_device_n_iid(riid) {
        (*this).ref_count.fetch_add(1, Ordering::Relaxed);
        *out = this as *mut c_void;
        return S_OK;
    }
    if *riid == ext::IID_MULTITHREAD {
        (*this).ref_count.fetch_add(1, Ordering::Relaxed);
        *out = std::ptr::addr_of_mut!((*this).multithread).cast();
        return S_OK;
    }
    if is_dxgi_device_iid(riid) {
        (*this).ref_count.fetch_add(1, Ordering::Relaxed);
        *out = std::ptr::addr_of_mut!((*this).dxgi).cast();
        return S_OK;
    }
    crate::dlls::comobj::log_no_interface("d3d11 (appareil)", riid);
    *out = std::ptr::null_mut();
    E_NOINTERFACE
}
unsafe extern "win64" fn dev_addref(this: *mut Device) -> u32 {
    (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn dev_release(this: *mut Device) -> u32 {
    let prev = (*this).ref_count.fetch_sub(1, Ordering::AcqRel);
    if prev == 1 {
        crate::dlls::dxgi_private::forget(this as *const c_void);
        let _ = LAST_DEVICE.compare_exchange(
            this,
            std::ptr::null_mut(),
            Ordering::AcqRel,
            Ordering::Relaxed,
        );
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
    crate::dlls::d2d1::unsupported_call("ID3D11Device (création non prise en charge)")
}

unsafe extern "win64" fn dev_create_srv(
    _this: *mut Device,
    resource: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let view = Box::into_raw(Box::new(SimpleComObj {
        vtable: &SIMPLE_COM_VTABLE,
        ref_count: AtomicU32::new(1),
        gl_id: 0,
        resource_tex: 0,
        view_resource: std::ptr::null_mut(),
        view_desc: [0; 6],
        view_desc_len: 0,
    }));
    attach_view(view, ViewKind::ShaderResource, resource, desc);
    // Texture vue par la vue : glTextureView (bonne cible, niveaux et
    // tranches), sinon la texture elle-même.
    if let Some(t) = as_texture2d(resource) {
        let gl_view = gl_here().map_or(0, |g| create_texture_view(g, &*t, &(*view).view_desc));
        (*view).resource_tex = if gl_view != 0 {
            gl_view
        } else {
            (*t).gl_texture
        };
    }
    *out = view.cast();
    S_OK
}

unsafe extern "win64" fn dev_create_uav(
    _: *mut Device,
    resource: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    if as_buffer(resource).is_none() && as_texture2d(resource).is_none() {
        *out = std::ptr::null_mut();
        return E_INVALIDARG;
    }
    let tex = as_texture2d(resource).map_or(0, |t| (*t).gl_texture);
    let view = Box::into_raw(Box::new(SimpleComObj {
        vtable: &SIMPLE_COM_VTABLE,
        ref_count: AtomicU32::new(1),
        gl_id: 0,
        resource_tex: tex,
        view_resource: std::ptr::null_mut(),
        view_desc: [0; 6],
        view_desc_len: 0,
    }));
    attach_view(view, ViewKind::UnorderedAccess, resource, desc);
    *out = view.cast();
    S_OK
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
    let rtv = create_render_target_view(resource, _desc);
    if rtv.is_null() {
        return E_FAIL;
    }
    attach_view(rtv, ViewKind::RenderTarget, resource, _desc);
    *out = rtv as *mut c_void;
    S_OK
}

unsafe extern "win64" fn dev_create_dsv(
    _this: *mut Device,
    resource: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let view = Box::into_raw(Box::new(SimpleComObj {
        vtable: &SIMPLE_COM_VTABLE,
        ref_count: AtomicU32::new(1),
        gl_id: 0,
        resource_tex: 0,
        view_resource: std::ptr::null_mut(),
        view_desc: [0; 6],
        view_desc_len: 0,
    }));
    attach_view(view, ViewKind::DepthStencil, resource, desc);
    *out = view.cast();
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
    crate::dlls::d2d1::unsupported_call("ID3D11Device::CreateGeometryShaderWithStreamOutput")
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
    crate::dlls::d2d1::unsupported_call("ID3D11Device::CreateHullShader")
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
    crate::dlls::d2d1::unsupported_call("ID3D11Device::CreateDomainShader")
}
unsafe extern "win64" fn dev_create_cs(
    _: *mut Device,
    bytecode: *const c_void,
    size: usize,
    _: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let s = create_shader_object(bytecode, size, ShaderType::Compute);
    if s.is_null() {
        return E_FAIL;
    }
    *out = s as *mut c_void;
    S_OK
}
unsafe extern "win64" fn dev_create_cl(_: *mut Device, out: *mut *mut c_void) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    crate::dlls::d2d1::unsupported_call("ID3D11Device::CreateClassLinkage")
}

fn create_dummy_com_obj() -> *mut c_void {
    Box::into_raw(Box::new(SimpleComObj {
        vtable: &SIMPLE_COM_VTABLE,
        ref_count: AtomicU32::new(1),
        gl_id: 0,
        resource_tex: 0,
        view_resource: std::ptr::null_mut(),
        view_desc: [0; 6],
        view_desc_len: 0,
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
    crate::dlls::d2d1::unsupported_call("ID3D11Device::CreateDeferredContext")
}
unsafe extern "win64" fn dev_open_shared(
    _: *mut Device,
    _: usize,
    _: *const Guid,
    _: *mut *mut c_void,
) -> i32 {
    crate::dlls::d2d1::unsupported_call("ID3D11Device::OpenSharedResource")
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
    crate::dlls::d2d1::unsupported_call("ID3D11Device::CheckCounter")
}

/// CheckFeatureSupport : la structure demandée est toujours remplie ; ce que
/// la couche OpenGL ne sait pas faire y reste à zéro.
unsafe extern "win64" fn dev_check_feature(
    this: *mut Device,
    feature: u32,
    data: *mut c_void,
    size: u32,
) -> i32 {
    if data.is_null() || size == 0 {
        return E_INVALIDARG;
    }
    // D3D11_FEATURE_FORMAT_SUPPORT : { InFormat, OutFormatSupport }.
    if feature == 2 {
        if size != 8 {
            return E_INVALIDARG;
        }
        let d = data as *mut u32;
        return dev_check_format(this, *d, d.add(1));
    }
    let keep_format = feature == 3 && size >= 4;
    let format = if keep_format {
        *(data as *const u32)
    } else {
        0
    };
    std::ptr::write_bytes(data as *mut u8, 0, size as usize);
    let b = data as *mut i32;
    let n = size as usize / 4;
    let set = |i: usize| {
        if i < n {
            *b.add(i) = 1;
        }
    };
    match feature {
        // D3D11_FEATURE_THREADING : créations concurrentes, pas de listes.
        0 => set(0),
        // D3D10_X_HARDWARE_OPTIONS : tampons bruts et structurés en calcul.
        4 => set(0),
        // D3D11_OPTIONS : ignorés/indications (Discard, Update/Copy), ClearView,
        // mise à jour partielle et décalage des tampons de constantes,
        // NO_OVERWRITE sur tampons de constantes dynamiques.
        5 => {
            for i in [2, 3, 4, 6, 7, 8] {
                set(i);
            }
        }
        // FORMAT_SUPPORT2 : rien d'atomique.
        3 => *(data as *mut u32) = format,
        _ => {}
    }
    S_OK
}

unsafe extern "win64" fn dev_get_priv(
    this: *mut Device,
    g: *const Guid,
    size: *mut u32,
    data: *mut c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::get(this as *const c_void, g, size, data) }
}
unsafe extern "win64" fn dev_set_priv(
    this: *mut Device,
    g: *const Guid,
    size: u32,
    data: *const c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::set(this as *const c_void, g, size, data) }
}
unsafe extern "win64" fn dev_set_priv_iface(
    this: *mut Device,
    g: *const Guid,
    iface: *const c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::set_iface(this as *const c_void, g, iface) }
}

unsafe extern "win64" fn dev_get_feature_level(this: *mut Device) -> u32 {
    (*this).feature_level
}
unsafe extern "win64" fn dev_get_creation_flags(this: *mut Device) -> u32 {
    (*this).flags
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
    create_blend_state: pipeline::dev_create_blend,
    create_depth_stencil_state: pipeline::dev_create_depth,
    create_rasterizer_state: pipeline::dev_create_raster,
    create_sampler_state: pipeline::dev_create_sampler,
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
    ext: ext::DEVICE_EXT,
};

/// Premier niveau demandé que la couche OpenGL sait tenir (11_1 au plus) ;
/// une liste qui n'en offre aucun garde son plus bas niveau.
pub(super) fn pick_feature_level(levels: &[u32]) -> u32 {
    levels
        .iter()
        .copied()
        .find(|&l| l <= D3D_FEATURE_LEVEL_11_1)
        .or_else(|| levels.iter().copied().min())
        .unwrap_or(D3D_FEATURE_LEVEL_11_0)
}

// ────────────────────── Entry points ──────────────────────

pub unsafe extern "win64" fn D3D11CreateDevice(
    _adapter: usize,
    _driver_type: u32,
    _software: usize,
    flags: u32,
    feature_levels: *const u32,
    num_levels: u32,
    _sdk_version: u32,
    device: *mut usize,
    feature_level: *mut u32,
    context: *mut usize,
) -> i32 {
    let fl = if !feature_levels.is_null() && num_levels > 0 {
        pick_feature_level(std::slice::from_raw_parts(
            feature_levels,
            num_levels as usize,
        ))
    } else {
        D3D_FEATURE_LEVEL_11_0
    };

    dxgi::ensure_offscreen_context();
    let ctx = create_device_context();
    let dev = Box::into_raw(Box::new(Device {
        vtable: &DEVICE_VTABLE,
        ref_count: AtomicU32::new(1),
        context: ctx,
        feature_level: fl,
        flags,
        dxgi: DxgiDevice {
            vtable: &DXGI_DEVICE_VTABLE,
            owner: std::ptr::null_mut(),
            frame_latency: AtomicU32::new(3),
        },
        multithread: ext::Multithread::new(),
    }));
    (*dev).dxgi.owner = dev;
    (*dev).multithread.set_owner(dev);
    (*ctx).device.store(dev, Ordering::Release);
    LAST_DEVICE.store(dev, Ordering::Release);

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

// ────────────────────── Surfaces DXGI pour Direct2D 1.1 ──────────────────────

/// Taille d'une texture 2D Winlux de format 8 bits BGRA ou RGBA, que
/// Direct2D peut prendre pour cible (CreateBitmapFromDxgiSurface).
pub(crate) unsafe fn d2d_surface_size(p: *mut c_void) -> Option<(u32, u32)> {
    let tex = as_texture2d(p)?;
    let d = &(*tex).desc;
    matches!(
        d.format,
        DXGI_FORMAT_B8G8R8A8_UNORM | 91 | DXGI_FORMAT_R8G8B8A8_UNORM | 29
    )
    .then_some((d.width, d.height))
}

/// Pixels BGRA prémultipliés de Direct2D → niveau 0 de la texture.
pub(crate) unsafe fn d2d_surface_write(p: *mut c_void, px: &[u8]) {
    let (Some(tex), Some(g)) = (as_texture2d(p), gl_here()) else {
        return;
    };
    let d = (*tex).desc;
    if px.len() != d.width as usize * d.height as usize * 4 {
        return;
    }
    let rgba = matches!(d.format, DXGI_FORMAT_R8G8B8A8_UNORM | 29);
    let fmt = if rgba { gl::GL_RGBA } else { gl::GL_BGRA };
    (g.BindTexture)(gl::GL_TEXTURE_2D, (*tex).gl_texture);
    (g.PixelStorei)(0x0CF5, 1); // GL_UNPACK_ALIGNMENT
    (g.TexSubImage2D)(
        gl::GL_TEXTURE_2D,
        0,
        0,
        0,
        d.width as i32,
        d.height as i32,
        fmt,
        gl::GL_UNSIGNED_BYTE,
        px.as_ptr().cast(),
    );
    (g.PixelStorei)(0x0CF5, 4);
}

/// Niveau 0 de la texture → pixels BGRA (relu à BeginDraw).
pub(crate) unsafe fn d2d_surface_read(p: *mut c_void, px: &mut [u8]) {
    let (Some(tex), Some(g)) = (as_texture2d(p), gl_here()) else {
        return;
    };
    read_level(g, &*tex, 0, 0, px);
    if matches!((*tex).desc.format, DXGI_FORMAT_R8G8B8A8_UNORM | 29) {
        for c in px.as_chunks_mut::<4>().0 {
            c.swap(0, 2);
        }
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "D3D11CreateDevice" => fn D3D11CreateDevice,
        "D3D11CreateDeviceAndSwapChain" => fn D3D11CreateDeviceAndSwapChain,
    }
}

#[cfg(test)]
mod vtable_tests {
    use super::DeviceContextVTable;

    #[test]
    fn device_context_vtable_matches_windows_order() {
        // ID3D11DeviceContext : ClearState est la méthode 110 (ANGLE de Warzone 2100).
        assert_eq!(
            core::mem::offset_of!(DeviceContextVTable, clear_state),
            110 * 8
        );
        assert_eq!(
            core::mem::offset_of!(DeviceContextVTable, finish_command_list),
            114 * 8
        );
        // ID3D11DeviceContext1 à 4 : 34 méthodes de plus (115 à 148).
        assert_eq!(core::mem::size_of::<DeviceContextVTable>(), 149 * 8);
    }

    #[test]
    fn dxgi_device_vtable_matches_windows_order() {
        use super::{is_dxgi_device_iid, DxgiDeviceVTable};
        assert_eq!(core::mem::offset_of!(DxgiDeviceVTable, get_adapter), 7 * 8);
        assert_eq!(
            core::mem::offset_of!(DxgiDeviceVTable, set_maximum_frame_latency),
            12 * 8
        );
        assert_eq!(
            core::mem::offset_of!(DxgiDeviceVTable, enqueue_set_event),
            16 * 8
        );
        // IDXGIDevice4 : OfferResources1 et ReclaimResources1 après Trim (18).
        assert_eq!(
            core::mem::offset_of!(DxgiDeviceVTable, reclaim_resources1),
            19 * 8
        );
        let mut g = super::IID_ID3D11_DEVICE;
        assert!(!is_dxgi_device_iid(&g));
        g.data1 = 0x0500_8617; // IDXGIDevice2, exigé par ANGLE
        assert!(is_dxgi_device_iid(&g));
    }

    #[test]
    fn simple_objects_implement_device_child() {
        // Vues, shaders, états : IUnknown + ID3D11DeviceChild + GetResource/GetDesc.
        use super::SimpleComVTable;
        assert_eq!(
            core::mem::offset_of!(SimpleComVTable, set_private_data),
            5 * 8
        );
        assert_eq!(core::mem::size_of::<SimpleComVTable>(), 9 * 8);
    }

    #[test]
    fn view_get_resource_and_desc() {
        use super::*;
        unsafe {
            // Une SRV sur une SRV-ressource factice : GetResource rend la
            // ressource avec une référence de plus, Release la rend.
            let res = create_dummy_com_obj() as *mut SimpleComObj;
            let view = create_dummy_com_obj() as *mut SimpleComObj;
            attach_view(view, ViewKind::ShaderResource, res.cast(), std::ptr::null());
            assert_eq!((*res).ref_count.load(Ordering::Relaxed), 2);
            let mut got = std::ptr::null_mut();
            view_get_resource(view, &mut got);
            assert_eq!(got, res.cast());
            assert_eq!(com_release(got), 2);
            let mut desc = [0xffu8; 24];
            view_get_desc(view, desc.as_mut_ptr().cast());
            assert_eq!(desc[4], 1); // pas une texture : D3D11_SRV_DIMENSION_BUFFER
            assert!(as_simple(view.cast()).is_some());
            assert_eq!(SimpleComObj::release(view), 0);
            assert_eq!((*res).ref_count.load(Ordering::Relaxed), 1);
            assert_eq!(SimpleComObj::release(res), 0);
        }
    }
}
