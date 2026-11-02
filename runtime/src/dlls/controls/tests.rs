use super::*;
use std::sync::Mutex as StdMutex;

/// Notifications WM_COMMAND reçues par la fenêtre parente : (id, code).
static NOTES: StdMutex<Vec<(u16, u16)>> = StdMutex::new(Vec::new());
static SERIAL: StdMutex<()> = StdMutex::new(());
/// Codes WM_NOTIFY reçus par la fenêtre parente.
static NOTIFY: StdMutex<Vec<i32>> = StdMutex::new(Vec::new());

extern "win64" fn parent_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    if msg == WM_COMMAND {
        NOTES.lock().unwrap().push((wp as u16, (wp >> 16) as u16));
        return 0;
    }
    if msg == 0x004E && lp != 0 {
        let code = unsafe { *((lp as *const u8).add(16) as *const i32) };
        NOTIFY.lock().unwrap().push(code);
        return 0;
    }
    gui::def_window_proc(hwnd, msg, wp, lp)
}

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

fn parent_window() -> usize {
    gui::register_class(
        "WinluxTestParent",
        gui::ClassInfo {
            wnd_proc: parent_proc as *const () as usize,
            background: 0,
            instance: 0,
        },
    );
    let class = w("WinluxTestParent");
    let title = w("Parent");
    unsafe {
        user32::create_window_ex_w(
            0,
            class.as_ptr(),
            title.as_ptr(),
            0x00CF_0000,
            100,
            100,
            400,
            300,
            0,
            0,
            0,
            std::ptr::null_mut(),
        )
    }
}

fn child(
    parent: usize,
    class: &str,
    text: &str,
    style: u32,
    id: usize,
    rect: (i32, i32, i32, i32),
) -> usize {
    let class = w(class);
    let text = w(text);
    unsafe {
        user32::create_window_ex_w(
            0,
            class.as_ptr(),
            text.as_ptr(),
            0x5000_0000 | style, // WS_CHILD | WS_VISIBLE
            rect.0,
            rect.1,
            rect.2,
            rect.3,
            parent,
            id,
            0,
            std::ptr::null_mut(),
        )
    }
}

fn notes() -> Vec<(u16, u16)> {
    std::mem::take(&mut *NOTES.lock().unwrap())
}

fn sendw(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    unsafe { user32::send_message_w(hwnd, msg, wp, lp) }
}

fn text_of(hwnd: usize) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { user32::get_window_text_w(hwnd, buf.as_mut_ptr(), 256) };
    String::from_utf16_lossy(&buf[..n as usize])
}

#[test]
fn les_classes_predefinies_existent_sans_tenir_compte_de_la_casse() {
    for name in [
        "BUTTON",
        "button",
        "Edit",
        "STATIC",
        "ListBox",
        "ComboBox",
        "ScrollBar",
    ] {
        assert!(gui::lookup_class(name).is_some(), "{name}");
    }
    for name in [
        "SysTreeView32",
        "SysListView32",
        "SysTabControl32",
        "msctls_statusbar32",
        "msctls_progress32",
        "msctls_trackbar32",
        "msctls_updown32",
        "tooltips_class32",
    ] {
        assert!(gui::lookup_class(name).is_some(), "{name}");
    }
    assert!(gui::lookup_class("SysPager").is_none());
    assert_eq!(atom_class(0x80), Some("Button"));
}

#[test]
fn boutons_cases_et_radios() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    notes();
    let ok = child(p, "BUTTON", "&OK", 0x1, 1, (10, 10, 80, 24));
    let check = child(p, "Button", "Garder", 0x3, 2, (10, 40, 120, 20));
    let r1 = child(p, "BUTTON", "Un", 0x9 | WS_GROUP, 3, (10, 70, 120, 20));
    let r2 = child(p, "BUTTON", "Deux", 0x9, 4, (10, 90, 120, 20));
    assert_ne!(ok, 0);
    assert_eq!(user32::get_dlg_item(p, 2), check);
    assert_eq!(
        sendw(ok, WM_GETDLGCODE, 0, 0),
        DLGC_BUTTON | DLGC_DEFPUSHBUTTON
    );
    // Clic de souris : enfoncé puis relâché dans le bouton.
    sendw(ok, WM_LBUTTONDOWN, 1, (5 << 16) | 5);
    sendw(ok, WM_LBUTTONUP, 0, (5 << 16) | 5);
    assert_eq!(notes(), vec![(1, 0)], "BN_CLICKED");
    // Relâché hors du bouton : pas de clic.
    sendw(ok, WM_LBUTTONDOWN, 1, (5 << 16) | 5);
    sendw(ok, WM_LBUTTONUP, 0, (5 << 16) | 500);
    assert!(notes().is_empty());
    // Case automatique.
    sendw(check, 0x00F5, 0, 0); // BM_CLICK
    assert_eq!(sendw(check, 0x00F0, 0, 0), 1);
    assert_eq!(unsafe { super::is_dlg_button_checked(p, 2) }, 1);
    unsafe { super::check_dlg_button(p, 2, 0) };
    assert_eq!(sendw(check, 0x00F0, 0, 0), 0);
    // Radios du même groupe : un seul coché.
    sendw(r1, 0x00F5, 0, 0);
    sendw(r2, 0x00F5, 0, 0);
    assert_eq!(sendw(r1, 0x00F0, 0, 0), 0);
    assert_eq!(sendw(r2, 0x00F0, 0, 0), 1);
    // Bouton désactivé : plus de clic.
    unsafe { user32::enable_window(ok, 0) };
    assert_eq!(unsafe { user32::is_window_enabled(ok) }, 0);
    notes();
    sendw(ok, WM_LBUTTONDOWN, 1, (5 << 16) | 5);
    sendw(ok, WM_LBUTTONUP, 0, (5 << 16) | 5);
    assert!(notes().iter().all(|n| n.0 != 1));
    gui::destroy_window(p);
    assert_eq!(kind_of(ok), None, "état libéré à la destruction");
}

#[test]
fn champ_de_saisie_texte_selection_et_notifications() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let e = child(p, "EDIT", "", WS_BORDER | 0x80, 10, (10, 10, 200, 24));
    notes();
    for ch in "putty".chars() {
        sendw(e, WM_CHAR, ch as usize, 0);
    }
    assert_eq!(text_of(e), "putty");
    assert!(notes().contains(&(10, 0x0300)), "EN_CHANGE");
    // Retour arrière, sélection, remplacement.
    sendw(e, WM_CHAR, 0x08, 0);
    assert_eq!(text_of(e), "putt");
    sendw(e, 0x00B1, 0, 2); // EM_SETSEL 0..2
    let r = sendw(e, 0x00B0, 0, 0);
    assert_eq!((r & 0xFFFF, r >> 16), (0, 2));
    let rep = w("ba");
    sendw(e, 0x00C2, 0, rep.as_ptr() as isize); // EM_REPLACESEL
    assert_eq!(text_of(e), "batt");
    // Limite de longueur.
    sendw(e, 0x00C5, 5, 0);
    sendw(e, 0x00B1, 4, 4);
    for ch in "xyz".chars() {
        sendw(e, WM_CHAR, ch as usize, 0);
    }
    assert_eq!(text_of(e), "battx");
    // SetDlgItemText / GetDlgItemText.
    let t = w("192.168.1.10");
    unsafe { user32::set_dlg_item_text_w(p, 10, t.as_ptr()) };
    let mut buf = [0u16; 64];
    let n = unsafe { user32::get_dlg_item_text_w(p, 10, buf.as_mut_ptr(), 64) };
    assert_eq!(String::from_utf16_lossy(&buf[..n as usize]), "192.168.1.10");
    // Chaînes ANSI par SendMessageA.
    let a = b"port\0";
    unsafe { user32::send_message_a(e, WM_SETTEXT, 0, a.as_ptr() as isize) };
    let mut abuf = [0u8; 16];
    let n = unsafe { user32::send_message_a(e, WM_GETTEXT, 16, abuf.as_mut_ptr() as isize) };
    assert_eq!(&abuf[..n as usize], b"port");
    assert_eq!(
        unsafe { user32::send_message_a(e, WM_GETTEXTLENGTH, 0, 0) },
        4
    );
    // Champ numérique : les lettres sont refusées.
    let num = child(p, "Edit", "", 0x2000, 11, (10, 40, 80, 24));
    sendw(num, WM_CHAR, '2' as usize, 0);
    sendw(num, WM_CHAR, 'a' as usize, 0);
    sendw(num, WM_CHAR, '2' as usize, 0);
    assert_eq!(text_of(num), "22");
    gui::destroy_window(p);
}

#[test]
fn champ_multiligne_lignes_et_touches() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let e = child(
        p,
        "EDIT",
        "un\r\ndeux\r\ntrois",
        0x4 | WS_BORDER,
        12,
        (0, 0, 200, 100),
    );
    assert_eq!(sendw(e, 0x00BA, 0, 0), 3); // EM_GETLINECOUNT
    assert_eq!(sendw(e, 0x00BB, 1, 0), 4); // EM_LINEINDEX
    assert_eq!(sendw(e, 0x00C1, 4, 0), 4); // EM_LINELENGTH
    let mut buf = [0u16; 16];
    buf[0] = 16;
    let n = sendw(e, 0x00C4, 2, buf.as_mut_ptr() as isize); // EM_GETLINE
    assert_eq!(String::from_utf16_lossy(&buf[..n as usize]), "trois");
    // Fin de la première ligne puis Entrée : nouvelle ligne.
    sendw(e, 0x00B1, 0, 0);
    sendw(e, WM_KEYDOWN, VK_END, 0);
    sendw(e, WM_CHAR, 0x0D, 0);
    assert_eq!(text_of(e), "un\r\n\r\ndeux\r\ntrois");
    // Retour arrière efface \r\n d'un coup.
    sendw(e, WM_CHAR, 0x08, 0);
    assert_eq!(text_of(e), "un\r\ndeux\r\ntrois");
    sendw(e, WM_KEYDOWN, VK_DOWN, 0);
    assert_eq!(sendw(e, 0x00C9, usize::MAX, 0), 1); // EM_LINEFROMCHAR
    gui::destroy_window(p);
}

#[test]
fn liste_ajout_tri_selection_double_clic() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let lb = child(
        p,
        "LISTBOX",
        "",
        LBS_NOTIFY | LBS_SORT | WS_BORDER,
        20,
        (0, 0, 150, 200),
    );
    for s in ["Default Settings", "serveur", "Box"] {
        let t = w(s);
        sendw(lb, 0x180, 0, t.as_ptr() as isize);
    }
    assert_eq!(sendw(lb, 0x18B, 0, 0), 3);
    let mut buf = [0u16; 64];
    let n = sendw(lb, 0x189, 0, buf.as_mut_ptr() as isize);
    assert_eq!(
        String::from_utf16_lossy(&buf[..n as usize]),
        "Box",
        "tri sans casse"
    );
    let needle = w("serv");
    assert_eq!(sendw(lb, 0x18F, usize::MAX, needle.as_ptr() as isize), 2);
    sendw(lb, 0x19A, 2, 77);
    assert_eq!(sendw(lb, 0x199, 2, 0), 77);
    notes();
    // Clic sur la deuxième ligne, puis double-clic.
    let ih = sendw(lb, 0x1A1, 0, 0) as isize;
    let y = ih + ih / 2;
    sendw(lb, WM_LBUTTONDOWN, 1, y << 16);
    assert_eq!(sendw(lb, 0x188, 0, 0), 1);
    sendw(lb, WM_LBUTTONDOWN, 1, y << 16);
    let codes: Vec<_> = notes().into_iter().filter(|n| n.1 != 4).collect(); // sans LBN_SETFOCUS
    assert_eq!(
        codes,
        vec![(20, 1), (20, 2)],
        "LBN_SELCHANGE puis LBN_DBLCLK"
    );
    sendw(lb, WM_KEYDOWN, VK_END, 0);
    assert_eq!(sendw(lb, 0x188, 0, 0), 2);
    sendw(lb, 0x182, 0, 0);
    assert_eq!(sendw(lb, 0x18B, 0, 0), 2);
    assert_eq!(
        sendw(lb, 0x188, 0, 0),
        1,
        "la sélection suit la suppression"
    );
    gui::destroy_window(p);
}

#[test]
fn liste_deroulante_selection_et_texte() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let cb = child(p, "COMBOBOX", "", 0x3, 30, (0, 0, 150, 200));
    // La fenêtre fermée ne garde que la hauteur du champ.
    let h = gui::window_rect(cb).unwrap().height();
    assert!(h < 60, "hauteur fermée {h}");
    for s in ["Telnet", "SSH", "Serial"] {
        let t = w(s);
        sendw(cb, 0x143, 0, t.as_ptr() as isize);
    }
    sendw(cb, 0x14E, 1, 0);
    assert_eq!(text_of(cb), "SSH");
    notes();
    sendw(cb, WM_KEYDOWN, VK_DOWN, 0);
    assert_eq!(sendw(cb, 0x147, 0, 0), 2);
    assert_eq!(text_of(cb), "Serial");
    assert_eq!(notes(), vec![(30, 1)], "CBN_SELCHANGE");
    // Ouverture : la fenêtre s'agrandit ; un clic dans la liste choisit.
    sendw(cb, 0x14F, 1, 0);
    assert_eq!(sendw(cb, 0x157, 0, 0), 1);
    assert!(gui::window_rect(cb).unwrap().height() > h);
    let ih = sendw(cb, 0x154, 0, 0) as isize;
    let y = h as isize + 2 + ih / 2;
    sendw(cb, WM_LBUTTONDOWN, 1, (y << 16) | 10);
    assert_eq!(sendw(cb, 0x147, 0, 0), 0);
    assert_eq!(text_of(cb), "Telnet");
    assert_eq!(sendw(cb, 0x157, 0, 0), 0, "refermée après le choix");
    assert_eq!(gui::window_rect(cb).unwrap().height(), h);
    let mut buf = [0u16; 32];
    let n = sendw(cb, 0x148, 1, buf.as_mut_ptr() as isize);
    assert_eq!(String::from_utf16_lossy(&buf[..n as usize]), "SSH");
    // Liste éditable : on tape librement.
    let ed = child(p, "ComboBox", "", 0x2, 31, (0, 40, 150, 200));
    sendw(ed, WM_CHAR, 'x' as usize, 0);
    assert_eq!(text_of(ed), "x");
    gui::destroy_window(p);
}

#[test]
fn barre_de_defilement_position_et_glisser() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let sb = child(p, "SCROLLBAR", "", 0x1, 40, (0, 0, 16, 200));
    let mut si = ScrollInfo {
        size: std::mem::size_of::<ScrollInfo>() as u32,
        mask: 0x7,
        min: 0,
        max: 99,
        page: 10,
        pos: 95,
        track_pos: 0,
    };
    unsafe { user32::set_scroll_info(sb, 2, (&mut si as *mut ScrollInfo).cast(), 1) };
    assert_eq!(
        sendw(sb, 0x00E1, 0, 0),
        90,
        "position bornée à max - page + 1"
    );
    sendw(sb, 0x00E0, 20, 1);
    let mut out = ScrollInfo { mask: 0x7, ..si };
    unsafe { user32::get_scroll_info(sb, 2, (&mut out as *mut ScrollInfo).cast()) };
    assert_eq!((out.min, out.max, out.page, out.pos), (0, 99, 10, 20));
    gui::destroy_window(p);
}

#[test]
fn coupure_des_lignes_et_marques_de_raccourci() {
    assert_eq!(strip_prefix("&Ouvrir && fermer"), "Ouvrir & fermer");
    let font = gdi32::get_stock_object(17);
    let one = measure(font, "mot").0;
    let lines = wrap(font, "mot mot mot", one * 2);
    assert!(lines.len() >= 2, "{lines:?}");
    assert_eq!(wrap(font, "a\r\nb", 1000), vec!["a", "b"]);
}

// ---------- comctl32 ----------

fn notify_codes() -> Vec<i32> {
    std::mem::take(&mut *NOTIFY.lock().unwrap())
}

#[repr(C)]
struct TvInsert {
    parent: usize,
    after: usize,
    mask: u32,
    item: usize,
    state: u32,
    state_mask: u32,
    text: *const u16,
    text_max: i32,
    image: i32,
    selected_image: i32,
    children: i32,
    param: isize,
}

fn tv_insert(tv: usize, parent: usize, text: &str, param: isize) -> usize {
    let t = w(text);
    let mut ins = TvInsert {
        parent,
        after: 0xFFFF_FFFF_FFFF_0002, // TVI_LAST
        mask: 0x1 | 0x4,              // TEXT | PARAM
        item: 0,
        state: 0,
        state_mask: 0,
        text: t.as_ptr(),
        text_max: 0,
        image: 0,
        selected_image: 0,
        children: 0,
        param,
    };
    sendw(tv, 0x1132, 0, &mut ins as *mut TvInsert as isize) as usize
}

#[test]
fn arborescence_insertion_depliage_et_selection() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let tv = child(p, "SysTreeView32", "", 0x7, 50, (0, 0, 200, 200));
    let session = tv_insert(tv, 0xFFFF_FFFF_FFFF_0000, "Session", 1);
    let terminal = tv_insert(tv, 0xFFFF_FFFF_FFFF_0000, "Terminal", 2);
    let clavier = tv_insert(tv, terminal, "Clavier", 3);
    assert!(session != 0 && terminal != 0 && clavier != 0);
    assert_eq!(sendw(tv, 0x1105, 0, 0), 3, "TVM_GETCOUNT");
    // TVM_GETNEXTITEM : racine, suivant, enfant, parent
    assert_eq!(sendw(tv, 0x110A, 0, 0) as usize, session);
    assert_eq!(sendw(tv, 0x110A, 1, session as isize) as usize, terminal);
    assert_eq!(sendw(tv, 0x110A, 4, terminal as isize) as usize, clavier);
    assert_eq!(sendw(tv, 0x110A, 3, clavier as isize) as usize, terminal);
    // Dépliage puis sélection : TVN_SELCHANGINGW / TVN_SELCHANGEDW
    notify_codes();
    assert_eq!(sendw(tv, 0x1102, 2, terminal as isize), 1, "TVM_EXPAND");
    assert_eq!(sendw(tv, 0x110B, 9, clavier as isize), 1, "TVM_SELECTITEM");
    assert_eq!(sendw(tv, 0x110A, 9, 0) as usize, clavier, "TVGN_CARET");
    let codes = notify_codes();
    assert!(codes.contains(&-450) && codes.contains(&-451), "{codes:?}");
    // TVM_GETITEMW : texte et lParam
    let mut buf = [0u16; 32];
    let mut it = TvInsert {
        parent: 0,
        after: 0,
        mask: 0x1 | 0x4 | 0x10,
        item: clavier,
        state: 0,
        state_mask: 0,
        text: buf.as_mut_ptr(),
        text_max: 32,
        image: 0,
        selected_image: 0,
        children: 0,
        param: 0,
    };
    let item_ptr = unsafe { (&mut it as *mut TvInsert as *mut u8).add(16) };
    assert_eq!(sendw(tv, 0x113E, 0, item_ptr as isize), 1);
    let n = buf.iter().position(|&c| c == 0).unwrap();
    assert_eq!(String::from_utf16_lossy(&buf[..n]), "Clavier");
    assert_eq!(it.param, 3);
    // Suppression d'un parent : ses enfants partent avec lui
    assert_eq!(sendw(tv, 0x1101, 0, terminal as isize), 1);
    assert_eq!(sendw(tv, 0x1105, 0, 0), 1);
    gui::destroy_window(p);
}

#[repr(C)]
struct LvItemW {
    mask: u32,
    item: i32,
    sub_item: i32,
    state: u32,
    state_mask: u32,
    text: *const u16,
    text_max: i32,
    image: i32,
    param: isize,
}

#[repr(C)]
struct LvColumnW {
    mask: u32,
    fmt: i32,
    cx: i32,
    text: *const u16,
    text_max: i32,
    sub_item: i32,
}

#[test]
fn liste_a_colonnes_lignes_textes_et_selection() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let lv = child(p, "SysListView32", "", 0x1 | 0x4, 51, (0, 0, 300, 200));
    for (i, name) in ["Nom", "Port"].iter().enumerate() {
        let t = w(name);
        let mut col = LvColumnW {
            mask: 0x2 | 0x4, // WIDTH | TEXT
            fmt: 0,
            cx: 120,
            text: t.as_ptr(),
            text_max: 0,
            sub_item: 0,
        };
        assert_eq!(
            sendw(lv, 0x1061, i, &mut col as *mut LvColumnW as isize),
            i as isize
        );
    }
    for (i, (host, port)) in [("serveur", "22"), ("routeur", "23")].iter().enumerate() {
        let t = w(host);
        let mut it = LvItemW {
            mask: 0x1 | 0x4,
            item: i as i32,
            sub_item: 0,
            state: 0,
            state_mask: 0,
            text: t.as_ptr(),
            text_max: 0,
            image: 0,
            param: 100 + i as isize,
        };
        assert_eq!(
            sendw(lv, 0x104D, 0, &mut it as *mut LvItemW as isize),
            i as isize
        );
        let t = w(port);
        let mut sub = LvItemW {
            mask: 0x1,
            item: i as i32,
            sub_item: 1,
            state: 0,
            state_mask: 0,
            text: t.as_ptr(),
            text_max: 0,
            image: 0,
            param: 0,
        };
        assert_eq!(sendw(lv, 0x104C, 0, &mut sub as *mut LvItemW as isize), 1);
    }
    assert_eq!(sendw(lv, 0x1004, 0, 0), 2, "LVM_GETITEMCOUNT");
    let mut buf = [0u16; 16];
    let mut get = LvItemW {
        mask: 0x1,
        item: 1,
        sub_item: 1,
        state: 0,
        state_mask: 0,
        text: buf.as_mut_ptr(),
        text_max: 16,
        image: 0,
        param: 0,
    };
    let n = sendw(lv, 0x1073, 1, &mut get as *mut LvItemW as isize);
    assert_eq!(String::from_utf16_lossy(&buf[..n as usize]), "23");
    // Sélection : LVM_SETITEMSTATE puis LVM_GETNEXTITEM(LVNI_SELECTED)
    notify_codes();
    let mut st = LvItemW {
        mask: 0x8,
        item: 0,
        sub_item: 0,
        state: 0x3,
        state_mask: 0x3,
        text: std::ptr::null(),
        text_max: 0,
        image: 0,
        param: 0,
    };
    sendw(lv, 0x102B, 1, &mut st as *mut LvItemW as isize);
    assert_eq!(sendw(lv, 0x100C, usize::MAX, 0x2), 1);
    assert!(notify_codes().contains(&-101), "LVN_ITEMCHANGED");
    gui::destroy_window(p);
}

#[repr(C)]
struct TcItemW {
    mask: u32,
    state: u32,
    state_mask: u32,
    text: *const u16,
    text_max: i32,
    image: i32,
    param: isize,
}

#[test]
fn onglets_barre_d_etat_progression_et_curseur() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let tab = child(p, "SysTabControl32", "", 0, 52, (0, 0, 300, 200));
    for (i, name) in ["Général", "Avancé"].iter().enumerate() {
        let t = w(name);
        let mut it = TcItemW {
            mask: 0x1,
            state: 0,
            state_mask: 0,
            text: t.as_ptr(),
            text_max: 0,
            image: 0,
            param: 0,
        };
        assert_eq!(
            sendw(tab, 0x133E, i, &mut it as *mut TcItemW as isize),
            i as isize
        );
    }
    assert_eq!(sendw(tab, 0x1304, 0, 0), 2);
    assert_eq!(sendw(tab, 0x130B, 0, 0), 0, "premier onglet choisi");
    assert_eq!(sendw(tab, 0x130C, 1, 0), 0, "SETCURSEL renvoie l'ancien");
    assert_eq!(sendw(tab, 0x130B, 0, 0), 1);

    let status = unsafe {
        let t = w("Prêt");
        crate::dlls::comctl32::create_status_window_w(0x1000_0000, t.as_ptr(), p, 53)
    };
    assert!(status != 0);
    let mut buf = [0u16; 16];
    let n = sendw(status, 0x040D, 0, buf.as_mut_ptr() as isize) & 0xFFFF;
    assert_eq!(String::from_utf16_lossy(&buf[..n as usize]), "Prêt");
    let r = gui::window_rect(status).unwrap();
    assert_eq!(r.bottom - r.top, 26, "hauteur de la barre d'état");

    let bar = child(p, "msctls_progress32", "", 0, 54, (0, 0, 200, 20));
    sendw(bar, 0x0406, 0, 50); // PBM_SETRANGE32
    sendw(bar, 0x0402, 30, 0); // PBM_SETPOS
    assert_eq!(sendw(bar, 0x0408, 0, 0), 30);
    sendw(bar, 0x0404, 40, 0); // PBM_SETSTEP
    sendw(bar, 0x0405, 0, 0); // PBM_STEPIT
    assert_eq!(
        sendw(bar, 0x0408, 0, 0),
        20,
        "repart du début comme Windows"
    );

    let track = child(p, "msctls_trackbar32", "", 0, 55, (0, 0, 200, 30));
    sendw(track, 0x0406, 1, (10 << 16) as isize); // TBM_SETRANGE 0..10
    sendw(track, 0x0405, 1, 7); // TBM_SETPOS
    assert_eq!(sendw(track, 0x0400, 0, 0), 7);
    gui::destroy_window(p);
}
