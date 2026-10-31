//! Stubs pour dxgi.dll — les apps Electron tentent d'énumérer les GPU.
//! On renvoie E_NOTIMPL pour forcer le rendu logiciel (software rendering).
#![allow(non_snake_case)]

const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;

pub unsafe extern "win64" fn CreateDXGIFactory(_riid: *const u8, _out: *mut usize) -> i32 {
    E_NOTIMPL
}
pub unsafe extern "win64" fn CreateDXGIFactory1(_riid: *const u8, _out: *mut usize) -> i32 {
    E_NOTIMPL
}
pub unsafe extern "win64" fn CreateDXGIFactory2(
    _flags: u32, _riid: *const u8, _out: *mut usize,
) -> i32 {
    E_NOTIMPL
}
pub unsafe extern "win64" fn DXGIGetDebugInterface1(
    _flags: u32, _riid: *const u8, _out: *mut usize,
) -> i32 {
    E_NOTIMPL
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CreateDXGIFactory" => fn CreateDXGIFactory,
        "CreateDXGIFactory1" => fn CreateDXGIFactory1,
        "CreateDXGIFactory2" => fn CreateDXGIFactory2,
        "DXGIGetDebugInterface1" => fn DXGIGetDebugInterface1,
    }
}
