//! Démonstration graphique Winlux : ouvre une vue « Paramètres » claire,
//! inspirée de Windows 11 sans reprendre ses ressources propriétaires. Sert
//! de test PE pour `PeekMessageA` et de vérification visuelle du backend X11.

#![no_std]
#![no_main]
#![allow(static_mut_refs, dead_code)]

use winlux_testexe::*;

#[path = "../shell/desktop.rs"]
mod desktop;
#[path = "../shell/diskcleaner.rs"]
mod diskcleaner;
#[path = "../shell/explorer.rs"]
mod explorer;
#[path = "../shell/gfx.rs"]
mod gfx;
#[path = "../shell/login.rs"]
mod login;
#[path = "../shell/notepad.rs"]
mod notepad;
#[path = "../shell/notifications.rs"]
mod notifications;
#[path = "../shell/settings.rs"]
mod settings;
#[path = "../shell/taskmgr.rs"]
mod taskmgr;
#[path = "../shell/utilities.rs"]
mod utilities;
#[path = "../shell/wm.rs"]
mod wm;

static mut DESKTOP: bool = false;
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
unsafe fn environment_percentage(name: *const core::ffi::c_char, fallback: u32) -> u32 {
    let mut value = [0i8; 16];
    let len = GetEnvironmentVariableA(name, value.as_mut_ptr(), value.len() as u32);
    if len == 0 || len >= value.len() as u32 {
        return fallback;
    }
    let mut parsed = 0u32;
    for index in 0..len as usize {
        let byte = value[index] as u8;
        if !byte.is_ascii_digit() {
            return fallback;
        }
        parsed = parsed
            .saturating_mul(10)
            .saturating_add((byte - b'0') as u32);
    }
    parsed.min(100)
}

/// Variante visuelle du centre de contrôle consacrée aux applications Linux
/// et Windows. Activée par `WINLUX_APPS_DEMO=1` pour produire une capture
/// séparée sans retirer la page de paramètres unifiés.
unsafe fn draw_apps_scene(hdc: Handle) {
    let background = CreateSolidBrush(0x00F3_F3F3);
    let surface = CreateSolidBrush(0x00FF_FFFF);
    let sidebar = CreateSolidBrush(0x00FA_FAFA);
    let border = CreateSolidBrush(0x00E5_E5E5);
    let accent = CreateSolidBrush(0x00C0_6700);
    let selection = CreateSolidBrush(0x00F5_E7D6);
    let linux = CreateSolidBrush(0x002F_9E49);
    let warning = CreateSolidBrush(0x0000_A5FF);
    let secondary = CreateSolidBrush(0x00E8_E8E8);

    let full = Rect {
        left: 0,
        top: 0,
        right: 720,
        bottom: 480,
    };
    let topbar = Rect {
        left: 0,
        top: 0,
        right: 720,
        bottom: 48,
    };
    let navigation = Rect {
        left: 0,
        top: 48,
        right: 204,
        bottom: 480,
    };
    FillRect(hdc, &full, background);
    FillRect(hdc, &topbar, surface);
    FillRect(hdc, &navigation, sidebar);
    let top_border = Rect {
        left: 0,
        top: 47,
        right: 720,
        bottom: 48,
    };
    FillRect(hdc, &top_border, border);

    SelectObject(hdc, accent);
    Ellipse(hdc, 16, 12, 40, 36);
    Ellipse(hdc, 666, 10, 696, 40);

    SetBkMode(hdc, 1);
    SetTextColor(hdc, 0x001F_1F1F);
    let title_font = CreateFontA(
        20,
        0,
        0,
        0,
        700,
        0,
        0,
        0,
        1,
        0,
        0,
        0,
        0,
        c"Segoe UI".as_ptr(),
    );
    let body_font = CreateFontA(
        14,
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        1,
        0,
        0,
        0,
        0,
        c"Segoe UI".as_ptr(),
    );
    let old_font = SelectObject(hdc, title_font);
    TextOutA(hdc, 50, 16, c"Winlux Control Center".as_ptr(), -1);

    SelectObject(hdc, body_font);
    SetTextColor(hdc, 0x0060_6060);
    TextOutA(hdc, 28, 88, c"Overview".as_ptr(), -1);
    TextOutA(hdc, 28, 124, c"Settings".as_ptr(), -1);
    let selected = Rect {
        left: 10,
        top: 150,
        right: 194,
        bottom: 184,
    };
    FillRect(hdc, &selected, selection);
    let selected_mark = Rect {
        left: 10,
        top: 157,
        right: 14,
        bottom: 177,
    };
    FillRect(hdc, &selected_mark, accent);
    SetTextColor(hdc, 0x001F_1F1F);
    TextOutA(hdc, 28, 160, c"Applications".as_ptr(), -1);
    SetTextColor(hdc, 0x0060_6060);
    TextOutA(hdc, 28, 204, c"Linux packages".as_ptr(), -1);
    TextOutA(hdc, 28, 236, c"Windows programs".as_ptr(), -1);
    TextOutA(hdc, 28, 268, c"Updates".as_ptr(), -1);
    TextOutA(hdc, 28, 300, c"Storage".as_ptr(), -1);

    SelectObject(hdc, title_font);
    SetTextColor(hdc, 0x001F_1F1F);
    TextOutA(hdc, 224, 68, c"Applications".as_ptr(), -1);
    SelectObject(hdc, body_font);

    let search = Rect {
        left: 220,
        top: 102,
        right: 690,
        bottom: 140,
    };
    FillRect(hdc, &search, surface);
    FrameRect(hdc, &search, border);
    SetTextColor(hdc, 0x0060_6060);
    TextOutA(
        hdc,
        238,
        114,
        c"Search Linux packages or Windows programs".as_ptr(),
        -1,
    );

    let linux_card = Rect {
        left: 220,
        top: 154,
        right: 448,
        bottom: 400,
    };
    let windows_card = Rect {
        left: 460,
        top: 154,
        right: 690,
        bottom: 400,
    };
    FillRect(hdc, &linux_card, surface);
    FillRect(hdc, &windows_card, surface);
    FrameRect(hdc, &linux_card, border);
    FrameRect(hdc, &windows_card, border);

    let linux_icon = Rect {
        left: 238,
        top: 172,
        right: 270,
        bottom: 204,
    };
    let windows_icon = Rect {
        left: 478,
        top: 172,
        right: 510,
        bottom: 204,
    };
    FillRect(hdc, &linux_icon, linux);
    FillRect(hdc, &windows_icon, accent);
    SetTextColor(hdc, 0x001F_1F1F);
    TextOutA(hdc, 284, 176, c"Linux apps".as_ptr(), -1);
    TextOutA(hdc, 524, 176, c"Windows apps".as_ptr(), -1);
    SetTextColor(hdc, 0x0060_6060);
    TextOutA(hdc, 238, 212, c"APT / Flatpak".as_ptr(), -1);
    TextOutA(hdc, 478, 212, c"EXE / MSI".as_ptr(), -1);

    SetTextColor(hdc, 0x001F_1F1F);
    TextOutA(hdc, 238, 250, c"Firefox".as_ptr(), -1);
    TextOutA(hdc, 238, 300, c"VLC media player".as_ptr(), -1);
    TextOutA(hdc, 478, 250, c"Notepad++".as_ptr(), -1);
    TextOutA(hdc, 478, 300, c"Paint.NET".as_ptr(), -1);
    SetTextColor(hdc, 0x0060_6060);
    TextOutA(hdc, 238, 270, c"Installed".as_ptr(), -1);
    TextOutA(hdc, 238, 320, c"Installed".as_ptr(), -1);
    TextOutA(hdc, 478, 270, c"Installed".as_ptr(), -1);
    FillRect(
        hdc,
        &Rect {
            left: 478,
            top: 320,
            right: 486,
            bottom: 328,
        },
        warning,
    );
    TextOutA(hdc, 492, 316, c"Needs .NET".as_ptr(), -1);

    let linux_install = Rect {
        left: 238,
        top: 352,
        right: 329,
        bottom: 382,
    };
    let linux_remove = Rect {
        left: 339,
        top: 352,
        right: 430,
        bottom: 382,
    };
    let windows_install = Rect {
        left: 478,
        top: 352,
        right: 569,
        bottom: 382,
    };
    let windows_remove = Rect {
        left: 581,
        top: 352,
        right: 672,
        bottom: 382,
    };
    FillRect(hdc, &linux_install, linux);
    FillRect(hdc, &linux_remove, secondary);
    FillRect(hdc, &windows_install, accent);
    FillRect(hdc, &windows_remove, secondary);
    SetTextColor(hdc, 0x00FF_FFFF);
    TextOutA(hdc, 256, 360, c"Install".as_ptr(), -1);
    TextOutA(hdc, 496, 360, c"Install".as_ptr(), -1);
    SetTextColor(hdc, 0x001F_1F1F);
    TextOutA(hdc, 347, 360, c"Uninstall".as_ptr(), -1);
    TextOutA(hdc, 589, 360, c"Uninstall".as_ptr(), -1);

    SetTextColor(hdc, 0x0060_6060);
    TextOutA(
        hdc,
        224,
        424,
        c"Select an installed app to update or uninstall it".as_ptr(),
        -1,
    );

    SelectObject(hdc, old_font);
    DeleteObject(background);
    DeleteObject(surface);
    DeleteObject(sidebar);
    DeleteObject(border);
    DeleteObject(accent);
    DeleteObject(selection);
    DeleteObject(linux);
    DeleteObject(warning);
    DeleteObject(secondary);
    DeleteObject(title_font);
    DeleteObject(body_font);
}

/// Peinture d'une vue de paramètres : barre d'application, navigation latérale,
/// cartes et bouton d'accent. Chaque commande passe par GDI invité → backend.
unsafe fn draw_scene(hwnd: Handle, first: bool) {
    let mut ps: PaintStruct = core::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    if hdc.is_null() {
        return;
    }
    if wm::native_app().is_some() {
        wm::native_draw(hwnd, hdc);
        EndPaint(hwnd, &ps);
        return;
    }

    if GetEnvironmentVariableA(c"WINLUX_DESKTOP_DEMO".as_ptr(), core::ptr::null_mut(), 0) > 0 {
        desktop::draw(hdc);
        EndPaint(hwnd, &ps);
        return;
    }

    if GetEnvironmentVariableA(c"WINLUX_APPS_DEMO".as_ptr(), core::ptr::null_mut(), 0) > 0 {
        draw_apps_scene(hdc);
        EndPaint(hwnd, &ps);
        return;
    }

    let background = CreateSolidBrush(0x00F3_F3F3);
    let surface = CreateSolidBrush(0x00FF_FFFF);
    let sidebar = CreateSolidBrush(0x00FA_FAFA);
    let border = CreateSolidBrush(0x00E5_E5E5);
    let accent = CreateSolidBrush(0x00C0_6700); // #0067C0 en COLORREF
    let selection = CreateSolidBrush(0x00F5_E7D6);
    let success = CreateSolidBrush(0x002F_9E49);

    let full = Rect {
        left: 0,
        top: 0,
        right: 720,
        bottom: 480,
    };
    let topbar = Rect {
        left: 0,
        top: 0,
        right: 720,
        bottom: 48,
    };
    let navigation = Rect {
        left: 0,
        top: 48,
        right: 204,
        bottom: 480,
    };
    let filled = FillRect(hdc, &full, background);
    FillRect(hdc, &topbar, surface);
    FillRect(hdc, &navigation, sidebar);
    let top_border = Rect {
        left: 0,
        top: 47,
        right: 720,
        bottom: 48,
    };
    FillRect(hdc, &top_border, border);

    // Marque d'application et avatar, tous deux rendus avec des primitives GDI.
    SelectObject(hdc, accent);
    let app_icon = Ellipse(hdc, 16, 12, 40, 36);
    let avatar = Ellipse(hdc, 666, 10, 696, 40);

    let previous = SetTextColor(hdc, 0x001F_1F1F);
    let previous_bk = SetBkMode(hdc, 1);
    let title_font = CreateFontA(
        20,
        0,
        0,
        0,
        700,
        0,
        0,
        0,
        1,
        0,
        0,
        0,
        0,
        c"Segoe UI".as_ptr(),
    );
    let body_font = CreateFontA(
        14,
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        1,
        0,
        0,
        0,
        0,
        c"Segoe UI".as_ptr(),
    );
    let old_font = SelectObject(hdc, title_font);
    let titled = TextOutA(hdc, 50, 16, c"Winlux Control Center".as_ptr(), -1);

    // Navigation de type Windows 11.
    SelectObject(hdc, body_font);
    let selected = Rect {
        left: 10,
        top: 78,
        right: 194,
        bottom: 112,
    };
    FillRect(hdc, &selected, selection);
    let selected_mark = Rect {
        left: 10,
        top: 85,
        right: 14,
        bottom: 105,
    };
    FillRect(hdc, &selected_mark, accent);
    TextOutA(hdc, 28, 88, c"Overview".as_ptr(), -1);
    SetTextColor(hdc, 0x0060_6060);
    TextOutA(hdc, 18, 130, c"LINUX".as_ptr(), -1);
    TextOutA(hdc, 28, 158, c"Linux system".as_ptr(), -1);
    TextOutA(hdc, 28, 190, c"Network".as_ptr(), -1);
    TextOutA(hdc, 28, 222, c"Audio & devices".as_ptr(), -1);
    TextOutA(hdc, 18, 266, c"WINDOWS".as_ptr(), -1);
    TextOutA(hdc, 28, 294, c"Compatibility".as_ptr(), -1);
    TextOutA(hdc, 28, 326, c"Windows apps".as_ptr(), -1);
    TextOutA(hdc, 28, 358, c"Registry & DLLs".as_ptr(), -1);

    // Titre de page et trois cartes de réglages.
    SelectObject(hdc, title_font);
    SetTextColor(hdc, 0x001F_1F1F);
    TextOutA(hdc, 224, 68, c"Unified settings".as_ptr(), -1);
    SelectObject(hdc, body_font);

    let linux_card = Rect {
        left: 220,
        top: 104,
        right: 448,
        bottom: 292,
    };
    let windows_card = Rect {
        left: 460,
        top: 104,
        right: 690,
        bottom: 292,
    };
    let shared_card = Rect {
        left: 220,
        top: 306,
        right: 690,
        bottom: 394,
    };
    FillRect(hdc, &linux_card, surface);
    FillRect(hdc, &windows_card, surface);
    FillRect(hdc, &shared_card, surface);
    let framed = FrameRect(hdc, &linux_card, border)
        & FrameRect(hdc, &windows_card, border)
        & FrameRect(hdc, &shared_card, border);

    let linux_icon = Rect {
        left: 238,
        top: 122,
        right: 270,
        bottom: 154,
    };
    let windows_icon = Rect {
        left: 478,
        top: 122,
        right: 510,
        bottom: 154,
    };
    FillRect(hdc, &linux_icon, success);
    FillRect(hdc, &windows_icon, accent);

    SetTextColor(hdc, 0x001F_1F1F);
    TextOutA(hdc, 286, 122, c"Linux".as_ptr(), -1);
    TextOutA(hdc, 526, 122, c"Windows".as_ptr(), -1);
    SetTextColor(hdc, 0x0060_6060);
    TextOutA(hdc, 238, 170, c"Host system".as_ptr(), -1);
    TextOutA(hdc, 238, 198, c"Network & audio".as_ptr(), -1);
    TextOutA(hdc, 238, 226, c"Updates & drivers".as_ptr(), -1);
    TextOutA(hdc, 478, 170, c"EXE compatibility".as_ptr(), -1);
    TextOutA(hdc, 478, 198, c"Runtime & DLLs".as_ptr(), -1);
    TextOutA(hdc, 478, 226, c"Registry & apps".as_ptr(), -1);

    let linux_action = Rect {
        left: 238,
        top: 250,
        right: 426,
        bottom: 278,
    };
    let windows_action = Rect {
        left: 478,
        top: 250,
        right: 672,
        bottom: 278,
    };
    FillRect(hdc, &linux_action, selection);
    FillRect(hdc, &windows_action, selection);
    SetTextColor(hdc, 0x001F_1F1F);
    TextOutA(hdc, 286, 257, c"Open Linux".as_ptr(), -1);
    TextOutA(hdc, 520, 257, c"Windows options".as_ptr(), -1);

    // Exerce aussi le texte opaque, sans casser la présentation de la carte.
    let previous_mode = SetBkMode(hdc, 2);
    let previous_bgc = SetBkColor(hdc, 0x00FF_FFFF);
    let opaque_text = TextOutA(hdc, 244, 326, c"Shared integration".as_ptr(), -1);
    SetBkMode(hdc, 1);

    // État commun et bouton principal.
    let status_dot = Rect {
        left: 226,
        top: 360,
        right: 236,
        bottom: 370,
    };
    FillRect(hdc, &status_dot, success);
    SetTextColor(hdc, 0x0060_6060);
    TextOutA(
        hdc,
        246,
        354,
        c"Files, clipboard and accounts connected".as_ptr(),
        -1,
    );
    let action = Rect {
        left: 520,
        top: 402,
        right: 690,
        bottom: 438,
    };
    FillRect(hdc, &action, accent);
    SetTextColor(hdc, 0x00FF_FFFF);
    TextOutA(hdc, 558, 413, c"Manage both".as_ptr(), -1);

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
            check!(
                captured == 1 && recopy == 1,
                "BitBlt fenêtre→bitmap→fenêtre"
            );
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
        let dib = CreateDIBSection(
            dib_dc,
            &dib_info,
            0,
            &mut dib_bits,
            core::ptr::null_mut(),
            0,
        );
        SelectObject(dib_dc, dib);
        for i in 0..64 {
            let pixel = dib_bits.cast::<u8>().add(i * 4);
            *pixel = 0; // B
            *pixel.add(1) = 0; // G
            *pixel.add(2) = 0xFF; // R
            *pixel.add(3) = 0;
        }
        let dib_blit = BitBlt(hdc, 682, 464, 8, 8, dib_dc, 0, 0, 0x00CC_0020);
        DeleteDC(dib_dc);
        let dib_deleted = DeleteObject(dib);
        if first {
            check!(
                !dib_bits.is_null() && dib_blit == 1 && dib_deleted != 0,
                "CreateDIBSection/BitBlt X11"
            );
        }
    }

    // Les opérations de validation ci-dessus sont rejouées par le backend ;
    // on recouvre leur zone technique pour garder une capture propre.
    let test_strip = Rect {
        left: 198,
        top: 448,
        right: 720,
        bottom: 480,
    };
    FillRect(hdc, &test_strip, background);

    if first {
        check!(
            !background.is_null() && !surface.is_null() && !accent.is_null(),
            "CreateSolidBrush"
        );
        check!(filled == 1, "FillRect surfaces Windows 11");
        check!(framed == 1, "FrameRect cartes");
        check!(previous == 0, "SetTextColor ancienne valeur");
        check!(previous_bk == 2, "SetBkMode ancien mode opague");
        check!(
            !title_font.is_null() && !body_font.is_null() && !old_font.is_null(),
            "CreateFontA/SelectObject"
        );
        check!(titled == 1, "TextOutA titre");
        check!(previous_mode == 1, "SetBkMode retour transparent");
        check!(previous_bgc == 0x00FF_FFFF, "SetBkColor ancienne valeur");
        check!(opaque_text == 1, "TextOutA fond opaque");
        check!(pat_blt == 1, "PatBlt PATCOPY");
        check!(pat_invert == 1, "PatBlt PATINVERT");
        check!(bit_blt == 1, "BitBlt SRCCOPY");
        check!(stretch_blt == 1, "StretchBlt SRCCOPY");
        check!(xor_blt == 1, "BitBlt SRCINVERT");
        check!(
            !memory_dc.is_null() && !memory_bitmap.is_null(),
            "CreateCompatibleBitmap mémoire"
        );
        check!(
            memory_fill == 1 && memory_blit == 1,
            "PatBlt/BitBlt bitmap mémoire"
        );
        check!(
            bitmap_deleted != 0 && memory_brush_deleted != 0,
            "DeleteObject bitmap mémoire"
        );
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
            if wm::native_app().is_some() {
                InvalidateRect(hwnd, core::ptr::null(), 0);
            }
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
            if DESKTOP {
                desktop::on_key(hwnd, 0, wparam as u16);
            } else if wm::native_app().is_some() {
                wm::native_key(hwnd, 0, wparam as u16);
            }
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
            if DESKTOP {
                desktop::on_key(hwnd, wparam as u32, 0);
            } else if wm::native_app().is_some() {
                wm::native_key(hwnd, wparam as u32, 0);
            }
            0
        }
        WM_KEYUP => {
            GOT_KEYUP += 1;
            THREAD_KEY_UP_CLEAR |= ((GetKeyState(wparam as i32) as u16 & 0x8000) == 0) as u32;
            0
        }
        WM_LBUTTONDOWN | WM_MOUSEMOVE | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_MBUTTONDOWN
        | WM_MOUSEWHEEL => {
            if wm::native_app().is_some() {
                let mut x = (lparam as u32 & 0xFFFF) as i16 as i32;
                let mut y = ((lparam as u32 >> 16) & 0xFFFF) as i16 as i32;
                if msg == WM_MOUSEWHEEL {
                    let mut bounds: Rect = core::mem::zeroed();
                    GetWindowRect(hwnd, &mut bounds);
                    x -= bounds.left;
                    y -= bounds.top;
                }
                let ev = match msg {
                    WM_LBUTTONDOWN => wm::Mouse::Down,
                    WM_LBUTTONUP => wm::Mouse::Up,
                    WM_MOUSEMOVE => wm::Mouse::Move,
                    WM_RBUTTONDOWN => wm::Mouse::RightDown,
                    WM_MBUTTONDOWN => wm::Mouse::MiddleDown,
                    _ => wm::Mouse::Wheel(((wparam >> 16) as u16 as i16) as i32),
                };
                wm::native_mouse(hwnd, ev, x, y);
                return 0;
            }
            if DESKTOP {
                let x = (lparam as u32 & 0xFFFF) as i16 as i32;
                let y = ((lparam as u32 >> 16) & 0xFFFF) as i16 as i32;
                match msg {
                    WM_LBUTTONDOWN => desktop::on_button_down(hwnd, x, y),
                    WM_MOUSEMOVE => desktop::on_mouse_move(hwnd, x, y),
                    WM_LBUTTONUP => desktop::on_button_up(hwnd, x, y),
                    WM_RBUTTONDOWN => desktop::on_other(hwnd, wm::Mouse::RightDown, x, y),
                    WM_MBUTTONDOWN => desktop::on_other(hwnd, wm::Mouse::MiddleDown, x, y),
                    _ => desktop::on_other(
                        hwnd,
                        wm::Mouse::Wheel((wparam >> 16) as u16 as i16 as i32),
                        x,
                        y,
                    ),
                }
            }
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
        let testing =
            GetEnvironmentVariableA(c"WINLUX_TEST".as_ptr(), core::ptr::null_mut(), 0) > 0;
        let desktop =
            GetEnvironmentVariableA(c"WINLUX_DESKTOP_DEMO".as_ptr(), core::ptr::null_mut(), 0) > 0;
        DESKTOP = desktop;
        wm::native_init();
        if desktop {
            let mut align = [0i8; 8];
            let align_len = GetEnvironmentVariableA(
                c"WINLUX_TASKBAR_ALIGN".as_ptr(),
                align.as_mut_ptr(),
                align.len() as u32,
            );
            desktop::init(
                environment_percentage(c"WINLUX_TRAY_VOLUME".as_ptr(), 0),
                environment_percentage(c"WINLUX_TRAY_BATTERY".as_ptr(), 0),
                environment_percentage(c"WINLUX_TRAY_BATTERY_PRESENT".as_ptr(), 0).min(1),
                environment_percentage(c"WINLUX_TRAY_NETWORK".as_ptr(), 0).min(1),
                align_len == 4 && align[0] as u8 == b'l',
            );
        }
        let duration = if testing {
            300u32
        } else if desktop || wm::native_app().is_some() {
            u32::MAX
        } else {
            3000
        };

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

        let title = if desktop {
            c"Winlux Desktop"
        } else if wm::native_app() == Some(wm::APP_EXPLORER) {
            c"Explorer Winlux"
        } else if wm::native_app().is_some() {
            c"Parametres Winlux"
        } else {
            c"Winlux Control Center"
        };
        let (window_x, window_y, window_width, window_height) = if desktop {
            (0, 0, desktop::SCREEN_W, desktop::SCREEN_H)
        } else if wm::native_app().is_some() {
            (100, 70, 980, 650)
        } else {
            (120, 80, 720, 480)
        };
        let hwnd = CreateWindowExA(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            window_x,
            window_y,
            window_width,
            window_height,
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
        let mut last_tick = start;
        let mut msg: Msg = core::mem::zeroed();
        while GOT_DESTROY == 0 && GetTickCount().wrapping_sub(start) < duration {
            if PeekMessageA(&mut msg, core::ptr::null_mut(), 0, 0, 1) > 0 {
                TranslateMessage(&msg);
                DispatchMessageA(&msg);
            } else {
                if desktop {
                    desktop::frame(hwnd);
                    if GetTickCount().wrapping_sub(last_tick) >= 500 {
                        last_tick = GetTickCount();
                        desktop::tick(hwnd);
                    }
                } else if wm::native_app().is_some()
                    && GetTickCount().wrapping_sub(last_tick) >= 500
                {
                    last_tick = GetTickCount();
                    wm::native_tick(hwnd);
                }
                Sleep(if desktop { 4 } else { 10 });
            }
        }

        check!(GOT_PAINT >= 1, "WM_PAINT via PeekMessage");
        if GOT_SIZE > 1 {
            check!(
                CLIENT_SIZE_MATCH != 0,
                "GetClientRect synchronise avec ConfigureNotify"
            );
        }
        if GOT_MOVE > 0 {
            check!(
                WINDOW_POSITION_MATCH != 0,
                "GetWindowRect synchronise avec ConfigureNotify"
            );
        }
        if GOT_CHAR > 0 {
            check!(LAST_CHAR != 0, "WM_CHAR traduit depuis le clavier");
        }
        if GOT_KEYDOWN > 0 {
            check!(
                THREAD_KEY_STATE_MATCH != 0,
                "GetKeyState reflète WM_KEYDOWN"
            );
            check!(
                KEYBOARD_STATE_MATCH != 0,
                "GetKeyboardState reflète WM_KEYDOWN"
            );
            check!(
                ASYNC_KEY_PRESSED != 0,
                "GetAsyncKeyState détecte la pression"
            );
        }
        if GOT_KEYUP > 0 {
            check!(THREAD_KEY_UP_CLEAR != 0, "GetKeyState reflète WM_KEYUP");
        }
        if GOT_FOCUS > 0 {
            if GOT_KILLFOCUS > 0 {
                check!(
                    GOT_KILLFOCUS >= 1,
                    "WM_KILLFOCUS traduit depuis FocusOut X11"
                );
            } else {
                check!(GetFocus() == hwnd, "GetFocus synchronise avec FocusIn X11");
            }
        }
        if GOT_DESTROY == 0 {
            DestroyWindow(hwnd);
        }
        check!(GOT_DESTROY >= 1, "WM_DESTROY destruction");
        finish()
    }
}
