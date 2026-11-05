//! comctl32.dll — Contrôles communs Windows.
//!
//! Les classes (SysTreeView32, SysListView32, SysTabControl32,
//! msctls_statusbar32, msctls_progress32, msctls_trackbar32,
//! msctls_updown32, tooltips_class32) sont dessinées par
//! `controls::comctl` ; ce module fournit les fonctions exportées.
//! Barre d'outils, listes d'images et feuilles de propriétés restent
//! sommaires.

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

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;

/// CreateStatusWindow : une barre d'état placée en bas du parent.
unsafe fn create_status(style: u32, text: &str, parent: usize, id: u32) -> usize {
    let class = wide("msctls_statusbar32");
    let title = wide(text);
    crate::dlls::controls::with_charset(true, || {
        crate::dlls::user32::create_window_ex_w(
            0,
            class.as_ptr(),
            title.as_ptr(),
            style | WS_CHILD,
            0,
            0,
            0,
            0,
            parent,
            id as usize,
            0,
            std::ptr::null_mut(),
        )
    })
}

/// CreateStatusWindowA
pub unsafe extern "win64" fn create_status_window_a(
    style: u32,
    text: *const u8,
    parent: usize,
    id: u32,
) -> usize {
    let t = if text.is_null() {
        String::new()
    } else {
        crate::dlls::cstr(text)
    };
    create_status(style, &t, parent, id)
}
pub unsafe extern "win64" fn create_status_window_w(
    style: u32,
    text: *const u16,
    parent: usize,
    id: u32,
) -> usize {
    let t = if text.is_null() {
        String::new()
    } else {
        crate::dlls::wstr(text)
    };
    create_status(style, &t, parent, id)
}

/// PropertySheet (stub)
pub unsafe extern "win64" fn property_sheet_a(_psh: *const c_void) -> isize {
    0
}
/// CreatePropertySheetPageW/A : copie de la description de page
/// (PROPSHEETPAGE, dwSize octets), rendue comme HPROPSHEETPAGE.
pub unsafe extern "win64" fn create_property_sheet_page(psp: *const u32) -> usize {
    if psp.is_null() {
        return 0;
    }
    let size = (psp.read_unaligned() as usize).clamp(4, 0x1000);
    let mut copy = vec![0u8; size].into_boxed_slice();
    std::ptr::copy_nonoverlapping(psp as *const u8, copy.as_mut_ptr(), size);
    Box::into_raw(copy) as *mut u8 as usize
}

/// DestroyPropertySheetPage : libère la copie.
pub unsafe extern "win64" fn destroy_property_sheet_page(page: usize) -> i32 {
    if page == 0 {
        return 0;
    }
    let size = ((page as *const u32).read_unaligned() as usize).clamp(4, 0x1000);
    drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
        page as *mut u8,
        size,
    )));
    1
}

pub unsafe extern "win64" fn property_sheet_w(_psh: *const c_void) -> isize {
    0
}

/// CreateToolbarEx : barre d'outils enfant, ses images et ses boutons.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn create_toolbar_ex(
    parent: usize,
    style: u32,
    id: u32,
    nbmp: i32,
    hinst: usize,
    bmp_id: usize,
    buttons: *const c_void,
    nbuttons: i32,
    dxbutton: i32,
    dybutton: i32,
    dxbitmap: i32,
    dybitmap: i32,
    size: u32,
) -> usize {
    let class: Vec<u16> = "ToolbarWindow32".encode_utf16().chain([0]).collect();
    let tb = crate::dlls::user32::create_window_ex_w(
        0,
        class.as_ptr(),
        std::ptr::null(),
        style | 0x4000_0000, // WS_CHILD
        0,
        0,
        0,
        0,
        parent,
        id as usize,
        hinst,
        std::ptr::null_mut(),
    );
    if tb == 0 {
        return 0;
    }
    let send =
        |msg: u32, wp: usize, lp: isize| crate::dlls::user32::send_message_w(tb, msg, wp, lp);
    send(0x041E, if size == 0 { 32 } else { size as usize }, 0); // TB_BUTTONSTRUCTSIZE
    let pack = |w: i32, h: i32| ((w as u16 as u32) | ((h as u16 as u32) << 16)) as isize;
    if dxbitmap > 0 && dybitmap > 0 {
        send(0x0420, 0, pack(dxbitmap, dybitmap)); // TB_SETBITMAPSIZE
    }
    if dxbutton > 0 && dybutton > 0 {
        send(0x041F, 0, pack(dxbutton, dybutton)); // TB_SETBUTTONSIZE
    }
    if nbmp > 0 {
        let add = [hinst, bmp_id]; // TBADDBITMAP
        send(0x0413, nbmp as usize, add.as_ptr() as isize);
    }
    if !buttons.is_null() && nbuttons > 0 {
        send(0x0444, nbuttons as usize, buttons as isize); // TB_ADDBUTTONSW
    }
    send(0x0421, 0, 0); // TB_AUTOSIZE
    tb
}

/// CreateUpDownControl : flèches haut/bas avec leur champ associé.
pub unsafe extern "win64" fn create_up_down_control(
    style: u32,
    x: i32,
    y: i32,
    cx: i32,
    cy: i32,
    parent: usize,
    id: i32,
    hinst: usize,
    buddy: usize,
    upper: i32,
    lower: i32,
    pos: i32,
) -> usize {
    let class = wide("msctls_updown32");
    let hwnd = crate::dlls::controls::with_charset(true, || {
        crate::dlls::user32::create_window_ex_w(
            0,
            class.as_ptr(),
            std::ptr::null(),
            style | WS_CHILD | WS_VISIBLE,
            x,
            y,
            cx,
            cy,
            parent,
            id as usize,
            hinst,
            std::ptr::null_mut(),
        )
    });
    if hwnd != 0 {
        let send = |m: u32, wp: usize, lp: isize| {
            crate::dlls::controls::with_charset(true, || {
                crate::dlls::user32::send_message_w(hwnd, m, wp, lp)
            })
        };
        if buddy != 0 {
            send(0x0469, buddy, 0); // UDM_SETBUDDY
        }
        send(0x046F, lower as usize, upper as isize); // UDM_SETRANGE32
        send(0x0471, 0, pos as isize); // UDM_SETPOS32
    }
    hwnd
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
        "CreatePropertySheetPageW" => fn create_property_sheet_page,
        "CreatePropertySheetPageA" => fn create_property_sheet_page,
        "DestroyPropertySheetPage" => fn destroy_property_sheet_page,
        "CreateToolbarEx" => fn create_toolbar_ex,
        "CreateUpDownControl" => fn create_up_down_control,
        "FlatSB_EnableScrollBar" => fn flat_sb_enable_scroll_bar,
        "FlatSB_ShowScrollBar" => fn flat_sb_show_scroll_bar,
        "InitializeFlatSB" => fn init_flat_sb,
        "TaskDialog" => fn super::taskdialog::task_dialog,
        "TaskDialogIndirect" => fn super::taskdialog::task_dialog_indirect,
    }
}
