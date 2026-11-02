//! WinINet (`wininet.dll`) et les téléchargements d'`urlmon.dll`, sur le
//! moteur de WinHTTP de Winlux (curl de l'hôte, certificats vérifiés).
//!
//! WinINet est l'API réseau historique de Windows : téléchargements,
//! mises à jour intégrées, vérification de licence… Ses poignées sont celles
//! de WinHTTP, ses constantes de requête (`HTTP_QUERY_*`, `HTTP_ADDREQ_*`,
//! `INTERNET_FLAG_SECURE`) ont les mêmes valeurs. Seul le mode synchrone est
//! pris en charge : une session ouverte avec `INTERNET_FLAG_ASYNC` est
//! refusée plutôt que de ne jamais rappeler le programme. FTP et Gopher ne
//! sont pas gérés.

use super::winhttp;
use super::{lock, Exports};
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::{Mutex, OnceLock};

const INTERNET_FLAG_ASYNC: u32 = 0x1000_0000;
const INTERNET_FLAG_SECURE: u32 = 0x0080_0000;
const INTERNET_SERVICE_HTTP: u32 = 3;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INTERNET_UNRECOGNIZED_SCHEME: u32 = 12006;
const ERROR_INTERNET_INVALID_URL: u32 = 12005;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

fn fail<T: Default>(code: u32) -> T {
    crate::process::set_last_error(code);
    T::default()
}

/// Poignées liées : une requête ouverte par InternetOpenUrl porte sa connexion.
fn owned() -> &'static Mutex<HashMap<usize, usize>> {
    static O: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
    O.get_or_init(|| Mutex::new(HashMap::new()))
}

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

unsafe fn a2w(p: *const u8) -> Option<Vec<u16>> {
    (!p.is_null()).then(|| w(&super::cstr(p)))
}

unsafe fn a2w_len(p: *const u8, len: u32) -> Option<Vec<u16>> {
    if p.is_null() {
        return None;
    }
    let s = if len == u32::MAX {
        super::cstr(p)
    } else {
        super::decode_ansi(std::slice::from_raw_parts(p, len as usize))
    };
    Some(w(&s))
}

fn ptr(v: &Option<Vec<u16>>) -> *const u16 {
    v.as_ref().map_or(null(), |v| v.as_ptr())
}

// ---------- Adresses ----------

/// Morceaux d'une adresse, comme les découpe InternetCrackUrl.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Url {
    pub scheme: String,
    pub scheme_id: i32,
    pub user: String,
    pub password: String,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub extra: String,
}

pub fn crack(url: &str) -> Option<Url> {
    let (scheme, rest) = url.split_once("://")?;
    let lower = scheme.to_ascii_lowercase();
    let (scheme_id, default_port) = match lower.as_str() {
        "ftp" => (1, 21),
        "http" => (3, 80),
        "https" => (4, 443),
        "file" => (5, 0),
        _ => (0, 0),
    };
    let split = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(split);
    let (cred, hostport) = match authority.rsplit_once('@') {
        Some((c, h)) => (Some(c), h),
        None => (None, authority),
    };
    let (user, password) = match cred {
        Some(c) => match c.split_once(':') {
            Some((u, p)) => (u.to_string(), p.to_string()),
            None => (c.to_string(), String::new()),
        },
        None => (String::new(), String::new()),
    };
    let (host, port) = if hostport.starts_with('[') {
        let end = hostport.find(']')?;
        let port = hostport[end + 1..]
            .strip_prefix(':')
            .and_then(|p| p.parse().ok());
        (hostport[..=end].to_string(), port.unwrap_or(default_port))
    } else {
        match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().ok()?),
            None => (hostport.to_string(), default_port),
        }
    };
    if host.is_empty() && scheme_id != 5 {
        return None;
    }
    let extra_at = tail.find(['?', '#']).unwrap_or(tail.len());
    Some(Url {
        scheme: scheme.to_string(),
        scheme_id,
        user,
        password,
        host,
        port,
        path: tail[..extra_at].to_string(),
        extra: tail[extra_at..].to_string(),
    })
}

/// URL_COMPONENTS : (pointeur, longueur) aux positions de la structure x64.
const FIELDS: [(usize, usize); 6] = [(8, 16), (24, 32), (40, 48), (56, 64), (72, 80), (88, 96)];

/// Remplit un URL_COMPONENTS (A ou W) : pointeurs dans l'adresse quand le
/// programme passe un pointeur nul et une longueur non nulle, copie sinon.
unsafe fn fill_components(
    url: &str,
    base: usize,
    u: &Url,
    comps: *mut u8,
    wide: bool,
) -> Result<(), u32> {
    let parts = [&u.scheme, &u.host, &u.user, &u.password, &u.path, &u.extra];
    let unit = if wide { 2 } else { 1 };
    // Position de chaque morceau dans l'adresse d'origine (en caractères).
    let mut offsets = [0usize; 6];
    let mut from = 0;
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() {
            offsets[i] = from;
            continue;
        }
        let at = url[from..]
            .find(p.as_str())
            .map(|x| x + from)
            .unwrap_or(from);
        offsets[i] = at;
        from = at + p.len();
    }
    for (i, (pp, lp)) in FIELDS.iter().enumerate() {
        let pptr = comps.add(*pp) as *mut usize;
        let plen = comps.add(*lp) as *mut u32;
        let text = parts[i];
        if *pptr == 0 {
            if *plen != 0 {
                let chars = if wide {
                    url[..offsets[i]].encode_utf16().count()
                } else {
                    url[..offsets[i]].len()
                };
                *pptr = base + chars * unit;
                *plen = if wide {
                    text.encode_utf16().count()
                } else {
                    text.len()
                } as u32;
            }
            continue;
        }
        if wide {
            let units: Vec<u16> = text.encode_utf16().collect();
            if (*plen as usize) <= units.len() {
                *plen = units.len() as u32 + 1;
                return Err(ERROR_INSUFFICIENT_BUFFER);
            }
            let dst = *pptr as *mut u16;
            std::ptr::copy_nonoverlapping(units.as_ptr(), dst, units.len());
            *dst.add(units.len()) = 0;
            *plen = units.len() as u32;
        } else {
            let bytes = super::encode_ansi(text);
            if (*plen as usize) <= bytes.len() {
                *plen = bytes.len() as u32 + 1;
                return Err(ERROR_INSUFFICIENT_BUFFER);
            }
            let dst = *pptr as *mut u8;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
            *dst.add(bytes.len()) = 0;
            *plen = bytes.len() as u32;
        }
    }
    *(comps.add(20) as *mut i32) = u.scheme_id;
    *(comps.add(36) as *mut u16) = u.port;
    Ok(())
}

extern "win64" fn crack_url_w(url: *const u16, len: u32, _flags: u32, comps: *mut u8) -> i32 {
    if url.is_null() || comps.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let s = unsafe {
        if len == 0 {
            super::wstr(url)
        } else {
            String::from_utf16_lossy(std::slice::from_raw_parts(url, len as usize))
        }
    };
    let Some(u) = crack(&s) else {
        return fail(ERROR_INTERNET_UNRECOGNIZED_SCHEME);
    };
    match unsafe { fill_components(&s, url as usize, &u, comps, true) } {
        Ok(()) => 1,
        Err(e) => fail(e),
    }
}

extern "win64" fn crack_url_a(url: *const u8, len: u32, _flags: u32, comps: *mut u8) -> i32 {
    if url.is_null() || comps.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let s = unsafe {
        if len == 0 {
            super::cstr(url)
        } else {
            super::decode_ansi(std::slice::from_raw_parts(url, len as usize))
        }
    };
    let Some(u) = crack(&s) else {
        return fail(ERROR_INTERNET_UNRECOGNIZED_SCHEME);
    };
    match unsafe { fill_components(&s, url as usize, &u, comps, false) } {
        Ok(()) => 1,
        Err(e) => fail(e),
    }
}

// ---------- Sessions, connexions, requêtes ----------

unsafe fn open_inner(
    agent: *const u16,
    access: u32,
    proxy: *const u16,
    bypass: *const u16,
    flags: u32,
) -> usize {
    if flags & INTERNET_FLAG_ASYNC != 0 {
        crate::log_warn!("wininet", "InternetOpen asynchrone non pris en charge");
        return fail(ERROR_NOT_SUPPORTED);
    }
    // INTERNET_OPEN_TYPE_* = WINHTTP_ACCESS_TYPE_* (0 préconfiguré, 1 direct, 3 proxy, 4).
    winhttp::open(agent, access, proxy, bypass, 0)
}

extern "win64" fn internet_open_w(
    agent: *const u16,
    access: u32,
    proxy: *const u16,
    bypass: *const u16,
    flags: u32,
) -> usize {
    unsafe { open_inner(agent, access, proxy, bypass, flags) }
}

extern "win64" fn internet_open_a(
    agent: *const u8,
    access: u32,
    proxy: *const u8,
    bypass: *const u8,
    flags: u32,
) -> usize {
    unsafe {
        let (a, p, b) = (a2w(agent), a2w(proxy), a2w(bypass));
        open_inner(ptr(&a), access, ptr(&p), ptr(&b), flags)
    }
}

#[allow(clippy::too_many_arguments)]
extern "win64" fn internet_connect_w(
    session: usize,
    server: *const u16,
    port: u16,
    _user: *const u16,
    _pass: *const u16,
    service: u32,
    _flags: u32,
    _ctx: usize,
) -> usize {
    if service != INTERNET_SERVICE_HTTP {
        crate::log_warn!(
            "wininet",
            "service {service} (FTP/Gopher) non pris en charge"
        );
        return fail(ERROR_NOT_SUPPORTED);
    }
    unsafe { winhttp::connect(session, server, port, 0) }
}

#[allow(clippy::too_many_arguments)]
extern "win64" fn internet_connect_a(
    session: usize,
    server: *const u8,
    port: u16,
    user: *const u8,
    pass: *const u8,
    service: u32,
    flags: u32,
    ctx: usize,
) -> usize {
    let s = unsafe { a2w(server) };
    let _ = (user, pass);
    internet_connect_w(session, ptr(&s), port, null(), null(), service, flags, ctx)
}

#[allow(clippy::too_many_arguments)]
unsafe fn open_request_inner(
    conn: usize,
    verb: *const u16,
    object: *const u16,
    version: *const u16,
    referrer: *const u16,
    accept: *const *const u16,
    flags: u32,
) -> usize {
    let verb_default = w("GET");
    let verb = if verb.is_null() {
        verb_default.as_ptr()
    } else {
        verb
    };
    let path_default = w("/");
    let object = if object.is_null() {
        path_default.as_ptr()
    } else {
        object
    };
    // Seul INTERNET_FLAG_SECURE compte : cache, cookies et maintien de la
    // connexion sont gérés par le moteur ; les drapeaux qui désactiveraient
    // la vérification des certificats sont ignorés (plus sûr).
    winhttp::open_request(
        conn,
        verb,
        object,
        version,
        referrer,
        accept,
        flags & INTERNET_FLAG_SECURE,
    )
}

#[allow(clippy::too_many_arguments)]
extern "win64" fn http_open_request_w(
    conn: usize,
    verb: *const u16,
    object: *const u16,
    version: *const u16,
    referrer: *const u16,
    accept: *const *const u16,
    flags: u32,
    _ctx: usize,
) -> usize {
    unsafe { open_request_inner(conn, verb, object, version, referrer, accept, flags) }
}

#[allow(clippy::too_many_arguments)]
extern "win64" fn http_open_request_a(
    conn: usize,
    verb: *const u8,
    object: *const u8,
    version: *const u8,
    referrer: *const u8,
    accept: *const *const u8,
    flags: u32,
    _ctx: usize,
) -> usize {
    unsafe {
        let (v, o, ver, r) = (a2w(verb), a2w(object), a2w(version), a2w(referrer));
        let mut types: Vec<Vec<u16>> = Vec::new();
        if !accept.is_null() {
            let mut p = accept;
            while !(*p).is_null() && types.len() < 64 {
                types.push(w(&super::cstr(*p)));
                p = p.add(1);
            }
        }
        let mut list: Vec<*const u16> = types.iter().map(|t| t.as_ptr()).collect();
        list.push(null());
        let acc = if accept.is_null() {
            null()
        } else {
            list.as_ptr()
        };
        open_request_inner(conn, ptr(&v), ptr(&o), ptr(&ver), ptr(&r), acc, flags)
    }
}

unsafe fn send_inner(
    req: usize,
    headers: *const u16,
    len: u32,
    optional: *const c_void,
    optlen: u32,
) -> i32 {
    let len = if headers.is_null() { 0 } else { len };
    if winhttp::send_request(req, headers, len, optional, optlen, optlen, 0) == 0 {
        return 0;
    }
    winhttp::receive_response(req, null())
}

extern "win64" fn http_send_request_w(
    req: usize,
    headers: *const u16,
    len: u32,
    optional: *const c_void,
    optlen: u32,
) -> i32 {
    unsafe { send_inner(req, headers, len, optional, optlen) }
}

extern "win64" fn http_send_request_a(
    req: usize,
    headers: *const u8,
    len: u32,
    optional: *const c_void,
    optlen: u32,
) -> i32 {
    unsafe {
        let h = a2w_len(headers, len);
        send_inner(req, ptr(&h), u32::MAX, optional, optlen)
    }
}

/// HttpSendRequestEx : envoi en plusieurs morceaux (InternetWriteFile), la
/// taille totale vient d'INTERNET_BUFFERS (dwBufferTotal en +40).
extern "win64" fn http_send_request_ex(
    req: usize,
    buffers_in: *const u8,
    _out: usize,
    _flags: u32,
    _ctx: usize,
) -> i32 {
    unsafe {
        let total = if buffers_in.is_null() {
            u32::MAX
        } else {
            *(buffers_in.add(40) as *const u32)
        };
        let total = if total == 0 { u32::MAX } else { total };
        winhttp::send_request(req, null(), 0, null(), 0, total, 0)
    }
}

extern "win64" fn http_end_request(req: usize, _out: usize, _flags: u32, _ctx: usize) -> i32 {
    unsafe { winhttp::receive_response(req, null()) }
}

extern "win64" fn internet_write_file(
    h: usize,
    data: *const c_void,
    size: u32,
    written: *mut u32,
) -> i32 {
    unsafe { winhttp::write_data(h, data, size, written) }
}

extern "win64" fn http_add_request_headers_w(
    req: usize,
    headers: *const u16,
    len: u32,
    flags: u32,
) -> i32 {
    unsafe { winhttp::add_request_headers(req, headers, len, flags) }
}

extern "win64" fn http_add_request_headers_a(
    req: usize,
    headers: *const u8,
    len: u32,
    flags: u32,
) -> i32 {
    unsafe {
        let h = a2w_len(headers, len);
        winhttp::add_request_headers(req, ptr(&h), u32::MAX, flags)
    }
}

/// HTTP_QUERY_CUSTOM : WinINet passe le nom de l'en-tête dans le tampon,
/// WinHTTP dans un paramètre à part.
const HTTP_QUERY_CUSTOM: u32 = 65535;

extern "win64" fn http_query_info_w(
    req: usize,
    level: u32,
    buf: *mut c_void,
    len: *mut u32,
    index: *mut u32,
) -> i32 {
    // WinINet compte en octets, WinHTTP aussi : passage direct.
    unsafe {
        let name = (level & 0xffff == HTTP_QUERY_CUSTOM && !buf.is_null())
            .then(|| w(&super::wstr(buf.cast())));
        winhttp::query_headers(req, level, ptr(&name), buf, len, index)
    }
}

extern "win64" fn http_query_info_a(
    req: usize,
    level: u32,
    buf: *mut c_void,
    len: *mut u32,
    index: *mut u32,
) -> i32 {
    if len.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    unsafe {
        if level & 0x2000_0000 != 0 {
            // HTTP_QUERY_FLAG_NUMBER : valeur binaire, identique.
            return winhttp::query_headers(req, level, null(), buf, len, index);
        }
        let name = (level & 0xffff == HTTP_QUERY_CUSTOM && !buf.is_null())
            .then(|| w(&super::cstr(buf.cast())));
        let mut wlen = 0u32;
        winhttp::query_headers(req, level, ptr(&name), null_mut(), &mut wlen, index);
        if wlen == 0 {
            return 0;
        }
        let mut tmp = vec![0u16; wlen as usize / 2 + 1];
        let mut got = wlen;
        if winhttp::query_headers(
            req,
            level,
            ptr(&name),
            tmp.as_mut_ptr().cast(),
            &mut got,
            index,
        ) == 0
        {
            return 0;
        }
        let text = String::from_utf16_lossy(&tmp[..got as usize / 2]);
        let bytes = super::encode_ansi(&text);
        if buf.is_null() || (*len as usize) <= bytes.len() {
            *len = bytes.len() as u32 + 1;
            return fail(ERROR_INSUFFICIENT_BUFFER);
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.cast::<u8>(), bytes.len());
        *buf.cast::<u8>().add(bytes.len()) = 0;
        *len = bytes.len() as u32;
        1
    }
}

/// InternetReadFile attend d'avoir rempli le tampon ou atteint la fin.
extern "win64" fn internet_read_file(h: usize, buf: *mut c_void, size: u32, read: *mut u32) -> i32 {
    if read.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let mut total = 0u32;
    unsafe {
        while total < size {
            let mut n = 0u32;
            if winhttp::read_data(
                h,
                buf.cast::<u8>().add(total as usize).cast(),
                size - total,
                &mut n,
            ) == 0
            {
                *read = total;
                return 0;
            }
            if n == 0 {
                break;
            }
            total += n;
        }
        *read = total;
    }
    1
}

extern "win64" fn internet_query_data_available(
    h: usize,
    avail: *mut u32,
    _flags: u32,
    _ctx: usize,
) -> i32 {
    unsafe { winhttp::query_data_available(h, avail) }
}

extern "win64" fn internet_close_handle(h: usize) -> i32 {
    let conn = lock(owned()).remove(&h);
    let r = unsafe { winhttp::close(h) };
    if let Some(c) = conn {
        unsafe { winhttp::close(c) };
    }
    r
}

unsafe fn open_url_inner(
    session: usize,
    url: &str,
    headers: Option<Vec<u16>>,
    flags: u32,
) -> usize {
    let Some(u) = crack(url) else {
        return fail(ERROR_INTERNET_INVALID_URL);
    };
    if u.scheme_id != 3 && u.scheme_id != 4 {
        crate::log_warn!(
            "wininet",
            "InternetOpenUrl({}) : schéma non pris en charge",
            u.scheme
        );
        return fail(ERROR_INTERNET_UNRECOGNIZED_SCHEME);
    }
    let host = w(u.host.trim_start_matches('[').trim_end_matches(']'));
    let conn = winhttp::connect(session, host.as_ptr(), u.port, 0);
    if conn == 0 {
        return 0;
    }
    let path = if u.path.is_empty() { "/" } else { &u.path };
    let object = w(&format!(
        "{path}{}",
        u.extra.split('#').next().unwrap_or("")
    ));
    let secure = if u.scheme_id == 4 {
        INTERNET_FLAG_SECURE
    } else {
        0
    };
    let req = open_request_inner(
        conn,
        null(),
        object.as_ptr(),
        null(),
        null(),
        null(),
        secure | flags,
    );
    if req == 0 {
        winhttp::close(conn);
        return 0;
    }
    if send_inner(req, ptr(&headers), u32::MAX, null(), 0) == 0 {
        let err = crate::process::last_error();
        winhttp::close(req);
        winhttp::close(conn);
        return fail(err);
    }
    lock(owned()).insert(req, conn);
    req
}

extern "win64" fn internet_open_url_w(
    session: usize,
    url: *const u16,
    headers: *const u16,
    len: u32,
    flags: u32,
    _ctx: usize,
) -> usize {
    unsafe {
        let u = super::wstr(url);
        let h = (!headers.is_null()).then(|| {
            let s = if len == u32::MAX || len == 0 {
                super::wstr(headers)
            } else {
                String::from_utf16_lossy(std::slice::from_raw_parts(headers, len as usize))
            };
            w(&s)
        });
        open_url_inner(session, &u, h, flags)
    }
}

extern "win64" fn internet_open_url_a(
    session: usize,
    url: *const u8,
    headers: *const u8,
    len: u32,
    flags: u32,
    _ctx: usize,
) -> usize {
    unsafe {
        let u = super::cstr(url);
        let h = if len == 0 {
            a2w(headers)
        } else {
            a2w_len(headers, len)
        };
        open_url_inner(session, &u, h, flags)
    }
}

// ---------- État et options ----------

/// Une route par défaut existe (connexion réseau active).
pub fn connected() -> bool {
    std::fs::read_to_string("/proc/net/route")
        .map(|t| {
            t.lines()
                .skip(1)
                .any(|l| l.split_whitespace().nth(1) == Some("00000000"))
        })
        .unwrap_or(false)
        || std::fs::read_to_string("/proc/net/ipv6_route")
            .map(|t| {
                t.lines().any(|l| {
                    l.starts_with("00000000000000000000000000000000 00") && !l.ends_with("lo")
                })
            })
            .unwrap_or(false)
}

extern "win64" fn internet_get_connected_state(flags: *mut u32, _reserved: u32) -> i32 {
    let on = connected();
    if !flags.is_null() {
        // INTERNET_CONNECTION_LAN | INTERNET_RAS_INSTALLED ; OFFLINE sinon.
        unsafe { *flags = if on { 0x02 | 0x10 } else { 0x20 } };
    }
    on as i32
}

extern "win64" fn internet_get_connected_state_ex(
    flags: *mut u32,
    name: *mut u16,
    len: u32,
    reserved: u32,
) -> i32 {
    if !name.is_null() && len > 0 {
        unsafe { super::put_text_w("LAN", name, len) };
    }
    internet_get_connected_state(flags, reserved)
}

extern "win64" fn internet_check_connection(_url: usize, _flags: u32, _reserved: u32) -> i32 {
    if connected() {
        1
    } else {
        fail(12029)
    }
}

extern "win64" fn internet_attempt_connect(_reserved: u32) -> u32 {
    if connected() {
        0
    } else {
        12029
    }
}

/// Options : les délais et réglages de cache sont acceptés (le moteur a les
/// siens), les autres refusés honnêtement.
extern "win64" fn internet_set_option(
    _h: usize,
    option: u32,
    _buf: *const c_void,
    _len: u32,
) -> i32 {
    match option {
        2 | 3 | 5 | 6 | 7 | 8 | 31 | 37 | 39 | 41 | 65 | 73 | 77 | 81 | 82 | 84 => 1,
        _ => {
            crate::log_warn!("wininet", "InternetSetOption({option}) ignorée");
            1
        }
    }
}

extern "win64" fn internet_query_option(
    _h: usize,
    option: u32,
    buf: *mut c_void,
    len: *mut u32,
) -> i32 {
    if len.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    unsafe {
        match option {
            // INTERNET_OPTION_CONNECT_TIMEOUT, RECEIVE/SEND_TIMEOUT (ms), SECURITY_FLAGS.
            2 | 5 | 6 | 7 | 8 | 31 => {
                let v: u32 = if option == 31 { 0 } else { 60_000 };
                if buf.is_null() || *len < 4 {
                    *len = 4;
                    return fail(ERROR_INSUFFICIENT_BUFFER);
                }
                *buf.cast::<u32>() = v;
                *len = 4;
                1
            }
            _ => fail(12018),
        }
    }
}

extern "win64" fn internet_set_status_callback(_h: usize, _cb: usize) -> usize {
    0
}

extern "win64" fn internet_get_last_response_info(
    error: *mut u32,
    buf: *mut u8,
    len: *mut u32,
) -> i32 {
    unsafe {
        if !error.is_null() {
            *error = 0;
        }
        if !len.is_null() {
            if !buf.is_null() && *len > 0 {
                *buf = 0;
            }
            *len = 0;
        }
    }
    1
}

extern "win64" fn delete_url_cache_entry(_url: usize) -> i32 {
    fail(2)
}

// ---------- urlmon ----------

const S_OK: u32 = 0;
const E_INVALIDARG: u32 = 0x8007_0057;
const INET_E_DOWNLOAD_FAILURE: u32 = 0x800C_0008;
const INET_E_RESOURCE_NOT_FOUND: u32 = 0x800C_0005;

pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)";

/// Télécharge `url` dans le fichier Linux `dest`. Code HTTP en cas d'échec.
pub fn download(url: &str, dest: &std::path::Path) -> Result<(), u32> {
    unsafe {
        let agent = w(USER_AGENT);
        let session = winhttp::open(agent.as_ptr(), 0, null(), null(), 0);
        if session == 0 {
            return Err(INET_E_DOWNLOAD_FAILURE);
        }
        let req = open_url_inner(session, url, None, 0);
        if req == 0 {
            winhttp::close(session);
            return Err(INET_E_DOWNLOAD_FAILURE);
        }
        let mut status = 0u32;
        let mut n = 4u32;
        winhttp::query_headers(
            req,
            19 | 0x2000_0000,
            null(),
            (&mut status as *mut u32).cast(),
            &mut n,
            null_mut(),
        );
        let mut result = Ok(());
        if !(200..300).contains(&status) {
            result = Err(if status == 404 {
                INET_E_RESOURCE_NOT_FOUND
            } else {
                INET_E_DOWNLOAD_FAILURE
            });
        } else {
            let mut data = Vec::new();
            let mut buf = vec![0u8; 65536];
            loop {
                let mut got = 0u32;
                if internet_read_file(req, buf.as_mut_ptr().cast(), buf.len() as u32, &mut got) == 0
                {
                    result = Err(INET_E_DOWNLOAD_FAILURE);
                    break;
                }
                if got == 0 {
                    break;
                }
                data.extend_from_slice(&buf[..got as usize]);
            }
            if result.is_ok() {
                if let Some(parent) = dest.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if std::fs::write(dest, &data).is_err() {
                    result = Err(INET_E_DOWNLOAD_FAILURE);
                }
            }
        }
        internet_close_handle(req);
        winhttp::close(session);
        result
    }
}

fn hresult(r: Result<(), u32>) -> u32 {
    match r {
        Ok(()) => S_OK,
        Err(e) => e,
    }
}

extern "win64" fn url_download_to_file_w(
    _caller: usize,
    url: *const u16,
    file: *const u16,
    _r: u32,
    _cb: usize,
) -> u32 {
    if url.is_null() || file.is_null() {
        return E_INVALIDARG;
    }
    let (u, f) = unsafe { (super::wstr(url), super::wstr(file)) };
    hresult(download(&u, &crate::paths::to_linux(&f)))
}

extern "win64" fn url_download_to_file_a(
    _caller: usize,
    url: *const u8,
    file: *const u8,
    _r: u32,
    _cb: usize,
) -> u32 {
    if url.is_null() || file.is_null() {
        return E_INVALIDARG;
    }
    let (u, f) = unsafe { (super::cstr(url), super::cstr(file)) };
    hresult(download(&u, &crate::paths::to_linux(&f)))
}

fn cache_target(url: &str) -> String {
    let name = url
        .split(['?', '#'])
        .next()
        .and_then(|u| u.rsplit('/').next())
        .filter(|n| !n.is_empty())
        .unwrap_or("telechargement");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("C:\\AppData\\Local\\Temp\\winlux-{stamp:x}-{name}")
}

extern "win64" fn url_download_to_cache_file_w(
    _c: usize,
    url: *const u16,
    out: *mut u16,
    cch: u32,
    _r: u32,
    _cb: usize,
) -> u32 {
    if url.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let u = unsafe { super::wstr(url) };
    let target = cache_target(&u);
    if let Err(e) = download(&u, &crate::paths::to_linux(&target)) {
        return e;
    }
    unsafe { super::put_text_w(&target, out, cch) };
    S_OK
}

extern "win64" fn url_download_to_cache_file_a(
    _c: usize,
    url: *const u8,
    out: *mut u8,
    cch: u32,
    _r: u32,
    _cb: usize,
) -> u32 {
    if url.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let u = unsafe { super::cstr(url) };
    let target = cache_target(&u);
    if let Err(e) = download(&u, &crate::paths::to_linux(&target)) {
        return e;
    }
    unsafe { super::put_text_a(&target, out, cch) };
    S_OK
}

extern "win64" fn obtain_user_agent_string(_option: u32, buf: *mut u8, size: *mut u32) -> u32 {
    if size.is_null() {
        return E_INVALIDARG;
    }
    unsafe {
        let need = USER_AGENT.len() as u32 + 1;
        if buf.is_null() || *size < need {
            *size = need;
            return E_INVALIDARG;
        }
        super::put_text_a(USER_AGENT, buf, *size);
        *size = need;
    }
    S_OK
}

pub fn exports() -> Exports {
    crate::exports! {
        "InternetOpenA" => fn internet_open_a,
        "InternetOpenW" => fn internet_open_w,
        "InternetConnectA" => fn internet_connect_a,
        "InternetConnectW" => fn internet_connect_w,
        "HttpOpenRequestA" => fn http_open_request_a,
        "HttpOpenRequestW" => fn http_open_request_w,
        "HttpSendRequestA" => fn http_send_request_a,
        "HttpSendRequestW" => fn http_send_request_w,
        "HttpSendRequestExA" => fn http_send_request_ex,
        "HttpSendRequestExW" => fn http_send_request_ex,
        "HttpEndRequestA" => fn http_end_request,
        "HttpEndRequestW" => fn http_end_request,
        "InternetWriteFile" => fn internet_write_file,
        "HttpAddRequestHeadersA" => fn http_add_request_headers_a,
        "HttpAddRequestHeadersW" => fn http_add_request_headers_w,
        "HttpQueryInfoA" => fn http_query_info_a,
        "HttpQueryInfoW" => fn http_query_info_w,
        "InternetReadFile" => fn internet_read_file,
        "InternetQueryDataAvailable" => fn internet_query_data_available,
        "InternetCloseHandle" => fn internet_close_handle,
        "InternetOpenUrlA" => fn internet_open_url_a,
        "InternetOpenUrlW" => fn internet_open_url_w,
        "InternetCrackUrlA" => fn crack_url_a,
        "InternetCrackUrlW" => fn crack_url_w,
        "InternetGetConnectedState" => fn internet_get_connected_state,
        "InternetGetConnectedStateExW" => fn internet_get_connected_state_ex,
        "InternetCheckConnectionA" => fn internet_check_connection,
        "InternetCheckConnectionW" => fn internet_check_connection,
        "InternetAttemptConnect" => fn internet_attempt_connect,
        "InternetSetOptionA" => fn internet_set_option,
        "InternetSetOptionW" => fn internet_set_option,
        "InternetQueryOptionA" => fn internet_query_option,
        "InternetQueryOptionW" => fn internet_query_option,
        "InternetSetStatusCallback" => fn internet_set_status_callback,
        "InternetSetStatusCallbackA" => fn internet_set_status_callback,
        "InternetSetStatusCallbackW" => fn internet_set_status_callback,
        "InternetGetLastResponseInfoA" => fn internet_get_last_response_info,
        "InternetGetLastResponseInfoW" => fn internet_get_last_response_info,
        "DeleteUrlCacheEntryA" => fn delete_url_cache_entry,
        "DeleteUrlCacheEntryW" => fn delete_url_cache_entry,
        "DeleteUrlCacheEntry" => fn delete_url_cache_entry,
    }
}

pub fn urlmon_exports() -> Exports {
    crate::exports! {
        "URLDownloadToFileA" => fn url_download_to_file_a,
        "URLDownloadToFileW" => fn url_download_to_file_w,
        "URLDownloadToCacheFileA" => fn url_download_to_cache_file_a,
        "URLDownloadToCacheFileW" => fn url_download_to_cache_file_w,
        "ObtainUserAgentString" => fn obtain_user_agent_string,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn decoupe_les_adresses() {
        let u = crack("https://moi:secret@exemple.fr:8443/dossier/page.html?q=1#ancre").unwrap();
        assert_eq!(u.scheme_id, 4);
        assert_eq!((u.user.as_str(), u.password.as_str()), ("moi", "secret"));
        assert_eq!((u.host.as_str(), u.port), ("exemple.fr", 8443));
        assert_eq!(u.path, "/dossier/page.html");
        assert_eq!(u.extra, "?q=1#ancre");
        let u = crack("http://exemple.fr").unwrap();
        assert_eq!((u.port, u.path.as_str()), (80, ""));
        assert_eq!(crack("https://[::1]/x").unwrap().host, "[::1]");
        assert!(crack("pas une adresse").is_none());
    }

    #[test]
    fn internet_crack_url_w_pointeurs_et_copie() {
        let url: Vec<u16> = w("https://exemple.fr:8443/a/b?c=d");
        let mut comps = [0u8; 104];
        comps[..4].copy_from_slice(&104u32.to_le_bytes());
        // Mode « pointeurs » pour l'hôte, copie pour le chemin.
        comps[32..36].copy_from_slice(&1u32.to_le_bytes());
        let mut path = [0u16; 32];
        comps[72..80].copy_from_slice(&(path.as_mut_ptr() as usize).to_le_bytes());
        comps[80..84].copy_from_slice(&32u32.to_le_bytes());
        assert_eq!(crack_url_w(url.as_ptr(), 0, 0, comps.as_mut_ptr()), 1);
        let host_ptr = usize::from_le_bytes(comps[24..32].try_into().unwrap());
        let host_len = u32::from_le_bytes(comps[32..36].try_into().unwrap()) as usize;
        let host = unsafe { std::slice::from_raw_parts(host_ptr as *const u16, host_len) };
        assert_eq!(String::from_utf16_lossy(host), "exemple.fr");
        assert_eq!(u16::from_le_bytes([comps[36], comps[37]]), 8443);
        assert_eq!(i32::from_le_bytes(comps[20..24].try_into().unwrap()), 4);
        assert_eq!(String::from_utf16_lossy(&path[..4]), "/a/b");
    }

    #[test]
    fn internet_open_url_et_url_download_to_file() {
        let _teb = crate::process::test_teb();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut s, _) = listener.accept().unwrap();
                let mut req = [0u8; 2048];
                let n = s.read(&mut req).unwrap();
                let text = String::from_utf8_lossy(&req[..n]).into_owned();
                let body = if text.starts_with("GET /fichier.txt") {
                    "Bonjour WinINet"
                } else {
                    "?"
                };
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nX-Winlux: oui\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        let agent = b"Test\0";
        let session = internet_open_a(agent.as_ptr(), 1, null(), null(), 0);
        assert_ne!(session, 0);
        let url = format!("http://127.0.0.1:{port}/fichier.txt\0");
        let req = internet_open_url_a(session, url.as_ptr(), null(), 0, 0, 0);
        assert_ne!(req, 0, "InternetOpenUrlA");
        let mut status = 0u32;
        let mut n = 4u32;
        assert_eq!(
            http_query_info_a(
                req,
                19 | 0x2000_0000,
                (&mut status as *mut u32).cast(),
                &mut n,
                null_mut()
            ),
            1
        );
        assert_eq!(status, 200);
        let mut buf = [0u8; 64];
        let mut got = 0u32;
        assert_eq!(
            internet_read_file(req, buf.as_mut_ptr().cast(), 64, &mut got),
            1
        );
        assert_eq!(&buf[..got as usize], b"Bonjour WinINet");
        // Texte d'un en-tête en ANSI.
        let mut name = *b"X-Winlux\0\0\0\0\0\0\0\0";
        let mut len = name.len() as u32;
        assert_eq!(
            http_query_info_a(req, 65535, name.as_mut_ptr().cast(), &mut len, null_mut()),
            1
        );
        assert_eq!(&name[..len as usize], b"oui");
        assert_eq!(internet_close_handle(req), 1);
        assert_eq!(internet_close_handle(session), 1);

        let dest = std::env::temp_dir().join(format!("winlux-urlmon-{}.txt", std::process::id()));
        let url = format!("http://127.0.0.1:{port}/fichier.txt");
        assert_eq!(download(&url, &dest), Ok(()));
        assert_eq!(std::fs::read(&dest).unwrap(), b"Bonjour WinINet");
        let _ = std::fs::remove_file(dest);
        server.join().unwrap();
    }
}
