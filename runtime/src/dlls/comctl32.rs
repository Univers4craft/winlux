//! comctl32.dll — Contrôles communs Windows (stubs).
//!
//! Notepad++ utilise les contrôles Tab, ListView, TreeView, Toolbar,
//! StatusBar et RichEdit. Pour l'instant on fournit les stubs nécessaires
//! au démarrage : les contrôles sont enregistrés mais rendus à vide.

use std::ffi::c_void;

/// INITCOMMONCONTROLSEX
#[repr(C)]
pub(crate) struct InitCCEx {
    size: u32,
    icc: u32,
}

pub unsafe extern "win64" fn init_common_controls_ex(icc: *const InitCCEx) -> i32 {
    if !icc.is_null() {
        let _ = (*icc).icc;
    }
    1
}

pub extern "win64" fn init_common_controls() {}

/// ImageList_Create
pub extern "win64" fn image_list_create(
    cx: i32,
    cy: i32,
    _flags: u32,
    initial: i32,
    _grow: i32,
) -> usize {
    // On renvoie un pointeur factice non nul.
    let _ = (cx, cy, initial);
    0x1 // factice, suffisant pour que les tests IsNull passent
}

pub extern "win64" fn image_list_destroy(_himl: usize) -> i32 {
    1
}
pub extern "win64" fn image_list_add(_himl: usize, _bmp: usize, _mask: usize) -> i32 {
    0
}
pub extern "win64" fn image_list_add_icon(_himl: usize, _icon: usize) -> i32 {
    0
}
pub extern "win64" fn image_list_replace_icon(_himl: usize, _i: i32, _icon: usize) -> i32 {
    0
}
pub extern "win64" fn image_list_set_image_count(_himl: usize, _count: u32) -> i32 {
    1
}
pub extern "win64" fn image_list_get_image_count(_himl: usize) -> i32 {
    0
}

/// CreateStatusWindowA
pub unsafe extern "win64" fn create_status_window_a(
    _style: u32,
    _text: *const u8,
    _parent: usize,
    _id: u32,
) -> usize {
    0
}
pub unsafe extern "win64" fn create_status_window_w(
    _style: u32,
    _text: *const u16,
    _parent: usize,
    _id: u32,
) -> usize {
    0
}

/// PropertySheet (stub)
pub unsafe extern "win64" fn property_sheet_a(_psh: *const c_void) -> isize {
    0
}
pub unsafe extern "win64" fn property_sheet_w(_psh: *const c_void) -> isize {
    0
}

/// CreateToolbarEx
pub unsafe extern "win64" fn create_toolbar_ex(
    _parent: usize,
    _style: u32,
    _id: u32,
    _nbmp: i32,
    _hinst: usize,
    _bmp_id: usize,
    _buttons: *const c_void,
    _nbuttons: i32,
    _dxbutton: i32,
    _dybutton: i32,
    _dxbitmap: i32,
    _dybitmap: i32,
    _size: u32,
) -> usize {
    0
}

/// CreateUpDownControl
pub extern "win64" fn create_up_down_control(
    _style: u32,
    _x: i32,
    _y: i32,
    _cx: i32,
    _cy: i32,
    _parent: usize,
    _id: i32,
    _hinst: usize,
    _buddy: usize,
    _upper: i32,
    _lower: i32,
    _pos: i32,
) -> usize {
    0
}

/// FlatSB_* (flat scroll bars)
pub extern "win64" fn flat_sb_enable_scroll_bar(_hwnd: usize, _code: i32, _arrows: u32) -> i32 {
    1
}
pub extern "win64" fn flat_sb_show_scroll_bar(_hwnd: usize, _code: i32, _show: i32) -> i32 {
    1
}
pub extern "win64" fn init_flat_sb(_hwnd: usize) -> i32 {
    0
}

/// TaskDialog (Vista+, stub renvoyant une erreur)
pub unsafe extern "win64" fn task_dialog(
    _parent: usize,
    _instance: usize,
    _title: *const u16,
    _main: *const u16,
    _content: *const u16,
    _buttons: i32,
    _icon: *const u16,
    _button: *mut i32,
) -> i32 {
    1 // E_NOTIMPL, les appelants testent le retour
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "InitCommonControlsEx" => fn init_common_controls_ex,
        "InitCommonControls" => fn init_common_controls,
        "ImageList_Create" => fn image_list_create,
        "ImageList_Destroy" => fn image_list_destroy,
        "ImageList_Add" => fn image_list_add,
        "ImageList_AddIcon" => fn image_list_add_icon,
        "ImageList_ReplaceIcon" => fn image_list_replace_icon,
        "ImageList_SetImageCount" => fn image_list_set_image_count,
        "ImageList_GetImageCount" => fn image_list_get_image_count,
        "CreateStatusWindowA" => fn create_status_window_a,
        "CreateStatusWindowW" => fn create_status_window_w,
        "PropertySheetA" => fn property_sheet_a,
        "PropertySheetW" => fn property_sheet_w,
        "CreateToolbarEx" => fn create_toolbar_ex,
        "CreateUpDownControl" => fn create_up_down_control,
        "FlatSB_EnableScrollBar" => fn flat_sb_enable_scroll_bar,
        "FlatSB_ShowScrollBar" => fn flat_sb_show_scroll_bar,
        "InitializeFlatSB" => fn init_flat_sb,
        "TaskDialog" => fn task_dialog,
    }
}
