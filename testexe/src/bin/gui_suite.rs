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
        check!(
            screen_w == 1920 && screen_h == 1080,
            "GetSystemMetrics ecran"
        );

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
            0,
            class.as_ptr(),
            c"Winlux GUI".as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            100,
            100,
            640,
            480,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        check!(!hwnd.is_null(), "CreateWindowExA");
        MAIN_HWND = hwnd;

        let unicode_title: [u16; 9] = [
            b'W' as u16,
            b'i' as u16,
            b'n' as u16,
            b'l' as u16,
            b'u' as u16,
            b'x' as u16,
            b' ' as u16,
            0x00E9,
            0,
        ];
        let mut title_out = [0u16; 16];
        let title_set = SetWindowTextW(hwnd, unicode_title.as_ptr());
        let title_len = GetWindowTextLengthW(hwnd);
        let title_read = GetWindowTextW(hwnd, title_out.as_mut_ptr(), title_out.len() as i32);
        check!(
            title_set != 0 && title_len == 8 && title_read == 8 && title_out[..9] == unicode_title,
            "titres de fenêtre Unicode W"
        );

        let mut unicode_msg: Msg = core::mem::zeroed();
        let posted_w = PostMessageW(hwnd, 0x8001, 7, 9);
        let peeked_w = PeekMessageW(&mut unicode_msg, hwnd, 0x8001, 0x8001, 1);
        let dispatched_w = DispatchMessageW(&unicode_msg);
        let sent_w = SendMessageW(hwnd, 0, 0, 0);
        let posted_for_get = PostMessageW(hwnd, 0x8002, 11, 13);
        let got_w = GetMessageW(&mut unicode_msg, hwnd, 0x8002, 0x8002);
        check!(
            posted_w != 0
                && peeked_w > 0
                && dispatched_w == 0
                && sent_w == 0
                && posted_for_get != 0
                && got_w > 0
                && unicode_msg.message == 0x8002
                && unicode_msg.wparam == 11
                && unicode_msg.lparam == 13,
            "boucle de messages Unicode W"
        );

        SetLastError(0);
        check!(
            SetWindowPos(
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                0,
                0,
                10,
                10,
                0
            ) == 0
                && GetLastError() == 1400,
            "SetWindowPos rejette HWND invalide"
        );
        SetLastError(0);
        check!(
            SetWindowPos(hwnd, core::ptr::null_mut(), 0, 0, 0, 10, 0) == 0 && GetLastError() == 87,
            "SetWindowPos rejette taille invalide"
        );

        check!(GetFocus().is_null(), "GetFocus initial");
        check!(
            SetFocus(hwnd).is_null() && GetFocus() == hwnd && GOT_SETFOCUS == 1,
            "SetFocus envoie WM_SETFOCUS"
        );
        check!(
            SetFocus(core::ptr::null_mut()) == hwnd && GetFocus().is_null() && GOT_KILLFOCUS == 1,
            "SetFocus(NULL) envoie WM_KILLFOCUS"
        );
        check!(
            SetFocus(hwnd).is_null() && GetFocus() == hwnd && GOT_SETFOCUS == 2,
            "SetFocus reprend le focus"
        );

        let mut keyboard: [u8; 256] = [0; 256];
        check!(GetKeyState(VK_A) == 0, "GetKeyState initial");
        check!(
            GetKeyboardState(keyboard.as_mut_ptr()) != 0 && keyboard[VK_A as usize] == 0,
            "GetKeyboardState initial"
        );
        SetLastError(0);
        check!(
            GetKeyboardState(core::ptr::null_mut()) == 0 && GetLastError() == 87,
            "GetKeyboardState rejette un pointeur nul"
        );
        check!(
            PostMessageA(hwnd, WM_KEYDOWN, VK_A as usize, (30isize << 16) | 1) != 0,
            "PostMessage WM_KEYDOWN"
        );
        let mut key_msg: Msg = core::mem::zeroed();
        check!(
            PeekMessageA(
                &mut key_msg,
                core::ptr::null_mut(),
                WM_KEYDOWN,
                WM_KEYDOWN,
                0
            ) > 0,
            "PeekMessage clavier sans retrait"
        );
        check!(GetKeyState(VK_A) == 0, "GetKeyState inchangé sans retrait");
        check!(
            PeekMessageA(
                &mut key_msg,
                core::ptr::null_mut(),
                WM_KEYDOWN,
                WM_KEYDOWN,
                1
            ) > 0,
            "PeekMessage clavier avec retrait"
        );
        check!(
            GetKeyState(VK_A) as u16 & 0x8000 != 0,
            "GetKeyState bit pressé"
        );
        check!(
            GetKeyboardState(keyboard.as_mut_ptr()) != 0 && keyboard[VK_A as usize] & 0x80 != 0,
            "GetKeyboardState bit pressé"
        );
        check!(
            PostMessageA(
                hwnd,
                WM_KEYUP,
                VK_A as usize,
                (30isize << 16) | (1 << 30) | (1 << 31)
            ) != 0,
            "PostMessage WM_KEYUP"
        );
        check!(
            PeekMessageA(&mut key_msg, core::ptr::null_mut(), WM_KEYUP, WM_KEYUP, 1) > 0,
            "PeekMessage relâchement avec retrait"
        );
        check!(
            GetKeyState(VK_A) as u16 & 0x8000 == 0,
            "GetKeyState bit relâché"
        );

        check!(GetSysColor(5) == 0x00FF_FFFF, "GetSysColor COLOR_WINDOW");
        check!(GetSysColor(-1) == 0, "GetSysColor index inconnu");
        let sys_brush = GetSysColorBrush(5);
        check!(
            !sys_brush.is_null() && sys_brush == GetSysColorBrush(5),
            "GetSysColorBrush stable"
        );
        let sys_hdc = GetDC(hwnd);
        let sys_rect = Rect {
            left: 0,
            top: 0,
            right: 2,
            bottom: 2,
        };
        check!(
            !sys_hdc.is_null() && FillRect(sys_hdc, &sys_rect, sys_brush) == 1,
            "FillRect avec pinceau système"
        );
        check!(
            DeleteObject(sys_brush) == 0,
            "DeleteObject refuse le pinceau système"
        );
        ReleaseDC(hwnd, sys_hdc);

        let fixed_w: [u16; 6] = [
            b'f' as u16,
            b'i' as u16,
            b'x' as u16,
            b'e' as u16,
            b'd' as u16,
            0,
        ];
        let font_w = CreateFontW(14, 0, 0, 0, 400, 0, 1, 0, 1, 0, 0, 0, 0, fixed_w.as_ptr());
        let mut font_info: LogFontW = core::mem::zeroed();
        check!(
            GetObjectW(
                font_w,
                core::mem::size_of::<LogFontW>() as i32,
                &mut font_info as *mut _ as *mut core::ffi::c_void
            ) == core::mem::size_of::<LogFontW>() as i32
                && font_info.height == 14
                && font_info.face[0] == b'f' as u16,
            "GetObjectW LOGFONT"
        );
        let font_hdc = GetDC(hwnd);
        let old_font = SelectObject(font_hdc, font_w);
        check!(
            !font_w.is_null() && !old_font.is_null(),
            "CreateFontW/SelectObject"
        );
        SelectObject(font_hdc, old_font);
        check!(
            DeleteObject(font_w) != 0,
            "DeleteObject police deselectionnee"
        );
        ReleaseDC(hwnd, font_hdc);

        let memory_dc = CreateCompatibleDC(core::ptr::null_mut());
        let bitmap = CreateCompatibleBitmap(memory_dc, 8, 8);
        let mut bitmap_info: BitmapObject = core::mem::zeroed();
        check!(
            GetObjectA(
                bitmap,
                core::mem::size_of::<BitmapObject>() as i32,
                &mut bitmap_info as *mut _ as *mut core::ffi::c_void
            ) == core::mem::size_of::<BitmapObject>() as i32
                && bitmap_info.bm_width == 8
                && bitmap_info.bm_height == 8
                && bitmap_info.bm_bits.is_null(),
            "GetObjectA BITMAP compatible"
        );
        let old_bitmap = SelectObject(memory_dc, bitmap);
        let bitmap_brush = CreateSolidBrush(0x0000_FF00);
        SelectObject(memory_dc, bitmap_brush);
        check!(
            !memory_dc.is_null() && !bitmap.is_null(),
            "CreateCompatibleDC/Bitmap"
        );
        check!(
            PatBlt(memory_dc, 0, 0, 8, 8, 0x00F0_0021) != 0,
            "PatBlt sur bitmap mémoire"
        );
        let blit_dc = GetDC(hwnd);
        check!(
            BitBlt(blit_dc, 10, 10, 8, 8, memory_dc, 0, 0, 0x00CC_0020) != 0,
            "BitBlt depuis bitmap mémoire"
        );
        check!(
            StretchBlt(blit_dc, 30, 10, 16, 16, memory_dc, 0, 0, 8, 8, 0x00CC_0020) != 0,
            "StretchBlt depuis bitmap mémoire"
        );
        ReleaseDC(hwnd, blit_dc);
        DeleteDC(memory_dc);
        check!(
            DeleteObject(bitmap) != 0 && DeleteObject(bitmap_brush) != 0,
            "DeleteObject bitmap memoire"
        );
        let _ = old_bitmap;

        let dib_dc = CreateCompatibleDC(core::ptr::null_mut());
        let dib_info = BitmapInfoHeader {
            size: core::mem::size_of::<BitmapInfoHeader>() as u32,
            width: 4,
            height: -4, // DIB top-down
            planes: 1,
            bit_count: 32,
            compression: 0, // BI_RGB
            size_image: 0,
            x_pels_per_meter: 0,
            y_pels_per_meter: 0,
            colors_used: 0,
            colors_important: 0,
        };
        let mut dib_bits: *mut core::ffi::c_void = core::ptr::null_mut();
        let dib = CreateDIBSection(
            dib_dc,
            &dib_info,
            0,
            &mut dib_bits,
            core::ptr::null_mut(),
            0,
        );
        let mut dib_object: BitmapObject = core::mem::zeroed();
        check!(
            GetObjectA(
                dib,
                core::mem::size_of::<BitmapObject>() as i32,
                &mut dib_object as *mut _ as *mut core::ffi::c_void
            ) == core::mem::size_of::<BitmapObject>() as i32
                && dib_object.bm_width == 4
                && dib_object.bm_height == 4
                && dib_object.bm_bits == dib_bits,
            "GetObjectA DIBSECTION"
        );
        SelectObject(dib_dc, dib);
        for pixel in 0..16 {
            let p = dib_bits.cast::<u8>().add(pixel * 4);
            *p = 0; // B
            *p.add(1) = 0; // G
            *p.add(2) = 0xFF; // R
            *p.add(3) = 0;
        }
        let dib_dst = GetDC(hwnd);
        check!(
            !dib.is_null() && !dib_bits.is_null(),
            "CreateDIBSection BI_RGB 32bpp"
        );
        check!(
            BitBlt(dib_dst, 50, 50, 4, 4, dib_dc, 0, 0, 0x00CC_0020) != 0,
            "BitBlt depuis DIB section"
        );
        ReleaseDC(hwnd, dib_dst);
        DeleteDC(dib_dc);
        check!(DeleteObject(dib) != 0, "DeleteObject DIB deselectionnee");

        check!(IsWindow(hwnd) != 0, "IsWindow apres creation");

        let mut rect: Rect = Default::default();
        check!(GetWindowRect(hwnd, &mut rect) != 0, "GetWindowRect");
        check!(
            rect.right - rect.left == 640 && rect.bottom - rect.top == 480,
            "dimensions fenetre"
        );

        let mut client: Rect = Default::default();
        check!(GetClientRect(hwnd, &mut client) != 0, "GetClientRect");
        check!(
            client.right == 640 && client.bottom == 480,
            "dimensions client"
        );

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
        check!(
            SetWindowPos(hwnd, core::ptr::null_mut(), 140, 160, 800, 520, 0) != 0,
            "SetWindowPos move+size"
        );
        check!(
            GetWindowRect(hwnd, &mut rect) != 0,
            "GetWindowRect apres SetWindowPos"
        );
        check!(
            rect.left == 140 && rect.top == 160 && rect.right == 940 && rect.bottom == 680,
            "rectangle apres SetWindowPos"
        );
        check!(
            GetClientRect(hwnd, &mut client) != 0 && client.right == 800 && client.bottom == 520,
            "client apres SetWindowPos"
        );
        while PeekMessageA(&mut msg, core::ptr::null_mut(), WM_MOVE, WM_SIZE, 1) > 0 {
            DispatchMessageA(&msg);
        }
        check!(
            GOT_SIZE == size_before + 1 && LAST_SIZE as u32 == ((520u32 << 16) | 800),
            "WM_SIZE apres SetWindowPos"
        );
        check!(
            GOT_MOVE == move_before + 1 && LAST_MOVE as u32 == ((160u32 << 16) | 140),
            "WM_MOVE apres SetWindowPos"
        );

        // SWP_NOSIZE doit préserver les dimensions même si cx/cy sont différents.
        let size_before = GOT_SIZE;
        let move_before = GOT_MOVE;
        check!(
            SetWindowPos(hwnd, core::ptr::null_mut(), 200, 220, 1, 1, SWP_NOSIZE) != 0,
            "SetWindowPos SWP_NOSIZE"
        );
        check!(
            GetWindowRect(hwnd, &mut rect) != 0
                && rect.left == 200
                && rect.top == 220
                && rect.right == 1000
                && rect.bottom == 740,
            "SWP_NOSIZE conserve la taille"
        );
        while PeekMessageA(&mut msg, core::ptr::null_mut(), WM_MOVE, WM_SIZE, 1) > 0 {
            DispatchMessageA(&msg);
        }
        check!(
            GOT_MOVE == move_before + 1 && GOT_SIZE == size_before,
            "SWP_NOSIZE publie seulement WM_MOVE"
        );

        // SWP_NOMOVE conserve la position mais applique la nouvelle taille.
        let size_before = GOT_SIZE;
        let move_before = GOT_MOVE;
        check!(
            SetWindowPos(hwnd, core::ptr::null_mut(), 0, 0, 900, 550, SWP_NOMOVE) != 0,
            "SetWindowPos SWP_NOMOVE"
        );
        check!(
            GetWindowRect(hwnd, &mut rect) != 0
                && rect.left == 200
                && rect.top == 220
                && rect.right == 1100
                && rect.bottom == 770,
            "SWP_NOMOVE conserve la position"
        );
        while PeekMessageA(&mut msg, core::ptr::null_mut(), WM_MOVE, WM_SIZE, 1) > 0 {
            DispatchMessageA(&msg);
        }
        check!(
            GOT_SIZE == size_before + 1 && GOT_MOVE == move_before,
            "SWP_NOMOVE publie seulement WM_SIZE"
        );

        let move_before = GOT_MOVE;
        check!(
            MoveWindow(hwnd, 210, 230, 900, 550, 0) != 0,
            "MoveWindow sans repaint"
        );
        check!(
            GetWindowRect(hwnd, &mut rect) != 0
                && rect.left == 210
                && rect.top == 230
                && rect.right == 1110
                && rect.bottom == 780,
            "MoveWindow met a jour le rectangle"
        );
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
