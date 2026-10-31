//! Lecteurs réseau : monter FTP, FTPS, SFTP, WebDAV, SMB, NFS ou S3 comme
//! un disque avec une lettre (`N:\`), comme « Connecter un lecteur réseau »
//! sous Windows.
//!
//! - configuration : `~/.config/winlux/netdrives.conf` (sans mot de passe) ;
//! - mots de passe : `~/.config/winlux/netdrives.secrets`, scellés en
//!   ChaCha20-Poly1305 avec une clé aléatoire propre à l'utilisateur
//!   (`~/.local/share/winlux/netdrives.key`, 0600) ; option « ne pas
//!   mémoriser » : rien n'est écrit ;
//! - lecteurs montés sous `<run>/drives/<lettre>` et publiés dans le registre
//!   des lecteurs (`<run>/mounts.tsv`, partagé avec les images disque) ;
//! - état lisible par les Paramètres : `<run>/netdrives.txt`.
//!
//! Commande : `winlux-drives` (ou `winlux --drives`) `status | test | add |
//! connect L | disconnect L | remove L | watch`. Les Paramètres déposent
//! leurs demandes (avec le mot de passe) dans `<run>/netdrive-request`, lu
//! puis effacé aussitôt : le mot de passe ne passe jamais en argument.

pub mod backend;
pub mod seal;

use backend::{Backend, System};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Ftp,
    Ftps,
    Sftp,
    WebDav,
    WebDavs,
    Smb,
    Nfs,
    S3,
}

impl Protocol {
    pub const ALL: [Protocol; 8] = [
        Protocol::Ftp,
        Protocol::Ftps,
        Protocol::Sftp,
        Protocol::WebDav,
        Protocol::WebDavs,
        Protocol::Smb,
        Protocol::Nfs,
        Protocol::S3,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Protocol::Ftp => "ftp",
            Protocol::Ftps => "ftps",
            Protocol::Sftp => "sftp",
            Protocol::WebDav => "webdav",
            Protocol::WebDavs => "webdavs",
            Protocol::Smb => "smb",
            Protocol::Nfs => "nfs",
            Protocol::S3 => "s3",
        }
    }

    pub fn from_id(s: &str) -> Option<Protocol> {
        let s = s.trim().to_ascii_lowercase();
        let s = match s.as_str() {
            "cifs" => "smb",
            "ssh" => "sftp",
            "dav" => "webdav",
            "davs" => "webdavs",
            other => other,
        };
        Protocol::ALL.iter().copied().find(|p| p.id() == s)
    }

    pub fn default_port(self) -> u16 {
        match self {
            Protocol::Ftp | Protocol::Ftps => 21,
            Protocol::Sftp => 22,
            Protocol::WebDav => 80,
            Protocol::WebDavs | Protocol::S3 => 443,
            Protocol::Smb => 445,
            Protocol::Nfs => 2049,
        }
    }
}

/// Un lecteur réseau configuré.
#[derive(Clone, Debug, PartialEq)]
pub struct Drive {
    pub letter: char,
    pub name: String,
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub user: String,
    /// Dossier distant ; pour SMB `partage/dossier`, pour S3 `bucket/préfixe`.
    pub path: String,
    /// Reconnecter automatiquement à l'ouverture de session.
    pub auto: bool,
    /// Mot de passe mémorisé (chiffré).
    pub remember: bool,
}

impl Drive {
    /// Adresse affichée et inscrite au registre (jamais de mot de passe).
    pub fn source(&self) -> String {
        let user = if self.user.is_empty() || self.protocol == Protocol::S3 {
            String::new()
        } else {
            format!("{}@", self.user)
        };
        format!(
            "{}://{user}{}:{}/{}",
            self.protocol.id(),
            self.host,
            self.port,
            self.path.trim_start_matches('/')
        )
    }

    pub fn label(&self) -> String {
        if self.name.trim().is_empty() {
            format!(
                "{} ({})",
                self.host,
                self.protocol.id().to_ascii_uppercase()
            )
        } else {
            self.name.clone()
        }
    }
}

fn bad_char(c: char) -> bool {
    c.is_control() || c == '\t'
}

/// Vérifie une demande avant de toucher au système. Les valeurs finissent
/// dans des options `-o a,b,c` : virgules et caractères de contrôle sont
/// refusés là où ils permettraient d'injecter une option.
pub fn validate(d: &Drive) -> Result<(), String> {
    let l = d.letter;
    if !l.is_ascii_uppercase() || matches!(l, 'A' | 'B' | 'C' | 'Z') {
        return Err("Choisissez une lettre de lecteur entre D et Y.".into());
    }
    let host = d.host.trim();
    if host.is_empty() {
        return Err("Indiquez l'adresse du serveur.".into());
    }
    if host.starts_with('-')
        || host
            .chars()
            .any(|c| bad_char(c) || c.is_whitespace() || matches!(c, ',' | '/' | '@' | '\\'))
    {
        return Err("L'adresse du serveur contient des caractères invalides.".into());
    }
    if d.port == 0 {
        return Err("Le port doit être compris entre 1 et 65535.".into());
    }
    if d.user.starts_with('-') || d.user.chars().any(|c| bad_char(c) || c == ',') {
        return Err("L'identifiant contient des caractères invalides.".into());
    }
    if d.path.chars().any(bad_char) || d.name.chars().any(bad_char) {
        return Err("Le dossier ou le nom contient des caractères invalides.".into());
    }
    match d.protocol {
        Protocol::Smb if d.path.trim_matches('/').is_empty() => {
            Err("Indiquez le nom du partage (par exemple Documents).".into())
        }
        Protocol::S3 if d.path.trim_matches('/').is_empty() => {
            Err("Indiquez le nom du bucket S3.".into())
        }
        Protocol::S3 if d.user.is_empty() => Err("Indiquez la clé d'accès S3.".into()),
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------- chemins

fn config_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(d).join("winlux");
    }
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".config/winlux"))
        .unwrap_or_else(|| PathBuf::from("/etc/winlux"))
}

/// Dossier d'exécution partagé avec le registre des lecteurs.
pub fn run_dir() -> PathBuf {
    let d = crate::mounts::state_dir();
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn mountpoint(letter: char) -> PathBuf {
    run_dir().join("drives").join(letter.to_string())
}

fn private_file(path: &Path, content: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(content)?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)
}

// ---------------------------------------------------------- configuration

pub fn parse_config(text: &str) -> Vec<Drive> {
    let mut out: Vec<Drive> = Vec::new();
    let mut cur: Option<BTreeMap<String, String>> = None;
    let mut letter = ' ';
    let flush = |letter: char, map: Option<BTreeMap<String, String>>, out: &mut Vec<Drive>| {
        if let Some(m) = map {
            if let Some(d) = drive_from_map(letter, &m) {
                out.retain(|o| o.letter != d.letter);
                out.push(d);
            }
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(sec) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            flush(letter, cur.take(), &mut out);
            letter = sec
                .trim()
                .chars()
                .next()
                .unwrap_or(' ')
                .to_ascii_uppercase();
            cur = Some(BTreeMap::new());
        } else if let (Some(m), Some((k, v))) = (cur.as_mut(), line.split_once('=')) {
            m.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    flush(letter, cur, &mut out);
    out.sort_by_key(|d| d.letter);
    out
}

fn drive_from_map(letter: char, m: &BTreeMap<String, String>) -> Option<Drive> {
    let get = |k: &str| m.get(k).cloned().unwrap_or_default();
    let protocol = Protocol::from_id(&get("protocol"))?;
    let port = get("port").parse().unwrap_or(protocol.default_port());
    let d = Drive {
        letter,
        name: get("name"),
        protocol,
        host: get("host"),
        port,
        user: get("user"),
        path: get("path"),
        auto: get("auto") != "0",
        remember: get("remember") == "1",
    };
    validate(&d).ok().map(|_| d)
}

pub fn format_config(drives: &[Drive]) -> String {
    let mut s = String::from("# Lecteurs réseau Winlux (les mots de passe ne sont jamais ici)\n");
    for d in drives {
        s.push_str(&format!(
            "\n[{}]\nname={}\nprotocol={}\nhost={}\nport={}\nuser={}\npath={}\nauto={}\nremember={}\n",
            d.letter,
            d.name,
            d.protocol.id(),
            d.host,
            d.port,
            d.user,
            d.path,
            u8::from(d.auto),
            u8::from(d.remember)
        ));
    }
    s
}

pub fn load() -> Vec<Drive> {
    std::fs::read_to_string(config_dir().join("netdrives.conf"))
        .map(|t| parse_config(&t))
        .unwrap_or_default()
}

fn store(drives: &[Drive]) -> Result<(), String> {
    private_file(
        &config_dir().join("netdrives.conf"),
        format_config(drives).as_bytes(),
    )
    .map_err(|e| format!("Enregistrement impossible : {e}"))
}

// ---------------------------------------------------------- mots de passe

fn key_path() -> PathBuf {
    crate::paths::data_dir()
        .unwrap_or_else(config_dir)
        .join("netdrives.key")
}

fn random(buf: &mut [u8]) -> bool {
    let mut done = 0;
    while done < buf.len() {
        let n = unsafe { libc::getrandom(buf[done..].as_mut_ptr().cast(), buf.len() - done, 0) };
        if n <= 0 {
            return std::fs::File::open("/dev/urandom")
                .and_then(|mut f| f.read_exact(buf))
                .is_ok();
        }
        done += n as usize;
    }
    true
}

fn secret_key(create: bool) -> Option<[u8; 32]> {
    let path = key_path();
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(k) = <[u8; 32]>::try_from(bytes.as_slice()) {
            return Some(k);
        }
    }
    if !create {
        return None;
    }
    let mut k = [0u8; 32];
    if !random(&mut k) {
        return None;
    }
    private_file(&path, &k).ok()?;
    Some(k)
}

fn aad(d: &Drive) -> Vec<u8> {
    format!("winlux-netdrive|{}|{}|{}", d.letter, d.host, d.user).into_bytes()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

pub fn seal_password(key: &[u8; 32], d: &Drive, password: &str) -> Option<String> {
    let mut nonce = [0u8; 12];
    if !random(&mut nonce) {
        return None;
    }
    let sealed = seal::seal(key, &nonce, &aad(d), password.as_bytes());
    Some(format!("{}{}", hex(&nonce), hex(&sealed)))
}

pub fn open_password(key: &[u8; 32], d: &Drive, text: &str) -> Option<String> {
    let raw = unhex(text.trim())?;
    if raw.len() < 12 + 16 {
        return None;
    }
    let nonce: [u8; 12] = raw[..12].try_into().ok()?;
    let plain = seal::open(key, &nonce, &aad(d), &raw[12..])?;
    String::from_utf8(plain).ok()
}

fn secrets_path() -> PathBuf {
    config_dir().join("netdrives.secrets")
}

fn read_secrets() -> BTreeMap<char, String> {
    std::fs::read_to_string(secrets_path())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            Some((k.trim().chars().next()?, v.trim().to_string()))
        })
        .collect()
}

fn write_secrets(map: &BTreeMap<char, String>) -> Result<(), String> {
    let text: String = map.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    private_file(&secrets_path(), text.as_bytes())
        .map_err(|e| format!("Enregistrement du mot de passe impossible : {e}"))
}

fn saved_password(d: &Drive) -> Option<String> {
    let text = read_secrets().remove(&d.letter)?;
    open_password(&secret_key(false)?, d, &text)
}

fn save_password(d: &Drive, password: Option<&str>) -> Result<(), String> {
    let mut map = read_secrets();
    match password.filter(|p| d.remember && !p.is_empty()) {
        Some(p) => {
            let key = secret_key(true).ok_or("Clé de chiffrement indisponible.")?;
            let sealed = seal_password(&key, d, p).ok_or("Aléa système indisponible.")?;
            map.insert(d.letter, sealed);
        }
        None => {
            map.remove(&d.letter);
        }
    }
    write_secrets(&map)
}

// ------------------------------------------------- registre des lecteurs

/// État d'un lecteur, publié dans la 9e colonne du registre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Online,
    Offline,
    Connecting,
    Disconnected,
    NeedsPassword,
}

impl State {
    fn id(self) -> &'static str {
        match self {
            State::Online => "ok",
            State::Offline => "offline",
            State::Connecting => "connecting",
            State::Disconnected => "disconnected",
            State::NeedsPassword => "password",
        }
    }
}

/// Ligne du registre partagé (`mounts.tsv`) pour un lecteur réseau.
pub fn registry_row(d: &Drive, b: Option<Backend>, st: State) -> crate::mounts::Mount {
    crate::mounts::Mount {
        letter: d.letter,
        kind: "network".into(),
        mountpoint: mountpoint(d.letter),
        label: d.label(),
        read_only: false,
        source: d.source(),
        device: String::new(),
        backend: b.map(Backend::id).unwrap_or("").into(),
        state: st.id().into(),
    }
}

/// Aligne les lignes `network` du registre sur la configuration, sans
/// toucher aux autres lecteurs (images disque…).
fn sync_registry(rows: &[crate::mounts::Mount]) {
    let current = crate::mounts::list();
    for m in current.iter().filter(|m| m.kind == "network") {
        if !rows.iter().any(|r| r.letter == m.letter) {
            let _ = crate::mounts::remove(m.letter);
        }
    }
    for r in rows {
        match current.iter().find(|m| m.letter == r.letter) {
            Some(m) if m == r => {}
            // Lettre prise entre-temps par une image disque : on n'écrase pas.
            Some(m) if m.kind != "network" => {}
            Some(_) => {
                let _ = crate::mounts::remove(r.letter);
                let _ = crate::mounts::add(r.clone());
            }
            None => {
                let _ = crate::mounts::add(r.clone());
            }
        }
    }
}

/// Lettres occupées par autre chose qu'un lecteur réseau (images disque…).
fn letters_in_use_elsewhere() -> Vec<char> {
    crate::mounts::list()
        .into_iter()
        .filter(|m| m.kind != "network")
        .map(|m| m.letter)
        .chain(('D'..='Y').filter(|l| std::env::var_os(format!("WINLUX_DRIVE_{l}")).is_some()))
        .collect()
}

// ------------------------------------------------------------- état global

struct Lock(std::fs::File);

impl Lock {
    fn take() -> Option<Lock> {
        let f = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(run_dir().join("netdrives.lock"))
            .ok()?;
        use std::os::fd::AsRawFd;
        (unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } == 0).then_some(Lock(f))
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
    }
}

/// Lettres que l'utilisateur veut connectées pendant cette session.
fn wanted() -> Vec<char> {
    std::fs::read_to_string(run_dir().join("netdrives-wanted"))
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_uppercase)
        .collect()
}

fn set_wanted(letter: char, on: bool) {
    let mut w = wanted();
    w.retain(|&l| l != letter);
    if on {
        w.push(letter);
    }
    let _ = std::fs::write(
        run_dir().join("netdrives-wanted"),
        w.into_iter().collect::<String>(),
    );
}

fn states_path() -> PathBuf {
    run_dir().join("netdrives-state")
}

fn read_states() -> BTreeMap<char, State> {
    std::fs::read_to_string(states_path())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            let st = match v {
                "ok" => State::Online,
                "offline" => State::Offline,
                "connecting" => State::Connecting,
                "password" => State::NeedsPassword,
                _ => State::Disconnected,
            };
            Some((k.chars().next()?, st))
        })
        .collect()
}

fn set_state(letter: char, st: State) {
    let mut m = read_states();
    m.insert(letter, st);
    let text: String = m.iter().map(|(k, v)| format!("{k}={}\n", v.id())).collect();
    let _ = std::fs::write(states_path(), text);
}

/// Décode `\040` & co. dans /proc/mounts.
fn unescape_mount(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c))
        {
            let v = (b[i + 1] - b'0') * 64 + (b[i + 2] - b'0') * 8 + (b[i + 3] - b'0');
            out.push(v);
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn is_mounted_in(mounts: &str, mp: &Path) -> bool {
    let want = mp.to_string_lossy();
    mounts
        .lines()
        .filter_map(|l| l.split_whitespace().nth(1))
        .any(|m| unescape_mount(m) == want)
}

fn is_mounted(mp: &Path) -> bool {
    std::fs::read_to_string("/proc/self/mounts")
        .map(|m| is_mounted_in(&m, mp))
        .unwrap_or(false)
}

/// Publie l'état pour les Paramètres et le registre pour l'Explorateur.
fn publish(sys: &System, message: Option<(bool, &str)>) {
    let drives = load();
    let states = read_states();
    let wanted = wanted();
    let rows: Vec<(Drive, State)> = drives
        .iter()
        .map(|d| {
            let mounted = is_mounted(&mountpoint(d.letter));
            let st = match states.get(&d.letter).copied() {
                _ if mounted && states.get(&d.letter) != Some(&State::Offline) => State::Online,
                Some(s) if wanted.contains(&d.letter) || s == State::NeedsPassword => s,
                _ => State::Disconnected,
            };
            (d.clone(), st)
        })
        .collect();
    let reg: Vec<_> = rows
        .iter()
        .map(|(d, st)| registry_row(d, sys.backend_for(d.protocol), *st))
        .collect();
    sync_registry(&reg);

    let mut s = String::new();
    let offered: Vec<&str> = Protocol::ALL
        .iter()
        .filter(|p| sys.backend_for(**p).is_some())
        .map(|p| p.id())
        .collect();
    s.push_str(&format!("protocols={}\n", offered.join(",")));
    let missing: Vec<String> = Protocol::ALL
        .iter()
        .filter_map(|p| {
            backend::missing_hint(sys, *p).map(|h| format!("{} : {h}", p.id().to_ascii_uppercase()))
        })
        .collect();
    s.push_str(&format!("missing={}\n", ascii(&missing.join(" ; "))));
    let used: Vec<char> = letters_in_use_elsewhere();
    let free = ('D'..='Y')
        .rev()
        .find(|l| !used.contains(l) && !drives.iter().any(|d| d.letter == *l))
        .unwrap_or('N');
    s.push_str(&format!("next_letter={free}\n"));
    s.push_str(&format!(
        "drives={}\n",
        rows.iter().map(|r| r.0.letter).collect::<String>()
    ));
    for (d, st) in &rows {
        let free = if *st == State::Online {
            disk_free(&mountpoint(d.letter))
        } else {
            String::new()
        };
        s.push_str(&format!(
            "drive.{}={}|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
            d.letter,
            ascii(&d.label()),
            d.protocol.id(),
            ascii(&d.host),
            d.port,
            ascii(&d.user),
            ascii(&d.path),
            u8::from(d.auto),
            u8::from(d.remember),
            st.id(),
            free
        ));
    }
    if let Some((ok, msg)) = message {
        s.push_str(&format!(
            "result={}\nmessage={}\nserial={}\n",
            if ok { "ok" } else { "error" },
            ascii(msg),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ));
    } else if let Ok(old) = std::fs::read_to_string(run_dir().join("netdrives.txt")) {
        for l in old.lines().filter(|l| {
            l.starts_with("result=") || l.starts_with("message=") || l.starts_with("serial=")
        }) {
            s.push_str(l);
            s.push('\n');
        }
    }
    let _ = private_publish(&run_dir().join("netdrives.txt"), &s);
}

fn private_publish(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("new");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)
}

/// Texte pour l'interface Win32 (Windows-1252) : les accents latins
/// passent, le reste devient `?`, les séparateurs sont neutralisés.
fn ascii(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '|' | '\n' | '\r' => ' ',
            c if (c as u32) < 0x100 => c,
            _ => '?',
        })
        .collect()
}

fn disk_free(p: &Path) -> String {
    let Ok(c) = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()) else {
        return String::new();
    };
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 || s.f_blocks == 0 {
        return String::new();
    }
    let frag = if s.f_frsize > 0 {
        s.f_frsize
    } else {
        s.f_bsize
    } as u64;
    let total = s.f_blocks as u64 * frag;
    // Certains moteurs (FTP via rclone) annoncent 1 Pio fictif : inconnu.
    if total >= 1 << 50 {
        return String::new();
    }
    format!("{}/{}", s.f_bavail as u64 * frag, total)
}

// --------------------------------------------------------------- montage

fn run_with_timeout(
    mut cmd: Command,
    stdin: Option<&str>,
    timeout: Duration,
) -> Result<String, String> {
    cmd.stdin(if stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Lancement impossible : {e}"))?;
    if let (Some(data), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let _ = pipe.write_all(data.as_bytes());
    }
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() {
                    let _ = e.read_to_string(&mut err);
                }
                return if status.success() {
                    Ok(err)
                } else {
                    Err(err.lines().last().unwrap_or("échec").trim().to_string())
                };
            }
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Le serveur ne répond pas (délai dépassé).".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Le serveur répond-il ? (connexion TCP avec délai court, sans bloquer.)
pub fn reachable(host: &str, port: u16, timeout: Duration) -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    let target = (host.to_string(), port);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let ok = target
            .to_socket_addrs()
            .map(|addrs| {
                addrs
                    .into_iter()
                    .any(|a| TcpStream::connect_timeout(&a, timeout).is_ok())
            })
            .unwrap_or(false);
        let _ = tx.send(ok);
    });
    rx.recv_timeout(timeout + Duration::from_secs(2))
        .unwrap_or(false)
}

/// Le point de montage répond-il encore ? (`stat` dans un fil séparé : un
/// montage mort peut bloquer, on ne bloque jamais l'appelant.)
fn responsive(mp: &Path) -> bool {
    let mp = mp.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(std::fs::read_dir(&mp).is_ok());
    });
    rx.recv_timeout(Duration::from_secs(4)).unwrap_or(false)
}

fn mount_drive(sys: &System, d: &Drive, password: Option<&str>, mp: &Path) -> Result<(), String> {
    let b = sys
        .backend_for(d.protocol)
        .ok_or("Ce type de connexion n'est pas pris en charge sur cette machine.")?;
    if !reachable(&d.host, d.port, Duration::from_secs(5)) {
        return Err(format!(
            "Le serveur {}:{} est injoignable. Vérifiez l'adresse et le réseau.",
            d.host, d.port
        ));
    }
    if is_mounted(mp) {
        unmount(sys, b, mp);
    }
    std::fs::create_dir_all(mp).map_err(|e| format!("Point de montage impossible : {e}"))?;
    let secret_dir = run_dir().join(format!(".secret-{}-{}", d.letter, std::process::id()));
    {
        use std::os::unix::fs::DirBuilderExt;
        let _ = std::fs::remove_dir_all(&secret_dir);
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&secret_dir)
            .map_err(|e| format!("Dossier privé impossible : {e}"))?;
    }
    let plan = backend::plan(b, d, password, mp, &secret_dir);
    let result = (|| -> Result<(), String> {
        for (name, content) in &plan.files {
            private_file(&secret_dir.join(name), content.as_bytes())
                .map_err(|e| format!("Fichier d'identification impossible : {e}"))?;
        }
        let mut cmd = Command::new(&plan.program);
        cmd.args(&plan.args);
        for (k, v) in &plan.env {
            cmd.env(k, v);
        }
        let mut stdin = plan.stdin.clone();
        if let Some(var) = &plan.obscured_env {
            let mut ob = Command::new("rclone");
            ob.args(["obscure", "-"]);
            let mut child = ob
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| e.to_string())?;
            if let Some(mut p) = child.stdin.take() {
                let _ = p.write_all(stdin.take().unwrap_or_default().as_bytes());
            }
            let out = child.wait_with_output().map_err(|e| e.to_string())?;
            cmd.env(var, String::from_utf8_lossy(&out.stdout).trim());
        }
        run_with_timeout(cmd, stdin.as_deref(), Duration::from_secs(25))?;
        // Les moteurs FUSE rendent la main avant d'être prêts.
        let start = Instant::now();
        while !is_mounted(mp) && start.elapsed() < Duration::from_secs(8) {
            std::thread::sleep(Duration::from_millis(100));
        }
        if !is_mounted(mp) {
            return Err("Le montage n'a pas abouti.".into());
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&secret_dir);
    result.map_err(|e| friendly(&e))
}

/// Traduit les erreurs courantes des outils en phrases compréhensibles.
fn friendly(raw: &str) -> String {
    let l = raw.to_ascii_lowercase();
    if l.contains("permission denied")
        || l.contains("login incorrect")
        || l.contains("access denied")
        || l.contains("authentication")
        || l.contains("530")
        || l.contains("401")
        || l.contains("403")
    {
        "Identifiant ou mot de passe refusé par le serveur.".into()
    } else if l.contains("no such file")
        || l.contains("not found")
        || l.contains("does not exist")
        || l.contains("nosuchbucket")
    {
        "Le dossier distant n'existe pas sur le serveur.".into()
    } else if l.contains("host key") {
        "La clé du serveur SSH a changé : connexion refusée par sécurité.".into()
    } else if l.contains("certificate") || l.contains("ssl") || l.contains("tls") {
        "Le certificat de sécurité du serveur n'a pas pu être vérifié.".into()
    } else if raw.is_empty() {
        "Connexion impossible.".into()
    } else {
        raw.to_string()
    }
}

fn unmount(sys: &System, b: Backend, mp: &Path) {
    for argv in backend::unmount_commands(b, sys, mp) {
        if !is_mounted(mp) {
            break;
        }
        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..]);
        let _ = run_with_timeout(cmd, None, Duration::from_secs(5));
    }
    if !is_mounted(mp) {
        let _ = std::fs::remove_dir(mp);
    }
}

// --------------------------------------------------------- demandes (CLI)

/// Lit puis efface la demande des Paramètres (elle peut contenir un mot
/// de passe). Les octets sont en Windows-1252.
fn take_request() -> BTreeMap<String, String> {
    let path = run_dir().join("netdrive-request");
    let raw = std::fs::read(&path).unwrap_or_default();
    let _ = std::fs::remove_file(&path);
    let text: String = raw.iter().map(|&b| b as char).collect();
    parse_request(&text)
}

pub fn parse_request(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            Some((k.trim().to_string(), v.trim_end_matches('\r').to_string()))
        })
        .collect()
}

pub fn drive_from_request(r: &BTreeMap<String, String>) -> Result<Drive, String> {
    let get = |k: &str| r.get(k).map(|s| s.trim().to_string()).unwrap_or_default();
    let protocol = Protocol::from_id(&get("protocol")).ok_or("Choisissez un type de connexion.")?;
    let port = match get("port") {
        p if p.is_empty() => protocol.default_port(),
        p => p
            .parse()
            .map_err(|_| "Le port doit être un nombre entre 1 et 65535.")?,
    };
    let d = Drive {
        letter: get("letter")
            .chars()
            .next()
            .unwrap_or(' ')
            .to_ascii_uppercase(),
        name: get("name"),
        protocol,
        host: get("host"),
        port,
        user: get("user"),
        path: get("path"),
        auto: get("auto") != "0",
        remember: get("remember") != "0",
    };
    validate(&d)?;
    Ok(d)
}

fn letter_arg(arg: Option<&str>) -> Option<char> {
    arg.and_then(|a| a.trim_end_matches([':', '\\']).chars().next())
        .map(|c| c.to_ascii_uppercase())
}

fn connect(sys: &System, d: &Drive, password: Option<String>) -> Result<(), String> {
    let pw = password
        .filter(|p| !p.is_empty())
        .or_else(|| saved_password(d));
    set_wanted(d.letter, true);
    set_state(d.letter, State::Connecting);
    publish(sys, None);
    let needs_pw =
        !d.user.is_empty() && pw.is_none() && !matches!(d.protocol, Protocol::Sftp | Protocol::Nfs);
    if needs_pw {
        set_state(d.letter, State::NeedsPassword);
        let _ = std::fs::write(run_dir().join("netdrive-ask"), d.letter.to_string());
        return Err("Mot de passe requis pour ce lecteur.".into());
    }
    match mount_drive(sys, d, pw.as_deref(), &mountpoint(d.letter)) {
        Ok(()) => {
            set_state(d.letter, State::Online);
            Ok(())
        }
        Err(e) => {
            set_state(d.letter, State::Offline);
            Err(e)
        }
    }
}

/// Point d'entrée `winlux-drives …`. Renvoie le code de sortie.
pub fn main(args: &[String]) -> i32 {
    let sys = System::detect();
    let cmd = args.first().map(String::as_str).unwrap_or("status");
    if cmd == "watch" {
        watch(&sys);
        return 0;
    }
    let Some(_lock) = Lock::take() else {
        eprintln!("winlux-drives: verrou indisponible");
        return 1;
    };
    let outcome: Result<String, String> = match cmd {
        "status" => {
            publish(&sys, None);
            return 0;
        }
        "protocols" => {
            for p in Protocol::ALL {
                match sys.backend_for(p) {
                    Some(b) => println!("{}\t{}", p.id(), b.id()),
                    None => println!("{}\t-", p.id()),
                }
            }
            return 0;
        }
        "test" => {
            let r = take_request();
            drive_from_request(&r).and_then(|d| {
                // Montage d'essai dans un dossier à part, aussitôt démonté.
                let mp = run_dir().join("drives").join(".test");
                let res = mount_drive(&sys, &d, r.get("password").map(String::as_str), &mp);
                if let Some(b) = sys.backend_for(d.protocol) {
                    unmount(&sys, b, &mp);
                }
                res.map(|_| {
                    "Connexion réussie : le serveur répond et accepte les identifiants.".into()
                })
            })
        }
        "add" => {
            let r = take_request();
            drive_from_request(&r).and_then(|d| {
                let others = letters_in_use_elsewhere();
                if others.contains(&d.letter) {
                    return Err(format!("La lettre {}: est déjà utilisée.", d.letter));
                }
                let pw = r.get("password").cloned();
                let mut drives = load();
                if let Some(old) = drives.iter().find(|o| o.letter == d.letter).cloned() {
                    if let Some(b) = sys.backend_for(old.protocol) {
                        unmount(&sys, b, &mountpoint(old.letter));
                    }
                }
                drives.retain(|o| o.letter != d.letter);
                drives.push(d.clone());
                drives.sort_by_key(|d| d.letter);
                store(&drives)?;
                save_password(&d, pw.as_deref())?;
                connect(&sys, &d, pw).map(|_| format!("Lecteur {}: connecté.", d.letter))
            })
        }
        "connect" => {
            let r = take_request();
            match letter_arg(args.get(1).map(String::as_str))
                .or_else(|| r.get("letter").and_then(|l| l.chars().next()))
                .and_then(|l| load().into_iter().find(|d| d.letter == l))
            {
                Some(d) => connect(&sys, &d, r.get("password").cloned())
                    .map(|_| format!("Lecteur {}: connecté.", d.letter)),
                None => Err("Lecteur inconnu.".into()),
            }
        }
        "disconnect" | "remove" => {
            match letter_arg(args.get(1).map(String::as_str))
                .and_then(|l| load().into_iter().find(|d| d.letter == l))
            {
                Some(d) => {
                    if let Some(b) = sys.backend_for(d.protocol) {
                        unmount(&sys, b, &mountpoint(d.letter));
                    }
                    set_wanted(d.letter, false);
                    set_state(d.letter, State::Disconnected);
                    if cmd == "remove" {
                        let mut drives = load();
                        drives.retain(|o| o.letter != d.letter);
                        let mut secrets = read_secrets();
                        secrets.remove(&d.letter);
                        store(&drives)
                            .and_then(|_| write_secrets(&secrets))
                            .map(|_| format!("Lecteur {}: supprimé.", d.letter))
                    } else {
                        Ok(format!("Lecteur {}: déconnecté.", d.letter))
                    }
                }
                None => Err("Lecteur inconnu.".into()),
            }
        }
        _ => {
            eprintln!(
                "usage: winlux-drives status|protocols|test|add|connect L|disconnect L|remove L|watch"
            );
            return 2;
        }
    };
    let (ok, msg) = match &outcome {
        Ok(m) => (true, m.as_str()),
        Err(e) => (false, e.as_str()),
    };
    publish(&sys, Some((ok, msg)));
    println!("{msg}");
    i32::from(!ok)
}

/// Surveillance : connecte les lecteurs « automatiques » à l'ouverture de
/// session, signale ceux qui deviennent injoignables (au lieu de laisser
/// l'Explorateur bloquer) et les reconnecte avec un délai croissant.
fn watch(sys: &System) {
    if let Some(_l) = Lock::take() {
        for d in load().iter().filter(|d| d.auto) {
            set_wanted(d.letter, true);
            set_state(d.letter, State::Offline);
        }
        publish(sys, None);
    }
    let mut next_try: BTreeMap<char, (Instant, u64)> = BTreeMap::new();
    loop {
        let drives = load();
        for d in drives.iter().filter(|d| wanted().contains(&d.letter)) {
            let mp = mountpoint(d.letter);
            let healthy = is_mounted(&mp)
                && reachable(&d.host, d.port, Duration::from_secs(3))
                && responsive(&mp);
            if healthy {
                next_try.remove(&d.letter);
                if read_states().get(&d.letter) != Some(&State::Online) {
                    let _l = Lock::take();
                    set_state(d.letter, State::Online);
                    publish(sys, None);
                }
                continue;
            }
            if read_states().get(&d.letter) == Some(&State::NeedsPassword) {
                continue;
            }
            let now = Instant::now();
            let (when, delay) = next_try.get(&d.letter).copied().unwrap_or((now, 5));
            if now < when {
                continue;
            }
            let _l = Lock::take();
            if !wanted().contains(&d.letter) {
                continue;
            }
            if let Some(b) = sys.backend_for(d.protocol) {
                if is_mounted(&mp) {
                    // Montage mort : on le retire pour que l'accès échoue vite.
                    unmount(sys, b, &mp);
                }
            }
            set_state(d.letter, State::Offline);
            publish(sys, None);
            let _ = connect(sys, d, None);
            publish(sys, None);
            let delay2 = (delay * 2).min(300);
            next_try.insert(d.letter, (now + Duration::from_secs(delay), delay2));
        }
        std::thread::sleep(Duration::from_secs(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Drive {
        Drive {
            letter: 'N',
            name: "NAS du salon".into(),
            protocol: Protocol::Sftp,
            host: "192.168.1.20".into(),
            port: 22,
            user: "damien".into(),
            path: "/srv/photos".into(),
            auto: true,
            remember: true,
        }
    }

    #[test]
    fn config_round_trip() {
        let mut b = sample();
        b.letter = 'S';
        b.protocol = Protocol::Smb;
        b.port = 445;
        b.path = "Documents".into();
        b.auto = false;
        b.remember = false;
        let text = format_config(&[sample(), b.clone()]);
        assert!(!text.contains("password"));
        assert_eq!(parse_config(&text), vec![sample(), b]);
    }

    #[test]
    fn config_skips_invalid_entries() {
        let text = "[C]\nprotocol=ftp\nhost=x\n[N]\nprotocol=gopher\nhost=x\n[M]\nprotocol=ftp\nhost=-oProxyCommand=evil\n[P]\nprotocol=ftp\nhost=ok.example\n";
        let d = parse_config(text);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].letter, 'P');
        assert_eq!(d[0].port, 21);
    }

    #[test]
    fn validation_blocks_option_injection() {
        let mut d = sample();
        d.user = "bob,allow_other".into();
        assert!(validate(&d).is_err());
        let mut d = sample();
        d.host = "evil host".into();
        assert!(validate(&d).is_err());
        let mut d = sample();
        d.letter = 'Z';
        assert!(validate(&d).is_err());
        let mut d = sample();
        d.protocol = Protocol::Smb;
        d.path = "/".into();
        assert!(validate(&d).is_err());
    }

    #[test]
    fn passwords_are_sealed_and_bound_to_drive() {
        let key = [7u8; 32];
        let d = sample();
        let sealed = seal_password(&key, &d, "mot de passe é").unwrap();
        assert!(!sealed.contains("mot"));
        assert_eq!(open_password(&key, &d, &sealed).unwrap(), "mot de passe é");
        // Même secret recopié sur un autre lecteur : refusé.
        let mut other = d.clone();
        other.host = "autre.example".into();
        assert!(open_password(&key, &other, &sealed).is_none());
        assert!(open_password(&[8u8; 32], &d, &sealed).is_none());
        // Deux scellements du même mot de passe diffèrent (nonce neuf).
        assert_ne!(sealed, seal_password(&key, &d, "mot de passe é").unwrap());
    }

    #[test]
    fn request_parsing_and_defaults() {
        let r = parse_request(
            "letter=n\nprotocol=FTPS\nhost=ftp.example.org\nport=\nuser=moi\npassword=a=b\nremember=0\n",
        );
        assert_eq!(r.get("password").unwrap(), "a=b");
        let d = drive_from_request(&r).unwrap();
        assert_eq!(d.letter, 'N');
        assert_eq!(d.protocol, Protocol::Ftps);
        assert_eq!(d.port, 21);
        assert!(!d.remember);
        assert!(d.auto);
        let r = parse_request("letter=N\nprotocol=sftp\nhost=h\nport=99999\n");
        assert!(drive_from_request(&r).is_err());
    }

    #[test]
    fn registry_row_columns() {
        let m = registry_row(&sample(), Some(Backend::Sshfs), State::Offline);
        assert_eq!(m.kind, "network");
        assert!(m.mountpoint.ends_with("drives/N"));
        assert_eq!(m.label, "NAS du salon");
        assert_eq!(m.source, "sftp://damien@192.168.1.20:22/srv/photos");
        assert_eq!(m.backend, "sshfs");
        assert!(!m.online());
    }

    #[test]
    fn mounts_table_parsing() {
        let m = "sshfs#x /run/winlux/drives/N fuse.sshfs rw 0 0\n\
                 //h/s /run/my\\040drive cifs rw 0 0\n";
        assert!(is_mounted_in(m, Path::new("/run/winlux/drives/N")));
        assert!(is_mounted_in(m, Path::new("/run/my drive")));
        assert!(!is_mounted_in(m, Path::new("/run/winlux/drives/M")));
    }

    #[test]
    fn friendly_errors() {
        assert!(friendly("mount error(13): Permission denied").contains("mot de passe"));
        assert!(friendly("curlftpfs: 530 Login incorrect").contains("mot de passe"));
        assert!(friendly("No such file or directory").contains("dossier distant"));
    }

    #[test]
    fn unreachable_server_fails_fast() {
        let start = Instant::now();
        // Port 1 sur la boucle locale : refus immédiat.
        assert!(!reachable("127.0.0.1", 1, Duration::from_secs(2)));
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}
