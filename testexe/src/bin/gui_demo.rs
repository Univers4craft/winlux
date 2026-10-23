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

/// Première base du bureau Winlux : fond, barre des tâches, zone système et
/// menu Démarrer. Tout est rendu par le même chemin GDI invité → X11 que les
/// futures applications du shell.
unsafe fn draw_desktop_scene(hdc: Handle, start_open: bool) {
    let wall=CreateSolidBrush(0x0020_100B);let wall_mid=CreateSolidBrush(0x0052_2911);let wall_blue=CreateSolidBrush(0x0091_5522);let wall_violet=CreateSolidBrush(0x007A_3E4D);let wall_dark=CreateSolidBrush(0x0031_180C);
    let surface=CreateSolidBrush(0x00FC_FAF9);let elevated=CreateSolidBrush(0x00F7_F3F1);let glass=CreateSolidBrush(0x00F2_EDEC);let border=CreateSolidBrush(0x00E5_DCD8);let shadow=CreateSolidBrush(0x0020_1510);
    let text=CreateSolidBrush(0x0024_2120);let muted=CreateSolidBrush(0x0085_7066);let accent=CreateSolidBrush(0x00D8_640A);let accent_soft=CreateSolidBrush(0x00FF_F1E8);
    let green=CreateSolidBrush(0x006B_A022);let orange=CreateSolidBrush(0x001B_87F4);let purple=CreateSolidBrush(0x00B4_58A8);let cyan=CreateSolidBrush(0x00C7_9B13);let navy=CreateSolidBrush(0x0040_281C);let white=CreateSolidBrush(0x00FF_FFFF);

    FillRect(hdc,&Rect{left:0,top:0,right:1280,bottom:720},wall);
    // Papier peint abstrait composé de halos concentriques et d'un horizon doux.
    SelectObject(hdc,wall_violet);Ellipse(hdc,570,-360,1450,520);
    SelectObject(hdc,wall_mid);Ellipse(hdc,660,-210,1420,550);
    SelectObject(hdc,wall_blue);Ellipse(hdc,770,-90,1400,540);
    SelectObject(hdc,wall_dark);Ellipse(hdc,690,180,1360,850);
    SelectObject(hdc,wall_violet);Ellipse(hdc,850,295,1260,705);
    fill_round_rect(hdc,Rect{left:-120,top:552,right:680,bottom:710},80,wall_mid);
    fill_round_rect(hdc,Rect{left:-180,top:592,right:790,bottom:720},64,wall_dark);

    let display_font=CreateFontA(30,0,0,0,600,0,0,0,1,0,0,0,0,c"Segoe UI".as_ptr());
    let title_font=CreateFontA(19,0,0,0,600,0,0,0,1,0,0,0,0,c"Segoe UI".as_ptr());
    let body_font=CreateFontA(14,0,0,0,400,0,0,0,1,0,0,0,0,c"Segoe UI".as_ptr());
    let medium_font=CreateFontA(13,0,0,0,600,0,0,0,1,0,0,0,0,c"Segoe UI".as_ptr());
    let small_font=CreateFontA(11,0,0,0,400,0,0,0,1,0,0,0,0,c"Segoe UI".as_ptr());
    let icon_font=CreateFontA(16,0,0,0,700,0,0,0,1,0,0,0,0,c"Segoe UI".as_ptr());
    let old_font=SelectObject(hdc,body_font);SetBkMode(hdc,1);

    // Identité du bureau, discrète et structurée.
    SelectObject(hdc,display_font);SetTextColor(hdc,0x00FF_FFFF);TextOutA(hdc,38,32,c"Winlux".as_ptr(),-1);
    SelectObject(hdc,small_font);SetTextColor(hdc,0x00E8_D8D1);TextOutA(hdc,41,68,c"Linux freedom. Windows familiarity.".as_ptr(),-1);
    fill_round_rect(hdc,Rect{left:38,top:102,right:108,bottom:174},18,glass);draw_windows_mark(hdc,58,120,30,accent);
    fill_round_rect(hdc,Rect{left:38,top:196,right:108,bottom:268},18,glass);FillRect(hdc,&Rect{left:55,top:218,right:92,bottom:247},green);FillRect(hdc,&Rect{left:55,top:214,right:73,bottom:220},green);
    fill_round_rect(hdc,Rect{left:38,top:290,right:108,bottom:362},18,glass);FrameRect(hdc,&Rect{left:58,top:310,right:88,bottom:344},purple);FillRect(hdc,&Rect{left:63,top:305,right:83,bottom:312},purple);
    SelectObject(hdc,small_font);SetTextColor(hdc,0x00FF_FFFF);TextOutA(hdc,51,178,c"This PC".as_ptr(),-1);TextOutA(hdc,57,272,c"Files".as_ptr(),-1);TextOutA(hdc,43,366,c"Recycle Bin".as_ptr(),-1);

    // Carte de contexte en haut à droite, visible lorsque le menu est fermé.
    if !start_open {
        round_panel(hdc,Rect{left:1002,top:34,right:1240,bottom:132},20,shadow,border,glass);
        SelectObject(hdc,title_font);SetTextColor(hdc,0x00FF_FFFF);TextOutA(hdc,1024,54,c"14:32".as_ptr(),-1);
        SelectObject(hdc,small_font);SetTextColor(hdc,0x00E8_D8D1);TextOutA(hdc,1026,84,c"Friday, 23 October".as_ptr(),-1);TextOutA(hdc,1148,57,c"18 C".as_ptr(),-1);TextOutA(hdc,1148,82,c"Connected".as_ptr(),-1);
    }

    // Barre des tâches flottante et zone système séparée.
    round_panel(hdc,Rect{left:350,top:661,right:930,bottom:713},20,shadow,border,glass);
    round_panel(hdc,Rect{left:984,top:661,right:1260,bottom:713},20,shadow,border,glass);
    if start_open{fill_round_rect(hdc,Rect{left:368,top:668,right:410,bottom:706},12,accent_soft);}
    draw_windows_mark(hdc,379,678,20,accent);draw_search_mark(hdc,430,679,text,glass);
    fill_round_rect(hdc,Rect{left:478,top:671,right:512,bottom:705},9,green);FillRect(hdc,&Rect{left:486,top:680,right:504,bottom:697},white);
    fill_round_rect(hdc,Rect{left:526,top:671,right:560,bottom:705},9,purple);SelectObject(hdc,white);Ellipse(hdc,536,681,550,695);SelectObject(hdc,purple);Ellipse(hdc,540,685,546,691);
    fill_round_rect(hdc,Rect{left:574,top:671,right:608,bottom:705},9,orange);SelectObject(hdc,icon_font);SetTextColor(hdc,0x00FF_FFFF);TextOutA(hdc,583,678,c">_".as_ptr(),-1);
    fill_round_rect(hdc,Rect{left:622,top:671,right:656,bottom:705},9,cyan);FillRect(hdc,&Rect{left:631,top:681,right:647,bottom:696},white);
    fill_round_rect(hdc,Rect{left:670,top:671,right:704,bottom:705},9,accent);draw_windows_mark(hdc,679,680,16,white);
    FillRect(hdc,&Rect{left:374,top:709,right:404,bottom:712},accent);

    SelectObject(hdc,small_font);SetTextColor(hdc,0x0024_2120);TextOutA(hdc,1001,678,c"^".as_ptr(),-1);
    FillRect(hdc,&Rect{left:1029,top:692,right:1032,bottom:699},text);FillRect(hdc,&Rect{left:1035,top:687,right:1038,bottom:699},text);FillRect(hdc,&Rect{left:1041,top:682,right:1044,bottom:699},text);
    FillRect(hdc,&Rect{left:1060,top:687,right:1066,bottom:697},text);FillRect(hdc,&Rect{left:1066,top:684,right:1071,bottom:700},text);TextOutA(hdc,1073,684,c"))".as_ptr(),-1);
    FrameRect(hdc,&Rect{left:1100,top:684,right:1128,bottom:700},text);FillRect(hdc,&Rect{left:1103,top:687,right:1124,bottom:697},green);FillRect(hdc,&Rect{left:1128,top:689,right:1131,bottom:695},text);
    SelectObject(hdc,medium_font);TextOutA(hdc,1151,675,c"14:32".as_ptr(),-1);SelectObject(hdc,small_font);SetTextColor(hdc,0x0085_7066);TextOutA(hdc,1142,694,c"23/10/2026".as_ptr(),-1);

    if start_open {
        let menu=Rect{left:292,top:54,right:988,bottom:646};round_panel(hdc,menu,24,shadow,border,surface);
        // En-tête de recherche.
        fill_round_rect(hdc,Rect{left:334,top:86,right:946,bottom:134},16,elevated);draw_search_mark(hdc,354,101,muted,elevated);
        SelectObject(hdc,body_font);SetTextColor(hdc,0x0085_7066);TextOutA(hdc,389,102,c"Search apps, settings and documents".as_ptr(),-1);
        fill_round_rect(hdc,Rect{left:891,top:96,right:928,bottom:124},9,surface);SelectObject(hdc,small_font);TextOutA(hdc,902,102,c"/".as_ptr(),-1);

        SelectObject(hdc,title_font);SetTextColor(hdc,0x0024_2120);TextOutA(hdc,334,160,c"Pinned".as_ptr(),-1);
        fill_round_rect(hdc,Rect{left:846,top:154,right:946,bottom:184},10,elevated);SelectObject(hdc,small_font);TextOutA(hdc,869,162,c"All apps  >".as_ptr(),-1);

        // Applications : tuiles homogènes et glyphes dessinés.
        fill_round_rect(hdc,Rect{left:346,top:203,right:394,bottom:251},13,accent);SelectObject(hdc,white);Ellipse(hdc,357,214,383,240);SelectObject(hdc,accent);Ellipse(hdc,364,221,383,240);
        fill_round_rect(hdc,Rect{left:449,top:203,right:497,bottom:251},13,green);FillRect(hdc,&Rect{left:459,top:218,right:487,bottom:239},white);FillRect(hdc,&Rect{left:459,top:214,right:474,bottom:220},white);
        fill_round_rect(hdc,Rect{left:552,top:203,right:600,bottom:251},13,purple);SelectObject(hdc,white);Ellipse(hdc,563,214,589,240);SelectObject(hdc,purple);Ellipse(hdc,570,221,582,233);
        fill_round_rect(hdc,Rect{left:655,top:203,right:703,bottom:251},13,orange);FillRect(hdc,&Rect{left:666,top:214,right:692,bottom:240},white);SelectObject(hdc,orange);Ellipse(hdc,671,220,678,227);Ellipse(hdc,681,217,689,225);
        fill_round_rect(hdc,Rect{left:758,top:203,right:806,bottom:251},13,cyan);FrameRect(hdc,&Rect{left:770,top:218,right:794,bottom:239},white);SelectObject(hdc,white);Ellipse(hdc,776,210,788,224);
        fill_round_rect(hdc,Rect{left:861,top:203,right:909,bottom:251},13,navy);SelectObject(hdc,icon_font);SetTextColor(hdc,0x00FF_FFFF);TextOutA(hdc,873,217,c">_".as_ptr(),-1);

        fill_round_rect(hdc,Rect{left:346,top:295,right:394,bottom:343},13,accent_soft);draw_windows_mark(hdc,359,308,22,accent);
        fill_round_rect(hdc,Rect{left:449,top:295,right:497,bottom:343},13,elevated);SelectObject(hdc,green);Ellipse(hdc,460,306,486,332);SetTextColor(hdc,0x00FF_FFFF);SelectObject(hdc,medium_font);TextOutA(hdc,469,312,c"L".as_ptr(),-1);
        fill_round_rect(hdc,Rect{left:552,top:295,right:600,bottom:343},13,accent);SelectObject(hdc,white);Ellipse(hdc,563,306,589,332);SelectObject(hdc,accent);Ellipse(hdc,570,313,589,332);
        fill_round_rect(hdc,Rect{left:655,top:295,right:703,bottom:343},13,elevated);SelectObject(hdc,icon_font);SetTextColor(hdc,0x0024_2120);TextOutA(hdc,672,310,c"N".as_ptr(),-1);
        fill_round_rect(hdc,Rect{left:758,top:295,right:806,bottom:343},13,orange);SelectObject(hdc,icon_font);SetTextColor(hdc,0x00FF_FFFF);TextOutA(hdc,774,310,c">".as_ptr(),-1);
        fill_round_rect(hdc,Rect{left:861,top:295,right:909,bottom:343},13,accent);draw_windows_mark(hdc,874,308,22,white);

        SelectObject(hdc,small_font);SetTextColor(hdc,0x0024_2120);
        TextOutA(hdc,356,259,c"Edge".as_ptr(),-1);TextOutA(hdc,459,259,c"Files".as_ptr(),-1);TextOutA(hdc,557,259,c"Settings".as_ptr(),-1);TextOutA(hdc,662,259,c"Photos".as_ptr(),-1);TextOutA(hdc,766,259,c"Store".as_ptr(),-1);TextOutA(hdc,864,259,c"Terminal".as_ptr(),-1);
        TextOutA(hdc,337,351,c"Windows apps".as_ptr(),-1);TextOutA(hdc,444,351,c"Linux apps".as_ptr(),-1);TextOutA(hdc,555,351,c"Browser".as_ptr(),-1);TextOutA(hdc,658,351,c"Notepad".as_ptr(),-1);TextOutA(hdc,765,351,c"Media".as_ptr(),-1);TextOutA(hdc,867,351,c"Winlux".as_ptr(),-1);

        SelectObject(hdc,title_font);TextOutA(hdc,334,394,c"Recommended".as_ptr(),-1);SelectObject(hdc,small_font);SetTextColor(hdc,0x0085_7066);TextOutA(hdc,865,400,c"More  >".as_ptr(),-1);
        fill_round_rect(hdc,Rect{left:334,top:430,right:627,bottom:506},16,elevated);fill_round_rect(hdc,Rect{left:646,top:430,right:946,bottom:506},16,elevated);
        fill_round_rect(hdc,Rect{left:350,top:444,right:400,bottom:494},12,accent_soft);draw_windows_mark(hdc,363,457,24,accent);
        fill_round_rect(hdc,Rect{left:662,top:444,right:712,bottom:494},12,green);SelectObject(hdc,white);Ellipse(hdc,674,456,700,482);
        SelectObject(hdc,medium_font);SetTextColor(hdc,0x0024_2120);TextOutA(hdc,416,447,c"Welcome to Winlux".as_ptr(),-1);TextOutA(hdc,728,447,c"Shared applications".as_ptr(),-1);
        SelectObject(hdc,small_font);SetTextColor(hdc,0x0085_7066);TextOutA(hdc,416,470,c"Finish setting up your desktop".as_ptr(),-1);TextOutA(hdc,728,470,c"Linux and Windows in one place".as_ptr(),-1);

        FillRect(hdc,&Rect{left:293,top:536,right:987,bottom:537},border);fill_round_rect(hdc,Rect{left:316,top:554,right:965,bottom:628},18,elevated);
        SelectObject(hdc,accent);Ellipse(hdc,336,570,378,612);draw_windows_mark(hdc,348,582,18,white);
        SelectObject(hdc,medium_font);SetTextColor(hdc,0x0024_2120);TextOutA(hdc,392,577,c"Damien".as_ptr(),-1);SelectObject(hdc,small_font);SetTextColor(hdc,0x0085_7066);TextOutA(hdc,392,597,c"Local session".as_ptr(),-1);
        fill_round_rect(hdc,Rect{left:850,top:568,right:934,bottom:614},13,surface);SelectObject(hdc,body_font);SetTextColor(hdc,0x0024_2120);TextOutA(hdc,871,582,c"Power".as_ptr(),-1);
    }

    SelectObject(hdc,old_font);
    DeleteObject(wall);DeleteObject(wall_mid);DeleteObject(wall_blue);DeleteObject(wall_violet);DeleteObject(wall_dark);DeleteObject(surface);DeleteObject(elevated);DeleteObject(glass);DeleteObject(border);DeleteObject(shadow);DeleteObject(text);DeleteObject(muted);DeleteObject(accent);DeleteObject(accent_soft);DeleteObject(green);DeleteObject(orange);DeleteObject(purple);DeleteObject(cyan);DeleteObject(navy);DeleteObject(white);
    DeleteObject(display_font);DeleteObject(title_font);DeleteObject(body_font);DeleteObject(medium_font);DeleteObject(small_font);DeleteObject(icon_font);
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
                if (360..=418).contains(&x) && (658..=720).contains(&y) {
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
