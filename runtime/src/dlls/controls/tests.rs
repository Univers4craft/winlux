use super::*;
use crate::dlls::menus;
use std::sync::Mutex as StdMutex;

/// Notifications WM_COMMAND reçues par la fenêtre parente : (id, code).
static NOTES: StdMutex<Vec<(u16, u16)>> = StdMutex::new(Vec::new());
static SERIAL: StdMutex<()> = StdMutex::new(());
/// Codes WM_NOTIFY reçus par la fenêtre parente.
static NOTIFY: StdMutex<Vec<i32>> = StdMutex::new(Vec::new());
/// URL des NM_CLICK de SysLink reçus par la fenêtre parente.
static LINK_URLS: StdMutex<Vec<String>> = StdMutex::new(Vec::new());
/// WM_CONTEXTMENU reçus par la fenêtre parente : (fenêtre, lParam).
static CONTEXT: StdMutex<Vec<(usize, isize)>> = StdMutex::new(Vec::new());
/// WM_DRAWITEM reçus par la fenêtre parente : (type de contrôle, élément, donnée).
static DRAWN: StdMutex<Vec<(u32, u32, usize)>> = StdMutex::new(Vec::new());

extern "win64" fn parent_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    if msg == WM_COMMAND {
        NOTES.lock().unwrap().push((wp as u16, (wp >> 16) as u16));
        return 0;
    }
    if msg == 0x002B && lp != 0 {
        // WM_DRAWITEM : CtlType, CtlID, itemID… itemData en fin de structure.
        unsafe {
            let p = lp as *const u8;
            let ctl_type = *(p as *const u32);
            let item = *(p.add(8) as *const u32);
            let data = *(p.add(56) as *const usize);
            DRAWN.lock().unwrap().push((ctl_type, item, data));
        }
        return 1;
    }
    if msg == 0x007B {
        CONTEXT.lock().unwrap().push((wp, lp));
        return 0;
    }
    if msg == 0x004E && lp != 0 {
        let code = unsafe { ((lp as *const u8).add(16) as *const i32).read_unaligned() };
        NOTIFY.lock().unwrap().push(code);
        if code == -176 {
            // LVN_ENDLABELEDITW : nouveau nom accepté.
            return 1;
        }
        if code == -2
            && crate::dlls::gui::window_class(unsafe { (lp as *const usize).read_unaligned() })
                .is_some_and(|c| c.eq_ignore_ascii_case("SysLink"))
        {
            // NMLINK : szUrl à 24 + 112.
            let url: Vec<u16> = unsafe {
                let p = (lp as *const u8).add(136) as *const u16;
                (0..2084)
                    .map(|k| *p.add(k))
                    .take_while(|&c| c != 0)
                    .collect()
            };
            LINK_URLS
                .lock()
                .unwrap()
                .push(String::from_utf16_lossy(&url));
        }
        if code == -452 {
            // TVN_GETDISPINFOW : NMHDR (24 octets) puis TVITEMW ; mask à 0,
            // hItem à 8, pszText à 24, cchTextMax à 32, lParam à 48.
            unsafe {
                let it = (lp as *mut u8).add(24);
                let param = (it.add(48) as *const isize).read_unaligned();
                let s = w(&format!("rappel{param}"));
                let buf = (it.add(24) as *const usize).read_unaligned() as *mut u16;
                let cap = (it.add(32) as *const i32).read_unaligned() as usize;
                std::ptr::copy_nonoverlapping(s.as_ptr(), buf, s.len().min(cap));
                if param == 2 {
                    // TVIF_DI_SETITEM : le contrôle retient le texte.
                    let mask = it as *mut u32;
                    mask.write_unaligned(mask.read_unaligned() | 0x1000);
                }
            }
        }
        if code == -177 {
            // LVN_GETDISPINFOW : NMHDR (24 octets) puis LVITEMW
            unsafe {
                let it = &mut *((lp as *mut u8).add(24) as *mut LvItemW);
                let s: Vec<u16> = format!("auto{}-{}", it.item, it.sub_item)
                    .encode_utf16()
                    .chain([0])
                    .collect();
                let n = s.len().min(it.text_max as usize);
                std::ptr::copy_nonoverlapping(s.as_ptr(), it.text as *mut u16, n);
            }
        }
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
fn champ_marges_em_setmargins() {
    // #1377 : marges de la police par défaut, puis celles d'EM_SETMARGINS ;
    // le texte aligné à droite ne touche plus le bord du champ.
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let e = child(p, "Edit", "1", 0x2002, 12, (10, 10, 42, 15));
    let m = sendw(e, 0x00D4, 0, 0); // EM_GETMARGINS
    let (l, r) = (m & 0xFFFF, m >> 16);
    assert!(l > 0 && r > 0, "marges de la police : {l}, {r}");
    sendw(e, 0x00D3, 2, 7 << 16); // EC_RIGHTMARGIN = 7
    assert_eq!(sendw(e, 0x00D4, 0, 0), (7 << 16) | l);
    let area = edit_area(e, client(e));
    assert_eq!((area.left, area.right), (l as i32, 42 - 7));
    sendw(e, 0x00D3, 0xFFFF, 0); // EC_USEFONTINFO
    assert_eq!(sendw(e, 0x00D4, 0, 0), m);
    // #1552 : EC_USEFONTINFO dans chaque mot de lParam (wxWidgets) donne
    // aussi les marges de la police, pas 65535 pixels.
    sendw(e, 0x00D3, 3, 7 | (7 << 16));
    sendw(e, 0x00D3, 3, 0xFFFF | (0xFFFF << 16));
    assert_eq!(sendw(e, 0x00D4, 0, 0), m);
    let area = edit_area(e, client(e));
    assert!(area.left < area.right, "zone de texte {area:?}");
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

/// #841 : comme sous Windows, SetWindowPos sur une liste déroulante fermée
/// ne change que la hauteur de la liste ouverte ; le champ garde la sienne
/// (wxWidgets relit cette taille pour régler le champ).
#[test]
fn liste_deroulante_garde_la_hauteur_du_champ() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let cb = child(p, "COMBOBOX", "", 0x2, 32, (0, 0, 150, 200));
    let h = gui::window_rect(cb).unwrap().height();
    unsafe { user32::set_window_pos(cb, 0, 0, 0, 300, 170, 0x0002 | 0x0004) };
    let r = gui::window_rect(cb).unwrap();
    assert_eq!((r.width(), r.height()), (300, h));
    // Champ plus haut : la fenêtre fermée suit.
    sendw(cb, 0x153, usize::MAX, 30);
    assert_eq!(gui::window_rect(cb).unwrap().height(), 36);
    // La liste ouverte reprend la hauteur demandée.
    sendw(cb, 0x14F, 1, 0);
    assert_eq!(gui::window_rect(cb).unwrap().height(), 170);
    sendw(cb, 0x14F, 0, 0);
    assert_eq!(gui::window_rect(cb).unwrap().height(), 36);
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
    // #951 : un mot trop large reste entier (DT_WORDBREAK ne coupe pas
    // dans un mot), les mots suivants passent à la ligne.
    let long = "SHA256:ABdT8Bs52isQHqLDwfvFHv1NyYJRY4VAZ3UpHJT/Ds";
    let narrow = measure(font, long).0 - 3;
    assert_eq!(wrap(font, long, narrow), vec![long]);
    assert_eq!(
        wrap(font, &format!("{long} fin"), narrow),
        vec![long.to_string(), "fin".to_string()]
    );
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

/// TTreeView de la VCL : éléments insérés en LPSTR_TEXTCALLBACK, texte rendu
/// par TVN_GETDISPINFOW au dessin (« New Site » vide dans WinSCP).
#[test]
fn arborescence_texte_demande_au_parent() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let tv = child(p, "SysTreeView32", "", 0x7, 50, (0, 0, 200, 200));
    let insert = |param: isize| {
        let mut ins = TvInsert {
            parent: 0xFFFF_FFFF_FFFF_0000,
            after: 0xFFFF_FFFF_FFFF_0002,
            mask: 0x1 | 0x4,
            item: 0,
            state: 0,
            state_mask: 0,
            text: usize::MAX as *const u16, // LPSTR_TEXTCALLBACKW
            text_max: 0,
            image: 0,
            selected_image: 0,
            children: 0,
            param,
        };
        sendw(tv, 0x1132, 0, &mut ins as *mut TvInsert as isize) as usize
    };
    let a = insert(1);
    let b = insert(2);
    assert!(a != 0 && b != 0);
    let text_of = |h: usize| {
        let mut buf = [0u16; 32];
        let mut it = TvInsert {
            parent: 0,
            after: 0,
            mask: 0x1 | 0x10,
            item: h,
            state: 0,
            state_mask: 0,
            text: buf.as_mut_ptr(),
            text_max: 32,
            image: 0,
            selected_image: 0,
            children: 0,
            param: 0,
        };
        let ptr = unsafe { (&mut it as *mut TvInsert as *mut u8).add(16) };
        sendw(tv, 0x113E, 0, ptr as isize);
        let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
        String::from_utf16_lossy(&buf[..n])
    };
    notify_codes();
    let (dc, bmp) = memory_dc(200, 200);
    sendw(tv, 0x0318, dc, 0); // WM_PRINTCLIENT
    let codes = notify_codes();
    assert_eq!(
        codes.iter().filter(|&&c| c == -452).count(),
        2,
        "TVN_GETDISPINFOW pour chaque élément : {codes:?}"
    );
    assert!(
        dark_pixels(bmp) > 0 || crate::dlls::usp10::spec_font(&gui::FontSpec::default()).is_none()
    );
    // TVIF_DI_SETITEM : le second texte est retenu, plus redemandé.
    assert_eq!(text_of(b), "rappel2");
    sendw(tv, 0x0318, dc, 0);
    let codes = notify_codes();
    assert_eq!(codes.iter().filter(|&&c| c == -452).count(), 1, "{codes:?}");
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
    // LVM_GETCOLUMNW : titre et largeur relus.
    let mut buf = [0u16; 16];
    let mut col = LvColumnW {
        mask: 0x2 | 0x4,
        fmt: 0,
        cx: 0,
        text: buf.as_mut_ptr(),
        text_max: 16,
        sub_item: 0,
    };
    assert_eq!(sendw(lv, 0x105F, 1, &mut col as *mut LvColumnW as isize), 1);
    assert_eq!(col.cx, 120);
    assert_eq!(
        &buf[..5],
        &[b'P' as u16, b'o' as u16, b'r' as u16, b't' as u16, 0]
    );
    assert_eq!(sendw(lv, 0x105F, 9, &mut col as *mut LvColumnW as isize), 0);
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

#[test]
fn liste_textes_fournis_a_la_demande_et_liste_virtuelle() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let lv = child(p, "SysListView32", "", 0x1, 52, (0, 0, 300, 200));
    let t = w("Nom");
    let mut col = LvColumnW {
        mask: 0x2 | 0x4,
        fmt: 0,
        cx: 120,
        text: t.as_ptr(),
        text_max: 0,
        sub_item: 0,
    };
    sendw(lv, 0x1061, 0, &mut col as *mut LvColumnW as isize);
    // LPSTR_TEXTCALLBACK : le texte vient de LVN_GETDISPINFOW
    let mut it = LvItemW {
        mask: 0x1,
        item: 0,
        sub_item: 0,
        state: 0,
        state_mask: 0,
        text: usize::MAX as *const u16,
        text_max: 0,
        image: 0,
        param: 0,
    };
    assert_eq!(sendw(lv, 0x104D, 0, &mut it as *mut LvItemW as isize), 0);
    let get = |item: usize, sub: i32| {
        let mut buf = [0u16; 32];
        let mut g = LvItemW {
            mask: 0x1,
            item: item as i32,
            sub_item: sub,
            state: 0,
            state_mask: 0,
            text: buf.as_mut_ptr(),
            text_max: 32,
            image: 0,
            param: 0,
        };
        let n = sendw(lv, 0x1073, item, &mut g as *mut LvItemW as isize);
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    };
    notify_codes();
    assert_eq!(get(0, 0), "auto0-0");
    assert!(notify_codes().contains(&-177), "LVN_GETDISPINFOW");
    // Sous-colonne jamais renseignée d'une ligne callback : demandée aussi
    // au parent (Taille, dates de 7-Zip, #553).
    assert_eq!(get(0, 1), "auto0-1");
    // LVM_GETCOLUMNW (largeur + LVCF_ORDER) : 7-Zip lance une exception si
    // l'appel échoue à l'ouverture d'un dossier (#553/#554).
    let mut raw = [0u8; 40];
    raw[0..4].copy_from_slice(&(0x2u32 | 0x20).to_le_bytes());
    raw[0x24..0x28].copy_from_slice(&(-1i32).to_le_bytes());
    assert_eq!(sendw(lv, 0x105F, 0, raw.as_mut_ptr() as isize), 1);
    assert_eq!(i32::from_le_bytes(raw[8..12].try_into().unwrap()), 120);
    assert_eq!(i32::from_le_bytes(raw[0x24..0x28].try_into().unwrap()), 0);
    assert_eq!(
        sendw(lv, 0x105F, 5, raw.as_mut_ptr() as isize),
        0,
        "colonne absente"
    );
    gui::destroy_window(p);

    // LVS_OWNERDATA : LVM_SETITEMCOUNT donne le nombre de lignes
    let p = parent_window();
    let lv2 = child(p, "SysListView32", "", 0x1 | 0x1000, 53, (0, 0, 300, 200));
    sendw(lv2, 0x1061, 0, &mut col as *mut LvColumnW as isize);
    assert_eq!(sendw(lv2, 0x102F, 4, 0), 1);
    assert_eq!(sendw(lv2, 0x1004, 0, 0), 4, "LVM_GETITEMCOUNT virtuel");
    let mut buf = [0u16; 32];
    let mut g = LvItemW {
        mask: 0x1,
        item: 3,
        sub_item: 1,
        state: 0,
        state_mask: 0,
        text: buf.as_mut_ptr(),
        text_max: 32,
        image: 0,
        param: 0,
    };
    let n = sendw(lv2, 0x1073, 3, &mut g as *mut LvItemW as isize);
    assert_eq!(String::from_utf16_lossy(&buf[..n as usize]), "auto3-1");
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

#[repr(C)]
struct TbButtonT {
    bitmap: i32,
    command: i32,
    state: u8,
    style: u8,
    reserved: [u8; 6],
    data: usize,
    string: isize,
}

#[test]
fn tb_addbitmap_nul_ne_consomme_pas_d_index() {
    // Notepad++ passe un HBITMAP nul pour chaque séparateur ; ses boutons
    // numérotent leurs images sans les compter (#1287).
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let tb = child(p, "ToolbarWindow32", "", 0x0800, 61, (0, 0, 0, 0));
    assert!(tb != 0);
    sendw(tb, 0x0420, 0, 0x0010_0010); // TB_SETBITMAPSIZE 16×16
                                       // Premier pixel blanc : couleur de transparence, le reste de la couleur.
    let bmp = |c: u32| {
        let mut px = [c; 256];
        px[0] = 0xFFFF_FFFF;
        gui::bitmap_from_pixels(16, 16, &px).unwrap()
    };
    let rouge = [0usize, bmp(0xFF00_00FF)];
    let nul = [0usize, 0];
    let vert = [0usize, bmp(0xFF00_FF00)];
    assert_eq!(sendw(tb, 0x0413, 1, rouge.as_ptr() as isize), 0);
    assert_eq!(sendw(tb, 0x0413, 1, nul.as_ptr() as isize), -1);
    assert_eq!(sendw(tb, 0x0413, 1, vert.as_ptr() as isize), 1);
    let list = sendw(tb, 0x0431, 0, 0) as usize; // TB_GETIMAGELIST
    assert_eq!(crate::dlls::icons::image_list_get_image_count(list), 2);
    let premier = crate::dlls::icons::list_image(list, 0).unwrap();
    let second = crate::dlls::icons::list_image(list, 1).unwrap();
    assert!(second.px.iter().any(|q| q >> 24 != 0), "image 1 vide");
    assert_ne!(premier.px[8 * 16 + 8], second.px[8 * 16 + 8]);
}

#[test]
fn barre_d_outils_bandes_et_liste_etendue() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for name in ["ToolbarWindow32", "ReBarWindow32", "ComboBoxEx32"] {
        assert!(gui::lookup_class(name).is_some(), "{name}");
    }
    let p = parent_window();
    notes();
    notify_codes();

    // Barre d'outils : boutons texte (chaîne en pointeur ou en index).
    let tb = child(
        p,
        "ToolbarWindow32",
        "",
        0x0800 | 0x4 | 0x8,
        60,
        (0, 0, 0, 0),
    );
    assert!(tb != 0);
    sendw(tb, 0x041E, std::mem::size_of::<TbButtonT>(), 0); // TB_BUTTONSTRUCTSIZE
    let names = w("Extraire\0Tester\0");
    assert_eq!(sendw(tb, 0x044D, 0, names.as_ptr() as isize), 0); // TB_ADDSTRINGW
    let add = w("Ajouter");
    let buttons = [
        TbButtonT {
            bitmap: 0,
            command: 1001,
            state: 0x4,
            style: 0,
            reserved: [0; 6],
            data: 7,
            string: add.as_ptr() as isize,
        },
        TbButtonT {
            bitmap: 1,
            command: 1002,
            state: 0x4,
            style: 0,
            reserved: [0; 6],
            data: 0,
            string: 0,
        },
        TbButtonT {
            bitmap: 0,
            command: 0,
            state: 0,
            style: 0x1, // séparateur
            reserved: [0; 6],
            data: 0,
            string: -1,
        },
        TbButtonT {
            bitmap: 2,
            command: 1003,
            state: 0x4,
            style: 0x2, // BTNS_CHECK
            reserved: [0; 6],
            data: 0,
            string: 1,
        },
    ];
    assert_eq!(sendw(tb, 0x0444, 4, buttons.as_ptr() as isize), 1); // TB_ADDBUTTONSW
    assert_eq!(sendw(tb, 0x0418, 0, 0), 4);
    assert_eq!(sendw(tb, 0x0419, 1003, 0), 3, "TB_COMMANDTOINDEX");
    let mut buf = [0u16; 32];
    let n = sendw(tb, 0x044B, 1002, buf.as_mut_ptr() as isize);
    assert_eq!(String::from_utf16_lossy(&buf[..n as usize]), "Extraire");
    let mut r = Rect::default();
    assert_eq!(sendw(tb, 0x041D, 0, &mut r as *mut Rect as isize), 1);
    assert!(r.width() > 0 && r.height() > 0);
    // Clic sur « Ajouter » : WM_COMMAND(1001) au parent.
    let at = (((r.top + 4) as isize) << 16) | (r.left + 4) as isize;
    sendw(tb, WM_LBUTTONDOWN, 1, at);
    sendw(tb, WM_LBUTTONUP, 0, at);
    assert_eq!(notes(), vec![(1001, 0)]);
    // Bouton à cocher ; bouton désactivé qui ne répond plus.
    let mut r3 = Rect::default();
    sendw(tb, 0x041D, 3, &mut r3 as *mut Rect as isize);
    let at3 = (((r3.top + 4) as isize) << 16) | (r3.left + 4) as isize;
    sendw(tb, WM_LBUTTONDOWN, 1, at3);
    sendw(tb, WM_LBUTTONUP, 0, at3);
    assert_eq!(sendw(tb, 0x040A, 1003, 0), 1, "TB_ISBUTTONCHECKED");
    sendw(tb, 0x0401, 1001, 0); // TB_ENABLEBUTTON(FALSE)
    assert_eq!(sendw(tb, 0x0409, 1001, 0), 0);
    notes();
    sendw(tb, WM_LBUTTONDOWN, 1, at);
    sendw(tb, WM_LBUTTONUP, 0, at);
    assert!(notes().is_empty());
    let mut size = [0i32; 2];
    sendw(tb, 0x0453, 0, size.as_mut_ptr() as isize); // TB_GETMAXSIZE
    assert!(size[0] > r.width() * 2 && size[1] > 0);

    // Bande : la barre d'outils devient son enfant, la hauteur suit.
    let rebar = child(p, "ReBarWindow32", "", 0x0400, 61, (0, 0, 400, 10));
    assert!(rebar != 0);
    let mut info = [0u8; 128];
    info[0..4].copy_from_slice(&128u32.to_le_bytes());
    info[4..8].copy_from_slice(&(0x10u32 | 0x20 | 0x40 | 0x1).to_le_bytes()); // CHILD|CHILDSIZE|SIZE|STYLE
    info[8..12].copy_from_slice(&0x100u32.to_le_bytes()); // RBBS_NOGRIPPER
    info[40..48].copy_from_slice(&tb.to_le_bytes());
    info[48..52].copy_from_slice(&50u32.to_le_bytes());
    info[52..56].copy_from_slice(&(size[1] as u32).to_le_bytes());
    info[56..60].copy_from_slice(&200u32.to_le_bytes());
    assert_eq!(sendw(rebar, 0x040A, usize::MAX, info.as_ptr() as isize), 1);
    assert_eq!(sendw(rebar, 0x040C, 0, 0), 1, "RB_GETBANDCOUNT");
    assert_eq!(
        gui::window_parent(tb),
        Some(rebar),
        "SetParent vers la bande"
    );
    let h = sendw(rebar, 0x041B, 0, 0);
    assert_eq!(h, size[1] as isize + 4, "RB_GETBARHEIGHT");
    assert_eq!(gui::window_rect(rebar).unwrap().height() as isize, h);
    assert!(notify_codes().contains(&-831), "RBN_HEIGHTCHANGE");
    // SetWindowPos ne peut pas écraser la hauteur d'une barre à bandes (7-Zip
    // la redimensionnait à 0 et la barre d'adresse disparaissait, #555).
    unsafe { user32::set_window_pos(rebar, 0, 0, 0, 400, 2, 0x4) };
    assert_eq!(gui::window_rect(rebar).unwrap().height() as isize, h);
    // Double-clic : les contrôles prédéfinis qui le traitent le reçoivent.
    assert!(gui::wants_double_clicks(tb));
    assert!(!gui::wants_double_clicks(rebar));
    // Un WM_COMMAND de la barre d'outils arrive toujours au parent d'origine.
    sendw(tb, 0x0401, 1001, 1);
    sendw(tb, WM_LBUTTONDOWN, 1, at);
    sendw(tb, WM_LBUTTONUP, 0, at);
    assert_eq!(notes(), vec![(1001, 0)]);

    // ComboBoxEx : éléments, texte du champ et notifications relayées.
    let cbex = child(
        p,
        "ComboBoxEx32",
        "",
        0x0002 | 0x0020_0000,
        62,
        (0, 40, 300, 200),
    );
    assert!(cbex != 0);
    let combo = sendw(cbex, 0x0406, 0, 0) as usize; // CBEM_GETCOMBOCONTROL
    assert!(combo != 0);
    assert_eq!(gui::window_parent(combo), Some(cbex));
    assert_eq!(
        sendw(cbex, 0x0407, 0, 0) as usize,
        combo,
        "CBEM_GETEDITCONTROL"
    );
    for (i, t) in ["C:\\", "Documents", "Bureau"].iter().enumerate() {
        let text = w(t);
        let item = [
            0x21usize, // CBEIF_TEXT | CBEIF_LPARAM
            usize::MAX,
            text.as_ptr() as usize,
            0,
            0,
            0,
            100 + i,
        ];
        assert_eq!(sendw(cbex, 0x040B, 0, item.as_ptr() as isize), i as isize);
    }
    assert_eq!(sendw(cbex, 0x0146, 0, 0), 3, "CB_GETCOUNT relayé");
    let mut out = [0u16; 32];
    let mut item = [0x21usize, 1, out.as_mut_ptr() as usize, 32, 0, 0, 0];
    assert_eq!(sendw(cbex, 0x040D, 0, item.as_mut_ptr() as isize), 1);
    let n = out.iter().position(|&c| c == 0).unwrap();
    assert_eq!(String::from_utf16_lossy(&out[..n]), "Documents");
    assert_eq!(item[6], 101);
    let path = w("D:\\Photos");
    let set = [0x1usize, usize::MAX, path.as_ptr() as usize, 0, 0, 0, 0];
    assert_eq!(sendw(cbex, 0x040C, 0, set.as_ptr() as isize), 1);
    assert_eq!(text_of(combo), "D:\\Photos");
    // SetWindowText sur la ComboBoxEx remplit son champ (barre d'adresse de 7-Zip).
    let addr = w("C:\\Users");
    unsafe { user32::set_window_text_w(cbex, addr.as_ptr()) };
    assert_eq!(text_of(combo), "C:\\Users");
    notes();
    sendw(cbex, 0x014E, 2, 0); // CB_SETCURSEL
    sendw(combo, WM_SETFOCUS, 0, 0); // CBN_SETFOCUS
    let got = notes();
    assert!(
        got.iter().any(|&(id, _)| id == 62),
        "WM_COMMAND avec l'identifiant de la ComboBoxEx : {got:?}"
    );
    notify_codes();
    sendw(combo, WM_KEYDOWN, VK_RETURN, 0);
    assert!(notify_codes().contains(&-806), "CBEN_ENDEDITW");
    gui::destroy_window(p);
}

#[test]
fn barre_de_menus_reduit_la_zone_client_et_place_les_titres() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    gui::register_class(
        "WinluxTestMenuBar",
        gui::ClassInfo {
            wnd_proc: parent_proc as *const () as usize,
            background: 0,
            instance: 0,
        },
    );
    let bar = menus::create();
    let file = menus::create();
    menus::append(file, 0, 101, Some("&Ouvrir\tCtrl+O".into()));
    menus::append(bar, 0x10, file, Some("&Fichier".into())); // MF_POPUP
    menus::append(bar, 0, 102, Some("&Aide".into()));
    let class = w("WinluxTestMenuBar");
    let title = w("Menus");
    let p = unsafe {
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
            bar,
            0,
            std::ptr::null_mut(),
        )
    };
    assert!(p != 0);
    assert_eq!(gui::menu_bar_height(p), menus::BAR_HEIGHT);
    let mut r = Rect::default();
    unsafe { user32::get_client_rect(p, &mut r) };
    assert_eq!((r.width(), r.height()), (400, 300 - menus::BAR_HEIGHT));
    // Les enfants se placent sous la barre.
    let c = child(p, "STATIC", "x", 0, 7, (0, 0, 50, 20));
    let cr = gui::screen_rect(c).unwrap();
    assert_eq!((cr.left, cr.top), (100, 100 + menus::BAR_HEIGHT));
    // Titres de gauche à droite, rien sous la barre.
    assert_eq!(menus::bar_hit(p, 102, 105), Some(0));
    let second = (1..400).find(|&x| menus::bar_hit(p, 100 + x, 105) == Some(1));
    assert!(second.is_some(), "« Aide » placé après « Fichier »");
    assert_eq!(menus::bar_hit(p, 102, 100 + menus::BAR_HEIGHT + 1), None);
    // Commande directe dans la barre : WM_COMMAND.
    notes();
    menus::open_bar_menu(p, 1);
    let mut m: gui::Msg = unsafe { std::mem::zeroed() };
    while unsafe { user32::peek_message_w(&mut m, p, 0, 0, 1) } != 0 {
        unsafe { user32::dispatch_message_w(&m) };
    }
    assert!(notes().contains(&(102, 0)));
    // Sans menu, la zone client reprend toute la fenêtre.
    unsafe {
        let set: extern "win64" fn(usize, usize) -> i32 =
            std::mem::transmute(crate::dlls::lookup("user32", "SetMenu").unwrap());
        set(p, 0);
        user32::get_client_rect(p, &mut r);
    }
    assert_eq!(r.height(), 300);
    gui::destroy_window(p);
}

/// Mémoire DC + image de `w`×`h` remplie de blanc.
fn memory_dc(w: i32, h: i32) -> (usize, usize) {
    let dc = crate::dlls::gdi32::create_compatible_dc(0);
    let bmp = crate::dlls::gdi32::create_compatible_bitmap(dc, w, h);
    crate::dlls::gdi32::select_object(dc, bmp);
    gui::record(
        dc,
        gui::DrawCmd::FillRect {
            rect: Rect::from_size(0, 0, w, h),
            color: 0x00FF_FFFF,
        },
    );
    (dc, bmp)
}

fn dark_pixels(bmp: usize) -> usize {
    let (_, _, px, _) = gui::bitmap_pixels(bmp).unwrap();
    px.iter().filter(|&&p| p & 0xFF < 0x80).count()
}

#[test]
fn texte_dans_une_image_hors_ecran() {
    let spec = gui::FontSpec {
        height: -14,
        face: "Segoe UI".into(),
        ..gui::FontSpec::default()
    };
    if crate::dlls::usp10::spec_font(&spec).is_none() {
        return; // aucune police TrueType sur cette machine
    }
    let (dc, bmp) = memory_dc(80, 24);
    assert_eq!(dark_pixels(bmp), 0);
    gui::record(
        dc,
        gui::DrawCmd::Text {
            x: 2,
            y: 2,
            text: "Ln 1".into(),
            color: 0,
            bk: None,
            font: spec,
        },
    );
    assert!(dark_pixels(bmp) > 10, "glyphes posés dans l'image");
}

#[repr(C)]
struct TestTcItem {
    mask: u32,
    state: u32,
    state_mask: u32,
    text: *const u16,
    text_max: i32,
    image: i32,
    param: isize,
}

#[test]
fn wm_print_peint_barre_d_etat_et_onglets_dessines_par_le_parent() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let status = child(p, "msctls_statusbar32", "", 0, 30, (0, 0, 0, 0));
    let text = w("Ln : 1");
    sendw(status, 0x040B, 0, text.as_ptr() as isize); // SB_SETTEXTW
    sendw(status, 0x040B, 1 | 0x1000, 0x55); // SBT_OWNERDRAW
    let parts = [100i32, -1];
    sendw(status, 0x0404, 2, parts.as_ptr() as isize); // SB_SETPARTS
                                                       // SB_GETBORDERS remplit ses trois entiers : wxWidgets en tire la
                                                       // hauteur de la barre (#1053).
    let mut borders = [i32::MIN; 3];
    assert_eq!(sendw(status, 0x0407, 0, borders.as_mut_ptr() as isize), 1);
    assert_eq!(borders, [0, 2, 2]);
    DRAWN.lock().unwrap().clear();
    let (dc, bmp) = memory_dc(400, 26);
    // WM_PRINT (PRF_CLIENT | PRF_ERASEBKGND) : rendu dans le DC fourni.
    sendw(status, 0x0317, dc, 0xC);
    let (_, _, px, _) = gui::bitmap_pixels(bmp).unwrap();
    assert!(
        px.iter().any(|&c| c & 0x00FF_FFFF != 0x00FF_FFFF),
        "barre peinte"
    );
    assert!(
        DRAWN.lock().unwrap().contains(&(0, 1, 0x55)),
        "partie SBT_OWNERDRAW"
    );

    // Onglets TCS_OWNERDRAWFIXED : chaque onglet passe par WM_DRAWITEM.
    let tab = child(p, "SysTabControl32", "", 0x2000, 31, (0, 0, 300, 200));
    let label = w("new 1");
    let item = TestTcItem {
        mask: 0x9, // TCIF_TEXT | TCIF_PARAM
        state: 0,
        state_mask: 0,
        text: label.as_ptr(),
        text_max: 0,
        image: -1,
        param: 0x77,
    };
    assert_eq!(
        sendw(tab, 0x133E, 0, &item as *const TestTcItem as isize),
        0
    );
    assert_eq!(sendw(tab, 0x132C, 0, 0), 1, "TCM_GETROWCOUNT");
    sendw(tab, 0x1329, 0, (22 << 16) | 44); // TCM_SETITEMSIZE
    assert_eq!(sendw(tab, 0x1329, 0, (22 << 16) | 60), (22 << 16) | 44);
    let mut r = Rect::from_size(0, 0, 300, 200);
    sendw(tab, 0x1328, 0, &mut r as *mut Rect as isize); // TCM_ADJUSTRECT
    assert_eq!(r.top, 22 + 2, "zone d'affichage sous la rangée d'onglets");
    DRAWN.lock().unwrap().clear();
    let (dc2, _) = memory_dc(300, 200);
    sendw(tab, 0x0317, dc2, 0xC);
    assert!(DRAWN.lock().unwrap().contains(&(101, 0, 0x77)), "ODT_TAB");
    gui::destroy_window(p);
}

/// DeferWindowPos applique les positions à EndDeferWindowPos, et une taille
/// nulle est acceptée : wxWidgets place ainsi les contrôles de ses sizers.
#[test]
fn defer_window_pos_place_les_enfants() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let parent = parent_window();
    let a = child(parent, "STATIC", "a", 0, 1, (0, 0, 10, 10));
    let b = child(parent, "STATIC", "b", 0, 2, (0, 0, 10, 10));
    let hdwp = user32::begin_defer_window_pos(2);
    assert_ne!(hdwp, 0);
    assert_eq!(
        user32::defer_window_pos(hdwp, a, 0, 5, 6, 70, 12, 0x14),
        hdwp
    );
    assert_eq!(
        user32::defer_window_pos(hdwp, b, 0, 5, 30, 0, 0, 0x14),
        hdwp
    );
    // Rien ne bouge avant EndDeferWindowPos.
    assert_eq!(gui::window_rect(a).unwrap().width(), 10);
    assert_eq!(user32::end_defer_window_pos(hdwp), 1);
    let ra = gui::window_rect(a).unwrap();
    assert_eq!((ra.left, ra.top, ra.width(), ra.height()), (5, 6, 70, 12));
    let rb = gui::window_rect(b).unwrap();
    assert_eq!((rb.top, rb.width(), rb.height()), (30, 0, 0));
    assert_eq!(user32::end_defer_window_pos(hdwp), 0);
    unsafe { user32::destroy_window(parent) };
}

/// « À propos » de Notepad++ (#505) : un STATIC SS_OWNERDRAW est dessiné
/// par le parent (WM_DRAWITEM, ODT_STATIC) sans fond qui masquerait ses
/// voisins, et le texte d'un champ multiligne en lecture seule revient à la
/// ligne pour tenir dans sa largeur.
#[test]
fn statique_dessine_par_le_parent_et_licence_coupee() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let logo = child(p, "Static", "", 0x0D, 40, (0, 0, 192, 192));
    assert!(is_backdrop(logo), "derrière ses voisins");
    DRAWN.lock().unwrap().clear();
    let (dc, bmp) = memory_dc(192, 192);
    sendw(logo, 0x0317, dc, 0xC); // WM_PRINT
    assert!(DRAWN.lock().unwrap().contains(&(5, 0, 0)), "ODT_STATIC");
    let (_, _, px, _) = gui::bitmap_pixels(bmp).unwrap();
    assert!(
        px.iter().all(|&c| c == px[0]),
        "aucun fond peint par Winlux"
    );

    let font = 0;
    let text: Vec<char> = "un deux trois quatre\r\ncinq".chars().collect();
    let width = measure(font, "un deux trois").0;
    let rows = wrapped_rows(font, &text, width);
    let lines: Vec<&str> = rows.iter().map(|(_, s)| s.trim_end()).collect();
    assert_eq!(lines, ["un deux trois", "quatre", "cinq"]);
    assert_eq!(rows[1].0, 14, "« quatre » commence au 15e caractère");
    assert_eq!(rows[2].0, 22, "ligne suivant le \\r\\n");
    gui::destroy_window(p);
}

#[test]
fn tabulations_des_statiques_deviennent_des_espaces() {
    assert_eq!(expand_tabs("a\tb"), "a       b");
    assert_eq!(
        expand_tabs("Dossiers:\t1\nC:\\\tBureau"),
        "Dossiers:       1\nC:\\     Bureau"
    );
    assert_eq!(expand_tabs("sans"), "sans");
}

/// #795 : un RichEdit (RICHEDIT50W) se crée, répond aux messages RichEdit
/// et ne défile pas au-delà de la dernière page (journal de FileZilla).
#[test]
fn richedit_journal_qui_defile_jusqu_a_la_derniere_page() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    // ES_MULTILINE | ES_READONLY | WS_VSCROLL, 5 lignes de haut environ.
    let e = child(
        p,
        "RICHEDIT50W",
        "",
        0x4 | 0x800 | 0x0020_0000,
        11,
        (10, 10, 300, 90),
    );
    assert_eq!(kind_of(e), Some(Kind::Edit));
    for i in 0..30 {
        let n = title(e).chars().count();
        sendw(e, 0x00B1, n, n as isize); // fin
        let line = w(&format!("Status:\tligne {i}\n"));
        sendw(e, 0x00C2, 0, line.as_ptr() as isize);
    }
    // EM_GETTEXTLENGTHEX (GETTEXTLENGTHEX par défaut).
    let gtl = [0u32, 1200];
    let len = sendw(e, 0x045F, gtl.as_ptr() as usize, gtl.as_ptr() as isize);
    assert_eq!(len as usize, title(e).encode_utf16().count());
    // EM_EXSETSEL / EM_EXGETSEL.
    let range = [3i32, 9];
    sendw(e, 0x0437, 0, range.as_ptr() as isize);
    let mut got = [0i32; 2];
    sendw(e, 0x0434, 0, got.as_mut_ptr() as isize);
    assert_eq!(got, [3, 9]);
    // EM_GETTEXTRANGE.
    let mut buf = [0u16; 16];
    #[repr(C)]
    struct TextRange {
        min: i32,
        max: i32,
        text: *mut u16,
    }
    let tr = TextRange {
        min: 0,
        max: 6,
        text: buf.as_mut_ptr(),
    };
    let n = sendw(e, 0x044B, 0, &tr as *const TextRange as isize);
    assert_eq!(String::from_utf16_lossy(&buf[..n as usize]), "Status");
    // EM_SETEVENTMASK renvoie l'ancien masque.
    assert_eq!(sendw(e, 0x0445, 0, 0x1), 0);
    assert_eq!(sendw(e, 0x043B, 0, 0), 1);
    // ShowPosition de wxWidgets : EM_LINESCROLL jusqu'à la dernière ligne.
    let last = sendw(e, 0x00C9, title(e).chars().count(), 0);
    let first = sendw(e, 0x00CE, 0, 0);
    sendw(e, 0x00B6, 0, last - first);
    let top = sendw(e, 0x00CE, 0, 0);
    assert!(
        top > 0 && top < last,
        "dernière page pleine, pas une ligne seule : {top}/{last}"
    );
    // L'EDIT ordinaire garde son comportement (dernière ligne en haut).
    let plain = child(p, "EDIT", "", 0x4 | 0x0020_0000, 12, (10, 110, 300, 90));
    let t = w("a\nb\nc\nd\ne\nf\ng\nh\ni\nj");
    unsafe { user32::set_window_text_w(plain, t.as_ptr()) };
    sendw(plain, 0x00B6, 0, 100);
    assert_eq!(sendw(plain, 0x00CE, 0, 0), 9);
    unsafe {
        user32::destroy_window(e);
        user32::destroy_window(plain);
        user32::destroy_window(p);
    }
}

/// #802 : Ctrl+A dans un champ sélectionne tout ; la frappe suivante remplace.
#[test]
fn ctrl_a_selectionne_tout_le_champ() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let e = child(p, "EDIT", "22", WS_BORDER | 0x80, 13, (10, 10, 120, 24));
    gui::observe_key_message(&gui::Msg::new(e, gui::WM_KEYDOWN, 0x11, 0));
    sendw(e, gui::WM_KEYDOWN, 0x41, 0);
    gui::observe_key_message(&gui::Msg::new(e, gui::WM_KEYUP, 0x11, 0));
    let r = sendw(e, 0x00B0, 0, 0);
    assert_eq!((r & 0xFFFF, r >> 16), (0, 2));
    for ch in "2222".chars() {
        sendw(e, WM_CHAR, ch as usize, 0);
    }
    assert_eq!(text_of(e), "2222");
    unsafe {
        user32::destroy_window(e);
        user32::destroy_window(p);
    }
}

/// #865 : SysLink existe (wxHyperlinkCtrl), donne sa taille idéale et
/// signale un clic sur un lien par NM_CLICK avec l'URL.
#[test]
fn syslink_taille_et_clic() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let link = child(
        p,
        "SysLink",
        "Voir <A HREF=\"https://exemple.org/aide\">l'aide</A>",
        0x5001_0000,
        40,
        (0, 0, 300, 20),
    );
    assert_ne!(link, 0, "SysLink créé");
    let mut size = [0i32; 2];
    let h = sendw(link, 0x0701, 0, size.as_mut_ptr() as isize);
    assert!(
        size[0] > 20 && size[1] > 5 && h == size[1] as isize,
        "{size:?}"
    );
    LINK_URLS.lock().unwrap().clear();
    // Clic sur « Voir » : rien ; clic au bout du texte, dans le lien.
    sendw(link, WM_LBUTTONUP, 0, (5 << 16) | 2);
    assert!(LINK_URLS.lock().unwrap().is_empty());
    let x = (size[0] - 3) as isize;
    sendw(link, WM_LBUTTONUP, 0, (5 << 16) | x);
    assert_eq!(
        *LINK_URLS.lock().unwrap(),
        vec!["https://exemple.org/aide".to_string()]
    );
    gui::destroy_window(p);
}

#[test]
fn roundrect_prend_la_largeur_de_l_ellipse() {
    // RoundRect(…, 6, 6) sur un carré de 12 : coins de rayon 3 (#866). Avec
    // un rayon de 6, le pixel (1, 1) restait blanc et la case devenait ronde.
    use crate::dlls::gdi32;
    let (dc, bmp) = memory_dc(12, 12);
    let brush = gdi32::create_solid_brush(0);
    gdi32::select_object(dc, brush);
    let px = |x, y| unsafe { gdi32::get_pixel(dc, x, y) };
    unsafe { gdi32::round_rect(dc, 0, 0, 12, 12, 6, 6) };
    assert_eq!(px(1, 1), 0, "coin trop arrondi");
    assert_eq!(px(0, 0), 0x00FF_FFFF, "coin pas arrondi");
    assert_eq!(px(6, 0), 0);
    gdi32::delete_object(brush);
    gdi32::delete_object(bmp);
}

/// #952 : wxWidgets pose l'icône de ses boîtes (point d'interrogation,
/// avertissement) par STM_SETIMAGE sur un statique SS_ICON ; elle doit être
/// gardée et dessinée.
#[test]
fn statique_ss_icon_garde_et_dessine_son_icone() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let parent = parent_window();
    let st = child(parent, "STATIC", "", 0x03, 50, (10, 10, 32, 32));
    let icon = unsafe { crate::dlls::icons::load_icon_w(0, 32514 as *const u16) }; // IDI_QUESTION
    assert_ne!(icon, 0);
    unsafe {
        assert_eq!(user32::send_message_w(st, 0x0172, 1, icon as isize), 0);
        assert_eq!(user32::send_message_w(st, 0x0173, 1, 0), icon as isize);
        assert_eq!(user32::send_message_w(st, 0x0171, 0, 0), icon as isize);
    }
    // Dessin dans un DC mémoire blanc : des pixels de l'icône apparaissent.
    let dc = gui::create_dc(0);
    let bmp = gui::create_compatible_bitmap(32, 32).unwrap();
    gui::select_object(dc, bmp);
    gui::record(
        dc,
        gui::DrawCmd::FillRect {
            rect: Rect::from_size(0, 0, 32, 32),
            color: 0x00FF_FFFF,
        },
    );
    let p = Painter { hdc: dc, font: 0 };
    paint_static_image(st, &p, Rect::from_size(0, 0, 32, 32), true, 0);
    let snap = gui::bitmap_snapshot(dc, Rect::from_size(0, 0, 32, 32)).unwrap();
    let drawn = snap
        .pixels
        .iter()
        .filter(|&&px| px & 0x00FF_FFFF != 0x00FF_FFFF)
        .count();
    assert!(drawn > 20, "icône non dessinée ({drawn} pixels)");
    gui::select_object(dc, gui::DEFAULT_BITMAP);
    gui::delete_object(bmp);
    gui::free_dc(dc);
    unsafe {
        user32::destroy_window(parent);
    }
    assert_eq!(static_image(st), 0);
}

#[test]
fn colonnes_ajustees_jamais_negatives_ni_chevauchees() {
    // #1055 : wxWidgets insère ses colonnes avec cx = -2 (LVSCW_AUTOSIZE_USEHEADER)
    // puis les règle par LVM_SETCOLUMNWIDTH -1 / -2.
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let lv = child(p, "SysListView32", "", 0x1, 52, (0, 0, 400, 200));
    for (i, name) in ["Nom du projet", "Date"].iter().enumerate() {
        let t = w(name);
        let mut col = LvColumnW {
            mask: 0x2 | 0x4,
            fmt: 0,
            cx: -2,
            text: t.as_ptr(),
            text_max: 0,
            sub_item: 0,
        };
        sendw(lv, 0x1061, i, &mut col as *mut LvColumnW as isize);
    }
    let w0 = sendw(lv, 0x101D, 0, 0);
    let w1 = sendw(lv, 0x101D, 1, 0);
    assert!(w0 > 0 && w1 > 0, "largeurs {w0} {w1}");
    assert!(w0 > w1, "l'en-tête plus long donne la colonne plus large");
    let t = w("un nom de projet vraiment très long");
    let mut it = LvItemW {
        mask: 0x1,
        item: 0,
        sub_item: 0,
        state: 0,
        state_mask: 0,
        text: t.as_ptr(),
        text_max: 0,
        image: 0,
        param: 0,
    };
    sendw(lv, 0x104D, 0, &mut it as *mut LvItemW as isize);
    sendw(lv, 0x101E, 0, -1);
    assert!(sendw(lv, 0x101D, 0, 0) > w0, "ajustée au texte de la ligne");
    // Comme la macro ListView_SetColumnWidth : MAKELPARAM(-2, 0) = 0xFFFE.
    sendw(lv, 0x101E, 1, 0xFFFE);
    assert_eq!(
        sendw(lv, 0x101D, 1, 0),
        w1,
        "colonne vide : ajustée à l'en-tête"
    );
}

#[test]
fn glisser_depuis_une_liste_ou_une_arborescence() {
    // #1138 : FileZilla (wxWidgets) lance DoDragDrop sur LVN_BEGINDRAG ;
    // sans cette notification, tirer un fichier ne faisait que le
    // sélectionner.
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let lv = child(p, "SysListView32", "", 0x1, 54, (0, 0, 300, 200));
    let t = w("Nom");
    let mut col = LvColumnW {
        mask: 0x2 | 0x4,
        fmt: 0,
        cx: 200,
        text: t.as_ptr(),
        text_max: 0,
        sub_item: 0,
    };
    sendw(lv, 0x1061, 0, &mut col as *mut LvColumnW as isize);
    let name = w("rapport.txt");
    let mut it = LvItemW {
        mask: 0x1,
        item: 0,
        sub_item: 0,
        state: 0,
        state_mask: 0,
        text: name.as_ptr(),
        text_max: 0,
        image: 0,
        param: 0,
    };
    sendw(lv, 0x104D, 0, &mut it as *mut LvItemW as isize);
    let y = (0..150)
        .find(|&y| {
            let mut hit = [10i32, y, 0, -1, 0, 0];
            sendw(lv, 0x1012, 0, hit.as_mut_ptr() as isize) == 0
        })
        .expect("ligne 0 visible");
    let at = ((y as isize) << 16) | 10;
    // Pointeur déplacé de 20 px bouton tenu : LVN_BEGINDRAG, pas de NM_CLICK.
    notify_codes();
    unsafe { user32::post_message_w(lv, 0x0200, 1, ((y as isize + 20) << 16) | 30) };
    sendw(lv, WM_LBUTTONDOWN, 1, at);
    let codes = notify_codes();
    assert!(codes.contains(&-109), "LVN_BEGINDRAG : {codes:?}");
    assert!(!codes.contains(&-2), "pas de NM_CLICK : {codes:?}");
    // Simple clic : NM_CLICK, pas de glisser.
    sendw(lv, WM_LBUTTONDOWN, 1, at);
    let codes = notify_codes();
    assert!(codes.contains(&-2) && !codes.contains(&-109), "{codes:?}");
    // Déplacement sous le seuil (SM_CXDRAG) : toujours un clic.
    unsafe { user32::post_message_w(lv, 0x0200, 1, ((y as isize) << 16) | 11) };
    sendw(lv, WM_LBUTTONDOWN, 1, at);
    let codes = notify_codes();
    assert!(codes.contains(&-2) && !codes.contains(&-109), "{codes:?}");
    gui::destroy_window(p);

    // Arborescence : TVN_BEGINDRAGW.
    let p = parent_window();
    let tv = child(p, "SysTreeView32", "", 0x7, 55, (0, 0, 200, 200));
    let h = tv_insert(tv, 0xFFFF_FFFF_FFFF_0000, "Documents", 1);
    let mut r = [0i32; 4];
    unsafe { *(r.as_mut_ptr() as *mut usize) = h };
    assert_eq!(sendw(tv, 0x1104, 1, r.as_mut_ptr() as isize), 1);
    let (x, y) = (100, (r[1] + r[3]) / 2);
    notify_codes();
    unsafe { user32::post_message_w(tv, 0x0200, 1, ((y as isize + 30) << 16) | x as isize) };
    sendw(tv, WM_LBUTTONDOWN, 1, ((y as isize) << 16) | x as isize);
    let codes = notify_codes();
    assert!(codes.contains(&-456), "TVN_BEGINDRAGW : {codes:?}");
    gui::destroy_window(p);
}

#[test]
fn clic_droit_menu_contextuel_et_renommage_dans_une_liste() {
    // #1139 : FileZilla attend NM_RCLICK puis WM_CONTEXTMENU sur un clic
    // droit, et renomme par LVM_EDITLABEL (LVN_BEGIN/ENDLABELEDIT).
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let lv = child(p, "SysListView32", "", 0x1 | 0x200, 56, (0, 0, 300, 200));
    let t = w("Nom");
    let mut col = LvColumnW {
        mask: 0x2 | 0x4,
        fmt: 0,
        cx: 200,
        text: t.as_ptr(),
        text_max: 0,
        sub_item: 0,
    };
    sendw(lv, 0x1061, 0, &mut col as *mut LvColumnW as isize);
    for (i, n) in ["essai.txt", "retour.txt"].iter().enumerate() {
        let name = w(n);
        let mut it = LvItemW {
            mask: 0x1,
            item: i as i32,
            sub_item: 0,
            state: 0,
            state_mask: 0,
            text: name.as_ptr(),
            text_max: 0,
            image: 0,
            param: 0,
        };
        sendw(lv, 0x104D, 0, &mut it as *mut LvItemW as isize);
    }
    let y = (0..150)
        .find(|&y| {
            let mut hit = [10i32, y, 0, -1, 0, 0];
            sendw(lv, 0x1012, 0, hit.as_mut_ptr() as isize) == 1
        })
        .expect("ligne 1 visible");
    notify_codes();
    CONTEXT.lock().unwrap().clear();
    let at = ((y as isize) << 16) | 10;
    sendw(lv, 0x0204, 2, at);
    sendw(lv, 0x0205, 0, at);
    let codes = notify_codes();
    assert!(codes.contains(&-5), "NM_RCLICK : {codes:?}");
    // LVM_GETITEMSTATE(1, LVIS_SELECTED)
    assert_eq!(sendw(lv, 0x102C, 1, 0x2), 0x2, "ligne visée sélectionnée");
    let menus = std::mem::take(&mut *CONTEXT.lock().unwrap());
    assert_eq!(menus.len(), 1, "un seul WM_CONTEXTMENU : {menus:?}");
    assert_eq!(menus[0].0, lv);
    let origin = gui::screen_rect(lv).unwrap();
    assert_eq!((menus[0].1 >> 16) as i16 as i32, y + origin.top);
    // LVM_GETITEMRECT (LVIR_BOUNDS) : où 7-Zip ouvre le menu au clavier.
    let mut r = [0i32; 4];
    assert_eq!(sendw(lv, 0x100E, 1, r.as_mut_ptr() as isize), 1);
    assert!(r[1] <= y && y < r[3] && r[2] > r[0], "{r:?}");
    // Touche Applications : WM_CONTEXTMENU au clavier (lParam = -1),
    // remonté au parent par DefWindowProc.
    sendw(lv, 0x0101, 0x5D, 0);
    assert_eq!(*CONTEXT.lock().unwrap(), vec![(lv, -1)]);
    // Renommage en place.
    let edit = sendw(lv, 0x1076, 1, 0) as usize;
    assert_ne!(edit, 0);
    assert_eq!(sendw(lv, 0x1018, 0, 0) as usize, edit);
    let nouveau = w("nouveau.txt");
    sendw(edit, WM_SETTEXT, 0, nouveau.as_ptr() as isize);
    sendw(lv, 0x10F0, 1, 0);
    let codes = notify_codes();
    assert!(codes.contains(&-175) && codes.contains(&-176), "{codes:?}");
    assert_eq!(sendw(lv, 0x1018, 0, 0), 0, "champ fermé");
    let mut buf = [0u16; 64];
    let mut it = LvItemW {
        mask: 0x1,
        item: 1,
        sub_item: 0,
        state: 0,
        state_mask: 0,
        text: buf.as_mut_ptr() as *const u16,
        text_max: 64,
        image: 0,
        param: 0,
    };
    let n = sendw(lv, 0x1073, 1, &mut it as *mut LvItemW as isize) as usize;
    assert_eq!(String::from_utf16_lossy(&buf[..n]), "nouveau.txt");
    gui::destroy_window(p);
}

#[test]
fn cases_a_cocher_des_lignes_d_une_liste() {
    // #1153 : la boîte de récupération d'Audacity active LVS_EX_CHECKBOXES
    // et lit l'image d'état (1 décochée, 2 cochée) de chaque projet.
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let lv = child(p, "SysListView32", "", 0x1, 57, (0, 0, 300, 200));
    let name = w("projet");
    let mut it = LvItemW {
        mask: 0x1,
        item: 0,
        sub_item: 0,
        state: 0,
        state_mask: 0,
        text: name.as_ptr(),
        text_max: 0,
        image: 0,
        param: 0,
    };
    sendw(lv, 0x104D, 0, &mut it as *mut LvItemW as isize);
    assert_eq!(sendw(lv, 0x1036, 0x4, 0x4), 0);
    assert_eq!(sendw(lv, 0x1037, 0, 0), 0x4);
    assert_eq!(sendw(lv, 0x102C, 0, 0xF000), 0x1000, "décochée");
    let y = (0..150)
        .find(|&y| {
            let mut hit = [10i32, y, 0, -1, 0, 0];
            sendw(lv, 0x1012, 0, hit.as_mut_ptr() as isize) == 0
        })
        .expect("ligne 0 visible");
    notify_codes();
    sendw(lv, WM_LBUTTONDOWN, 1, ((y as isize) << 16) | 10);
    assert_eq!(sendw(lv, 0x102C, 0, 0xF000), 0x2000, "cochée au clic");
    assert!(notify_codes().contains(&-101), "LVN_ITEMCHANGED");
    sendw(lv, WM_KEYDOWN, 0x20, 0);
    assert_eq!(sendw(lv, 0x102C, 0, 0xF000), 0x1000, "Espace décoche");
    gui::destroy_window(p);
}

#[test]
fn superclasse_retrouve_le_controle_d_origine() {
    // #1129 : TThemePageControl de WinSCP reprend la procédure de
    // SysTabControl32 ; la conversion x86 doit la reconnaître.
    let tab = super::builtin_class("SysTabControl32").unwrap();
    assert_eq!(super::builtin_class_of_proc(tab), Some("systabcontrol32"));
    let lv = super::builtin_class("SysListView32").unwrap();
    assert_eq!(super::builtin_class_of_proc(lv), Some("syslistview32"));
    assert_eq!(super::builtin_class_of_proc(0), None);
    let button = super::builtin_class("Button").unwrap();
    assert_eq!(super::builtin_class_of_proc(button), None);
}

#[test]
fn wm_setcursor_applique_le_curseur_de_classe() {
    // #1253 : la fenêtre sous le pointeur reçoit WM_SETCURSOR ; DefWindowProc
    // consulte d'abord le parent puis applique le curseur de classe.
    const WM_SETCURSOR: u32 = 0x0020;
    let lp = (0x0200isize << 16) | 1; // HTCLIENT, WM_MOUSEMOVE
    let parent = parent_window();
    let edit = child(parent, "Edit", "texte", 0, 1, (0, 0, 100, 20));
    let button = child(parent, "Button", "OK", 0, 2, (0, 30, 100, 20));
    let current = || gui::CURSOR_HANDLE.load(std::sync::atomic::Ordering::Relaxed);
    let r = unsafe { user32::send_message_w(edit, WM_SETCURSOR, edit, lp) };
    assert_eq!(r, 1);
    assert_eq!(
        current(),
        gui::load_cursor(0, 32513),
        "barre en I sur une zone de texte"
    );
    let r = unsafe { user32::send_message_w(button, WM_SETCURSOR, button, lp) };
    assert_eq!(r, 1);
    assert_eq!(
        current(),
        gui::load_cursor(0, 32512),
        "flèche sur un bouton"
    );
    // Classe enregistrée sans curseur : rien n'est choisi, l'appli le fait.
    gui::set_class_cursor("WinluxTestSansCurseur", 0);
    gui::register_class(
        "WinluxTestSansCurseur",
        gui::ClassInfo {
            wnd_proc: parent_proc as *const () as usize,
            background: 0,
            instance: 0,
        },
    );
    let class = w("WinluxTestSansCurseur");
    let title = w("Sans curseur");
    let bare = unsafe {
        user32::create_window_ex_w(
            0,
            class.as_ptr(),
            title.as_ptr(),
            0x00CF_0000,
            0,
            0,
            100,
            100,
            0,
            0,
            0,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(bare, 0);
    assert_eq!(
        unsafe { user32::send_message_w(bare, WM_SETCURSOR, bare, lp) },
        0
    );
}

static ENUMERES: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

unsafe extern "win64" fn enum_collecte(hwnd: usize, limite: isize) -> i32 {
    let mut v = ENUMERES.lock().unwrap();
    v.push(hwnd);
    i32::from(v.len() < limite as usize)
}

#[test]
fn enum_child_windows_parcourt_tous_les_descendants() {
    // Sumatra PDF invalide chaque enfant par EnumChildWindows en passant à
    // l'onglet d'accueil ; le bouchon ne rappelait jamais.
    let parent = parent_window();
    let a = child(parent, "Static", "a", 0, 1, (0, 0, 10, 10));
    let b = child(parent, "Static", "b", 0, 2, (0, 20, 10, 10));
    let petit = child(a, "Static", "petit", 0, 3, (0, 0, 5, 5));
    let cb = enum_collecte as *const () as usize;
    ENUMERES.lock().unwrap().clear();
    unsafe { user32::enum_child_windows(parent, cb, 100) };
    let mut vus = ENUMERES.lock().unwrap().clone();
    vus.sort_unstable();
    let mut attendus = vec![a, b, petit];
    attendus.sort_unstable();
    assert_eq!(vus, attendus);
    // FALSE arrête l'énumération.
    ENUMERES.lock().unwrap().clear();
    unsafe { user32::enum_child_windows(parent, cb, 1) };
    assert_eq!(ENUMERES.lock().unwrap().len(), 1);
}

/// LVM_FINDITEM avec LVFI_PARAM seul : psz n'est pas lu (WinSCP x86 y
/// laisse 1, #1325).
#[test]
fn liste_recherche_par_parametre_sans_texte() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let lv = child(p, "SysListView32", "", 0x1, 52, (0, 0, 300, 200));
    for i in 0..3 {
        let t = w("fichier");
        let mut it = LvItemW {
            mask: 0x1 | 0x4,
            item: i,
            sub_item: 0,
            state: 0,
            state_mask: 0,
            text: t.as_ptr(),
            text_max: 0,
            image: 0,
            param: 500 + i as isize,
        };
        assert_eq!(
            sendw(lv, 0x104D, 0, &mut it as *mut LvItemW as isize),
            i as isize
        );
    }
    // LVFINDINFOW 64 bits : flags, psz, lParam, pt, vkDirection.
    let mut find = [0usize; 5];
    find[0] = 0x1; // LVFI_PARAM
    find[1] = 1; // psz quelconque
    find[2] = 502;
    assert_eq!(sendw(lv, 0x1053, usize::MAX, find.as_mut_ptr() as isize), 2);
    find[2] = 9;
    assert_eq!(
        sendw(lv, 0x1053, usize::MAX, find.as_mut_ptr() as isize),
        -1
    );
    // GetWindowPlacement d'une fenêtre enfant : relatif à son parent (#1326).
    let tb = child(p, "Static", "", 0, 53, (0, 40, 120, 55));
    let mut wp = user32::WindowPlacement::default();
    assert_eq!(unsafe { user32::get_window_placement(tb, &mut wp) }, 1);
    assert_eq!(
        (
            wp.normal.left,
            wp.normal.top,
            wp.normal.right,
            wp.normal.bottom
        ),
        (0, 40, 120, 95)
    );
    unsafe {
        user32::destroy_window(p);
    }
}

/// Source d'un flux EM_STREAMIN / puits d'EM_STREAMOUT pour les tests.
struct Flux {
    data: Vec<u8>,
    pos: usize,
}

unsafe extern "win64" fn flux_lire(cookie: usize, buf: *mut u8, cb: i32, pcb: *mut i32) -> u32 {
    let f = &mut *(cookie as *mut Flux);
    // Morceaux de 7 octets : le contrôle doit rappeler jusqu'à la fin.
    let n = (f.data.len() - f.pos).min(cb as usize).min(7);
    std::ptr::copy_nonoverlapping(f.data.as_ptr().add(f.pos), buf, n);
    f.pos += n;
    *pcb = n as i32;
    0
}

unsafe extern "win64" fn flux_ecrire(cookie: usize, buf: *mut u8, cb: i32, pcb: *mut i32) -> u32 {
    let f = &mut *(cookie as *mut Flux);
    f.data
        .extend_from_slice(std::slice::from_raw_parts(buf, cb as usize));
    *pcb = cb;
    0
}

/// EDITSTREAM x64 telle que richedit.h la déclare sous pack(4) : dwCookie à 0,
/// dwError à 8, pfnCallback à 12, 20 octets (#1454). Les octets qui suivent
/// restent nuls : relire le rappel à 16 donnerait un pointeur faux.
fn edit_stream(f: &mut Flux, callback: usize) -> [u8; 24] {
    let mut es = [0u8; 24];
    es[..8].copy_from_slice(&(f as *mut Flux as usize).to_le_bytes());
    es[12..20].copy_from_slice(&callback.to_le_bytes());
    es
}

/// #1433 : RichTextBox.Text de WinForms passe par EM_STREAMIN
/// (SF_TEXT | SF_UNICODE) ; la zone de détails de Paint.NET restait vide.
#[test]
fn richedit_flux_texte_et_rtf() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let p = parent_window();
    let e = child(p, "RICHEDIT50W", "", 0x4 | 0x800, 12, (10, 10, 300, 90));
    let texte = "Exception:\r\n\r\nSystem.EntryPointNotFoundException: é";
    let mut src = Flux {
        data: texte.encode_utf16().flat_map(|u| u.to_le_bytes()).collect(),
        pos: 0,
    };
    // EDITSTREAM (pack(4)) : dwCookie, dwError, pfnCallback.
    let mut es = edit_stream(&mut src, flux_lire as usize);
    let n = sendw(e, 0x0449, 0x1 | 0x10, es.as_mut_ptr() as isize);
    assert_eq!(n as usize, texte.chars().count());
    assert_eq!(title(e), texte);
    // EM_STREAMOUT rend le même texte.
    let mut dst = Flux {
        data: Vec::new(),
        pos: 0,
    };
    let mut es = edit_stream(&mut dst, flux_ecrire as usize);
    sendw(e, 0x044A, 0x1 | 0x10, es.as_mut_ptr() as isize);
    let units: Vec<u16> = dst
        .data
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    assert_eq!(String::from_utf16_lossy(&units), texte);
    // RTF : tables ignorées, \par, \'e9 et \u.
    let rtf = br"{\rtf1\ansi{\fonttbl{\f0 Arial;}}{\colortbl;\red0\green0\blue0;}\f0 Caf\'e9\par Pile \{1\}\tab \u8364?}";
    let mut src = Flux {
        data: rtf.to_vec(),
        pos: 0,
    };
    let mut es = edit_stream(&mut src, flux_lire as usize);
    sendw(e, 0x0449, 0x2, es.as_mut_ptr() as isize);
    assert_eq!(title(e), "Café\r\nPile {1}\t€");
    gui::destroy_window(p);
}

/// #1803 : les délais d'une infobulle ne valent jamais 0. WinForms en fait
/// l'intervalle d'un minuteur (Paint.NET, Tab dans la barre d'outils).
#[test]
fn infobulle_delais_par_defaut_et_reglages() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    const TTM_SETDELAYTIME: u32 = 0x0403;
    const TTM_GETDELAYTIME: u32 = 0x0415;
    let p = parent_window();
    let tip = child(p, "tooltips_class32", "", 0, 0, (0, 0, 0, 0));
    assert_ne!(tip, 0);
    let clic = user32::get_double_click_time() as isize;
    let delais = |tip| [1, 2, 3].map(|w| sendw(tip, TTM_GETDELAYTIME, w, 0));
    // Réapparition, disparition, apparition.
    assert_eq!(delais(tip), [clic / 5, clic * 10, clic]);
    sendw(tip, TTM_SETDELAYTIME, 0, 1000);
    assert_eq!(delais(tip), [200, 10000, 1000]);
    sendw(tip, TTM_SETDELAYTIME, 3, 250);
    assert_eq!(sendw(tip, TTM_GETDELAYTIME, 3, 0), 250);
    sendw(tip, TTM_SETDELAYTIME, 3, 0xFFFF); // -1 : valeur par défaut
    assert_eq!(sendw(tip, TTM_GETDELAYTIME, 3, 0), clic);
    // Largeur maximale : -1 au départ, l'ancienne valeur rendue au réglage.
    assert_eq!(sendw(tip, 0x0419, 0, 0), -1);
    assert_eq!(sendw(tip, 0x0418, 0, 300), -1);
    assert_eq!(sendw(tip, 0x0419, 0, 0), 300);
    // Couleurs : système par défaut, puis celle fixée.
    assert_eq!(sendw(tip, 0x0416, 0, 0) as u32, user32::get_sys_color(24));
    sendw(tip, 0x0413, 0x00FF_00FF, 0);
    assert_eq!(sendw(tip, 0x0416, 0, 0), 0x00FF_00FF);
}
