//! Autorisations des applications Windows (.exe). Lancer une application
//! installée ne pose jamais de question (règle de damien, #1733) : un
//! double-clic la lance. Le réseau, les dossiers personnels et le retrait du
//! bac à sable de Chromium sont accordés d'office, mémorisés par application
//! et révocables dans Paramètres > Confidentialité et sécurité ; la sécurité
//! vient du confinement que Winlux applique lui-même. Caméra, micro, capture
//! d'écran et saisie à la place de la personne sont demandés au moment de
//! leur usage, dans une fenêtre claire (`winlux-permission ask`).
//!
//! - Dossiers personnels : Bureau, Documents, Images, Musique, Vidéos et
//!   Téléchargements. Un refus mémorisé (Paramètres) est respecté ; restent
//!   alors libres le dossier de l'application et les fichiers que
//!   l'utilisateur lui a donnés (double-clic, glisser-déposer).
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
    /// Démarrer un programme Chromium/Electron sans son bac à sable
    /// (`--no-sandbox`), que Winlux ne sait pas encore fournir (#767).
    /// Accordé sans question : un double-clic lance l'application (règle de
    /// damien, #1733) ; ses processus enfants sont alors confinés par
    /// Winlux. Mémorisé et visible dans Paramètres, où un refus est respecté.
    Sandbox,
    /// Taper au clavier et piloter la souris à la place de la personne
    /// (SendInput, keybd_event, mouse_event ; macros des logiciels de
    /// périphériques, #1655). Jamais accordé sans un « Autoriser » explicite.
    Input,
}

impl Capability {
    pub fn key(self) -> &'static str {
        match self {
            Capability::Files => "files",
            Capability::Camera => "camera",
            Capability::Microphone => "microphone",
            Capability::Screen => "screen",
            Capability::Network => "network",
            Capability::Sandbox => "sandbox",
            Capability::Input => "input",
        }
    }

    /// Lancer une application installée ne pose jamais de question (#1733) :
    /// réseau, dossiers personnels et bac à sable sont accordés d'office,
    /// mémorisés et révocables dans Paramètres. Caméra, micro, capture
    /// d'écran et saisie à la place de la personne restent demandés au
    /// moment de leur usage.
    fn asks(self) -> bool {
        matches!(
            self,
            Capability::Camera | Capability::Microphone | Capability::Screen | Capability::Input
        )
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
    let personal = personal_dirs(&home, &user_dirs);
    let granted = granted_paths(args, std::env::current_dir().ok(), &personal);
    let _ = APP.set(App {
        name: exe
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        dir: exe.parent().map(Path::to_path_buf).unwrap_or_default(),
        trusted: is_trusted(&exe),
        personal,
        exe,
        granted,
    });
}

/// Ce que l'utilisateur a donné au programme en le lançant : les fichiers et
/// dossiers de la ligne de commande, et le dossier où il l'a lancé (#1019).
/// Lancé depuis un terminal ouvert sur le Bureau, claude.exe lit d'abord son
/// dossier courant (CreateFileW \\?\C:\Users\…\Bureau) : sans cela il
/// attendait l'autorisation « fichiers personnels » avant d'afficher quoi
/// que ce soit. Seul un dossier de lancement situé dans un dossier personnel
/// compte : lancé depuis le dossier personnel lui-même (cas courant), rien
/// n'est accordé et Bureau, Documents… restent protégés.
fn granted_paths(args: &[String], cwd: Option<PathBuf>, personal: &[PathBuf]) -> Vec<PathBuf> {
    let mut granted: Vec<PathBuf> = cwd
        .map(|c| normalize(&c))
        .filter(|c| personal.iter().any(|d| c.starts_with(d)))
        .into_iter()
        .collect();
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
    granted
}

/// Programme installé (Program Files, AppData) : son chemin Windows.
pub fn is_installed_program(exe_windows: &str) -> bool {
    let w = exe_windows.to_lowercase();
    let Some(rest) = w.strip_prefix("c:\\") else {
        return false;
    };
    if rest.starts_with("program files") {
        return true;
    }
    // C:\Users\<nom>\AppData\…
    let mut parts = rest.split('\\');
    parts.next() == Some("users") && parts.nth(1) == Some("appdata")
}

/// Au lancement d'un programme installé, ouvert par l'utilisateur (pas par
/// un autre programme Windows) : la question des dossiers personnels est
/// posée tout de suite, avant que l'application ne démarre, et non plus au
/// milieu de son démarrage (Claude pour Windows ouvrait sa fenêtre
/// pendant que la boîte attendait). Rien ne change si la réponse est déjà
/// connue.
pub fn ask_files_before_start(exe_windows: &str) {
    const PARENT: &str = "WINLUX_PARENT_PID";
    let launched_by_program = std::env::var_os(PARENT).is_some();
    // Les enfants de ce processus sauront qu'un programme les a lancés.
    std::env::set_var(PARENT, std::process::id().to_string());
    let Some(app) = APP.get() else {
        return;
    };
    if launched_by_program || app.trusted || is_service() || !is_installed_program(exe_windows) {
        return;
    }
    let text = store_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    if stored_in(&text, &app.exe, Capability::Files).is_none() {
        allowed(Capability::Files);
    }
}

/// Processus lancé par le gestionnaire de services (`WINLUX_SERVICE_NAME`,
/// retiré de l'environnement de ses enfants) : aucune fenêtre ne lui
/// appartient, personne n'attend devant lui.
fn is_service() -> bool {
    std::env::var_os("WINLUX_SERVICE_NAME").is_some_and(|n| !n.is_empty())
}

/// Ouvre la fenêtre de demande. `None` : impossible de demander.
/// `ask`, en notant le fil qui pose la question, pour la viser (#1882).
fn ask_logged(app: &App, cap: Capability) -> Option<Option<Decision>> {
    crate::log_info!(
        "permissions",
        "question « {} » posée depuis le fil {:?}",
        cap.key(),
        std::thread::current().name().unwrap_or("?")
    );
    ask(app, cap)
}

fn ask(app: &App, cap: Capability) -> Option<Option<Decision>> {
    // Un service sans fenêtre ne pose pas de question au démarrage de la
    // session (#1620) : refus, mémorisé pour qu'il apparaisse dans
    // Paramètres > Confidentialité et sécurité, où l'utilisateur peut
    // l'autoriser.
    if is_service() {
        crate::log_info!(
            "permissions",
            "service {} : accès « {} » refusé sans fenêtre, à autoriser dans Paramètres si besoin",
            app.name,
            cap.key()
        );
        return Some(Some(Decision::Deny));
    }
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

/// Décision déjà mémorisée pour l'application lancée (sans rien demander).
pub fn decided(cap: Capability) -> Option<Decision> {
    let app = APP.get()?;
    let text = store_path().and_then(|p| std::fs::read_to_string(p).ok())?;
    stored_in(&text, &app.exe, cap)
}

/// La capacité n'est-elle pas refusée, sans jamais rien demander ? Pour
/// énumérer les périphériques (nombre de micros, leurs caractéristiques) :
/// Steam les compte dès son démarrage, et la question « micro » s'ouvrait
/// au lancement, contre la règle #1733. La question reste posée au moment
/// d'ouvrir vraiment le périphérique, par `allowed` (#1882).
pub fn not_refused(cap: Capability) -> bool {
    if globally_denied(cap) {
        return false;
    }
    if let Some(&known) = lock(&SESSION).as_ref().and_then(|m| m.get(&cap)) {
        return known;
    }
    decided(cap) != Some(Decision::Deny)
}

/// L'application lancée a-t-elle droit à cette capacité ? Peut ouvrir la
/// fenêtre de demande (le fil appelant attend la réponse).
pub fn allowed(cap: Capability) -> bool {
    if globally_denied(cap) {
        return false;
    }
    // La saisie à la place de la personne ne s'accorde que sur un choix
    // explicite (#1655).
    let implied = cap != Capability::Input;
    let Some(app) = APP.get() else {
        return implied;
    };
    if app.trusted {
        return implied;
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
        None => match ask_logged(app, cap) {
            Some(Some(d)) => {
                remember(&app.exe, cap, d);
                d == Decision::Allow
            }
            Some(None) => false,
            // Impossible de demander (pas d'assistant ni d'écran) : ce qui
            // touche à la vie privée est refusé plutôt qu'accordé en silence.
            None => !matches!(
                cap,
                Capability::Camera
                    | Capability::Microphone
                    | Capability::Screen
                    | Capability::Input
            ),
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

/// Texte du message « accès aux fichiers refusé », dans la langue de
/// l'interface (fr, en, es, de).
pub fn denied_notice(app: &str, lang: &str) -> String {
    let t = match lang.get(..2) {
        Some("en") => "“{app}” cannot access your personal files. If the app does not work, allow it in Settings > Privacy & security.",
        Some("es") => "«{app}» no tiene acceso a sus archivos personales. Si la aplicación no funciona, permítalo en Configuración > Privacidad y seguridad.",
        Some("de") => "„{app}“ hat keinen Zugriff auf Ihre persönlichen Dateien. Wenn die App nicht funktioniert, erlauben Sie es unter Einstellungen > Datenschutz und Sicherheit.",
        _ => "« {app} » n’a pas accès à vos fichiers personnels. Si l’application ne fonctionne pas, autorisez-la dans Paramètres > Confidentialité et sécurité.",
    };
    t.replace("{app}", app)
}

pub(crate) fn interface_lang() -> String {
    if let Ok(l) = std::env::var("WINLUX_LANG") {
        if !l.is_empty() {
            return l;
        }
    }
    std::fs::read_to_string("/etc/winlux/locale.conf")
        .ok()
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("LANG=").map(str::to_string))
        })
        .or_else(|| std::env::var("LANG").ok())
        .unwrap_or_else(|| "fr".into())
}

/// Premier accès refusé de cette exécution : l'utilisateur est prévenu
/// (une application Electron à qui l'on refuse ses dossiers peut s'arrêter
/// au démarrage sans rien dire). Les enfants de Chromium (`--type=`) se
/// taisent : leur navigateur a déjà prévenu.
fn tell_denied_once() {
    static TOLD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if TOLD.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let Some(app) = APP.get() else {
        return;
    };
    if std::env::args().any(|a| a.starts_with("--type="))
        || std::env::var("WINLUX_COMMAND_LINE").is_ok_and(|l| l.contains(" --type="))
        || std::env::var_os("DISPLAY").is_none()
    {
        return;
    }
    let text = denied_notice(&app.name, &interface_lang());
    crate::log_warn!("permissions", "{text}");
    if let Ok(mut child) = std::process::Command::new("notify-send")
        .args(["-i", "folder-documents", "Winlux", &text])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

const DENIED_ROOT: &str = "/proc/winlux-permission-denied";

/// Chemin inexistant renvoyé à la place d'un fichier refusé : l'application
/// reçoit « fichier introuvable » et ne peut rien y créer.
pub fn denied_path(path: &Path) -> PathBuf {
    tell_denied_once();
    let mut p = PathBuf::from(DENIED_ROOT);
    if let Some(name) = path.file_name() {
        p.push(name);
    }
    p
}

/// Chemin produit par `denied_path` : l'accès a été refusé à l'application.
pub fn is_denied_path(path: &Path) -> bool {
    path.starts_with(DENIED_ROOT)
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
        // ::1, ou 127.x en IPv4 mappé (::ffff:127.0.0.0/8).
        10 | 23 => {
            bytes.len() >= 24
                && ((bytes[8..23].iter().all(|&b| b == 0) && bytes[23] == 1)
                    || (bytes[8..18].iter().all(|&b| b == 0)
                        && bytes[18] == 0xff
                        && bytes[19] == 0xff
                        && bytes[20] == 127))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn refus_explique_en_quatre_langues() {
        for lang in ["fr_FR.UTF-8", "en", "es_ES", "de_DE"] {
            let t = super::denied_notice("Claude", lang);
            assert!(t.contains("Claude") && t.contains('>'), "{lang}: {t}");
        }
        assert!(super::denied_notice("X", "de").contains("Einstellungen"));
    }

    #[test]
    fn programme_installe() {
        use super::is_installed_program as installed;
        assert!(installed(
            "C:\\Users\\damien\\AppData\\Local\\AnthropicClaude\\claude.exe"
        ));
        assert!(installed("C:\\Program Files\\7-Zip\\7zFM.exe"));
        assert!(!installed("C:\\Users\\damien\\Téléchargements\\Setup.exe"));
        assert!(!installed("Z:\\tmp\\test.exe"));
    }

    use super::*;

    /// Compter les micros ne demande rien : sans décision ni refus global,
    /// le micro n'est pas refusé (#1882).
    #[test]
    fn enumerer_le_micro_ne_demande_rien() {
        assert_eq!(
            not_refused(Capability::Microphone),
            !globally_denied(Capability::Microphone)
        );
    }

    /// Lancer une application ne pose pas de question (#1733) : bac à
    /// sable et dossiers personnels sont accordés d'office ; caméra, micro,
    /// écran et saisie restent demandés.
    #[test]
    fn lancement_sans_question() {
        if APP.get().is_none() {
            assert!(allowed(Capability::Sandbox));
            assert!(allowed(Capability::Files));
        }
        for cap in [Capability::Files, Capability::Sandbox, Capability::Network] {
            assert!(!cap.asks(), "{}", cap.key());
        }
        for cap in [
            Capability::Camera,
            Capability::Microphone,
            Capability::Screen,
            Capability::Input,
        ] {
            assert!(cap.asks(), "{}", cap.key());
        }
        assert_eq!(Capability::Sandbox.key(), "sandbox");
    }

    /// Taper à la place de la personne n'est jamais implicite (#1655).
    #[test]
    fn saisie_jamais_accordee_sans_reponse() {
        if APP.get().is_none() {
            assert!(!allowed(Capability::Input));
        }
        assert_eq!(Capability::Input.key(), "input");
    }

    /// Un service sans fenêtre n'ouvre jamais la demande : refus (#1620).
    #[test]
    fn un_service_ne_demande_pas() {
        let _g = crate::mounts::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let app = App {
            exe: PathBuf::from("/c/Program Files/Editeur/Service.exe"),
            name: "Service.exe".into(),
            dir: PathBuf::from("/c/Program Files/Editeur"),
            personal: Vec::new(),
            granted: Vec::new(),
            trusted: false,
        };
        let saved = std::env::var_os("WINLUX_SERVICE_NAME");
        std::env::set_var("WINLUX_SERVICE_NAME", "EditeurService");
        assert!(is_service());
        for cap in [
            Capability::Files,
            Capability::Camera,
            Capability::Microphone,
        ] {
            assert_eq!(ask(&app, cap), Some(Some(Decision::Deny)));
        }
        std::env::set_var("WINLUX_SERVICE_NAME", "");
        assert!(!is_service());
        match saved {
            Some(v) => std::env::set_var("WINLUX_SERVICE_NAME", v),
            None => std::env::remove_var("WINLUX_SERVICE_NAME"),
        }
    }

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
    fn launch_folder_and_arguments_are_granted() {
        let personal = vec![
            PathBuf::from("/home/a/Bureau"),
            PathBuf::from("/home/a/Documents"),
        ];
        let granted = granted_paths(
            &[
                "/home/a/Documents/lettre.txt".to_string(),
                "--version".to_string(),
            ],
            Some(PathBuf::from("/home/a/Bureau")),
            &personal,
        );
        let app = Path::new("/home/a/.local/share/winlux/claude-code");
        let need =
            |p: &str| personal_access_needs_permission(Path::new(p), &personal, app, &granted);
        // Dossier de lancement (#1019) et dossier du fichier donné : libres.
        assert!(!need("/home/a/Bureau"));
        assert!(!need("/home/a/Bureau/notes.txt"));
        assert!(!need("/home/a/Documents/autre.txt"));
        // Lancé depuis le dossier personnel lui-même : le Bureau reste protégé.
        let granted = granted_paths(&[], Some(PathBuf::from("/home/a")), &personal);
        assert!(granted.is_empty());
        assert!(personal_access_needs_permission(
            Path::new("/home/a/Bureau/x"),
            &personal,
            app,
            &granted
        ));
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
