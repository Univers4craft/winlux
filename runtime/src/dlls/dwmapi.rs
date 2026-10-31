//! Stubs pour dwmapi.dll — Desktop Window Manager.
//! Electron interroge DWM pour la composition et les effets visuels.
#![allow(non_snake_case)]

pub unsafe extern "win64" fn DwmIsCompositionEnabled(enabled: *mut i32) -> i32 {
    if !enabled.is_null() {
        *enabled = 0;
    }
    0
}
pub unsafe extern "win64" fn DwmSetWindowAttribute(
    _hwnd: usize,
    _attr: u32,
    _value: usize,
    _size: u32,
) -> i32 {
    0
}
pub unsafe extern "win64" fn DwmGetWindowAttribute(
    _hwnd: usize,
    _attr: u32,
    _value: usize,
    _size: u32,
) -> i32 {
    0
}
pub unsafe extern "win64" fn DwmExtendFrameIntoClientArea(_hwnd: usize, _margins: usize) -> i32 {
    0
}
pub unsafe extern "win64" fn DwmEnableBlurBehindWindow(_hwnd: usize, _blur: usize) -> i32 {
    0
}
pub extern "win64" fn DwmFlush() -> i32 {
    0
}
pub unsafe extern "win64" fn DwmDefWindowProc(
    _hwnd: usize,
    _msg: u32,
    _wp: usize,
    _lp: usize,
    result: *mut usize,
) -> i32 {
    if !result.is_null() {
        *result = 0;
    }
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "DwmIsCompositionEnabled" => fn DwmIsCompositionEnabled,
        "DwmSetWindowAttribute" => fn DwmSetWindowAttribute,
        "DwmGetWindowAttribute" => fn DwmGetWindowAttribute,
        "DwmExtendFrameIntoClientArea" => fn DwmExtendFrameIntoClientArea,
        "DwmEnableBlurBehindWindow" => fn DwmEnableBlurBehindWindow,
        "DwmFlush" => fn DwmFlush,
        "DwmDefWindowProc" => fn DwmDefWindowProc,
    }
}
