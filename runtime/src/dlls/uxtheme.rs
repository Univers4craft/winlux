//! uxtheme.dll — thèmes visuels.
//!
//! Les styles visuels de la session sont actifs, comme sur tout Windows
//! 10/11 (`IsThemeActive` vrai, thème « aero » en `NormalColor`) : WPF
//! choisit d'après eux son thème Aero2, au lieu du thème « classique »
//! peint avec les couleurs système (#847). Les programmes Win32, eux,
//! gardent le rendu classique : `IsAppThemed` reste faux et `OpenThemeData`
//! ne rend aucun thème, ce que la documentation prévoit pour une fenêtre
//! sans styles visuels (Qt, wxWidgets, la VCL et MFC testent les deux).
#![allow(non_snake_case)]

const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_NOT_SUFFICIENT_BUFFER: i32 = 0x8007_007Au32 as i32;

pub extern "win64" fn IsThemeActive() -> i32 {
    1
}
pub extern "win64" fn IsAppThemed() -> i32 {
    0
}

/// Fichier, couleur et taille du thème de la session.
pub const THEME_FILE: &str = r"C:\Windows\resources\Themes\aero\aero.msstyles";
pub const THEME_COLOR: &str = "NormalColor";
pub const THEME_SIZE: &str = "NormalSize";

/// GetCurrentThemeName(pszThemeFileName, cchMax, pszColorBuff, cchMax,
/// pszSizeBuff, cchMax) : chaque tampon non nul reçoit sa chaîne terminée.
pub unsafe extern "win64" fn GetCurrentThemeName(
    name: *mut u16,
    name_len: i32,
    color: *mut u16,
    color_len: i32,
    size: *mut u16,
    size_len: i32,
) -> i32 {
    let mut hr = 0;
    for (p, n, v) in [
        (name, name_len, THEME_FILE),
        (color, color_len, THEME_COLOR),
        (size, size_len, THEME_SIZE),
    ] {
        if p.is_null() {
            continue;
        }
        if n <= 0 {
            hr = E_INVALIDARG;
            continue;
        }
        let w: Vec<u16> = v.encode_utf16().collect();
        let k = w.len().min(n as usize - 1);
        std::ptr::copy_nonoverlapping(w.as_ptr(), p, k);
        *p.add(k) = 0;
        if k < w.len() {
            hr = E_NOT_SUFFICIENT_BUFFER;
        }
    }
    hr
}
pub unsafe extern "win64" fn OpenThemeData(_hwnd: usize, _cls: *const u16) -> usize {
    0
}
pub extern "win64" fn CloseThemeData(_theme: usize) -> i32 {
    0
}
pub unsafe extern "win64" fn DrawThemeBackground(
    _theme: usize,
    _hdc: usize,
    _part: i32,
    _state: i32,
    _rect: usize,
    _clip: usize,
) -> i32 {
    E_NOTIMPL
}
pub unsafe extern "win64" fn GetThemeColor(
    _theme: usize,
    _part: i32,
    _state: i32,
    _prop: i32,
    _color: *mut u32,
) -> i32 {
    E_NOTIMPL
}
pub unsafe extern "win64" fn GetThemeSysColor(_theme: usize, _id: i32) -> u32 {
    0
}
/// Les fonds de thème de Winlux sont opaques.
pub unsafe extern "win64" fn IsThemeBackgroundPartiallyTransparent(
    _theme: usize,
    _part: i32,
    _state: i32,
) -> i32 {
    0
}
pub unsafe extern "win64" fn IsThemePartDefined(_theme: usize, _part: i32, _state: i32) -> i32 {
    0
}
pub unsafe extern "win64" fn SetWindowTheme(
    _hwnd: usize,
    _app: *const u16,
    _idlist: *const u16,
) -> i32 {
    0
}
pub unsafe extern "win64" fn GetThemePartSize(
    _theme: usize,
    _hdc: usize,
    _part: i32,
    _state: i32,
    _rect: usize,
    _type: u32,
    _size: usize,
) -> i32 {
    E_NOTIMPL
}
pub unsafe extern "win64" fn DrawThemeText(
    _theme: usize,
    _hdc: usize,
    _part: i32,
    _state: i32,
    _text: *const u16,
    _cch: i32,
    _flags: u32,
    _flags2: u32,
    _rect: usize,
) -> i32 {
    E_NOTIMPL
}
pub extern "win64" fn BufferedPaintInit() -> i32 {
    0
}
pub extern "win64" fn BufferedPaintUnInit() -> i32 {
    0
}
pub unsafe extern "win64" fn BeginBufferedPaint(
    _target: usize,
    _rect: usize,
    _format: u32,
    _params: usize,
    _bits: *mut usize,
) -> usize {
    0
}
pub extern "win64" fn EndBufferedPaint(_h: usize, _update: i32) -> i32 {
    0
}
pub unsafe extern "win64" fn GetThemeFont(
    _theme: usize,
    _hdc: usize,
    _part: i32,
    _state: i32,
    _prop: i32,
    _font: usize,
) -> i32 {
    E_NOTIMPL
}
pub unsafe extern "win64" fn OpenThemeDataEx(_hwnd: usize, _cls: *const u16, _flags: u32) -> usize {
    0
}
pub unsafe extern "win64" fn GetThemeSysFont(_theme: usize, _id: i32, _font: usize) -> i32 {
    E_NOTIMPL
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "IsThemeActive" => fn IsThemeActive,
        "IsAppThemed" => fn IsAppThemed,
        "GetCurrentThemeName" => fn GetCurrentThemeName,
        "OpenThemeData" => fn OpenThemeData,
        "OpenThemeDataEx" => fn OpenThemeDataEx,
        "CloseThemeData" => fn CloseThemeData,
        "DrawThemeBackground" => fn DrawThemeBackground,
        "GetThemeColor" => fn GetThemeColor,
        "GetThemeSysColor" => fn GetThemeSysColor,
        "GetThemeSysFont" => fn GetThemeSysFont,
        "IsThemePartDefined" => fn IsThemePartDefined,
        "IsThemeBackgroundPartiallyTransparent" => fn IsThemeBackgroundPartiallyTransparent,
        "SetWindowTheme" => fn SetWindowTheme,
        "GetThemePartSize" => fn GetThemePartSize,
        "DrawThemeText" => fn DrawThemeText,
        "BufferedPaintInit" => fn BufferedPaintInit,
        "BufferedPaintUnInit" => fn BufferedPaintUnInit,
        "BeginBufferedPaint" => fn BeginBufferedPaint,
        "EndBufferedPaint" => fn EndBufferedPaint,
        "GetThemeFont" => fn GetThemeFont,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(buf: &[u16]) -> String {
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..n])
    }

    #[test]
    fn theme_aero_actif_pour_wpf_mais_pas_pour_les_fenetres_win32() {
        assert_eq!(IsThemeActive(), 1);
        assert_eq!(IsAppThemed(), 0);
        unsafe {
            assert_eq!(OpenThemeData(1, std::ptr::null()), 0);
            let (mut n, mut c, mut s) = ([0u16; 260], [0u16; 64], [0u16; 64]);
            assert_eq!(
                GetCurrentThemeName(n.as_mut_ptr(), 260, c.as_mut_ptr(), 64, s.as_mut_ptr(), 64),
                0
            );
            // WPF prend le nom du fichier sans extension (« aero » → Aero2).
            assert!(read(&n).to_ascii_lowercase().ends_with("\\aero.msstyles"));
            assert_eq!(read(&c), "NormalColor");
            assert_eq!(read(&s), "NormalSize");
            // Tampons facultatifs, et tampon trop court tronqué.
            assert_eq!(
                GetCurrentThemeName(
                    std::ptr::null_mut(),
                    0,
                    c.as_mut_ptr(),
                    4,
                    std::ptr::null_mut(),
                    0
                ),
                E_NOT_SUFFICIENT_BUFFER
            );
            assert_eq!(read(&c), "Nor");
        }
    }
}
