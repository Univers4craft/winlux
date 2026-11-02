//! d3dcompiler_47 minimal : `D3DCompile` ne compile pas le HLSL, il l'emballe
//! dans un conteneur DXBC (chunk SHEX portant le type de shader déduit de la
//! cible vs_/ps_/gs_/cs_, plus un chunk « HLSL » contenant le texte source).
//! La couche D3D11 reconnaît ainsi le type de shader ; le texte reste
//! disponible pour une vraie traduction HLSL→GLSL. Sert à ANGLE (moteur
//! D3D11 de libGLESv2), qui charge d3dcompiler_47.dll à l'initialisation.
#![allow(non_snake_case)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::dlls::ole32::Guid;

const S_OK: i32 = 0;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;

#[repr(C)]
struct BlobVTable {
    query_interface: unsafe extern "win64" fn(*mut Blob, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut Blob) -> u32,
    release: unsafe extern "win64" fn(*mut Blob) -> u32,
    get_buffer_pointer: unsafe extern "win64" fn(*mut Blob) -> *mut c_void,
    get_buffer_size: unsafe extern "win64" fn(*mut Blob) -> usize,
}

#[repr(C)]
struct Blob {
    vtable: *const BlobVTable,
    ref_count: AtomicU32,
    data: Vec<u8>,
}

/// IUnknown et ID3DBlob (ID3D10Blob, 8ba5fb08-5195-40e2-ac58-0d989c3a0102).
unsafe extern "win64" fn blob_qi(this: *mut Blob, riid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() || riid.is_null() {
        return E_INVALIDARG;
    }
    let d1 = unsafe { (*riid).data1 };
    if d1 == 0 || d1 == 0x8ba5_fb08 {
        unsafe {
            (*this).ref_count.fetch_add(1, Ordering::Relaxed);
            *out = this.cast();
        }
        return S_OK;
    }
    unsafe { *out = std::ptr::null_mut() };
    E_NOINTERFACE
}
unsafe extern "win64" fn blob_addref(this: *mut Blob) -> u32 {
    unsafe { (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1 }
}
unsafe extern "win64" fn blob_release(this: *mut Blob) -> u32 {
    let prev = unsafe { (*this).ref_count.fetch_sub(1, Ordering::AcqRel) };
    if prev == 1 {
        drop(unsafe { Box::from_raw(this) });
        return 0;
    }
    prev - 1
}
unsafe extern "win64" fn blob_ptr(this: *mut Blob) -> *mut c_void {
    unsafe { (*this).data.as_mut_ptr().cast() }
}
unsafe extern "win64" fn blob_size(this: *mut Blob) -> usize {
    unsafe { (*this).data.len() }
}

static BLOB_VTABLE: BlobVTable = BlobVTable {
    query_interface: blob_qi,
    add_ref: blob_addref,
    release: blob_release,
    get_buffer_pointer: blob_ptr,
    get_buffer_size: blob_size,
};

fn new_blob(data: Vec<u8>) -> *mut c_void {
    Box::into_raw(Box::new(Blob {
        vtable: &BLOB_VTABLE,
        ref_count: AtomicU32::new(1),
        data,
    }))
    .cast()
}

/// Type de programme DXBC (bits 16..31 du jeton de version) d'après la cible.
fn program_type(target: &str) -> u32 {
    match target.get(..2) {
        Some("vs") => 1,
        Some("gs") => 2,
        Some("hs") => 3,
        Some("ds") => 4,
        Some("cs") => 5,
        _ => 0, // ps et inconnu
    }
}

/// Conteneur DXBC : en-tête, chunk SHEX (jeton de version seul) et chunk HLSL.
fn dxbc_container(target: &str, source: &[u8]) -> Vec<u8> {
    let version = (program_type(target) << 16) | 0x50;
    let shex: Vec<u8> = version.to_le_bytes().to_vec();
    let chunks: [(&[u8; 4], &[u8]); 2] = [(b"SHEX", &shex), (b"HLSL", source)];
    let header_len = 32 + 4 * chunks.len();
    let mut offsets = Vec::new();
    let mut body = Vec::new();
    for (fourcc, data) in chunks {
        offsets.push((header_len + body.len()) as u32);
        body.extend_from_slice(fourcc);
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
    }
    let total = header_len + body.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"DXBC");
    out.extend_from_slice(&[0u8; 16]); // somme de contrôle non calculée
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
    for o in offsets {
        out.extend_from_slice(&o.to_le_bytes());
    }
    out.extend_from_slice(&body);
    out
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn d3d_compile(
    src: *const c_void,
    src_size: usize,
    _source_name: *const u8,
    _defines: *const c_void,
    _include: *const c_void,
    _entry: *const u8,
    target: *const u8,
    _flags1: u32,
    _flags2: u32,
    code: *mut *mut c_void,
    errors: *mut *mut c_void,
) -> i32 {
    if !errors.is_null() {
        unsafe { *errors = std::ptr::null_mut() };
    }
    if code.is_null() || (src.is_null() && src_size != 0) {
        return E_INVALIDARG;
    }
    let source = if src.is_null() {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(src.cast::<u8>(), src_size) }
    };
    let target = if target.is_null() {
        String::new()
    } else {
        unsafe { crate::dlls::cstr(target) }
    };
    crate::trace!("D3DCompile(cible {target}, {src_size} octets de HLSL)");
    unsafe { *code = new_blob(dxbc_container(&target, source)) };
    S_OK
}

unsafe extern "win64" fn d3d_create_blob(size: usize, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    unsafe { *out = new_blob(vec![0; size]) };
    S_OK
}

unsafe extern "win64" fn d3d_not_impl() -> i32 {
    E_NOTIMPL
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "D3DCompile"=>fn d3d_compile,
        "D3DCompile2"=>fn d3d_not_impl,
        "D3DCreateBlob"=>fn d3d_create_blob,
        "D3DDisassemble"=>fn d3d_not_impl,
        "D3DReflect"=>fn d3d_not_impl,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conteneur_dxbc_porte_le_type_de_shader() {
        let v = dxbc_container("vs_5_0", b"float4 main() : SV_Position { return 0; }");
        let t = crate::dlls::d3d_shader::translate_dxbc(&v).expect("DXBC lisible");
        assert_eq!(t.shader_type, crate::dlls::d3d_shader::ShaderType::Vertex);
        let p = dxbc_container("ps_5_0", b"");
        let t = crate::dlls::d3d_shader::translate_dxbc(&p).expect("DXBC lisible");
        assert_eq!(t.shader_type, crate::dlls::d3d_shader::ShaderType::Pixel);
    }
}
