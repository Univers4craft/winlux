use super::*;
use crate::dlls::ole32_storage as stg;
use std::sync::atomic::{AtomicU32, Ordering};

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

const CLSID_DOC: Guid = Guid {
    data1: 0x1109_0001,
    data2: 0x0f11,
    data3: 0x4e11,
    data4: [9, 8, 7, 6, 5, 4, 3, 2],
};

#[test]
fn classe_par_extension() {
    let reg = |path: &str, _: &str| -> Option<String> {
        match path {
            "hkcr\\.essai" => Some("Winlux.Essai.1".into()),
            "hkcr\\winlux.essai.1\\clsid" => Some("{11090001-0F11-4E11-0908-070605040302}".into()),
            _ => None,
        }
    };
    assert_eq!(
        class_of_extension("C:\\docs\\note.ESSAI", reg),
        Some(CLSID_DOC)
    );
    assert_eq!(class_of_extension("C:\\docs\\note.autre", reg), None);
    assert_eq!(class_of_extension("C:\\docs.essai\\sans", reg), None);
}

static CHARGE: Mutex<Vec<u8>> = Mutex::new(Vec::new());
#[repr(C)]
struct Objet {
    vtbl: *const usize,
    refs: AtomicU32,
}
unsafe extern "win64" fn o_qi(this: usize, _iid: *const Guid, out: *mut usize) -> i32 {
    (*(this as *const Objet))
        .refs
        .fetch_add(1, Ordering::AcqRel);
    *out = this;
    S_OK
}
unsafe extern "win64" fn o_add_ref(this: usize) -> u32 {
    (*(this as *const Objet))
        .refs
        .fetch_add(1, Ordering::AcqRel)
        + 1
}
unsafe extern "win64" fn o_release(this: usize) -> u32 {
    let n = (*(this as *const Objet))
        .refs
        .fetch_sub(1, Ordering::AcqRel)
        - 1;
    if n == 0 {
        drop(Box::from_raw(this as *mut Objet));
    }
    n
}
unsafe extern "win64" fn o_ok(_: usize) -> i32 {
    S_OK
}
/// IPersistStorage::Load (entrée 6) : lit le flux « Texte ».
unsafe extern "win64" fn o_load(_: usize, storage: usize) -> i32 {
    let name = w("Texte");
    let open: unsafe extern "win64" fn(usize, *const u16, usize, u32, u32, *mut usize) -> i32 =
        std::mem::transmute(slot(storage, 4));
    let mut s = 0;
    let hr = open(storage, name.as_ptr(), 0, 0x10, 0, &mut s);
    if hr < 0 {
        return hr;
    }
    let read: unsafe extern "win64" fn(usize, *mut u8, u32, *mut u32) -> i32 =
        std::mem::transmute(slot(s, 3));
    let mut b = [0u8; 32];
    let mut n = 0;
    read(s, b.as_mut_ptr(), 32, &mut n);
    release(s);
    *CHARGE.lock().unwrap() = b[..n as usize].to_vec();
    S_OK
}
struct V<const N: usize>([*const (); N]);
unsafe impl<const N: usize> Sync for V<N> {}
static OBJ_V: V<10> = V([
    o_qi as *const (),
    o_add_ref as *const (),
    o_release as *const (),
    o_ok as *const (),
    o_ok as *const (),
    o_ok as *const (),
    o_load as *const (),
    o_ok as *const (),
    o_ok as *const (),
    o_ok as *const (),
]);
unsafe extern "win64" fn cf_create(
    _: usize,
    _outer: usize,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    let o = Box::into_raw(Box::new(Objet {
        vtbl: OBJ_V.0.as_ptr().cast(),
        refs: AtomicU32::new(1),
    })) as usize;
    let hr = o_qi(o, iid, out);
    o_release(o);
    hr
}
unsafe extern "win64" fn cf_ref(_: usize) -> u32 {
    1
}
unsafe extern "win64" fn cf_qi(this: usize, _iid: *const Guid, out: *mut usize) -> i32 {
    *out = this;
    S_OK
}
static CF_V: V<5> = V([
    cf_qi as *const (),
    cf_ref as *const (),
    cf_ref as *const (),
    cf_create as *const (),
    o_ok as *const (),
]);

/// GetClassFile et OleCreateFromFile : un docfile est copié dans le
/// stockage de l'objet puis chargé ; un fichier sans classe demande
/// l'objet paquet, absent ici.
#[test]
fn insertion_depuis_un_fichier() {
    unsafe {
        let dir = std::env::temp_dir().join(format!("winlux-ole-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let doc = w(dir.join("objet.doc").to_str().unwrap());
        let mut s = 0;
        assert_eq!(
            stg::stg_create_docfile(doc.as_ptr(), 0x1012, 0, &mut s),
            S_OK
        );
        assert_eq!(stg::write_class_stg(s, &CLSID_DOC), S_OK);
        let name = w("Texte");
        let create: unsafe extern "win64" fn(usize, *const u16, u32, u32, u32, *mut usize) -> i32 =
            std::mem::transmute(slot(s, 3));
        let mut stm = 0;
        assert_eq!(create(s, name.as_ptr(), 0x1012, 0, 0, &mut stm), S_OK);
        let write: unsafe extern "win64" fn(usize, *const u8, u32, *mut u32) -> i32 =
            std::mem::transmute(slot(stm, 4));
        let mut n = 0;
        write(stm, b"depuis le fichier".as_ptr(), 17, &mut n);
        release(stm);
        release(s);

        let mut clsid = Guid::default();
        assert_eq!(get_class_file(doc.as_ptr(), &mut clsid), S_OK);
        assert_eq!(clsid, CLSID_DOC);
        let absent = w(dir.join("absent.doc").to_str().unwrap());
        assert_eq!(
            get_class_file(absent.as_ptr(), &mut clsid),
            MK_E_CANTOPENFILE
        );
        let texte = dir.join("note.winluxsansclasse");
        std::fs::write(&texte, b"x").unwrap();
        let texte = w(texte.to_str().unwrap());
        assert_eq!(
            get_class_file(texte.as_ptr(), &mut clsid),
            MK_E_INVALIDEXTENSION
        );

        let factory = [CF_V.0.as_ptr() as usize];
        let mut cookie = 0;
        assert_eq!(
            super::super::ole32::co_register_class_object(
                &CLSID_DOC,
                factory.as_ptr() as usize,
                1,
                1,
                &mut cookie
            ),
            S_OK
        );
        let mut lb = 0;
        stg::create_ilockbytes_on_hglobal(0, 1, &mut lb);
        let mut dest = 0;
        assert_eq!(
            stg::stg_create_docfile_on_ilockbytes(lb, 0x1012, 0, &mut dest),
            S_OK
        );
        let mut obj = 0;
        assert_eq!(
            ole_create_from_file(
                std::ptr::null(),
                doc.as_ptr(),
                &IID_IUNKNOWN,
                1,
                0,
                0,
                dest,
                &mut obj
            ),
            S_OK
        );
        assert_eq!(&*CHARGE.lock().unwrap(), b"depuis le fichier");
        let mut c = Guid::default();
        assert_eq!(stg::read_class_stg(dest, &mut c), S_OK);
        assert_eq!(c, CLSID_DOC);
        release(obj);

        // Sans classe associée : CLSID_Package, non inscrit ici.
        let mut x = 0;
        let hr = ole_create_from_file(
            std::ptr::null(),
            texte.as_ptr(),
            &IID_IUNKNOWN,
            1,
            0,
            0,
            dest,
            &mut x,
        );
        assert_eq!((hr, x), (0x8004_0154u32 as i32, 0), "REGDB_E_CLASSNOTREG");
        assert_eq!(
            ole_create_from_file(
                std::ptr::null(),
                absent.as_ptr(),
                &IID_IUNKNOWN,
                1,
                0,
                0,
                dest,
                &mut x
            ),
            MK_E_CANTOPENFILE
        );
        release(dest);
        release(lb);
        super::super::ole32::co_revoke_class_object(cookie);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

static TRADUIT: Mutex<Option<u16>> = Mutex::new(None);
unsafe extern "win64" fn frame_translate(_: usize, _msg: usize, cmd: u16) -> i32 {
    *TRADUIT.lock().unwrap() = Some(cmd);
    S_OK
}

/// IsAccelerator et OleTranslateAccelerator sur une table du conteneur ;
/// groupes de menus de l'objet ; cycle d'un descripteur.
#[test]
fn accelerateurs_et_descripteurs_de_menu() {
    unsafe {
        // F5 (touche virtuelle) → commande 501.
        let accel: [u8; 6] = [0x01, 0, 0x74, 0, 0xF5, 0x01];
        let h = super::super::accel::create_accelerator_table_w(accel.as_ptr(), 1);
        assert_ne!(h, 0);
        let msg = super::super::gui::Msg {
            hwnd: 1,
            message: 0x0100,
            wparam: 0x74,
            lparam: 0,
            time: 0,
            pt: Default::default(),
            l_private: 0,
        };
        let mut cmd = 0u16;
        assert_eq!(is_accelerator(h, 1, &msg, &mut cmd), 1);
        assert_eq!(cmd, 501);
        assert_eq!(is_accelerator(h, 0, &msg, &mut cmd), 0);
        let autre = super::super::gui::Msg {
            wparam: 0x75,
            ..msg
        };
        assert_eq!(is_accelerator(h, 1, &autre, &mut cmd), 0);

        let mut table = [o_ok as *const (); 15];
        table[14] = frame_translate as *const ();
        let frame = [table.as_ptr() as usize];
        let mut info = [0u64; 4];
        info[0] = 32;
        info[2] = h as u64;
        info[3] = 1;
        assert_eq!(
            ole_translate_accelerator(frame.as_ptr() as usize, info.as_ptr().cast(), &msg),
            S_OK
        );
        assert_eq!(*TRADUIT.lock().unwrap(), Some(501));
        assert_eq!(
            ole_translate_accelerator(frame.as_ptr() as usize, info.as_ptr().cast(), &autre),
            S_FALSE
        );

        // Barre : Fichier | Édition (objet) | — | Objet x2 | Fenêtre | Aide (objet).
        let widths = [1, 1, 0, 2, 1, 1];
        let groups: Vec<bool> = (0..6).map(|i| is_object_group(&widths, i)).collect();
        assert_eq!(groups, [false, true, true, true, false, true]);
        assert!(!is_object_group(&widths, 9));
        let d = ole_create_menu_descriptor(0x1234, widths.as_ptr());
        assert_ne!(d, 0);
        assert_eq!(ole_create_menu_descriptor(0, widths.as_ptr()), 0);
        assert_eq!(ole_set_menu_descriptor(d, 0, 0, 0, 0), E_INVALIDARG);
        assert_eq!(ole_destroy_menu_descriptor(d), S_OK);
        assert_eq!(ole_set_menu_descriptor(d, 0x99, 0x98, 0, 0), E_INVALIDARG);
        super::super::accel::destroy_accelerator_table(h);
    }
}
