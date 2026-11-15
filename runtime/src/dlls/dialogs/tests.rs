use super::*;
use std::sync::Mutex as StdMutex;

pub(crate) static SERIAL: StdMutex<()> = StdMutex::new(());
static SEEN: StdMutex<Vec<(u32, usize)>> = StdMutex::new(Vec::new());

/// Modèle DLGTEMPLATE (ou EX) : une boîte, un champ, deux boutons.
fn template(ex: bool) -> Vec<u8> {
    template_font(ex, true)
}

/// Même modèle ; sans police, il porte DS_FIXEDSYS (0x8) sans DS_SETFONT
/// et donc aucun champ de police dans l'en-tête.
fn template_font(ex: bool, font: bool) -> Vec<u8> {
    let mut o = Vec::new();
    let p16 = |o: &mut Vec<u8>, v: u16| o.extend_from_slice(&v.to_le_bytes());
    let p32 = |o: &mut Vec<u8>, v: u32| o.extend_from_slice(&v.to_le_bytes());
    let ps = |o: &mut Vec<u8>, s: &str| {
        for u in s.encode_utf16() {
            o.extend_from_slice(&u.to_le_bytes());
        }
        o.extend_from_slice(&[0, 0]);
    };
    let al = |o: &mut Vec<u8>| {
        while o.len() % 4 != 0 {
            o.push(0);
        }
    };
    let style = 0x80C8_0000 | DS_CENTER | if font { DS_SETFONT } else { 0x8 };
    if ex {
        p16(&mut o, 1);
        p16(&mut o, 0xFFFF);
        p32(&mut o, 0);
        p32(&mut o, 0);
        p32(&mut o, style);
    } else {
        p32(&mut o, style);
        p32(&mut o, 0);
    }
    p16(&mut o, 3);
    for v in [0u16, 0, 200, 80] {
        p16(&mut o, v);
    }
    p16(&mut o, 0);
    p16(&mut o, 0);
    ps(&mut o, "Réglages");
    if font {
        p16(&mut o, 9);
        if ex {
            p16(&mut o, 400);
            o.push(0);
            o.push(1);
        }
        ps(&mut o, "MS Shell Dlg");
    }
    let items: [(u32, u16, u16, &str); 3] = [
        (0x5081_0080, 100, 0x81, ""),  // EDIT, WS_TABSTOP|WS_BORDER
        (0x5001_0001, 1, 0x80, "&OK"), // BS_DEFPUSHBUTTON
        (0x5001_0000, 2, 0x80, "Annuler"),
    ];
    for (i, (st, id, class, text)) in items.iter().enumerate() {
        al(&mut o);
        if ex {
            p32(&mut o, 0);
            p32(&mut o, 0);
            p32(&mut o, *st);
        } else {
            p32(&mut o, *st);
            p32(&mut o, 0);
        }
        for v in [10u16, 10 + 20 * i as u16, 100, 14] {
            p16(&mut o, v);
        }
        if ex {
            p32(&mut o, *id as u32);
        } else {
            p16(&mut o, *id);
        }
        p16(&mut o, 0xFFFF);
        p16(&mut o, *class);
        ps(&mut o, text);
        p16(&mut o, 0);
    }
    o
}

#[test]
fn lecture_des_modeles_classique_et_etendu() {
    for ex in [false, true] {
        let t = parse_template(&template(ex)).expect("modèle lisible");
        assert_eq!(t.title, "Réglages");
        assert_eq!(t.point_size, 9);
        assert_eq!(t.face, "MS Shell Dlg");
        assert_eq!(t.items.len(), 3);
        assert_eq!(t.items[1].id, 1);
        assert_eq!(t.items[1].class, NameOrId::Id(0x80));
        assert_eq!(t.items[1].title, NameOrId::Name("&OK".into()));
        assert_eq!((t.cx, t.cy), (200, 80));
    }
}

#[test]
fn ds_fixedsys_seul_sans_police_dans_l_en_tete() {
    for ex in [false, true] {
        let t = parse_template(&template_font(ex, false)).expect("modèle lisible (#1685)");
        assert_eq!(t.title, "Réglages");
        assert!(t.face.is_empty());
        assert_eq!(t.items.len(), 3);
        assert_eq!(t.items[2].title, NameOrId::Name("Annuler".into()));
    }
}

unsafe extern "win64" fn modal_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    SEEN.lock().unwrap().push((msg, wp));
    match msg {
        WM_INITDIALOG => {
            assert_eq!(lp, 1234, "paramètre d'initialisation");
            let t: Vec<u16> = "putty.exe".encode_utf16().chain([0]).collect();
            user32::set_dlg_item_text_w(hwnd, 100, t.as_ptr());
            1
        }
        WM_COMMAND if wp & 0xFFFF == IDOK => {
            let mut buf = [0u16; 32];
            let n = user32::get_dlg_item_text_w(hwnd, 100, buf.as_mut_ptr(), 32);
            let ok = String::from_utf16_lossy(&buf[..n as usize]) == "putty.exe";
            end_dialog(hwnd, if ok { 42 } else { 7 });
            1
        }
        _ => 0,
    }
}

#[test]
fn boite_modale_depuis_un_modele_et_touche_entree() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    SEEN.lock().unwrap().clear();
    let data = template(true);
    // Entrée sera traitée par la boucle modale : on la met en file d'avance
    // pour la boîte que DialogBox va créer (la fenêtre active reçoit la touche).
    unsafe extern "win64" fn press_enter(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
        if msg == WM_INITDIALOG {
            modal_proc(hwnd, msg, wp, lp);
            let edit = user32::get_dlg_item(hwnd, 100);
            user32::post_message_w(edit, WM_KEYDOWN, 0x0D, 0);
            return 1;
        }
        modal_proc(hwnd, msg, wp, lp)
    }
    let r = unsafe {
        dialog_box_indirect_param_w(0, data.as_ptr(), 0, press_enter as *const () as usize, 1234)
    };
    assert_eq!(
        r, 42,
        "Entrée presse le bouton par défaut, EndDialog renvoie 42"
    );
    let seen = SEEN.lock().unwrap();
    assert!(seen.iter().any(|&(m, _)| m == WM_INITDIALOG));
}

#[test]
fn navigation_tab_et_echap_dans_une_boite_non_modale() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    unsafe extern "win64" fn proc_(hwnd: usize, msg: u32, wp: usize, _lp: isize) -> isize {
        if msg == WM_COMMAND && wp & 0xFFFF == IDCANCEL {
            end_dialog(hwnd, 2);
            return 1;
        }
        (msg == WM_INITDIALOG) as isize
    }
    let data = template(false);
    let dlg = unsafe {
        create_dialog_indirect_param_w(0, data.as_ptr(), 0, proc_ as *const () as usize, 0)
    };
    assert_ne!(dlg, 0);
    let edit = user32::get_dlg_item(dlg, 100);
    let ok = user32::get_dlg_item(dlg, 1);
    let cancel = user32::get_dlg_item(dlg, 2);
    assert_eq!(
        gui::focused_window(),
        edit,
        "WM_INITDIALOG TRUE : focus au premier contrôle"
    );
    let key = |hwnd: usize, vk: usize| Msg {
        hwnd,
        message: WM_KEYDOWN,
        wparam: vk,
        lparam: 0,
        time: 0,
        pt: gui::Point::default(),
        l_private: 0,
    };
    unsafe {
        assert!(is_dialog_message(dlg, &key(edit, 0x09)));
        assert_eq!(gui::focused_window(), ok);
        assert!(is_dialog_message(dlg, &key(ok, 0x09)));
        assert_eq!(gui::focused_window(), cancel);
        assert!(is_dialog_message(dlg, &key(cancel, 0x09)));
        assert_eq!(gui::focused_window(), edit, "Tab revient au début");
        assert_eq!(get_next_dlg_tab_item(dlg, edit, 1), cancel);
        assert!(is_dialog_message(dlg, &key(edit, 0x1B)));
        assert!(lock(dialogs())
            .get(&dlg)
            .is_some_and(|d| d.ended && d.result == 2));
        assert!(
            !is_dialog_message(dlg, &key(12345, 0x09)),
            "message d'une autre fenêtre"
        );
    }
    gui::destroy_window(dlg);
    assert!(!is_dialog(dlg));
}

#[test]
fn champ_desactive_dans_wm_initdialog_ne_prend_pas_le_focus() {
    // #704 : Notepad++ désactive l'heure de compilation (premier WS_TABSTOP)
    // dans WM_INITDIALOG ; le focus doit aller au bouton suivant.
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    unsafe extern "win64" fn proc_(hwnd: usize, msg: u32, _wp: usize, _lp: isize) -> isize {
        if msg == WM_INITDIALOG {
            user32::enable_window(user32::get_dlg_item(hwnd, 100), 0);
            return 1;
        }
        0
    }
    let data = template(false);
    let dlg = unsafe {
        create_dialog_indirect_param_w(0, data.as_ptr(), 0, proc_ as *const () as usize, 0)
    };
    assert_ne!(dlg, 0);
    assert_eq!(gui::focused_window(), user32::get_dlg_item(dlg, 1));
    gui::destroy_window(dlg);
}

#[test]
fn boite_de_message_sans_affichage_et_boutons() {
    assert_eq!(
        message_box_buttons(4)
            .iter()
            .map(|b| b.0)
            .collect::<Vec<_>>(),
        vec![6, 7]
    );
    assert_eq!(message_box_buttons(3).len(), 3);
    // Sans affichage : bouton par défaut (MB_DEFBUTTON2 = 0x100).
    if !gui::backend_is_interactive() {
        assert_eq!(unsafe { message_box(0, "Fichier ?", "Test", 4 | 0x100) }, 7);
        assert_eq!(unsafe { message_box(0, "Info", "", 0) }, 1);
    }
    let t = message_box_template("Une ligne\nDeux", "Titre", 1, gui::work_area());
    let parsed = parse_template(&t).expect("modèle de MessageBox lisible");
    assert_eq!(parsed.title, "Titre");
    assert_eq!(parsed.items.len(), 3);
    assert_eq!(parsed.items[2].id, 2);
    assert_eq!(mnemonic("&Ouvrir"), Some('o'));
    assert_eq!(mnemonic("A && B"), None);
}

#[test]
fn retour_chariot_seul_coupe_la_ligne_d_une_boite_de_message() {
    assert_eq!(
        message_box_lines("\"a.txt\"\r\rModifié.\r\nRecharger ?"),
        "\"a.txt\"\n\nModifié.\nRecharger ?"
    );
}

#[test]
fn longue_boite_de_message_tient_dans_l_ecran() {
    // #1161 : la pile d'appels de Paint.NET donnait une boîte de 1492 px.
    let measure = |s: &str| s.chars().count() as i32 * 8;
    assert_eq!(message_box_wrap("un deux trois", 1000, &measure), (1, 104));
    // 40 px par ligne : « un deux » (56) ne tient pas, trois lignes.
    assert_eq!(message_box_wrap("un deux trois", 40, &measure).0, 3);
    // Un mot plus long que la ligne n'en prend qu'une.
    assert_eq!(message_box_wrap("abcdefghij", 40, &measure), (1, 40));
    assert_eq!(message_box_wrap("a\nb\r\nc", 40, &measure).0, 3);

    let line = "   at PaintDotNet.Interop.ComObject.GetInterfacePointer(Void* pObject) in D:\\src\\pdn\\ComObject.cs:line 49";
    let text = vec![line; 120].join("\n");
    let font = crate::dlls::gdi32::get_stock_object(17);
    // Écran de bureau et portable 1366 x 768 (barre des tâches de 48 px).
    for area in [
        Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1032,
        },
        Rect {
            left: 0,
            top: 0,
            right: 1366,
            bottom: 720,
        },
    ] {
        let t = message_box_template(&text, "Erreur", 0x10, area);
        let cy = u16::from_le_bytes([t[16], t[17]]) as i32;
        let px = cy * base_units(font).1 / 8;
        assert!(
            px + 40 <= area.height() + 8,
            "{px} px pour {} px",
            area.height()
        );
        let cx = u16::from_le_bytes([t[14], t[15]]) as i32 * base_units(font).0 / 4;
        assert!(cx > 508 && cx <= area.width(), "boîte élargie : {cx} px");
    }
    // Deuxième écran : la boîte reste dans sa zone de travail.
    let second = Rect {
        left: 1920,
        top: 0,
        right: 3286,
        bottom: 768,
    };
    let work = Rect {
        left: 0,
        top: 0,
        right: 3286,
        bottom: 1032,
    };
    assert_eq!(clip_to_work_area(second, work), second);
    let work = Rect {
        left: 0,
        top: 0,
        right: 3286,
        bottom: 720,
    };
    assert_eq!(clip_to_work_area(second, work).bottom, 720);
}

/// #2421 : la procédure posée par SetWindowLongPtr(DWLP_DLGPROC) reçoit les
/// messages suivants ; l'ancienne ne les voit plus.
#[test]
fn procedure_remplacee_par_dwlp_dlgproc() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    static VUS_PAR_L_ANCIENNE: StdMutex<Vec<u32>> = StdMutex::new(Vec::new());
    unsafe extern "win64" fn nouvelle(hwnd: usize, msg: u32, wp: usize, _lp: isize) -> isize {
        if msg == WM_COMMAND && wp & 0xFFFF == IDCANCEL {
            end_dialog(hwnd, 3);
            return 1;
        }
        0
    }
    unsafe extern "win64" fn ancienne(hwnd: usize, msg: u32, _wp: usize, _lp: isize) -> isize {
        VUS_PAR_L_ANCIENNE.lock().unwrap().push(msg);
        if msg == WM_INITDIALOG {
            user32::set_window_long_ptr_w(hwnd, DWLP_DLGPROC, nouvelle as *const () as isize);
            return 1;
        }
        0
    }
    VUS_PAR_L_ANCIENNE.lock().unwrap().clear();
    let data = template(false);
    let dlg = unsafe {
        create_dialog_indirect_param_w(0, data.as_ptr(), 0, ancienne as *const () as usize, 0)
    };
    assert_ne!(dlg, 0);
    assert_eq!(
        unsafe { user32::get_window_long_ptr_w(dlg, DWLP_DLGPROC) },
        nouvelle as *const () as isize
    );
    VUS_PAR_L_ANCIENNE.lock().unwrap().clear();
    let edit = user32::get_dlg_item(dlg, 100);
    let echap = Msg {
        hwnd: edit,
        message: WM_KEYDOWN,
        wparam: 0x1B,
        lparam: 0,
        time: 0,
        pt: gui::Point::default(),
        l_private: 0,
    };
    unsafe {
        assert!(is_dialog_message(dlg, &echap));
    }
    assert!(
        lock(dialogs())
            .get(&dlg)
            .is_some_and(|d| d.ended && d.result == 3),
        "IDCANCEL atteint la nouvelle procédure"
    );
    assert!(
        VUS_PAR_L_ANCIENNE.lock().unwrap().is_empty(),
        "l'ancienne procédure ne reçoit plus rien"
    );
    gui::destroy_window(dlg);
}
