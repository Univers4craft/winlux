//! Autorisations des applications Windows (.exe), comme sous Windows 11 :
//! la première fois qu'une application veut lire vos dossiers personnels,
//! utiliser la caméra, le micro ou capturer l'écran, Winlux pose la question
//! dans une fenêtre claire (`winlux-permission ask`). La réponse est
//! mémorisée par application et se révoque dans Paramètres > Confidentialité
//! et sécurité.
//!
//! - Réseau : autorisé sans question (Windows ne demande rien non plus), mais
//!   l'application apparaît dans la liste et peut y être bloquée. Les
//!   connexions entrantes restent du ressort du pare-feu (alerte de première
//!   écoute).
//! - Dossiers personnels : Bureau, Documents, Images, Musique, Vidéos et
//!   Téléchargements. Restent libres sans question : le dossier de
//!   l'application elle-même et les fichiers que l'utilisateur lui a donnés
//!   (double-clic, glisser-déposer : arguments de la ligne de commande).
//! - Les interrupteurs globaux de `/etc/winlux/privacy.conf` (CAMERA=deny,
//!   MICROPHONE=deny) l'emportent sur tout.
//! - Sans fenêtre de demande possible (pas d'affichage, outil absent), le
//!   comportement d'avant s'applique : accès autorisé, rien n'est mémorisé.
//!
//! Mémoire des choix : `~/.config/winlux/permissions.tsv`, une ligne par
//! décision : `capacité<TAB>allow|deny<TAB>chemin absolu du .exe`.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Capability {
    Files,
    Camera,
    Microphone,
    Screen,
    Network,
}

impl Capability {
    pub fn key(self) -> &'static str {
        match self {
            Capability::Files => "files",
            Capability::Camera => "camera",
            Capability::Microphone => "microphone",
            Capability::Screen => "screen",
            Capability::Network => "network",
        }
    }

    /// Le réseau est accordé d'office ; le reste est demandé.
    fn asks(self) -> bool {
        self != Capability::Network
    }

    fn global_key(self) -> Option<&'static str> {
        match self {
            Capability::Camera => Some("CAMERA"),
            Capability::Microphone => Some("MICROPHONE"),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
}

impl Decision {
    fn word(self) -> &'static str {
        match self {
            Decision::Allow => "allow",
            Decision::Deny => "deny",
        }
    }
}

struct App {
    exe: PathBuf,
    name: String,
    dir: PathBuf,
    personal: Vec<PathBuf>,
    granted: Vec<PathBuf>,
    trusted: bool,
}

static APP: OnceLock<App> = OnceLock::new();
/// Décisions de cette exécution (y compris un refus non mémorisé).
static SESSION: Mutex<Option<HashMap<Capability, bool>>> = Mutex::new(None);
/// Une seule fenêtre de demande à la fois par processus.
static ASKING: Mutex<()> = Mutex::new(());

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Chemin sans `.` ni `..`, sans toucher au disque.
pub fn normalize(p: &Path) -> PathBuf {
    let absolute = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(p)
    };
    let mut out = PathBuf::from("/");
    for c in absolute.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(n) => out.push(n),
            _ => {}
        }
    }
    out
}

/// Dossiers personnels de l'utilisateur (`user-dirs.dirs`, sinon les noms
/// usuels français et anglais présents dans le dossier personnel).
pub fn personal_dirs(home: &Path, user_dirs: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for line in user_dirs.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !matches!(
            key,
            "XDG_DESKTOP_DIR"
                | "XDG_DOCUMENTS_DIR"
                | "XDG_DOWNLOAD_DIR"
                | "XDG_MUSIC_DIR"
                | "XDG_PICTURES_DIR"
                | "XDG_VIDEOS_DIR"
        ) {
            continue;
        }
        let value = value.trim().trim_matches('"');
        let path = match value.strip_prefix("$HOME") {
            Some(rest) => home.join(rest.trim_start_matches('/')),
            None => PathBuf::from(value),
        };
        // « XDG_DESKTOP_DIR=$HOME » désactive un dossier : jamais tout le dossier personnel.
        let path = normalize(&path);
        if path != normalize(home) && !dirs.contains(&path) {
            dirs.push(path);
        }
    }
    for name in [
        "Bureau",
        "Desktop",
        "Documents",
        "Images",
        "Pictures",
        "Musique",
        "Music",
        "Vidéos",
        "Videos",
        "Téléchargements",
        "Downloads",
    ] {
        let path = normalize(&home.join(name));
        if path.is_dir() && !dirs.contains(&path) {
            dirs.push(path);
        }
    }
    dirs
}

fn store_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("WINLUX_PERMISSIONS_FILE") {
        return Some(PathBuf::from(p));
    }
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(config.join("winlux/permissions.tsv"))
}

/// Décision mémorisée pour cette application et cette capacité.
pub fn stored_in(text: &str, exe: &Path, cap: Capability) -> Option<Decision> {
    let exe = exe.to_string_lossy();
    text.lines().rev().find_map(|line| {
        let mut f = line.splitn(3, '\t');
        let (c, d, p) = (f.next()?, f.next()?, f.next()?);
        if c != cap.key() || p != exe {
            return None;
        }
        match d {
            "allow" => Some(Decision::Allow),
            "deny" => Some(Decision::Deny),
            _ => None,
        }
    })
}

/// Contenu du fichier après mémorisation (l'ancienne ligne est remplacée).
pub fn remembered(text: &str, exe: &Path, cap: Capability, decision: Decision) -> String {
    let exe = exe.to_string_lossy();
    let mut out: String = text
        .lines()
        .filter(|line| {
            let mut f = line.splitn(3, '\t');
            !(f.next() == Some(cap.key()) && f.nth(1) == Some(exe.as_ref()))
        })
        .map(|l| format!("{l}\n"))
        .collect();
    out.push_str(&format!("{}\t{}\t{exe}\n", cap.key(), decision.word()));
    out
}

fn remember(exe: &Path, cap: Capability, decision: Decision) {
    let Some(path) = store_path() else { return };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    if stored_in(&text, exe, cap) == Some(decision) {
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    if std::fs::write(&tmp, remembered(&text, exe, cap, decision)).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Interrupteur global de Paramètres > Confidentialité (« bloqué pour tous »).
pub fn globally_denied(cap: Capability) -> bool {
    let Some(key) = cap.global_key() else {
        return false;
    };
    let path = std::env::var_os("WINLUX_PRIVACY_CONF")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/winlux/privacy.conf"));
    std::fs::read_to_string(path).is_ok_and(|conf| {
        conf.lines()
            .any(|l| l.trim() == format!("{key}=deny").as_str())
    })
}

/// Programmes livrés avec Winlux (bureau, outils de test) : jamais de question.
fn is_trusted(exe: &Path) -> bool {
    if std::env::var("WINLUX_PERMISSIONS").as_deref() == Ok("off") {
        return true;
    }
    exe.starts_with("/opt/winlux") || exe.starts_with("/usr/lib/winlux")
}

/// Appelé au lancement du .exe : identité de l'application et fichiers
/// donnés par l'utilisateur sur la ligne de commande.
pub fn init(exe: &Path, args: &[String]) {
    let exe = std::fs::canonicalize(exe).unwrap_or_else(|_| normalize(exe));
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/root"));
    let user_dirs =
        std::fs::read_to_string(home.join(".config/user-dirs.dirs")).unwrap_or_default();
    let mut granted = Vec::new();
    for arg in args {
        let arg = arg.trim_matches('"');
        let looks_windows = arg.len() > 2 && arg.as_bytes()[1] == b':';
        if !(arg.starts_with('/') || looks_windows) {
            continue;
        }
        // APP n'est pas encore posé : la conversion ne déclenche aucune question.
        let path = normalize(&if looks_windows {
            crate::paths::to_linux(arg)
        } else {
            PathBuf::from(arg)
        });
        if path.is_dir() {
            granted.push(path);
        } else if let Some(parent) = path.parent() {
            // Une visionneuse ouvre aussi les fichiers voisins (suivant, précédent).
            granted.push(parent.to_path_buf());
        }
    }
    let _ = APP.set(App {
        name: exe
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        dir: exe.parent().map(Path::to_path_buf).unwrap_or_default(),
        trusted: is_trusted(&exe),
        personal: personal_dirs(&home, &user_dirs),
        exe,
        granted,
    });
}

/// Ouvre la fenêtre de demande. `None` : impossible de demander.
fn ask(app: &App, cap: Capability) -> Option<Option<Decision>> {
    let helper = std::env::var_os("WINLUX_PERMISSION_HELPER")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/bin/winlux-permission"));
    if !helper.is_file() || std::env::var_os("DISPLAY").is_none() {
        return None;
    }
    let status = std::process::Command::new(&helper)
        .arg("ask")
        .arg(cap.key())
        .arg(&app.name)
        .arg(&app.exe)
        .stdin(std::process::Stdio::null())
        .status()
        .ok()?;
    Some(match status.code() {
        Some(0) => Some(Decision::Allow),
        Some(1) => Some(Decision::Deny),
        // Fenêtre fermée ou délai dépassé : refus pour cette fois seulement.
        _ => None,
    })
}

/// L'application lancée a-t-elle droit à cette capacité ? Peut ouvrir la
/// fenêtre de demande (le fil appelant attend la réponse).
pub fn allowed(cap: Capability) -> bool {
    if globally_denied(cap) {
        return false;
    }
    let Some(app) = APP.get() else {
        return true;
    };
    if app.trusted {
        return true;
    }
    if let Some(&known) = lock(&SESSION).as_ref().and_then(|m| m.get(&cap)) {
        return known;
    }
    let _asking = lock(&ASKING);
    if let Some(&known) = lock(&SESSION).as_ref().and_then(|m| m.get(&cap)) {
        return known;
    }
    let text = store_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    let result = match stored_in(&text, &app.exe, cap) {
        Some(d) => d == Decision::Allow,
        None if !cap.asks() => {
            remember(&app.exe, cap, Decision::Allow);
            true
        }
        None => match ask(app, cap) {
            Some(Some(d)) => {
                remember(&app.exe, cap, d);
                d == Decision::Allow
            }
            Some(None) => false,
            None => true,
        },
    };
    lock(&SESSION)
        .get_or_insert_with(HashMap::new)
        .insert(cap, result);
    result
}

/// Règle des dossiers personnels, sans état global (testable).
pub fn personal_access_needs_permission(
    path: &Path,
    personal: &[PathBuf],
    app_dir: &Path,
    granted: &[PathBuf],
) -> bool {
    let path = normalize(path);
    let Some(folder) = personal.iter().find(|d| path.starts_with(d)) else {
        return false;
    };
    // Application rangée dans ce dossier (ex. Téléchargements\Setup.exe).
    if app_dir.starts_with(folder) && path.starts_with(app_dir) {
        return false;
    }
    !granted.iter().any(|g| path.starts_with(g))
}

/// Accès d'un .exe à un chemin Linux (appelé par la conversion des chemins).
pub fn path_allowed(path: &Path) -> bool {
    let Some(app) = APP.get() else {
        return true;
    };
    if app.trusted || !personal_access_needs_permission(path, &app.personal, &app.dir, &app.granted)
    {
        return true;
    }
    allowed(Capability::Files)
}

/// Chemin inexistant renvoyé à la place d'un fichier refusé : l'application
/// reçoit « fichier introuvable » et ne peut rien y créer.
pub fn denied_path(path: &Path) -> PathBuf {
    let mut p = PathBuf::from("/proc/winlux-permission-denied");
    if let Some(name) = path.file_name() {
        p.push(name);
    }
    p
}

/// Adresse d'une socket locale (boucle locale, socket Unix) : jamais demandée.
///
/// # Safety
/// `name` pointe sur `len` octets lisibles.
pub unsafe fn sockaddr_is_local(name: *const u8, len: i32) -> bool {
    if name.is_null() || len < 2 {
        return false;
    }
    let bytes = std::slice::from_raw_parts(name, len as usize);
    let family = u16::from_le_bytes([bytes[0], bytes[1]]);
    match family {
        1 => true,
        2 => bytes.len() >= 8 && bytes[4] == 127,
        10 | 23 => bytes.len() >= 24 && bytes[8..23].iter().all(|&b| b == 0) && bytes[23] == 1,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions_are_remembered_per_app() {
        let a = Path::new("/home/a/Jeux/jeu.exe");
        let b = Path::new("/home/a/Téléchargements/setup.exe");
        let text = remembered("", a, Capability::Camera, Decision::Allow);
        let text = remembered(&text, b, Capability::Camera, Decision::Deny);
        assert_eq!(
            stored_in(&text, a, Capability::Camera),
            Some(Decision::Allow)
        );
        assert_eq!(
            stored_in(&text, b, Capability::Camera),
            Some(Decision::Deny)
        );
        assert_eq!(stored_in(&text, a, Capability::Microphone), None);
        // Changer d'avis remplace la ligne au lieu d'en ajouter une.
        let text = remembered(&text, a, Capability::Camera, Decision::Deny);
        assert_eq!(
            stored_in(&text, a, Capability::Camera),
            Some(Decision::Deny)
        );
        assert_eq!(text.lines().count(), 2);
        // Une ligne abîmée est ignorée.
        assert_eq!(
            stored_in(
                "camera\tpeut-etre\t/x.exe\n",
                Path::new("/x.exe"),
                Capability::Camera
            ),
            None
        );
    }

    #[test]
    fn personal_folders_rule() {
        let personal = vec![
            PathBuf::from("/home/a/Documents"),
            PathBuf::from("/home/a/Téléchargements"),
        ];
        let jeux = Path::new("/home/a/Jeux/Jeu");
        let need = |p: &str, dir: &Path, granted: &[PathBuf]| {
            personal_access_needs_permission(Path::new(p), &personal, dir, granted)
        };
        assert!(need("/home/a/Documents/budget.xlsx", jeux, &[]));
        assert!(need("/home/a/Jeux/Jeu/../../Documents/x", jeux, &[]));
        assert!(!need("/home/a/Jeux/Jeu/save.dat", jeux, &[]));
        assert!(!need("/home/a/.local/share/winlux/x", jeux, &[]));
        assert!(!need("/home/a/Documentsbis/x", jeux, &[]));
        // Installateur lancé depuis Téléchargements : son dossier reste libre.
        let dl = Path::new("/home/a/Téléchargements");
        assert!(!need("/home/a/Téléchargements/setup.cab", dl, &[]));
        assert!(need("/home/a/Documents/x", dl, &[]));
        // Fichier ouvert par double-clic : son dossier est accordé.
        let granted = vec![PathBuf::from("/home/a/Documents/Photos")];
        assert!(!need("/home/a/Documents/Photos/2.jpg", jeux, &granted));
        assert!(need("/home/a/Documents/autre.txt", jeux, &granted));
    }

    #[test]
    fn user_dirs_parsing() {
        let home = Path::new("/home/a");
        let dirs = personal_dirs(
            home,
            "# commentaire\nXDG_DOCUMENTS_DIR=\"$HOME/Documents\"\nXDG_DESKTOP_DIR=\"$HOME\"\nXDG_TEMPLATES_DIR=\"$HOME/Modèles\"\nXDG_PICTURES_DIR=\"/data/Photos\"\n",
        );
        assert!(dirs.contains(&PathBuf::from("/home/a/Documents")));
        assert!(dirs.contains(&PathBuf::from("/data/Photos")));
        assert!(!dirs.contains(&PathBuf::from("/home/a")));
        assert!(!dirs.iter().any(|d| d.ends_with("Modèles")));
    }

    #[test]
    fn local_sockets_are_never_asked() {
        let mut v4 = [0u8; 16];
        v4[0] = 2;
        v4[4] = 127;
        v4[7] = 1;
        assert!(unsafe { sockaddr_is_local(v4.as_ptr(), 16) });
        v4[4] = 93;
        assert!(!unsafe { sockaddr_is_local(v4.as_ptr(), 16) });
        let mut v6 = [0u8; 28];
        v6[0] = 23;
        v6[23] = 1;
        assert!(unsafe { sockaddr_is_local(v6.as_ptr(), 28) });
        v6[8] = 0x20;
        assert!(!unsafe { sockaddr_is_local(v6.as_ptr(), 28) });
    }
}
