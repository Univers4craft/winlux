use super::*;
use std::sync::Mutex as StdMutex;

static EVENTS: StdMutex<Vec<(u32, usize)>> = StdMutex::new(Vec::new());

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// TASKDIALOGCONFIG compactée (160 octets) construite à la main.
struct Config {
    bytes: Vec<u8>,
    _keep: Vec<Vec<u16>>,
    _buttons: Vec<u8>,
    _radios: Vec<u8>,
}

fn put<T: Copy>(b: &mut [u8], offset: usize, v: T) {
    unsafe { std::ptr::write_unaligned(b.as_mut_ptr().add(offset) as *mut T, v) }
}

fn config(
    flags: u32,
    common: u32,
    buttons: &[(i32, &str)],
    radios: &[(i32, &str)],
    callback: usize,
) -> Config {
    let mut keep = Vec::new();
    let mut text = |s: &str| {
        let v = w(s);
        let p = v.as_ptr() as usize;
        keep.push(v);
        p
    };
    let mut bytes = vec![0u8; 160];
    put(&mut bytes, 0, 160u32);
    put(&mut bytes, 20, flags);
    put(&mut bytes, 24, common);
    put(&mut bytes, 28, text("Programme"));
    put(&mut bytes, 36, 0xFFFFusize); // TD_WARNING_ICON
    put(&mut bytes, 44, text("Enregistrer les modifications ?"));
    put(
        &mut bytes,
        52,
        text("Le <a href=\"x\">document</a> a changé."),
    );
    let mut list = |items: &[(i32, &str)]| {
        let mut out = vec![0u8; items.len() * 12];
        for (i, (id, label)) in items.iter().enumerate() {
            put(&mut out, i * 12, *id);
            put(&mut out, i * 12 + 4, text(label));
        }
        out
    };
    let b = list(buttons);
    let r = list(radios);
    put(&mut bytes, 60, buttons.len() as u32);
    put(
        &mut bytes,
        64,
        if buttons.is_empty() {
            0
        } else {
            b.as_ptr() as usize
        },
    );
    put(&mut bytes, 76, radios.len() as u32);
    put(
        &mut bytes,
        80,
        if radios.is_empty() {
            0
        } else {
            r.as_ptr() as usize
        },
    );
    put(&mut bytes, 92, text("Ne plus demander"));
    put(&mut bytes, 100, text("Détails techniques"));
    put(&mut bytes, 132, text("Pied de page"));
    put(&mut bytes, 140, callback);
    put(&mut bytes, 148, 77isize);
    Config {
        bytes,
        _keep: keep,
        _buttons: b,
        _radios: r,
    }
}

#[test]
fn liens_retires_et_boutons_communs() {
    assert_eq!(
        strip_links("Voir <a href=\"https://x\">l'aide</a>."),
        "Voir l'aide."
    );
    assert_eq!(strip_links("a < b <b>"), "a < b <b>");
    let ids: Vec<i32> = common_buttons(0x3F).iter().map(|b| b.0).collect();
    assert_eq!(ids, vec![1, 6, 7, 4, 2, 8], "ordre de Windows");
}

#[test]
fn placement_et_details_depliables() {
    let spec = Spec {
        main: "Titre".into(),
        content: "Texte".into(),
        expanded: "Une ligne\nDeux lignes\nTrois".into(),
        verification: "Ne plus afficher".into(),
        icon: Some(Icon::Information),
        buttons: common_buttons(0x06),
        ..Spec::default()
    };
    let (closed, w, h) = plan(&spec, false);
    let (open, w2, h2) = plan(&spec, true);
    assert_eq!(w, w2);
    assert!(h2 > h, "les détails dépliés agrandissent la boîte");
    assert!(!closed.iter().any(|(e, _)| *e == Elem::Expanded));
    assert!(open.iter().any(|(e, _)| *e == Elem::Expanded));
    let find = |e: Elem| closed.iter().find(|(x, _)| *x == e).unwrap().1;
    assert_eq!(
        find(Elem::Button(1)).right,
        w - MARGIN,
        "boutons alignés à droite"
    );
    assert!(find(Elem::Button(0)).right < find(Elem::Button(1)).left);
    assert!(
        find(Elem::Main).left > find(Elem::Icon).right,
        "texte à droite de l'icône"
    );
    assert!(find(Elem::Verification).top > find(Elem::Button(0)).bottom);
    // Liens de commande : un par ligne, pleine largeur, au-dessus des boutons.
    let links = Spec {
        flags: TDF_USE_COMMAND_LINKS,
        buttons: vec![
            (100, "Installer\nRecommandé".into()),
            (101, "Annuler l'installation".into()),
            (2, "Annuler".into()),
        ],
        custom: 2,
        ..Spec::default()
    };
    let (rects, w, _) = plan(&links, false);
    let r = |e: Elem| rects.iter().find(|(x, _)| *x == e).unwrap().1;
    assert!(r(Elem::Button(0)).right - r(Elem::Button(0)).left > w / 2);
    assert!(r(Elem::Button(1)).top > r(Elem::Button(0)).bottom);
    assert!(r(Elem::Button(2)).top > r(Elem::Button(1)).bottom);
}

unsafe extern "win64" fn callback(
    hwnd: usize,
    code: u32,
    wp: usize,
    _lp: isize,
    data: isize,
) -> i32 {
    EVENTS.lock().unwrap().push((code, wp));
    if data != 77 {
        EVENTS.lock().unwrap().push((999, data as usize));
    }
    match code {
        TDN_CREATED => {
            user32::send_message_w(hwnd, TDM_CLICK_RADIO_BUTTON, 11, 0);
            user32::send_message_w(hwnd, TDM_CLICK_VERIFICATION, 1, 0);
            let text = w("Nouveau texte");
            let set = user32::send_message_w(hwnd, TDM_SET_ELEMENT_TEXT, 0, text.as_ptr() as isize);
            EVENTS.lock().unwrap().push((1000, set as usize));
            user32::send_message_w(hwnd, TDM_ENABLE_BUTTON, 101, 0);
            user32::send_message_w(hwnd, TDM_CLICK_BUTTON, 101, 0);
            user32::send_message_w(hwnd, TDM_CLICK_BUTTON, 102, 0);
            S_OK
        }
        // Le premier bouton refuse la fermeture (S_FALSE).
        TDN_BUTTON_CLICKED if wp == 101 => S_FALSE,
        _ => S_OK,
    }
}

#[test]
fn procedure_de_rappel_et_messages() {
    let _g = dialogs::tests::SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    EVENTS.lock().unwrap().clear();
    let c = config(
        TDF_ENABLE_HYPERLINKS | TDF_ALLOW_DIALOG_CANCELLATION | TDF_SHOW_PROGRESS_BAR,
        0x08,
        &[(101, "Enregistrer"), (102, "Ne pas enregistrer")],
        &[(10, "Format A"), (11, "Format B")],
        callback as *const () as usize,
    );
    let (mut b, mut r, mut v) = (0, 0, 0);
    let hr = unsafe { task_dialog_indirect(c.bytes.as_ptr(), &mut b, &mut r, &mut v) };
    assert_eq!(hr, S_OK);
    assert_eq!(b, 102, "le second clic ferme la boîte");
    assert_eq!(r, 11, "radio choisie par TDM_CLICK_RADIO_BUTTON");
    assert_eq!(v, 1, "case cochée par TDM_CLICK_VERIFICATION");
    let events = EVENTS.lock().unwrap().clone();
    assert!(
        !events.iter().any(|e| e.0 == 999),
        "lpCallbackData transmis"
    );
    assert!(
        events.contains(&(1000, 1)),
        "TDM_SET_ELEMENT_TEXT : {events:?}"
    );
    let codes: Vec<u32> = events.iter().map(|e| e.0).filter(|&c| c < 999).collect();
    assert_eq!(&codes[..2], &[TDN_DIALOG_CONSTRUCTED, TDN_CREATED]);
    assert!(codes.contains(&TDN_RADIO_BUTTON_CLICKED));
    assert!(codes.contains(&TDN_VERIFICATION_CLICKED));
    assert_eq!(
        codes.iter().filter(|&&c| c == TDN_BUTTON_CLICKED).count(),
        2
    );
    assert_eq!(codes.last(), Some(&TDN_DESTROYED));
    assert!(lock(boxes()).is_empty(), "état libéré à la destruction");
}

#[test]
fn bouton_par_defaut_sans_affichage_et_erreurs() {
    let _g = dialogs::tests::SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut c = config(0, 0x06, &[], &[], 0);
    put(&mut c.bytes, 72, 7i32); // nDefaultButton = IDNO
    let mut b = 0;
    let hr = unsafe {
        task_dialog_indirect(
            c.bytes.as_ptr(),
            &mut b,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(hr, S_OK);
    if !gui::backend_is_interactive() {
        assert_eq!(b, 7, "sans affichage, le bouton par défaut");
    }
    put(&mut c.bytes, 0, 96u32);
    assert_eq!(
        unsafe {
            task_dialog_indirect(
                c.bytes.as_ptr(),
                &mut b,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        },
        E_INVALIDARG
    );
    assert_eq!(
        unsafe {
            task_dialog_indirect(
                std::ptr::null(),
                &mut b,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        },
        E_INVALIDARG
    );
    // TaskDialog simple : Oui/Non, Oui par défaut.
    let (t, m) = (w("Titre"), w("Question ?"));
    let hr = unsafe {
        task_dialog(
            0,
            0,
            t.as_ptr(),
            m.as_ptr(),
            std::ptr::null(),
            0x06,
            0xFFFD as *const u16,
            &mut b,
        )
    };
    assert_eq!(hr, S_OK);
    if !gui::backend_is_interactive() {
        assert_eq!(b, 6);
    }
}
