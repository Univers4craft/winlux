//! Test PE de la couche GUI : enregistre une classe, crée une fenêtre, fait
//! tourner la boucle de messages et vérifie la séquence WM_CREATE / WM_SIZE /
//! WM_PAINT / WM_DESTROY. Backend headless : aucune fenêtre réelle n'est
//! affichée, mais la sémantique des messages est exercée de bout en bout.

#![no_std]
#![no_main]

use winlux_testexe::*;

// Compteurs partagés avec le wndproc (variables statiques).
static mut GOT_CREATE: u32 = 0;
static mut GOT_SIZE: u32 = 0;
static mut GOT_PAINT: u32 = 0;
static mut GOT_DESTROY: u32 = 0;
static mut MAIN_HWND: Handle = core::ptr::null_mut();

unsafe extern "win64" fn wnd_proc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        WM_CREATE => {
            GOT_CREATE += 1;
            0
        }
        WM_SIZE => {
            GOT_SIZE += 1;
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

    // Destruction explicite → WM_DESTROY.
    DestroyWindow(hwnd);
    check!(GOT_DESTROY >= 1, "WM_DESTROY recu");
    check!(IsWindow(hwnd) == 0, "IsWindow apres destruction");

    finish()
    }
}
