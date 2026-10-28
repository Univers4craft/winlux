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
    connect_ms: i32,
    total_ms: i32,
    context: usize,
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
fn lock<T>(value: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    value
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
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
    if flags != 0 || !matches!(access, 0 | 1 | 3) {
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
            connect_ms: 60000,
            total_ms: 120000,
            context: 0,
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
pub unsafe extern "win64" fn open_request(
    connection: usize,
    verb: *const u16,
    path: *const u16,
    version: *const u16,
    referer: *const u16,
    accept: *const *const u16,
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
            if v.is_empty() { "GET".into() } else { v },
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
    if !accept.is_null() {
        let mut values = Vec::new();
        for index in 0..64 {
            let pointer = *accept.add(index);
            if pointer.is_null() {
                break;
            }
            let value = match wide(pointer, u32::MAX) {
                Ok(value) if clean(&value) => value,
                _ => return fail(87),
            };
            values.push(value);
            if index == 63 {
                return fail(87);
            }
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
    let mut config = format!("silent\nshow-error\nlocation\nmax-redirs = 10\nproto = \"=http,https\"\nproto-redir = {}\nmax-filesize = 1073741824\nurl = {}\nrequest = {}\nuser-agent = {}\noutput = {}\ndump-header = {}\n",
        quote(if request.secure { "=https" } else { "=http,https" }), quote(&url), quote(&request.verb), quote(&request.options.agent), quote(directory.0.join("body").to_str().ok_or(87u32)?), quote(directory.0.join("headers").to_str().ok_or(87u32)?));
    if request.options.connect_ms > 0 {
        config.push_str(&format!(
            "connect-timeout = {:.3}\n",
            request.options.connect_ms as f64 / 1000.0
        ));
    }
    if request.options.total_ms > 0 {
        config.push_str(&format!(
            "max-time = {:.3}\n",
            request.options.total_ms as f64 / 1000.0
        ));
    }
    if let Some(proxy) = &request.options.proxy {
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
    if option == 45 && size as usize == std::mem::size_of::<usize>() {
        options_mut(&mut entry.kind).context = value.cast::<usize>().read_unaligned();
        return 1;
    }
    // In particular, do not silently honor certificate-validation bypass flags.
    fail(50)
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
        return output_bytes(buffer, size, &bytes, bytes.len());
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
pub unsafe extern "win64" fn default_proxy(out: *mut ProxyInfo) -> i32 {
    if out.is_null() {
        return fail(87);
    }
    out.write(ProxyInfo {
        access: 1,
        proxy: std::ptr::null_mut(),
        bypass: std::ptr::null_mut(),
    });
    1
}
pub unsafe extern "win64" fn ie_proxy(out: *mut IeProxyConfig) -> i32 {
    if out.is_null() {
        return fail(87);
    }
    out.write(IeProxyConfig {
        autodetect: 0,
        url: std::ptr::null_mut(),
        proxy: std::ptr::null_mut(),
        bypass: std::ptr::null_mut(),
    });
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
pub unsafe extern "win64" fn status_callback(
    handle: usize,
    _callback: usize,
    _flags: u32,
    _reserved: usize,
) -> usize {
    crate::process::set_last_error(if lock(handles()).contains_key(&handle) {
        50
    } else {
        6
    });
    usize::MAX
}
pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WinHttpOpen" => fn open, "WinHttpConnect" => fn connect,
        "WinHttpOpenRequest" => fn open_request, "WinHttpAddRequestHeaders" => fn add_request_headers,
        "WinHttpSendRequest" => fn send_request, "WinHttpWriteData" => fn write_data,
        "WinHttpReceiveResponse" => fn receive_response, "WinHttpQueryHeaders" => fn query_headers,
        "WinHttpQueryDataAvailable" => fn query_data_available, "WinHttpReadData" => fn read_data,
        "WinHttpCloseHandle" => fn close, "WinHttpSetTimeouts" => fn set_timeouts,
        "WinHttpSetOption" => fn set_option, "WinHttpQueryOption" => fn query_option,
        "WinHttpGetDefaultProxyConfiguration" => fn default_proxy,
        "WinHttpGetIEProxyConfigForCurrentUser" => fn ie_proxy,
        "WinHttpGetProxyForUrl" => fn proxy_for_url, "WinHttpSetStatusCallback" => fn status_callback,
    }
}

#[cfg(test)]
mod tests {
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
