//! Stubs pour d3d11.dll — Electron tente de créer un device D3D11.
//! On renvoie DXGI_ERROR_NOT_FOUND pour forcer le fallback logiciel.
#![allow(non_snake_case)]

const DXGI_ERROR_NOT_FOUND: i32 = 0x887A_0002u32 as i32;

pub unsafe extern "win64" fn D3D11CreateDevice(
    _adapter: usize, _driver_type: u32, _software: usize, _flags: u32,
    _feature_levels: *const u32, _num_levels: u32, _sdk_version: u32,
    _device: *mut usize, _feature_level: *mut u32, _context: *mut usize,
) -> i32 {
    DXGI_ERROR_NOT_FOUND
}

pub unsafe extern "win64" fn D3D11CreateDeviceAndSwapChain(
    _adapter: usize, _driver_type: u32, _software: usize, _flags: u32,
    _feature_levels: *const u32, _num_levels: u32, _sdk_version: u32,
    _swap_chain_desc: usize, _swap_chain: *mut usize,
    _device: *mut usize, _feature_level: *mut u32, _context: *mut usize,
) -> i32 {
    DXGI_ERROR_NOT_FOUND
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "D3D11CreateDevice" => fn D3D11CreateDevice,
        "D3D11CreateDeviceAndSwapChain" => fn D3D11CreateDeviceAndSwapChain,
    }
}
