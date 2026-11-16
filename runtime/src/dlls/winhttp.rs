//! Synchronous WinHTTP subset backed by the OS curl client, with certificate
//! verification preserved. Uploads are buffered until ReceiveResponse; response
//! bodies are file-backed. Async callbacks, PAC/WPAD and client certificates are
//! deliberately unsupported rather than reported as successful operations.
use std::collections::HashMap;
use std::ffi::c_void;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const MAX_UPLOAD: usize = 64 * 1024 * 1024;
const MAX_WIDE: usize = 32768;
const SECURE: u32 = 0x0080_0000;
static NEXT: AtomicUsize = AtomicUsize::new(0x5748_0000);

#[derive(Clone)]
struct Options {
    agent: String,
    proxy: Option<String>,
    bypass: String,
    /// Accès par défaut ou automatique : proxy système commun à Linux.
    system_proxy: bool,
    connect_ms: i32,
    total_ms: i32,
    context: usize,
    /// WINHTTP_OPTION_SECURE_PROTOCOLS : versions TLS 1.x permises
    /// (minimum, maximum), x de 0 à 3 (#2609).
    tls: Option<(u8, u8)>,
    /// Rappel de WinHttpSetStatusCallback, hérité par les poignées filles
    /// ouvertes ensuite (#2619). Jamais appelé : Winlux n'ouvre que des
    /// sessions synchrones, où chaque appel rend lui-même son erreur.
    callback: usize,
    /// WINHTTP_OPTION_REDIRECT_POLICY (#2626) : 0 jamais, 1 tout sauf
    /// https vers http (valeur par défaut), 2 toujours.
    redirect_policy: u8,
    /// WINHTTP_OPTION_DISABLE_FEATURE (#2626) : drapeaux WINHTTP_DISABLE_*.
    disabled: u32,
}
#[derive(Clone)]
struct Request {
    options: Options,
    host: String,
    port: u16,
    secure: bool,
    verb: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    expected: u32,
    sent: bool,
    response: Option<Arc<Mutex<Response>>>,
}
#[derive(Clone)]
enum Kind {
    Session(Options),
    Connection(Options, String, u16),
    Request(Request),
}
struct Handle {
    parent: usize,
    kind: Kind,
}
struct PrivateDir(PathBuf);
impl Drop for PrivateDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Response {
    _directory: PrivateDir,
    file: File,
    length: u64,
    cursor: u64,
    status: u32,
    reason: String,
    raw: String,
    headers: Vec<(String, String)>,
}
fn handles() -> &'static Mutex<HashMap<usize, Handle>> {
    static HANDLES: OnceLock<Mutex<HashMap<usize, Handle>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}
use super::lock;
fn fail<T: Default>(code: u32) -> T {
    crate::process::set_last_error(code);
    T::default()
}
fn insert(parent: usize, kind: Kind) -> usize {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let mut entries = lock(handles());
    if entries.len() >= 4096 {
        return fail(8);
    }
    if parent != 0 && !entries.contains_key(&parent) {
        return fail(6);
    }
    entries.insert(id, Handle { parent, kind });
    // Handle rendu : dernier code d'erreur à 0. L'outil de déploiement
    // d'Office lit GetLastError après un WinHttpConnect ou un
    // WinHttpOpenRequest réussi, et prenait pour une erreur le 122 laissé
    // par un GetTokenInformation de dimensionnement (#2651).
    crate::process::set_last_error(0);
    id
}
unsafe fn wide(pointer: *const u16, length: u32) -> Result<String, u32> {
    if pointer.is_null() {
        return Ok(String::new());
    }
    let count = if length == u32::MAX {
        let mut count = 0;
        while count < MAX_WIDE && *pointer.add(count) != 0 {
            count += 1;
        }
        if count == MAX_WIDE {
            return Err(87);
        }
        count
    } else {
        length as usize
    };
    if count > MAX_WIDE {
        return Err(87);
    }
    String::from_utf16(std::slice::from_raw_parts(pointer, count)).map_err(|_| 1113)
}
fn clean(value: &str) -> bool {
    !value.chars().any(char::is_control)
}
fn options_mut(kind: &mut Kind) -> &mut Options {
    match kind {
        Kind::Session(value) | Kind::Connection(value, _, _) => value,
        Kind::Request(value) => &mut value.options,
    }
}
fn response(handle: usize) -> Result<Arc<Mutex<Response>>, u32> {
    let entries = lock(handles());
    match entries.get(&handle).map(|value| &value.kind) {
        Some(Kind::Request(value)) => value.response.clone().ok_or(12019),
        Some(_) => Err(12018),
        None => Err(6),
    }
}
fn add_headers(target: &mut Vec<(String, String)>, text: &str, flags: u32) -> Result<(), u32> {
    if flags & !(0x1000_0000 | 0x2000_0000 | 0x8000_0000) != 0 {
        return Err(50);
    }
    let mut result = target.clone();
    for line in text
        .split('\n')
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.is_empty())
    {
        let (name, value) = line.split_once(':').ok_or(87u32)?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
            || !clean(value)
        {
            return Err(87);
        }
        let existing = result.iter().any(|(key, _)| key.eq_ignore_ascii_case(name));
        if existing && flags & 0x1000_0000 != 0 {
            return Err(183);
        }
        if flags & 0x8000_0000 != 0 {
            result.retain(|(key, _)| !key.eq_ignore_ascii_case(name));
        }
        if !value.trim().is_empty() || flags & 0x8000_0000 == 0 {
            result.push((name.into(), value.trim().into()));
        }
    }
    *target = result;
    Ok(())
}
pub unsafe extern "win64" fn open(
    agent: *const u16,
    access: u32,
    proxy: *const u16,
    bypass: *const u16,
    flags: u32,
) -> usize {
    if flags != 0 || !matches!(access, 0 | 1 | 3 | 4) {
        return fail(50);
    }
    let (agent, proxy, bypass) = match (
        wide(agent, u32::MAX),
        wide(proxy, u32::MAX),
        wide(bypass, u32::MAX),
    ) {
        (Ok(a), Ok(p), Ok(b)) if clean(&a) && clean(&p) && clean(&b) => (a, p, b),
        _ => return fail(87),
    };
    if access == 3 && proxy.is_empty() {
        return fail(87);
    }
    insert(
        0,
        Kind::Session(Options {
            agent,
            proxy: (access == 3).then_some(proxy),
            bypass,
            system_proxy: matches!(access, 0 | 4),
            connect_ms: 60000,
            total_ms: 120000,
            context: 0,
            tls: None,
            callback: 0,
            redirect_policy: 1,
            disabled: 0,
        }),
    )
}
pub unsafe extern "win64" fn connect(
    session: usize,
    host: *const u16,
    port: u16,
    reserved: u32,
) -> usize {
    if reserved != 0 || host.is_null() {
        return fail(87);
    }
    let host = match wide(host, u32::MAX) {
        Ok(value)
            if !value.is_empty()
                && clean(&value)
                && !value.contains(['/', '\\', '?', '#', '@', ' ']) =>
        {
            value
        }
        _ => return fail(87),
    };
    let options = match lock(handles()).get(&session).map(|value| &value.kind) {
        Some(Kind::Session(options)) => options.clone(),
        Some(_) => return fail(12018),
        None => return fail(6),
    };
    insert(session, Kind::Connection(options, host, port))
}
/// Pointeurs d'un tableau du programme terminé par NULL (types acceptés de
/// WinHttpOpenRequest et HttpOpenRequestA/W), lus à la taille d'un pointeur
/// du programme : 4 octets pour un programme x86 (#2597). `None` au-delà de
/// `max` entrées.
///
/// # Safety
/// `array` doit désigner un tableau lisible terminé par un pointeur nul.
pub(crate) unsafe fn program_pointers(array: usize, max: usize) -> Option<Vec<usize>> {
    let step = crate::wow32::pointer_size();
    let mut out = Vec::new();
    loop {
        let pointer = crate::wow32::read_ptr(array + out.len() * step);
        if pointer == 0 {
            return Some(out);
        }
        if out.len() == max {
            return None;
        }
        out.push(pointer);
    }
}

/// Types acceptés (UTF-16) d'un tableau du programme ; erreur 87 si une
/// chaîne est illisible ou s'il y en a plus de 63.
///
/// # Safety
/// `accept` est nul ou désigne un tableau lisible terminé par NULL.
pub(crate) unsafe fn accept_types_wide(accept: usize) -> Result<Option<Vec<String>>, u32> {
    if accept == 0 {
        return Ok(None);
    }
    let pointers = program_pointers(accept, 63).ok_or(87u32)?;
    let mut values = Vec::new();
    for pointer in pointers {
        values.push(wide(pointer as *const u16, u32::MAX).map_err(|_| 87u32)?);
    }
    Ok(Some(values))
}

pub unsafe extern "win64" fn open_request(
    connection: usize,
    verb: *const u16,
    path: *const u16,
    version: *const u16,
    referer: *const u16,
    accept: *const *const u16,
    flags: u32,
) -> usize {
    match accept_types_wide(accept as usize) {
        Ok(types) => open_request_types(connection, verb, path, version, referer, types, flags),
        Err(code) => fail(code),
    }
}

/// WinHttpOpenRequest, types acceptés déjà lus.
///
/// # Safety
/// Chaînes nulles ou lisibles et terminées par un zéro.
pub(crate) unsafe fn open_request_types(
    connection: usize,
    verb: *const u16,
    path: *const u16,
    version: *const u16,
    referer: *const u16,
    accept: Option<Vec<String>>,
    flags: u32,
) -> usize {
    if flags & !SECURE != 0 {
        return fail(50);
    }
    let (options, host, port) = match lock(handles()).get(&connection).map(|value| &value.kind) {
        Some(Kind::Connection(options, host, port)) => (options.clone(), host.clone(), *port),
        Some(_) => return fail(12018),
        None => return fail(6),
    };
    let (verb, path, version, referer) = match (
        wide(verb, u32::MAX),
        wide(path, u32::MAX),
        wide(version, u32::MAX),
        wide(referer, u32::MAX),
    ) {
        (Ok(v), Ok(p), Ok(version), Ok(r)) => (
            if v.is_empty() {
                "GET".into()
            } else {
                standard_verb(v)
            },
            if p.is_empty() { "/".into() } else { p },
            version,
            r,
        ),
        _ => return fail(87),
    };
    if !verb
        .bytes()
        .all(|byte| byte.is_ascii_alphabetic() || byte == b'-')
        || !path.starts_with('/')
        || !clean(&path)
        || !clean(&referer)
    {
        return fail(87);
    }
    if !version.is_empty() && version != "HTTP/1.1" {
        return fail(50);
    }
    let mut headers = Vec::new();
    if !referer.is_empty() {
        headers.push(("Referer".into(), referer));
    }
    if let Some(values) = accept {
        if !values.iter().all(|value| clean(value)) {
            return fail(87);
        }
        if !values.is_empty() {
            headers.push(("Accept".into(), values.join(", ")));
        }
    }
    insert(
        connection,
        Kind::Request(Request {
            options,
            host,
            port,
            secure: flags & SECURE != 0,
            verb,
            path,
            headers,
            body: Vec::new(),
            expected: 0,
            sent: false,
            response: None,
        }),
    )
}
/// #2730 : un verbe standard écrit en casse mixte (l'ODT d'Office passe
/// « Get ») part en majuscules. Learn (WinHttpOpenRequest, pwszVerb) : « This
/// string should be all uppercase. Many servers treat HTTP verbs as
/// case-sensitive » ; RFC 9110 §9.1 : méthodes standard en majuscules.
/// Inférence : l'ODT réussit sous Windows avec « Get », donc WinHTTP envoie
/// « GET ». Un verbe non standard reste tel quel.
fn standard_verb(verb: String) -> String {
    const STANDARD: [&str; 9] = [
        "GET", "HEAD", "POST", "PUT", "DELETE", "CONNECT", "OPTIONS", "TRACE", "PATCH",
    ];
    match STANDARD.iter().find(|s| s.eq_ignore_ascii_case(&verb)) {
        Some(s) => (*s).to_string(),
        None => verb,
    }
}
pub unsafe extern "win64" fn add_request_headers(
    handle: usize,
    text: *const u16,
    length: u32,
    flags: u32,
) -> i32 {
    let text = match wide(text, length) {
        Ok(value) => value,
        Err(error) => return fail(error),
    };
    let mut entries = lock(handles());
    match entries.get_mut(&handle).map(|entry| &mut entry.kind) {
        Some(Kind::Request(request)) if !request.sent => {
            match add_headers(&mut request.headers, &text, flags) {
                Ok(()) => 1,
                Err(error) => fail(error),
            }
        }
        Some(Kind::Request(_)) => fail(12019),
        Some(_) => fail(12018),
        None => fail(6),
    }
}
pub unsafe extern "win64" fn send_request(
    handle: usize,
    headers: *const u16,
    length: u32,
    optional: *const c_void,
    size: u32,
    total: u32,
    context: usize,
) -> i32 {
    if size as usize > MAX_UPLOAD
        || (size != 0 && optional.is_null())
        || (total != u32::MAX && size > total)
    {
        return fail(87);
    }
    let text = match wide(headers, length) {
        Ok(value) => value,
        Err(error) => return fail(error),
    };
    let mut entries = lock(handles());
    match entries.get_mut(&handle).map(|entry| &mut entry.kind) {
        Some(Kind::Request(request)) => {
            if request.sent && request.expected != total {
                return fail(87);
            }
            if let Err(error) = add_headers(&mut request.headers, &text, 0x2000_0000) {
                return fail(error);
            }
            request.body = if size == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(optional.cast::<u8>(), size as usize).to_vec()
            };
            request.expected = total;
            request.options.context = context;
            request.sent = true;
            request.response = None;
            1
        }
        Some(_) => fail(12018),
        None => fail(6),
    }
}
pub unsafe extern "win64" fn write_data(
    handle: usize,
    data: *const c_void,
    size: u32,
    written: *mut u32,
) -> i32 {
    if !written.is_null() {
        *written = 0;
    }
    if size != 0 && data.is_null() {
        return fail(87);
    }
    let mut entries = lock(handles());
    match entries.get_mut(&handle).map(|entry| &mut entry.kind) {
        Some(Kind::Request(request)) if request.sent && request.response.is_none() => {
            let total = request.body.len().saturating_add(size as usize);
            if total > MAX_UPLOAD
                || (request.expected != u32::MAX && total > request.expected as usize)
            {
                return fail(87);
            }
            if size != 0 {
                request.body.extend_from_slice(std::slice::from_raw_parts(
                    data.cast::<u8>(),
                    size as usize,
                ));
            }
            if !written.is_null() {
                *written = size;
            }
            1
        }
        Some(Kind::Request(_)) => fail(12019),
        Some(_) => fail(12018),
        None => fail(6),
    }
}
fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}
fn private_file(directory: &PrivateDir, name: &str, bytes: &[u8]) -> Result<PathBuf, u32> {
    let path = directory.0.join(name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|_| 8u32)?;
    file.write_all(bytes).map_err(|_| 8u32)?;
    Ok(path)
}
fn perform(request: &Request) -> Result<Response, u32> {
    for (name, value) in &request.headers {
        if name.eq_ignore_ascii_case("content-length")
            && value.parse::<usize>().ok() != Some(request.body.len())
        {
            return Err(87);
        }
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(50);
        }
    }
    let path = std::env::temp_dir().join(format!(
        "winlux-winhttp-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&path)
        .map_err(|_| 8u32)?;
    let directory = PrivateDir(path);
    let host = if request.host.contains(':') && !request.host.starts_with('[') {
        format!("[{}]", request.host)
    } else {
        request.host.clone()
    };
    let mut path = String::new();
    for byte in request.path.split('#').next().unwrap_or("/").bytes() {
        if byte <= b' ' || byte >= 127 || b"\"<>\\^`{|}".contains(&byte) {
            path.push_str(&format!("%{byte:02X}"));
        } else {
            path.push(byte as char);
        }
    }
    let scheme = if request.secure { "https" } else { "http" };
    let port = if request.port == 0 {
        if request.secure {
            443
        } else {
            80
        }
    } else {
        request.port
    };
    let url = format!("{scheme}://{host}:{port}{path}");
    let mut config = format!("silent\nshow-error\nproto = \"=http,https\"\nmax-filesize = 1073741824\nurl = {}\nrequest = {}\nuser-agent = {}\noutput = {}\ndump-header = {}\n",
        quote(&url), quote(&request.verb), quote(&request.options.agent), quote(directory.0.join("body").to_str().ok_or(87u32)?), quote(directory.0.join("headers").to_str().ok_or(87u32)?));
    // Redirections : WINHTTP_DISABLE_REDIRECTS ou REDIRECT_POLICY_NEVER
    // rendent le 30x à l'appelant ; DISALLOW_HTTPS_TO_HTTP (défaut) ne
    // quitte pas https ; ALWAYS suit tout, 10 au plus (#2626).
    let policy = request.options.redirect_policy;
    if request.options.disabled & 0x2 == 0 && policy != 0 {
        let allowed = if request.secure && policy == 1 {
            "=https"
        } else {
            "=http,https"
        };
        config.push_str(&format!(
            "location\nmax-redirs = 10\nproto-redir = {}\n",
            quote(allowed)
        ));
    }
    if request.options.connect_ms > 0 {
        config.push_str(&format!(
            "connect-timeout = {:.3}\n",
            request.options.connect_ms as f64 / 1000.0
        ));
    }
    if let Some((min, max)) = request.options.tls {
        config.push_str(&format!("tlsv1.{min}\ntls-max = \"1.{max}\"\n"));
    }
    if request.options.total_ms > 0 {
        config.push_str(&format!(
            "max-time = {:.3}\n",
            request.options.total_ms as f64 / 1000.0
        ));
    }
    let system = if request.options.system_proxy {
        crate::proxy::load()
            .and_then(|p| crate::proxy::for_request(&p, &request.host, request.secure))
    } else {
        None
    };
    if let Some(proxy) = system {
        config.push_str(&format!("proxy = {}\n", quote(&proxy)));
    } else if let Some(proxy) = &request.options.proxy {
        if proxy.contains([';', '=']) {
            return Err(50);
        }
        config.push_str(&format!("proxy = {}\n", quote(proxy)));
        if request.options.bypass.contains('<') {
            return Err(50);
        }
        config.push_str(&format!(
            "noproxy = {}\n",
            quote(&request.options.bypass.replace(';', ","))
        ));
    } else {
        config.push_str("noproxy = \"*\"\n");
    }
    for (name, value) in &request.headers {
        config.push_str(&format!(
            "header = {}\n",
            quote(&format!("{name}: {value}"))
        ));
    }
    if request.verb == "HEAD" {
        config.push_str("head\n");
    } else if !request.body.is_empty() || !matches!(request.verb.as_str(), "GET" | "OPTIONS") {
        let body = private_file(&directory, "upload", &request.body)?;
        config.push_str(&format!(
            "data-binary = {}\n",
            quote(&format!("@{}", body.display()))
        ));
    }
    let config = private_file(&directory, "request.conf", config.as_bytes())?;
    // No shell, no URL/header/credential in the process command line. curl's
    // user config is disabled; there is no switch to ignore TLS validation.
    if !crate::permissions::allowed(crate::permissions::Capability::Network) {
        return Err(12029); // ERROR_WINHTTP_CANNOT_CONNECT
    }
    let output = Command::new("/usr/bin/curl")
        .arg("--disable")
        .arg("--config")
        .arg(config)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| 12029u32)?;
    if !output.status.success() {
        return Err(match output.status.code() {
            Some(3) => 12005,
            Some(5 | 6) => 12007,
            Some(7) => 12029,
            Some(28) => 12002,
            Some(35 | 51 | 58 | 60 | 77 | 83 | 90 | 91) => 12175,
            Some(47) => 12156,
            Some(63) => 8,
            _ => 12030,
        });
    }
    let headers = directory.0.join("headers");
    if fs::metadata(&headers).map_err(|_| 12030u32)?.len() > 1024 * 1024 {
        return Err(12182);
    }
    let raw: String = fs::read(headers)
        .map_err(|_| 12030u32)?
        .into_iter()
        .map(char::from)
        .collect();
    let raw = raw
        .rsplit("\r\n\r\n")
        .find(|block| block.starts_with("HTTP/"))
        .ok_or(12152u32)?
        .to_string()
        + "\r\n\r\n";
    let mut lines = raw.split("\r\n");
    let status_line = lines.next().ok_or(12152u32)?;
    let mut parts = status_line.splitn(3, ' ');
    let _version = parts.next();
    let status = parts
        .next()
        .and_then(|status| status.parse::<u32>().ok())
        .filter(|status| (100..600).contains(status))
        .ok_or(12152u32)?;
    let reason = parts.next().unwrap_or("").to_string();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.into(), value.trim().into()))
        .collect();
    let file = File::open(directory.0.join("body")).map_err(|_| 12030u32)?;
    let length = file.metadata().map_err(|_| 12030u32)?.len();
    Ok(Response {
        _directory: directory,
        file,
        length,
        cursor: 0,
        status,
        reason,
        raw,
        headers,
    })
}
pub unsafe extern "win64" fn receive_response(handle: usize, reserved: *const c_void) -> i32 {
    if !reserved.is_null() {
        return fail(87);
    }
    let request = match lock(handles()).get(&handle).map(|entry| &entry.kind) {
        Some(Kind::Request(request))
            if request.sent
                && request.response.is_none()
                && (request.expected == u32::MAX
                    || request.body.len() == request.expected as usize) =>
        {
            request.clone()
        }
        Some(Kind::Request(_)) => return fail(12019),
        Some(_) => return fail(12018),
        None => return fail(6),
    };
    let response = match perform(&request) {
        Ok(value) => Arc::new(Mutex::new(value)),
        Err(error) => return fail(error),
    };
    match lock(handles())
        .get_mut(&handle)
        .map(|entry| &mut entry.kind)
    {
        Some(Kind::Request(request)) => {
            request.response = Some(response);
            1
        }
        _ => fail(995),
    }
}
pub unsafe extern "win64" fn query_data_available(handle: usize, available: *mut u32) -> i32 {
    if available.is_null() {
        return fail(87);
    }
    *available = 0;
    let response = match response(handle) {
        Ok(value) => value,
        Err(error) => return fail(error),
    };
    let response = lock(&response);
    *available = response
        .length
        .saturating_sub(response.cursor)
        .min(u32::MAX as u64) as u32;
    1
}
pub unsafe extern "win64" fn read_data(
    handle: usize,
    buffer: *mut c_void,
    size: u32,
    read: *mut u32,
) -> i32 {
    if read.is_null() || (size != 0 && buffer.is_null()) {
        return fail(87);
    }
    *read = 0;
    let response = match response(handle) {
        Ok(value) => value,
        Err(error) => return fail(error),
    };
    let mut response = lock(&response);
    if size == 0 {
        return 1;
    }
    let count = match response.file.read(std::slice::from_raw_parts_mut(
        buffer.cast::<u8>(),
        size as usize,
    )) {
        Ok(value) => value,
        Err(_) => return fail(12030),
    };
    response.cursor += count as u64;
    *read = count as u32;
    1
}
/// En-têtes bruts « ligne\0ligne\0 » : la ligne d'état puis chaque
/// en-tête. Le \0 final de la liste est ajouté par l'appelant avec celui
/// de toute chaîne rendue ; la longueur rendue en cas de succès l'exclut,
/// comme pour toute chaîne (inférence : Learn ne précise pas ce cas).
fn raw_headers_nul(raw: &str) -> String {
    raw.split("\r\n")
        .filter(|line| !line.is_empty())
        .map(|line| format!("{line}\0"))
        .collect()
}

unsafe fn output_bytes(
    buffer: *mut c_void,
    length: *mut u32,
    bytes: &[u8],
    success_length: usize,
) -> i32 {
    if length.is_null() {
        return fail(87);
    }
    if buffer.is_null() || (*length as usize) < bytes.len() {
        *length = bytes.len() as u32;
        return fail(122);
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast(), bytes.len());
    *length = success_length as u32;
    1
}
pub unsafe extern "win64" fn query_headers(
    handle: usize,
    level: u32,
    name: *const u16,
    buffer: *mut c_void,
    length: *mut u32,
    index: *mut u32,
) -> i32 {
    let response = match response(handle) {
        Ok(value) => value,
        Err(error) => return fail(error),
    };
    let response = lock(&response);
    let query = level & 0xffff;
    if level & !(0xffff | 0x2000_0000) != 0 {
        return fail(50);
    }
    let position = if index.is_null() { 0 } else { *index as usize };
    let value = match query {
        19 => response.status.to_string(),
        20 => response.reason.clone(),
        // WINHTTP_QUERY_RAW_HEADERS (Learn, « Query Info Flags ») : chaque
        // en-tête terminé par "\0", puis un "\0" final (#2716).
        21 => raw_headers_nul(&response.raw),
        22 => response.raw.clone(),
        1 | 5 | 65535 => {
            let custom = if query == 1 {
                "Content-Type".into()
            } else if query == 5 {
                "Content-Length".into()
            } else {
                match wide(name, u32::MAX) {
                    Ok(value) => value,
                    Err(error) => return fail(error),
                }
            };
            match response
                .headers
                .iter()
                .filter(|(key, _)| key.eq_ignore_ascii_case(&custom))
                .nth(position)
            {
                Some((_, value)) => value.clone(),
                None => return fail(12150),
            }
        }
        _ => return fail(50),
    };
    let result = if level & 0x2000_0000 != 0 {
        let value = match value.parse::<u32>() {
            Ok(value) => value,
            Err(_) => return fail(87),
        };
        output_bytes(buffer, length, &value.to_le_bytes(), 4)
    } else {
        let mut data: Vec<u8> = value.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let size = data.len();
        data.extend_from_slice(&[0, 0]);
        output_bytes(buffer, length, &data, size)
    };
    if result != 0 && !index.is_null() {
        *index = index.read().saturating_add(1);
    }
    result
}
/// Schémas d'authentification annoncés par `WWW-Authenticate` (401,
/// cible serveur) ou `Proxy-Authenticate` (407, cible mandataire) :
/// masque WINHTTP_AUTH_SCHEME_*, premier schéma proposé, et cible.
fn auth_schemes(status: u32, headers: &[(String, String)]) -> Option<(u32, u32, u32)> {
    let (header, target) = match status {
        407 => ("Proxy-Authenticate", 1),
        _ => ("WWW-Authenticate", 0),
    };
    let mut all = 0u32;
    let mut first = 0u32;
    for (_, value) in headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case(header))
    {
        let word = value
            .split([' ', ','])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let bit = match word.as_str() {
            "basic" => 1,
            "ntlm" => 2,
            "passport1.4" => 4,
            "digest" => 8,
            "negotiate" => 0x10,
            _ => 0,
        };
        if bit != 0 && first == 0 {
            first = bit;
        }
        all |= bit;
    }
    (all != 0).then_some((all, first, target))
}

/// WinHttpQueryAuthSchemes : après une réponse 401 ou 407.
pub unsafe extern "win64" fn query_auth_schemes(
    handle: usize,
    supported: *mut u32,
    first: *mut u32,
    target: *mut u32,
) -> i32 {
    if supported.is_null() || first.is_null() || target.is_null() {
        return fail(87);
    }
    let response = match response(handle) {
        Ok(value) => value,
        Err(error) => return fail(error),
    };
    let response = lock(&response);
    match auth_schemes(response.status, &response.headers) {
        Some((all, one, to)) => {
            *supported = all;
            *first = one;
            *target = to;
            1
        }
        // ERROR_INVALID_OPERATION : aucun en-tête d'authentification.
        None => fail(4317),
    }
}
pub unsafe extern "win64" fn close(handle: usize) -> i32 {
    let mut entries = lock(handles());
    if entries.remove(&handle).is_none() {
        return fail(6);
    }
    let mut pending = vec![handle];
    while let Some(parent) = pending.pop() {
        let children: Vec<usize> = entries
            .iter()
            .filter(|(_, entry)| entry.parent == parent)
            .map(|(id, _)| *id)
            .collect();
        for child in children {
            entries.remove(&child);
            pending.push(child);
        }
    }
    1
}
pub unsafe extern "win64" fn set_timeouts(
    handle: usize,
    resolve: i32,
    connect: i32,
    send: i32,
    receive: i32,
) -> i32 {
    if [resolve, connect, send, receive]
        .iter()
        .any(|value| *value < -1)
    {
        return fail(87);
    }
    let mut entries = lock(handles());
    let Some(entry) = entries.get_mut(&handle) else {
        return fail(6);
    };
    let options = options_mut(&mut entry.kind);
    options.connect_ms = connect;
    options.total_ms = if send <= 0 || receive <= 0 {
        0
    } else {
        connect.max(0).saturating_add(send).saturating_add(receive)
    };
    1
}
pub unsafe extern "win64" fn set_option(
    handle: usize,
    option: u32,
    value: *const c_void,
    size: u32,
) -> i32 {
    let mut entries = lock(handles());
    let Some(entry) = entries.get_mut(&handle) else {
        return fail(6);
    };
    if value.is_null() {
        return fail(87);
    }
    // WINHTTP_OPTION_CONTEXT_VALUE : DWORD_PTR, 4 octets en x86.
    if option == 45 && size as usize == crate::wow32::pointer_size() {
        options_mut(&mut entry.kind).context = crate::wow32::read_ptr(value as usize);
        return 1;
    }
    // WINHTTP_OPTION_CONFIGURE_PASSPORT_AUTH (83) sur une session : DWORD de
    // drapeaux WINHTTP_{DIS,EN}ABLE_PASSPORT_AUTH (0, 0x10000000) et
    // _KEYRING (0x20000000, 0x40000000), d'après winhttp.h de mingw-w64 et la
    // page « Option Flags » de Microsoft Learn. Winlux ne fait pas
    // d'authentification Passport : l'option est acceptée sans effet (#2606).
    if option == 83 {
        if !matches!(entry.kind, Kind::Session(_)) {
            return fail(12018);
        }
        if size != 4 || std::ptr::read_unaligned(value.cast::<u32>()) & !0x7000_0000 != 0 {
            return fail(87);
        }
        return 1;
    }
    // WINHTTP_OPTION_SECURE_PROTOCOLS (84) sur une session : DWORD de
    // drapeaux WINHTTP_FLAG_SECURE_PROTOCOL_* (SSL2 0x8, SSL3 0x20, TLS1
    // 0x80, TLS1_1 0x200, TLS1_2 0x800, TLS1_3 0x2000), d'après winhttp.h de
    // mingw-w64 et la page « Option Flags » de Microsoft Learn. Les
    // connexions de la session sont tenues entre la plus basse et la plus
    // haute version TLS demandées ; SSL2/SSL3 ne sont jamais parlés, une
    // valeur qui ne demande qu'eux est refusée (#2609).
    if option == 84 {
        if !matches!(entry.kind, Kind::Session(_)) {
            return fail(12018);
        }
        if size != 4 {
            return fail(87);
        }
        let flags = std::ptr::read_unaligned(value.cast::<u32>());
        match secure_protocols(flags) {
            Some(range) => {
                options_mut(&mut entry.kind).tls = Some(range);
                return 1;
            }
            None => return fail(87),
        }
    }
    // WINHTTP_OPTION_DISABLE_FEATURE (63), sur une requête seulement :
    // DWORD de drapeaux WINHTTP_DISABLE_COOKIES (1), _REDIRECTS (2),
    // _AUTHENTICATION (4), _KEEP_ALIVE (8), d'après winhttp.h de mingw-w64
    // et la page « Option Flags » de Microsoft Learn. Seules les redirections
    // changent quelque chose ici : Winlux ne garde ni cookie, ni connexion,
    // ni authentification automatique d'une requête à l'autre (#2626).
    if option == 63 {
        if !matches!(entry.kind, Kind::Request(_)) {
            return fail(12018);
        }
        if size != 4 {
            return fail(87);
        }
        let flags = std::ptr::read_unaligned(value.cast::<u32>());
        if flags & !0xF != 0 {
            return fail(87);
        }
        options_mut(&mut entry.kind).disabled |= flags;
        return 1;
    }
    // WINHTTP_OPTION_AUTOLOGON_POLICY (77) : DWORD
    // WINHTTP_AUTOLOGON_SECURITY_LEVEL_MEDIUM (0), _LOW (1), _HIGH (2).
    // Winlux n'envoie jamais d'identifiants par défaut : accepté sans effet
    // (#2626).
    if option == 77 {
        if size != 4 || std::ptr::read_unaligned(value.cast::<u32>()) > 2 {
            return fail(87);
        }
        return 1;
    }
    // WINHTTP_OPTION_REDIRECT_POLICY (88), sur une session ou une requête :
    // _NEVER (0), _DISALLOW_HTTPS_TO_HTTP (1, défaut), _ALWAYS (2) (#2626).
    if option == 88 {
        if matches!(entry.kind, Kind::Connection(..)) {
            return fail(12018);
        }
        let policy = if size == 4 {
            std::ptr::read_unaligned(value.cast::<u32>())
        } else {
            u32::MAX
        };
        if policy > 2 {
            return fail(87);
        }
        options_mut(&mut entry.kind).redirect_policy = policy as u8;
        return 1;
    }
    // In particular, do not silently honor certificate-validation bypass flags.
    fail(50)
}

/// Versions TLS 1.x (minimum, maximum) d'une valeur de
/// WINHTTP_OPTION_SECURE_PROTOCOLS ; `None` pour un drapeau inconnu ou
/// une valeur sans aucune version TLS.
fn secure_protocols(flags: u32) -> Option<(u8, u8)> {
    const SSL: u32 = 0x8 | 0x20;
    const TLS: [u32; 4] = [0x80, 0x200, 0x800, 0x2000];
    if flags & !(SSL | TLS.iter().fold(0, |a, b| a | b)) != 0 {
        return None;
    }
    let present: Vec<u8> = (0..4u8).filter(|i| flags & TLS[*i as usize] != 0).collect();
    Some((*present.first()?, *present.last()?))
}
pub unsafe extern "win64" fn query_option(
    handle: usize,
    option: u32,
    buffer: *mut c_void,
    size: *mut u32,
) -> i32 {
    let mut entries = lock(handles());
    let Some(entry) = entries.get_mut(&handle) else {
        return fail(6);
    };
    if option == 45 {
        let bytes = options_mut(&mut entry.kind).context.to_le_bytes();
        let n = crate::wow32::pointer_size();
        return output_bytes(buffer, size, &bytes[..n], n);
    }
    if option == 9 {
        let value: u32 = match entry.kind {
            Kind::Session(_) => 1,
            Kind::Connection(_, _, _) => 2,
            Kind::Request(_) => 3,
        };
        return output_bytes(buffer, size, &value.to_le_bytes(), 4);
    }
    fail(50)
}
#[repr(C)]
pub struct ProxyInfo {
    access: u32,
    proxy: *mut u16,
    bypass: *mut u16,
}
#[repr(C)]
pub struct IeProxyConfig {
    autodetect: i32,
    url: *mut u16,
    proxy: *mut u16,
    bypass: *mut u16,
}
/// Chaîne large allouée pour l'appelant (qui la libère avec GlobalFree ;
/// la libération échoue sans dommage, la chaîne est minuscule).
fn alloc_wide(s: &str) -> *mut u16 {
    if s.is_empty() {
        return std::ptr::null_mut();
    }
    let w: Vec<u16> = s.encode_utf16().chain([0]).collect();
    unsafe {
        let p = libc::malloc(w.len() * 2) as *mut u16;
        if !p.is_null() {
            std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
        }
        p
    }
}
/// Proxy système (commun à Linux) au format WinHTTP.
fn system_proxy_strings() -> Option<(String, String)> {
    let p = crate::proxy::load()?;
    Some((crate::proxy::windows_server(&p), p.bypass.clone()))
}
pub unsafe extern "win64" fn default_proxy(out: *mut ProxyInfo) -> i32 {
    if out.is_null() {
        return fail(87);
    }
    let info = match system_proxy_strings() {
        Some((server, bypass)) => ProxyInfo {
            access: 3,
            proxy: alloc_wide(&server),
            bypass: alloc_wide(&bypass),
        },
        None => ProxyInfo {
            access: 1,
            proxy: std::ptr::null_mut(),
            bypass: std::ptr::null_mut(),
        },
    };
    if crate::wow32::pointer_size() == 4 {
        // WINHTTP_PROXY_INFO x86 : 12 octets (#779).
        let o = out as *mut u32;
        o.write_unaligned(info.access);
        o.add(1).write_unaligned(info.proxy as usize as u32);
        o.add(2).write_unaligned(info.bypass as usize as u32);
    } else {
        out.write(info);
    }
    1
}
pub unsafe extern "win64" fn ie_proxy(out: *mut IeProxyConfig) -> i32 {
    if out.is_null() {
        return fail(87);
    }
    let (server, bypass) = system_proxy_strings().unwrap_or_default();
    let cfg = IeProxyConfig {
        autodetect: 0,
        url: std::ptr::null_mut(),
        proxy: alloc_wide(&server),
        bypass: alloc_wide(&bypass),
    };
    if crate::wow32::pointer_size() == 4 {
        // WINHTTP_CURRENT_USER_IE_PROXY_CONFIG x86 : 16 octets (#779).
        let o = out as *mut u32;
        o.write_unaligned(cfg.autodetect as u32);
        o.add(1).write_unaligned(cfg.url as usize as u32);
        o.add(2).write_unaligned(cfg.proxy as usize as u32);
        o.add(3).write_unaligned(cfg.bypass as usize as u32);
    } else {
        out.write(cfg);
    }
    1
}
pub unsafe extern "win64" fn proxy_for_url(
    handle: usize,
    _url: *const u16,
    options: *const c_void,
    out: *mut ProxyInfo,
) -> i32 {
    if !matches!(
        lock(handles()).get(&handle).map(|entry| &entry.kind),
        Some(Kind::Session(_))
    ) {
        return fail(6);
    }
    if options.is_null() || out.is_null() {
        return fail(87);
    }
    // No PAC engine or WPAD discovery, hence no fabricated successful answer.
    fail(12180)
}
/// WinHttpDetectAutoProxyConfigUrl : sans découverte WPAD (DHCP ni DNS),
/// réponse documentée ERROR_WINHTTP_AUTODETECTION_FAILED et URL nulle.
/// Steam x86 la cherche par GetProcAddress au démarrage (#1439).
pub unsafe extern "win64" fn detect_auto_proxy_config_url(flags: u32, out: *mut *mut u16) -> i32 {
    if out.is_null() || flags & 3 == 0 {
        return fail(87);
    }
    crate::wow32::write_ptr(out as usize, 0);
    fail(12180)
}
/// WinHttpSetStatusCallback (#2619) : d'après Microsoft Learn, rend le
/// rappel défini auparavant sur la poignée (NULL s'il n'y en avait pas), et
/// WINHTTP_INVALID_STATUS_CALLBACK seulement en cas d'échec. Le rappel est
/// retenu et hérité par les poignées filles, mais jamais appelé : les
/// sessions de Winlux sont synchrones (WINHTTP_FLAG_ASYNC est refusé par
/// WinHttpOpen), et les notifications d'achèvement comme REQUEST_ERROR ne
/// concernent que le mode asynchrone ; l'erreur est rendue par l'appel.
pub unsafe extern "win64" fn status_callback(
    handle: usize,
    callback: usize,
    _flags: u32,
    _reserved: usize,
) -> usize {
    let mut entries = lock(handles());
    let Some(entry) = entries.get_mut(&handle) else {
        crate::process::set_last_error(6);
        return invalid_status_callback();
    };
    // Valeur du programme : 4 octets en x86 (WINHTTP_STATUS_CALLBACK).
    let callback = if crate::wow32::pointer_size() == 4 {
        callback & 0xffff_ffff
    } else {
        callback
    };
    std::mem::replace(&mut options_mut(&mut entry.kind).callback, callback)
}

/// WINHTTP_INVALID_STATUS_CALLBACK, ((WINHTTP_STATUS_CALLBACK)(-1L)), à la
/// taille d'un pointeur du programme.
fn invalid_status_callback() -> usize {
    if crate::wow32::pointer_size() == 4 {
        0xffff_ffff
    } else {
        usize::MAX
    }
}
pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WinHttpOpen" => fn open, "WinHttpConnect" => fn connect,
        "WinHttpOpenRequest" => fn open_request, "WinHttpAddRequestHeaders" => fn add_request_headers,
        "WinHttpSendRequest" => fn send_request, "WinHttpWriteData" => fn write_data,
        "WinHttpReceiveResponse" => fn receive_response, "WinHttpQueryHeaders" => fn query_headers,
        "WinHttpQueryAuthSchemes" => fn query_auth_schemes,
        "WinHttpQueryDataAvailable" => fn query_data_available, "WinHttpReadData" => fn read_data,
        "WinHttpCloseHandle" => fn close, "WinHttpSetTimeouts" => fn set_timeouts,
        "WinHttpSetOption" => fn set_option, "WinHttpQueryOption" => fn query_option,
        "WinHttpGetDefaultProxyConfiguration" => fn default_proxy,
        "WinHttpGetIEProxyConfigForCurrentUser" => fn ie_proxy,
        "WinHttpGetProxyForUrl" => fn proxy_for_url, "WinHttpSetStatusCallback" => fn status_callback,
        "WinHttpDetectAutoProxyConfigUrl" => fn detect_auto_proxy_config_url,
    }
}

#[cfg(test)]
mod tests {
    /// #2730 : « Get » part en « GET » (la passerelle d'Office refuse « Get »
    /// par un 400) ; un verbe non standard garde sa casse.
    #[test]
    fn verbe_standard_en_majuscules_2730() {
        let _teb = crate::process::test_teb();
        unsafe {
            for (given, sent) in [
                ("Get", "GET"),
                ("post", "POST"),
                ("Head", "HEAD"),
                ("PROPFIND", "PROPFIND"),
                ("Custom-Verb", "Custom-Verb"),
            ] {
                let (session, connection, request) = request_for(80, given, "/", false);
                let verb = match lock(handles()).get(&request).map(|entry| &entry.kind) {
                    Some(Kind::Request(r)) => r.verb.clone(),
                    _ => String::new(),
                };
                assert_eq!(verb, sent, "verbe {given}");
                close(request);
                close(connection);
                close(session);
            }
        }
    }

    /// #2716 : WINHTTP_QUERY_RAW_HEADERS, chaque ligne terminée par \0.
    #[test]
    fn en_tetes_bruts_separes_par_nul_2716() {
        let raw = "HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nX-A: 1\r\n\r\n";
        assert_eq!(
            raw_headers_nul(raw),
            "HTTP/1.1 200 OK\0Content-Type: text/xml\0X-A: 1\0"
        );
    }

    /// #1439 : pas de WPAD, échec documenté et URL nulle.
    #[test]
    fn detection_auto_du_proxy_echoue_proprement() {
        let _teb = crate::process::test_teb();
        let mut url = 1usize as *mut u16;
        assert_eq!(
            unsafe { super::detect_auto_proxy_config_url(3, &mut url) },
            0
        );
        assert!(url.is_null());
        assert_eq!(crate::process::last_error(), 12180);
        assert_eq!(
            unsafe { super::detect_auto_proxy_config_url(0, &mut url) },
            0
        );
        assert_eq!(crate::process::last_error(), 87);
    }

    /// #687 : schémas proposés par WWW-Authenticate et Proxy-Authenticate.
    #[test]
    fn schemas_d_authentification() {
        let h = |k: &str, v: &str| (k.to_string(), v.to_string());
        let server = [
            h("WWW-Authenticate", "Negotiate"),
            h("www-authenticate", "NTLM"),
            h("WWW-Authenticate", "Basic realm=\"x\""),
        ];
        assert_eq!(auth_schemes(401, &server), Some((0x13, 0x10, 0)));
        let proxy = [h("Proxy-Authenticate", "Digest realm=\"p\", nonce=\"n\"")];
        assert_eq!(auth_schemes(407, &proxy), Some((8, 8, 1)));
        assert_eq!(auth_schemes(200, &[h("Content-Type", "text/html")]), None);
    }

    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::{TcpListener, TcpStream};
    use std::ptr::{null, null_mut};
    use std::time::{Duration, Instant};

    fn w(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    unsafe fn request_for(
        port: u16,
        verb: &str,
        path: &str,
        secure: bool,
    ) -> (usize, usize, usize) {
        let session = open(w("Winlux test").as_ptr(), 1, null(), null(), 0);
        assert_ne!(session, 0);
        let connection = connect(session, w("127.0.0.1").as_ptr(), port, 0);
        assert_ne!(connection, 0);
        let request = open_request(
            connection,
            w(verb).as_ptr(),
            w(path).as_ptr(),
            null(),
            null(),
            null(),
            if secure { SECURE } else { 0 },
        );
        assert_ne!(request, 0);
        (session, connection, request)
    }

    /// #2606 : WINHTTP_OPTION_CONFIGURE_PASSPORT_AUTH (83) acceptée sur une
    /// session (l'outil de déploiement d'Office n'ouvrait plus de connexion),
    /// refusée ailleurs ou avec une valeur inconnue.
    #[test]
    fn option_passport_acceptee_sur_une_session() {
        let _teb = crate::process::test_teb();
        unsafe {
            let session = open(w("Winlux test").as_ptr(), 1, null(), null(), 0);
            assert_ne!(session, 0);
            for value in [0u32, 0x1000_0000, 0x2000_0000, 0x4000_0000] {
                assert_eq!(set_option(session, 83, (&value as *const u32).cast(), 4), 1);
            }
            let inconnue = 1u32;
            assert_eq!(
                set_option(session, 83, (&inconnue as *const u32).cast(), 4),
                0
            );
            assert_eq!(crate::process::last_error(), 87);
            let connection = connect(session, w("127.0.0.1").as_ptr(), 80, 0);
            assert_ne!(connection, 0);
            let zero = 0u32;
            assert_eq!(
                set_option(connection, 83, (&zero as *const u32).cast(), 4),
                0
            );
            assert_eq!(crate::process::last_error(), 12018);
            assert_eq!(close(session), 1);
        }
    }

    /// #2609 : WINHTTP_OPTION_SECURE_PROTOCOLS (84) acceptée sur une session
    /// et retenue comme bornes TLS ; SSL seul, drapeau inconnu ou autre
    /// poignée refusés.
    #[test]
    fn option_protocoles_securises_sur_une_session() {
        let _teb = crate::process::test_teb();
        assert_eq!(secure_protocols(0x800 | 0x2000), Some((2, 3)));
        assert_eq!(secure_protocols(0x80 | 0x200 | 0x800), Some((0, 2)));
        assert_eq!(secure_protocols(0x8 | 0x20 | 0x800), Some((2, 2)));
        assert_eq!(secure_protocols(0x8 | 0x20), None);
        assert_eq!(secure_protocols(0x800 | 0x1), None);
        unsafe {
            let session = open(w("Winlux test").as_ptr(), 1, null(), null(), 0);
            assert_ne!(session, 0);
            let v = 0x800u32 | 0x2000;
            assert_eq!(set_option(session, 84, (&v as *const u32).cast(), 4), 1);
            match &lock(handles()).get(&session).unwrap().kind {
                Kind::Session(o) => assert_eq!(o.tls, Some((2, 3))),
                _ => unreachable!(),
            }
            let ssl = 0x20u32;
            assert_eq!(set_option(session, 84, (&ssl as *const u32).cast(), 4), 0);
            assert_eq!(crate::process::last_error(), 87);
            let connection = connect(session, w("127.0.0.1").as_ptr(), 80, 0);
            assert_ne!(connection, 0);
            assert_eq!(set_option(connection, 84, (&v as *const u32).cast(), 4), 0);
            assert_eq!(crate::process::last_error(), 12018);
            assert_eq!(close(session), 1);
        }
    }

    /// #2619 : WinHttpSetStatusCallback rend le rappel précédent (NULL au
    /// départ), le rappel est hérité par une requête ouverte ensuite, et
    /// WINHTTP_INVALID_STATUS_CALLBACK seulement sur une poignée inconnue.
    /// #2651 : un handle rendu remet le dernier code d'erreur à 0.
    #[test]
    fn handle_rendu_efface_la_derniere_erreur() {
        let _teb = crate::process::test_teb();
        unsafe {
            crate::process::set_last_error(122);
            let session = open(w("Winlux test").as_ptr(), 1, null(), null(), 0);
            assert_ne!(session, 0);
            assert_eq!(crate::process::last_error(), 0);
            crate::process::set_last_error(122);
            let connection = connect(session, w("127.0.0.1").as_ptr(), 80, 0);
            assert_ne!(connection, 0);
            assert_eq!(crate::process::last_error(), 0);
            crate::process::set_last_error(122);
            let request = open_request(
                connection,
                w("GET").as_ptr(),
                w("/").as_ptr(),
                null(),
                null(),
                null(),
                0,
            );
            assert_ne!(request, 0);
            assert_eq!(crate::process::last_error(), 0);
            // Un échec garde son propre code.
            assert_eq!(connect(0xdead_0000, w("127.0.0.1").as_ptr(), 80, 0), 0);
            assert_eq!(crate::process::last_error(), 6);
            assert_eq!(close(session), 1);
        }
    }

    #[test]
    fn rappel_d_etat_retenu_et_rendu() {
        let _teb = crate::process::test_teb();
        unsafe {
            let session = open(w("Winlux test").as_ptr(), 1, null(), null(), 0);
            assert_ne!(session, 0);
            assert_eq!(status_callback(session, 0x1234, 0x10000, 0), 0);
            let connection = connect(session, w("127.0.0.1").as_ptr(), 80, 0);
            assert_ne!(connection, 0);
            let request = open_request(
                connection,
                w("GET").as_ptr(),
                w("/").as_ptr(),
                null(),
                null(),
                null(),
                0,
            );
            assert_ne!(request, 0);
            assert_eq!(status_callback(request, 0x5678, 0x10000, 0), 0x1234);
            assert_eq!(status_callback(request, 0, 0, 0), 0x5678);
            assert_eq!(status_callback(session, 0, 0, 0), 0x1234);
            assert_eq!(status_callback(0xdead_0000, 1, 0, 0), usize::MAX);
            assert_eq!(crate::process::last_error(), 6);
            assert_eq!(close(session), 1);
        }
    }

    /// #2626 : DISABLE_FEATURE (63), AUTOLOGON_POLICY (77) et
    /// REDIRECT_POLICY (88) acceptées avec une valeur valide, refusées sinon.
    #[test]
    fn options_redirection_et_ouverture_de_session_2626() {
        let _teb = crate::process::test_teb();
        unsafe {
            let (session, connection, request) = request_for(80, "GET", "/", false);
            let set = |h: usize, option: u32, value: u32| {
                set_option(h, option, (&value as *const u32).cast(), 4)
            };
            for flags in [1u32, 2, 4, 8, 0xF] {
                assert_eq!(set(request, 63, flags), 1);
            }
            assert_eq!(set(request, 63, 0x10), 0);
            assert_eq!(crate::process::last_error(), 87);
            assert_eq!(set(session, 63, 2), 0);
            assert_eq!(crate::process::last_error(), 12018);
            for level in 0..=2u32 {
                assert_eq!(set(request, 77, level), 1);
                assert_eq!(set(session, 77, level), 1);
            }
            assert_eq!(set(request, 77, 3), 0);
            assert_eq!(crate::process::last_error(), 87);
            for policy in 0..=2u32 {
                assert_eq!(set(request, 88, policy), 1);
                assert_eq!(set(session, 88, policy), 1);
            }
            assert_eq!(set(session, 88, 3), 0);
            assert_eq!(crate::process::last_error(), 87);
            assert_eq!(set(connection, 88, 0), 0);
            assert_eq!(crate::process::last_error(), 12018);
            assert_eq!(close(session), 1);
        }
    }

    /// #2626 : une redirection est suivie par défaut ; WINHTTP_DISABLE_REDIRECTS
    /// sur la requête ou REDIRECT_POLICY_NEVER sur la session rendent le 302.
    #[test]
    fn redirections_suivies_ou_rendues_2626() {
        let _teb = crate::process::test_teb();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            let mut served = 0;
            let deadline = Instant::now() + Duration::from_secs(20);
            while served < 4 && Instant::now() < deadline {
                let (stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("HTTP fixture: {error}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(stream);
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                }
                if first.starts_with("GET /depart ") {
                    write!(reader.get_mut(), "HTTP/1.1 302 Found\r\nLocation: /cible\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                } else {
                    write!(
                        reader.get_mut(),
                        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                    )
                    .unwrap();
                }
                served += 1;
            }
        });
        let status_of = |request: usize| unsafe {
            assert_eq!(send_request(request, null(), 0, null(), 0, 0, 0), 1);
            assert_eq!(receive_response(request, null()), 1);
            let mut status = 0u32;
            let mut size = 4;
            assert_eq!(
                query_headers(
                    request,
                    19 | 0x2000_0000,
                    null(),
                    (&mut status as *mut u32).cast(),
                    &mut size,
                    null_mut()
                ),
                1
            );
            status
        };
        unsafe {
            let (session, _, request) = request_for(port, "GET", "/depart", false);
            assert_eq!(status_of(request), 200);
            assert_eq!(close(session), 1);

            let (session, _, request) = request_for(port, "GET", "/depart", false);
            let flag = 2u32;
            assert_eq!(set_option(request, 63, (&flag as *const u32).cast(), 4), 1);
            assert_eq!(status_of(request), 302);
            assert_eq!(close(session), 1);

            let session = open(w("Winlux test").as_ptr(), 1, null(), null(), 0);
            let never = 0u32;
            assert_eq!(set_option(session, 88, (&never as *const u32).cast(), 4), 1);
            let connection = connect(session, w("127.0.0.1").as_ptr(), port, 0);
            let request = open_request(
                connection,
                w("GET").as_ptr(),
                w("/depart").as_ptr(),
                null(),
                null(),
                null(),
                0,
            );
            assert_ne!(request, 0);
            assert_eq!(status_of(request), 302);
            assert_eq!(close(session), 1);
        }
        server.join().unwrap();
    }

    #[test]
    fn winhttp_real_http_upload_headers_read_and_close() {
        let _teb = crate::process::test_teb();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            let mut requests = Vec::new();
            let deadline = Instant::now() + Duration::from_secs(8);
            while requests.len() < 2 && Instant::now() < deadline {
                let (stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("HTTP fixture: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(stream);
                let mut headers = String::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    assert!(!line.is_empty(), "request ended before its headers");
                    headers.push_str(&line);
                }
                let size = headers
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                    .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                    .unwrap_or(0);
                let mut body = vec![0; size];
                reader.read_exact(&mut body).unwrap();
                let response: &[u8] = if requests.is_empty() {
                    b"hello"
                } else {
                    b"posted"
                };
                write!(reader.get_mut(), "HTTP/1.1 201 Created\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nX-Test: one\r\nX-Test: two\r\nConnection: close\r\n\r\n", response.len()).unwrap();
                reader.get_mut().write_all(response).unwrap();
                requests.push((headers, body));
            }
            requests
        });
        unsafe {
            let (session, _, request) = request_for(port, "GET", "/Bonjour Monde?q=é", false);
            let header = w("X-Quote: a \" b\\c\r\nAuthorization: Bearer test-only-token\r\n");
            assert_eq!(
                add_request_headers(request, header.as_ptr(), u32::MAX, 0x2000_0000),
                1
            );
            let mut available = 123;
            assert_eq!(query_data_available(request, &mut available), 0);
            assert_eq!(crate::process::last_error(), 12019);
            assert_eq!(send_request(request, null(), 0, null(), 0, 0, 42), 1);
            assert_eq!(receive_response(request, null()), 1);
            assert_eq!(receive_response(request, null()), 0); // never replay a POST/GET
            assert_eq!(crate::process::last_error(), 12019);
            let mut status = 0u32;
            let mut size = 4;
            assert_eq!(
                query_headers(
                    request,
                    19 | 0x2000_0000,
                    null(),
                    (&mut status as *mut u32).cast(),
                    &mut size,
                    null_mut()
                ),
                1
            );
            assert_eq!(status, 201);
            let mut required = 0;
            assert_eq!(
                query_headers(request, 22, null(), null_mut(), &mut required, null_mut()),
                0
            );
            assert_eq!(crate::process::last_error(), 122);
            let mut raw = vec![0u16; required as usize / 2];
            assert_eq!(
                query_headers(
                    request,
                    22,
                    null(),
                    raw.as_mut_ptr().cast(),
                    &mut required,
                    null_mut()
                ),
                1
            );
            assert!(String::from_utf16(&raw[..required as usize / 2])
                .unwrap()
                .contains("201 Created"));
            // WINHTTP_QUERY_RAW_HEADERS (21) : demande de taille puis lecture (#2716).
            let mut required = 0;
            assert_eq!(
                query_headers(request, 21, null(), null_mut(), &mut required, null_mut()),
                0
            );
            assert_eq!(crate::process::last_error(), 122);
            let mut nul = vec![0u16; required as usize / 2];
            assert_eq!(
                query_headers(
                    request,
                    21,
                    null(),
                    nul.as_mut_ptr().cast(),
                    &mut required,
                    null_mut()
                ),
                1
            );
            let nul = String::from_utf16(&nul).unwrap();
            assert!(nul.starts_with("HTTP/1.1 201 Created\0"));
            assert!(nul.contains("\0X-Test: two\0"));
            assert!(nul.ends_with("\0\0"));
            assert!(!nul.contains('\r'));
            let mut index = 0;
            for expected in ["one", "two"] {
                let mut data = [0u16; 16];
                let mut size = 32;
                assert_eq!(
                    query_headers(
                        request,
                        65535,
                        w("X-Test").as_ptr(),
                        data.as_mut_ptr().cast(),
                        &mut size,
                        &mut index
                    ),
                    1
                );
                assert_eq!(
                    String::from_utf16(&data[..size as usize / 2]).unwrap(),
                    expected
                );
            }
            assert_eq!(index, 2);
            assert_eq!(query_data_available(request, &mut available), 1);
            assert_eq!(available, 5);
            let mut bytes = [0u8; 8];
            let mut count = 0;
            assert_eq!(
                read_data(request, bytes.as_mut_ptr().cast(), 2, &mut count),
                1
            );
            assert_eq!(&bytes[..count as usize], b"he");
            assert_eq!(
                read_data(request, bytes.as_mut_ptr().cast(), 8, &mut count),
                1
            );
            assert_eq!(&bytes[..count as usize], b"llo");
            assert_eq!(
                read_data(request, bytes.as_mut_ptr().cast(), 8, &mut count),
                1
            );
            assert_eq!(count, 0);
            let directory = {
                let response = response(request).unwrap();
                let path = lock(&response)._directory.0.clone();
                path
            };
            assert!(directory.exists());
            assert_eq!(close(session), 1);
            assert!(!directory.exists());
            assert_eq!(close(request), 0);
            assert_eq!(crate::process::last_error(), 6);

            let (session, _, request) = request_for(port, "POST", "/upload", false);
            assert_eq!(
                send_request(request, null(), 0, b"ab".as_ptr().cast(), 2, 6, 0),
                1
            );
            assert_eq!(receive_response(request, null()), 0); // body not finished
            assert_eq!(
                write_data(request, b"cdef".as_ptr().cast(), 4, &mut count),
                1
            );
            assert_eq!(count, 4);
            assert_eq!(receive_response(request, null()), 1);
            assert_eq!(
                read_data(request, bytes.as_mut_ptr().cast(), 8, &mut count),
                1
            );
            assert_eq!(&bytes[..count as usize], b"posted");
            assert_eq!(close(session), 1);
        }
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[0]
            .0
            .starts_with("GET /Bonjour%20Monde?q=%C3%A9 HTTP/1.1"));
        assert!(requests[0].0.contains("X-Quote: a \" b\\c\r\n"));
        assert_eq!(requests[1].1, b"abcdef");
    }

    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn winhttp_rejects_untrusted_tls_and_security_bypass() {
        let _teb = crate::process::test_teb();
        let directory = std::env::temp_dir().join(format!(
            "winlux-tls-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let directory = PrivateDir(directory);
        let key = directory.0.join("key.pem");
        let cert = directory.0.join("cert.pem");
        let result = Command::new("openssl")
            .args([
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-days",
                "1",
                "-subj",
                "/CN=localhost",
                "-addext",
                "subjectAltName=DNS:localhost,IP:127.0.0.1",
                "-keyout",
            ])
            .arg(&key)
            .arg("-out")
            .arg(&cert)
            .output()
            .expect("openssl is required for the local TLS regression");
        assert!(
            result.status.success(),
            "TLS fixture certificate generation failed"
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let mut server = ChildGuard(
            Command::new("openssl")
                .args(["s_server", "-quiet", "-www", "-accept"])
                .arg(address.to_string())
                .arg("-cert")
                .arg(cert)
                .arg("-key")
                .arg(key)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if TcpStream::connect_timeout(&address, Duration::from_millis(50)).is_ok() {
                break;
            }
            assert!(
                Instant::now() < deadline && server.0.try_wait().unwrap().is_none(),
                "TLS fixture did not start"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        unsafe {
            let (session, _, request) = request_for(address.port(), "GET", "/", true);
            let ignore_unknown_ca = 0x100u32;
            assert_eq!(
                set_option(request, 31, (&ignore_unknown_ca as *const u32).cast(), 4),
                0
            );
            assert_eq!(crate::process::last_error(), 50);
            assert_eq!(send_request(request, null(), 0, null(), 0, 0, 0), 1);
            assert_eq!(receive_response(request, null()), 0);
            assert_eq!(crate::process::last_error(), 12175);
            assert_eq!(close(session), 1);
            assert_eq!(open(null(), 1, null(), null(), 0x1000_0000), 0);
            assert_eq!(crate::process::last_error(), 50);
        }
    }
}
