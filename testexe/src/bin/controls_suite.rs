//! Test PE des contrôles standard de Windows (BUTTON, STATIC, EDIT, LISTBOX,
//! COMBOBOX, SCROLLBAR) créés comme le fait un programme Win32 classique :
//! fenêtres enfants par `CreateWindowExA`, réglages par `SendMessageA`,
//! notifications `WM_COMMAND` reçues par la fenêtre parente.
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
    fn MessageBoxW(owner: Handle, text: *const u16, caption: *const u16, kind: u32) -> i32;
    fn GetDlgItem(dlg: Handle, id: i32) -> Handle;
    fn SetDlgItemTextA(dlg: Handle, id: i32, text: *const c_char) -> i32;
    fn GetDlgItemTextA(dlg: Handle, id: i32, buf: *mut c_char, max: i32) -> u32;
    fn CheckDlgButton(dlg: Handle, id: i32, check: u32) -> i32;
    fn IsDlgButtonChecked(dlg: Handle, id: i32) -> u32;
    fn EnableWindow(hwnd: Handle, enable: i32) -> i32;
    fn IsWindowEnabled(hwnd: Handle) -> i32;
}

#[link(name = "kernel32", kind = "raw-dylib")]
extern "C" {
    fn GetEnvironmentVariableA(name: *const c_char, buf: *mut c_char, size: u32) -> u32;
}

const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const WS_GROUP: u32 = 0x0002_0000;
const WS_BORDER: u32 = 0x0080_0000;
const WS_EX_CLIENTEDGE: u32 = 0x200;
const WM_COMMAND: u32 = 0x0111;
const WM_SETFONT: u32 = 0x0030;
const WM_TIMER: u32 = 0x0113;
const WM_CLOSE: u32 = 0x0010;

const ID_HOST: i32 = 101;
const ID_PORT: i32 = 102;
const ID_RAW: i32 = 103;
const ID_TELNET: i32 = 104;
const ID_SSH: i32 = 105;
const ID_SESSIONS: i32 = 106;
const ID_CLOSE: i32 = 107;
const ID_OPEN: i32 = 108;
const ID_CANCEL: i32 = 109;
const ID_KEEP: i32 = 110;
const ID_SCROLL: i32 = 111;

static mut CLICKS_OPEN: u32 = 0;
static mut EDIT_CHANGES: u32 = 0;
static mut LIST_CHANGES: u32 = 0;
static mut COMBO_CHANGES: u32 = 0;

unsafe extern "win64" fn wnd_proc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as i32;
            let code = (wparam >> 16) as u16;
            match (id, code) {
                (ID_OPEN, 0) => CLICKS_OPEN += 1,
                (ID_HOST, 0x0300) => EDIT_CHANGES += 1,
                (ID_SESSIONS, 1) => LIST_CHANGES += 1,
                (ID_CLOSE, 1) => COMBO_CHANGES += 1,
                _ => {}
            }
            0
        }
        WM_TIMER => {
            DestroyWindow(hwnd);
            0
        }
        WM_CLOSE => {
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
    text: &core::ffi::CStr,
    style: u32,
    ex: u32,
    id: i32,
    rect: (i32, i32, i32, i32),
) -> Handle {
    let h = CreateWindowExA(
        ex,
        class.as_ptr(),
        text.as_ptr(),
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

fn eq(a: &[u8], b: &[u8]) -> bool {
    a == b
}

/// Ferme la boîte de message comme un utilisateur : touche Entrée.
unsafe extern "win64" fn press_enter(hwnd: Handle, _msg: u32, id: usize, _time: u32) {
    KillTimer(hwnd, id);
    PostMessageW(GetFocus(), 0x0100, 0x0D, 0);
}

fn wide<const N: usize>(s: &[u8; N]) -> [u16; N] {
    let mut out = [0u16; N];
    for (o, &b) in out.iter_mut().zip(s.iter()) {
        *o = b as u16;
    }
    out
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        let class = c"WinluxControlsSuite";
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
            c"Configuration de la connexion".as_ptr(),
            0x00CF_0000 | WS_VISIBLE,
            80,
            60,
            520,
            420,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        check!(!main.is_null(), "fenetre principale");

        control(
            main,
            c"BUTTON",
            c"Destination de la connexion",
            0x7,
            0,
            -1,
            (12, 8, 480, 128),
        );
        control(
            main,
            c"STATIC",
            c"Nom d'&hote (ou adresse IP)",
            0,
            0,
            -1,
            (24, 30, 300, 18),
        );
        let host = control(
            main,
            c"EDIT",
            c"",
            WS_TABSTOP | 0x80,
            WS_EX_CLIENTEDGE,
            ID_HOST,
            (24, 50, 340, 26),
        );
        control(main, c"STATIC", c"Port", 0, 0, -1, (380, 30, 80, 18));
        control(
            main,
            c"EDIT",
            c"22",
            WS_TABSTOP | WS_BORDER | 0x2000,
            0,
            ID_PORT,
            (380, 50, 100, 26),
        );
        control(
            main,
            c"STATIC",
            c"Type de connexion :",
            0,
            0,
            -1,
            (24, 88, 140, 20),
        );
        let raw = control(
            main,
            c"BUTTON",
            c"&Raw",
            0x9 | WS_GROUP | WS_TABSTOP,
            0,
            ID_RAW,
            (170, 86, 70, 22),
        );
        control(
            main,
            c"BUTTON",
            c"&Telnet",
            0x9,
            0,
            ID_TELNET,
            (250, 86, 90, 22),
        );
        control(main, c"BUTTON", c"&SSH", 0x9, 0, ID_SSH, (350, 86, 80, 22));
        check!(!raw.is_null(), "boutons radio crees");

        control(
            main,
            c"BUTTON",
            c"Sessions enregistrees",
            0x7,
            0,
            -1,
            (12, 144, 480, 170),
        );
        let list = control(
            main,
            c"LISTBOX",
            c"",
            0x1 | 0x2 | WS_TABSTOP,
            WS_EX_CLIENTEDGE,
            ID_SESSIONS,
            (24, 166, 300, 136),
        );
        for s in [
            c"Default Settings",
            c"serveur-maison",
            c"raspberry",
            c"nas",
            c"routeur",
            c"vm-winlux",
            c"backup",
        ] {
            SendMessageA(list, 0x180, 0, s.as_ptr() as isize);
        }
        control(
            main,
            c"BUTTON",
            c"&Charger",
            WS_TABSTOP,
            0,
            -1,
            (340, 166, 140, 30),
        );
        control(
            main,
            c"BUTTON",
            c"&Enregistrer",
            WS_TABSTOP,
            0,
            -1,
            (340, 204, 140, 30),
        );
        let scroll = control(
            main,
            c"SCROLLBAR",
            c"",
            0x1,
            0,
            ID_SCROLL,
            (470, 244, 12, 58),
        );

        control(
            main,
            c"STATIC",
            c"Fermer la fenetre a la sortie :",
            0,
            0,
            -1,
            (12, 330, 200, 20),
        );
        let combo = control(
            main,
            c"COMBOBOX",
            c"",
            0x3 | WS_TABSTOP,
            0,
            ID_CLOSE,
            (214, 326, 140, 160),
        );
        for s in [c"Toujours", c"Jamais", c"Seulement si sortie propre"] {
            SendMessageA(combo, 0x143, 0, s.as_ptr() as isize);
        }
        SendMessageA(combo, 0x14E, 2, 0);
        control(
            main,
            c"BUTTON",
            c"Garder ouverte",
            0x3 | WS_TABSTOP,
            0,
            ID_KEEP,
            (364, 328, 130, 22),
        );
        control(
            main,
            c"BUTTON",
            c"&Ouvrir",
            0x1 | WS_TABSTOP,
            0,
            ID_OPEN,
            (300, 362, 92, 30),
        );
        control(
            main,
            c"BUTTON",
            c"Annuler",
            WS_TABSTOP,
            0,
            ID_CANCEL,
            (400, 362, 92, 30),
        );

        // GetDlgItem retrouve chaque contrôle par son identifiant.
        check!(GetDlgItem(main, ID_HOST) == host, "GetDlgItem EDIT");
        check!(GetDlgItem(main, ID_SESSIONS) == list, "GetDlgItem LISTBOX");

        // Texte d'un champ par SetDlgItemTextA / GetDlgItemTextA (ANSI).
        SetDlgItemTextA(main, ID_HOST, c"serveur-maison.local".as_ptr());
        let mut buf = [0u8; 64];
        let n = GetDlgItemTextA(main, ID_HOST, buf.as_mut_ptr().cast(), 64) as usize;
        check!(eq(&buf[..n], b"serveur-maison.local"), "GetDlgItemTextA");
        check!(EDIT_CHANGES >= 1, "EN_CHANGE au parent");

        // Boutons radio : SSH coché décoche les autres du groupe.
        CheckDlgButton(main, ID_RAW, 1);
        SendMessageA(GetDlgItem(main, ID_SSH), 0x00F5, 0, 0); // BM_CLICK
        check!(IsDlgButtonChecked(main, ID_SSH) == 1, "radio SSH cochee");
        check!(IsDlgButtonChecked(main, ID_RAW) == 0, "radio Raw decochee");
        SendMessageA(GetDlgItem(main, ID_KEEP), 0x00F5, 0, 0);
        check!(IsDlgButtonChecked(main, ID_KEEP) == 1, "case a cocher");

        // Liste triée, sélection et texte ANSI.
        check!(SendMessageA(list, 0x18B, 0, 0) == 7, "LB_GETCOUNT");
        let n = SendMessageA(list, 0x189, 0, buf.as_mut_ptr() as isize) as usize;
        check!(eq(&buf[..n], b"backup"), "LB_GETTEXT trie");
        SendMessageA(list, 0x186, 5, 0);
        check!(SendMessageA(list, 0x188, 0, 0) == 5, "LB_SETCURSEL");

        // Liste déroulante.
        let n = SendMessageA(combo, 0x148, 2, buf.as_mut_ptr() as isize) as usize;
        check!(eq(&buf[..n], b"Seulement si sortie propre"), "CB_GETLBTEXT");
        check!(SendMessageA(combo, 0x147, 0, 0) == 2, "CB_GETCURSEL");

        // Barre de défilement.
        SendMessageA(scroll, 0x00E2, 0, 50);
        SendMessageA(scroll, 0x00E0, 20, 1);
        check!(SendMessageA(scroll, 0x00E1, 0, 0) == 20, "SBM_SETPOS");

        // Bouton désactivé puis réactivé, clic simulé.
        let open = GetDlgItem(main, ID_OPEN);
        EnableWindow(open, 0);
        check!(IsWindowEnabled(open) == 0, "EnableWindow FALSE");
        SendMessageA(open, 0x00F5, 0, 0);
        check!(CLICKS_OPEN == 0, "pas de clic sur un bouton desactive");
        EnableWindow(open, 1);
        SendMessageA(open, 0x00F5, 0, 0);
        check!(CLICKS_OPEN == 1, "BN_CLICKED au parent");
        let _ = (LIST_CHANGES, COMBO_CHANGES);

        // Position écran d'un enfant = parent + position relative.
        let mut r: Rect = core::mem::zeroed();
        GetWindowRect(host, &mut r);
        check!(
            r.left == 80 + 24 && r.top == 60 + 50,
            "GetWindowRect d'un enfant"
        );

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
        // MessageBox : vraie boîte modale sous X11 (fermée par Entrée), choix
        // par défaut sans affichage. Les deux donnent OK.
        SetTimer(
            main,
            2,
            if ms > 0 { 1500 } else { 20 },
            press_enter as *const () as usize,
        );
        let text = wide(b"Voulez-vous enregistrer la session \"serveur-maison\" ?\0");
        let caption = wide(b"Connexion\0");
        let answer = MessageBoxW(main, text.as_ptr(), caption.as_ptr(), 0x21); // MB_OKCANCEL | MB_ICONQUESTION
        check!(answer == 1, "MessageBox OK");
        SetFocus(host);
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
