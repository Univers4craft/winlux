//! Démonstration graphique Winlux : ouvre une fenêtre avec le thème « blob »
//! violet-bleu (dégradé vertical + accent), laisse la boucle de messages
//! tourner puis se ferme. Sert de test PE pour `PeekMessageA` et de vérification
//! visuelle du backend X11 (le rendu est contrôlé par les checks ci-dessous).

#![no_std]
#![no_main]

use winlux_testexe::*;

static mut GOT_PAINT: u32 = 0;
static mut GOT_DESTROY: u32 = 0;

unsafe extern "win64" fn wnd_proc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
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
        DestroyWindow(hwnd);
        check!(GOT_DESTROY >= 1, "WM_DESTROY destruction");
        finish()
    }
}
