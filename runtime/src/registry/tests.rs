use super::*;
use crate::dlls::advapi32 as api;

/// Registre isolé dans un fichier temporaire pour la durée du test.
struct Sandbox {
    path: PathBuf,
    previous: Option<std::ffi::OsString>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let lock = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "winlux-reg-{tag}-{}-{:?}.tsv",
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

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[test]
fn chaines_partagees_entre_api_a_et_w() {
    let _s = Sandbox::new("aw");
    unsafe {
        let mut h = 0;
        assert_eq!(
            api::reg_create_key_ex_w(
                HKEY_CURRENT_USER,
                wide("Software\\Éditeur\\MonAppli").as_ptr(),
                0,
                std::ptr::null(),
                0,
                0,
                0,
                &mut h,
                std::ptr::null_mut()
            ),
            0
        );
        let data = utf16z("Café");
        assert_eq!(
            api::reg_set_value_ex_w(
                h,
                wide("Dossier").as_ptr(),
                0,
                REG_SZ,
                data.as_ptr(),
                data.len() as u32
            ),
            0
        );
        // Lecture « A » : la même chaîne en Windows-1252.
        let mut buf = [0u8; 16];
        let mut size = 16u32;
        let mut kind = 0;
        assert_eq!(
            api::reg_query_value_ex_a(
                h,
                c"dossier".as_ptr().cast(),
                std::ptr::null_mut(),
                &mut kind,
                buf.as_mut_ptr(),
                &mut size
            ),
            0
        );
        assert_eq!((kind, &buf[..size as usize]), (REG_SZ, &b"Caf\xE9\0"[..]));
        // Écriture « A », lecture « W ».
        let ansi = b"Gr\xFCn\0";
        assert_eq!(
            api::reg_set_value_ex_a(
                h,
                c"Couleur".as_ptr().cast(),
                0,
                REG_SZ,
                ansi.as_ptr(),
                ansi.len() as u32
            ),
            0
        );
        let mut out = [0u8; 32];
        let mut size = 32u32;
        assert_eq!(
            api::reg_query_value_ex_w(
                h,
                wide("COULEUR").as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                out.as_mut_ptr(),
                &mut size
            ),
            0
        );
        assert_eq!(string_of(&out[..size as usize]), "Grün");
        // Énumération : la casse d'origine est conservée.
        let mut parent = 0;
        assert_eq!(
            api::reg_open_key_ex_a(
                HKEY_CURRENT_USER,
                c"software\\\xC9diteur".as_ptr().cast(),
                0,
                0,
                &mut parent
            ),
            0
        );
        let mut name = [0u16; 32];
        let mut len = 32u32;
        assert_eq!(
            api::reg_enum_key_ex_w(
                parent,
                0,
                name.as_mut_ptr(),
                &mut len,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut()
            ),
            0
        );
        assert_eq!(String::from_utf16_lossy(&name[..len as usize]), "MonAppli");
        let mut vname = [0u16; 32];
        let mut vlen = 32u32;
        assert_eq!(
            api::reg_enum_value_w(
                h,
                1,
                vname.as_mut_ptr(),
                &mut vlen,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut()
            ),
            0
        );
        assert_eq!(String::from_utf16_lossy(&vname[..vlen as usize]), "Dossier");
        // RegGetValue développe REG_EXPAND_SZ et garantit le zéro final.
        std::env::set_var("WINLUX_TEST_DOSSIER", "/maison");
        let expand = utf16z("%WINLUX_TEST_DOSSIER%\\jeux");
        api::reg_set_value_ex_w(
            h,
            wide("Jeux").as_ptr(),
            0,
            REG_EXPAND_SZ,
            expand.as_ptr(),
            expand.len() as u32,
        );
        let mut size = 64u32;
        let mut out = [0u8; 64];
        let mut kind = 0;
        assert_eq!(
            api::reg_get_value_w(
                h,
                std::ptr::null(),
                wide("Jeux").as_ptr(),
                0x02,
                &mut kind,
                out.as_mut_ptr(),
                &mut size
            ),
            0
        );
        assert_eq!(kind, REG_SZ);
        assert_eq!(string_of(&out[..size as usize]), "/maison\\jeux");
        assert_eq!(
            api::reg_get_value_w(
                h,
                std::ptr::null(),
                wide("Jeux").as_ptr(),
                0x10,
                &mut kind,
                out.as_mut_ptr(),
                &mut size
            ),
            1630
        );
        // RegDeleteKey refuse une clé qui a encore des sous-clés.
        assert_eq!(
            api::reg_delete_key_w(HKEY_CURRENT_USER, wide("Software\\Éditeur").as_ptr()),
            ERROR_ACCESS_DENIED
        );
        assert_eq!(
            api::reg_delete_tree_w(HKEY_CURRENT_USER, wide("Software\\Éditeur").as_ptr()),
            0
        );
        assert_eq!(
            api::reg_open_key_ex_w(
                HKEY_CURRENT_USER,
                wide("Software\\Éditeur").as_ptr(),
                0,
                0,
                &mut parent
            ),
            ERROR_FILE_NOT_FOUND
        );
        api::reg_close_key(h);
    }
}

#[test]
fn hkcr_fusionne_hkcu_et_hklm() {
    let _s = Sandbox::new("hkcr");
    write(|d| {
        d.set(
            "hklm\\Software\\Classes\\.txt",
            "",
            REG_SZ,
            utf16z("txtfile"),
        );
        d.set(
            "hklm\\Software\\Classes\\.txt",
            "Content Type",
            REG_SZ,
            utf16z("text/plain"),
        );
        d.set(
            "hkcu\\Software\\Classes\\.txt",
            "",
            REG_SZ,
            utf16z("monediteur"),
        );
        d.ensure("hkcu\\Software\\Classes\\.perso");
        Ok(())
    })
    .unwrap();
    unsafe {
        let mut h = 0;
        assert_eq!(
            api::reg_open_key_ex_a(0x8000_0000, c".TXT".as_ptr().cast(), 0, 0, &mut h),
            0
        );
        let mut buf = [0u8; 32];
        let mut size = 32u32;
        api::reg_query_value_ex_a(
            h,
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            &mut size,
        );
        assert_eq!(&buf[..size as usize], b"monediteur\0");
        // Valeur absente de HKCU : reprise de HKLM.
        let mut size = 32u32;
        assert_eq!(
            api::reg_query_value_ex_a(
                h,
                c"Content Type".as_ptr().cast(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                buf.as_mut_ptr(),
                &mut size
            ),
            0
        );
        assert_eq!(&buf[..size as usize], b"text/plain\0");
        api::reg_close_key(h);
    }
    // Les deux ruches apparaissent sous HKCR ; .exe vient des valeurs par défaut.
    let children = read(|d| Ok(d.subkeys(HKCR))).unwrap();
    for name in [".exe", ".perso", ".txt"] {
        assert!(
            children.iter().any(|c| c == name),
            "{name} dans {children:?}"
        );
    }
    // Une nouvelle clé HKCR est écrite dans HKLM\Software\Classes.
    unsafe {
        let mut h = 0;
        api::reg_create_key_a(0x8000_0000, c"Winlux.Doc".as_ptr().cast(), &mut h);
        api::reg_close_key(h);
    }
    assert!(read(|d| Ok(d.key("hklm\\software\\classes\\winlux.doc").is_some())).unwrap());
    assert!(!std::fs::read_to_string(store_path())
        .unwrap()
        .contains(&hex(b"hkcr")));
}

#[test]
fn ancien_fichier_v1_est_converti() {
    let line = |p: &str, n: &str, k: u32, v: &[u8]| {
        format!(
            "V\t{}\t{}\t{k}\t{}",
            hex(p.as_bytes()),
            hex(n.as_bytes()),
            hex(v)
        )
    };
    let text = [
        HEADER_V1.to_string(),
        format!("K\t{}", hex(b"hkcr\\.abc")),
        line("hkcr\\.abc", "", 1, b"abcfile\0"),
        line("hkcu\\software\\x", "nom", 1, "Été\0".as_bytes()),
        line("hkcu\\software\\x", "large", 1, &utf16z("déjà")),
        line("hkcu\\software\\x", "n", 4, &[1, 0, 0, 0]),
    ]
    .join("\n");
    let d = parse(&text).unwrap();
    assert_eq!(
        string_of(&d.get("hklm\\software\\classes\\.abc", "").unwrap().bytes),
        "abcfile"
    );
    assert_eq!(
        string_of(&d.get(HKCR, "").map_or(Vec::new(), |v| v.bytes.clone())),
        ""
    );
    assert_eq!(
        string_of(&d.get("hkcr\\.abc", "").unwrap().bytes),
        "abcfile"
    );
    assert_eq!(
        string_of(&d.get("hkcu\\software\\x", "NOM").unwrap().bytes),
        "Été"
    );
    assert_eq!(
        string_of(&d.get("hkcu\\software\\x", "large").unwrap().bytes),
        "déjà"
    );
    assert_eq!(d.get("hkcu\\software\\x", "n").unwrap().bytes, [1, 0, 0, 0]);
    // Le nouveau format se relit à l'identique.
    let again = parse(&serialize(&d)).unwrap();
    assert_eq!(serialize(&again), serialize(&d));
}

#[test]
fn autre_appli_voit_les_modifications() {
    let _s = Sandbox::new("partage");
    write(|d| {
        d.set(
            "hkcu\\Software\\Jeu",
            "Score",
            REG_DWORD,
            5u32.to_le_bytes().to_vec(),
        );
        Ok(())
    })
    .unwrap();
    assert!(read(|d| Ok(d.get("hkcu\\software\\jeu", "score").is_some())).unwrap());
    // Une autre appli remplace le fichier : la copie en mémoire est relue.
    let mut other = read(|d| Ok(d.clone())).unwrap();
    other.set(
        "hkcu\\Software\\Jeu",
        "Score",
        REG_DWORD,
        9u32.to_le_bytes().to_vec(),
    );
    std::thread::sleep(std::time::Duration::from_millis(20));
    let tmp = store_path().with_extension("autre");
    std::fs::write(&tmp, serialize(&other)).unwrap();
    std::fs::rename(&tmp, store_path()).unwrap();
    let score = read(|d| Ok(d.get("hkcu\\software\\jeu", "score").unwrap().bytes.clone())).unwrap();
    assert_eq!(score, 9u32.to_le_bytes());
}

const SAMPLE: &str = "Windows Registry Editor Version 5.00\r\n\
\r\n\
; réglages livrés avec l'appli\r\n\
[HKEY_CURRENT_USER\\Software\\Studio Exemple\\Appli]\r\n\
@=\"Appli Exemple\"\r\n\
\"Chemin\"=\"C:\\\\Program Files\\\\Appli \\\"pro\\\"\"\r\n\
\"Niveau\"=dword:0000002a\r\n\
\"Données\"=hex:01,02,ff,\\\r\n\
  10\r\n\
\"Dossier\"=hex(2):25,00,41,00,25,00,00,00\r\n\
\"Liste\"=hex(7):61,00,00,00,62,00,00,00,00,00\r\n\
\"Gros\"=hex(b):01,00,00,00,00,00,00,00\r\n\
\r\n\
[HKEY_CLASSES_ROOT\\.exemple]\r\n\
@=\"Exemple.Document\"\r\n\
\r\n\
[-HKEY_CURRENT_USER\\Software\\Vieux]\r\n\
\r\n\
[HKEY_CURRENT_USER\\Software\\Studio Exemple]\r\n\
\"Obsolete\"=-\r\n";

#[test]
fn fichier_reg_importe_et_reexporte() {
    let _s = Sandbox::new("regfile");
    write(|d| {
        d.ensure("hkcu\\Software\\Vieux\\Sous");
        d.set(
            "hkcu\\Software\\Studio Exemple",
            "Obsolete",
            REG_SZ,
            utf16z("x"),
        );
        Ok(())
    })
    .unwrap();
    let ops = regfile::parse(SAMPLE).unwrap();
    assert_eq!(regfile::apply(&ops).unwrap(), ops.len());
    let k = "hkcu\\software\\studio exemple\\appli";
    read(|d| {
        assert_eq!(string_of(&d.get(k, "").unwrap().bytes), "Appli Exemple");
        assert_eq!(
            string_of(&d.get(k, "chemin").unwrap().bytes),
            "C:\\Program Files\\Appli \"pro\""
        );
        assert_eq!(d.get(k, "niveau").unwrap().bytes, 42u32.to_le_bytes());
        assert_eq!(d.get(k, "données").unwrap().bytes, [1, 2, 0xff, 0x10]);
        assert_eq!(d.get(k, "dossier").unwrap().kind, REG_EXPAND_SZ);
        assert_eq!(multi_of(&d.get(k, "liste").unwrap().bytes), ["a", "b"]);
        assert_eq!(d.get(k, "gros").unwrap().kind, REG_QWORD);
        assert_eq!(
            string_of(
                &d.get("hklm\\software\\classes\\.exemple", "")
                    .unwrap()
                    .bytes
            ),
            "Exemple.Document"
        );
        assert!(d.key("hkcu\\software\\vieux").is_none());
        assert!(d
            .get("hkcu\\software\\studio exemple", "obsolete")
            .is_none());
        // La casse d'origine est gardée pour regedit.
        assert_eq!(
            d.key(k).unwrap().name,
            "hkcu\\Software\\Studio Exemple\\Appli"
        );
        Ok(())
    })
    .unwrap();
    // Export puis réimport dans un registre vide : mêmes valeurs.
    let file = std::env::temp_dir().join(format!("winlux-export-{}.reg", std::process::id()));
    assert_eq!(
        regfile::export_file(&file, Some("hkcu\\Software\\Studio Exemple")).unwrap(),
        2
    );
    let bytes = std::fs::read(&file).unwrap();
    assert_eq!(&bytes[..2], &[0xFF, 0xFE]);
    let text = regfile::decode(&bytes);
    assert!(text.contains("[HKEY_CURRENT_USER\\Software\\Studio Exemple\\Appli]"));
    assert!(text.contains("\"Niveau\"=dword:0000002a"));
    let before = read(|d| Ok(d.values(k))).unwrap();
    write(|d| {
        d.delete_tree("hkcu\\software\\studio exemple");
        Ok(())
    })
    .unwrap();
    regfile::import_file(&file).unwrap();
    assert_eq!(read(|d| Ok(d.values(k))).unwrap(), before);
    let _ = std::fs::remove_file(&file);
}

#[test]
fn regedit4_ansi_et_erreurs() {
    let text =
        "REGEDIT4\n\n[HKEY_LOCAL_MACHINE\\Software\\Vieux]\n\"Chemin\"=hex(2):25,41,25,e9,00\n";
    let ops = regfile::parse(text).unwrap();
    match &ops[1] {
        regfile::Op::Set(_, _, kind, bytes) => {
            assert_eq!(*kind, REG_EXPAND_SZ);
            assert_eq!(string_of(bytes), "%A%é");
        }
        other => panic!("{other:?}"),
    }
    assert!(regfile::parse("bonjour\n[HKEY_CURRENT_USER\\x]").is_err());
    assert!(regfile::parse("REGEDIT4\n[HKEY_INCONNU\\x]").is_err());
    assert!(regfile::parse("REGEDIT4\n[HKEY_CURRENT_USER\\x]\n\"a\"=dword:zz").is_err());
}

#[test]
fn commande_reg_ajoute_lit_et_supprime() {
    let _s = Sandbox::new("cli");
    let run =
        |args: &[&str]| cli::reg_main(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>());
    assert_eq!(
        run(&[
            "add",
            "HKCU\\Software\\Outil",
            "/v",
            "Port",
            "/t",
            "REG_DWORD",
            "/d",
            "0x1f90",
            "/f"
        ]),
        0
    );
    assert_eq!(
        run(&[
            "add",
            "HKCU\\Software\\Outil",
            "/v",
            "Langues",
            "/t",
            "REG_MULTI_SZ",
            "/d",
            "fr\\0en"
        ]),
        0
    );
    assert_eq!(
        run(&["add", "HKCU\\Software\\Outil", "/ve", "/d", "Outil"]),
        0
    );
    read(|d| {
        assert_eq!(
            d.get("hkcu\\software\\outil", "port").unwrap().bytes,
            8080u32.to_le_bytes()
        );
        assert_eq!(
            multi_of(&d.get("hkcu\\software\\outil", "langues").unwrap().bytes),
            ["fr", "en"]
        );
        assert_eq!(
            string_of(&d.get("hkcu\\software\\outil", "").unwrap().bytes),
            "Outil"
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(run(&["query", "HKCU\\Software\\Outil", "/v", "Port"]), 0);
    assert_eq!(run(&["query", "HKCU\\Software\\Outil", "/v", "Absent"]), 1);
    assert_eq!(
        run(&["delete", "HKCU\\Software\\Outil", "/v", "Port", "/f"]),
        0
    );
    assert_eq!(run(&["delete", "HKCU\\Software\\Outil", "/f"]), 0);
    assert_eq!(run(&["query", "HKCU\\Software\\Outil"]), 1);
    assert_eq!(
        run(&["add", "HKCU\\x", "/v", "n", "/t", "REG_DWORD", "/d", "abc"]),
        1
    );
    assert!(cli::is_reg(Path::new("C:\\Windows\\System32\\REG.EXE")));
    assert!(cli::is_regedit(Path::new("regedit")));
    assert!(!cli::is_reg(Path::new("regedit.exe")));
}

#[test]
fn renommer_cles_et_valeurs() {
    let mut d = defaults();
    d.set("hkcu\\Software\\A\\B", "v", REG_SZ, utf16z("1"));
    d.rename_key("hkcu\\software\\a", "Nouveau").unwrap();
    assert!(d.key("hkcu\\software\\a").is_none());
    assert_eq!(
        d.key("hkcu\\software\\nouveau\\b").unwrap().name,
        "hkcu\\Software\\Nouveau\\B"
    );
    d.rename_value("hkcu\\software\\nouveau\\b", "v", "W")
        .unwrap();
    assert_eq!(d.values("hkcu\\software\\nouveau\\b")[0].name, "W");
    assert_eq!(d.rename_key("hkcu", "x"), Err(ERROR_FILE_NOT_FOUND));
    d.ensure("hkcu\\Software\\Autre");
    assert_eq!(
        d.rename_key("hkcu\\software\\autre", "nouveau"),
        Err(ERROR_ALREADY_EXISTS)
    );
    assert!(!d.delete_tree("hklm"));
    assert!(!d.delete_tree(HKCR));
}

#[test]
fn chemins_saisis() {
    assert_eq!(
        canonical("HKEY_LOCAL_MACHINE\\Software\\\\X\\").as_deref(),
        Some("hklm\\Software\\X")
    );
    assert_eq!(canonical("hkcr/.txt").as_deref(), Some("hkcr\\.txt"));
    assert_eq!(canonical("Ordinateur\\HKCU"), None);
    assert_eq!(
        display_path("hkcu\\Software"),
        "HKEY_CURRENT_USER\\Software"
    );
    assert_eq!(display_path("hkcr"), "HKEY_CLASSES_ROOT");
    assert_eq!(predefined(0xFFFF_FFFF_8000_0001), Some("hkcu"));
    assert_eq!(predefined(0x1_8000_0001), None);
}

#[test]
fn proxy_partage_avec_linux() {
    let _serial = TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let root = crate::admin::test_root::Root::new("registry-proxy");
    let previous = std::env::var_os("WINLUX_REGISTRY_PATH");
    std::env::set_var("WINLUX_REGISTRY_PATH", root.dir.join("registry.tsv"));
    // Proxy Linux → visible dans Internet Settings.
    crate::proxy::save(Some(&crate::proxy::Proxy {
        http: "proxy.lan:3128".into(),
        https: "proxy.lan:3128".into(),
        bypass: "<local>".into(),
    }))
    .unwrap();
    let seen = read(|d| Ok(proxy_values(d))).unwrap();
    assert_eq!(seen, (true, "proxy.lan:3128".into(), "<local>".into()));
    // Une appli Windows coupe le proxy → les applis Linux aussi.
    write(|d| {
        d.set(INTERNET_SETTINGS, "ProxyEnable", REG_DWORD, vec![0; 4]);
        Ok(())
    })
    .unwrap();
    assert_eq!(crate::proxy::load(), None);
    let env = std::fs::read_to_string(root.dir.join(crate::proxy::ENV_FILE)).unwrap();
    assert!(env.contains("unset http_proxy"));
    // Puis en règle un autre, par protocole.
    write(|d| {
        d.set(
            INTERNET_SETTINGS,
            "ProxyServer",
            REG_SZ,
            utf16z("http=a:1;https=b:2"),
        );
        d.set(
            INTERNET_SETTINGS,
            "ProxyEnable",
            REG_DWORD,
            vec![1, 0, 0, 0],
        );
        Ok(())
    })
    .unwrap();
    let p = crate::proxy::load().unwrap();
    assert_eq!((p.http.as_str(), p.https.as_str()), ("a:1", "b:2"));
    match previous {
        Some(v) => std::env::set_var("WINLUX_REGISTRY_PATH", v),
        None => std::env::remove_var("WINLUX_REGISTRY_PATH"),
    }
}
