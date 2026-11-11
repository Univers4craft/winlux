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
const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057_u32 as i32;
const E_HANDLE: i32 = 0x8007_0006_u32 as i32;
/// Attributs de dwmapi.h (SDK Windows public).
const DWMWA_NCRENDERING_ENABLED: u32 = 1;
const DWMWA_CAPTION_BUTTON_BOUNDS: u32 = 5;
const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;
const DWMWA_CLOAKED: u32 = 14;

/// DwmGetWindowAttribute : la valeur demandée est toujours écrite.
/// Elle rendait S_OK sans rien écrire : l'appelant lisait sa propre
/// mémoire, non initialisée. Le suivi d'occultation de Chromium y lit
/// DWMWA_CLOAKED pour chaque fenêtre ; une valeur au hasard faisait juger
/// la fenêtre principale de Discord masquée par moments, et elle ne se
/// redessinait plus (#1969, #1886).
pub unsafe extern "win64" fn DwmGetWindowAttribute(
    hwnd: usize,
    attr: u32,
    value: usize,
    size: u32,
) -> i32 {
    if value == 0 {
        return E_INVALIDARG;
    }
    let out = value as *mut u8;
    match attr {
        // Composition annoncée désactivée (DwmIsCompositionEnabled), et
        // aucune fenêtre n'est jamais masquée par DWM.
        DWMWA_NCRENDERING_ENABLED | DWMWA_CLOAKED => {
            if size != 4 {
                return E_INVALIDARG;
            }
            out.cast::<u32>().write_unaligned(0);
        }
        DWMWA_EXTENDED_FRAME_BOUNDS | DWMWA_CAPTION_BUTTON_BOUNDS => {
            if size != 16 {
                return E_INVALIDARG;
            }
            std::ptr::write_bytes(out, 0, 16);
            // Sans cadre DWM, les limites étendues sont celles de GetWindowRect.
            if attr == DWMWA_EXTENDED_FRAME_BOUNDS
                && crate::dlls::user32::get_window_rect(hwnd, out.cast()) == 0
            {
                return E_HANDLE;
            }
        }
        // Autres attributs : valeur nulle, de la taille demandée.
        _ => std::ptr::write_bytes(out, 0, size as usize),
    }
    S_OK
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
        // Ordinal d'après dwmapi.lib du SDK Windows public (#1606).
        "#102" => fn DwmEnableComposition,
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

    /// #1969 : la valeur est toujours écrite, jamais laissée à l'appelant.
    #[test]
    fn attribut_de_fenetre_toujours_ecrit() {
        unsafe {
            let mut cloaked: u32 = 0xdead_beef;
            assert_eq!(
                DwmGetWindowAttribute(0x1234, DWMWA_CLOAKED, &mut cloaked as *mut u32 as usize, 4),
                S_OK
            );
            assert_eq!(cloaked, 0);
            assert_eq!(
                DwmGetWindowAttribute(0x1234, DWMWA_CLOAKED, &mut cloaked as *mut u32 as usize, 8),
                E_INVALIDARG
            );
            assert_eq!(
                DwmGetWindowAttribute(0x1234, DWMWA_CLOAKED, 0, 4),
                E_INVALIDARG
            );
            let mut other = [0xffu8; 8];
            assert_eq!(
                DwmGetWindowAttribute(0x1234, 20, other.as_mut_ptr() as usize, 8),
                S_OK
            );
            assert_eq!(other, [0u8; 8]);
            let mut rect = [7i32; 4];
            assert_eq!(
                DwmGetWindowAttribute(
                    0,
                    DWMWA_EXTENDED_FRAME_BOUNDS,
                    rect.as_mut_ptr() as usize,
                    16
                ),
                E_HANDLE
            );
        }
    }
}
