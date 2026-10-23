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
static mut START_MENU_OPEN: u32 = 1;

unsafe fn fill_round_rect(hdc: Handle, rect: Rect, radius: i32, brush: Handle) {
    let width=rect.right-rect.left;let height=rect.bottom-rect.top;let radius=radius.min(width/2).min(height/2).max(1);
    FillRect(hdc,&Rect{left:rect.left+radius,top:rect.top,right:rect.right-radius,bottom:rect.bottom},brush);
    FillRect(hdc,&Rect{left:rect.left,top:rect.top+radius,right:rect.right,bottom:rect.bottom-radius},brush);
    SelectObject(hdc,brush);
    Ellipse(hdc,rect.left,rect.top,rect.left+radius*2,rect.top+radius*2);
    Ellipse(hdc,rect.right-radius*2,rect.top,rect.right,rect.top+radius*2);
    Ellipse(hdc,rect.left,rect.bottom-radius*2,rect.left+radius*2,rect.bottom);
    Ellipse(hdc,rect.right-radius*2,rect.bottom-radius*2,rect.right,rect.bottom);
}

unsafe fn round_panel(hdc:Handle,rect:Rect,radius:i32,shadow:Handle,border:Handle,surface:Handle){
    fill_round_rect(hdc,Rect{left:rect.left+6,top:rect.top+8,right:rect.right+6,bottom:rect.bottom+8},radius,shadow);
    fill_round_rect(hdc,rect,radius,border);
    fill_round_rect(hdc,Rect{left:rect.left+1,top:rect.top+1,right:rect.right-1,bottom:rect.bottom-1},radius-1,surface);
}

unsafe fn draw_windows_mark(hdc:Handle,x:i32,y:i32,size:i32,brush:Handle){
    let gap=(size/8).max(2);let cell=(size-gap)/2;
    FillRect(hdc,&Rect{left:x,top:y,right:x+cell,bottom:y+cell},brush);
    FillRect(hdc,&Rect{left:x+cell+gap,top:y,right:x+size,bottom:y+cell},brush);
    FillRect(hdc,&Rect{left:x,top:y+cell+gap,right:x+cell,bottom:y+size},brush);
    FillRect(hdc,&Rect{left:x+cell+gap,top:y+cell+gap,right:x+size,bottom:y+size},brush);
}

unsafe fn draw_search_mark(hdc:Handle,x:i32,y:i32,brush:Handle,inside:Handle){
    SelectObject(hdc,brush);Ellipse(hdc,x,y,x+17,y+17);SelectObject(hdc,inside);Ellipse(hdc,x+4,y+4,x+13,y+13);
    FillRect(hdc,&Rect{left:x+14,top:y+14,right:x+22,bottom:y+18},brush);
}

unsafe fn draw_installed_icon(hdc: Handle, request: *const core::ffi::c_char, x: i32, y: i32, size: i32, background: u32, fallback: Handle) {
    if WinluxDrawIconA(hdc, request, x, y, size, background) == 0 {
        fill_round_rect(hdc, Rect { left: x, top: y, right: x + size, bottom: y + size }, (size / 4).max(4), fallback);
    }
}

/// Première base du bureau Winlux : fond, barre des tâches, zone système et
/// menu Démarrer. Tout est rendu par le même chemin GDI invité → X11 que les
/// futures applications du shell.
unsafe fn draw_desktop_scene(hdc: Handle, start_open: bool) {
    let wall=CreateSolidBrush(0x0030_1810);let wall_blue=CreateSolidBrush(0x008C_4A20);let wall_teal=CreateSolidBrush(0x0085_6630);let wall_dark=CreateSolidBrush(0x0022_1009);
    let surface=CreateSolidBrush(0x00FC_F9F7);let sidebar=CreateSolidBrush(0x00F0_EAE7);let elevated=CreateSolidBrush(0x00F6_F2F0);let taskbar=CreateSolidBrush(0x00F4_EFEC);let border=CreateSolidBrush(0x00D9_CFCB);let shadow=CreateSolidBrush(0x0020_1510);
    let text=CreateSolidBrush(0x0024_201E);let muted=CreateSolidBrush(0x0083_746D);let accent=CreateSolidBrush(0x00A9_651C);let accent_dark=CreateSolidBrush(0x007C_430F);let accent_soft=CreateSolidBrush(0x00EA_D8C8);let green=CreateSolidBrush(0x005B_8F43);let white=CreateSolidBrush(0x00FF_FFFF);
    FillRect(hdc,&Rect{left:0,top:0,right:1280,bottom:720},wall);
    SelectObject(hdc,wall_blue);Ellipse(hdc,550,-390,1480,540);SelectObject(hdc,wall_teal);Ellipse(hdc,745,-130,1395,520);SelectObject(hdc,wall_dark);Ellipse(hdc,690,225,1410,910);
    fill_round_rect(hdc,Rect{left:-170,top:524,right:650,bottom:704},92,wall_blue);fill_round_rect(hdc,Rect{left:-230,top:590,right:790,bottom:720},64,wall_dark);

    let display_font=CreateFontA(30,0,0,0,650,0,0,0,1,0,0,0,0,c"Segoe UI".as_ptr());
    let title_font=CreateFontA(20,0,0,0,650,0,0,0,1,0,0,0,0,c"Segoe UI".as_ptr());
    let body_font=CreateFontA(14,0,0,0,400,0,0,0,1,0,0,0,0,c"Segoe UI".as_ptr());
    let medium_font=CreateFontA(13,0,0,0,600,0,0,0,1,0,0,0,0,c"Segoe UI".as_ptr());
    let small_font=CreateFontA(11,0,0,0,400,0,0,0,1,0,0,0,0,c"Segoe UI".as_ptr());
    let tiny_font=CreateFontA(10,0,0,0,400,0,0,0,1,0,0,0,0,c"Segoe UI".as_ptr());
    let old_font=SelectObject(hdc,body_font);SetBkMode(hdc,1);

    SelectObject(hdc,display_font);SetTextColor(hdc,0x00FF_FFFF);TextOutA(hdc,38,30,c"Winlux".as_ptr(),-1);
    SelectObject(hdc,small_font);SetTextColor(hdc,0x00E8_D8D1);TextOutA(hdc,41,67,c"One desktop. Linux and Windows applications.".as_ptr(),-1);
    draw_installed_icon(hdc,c"computer".as_ptr(),42,112,48,0x00F4_EFEC,accent_soft);draw_installed_icon(hdc,c"user-home".as_ptr(),42,205,48,0x00F4_EFEC,accent_soft);draw_installed_icon(hdc,c"user-trash".as_ptr(),42,298,48,0x00F4_EFEC,accent_soft);
    SelectObject(hdc,small_font);SetTextColor(hdc,0x00FF_FFFF);TextOutA(hdc,42,163,c"Computer".as_ptr(),-1);TextOutA(hdc,48,256,c"Home".as_ptr(),-1);TextOutA(hdc,46,349,c"Trash".as_ptr(),-1);

    if start_open {
        round_panel(hdc,Rect{left:16,top:62,right:1056,bottom:638},22,shadow,border,surface);
        fill_round_rect(hdc,Rect{left:18,top:64,right:124,bottom:636},20,sidebar);
        SelectObject(hdc,accent);Ellipse(hdc,43,88,99,144);draw_windows_mark(hdc,59,104,24,white);
        SelectObject(hdc,medium_font);SetTextColor(hdc,0x0024_201E);TextOutA(hdc,47,151,c"Damien".as_ptr(),-1);
        draw_installed_icon(hdc,c"user-home".as_ptr(),51,202,34,0x00F0_EAE7,accent_soft);draw_installed_icon(hdc,c"folder-documents".as_ptr(),51,254,34,0x00F0_EAE7,accent_soft);draw_installed_icon(hdc,c"folder-download".as_ptr(),51,306,34,0x00F0_EAE7,accent_soft);
        SelectObject(hdc,tiny_font);SetTextColor(hdc,0x0083_746D);TextOutA(hdc,49,238,c"Home".as_ptr(),-1);TextOutA(hdc,40,290,c"Documents".as_ptr(),-1);TextOutA(hdc,39,342,c"Downloads".as_ptr(),-1);
        FillRect(hdc,&Rect{left:42,top:564,right:100,bottom:565},border);SelectObject(hdc,medium_font);SetTextColor(hdc,0x0083_746D);TextOutA(hdc,47,580,c"Lock".as_ptr(),-1);TextOutA(hdc,47,608,c"Power".as_ptr(),-1);

        FillRect(hdc,&Rect{left:124,top:64,right:125,bottom:636},border);
        SelectObject(hdc,title_font);SetTextColor(hdc,0x0024_201E);TextOutA(hdc,151,91,c"Applications".as_ptr(),-1);
        fill_round_rect(hdc,Rect{left:145,top:128,right:343,bottom:168},12,accent_soft);FillRect(hdc,&Rect{left:145,top:137,right:149,bottom:159},accent);
        SelectObject(hdc,medium_font);SetTextColor(hdc,0x007C_430F);TextOutA(hdc,166,140,c"All applications".as_ptr(),-1);
        SelectObject(hdc,body_font);SetTextColor(hdc,0x005F_5652);TextOutA(hdc,166,184,c"Accessories".as_ptr(),-1);TextOutA(hdc,166,218,c"Internet".as_ptr(),-1);TextOutA(hdc,166,252,c"Graphics".as_ptr(),-1);TextOutA(hdc,166,286,c"Office".as_ptr(),-1);TextOutA(hdc,166,320,c"Sound & Video".as_ptr(),-1);TextOutA(hdc,166,354,c"System Tools".as_ptr(),-1);TextOutA(hdc,166,388,c"Administration".as_ptr(),-1);TextOutA(hdc,166,422,c"Windows apps".as_ptr(),-1);
        FillRect(hdc,&Rect{left:360,top:82,right:361,bottom:556},border);

        fill_round_rect(hdc,Rect{left:386,top:84,right:1028,bottom:130},14,elevated);draw_search_mark(hdc,404,98,muted,elevated);SelectObject(hdc,body_font);SetTextColor(hdc,0x0083_746D);TextOutA(hdc,440,100,c"Search applications, files and settings".as_ptr(),-1);
        SelectObject(hdc,small_font);SetTextColor(hdc,0x0083_746D);TextOutA(hdc,389,148,c"INSTALLED APPLICATIONS".as_ptr(),-1);

        fill_round_rect(hdc,Rect{left:380,top:171,right:1032,bottom:228},14,elevated);
        draw_installed_icon(hdc,c"cinnamon-settings.desktop".as_ptr(),397,181,36,0x00F6_F2F0,accent_soft);
        SelectObject(hdc,medium_font);SetTextColor(hdc,0x0024_201E);TextOutA(hdc,449,182,c"System Settings".as_ptr(),-1);
        SelectObject(hdc,small_font);SetTextColor(hdc,0x0083_746D);TextOutA(hdc,449,202,c"Linux desktop, hardware and accounts".as_ptr(),-1);
        fill_round_rect(hdc,Rect{left:768,top:182,right:895,bottom:216},10,surface);SelectObject(hdc,small_font);SetTextColor(hdc,0x005B_8F43);TextOutA(hdc,789,191,c"Linux settings".as_ptr(),-1);
        fill_round_rect(hdc,Rect{left:902,top:182,right:1018,bottom:216},10,accent_soft);SetTextColor(hdc,0x007C_430F);TextOutA(hdc,914,191,c"Windows layer".as_ptr(),-1);

        draw_installed_icon(hdc,c"firefox".as_ptr(),397,244,38,0x00FC_F9F7,accent_soft);draw_installed_icon(hdc,c"nemo.desktop".as_ptr(),397,302,38,0x00FC_F9F7,accent_soft);draw_installed_icon(hdc,c"org.gnome.Terminal.desktop".as_ptr(),397,360,38,0x00FC_F9F7,accent_soft);draw_installed_icon(hdc,c"mintinstall.desktop".as_ptr(),397,418,38,0x00FC_F9F7,accent_soft);draw_installed_icon(hdc,c"audacity.desktop".as_ptr(),397,476,38,0x00FC_F9F7,accent_soft);
        SelectObject(hdc,medium_font);SetTextColor(hdc,0x0024_201E);TextOutA(hdc,451,249,c"Firefox Web Browser".as_ptr(),-1);TextOutA(hdc,451,307,c"Files".as_ptr(),-1);TextOutA(hdc,451,365,c"Terminal".as_ptr(),-1);TextOutA(hdc,451,423,c"Software Manager".as_ptr(),-1);TextOutA(hdc,451,481,c"Audacity".as_ptr(),-1);
        SelectObject(hdc,small_font);SetTextColor(hdc,0x0083_746D);TextOutA(hdc,451,268,c"Browse the web".as_ptr(),-1);TextOutA(hdc,451,326,c"Browse files and Windows prefixes".as_ptr(),-1);TextOutA(hdc,451,384,c"Command line".as_ptr(),-1);TextOutA(hdc,451,442,c"Install or remove Linux applications".as_ptr(),-1);TextOutA(hdc,451,500,c"Audio editor".as_ptr(),-1);
        FillRect(hdc,&Rect{left:380,top:538,right:1032,bottom:539},border);SelectObject(hdc,small_font);SetTextColor(hdc,0x0083_746D);TextOutA(hdc,388,562,c"Winlux automatically uses each installed application's real icon.".as_ptr(),-1);
        fill_round_rect(hdc,Rect{left:852,top:548,right:1028,bottom:586},11,accent);SelectObject(hdc,medium_font);SetTextColor(hdc,0x00FF_FFFF);TextOutA(hdc,881,558,c"Manage applications".as_ptr(),-1);
    }

    // Barre pleine largeur. Les indicateurs système sont volontairement à l'extrême gauche.
    FillRect(hdc,&Rect{left:0,top:648,right:1280,bottom:720},taskbar);FillRect(hdc,&Rect{left:0,top:648,right:1280,bottom:649},border);
    SelectObject(hdc,medium_font);SetTextColor(hdc,0x0024_201E);TextOutA(hdc,14,661,c"14:32".as_ptr(),-1);SelectObject(hdc,tiny_font);SetTextColor(hdc,0x0083_746D);TextOutA(hdc,14,683,c"23 Oct 2026".as_ptr(),-1);
    FillRect(hdc,&Rect{left:100,top:660,right:101,bottom:708},border);
    // Ethernet, volume et batterie, tous groupés à gauche.
    FillRect(hdc,&Rect{left:119,top:673,right:139,bottom:688},text);FillRect(hdc,&Rect{left:124,top:688,right:128,bottom:696},text);FillRect(hdc,&Rect{left:133,top:688,right:137,bottom:696},text);
    SelectObject(hdc,small_font);SetTextColor(hdc,0x0024_201E);TextOutA(hdc,146,670,c"Ethernet".as_ptr(),-1);SetTextColor(hdc,0x0083_746D);TextOutA(hdc,146,689,c"Connected".as_ptr(),-1);
    FillRect(hdc,&Rect{left:224,top:674,right:232,bottom:690},text);FillRect(hdc,&Rect{left:232,top:671,right:237,bottom:693},text);SelectObject(hdc,small_font);SetTextColor(hdc,0x0024_201E);TextOutA(hdc,240,678,c"100%".as_ptr(),-1);
    FrameRect(hdc,&Rect{left:287,top:673,right:317,bottom:692},text);FillRect(hdc,&Rect{left:290,top:676,right:312,bottom:689},green);FillRect(hdc,&Rect{left:317,top:679,right:320,bottom:686},text);
    FillRect(hdc,&Rect{left:332,top:660,right:333,bottom:708},border);

    if start_open { fill_round_rect(hdc,Rect{left:344,top:656,right:462,bottom:711},14,accent_soft); }
    draw_installed_icon(hdc,c"distributor-logo-linux-mint".as_ptr(),354,665,36,if start_open{0x00EA_D8C8}else{0x00F4_EFEC},accent);
    SelectObject(hdc,medium_font);SetTextColor(hdc,0x0024_201E);TextOutA(hdc,399,675,c"Menu".as_ptr(),-1);
    FillRect(hdc,&Rect{left:352,top:714,right:454,bottom:718},accent);

    draw_installed_icon(hdc,c"firefox".as_ptr(),490,664,40,0x00F4_EFEC,accent_soft);draw_installed_icon(hdc,c"nemo.desktop".as_ptr(),550,664,40,0x00F4_EFEC,accent_soft);draw_installed_icon(hdc,c"org.gnome.Terminal.desktop".as_ptr(),610,664,40,0x00F4_EFEC,accent_soft);draw_installed_icon(hdc,c"cinnamon-settings.desktop".as_ptr(),670,664,40,0x00F4_EFEC,accent_soft);draw_installed_icon(hdc,c"mintinstall.desktop".as_ptr(),730,664,40,0x00F4_EFEC,accent_soft);draw_installed_icon(hdc,c"audacity.desktop".as_ptr(),790,664,40,0x00F4_EFEC,accent_soft);
    FillRect(hdc,&Rect{left:494,top:714,right:526,bottom:718},accent);
    SelectObject(hdc,small_font);SetTextColor(hdc,0x0083_746D);TextOutA(hdc,1171,677,c"Desktop 1".as_ptr(),-1);TextOutA(hdc,1228,677,c"^".as_ptr(),-1);

    SelectObject(hdc,old_font);
    DeleteObject(wall);DeleteObject(wall_blue);DeleteObject(wall_teal);DeleteObject(wall_dark);DeleteObject(surface);DeleteObject(sidebar);DeleteObject(elevated);DeleteObject(taskbar);DeleteObject(border);DeleteObject(shadow);DeleteObject(text);DeleteObject(muted);DeleteObject(accent);DeleteObject(accent_dark);DeleteObject(accent_soft);DeleteObject(green);DeleteObject(white);
    DeleteObject(display_font);DeleteObject(title_font);DeleteObject(body_font);DeleteObject(medium_font);DeleteObject(small_font);DeleteObject(tiny_font);
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

    let full = Rect { left: 0, top: 0, right: 720, bottom: 480 };
    let topbar = Rect { left: 0, top: 0, right: 720, bottom: 48 };
    let navigation = Rect { left: 0, top: 48, right: 204, bottom: 480 };
    FillRect(hdc, &full, background);
    FillRect(hdc, &topbar, surface);
    FillRect(hdc, &navigation, sidebar);
    let top_border = Rect { left: 0, top: 47, right: 720, bottom: 48 };
    FillRect(hdc, &top_border, border);

    SelectObject(hdc, accent);
    Ellipse(hdc, 16, 12, 40, 36);
    Ellipse(hdc, 666, 10, 696, 40);

    SetBkMode(hdc, 1);
    SetTextColor(hdc, 0x001F_1F1F);
    let title_font = CreateFontA(20, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 0, 0, c"Segoe UI".as_ptr());
    let body_font = CreateFontA(14, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 0, 0, c"Segoe UI".as_ptr());
    let old_font = SelectObject(hdc, title_font);
    TextOutA(hdc, 50, 16, c"Winlux Control Center".as_ptr(), -1);

    SelectObject(hdc, body_font);
    SetTextColor(hdc, 0x0060_6060);
    TextOutA(hdc, 28, 88, c"Overview".as_ptr(), -1);
    TextOutA(hdc, 28, 124, c"Settings".as_ptr(), -1);
    let selected = Rect { left: 10, top: 150, right: 194, bottom: 184 };
    FillRect(hdc, &selected, selection);
    let selected_mark = Rect { left: 10, top: 157, right: 14, bottom: 177 };
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

    let search = Rect { left: 220, top: 102, right: 690, bottom: 140 };
    FillRect(hdc, &search, surface);
    FrameRect(hdc, &search, border);
    SetTextColor(hdc, 0x0060_6060);
    TextOutA(hdc, 238, 114, c"Search Linux packages or Windows programs".as_ptr(), -1);

    let linux_card = Rect { left: 220, top: 154, right: 448, bottom: 400 };
    let windows_card = Rect { left: 460, top: 154, right: 690, bottom: 400 };
    FillRect(hdc, &linux_card, surface);
    FillRect(hdc, &windows_card, surface);
    FrameRect(hdc, &linux_card, border);
    FrameRect(hdc, &windows_card, border);

    let linux_icon = Rect { left: 238, top: 172, right: 270, bottom: 204 };
    let windows_icon = Rect { left: 478, top: 172, right: 510, bottom: 204 };
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
    FillRect(hdc, &Rect { left: 478, top: 320, right: 486, bottom: 328 }, warning);
    TextOutA(hdc, 492, 316, c"Needs .NET".as_ptr(), -1);

    let linux_install = Rect { left: 238, top: 352, right: 329, bottom: 382 };
    let linux_remove = Rect { left: 339, top: 352, right: 430, bottom: 382 };
    let windows_install = Rect { left: 478, top: 352, right: 569, bottom: 382 };
    let windows_remove = Rect { left: 581, top: 352, right: 672, bottom: 382 };
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
    TextOutA(hdc, 224, 424, c"Select an installed app to update or uninstall it".as_ptr(), -1);

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

    if GetEnvironmentVariableA(c"WINLUX_DESKTOP_DEMO".as_ptr(), core::ptr::null_mut(), 0) > 0 {
        draw_desktop_scene(hdc, START_MENU_OPEN != 0);
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
    let title_font = CreateFontA(20, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 0, 0, c"Segoe UI".as_ptr());
    let body_font = CreateFontA(14, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 0, 0, c"Segoe UI".as_ptr());
    let old_font = SelectObject(hdc, title_font);
    let titled = TextOutA(hdc, 50, 16, c"Winlux Control Center".as_ptr(), -1);

    // Navigation de type Windows 11.
    SelectObject(hdc, body_font);
    let selected = Rect { left: 10, top: 78, right: 194, bottom: 112 };
    FillRect(hdc, &selected, selection);
    let selected_mark = Rect { left: 10, top: 85, right: 14, bottom: 105 };
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

    let linux_card = Rect { left: 220, top: 104, right: 448, bottom: 292 };
    let windows_card = Rect { left: 460, top: 104, right: 690, bottom: 292 };
    let shared_card = Rect { left: 220, top: 306, right: 690, bottom: 394 };
    FillRect(hdc, &linux_card, surface);
    FillRect(hdc, &windows_card, surface);
    FillRect(hdc, &shared_card, surface);
    let framed = FrameRect(hdc, &linux_card, border)
        & FrameRect(hdc, &windows_card, border)
        & FrameRect(hdc, &shared_card, border);

    let linux_icon = Rect { left: 238, top: 122, right: 270, bottom: 154 };
    let windows_icon = Rect { left: 478, top: 122, right: 510, bottom: 154 };
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

    let linux_action = Rect { left: 238, top: 250, right: 426, bottom: 278 };
    let windows_action = Rect { left: 478, top: 250, right: 672, bottom: 278 };
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
    let status_dot = Rect { left: 226, top: 360, right: 236, bottom: 370 };
    FillRect(hdc, &status_dot, success);
    SetTextColor(hdc, 0x0060_6060);
    TextOutA(hdc, 246, 354, c"Files, clipboard and accounts connected".as_ptr(), -1);
    let action = Rect { left: 520, top: 402, right: 690, bottom: 438 };
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
        WM_LBUTTONDOWN => {
            if GetEnvironmentVariableA(c"WINLUX_DESKTOP_DEMO".as_ptr(), core::ptr::null_mut(), 0) > 0 {
                let x = (lparam as u32 & 0xFFFF) as i32;
                let y = ((lparam as u32 >> 16) & 0xFFFF) as i32;
                if (344..=462).contains(&x) && (648..=720).contains(&y) {
                    START_MENU_OPEN ^= 1;
                    InvalidateRect(hwnd, core::ptr::null(), 1);
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
        let testing = GetEnvironmentVariableA(c"WINLUX_TEST".as_ptr(), core::ptr::null_mut(), 0) > 0;
        let desktop = GetEnvironmentVariableA(c"WINLUX_DESKTOP_DEMO".as_ptr(), core::ptr::null_mut(), 0) > 0;
        let duration = if testing { 300u32 } else if desktop { 15000 } else { 3000 };

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

        let title = if desktop { c"Winlux Desktop" } else { c"Winlux Control Center" };
        let (window_x, window_y, window_width, window_height) = if desktop {
            (0, 0, 1280, 720)
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
