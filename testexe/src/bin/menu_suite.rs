//! Test PE des menus surgissants, comme le menu contextuel de PuTTY
//! (Ctrl+clic droit) et son menu système enrichi : TrackPopupMenu avec et
//! sans TPM_RETURNCMD, sous-menu, éléments cochés et grisés, WM_COMMAND et
//! WM_SYSCOMMAND envoyés à la fenêtre.
//!
//! Sans affichage, `WINLUX_POPUPMENU=<id>` (posée ici) fait le choix. Avec
//! `WINLUX_CONTROLS_SHOW=<ms>`, le menu est affiché pour de vrai et piloté
//! au clavier (Bas, Droite, Entrée) par des messages postés : il reste
//! ouvert ce temps-là pour une capture d'écran.

#![no_std]
#![no_main]
#![allow(static_mut_refs)]

use core::ffi::c_char;
use winlux_testexe::*;

#[link(name = "user32", kind = "raw-dylib")]
extern "C" {
    fn CreatePopupMenu() -> Handle;
    fn AppendMenuA(menu: Handle, flags: u32, id: usize, text: *const c_char) -> i32;
    fn TrackPopupMenu(
        menu: Handle,
        flags: u32,
        x: i32,
        y: i32,
        reserved: i32,
        hwnd: Handle,
        rect: *const Rect,
    ) -> i32;
    fn GetSystemMenu(hwnd: Handle, revert: i32) -> Handle;
    fn DestroyMenu(menu: Handle) -> i32;
    fn PeekMessageA(msg: *mut Msg, hwnd: Handle, min: u32, max: u32, remove: u32) -> i32;
}

#[link(name = "kernel32", kind = "raw-dylib")]
extern "C" {
    fn GetEnvironmentVariableA(name: *const c_char, buf: *mut c_char, size: u32) -> u32;
}

const WS_VISIBLE: u32 = 0x1000_0000;
const WM_COMMAND: u32 = 0x0111;
const WM_SYSCOMMAND: u32 = 0x0112;
const WM_INITMENUPOPUP: u32 = 0x0117;
const WM_TIMER: u32 = 0x0113;
const WM_KEYDOWN: u32 = 0x0100;
const MF_GRAYED: u32 = 0x1;
const MF_CHECKED: u32 = 0x8;
const MF_POPUP: u32 = 0x10;
const MF_SEPARATOR: u32 = 0x800;
const TPM_RETURNCMD: u32 = 0x100;

static mut LAST_COMMAND: usize = 0;
static mut LAST_SYSCOMMAND: usize = 0;
static mut INIT_POPUPS: u32 = 0;
static mut MAIN: Handle = core::ptr::null_mut();

unsafe extern "win64" fn wnd_proc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        WM_COMMAND => {
            LAST_COMMAND = wparam & 0xFFFF;
            0
        }
        WM_SYSCOMMAND => {
            LAST_SYSCOMMAND = wparam;
            0
        }
        WM_INITMENUPOPUP => {
            INIT_POPUPS += 1;
            0
        }
        WM_TIMER => {
            // Pilotage clavier du menu affiché (la boucle du menu distribue
            // WM_TIMER et prend les touches, quelle que soit la fenêtre).
            let keys: &[usize] = match wparam {
                1 => &[0x28, 0x28, 0x28], // Bas x3 : « Commandes spéciales »
                2 => &[0x27],             // Droite : ouvre le sous-menu
                _ => &[0x0D],             // Entrée : premier élément
            };
            for &k in keys {
                PostMessageA(MAIN, WM_KEYDOWN, k, 0);
            }
            KillTimer(hwnd, wparam);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcA(hwnd, msg, wparam, lparam),
    }
}

unsafe fn pump() {
    let mut msg: Msg = core::mem::zeroed();
    while PeekMessageA(&mut msg, core::ptr::null_mut(), 0, 0, 1) != 0 {
        if msg.message == 0x0012 {
            break; // WM_QUIT
        }
        TranslateMessage(&msg);
        DispatchMessageA(&msg);
    }
}

unsafe fn show_ms() -> u32 {
    let mut wait = [0u8; 16];
    let len = GetEnvironmentVariableA(
        c"WINLUX_CONTROLS_SHOW".as_ptr(),
        wait.as_mut_ptr().cast(),
        16,
    );
    let mut ms = 0u32;
    for &b in &wait[..len.min(16) as usize] {
        if b.is_ascii_digit() {
            ms = ms * 10 + (b - b'0') as u32;
        }
    }
    ms
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        let class = c"WinluxMenuSuite";
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
            c"Terminal".as_ptr(),
            0x00CF_0000 | WS_VISIBLE,
            80,
            60,
            640,
            400,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        check!(!main.is_null(), "fenetre principale");
        MAIN = main;

        // Menu contextuel façon PuTTY.
        let special = CreatePopupMenu();
        AppendMenuA(special, 0, 201, c"&Break".as_ptr());
        AppendMenuA(special, MF_CHECKED, 202, c"&Ping".as_ptr());
        let menu = CreatePopupMenu();
        check!(!menu.is_null(), "CreatePopupMenu");
        AppendMenuA(menu, 0, 101, c"&Copier\tCtrl+C".as_ptr());
        AppendMenuA(menu, 0, 102, c"C&oller".as_ptr());
        AppendMenuA(menu, MF_SEPARATOR, 0, core::ptr::null());
        AppendMenuA(
            menu,
            MF_POPUP,
            special as usize,
            c"Commandes &speciales".as_ptr(),
        );
        AppendMenuA(menu, MF_GRAYED, 103, c"&Grise".as_ptr());
        AppendMenuA(menu, MF_CHECKED, 104, c"&Plein ecran".as_ptr());

        let sys = GetSystemMenu(main, 0);
        check!(!sys.is_null(), "GetSystemMenu");
        AppendMenuA(sys, MF_SEPARATOR, 0, core::ptr::null());
        AppendMenuA(sys, 0, 0x0150, c"&Nouvelle session...".as_ptr());

        let ms = show_ms();
        if ms == 0 {
            SetEnvironmentVariableA(c"WINLUX_POPUPMENU".as_ptr(), c"202".as_ptr());
            let r = TrackPopupMenu(menu, TPM_RETURNCMD, 200, 150, 0, main, core::ptr::null());
            check!(r == 202, "TPM_RETURNCMD renvoie l'element du sous-menu");
            check!(INIT_POPUPS > 0, "WM_INITMENUPOPUP envoye avant l'affichage");

            SetEnvironmentVariableA(c"WINLUX_POPUPMENU".as_ptr(), c"103".as_ptr());
            let r = TrackPopupMenu(menu, TPM_RETURNCMD, 200, 150, 0, main, core::ptr::null());
            check!(r == 0, "element grise non choisi");

            SetEnvironmentVariableA(c"WINLUX_POPUPMENU".as_ptr(), c"102".as_ptr());
            let r = TrackPopupMenu(menu, 0, 200, 150, 0, main, core::ptr::null());
            pump();
            check!(r == 1 && LAST_COMMAND == 102, "WM_COMMAND poste a la fenetre");

            SetEnvironmentVariableA(c"WINLUX_POPUPMENU".as_ptr(), c"336".as_ptr());
            let r = TrackPopupMenu(sys, 0, 90, 70, 0, main, core::ptr::null());
            pump();
            check!(
                r == 1 && LAST_SYSCOMMAND == 0x0150,
                "menu systeme : WM_SYSCOMMAND"
            );
            SetEnvironmentVariableA(c"WINLUX_POPUPMENU".as_ptr(), core::ptr::null());
        } else {
            // Affichage réel : le menu reste ouvert `ms`, piloté au clavier.
            SetTimer(main, 1, ms / 3 + 1, 0);
            SetTimer(main, 2, ms * 2 / 3 + 1, 0);
            SetTimer(main, 3, ms + 1, 0);
            let r = TrackPopupMenu(menu, TPM_RETURNCMD, 200, 150, 0, main, core::ptr::null());
            check!(r == 201, "menu pilote au clavier : sous-menu puis Entree");
        }
        check!(DestroyMenu(menu) != 0, "DestroyMenu");
        DestroyWindow(main);
        pump();
        ExitProcess(if FAILURES == 0 { 0 } else { 1 });
    }
}
