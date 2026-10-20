//! Démonstration graphique Winlux : ouvre une fenêtre avec le thème « blob »
//! violet-bleu (dégradé vertical + accent), laisse la boucle de messages
//! tourner puis se ferme. Sert de test PE pour `PeekMessageA` et de vérification
//! visuelle du backend X11 (le rendu est contrôlé par les checks ci-dessous).

#![no_std]
#![no_main]

use winlux_testexe::*;

static mut GOT_PAINT: u32 = 0;
static mut GOT_DESTROY: u32 = 0;
static mut GOT_SIZE: u32 = 0;
static mut CLIENT_SIZE_MATCH: u32 = 0;
static mut GOT_FOCUS: u32 = 0;
static mut GOT_KILLFOCUS: u32 = 0;
static mut GOT_CHAR: u32 = 0;
static mut LAST_CHAR: usize = 0;

/// Peinture de la scène de démonstration : panneau, titre, barres d'accent et
/// ellipse « blob ». Chaque commande passe par GDI invité → noyau → backend.
unsafe fn draw_scene(hwnd: Handle, first: bool) {
    let mut ps: PaintStruct = core::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    if hdc.is_null() {
        return;
    }

    // Panneau supérieur gauche (surface plum) et son cadre accent.
    let panel = Rect { left: 24, top: 24, right: 320, bottom: 150 };
    let surface = CreateSolidBrush(0x003A_2555);
    let accent = CreateSolidBrush(0x00C8_6BF0);
    let filled = FillRect(hdc, &panel, surface);
    let framed = FrameRect(hdc, &panel, accent);

    // Titre en clair : fond transparent (SetBkMode(TRANSPARENT) = 1), police du
    // serveur X. Sans ça, la couleur de fond du DC encadrerait les glyphes.
    let previous = SetTextColor(hdc, 0x00FF_FFFF);
    let previous_bk = SetBkMode(hdc, 1);
    let titled = TextOutA(hdc, 48, 56, c"Winlux GDI".as_ptr(), -1);

    // Second libellé en mode opaque (2) : le fond du DC est peint derrière les
    // glyphes ; on le choisit de la couleur du panneau pour rester cohérent.
    let previous_mode = SetBkMode(hdc, 2);
    let previous_bgc = SetBkColor(hdc, 0x003A_2555);
    let opaque_text = TextOutA(hdc, 48, 68, c"fond opaque".as_ptr(), -1);
    SetBkMode(hdc, 1);

    // Deux barres d'accent posées sur le panneau.
    let bars = CreateSolidBrush(0x00B4_4BEE);
    let bar1 = Rect { left: 48, top: 88, right: 300, bottom: 106 };
    let bar2 = Rect { left: 48, top: 112, right: 220, bottom: 130 };
    let bars_filled = FillRect(hdc, &bar1, bars) & FillRect(hdc, &bar2, bars);

    // Accent « blob » : ellipse pleine avec le pinceau sélectionné.
    SelectObject(hdc, accent);
    let blob = Ellipse(hdc, 480, 300, 640, 440);

    if first {
        check!(!surface.is_null() && !accent.is_null() && !bars.is_null(), "CreateSolidBrush");
        check!(filled == 1, "FillRect panneau");
        check!(framed == 1, "FrameRect panneau");
        check!(previous == 0, "SetTextColor ancienne valeur");
        check!(previous_bk == 2, "SetBkMode ancien mode opague");
        check!(titled == 1, "TextOutA titre");
        check!(previous_mode == 1, "SetBkMode retour transparent");
        check!(previous_bgc == 0x00FF_FFFF, "SetBkColor ancienne valeur");
        check!(opaque_text == 1, "TextOutA fond opaque");
        check!(bars_filled == 1, "FillRect barres accent");
        check!(blob == 1, "Ellipse blob");
    }

    DeleteObject(surface);
    DeleteObject(accent);
    DeleteObject(bars);
    EndPaint(hwnd, &ps);
}

unsafe extern "win64" fn wnd_proc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        WM_PAINT => {
            let first = GOT_PAINT == 0;
            GOT_PAINT += 1;
            draw_scene(hwnd, first);
            0
        }
        WM_DESTROY => {
            GOT_DESTROY += 1;
            0
        }
        WM_SIZE => {
            GOT_SIZE += 1;
            let mut client: Rect = core::mem::zeroed();
            let expected_w = (lparam as u32 & 0xFFFF) as i32;
            let expected_h = ((lparam as u32 >> 16) & 0xFFFF) as i32;
            CLIENT_SIZE_MATCH = (GetClientRect(hwnd, &mut client) != 0
                && client.right == expected_w
                && client.bottom == expected_h) as u32;
            0
        }
        WM_SETFOCUS => {
            GOT_FOCUS += 1;
            0
        }
        WM_KILLFOCUS => {
            GOT_KILLFOCUS += 1;
            0
        }
        WM_CHAR => {
            GOT_CHAR += 1;
            LAST_CHAR = wparam;
            0
        }
        _ => DefWindowProcA(hwnd, msg, wparam, lparam),
    }
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        // Durée courte pendant les tests automatiques, plus longue en démo
        // manuelle pour laisser le temps d'observer la fenêtre.
        let testing = GetEnvironmentVariableA(c"WINLUX_TEST".as_ptr(), core::ptr::null_mut(), 0) > 0;
        let duration = if testing { 300u32 } else { 3000 };

        let class = c"WinluxDemoClass";
        let wc = WndClassExA {
            size: core::mem::size_of::<WndClassExA>() as u32,
            style: 0,
            wnd_proc: wnd_proc as *const () as usize,
            cls_extra: 0,
            wnd_extra: 0,
            instance: core::ptr::null_mut(),
            icon: core::ptr::null_mut(),
            cursor: core::ptr::null_mut(),
            background: core::ptr::null_mut(),
            menu_name: core::ptr::null(),
            class_name: class.as_ptr(),
            icon_sm: core::ptr::null_mut(),
        };
        let atom = RegisterClassExA(&wc);
        check!(atom != 0, "RegisterClassExA");

        let hwnd = CreateWindowExA(
            0,
            class.as_ptr(),
            c"Winlux - theme blob".as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            120,
            80,
            720,
            480,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        check!(!hwnd.is_null(), "CreateWindowExA demo");
        ShowWindow(hwnd, SW_SHOWNORMAL);
        UpdateWindow(hwnd);

        // Boucle de messages sur PeekMessageA : on traite ce qui arrive et on
        // pulse sinon, jusqu'à expiration de la durée demandée.
        let start = GetTickCount();
        let mut msg: Msg = core::mem::zeroed();
        while GetTickCount().wrapping_sub(start) < duration {
            if PeekMessageA(&mut msg, core::ptr::null_mut(), 0, 0, 1) > 0 {
                TranslateMessage(&msg);
                DispatchMessageA(&msg);
            } else {
                Sleep(10);
            }
        }

        check!(GOT_PAINT >= 1, "WM_PAINT via PeekMessage");
        if GOT_SIZE > 1 {
            check!(CLIENT_SIZE_MATCH != 0, "GetClientRect synchronise avec ConfigureNotify");
        }
        if GOT_CHAR > 0 {
            check!(LAST_CHAR != 0, "WM_CHAR traduit depuis le clavier");
        }
        if GOT_FOCUS > 0 {
            if GOT_KILLFOCUS > 0 {
                check!(GOT_KILLFOCUS >= 1, "WM_KILLFOCUS traduit depuis FocusOut X11");
            } else {
                check!(GetFocus() == hwnd, "GetFocus synchronise avec FocusIn X11");
            }
        }
        DestroyWindow(hwnd);
        check!(GOT_DESTROY >= 1, "WM_DESTROY destruction");
        finish()
    }
}
