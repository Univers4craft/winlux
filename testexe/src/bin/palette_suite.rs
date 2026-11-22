//! Test PE d'une palette flottante (WS_CAPTION + WS_EX_TOOLWINDOW, possédée
//! par la fenêtre principale), comme celles de Paint.NET (#2984, #2983).
//!
//! Sous Windows, le rectangle passé à CreateWindowEx et SetWindowPos est
//! celui de la fenêtre, barre de titre comprise, et GetWindowRect le relit
//! tel quel (Learn, « GetWindowRect », « SetWindowPos »). Une palette calée
//! sur le bas de l'écran y reste donc, cadre compris, et la replacer au
//! même endroit ne la déplace pas. La zone client est l'intérieur du cadre.
//!
//! AdjustWindowRectEx annonce ce même cadre : une fenêtre créée à la taille
//! qu'il calcule pour une zone client voulue a bien cette zone client
//! (Learn, « AdjustWindowRectEx »). Sinon WinForms recalcule sans fin
//! (boîte de progression de Paint.NET, #3016).
//!
//! La suite affiche « palette X Y L H » (rectangle Win32 relu) : le script
//! de test-all compare le cadre réel du gestionnaire (xwininfo et
//! _NET_FRAME_EXTENTS) à ce rectangle.

#![no_std]
#![no_main]

use winlux_testexe::*;

const WS_CAPTION: u32 = 0x00C0_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;

#[link(name = "user32", kind = "raw-dylib")]
extern "C" {
    fn AdjustWindowRectEx(rect: *mut Rect, style: u32, menu: i32, ex_style: u32) -> i32;
}

/// Fenêtre de premier niveau créée à la taille qu'AdjustWindowRectEx donne
/// pour une zone client de 300x40 : GetClientRect doit la retrouver (#3016).
unsafe fn client_voulu(style: u32, ex_style: u32, owner: Handle, label: &str) {
    let mut r = Rect {
        left: 0,
        top: 0,
        right: 300,
        bottom: 40,
    };
    AdjustWindowRectEx(&mut r, style, 0, ex_style);
    let win = CreateWindowExA(
        ex_style,
        c"WinluxPaletteSuite".as_ptr(),
        c"Progression".as_ptr(),
        style,
        200,
        200,
        r.right - r.left,
        r.bottom - r.top,
        owner,
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        core::ptr::null_mut(),
    );
    ShowWindow(win, 4);
    pump(500);
    let mut c: Rect = core::mem::zeroed();
    GetClientRect(win, &mut c);
    out(label.as_bytes());
    out(b" : fenetre ");
    out_num(r.right - r.left);
    out(b"x");
    out_num(r.bottom - r.top);
    out(b", client ");
    out_num(c.right);
    out(b"x");
    out_num(c.bottom);
    out(b"\n");
    check!(
        c.right == 300 && c.bottom == 40,
        "AdjustWindowRectEx puis CreateWindowEx : zone client voulue (#3016)"
    );
    DestroyWindow(win);
}

unsafe extern "win64" fn wnd_proc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    DefWindowProcA(hwnd, msg, wparam, lparam)
}

unsafe fn register(name: &core::ffi::CStr) -> bool {
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
        class_name: name.as_ptr(),
        icon_sm: core::ptr::null_mut(),
    };
    RegisterClassExA(&wc) != 0
}

/// Traite les messages pendant `ms` millisecondes.
unsafe fn pump(ms: u32) {
    let start = GetTickCount();
    let mut msg: Msg = core::mem::zeroed();
    while GetTickCount().wrapping_sub(start) < ms {
        while PeekMessageA(&mut msg, core::ptr::null_mut(), 0, 0, 1) != 0 {
            TranslateMessage(&msg);
            DispatchMessageA(&msg);
        }
        Sleep(10);
    }
}

fn out_num(v: i32) {
    let mut buf = [0u8; 12];
    let mut i = buf.len();
    let neg = v < 0;
    let mut n = (v as i64).unsigned_abs();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    if neg {
        i -= 1;
        buf[i] = b'-';
    }
    out(&buf[i..]);
}

unsafe fn rect_of(hwnd: Handle) -> Rect {
    let mut r: Rect = core::mem::zeroed();
    GetWindowRect(hwnd, &mut r);
    r
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        check!(register(c"WinluxPaletteSuite"), "RegisterClassExA");
        let main = CreateWindowExA(
            0,
            c"WinluxPaletteSuite".as_ptr(),
            c"Principale".as_ptr(),
            0x00CF_0000 | WS_VISIBLE,
            40,
            40,
            600,
            400,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        check!(!main.is_null(), "fenetre principale");
        // Zone de travail : la palette est calée sur son coin bas droit.
        let mut work: Rect = core::mem::zeroed();
        SystemParametersInfoA(0x0030, 0, (&mut work as *mut Rect).cast(), 0);
        let (w, h) = (180, 180);
        let (x, y) = (work.right - w, work.bottom - h);
        let palette = CreateWindowExA(
            WS_EX_TOOLWINDOW,
            c"WinluxPaletteSuite".as_ptr(),
            c"Palette".as_ptr(),
            WS_CAPTION | WS_SYSMENU,
            x,
            y,
            w,
            h,
            main,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        check!(!palette.is_null(), "palette creee");
        ShowWindow(palette, 4); // SW_SHOWNOACTIVATE
        pump(800);
        let r = rect_of(palette);
        out(b"apres affichage ");
        out_num(r.left);
        out(b" ");
        out_num(r.top);
        out(b" ");
        out_num(r.right - r.left);
        out(b" ");
        out_num(r.bottom - r.top);
        out(b"\n");
        check!(
            r.left == x && r.top == y && r.right == x + w && r.bottom == y + h,
            "GetWindowRect : le rectangle demande, cadre compris (#2984)"
        );
        let mut c: Rect = core::mem::zeroed();
        GetClientRect(palette, &mut c);
        check!(
            c.right <= w && c.bottom <= h && c.right > 0 && c.bottom > 0,
            "GetClientRect : l'interieur du cadre (#2984)"
        );
        // Replacée au même endroit : elle n'y bouge pas (#2983).
        SetWindowPos(
            palette,
            core::ptr::null_mut(),
            x,
            y,
            w,
            h,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        pump(400);
        let r2 = rect_of(palette);
        check!(
            r2.left == x && r2.top == y && r2.right == x + w && r2.bottom == y + h,
            "SetWindowPos au meme endroit : rectangle inchange (#2983)"
        );
        out(b"palette ");
        out_num(r2.left);
        out(b" ");
        out_num(r2.top);
        out(b" ");
        out_num(r2.right - r2.left);
        out(b" ");
        out_num(r2.bottom - r2.top);
        out(b"\nclient ");
        out_num(c.right);
        out(b" ");
        out_num(c.bottom);
        out(b"\n");
        // Boîte de progression WinForms (WS_CAPTION | WS_SYSMENU, sans
        // WS_POPUP, possédée), puis palette et fenêtre ordinaire.
        client_voulu(0x02C8_0000, 0x0001_0001, main, "boite");
        client_voulu(WS_CAPTION | WS_SYSMENU, WS_EX_TOOLWINDOW, main, "outil");
        client_voulu(0x00CF_0000, 0, core::ptr::null_mut(), "ordinaire");
        // Laisse le temps au script de mesurer le cadre réel.
        pump(1500);
        ExitProcess(if FAILURES == 0 { 0 } else { 1 });
    }
}
