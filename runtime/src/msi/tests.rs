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
    let uninstall =
        "hklm\\software\\microsoft\\windows\\currentversion\\uninstall\\{6a1b5c0e-1111-4222-8333-944455556666}";
    assert_eq!(
        utf16(&reg(uninstall, "DisplayName").unwrap()),
        "Winlux Exemple"
    );
    assert_eq!(utf16(&reg(uninstall, "DisplayVersion").unwrap()), "1.2.3");
    assert!(utf16(&reg(uninstall, "UninstallString").unwrap()).starts_with("MsiExec.exe /X{"));

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
    assert!(reg(uninstall, "DisplayName").is_none());
    assert!(install::Manifest::all().is_empty());
    assert_eq!(
        cli(&["/x".into(), "{00000000-0000-0000-0000-000000000000}".into()]),
        1605
    );
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
