//! Test PE des barres de comctl32, disposées comme le gestionnaire de
//! fichiers de 7-Zip : tout est créé dans le WM_CREATE de la fenêtre
//! principale (ReBarWindow32 qui porte une ToolbarWindow32), puis un
//! panneau enfant crée lui-même, dans son propre WM_CREATE, sa barre
//! d'adresse (ComboBoxEx32) et sa liste (SysListView32).
//!
//! Avant Winlux 0.2.17, une fenêtre enfant créée pendant le WM_CREATE de son
//! parent devenait une fenêtre isolée sans titre (le parent n'avait pas
//! encore de fenêtre X), et ces trois classes n'existaient pas.
//!
//! `WINLUX_CONTROLS_SHOW=<ms>` garde la fenêtre ouverte ce temps-là (capture
//! d'écran sous X11) ; sinon la suite se termine dès les vérifications faites.

#![no_std]
#![no_main]
#![allow(static_mut_refs)]

use core::ffi::{c_char, c_void};
use winlux_testexe::*;

#[link(name = "user32", kind = "raw-dylib")]
extern "C" {
    fn SendMessageA(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn GetParent(hwnd: Handle) -> Handle;
    fn IsWindowVisible(hwnd: Handle) -> i32;
    fn SetWindowLongPtrA(hwnd: Handle, index: i32, value: isize) -> isize;
    fn GetWindowLongPtrA(hwnd: Handle, index: i32) -> isize;
    fn CallWindowProcA(proc_: isize, hwnd: Handle, msg: u32, wparam: usize, lparam: isize)
        -> isize;
}

#[link(name = "kernel32", kind = "raw-dylib")]
extern "C" {
    fn GetEnvironmentVariableA(name: *const c_char, buf: *mut c_char, size: u32) -> u32;
}

#[link(name = "comctl32", kind = "raw-dylib")]
extern "C" {
    fn InitCommonControlsEx(icc: *const [u32; 2]) -> i32;
    fn CreateToolbarEx(
        parent: Handle,
        style: u32,
        id: u32,
        nbmp: i32,
        hinst: Handle,
        bmp_id: usize,
        buttons: *const TbButton,
        nbuttons: i32,
        dxbutton: i32,
        dybutton: i32,
        dxbitmap: i32,
        dybitmap: i32,
        size: u32,
    ) -> Handle;
}

const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_BORDER: u32 = 0x0080_0000;
const WS_CLIPCHILDREN: u32 = 0x0200_0000;
const WS_EX_CLIENTEDGE: u32 = 0x200;
const WM_CREATE: u32 = 0x0001;
const WM_SIZE: u32 = 0x0005;
const WM_COMMAND: u32 = 0x0111;
const WM_NOTIFY: u32 = 0x004E;
const WM_SETFONT: u32 = 0x0030;
const WM_TIMER: u32 = 0x0113;
const WM_CLOSE: u32 = 0x0010;

static mut REBAR: Handle = core::ptr::null_mut();
static mut TOOLBAR: Handle = core::ptr::null_mut();
static mut PANEL: Handle = core::ptr::null_mut();
static mut ADDRESS: Handle = core::ptr::null_mut();
static mut LIST: Handle = core::ptr::null_mut();
static mut COMMANDS: u32 = 0;
static mut HEIGHT_CHANGES: u32 = 0;
static mut CREATE_OK: bool = false;
static mut PANEL_CREATE_OK: bool = false;
/// Sous-classement du champ de la barre d'adresse, comme CMyComboBoxEdit.
static mut EDIT_ORIG: isize = 0;
static mut EDIT_DEPTH: u32 = 0;
static mut EDIT_MAX_DEPTH: u32 = 0;
static mut END_EDITS: u32 = 0;
static mut END_EDIT_TEXT: [u8; 64] = [0; 64];

unsafe extern "win64" fn edit_subclass(
    hwnd: Handle,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    EDIT_DEPTH += 1;
    EDIT_MAX_DEPTH = EDIT_MAX_DEPTH.max(EDIT_DEPTH);
    let orig = if GetWindowLongPtrA(hwnd, -21) == 0x7A {
        EDIT_ORIG
    } else {
        0
    };
    let r = if orig == 0 || EDIT_DEPTH > 64 {
        0
    } else {
        CallWindowProcA(orig, hwnd, msg, wparam, lparam)
    };
    EDIT_DEPTH -= 1;
    r
}

#[repr(C)]
struct TbButton {
    bitmap: i32,
    command: i32,
    state: u8,
    style: u8,
    reserved: [u8; 6],
    data: usize,
    string: isize,
}

/// REBARBANDINFOA (taille Vista, 128 octets).
#[repr(C)]
struct RebarBand {
    size: u32,
    mask: u32,
    style: u32,
    fore: u32,
    back: u32,
    text: *const c_char,
    cch: u32,
    image: i32,
    child: Handle,
    cx_min: u32,
    cy_min: u32,
    cx: u32,
    background: Handle,
    id: u32,
    cy_child: u32,
    cy_max: u32,
    cy_integral: u32,
    cx_ideal: u32,
    param: isize,
    cx_header: u32,
    chevron: [i32; 4],
    chevron_state: u32,
}

/// COMBOBOXEXITEMA.
#[repr(C)]
struct CbeItem {
    mask: u32,
    item: isize,
    text: *const c_char,
    text_max: i32,
    image: i32,
    selected_image: i32,
    overlay: i32,
    indent: i32,
    param: isize,
}

unsafe fn create(
    ex: u32,
    class: &core::ffi::CStr,
    style: u32,
    rect: (i32, i32, i32, i32),
    parent: Handle,
    id: usize,
) -> Handle {
    let h = CreateWindowExA(
        ex,
        class.as_ptr(),
        c"".as_ptr(),
        style,
        rect.0,
        rect.1,
        rect.2,
        rect.3,
        parent,
        id as Handle,
        core::ptr::null_mut(),
        core::ptr::null_mut(),
    );
    if !h.is_null() {
        SendMessageA(h, WM_SETFONT, GetStockObject(17) as usize, 0);
    }
    h
}

unsafe fn layout(hwnd: Handle) {
    let mut r: Rect = core::mem::zeroed();
    GetClientRect(hwnd, &mut r);
    let w = r.right - r.left;
    let bar_h = SendMessageA(REBAR, 0x041B, 0, 0) as i32 + 2; // RB_GETBARHEIGHT
    MoveWindow(REBAR, 0, 0, w, bar_h, 1);
    MoveWindow(PANEL, 0, bar_h, w, r.bottom - bar_h, 1);
}

unsafe extern "win64" fn panel_proc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        WM_CREATE => {
            // Comme CPanel::OnCreate : barre d'adresse puis liste des fichiers.
            ADDRESS = create(
                0,
                c"ComboBoxEx32",
                WS_CHILD | WS_VISIBLE | 0x2 | 0x0020_0000, // CBS_DROPDOWN | WS_VSCROLL
                (0, 0, 300, 300),
                hwnd,
                1001,
            );
            for (i, t) in [c"C:\\", c"C:\\Users", c"C:\\Users\\Public"]
                .iter()
                .enumerate()
            {
                let item = CbeItem {
                    mask: 0x1 | 0x20,
                    item: -1,
                    text: t.as_ptr(),
                    text_max: 0,
                    image: 0,
                    selected_image: 0,
                    overlay: 0,
                    indent: i as i32,
                    param: i as isize,
                };
                SendMessageA(ADDRESS, 0x0401, 0, &item as *const CbeItem as isize);
            }
            let path = CbeItem {
                mask: 0x1,
                item: -1,
                text: c"C:\\Users\\Public".as_ptr(),
                text_max: 0,
                image: 0,
                selected_image: 0,
                overlay: 0,
                indent: 0,
                param: 0,
            };
            SendMessageA(ADDRESS, 0x0405, 0, &path as *const CbeItem as isize);
            LIST = create(
                WS_EX_CLIENTEDGE,
                c"SysListView32",
                WS_CHILD | WS_VISIBLE | 0x1, // LVS_REPORT
                (0, 30, 300, 200),
                hwnd,
                1002,
            );
            PANEL_CREATE_OK = !ADDRESS.is_null() && !LIST.is_null();
            0
        }
        WM_SIZE => {
            let w = (lparam & 0xFFFF) as i32;
            let h = ((lparam >> 16) & 0xFFFF) as i32;
            let mut r: Rect = core::mem::zeroed();
            GetWindowRect(ADDRESS, &mut r);
            let ah = r.bottom - r.top;
            MoveWindow(ADDRESS, 0, 0, w, ah, 1);
            MoveWindow(LIST, 0, ah + 2, w, (h - ah - 2).max(1), 1);
            0
        }
        WM_NOTIFY => {
            let code = *((lparam as *const u8).add(16) as *const i32);
            if code == -805 {
                // CBEN_ENDEDITA : NMHDR, fChanged, iNewSelection, szText[260], iWhy
                END_EDITS += 1;
                let text = (lparam as *const u8).add(32);
                for i in 0..63 {
                    END_EDIT_TEXT[i] = *text.add(i);
                }
            }
            0
        }
        _ => DefWindowProcA(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "win64" fn wnd_proc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        WM_CREATE => {
            // Comme CApp::Create de 7-Zip : bande + barre d'outils, puis panneau.
            REBAR = create(
                0x80, // WS_EX_TOOLWINDOW
                c"ReBarWindow32",
                WS_CHILD | WS_VISIBLE | WS_BORDER | WS_CLIPCHILDREN | 0x40 | 0x8 | 0x200 | 0x400,
                (0, 0, 0, 0),
                hwnd,
                100,
            );
            TOOLBAR = create(
                0,
                c"ToolbarWindow32",
                WS_CHILD | WS_VISIBLE | 0x0800 | 0x0200 | 0x8 | 0x4 | 0x40 | 0x10,
                (0, 0, 0, 0),
                hwnd,
                101,
            );
            SendMessageA(TOOLBAR, 0x041E, core::mem::size_of::<TbButton>(), 0);
            let labels = [
                c"Ajouter",
                c"Extraire",
                c"Tester",
                c"Copier",
                c"Deplacer",
                c"Supprimer",
                c"Infos",
            ];
            for (i, label) in labels.iter().enumerate() {
                let b = TbButton {
                    bitmap: i as i32,
                    command: 2000 + i as i32,
                    state: 0x4,
                    style: 0x10, // BTNS_AUTOSIZE
                    reserved: [0; 6],
                    data: 0,
                    string: label.as_ptr() as isize,
                };
                SendMessageA(TOOLBAR, 0x0414, 1, &b as *const TbButton as isize);
            }
            SendMessageA(TOOLBAR, 0x0421, 0, 0); // TB_AUTOSIZE
            let mut size = [0i32; 2];
            SendMessageA(TOOLBAR, 0x0453, 0, size.as_mut_ptr() as isize); // TB_GETMAXSIZE
            let band = RebarBand {
                size: core::mem::size_of::<RebarBand>() as u32,
                mask: 0x1 | 0x10 | 0x20 | 0x40, // STYLE | CHILD | CHILDSIZE | SIZE
                style: 0x100 | 0x4,             // NOGRIPPER | CHILDEDGE
                fore: 0,
                back: 0,
                text: core::ptr::null(),
                cch: 0,
                image: 0,
                child: TOOLBAR,
                cx_min: size[0] as u32,
                cy_min: size[1] as u32,
                cx: size[0] as u32,
                background: core::ptr::null_mut(),
                id: 1,
                cy_child: 0,
                cy_max: 0,
                cy_integral: 0,
                cx_ideal: 0,
                param: 0,
                cx_header: 0,
                chevron: [0; 4],
                chevron_state: 0,
            };
            SendMessageA(
                REBAR,
                0x0401,
                usize::MAX,
                &band as *const RebarBand as isize,
            );
            PANEL = create(
                0x10000, // WS_EX_CONTROLPARENT
                c"WinluxBarsPanel",
                WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN,
                (0, 40, 600, 400),
                hwnd,
                102,
            );
            CREATE_OK = !REBAR.is_null() && !TOOLBAR.is_null() && !PANEL.is_null();
            0
        }
        WM_SIZE => {
            if !REBAR.is_null() && !PANEL.is_null() {
                layout(hwnd);
            }
            0
        }
        WM_COMMAND => {
            if (2000..2010).contains(&(wparam & 0xFFFF)) {
                COMMANDS += 1;
            }
            0
        }
        WM_NOTIFY => {
            let code = *((lparam as *const u8).add(16) as *const i32);
            if code == -831 {
                HEIGHT_CHANGES += 1; // RBN_HEIGHTCHANGE
            }
            0
        }
        WM_TIMER | WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcA(hwnd, msg, wparam, lparam),
    }
}

unsafe fn register(name: &core::ffi::CStr, proc_addr: usize) -> bool {
    let wc = WndClassExA {
        size: core::mem::size_of::<WndClassExA>() as u32,
        style: 0,
        wnd_proc: proc_addr,
        cls_extra: 0,
        wnd_extra: 0,
        instance: core::ptr::null_mut(),
        icon: core::ptr::null_mut(),
        cursor: core::ptr::null_mut(),
        background: GetSysColorBrush(15),
        menu_name: core::ptr::null(),
        class_name: name.as_ptr(),
        icon_sm: core::ptr::null_mut(),
    };
    RegisterClassExA(&wc) != 0
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        let icc = [8u32, 0x4 | 0x400 | 0x200 | 0x1];
        check!(InitCommonControlsEx(&icc) != 0, "InitCommonControlsEx");
        check!(
            register(c"WinluxBarsSuite", wnd_proc as *const () as usize),
            "RegisterClassExA (fenetre principale)"
        );
        check!(
            register(c"WinluxBarsPanel", panel_proc as *const () as usize),
            "RegisterClassExA (panneau)"
        );
        let main = CreateWindowExA(
            0,
            c"WinluxBarsSuite".as_ptr(),
            c"Gestionnaire de fichiers".as_ptr(),
            0x00CF_0000 | WS_VISIBLE,
            80,
            60,
            720,
            480,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        check!(!main.is_null(), "fenetre principale");
        check!(
            CREATE_OK,
            "ReBar, barre d'outils et panneau crees dans WM_CREATE"
        );
        check!(
            PANEL_CREATE_OK,
            "ComboBoxEx et liste creees dans le WM_CREATE du panneau"
        );
        check!(
            GetParent(TOOLBAR) == REBAR,
            "la barre d'outils appartient a la bande"
        );
        check!(
            GetParent(PANEL) == main,
            "le panneau appartient a la fenetre"
        );
        check!(
            GetParent(ADDRESS) == PANEL,
            "la barre d'adresse appartient au panneau"
        );
        check!(IsWindowVisible(PANEL) != 0, "panneau visible");
        check!(SendMessageA(REBAR, 0x040C, 0, 0) == 1, "RB_GETBANDCOUNT");
        check!(SendMessageA(TOOLBAR, 0x0418, 0, 0) == 7, "TB_BUTTONCOUNT");
        check!(HEIGHT_CHANGES >= 1, "RBN_HEIGHTCHANGE au parent");
        let bar_h = SendMessageA(REBAR, 0x041B, 0, 0);
        check!(bar_h >= 24, "RB_GETBARHEIGHT");
        check!(
            SendMessageA(ADDRESS, 0x0146, 0, 0) == 3,
            "CB_GETCOUNT de la ComboBoxEx"
        );
        let mut buf = [0u8; 64];
        let item = CbeItem {
            mask: 0x1,
            item: 1,
            text: buf.as_mut_ptr().cast(),
            text_max: 64,
            image: 0,
            selected_image: 0,
            overlay: 0,
            indent: 0,
            param: 0,
        };
        SendMessageA(ADDRESS, 0x0404, 0, &item as *const CbeItem as isize);
        check!(&buf[..9] == b"C:\\Users\0", "CBEM_GETITEMA");
        let combo = SendMessageA(ADDRESS, 0x0406, 0, 0) as Handle;
        let n = SendMessageA(combo, 0x000D, 64, buf.as_mut_ptr() as isize) as usize;
        check!(
            &buf[..n] == b"C:\\Users\\Public",
            "CBEM_SETITEMA sur le champ"
        );

        // Champ de la barre d'adresse sous-classé comme dans 7-Zip, puis
        // « \\ » tapé (AltGr+8 en AZERTY) et Entrée.
        let edit = SendMessageA(ADDRESS, 0x0407, 0, 0) as Handle; // CBEM_GETEDITCONTROL
        check!(!edit.is_null(), "CBEM_GETEDITCONTROL");
        SetWindowLongPtrA(edit, -21, 0x7A);
        EDIT_ORIG = SetWindowLongPtrA(edit, -4, edit_subclass as *const () as isize);
        check!(EDIT_ORIG != 0, "sous-classement du champ");
        SetFocus(edit);
        SendMessageA(edit, 0x000C, 0, c"C:".as_ptr() as isize);
        SendMessageA(edit, 0x0100, 0x23, 0); // VK_END
        SendMessageA(edit, 0x0100, 0x11, 0); // WM_KEYDOWN VK_CONTROL
        SendMessageA(edit, 0x0100, 0x12, 0x2000_0000); // VK_MENU
        SendMessageA(edit, 0x0100, 0x38, 0x2000_0000); // '8'
        SendMessageA(edit, 0x0102, b'\\' as usize, 0x2000_0000); // WM_CHAR
        SendMessageA(edit, 0x0101, 0x38, 0); // WM_KEYUP
        SendMessageA(edit, 0x0101, 0x12, 0);
        SendMessageA(edit, 0x0101, 0x11, 0);
        for c in b"Users" {
            SendMessageA(edit, 0x0102, *c as usize, 0);
        }
        check!(
            EDIT_MAX_DEPTH <= 4,
            "pas de recursion dans le champ sous-classe"
        );
        let n = SendMessageA(edit, 0x000D, 64, buf.as_mut_ptr() as isize) as usize;
        check!(
            &buf[..n] == b"C:\\Users",
            "texte tape dans la barre d'adresse"
        );
        SendMessageA(edit, 0x0100, 0x0D, 0); // VK_RETURN
        SendMessageA(edit, 0x0102, 0x0D, 0);
        check!(
            END_EDITS == 1 && &END_EDIT_TEXT[..9] == b"C:\\Users\0",
            "CBEN_ENDEDIT avec le chemin tape"
        );

        // CreateToolbarEx (barre d'en-tête d'un panneau de 7-Zip) : boutons
        // ajoutés, et une ComboBoxEx posée dessus.
        let header_buttons = [
            TbButton {
                bitmap: 0,
                command: 3000,
                state: 0x4,
                style: 0,
                reserved: [0; 6],
                data: 0,
                string: c"Dossier parent".as_ptr() as isize,
            },
            TbButton {
                bitmap: 1,
                command: 3001,
                state: 0x4,
                style: 0,
                reserved: [0; 6],
                data: 0,
                string: c"Vue".as_ptr() as isize,
            },
        ];
        let header = CreateToolbarEx(
            PANEL,
            WS_CHILD | WS_VISIBLE | 0x0800,
            200,
            11,
            -1isize as Handle, // HINST_COMMCTRL
            1,
            header_buttons.as_ptr(),
            2,
            0,
            0,
            0,
            0,
            core::mem::size_of::<TbButton>() as u32,
        );
        check!(!header.is_null(), "CreateToolbarEx");
        check!(
            SendMessageA(header, 0x0418, 0, 0) == 2,
            "boutons de CreateToolbarEx"
        );
        let on_bar = create(
            0,
            c"ComboBoxEx32",
            WS_CHILD | WS_VISIBLE | 0x2,
            (60, 0, 200, 200),
            header,
            201,
        );
        check!(
            !on_bar.is_null() && GetParent(on_bar) == header,
            "ComboBoxEx posee sur la barre"
        );
        let orphan = CreateWindowExA(
            0,
            c"STATIC".as_ptr(),
            c"".as_ptr(),
            WS_CHILD,
            0,
            0,
            10,
            10,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        check!(
            orphan.is_null() && GetLastError() == 1406,
            "WS_CHILD sans parent refusee"
        );
        DestroyWindow(header);

        // Clic sur le premier bouton : WM_COMMAND à la fenêtre principale.
        let mut r: Rect = core::mem::zeroed();
        SendMessageA(TOOLBAR, 0x041D, 0, &mut r as *mut Rect as isize);
        let at = (((r.top + 5) as isize) << 16) | (r.left + 5) as isize;
        SendMessageA(TOOLBAR, 0x0201, 1, at);
        SendMessageA(TOOLBAR, 0x0202, 0, at);
        check!(COMMANDS == 1, "WM_COMMAND du bouton Ajouter");

        let mut wait = [0u8; 16];
        let len = GetEnvironmentVariableA(
            c"WINLUX_CONTROLS_SHOW".as_ptr(),
            wait.as_mut_ptr().cast(),
            16,
        );
        let mut ms = 0u32;
        for &b in &wait[..len as usize] {
            if b.is_ascii_digit() {
                ms = ms * 10 + (b - b'0') as u32;
            }
        }
        SetTimer(main, 1, ms.max(50), 0);
        let mut msg: Msg = core::mem::zeroed();
        while GetMessageA(&mut msg, core::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageA(&msg);
        }
        let _: *mut c_void = core::ptr::null_mut();
        ExitProcess(if FAILURES == 0 { 0 } else { 1 });
    }
}
