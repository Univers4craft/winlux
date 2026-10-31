//! Fonctions registre de shlwapi (SHGetValue, SHRegGetValue, SHDeleteKey,
//! SHRegGetUSValue, SHOpenRegStream2…) et AssocQueryString, au-dessus du
//! registre partagé `crate::registry` et des fonctions Reg* d'advapi32.
//!
//! Comme sous Windows, les lectures « SH » développent les REG_EXPAND_SZ
//! (rendus en REG_SZ) et terminent toujours les chaînes par un zéro.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::dlls::advapi32 as api;
use crate::dlls::lock;
use crate::registry::{
    self as reg, Value, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_INVALID_HANDLE,
    ERROR_INVALID_PARAMETER, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS,
};

const ERROR_KEY_HAS_CHILDREN: i32 = 1020;
const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
/// HRESULT_FROM_WIN32(ERROR_NO_ASSOCIATION).
const E_NO_ASSOCIATION: i32 = 0x8007_0483u32 as i32;
const STG_E_INVALIDFUNCTION: i32 = 0x8003_0001u32 as i32;
const STG_E_ACCESSDENIED: i32 = 0x8003_0005u32 as i32;

// ── Outils ─────────────────────────────────────────────────────────

unsafe fn text_a(p: *const u8) -> String {
    if p.is_null() {
        String::new()
    } else {
        crate::dlls::cstr(p)
    }
}

unsafe fn text_w(p: *const u16) -> String {
    if p.is_null() {
        String::new()
    } else {
        crate::dlls::wstr(p)
    }
}

fn status<T>(r: Result<T, i32>) -> i32 {
    match r {
        Ok(_) => ERROR_SUCCESS,
        Err(e) => e,
    }
}

fn path_of(root: usize, sub: &str) -> Result<String, i32> {
    if sub.contains('\0') {
        return Err(ERROR_INVALID_PARAMETER);
    }
    Ok(reg::join(&reg::handle_path(root)?, sub))
}

/// Valeur lue façon shlwapi : REG_EXPAND_SZ développé et rendu en REG_SZ,
/// chaînes terminées par un zéro.
fn fetch(path: &str, name: &str) -> Result<Value, i32> {
    let mut v = reg::read(|d| {
        if !d.exists(path) {
            return Err(ERROR_FILE_NOT_FOUND);
        }
        d.get(path, name).cloned().ok_or(ERROR_FILE_NOT_FOUND)
    })?;
    if v.kind == reg::REG_EXPAND_SZ {
        v.kind = reg::REG_SZ;
        v.bytes = reg::utf16z(&reg::expand(&reg::string_of(&v.bytes)));
    } else if reg::is_string(v.kind) {
        let mut units = reg::utf16_units(&v.bytes);
        if units.last() != Some(&0) {
            units.push(0);
        }
        v.bytes = units.iter().flat_map(|u| u.to_le_bytes()).collect();
    }
    Ok(v)
}

/// Copie des données dans un tampon Win32 (`size` en octets).
unsafe fn put_data(bytes: &[u8], data: *mut u8, size: *mut u32) -> i32 {
    if size.is_null() {
        return if data.is_null() {
            ERROR_SUCCESS
        } else {
            ERROR_INVALID_PARAMETER
        };
    }
    let cap = *size as usize;
    *size = bytes.len() as u32;
    if data.is_null() {
        return ERROR_SUCCESS;
    }
    if cap < bytes.len() {
        return ERROR_MORE_DATA;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
    ERROR_SUCCESS
}

unsafe fn deliver(v: Value, ansi: bool, kind: *mut u32, data: *mut u8, size: *mut u32) -> i32 {
    if !kind.is_null() {
        *kind = v.kind;
    }
    let bytes = if ansi {
        reg::to_ansi(v.kind, &v.bytes)
    } else {
        v.bytes
    };
    put_data(&bytes, data, size)
}

/// Nom rendu dans un tampon « A » ou « W » (`size` en caractères).
unsafe fn put_name(s: &str, ansi: bool, buf: *mut c_void, size: *mut u32) -> i32 {
    if buf.is_null() || size.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    if ansi {
        let b = crate::dlls::encode_ansi(s);
        if (*size as usize) <= b.len() {
            *size = b.len() as u32 + 1;
            return ERROR_MORE_DATA;
        }
        let buf = buf.cast::<u8>();
        std::ptr::copy_nonoverlapping(b.as_ptr(), buf, b.len());
        *buf.add(b.len()) = 0;
        *size = b.len() as u32;
    } else {
        let w: Vec<u16> = s.encode_utf16().collect();
        if (*size as usize) <= w.len() {
            *size = w.len() as u32 + 1;
            return ERROR_MORE_DATA;
        }
        let buf = buf.cast::<u16>();
        std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
        *buf.add(w.len()) = 0;
        *size = w.len() as u32;
    }
    ERROR_SUCCESS
}

unsafe fn input(data: *const u8, size: u32, kind: u32, ansi: bool) -> Vec<u8> {
    let bytes = if data.is_null() || size == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(data, size as usize).to_vec()
    };
    if ansi {
        reg::from_ansi(kind, &bytes)
    } else {
        bytes
    }
}

fn store(path: &str, name: &str, kind: u32, bytes: Vec<u8>) -> i32 {
    status(reg::write(|d| {
        d.ensure(path);
        d.set(path, name, kind, bytes);
        Ok(())
    }))
}

fn remove_value(path: &str, name: &str) -> i32 {
    status(reg::write(|d| {
        if d.exists(path) && d.delete_value(path, name) {
            Ok(())
        } else {
            Err(ERROR_FILE_NOT_FOUND)
        }
    }))
}

// ── SHGetValue, SHSetValue, SHDeleteValue, SHQueryValueEx ──────────

unsafe fn get_impl(
    root: usize,
    sub: &str,
    name: &str,
    ansi: bool,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
) -> i32 {
    match path_of(root, sub).and_then(|p| fetch(&p, name)) {
        Ok(v) => deliver(v, ansi, kind, data, size),
        Err(e) => e,
    }
}

pub unsafe extern "win64" fn sh_get_value_a(
    root: usize,
    sub: *const u8,
    name: *const u8,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
) -> i32 {
    get_impl(root, &text_a(sub), &text_a(name), true, kind, data, size)
}
pub unsafe extern "win64" fn sh_get_value_w(
    root: usize,
    sub: *const u16,
    name: *const u16,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
) -> i32 {
    get_impl(root, &text_w(sub), &text_w(name), false, kind, data, size)
}

pub unsafe extern "win64" fn sh_query_value_ex_a(
    handle: usize,
    name: *const u8,
    _reserved: *mut u32,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
) -> i32 {
    get_impl(handle, "", &text_a(name), true, kind, data, size)
}
pub unsafe extern "win64" fn sh_query_value_ex_w(
    handle: usize,
    name: *const u16,
    _reserved: *mut u32,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
) -> i32 {
    get_impl(handle, "", &text_w(name), false, kind, data, size)
}

#[allow(clippy::too_many_arguments)]
unsafe fn set_impl(
    root: usize,
    sub: &str,
    name: &str,
    kind: u32,
    data: *const u8,
    size: u32,
    ansi: bool,
) -> i32 {
    match path_of(root, sub) {
        Ok(p) => store(&p, name, kind, input(data, size, kind, ansi)),
        Err(e) => e,
    }
}

pub unsafe extern "win64" fn sh_set_value_a(
    root: usize,
    sub: *const u8,
    name: *const u8,
    kind: u32,
    data: *const u8,
    size: u32,
) -> i32 {
    set_impl(root, &text_a(sub), &text_a(name), kind, data, size, true)
}
pub unsafe extern "win64" fn sh_set_value_w(
    root: usize,
    sub: *const u16,
    name: *const u16,
    kind: u32,
    data: *const u8,
    size: u32,
) -> i32 {
    set_impl(root, &text_w(sub), &text_w(name), kind, data, size, false)
}

pub unsafe extern "win64" fn sh_delete_value_a(
    root: usize,
    sub: *const u8,
    name: *const u8,
) -> i32 {
    match path_of(root, &text_a(sub)) {
        Ok(p) => remove_value(&p, &text_a(name)),
        Err(e) => e,
    }
}
pub unsafe extern "win64" fn sh_delete_value_w(
    root: usize,
    sub: *const u16,
    name: *const u16,
) -> i32 {
    match path_of(root, &text_w(sub)) {
        Ok(p) => remove_value(&p, &text_w(name)),
        Err(e) => e,
    }
}

// ── SHDeleteKey, SHDeleteEmptyKey, SHCopyKey ───────────────────────

fn delete_key(root: usize, sub: &str, only_empty: bool) -> i32 {
    let path = match path_of(root, sub) {
        Ok(p) => p,
        Err(e) => return e,
    };
    // Une ruche entière (HKCU, HKLM, HKCR…) ne se supprime pas.
    if !path.contains('\\') {
        return ERROR_ACCESS_DENIED;
    }
    status(reg::write(|d| {
        if !d.exists(&path) {
            return Err(ERROR_FILE_NOT_FOUND);
        }
        if only_empty && (d.has_subkeys(&path) || !d.values(&path).is_empty()) {
            return Err(ERROR_KEY_HAS_CHILDREN);
        }
        if d.delete_tree(&path) {
            Ok(())
        } else {
            Err(ERROR_ACCESS_DENIED)
        }
    }))
}

pub unsafe extern "win64" fn sh_delete_key_a(root: usize, sub: *const u8) -> i32 {
    delete_key(root, &text_a(sub), false)
}
pub unsafe extern "win64" fn sh_delete_key_w(root: usize, sub: *const u16) -> i32 {
    delete_key(root, &text_w(sub), false)
}
pub unsafe extern "win64" fn sh_delete_empty_key_a(root: usize, sub: *const u8) -> i32 {
    delete_key(root, &text_a(sub), true)
}
pub unsafe extern "win64" fn sh_delete_empty_key_w(root: usize, sub: *const u16) -> i32 {
    delete_key(root, &text_w(sub), true)
}

pub unsafe extern "win64" fn sh_copy_key_a(
    src: usize,
    sub: *const u8,
    dest: usize,
    _reserved: u32,
) -> i32 {
    api::reg_copy_tree_a(src, sub, dest)
}
pub unsafe extern "win64" fn sh_copy_key_w(
    src: usize,
    sub: *const u16,
    dest: usize,
    _reserved: u32,
) -> i32 {
    api::reg_copy_tree_w(src, sub, dest)
}

// ── Énumération ────────────────────────────────────────────────────

pub unsafe extern "win64" fn sh_enum_key_ex_a(
    handle: usize,
    index: u32,
    name: *mut u8,
    size: *mut u32,
) -> i32 {
    let n = std::ptr::null_mut();
    api::reg_enum_key_ex_a(
        handle,
        index,
        name,
        size,
        n,
        std::ptr::null_mut(),
        n,
        n.cast(),
    )
}
pub unsafe extern "win64" fn sh_enum_key_ex_w(
    handle: usize,
    index: u32,
    name: *mut u16,
    size: *mut u32,
) -> i32 {
    let n = std::ptr::null_mut();
    api::reg_enum_key_ex_w(
        handle,
        index,
        name,
        size,
        n,
        std::ptr::null_mut(),
        n,
        n.cast(),
    )
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sh_enum_value_a(
    handle: usize,
    index: u32,
    name: *mut u8,
    name_size: *mut u32,
    kind: *mut u32,
    data: *mut u8,
    data_size: *mut u32,
) -> i32 {
    let mut cap = 16384u32;
    let (buf, size) = if name.is_null() {
        (vec![0u8; 16384], &mut cap as *mut u32)
    } else {
        (Vec::new(), name_size)
    };
    let target = if name.is_null() {
        buf.as_ptr().cast_mut()
    } else {
        name
    };
    api::reg_enum_value_a(
        handle,
        index,
        target,
        size,
        std::ptr::null_mut(),
        kind,
        data,
        data_size,
    )
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sh_enum_value_w(
    handle: usize,
    index: u32,
    name: *mut u16,
    name_size: *mut u32,
    kind: *mut u32,
    data: *mut u8,
    data_size: *mut u32,
) -> i32 {
    let mut cap = 16384u32;
    let (buf, size) = if name.is_null() {
        (vec![0u16; 16384], &mut cap as *mut u32)
    } else {
        (Vec::new(), name_size)
    };
    let target = if name.is_null() {
        buf.as_ptr().cast_mut()
    } else {
        name
    };
    api::reg_enum_value_w(
        handle,
        index,
        target,
        size,
        std::ptr::null_mut(),
        kind,
        data,
        data_size,
    )
}

unsafe fn query_info(
    handle: usize,
    sub_keys: *mut u32,
    max_sub_key: *mut u32,
    values: *mut u32,
    max_value_name: *mut u32,
) -> i32 {
    let n: *mut u32 = std::ptr::null_mut();
    api::reg_query_info_key_w(
        handle,
        n.cast(),
        n,
        n,
        sub_keys,
        max_sub_key,
        n,
        values,
        max_value_name,
        n,
        n,
        n.cast(),
    )
}
pub unsafe extern "win64" fn sh_query_info_key_a(
    handle: usize,
    sub_keys: *mut u32,
    max_sub_key: *mut u32,
    values: *mut u32,
    max_value_name: *mut u32,
) -> i32 {
    query_info(handle, sub_keys, max_sub_key, values, max_value_name)
}
pub unsafe extern "win64" fn sh_query_info_key_w(
    handle: usize,
    sub_keys: *mut u32,
    max_sub_key: *mut u32,
    values: *mut u32,
    max_value_name: *mut u32,
) -> i32 {
    query_info(handle, sub_keys, max_sub_key, values, max_value_name)
}

// ── SHRegGetValue, SHRegGetPath, SHRegSetPath, SHRegDuplicateHKey ──

/// Les drapeaux SRRF_RM_* (mode de démarrage) n'ont pas d'équivalent RRF.
const SRRF_RM_MASK: u32 = 0x000F_0000;

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sh_reg_get_value_a(
    root: usize,
    sub: *const u8,
    name: *const u8,
    flags: u32,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
) -> i32 {
    api::reg_get_value_a(root, sub, name, flags & !SRRF_RM_MASK, kind, data, size)
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sh_reg_get_value_w(
    root: usize,
    sub: *const u16,
    name: *const u16,
    flags: u32,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
) -> i32 {
    api::reg_get_value_w(root, sub, name, flags & !SRRF_RM_MASK, kind, data, size)
}

const MAX_PATH: usize = 260;

fn path_value(root: usize, sub: &str, name: &str) -> Result<String, i32> {
    let v = fetch(&path_of(root, sub)?, name)?;
    if !reg::is_string(v.kind) {
        return Err(ERROR_FILE_NOT_FOUND);
    }
    Ok(reg::string_of(&v.bytes))
}

pub unsafe extern "win64" fn sh_reg_get_path_a(
    root: usize,
    sub: *const u8,
    name: *const u8,
    out: *mut u8,
    _flags: u32,
) -> i32 {
    if out.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    match path_value(root, &text_a(sub), &text_a(name)) {
        Ok(text) => {
            crate::dlls::put_a(&text, out, MAX_PATH as u32);
            ERROR_SUCCESS
        }
        Err(e) => e,
    }
}
pub unsafe extern "win64" fn sh_reg_get_path_w(
    root: usize,
    sub: *const u16,
    name: *const u16,
    out: *mut u16,
    _flags: u32,
) -> i32 {
    if out.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    match path_value(root, &text_w(sub), &text_w(name)) {
        Ok(text) => {
            crate::dlls::put_w(&text, out, MAX_PATH as u32);
            ERROR_SUCCESS
        }
        Err(e) => e,
    }
}

fn set_path(root: usize, sub: &str, name: &str, value: &str) -> i32 {
    match path_of(root, sub) {
        Ok(p) => store(&p, name, reg::REG_SZ, reg::utf16z(value)),
        Err(e) => e,
    }
}
pub unsafe extern "win64" fn sh_reg_set_path_a(
    root: usize,
    sub: *const u8,
    name: *const u8,
    value: *const u8,
    _flags: u32,
) -> i32 {
    set_path(root, &text_a(sub), &text_a(name), &text_a(value))
}
pub unsafe extern "win64" fn sh_reg_set_path_w(
    root: usize,
    sub: *const u16,
    name: *const u16,
    value: *const u16,
    _flags: u32,
) -> i32 {
    set_path(root, &text_w(sub), &text_w(name), &text_w(value))
}

pub extern "win64" fn sh_reg_duplicate_hkey(handle: usize) -> usize {
    if reg::predefined(handle).is_some() {
        return handle;
    }
    reg::handle_path(handle)
        .and_then(|p| reg::open(&p))
        .unwrap_or(0)
}

// ── Valeurs « utilisateur puis machine » (SHRegGetUSValue…) ────────

const SHREGSET_HKCU: u32 = 0x1;
const SHREGSET_FORCE_HKCU: u32 = 0x2;
const SHREGSET_HKLM: u32 = 0x4;
const SHREGSET_FORCE_HKLM: u32 = 0x8;
const SHREGSET_DEFAULT: u32 = SHREGSET_FORCE_HKCU | SHREGSET_HKLM;
const SHREG_HKCU: u32 = 0x1;
const SHREG_HKLM: u32 = 0x10;

/// HKCU d'abord (sauf `ignore_hkcu`), puis HKLM.
fn us_lookup(sub: &str, name: &str, ignore_hkcu: bool) -> Result<Value, i32> {
    if !ignore_hkcu {
        if let Ok(v) = fetch(&reg::join("hkcu", sub), name) {
            return Ok(v);
        }
    }
    fetch(&reg::join("hklm", sub), name)
}

#[allow(clippy::too_many_arguments)]
unsafe fn us_get(
    sub: &str,
    name: &str,
    ansi: bool,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
    ignore_hkcu: bool,
    default: *const u8,
    default_size: u32,
) -> i32 {
    match us_lookup(sub, name, ignore_hkcu) {
        Ok(v) => deliver(v, ansi, kind, data, size),
        Err(e) if default.is_null() || default_size == 0 => e,
        Err(_) => {
            // Valeur absente : la valeur par défaut fournie est rendue.
            let fallback = std::slice::from_raw_parts(default, default_size as usize);
            put_data(fallback, data, size)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sh_reg_get_us_value_a(
    sub: *const u8,
    name: *const u8,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
    ignore_hkcu: i32,
    default: *const u8,
    default_size: u32,
) -> i32 {
    us_get(
        &text_a(sub),
        &text_a(name),
        true,
        kind,
        data,
        size,
        ignore_hkcu != 0,
        default,
        default_size,
    )
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sh_reg_get_us_value_w(
    sub: *const u16,
    name: *const u16,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
    ignore_hkcu: i32,
    default: *const u8,
    default_size: u32,
) -> i32 {
    us_get(
        &text_w(sub),
        &text_w(name),
        false,
        kind,
        data,
        size,
        ignore_hkcu != 0,
        default,
        default_size,
    )
}

/// « yes »/« true »/« 1 » ou un nombre non nul → vrai.
fn as_bool(v: &Value) -> Option<bool> {
    if reg::is_string(v.kind) {
        return match reg::string_of(&v.bytes)
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "yes" | "true" | "1" => Some(true),
            "no" | "false" | "0" => Some(false),
            _ => None,
        };
    }
    if v.bytes.is_empty() {
        return None;
    }
    Some(v.bytes.iter().any(|&b| b != 0))
}

fn us_bool(sub: &str, name: &str, ignore_hkcu: bool, default: i32) -> i32 {
    us_lookup(sub, name, ignore_hkcu)
        .ok()
        .and_then(|v| as_bool(&v))
        .map_or((default != 0) as i32, i32::from)
}
pub unsafe extern "win64" fn sh_reg_get_bool_us_value_a(
    sub: *const u8,
    name: *const u8,
    ignore_hkcu: i32,
    default: i32,
) -> i32 {
    us_bool(&text_a(sub), &text_a(name), ignore_hkcu != 0, default)
}
pub unsafe extern "win64" fn sh_reg_get_bool_us_value_w(
    sub: *const u16,
    name: *const u16,
    ignore_hkcu: i32,
    default: i32,
) -> i32 {
    us_bool(&text_w(sub), &text_w(name), ignore_hkcu != 0, default)
}

/// Écrit dans HKCU et/ou HKLM ; sans « FORCE », seulement si la valeur
/// n'existe pas encore.
fn us_set(sub: &str, name: &str, kind: u32, bytes: Vec<u8>, flags: u32) -> i32 {
    let flags = if flags == 0 { SHREGSET_DEFAULT } else { flags };
    let mut result = ERROR_INVALID_PARAMETER;
    for (root, soft, force) in [
        ("hkcu", SHREGSET_HKCU, SHREGSET_FORCE_HKCU),
        ("hklm", SHREGSET_HKLM, SHREGSET_FORCE_HKLM),
    ] {
        if flags & (soft | force) == 0 {
            continue;
        }
        let path = reg::join(root, sub);
        let r = status(reg::write(|d| {
            if flags & force == 0 && d.get(&path, name).is_some() {
                return Ok(());
            }
            d.ensure(&path);
            d.set(&path, name, kind, bytes.clone());
            Ok(())
        }));
        if result != ERROR_SUCCESS {
            result = r;
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sh_reg_set_us_value_a(
    sub: *const u8,
    name: *const u8,
    kind: u32,
    data: *const u8,
    size: u32,
    flags: u32,
) -> i32 {
    us_set(
        &text_a(sub),
        &text_a(name),
        kind,
        input(data, size, kind, true),
        flags,
    )
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sh_reg_set_us_value_w(
    sub: *const u16,
    name: *const u16,
    kind: u32,
    data: *const u8,
    size: u32,
    flags: u32,
) -> i32 {
    us_set(
        &text_w(sub),
        &text_w(name),
        kind,
        input(data, size, kind, false),
        flags,
    )
}

// ── Clés « US » (HUSKEY) ───────────────────────────────────────────

struct UsKey {
    sub: String,
    ignore_hkcu: bool,
}

static US_KEYS: Mutex<Option<HashMap<usize, UsKey>>> = Mutex::new(None);
static NEXT_US_KEY: AtomicUsize = AtomicUsize::new(0x5553_0000);

fn us_key(handle: usize) -> Result<(String, bool), i32> {
    lock(&US_KEYS)
        .as_ref()
        .and_then(|m| m.get(&handle))
        .map(|k| (k.sub.clone(), k.ignore_hkcu))
        .ok_or(ERROR_INVALID_HANDLE)
}

/// Sous-clé relative jointe à une autre (sans racine).
fn sub_join(base: &str, sub: &str) -> String {
    let parts: Vec<String> = format!("{base}\\{sub}")
        .replace('/', "\\")
        .split('\\')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    parts.join("\\")
}

unsafe fn us_open(
    path: &str,
    relative: usize,
    out: *mut usize,
    ignore_hkcu: bool,
    create: Option<u32>,
) -> i32 {
    if out.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let (sub, ignore_hkcu) = if relative != 0 {
        match us_key(relative) {
            Ok((base, ignore)) => (sub_join(&base, path), ignore || ignore_hkcu),
            Err(e) => return e,
        }
    } else {
        (sub_join("", path), ignore_hkcu)
    };
    if let Some(flags) = create {
        let flags = if flags == 0 { SHREGSET_HKCU } else { flags };
        let r = status(reg::write(|d| {
            if flags & (SHREGSET_HKCU | SHREGSET_FORCE_HKCU) != 0 {
                d.ensure(&reg::join("hkcu", &sub));
            }
            if flags & (SHREGSET_HKLM | SHREGSET_FORCE_HKLM) != 0 {
                d.ensure(&reg::join("hklm", &sub));
            }
            Ok(())
        }));
        if r != ERROR_SUCCESS {
            return r;
        }
    } else {
        let found = reg::read(|d| {
            Ok((!ignore_hkcu && d.exists(&reg::join("hkcu", &sub)))
                || d.exists(&reg::join("hklm", &sub)))
        })
        .unwrap_or(false);
        if !found {
            return ERROR_FILE_NOT_FOUND;
        }
    }
    let handle = NEXT_US_KEY.fetch_add(4, Ordering::Relaxed);
    lock(&US_KEYS)
        .get_or_insert_with(HashMap::new)
        .insert(handle, UsKey { sub, ignore_hkcu });
    *out = handle;
    ERROR_SUCCESS
}

pub unsafe extern "win64" fn sh_reg_open_us_key_a(
    path: *const u8,
    _access: u32,
    relative: usize,
    out: *mut usize,
    ignore_hkcu: i32,
) -> i32 {
    us_open(&text_a(path), relative, out, ignore_hkcu != 0, None)
}
pub unsafe extern "win64" fn sh_reg_open_us_key_w(
    path: *const u16,
    _access: u32,
    relative: usize,
    out: *mut usize,
    ignore_hkcu: i32,
) -> i32 {
    us_open(&text_w(path), relative, out, ignore_hkcu != 0, None)
}
pub unsafe extern "win64" fn sh_reg_create_us_key_a(
    path: *const u8,
    _access: u32,
    relative: usize,
    out: *mut usize,
    flags: u32,
) -> i32 {
    us_open(&text_a(path), relative, out, false, Some(flags))
}
pub unsafe extern "win64" fn sh_reg_create_us_key_w(
    path: *const u16,
    _access: u32,
    relative: usize,
    out: *mut usize,
    flags: u32,
) -> i32 {
    us_open(&text_w(path), relative, out, false, Some(flags))
}

pub extern "win64" fn sh_reg_close_us_key(handle: usize) -> i32 {
    match lock(&US_KEYS).as_mut().and_then(|m| m.remove(&handle)) {
        Some(_) => ERROR_SUCCESS,
        None => ERROR_INVALID_PARAMETER,
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn us_query(
    handle: usize,
    name: &str,
    ansi: bool,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
    ignore_hkcu: bool,
    default: *const u8,
    default_size: u32,
) -> i32 {
    match us_key(handle) {
        Ok((sub, ignore)) => us_get(
            &sub,
            name,
            ansi,
            kind,
            data,
            size,
            ignore || ignore_hkcu,
            default,
            default_size,
        ),
        Err(e) => e,
    }
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sh_reg_query_us_value_a(
    handle: usize,
    name: *const u8,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
    ignore_hkcu: i32,
    default: *const u8,
    default_size: u32,
) -> i32 {
    us_query(
        handle,
        &text_a(name),
        true,
        kind,
        data,
        size,
        ignore_hkcu != 0,
        default,
        default_size,
    )
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sh_reg_query_us_value_w(
    handle: usize,
    name: *const u16,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
    ignore_hkcu: i32,
    default: *const u8,
    default_size: u32,
) -> i32 {
    us_query(
        handle,
        &text_w(name),
        false,
        kind,
        data,
        size,
        ignore_hkcu != 0,
        default,
        default_size,
    )
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sh_reg_write_us_value_a(
    handle: usize,
    name: *const u8,
    kind: u32,
    data: *const u8,
    size: u32,
    flags: u32,
) -> i32 {
    match us_key(handle) {
        Ok((sub, _)) => us_set(
            &sub,
            &text_a(name),
            kind,
            input(data, size, kind, true),
            flags,
        ),
        Err(e) => e,
    }
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sh_reg_write_us_value_w(
    handle: usize,
    name: *const u16,
    kind: u32,
    data: *const u8,
    size: u32,
    flags: u32,
) -> i32 {
    match us_key(handle) {
        Ok((sub, _)) => us_set(
            &sub,
            &text_w(name),
            kind,
            input(data, size, kind, false),
            flags,
        ),
        Err(e) => e,
    }
}

/// SHREGDEL_DEFAULT : HKCU s'il y a la valeur, sinon HKLM.
fn us_delete(handle: usize, name: &str, flags: u32) -> i32 {
    let sub = match us_key(handle) {
        Ok((s, _)) => s,
        Err(e) => return e,
    };
    let user = reg::join("hkcu", &sub);
    let machine = reg::join("hklm", &sub);
    match flags & (SHREG_HKCU | SHREG_HKLM) {
        SHREG_HKCU => remove_value(&user, name),
        SHREG_HKLM => remove_value(&machine, name),
        f if f == SHREG_HKCU | SHREG_HKLM => {
            let a = remove_value(&user, name);
            let b = remove_value(&machine, name);
            if a == ERROR_SUCCESS {
                a
            } else {
                b
            }
        }
        _ => match remove_value(&user, name) {
            ERROR_SUCCESS => ERROR_SUCCESS,
            _ => remove_value(&machine, name),
        },
    }
}
pub unsafe extern "win64" fn sh_reg_delete_us_value_a(
    handle: usize,
    name: *const u8,
    flags: u32,
) -> i32 {
    us_delete(handle, &text_a(name), flags)
}
pub unsafe extern "win64" fn sh_reg_delete_us_value_w(
    handle: usize,
    name: *const u16,
    flags: u32,
) -> i32 {
    us_delete(handle, &text_w(name), flags)
}

/// Chemins à énumérer selon SHREGENUM_* (par défaut : HKCU s'il existe,
/// sinon HKLM ; les deux fusionnés pour SHREGENUM_BOTH).
fn enum_paths(sub: &str, ignore_hkcu: bool, flags: u32) -> Vec<String> {
    let user = reg::join("hkcu", sub);
    let machine = reg::join("hklm", sub);
    match flags & (SHREG_HKCU | SHREG_HKLM) {
        SHREG_HKCU => vec![user],
        SHREG_HKLM => vec![machine],
        f if f == SHREG_HKCU | SHREG_HKLM => vec![user, machine],
        _ => {
            let user_exists = reg::read(|d| Ok(d.exists(&user))).unwrap_or(false);
            if user_exists && !ignore_hkcu {
                vec![user]
            } else {
                vec![machine]
            }
        }
    }
}

fn merged_subkeys(paths: &[String]) -> Vec<String> {
    reg::read(|d| {
        let mut all: Vec<String> = Vec::new();
        for p in paths {
            for name in d.subkeys(p) {
                if !all.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                    all.push(name);
                }
            }
        }
        Ok(all)
    })
    .unwrap_or_default()
}

fn merged_values(paths: &[String]) -> Vec<Value> {
    reg::read(|d| {
        let mut all: Vec<Value> = Vec::new();
        for p in paths {
            for v in d.values(p) {
                if !all.iter().any(|u| u.name.eq_ignore_ascii_case(&v.name)) {
                    all.push(v);
                }
            }
        }
        Ok(all)
    })
    .unwrap_or_default()
}

unsafe fn us_enum_key(
    handle: usize,
    index: u32,
    ansi: bool,
    name: *mut c_void,
    size: *mut u32,
    flags: u32,
) -> i32 {
    let (sub, ignore) = match us_key(handle) {
        Ok(k) => k,
        Err(e) => return e,
    };
    match merged_subkeys(&enum_paths(&sub, ignore, flags)).get(index as usize) {
        Some(n) => put_name(n, ansi, name, size),
        None => ERROR_NO_MORE_ITEMS,
    }
}
pub unsafe extern "win64" fn sh_reg_enum_us_key_a(
    handle: usize,
    index: u32,
    name: *mut u8,
    size: *mut u32,
    flags: u32,
) -> i32 {
    us_enum_key(handle, index, true, name.cast(), size, flags)
}
pub unsafe extern "win64" fn sh_reg_enum_us_key_w(
    handle: usize,
    index: u32,
    name: *mut u16,
    size: *mut u32,
    flags: u32,
) -> i32 {
    us_enum_key(handle, index, false, name.cast(), size, flags)
}

#[allow(clippy::too_many_arguments)]
unsafe fn us_enum_value(
    handle: usize,
    index: u32,
    ansi: bool,
    name: *mut c_void,
    name_size: *mut u32,
    kind: *mut u32,
    data: *mut u8,
    data_size: *mut u32,
    flags: u32,
) -> i32 {
    let (sub, ignore) = match us_key(handle) {
        Ok(k) => k,
        Err(e) => return e,
    };
    let Some(v) = merged_values(&enum_paths(&sub, ignore, flags))
        .into_iter()
        .nth(index as usize)
    else {
        return ERROR_NO_MORE_ITEMS;
    };
    if !name.is_null() {
        let r = put_name(&v.name, ansi, name, name_size);
        if r != ERROR_SUCCESS {
            return r;
        }
    }
    deliver(v, ansi, kind, data, data_size)
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sh_reg_enum_us_value_a(
    handle: usize,
    index: u32,
    name: *mut u8,
    name_size: *mut u32,
    kind: *mut u32,
    data: *mut u8,
    data_size: *mut u32,
    flags: u32,
) -> i32 {
    us_enum_value(
        handle,
        index,
        true,
        name.cast(),
        name_size,
        kind,
        data,
        data_size,
        flags,
    )
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sh_reg_enum_us_value_w(
    handle: usize,
    index: u32,
    name: *mut u16,
    name_size: *mut u32,
    kind: *mut u32,
    data: *mut u8,
    data_size: *mut u32,
    flags: u32,
) -> i32 {
    us_enum_value(
        handle,
        index,
        false,
        name.cast(),
        name_size,
        kind,
        data,
        data_size,
        flags,
    )
}

pub unsafe extern "win64" fn sh_reg_query_info_us_key(
    handle: usize,
    sub_keys: *mut u32,
    max_sub_key: *mut u32,
    values: *mut u32,
    max_value_name: *mut u32,
    flags: u32,
) -> i32 {
    let (sub, ignore) = match us_key(handle) {
        Ok(k) => k,
        Err(e) => return e,
    };
    let paths = enum_paths(&sub, ignore, flags);
    let keys = merged_subkeys(&paths);
    let vals = merged_values(&paths);
    let longest_key = keys.iter().map(|k| k.encode_utf16().count()).max();
    let longest_value = vals.iter().map(|v| v.name.encode_utf16().count()).max();
    for (p, v) in [
        (sub_keys, keys.len() as u32),
        (max_sub_key, longest_key.unwrap_or(0) as u32),
        (values, vals.len() as u32),
        (max_value_name, longest_value.unwrap_or(0) as u32),
    ] {
        if !p.is_null() {
            *p = v;
        }
    }
    ERROR_SUCCESS
}

// ── SHOpenRegStream2 : un IStream sur une valeur binaire ───────────

#[repr(C)]
struct StreamVtbl {
    query_interface:
        unsafe extern "win64" fn(*mut RegStream, *const [u8; 16], *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut RegStream) -> u32,
    release: unsafe extern "win64" fn(*mut RegStream) -> u32,
    read: unsafe extern "win64" fn(*mut RegStream, *mut u8, u32, *mut u32) -> i32,
    write: unsafe extern "win64" fn(*mut RegStream, *const u8, u32, *mut u32) -> i32,
    seek: unsafe extern "win64" fn(*mut RegStream, i64, u32, *mut u64) -> i32,
    set_size: unsafe extern "win64" fn(*mut RegStream, u64) -> i32,
    copy_to: unsafe extern "win64" fn(*mut RegStream, *mut c_void, u64, *mut u64, *mut u64) -> i32,
    commit: unsafe extern "win64" fn(*mut RegStream, u32) -> i32,
    revert: unsafe extern "win64" fn(*mut RegStream) -> i32,
    lock_region: unsafe extern "win64" fn(*mut RegStream, u64, u64, u32) -> i32,
    unlock_region: unsafe extern "win64" fn(*mut RegStream, u64, u64, u32) -> i32,
    stat: unsafe extern "win64" fn(*mut RegStream, *mut u8, u32) -> i32,
    clone: unsafe extern "win64" fn(*mut RegStream, *mut *mut c_void) -> i32,
}

struct StreamState {
    data: Vec<u8>,
    pos: usize,
    dirty: bool,
    /// Clé et valeur où réécrire les données ; `None` : flux en lecture seule.
    target: Option<(String, String)>,
}

#[repr(C)]
struct RegStream {
    vtbl: *const StreamVtbl,
    refs: AtomicU32,
    state: Mutex<StreamState>,
}

const IID_IUNKNOWN: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
const IID_ISTREAM: [u8; 16] = [0x0C, 0, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
/// {0C733A30-2A1C-11CE-ADE5-00AA0044773D}
const IID_ISEQUENTIALSTREAM: [u8; 16] = [
    0x30, 0x3A, 0x73, 0x0C, 0x1C, 0x2A, 0xCE, 0x11, 0xAD, 0xE5, 0x00, 0xAA, 0x00, 0x44, 0x77, 0x3D,
];

fn flush(state: &mut StreamState) -> i32 {
    if !state.dirty {
        return S_OK;
    }
    let Some((key, name)) = &state.target else {
        return STG_E_ACCESSDENIED;
    };
    match store(key, name, reg::REG_BINARY, state.data.clone()) {
        ERROR_SUCCESS => {
            state.dirty = false;
            S_OK
        }
        _ => STG_E_ACCESSDENIED,
    }
}

unsafe extern "win64" fn rs_query_interface(
    this: *mut RegStream,
    iid: *const [u8; 16],
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || iid.is_null() {
        return E_POINTER;
    }
    if [IID_IUNKNOWN, IID_ISTREAM, IID_ISEQUENTIALSTREAM].contains(&*iid) {
        rs_add_ref(this);
        *out = this.cast();
        S_OK
    } else {
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
}
unsafe extern "win64" fn rs_add_ref(this: *mut RegStream) -> u32 {
    (*this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn rs_release(this: *mut RegStream) -> u32 {
    let left = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if left == 0 {
        let stream = Box::from_raw(this);
        flush(&mut lock(&stream.state));
    }
    left
}
unsafe extern "win64" fn rs_read(
    this: *mut RegStream,
    buf: *mut u8,
    cb: u32,
    read: *mut u32,
) -> i32 {
    let mut s = lock(&(*this).state);
    let pos = s.pos.min(s.data.len());
    let n = (cb as usize).min(s.data.len() - pos);
    if n > 0 {
        if buf.is_null() {
            return E_POINTER;
        }
        std::ptr::copy_nonoverlapping(s.data[pos..].as_ptr(), buf, n);
    }
    s.pos = pos + n;
    if !read.is_null() {
        *read = n as u32;
    }
    S_OK
}
unsafe extern "win64" fn rs_write(
    this: *mut RegStream,
    buf: *const u8,
    cb: u32,
    written: *mut u32,
) -> i32 {
    let mut s = lock(&(*this).state);
    if s.target.is_none() {
        return STG_E_ACCESSDENIED;
    }
    if cb > 0 && buf.is_null() {
        return E_POINTER;
    }
    let pos = s.pos;
    let end = pos + cb as usize;
    if s.data.len() < end {
        s.data.resize(end, 0);
    }
    if cb > 0 {
        std::ptr::copy_nonoverlapping(buf, s.data[pos..].as_mut_ptr(), cb as usize);
    }
    s.pos = end;
    s.dirty = true;
    if !written.is_null() {
        *written = cb;
    }
    S_OK
}
unsafe extern "win64" fn rs_seek(
    this: *mut RegStream,
    delta: i64,
    origin: u32,
    out: *mut u64,
) -> i32 {
    let mut s = lock(&(*this).state);
    let base = match origin {
        0 => 0,
        1 => s.pos as i64,
        2 => s.data.len() as i64,
        _ => return STG_E_INVALIDFUNCTION,
    };
    let Some(pos) = base.checked_add(delta).filter(|p| *p >= 0) else {
        return STG_E_INVALIDFUNCTION;
    };
    s.pos = pos as usize;
    if !out.is_null() {
        *out = pos as u64;
    }
    S_OK
}
unsafe extern "win64" fn rs_set_size(this: *mut RegStream, size: u64) -> i32 {
    let mut s = lock(&(*this).state);
    if s.target.is_none() {
        return STG_E_ACCESSDENIED;
    }
    s.data.resize(size as usize, 0);
    s.dirty = true;
    S_OK
}
unsafe extern "win64" fn rs_copy_to(
    this: *mut RegStream,
    dest: *mut c_void,
    cb: u64,
    read: *mut u64,
    written: *mut u64,
) -> i32 {
    if dest.is_null() {
        return E_POINTER;
    }
    let chunk = {
        let mut s = lock(&(*this).state);
        let pos = s.pos.min(s.data.len());
        let n = (cb as usize).min(s.data.len() - pos);
        s.pos = pos + n;
        s.data[pos..pos + n].to_vec()
    };
    // ISequentialStream::Write est la cinquième entrée de toute table IStream.
    type WriteFn = unsafe extern "win64" fn(*mut c_void, *const u8, u32, *mut u32) -> i32;
    let vtbl = *(dest as *const *const usize);
    let write: WriteFn = std::mem::transmute(*vtbl.add(4));
    let mut done = 0u32;
    let hr = write(dest, chunk.as_ptr(), chunk.len() as u32, &mut done);
    if !read.is_null() {
        *read = chunk.len() as u64;
    }
    if !written.is_null() {
        *written = done as u64;
    }
    hr
}
unsafe extern "win64" fn rs_commit(this: *mut RegStream, _flags: u32) -> i32 {
    flush(&mut lock(&(*this).state))
}
unsafe extern "win64" fn rs_revert(_this: *mut RegStream) -> i32 {
    S_OK
}
unsafe extern "win64" fn rs_lock_region(_this: *mut RegStream, _o: u64, _n: u64, _t: u32) -> i32 {
    STG_E_INVALIDFUNCTION
}
unsafe extern "win64" fn rs_stat(this: *mut RegStream, out: *mut u8, _flags: u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let s = lock(&(*this).state);
    // STATSTG : pwcsName (8), type (4 + 4), cbSize (8), trois FILETIME,
    // grfMode, grfLocksSupported, clsid, grfStateBits, réservé = 80 octets.
    std::ptr::write_bytes(out, 0, 80);
    (out.add(8) as *mut u32).write_unaligned(2); // STGTY_STREAM
    (out.add(16) as *mut u64).write_unaligned(s.data.len() as u64);
    (out.add(48) as *mut u32).write_unaligned(if s.target.is_some() { 2 } else { 0 });
    S_OK
}
unsafe extern "win64" fn rs_clone(_this: *mut RegStream, out: *mut *mut c_void) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

static STREAM_VTBL: StreamVtbl = StreamVtbl {
    query_interface: rs_query_interface,
    add_ref: rs_add_ref,
    release: rs_release,
    read: rs_read,
    write: rs_write,
    seek: rs_seek,
    set_size: rs_set_size,
    copy_to: rs_copy_to,
    commit: rs_commit,
    revert: rs_revert,
    lock_region: rs_lock_region,
    unlock_region: rs_lock_region,
    stat: rs_stat,
    clone: rs_clone,
};

fn new_stream(data: Vec<u8>, target: Option<(String, String)>) -> *mut c_void {
    Box::into_raw(Box::new(RegStream {
        vtbl: &STREAM_VTBL,
        refs: AtomicU32::new(1),
        state: Mutex::new(StreamState {
            data,
            pos: 0,
            dirty: false,
            target,
        }),
    }))
    .cast()
}

const STGM_WRITE: u32 = 0x1;
const STGM_READWRITE: u32 = 0x2;

/// Flux sur `clé\sous-clé` → valeur ; `None` si la valeur manque en lecture.
fn open_stream(root: usize, sub: &str, name: &str, mode: u32) -> Option<*mut c_void> {
    let path = path_of(root, sub).ok()?;
    let writable = mode & (STGM_WRITE | STGM_READWRITE) != 0;
    let current = reg::read(|d| Ok(d.get(&path, name).map(|v| v.bytes.clone())))
        .ok()
        .flatten();
    match (current, writable) {
        (None, false) => None,
        (data, true) => Some(new_stream(
            data.unwrap_or_default(),
            Some((path, name.to_string())),
        )),
        (Some(data), false) => Some(new_stream(data, None)),
    }
}

pub unsafe extern "win64" fn sh_open_reg_stream2_a(
    root: usize,
    sub: *const u8,
    name: *const u8,
    mode: u32,
) -> *mut c_void {
    open_stream(root, &text_a(sub), &text_a(name), mode).unwrap_or(std::ptr::null_mut())
}
pub unsafe extern "win64" fn sh_open_reg_stream2_w(
    root: usize,
    sub: *const u16,
    name: *const u16,
    mode: u32,
) -> *mut c_void {
    open_stream(root, &text_w(sub), &text_w(name), mode).unwrap_or(std::ptr::null_mut())
}
/// SHOpenRegStream (ancienne version) : un flux vide plutôt que NULL.
pub unsafe extern "win64" fn sh_open_reg_stream_a(
    root: usize,
    sub: *const u8,
    name: *const u8,
    mode: u32,
) -> *mut c_void {
    open_stream(root, &text_a(sub), &text_a(name), mode)
        .unwrap_or_else(|| new_stream(Vec::new(), None))
}
pub unsafe extern "win64" fn sh_open_reg_stream_w(
    root: usize,
    sub: *const u16,
    name: *const u16,
    mode: u32,
) -> *mut c_void {
    open_stream(root, &text_w(sub), &text_w(name), mode)
        .unwrap_or_else(|| new_stream(Vec::new(), None))
}

// ── AssocQueryString / AssocQueryKey ───────────────────────────────

const ASSOCSTR_COMMAND: u32 = 1;
const ASSOCSTR_EXECUTABLE: u32 = 2;
const ASSOCSTR_FRIENDLYDOCNAME: u32 = 3;
const ASSOCSTR_FRIENDLYAPPNAME: u32 = 4;
const ASSOCSTR_CONTENTTYPE: u32 = 14;
const ASSOCSTR_DEFAULTICON: u32 = 15;
const ASSOCSTR_SHELLEXTENSION: u32 = 16;
const ASSOCSTR_PROGID: u32 = 20;
const ASSOCKEY_SHELLEXECCLASS: u32 = 1;
const ASSOCKEY_APP: u32 = 2;
const ASSOCKEY_CLASS: u32 = 3;

fn hkcr(sub: &str) -> String {
    reg::join(reg::HKCR, sub)
}

fn text_value(path: &str, name: &str) -> Option<String> {
    fetch(path, name)
        .ok()
        .filter(|v| reg::is_string(v.kind))
        .map(|v| reg::string_of(&v.bytes))
}

/// `.txt` → identifiant de classe (« txtfile ») ; un nom de classe est
/// rendu tel quel s'il existe.
fn prog_id(assoc: &str) -> Option<String> {
    let exists = |p: &str| reg::read(|d| Ok(d.exists(p))).unwrap_or(false);
    if assoc.starts_with('.') {
        let ext = hkcr(assoc);
        if !exists(&ext) {
            return None;
        }
        return Some(
            text_value(&ext, "")
                .filter(|p| !p.is_empty() && exists(&hkcr(p)))
                .unwrap_or_else(|| assoc.to_string()),
        );
    }
    exists(&hkcr(assoc)).then(|| assoc.to_string())
}

/// Verbe demandé, sinon le verbe par défaut de la classe, sinon « open »,
/// sinon le premier verbe déclaré.
fn verb(class: &str, extra: &str) -> Option<String> {
    if !extra.is_empty() {
        return Some(extra.to_string());
    }
    let shell = hkcr(&format!("{class}\\shell"));
    if let Some(v) = text_value(&shell, "").filter(|v| !v.is_empty()) {
        return Some(v);
    }
    let verbs = reg::read(|d| Ok(d.subkeys(&shell))).unwrap_or_default();
    verbs
        .iter()
        .find(|v| v.eq_ignore_ascii_case("open"))
        .or(verbs.first())
        .cloned()
}

fn command(class: &str, extra: &str) -> Option<String> {
    let verb = verb(class, extra)?;
    text_value(&hkcr(&format!("{class}\\shell\\{verb}\\command")), "")
}

/// Programme d'une ligne de commande : `"C:\…\app.exe" "%1"` → `C:\…\app.exe`.
fn executable(cmd: &str) -> String {
    let cmd = cmd.trim();
    if let Some(rest) = cmd.strip_prefix('"') {
        return rest.split('"').next().unwrap_or("").to_string();
    }
    let lower = cmd.to_ascii_lowercase();
    match lower.find(".exe") {
        Some(i) => cmd[..i + 4].to_string(),
        None => cmd.split_whitespace().next().unwrap_or("").to_string(),
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

fn assoc_string(kind: u32, assoc: &str, extra: &str) -> Option<String> {
    let class = prog_id(assoc)?;
    match kind {
        ASSOCSTR_COMMAND => command(&class, extra),
        ASSOCSTR_EXECUTABLE => command(&class, extra)
            .map(|c| executable(&c))
            .filter(|e| !e.is_empty()),
        ASSOCSTR_FRIENDLYDOCNAME => text_value(&hkcr(&class), "")
            .filter(|n| !n.is_empty())
            .or_else(|| {
                text_value(&hkcr(&class), "FriendlyTypeName").filter(|n| !n.starts_with('@'))
            }),
        ASSOCSTR_FRIENDLYAPPNAME => {
            let exe = executable(&command(&class, extra)?);
            let name = file_name(&exe).to_string();
            text_value(&hkcr(&format!("Applications\\{name}")), "FriendlyAppName")
                .or_else(|| {
                    let lower = name.to_ascii_lowercase();
                    Some(match lower.strip_suffix(".exe") {
                        Some(_) => name[..name.len() - 4].to_string(),
                        None => name,
                    })
                })
                .filter(|n| !n.is_empty())
        }
        ASSOCSTR_CONTENTTYPE => assoc
            .starts_with('.')
            .then(|| text_value(&hkcr(assoc), "Content Type"))
            .flatten(),
        ASSOCSTR_DEFAULTICON => text_value(&hkcr(&format!("{class}\\DefaultIcon")), ""),
        ASSOCSTR_SHELLEXTENSION => {
            let key = if assoc.starts_with('.') {
                assoc
            } else {
                &class
            };
            text_value(&hkcr(&format!("{key}\\shellex\\{extra}")), "")
                .or_else(|| text_value(&hkcr(&format!("{class}\\shellex\\{extra}")), ""))
        }
        ASSOCSTR_PROGID => assoc.starts_with('.').then_some(class),
        _ => None,
    }
}

unsafe fn put_assoc(s: &str, ansi: bool, out: *mut c_void, cch: *mut u32) -> i32 {
    if cch.is_null() {
        return E_POINTER;
    }
    let units: Vec<u16> = s.encode_utf16().chain([0]).collect();
    let bytes = {
        let mut b = crate::dlls::encode_ansi(s);
        b.push(0);
        b
    };
    let need = if ansi { bytes.len() } else { units.len() } as u32;
    if out.is_null() {
        *cch = need;
        return S_FALSE;
    }
    if *cch < need {
        *cch = need;
        return E_POINTER;
    }
    if ansi {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out.cast::<u8>(), bytes.len());
    } else {
        std::ptr::copy_nonoverlapping(units.as_ptr(), out.cast::<u16>(), units.len());
    }
    *cch = need;
    S_OK
}

pub unsafe extern "win64" fn assoc_query_string_a(
    _flags: u32,
    kind: u32,
    assoc: *const u8,
    extra: *const u8,
    out: *mut u8,
    cch: *mut u32,
) -> i32 {
    if assoc.is_null() {
        return E_INVALIDARG;
    }
    match assoc_string(kind, &text_a(assoc), &text_a(extra)) {
        Some(s) => put_assoc(&s, true, out.cast(), cch),
        None => E_NO_ASSOCIATION,
    }
}
pub unsafe extern "win64" fn assoc_query_string_w(
    _flags: u32,
    kind: u32,
    assoc: *const u16,
    extra: *const u16,
    out: *mut u16,
    cch: *mut u32,
) -> i32 {
    if assoc.is_null() {
        return E_INVALIDARG;
    }
    match assoc_string(kind, &text_w(assoc), &text_w(extra)) {
        Some(s) => put_assoc(&s, false, out.cast(), cch),
        None => E_NO_ASSOCIATION,
    }
}

unsafe fn assoc_key(kind: u32, assoc: &str, extra: &str, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    let Some(class) = prog_id(assoc) else {
        return E_NO_ASSOCIATION;
    };
    let path = match kind {
        ASSOCKEY_SHELLEXECCLASS | ASSOCKEY_CLASS => hkcr(&class),
        ASSOCKEY_APP => match command(&class, extra) {
            Some(c) => hkcr(&format!("Applications\\{}", file_name(&executable(&c)))),
            None => return E_NO_ASSOCIATION,
        },
        _ => return E_INVALIDARG,
    };
    match reg::open(&path) {
        Ok(h) => {
            *out = h;
            S_OK
        }
        Err(_) => E_NO_ASSOCIATION,
    }
}
pub unsafe extern "win64" fn assoc_query_key_a(
    _flags: u32,
    kind: u32,
    assoc: *const u8,
    extra: *const u8,
    out: *mut usize,
) -> i32 {
    assoc_key(kind, &text_a(assoc), &text_a(extra), out)
}
pub unsafe extern "win64" fn assoc_query_key_w(
    _flags: u32,
    kind: u32,
    assoc: *const u16,
    extra: *const u16,
    out: *mut usize,
) -> i32 {
    assoc_key(kind, &text_w(assoc), &text_w(extra), out)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "SHGetValueA" => fn sh_get_value_a,
        "SHGetValueW" => fn sh_get_value_w,
        "SHSetValueA" => fn sh_set_value_a,
        "SHSetValueW" => fn sh_set_value_w,
        "SHDeleteValueA" => fn sh_delete_value_a,
        "SHDeleteValueW" => fn sh_delete_value_w,
        "SHQueryValueExA" => fn sh_query_value_ex_a,
        "SHQueryValueExW" => fn sh_query_value_ex_w,
        "SHDeleteKeyA" => fn sh_delete_key_a,
        "SHDeleteKeyW" => fn sh_delete_key_w,
        "SHDeleteEmptyKeyA" => fn sh_delete_empty_key_a,
        "SHDeleteEmptyKeyW" => fn sh_delete_empty_key_w,
        "SHCopyKeyA" => fn sh_copy_key_a,
        "SHCopyKeyW" => fn sh_copy_key_w,
        "SHEnumKeyExA" => fn sh_enum_key_ex_a,
        "SHEnumKeyExW" => fn sh_enum_key_ex_w,
        "SHEnumValueA" => fn sh_enum_value_a,
        "SHEnumValueW" => fn sh_enum_value_w,
        "SHQueryInfoKeyA" => fn sh_query_info_key_a,
        "SHQueryInfoKeyW" => fn sh_query_info_key_w,
        "SHRegGetValueA" => fn sh_reg_get_value_a,
        "SHRegGetValueW" => fn sh_reg_get_value_w,
        "SHRegGetPathA" => fn sh_reg_get_path_a,
        "SHRegGetPathW" => fn sh_reg_get_path_w,
        "SHRegSetPathA" => fn sh_reg_set_path_a,
        "SHRegSetPathW" => fn sh_reg_set_path_w,
        "SHRegDuplicateHKey" => fn sh_reg_duplicate_hkey,
        "SHRegGetUSValueA" => fn sh_reg_get_us_value_a,
        "SHRegGetUSValueW" => fn sh_reg_get_us_value_w,
        "SHRegSetUSValueA" => fn sh_reg_set_us_value_a,
        "SHRegSetUSValueW" => fn sh_reg_set_us_value_w,
        "SHRegGetBoolUSValueA" => fn sh_reg_get_bool_us_value_a,
        "SHRegGetBoolUSValueW" => fn sh_reg_get_bool_us_value_w,
        "SHRegOpenUSKeyA" => fn sh_reg_open_us_key_a,
        "SHRegOpenUSKeyW" => fn sh_reg_open_us_key_w,
        "SHRegCreateUSKeyA" => fn sh_reg_create_us_key_a,
        "SHRegCreateUSKeyW" => fn sh_reg_create_us_key_w,
        "SHRegQueryUSValueA" => fn sh_reg_query_us_value_a,
        "SHRegQueryUSValueW" => fn sh_reg_query_us_value_w,
        "SHRegWriteUSValueA" => fn sh_reg_write_us_value_a,
        "SHRegWriteUSValueW" => fn sh_reg_write_us_value_w,
        "SHRegDeleteUSValueA" => fn sh_reg_delete_us_value_a,
        "SHRegDeleteUSValueW" => fn sh_reg_delete_us_value_w,
        "SHRegEnumUSKeyA" => fn sh_reg_enum_us_key_a,
        "SHRegEnumUSKeyW" => fn sh_reg_enum_us_key_w,
        "SHRegEnumUSValueA" => fn sh_reg_enum_us_value_a,
        "SHRegEnumUSValueW" => fn sh_reg_enum_us_value_w,
        "SHRegQueryInfoUSKeyA" => fn sh_reg_query_info_us_key,
        "SHRegQueryInfoUSKeyW" => fn sh_reg_query_info_us_key,
        "SHRegCloseUSKey" => fn sh_reg_close_us_key,
        "SHOpenRegStream2A" => fn sh_open_reg_stream2_a,
        "SHOpenRegStream2W" => fn sh_open_reg_stream2_w,
        "SHOpenRegStreamA" => fn sh_open_reg_stream_a,
        "SHOpenRegStreamW" => fn sh_open_reg_stream_w,
        "AssocQueryStringA" => fn assoc_query_string_a,
        "AssocQueryStringW" => fn assoc_query_string_w,
        "AssocQueryKeyA" => fn assoc_query_key_a,
        "AssocQueryKeyW" => fn assoc_query_key_w,
    }
}

#[cfg(test)]
mod tests;
