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

/// Couleur d'accent de la session, en 0xAARRGGBB : le violet Prisme
/// (#6A3DF0). Notepad++ 8.9 l'interroge au démarrage.
pub unsafe extern "win64" fn DwmGetColorizationColor(color: *mut u32, opaque: *mut i32) -> i32 {
    const E_INVALIDARG: i32 = 0x8007_0057_u32 as i32;
    if color.is_null() || opaque.is_null() {
        return E_INVALIDARG;
    }
    *color = 0xC46A_3DF0;
    *opaque = 0;
    0
}
pub extern "win64" fn DwmEnableComposition(_action: u32) -> i32 {
    0
}
/// Sans composition, Windows répond DWM_E_COMPOSITIONDISABLED.
pub unsafe extern "win64" fn DwmGetCompositionTimingInfo(_hwnd: usize, _info: usize) -> i32 {
    0x8026_3001_u32 as i32
}
pub extern "win64" fn DwmInvalidateIconicBitmaps(_hwnd: usize) -> i32 {
    0
}
pub unsafe extern "win64" fn DwmSetIconicThumbnail(_hwnd: usize, _bmp: usize, _flags: u32) -> i32 {
    0
}
pub unsafe extern "win64" fn DwmSetIconicLivePreviewBitmap(
    _hwnd: usize,
    _bmp: usize,
    _pt: usize,
    _flags: u32,
) -> i32 {
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
        "DwmGetColorizationColor" => fn DwmGetColorizationColor,
        "DwmEnableComposition" => fn DwmEnableComposition,
        "DwmGetCompositionTimingInfo" => fn DwmGetCompositionTimingInfo,
        "DwmInvalidateIconicBitmaps" => fn DwmInvalidateIconicBitmaps,
        "DwmSetIconicThumbnail" => fn DwmSetIconicThumbnail,
        "DwmSetIconicLivePreviewBitmap" => fn DwmSetIconicLivePreviewBitmap,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn couleur_d_accent_violet_prisme() {
        let (mut color, mut opaque) = (0u32, 1i32);
        unsafe {
            assert_eq!(DwmGetColorizationColor(&mut color, &mut opaque), 0);
            assert_ne!(
                DwmGetColorizationColor(std::ptr::null_mut(), &mut opaque),
                0
            );
        }
        assert_eq!(color & 0x00FF_FFFF, 0x006A_3DF0);
        assert_eq!(opaque, 0);
        assert!(crate::dlls::lookup("dwmapi", "DwmGetColorizationColor").is_some());
    }
}
