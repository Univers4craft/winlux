//! Pont CUPS maison : encodage du protocole IPP (RFC 8010/8011) et transport
//! HTTP vers `cupsd`, sans libcups. Le spooler Winlux (`winspool`) s'en sert
//! pour découvrir les imprimantes, envoyer les travaux et suivre leur état.
//!
//! Le serveur est choisi comme le fait CUPS : variable `CUPS_SERVER` (chemin de
//! socket Unix ou `hôte[:port]`), sinon la socket locale `/run/cups/cups.sock`,
//! sinon `localhost:631`.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::unix::net::UnixStream;
use std::time::Duration;

// ---------- Opérations IPP utilisées ----------

pub const OP_PRINT_JOB: u16 = 0x0002;
pub const OP_CANCEL_JOB: u16 = 0x0008;
pub const OP_GET_JOBS: u16 = 0x000A;
pub const OP_PAUSE_PRINTER: u16 = 0x0010;
pub const OP_RESUME_PRINTER: u16 = 0x0011;
pub const OP_CUPS_GET_DEFAULT: u16 = 0x4001;
pub const OP_CUPS_GET_PRINTERS: u16 = 0x4002;
pub const OP_CUPS_ADD_MODIFY_PRINTER: u16 = 0x4003;
pub const OP_CUPS_DELETE_PRINTER: u16 = 0x4004;
pub const OP_CUPS_ACCEPT_JOBS: u16 = 0x4008;
pub const OP_CUPS_SET_DEFAULT: u16 = 0x400A;
pub const OP_CUPS_GET_DEVICES: u16 = 0x400B;

// ---------- Étiquettes de groupe et de valeur ----------

pub const TAG_OPERATION: u8 = 0x01;
pub const TAG_JOB: u8 = 0x02;
pub const TAG_END: u8 = 0x03;
pub const TAG_PRINTER: u8 = 0x04;

const TAG_INTEGER: u8 = 0x21;
const TAG_BOOLEAN: u8 = 0x22;
const TAG_ENUM: u8 = 0x23;
const TAG_RANGE: u8 = 0x33;
const TAG_RESOLUTION: u8 = 0x32;
const TAG_TEXT: u8 = 0x41;
const TAG_NAME: u8 = 0x42;
const TAG_KEYWORD: u8 = 0x44;
const TAG_URI: u8 = 0x45;
const TAG_CHARSET: u8 = 0x47;
const TAG_LANGUAGE: u8 = 0x48;
const TAG_MIME: u8 = 0x49;

/// Valeur d'un attribut IPP, réduite aux types dont le spooler a besoin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Integer(i32),
    Enum(i32),
    Boolean(bool),
    /// Texte, nom, mot-clé, URI, type MIME… (`tag` d'origine conservé).
    Text(u8, String),
    Range(i32, i32),
    /// Résolution `(x, y, unité)` ; unité 3 = dpi.
    Resolution(i32, i32, u8),
    /// Valeur non interprétée (date, collection…), gardée brute.
    Other(u8, Vec<u8>),
}

impl Value {
    pub fn as_int(&self) -> Option<i32> {
        match self {
            Value::Integer(v) | Value::Enum(v) => Some(*v),
            Value::Boolean(b) => Some(i32::from(*b)),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Text(_, s) => Some(s),
            _ => None,
        }
    }

    fn tag(&self) -> u8 {
        match self {
            Value::Integer(_) => TAG_INTEGER,
            Value::Enum(_) => TAG_ENUM,
            Value::Boolean(_) => TAG_BOOLEAN,
            Value::Text(t, _) | Value::Other(t, _) => *t,
            Value::Range(..) => TAG_RANGE,
            Value::Resolution(..) => TAG_RESOLUTION,
        }
    }

    fn encode(&self, out: &mut Vec<u8>) {
        let bytes: Vec<u8> = match self {
            Value::Integer(v) | Value::Enum(v) => v.to_be_bytes().to_vec(),
            Value::Boolean(b) => vec![u8::from(*b)],
            Value::Text(_, s) => s.as_bytes().to_vec(),
            Value::Range(a, b) => [a.to_be_bytes(), b.to_be_bytes()].concat(),
            Value::Resolution(x, y, u) => {
                let mut v = [x.to_be_bytes(), y.to_be_bytes()].concat();
                v.push(*u);
                v
            }
            Value::Other(_, b) => b.clone(),
        };
        out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        out.extend_from_slice(&bytes);
    }

    fn decode(tag: u8, b: &[u8]) -> Value {
        let int =
            |b: &[u8]| -> Option<i32> { Some(i32::from_be_bytes(b.get(..4)?.try_into().ok()?)) };
        match tag {
            TAG_INTEGER if b.len() == 4 => Value::Integer(int(b).unwrap_or(0)),
            TAG_ENUM if b.len() == 4 => Value::Enum(int(b).unwrap_or(0)),
            TAG_BOOLEAN if b.len() == 1 => Value::Boolean(b[0] != 0),
            TAG_RANGE if b.len() == 8 => {
                Value::Range(int(&b[..4]).unwrap_or(0), int(&b[4..]).unwrap_or(0))
            }
            TAG_RESOLUTION if b.len() == 9 => {
                Value::Resolution(int(&b[..4]).unwrap_or(0), int(&b[4..8]).unwrap_or(0), b[8])
            }
            0x35 | 0x36 => {
                // texte/nom avec langue : longueur + langue + longueur + texte
                let text = (|| {
                    let ll = u16::from_be_bytes(b.get(..2)?.try_into().ok()?) as usize;
                    let rest = b.get(2 + ll..)?;
                    let tl = u16::from_be_bytes(rest.get(..2)?.try_into().ok()?) as usize;
                    Some(String::from_utf8_lossy(rest.get(2..2 + tl)?).into_owned())
                })();
                match text {
                    Some(t) => Value::Text(if tag == 0x35 { TAG_TEXT } else { TAG_NAME }, t),
                    None => Value::Other(tag, b.to_vec()),
                }
            }
            0x40..=0x5f => Value::Text(tag, String::from_utf8_lossy(b).into_owned()),
            _ => Value::Other(tag, b.to_vec()),
        }
    }
}

/// Attribut IPP (un nom, une ou plusieurs valeurs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    pub name: String,
    pub values: Vec<Value>,
}

/// Groupe d'attributs (opération, travail, imprimante…).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub tag: u8,
    pub attrs: Vec<Attribute>,
}

impl Group {
    pub fn get(&self, name: &str) -> Option<&Attribute> {
        self.attrs.iter().find(|a| a.name == name)
    }

    pub fn str(&self, name: &str) -> Option<&str> {
        self.get(name)?.values.first()?.as_str()
    }

    pub fn int(&self, name: &str) -> Option<i32> {
        self.get(name)?.values.first()?.as_int()
    }

    pub fn strings(&self, name: &str) -> Vec<String> {
        self.get(name)
            .map(|a| {
                a.values
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Message IPP complet (requête ou réponse).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub version: (u8, u8),
    /// Opération (requête) ou statut (réponse).
    pub code: u16,
    pub request_id: u32,
    pub groups: Vec<Group>,
    /// Données qui suivent l'en-tête (document à imprimer).
    pub data: Vec<u8>,
}

impl Message {
    /// Requête avec les attributs d'opération obligatoires (charset, langue).
    pub fn request(op: u16, request_id: u32) -> Message {
        let mut m = Message {
            version: (2, 0),
            code: op,
            request_id,
            groups: vec![Group {
                tag: TAG_OPERATION,
                attrs: Vec::new(),
            }],
            data: Vec::new(),
        };
        m.add(
            TAG_OPERATION,
            "attributes-charset",
            text(TAG_CHARSET, "utf-8"),
        );
        m.add(
            TAG_OPERATION,
            "attributes-natural-language",
            text(TAG_LANGUAGE, "fr"),
        );
        m
    }

    /// Ajoute un attribut au dernier groupe `tag` (créé au besoin).
    pub fn add(&mut self, tag: u8, name: &str, value: Value) -> &mut Self {
        self.add_values(tag, name, vec![value])
    }

    pub fn add_values(&mut self, tag: u8, name: &str, values: Vec<Value>) -> &mut Self {
        if self.groups.last().map(|g| g.tag) != Some(tag) {
            self.groups.push(Group {
                tag,
                attrs: Vec::new(),
            });
        }
        if let Some(g) = self.groups.last_mut() {
            g.attrs.push(Attribute {
                name: name.to_string(),
                values,
            });
        }
        self
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![self.version.0, self.version.1];
        out.extend_from_slice(&self.code.to_be_bytes());
        out.extend_from_slice(&self.request_id.to_be_bytes());
        for g in &self.groups {
            out.push(g.tag);
            for a in &g.attrs {
                for (i, v) in a.values.iter().enumerate() {
                    out.push(v.tag());
                    let name = if i == 0 { a.name.as_bytes() } else { b"" };
                    out.extend_from_slice(&(name.len() as u16).to_be_bytes());
                    out.extend_from_slice(name);
                    v.encode(&mut out);
                }
            }
        }
        out.push(TAG_END);
        out.extend_from_slice(&self.data);
        out
    }

    pub fn decode(b: &[u8]) -> Option<Message> {
        if b.len() < 9 {
            return None;
        }
        let mut m = Message {
            version: (b[0], b[1]),
            code: u16::from_be_bytes([b[2], b[3]]),
            request_id: u32::from_be_bytes([b[4], b[5], b[6], b[7]]),
            groups: Vec::new(),
            data: Vec::new(),
        };
        let mut i = 8;
        // Profondeur des collections ignorées (begCollection … endCollection).
        let mut skip_depth = 0usize;
        loop {
            let tag = *b.get(i)?;
            i += 1;
            if tag == TAG_END {
                m.data = b[i..].to_vec();
                return Some(m);
            }
            if tag < 0x10 {
                m.groups.push(Group {
                    tag,
                    attrs: Vec::new(),
                });
                continue;
            }
            let nl = u16::from_be_bytes(b.get(i..i + 2)?.try_into().ok()?) as usize;
            let name = String::from_utf8_lossy(b.get(i + 2..i + 2 + nl)?).into_owned();
            i += 2 + nl;
            let vl = u16::from_be_bytes(b.get(i..i + 2)?.try_into().ok()?) as usize;
            let raw = b.get(i + 2..i + 2 + vl)?;
            i += 2 + vl;
            match tag {
                0x34 => {
                    skip_depth += 1;
                    if skip_depth == 1 {
                        push_value(&mut m, &name, Value::Other(tag, Vec::new()));
                    }
                    continue;
                }
                0x37 => {
                    skip_depth = skip_depth.saturating_sub(1);
                    continue;
                }
                _ if skip_depth > 0 => continue,
                _ => {}
            }
            push_value(&mut m, &name, Value::decode(tag, raw));
        }
    }

    /// Statut de réussite IPP (`successful-ok*`).
    pub fn ok(&self) -> bool {
        self.code < 0x0100
    }

    pub fn groups(&self, tag: u8) -> impl Iterator<Item = &Group> {
        self.groups.iter().filter(move |g| g.tag == tag)
    }
}

fn push_value(m: &mut Message, name: &str, value: Value) {
    let Some(g) = m.groups.last_mut() else {
        return;
    };
    if name.is_empty() {
        if let Some(a) = g.attrs.last_mut() {
            a.values.push(value);
        }
    } else {
        g.attrs.push(Attribute {
            name: name.to_string(),
            values: vec![value],
        });
    }
}

pub fn text(tag: u8, s: &str) -> Value {
    Value::Text(tag, s.to_string())
}
pub fn name(s: &str) -> Value {
    text(TAG_NAME, s)
}
pub fn keyword(s: &str) -> Value {
    text(TAG_KEYWORD, s)
}
pub fn uri(s: &str) -> Value {
    text(TAG_URI, s)
}
pub fn mime(s: &str) -> Value {
    text(TAG_MIME, s)
}
pub fn text_value(s: &str) -> Value {
    text(TAG_TEXT, s)
}

// ---------- Transport HTTP ----------

/// Destination du serveur CUPS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Server {
    Unix(String),
    Tcp(String, u16),
}

impl Server {
    /// Serveur configuré : `CUPS_SERVER`, puis socket locale, puis TCP 631.
    pub fn detect() -> Server {
        if let Ok(s) = std::env::var("CUPS_SERVER") {
            if let Some(server) = Server::parse(&s) {
                return server;
            }
        }
        for path in ["/run/cups/cups.sock", "/var/run/cups/cups.sock"] {
            if std::path::Path::new(path).exists() {
                return Server::Unix(path.to_string());
            }
        }
        Server::Tcp("localhost".into(), 631)
    }

    pub fn parse(s: &str) -> Option<Server> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        if s.starts_with('/') {
            return Some(Server::Unix(s.to_string()));
        }
        match s.rsplit_once(':') {
            Some((h, p)) if !h.contains(']') || h.ends_with(']') => match p.parse() {
                Ok(port) => Some(Server::Tcp(h.to_string(), port)),
                Err(_) => Some(Server::Tcp(s.to_string(), 631)),
            },
            _ => Some(Server::Tcp(s.to_string(), 631)),
        }
    }

    /// Nom d'hôte à mettre dans les URI `ipp://` envoyées au serveur.
    pub fn host(&self) -> String {
        match self {
            Server::Unix(_) => "localhost".into(),
            Server::Tcp(h, 631) => h.clone(),
            Server::Tcp(h, p) => format!("{h}:{p}"),
        }
    }
}

trait Stream: Read + Write {}
impl<T: Read + Write> Stream for T {}

fn connect(server: &Server, timeout: Duration) -> std::io::Result<Box<dyn Stream>> {
    match server {
        Server::Unix(path) => {
            let s = UnixStream::connect(path)?;
            s.set_read_timeout(Some(timeout))?;
            s.set_write_timeout(Some(timeout))?;
            Ok(Box::new(s))
        }
        Server::Tcp(host, port) => {
            use std::net::ToSocketAddrs;
            let host = host.trim_start_matches('[').trim_end_matches(']');
            let mut last = std::io::Error::other("adresse introuvable");
            for addr in (host, *port).to_socket_addrs()? {
                match TcpStream::connect_timeout(&addr, timeout.min(Duration::from_secs(3))) {
                    Ok(s) => {
                        s.set_read_timeout(Some(timeout))?;
                        s.set_write_timeout(Some(timeout))?;
                        return Ok(Box::new(s));
                    }
                    Err(e) => last = e,
                }
            }
            Err(last)
        }
    }
}

/// Utilisateur local (pour `requesting-user-name` et l'authentification PeerCred).
pub fn user_name() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| {
            // SAFETY: getuid/getpwuid lisent la base des comptes du processus.
            unsafe {
                let pw = libc::getpwuid(libc::getuid());
                if pw.is_null() || (*pw).pw_name.is_null() {
                    "winlux".into()
                } else {
                    std::ffi::CStr::from_ptr((*pw).pw_name)
                        .to_string_lossy()
                        .into_owned()
                }
            }
        })
}

/// Découpe une réponse HTTP : (code, corps), corps « chunked » décodé.
fn parse_http(raw: &[u8]) -> Option<(u16, Vec<u8>)> {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&raw[..end]);
    let mut lines = head.split("\r\n");
    let status: u16 = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    let mut chunked = false;
    let mut length = None;
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            let k = k.trim().to_ascii_lowercase();
            let v = v.trim();
            if k == "transfer-encoding" && v.eq_ignore_ascii_case("chunked") {
                chunked = true;
            } else if k == "content-length" {
                length = v.parse::<usize>().ok();
            }
        }
    }
    let body = &raw[end + 4..];
    if chunked {
        let mut out = Vec::new();
        let mut i = 0;
        loop {
            let le = body.get(i..)?.windows(2).position(|w| w == b"\r\n")?;
            let size_text = String::from_utf8_lossy(&body[i..i + le]);
            let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
            i += le + 2;
            if size == 0 {
                return Some((status, out));
            }
            out.extend_from_slice(body.get(i..i + size)?);
            i += size + 2;
        }
    }
    let body = match length {
        Some(n) => body.get(..n)?,
        None => body,
    };
    Some((status, body.to_vec()))
}

/// Erreur d'un échange IPP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IppError {
    /// CUPS injoignable (service absent, socket fermée).
    Unavailable(String),
    /// Réponse HTTP non 200 (401 = authentification, 403 = refus).
    Http(u16),
    /// Réponse IPP illisible.
    Protocol,
    /// Statut IPP d'échec (`client-error-*`, `server-error-*`).
    Status(u16, String),
}

impl std::fmt::Display for IppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IppError::Unavailable(e) => write!(f, "service d'impression indisponible ({e})"),
            IppError::Http(401) => write!(f, "authentification requise"),
            IppError::Http(403) => write!(f, "opération refusée par CUPS"),
            IppError::Http(c) => write!(f, "erreur HTTP {c}"),
            IppError::Protocol => write!(f, "réponse IPP invalide"),
            IppError::Status(c, m) if m.is_empty() => write!(f, "statut IPP {c:#06x}"),
            IppError::Status(_, m) => write!(f, "{m}"),
        }
    }
}

/// Client IPP vers un serveur CUPS.
pub struct Client {
    pub server: Server,
    pub timeout: Duration,
    next_id: std::sync::atomic::AtomicU32,
}

impl Client {
    pub fn new(server: Server) -> Client {
        Client {
            server,
            timeout: Duration::from_secs(5),
            next_id: std::sync::atomic::AtomicU32::new(1),
        }
    }

    pub fn detect() -> Client {
        Client::new(Server::detect())
    }

    pub fn request(&self, op: u16) -> Message {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Message::request(op, id)
    }

    /// URI d'une file CUPS (`ipp://hôte/printers/nom`).
    pub fn printer_uri(&self, printer: &str) -> String {
        format!(
            "ipp://{}/printers/{}",
            self.server.host(),
            percent_encode(printer)
        )
    }

    /// Envoie une requête et renvoie la réponse décodée (statut IPP vérifié).
    pub fn send(&self, path: &str, msg: &Message) -> Result<Message, IppError> {
        let body = msg.encode();
        let mut status = self.post(path, &body, None)?;
        if status.0 == 401 && matches!(self.server, Server::Unix(_)) {
            // CUPS accepte l'identité du pair sur sa socket Unix locale.
            status = self.post(path, &body, Some(format!("PeerCred {}", user_name())))?;
        }
        let (code, payload) = status;
        if code != 200 {
            return Err(IppError::Http(code));
        }
        let reply = Message::decode(&payload).ok_or(IppError::Protocol)?;
        if !reply.ok() {
            let message = reply
                .groups(TAG_OPERATION)
                .find_map(|g| g.str("status-message").map(str::to_string))
                .unwrap_or_default();
            return Err(IppError::Status(reply.code, message));
        }
        Ok(reply)
    }

    fn post(
        &self,
        path: &str,
        body: &[u8],
        auth: Option<String>,
    ) -> Result<(u16, Vec<u8>), IppError> {
        let mut s = connect(&self.server, self.timeout)
            .map_err(|e| IppError::Unavailable(e.to_string()))?;
        let mut head = format!(
            "POST {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/ipp\r\n\
             Content-Length: {}\r\nConnection: close\r\nUser-Agent: Winlux-Spooler/1.0\r\n",
            self.server.host(),
            body.len()
        );
        if let Some(a) = auth {
            head.push_str(&format!("Authorization: {a}\r\n"));
        }
        head.push_str("\r\n");
        let io = |e: std::io::Error| IppError::Unavailable(e.to_string());
        s.write_all(head.as_bytes()).map_err(io)?;
        s.write_all(body).map_err(io)?;
        s.flush().map_err(io)?;
        let mut raw = Vec::new();
        let mut buf = [0u8; 16384];
        loop {
            match s.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    raw.extend_from_slice(&buf[..n]);
                    // Réponse complète dès que Content-Length est atteint.
                    if let Some((_, b)) = parse_http(&raw) {
                        if http_complete(&raw, b.len()) {
                            break;
                        }
                    }
                }
                Err(e) if raw.is_empty() => return Err(io(e)),
                Err(_) => break,
            }
        }
        parse_http(&raw).ok_or(IppError::Protocol)
    }
}

/// Vrai quand le corps reçu atteint la longueur annoncée (évite d'attendre la
/// fermeture si le serveur garde la connexion ouverte).
fn http_complete(raw: &[u8], body_len: usize) -> bool {
    let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
        return false;
    };
    let head = String::from_utf8_lossy(&raw[..end]).to_ascii_lowercase();
    if head.contains("transfer-encoding: chunked") {
        return raw.ends_with(b"0\r\n\r\n");
    }
    head.lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse::<usize>().ok())
        .is_some_and(|n| body_len >= n)
}

pub fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

// ---------- Opérations CUPS de haut niveau ----------

/// File d'impression vue par CUPS.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CupsPrinter {
    pub name: String,
    pub info: String,
    pub location: String,
    pub make_model: String,
    pub device_uri: String,
    /// 3 = inactive, 4 = en cours, 5 = arrêtée.
    pub state: i32,
    pub state_message: String,
    pub accepting: bool,
    pub color: bool,
    pub queued_jobs: i32,
    pub media: Vec<String>,
    pub media_default: String,
    pub duplex: bool,
    pub resolutions: Vec<(i32, i32)>,
}

impl CupsPrinter {
    fn from_group(g: &Group) -> Option<CupsPrinter> {
        Some(CupsPrinter {
            name: g.str("printer-name")?.to_string(),
            info: g.str("printer-info").unwrap_or_default().to_string(),
            location: g.str("printer-location").unwrap_or_default().to_string(),
            make_model: g
                .str("printer-make-and-model")
                .unwrap_or_default()
                .to_string(),
            device_uri: g.str("device-uri").unwrap_or_default().to_string(),
            state: g.int("printer-state").unwrap_or(3),
            state_message: g
                .str("printer-state-message")
                .unwrap_or_default()
                .to_string(),
            accepting: g.int("printer-is-accepting-jobs").unwrap_or(1) != 0,
            color: g.int("color-supported").unwrap_or(0) != 0,
            queued_jobs: g.int("queued-job-count").unwrap_or(0),
            media: g.strings("media-supported"),
            media_default: g.str("media-default").unwrap_or_default().to_string(),
            duplex: g
                .strings("sides-supported")
                .iter()
                .any(|s| s.starts_with("two-sided")),
            resolutions: g
                .get("printer-resolution-supported")
                .map(|a| {
                    a.values
                        .iter()
                        .filter_map(|v| match v {
                            Value::Resolution(x, y, 3) => Some((*x, *y)),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

const PRINTER_ATTRS: [&str; 16] = [
    "printer-name",
    "printer-info",
    "printer-location",
    "printer-make-and-model",
    "device-uri",
    "printer-state",
    "printer-state-message",
    "printer-is-accepting-jobs",
    "color-supported",
    "queued-job-count",
    "media-supported",
    "media-default",
    "sides-supported",
    "printer-resolution-supported",
    "printer-type",
    "printer-uri-supported",
];

/// Travail d'impression vu par CUPS.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CupsJob {
    pub id: i32,
    pub name: String,
    pub user: String,
    pub printer: String,
    /// 3 en attente, 4 retenu, 5 en cours, 6 arrêté, 7 annulé, 8 abandonné, 9 terminé.
    pub state: i32,
    pub size_k: i32,
    pub pages_done: i32,
    pub created: i64,
}

impl Client {
    pub fn printers(&self) -> Result<Vec<CupsPrinter>, IppError> {
        let mut m = self.request(OP_CUPS_GET_PRINTERS);
        m.add_values(
            TAG_OPERATION,
            "requested-attributes",
            PRINTER_ATTRS.iter().map(|a| keyword(a)).collect(),
        );
        let reply = self.send("/", &m)?;
        Ok(reply
            .groups(TAG_PRINTER)
            .filter_map(CupsPrinter::from_group)
            .collect())
    }

    pub fn default_printer(&self) -> Result<Option<String>, IppError> {
        let mut m = self.request(OP_CUPS_GET_DEFAULT);
        m.add(
            TAG_OPERATION,
            "requested-attributes",
            keyword("printer-name"),
        );
        match self.send("/", &m) {
            Ok(r) => Ok(r
                .groups(TAG_PRINTER)
                .find_map(|g| g.str("printer-name").map(str::to_string))),
            // client-error-not-found : aucune imprimante par défaut.
            Err(IppError::Status(0x0406, _)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Envoie un document ; renvoie l'identifiant du travail CUPS.
    pub fn print_job(
        &self,
        printer: &str,
        title: &str,
        format: &str,
        options: &JobOptions,
        document: Vec<u8>,
    ) -> Result<i32, IppError> {
        let mut m = self.request(OP_PRINT_JOB);
        m.add(
            TAG_OPERATION,
            "printer-uri",
            uri(&self.printer_uri(printer)),
        );
        m.add(TAG_OPERATION, "requesting-user-name", name(&user_name()));
        m.add(TAG_OPERATION, "job-name", name(title));
        m.add(TAG_OPERATION, "document-format", mime(format));
        options.encode(&mut m);
        m.data = document;
        let mut client = Client::new(self.server.clone());
        client.timeout = Duration::from_secs(60);
        let reply = client.send(&format!("/printers/{}", percent_encode(printer)), &m)?;
        let id = reply.groups(TAG_JOB).find_map(|g| g.int("job-id"));
        id.ok_or(IppError::Protocol)
    }

    pub fn jobs(&self, printer: Option<&str>) -> Result<Vec<CupsJob>, IppError> {
        let mut m = self.request(OP_GET_JOBS);
        let target = match printer {
            Some(p) => self.printer_uri(p),
            None => format!("ipp://{}/", self.server.host()),
        };
        m.add(TAG_OPERATION, "printer-uri", uri(&target));
        m.add(TAG_OPERATION, "requesting-user-name", name(&user_name()));
        m.add(TAG_OPERATION, "which-jobs", keyword("not-completed"));
        m.add_values(
            TAG_OPERATION,
            "requested-attributes",
            [
                "job-id",
                "job-name",
                "job-state",
                "job-originating-user-name",
                "job-k-octets",
                "job-printer-uri",
                "job-media-sheets-completed",
                "time-at-creation",
            ]
            .iter()
            .map(|a| keyword(a))
            .collect(),
        );
        let reply = self.send("/", &m)?;
        Ok(reply
            .groups(TAG_JOB)
            .filter_map(|g| {
                Some(CupsJob {
                    id: g.int("job-id")?,
                    name: g.str("job-name").unwrap_or_default().to_string(),
                    user: g
                        .str("job-originating-user-name")
                        .unwrap_or_default()
                        .to_string(),
                    printer: g
                        .str("job-printer-uri")
                        .and_then(|u| u.rsplit('/').next())
                        .map(percent_decode)
                        .unwrap_or_default(),
                    state: g.int("job-state").unwrap_or(3),
                    size_k: g.int("job-k-octets").unwrap_or(0),
                    pages_done: g.int("job-media-sheets-completed").unwrap_or(0),
                    created: i64::from(g.int("time-at-creation").unwrap_or(0)),
                })
            })
            .collect())
    }

    pub fn cancel_job(&self, printer: &str, job: i32) -> Result<(), IppError> {
        let mut m = self.request(OP_CANCEL_JOB);
        m.add(
            TAG_OPERATION,
            "printer-uri",
            uri(&self.printer_uri(printer)),
        );
        m.add(TAG_OPERATION, "job-id", Value::Integer(job));
        m.add(TAG_OPERATION, "requesting-user-name", name(&user_name()));
        self.send("/jobs/", &m).map(|_| ())
    }

    fn printer_op(&self, op: u16, printer: &str) -> Result<(), IppError> {
        let mut m = self.request(op);
        m.add(
            TAG_OPERATION,
            "printer-uri",
            uri(&self.printer_uri(printer)),
        );
        m.add(TAG_OPERATION, "requesting-user-name", name(&user_name()));
        self.send("/admin/", &m).map(|_| ())
    }

    pub fn pause_printer(&self, printer: &str) -> Result<(), IppError> {
        self.printer_op(OP_PAUSE_PRINTER, printer)
    }

    pub fn resume_printer(&self, printer: &str) -> Result<(), IppError> {
        self.printer_op(OP_RESUME_PRINTER, printer)
    }

    pub fn delete_printer(&self, printer: &str) -> Result<(), IppError> {
        self.printer_op(OP_CUPS_DELETE_PRINTER, printer)
    }

    pub fn set_default(&self, printer: &str) -> Result<(), IppError> {
        self.printer_op(OP_CUPS_SET_DEFAULT, printer)
    }

    /// Crée (ou modifie) une file pilotée sans pilote (« everywhere », IPP
    /// Everywhere/AirPrint) vers `device_uri`, puis l'active.
    pub fn add_printer(
        &self,
        printer: &str,
        device_uri: &str,
        info: &str,
        location: &str,
    ) -> Result<(), IppError> {
        let mut m = self.request(OP_CUPS_ADD_MODIFY_PRINTER);
        m.add(
            TAG_OPERATION,
            "printer-uri",
            uri(&self.printer_uri(printer)),
        );
        m.add(TAG_OPERATION, "requesting-user-name", name(&user_name()));
        m.add(TAG_OPERATION, "ppd-name", name("everywhere"));
        m.add(TAG_PRINTER, "device-uri", uri(device_uri));
        m.add(TAG_PRINTER, "printer-info", text_value(info));
        m.add(TAG_PRINTER, "printer-location", text_value(location));
        m.add(
            TAG_PRINTER,
            "printer-is-accepting-jobs",
            Value::Boolean(true),
        );
        m.add(TAG_PRINTER, "printer-state", Value::Enum(3));
        self.send("/admin/", &m)?;
        // Certaines versions ignorent l'état demandé : on l'active explicitement.
        let _ = self.printer_op(OP_CUPS_ACCEPT_JOBS, printer);
        let _ = self.resume_printer(printer);
        Ok(())
    }

    /// Périphériques détectés par CUPS (USB, réseau via DNS-SD…).
    pub fn devices(&self, timeout_s: i32) -> Result<Vec<Device>, IppError> {
        let mut m = self.request(OP_CUPS_GET_DEVICES);
        m.add(TAG_OPERATION, "timeout", Value::Integer(timeout_s));
        let mut client = Client::new(self.server.clone());
        client.timeout = Duration::from_secs(timeout_s.max(1) as u64 + 10);
        let reply = client.send("/", &m)?;
        Ok(reply
            .groups(TAG_PRINTER)
            .filter_map(|g| {
                let uri = g.str("device-uri")?.to_string();
                Some(Device {
                    class: g.str("device-class").unwrap_or_default().to_string(),
                    info: g.str("device-info").unwrap_or_default().to_string(),
                    make_model: g
                        .str("device-make-and-model")
                        .unwrap_or_default()
                        .to_string(),
                    uri,
                })
            })
            // Les « backends » génériques sans imprimante réelle sont ignorés.
            .filter(|d| d.uri.contains(':') && !d.uri.ends_with(':') && d.make_model != "Unknown")
            .collect())
    }
}

/// Imprimante détectée mais pas encore installée.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Device {
    pub class: String,
    pub info: String,
    pub make_model: String,
    pub uri: String,
}

impl Device {
    /// `true` pour une imprimante USB/locale, `false` pour le réseau.
    pub fn is_usb(&self) -> bool {
        self.uri.starts_with("usb:") || self.uri.starts_with("parallel:")
    }
}

/// Options de travail traduites depuis le DEVMODE Windows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobOptions {
    pub copies: i32,
    pub landscape: bool,
    /// 1 recto, 2 recto-verso bord long, 3 bord court (valeurs `DMDUP_*`).
    pub duplex: i16,
    pub color: bool,
    /// 3 brouillon, 4 normale, 5 haute (enum IPP `print-quality`).
    pub quality: i32,
    pub media: String,
    pub collate: bool,
}

impl Default for JobOptions {
    fn default() -> Self {
        JobOptions {
            copies: 1,
            landscape: false,
            duplex: 1,
            color: true,
            quality: 4,
            media: "iso_a4_210x297mm".into(),
            collate: true,
        }
    }
}

impl JobOptions {
    fn encode(&self, m: &mut Message) {
        m.add(TAG_JOB, "copies", Value::Integer(self.copies.max(1)));
        // Le PDF porte déjà l'orientation des pages : CUPS ne doit pas la
        // retourner une seconde fois, on l'indique seulement pour l'affichage.
        m.add(
            TAG_JOB,
            "sides",
            keyword(match self.duplex {
                2 => "two-sided-long-edge",
                3 => "two-sided-short-edge",
                _ => "one-sided",
            }),
        );
        m.add(
            TAG_JOB,
            "print-color-mode",
            keyword(if self.color { "color" } else { "monochrome" }),
        );
        m.add(
            TAG_JOB,
            "print-quality",
            Value::Enum(self.quality.clamp(3, 5)),
        );
        if !self.media.is_empty() {
            m.add(TAG_JOB, "media", keyword(&self.media));
        }
        if self.copies > 1 {
            m.add(
                TAG_JOB,
                "multiple-document-handling",
                keyword(if self.collate {
                    "separate-documents-collated-copies"
                } else {
                    "separate-documents-uncollated-copies"
                }),
            );
        }
    }
}

pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok();
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn encodage_et_decodage_ipp() {
        let mut m = Message::request(OP_PRINT_JOB, 7);
        m.add(
            TAG_OPERATION,
            "printer-uri",
            uri("ipp://localhost/printers/P"),
        );
        m.add(TAG_JOB, "copies", Value::Integer(2));
        m.add_values(
            TAG_OPERATION,
            "requested-attributes",
            vec![keyword("a"), keyword("b")],
        );
        m.data = b"%PDF".to_vec();
        let d = Message::decode(&m.encode()).unwrap();
        assert_eq!(d.code, OP_PRINT_JOB);
        assert_eq!(d.request_id, 7);
        assert_eq!(d.data, b"%PDF");
        let op = d.groups(TAG_OPERATION).next().unwrap();
        assert_eq!(op.str("attributes-charset"), Some("utf-8"));
        assert_eq!(op.str("printer-uri"), Some("ipp://localhost/printers/P"));
        // Le groupe travail s'intercale : le dernier attribut d'opération
        // ouvre un second groupe opération, comme l'autorise la RFC.
        let all: Vec<_> = d.groups(TAG_OPERATION).collect();
        assert_eq!(all[1].strings("requested-attributes"), vec!["a", "b"]);
        assert_eq!(d.groups(TAG_JOB).next().unwrap().int("copies"), Some(2));
    }

    #[test]
    fn decode_collections_et_resolutions() {
        let mut m = Message::request(0, 1);
        m.add(TAG_PRINTER, "printer-name", name("Bureau"));
        m.add_values(
            TAG_PRINTER,
            "printer-resolution-supported",
            vec![
                Value::Resolution(300, 300, 3),
                Value::Resolution(600, 600, 3),
            ],
        );
        let mut raw = m.encode();
        raw.pop(); // retirer TAG_END pour ajouter une collection
        raw.push(0x34);
        raw.extend_from_slice(&[0, 3]);
        raw.extend_from_slice(b"col");
        raw.extend_from_slice(&[0, 0]);
        raw.push(0x4a);
        raw.extend_from_slice(&[0, 0, 0, 1, b'x']);
        raw.push(0x21);
        raw.extend_from_slice(&[0, 0, 0, 4, 0, 0, 0, 5]);
        raw.push(0x37);
        raw.extend_from_slice(&[0, 0, 0, 0]);
        raw.push(TAG_END);
        let d = Message::decode(&raw).unwrap();
        let p = CupsPrinter::from_group(d.groups(TAG_PRINTER).next().unwrap()).unwrap();
        assert_eq!(p.name, "Bureau");
        assert_eq!(p.resolutions, vec![(300, 300), (600, 600)]);
    }

    #[test]
    fn http_chunked() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n";
        assert_eq!(parse_http(raw), Some((200, b"abcde".to_vec())));
        assert!(http_complete(raw, 5));
    }

    #[test]
    fn serveur_cups_parse() {
        assert_eq!(
            Server::parse("/run/cups/cups.sock"),
            Some(Server::Unix("/run/cups/cups.sock".into()))
        );
        assert_eq!(
            Server::parse("imprimantes:8631"),
            Some(Server::Tcp("imprimantes".into(), 8631))
        );
        assert_eq!(
            Server::parse("serveur"),
            Some(Server::Tcp("serveur".into(), 631))
        );
        assert_eq!(percent_encode("HP Bureau"), "HP%20Bureau");
        assert_eq!(percent_decode("HP%20Bureau"), "HP Bureau");
    }

    /// Faux `cupsd` : répond une fois à chaque requête par `reply(req)`.
    pub(crate) fn fake_cups(
        reply: impl Fn(&Message) -> Message + Send + 'static,
        count: usize,
    ) -> (Server, std::thread::JoinHandle<Vec<Message>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for _ in 0..count {
                let (mut s, _) = l.accept().unwrap();
                let mut raw = Vec::new();
                let mut buf = [0u8; 8192];
                loop {
                    let n = s.read(&mut buf).unwrap();
                    raw.extend_from_slice(&buf[..n]);
                    let body = raw
                        .windows(4)
                        .position(|w| w == b"\r\n\r\n")
                        .map(|e| raw.len() - e - 4);
                    if n == 0 || body.is_some_and(|len| http_complete(&raw, len)) {
                        break;
                    }
                }
                let end = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
                let req = Message::decode(&raw[end + 4..]).unwrap();
                let body = reply(&req).encode();
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );
                s.write_all(head.as_bytes()).unwrap();
                s.write_all(&body).unwrap();
                seen.push(req);
            }
            seen
        });
        (Server::Tcp("127.0.0.1".into(), port), h)
    }

    #[test]
    fn client_liste_et_impression() {
        let (server, h) = fake_cups(
            |req| {
                let mut r = Message::request(0, req.request_id);
                match req.code {
                    OP_CUPS_GET_PRINTERS => {
                        r.add(TAG_PRINTER, "printer-name", name("Laser"));
                        r.add(TAG_PRINTER, "printer-state", Value::Enum(3));
                        r.add(TAG_PRINTER, "color-supported", Value::Boolean(true));
                        r.groups.push(Group {
                            tag: TAG_PRINTER,
                            attrs: Vec::new(),
                        });
                        r.add(TAG_PRINTER, "printer-name", name("Jet"));
                    }
                    OP_PRINT_JOB => {
                        r.add(TAG_JOB, "job-id", Value::Integer(42));
                    }
                    _ => r.code = 0x0400,
                }
                r
            },
            2,
        );
        let c = Client::new(server);
        let printers = c.printers().unwrap();
        assert_eq!(printers.len(), 2);
        assert_eq!(printers[0].name, "Laser");
        assert!(printers[0].color);
        assert_eq!(printers[1].name, "Jet");
        let id = c
            .print_job(
                "Laser",
                "Doc",
                "application/pdf",
                &JobOptions::default(),
                b"%PDF-1.4".to_vec(),
            )
            .unwrap();
        assert_eq!(id, 42);
        let seen = h.join().unwrap();
        assert_eq!(seen[1].data, b"%PDF-1.4");
        let job = seen[1].groups(TAG_JOB).next().unwrap();
        assert_eq!(job.str("sides"), Some("one-sided"));
        assert_eq!(job.int("print-quality"), Some(4));
    }

    #[test]
    fn cups_absent() {
        let c = Client::new(Server::Unix("/nonexistent/winlux-cups.sock".into()));
        assert!(matches!(c.printers(), Err(IppError::Unavailable(_))));
    }
}
