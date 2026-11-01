//! winlux-remote : serveur de bureau à distance Winlux (protocole RFB/VNC
//! écrit maison). Il expose l'écran X11 du bureau et accepte le clavier et
//! la souris. La connexion est chiffrée (RSA-AES, compatible TigerVNC 1.13+
//! et RealVNC) et réservée aux comptes Winlux autorisés ; l'ancien mot de
//! passe VNC, non chiffré, n'est proposé que si l'administrateur l'active.
//!
//! Configuration : /etc/winlux/remote.conf (ENABLED, PORT, USERS,
//! LEGACY_VNC, VIEW_ONLY). Comptes : /etc/winlux/users.conf. État publié
//! pour les Paramètres : /run/winlux/remote-state.txt.

mod crypto;
mod rfb;
mod screen;

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crypto::RsaKey;

fn root() -> PathBuf {
    PathBuf::from(std::env::var("WINLUX_CONTROL_ROOT").unwrap_or_default() + "/")
}
fn path(rel: &str) -> PathBuf {
    root().join(rel)
}

#[derive(Clone, Debug, Default)]
struct Config {
    port: u16,
    users: Vec<String>,
    legacy: bool,
    view_only: bool,
}

fn conf_value(text: &str, key: &str) -> Option<String> {
    text.lines()
        .filter_map(|l| l.strip_prefix(key)?.strip_prefix('='))
        .map(|v| v.trim().to_string())
        .next_back()
}

fn load_config() -> Config {
    let text = std::fs::read_to_string(path("etc/winlux/remote.conf")).unwrap_or_default();
    Config {
        port: conf_value(&text, "PORT")
            .and_then(|p| p.parse().ok())
            .filter(|&p| p > 0)
            .unwrap_or(5900),
        users: conf_value(&text, "USERS")
            .unwrap_or_default()
            .split(',')
            .map(|u| u.trim().to_string())
            .filter(|u| !u.is_empty())
            .collect(),
        legacy: conf_value(&text, "LEGACY_VNC").as_deref() == Some("1"),
        view_only: conf_value(&text, "VIEW_ONLY").as_deref() == Some("1"),
    }
}

/// Compte Winlux autorisé : mot de passe exact, compte non vide, et soit
/// listé dans USERS, soit administrateur quand la liste est vide.
fn check_account(cfg: &Config, user: &str, password: &[u8]) -> bool {
    let users = std::fs::read_to_string(path("etc/winlux/users.conf")).unwrap_or_default();
    for line in users.lines() {
        let mut parts = line.splitn(3, ':');
        let (Some(name), Some(role), Some(stored)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if name != user {
            continue;
        }
        let allowed = if cfg.users.is_empty() {
            role == "root"
        } else {
            cfg.users.iter().any(|u| u == user)
        };
        // Compte du système installé : le mot de passe est celui de
        // /etc/shadow, vérifié par le relais root (winlux-root check).
        if stored == "shadow" {
            return allowed && !password.is_empty() && system_password_ok(user, password);
        }
        // Un compte sans mot de passe ne s'ouvre jamais à distance.
        let stored = stored.as_bytes();
        let same = stored.len() == password.len()
            && stored
                .iter()
                .zip(password)
                .fold(0u8, |d, (a, b)| d | (a ^ b))
                == 0;
        return allowed && stored != b"nopass" && !stored.is_empty() && same;
    }
    false
}

fn system_password_ok(user: &str, password: &[u8]) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let tool = std::env::var("WINLUX_ROOT_TOOL").unwrap_or_else(|_| "/usr/bin/winlux-root".into());
    let Ok(mut child) = Command::new(tool)
        .args(["check", user])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(password);
        let _ = stdin.write_all(b"\n");
    }
    child.wait().is_ok_and(|s| s.success())
}

fn legacy_password() -> Option<Vec<u8>> {
    let text = std::fs::read_to_string(path("etc/winlux/remote-vnc.passwd")).ok()?;
    let pw = text.lines().next()?.as_bytes().to_vec();
    (!pw.is_empty()).then_some(pw)
}

/// Clé RSA du serveur, créée au premier lancement (2048 bits, 0600).
fn load_key() -> io::Result<RsaKey> {
    let file = path("etc/winlux/remote-key");
    if let Ok(text) = std::fs::read_to_string(&file) {
        if let Some(key) = RsaKey::from_text(&text) {
            return Ok(key);
        }
        eprintln!("winlux-remote : clé illisible, une nouvelle clé est générée");
    }
    let bits = std::env::var("WINLUX_REMOTE_KEY_BITS")
        .ok()
        .and_then(|b| b.parse().ok())
        .unwrap_or(2048);
    let key = RsaKey::generate(bits);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension("tmp");
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        io::Write::write_all(&mut f, key.to_text().as_bytes())?;
    }
    std::fs::rename(tmp, file)?;
    Ok(key)
}

/// Écran à exposer : la session de ce compte s'il en a une (sessions
/// simultanées), sinon l'écran principal.
fn display_for(user: &str, default: &str) -> String {
    if !user.is_empty() {
        if let Ok(text) = std::fs::read_to_string(path(&format!("run/winlux/sessions/{user}"))) {
            if let Some(d) = conf_value(&text, "display").filter(|d| d.starts_with(':')) {
                return d;
            }
        }
    }
    default.to_string()
}

struct Client {
    user: String,
    peer: IpAddr,
    since: u64,
}

struct State {
    clients: Mutex<HashMap<u64, Client>>,
    failures: Mutex<HashMap<IpAddr, (u32, Instant)>>,
    fingerprint: String,
    port: u16,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn publish(state: &State) {
    let mut text = format!(
        "running=1\npid={}\nport={}\nfingerprint={}\n",
        std::process::id(),
        state.port,
        state.fingerprint
    );
    let clients = state.clients.lock().unwrap();
    text += &format!("clients={}\n", clients.len());
    for c in clients.values() {
        let who = if c.user.is_empty() {
            "(mot de passe VNC)"
        } else {
            &c.user
        };
        text += &format!("client={}|{}|{}\n", who, c.peer, c.since);
    }
    let file = path("run/winlux/remote-state.txt");
    let _ = std::fs::create_dir_all(file.parent().unwrap());
    let tmp = file.with_extension("tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(tmp, file);
    }
}

const MAX_FAILURES: u32 = 5;
const BAN: Duration = Duration::from_secs(60);
const MAX_CLIENTS: usize = 4;

fn banned(state: &State, ip: IpAddr) -> bool {
    let mut f = state.failures.lock().unwrap();
    match f.get(&ip) {
        Some(&(n, at)) if n >= MAX_FAILURES && at.elapsed() < BAN => true,
        Some(&(_, at)) if at.elapsed() >= BAN => {
            f.remove(&ip);
            false
        }
        _ => false,
    }
}

fn handle(
    stream: TcpStream,
    id: u64,
    key: Arc<RsaKey>,
    state: Arc<State>,
    default_display: String,
) {
    let peer = stream
        .peer_addr()
        .map(|a| a.ip())
        .unwrap_or(IpAddr::from([0, 0, 0, 0]));
    if banned(&state, peer) {
        eprintln!("winlux-remote : {peer} bloqué après trop d'échecs");
        return;
    }
    let cfg = load_config();
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let (Ok(r), Ok(w)) = (stream.try_clone(), stream.try_clone()) else {
        return;
    };
    let mut inp = rfb::In::new(Box::new(r));
    let mut out = rfb::Out::new(Box::new(w));
    let check_cfg = cfg.clone();
    let check = move |u: &str, p: &[u8]| check_account(&check_cfg, u, p);
    let sec = rfb::Security {
        key: &key,
        check: &check,
        legacy_password: if cfg.legacy { legacy_password() } else { None },
    };
    let user = match rfb::handshake(&mut inp, &mut out, &sec) {
        Ok((user, _)) => user,
        Err(e) => {
            eprintln!("winlux-remote : {peer} : {e}");
            if e.kind() == io::ErrorKind::PermissionDenied {
                let mut f = state.failures.lock().unwrap();
                let entry = f.entry(peer).or_insert((0, Instant::now()));
                entry.0 += 1;
                entry.1 = Instant::now();
                // Ralentir les essais successifs de mots de passe.
                std::thread::sleep(Duration::from_millis(500 * entry.0.min(6) as u64));
            }
            return;
        }
    };
    state.failures.lock().unwrap().remove(&peer);
    let _ = stream.set_read_timeout(None);
    let display = display_for(&user, &default_display);
    let screen = match screen::X11Screen::open(&display) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("winlux-remote : {e}");
            return;
        }
    };
    let input: Option<Box<dyn screen::Input>> = match screen::X11Input::open(&display) {
        Ok(i) => Some(Box::new(i)),
        Err(e) => {
            eprintln!("winlux-remote : {e} ; session en lecture seule");
            None
        }
    };
    eprintln!(
        "winlux-remote : {peer} connecté ({}, écran {display})",
        if user.is_empty() {
            "mot de passe VNC"
        } else {
            &user
        }
    );
    state.clients.lock().unwrap().insert(
        id,
        Client {
            user: user.clone(),
            peer,
            since: now_secs(),
        },
    );
    publish(&state);
    let host = std::fs::read_to_string(path("etc/hostname"))
        .map(|h| h.trim().to_string())
        .unwrap_or_else(|_| "winlux".into());
    let opts = rfb::Options {
        name: if user.is_empty() {
            format!("Winlux - {host}")
        } else {
            format!("Winlux - {user}@{host}")
        },
        view_only: cfg.view_only,
        frame_interval: Duration::from_millis(33),
    };
    if let Err(e) = rfb::serve(inp, out, Box::new(screen), input, &opts) {
        eprintln!("winlux-remote : {peer} : {e}");
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
    state.clients.lock().unwrap().remove(&id);
    publish(&state);
    eprintln!("winlux-remote : {peer} déconnecté");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("usage : winlux-remote [--port N] [--display :0] [--fingerprint]");
        return;
    }
    let key = match load_key() {
        Ok(k) => Arc::new(k),
        Err(e) => {
            eprintln!("winlux-remote : clé du serveur impossible à créer : {e}");
            std::process::exit(1);
        }
    };
    if args.iter().any(|a| a == "--fingerprint") {
        println!("{}", rfb::fingerprint(&key));
        return;
    }
    let cfg = load_config();
    let port = flag("--port")
        .and_then(|p| p.parse().ok())
        .unwrap_or(cfg.port);
    let display = flag("--display")
        .or_else(|| std::env::var("DISPLAY").ok())
        .unwrap_or_else(|| ":0".into());
    let listener =
        match TcpListener::bind(("0.0.0.0", port)).or_else(|_| TcpListener::bind(("::", port))) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("winlux-remote : port {port} indisponible : {e}");
                std::process::exit(1);
            }
        };
    let state = Arc::new(State {
        clients: Mutex::new(HashMap::new()),
        failures: Mutex::new(HashMap::new()),
        fingerprint: rfb::fingerprint(&key),
        port,
    });
    publish(&state);
    eprintln!(
        "winlux-remote : écoute sur le port {port} (écran {display}, empreinte {})",
        state.fingerprint
    );
    let mut next = 0u64;
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if state.clients.lock().unwrap().len() >= MAX_CLIENTS {
            continue;
        }
        next += 1;
        let (key, state, display) = (key.clone(), state.clone(), display.clone());
        let id = next;
        std::thread::spawn(move || handle(stream, id, key, state, display));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comptes_autorises() {
        let dir = std::env::temp_dir().join(format!("winlux-remote-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("etc/winlux")).unwrap();
        std::fs::write(
            dir.join("etc/winlux/users.conf"),
            "root:root:admin\nalice:user:s3cret\nbob:user:nopass\n",
        )
        .unwrap();
        std::env::set_var("WINLUX_CONTROL_ROOT", &dir);
        let mut cfg = Config::default();
        // Liste vide : seuls les administrateurs.
        assert!(check_account(&cfg, "root", b"admin"));
        assert!(!check_account(&cfg, "alice", b"s3cret"));
        cfg.users = vec!["alice".into(), "bob".into()];
        assert!(check_account(&cfg, "alice", b"s3cret"));
        assert!(!check_account(&cfg, "alice", b"s3cre"));
        assert!(!check_account(&cfg, "root", b"admin"));
        // Sans mot de passe : jamais à distance.
        assert!(!check_account(&cfg, "bob", b"nopass"));
        assert!(!check_account(&cfg, "bob", b""));
        std::fs::write(
            dir.join("etc/winlux/remote.conf"),
            "PORT=5901\nUSERS=alice, bob\nLEGACY_VNC=1\n",
        )
        .unwrap();
        let loaded = load_config();
        assert_eq!(loaded.port, 5901);
        assert_eq!(loaded.users, vec!["alice", "bob"]);
        assert!(loaded.legacy && !loaded.view_only);
        std::fs::create_dir_all(dir.join("run/winlux/sessions")).unwrap();
        std::fs::write(dir.join("run/winlux/sessions/alice"), "display=:2\nvt=3\n").unwrap();
        assert_eq!(display_for("alice", ":0"), ":2");
        assert_eq!(display_for("root", ":0"), ":0");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
