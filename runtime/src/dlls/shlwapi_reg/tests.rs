use super::*;

/// Registre isolé dans un fichier temporaire pour la durée du test.
struct Sandbox {
    path: std::path::PathBuf,
    previous: Option<std::ffi::OsString>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let lock = crate::registry::TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "winlux-shreg-{tag}-{}-{:?}.tsv",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);
        let previous = std::env::var_os("WINLUX_REGISTRY_PATH");
        std::env::set_var("WINLUX_REGISTRY_PATH", &path);
        Sandbox {
            path,
            previous,
            _lock: lock,
        }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(self.path.with_extension("lock"));
        match &self.previous {
            Some(v) => std::env::set_var("WINLUX_REGISTRY_PATH", v),
            None => std::env::remove_var("WINLUX_REGISTRY_PATH"),
        }
    }
}

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

fn a(s: &str) -> Vec<u8> {
    s.bytes().chain([0]).collect()
}

const HKCU: usize = reg::HKEY_CURRENT_USER;
const HKLM: usize = reg::HKEY_LOCAL_MACHINE;
const HKCR: usize = 0x8000_0000;

fn set_sz(root: usize, sub: &str, name: &str, kind: u32, text: &str) {
    let data = reg::utf16z(text);
    let r = unsafe {
        sh_set_value_w(
            root,
            w(sub).as_ptr(),
            w(name).as_ptr(),
            kind,
            data.as_ptr(),
            data.len() as u32,
        )
    };
    assert_eq!(r, ERROR_SUCCESS);
}

fn get_w(root: usize, sub: &str, name: &str) -> Result<(u32, String), i32> {
    let mut kind = 0u32;
    let mut buf = [0u16; 260];
    let mut size = (buf.len() * 2) as u32;
    let r = unsafe {
        sh_get_value_w(
            root,
            w(sub).as_ptr(),
            w(name).as_ptr(),
            &mut kind,
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if r != ERROR_SUCCESS {
        return Err(r);
    }
    let text = String::from_utf16_lossy(&buf[..size as usize / 2]);
    Ok((kind, text.trim_end_matches('\0').to_string()))
}

#[test]
fn get_set_delete_et_chaines_developpees() {
    let _s = Sandbox::new("gsd");
    set_sz(HKCU, "Software\\Studio\\Jeu", "Nom", reg::REG_SZ, "Été");
    assert_eq!(
        get_w(HKCU, "Software\\Studio\\Jeu", "Nom"),
        Ok((reg::REG_SZ, "Été".into()))
    );

    // REG_EXPAND_SZ développé et rendu comme REG_SZ.
    std::env::set_var("WINLUX_SHREG_TEST", "C:\\Jeux");
    set_sz(
        HKCU,
        "Software\\Studio\\Jeu",
        "Dossier",
        reg::REG_EXPAND_SZ,
        "%WINLUX_SHREG_TEST%\\Sauvegardes",
    );
    assert_eq!(
        get_w(HKCU, "Software\\Studio\\Jeu", "Dossier"),
        Ok((reg::REG_SZ, "C:\\Jeux\\Sauvegardes".into()))
    );
    std::env::remove_var("WINLUX_SHREG_TEST");

    // Version « A » : Windows-1252 dans les deux sens.
    let mut buf = [0u8; 32];
    let mut size = buf.len() as u32;
    let r = unsafe {
        sh_get_value_a(
            HKCU,
            a("Software\\Studio\\Jeu").as_ptr(),
            a("Nom").as_ptr(),
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            &mut size,
        )
    };
    assert_eq!(r, ERROR_SUCCESS);
    assert_eq!(&buf[..size as usize], &[0xC9, b't', 0xE9, 0]);

    // Tampon trop petit : ERROR_MORE_DATA et taille nécessaire.
    let mut small = [0u16; 2];
    let mut size = 4u32;
    let r = unsafe {
        sh_get_value_w(
            HKCU,
            w("Software\\Studio\\Jeu").as_ptr(),
            w("Nom").as_ptr(),
            std::ptr::null_mut(),
            small.as_mut_ptr().cast(),
            &mut size,
        )
    };
    assert_eq!((r, size), (ERROR_MORE_DATA, 8));

    let r =
        unsafe { sh_delete_value_w(HKCU, w("Software\\Studio\\Jeu").as_ptr(), w("Nom").as_ptr()) };
    assert_eq!(r, ERROR_SUCCESS);
    assert_eq!(
        get_w(HKCU, "Software\\Studio\\Jeu", "Nom"),
        Err(ERROR_FILE_NOT_FOUND)
    );
}

#[test]
fn delete_key_et_delete_empty_key() {
    let _s = Sandbox::new("del");
    set_sz(
        HKCU,
        "Software\\Studio\\Jeu\\Profils",
        "Actif",
        reg::REG_SZ,
        "1",
    );
    // SHDeleteEmptyKey refuse une clé qui a encore des sous-clés.
    let r = unsafe { sh_delete_empty_key_w(HKCU, w("Software\\Studio\\Jeu").as_ptr()) };
    assert_eq!(r, ERROR_KEY_HAS_CHILDREN);
    // SHDeleteKey supprime toute l'arborescence.
    let r = unsafe { sh_delete_key_w(HKCU, w("Software\\Studio").as_ptr()) };
    assert_eq!(r, ERROR_SUCCESS);
    assert!(!reg::read(|d| Ok(d.exists("hkcu\\Software\\Studio"))).unwrap());
    // Une ruche ne se supprime pas.
    assert_eq!(
        unsafe { sh_delete_key_w(HKCU, w("").as_ptr()) },
        ERROR_ACCESS_DENIED
    );
    // Clé vide : supprimée.
    reg::write(|d| {
        d.ensure("hkcu\\Software\\Vide");
        Ok(())
    })
    .unwrap();
    assert_eq!(
        unsafe { sh_delete_empty_key_w(HKCU, w("Software\\Vide").as_ptr()) },
        ERROR_SUCCESS
    );
}

#[test]
fn valeurs_us_utilisateur_puis_machine() {
    let _s = Sandbox::new("us");
    set_sz(HKLM, "Software\\Studio", "Langue", reg::REG_SZ, "en");
    set_sz(HKCU, "Software\\Studio", "Langue", reg::REG_SZ, "fr");
    set_sz(HKLM, "Software\\Studio", "Plein", reg::REG_SZ, "yes");

    let read = |ignore: i32| {
        let mut buf = [0u16; 16];
        let mut size = 32u32;
        let r = unsafe {
            sh_reg_get_us_value_w(
                w("Software\\Studio").as_ptr(),
                w("Langue").as_ptr(),
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
                ignore,
                std::ptr::null(),
                0,
            )
        };
        assert_eq!(r, ERROR_SUCCESS);
        String::from_utf16_lossy(&buf[..size as usize / 2 - 1])
    };
    assert_eq!(read(0), "fr");
    assert_eq!(read(1), "en");

    let bool_of = |name: &str, default: i32| unsafe {
        sh_reg_get_bool_us_value_w(w("Software\\Studio").as_ptr(), w(name).as_ptr(), 0, default)
    };
    assert_eq!(bool_of("Plein", 0), 1);
    assert_eq!(bool_of("Absente", 1), 1);
    assert_eq!(bool_of("Absente", 0), 0);

    // Valeur absente : la valeur par défaut est rendue.
    let default = 42u32.to_le_bytes();
    let mut out = 0u32;
    let mut size = 4u32;
    let r = unsafe {
        sh_reg_get_us_value_w(
            w("Software\\Studio").as_ptr(),
            w("Volume").as_ptr(),
            std::ptr::null_mut(),
            (&mut out as *mut u32).cast(),
            &mut size,
            0,
            default.as_ptr(),
            4,
        )
    };
    assert_eq!((r, out), (ERROR_SUCCESS, 42));

    // Sans FORCE, HKLM n'est écrit que si la valeur n'existe pas.
    let data = reg::utf16z("de");
    let r = unsafe {
        sh_reg_set_us_value_w(
            w("Software\\Studio").as_ptr(),
            w("Langue").as_ptr(),
            reg::REG_SZ,
            data.as_ptr(),
            data.len() as u32,
            SHREGSET_FORCE_HKCU | SHREGSET_HKLM,
        )
    };
    assert_eq!(r, ERROR_SUCCESS);
    assert_eq!(read(0), "de");
    assert_eq!(read(1), "en");
}

#[test]
fn cles_us_ouverture_enumeration_fermeture() {
    let _s = Sandbox::new("uskey");
    set_sz(HKCU, "Software\\Studio\\Jeu", "Joueur", reg::REG_SZ, "Ana");
    set_sz(HKLM, "Software\\Studio\\Jeu", "Version", reg::REG_SZ, "2");
    set_sz(HKLM, "Software\\Studio\\Jeu\\Cartes", "", reg::REG_SZ, "");

    let mut parent = 0usize;
    let r = unsafe { sh_reg_open_us_key_w(w("Software\\Studio").as_ptr(), 0, 0, &mut parent, 0) };
    assert_eq!(r, ERROR_SUCCESS);
    let mut key = 0usize;
    let r = unsafe { sh_reg_open_us_key_w(w("Jeu").as_ptr(), 0, parent, &mut key, 0) };
    assert_eq!(r, ERROR_SUCCESS);

    let mut buf = [0u16; 16];
    let mut size = 32u32;
    let r = unsafe {
        sh_reg_query_us_value_w(
            key,
            w("Version").as_ptr(),
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
            0,
            std::ptr::null(),
            0,
        )
    };
    assert_eq!(r, ERROR_SUCCESS);
    assert_eq!(String::from_utf16_lossy(&buf[..1]), "2");

    // SHREGENUM_BOTH : valeurs de HKCU et HKLM réunies.
    let mut names = Vec::new();
    for i in 0.. {
        let mut name = [0u16; 32];
        let mut cch = 32u32;
        let r = unsafe {
            sh_reg_enum_us_value_w(
                key,
                i,
                name.as_mut_ptr(),
                &mut cch,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                SHREG_HKCU | SHREG_HKLM,
            )
        };
        if r == ERROR_NO_MORE_ITEMS {
            break;
        }
        assert_eq!(r, ERROR_SUCCESS);
        names.push(String::from_utf16_lossy(&name[..cch as usize]));
    }
    assert_eq!(names, ["Joueur", "Version"]);

    let mut name = [0u16; 32];
    let mut cch = 32u32;
    let r = unsafe { sh_reg_enum_us_key_w(key, 0, name.as_mut_ptr(), &mut cch, SHREG_HKLM) };
    assert_eq!(r, ERROR_SUCCESS);
    assert_eq!(String::from_utf16_lossy(&name[..cch as usize]), "Cartes");

    let mut values = 0u32;
    let r = unsafe {
        sh_reg_query_info_us_key(
            key,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut values,
            std::ptr::null_mut(),
            SHREG_HKCU | SHREG_HKLM,
        )
    };
    assert_eq!((r, values), (ERROR_SUCCESS, 2));

    assert_eq!(sh_reg_close_us_key(key), ERROR_SUCCESS);
    assert_eq!(sh_reg_close_us_key(parent), ERROR_SUCCESS);
    assert_eq!(sh_reg_close_us_key(parent), ERROR_INVALID_PARAMETER);

    let mut missing = 0usize;
    let r = unsafe { sh_reg_open_us_key_w(w("Software\\Absent").as_ptr(), 0, 0, &mut missing, 0) };
    assert_eq!(r, ERROR_FILE_NOT_FOUND);
}

#[test]
fn flux_sur_une_valeur_binaire() {
    let _s = Sandbox::new("stream");
    let sub = w("Software\\Studio\\Fenetres");
    let name = w("Disposition");
    unsafe {
        // Lecture d'une valeur absente : pas de flux.
        assert!(sh_open_reg_stream2_w(HKCU, sub.as_ptr(), name.as_ptr(), 0).is_null());
        // L'ancienne version rend un flux vide.
        let empty = sh_open_reg_stream_w(HKCU, sub.as_ptr(), name.as_ptr(), 0);
        assert!(!empty.is_null());
        let vt = &**(empty as *const *const StreamVtbl);
        (vt.release)(empty.cast());

        // Écriture puis Release : la valeur binaire est enregistrée.
        let s = sh_open_reg_stream2_w(HKCU, sub.as_ptr(), name.as_ptr(), STGM_WRITE);
        let vt = &**(s as *const *const StreamVtbl);
        let mut written = 0;
        assert_eq!(
            (vt.write)(s.cast(), [1u8, 2, 3, 4].as_ptr(), 4, &mut written),
            S_OK
        );
        assert_eq!(written, 4);
        assert_eq!((vt.release)(s.cast()), 0);
        let v = reg::read(|d| {
            Ok(d.get("hkcu\\Software\\Studio\\Fenetres", "Disposition")
                .cloned())
        })
        .unwrap()
        .unwrap();
        assert_eq!((v.kind, v.bytes), (reg::REG_BINARY, vec![1, 2, 3, 4]));

        // Relecture, déplacement et lecture seule respectée.
        let s = sh_open_reg_stream2_w(HKCU, sub.as_ptr(), name.as_ptr(), 0);
        let vt = &**(s as *const *const StreamVtbl);
        let mut pos = 0u64;
        assert_eq!((vt.seek)(s.cast(), 2, 0, &mut pos), S_OK);
        let mut buf = [0u8; 8];
        let mut read = 0;
        assert_eq!((vt.read)(s.cast(), buf.as_mut_ptr(), 8, &mut read), S_OK);
        assert_eq!(&buf[..read as usize], &[3, 4]);
        assert_eq!(
            (vt.write)(s.cast(), buf.as_ptr(), 1, std::ptr::null_mut()),
            STG_E_ACCESSDENIED
        );
        let mut iface = std::ptr::null_mut();
        assert_eq!(
            (vt.query_interface)(s.cast(), &IID_ISTREAM, &mut iface),
            S_OK
        );
        (vt.release)(iface.cast());
        assert_eq!((vt.release)(s.cast()), 0);
    }
}

fn assoc_w(kind: u32, assoc: &str, extra: &str) -> Result<String, i32> {
    let mut cch = 0u32;
    let extra = if extra.is_empty() {
        None
    } else {
        Some(w(extra))
    };
    let extra_ptr = extra.as_ref().map_or(std::ptr::null(), |e| e.as_ptr());
    let r = unsafe {
        assoc_query_string_w(
            0,
            kind,
            w(assoc).as_ptr(),
            extra_ptr,
            std::ptr::null_mut(),
            &mut cch,
        )
    };
    if r != S_FALSE {
        return Err(r);
    }
    let mut buf = vec![0u16; cch as usize];
    let r = unsafe {
        assoc_query_string_w(
            0,
            kind,
            w(assoc).as_ptr(),
            extra_ptr,
            buf.as_mut_ptr(),
            &mut cch,
        )
    };
    assert_eq!(r, S_OK);
    Ok(String::from_utf16_lossy(&buf[..cch as usize - 1]))
}

#[test]
fn associations_de_fichiers() {
    let _s = Sandbox::new("assoc");
    let classes = reg::HKEY_LOCAL_MACHINE;
    set_sz(
        classes,
        "Software\\Classes\\.carte",
        "",
        reg::REG_SZ,
        "Studio.Carte",
    );
    set_sz(
        classes,
        "Software\\Classes\\.carte",
        "Content Type",
        reg::REG_SZ,
        "application/x-carte",
    );
    set_sz(
        classes,
        "Software\\Classes\\Studio.Carte",
        "",
        reg::REG_SZ,
        "Carte du Studio",
    );
    set_sz(
        classes,
        "Software\\Classes\\Studio.Carte\\DefaultIcon",
        "",
        reg::REG_SZ,
        "C:\\Program Files\\Studio\\editeur.exe,1",
    );
    set_sz(
        classes,
        "Software\\Classes\\Studio.Carte\\shell\\open\\command",
        "",
        reg::REG_SZ,
        "\"C:\\Program Files\\Studio\\editeur.exe\" \"%1\"",
    );
    set_sz(
        classes,
        "Software\\Classes\\Studio.Carte\\shell\\imprimer\\command",
        "",
        reg::REG_SZ,
        "C:\\Studio\\imprime.exe /p \"%1\"",
    );
    // Une préférence de l'utilisateur (HKCU) passe devant celle de la machine.
    set_sz(
        HKCU,
        "Software\\Classes\\Applications\\editeur.exe",
        "FriendlyAppName",
        reg::REG_SZ,
        "Éditeur de cartes",
    );

    assert_eq!(
        assoc_w(ASSOCSTR_COMMAND, ".carte", ""),
        Ok("\"C:\\Program Files\\Studio\\editeur.exe\" \"%1\"".into())
    );
    assert_eq!(
        assoc_w(ASSOCSTR_EXECUTABLE, ".carte", ""),
        Ok("C:\\Program Files\\Studio\\editeur.exe".into())
    );
    assert_eq!(
        assoc_w(ASSOCSTR_EXECUTABLE, ".carte", "imprimer"),
        Ok("C:\\Studio\\imprime.exe".into())
    );
    assert_eq!(
        assoc_w(ASSOCSTR_FRIENDLYDOCNAME, ".carte", ""),
        Ok("Carte du Studio".into())
    );
    assert_eq!(
        assoc_w(ASSOCSTR_FRIENDLYAPPNAME, ".carte", ""),
        Ok("Éditeur de cartes".into())
    );
    assert_eq!(
        assoc_w(ASSOCSTR_FRIENDLYAPPNAME, ".carte", "imprimer"),
        Ok("imprime".into())
    );
    assert_eq!(
        assoc_w(ASSOCSTR_CONTENTTYPE, ".carte", ""),
        Ok("application/x-carte".into())
    );
    assert_eq!(
        assoc_w(ASSOCSTR_DEFAULTICON, "Studio.Carte", ""),
        Ok("C:\\Program Files\\Studio\\editeur.exe,1".into())
    );
    assert_eq!(
        assoc_w(ASSOCSTR_PROGID, ".carte", ""),
        Ok("Studio.Carte".into())
    );
    assert_eq!(
        assoc_w(ASSOCSTR_COMMAND, ".inconnu", ""),
        Err(E_NO_ASSOCIATION)
    );

    // Tampon trop petit : E_POINTER et taille nécessaire.
    let mut buf = [0u16; 4];
    let mut cch = 4u32;
    let r = unsafe {
        assoc_query_string_w(
            0,
            ASSOCSTR_PROGID,
            w(".carte").as_ptr(),
            std::ptr::null(),
            buf.as_mut_ptr(),
            &mut cch,
        )
    };
    assert_eq!((r, cch), (E_POINTER, 13));

    let mut key = 0usize;
    let r = unsafe {
        assoc_query_key_w(
            0,
            ASSOCKEY_CLASS,
            w(".carte").as_ptr(),
            std::ptr::null(),
            &mut key,
        )
    };
    assert_eq!(r, S_OK);
    assert_eq!(reg::handle_path(key).unwrap(), "hkcr\\Studio.Carte");
    reg::close_handle(key);
    let _ = HKCR;
}

#[test]
fn reg_get_value_et_chemins() {
    let _s = Sandbox::new("path");
    set_sz(
        HKCU,
        "Software\\Studio",
        "Sauvegardes",
        reg::REG_SZ,
        "C:\\Jeux\\Saves",
    );
    let mut out = [0u16; MAX_PATH];
    let r = unsafe {
        sh_reg_get_path_w(
            HKCU,
            w("Software\\Studio").as_ptr(),
            w("Sauvegardes").as_ptr(),
            out.as_mut_ptr(),
            0,
        )
    };
    assert_eq!(r, ERROR_SUCCESS);
    assert_eq!(
        unsafe { crate::dlls::wstr(out.as_ptr()) },
        "C:\\Jeux\\Saves"
    );

    let r = unsafe {
        sh_reg_set_path_w(
            HKCU,
            w("Software\\Studio").as_ptr(),
            w("Captures").as_ptr(),
            w("C:\\Images").as_ptr(),
            0,
        )
    };
    assert_eq!(r, ERROR_SUCCESS);
    assert_eq!(
        get_w(HKCU, "Software\\Studio", "Captures"),
        Ok((reg::REG_SZ, "C:\\Images".into()))
    );

    // SHRegGetValue accepte les drapeaux SRRF_RM_* sans les confondre avec
    // les drapeaux WOW64 de RegGetValue.
    let mut size = 0u32;
    let r = unsafe {
        sh_reg_get_value_w(
            HKCU,
            w("Software\\Studio").as_ptr(),
            w("Captures").as_ptr(),
            0x0000_0002 | 0x0001_0000,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    assert_eq!((r, size), (ERROR_SUCCESS, 20));

    let dup = sh_reg_duplicate_hkey(HKCU);
    assert_eq!(dup, HKCU);
}
