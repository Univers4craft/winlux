//! Test PE de la couche GUI : enregistre une classe, crée une fenêtre, fait
//! tourner la boucle de messages et vérifie la séquence WM_CREATE / WM_SIZE /
//! WM_PAINT / WM_DESTROY. Backend headless : aucune fenêtre réelle n'est
//! affichée, mais la sémantique des messages est exercée de bout en bout.

#![no_std]
#![no_main]

use winlux_testexe::*;

// Compteurs partagés avec le wndproc (variables statiques).
static mut GOT_CREATE: u32 = 0;
static mut GOT_MOVE: u32 = 0;
static mut GOT_SETFOCUS: u32 = 0;
static mut GOT_KILLFOCUS: u32 = 0;
static mut GOT_SIZE: u32 = 0;
static mut GOT_PAINT: u32 = 0;
static mut GOT_DESTROY: u32 = 0;
static mut LAST_MOVE: isize = 0;
static mut LAST_SIZE: isize = 0;
static mut MAIN_HWND: Handle = core::ptr::null_mut();

unsafe extern "win64" fn wnd_proc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        WM_CREATE => {
            GOT_CREATE += 1;
            0
        }
        WM_SIZE => {
            GOT_SIZE += 1;
            LAST_SIZE = lparam;
            0
        }
        WM_MOVE => {
            GOT_MOVE += 1;
            LAST_MOVE = lparam;
            0
        }
        WM_SETFOCUS => {
            GOT_SETFOCUS += 1;
            0
        }
        WM_KILLFOCUS => {
            GOT_KILLFOCUS += 1;
            0
        }
        WM_PAINT => {
            GOT_PAINT += 1;
            let mut ps: PaintStruct = core::mem::zeroed();
            BeginPaint(hwnd, &mut ps);
            EndPaint(hwnd, &ps);
            0
        }
        WM_DESTROY => {
            GOT_DESTROY += 1;
            0
        }
        _ => DefWindowProcA(hwnd, msg, wparam, lparam),
    }
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
    // Largeur/hauteur par défaut de l'écran (métriques).
    let screen_w = GetSystemMetrics(0);
    let screen_h = GetSystemMetrics(1);
    check!(screen_w == 1920 && screen_h == 1080, "GetSystemMetrics ecran");

    // Enregistrement de la classe.
    let class = c"GuiSuiteClass";
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

    // Création d'une fenêtre visible.
    let hwnd = CreateWindowExA(
        0, class.as_ptr(), c"Winlux GUI".as_ptr(), WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        100, 100, 640, 480, core::ptr::null_mut(), core::ptr::null_mut(), core::ptr::null_mut(), core::ptr::null_mut(),
    );
    check!(!hwnd.is_null(), "CreateWindowExA");
    MAIN_HWND = hwnd;

    SetLastError(0);
    check!(SetWindowPos(core::ptr::null_mut(), core::ptr::null_mut(), 0, 0, 10, 10, 0) == 0 && GetLastError() == 1400, "SetWindowPos rejette HWND invalide");
    SetLastError(0);
    check!(SetWindowPos(hwnd, core::ptr::null_mut(), 0, 0, 0, 10, 0) == 0 && GetLastError() == 87, "SetWindowPos rejette taille invalide");

    check!(GetFocus().is_null(), "GetFocus initial");
    check!(SetFocus(hwnd).is_null() && GetFocus() == hwnd && GOT_SETFOCUS == 1, "SetFocus envoie WM_SETFOCUS");
    check!(SetFocus(core::ptr::null_mut()) == hwnd && GetFocus().is_null() && GOT_KILLFOCUS == 1, "SetFocus(NULL) envoie WM_KILLFOCUS");
    check!(SetFocus(hwnd).is_null() && GetFocus() == hwnd && GOT_SETFOCUS == 2, "SetFocus reprend le focus");

    let mut keyboard: [u8; 256] = [0; 256];
    check!(GetKeyState(VK_A) == 0, "GetKeyState initial");
    check!(GetKeyboardState(keyboard.as_mut_ptr()) != 0 && keyboard[VK_A as usize] == 0, "GetKeyboardState initial");
    SetLastError(0);
    check!(GetKeyboardState(core::ptr::null_mut()) == 0 && GetLastError() == 87, "GetKeyboardState rejette un pointeur nul");
    check!(PostMessageA(hwnd, WM_KEYDOWN, VK_A as usize, (30isize << 16) | 1) != 0, "PostMessage WM_KEYDOWN");
    let mut key_msg: Msg = core::mem::zeroed();
    check!(PeekMessageA(&mut key_msg, core::ptr::null_mut(), WM_KEYDOWN, WM_KEYDOWN, 0) > 0, "PeekMessage clavier sans retrait");
    check!(GetKeyState(VK_A) == 0, "GetKeyState inchangé sans retrait");
    check!(PeekMessageA(&mut key_msg, core::ptr::null_mut(), WM_KEYDOWN, WM_KEYDOWN, 1) > 0, "PeekMessage clavier avec retrait");
    check!(GetKeyState(VK_A) as u16 & 0x8000 != 0, "GetKeyState bit pressé");
    check!(GetKeyboardState(keyboard.as_mut_ptr()) != 0 && keyboard[VK_A as usize] & 0x80 != 0, "GetKeyboardState bit pressé");
    check!(PostMessageA(hwnd, WM_KEYUP, VK_A as usize, (30isize << 16) | (1 << 30) | (1 << 31)) != 0, "PostMessage WM_KEYUP");
    check!(PeekMessageA(&mut key_msg, core::ptr::null_mut(), WM_KEYUP, WM_KEYUP, 1) > 0, "PeekMessage relâchement avec retrait");
    check!(GetKeyState(VK_A) as u16 & 0x8000 == 0, "GetKeyState bit relâché");

    check!(GetSysColor(5) == 0x003A_2555, "GetSysColor COLOR_WINDOW");
    check!(GetSysColor(-1) == 0, "GetSysColor index inconnu");
    let sys_brush = GetSysColorBrush(5);
    check!(!sys_brush.is_null() && sys_brush == GetSysColorBrush(5), "GetSysColorBrush stable");
    let sys_hdc = GetDC(hwnd);
    let sys_rect = Rect { left: 0, top: 0, right: 2, bottom: 2 };
    check!(!sys_hdc.is_null() && FillRect(sys_hdc, &sys_rect, sys_brush) == 1, "FillRect avec pinceau système");
    check!(DeleteObject(sys_brush) == 0, "DeleteObject refuse le pinceau système");
    ReleaseDC(hwnd, sys_hdc);

    let fixed_w: [u16; 6] = [b'f' as u16, b'i' as u16, b'x' as u16, b'e' as u16, b'd' as u16, 0];
    let font_w = CreateFontW(14, 0, 0, 0, 400, 0, 1, 0, 1, 0, 0, 0, 0, fixed_w.as_ptr());
    let font_hdc = GetDC(hwnd);
    let old_font = SelectObject(font_hdc, font_w);
    check!(!font_w.is_null() && !old_font.is_null(), "CreateFontW/SelectObject");
    SelectObject(font_hdc, old_font);
    check!(DeleteObject(font_w) != 0, "DeleteObject police deselectionnee");
    ReleaseDC(hwnd, font_hdc);

    check!(IsWindow(hwnd) != 0, "IsWindow apres creation");

    let mut rect: Rect = Default::default();
    check!(GetWindowRect(hwnd, &mut rect) != 0, "GetWindowRect");
    check!(rect.right - rect.left == 640 && rect.bottom - rect.top == 480, "dimensions fenetre");

    let mut client: Rect = Default::default();
    check!(GetClientRect(hwnd, &mut client) != 0, "GetClientRect");
    check!(client.right == 640 && client.bottom == 480, "dimensions client");

    ShowWindow(hwnd, SW_SHOW);
    UpdateWindow(hwnd);

    // Boucle de messages : consomme tout ce qui est en file, jusqu'à WM_QUIT
    // (ou fin de file en headless). Plafond de sécurité pour ne pas tourner à vide.
    let mut msg: Msg = core::mem::zeroed();
    let mut iterations = 0u32;
    loop {
        let r = GetMessageA(&mut msg, core::ptr::null_mut(), 0, 0);
        if r == 0 || r == -1 {
            break;
        }
        TranslateMessage(&msg);
        DispatchMessageA(&msg);
        iterations += 1;
        if iterations > 64 {
            break;
        }
        // Messages attendus déjà traités : on ne va pas bloquer dans GetMessage
        // en attente d'événements X11 supplémentaires.
        if GOT_PAINT >= 1 && GOT_SIZE >= 1 && GOT_CREATE >= 1 {
            break;
        }
    }

    check!(GOT_CREATE >= 1, "WM_CREATE recu");
    check!(GOT_SIZE >= 1, "WM_SIZE recu");
    check!(GOT_PAINT >= 1, "WM_PAINT recu");

    // SetWindowPos déplace et redimensionne la fenêtre, actualise les rectangles
    // puis publie WM_SIZE/WM_MOVE dans la file du thread.
    let size_before = GOT_SIZE;
    let move_before = GOT_MOVE;
    check!(SetWindowPos(hwnd, core::ptr::null_mut(), 140, 160, 800, 520, 0) != 0, "SetWindowPos move+size");
    check!(GetWindowRect(hwnd, &mut rect) != 0, "GetWindowRect apres SetWindowPos");
    check!(rect.left == 140 && rect.top == 160 && rect.right == 940 && rect.bottom == 680, "rectangle apres SetWindowPos");
    check!(GetClientRect(hwnd, &mut client) != 0 && client.right == 800 && client.bottom == 520, "client apres SetWindowPos");
    while PeekMessageA(&mut msg, core::ptr::null_mut(), WM_MOVE, WM_SIZE, 1) > 0 {
        DispatchMessageA(&msg);
    }
    check!(GOT_SIZE == size_before + 1 && LAST_SIZE as u32 == ((520u32 << 16) | 800), "WM_SIZE apres SetWindowPos");
    check!(GOT_MOVE == move_before + 1 && LAST_MOVE as u32 == ((160u32 << 16) | 140), "WM_MOVE apres SetWindowPos");

    // SWP_NOSIZE doit préserver les dimensions même si cx/cy sont différents.
    let size_before = GOT_SIZE;
    let move_before = GOT_MOVE;
    check!(SetWindowPos(hwnd, core::ptr::null_mut(), 200, 220, 1, 1, SWP_NOSIZE) != 0, "SetWindowPos SWP_NOSIZE");
    check!(GetWindowRect(hwnd, &mut rect) != 0 && rect.left == 200 && rect.top == 220 && rect.right == 1000 && rect.bottom == 740, "SWP_NOSIZE conserve la taille");
    while PeekMessageA(&mut msg, core::ptr::null_mut(), WM_MOVE, WM_SIZE, 1) > 0 {
        DispatchMessageA(&msg);
    }
    check!(GOT_MOVE == move_before + 1 && GOT_SIZE == size_before, "SWP_NOSIZE publie seulement WM_MOVE");

    // SWP_NOMOVE conserve la position mais applique la nouvelle taille.
    let size_before = GOT_SIZE;
    let move_before = GOT_MOVE;
    check!(SetWindowPos(hwnd, core::ptr::null_mut(), 0, 0, 900, 550, SWP_NOMOVE) != 0, "SetWindowPos SWP_NOMOVE");
    check!(GetWindowRect(hwnd, &mut rect) != 0 && rect.left == 200 && rect.top == 220 && rect.right == 1100 && rect.bottom == 770, "SWP_NOMOVE conserve la position");
    while PeekMessageA(&mut msg, core::ptr::null_mut(), WM_MOVE, WM_SIZE, 1) > 0 {
        DispatchMessageA(&msg);
    }
    check!(GOT_SIZE == size_before + 1 && GOT_MOVE == move_before, "SWP_NOMOVE publie seulement WM_SIZE");

    let move_before = GOT_MOVE;
    check!(MoveWindow(hwnd, 210, 230, 900, 550, 0) != 0, "MoveWindow sans repaint");
    check!(GetWindowRect(hwnd, &mut rect) != 0 && rect.left == 210 && rect.top == 230 && rect.right == 1110 && rect.bottom == 780, "MoveWindow met a jour le rectangle");
    while PeekMessageA(&mut msg, core::ptr::null_mut(), WM_MOVE, WM_SIZE, 1) > 0 {
        DispatchMessageA(&msg);
    }
    check!(GOT_MOVE == move_before + 1, "MoveWindow publie WM_MOVE");

    // Destruction explicite → WM_DESTROY.
    DestroyWindow(hwnd);
    check!(GOT_DESTROY >= 1, "WM_DESTROY recu");
    check!(GOT_KILLFOCUS >= 2, "DestroyWindow retire le focus");
    check!(IsWindow(hwnd) == 0, "IsWindow apres destruction");

    finish()
    }
}
