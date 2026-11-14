//! Paquet réel produit par msitools (`testdata/sample.wxs`) : MSZIP sur
//! plusieurs blocs, registre, raccourci, fonctionnalité de niveau 2.

use super::*;
use std::ffi::OsString;

const SAMPLE: &[u8] = include_bytes!("testdata/sample.msi");

fn donnees() -> String {
    (0..2500)
        .map(|i| format!("ligne {} : Winlux installe ce paquet\n", i * 7919 % 10007))
        .collect()
}

struct Env {
    saved: Vec<(&'static str, Option<OsString>)>,
    root: PathBuf,
}

impl Env {
    fn new(tag: &str) -> Env {
        let root = std::env::temp_dir().join(format!("winlux-msi-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("c")).unwrap();
        let vars = [
            ("WINLUX_DRIVE_C", root.join("c")),
            // C: et le profil (C:\Users\<compte>) sont le même dossier,
            // comme sur une installation (drive_c vaut home par défaut).
            ("WINLUX_HOME", root.join("c")),
            ("XDG_DATA_HOME", root.join("data")),
            ("XDG_CONFIG_HOME", root.join("config")),
            ("WINLUX_REGISTRY_PATH", root.join("registry.tsv")),
        ];
        let mut saved = Vec::new();
        for (k, v) in vars {
            saved.push((k, std::env::var_os(k)));
            std::env::set_var(k, v);
        }
        Env { saved, root }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        for (k, v) in &self.saved {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

#[test]
fn lit_les_tables_du_paquet() {
    let db = db::Database::parse(SAMPLE).unwrap();
    assert_eq!(
        db.property("ProductName").as_deref(),
        Some("Winlux Exemple")
    );
    assert_eq!(
        db.property("Manufacturer").as_deref(),
        Some("Équipe Winlux")
    );
    assert!(db.is_64bit());
    assert_eq!(db.word_count & 2, 2);
    let names: Vec<String> = db.rows("File").map(|r| r.str("FileName")).collect();
    assert!(names.iter().any(|n| n.ends_with("données.bin")));
    let cab = cab::Cabinet::parse(&db.streams["donnees.cab"]).unwrap();
    let mut found = 0;
    cab.extract(&mut |name, data| {
        if name == "donnees" {
            assert_eq!(data, donnees().as_bytes());
        }
        found += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(found, 4);
}

#[test]
fn installe_puis_desinstalle() {
    let _a = crate::mounts::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _b = crate::dlls::advapi32::REGISTRY_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let env = Env::new("install");
    let msi = env.root.join("Exemple.msi");
    std::fs::write(&msi, SAMPLE).unwrap();
    let code = cli(&[
        "/i".into(),
        msi.to_string_lossy().into_owned(),
        "/qn".into(),
    ]);
    assert_eq!(code, 0);
    let dir = env.root.join("c/Program Files/Winlux Exemple");
    assert_eq!(
        std::fs::read(dir.join("app.exe")).unwrap(),
        b"MZ-winlux-test"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("données.bin")).unwrap(),
        donnees()
    );
    assert!(dir.join("Lisez-moi.txt").is_file());
    assert!(
        !dir.join("option.txt").exists(),
        "fonctionnalité de niveau 2"
    );
    let lnk = env.root.join(
        "c/AppData/Roaming/Microsoft/Windows/Start Menu/Programs/Winlux Exemple/Winlux Exemple.lnk",
    );
    let lnk = std::fs::read(lnk).unwrap();
    let text = String::from_utf16_lossy(
        &lnk[76..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect::<Vec<_>>(),
    );
    assert!(text.contains("C:\\Program Files\\Winlux Exemple\\app.exe"));
    assert!(text.contains("--demo"));
    let reg = |k: &str, n: &str| crate::dlls::advapi32::read_registry_value_internal(k, n);
    let chemin = reg("hklm\\software\\winlux exemple", "Chemin").unwrap();
    assert_eq!(utf16(&chemin), "C:\\Program Files\\Winlux Exemple\\");
    assert_eq!(
        reg("hklm\\software\\winlux exemple", "Niveau").unwrap(),
        7u32.to_le_bytes()
    );
    // Casse des clés conservée (#830), clé parente créée par le paquet.
    let names = crate::registry::read(|d| Ok(d.subkeys("hklm\\software\\winlux editeur"))).unwrap();
    assert!(
        names.iter().any(|n| n.ends_with("Reglages Exemple")),
        "{names:?}"
    );
    let uninstall =
        "hklm\\software\\microsoft\\windows\\currentversion\\uninstall\\{6a1b5c0e-1111-4222-8333-944455556666}";
    assert_eq!(
        utf16(&reg(uninstall, "DisplayName").unwrap()),
        "Winlux Exemple"
    );
    assert_eq!(utf16(&reg(uninstall, "DisplayVersion").unwrap()), "1.2.3");
    assert!(utf16(&reg(uninstall, "UninstallString").unwrap()).starts_with("MsiExec.exe /X{"));
    // Composants retrouvables par un autre paquet (CompLocator, #1598) :
    // pip.msi de Python cherche python.exe posé par exe.msi.
    let cle = install::component_key_path("{11111111-2222-4333-8444-555555555555}");
    assert_eq!(
        cle.as_deref(),
        Some("C:\\Program Files\\Winlux Exemple\\app.exe")
    );
    let cle = cle.unwrap();
    assert_eq!(
        extra::component_search(&cle, 1, Some("APP.EXE")).as_deref(),
        Some("C:\\Program Files\\Winlux Exemple\\app.exe")
    );
    assert_eq!(
        extra::component_search(&cle, 1, Some("Lisez-moi.txt")).as_deref(),
        Some("C:\\Program Files\\Winlux Exemple\\Lisez-moi.txt")
    );
    assert_eq!(
        extra::component_search(&cle, 0, None).as_deref(),
        Some("C:\\Program Files\\Winlux Exemple\\")
    );
    assert!(extra::component_search(&cle, 1, Some("absent.exe")).is_none());
    // Le composant du raccourci a une clé de registre pour chemin clé.
    assert!(install::component_key_path("{33333333-2222-4333-8444-555555555555}").is_none());

    // Réinstaller avec INSTALLLEVEL=2 remplace la version en place.
    let code = cli(&[
        "/i".into(),
        msi.to_string_lossy().into_owned(),
        "/quiet".into(),
        "INSTALLLEVEL=2".into(),
    ]);
    assert_eq!(code, 0);
    assert!(dir.join("option.txt").is_file());
    assert_eq!(install::Manifest::all().len(), 1);

    let code = cli(&[
        "/x".into(),
        "{6A1B5C0E-1111-4222-8333-944455556666}".into(),
        "/qn".into(),
    ]);
    assert_eq!(code, 0);
    assert!(!dir.exists(), "dossier d'installation retiré");
    assert!(reg("hklm\\software\\winlux exemple", "Chemin").is_none());
    // Clés créées par le paquet retirées, parent compris (#829).
    assert!(!crate::dlls::advapi32::registry_key_exists(
        "hklm\\software\\winlux editeur"
    ));
    assert!(reg(uninstall, "DisplayName").is_none());
    assert!(install::Manifest::all().is_empty());
    assert!(install::component_key_path("{11111111-2222-4333-8444-555555555555}").is_none());
    assert_eq!(
        cli(&["/x".into(), "{00000000-0000-0000-0000-000000000000}".into()]),
        1605
    );
}

/// #2315 : retirer un produit laisse en place un fichier qu'un autre produit
/// installé a posé au même chemin (compteur de clients des composants).
#[test]
fn desinstaller_garde_les_fichiers_d_un_autre_produit() {
    let _a = crate::mounts::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _b = crate::dlls::advapi32::REGISTRY_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let env = Env::new("partage");
    let msi = env.root.join("Exemple.msi");
    std::fs::write(&msi, SAMPLE).unwrap();
    let code = cli(&[
        "/i".into(),
        msi.to_string_lossy().into_owned(),
        "/qn".into(),
    ]);
    assert_eq!(code, 0);
    let dir = env.root.join("c/Program Files/Winlux Exemple");
    // Un second produit a posé app.exe au même chemin (casse différente).
    let autre = install::Manifest {
        product: "{0B0B0B0B-1111-4222-8333-944455556666}".into(),
        name: "Autre produit".into(),
        files: vec![dir.join("APP.exe")],
        ..Default::default()
    };
    autre.save().unwrap();
    let code = cli(&[
        "/x".into(),
        "{6A1B5C0E-1111-4222-8333-944455556666}".into(),
        "/qn".into(),
    ]);
    assert_eq!(code, 0);
    assert!(dir.join("app.exe").is_file(), "fichier partagé conservé");
    assert!(!dir.join("Lisez-moi.txt").exists(), "fichier propre retiré");
    assert_eq!(install::Manifest::all().len(), 1);
}

#[test]
fn installation_administrative_et_erreurs() {
    let _a = crate::mounts::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _b = crate::dlls::advapi32::REGISTRY_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let env = Env::new("admin");
    let msi = env.root.join("Exemple.msi");
    std::fs::write(&msi, SAMPLE).unwrap();
    let target = env.root.join("extrait");
    let code = cli(&[
        "/a".into(),
        msi.to_string_lossy().into_owned(),
        format!("TARGETDIR={}\\extrait", crate::paths::to_windows(&env.root)),
        "/qn".into(),
    ]);
    assert_eq!(code, 0);
    assert!(target.join("Winlux Exemple/app.exe").is_file());
    assert!(install::Manifest::all().is_empty());
    let bad = env.root.join("abime.msi");
    std::fs::write(&bad, b"pas un msi").unwrap();
    assert_eq!(
        cli(&[
            "/i".into(),
            bad.to_string_lossy().into_owned(),
            "/qn".into()
        ]),
        1620
    );
    assert_eq!(
        cli(&["/i".into(), "/introuvable.msi".into(), "/qn".into()]),
        1619
    );
    assert_eq!(cli(&["/z".into()]), 1639);
    // Correctif .msp : refusé en clair dans le journal, rien n'est touché (#1715).
    let journal = env.root.join("correctif.log");
    assert_eq!(
        cli(&[
            "/p".into(),
            "maj.msp".into(),
            "/qn".into(),
            "/l*v".into(),
            journal.to_string_lossy().into_owned()
        ]),
        1637
    );
    let texte = std::fs::read_to_string(&journal).unwrap();
    assert!(
        texte.contains("correctif .msp non pris en charge"),
        "{texte}"
    );
    assert_eq!(
        install::package_info(&msi).unwrap(),
        (
            "{6A1B5C0E-1111-4222-8333-944455556666}".to_string(),
            "Winlux Exemple".to_string(),
            "1.2.3".to_string()
        )
    );
}

#[test]
fn raccourci_lisible() {
    let lnk = install::shortcut_bytes("C:\\App\\app.exe", "--x", "C:\\App\\", "Démo");
    assert_eq!(&lnk[..4], &[0x4C, 0, 0, 0]);
    assert_eq!(lnk.len(), 76 + 2 + 8 + 2 + 28 + 2 + 14 + 2 + 6);
}

/// `testdata/tables.msi` : `tables.wxs` construit par wixl, puis les tables
/// de `testdata/tables/*.idt` ajoutées par `msibuild -i` (wixl ne sait pas
/// les écrire).
#[test]
fn tables_complementaires() {
    let _a = crate::mounts::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _b = crate::dlls::advapi32::REGISTRY_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let env = Env::new("tables");
    let set = |k: &str, n: &str, v: &str| crate::dlls::advapi32::RegEdit::Set {
        key: k.into(),
        name: n.into(),
        kind: 1,
        bytes: v
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect(),
    };
    assert!(crate::dlls::advapi32::apply_registry_edits(&[
        set("hklm\\software\\ancienne appli", "Dossier", "C:\\Ancien\\"),
        set("hklm\\software\\ancienne appli", "AEffacer", "x"),
        set("hkcu\\environment", "WINLUX_LISTE", "a"),
        set("hkcu\\environment", "WINLUX_VIEUX", "v"),
    ]));
    let c = env.root.join("c");
    std::fs::create_dir_all(c.join("windows")).unwrap();
    std::fs::write(c.join("windows/winlux.ini"), "[Plugins]\r\nListe=x\r\n").unwrap();
    let dir = c.join("Program Files/Winlux Tables");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("ancien.log"), "journal").unwrap();
    let msi = env.root.join("tables.msi");
    std::fs::write(&msi, include_bytes!("testdata/tables.msi")).unwrap();
    let code = cli(&[
        "/i".into(),
        msi.to_string_lossy().into_owned(),
        "/qn".into(),
    ]);
    assert_eq!(code, 0);
    let reg = |k: &str, n: &str| {
        crate::dlls::advapi32::read_registry_value_internal(k, n).map(|b| utf16(&b))
    };
    // AppSearch + IniFile.
    let ini = std::fs::read_to_string(dir.join("app.ini")).unwrap();
    assert!(ini.contains("Dossier=C:\\Program Files\\Winlux Tables\\"));
    assert!(ini.contains("Ancien=C:\\Ancien\\"), "{ini}");
    let shared = || std::fs::read_to_string(c.join("windows/winlux.ini")).unwrap();
    assert!(shared().contains("Liste=x,tables"));
    // Environment.
    assert_eq!(
        reg("hkcu\\environment", "WINLUX_TABLES").as_deref(),
        Some("C:\\Program Files\\Winlux Tables\\")
    );
    assert_eq!(
        reg("hkcu\\environment", "WINLUX_LISTE").as_deref(),
        Some("a;nouveau")
    );
    assert!(reg("hkcu\\environment", "WINLUX_VIEUX").is_none());
    // RemoveFile + RemoveRegistry.
    assert!(!dir.join("ancien.log").exists());
    assert!(reg("hklm\\software\\ancienne appli", "AEffacer").is_none());

    std::fs::write(dir.join("nouveau.log"), "journal").unwrap();
    let code = cli(&[
        "/x".into(),
        "{7B2C6D1F-2222-4333-8444-A55566667777}".into(),
        "/qn".into(),
    ]);
    assert_eq!(code, 0);
    assert!(!dir.join("app.ini").exists());
    assert!(
        !dir.join("nouveau.log").exists(),
        "RemoveFile à la désinstallation"
    );
    assert!(shared().contains("Liste=x\r\n"));
    assert!(reg("hkcu\\environment", "WINLUX_TABLES").is_none());
    assert_eq!(
        reg("hkcu\\environment", "WINLUX_LISTE").as_deref(),
        Some("a")
    );
    assert_eq!(
        reg("hkcu\\environment", "WINLUX_VIEUX").as_deref(),
        Some("v")
    );
}

/// #1789 : SystemFolder est toujours SysWOW64 sous Windows 64 bits, même
/// pour un paquet x64 ; seul System64Folder désigne System32.
#[test]
fn systemfolder_reste_syswow64_pour_un_paquet_x64() {
    let db = db::Database::parse(SAMPLE).unwrap();
    assert!(db.is_64bit());
    let mut ctx = cond::Context::default();
    install::system_props(&mut ctx, &db, &install::Options::default());
    assert_eq!(ctx.get("SystemFolder"), "C:\\windows\\syswow64\\");
    assert_eq!(ctx.get("System64Folder"), "C:\\windows\\system32\\");
    assert_eq!(ctx.get("ProgramFilesFolder"), "C:\\Program Files (x86)\\");
    assert_eq!(
        ctx.get("CommonFilesFolder"),
        "C:\\Program Files (x86)\\Common Files\\"
    );
    assert_eq!(ctx.get("ProgramFiles64Folder"), "C:\\Program Files\\");
}

/// #2404 : les dossiers du profil sont sous C:\Users\<compte>, comme sous
/// Windows ; TempFolder est ce que rend GetTempPath.
#[test]
fn dossiers_du_profil_sous_users_compte() {
    let db = db::Database::parse(SAMPLE).unwrap();
    let mut ctx = cond::Context::default();
    install::system_props(&mut ctx, &db, &install::Options::default());
    let profile = format!("{}\\", crate::paths::profile_windows());
    assert!(profile.starts_with("C:\\Users\\"), "{profile}");
    assert_eq!(ctx.get("TempFolder"), crate::dlls::kernel32::temp_path());
    assert_eq!(
        ctx.get("AppDataFolder"),
        format!("{profile}AppData\\Roaming\\")
    );
    assert_eq!(
        ctx.get("LocalAppDataFolder"),
        format!("{profile}AppData\\Local\\")
    );
    assert_eq!(ctx.get("PersonalFolder"), format!("{profile}Documents\\"));
    for name in [
        "TempFolder",
        "AppDataFolder",
        "LocalAppDataFolder",
        "PersonalFolder",
        "MyPicturesFolder",
        "FavoritesFolder",
        "StartMenuFolder",
        "ProgramMenuFolder",
        "StartupFolder",
        "AdminToolsFolder",
        "SendToFolder",
        "TemplateFolder",
        "RecentFolder",
        "NetHoodFolder",
        "PrintHoodFolder",
    ] {
        assert!(
            ctx.get(name).starts_with(&profile),
            "{name} = {}",
            ctx.get(name)
        );
    }
}

/// `testdata/licence.msi` : la boîte de licence est préférée au lisez-moi,
/// son RTF est rendu en texte (#2123) ; un paquet sans licence n'en a pas.
#[test]
fn contrat_de_licence_du_paquet() {
    let dir = std::env::temp_dir().join(format!("winlux-msi-licence-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let msi = dir.join("licence.msi");
    std::fs::write(&msi, include_bytes!("testdata/licence.msi")).unwrap();
    assert_eq!(
        crate::setup::license::of_package(&msi).as_deref(),
        Some("CONTRAT DE LICENCE DE TEST\nAccepter avant d\u{2019}installer. Café")
    );
    let sans = dir.join("sample.msi");
    std::fs::write(&sans, SAMPLE).unwrap();
    assert_eq!(crate::setup::license::of_package(&sans), None);
    let _ = std::fs::remove_dir_all(&dir);
}
