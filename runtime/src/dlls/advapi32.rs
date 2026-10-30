//! Registre Win32 persistant dans un fichier portable (valeurs binaires hexadécimales).
//! Les données restent consultables/modifiables depuis Linux via registry.tsv.

use std::collections::HashMap;
use std::ffi::{c_void, CString};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const ERROR_SUCCESS: i32 = 0;
const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_ACCESS_DENIED: i32 = 5;
const ERROR_INVALID_HANDLE: i32 = 6;
const ERROR_INVALID_DATA: i32 = 13;
const ERROR_INVALID_PARAMETER: i32 = 87;
const ERROR_MORE_DATA: i32 = 234;
const REG_CREATED_NEW_KEY: u32 = 1;
const REG_OPENED_EXISTING_KEY: u32 = 2;

#[derive(Clone, Default)]
struct RegistryKey {
    values: HashMap<String, RegistryValue>,
}
#[derive(Clone)]
struct RegistryValue {
    kind: u32,
    bytes: Vec<u8>,
}
#[derive(Clone)]
struct RegistryData {
    keys: HashMap<String, RegistryKey>,
}

impl Default for RegistryData {
    fn default() -> Self {
        let mut keys = HashMap::new();
        for hive in ["hkcr", "hkcu", "hklm", "hku", "hkcc"] {
            keys.insert(hive.to_string(), RegistryKey::default());
        }
        Self { keys }
    }
}

static REGISTRY: OnceLock<Mutex<RegistryData>> = OnceLock::new();
static KEY_HANDLES: OnceLock<Mutex<HashMap<usize, String>>> = OnceLock::new();
static NEXT_KEY_HANDLE: AtomicUsize = AtomicUsize::new(0x4000_0000);
#[cfg(test)]
pub(crate) static REGISTRY_TEST_LOCK: Mutex<()> = Mutex::new(());

use super::lock;
fn cache() -> &'static Mutex<RegistryData> {
    REGISTRY.get_or_init(|| Mutex::new(RegistryData::default()))
}
fn key_handles() -> &'static Mutex<HashMap<usize, String>> {
    KEY_HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn registry_path() -> std::path::PathBuf {
    if let Some(p) = std::env::var_os("WINLUX_REGISTRY_PATH") {
        return p.into();
    }
    crate::paths::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("registry.tsv")
}

fn hex(bytes: &[u8]) -> String {
    const D: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(D[(b >> 4) as usize] as char);
        out.push(D[(b & 15) as usize] as char);
    }
    out
}
fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    for pair in s.as_bytes().as_chunks::<2>().0 {
        let nib = |x: u8| match x {
            b'0'..=b'9' => Some(x - b'0'),
            b'a'..=b'f' => Some(x - b'a' + 10),
            b'A'..=b'F' => Some(x - b'A' + 10),
            _ => None,
        };
        out.push((nib(pair[0])? << 4) | nib(pair[1])?);
    }
    Some(out)
}

fn load_registry() -> Result<RegistryData, i32> {
    let path = registry_path();
    let mut text = String::new();
    match File::open(path) {
        Ok(mut f) => {
            f.read_to_string(&mut text)
                .map_err(|_| ERROR_ACCESS_DENIED)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(RegistryData::default()),
        Err(_) => return Err(ERROR_ACCESS_DENIED),
    }
    let mut lines = text.lines();
    if lines.next() != Some("WINLUX-REGISTRY-1") {
        return Err(ERROR_INVALID_DATA);
    }
    let mut data = RegistryData::default();
    for line in lines {
        let fields: Vec<_> = line.split('\t').collect();
        match fields.as_slice() {
            ["K", p] => {
                let p = String::from_utf8(unhex(p).ok_or(ERROR_INVALID_DATA)?)
                    .map_err(|_| ERROR_INVALID_DATA)?;
                data.keys.entry(p).or_default();
            }
            ["V", p, n, kind, value] => {
                let p = String::from_utf8(unhex(p).ok_or(ERROR_INVALID_DATA)?)
                    .map_err(|_| ERROR_INVALID_DATA)?;
                let n = String::from_utf8(unhex(n).ok_or(ERROR_INVALID_DATA)?)
                    .map_err(|_| ERROR_INVALID_DATA)?;
                let kind = kind.parse::<u32>().map_err(|_| ERROR_INVALID_DATA)?;
                let bytes = unhex(value).ok_or(ERROR_INVALID_DATA)?;
                data.keys
                    .entry(p)
                    .or_default()
                    .values
                    .insert(n, RegistryValue { kind, bytes });
            }
            _ => return Err(ERROR_INVALID_DATA),
        }
    }
    Ok(data)
}

fn acquire_registry_file_lock(path: &std::path::Path) -> Result<File, i32> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| ERROR_ACCESS_DENIED)?;
    }
    let lock_path = path.with_extension("lock");
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .map_err(|_| ERROR_ACCESS_DENIED)?;
    let rc = unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&f), libc::LOCK_EX) };
    if rc != 0 {
        return Err(ERROR_ACCESS_DENIED);
    }
    Ok(f)
}

fn persist_registry(data: &RegistryData) -> Result<(), i32> {
    let path = registry_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| ERROR_ACCESS_DENIED)?;
    }
    let mut text = String::from("WINLUX-REGISTRY-1\n");
    let mut keys: Vec<_> = data.keys.iter().collect();
    keys.sort_by_key(|(k, _)| *k);
    for (path, key) in keys {
        text.push_str("K\t");
        text.push_str(&hex(path.as_bytes()));
        text.push('\n');
        let mut values: Vec<_> = key.values.iter().collect();
        values.sort_by_key(|(n, _)| *n);
        for (name, value) in values {
            text.push_str("V\t");
            text.push_str(&hex(path.as_bytes()));
            text.push('\t');
            text.push_str(&hex(name.as_bytes()));
            text.push('\t');
            text.push_str(&value.kind.to_string());
            text.push('\t');
            text.push_str(&hex(&value.bytes));
            text.push('\n');
        }
    }
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("registry.tsv");
    let temp = path.with_file_name(format!("{filename}.tmp.{}", std::process::id()));
    let mut f = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temp)
        .map_err(|_| ERROR_ACCESS_DENIED)?;
    f.write_all(text.as_bytes())
        .and_then(|_| f.sync_all())
        .map_err(|_| ERROR_ACCESS_DENIED)?;
    std::fs::rename(&temp, &path).map_err(|_| ERROR_ACCESS_DENIED)?;
    Ok(())
}

fn mutate_registry(f: impl FnOnce(&mut RegistryData) -> Result<(), i32>) -> Result<(), i32> {
    let mut current = lock(cache());
    let path = registry_path();
    let _file_lock = acquire_registry_file_lock(&path)?;
    let mut latest = load_registry()?;
    f(&mut latest)?;
    persist_registry(&latest)?;
    *current = latest;
    Ok(())
}

fn read_registry<T>(f: impl FnOnce(&RegistryData) -> Result<T, i32>) -> Result<T, i32> {
    let mut current = lock(cache());
    let latest = load_registry()?;
    *current = latest;
    f(&current)
}

fn hive_name(handle: usize) -> Option<&'static str> {
    match handle as u32 {
        0x8000_0000 => Some("hkcr"),
        0x8000_0001 => Some("hkcu"),
        0x8000_0002 => Some("hklm"),
        0x8000_0003 => Some("hku"),
        0x8000_0005 => Some("hkcc"),
        _ => None,
    }
}

unsafe fn resolve_key(root: usize, subkey: *const u8) -> Result<String, i32> {
    let base = if let Some(hive) = hive_name(root) {
        hive.to_string()
    } else {
        lock(key_handles())
            .get(&root)
            .cloned()
            .ok_or(ERROR_INVALID_HANDLE)?
    };
    let sub = if subkey.is_null() {
        String::new()
    } else {
        crate::dlls::cstr(subkey)
    };
    let sub = sub.replace('/', "\\").trim_matches('\\').to_lowercase();
    if sub.is_empty() {
        Ok(base)
    } else {
        Ok(format!("{base}\\{sub}"))
    }
}

unsafe fn resolve_existing_handle(handle: usize) -> Result<String, i32> {
    if let Some(hive) = hive_name(handle) {
        return Ok(hive.to_string());
    }
    lock(key_handles())
        .get(&handle)
        .cloned()
        .ok_or(ERROR_INVALID_HANDLE)
}

fn new_key_handle(path: String) -> usize {
    let h = NEXT_KEY_HANDLE.fetch_add(1, Ordering::Relaxed);
    lock(key_handles()).insert(h, path);
    h
}

/// The local Linux identity owns HKCU; this is not Windows impersonation.
pub(crate) fn native_current_user_key_path() -> String {
    format!("\\Registry\\User\\S-1-22-1-{}", unsafe { libc::geteuid() })
}

unsafe fn native_key_path(root: usize, name: &str) -> Result<String, i32> {
    let (hive, path) = if name.starts_with('\\') {
        let lower = name.to_lowercase();
        let current = native_current_user_key_path().to_lowercase();
        if lower == current || lower.starts_with(&(current.clone() + "\\")) {
            (0x8000_0001, &name[current.len()..])
        } else if lower == "\\registry\\machine" || lower.starts_with("\\registry\\machine\\") {
            (0x8000_0002, &name[17..])
        } else if lower == "\\registry\\user" || lower.starts_with("\\registry\\user\\") {
            (0x8000_0003, &name[14..])
        } else {
            return Err(ERROR_FILE_NOT_FOUND);
        }
    } else {
        (root, name)
    };
    if path.contains('\0') {
        return Err(ERROR_INVALID_PARAMETER);
    }
    let base = resolve_existing_handle(hive)?;
    let sub = path.replace('/', "\\").trim_matches('\\').to_lowercase();
    let full = if sub.is_empty() {
        base
    } else {
        format!("{base}\\{sub}")
    };
    Ok(full)
}

pub(crate) unsafe fn open_native_key(
    root: usize,
    name: &str,
    _access: u32,
    result: *mut usize,
) -> i32 {
    if result.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    match native_key_path(root, name) {
        Ok(path) => open_key_path(path, result),
        Err(error) => error,
    }
}

pub(crate) unsafe fn create_native_key(
    root: usize,
    name: &str,
    result: *mut usize,
    disposition: *mut u32,
) -> i32 {
    if result.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    match native_key_path(root, name) {
        Ok(path) => create_key_path(path, result, disposition),
        Err(error) => error,
    }
}

pub(crate) fn close_native_key(handle: usize) -> bool {
    lock(key_handles()).remove(&handle).is_some()
}

pub(crate) unsafe fn query_native_value(handle: usize, name: &str) -> Result<(u32, Vec<u8>), i32> {
    let path = resolve_existing_handle(handle)?;
    read_registry(|registry| {
        let value = registry
            .keys
            .get(&path)
            .and_then(|key| key.values.get(&name.to_lowercase()))
            .ok_or(ERROR_FILE_NOT_FOUND)?;
        Ok((value.kind, value.bytes.clone()))
    })
}

pub unsafe extern "win64" fn reg_open_key_ex_a(
    root: usize,
    subkey: *const u8,
    _options: u32,
    _access: u32,
    result: *mut usize,
) -> i32 {
    if result.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let path = match resolve_key(root, subkey) {
        Ok(p) => p,
        Err(e) => return e,
    };
    open_key_path(path, result)
}

unsafe fn open_key_path(path: String, result: *mut usize) -> i32 {
    match read_registry(|r| {
        if r.keys.contains_key(&path) {
            Ok(())
        } else {
            Err(ERROR_FILE_NOT_FOUND)
        }
    }) {
        Ok(()) => {
            *result = new_key_handle(path);
            ERROR_SUCCESS
        }
        Err(e) => e,
    }
}

pub unsafe extern "win64" fn reg_create_key_ex_a(
    root: usize,
    subkey: *const u8,
    _reserved: u32,
    _class: *const u8,
    _options: u32,
    _access: u32,
    _security: usize,
    result: *mut usize,
    disposition: *mut u32,
) -> i32 {
    if result.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let path = match resolve_key(root, subkey) {
        Ok(p) => p,
        Err(e) => return e,
    };
    create_key_path(path, result, disposition)
}

unsafe fn create_key_path(path: String, result: *mut usize, disposition: *mut u32) -> i32 {
    let mut created = false;
    if let Err(e) = mutate_registry(|r| {
        created = !r.keys.contains_key(&path);
        let mut p = String::new();
        for component in path.split('\\') {
            if !p.is_empty() {
                p.push('\\');
            }
            p.push_str(component);
            r.keys.entry(p.clone()).or_default();
        }
        Ok(())
    }) {
        return e;
    }
    *result = new_key_handle(path);
    if !disposition.is_null() {
        *disposition = if created {
            REG_CREATED_NEW_KEY
        } else {
            REG_OPENED_EXISTING_KEY
        };
    }
    ERROR_SUCCESS
}

pub unsafe extern "win64" fn reg_close_key(handle: usize) -> i32 {
    if hive_name(handle).is_some() {
        return ERROR_SUCCESS;
    }
    if lock(key_handles()).remove(&handle).is_some() {
        ERROR_SUCCESS
    } else {
        ERROR_INVALID_HANDLE
    }
}

pub unsafe extern "win64" fn reg_set_value_ex_a(
    handle: usize,
    name: *const u8,
    _reserved: u32,
    kind: u32,
    data: *const u8,
    size: u32,
) -> i32 {
    let path = match resolve_existing_handle(handle) {
        Ok(p) => p,
        Err(e) => return e,
    };
    if size != 0 && data.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let name = if name.is_null() {
        String::new()
    } else {
        crate::dlls::cstr(name).to_lowercase()
    };
    let bytes = if size == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(data, size as usize).to_vec()
    };
    mutate_registry(|r| {
        let Some(key) = r.keys.get_mut(&path) else {
            return Err(ERROR_FILE_NOT_FOUND);
        };
        key.values.insert(name, RegistryValue { kind, bytes });
        Ok(())
    })
    .map_or_else(|e| e, |_| ERROR_SUCCESS)
}

pub unsafe extern "win64" fn reg_query_value_ex_a(
    handle: usize,
    name: *const u8,
    _reserved: *mut u32,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
) -> i32 {
    if size.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let path = match resolve_existing_handle(handle) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let name = if name.is_null() {
        String::new()
    } else {
        crate::dlls::cstr(name).to_lowercase()
    };
    let value = match read_registry(|r| {
        r.keys
            .get(&path)
            .ok_or(ERROR_FILE_NOT_FOUND)?
            .values
            .get(&name)
            .cloned()
            .ok_or(ERROR_FILE_NOT_FOUND)
    }) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if !kind.is_null() {
        *kind = value.kind;
    }
    let capacity = *size as usize;
    *size = value.bytes.len() as u32;
    if data.is_null() {
        return ERROR_SUCCESS;
    }
    if capacity < value.bytes.len() {
        return ERROR_MORE_DATA;
    }
    if !value.bytes.is_empty() {
        std::ptr::copy_nonoverlapping(value.bytes.as_ptr(), data, value.bytes.len());
    }
    ERROR_SUCCESS
}

pub unsafe extern "win64" fn reg_delete_value_a(handle: usize, name: *const u8) -> i32 {
    let path = match resolve_existing_handle(handle) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let name = if name.is_null() {
        String::new()
    } else {
        crate::dlls::cstr(name).to_lowercase()
    };
    mutate_registry(|r| {
        let Some(k) = r.keys.get_mut(&path) else {
            return Err(ERROR_FILE_NOT_FOUND);
        };
        if k.values.remove(&name).is_some() {
            Ok(())
        } else {
            Err(ERROR_FILE_NOT_FOUND)
        }
    })
    .map_or_else(|e| e, |_| ERROR_SUCCESS)
}

pub unsafe extern "win64" fn reg_delete_key_a(handle: usize, subkey: *const u8) -> i32 {
    let path = match resolve_key(handle, subkey) {
        Ok(p) => p,
        Err(e) => return e,
    };
    if hive_name(handle).is_some() && subkey.is_null() {
        return ERROR_ACCESS_DENIED;
    }
    mutate_registry(|r| {
        let prefix = format!("{path}\\");
        if r.keys.keys().any(|k| k.starts_with(&prefix)) {
            return Err(ERROR_ACCESS_DENIED);
        }
        if r.keys.remove(&path).is_some() {
            Ok(())
        } else {
            Err(ERROR_FILE_NOT_FOUND)
        }
    })
    .map_or_else(|e| e, |_| ERROR_SUCCESS)
}

pub unsafe extern "win64" fn reg_flush_key(handle: usize) -> i32 {
    let path = match resolve_existing_handle(handle) {
        Ok(p) => p,
        Err(e) => return e,
    };
    read_registry(|r| {
        if r.keys.contains_key(&path) {
            Ok(())
        } else {
            Err(ERROR_FILE_NOT_FOUND)
        }
    })
    .map_or_else(|e| e, |_| ERROR_SUCCESS)
}

unsafe fn wide_to_cstr(p: *const u16) -> Result<CString, i32> {
    if p.is_null() {
        return Ok(CString::new(Vec::<u8>::new()).unwrap());
    }
    let s = crate::dlls::wstr(p);
    CString::new(s).map_err(|_| ERROR_INVALID_PARAMETER)
}
pub unsafe extern "win64" fn reg_open_key_ex_w(
    root: usize,
    sub: *const u16,
    options: u32,
    access: u32,
    result: *mut usize,
) -> i32 {
    let s = match wide_to_cstr(sub) {
        Ok(s) => s,
        Err(e) => return e,
    };
    reg_open_key_ex_a(root, s.as_ptr() as *const u8, options, access, result)
}
pub unsafe extern "win64" fn reg_create_key_ex_w(
    root: usize,
    sub: *const u16,
    res: u32,
    _class: *const u16,
    options: u32,
    access: u32,
    sec: usize,
    result: *mut usize,
    disp: *mut u32,
) -> i32 {
    let s = match wide_to_cstr(sub) {
        Ok(s) => s,
        Err(e) => return e,
    };
    reg_create_key_ex_a(
        root,
        s.as_ptr() as *const u8,
        res,
        std::ptr::null(),
        options,
        access,
        sec,
        result,
        disp,
    )
}
pub unsafe extern "win64" fn reg_set_value_ex_w(
    handle: usize,
    name: *const u16,
    res: u32,
    kind: u32,
    data: *const u8,
    size: u32,
) -> i32 {
    let n = match wide_to_cstr(name) {
        Ok(n) => n,
        Err(e) => return e,
    };
    reg_set_value_ex_a(handle, n.as_ptr() as *const u8, res, kind, data, size)
}
pub unsafe extern "win64" fn reg_query_value_ex_w(
    handle: usize,
    name: *const u16,
    res: *mut u32,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
) -> i32 {
    let n = match wide_to_cstr(name) {
        Ok(n) => n,
        Err(e) => return e,
    };
    reg_query_value_ex_a(handle, n.as_ptr() as *const u8, res, kind, data, size)
}
pub unsafe extern "win64" fn reg_delete_value_w(handle: usize, name: *const u16) -> i32 {
    let n = match wide_to_cstr(name) {
        Ok(n) => n,
        Err(e) => return e,
    };
    reg_delete_value_a(handle, n.as_ptr() as *const u8)
}

pub unsafe extern "win64" fn get_user_name_a(buf: *mut u8, size: *mut u32) -> i32 {
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
    if buf.is_null() || size.is_null() {
        return 0;
    }
    let need = user.len() as u32 + 1;
    if *size < need {
        *size = need;
        crate::process::set_last_error(122);
        return 0;
    }
    std::ptr::copy_nonoverlapping(user.as_ptr(), buf, user.len());
    *buf.add(user.len()) = 0;
    *size = need;
    1
}
pub extern "win64" fn crypt_release_context(h: usize, _flags: u32) -> i32 {
    if h == 1 {
        1
    } else {
        crate::process::set_last_error(6);
        0
    }
}
pub unsafe extern "win64" fn crypt_acquire_context_a(
    ph: *mut usize,
    _container: *const u8,
    _provider: *const u8,
    _type: u32,
    _flags: u32,
) -> i32 {
    if ph.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    *ph = 1;
    1
}
pub unsafe extern "win64" fn crypt_gen_random(provider: usize, size: u32, buf: *mut u8) -> i32 {
    if provider != 1 || buf.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    if crate::dlls::bcrypt::bcrypt_gen_random(0, buf, size, 2) == 0 {
        1
    } else {
        crate::process::set_last_error(31);
        0
    }
}
pub unsafe extern "win64" fn event_log_stub(_args: *const c_void) -> i32 {
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
     "RegOpenKeyExA"=>fn reg_open_key_ex_a,"RegCreateKeyExA"=>fn reg_create_key_ex_a,"RegCloseKey"=>fn reg_close_key,
     "RegSetValueExA"=>fn reg_set_value_ex_a,"RegQueryValueExA"=>fn reg_query_value_ex_a,"RegDeleteValueA"=>fn reg_delete_value_a,
     "RegOpenKeyExW"=>fn reg_open_key_ex_w,"RegCreateKeyExW"=>fn reg_create_key_ex_w,"RegSetValueExW"=>fn reg_set_value_ex_w,
     "RegQueryValueExW"=>fn reg_query_value_ex_w,"RegDeleteValueW"=>fn reg_delete_value_w,"RegDeleteKeyA"=>fn reg_delete_key_a,"RegFlushKey"=>fn reg_flush_key,
     "GetUserNameA"=>fn get_user_name_a,"CryptAcquireContextA"=>fn crypt_acquire_context_a,"CryptReleaseContext"=>fn crypt_release_context,"CryptGenRandom"=>fn crypt_gen_random,
     "OpenEventLogA"=>fn event_log_stub,"RegisterEventSourceA"=>fn event_log_stub
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn persiste_valeurs_binaires_et_respecte_la_casse_insensible() {
        let _serial = REGISTRY_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let path =
            std::env::temp_dir().join(format!("winlux-registry-test-{}.tsv", std::process::id()));
        let previous = std::env::var_os("WINLUX_REGISTRY_PATH");
        std::env::set_var("WINLUX_REGISTRY_PATH", &path);
        let sub = CString::new("Software\\WinluxTest").unwrap();
        let name = CString::new("Binary").unwrap();
        let mut h = 0usize;
        let mut disp = 0u32;
        unsafe {
            assert_eq!(
                reg_create_key_ex_a(
                    0x8000_0001usize,
                    sub.as_ptr() as *const u8,
                    0,
                    std::ptr::null(),
                    0,
                    0,
                    0,
                    &mut h,
                    &mut disp
                ),
                0
            );
            assert_eq!(disp, REG_CREATED_NEW_KEY);
            let data = [0, 0xff, 0x10, 0x09];
            assert_eq!(
                reg_set_value_ex_a(
                    h,
                    name.as_ptr() as *const u8,
                    0,
                    3,
                    data.as_ptr(),
                    data.len() as u32
                ),
                0
            );
            let mut kind = 0;
            let mut size = 0;
            assert_eq!(
                reg_query_value_ex_a(
                    h,
                    name.as_ptr() as *const u8,
                    std::ptr::null_mut(),
                    &mut kind,
                    std::ptr::null_mut(),
                    &mut size
                ),
                0
            );
            assert_eq!(size, 4);
            assert_eq!(kind, 3);
            let mut short = [0u8; 2];
            size = 2;
            assert_eq!(
                reg_query_value_ex_a(
                    h,
                    name.as_ptr() as *const u8,
                    std::ptr::null_mut(),
                    &mut kind,
                    short.as_mut_ptr(),
                    &mut size
                ),
                ERROR_MORE_DATA
            );
            assert_eq!(size, 4);
            assert_eq!(reg_close_key(h), 0);
            let sub2 = CString::new("software\\winluxtest").unwrap();
            assert_eq!(
                reg_open_key_ex_a(0x8000_0001usize, sub2.as_ptr() as *const u8, 0, 0, &mut h),
                0
            );
            let mut out = [0u8; 4];
            size = 4;
            assert_eq!(
                reg_query_value_ex_a(
                    h,
                    name.as_ptr() as *const u8,
                    std::ptr::null_mut(),
                    &mut kind,
                    out.as_mut_ptr(),
                    &mut size
                ),
                0
            );
            assert_eq!(out, data);
            // Native NT callers and Win32 callers share the same persisted key.
            let mut current = [0u8; 16];
            assert_eq!(
                crate::dlls::ntdll::rtl_format_current_user_key_path(current.as_mut_ptr()),
                0
            );
            let n = std::ptr::read_unaligned(current.as_ptr().cast::<u16>()) as usize / 2;
            let p = std::ptr::read_unaligned(current.as_ptr().add(8).cast::<*const u16>());
            let formatted = String::from_utf16(std::slice::from_raw_parts(p, n)).unwrap();
            assert_eq!(formatted, native_current_user_key_path());
            crate::dlls::ntdll::rtl_free_unicode_string(current.as_mut_ptr());
            assert_eq!(std::ptr::read_unaligned(current.as_ptr().cast::<u16>()), 0);
            let full = format!("{}\\{}", formatted, sub.to_str().unwrap());
            let wide: Vec<u16> = full.encode_utf16().collect();
            let mut unicode = [0u8; 16];
            std::ptr::write_unaligned(unicode.as_mut_ptr().cast::<u16>(), (wide.len() * 2) as u16);
            std::ptr::write_unaligned(
                unicode.as_mut_ptr().add(2).cast::<u16>(),
                (wide.len() * 2) as u16,
            );
            std::ptr::write_unaligned(
                unicode.as_mut_ptr().add(8).cast::<*const u16>(),
                wide.as_ptr(),
            );
            let mut attrs = [0u8; 48];
            std::ptr::write_unaligned(attrs.as_mut_ptr().cast::<u32>(), 48);
            std::ptr::write_unaligned(
                attrs.as_mut_ptr().add(16).cast::<*const u8>(),
                unicode.as_ptr(),
            );
            let mut native = 0;
            assert_eq!(
                crate::dlls::ntdll::nt_open_key(&mut native, 0x20019, attrs.as_ptr()),
                0
            );
            let mut native_size = 4;
            let mut native_out = [0; 4];
            assert_eq!(
                reg_query_value_ex_a(
                    native,
                    name.as_ptr().cast(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    native_out.as_mut_ptr(),
                    &mut native_size
                ),
                0
            );
            assert_eq!(native_out, data);
            let value_name: Vec<u16> = "Binary".encode_utf16().collect();
            let mut value_string = [0u8; 16];
            std::ptr::write_unaligned(
                value_string.as_mut_ptr().cast::<u16>(),
                (value_name.len() * 2) as u16,
            );
            std::ptr::write_unaligned(
                value_string.as_mut_ptr().add(2).cast::<u16>(),
                (value_name.len() * 2) as u16,
            );
            std::ptr::write_unaligned(
                value_string.as_mut_ptr().add(8).cast::<*const u16>(),
                value_name.as_ptr(),
            );
            let mut required = 0;
            let mut partial = [0xccu8; 17];
            assert_eq!(
                crate::dlls::ntdll::nt_query_value_key(
                    native,
                    value_string.as_ptr(),
                    2,
                    std::ptr::null_mut(),
                    0,
                    &mut required
                ),
                0xC000_0023u32 as i32
            );
            assert_eq!(required, 16);
            assert_eq!(
                crate::dlls::ntdll::nt_query_value_key(
                    native,
                    value_string.as_ptr(),
                    2,
                    partial.as_mut_ptr(),
                    15,
                    &mut required
                ),
                0x8000_0005u32 as i32
            );
            assert_eq!(partial[15], 0xcc);
            assert_eq!(
                crate::dlls::ntdll::nt_query_value_key(
                    native,
                    value_string.as_ptr(),
                    2,
                    partial.as_mut_ptr(),
                    16,
                    &mut required
                ),
                0
            );
            assert_eq!(&partial[12..16], &data);
            assert_eq!(partial[16], 0xcc);
            assert_eq!(u32::from_le_bytes(partial[4..8].try_into().unwrap()), 3);
            for class in [0, 1] {
                let mut buffer = [0u8; 64];
                assert_eq!(
                    crate::dlls::ntdll::nt_query_value_key(
                        native,
                        value_string.as_ptr(),
                        class,
                        buffer.as_mut_ptr(),
                        64,
                        &mut required
                    ),
                    0
                );
                let header = if class == 0 { 12 } else { 20 };
                let native_name: Vec<u8> = value_name
                    .iter()
                    .flat_map(|unit| unit.to_le_bytes())
                    .collect();
                assert_eq!(
                    &buffer[header..header + native_name.len()],
                    native_name.as_slice()
                );
                if class == 1 {
                    let offset = u32::from_le_bytes(buffer[8..12].try_into().unwrap()) as usize;
                    assert_eq!(&buffer[offset..offset + 4], &data);
                }
            }
            assert_eq!(crate::dlls::ntdll::nt_close(native), 0);
            assert_ne!(crate::dlls::ntdll::nt_close(native), 0);
            let mut native_disposition = 0;
            assert_eq!(
                crate::dlls::ntdll::nt_create_key(
                    &mut native,
                    0,
                    attrs.as_ptr(),
                    0,
                    std::ptr::null(),
                    0,
                    &mut native_disposition
                ),
                0
            );
            assert_eq!(native_disposition, REG_OPENED_EXISTING_KEY);
            assert!(close_native_key(native));
            let child: Vec<u16> = "Software\\WinluxNativeCreated".encode_utf16().collect();
            std::ptr::write_unaligned(unicode.as_mut_ptr().cast::<u16>(), (child.len() * 2) as u16);
            std::ptr::write_unaligned(
                unicode.as_mut_ptr().add(2).cast::<u16>(),
                (child.len() * 2) as u16,
            );
            std::ptr::write_unaligned(
                unicode.as_mut_ptr().add(8).cast::<*const u16>(),
                child.as_ptr(),
            );
            std::ptr::write_unaligned(attrs.as_mut_ptr().add(8).cast::<usize>(), 0x8000_0001);
            assert_eq!(
                crate::dlls::ntdll::nt_create_key(
                    &mut native,
                    0,
                    attrs.as_ptr(),
                    0,
                    std::ptr::null(),
                    0,
                    &mut native_disposition
                ),
                0
            );
            assert_eq!(native_disposition, REG_CREATED_NEW_KEY);
            assert!(close_native_key(native));
            assert_eq!(
                reg_open_key_ex_a(
                    0x8000_0001,
                    c"Software\\WinluxNativeCreated".as_ptr().cast(),
                    0,
                    0,
                    &mut native
                ),
                0
            );
            assert_eq!(reg_close_key(native), 0);
            assert_ne!(
                crate::dlls::ntdll::nt_create_key(
                    &mut native,
                    0,
                    attrs.as_ptr(),
                    0,
                    std::ptr::null(),
                    1,
                    &mut native_disposition
                ),
                0
            );
            assert_eq!(open_native_key(0, "\\Registry\\Machine", 0, &mut native), 0);
            assert!(close_native_key(native));
            assert_eq!(
                open_native_key(0x8000_0001, "WinluxMissingNativeKey", 0, &mut native),
                ERROR_FILE_NOT_FOUND
            );
            assert_eq!(
                open_native_key(
                    0,
                    "\\Registry\\User\\UnknownUser\\Software\\WinluxTest",
                    0,
                    &mut native
                ),
                ERROR_FILE_NOT_FOUND
            );
            reg_close_key(h);
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("lock"));
        if let Some(v) = previous {
            std::env::set_var("WINLUX_REGISTRY_PATH", v);
        } else {
            std::env::remove_var("WINLUX_REGISTRY_PATH");
        }
    }
}
