//! Lot E de #1410 (#1614) : wininet (cache d'URL, cookies, URL
//! canoniques), winspool (formulaires, ports, moniteurs, processeurs
//! d'impression, notifications), setupapi, shlwapi et ntdll.
//!
//! Le cache d'URL et les cookies de wininet vivent dans le processus :
//! ce qu'un programme y range, il le retrouve, comme sur un poste dont le
//! cache est vidé à chaque session. Les formulaires sont les formats
//! papier intégrés de Windows ; les ports sont ceux des imprimantes CUPS
//! plus FILE:.
//!
//! Sources : documentation Microsoft Learn des fonctions et des structures
//! INTERNET_CACHE_ENTRY_INFOW, INTERNET_CACHE_GROUP_INFOW, FORM_INFO_1W,
//! PORT_INFO_1W/2W, MONITOR_INFO_1W/2W, PRINTPROCESSOR_INFO_1W,
//! DATATYPES_INFO_1W, DRIVER_INFO_1W/2W, STRRET, KEY_BASIC_INFORMATION,
//! KEY_NODE_INFORMATION, KEY_FULL_INFORMATION, KEY_NAME_INFORMATION
//! (wininet.h, winspool.h, shtypes.h, wdm.h).
#![allow(non_snake_case)]
#![allow(clippy::too_many_arguments)]

use crate::dlls::lot_e_comptes::{pack_at, packed_size, F};
use crate::process::set_last_error;
use crate::wow32::{pointer_size, write_ptr};
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_NO_MORE_FILES: u32 = 18;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_INVALID_LEVEL: u32 = 124;
const ERROR_NO_MORE_ITEMS: u32 = 259;
const ERROR_UNKNOWN_PRINTPROCESSOR: u32 = 1798;
const ERROR_INVALID_PRINTER_NAME: u32 = 1801;
const ERROR_INVALID_FORM_NAME: u32 = 1902;
const ERROR_INTERNET_INCORRECT_HANDLE_TYPE: u32 = 12018;

unsafe fn wide_opt(p: *const u16) -> Option<String> {
    (!p.is_null()).then(|| crate::dlls::wstr(p))
}
fn utf16z(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

static NEXT_HANDLE: AtomicUsize = AtomicUsize::new(0x7A00_0000);
fn new_handle() -> usize {
    NEXT_HANDLE.fetch_add(4, Ordering::Relaxed)
}

// ============================================================
//  wininet : cache d'URL
// ============================================================

#[derive(Clone)]
struct CacheEntry {
    file: String,
    kind: u32,
    headers: Vec<u16>,
    ext: String,
    size: u64,
    modified: u64,
    expires: u64,
    groups: Vec<i64>,
}

#[derive(Default)]
struct Cache {
    entries: BTreeMap<String, CacheEntry>,
    groups: BTreeMap<i64, (u32, String)>,
    next_group: i64,
    /// Énumérations ouvertes : URL restantes ou groupes restants.
    finds: BTreeMap<usize, Vec<String>>,
    group_finds: BTreeMap<usize, Vec<i64>>,
}
static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

fn with_cache<R>(f: impl FnOnce(&mut Cache) -> R) -> R {
    let mut c = crate::dlls::lock(&CACHE);
    f(c.get_or_insert_with(|| Cache {
        next_group: 1,
        ..Cache::default()
    }))
}

fn cache_dir() -> String {
    format!(
        "{}\\AppData\\Local\\Microsoft\\Windows\\INetCache\\Winlux",
        crate::paths::profile_windows()
    )
}

/// CreateUrlCacheEntryW : nom de fichier neuf dans le dossier du cache.
unsafe extern "win64" fn CreateUrlCacheEntryW(
    url: *const u16,
    _expected: u32,
    ext: *const u16,
    out: *mut u16,
    _reserved: u32,
) -> i32 {
    let (Some(url), false) = (wide_opt(url), out.is_null()) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let dir = cache_dir();
    let _ = std::fs::create_dir_all(crate::paths::to_linux(&dir));
    static N: AtomicUsize = AtomicUsize::new(1);
    let mut hash: u32 = 0x811c_9dc5;
    for b in url.bytes() {
        hash = (hash ^ b as u32).wrapping_mul(0x0100_0193);
    }
    let ext = wide_opt(ext)
        .filter(|e| !e.is_empty())
        .map(|e| format!(".{}", e.trim_start_matches('.')))
        .unwrap_or_default();
    let path = format!(
        "{dir}\\{:08X}{:04X}{ext}",
        hash,
        N.fetch_add(1, Ordering::Relaxed) & 0xFFFF
    );
    let w = utf16z(&path);
    if w.len() > 260 {
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), out, w.len());
    1
}

/// CommitUrlCacheEntryW : l'entrée est rangée sous son URL.
unsafe extern "win64" fn CommitUrlCacheEntryW(
    url: *const u16,
    file: *const u16,
    expires: u64,
    modified: u64,
    kind: u32,
    headers: *const u16,
    headers_len: u32,
    ext: *const u16,
    _original: usize,
) -> i32 {
    let Some(url) = wide_opt(url) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let file = wide_opt(file).unwrap_or_default();
    let size = if file.is_empty() {
        0
    } else {
        match std::fs::metadata(crate::paths::to_linux(&file)) {
            Ok(m) => m.len(),
            Err(_) => {
                set_last_error(ERROR_FILE_NOT_FOUND);
                return 0;
            }
        }
    };
    let headers = if headers.is_null() {
        Vec::new()
    } else {
        std::slice::from_raw_parts(headers, headers_len as usize).to_vec()
    };
    let entry = CacheEntry {
        file,
        kind,
        headers,
        ext: wide_opt(ext).unwrap_or_default(),
        size,
        modified,
        expires,
        groups: Vec::new(),
    };
    with_cache(|c| c.entries.insert(url, entry));
    1
}

/// INTERNET_CACHE_ENTRY_INFOW suivie de ses chaînes ; taille en octets.
unsafe fn cache_info(url: &str, e: &CacheEntry, buf: *mut u8, size: *mut u32) -> i32 {
    if size.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let ps = pointer_size();
    let head = if ps == 4 { 80 } else { 112 };
    let (url_w, file_w, ext_w) = (utf16z(url), utf16z(&e.file), utf16z(&e.ext));
    let mut headers = e.headers.clone();
    headers.push(0);
    let need = head + 2 * (url_w.len() + file_w.len() + ext_w.len() + headers.len());
    if buf.is_null() || (*size as usize) < need {
        *size = need as u32;
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::write_bytes(buf, 0, head);
    let at = |o32: usize, o64: usize| buf.add(if ps == 4 { o32 } else { o64 });
    (buf as *mut u32).write_unaligned(head as u32);
    (at(12, 24) as *mut u32).write_unaligned(e.kind);
    (at(16, 28) as *mut u32).write_unaligned(1); // dwUseCount
    (at(24, 36) as *mut u32).write_unaligned(e.size as u32);
    (at(28, 40) as *mut u32).write_unaligned((e.size >> 32) as u32);
    (at(32, 44) as *mut u64).write_unaligned(e.modified);
    (at(40, 52) as *mut u64).write_unaligned(e.expires);
    (at(68, 88) as *mut u32).write_unaligned(e.headers.len() as u32 * 2);
    let mut text = buf.add(head);
    for (field, s) in [
        ((4, 8), &url_w),
        ((8, 16), &file_w),
        ((64, 80), &headers),
        ((72, 96), &ext_w),
    ] {
        write_ptr(at(field.0, field.1) as usize, text as usize);
        std::ptr::copy_nonoverlapping(s.as_ptr() as *const u8, text, s.len() * 2);
        text = text.add(s.len() * 2);
    }
    *size = need as u32;
    1
}

unsafe extern "win64" fn GetUrlCacheEntryInfoW(
    url: *const u16,
    buf: *mut u8,
    size: *mut u32,
) -> i32 {
    let Some(url) = wide_opt(url) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let Some(e) = with_cache(|c| c.entries.get(&url).cloned()) else {
        set_last_error(ERROR_FILE_NOT_FOUND);
        return 0;
    };
    cache_info(&url, &e, buf, size)
}

/// Prochaine entrée d'une énumération ; elle n'avance que si le tampon
/// suffit.
unsafe fn find_next(h: usize, buf: *mut u8, size: *mut u32) -> i32 {
    let next = with_cache(|c| {
        let list = c.finds.get(&h)?;
        let url = list.first()?.clone();
        Some((url.clone(), c.entries.get(&url).cloned()))
    });
    match next {
        None => {
            set_last_error(if with_cache(|c| c.finds.contains_key(&h)) {
                ERROR_NO_MORE_ITEMS
            } else {
                ERROR_INVALID_HANDLE
            });
            0
        }
        Some((url, None)) => {
            with_cache(|c| c.finds.get_mut(&h).map(|l| l.remove(0)));
            let _ = url;
            find_next(h, buf, size)
        }
        Some((url, Some(e))) => {
            let r = cache_info(&url, &e, buf, size);
            if r != 0 {
                with_cache(|c| c.finds.get_mut(&h).map(|l| l.remove(0)));
            }
            r
        }
    }
}

unsafe fn find_first(group: Option<i64>, buf: *mut u8, size: *mut u32) -> usize {
    let h = new_handle();
    with_cache(|c| {
        let list = c
            .entries
            .iter()
            .filter(|(_, e)| group.is_none_or(|g| g == 0 || e.groups.contains(&g)))
            .map(|(u, _)| u.clone())
            .collect();
        c.finds.insert(h, list)
    });
    if find_next(h, buf, size) == 0 {
        with_cache(|c| c.finds.remove(&h));
        return 0;
    }
    h
}

unsafe extern "win64" fn FindFirstUrlCacheEntryW(
    _pattern: usize,
    buf: *mut u8,
    size: *mut u32,
) -> usize {
    find_first(None, buf, size)
}
unsafe extern "win64" fn FindFirstUrlCacheEntryExW(
    _pattern: usize,
    _flags: u32,
    _filter: u32,
    group: i64,
    buf: *mut u8,
    size: *mut u32,
    _r1: usize,
    _r2: usize,
    _r3: usize,
) -> usize {
    find_first(Some(group), buf, size)
}
unsafe extern "win64" fn FindNextUrlCacheEntryW(h: usize, buf: *mut u8, size: *mut u32) -> i32 {
    find_next(h, buf, size)
}
unsafe extern "win64" fn FindNextUrlCacheEntryExW(
    h: usize,
    buf: *mut u8,
    size: *mut u32,
    _r1: usize,
    _r2: usize,
    _r3: usize,
) -> i32 {
    find_next(h, buf, size)
}
unsafe extern "win64" fn FindCloseUrlCache(h: usize) -> i32 {
    if with_cache(|c| c.finds.remove(&h).is_some() || c.group_finds.remove(&h).is_some()) {
        1
    } else {
        set_last_error(ERROR_INVALID_HANDLE);
        0
    }
}

unsafe extern "win64" fn CreateUrlCacheGroup(flags: u32, _reserved: usize) -> i64 {
    with_cache(|c| {
        let id = c.next_group;
        c.next_group += 1;
        c.groups.insert(id, (flags, String::new()));
        id
    })
}
unsafe extern "win64" fn DeleteUrlCacheGroup(group: i64, _flags: u32, _reserved: usize) -> i32 {
    let found = with_cache(|c| {
        let found = c.groups.remove(&group).is_some();
        for e in c.entries.values_mut() {
            e.groups.retain(|&g| g != group);
        }
        found
    });
    if found {
        1
    } else {
        set_last_error(ERROR_FILE_NOT_FOUND);
        0
    }
}

unsafe fn next_group(h: usize, out: *mut i64) -> i32 {
    match with_cache(|c| c.group_finds.get_mut(&h).map(|l| l.pop())) {
        Some(Some(g)) => {
            if !out.is_null() {
                *out = g;
            }
            1
        }
        Some(None) => {
            set_last_error(ERROR_NO_MORE_FILES);
            0
        }
        None => {
            set_last_error(ERROR_INVALID_HANDLE);
            0
        }
    }
}
unsafe extern "win64" fn FindFirstUrlCacheGroup(
    _reserved: u32,
    _filter: u32,
    _cond: usize,
    _cond_size: u32,
    out: *mut i64,
    _r2: usize,
) -> usize {
    let h = new_handle();
    with_cache(|c| {
        let list: Vec<i64> = c.groups.keys().rev().copied().collect();
        c.group_finds.insert(h, list)
    });
    if next_group(h, out) == 0 {
        with_cache(|c| c.group_finds.remove(&h));
        return 0;
    }
    h
}
unsafe extern "win64" fn FindNextUrlCacheGroup(h: usize, out: *mut i64, _reserved: usize) -> i32 {
    next_group(h, out)
}

/// INTERNET_CACHE_GROUP_INFOW : dwGroupSize, dwGroupFlags, dwGroupType,
/// dwDiskUsage, dwDiskQuota, dwOwnerStorage[4], szGroupName[120].
const GROUP_INFO: usize = 276;

unsafe extern "win64" fn GetUrlCacheGroupAttributeW(
    group: i64,
    _reserved: u32,
    _attributes: u32,
    info: *mut u8,
    size: *mut u32,
    _r2: usize,
) -> i32 {
    let Some((flags, name)) = with_cache(|c| c.groups.get(&group).cloned()) else {
        set_last_error(ERROR_FILE_NOT_FOUND);
        return 0;
    };
    if info.is_null() || size.is_null() || (*size as usize) < GROUP_INFO {
        if !size.is_null() {
            *size = GROUP_INFO as u32;
        }
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::write_bytes(info, 0, GROUP_INFO);
    (info as *mut u32).write_unaligned(GROUP_INFO as u32);
    (info.add(4) as *mut u32).write_unaligned(flags);
    for (i, c) in name.encode_utf16().take(119).enumerate() {
        (info.add(36 + 2 * i) as *mut u16).write_unaligned(c);
    }
    *size = GROUP_INFO as u32;
    1
}

unsafe extern "win64" fn SetUrlCacheGroupAttributeW(
    group: i64,
    _reserved: u32,
    attributes: u32,
    info: *const u8,
    _r2: usize,
) -> i32 {
    const CACHEGROUP_ATTRIBUTE_FLAG: u32 = 0x0000_0002;
    const CACHEGROUP_ATTRIBUTE_GROUPNAME: u32 = 0x0000_0010;
    if info.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let flags = (info.add(4) as *const u32).read_unaligned();
    let name: Vec<u16> = (0..120)
        .map(|i| (info.add(36 + 2 * i) as *const u16).read_unaligned())
        .take_while(|&c| c != 0)
        .collect();
    let ok = with_cache(|c| {
        let Some(g) = c.groups.get_mut(&group) else {
            return false;
        };
        if attributes & CACHEGROUP_ATTRIBUTE_FLAG != 0 {
            g.0 = flags;
        }
        if attributes & CACHEGROUP_ATTRIBUTE_GROUPNAME != 0 {
            g.1 = String::from_utf16_lossy(&name);
        }
        true
    });
    if ok {
        1
    } else {
        set_last_error(ERROR_FILE_NOT_FOUND);
        0
    }
}

unsafe extern "win64" fn SetUrlCacheEntryGroupW(
    url: *const u16,
    flags: u32,
    group: i64,
    _r1: usize,
    _r2: u32,
    _r3: usize,
) -> i32 {
    const INTERNET_CACHE_GROUP_REMOVE: u32 = 4;
    let Some(url) = wide_opt(url) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let ok = with_cache(|c| {
        if !c.groups.contains_key(&group) {
            return false;
        }
        let Some(e) = c.entries.get_mut(&url) else {
            return false;
        };
        e.groups.retain(|&g| g != group);
        if flags & INTERNET_CACHE_GROUP_REMOVE == 0 {
            e.groups.push(group);
        }
        true
    });
    if ok {
        1
    } else {
        set_last_error(ERROR_FILE_NOT_FOUND);
        0
    }
}

// ============================================================
//  wininet : cookies et URL
// ============================================================

/// (hôte, chemin, nom, valeur).
static COOKIES: Mutex<Vec<(String, String, String, String)>> = Mutex::new(Vec::new());

/// Hôte et chemin d'une URL http(s).
fn host_path(url: &str) -> Option<(String, String)> {
    let rest = url.split_once("://")?.1;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let host = host
        .rsplit('@')
        .next()?
        .split(':')
        .next()?
        .to_ascii_lowercase();
    let path = path.split(['?', '#']).next().unwrap_or("/").to_string();
    (!host.is_empty()).then_some((host, path))
}

unsafe extern "win64" fn InternetSetCookieW(
    url: *const u16,
    name: *const u16,
    data: *const u16,
) -> i32 {
    let (Some((host, url_path)), Some(data)) =
        (wide_opt(url).and_then(|u| host_path(&u)), wide_opt(data))
    else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let mut parts = data.split(';').map(str::trim);
    let first = parts.next().unwrap_or("");
    let (cname, value) = match wide_opt(name).filter(|n| !n.is_empty()) {
        Some(n) => (n, first.to_string()),
        None => match first.split_once('=') {
            Some((n, v)) => (n.trim().to_string(), v.trim().to_string()),
            None => (String::new(), first.to_string()),
        },
    };
    let mut path = url_path;
    let mut domain = host;
    for attr in parts {
        let (k, v) = attr.split_once('=').unwrap_or((attr, ""));
        match k.trim().to_ascii_lowercase().as_str() {
            "path" if v.starts_with('/') => path = v.to_string(),
            "domain" if !v.is_empty() => domain = v.trim_start_matches('.').to_ascii_lowercase(),
            _ => {}
        }
    }
    let mut jar = crate::dlls::lock(&COOKIES);
    jar.retain(|c| !(c.0 == domain && c.1 == path && c.2 == cname));
    jar.push((domain, path, cname, value));
    1
}

unsafe extern "win64" fn InternetGetCookieW(
    url: *const u16,
    name: *const u16,
    data: *mut u16,
    size: *mut u32,
) -> i32 {
    let (Some((host, path)), false) = (wide_opt(url).and_then(|u| host_path(&u)), size.is_null())
    else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let only = wide_opt(name).filter(|n| !n.is_empty());
    let text = crate::dlls::lock(&COOKIES)
        .iter()
        .filter(|c| host == c.0 || host.ends_with(&format!(".{}", c.0)))
        .filter(|c| path.starts_with(&c.1))
        .filter(|c| only.as_ref().is_none_or(|n| *n == c.2))
        .map(|c| {
            if c.2.is_empty() {
                c.3.clone()
            } else {
                format!("{}={}", c.2, c.3)
            }
        })
        .collect::<Vec<_>>()
        .join("; ");
    if text.is_empty() {
        set_last_error(ERROR_NO_MORE_ITEMS);
        return 0;
    }
    let w = utf16z(&text);
    if data.is_null() {
        *size = w.len() as u32;
        return 1;
    }
    if (*size as usize) < w.len() {
        *size = w.len() as u32;
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), data, w.len());
    *size = w.len() as u32 - 1;
    1
}

/// Forme canonique d'une URL (drapeaux ICU_* de wininet.h).
fn canonicalize(url: &str, flags: u32) -> String {
    const ICU_DECODE: u32 = 0x1000_0000;
    const ICU_NO_ENCODE: u32 = 0x2000_0000;
    const ICU_NO_META: u32 = 0x0800_0000;
    const ICU_ENCODE_SPACES_ONLY: u32 = 0x0400_0000;
    const ICU_BROWSER_MODE: u32 = 0x0200_0000;
    let (scheme, rest) = match url.split_once("://") {
        Some((s, r)) => (format!("{}://", s.to_ascii_lowercase()), r.to_string()),
        None => (String::new(), url.to_string()),
    };
    let (authority, path) = match rest.find('/') {
        Some(i) if !scheme.is_empty() => (rest[..i].to_string(), rest[i..].to_string()),
        _ if !scheme.is_empty() => (rest.clone(), "/".to_string()),
        _ => (String::new(), rest.clone()),
    };
    let (path, tail) = match path.find(['?', '#']) {
        Some(i) => (path[..i].to_string(), path[i..].to_string()),
        None => (path, String::new()),
    };
    let mut path = path;
    if flags & ICU_NO_META == 0 && path.contains("/.") {
        let mut out: Vec<&str> = Vec::new();
        let segs: Vec<&str> = path.split('/').collect();
        for (i, s) in segs.iter().enumerate() {
            match *s {
                "." => {
                    if i == segs.len() - 1 {
                        out.push("");
                    }
                }
                ".." => {
                    if out.len() > 1 {
                        out.pop();
                    }
                    if i == segs.len() - 1 {
                        out.push("");
                    }
                }
                s => out.push(s),
            }
        }
        path = out.join("/");
    }
    let mut text = format!("{scheme}{authority}{path}{tail}");
    if flags & ICU_DECODE != 0 {
        let b = text.as_bytes();
        let mut v = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'%' && i + 2 < b.len() {
                if let Ok(x) = u8::from_str_radix(&text[i + 1..i + 3], 16) {
                    v.push(x);
                    i += 3;
                    continue;
                }
            }
            v.push(b[i]);
            i += 1;
        }
        text = String::from_utf8_lossy(&v).into_owned();
    }
    if flags & ICU_NO_ENCODE == 0 && flags & ICU_DECODE == 0 {
        let spaces_only = flags & ICU_ENCODE_SPACES_ONLY != 0;
        let keep_tail = flags & ICU_BROWSER_MODE != 0;
        let cut = if keep_tail {
            text.find(['?', '#']).unwrap_or(text.len())
        } else {
            text.len()
        };
        let mut out = String::new();
        for (i, c) in text.char_indices() {
            let unsafe_char = if spaces_only {
                c == ' '
            } else {
                c == ' ' || c.is_control() || "<>\"{}|\\^`".contains(c) || !c.is_ascii()
            };
            if unsafe_char && i < cut {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).bytes() {
                    out.push_str(&format!("%{b:02X}"));
                }
            } else {
                out.push(c);
            }
        }
        text = out;
    }
    text
}

unsafe extern "win64" fn InternetCanonicalizeUrlW(
    url: *const u16,
    buf: *mut u16,
    len: *mut u32,
    flags: u32,
) -> i32 {
    let (Some(url), false) = (wide_opt(url), len.is_null()) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let w = utf16z(&canonicalize(&url, flags));
    if buf.is_null() || (*len as usize) < w.len() {
        *len = w.len() as u32;
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    *len = w.len() as u32 - 1;
    1
}

unsafe extern "win64" fn InternetGoOnlineW(_url: usize, _hwnd: usize, _flags: u32) -> i32 {
    1
}

/// FtpCommandW et FtpOpenFileW : le wininet de Winlux n'ouvre pas de
/// session FTP (InternetConnect sert HTTP), la poignée n'est jamais du
/// bon type.
unsafe extern "win64" fn FtpCommandW(
    _h: usize,
    _expect: i32,
    _flags: u32,
    _cmd: usize,
    _ctx: usize,
    out: usize,
) -> i32 {
    if out != 0 {
        write_ptr(out, 0);
    }
    set_last_error(ERROR_INTERNET_INCORRECT_HANDLE_TYPE);
    0
}
unsafe extern "win64" fn FtpOpenFileW(
    _h: usize,
    _name: usize,
    _access: u32,
    _flags: u32,
    _ctx: usize,
) -> usize {
    set_last_error(ERROR_INTERNET_INCORRECT_HANDLE_TYPE);
    0
}

pub fn wininet_exports() -> crate::dlls::Exports {
    crate::exports! {
        "CreateUrlCacheEntryW" => fn CreateUrlCacheEntryW,
        "CommitUrlCacheEntryW" => fn CommitUrlCacheEntryW,
        "GetUrlCacheEntryInfoW" => fn GetUrlCacheEntryInfoW,
        "FindFirstUrlCacheEntryW" => fn FindFirstUrlCacheEntryW,
        "FindFirstUrlCacheEntryExW" => fn FindFirstUrlCacheEntryExW,
        "FindNextUrlCacheEntryW" => fn FindNextUrlCacheEntryW,
        "FindNextUrlCacheEntryExW" => fn FindNextUrlCacheEntryExW,
        "FindCloseUrlCache" => fn FindCloseUrlCache,
        "CreateUrlCacheGroup" => fn CreateUrlCacheGroup,
        "DeleteUrlCacheGroup" => fn DeleteUrlCacheGroup,
        "FindFirstUrlCacheGroup" => fn FindFirstUrlCacheGroup,
        "FindNextUrlCacheGroup" => fn FindNextUrlCacheGroup,
        "GetUrlCacheGroupAttributeW" => fn GetUrlCacheGroupAttributeW,
        "SetUrlCacheGroupAttributeW" => fn SetUrlCacheGroupAttributeW,
        "SetUrlCacheEntryGroupW" => fn SetUrlCacheEntryGroupW,
        "InternetSetCookieW" => fn InternetSetCookieW,
        "InternetGetCookieW" => fn InternetGetCookieW,
        "InternetCanonicalizeUrlW" => fn InternetCanonicalizeUrlW,
        "InternetGoOnlineW" => fn InternetGoOnlineW,
        "FtpCommandW" => fn FtpCommandW,
        "FtpOpenFileW" => fn FtpOpenFileW,
    }
}

// ============================================================
//  winspool
// ============================================================

/// Tableau de structures dans le tampon de l'appelant (cbBuf octets),
/// avec pcbNeeded et pcReturned.
unsafe fn fill(
    records: Vec<Vec<F>>,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
    returned: *mut u32,
) -> i32 {
    let need = packed_size(&records);
    if !needed.is_null() {
        *needed = need as u32;
    }
    if !returned.is_null() {
        *returned = 0;
    }
    if need > 0 && (buf.is_null() || (cb as usize) < need) {
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    if need > 0 {
        pack_at(&records, buf as usize);
    }
    if !returned.is_null() {
        *returned = records.len() as u32;
    }
    1
}

/// Formats papier intégrés : nom, largeur et hauteur en millièmes de mm.
const FORMS: [(&str, i32, i32); 8] = [
    ("Letter", 215_900, 279_400),
    ("Legal", 215_900, 355_600),
    ("Executive", 184_150, 266_700),
    ("A3", 297_000, 420_000),
    ("A4", 210_000, 297_000),
    ("A5", 148_000, 210_000),
    ("B5 (JIS)", 182_000, 257_000),
    ("Envelope DL", 110_000, 220_000),
];

/// FORM_INFO_1W : Flags (FORM_BUILTIN), pName, Size, ImageableArea.
fn form_record(name: &str, w: i32, h: i32) -> Vec<F> {
    vec![
        F::U32(2),
        F::Str(Some(name.into())),
        F::U32(w as u32),
        F::U32(h as u32),
        F::U32(0),
        F::U32(0),
        F::U32(w as u32),
        F::U32(h as u32),
    ]
}

unsafe extern "win64" fn EnumFormsW(
    h: usize,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
    returned: *mut u32,
) -> i32 {
    if !crate::dlls::winspool::is_printer_handle(h) {
        set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    }
    if level != 1 {
        set_last_error(ERROR_INVALID_LEVEL);
        return 0;
    }
    let recs = FORMS
        .iter()
        .map(|&(n, w, h)| form_record(n, w, h))
        .collect();
    fill(recs, buf, cb, needed, returned)
}

unsafe extern "win64" fn GetFormW(
    h: usize,
    name: *const u16,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
) -> i32 {
    if !crate::dlls::winspool::is_printer_handle(h) {
        set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    }
    if level != 1 {
        set_last_error(ERROR_INVALID_LEVEL);
        return 0;
    }
    let wanted = wide_opt(name).unwrap_or_default();
    let Some(&(n, w, hh)) = FORMS
        .iter()
        .find(|(n, _, _)| n.eq_ignore_ascii_case(&wanted))
    else {
        set_last_error(ERROR_INVALID_FORM_NAME);
        return 0;
    };
    fill(
        vec![form_record(n, w, hh)],
        buf,
        cb,
        needed,
        std::ptr::null_mut(),
    )
}

/// Ajout, modification et suppression de formulaires, de pilotes ou de
/// files : réservés à l'administration de CUPS, par les Paramètres.
unsafe extern "win64" fn spool_denied(_a: usize, _b: usize, _c: usize, _d: usize) -> i32 {
    set_last_error(ERROR_ACCESS_DENIED);
    0
}
/// DeleteMonitor(serveur, environnement, nom) : seul « Local Port » existe
/// (EnumMonitors) et il ne se supprime pas ; un autre nom est inconnu,
/// comme sur un Windows où ce moniteur n'a jamais été installé (#1757).
unsafe extern "win64" fn DeleteMonitorW(_server: usize, _env: usize, name: *const u16) -> i32 {
    let name = crate::dlls::wstr(name);
    crate::trace!("DeleteMonitor({name}) : moniteur absent");
    set_last_error(if name.eq_ignore_ascii_case(LOCAL_PORT) {
        ERROR_ACCESS_DENIED
    } else {
        ERROR_UNKNOWN_PRINT_MONITOR
    });
    0
}
unsafe extern "win64" fn DeleteMonitorA(_server: usize, _env: usize, name: *const u8) -> i32 {
    let name = crate::dlls::cstr(name);
    set_last_error(if name.eq_ignore_ascii_case(LOCAL_PORT) {
        ERROR_ACCESS_DENIED
    } else {
        ERROR_UNKNOWN_PRINT_MONITOR
    });
    0
}
const ERROR_UNKNOWN_PRINT_MONITOR: u32 = 3000;
unsafe extern "win64" fn no_connection(_name: usize) -> i32 {
    set_last_error(ERROR_INVALID_PRINTER_NAME);
    0
}

const LOCAL_PORT: &str = "Local Port";

unsafe extern "win64" fn EnumMonitorsW(
    _server: usize,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
    returned: *mut u32,
) -> i32 {
    let r = match level {
        1 => vec![F::Str(Some(LOCAL_PORT.into()))],
        2 => vec![
            F::Str(Some(LOCAL_PORT.into())),
            F::Str(Some(environment().into())),
            F::Str(Some("localspl.dll".into())),
        ],
        _ => {
            set_last_error(ERROR_INVALID_LEVEL);
            return 0;
        }
    };
    fill(vec![r], buf, cb, needed, returned)
}

fn environment() -> &'static str {
    if pointer_size() == 4 {
        "Windows NT x86"
    } else {
        "Windows x64"
    }
}

/// Ports : FILE: et celui de chaque imprimante (URI de CUPS).
fn ports() -> Vec<String> {
    let mut v = vec!["FILE:".to_string()];
    for p in crate::dlls::winspool::printers() {
        if !p.port.is_empty() && !v.contains(&p.port) {
            v.push(p.port);
        }
    }
    v
}

unsafe extern "win64" fn EnumPortsW(
    _server: usize,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
    returned: *mut u32,
) -> i32 {
    let recs = match level {
        1 => ports().into_iter().map(|p| vec![F::Str(Some(p))]).collect(),
        2 => ports()
            .into_iter()
            .map(|p| {
                vec![
                    F::Str(Some(p)),
                    F::Str(Some(LOCAL_PORT.into())),
                    F::Str(Some("Port local".into())),
                    F::U32(1), // PORT_TYPE_WRITE
                    F::U32(0),
                ]
            })
            .collect(),
        _ => {
            set_last_error(ERROR_INVALID_LEVEL);
            return 0;
        }
    };
    fill(recs, buf, cb, needed, returned)
}

unsafe extern "win64" fn EnumPrintProcessorsW(
    _server: usize,
    _env: usize,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
    returned: *mut u32,
) -> i32 {
    if level != 1 {
        set_last_error(ERROR_INVALID_LEVEL);
        return 0;
    }
    fill(
        vec![vec![F::Str(Some("winprint".into()))]],
        buf,
        cb,
        needed,
        returned,
    )
}

unsafe extern "win64" fn EnumPrintProcessorDatatypesW(
    _server: usize,
    processor: *const u16,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
    returned: *mut u32,
) -> i32 {
    if !wide_opt(processor).is_some_and(|p| p.eq_ignore_ascii_case("winprint")) {
        set_last_error(ERROR_UNKNOWN_PRINTPROCESSOR);
        return 0;
    }
    if level != 1 {
        set_last_error(ERROR_INVALID_LEVEL);
        return 0;
    }
    let recs = ["RAW", "NT EMF 1.008", "TEXT"]
        .iter()
        .map(|t| vec![F::Str(Some(t.to_string()))])
        .collect();
    fill(recs, buf, cb, needed, returned)
}

unsafe extern "win64" fn GetPrintProcessorDirectoryW(
    _server: usize,
    env: *const u16,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
) -> i32 {
    if level != 1 {
        set_last_error(ERROR_INVALID_LEVEL);
        return 0;
    }
    let env = wide_opt(env).unwrap_or_else(|| environment().into());
    let arch = if env.eq_ignore_ascii_case("Windows NT x86") {
        "W32X86"
    } else {
        "x64"
    };
    let w = utf16z(&format!("C:\\Windows\\System32\\spool\\prtprocs\\{arch}"));
    let need = (w.len() * 2) as u32;
    if !needed.is_null() {
        *needed = need;
    }
    if buf.is_null() || cb < need {
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr() as *const u8, buf, need as usize);
    1
}

/// EnumPrinterDriversW : le modèle de chaque imprimante sert de pilote.
unsafe extern "win64" fn EnumPrinterDriversW(
    _server: usize,
    _env: usize,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
    returned: *mut u32,
) -> i32 {
    let mut models: Vec<String> = crate::dlls::winspool::printers()
        .into_iter()
        .map(|p| p.model)
        .filter(|m| !m.is_empty())
        .collect();
    models.sort();
    models.dedup();
    let recs = match level {
        1 => models.into_iter().map(|m| vec![F::Str(Some(m))]).collect(),
        2 => models
            .into_iter()
            .map(|m| {
                vec![
                    F::U32(3),
                    F::Str(Some(m)),
                    F::Str(Some(environment().into())),
                    F::Str(Some(String::new())),
                    F::Str(Some(String::new())),
                    F::Str(Some(String::new())),
                ]
            })
            .collect(),
        _ => {
            set_last_error(ERROR_INVALID_LEVEL);
            return 0;
        }
    };
    fill(recs, buf, cb, needed, returned)
}

/// AddJobW : pas de file d'attente en fichier ; StartDocPrinter reste
/// la voie d'impression.
unsafe extern "win64" fn AddJobW(
    _h: usize,
    _level: u32,
    _buf: usize,
    _cb: u32,
    needed: *mut u32,
) -> i32 {
    if !needed.is_null() {
        *needed = 0;
    }
    set_last_error(ERROR_NOT_SUPPORTED);
    0
}
unsafe extern "win64" fn ScheduleJob(_h: usize, _job: u32) -> i32 {
    set_last_error(ERROR_INVALID_PARAMETER);
    0
}

/// FlushPrinter : les octets vont à l'imprimante comme par WritePrinter,
/// puis l'attente demandée.
unsafe extern "win64" fn FlushPrinter(
    h: usize,
    buf: *const u8,
    cb: u32,
    written: *mut u32,
    sleep_ms: u32,
) -> i32 {
    let r = crate::dlls::winspool::write_printer(h, buf, cb, written);
    if r != 0 && sleep_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(sleep_ms as u64));
    }
    r
}

static CHANGES: Mutex<Vec<usize>> = Mutex::new(Vec::new());

/// FindFirstPrinterChangeNotification : événement jamais signalé (la
/// liste des imprimantes se relit par EnumPrinters).
unsafe extern "win64" fn FindFirstPrinterChangeNotification(
    h: usize,
    _filter: u32,
    _options: u32,
    _notify: usize,
) -> usize {
    if !crate::dlls::winspool::is_printer_handle(h) {
        set_last_error(ERROR_INVALID_HANDLE);
        return usize::MAX;
    }
    let e = crate::dlls::kernel32::create_event_w(0, 1, 0, std::ptr::null());
    if e != 0 {
        crate::dlls::lock(&CHANGES).push(e);
    }
    e
}
unsafe extern "win64" fn FindNextPrinterChangeNotification(
    h: usize,
    change: *mut u32,
    _options: usize,
    info: usize,
) -> i32 {
    if !crate::dlls::lock(&CHANGES).contains(&h) {
        set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    }
    if !change.is_null() {
        *change = 0;
    }
    if info != 0 {
        write_ptr(info, 0);
    }
    1
}
unsafe extern "win64" fn FindClosePrinterChangeNotification(h: usize) -> i32 {
    let mut list = crate::dlls::lock(&CHANGES);
    let Some(i) = list.iter().position(|&x| x == h) else {
        set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    list.remove(i);
    crate::dlls::kernel32::close_handle(h)
}

pub fn winspool_exports() -> crate::dlls::Exports {
    crate::exports! {
        "EnumFormsW" => fn EnumFormsW,
        "GetFormW" => fn GetFormW,
        "AddFormW" => fn spool_denied,
        "SetFormW" => fn spool_denied,
        "DeleteFormW" => fn spool_denied,
        "EnumMonitorsW" => fn EnumMonitorsW,
        "DeleteMonitorW" => fn DeleteMonitorW,
        "DeleteMonitorA" => fn DeleteMonitorA,
        "EnumPortsW" => fn EnumPortsW,
        "EnumPrintProcessorsW" => fn EnumPrintProcessorsW,
        "EnumPrintProcessorDatatypesW" => fn EnumPrintProcessorDatatypesW,
        "GetPrintProcessorDirectoryW" => fn GetPrintProcessorDirectoryW,
        "EnumPrinterDriversW" => fn EnumPrinterDriversW,
        "DeletePrinterDriverW" => fn spool_denied,
        "DeletePrinterDriverExW" => fn spool_denied,
        "AddPrinterConnectionW" => fn no_connection,
        "DeletePrinterConnectionW" => fn no_connection,
        "AddJobW" => fn AddJobW,
        "ScheduleJob" => fn ScheduleJob,
        "FlushPrinter" => fn FlushPrinter,
        "FindFirstPrinterChangeNotification" => fn FindFirstPrinterChangeNotification,
        "FindNextPrinterChangeNotification" => fn FindNextPrinterChangeNotification,
        "FindClosePrinterChangeNotification" => fn FindClosePrinterChangeNotification,
    }
}

// ============================================================
//  setupapi et cfgmgr32
// ============================================================

/// CM_Get_DevNode_Registry_PropertyA : la valeur W, chaînes en ANSI.
unsafe extern "win64" fn CM_Get_DevNode_Registry_PropertyA(
    dn: u32,
    prop: u32,
    kind: *mut u32,
    buf: *mut u8,
    len: *mut u32,
    flags: u32,
) -> u32 {
    const CR_BUFFER_SMALL: u32 = 0x1A;
    if len.is_null() {
        return 3; // CR_INVALID_POINTER
    }
    let mut t = 0u32;
    let mut size = 0u32;
    let r = crate::dlls::devnodes::CM_Get_DevNode_Registry_PropertyW(
        dn,
        prop,
        &mut t,
        std::ptr::null_mut(),
        &mut size,
        flags,
    );
    if r != CR_BUFFER_SMALL && r != 0 {
        return r;
    }
    let mut wide = vec![0u8; size as usize];
    let r = crate::dlls::devnodes::CM_Get_DevNode_Registry_PropertyW(
        dn,
        prop,
        &mut t,
        wide.as_mut_ptr(),
        &mut size,
        flags,
    );
    if r != 0 {
        return r;
    }
    let out: Vec<u8> = if t == 1 || t == 2 || t == 7 {
        let units: Vec<u16> = wide
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        crate::dlls::encode_ansi(&String::from_utf16_lossy(&units))
    } else {
        wide
    };
    if !kind.is_null() {
        *kind = t;
    }
    let room = *len;
    *len = out.len() as u32;
    if buf.is_null() || (room as usize) < out.len() {
        return CR_BUFFER_SMALL;
    }
    std::ptr::copy_nonoverlapping(out.as_ptr(), buf, out.len());
    0
}

pub fn setupapi_exports() -> crate::dlls::Exports {
    crate::exports! {
        "CM_Get_DevNode_Registry_PropertyA" => fn CM_Get_DevNode_Registry_PropertyA,
    }
}

// ============================================================
//  shlwapi
// ============================================================

const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const CLASS_E_CLASSNOTAVAILABLE: i32 = 0x8004_0111u32 as i32;

unsafe extern "win64" fn StrTrimW(text: *mut u16, chars: *const u16) -> i32 {
    if text.is_null() || chars.is_null() {
        return 0;
    }
    let set: Vec<u16> = std::slice::from_raw_parts(chars, crate::printf::wide_len(chars)).to_vec();
    let n = crate::printf::wide_len(text);
    let s = std::slice::from_raw_parts(text, n);
    let start = s.iter().position(|c| !set.contains(c)).unwrap_or(n);
    let end = s
        .iter()
        .rposition(|c| !set.contains(c))
        .map_or(start, |i| i + 1);
    if start == 0 && end == n {
        return 0;
    }
    std::ptr::copy(text.add(start), text, end - start);
    *text.add(end - start) = 0;
    1
}

/// StrRetToStrW : STRRET (uType, puis l'union alignée sur un pointeur) →
/// chaîne CoTaskMemAlloc ; un STRRET_WSTR est libéré.
unsafe extern "win64" fn StrRetToStrW(sr: *mut u8, pidl: *const u8, out: usize) -> i32 {
    if sr.is_null() || out == 0 {
        return E_INVALIDARG;
    }
    let ps = pointer_size();
    let union = sr.add(ps.max(4));
    let text = match (sr as *const u32).read_unaligned() {
        0 => {
            let p = crate::wow32::read_ptr(union as usize) as *mut u16;
            let s = wide_opt(p).unwrap_or_default();
            if !p.is_null() {
                crate::dlls::ole32::co_task_mem_free(p as *mut c_void);
            }
            s
        }
        1 if !pidl.is_null() => {
            crate::dlls::cstr(pidl.add((union as *const u32).read_unaligned() as usize))
        }
        2 => crate::dlls::cstr(sr.add(4)),
        _ => return E_INVALIDARG,
    };
    let w = utf16z(&text);
    let p = crate::dlls::ole32::co_task_mem_alloc(w.len() * 2) as *mut u16;
    if p.is_null() {
        return 0x8007_000Eu32 as i32;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
    write_ptr(out, p as usize);
    0
}

/// AssocCreate : CLSID_QueryAssociations seulement.
unsafe extern "win64" fn AssocCreate(
    clsid: *const u8,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    const CLSID_QUERY_ASSOCIATIONS: [u8; 16] = [
        0xfd, 0x34, 0x70, 0xa0, 0xaa, 0x6c, 0x54, 0x49, 0xac, 0x3f, 0x97, 0xa2, 0x72, 0x16, 0xf9,
        0x8a,
    ];
    if clsid.is_null() || std::slice::from_raw_parts(clsid, 16) != CLSID_QUERY_ASSOCIATIONS {
        if !out.is_null() {
            *out = std::ptr::null_mut();
        }
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    crate::dlls::shell32_releve::assoc_create_empty(riid, out)
}

/// SHGetViewStatePropertyBag : pas de sacs d'état de vue de l'Explorateur.
unsafe extern "win64" fn SHGetViewStatePropertyBag(
    _pidl: usize,
    _name: usize,
    _flags: u32,
    _riid: usize,
    out: usize,
) -> i32 {
    if out != 0 {
        write_ptr(out, 0);
    }
    E_NOTIMPL
}

pub fn shlwapi_exports() -> crate::dlls::Exports {
    crate::exports! {
        "StrTrimW" => fn StrTrimW,
        "StrRetToStrW" => fn StrRetToStrW,
        "AssocCreate" => fn AssocCreate,
        "SHGetViewStatePropertyBag" => fn SHGetViewStatePropertyBag,
    }
}

// ============================================================
//  Imports non bloquants de libcef (#1638)
// ============================================================

/// WSCEnumProtocols : comme WSAEnumProtocolsW, aucun fournisseur de
/// transport inscrit (catalogue Winsock vide).
unsafe extern "win64" fn WSCEnumProtocols(
    _protocols: *const i32,
    _buf: *mut u8,
    len: *mut u32,
    _errno: *mut i32,
) -> i32 {
    if !len.is_null() {
        *len = 0;
    }
    0
}

/// WSCGetProviderPath : aucun fournisseur inscrit, WSAEINVAL.
unsafe extern "win64" fn WSCGetProviderPath(
    _id: *const u8,
    _path: *mut u16,
    _len: *mut i32,
    errno: *mut i32,
) -> i32 {
    if !errno.is_null() {
        *errno = 10022;
    }
    -1
}

/// CreateAppContainerProfile : pas de conteneurs d'application sous
/// Winlux ; HRESULT_FROM_WIN32(ERROR_NOT_SUPPORTED). Le bac à sable de
/// Chromium se passe alors de conteneur.
unsafe extern "win64" fn CreateAppContainerProfile(
    _name: usize,
    _display: usize,
    _description: usize,
    _caps: usize,
    _count: u32,
    sid: *mut usize,
) -> i32 {
    if !sid.is_null() {
        crate::wow32::write_ptr(sid as usize, 0);
    }
    0x8007_0032u32 as i32
}

/// Section critique de stratégie de groupe : aucune stratégie n'est
/// appliquée en arrière-plan, la poignée ne protège rien.
extern "win64" fn EnterCriticalPolicySection(machine: i32) -> usize {
    if machine != 0 {
        0x6D01
    } else {
        0x6D02
    }
}
extern "win64" fn LeaveCriticalPolicySection(section: usize) -> i32 {
    (section == 0x6D01 || section == 0x6D02) as i32
}
/// Aucune actualisation de stratégie n'arrive : l'événement n'est jamais
/// signalé, l'inscription réussit.
extern "win64" fn RegisterGPNotification(event: usize, _machine: i32) -> i32 {
    (event != 0) as i32
}
extern "win64" fn UnregisterGPNotification(event: usize) -> i32 {
    (event != 0) as i32
}

/// LsaLogonUser : pas de paquet d'authentification LSA ; ouvrir une
/// session passe par LogonUserW.
extern "win64" fn LsaLogonUser() -> i32 {
    0xC000_00BBu32 as i32 // STATUS_NOT_SUPPORTED
}

fn cef_exports() -> Vec<(&'static str, crate::dlls::Exports)> {
    vec![
        (
            "ws2_32",
            crate::exports! {
                "WSCEnumProtocols" => fn WSCEnumProtocols,
                "WSCGetProviderPath" => fn WSCGetProviderPath,
            },
        ),
        (
            "userenv",
            crate::exports! {
                "CreateAppContainerProfile" => fn CreateAppContainerProfile,
                "EnterCriticalPolicySection" => fn EnterCriticalPolicySection,
                "LeaveCriticalPolicySection" => fn LeaveCriticalPolicySection,
                "RegisterGPNotification" => fn RegisterGPNotification,
                "UnregisterGPNotification" => fn UnregisterGPNotification,
            },
        ),
        (
            "secur32",
            crate::exports! { "LsaLogonUser" => fn LsaLogonUser },
        ),
    ]
}

/// Exports du lot E (#1614) et des imports de libcef (#1638).
pub fn all() -> Vec<(&'static str, crate::dlls::Exports)> {
    let mut v = vec![
        ("wininet", wininet_exports()),
        ("winspool", winspool_exports()),
        ("setupapi", setupapi_exports()),
        ("cfgmgr32", setupapi_exports()),
        ("shlwapi", shlwapi_exports()),
    ];
    v.extend(cef_exports());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_et_cookies() {
        unsafe {
            let url = utf16z("https://exemple.org/a/b.png");
            let mut name = [0u16; 260];
            assert_eq!(
                CreateUrlCacheEntryW(
                    url.as_ptr(),
                    0,
                    utf16z("png").as_ptr(),
                    name.as_mut_ptr(),
                    0
                ),
                1
            );
            let file = crate::dlls::wstr(name.as_ptr());
            assert!(file.ends_with(".png"));
            std::fs::write(crate::paths::to_linux(&file), b"1234").unwrap();
            assert_eq!(
                CommitUrlCacheEntryW(
                    url.as_ptr(),
                    name.as_ptr(),
                    0,
                    0,
                    1,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0
                ),
                1
            );
            let mut size = 0u32;
            assert_eq!(
                GetUrlCacheEntryInfoW(url.as_ptr(), std::ptr::null_mut(), &mut size),
                0
            );
            let mut info = vec![0u8; size as usize];
            assert_eq!(
                GetUrlCacheEntryInfoW(url.as_ptr(), info.as_mut_ptr(), &mut size),
                1
            );
            assert_eq!(u32::from_le_bytes(info[36..40].try_into().unwrap()), 4);
            assert_eq!(
                crate::dlls::wstr(*(info.as_ptr().add(8) as *const *const u16)),
                "https://exemple.org/a/b.png"
            );
            let g = CreateUrlCacheGroup(0, 0);
            assert_eq!(SetUrlCacheEntryGroupW(url.as_ptr(), 0, g, 0, 0, 0), 1);
            let h = FindFirstUrlCacheEntryExW(0, 0, 0, g, info.as_mut_ptr(), &mut size, 0, 0, 0);
            assert_ne!(h, 0);
            assert_eq!(FindNextUrlCacheEntryW(h, info.as_mut_ptr(), &mut size), 0);
            assert_eq!(FindCloseUrlCache(h), 1);
            assert_eq!(DeleteUrlCacheGroup(g, 0, 0), 1);
            let _ = std::fs::remove_file(crate::paths::to_linux(&file));

            let site = utf16z("http://www.exemple.org/x/y");
            assert_eq!(
                InternetSetCookieW(
                    site.as_ptr(),
                    std::ptr::null(),
                    utf16z("a=1; path=/x; domain=.exemple.org").as_ptr()
                ),
                1
            );
            assert_eq!(
                InternetSetCookieW(site.as_ptr(), utf16z("b").as_ptr(), utf16z("2").as_ptr()),
                1
            );
            let mut n = 0u32;
            assert_eq!(
                InternetGetCookieW(
                    site.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    &mut n
                ),
                1
            );
            let mut out = vec![0u16; n as usize];
            assert_eq!(
                InternetGetCookieW(site.as_ptr(), std::ptr::null(), out.as_mut_ptr(), &mut n),
                1
            );
            assert_eq!(crate::dlls::wstr(out.as_ptr()), "a=1; b=2");
            let other = utf16z("http://autre.org/");
            assert_eq!(
                InternetGetCookieW(other.as_ptr(), std::ptr::null(), out.as_mut_ptr(), &mut n),
                0
            );
        }
    }

    #[test]
    fn url_canoniques() {
        assert_eq!(canonicalize("HTTP://h/a/./b/../c d", 0), "http://h/a/c%20d");
        assert_eq!(canonicalize("http://h/a%20b", 0x1000_0000), "http://h/a b");
        assert_eq!(canonicalize("http://h/a b", 0x2000_0000), "http://h/a b");
        assert_eq!(canonicalize("http://h", 0), "http://h/");
    }

    #[test]
    fn shlwapi_et_strret() {
        unsafe {
            let mut t = utf16z("  xx  ");
            assert_eq!(StrTrimW(t.as_mut_ptr(), utf16z(" ").as_ptr()), 1);
            assert_eq!(crate::dlls::wstr(t.as_ptr()), "xx");
            let mut sr = [0u8; 272];
            sr[0..4].copy_from_slice(&2u32.to_le_bytes());
            sr[4..8].copy_from_slice(b"abc\0");
            let mut out = 0usize;
            assert_eq!(
                StrRetToStrW(
                    sr.as_mut_ptr(),
                    std::ptr::null(),
                    &mut out as *mut _ as usize
                ),
                0
            );
            assert_eq!(crate::dlls::wstr(out as *const u16), "abc");
            crate::dlls::ole32::co_task_mem_free(out as *mut c_void);
        }
    }

    #[test]
    fn formulaires_et_ports() {
        unsafe {
            let (mut need, mut ret) = (0u32, 0u32);
            assert_eq!(
                EnumPortsW(0, 1, std::ptr::null_mut(), 0, &mut need, &mut ret),
                0
            );
            let mut buf = vec![0u8; need as usize];
            assert_eq!(
                EnumPortsW(0, 1, buf.as_mut_ptr(), need, &mut need, &mut ret),
                1
            );
            assert!(ret >= 1);
            assert_eq!(
                crate::dlls::wstr(*(buf.as_ptr() as *const *const u16)),
                "FILE:"
            );
            assert_eq!(
                EnumFormsW(0x1234, 1, buf.as_mut_ptr(), need, &mut need, &mut ret),
                0
            );
        }
    }
}
