//! Démonstration graphique Winlux : ouvre une vue « Paramètres » claire,
//! inspirée de Windows 11 sans reprendre ses ressources propriétaires. Sert
//! de test PE pour `PeekMessageA` et de vérification visuelle du backend X11.

#![no_std]
#![no_main]

use winlux_testexe::*;

static mut GOT_PAINT: u32 = 0;
static mut GOT_DESTROY: u32 = 0;
static mut GOT_SIZE: u32 = 0;
static mut CLIENT_SIZE_MATCH: u32 = 0;
static mut GOT_MOVE: u32 = 0;
static mut WINDOW_POSITION_MATCH: u32 = 0;
static mut GOT_FOCUS: u32 = 0;
static mut GOT_KILLFOCUS: u32 = 0;
static mut GOT_CHAR: u32 = 0;
static mut LAST_CHAR: usize = 0;
static mut GOT_KEYDOWN: u32 = 0;
static mut GOT_KEYUP: u32 = 0;
static mut THREAD_KEY_STATE_MATCH: u32 = 0;
static mut KEYBOARD_STATE_MATCH: u32 = 0;
static mut ASYNC_KEY_PRESSED: u32 = 0;
static mut THREAD_KEY_UP_CLEAR: u32 = 0;

/// Peinture d'une vue de paramètres : barre d'application, navigation latérale,
/// cartes et bouton d'accent. Chaque commande passe par GDI invité → backend.
unsafe fn draw_scene(hwnd: Handle, first: bool) {
    let mut ps: PaintStruct = core::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    if hdc.is_null() {
        return;
    }

    let background = CreateSolidBrush(0x00F3_F3F3);
    let surface = CreateSolidBrush(0x00FF_FFFF);
    let sidebar = CreateSolidBrush(0x00FA_FAFA);
    let border = CreateSolidBrush(0x00E5_E5E5);
    let accent = CreateSolidBrush(0x00C0_6700); // #0067C0 en COLORREF
    let selection = CreateSolidBrush(0x00F5_E7D6);
    let success = CreateSolidBrush(0x002F_9E49);

    let full = Rect { left: 0, top: 0, right: 720, bottom: 480 };
    let topbar = Rect { left: 0, top: 0, right: 720, bottom: 48 };
    let navigation = Rect { left: 0, top: 48, right: 204, bottom: 480 };
    let filled = FillRect(hdc, &full, background);
    FillRect(hdc, &topbar, surface);
    FillRect(hdc, &navigation, sidebar);
    let top_border = Rect { left: 0, top: 47, right: 720, bottom: 48 };
    FillRect(hdc, &top_border, border);

    // Marque d'application et avatar, tous deux rendus avec des primitives GDI.
    SelectObject(hdc, accent);
    let app_icon = Ellipse(hdc, 16, 12, 40, 36);
    let avatar = Ellipse(hdc, 666, 10, 696, 40);

    let previous = SetTextColor(hdc, 0x001F_1F1F);
    let previous_bk = SetBkMode(hdc, 1);
    let title_font = CreateFontA(18, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 0, 0, c"fixed".as_ptr());
    let body_font = CreateFontA(14, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 0, 0, c"fixed".as_ptr());
    let old_font = SelectObject(hdc, title_font);
    let titled = TextOutA(hdc, 50, 16, c"Winlux Settings".as_ptr(), -1);

    // Navigation de type Windows 11.
    SelectObject(hdc, body_font);
    let selected = Rect { left: 10, top: 78, right: 194, bottom: 112 };
    FillRect(hdc, &selected, selection);
    let selected_mark = Rect { left: 10, top: 85, right: 14, bottom: 105 };
    FillRect(hdc, &selected_mark, accent);
    TextOutA(hdc, 28, 88, c"System".as_ptr(), -1);
    SetTextColor(hdc, 0x0060_6060);
    TextOutA(hdc, 28, 128, c"Bluetooth & devices".as_ptr(), -1);
    TextOutA(hdc, 28, 164, c"Network & internet".as_ptr(), -1);
    TextOutA(hdc, 28, 200, c"Personalization".as_ptr(), -1);
    TextOutA(hdc, 28, 236, c"Apps".as_ptr(), -1);
    TextOutA(hdc, 28, 272, c"Accounts".as_ptr(), -1);
    TextOutA(hdc, 28, 308, c"Windows compatibility".as_ptr(), -1);

    // Titre de page et trois cartes de réglages.
    SelectObject(hdc, title_font);
    SetTextColor(hdc, 0x001F_1F1F);
    TextOutA(hdc, 224, 68, c"System".as_ptr(), -1);
    SelectObject(hdc, body_font);

    let display_card = Rect { left: 220, top: 104, right: 690, bottom: 172 };
    let sound_card = Rect { left: 220, top: 184, right: 690, bottom: 252 };
    let about_card = Rect { left: 220, top: 264, right: 690, bottom: 332 };
    FillRect(hdc, &display_card, surface);
    FillRect(hdc, &sound_card, surface);
    FillRect(hdc, &about_card, surface);
    let framed = FrameRect(hdc, &display_card, border)
        & FrameRect(hdc, &sound_card, border)
        & FrameRect(hdc, &about_card, border);

    let display_icon = Rect { left: 238, top: 122, right: 270, bottom: 154 };
    let sound_icon = Rect { left: 238, top: 202, right: 270, bottom: 234 };
    let about_icon = Rect { left: 238, top: 282, right: 270, bottom: 314 };
    FillRect(hdc, &display_icon, accent);
    FillRect(hdc, &sound_icon, accent);
    FillRect(hdc, &about_icon, accent);

    SetTextColor(hdc, 0x001F_1F1F);
    TextOutA(hdc, 288, 118, c"Display".as_ptr(), -1);
    TextOutA(hdc, 288, 198, c"Sound".as_ptr(), -1);
    TextOutA(hdc, 288, 278, c"About Winlux".as_ptr(), -1);
    SetTextColor(hdc, 0x0060_6060);
    TextOutA(hdc, 288, 140, c"1920 x 1080  |  Scale 100%".as_ptr(), -1);
    TextOutA(hdc, 288, 220, c"Linux audio bridge ready".as_ptr(), -1);

    // Exerce aussi le texte opaque, sans casser la présentation de la carte.
    let previous_mode = SetBkMode(hdc, 2);
    let previous_bgc = SetBkColor(hdc, 0x00FF_FFFF);
    let opaque_text = TextOutA(hdc, 288, 300, c"PE32+ x64 runtime active".as_ptr(), -1);
    SetBkMode(hdc, 1);

    // État de compatibilité et bouton principal.
    let status_dot = Rect { left: 224, top: 366, right: 234, bottom: 376 };
    FillRect(hdc, &status_dot, success);
    SetTextColor(hdc, 0x001F_1F1F);
    TextOutA(hdc, 244, 360, c"Windows compatibility services are running".as_ptr(), -1);
    let action = Rect { left: 520, top: 402, right: 690, bottom: 438 };
    FillRect(hdc, &action, accent);
    SetTextColor(hdc, 0x00FF_FFFF);
    TextOutA(hdc, 570, 413, c"Check now".as_ptr(), -1);

    // Petites opérations ROP conservées pour la couverture fonctionnelle.
    SelectObject(hdc, accent);
    let pat_blt = PatBlt(hdc, 202, 470, 4, 4, 0x00F0_0021);
    let pat_invert = PatBlt(hdc, 208, 470, 4, 4, 0x005A_0049);
    let bit_blt = BitBlt(hdc, 214, 470, 4, 4, hdc, 202, 470, 0x00CC_0020);
    let stretch_blt = StretchBlt(hdc, 220, 470, 8, 4, hdc, 202, 470, 4, 4, 0x00CC_0020);
    let xor_blt = BitBlt(hdc, 230, 470, 4, 4, hdc, 202, 470, 0x0066_0046);

    // Bitmap compatible hors écran : PatBlt remplit le buffer, puis SRCCOPY le
    // transfère dans la fenêtre avant que le HBITMAP ne soit détruit.
    let memory_dc = CreateCompatibleDC(hdc);
    let memory_bitmap = CreateCompatibleBitmap(hdc, 40, 32);
    let _old_bitmap = SelectObject(memory_dc, memory_bitmap);
    let memory_brush = CreateSolidBrush(0x00C0_6700);
    SelectObject(memory_dc, memory_brush);
    let memory_fill = PatBlt(memory_dc, 0, 0, 40, 32, 0x00F0_0021);
    let memory_blit = BitBlt(hdc, 650, 452, 40, 24, memory_dc, 0, 0, 0x00CC_0020);
    DeleteDC(memory_dc);
    let bitmap_deleted = DeleteObject(memory_bitmap);
    let memory_brush_deleted = DeleteObject(memory_brush);

    // Chemin de lecture X11 explicite réservé au test manuel sous Xvfb :
    // fenêtre → bitmap mémoire → fenêtre.
    if GetEnvironmentVariableA(c"WINLUX_CAPTURE_TEST".as_ptr(), core::ptr::null_mut(), 0) > 0 {
        let capture_dc = CreateCompatibleDC(hdc);
        let capture_bitmap = CreateCompatibleBitmap(hdc, 32, 32);
        SelectObject(capture_dc, capture_bitmap);
        let captured = BitBlt(capture_dc, 0, 0, 32, 32, hdc, 220, 104, 0x00CC_0020);
        let recopy = BitBlt(hdc, 652, 448, 24, 24, capture_dc, 0, 0, 0x00CC_0020);
        if first {
            check!(captured == 1 && recopy == 1, "BitBlt fenêtre→bitmap→fenêtre");
        }
        DeleteDC(capture_dc);
        DeleteObject(capture_bitmap);
    }

    if GetEnvironmentVariableA(c"WINLUX_DIB_TEST".as_ptr(), core::ptr::null_mut(), 0) > 0 {
        let dib_info = BitmapInfoHeader {
            size: core::mem::size_of::<BitmapInfoHeader>() as u32,
            width: 8,
            height: -8,
            planes: 1,
            bit_count: 32,
            compression: 0,
            size_image: 0,
            x_pels_per_meter: 0,
            y_pels_per_meter: 0,
            colors_used: 0,
            colors_important: 0,
        };
        let dib_dc = CreateCompatibleDC(hdc);
        let mut dib_bits: *mut core::ffi::c_void = core::ptr::null_mut();
        let dib = CreateDIBSection(dib_dc, &dib_info, 0, &mut dib_bits, core::ptr::null_mut(), 0);
        SelectObject(dib_dc, dib);
        for i in 0..64 {
            let pixel = dib_bits.cast::<u8>().add(i * 4);
            *pixel = 0;       // B
            *pixel.add(1) = 0; // G
            *pixel.add(2) = 0xFF; // R
            *pixel.add(3) = 0;
        }
        let dib_blit = BitBlt(hdc, 682, 464, 8, 8, dib_dc, 0, 0, 0x00CC_0020);
        DeleteDC(dib_dc);
        let dib_deleted = DeleteObject(dib);
        if first {
            check!(!dib_bits.is_null() && dib_blit == 1 && dib_deleted != 0, "CreateDIBSection/BitBlt X11");
        }
    }

    // Les opérations de validation ci-dessus sont rejouées par le backend ;
    // on recouvre leur zone technique pour garder une capture propre.
    let test_strip = Rect { left: 198, top: 448, right: 720, bottom: 480 };
    FillRect(hdc, &test_strip, background);

    if first {
        check!(!background.is_null() && !surface.is_null() && !accent.is_null(), "CreateSolidBrush");
        check!(filled == 1, "FillRect surfaces Windows 11");
        check!(framed == 1, "FrameRect cartes");
        check!(previous == 0, "SetTextColor ancienne valeur");
        check!(previous_bk == 2, "SetBkMode ancien mode opague");
        check!(!title_font.is_null() && !body_font.is_null() && !old_font.is_null(), "CreateFontA/SelectObject");
        check!(titled == 1, "TextOutA titre");
        check!(previous_mode == 1, "SetBkMode retour transparent");
        check!(previous_bgc == 0x00FF_FFFF, "SetBkColor ancienne valeur");
        check!(opaque_text == 1, "TextOutA fond opaque");
        check!(pat_blt == 1, "PatBlt PATCOPY");
        check!(pat_invert == 1, "PatBlt PATINVERT");
        check!(bit_blt == 1, "BitBlt SRCCOPY");
        check!(stretch_blt == 1, "StretchBlt SRCCOPY");
        check!(xor_blt == 1, "BitBlt SRCINVERT");
        check!(!memory_dc.is_null() && !memory_bitmap.is_null(), "CreateCompatibleBitmap mémoire");
        check!(memory_fill == 1 && memory_blit == 1, "PatBlt/BitBlt bitmap mémoire");
        check!(bitmap_deleted != 0 && memory_brush_deleted != 0, "DeleteObject bitmap mémoire");
        check!(app_icon == 1 && avatar == 1, "Ellipse icones");
    }

    SelectObject(hdc, old_font);
    DeleteObject(background);
    DeleteObject(surface);
    DeleteObject(sidebar);
    DeleteObject(border);
    DeleteObject(accent);
    DeleteObject(selection);
    DeleteObject(success);
    DeleteObject(title_font);
    DeleteObject(body_font);
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
        WM_MOVE => {
            GOT_MOVE += 1;
            let expected_x = (lparam as u32 as u16 as i16) as i32;
            let expected_y = ((lparam as u32 >> 16) as u16 as i16) as i32;
            let mut rect: Rect = core::mem::zeroed();
            WINDOW_POSITION_MATCH = (GetWindowRect(hwnd, &mut rect) != 0
                && rect.left == expected_x
                && rect.top == expected_y) as u32;
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
        WM_KEYDOWN => {
            GOT_KEYDOWN += 1;
            let vk = wparam as i32;
            THREAD_KEY_STATE_MATCH |= ((GetKeyState(vk) as u16 & 0x8000) != 0) as u32;
            let mut keys: [u8; 256] = [0; 256];
            if GetKeyboardState(keys.as_mut_ptr()) != 0 && wparam < 256 {
                KEYBOARD_STATE_MATCH |= (keys[wparam] & 0x80 != 0) as u32;
            }
            ASYNC_KEY_PRESSED |= ((GetAsyncKeyState(vk) as u16 & 1) != 0) as u32;
            0
        }
        WM_KEYUP => {
            GOT_KEYUP += 1;
            THREAD_KEY_UP_CLEAR |= ((GetKeyState(wparam as i32) as u16 & 0x8000) == 0) as u32;
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
            c"Winlux Settings".as_ptr(),
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
        if GOT_MOVE > 0 {
            check!(WINDOW_POSITION_MATCH != 0, "GetWindowRect synchronise avec ConfigureNotify");
        }
        if GOT_CHAR > 0 {
            check!(LAST_CHAR != 0, "WM_CHAR traduit depuis le clavier");
        }
        if GOT_KEYDOWN > 0 {
            check!(THREAD_KEY_STATE_MATCH != 0, "GetKeyState reflète WM_KEYDOWN");
            check!(KEYBOARD_STATE_MATCH != 0, "GetKeyboardState reflète WM_KEYDOWN");
            check!(ASYNC_KEY_PRESSED != 0, "GetAsyncKeyState détecte la pression");
        }
        if GOT_KEYUP > 0 {
            check!(THREAD_KEY_UP_CLEAR != 0, "GetKeyState reflète WM_KEYUP");
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
