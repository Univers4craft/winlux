use super::*;
use std::sync::Mutex as StdMutex;

pub(crate) static SERIAL: StdMutex<()> = StdMutex::new(());
static SEEN: StdMutex<Vec<(u32, usize)>> = StdMutex::new(Vec::new());

/// Modèle DLGTEMPLATE (ou EX) : une boîte, un champ, deux boutons.
fn template(ex: bool) -> Vec<u8> {
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
    let style = 0x80C8_0000 | DS_SETFONT | DS_CENTER;
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
    p16(&mut o, 9);
    if ex {
        p16(&mut o, 400);
        o.push(0);
        o.push(1);
    }
    ps(&mut o, "MS Shell Dlg");
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
    let t = message_box_template("Une ligne\nDeux", "Titre", 1);
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
