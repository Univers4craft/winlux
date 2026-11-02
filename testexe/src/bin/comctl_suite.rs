//! Test PE des contrôles communs de comctl32, disposés comme la fenêtre de
//! configuration de PuTTY : arborescence des catégories à gauche, liste à
//! colonnes des sessions, onglets, barre de progression, curseur, flèches
//! haut/bas et barre d'état. Messages ANSI (`SendMessageA`) et notifications
//! `WM_NOTIFY` reçues par la fenêtre parente.
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
}

#[link(name = "kernel32", kind = "raw-dylib")]
extern "C" {
    fn GetEnvironmentVariableA(name: *const c_char, buf: *mut c_char, size: u32) -> u32;
}

#[link(name = "comctl32", kind = "raw-dylib")]
extern "C" {
    fn InitCommonControlsEx(icc: *const [u32; 2]) -> i32;
    fn CreateStatusWindowA(style: u32, text: *const c_char, parent: Handle, id: u32) -> Handle;
    fn CreateUpDownControl(
        style: u32,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        parent: Handle,
        id: i32,
        inst: Handle,
        buddy: Handle,
        upper: i32,
        lower: i32,
        pos: i32,
    ) -> Handle;
}

const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const WS_EX_CLIENTEDGE: u32 = 0x200;
const WM_NOTIFY: u32 = 0x004E;
const WM_SETFONT: u32 = 0x0030;
const WM_TIMER: u32 = 0x0113;
const WM_CLOSE: u32 = 0x0010;

const TVI_ROOT: usize = 0xFFFF_FFFF_FFFF_0000;
const TVI_LAST: usize = 0xFFFF_FFFF_FFFF_0002;

static mut TREE_SELECTED: u32 = 0;
static mut LIST_CHANGED: u32 = 0;
static mut TAB_CHANGED: u32 = 0;

#[repr(C)]
struct TvInsertA {
    parent: usize,
    after: usize,
    mask: u32,
    item: usize,
    state: u32,
    state_mask: u32,
    text: *const c_char,
    text_max: i32,
    image: i32,
    selected_image: i32,
    children: i32,
    param: isize,
}

#[repr(C)]
struct LvItemA {
    mask: u32,
    item: i32,
    sub_item: i32,
    state: u32,
    state_mask: u32,
    text: *const c_char,
    text_max: i32,
    image: i32,
    param: isize,
}

#[repr(C)]
struct LvColumnA {
    mask: u32,
    fmt: i32,
    cx: i32,
    text: *const c_char,
    text_max: i32,
    sub_item: i32,
}

#[repr(C)]
struct TcItemA {
    mask: u32,
    state: u32,
    state_mask: u32,
    text: *const c_char,
    text_max: i32,
    image: i32,
    param: isize,
}

unsafe extern "win64" fn wnd_proc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        WM_NOTIFY => {
            let code = *((lparam as *const u8).add(16) as *const i32);
            match code {
                -402 => TREE_SELECTED += 1, // TVN_SELCHANGEDA
                -101 => LIST_CHANGED += 1,  // LVN_ITEMCHANGED
                -551 => TAB_CHANGED += 1,   // TCN_SELCHANGE
                _ => {}
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

unsafe fn control(
    parent: Handle,
    class: &core::ffi::CStr,
    style: u32,
    ex: u32,
    id: i32,
    rect: (i32, i32, i32, i32),
) -> Handle {
    let h = CreateWindowExA(
        ex,
        class.as_ptr(),
        c"".as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        rect.0,
        rect.1,
        rect.2,
        rect.3,
        parent,
        id as usize as Handle,
        core::ptr::null_mut(),
        core::ptr::null_mut(),
    );
    SendMessageA(h, WM_SETFONT, GetStockObject(17) as usize, 0);
    h
}

unsafe fn tree_add(tv: Handle, parent: usize, text: &core::ffi::CStr) -> usize {
    let mut ins = TvInsertA {
        parent,
        after: TVI_LAST,
        mask: 0x1,
        item: 0,
        state: 0,
        state_mask: 0,
        text: text.as_ptr(),
        text_max: 0,
        image: 0,
        selected_image: 0,
        children: 0,
        param: 0,
    };
    SendMessageA(tv, 0x1100, 0, &mut ins as *mut TvInsertA as isize) as usize
}

unsafe fn list_set(lv: Handle, row: i32, col: i32, text: &core::ffi::CStr) -> isize {
    let mut it = LvItemA {
        mask: 0x1,
        item: row,
        sub_item: col,
        state: 0,
        state_mask: 0,
        text: text.as_ptr(),
        text_max: 0,
        image: 0,
        param: 0,
    };
    let msg = if col == 0 { 0x1007 } else { 0x1006 }; // INSERTITEMA / SETITEMA
    SendMessageA(lv, msg, 0, &mut it as *mut LvItemA as isize)
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        let icc = [8u32, 0xFFFF];
        check!(InitCommonControlsEx(&icc) != 0, "InitCommonControlsEx");
        let class = c"WinluxComctlSuite";
        let wc = WndClassExA {
            size: core::mem::size_of::<WndClassExA>() as u32,
            style: 0,
            wnd_proc: wnd_proc as *const () as usize,
            cls_extra: 0,
            wnd_extra: 0,
            instance: core::ptr::null_mut(),
            icon: core::ptr::null_mut(),
            cursor: core::ptr::null_mut(),
            background: GetSysColorBrush(15),
            menu_name: core::ptr::null(),
            class_name: class.as_ptr(),
            icon_sm: core::ptr::null_mut(),
        };
        check!(RegisterClassExA(&wc) != 0, "RegisterClassExA");
        let main = CreateWindowExA(
            0,
            class.as_ptr(),
            c"Configuration".as_ptr(),
            0x00CF_0000 | WS_VISIBLE,
            80,
            60,
            640,
            460,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        check!(!main.is_null(), "fenetre principale");

        // Arborescence des catégories, comme PuTTY.
        let tv = control(
            main,
            c"SysTreeView32",
            0x7 | WS_TABSTOP, // HASBUTTONS | HASLINES | LINESATROOT
            WS_EX_CLIENTEDGE,
            201,
            (12, 12, 180, 370),
        );
        check!(!tv.is_null(), "SysTreeView32");
        let session = tree_add(tv, TVI_ROOT, c"Session");
        tree_add(tv, session, c"Journal");
        let terminal = tree_add(tv, TVI_ROOT, c"Terminal");
        tree_add(tv, terminal, c"Clavier");
        tree_add(tv, terminal, c"Cloche");
        let window = tree_add(tv, TVI_ROOT, c"Fenetre");
        tree_add(tv, window, c"Apparence");
        let conn = tree_add(tv, TVI_ROOT, c"Connexion");
        let ssh = tree_add(tv, conn, c"SSH");
        tree_add(tv, ssh, c"Authentification");
        SendMessageA(tv, 0x1102, 2, session as isize);
        SendMessageA(tv, 0x1102, 2, conn as isize);
        SendMessageA(tv, 0x1102, 2, ssh as isize);
        check!(SendMessageA(tv, 0x1105, 0, 0) == 10, "TVM_GETCOUNT");
        SendMessageA(tv, 0x110B, 9, session as isize);
        check!(TREE_SELECTED == 1, "TVN_SELCHANGED au parent");
        check!(
            SendMessageA(tv, 0x110A, 9, 0) as usize == session,
            "TVM_GETNEXTITEM(TVGN_CARET)"
        );

        // Onglets au-dessus de la liste des sessions.
        let tab = control(main, c"SysTabControl32", 0, 0, 202, (204, 12, 410, 236));
        for (i, name) in [c"Sessions", c"Proxy", c"Avance"].iter().enumerate() {
            let mut it = TcItemA {
                mask: 0x1,
                state: 0,
                state_mask: 0,
                text: name.as_ptr(),
                text_max: 0,
                image: 0,
                param: 0,
            };
            SendMessageA(tab, 0x1307, i, &mut it as *mut TcItemA as isize);
        }
        check!(SendMessageA(tab, 0x1304, 0, 0) == 3, "TCM_GETITEMCOUNT");
        check!(SendMessageA(tab, 0x130B, 0, 0) == 0, "TCM_GETCURSEL");

        // Liste à colonnes (vue détails).
        let lv = control(
            main,
            c"SysListView32",
            0x1 | 0x4 | WS_TABSTOP, // REPORT | SINGLESEL
            WS_EX_CLIENTEDGE,
            203,
            (216, 50, 386, 186),
        );
        for (i, (name, cx)) in [(c"Session", 160), (c"Hote", 150), (c"Port", 60)]
            .iter()
            .enumerate()
        {
            let mut col = LvColumnA {
                mask: 0x2 | 0x4,
                fmt: 0,
                cx: *cx,
                text: name.as_ptr(),
                text_max: 0,
                sub_item: 0,
            };
            SendMessageA(lv, 0x101B, i, &mut col as *mut LvColumnA as isize);
        }
        let rows = [
            (c"serveur-maison", c"192.168.1.10", c"22"),
            (c"routeur", c"192.168.1.1", c"23"),
            (c"nas", c"nas.local", c"22"),
        ];
        for (i, (a, b, c)) in rows.iter().enumerate() {
            list_set(lv, i as i32, 0, a);
            list_set(lv, i as i32, 1, b);
            list_set(lv, i as i32, 2, c);
        }
        check!(SendMessageA(lv, 0x1004, 0, 0) == 3, "LVM_GETITEMCOUNT");
        let mut st = LvItemA {
            mask: 0x8,
            item: 0,
            sub_item: 0,
            state: 0x3,
            state_mask: 0x3,
            text: core::ptr::null(),
            text_max: 0,
            image: 0,
            param: 0,
        };
        SendMessageA(lv, 0x102B, 0, &mut st as *mut LvItemA as isize);
        check!(LIST_CHANGED >= 1, "LVN_ITEMCHANGED au parent");
        let mut buf = [0u8; 32];
        let mut get = LvItemA {
            mask: 0x1,
            item: 1,
            sub_item: 1,
            state: 0,
            state_mask: 0,
            text: buf.as_mut_ptr().cast(),
            text_max: 32,
            image: 0,
            param: 0,
        };
        let n = SendMessageA(lv, 0x102D, 1, &mut get as *mut LvItemA as isize) as usize;
        check!(&buf[..n] == b"192.168.1.1", "LVM_GETITEMTEXTA");

        // Barre de progression, curseur et flèches haut/bas.
        let bar = control(main, c"msctls_progress32", 0, 0, 204, (204, 262, 410, 18));
        SendMessageA(bar, 0x0406, 0, 100);
        SendMessageA(bar, 0x0402, 65, 0);
        check!(SendMessageA(bar, 0x0408, 0, 0) == 65, "PBM_GETPOS");
        let track = control(main, c"msctls_trackbar32", 0, 0, 205, (204, 292, 300, 32));
        SendMessageA(track, 0x0406, 1, 20 << 16);
        SendMessageA(track, 0x0405, 1, 12);
        check!(SendMessageA(track, 0x0400, 0, 0) == 12, "TBM_GETPOS");
        let edit = CreateWindowExA(
            WS_EX_CLIENTEDGE,
            c"EDIT".as_ptr(),
            c"".as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            520,
            294,
            60,
            26,
            main,
            206 as Handle,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        SendMessageA(edit, WM_SETFONT, GetStockObject(17) as usize, 0);
        let updown = CreateUpDownControl(
            0x2 | 0x4 | 0x20, // SETBUDDYINT | ALIGNRIGHT | ARROWKEYS
            0,
            0,
            18,
            26,
            main,
            207,
            core::ptr::null_mut(),
            edit,
            99,
            1,
            22,
        );
        check!(!updown.is_null(), "CreateUpDownControl");
        let mut num = [0u8; 8];
        let n = SendMessageA(edit, 0x000D, 8, num.as_mut_ptr() as isize) as usize; // WM_GETTEXT
        check!(&num[..n] == b"22", "UDS_SETBUDDYINT ecrit dans le champ");

        let status = CreateStatusWindowA(WS_VISIBLE, c"Pret".as_ptr(), main, 208);
        check!(!status.is_null(), "CreateStatusWindowA");
        let n = SendMessageA(status, 0x0402, 0, buf.as_mut_ptr() as isize) as usize & 0xFFFF;
        check!(&buf[..n] == b"Pret", "SB_GETTEXTA");

        SendMessageA(tab, 0x130C, 1, 0);
        check!(SendMessageA(tab, 0x130B, 0, 0) == 1, "TCM_SETCURSEL");
        let _ = TAB_CHANGED;

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
        SendMessageA(tab, 0x130C, 0, 0);
        SetFocus(tv);
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
