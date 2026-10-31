//! Stubs pour uxtheme.dll — thèmes visuels Windows.
//! On signale l'absence de thème, ce qui force le rendu classique.
#![allow(non_snake_case)]

const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;

pub extern "win64" fn IsThemeActive() -> i32 {
    0
}
pub extern "win64" fn IsAppThemed() -> i32 {
    0
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
        "OpenThemeData" => fn OpenThemeData,
        "OpenThemeDataEx" => fn OpenThemeDataEx,
        "CloseThemeData" => fn CloseThemeData,
        "DrawThemeBackground" => fn DrawThemeBackground,
        "GetThemeColor" => fn GetThemeColor,
        "GetThemeSysColor" => fn GetThemeSysColor,
        "GetThemeSysFont" => fn GetThemeSysFont,
        "IsThemePartDefined" => fn IsThemePartDefined,
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
