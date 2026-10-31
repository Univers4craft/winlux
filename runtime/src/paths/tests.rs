//! Cas tordus de la correspondance Windows ↔ Linux.

use super::*;
use std::ffi::OsString;

/// Profil de test : dossier personnel, lecteur C:, dossiers XDG français et
/// registre des lecteurs isolés dans un dossier temporaire.
struct Env {
    _guard: std::sync::MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<OsString>)>,
    root: PathBuf,
    home: PathBuf,
}

impl Env {
    fn new(tag: &str) -> Env {
        let guard = crate::mounts::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("winlux-paths-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        for d in [
            "Mes documents",
            "Bureau",
            "Téléchargements",
            "Public",
            ".config",
        ] {
            std::fs::create_dir_all(home.join(d)).unwrap();
        }
        std::fs::write(
            home.join(".config/user-dirs.dirs"),
            "# généré par xdg-user-dirs-update\n\
             XDG_DESKTOP_DIR=\"$HOME/Bureau\"\n\
             XDG_DOCUMENTS_DIR=\"$HOME/Mes documents\"\n\
             XDG_DOWNLOAD_DIR=\"$HOME/Téléchargements\"\n\
             XDG_PUBLICSHARE_DIR=\"$HOME/Public\"\n\
             XDG_MUSIC_DIR=\"$HOME/\"\n",
        )
        .unwrap();
        let vars = [
            ("WINLUX_HOME", home.clone().into_os_string()),
            ("WINLUX_DRIVE_C", home.clone().into_os_string()),
            ("XDG_CONFIG_HOME", home.join(".config").into_os_string()),
            ("WINLUX_STATE_DIR", root.join("state").into_os_string()),
        ];
        let mut saved = Vec::new();
        for (k, v) in vars {
            saved.push((k, std::env::var_os(k)));
            std::env::set_var(k, v);
        }
        Env {
            _guard: guard,
            saved,
            root,
            home,
        }
    }

    fn profile(&self) -> String {
        profile_windows()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        for (k, v) in self.saved.drain(..) {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn noms_interdits_aller_retour() {
    for name in [
        "réunion 10:30.txt",
        "a*b?c\"d<e>f|g",
        "notes.",
        "fin ",
        "points...",
        "a\\b",
        "tab\tligne",
        "normal.txt",
        "日本語.txt",
        ".cache",
    ] {
        let w = win_name(name);
        assert!(
            !w.chars().any(|c| forbidden(c)),
            "{name:?} → {w:?} contient encore un caractère interdit"
        );
        assert!(
            !w.ends_with(['.', ' ']),
            "{w:?} finit par un point ou un espace"
        );
        assert_eq!(linux_name(&w), name);
    }
    assert_eq!(win_name("normal.txt"), "normal.txt");
    assert_eq!(win_name("."), ".");
    assert_eq!(win_name(".."), "..");
    // Un vrai caractère de la zone privée qui ne code rien reste tel quel.
    assert_eq!(linux_name("\u{F041}"), "\u{F041}");
}

#[test]
fn profil_et_dossiers_connus_suivent_xdg() {
    let env = Env::new("profil");
    let user = user_name();
    let docs = env.home.join("Mes documents");
    assert_eq!(
        to_linux(&format!("C:\\Users\\{user}\\Documents\\lettre.odt")),
        docs.join("lettre.odt")
    );
    // Casse, ancien nom « My Documents », séparateurs mélangés.
    assert_eq!(
        to_linux(&format!(
            "c:/USERS//{}\\my documents\\lettre.odt",
            user.to_uppercase()
        )),
        docs.join("lettre.odt")
    );
    assert_eq!(
        to_linux(&format!("C:\\Users\\{user}\\Desktop")),
        env.home.join("Bureau")
    );
    assert_eq!(
        to_linux(&format!("C:\\Users\\{user}\\Downloads\\x.zip")),
        env.home.join("Téléchargements/x.zip")
    );
    // Dossier XDG désactivé (= dossier personnel) : nom anglais par défaut.
    assert_eq!(
        to_linux(&format!("C:\\Users\\{user}\\Music")),
        env.home.join("Music")
    );
    assert_eq!(
        to_linux(&format!("C:\\Users\\{user}\\AppData\\Roaming\\App")),
        env.home.join("AppData/Roaming/App")
    );
    assert_eq!(to_linux(&format!("C:\\Users\\{user}")), env.home);
    assert_eq!(to_linux("C:\\Users\\Public\\x"), env.home.join("Public/x"));
    // Un autre compte n'est pas le dossier personnel.
    assert_eq!(
        to_linux("C:\\Users\\quelquun\\Documents"),
        env.home.join("Users/quelquun/Documents")
    );

    // Dans l'autre sens : forme profil pour les dossiers connus et AppData.
    let p = env.profile();
    assert_eq!(
        to_windows(&docs.join("lettre.odt")),
        format!("{p}\\Documents\\lettre.odt")
    );
    assert_eq!(
        to_windows(&env.home.join("Bureau")),
        format!("{p}\\Desktop")
    );
    assert_eq!(
        to_windows(&env.home.join("AppData/Local/Temp")),
        format!("{p}\\AppData\\Local\\Temp")
    );
    assert_eq!(to_windows(&env.home), "C:\\");
    assert_eq!(
        to_windows(&env.home.join("Program Files/App")),
        "C:\\Program Files\\App"
    );
    assert_eq!(to_windows(Path::new("/usr/share")), "Z:\\usr\\share");
    assert_eq!(to_windows(Path::new("/")), "Z:\\");

    let vars = windows_env();
    let get = |k: &str| vars.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    assert_eq!(get("USERPROFILE"), Some(p.clone()));
    assert_eq!(
        get("HOMEPATH"),
        Some(format!("\\Users\\{}", win_name(&user)))
    );
    assert_eq!(get("APPDATA"), Some(format!("{p}\\AppData\\Roaming")));
}

#[test]
fn aller_retour_avec_noms_interdits_et_casse() {
    let env = Env::new("noms");
    let docs = env.home.join("Mes documents");
    let bizarre = docs.join("Réunion 10:30?.txt");
    std::fs::write(&bizarre, b"ordre du jour").unwrap();
    std::fs::write(docs.join("notes."), b"x").unwrap();

    let w = to_windows(&bizarre);
    assert!(!w[2..].contains([':', '?']), "{w}");
    assert_eq!(to_linux(&w), bizarre);
    // Insensible à la casse, même avec des noms codés.
    assert_eq!(to_linux(&w.to_uppercase().replace("C:", "c:")), bizarre);

    let wn = to_windows(&docs.join("notes."));
    assert!(!wn.ends_with('.'));
    assert_eq!(to_linux(&wn), docs.join("notes."));
    // Sans le point final codé, Windows retire le point : c'est « notes ».
    assert_eq!(
        to_linux(&format!("{}\\Documents\\notes.", env.profile())),
        docs.join("notes")
    );
}

#[test]
fn casse_ambigue_prefere_le_nom_exact_puis_l_ordre_alphabetique() {
    let env = Env::new("casse");
    let d = env.home.join("Projets");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("README"), b"1").unwrap();
    std::fs::write(d.join("Readme"), b"2").unwrap();
    assert_eq!(to_linux("C:\\Projets\\Readme"), d.join("Readme"));
    assert_eq!(to_linux("C:\\Projets\\README"), d.join("README"));
    assert_eq!(to_linux("C:\\projets\\readme"), d.join("README"));
    // Création : le dossier prend la bonne casse, le nouveau nom reste tel quel.
    assert_eq!(to_linux("C:\\PROJETS\\Nouveau.TXT"), d.join("Nouveau.TXT"));
    // Dossier intermédiaire absent : le chemin est rendu tel quel.
    assert_eq!(to_linux("C:\\absent\\x"), env.home.join("absent/x"));
}

#[test]
fn normalisation_win32() {
    let env = Env::new("norm");
    let h = &env.home;
    assert_eq!(to_linux("C:\\a\\.\\b\\..\\c"), h.join("a/c"));
    assert_eq!(to_linux("C:\\..\\..\\a"), h.join("a"));
    assert_eq!(
        to_linux("C:\\dossier.\\fichier.txt . "),
        h.join("dossier/fichier.txt")
    );
    assert_eq!(to_linux("C:/a//b///c"), h.join("a/b/c"));
    assert_eq!(
        to_linux("Z:\\etc\\hostname"),
        PathBuf::from("/etc/hostname")
    );
    // \\?\ : aucune normalisation, les chemins longs passent.
    assert_eq!(to_linux("\\\\?\\C:\\a.\\b"), h.join("a./b"));
    assert_eq!(to_linux("\\??\\C:\\x"), h.join("x"));
    // Périphériques.
    assert_eq!(to_linux("NUL"), PathBuf::from("/dev/null"));
    assert_eq!(to_linux("nul:"), PathBuf::from("/dev/null"));
    assert_eq!(to_linux("C:\\Temp\\NUL"), PathBuf::from("/dev/null"));
    assert_eq!(to_linux("\\\\.\\NUL"), PathBuf::from("/dev/null"));
    assert_eq!(to_linux("CONOUT$"), PathBuf::from("/dev/stdout"));
    assert_eq!(to_linux("\\\\.\\C:\\x"), h.join("x"));
    assert_eq!(to_linux(""), PathBuf::new());
    // Lecteur inconnu et partage inconnu : introuvables, jamais C:.
    assert!(to_linux("Q:\\x").starts_with(ABSENT));
    assert!(to_linux("\\\\serveur\\partage\\x").starts_with(ABSENT));
    // \\localhost\c$ = C:.
    assert_eq!(to_linux("\\\\localhost\\C$\\a\\b"), h.join("a/b"));
    assert_eq!(to_linux("\\\\?\\UNC\\127.0.0.1\\c$\\a"), h.join("a"));
}

#[test]
fn partage_reseau_monte() {
    let env = Env::new("unc");
    let mp = env.root.join("mnt-nas");
    std::fs::create_dir_all(mp.join("Photos")).unwrap();
    crate::mounts::add(crate::mounts::Mount {
        letter: 'Y',
        kind: "network".into(),
        mountpoint: mp.clone(),
        label: "NAS".into(),
        read_only: false,
        source: "smb://damien@nas:445/partage".into(),
        device: String::new(),
        backend: "rclone".into(),
        state: String::new(),
    })
    .unwrap();
    assert_eq!(to_linux("\\\\NAS\\Partage\\photos"), mp.join("Photos"));
    assert_eq!(to_linux("Y:\\Photos"), mp.join("Photos"));
    assert_eq!(to_windows(&mp.join("Photos")), "Y:\\Photos");
    assert!(to_linux("\\\\nas\\autre\\x").starts_with(ABSENT));
}

#[test]
fn liens_symboliques() {
    let env = Env::new("liens");
    let docs = env.home.join("Mes documents");
    let ailleurs = env.root.join("disque-data/projets");
    std::fs::create_dir_all(&ailleurs).unwrap();
    std::fs::write(ailleurs.join("plan.txt"), b"x").unwrap();
    std::os::unix::fs::symlink(&ailleurs, docs.join("Projets")).unwrap();
    std::os::unix::fs::symlink(env.root.join("absent"), docs.join("cassé")).unwrap();

    // Le chemin reste celui que l'appli a utilisé (le lien n'est pas suivi).
    let w = to_windows(&docs.join("Projets/plan.txt"));
    assert_eq!(
        w,
        format!("{}\\Documents\\Projets\\plan.txt", env.profile())
    );
    assert_eq!(to_linux(&w), docs.join("Projets/plan.txt"));
    // `..` est résolu sur le texte, comme sous Windows.
    assert_eq!(
        to_linux(&format!("{}\\Documents\\Projets\\..\\x", env.profile())),
        docs.join("x")
    );
    // Un lien cassé est trouvé (on peut le supprimer), même en changeant la casse.
    assert_eq!(
        to_linux(&format!("{}\\Documents\\CASSÉ", env.profile())),
        docs.join("cassé")
    );
    // Le dossier personnel atteint par un lien est reconnu.
    let alias = env.root.join("alias-home");
    std::os::unix::fs::symlink(&env.home, &alias).unwrap();
    std::env::set_var("WINLUX_HOME", &alias);
    std::env::set_var("WINLUX_DRIVE_C", &alias);
    assert_eq!(
        to_windows(&env.home.join("Program Files")),
        "C:\\Program Files"
    );
}

#[test]
fn chemins_longs() {
    let env = Env::new("long");
    let mut w = String::from("\\\\?\\C:\\long");
    let mut linux = env.home.join("long");
    for i in 0..12 {
        let part = format!("dossier-numero-{i:02}-avec-un-nom-assez-long");
        w.push('\\');
        w.push_str(&part);
        linux.push(&part);
    }
    assert!(w.len() > 260);
    let p = to_linux(&w);
    assert_eq!(p, linux);
    std::fs::create_dir_all(&p).unwrap();
    std::fs::write(p.join("f.txt"), b"ok").unwrap();
    // Sans le préfixe aussi : Winlux n'impose pas la limite MAX_PATH.
    let sans = format!("{}\\f.txt", &w[4..]);
    assert_eq!(std::fs::read(to_linux(&sans)).unwrap(), b"ok");
    assert_eq!(to_linux(&to_windows(&p.join("f.txt"))), p.join("f.txt"));
}

#[test]
fn chemin_complet_calcule_sur_le_texte() {
    let _env = Env::new("full");
    assert_eq!(full_windows("C:\\a\\..\\b\\.\\c"), "C:\\b\\c");
    assert_eq!(full_windows("c:/a//b/"), "c:\\a\\b\\".replacen('c', "C", 1));
    assert_eq!(full_windows("C:\\"), "C:\\");
    assert_eq!(full_windows("C:\\fichier.txt. "), "C:\\fichier.txt");
    assert_eq!(full_windows("\\\\?\\C:\\a\\..\\b"), "\\\\?\\C:\\a\\..\\b");
    assert_eq!(
        full_windows("\\\\serveur\\partage\\a\\..\\b"),
        "\\\\serveur\\partage\\b"
    );
    assert_eq!(full_windows("D:x\\y"), "D:\\x\\y");
    assert_eq!(full_windows("C:\\Temp\\nul"), "\\\\.\\nul");
    let rel = full_windows("sous\\..\\f.txt");
    assert!(
        rel.ends_with("\\f.txt") && rel.as_bytes()[1] == b':',
        "{rel}"
    );
}

#[test]
fn user_dirs_parse() {
    let h = Path::new("/home/u");
    let t = "XDG_DOCUMENTS_DIR=\"$HOME/Documents\"\nXDG_DESKTOP_DIR = \"/srv/bureau\"\nXDG_X_DIR=\"$HOME\"\n";
    assert_eq!(
        parse_user_dirs(t, "XDG_DOCUMENTS_DIR", h),
        Some(h.join("Documents"))
    );
    assert_eq!(
        parse_user_dirs(t, "XDG_DESKTOP_DIR", h),
        Some(PathBuf::from("/srv/bureau"))
    );
    assert_eq!(parse_user_dirs(t, "XDG_X_DIR", h), None);
    assert_eq!(parse_user_dirs(t, "XDG_MUSIC_DIR", h), None);
}
