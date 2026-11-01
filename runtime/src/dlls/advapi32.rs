//! API Win32 du registre (Reg*) au-dessus du magasin partagé `crate::registry`.
//! Les fonctions « A » convertissent noms et chaînes en Windows-1252, les
//! fonctions « W » travaillent en UTF-16 comme le stockage.

use std::ffi::c_void;

use crate::registry::{
    self as reg, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_INVALID_HANDLE,
    ERROR_INVALID_PARAMETER, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS,
};

const REG_CREATED_NEW_KEY: u32 = 1;
const REG_OPENED_EXISTING_KEY: u32 = 2;
const ERROR_UNSUPPORTED_TYPE: i32 = 1630;

#[cfg(test)]
pub(crate) use crate::registry::TEST_LOCK as REGISTRY_TEST_LOCK;

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

fn sub_path(root: usize, sub: &str) -> Result<String, i32> {
    if sub.contains('\0') {
        return Err(ERROR_INVALID_PARAMETER);
    }
    Ok(reg::join(&reg::handle_path(root)?, sub))
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

/// Copie un nom dans un tampon « A » (`size` en caractères, zéro compris).
unsafe fn put_name_a(s: &str, buf: *mut u8, size: *mut u32) -> i32 {
    let b = crate::dlls::encode_ansi(s);
    if (*size as usize) <= b.len() {
        *size = b.len() as u32 + 1;
        return ERROR_MORE_DATA;
    }
    std::ptr::copy_nonoverlapping(b.as_ptr(), buf, b.len());
    *buf.add(b.len()) = 0;
    *size = b.len() as u32;
    ERROR_SUCCESS
}

unsafe fn put_name_w(s: &str, buf: *mut u16, size: *mut u32) -> i32 {
    let w: Vec<u16> = s.encode_utf16().collect();
    if (*size as usize) <= w.len() {
        *size = w.len() as u32 + 1;
        return ERROR_MORE_DATA;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    *buf.add(w.len()) = 0;
    *size = w.len() as u32;
    ERROR_SUCCESS
}

// ── Ouverture, création, fermeture ─────────────────────────────────

unsafe fn open_impl(root: usize, sub: &str, result: *mut usize) -> i32 {
    if result.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let path = match sub_path(root, sub) {
        Ok(p) => p,
        Err(e) => return e,
    };
    match reg::open(&path) {
        Ok(h) => {
            *result = h;
            ERROR_SUCCESS
        }
        Err(e) => e,
    }
}

unsafe fn create_impl(root: usize, sub: &str, result: *mut usize, disposition: *mut u32) -> i32 {
    if result.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let path = match sub_path(root, sub) {
        Ok(p) => p,
        Err(e) => return e,
    };
    match reg::create(&path) {
        Ok((h, created)) => {
            *result = h;
            if !disposition.is_null() {
                *disposition = if created {
                    REG_CREATED_NEW_KEY
                } else {
                    REG_OPENED_EXISTING_KEY
                };
            }
            ERROR_SUCCESS
        }
        Err(e) => e,
    }
}

pub unsafe extern "win64" fn reg_open_key_ex_a(
    root: usize,
    sub: *const u8,
    _options: u32,
    _access: u32,
    result: *mut usize,
) -> i32 {
    open_impl(root, &text_a(sub), result)
}
pub unsafe extern "win64" fn reg_open_key_ex_w(
    root: usize,
    sub: *const u16,
    _options: u32,
    _access: u32,
    result: *mut usize,
) -> i32 {
    open_impl(root, &text_w(sub), result)
}
pub unsafe extern "win64" fn reg_open_key_a(
    root: usize,
    sub: *const u8,
    result: *mut usize,
) -> i32 {
    open_impl(root, &text_a(sub), result)
}
pub unsafe extern "win64" fn reg_open_key_w(
    root: usize,
    sub: *const u16,
    result: *mut usize,
) -> i32 {
    open_impl(root, &text_w(sub), result)
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn reg_create_key_ex_a(
    root: usize,
    sub: *const u8,
    _reserved: u32,
    _class: *const u8,
    _options: u32,
    _access: u32,
    _security: usize,
    result: *mut usize,
    disposition: *mut u32,
) -> i32 {
    create_impl(root, &text_a(sub), result, disposition)
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn reg_create_key_ex_w(
    root: usize,
    sub: *const u16,
    _reserved: u32,
    _class: *const u16,
    _options: u32,
    _access: u32,
    _security: usize,
    result: *mut usize,
    disposition: *mut u32,
) -> i32 {
    create_impl(root, &text_w(sub), result, disposition)
}
pub unsafe extern "win64" fn reg_create_key_a(
    root: usize,
    sub: *const u8,
    result: *mut usize,
) -> i32 {
    create_impl(root, &text_a(sub), result, std::ptr::null_mut())
}
pub unsafe extern "win64" fn reg_create_key_w(
    root: usize,
    sub: *const u16,
    result: *mut usize,
) -> i32 {
    create_impl(root, &text_w(sub), result, std::ptr::null_mut())
}

pub unsafe extern "win64" fn reg_open_current_user(_access: u32, result: *mut usize) -> i32 {
    if result.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    *result = reg::HKEY_CURRENT_USER;
    ERROR_SUCCESS
}

pub unsafe extern "win64" fn reg_open_user_classes_root(
    _token: usize,
    _options: u32,
    _access: u32,
    result: *mut usize,
) -> i32 {
    create_impl(
        reg::HKEY_CURRENT_USER,
        "Software\\Classes",
        result,
        std::ptr::null_mut(),
    )
}

pub unsafe extern "win64" fn reg_close_key(handle: usize) -> i32 {
    if reg::predefined(handle).is_some() || reg::close_handle(handle) {
        ERROR_SUCCESS
    } else {
        ERROR_INVALID_HANDLE
    }
}

pub extern "win64" fn reg_success_stub() -> i32 {
    ERROR_SUCCESS
}

// ── Valeurs ────────────────────────────────────────────────────────

fn set_impl(path: &str, name: &str, kind: u32, bytes: Vec<u8>) -> i32 {
    status(reg::write(|d| {
        if !d.exists(path) {
            return Err(ERROR_FILE_NOT_FOUND);
        }
        d.set(path, name, kind, bytes);
        Ok(())
    }))
}

unsafe fn raw(data: *const u8, size: u32) -> Result<Vec<u8>, i32> {
    if size == 0 {
        return Ok(Vec::new());
    }
    if data.is_null() {
        return Err(ERROR_INVALID_PARAMETER);
    }
    Ok(std::slice::from_raw_parts(data, size as usize).to_vec())
}

pub unsafe extern "win64" fn reg_set_value_ex_a(
    handle: usize,
    name: *const u8,
    _reserved: u32,
    kind: u32,
    data: *const u8,
    size: u32,
) -> i32 {
    let (path, bytes) = match (reg::handle_path(handle), raw(data, size)) {
        (Ok(p), Ok(b)) => (p, b),
        (Err(e), _) | (_, Err(e)) => return e,
    };
    set_impl(&path, &text_a(name), kind, reg::from_ansi(kind, &bytes))
}
pub unsafe extern "win64" fn reg_set_value_ex_w(
    handle: usize,
    name: *const u16,
    _reserved: u32,
    kind: u32,
    data: *const u8,
    size: u32,
) -> i32 {
    let (path, bytes) = match (reg::handle_path(handle), raw(data, size)) {
        (Ok(p), Ok(b)) => (p, b),
        (Err(e), _) | (_, Err(e)) => return e,
    };
    set_impl(&path, &text_w(name), kind, bytes)
}

/// RegSetKeyValue : crée la sous-clé au besoin puis écrit la valeur.
fn set_key_value(root: usize, sub: &str, name: &str, kind: u32, bytes: Vec<u8>) -> i32 {
    let path = match sub_path(root, sub) {
        Ok(p) => p,
        Err(e) => return e,
    };
    status(reg::write(|d| {
        d.ensure(&path);
        d.set(&path, name, kind, bytes);
        Ok(())
    }))
}
pub unsafe extern "win64" fn reg_set_key_value_a(
    root: usize,
    sub: *const u8,
    name: *const u8,
    kind: u32,
    data: *const u8,
    size: u32,
) -> i32 {
    match raw(data, size) {
        Ok(b) => set_key_value(
            root,
            &text_a(sub),
            &text_a(name),
            kind,
            reg::from_ansi(kind, &b),
        ),
        Err(e) => e,
    }
}
pub unsafe extern "win64" fn reg_set_key_value_w(
    root: usize,
    sub: *const u16,
    name: *const u16,
    kind: u32,
    data: *const u8,
    size: u32,
) -> i32 {
    match raw(data, size) {
        Ok(b) => set_key_value(root, &text_w(sub), &text_w(name), kind, b),
        Err(e) => e,
    }
}
/// RegSetValue (Win16) : valeur par défaut d'une sous-clé, toujours REG_SZ.
pub unsafe extern "win64" fn reg_set_value_a(
    root: usize,
    sub: *const u8,
    kind: u32,
    data: *const u8,
    _size: u32,
) -> i32 {
    if kind != reg::REG_SZ || data.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    set_key_value(
        root,
        &text_a(sub),
        "",
        reg::REG_SZ,
        reg::utf16z(&text_a(data)),
    )
}
pub unsafe extern "win64" fn reg_set_value_w(
    root: usize,
    sub: *const u16,
    kind: u32,
    data: *const u16,
    _size: u32,
) -> i32 {
    if kind != reg::REG_SZ || data.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    set_key_value(
        root,
        &text_w(sub),
        "",
        reg::REG_SZ,
        reg::utf16z(&text_w(data)),
    )
}

fn get_value(path: &str, name: &str) -> Result<reg::Value, i32> {
    reg::read(|d| {
        if !d.exists(path) {
            return Err(ERROR_INVALID_HANDLE);
        }
        d.get(path, name).cloned().ok_or(ERROR_FILE_NOT_FOUND)
    })
}

unsafe fn query_impl(
    handle: usize,
    name: &str,
    ansi: bool,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
) -> i32 {
    let path = match reg::handle_path(handle) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let value = match get_value(&path, name) {
        Ok(v) => v,
        Err(ERROR_INVALID_HANDLE) => return ERROR_FILE_NOT_FOUND,
        Err(e) => return e,
    };
    if !kind.is_null() {
        *kind = value.kind;
    }
    let bytes = if ansi {
        reg::to_ansi(value.kind, &value.bytes)
    } else {
        value.bytes
    };
    put_data(&bytes, data, size)
}

pub unsafe extern "win64" fn reg_query_value_ex_a(
    handle: usize,
    name: *const u8,
    _reserved: *mut u32,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
) -> i32 {
    query_impl(handle, &text_a(name), true, kind, data, size)
}
pub unsafe extern "win64" fn reg_query_value_ex_w(
    handle: usize,
    name: *const u16,
    _reserved: *mut u32,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
) -> i32 {
    query_impl(handle, &text_w(name), false, kind, data, size)
}

/// RegQueryValue (Win16) : texte de la valeur par défaut d'une sous-clé.
unsafe fn query_default(root: usize, sub: &str, ansi: bool, data: *mut u8, size: *mut i32) -> i32 {
    let path = match sub_path(root, sub) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let text = match reg::read(|d| {
        if !d.exists(&path) {
            return Err(ERROR_FILE_NOT_FOUND);
        }
        Ok(d.get(&path, "")
            .filter(|v| reg::is_string(v.kind))
            .map(|v| reg::string_of(&v.bytes))
            .unwrap_or_default())
    }) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let bytes = if ansi {
        let mut b = crate::dlls::encode_ansi(&text);
        b.push(0);
        b
    } else {
        reg::utf16z(&text)
    };
    put_data(&bytes, data, size.cast())
}
pub unsafe extern "win64" fn reg_query_value_a(
    root: usize,
    sub: *const u8,
    data: *mut u8,
    size: *mut i32,
) -> i32 {
    query_default(root, &text_a(sub), true, data, size)
}
pub unsafe extern "win64" fn reg_query_value_w(
    root: usize,
    sub: *const u16,
    data: *mut u16,
    size: *mut i32,
) -> i32 {
    query_default(root, &text_w(sub), false, data.cast(), size)
}

/// RegGetValue : sous-clé facultative, filtre de types (RRF_RT_*),
/// chaînes toujours terminées et REG_EXPAND_SZ développé sauf RRF_NOEXPAND.
#[allow(clippy::too_many_arguments)]
unsafe fn get_value_impl(
    root: usize,
    sub: &str,
    name: &str,
    flags: u32,
    ansi: bool,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
) -> i32 {
    const RRF_NOEXPAND: u32 = 0x1000_0000;
    const RRF_ZEROONFAILURE: u32 = 0x2000_0000;
    if flags & 0xFFFF == 0 || (!data.is_null() && size.is_null()) {
        return ERROR_INVALID_PARAMETER;
    }
    let cap = if size.is_null() { 0 } else { *size };
    let fail = |code: i32| {
        if flags & RRF_ZEROONFAILURE != 0 && !data.is_null() {
            std::ptr::write_bytes(data, 0, cap as usize);
        }
        code
    };
    let path = match sub_path(root, sub) {
        Ok(p) => p,
        Err(e) => return fail(e),
    };
    let mut value = match get_value(&path, name) {
        Ok(v) => v,
        Err(ERROR_INVALID_HANDLE) => return fail(ERROR_FILE_NOT_FOUND),
        Err(e) => return fail(e),
    };
    if value.kind == reg::REG_EXPAND_SZ && flags & RRF_NOEXPAND == 0 {
        value.kind = reg::REG_SZ;
        value.bytes = reg::utf16z(&reg::expand(&reg::string_of(&value.bytes)));
    }
    let bit = match value.kind {
        reg::REG_SZ => 0x02,
        reg::REG_EXPAND_SZ => 0x04,
        reg::REG_BINARY => 0x08,
        reg::REG_DWORD => 0x10,
        reg::REG_MULTI_SZ => 0x20,
        reg::REG_QWORD => 0x40,
        reg::REG_NONE => 0x01,
        _ => 0,
    };
    if flags & 0xFFFF & bit == 0 {
        return fail(ERROR_UNSUPPORTED_TYPE);
    }
    if reg::is_string(value.kind) {
        let mut units = reg::utf16_units(&value.bytes);
        if units.last() != Some(&0) {
            units.push(0);
        }
        if value.kind == reg::REG_MULTI_SZ && units.len() > 1 && units[units.len() - 2] != 0 {
            units.push(0);
        }
        value.bytes = units.iter().flat_map(|u| u.to_le_bytes()).collect();
    }
    if !kind.is_null() {
        *kind = value.kind;
    }
    let bytes = if ansi {
        reg::to_ansi(value.kind, &value.bytes)
    } else {
        value.bytes
    };
    match put_data(&bytes, data, size) {
        ERROR_SUCCESS => ERROR_SUCCESS,
        e => fail(e),
    }
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn reg_get_value_a(
    root: usize,
    sub: *const u8,
    name: *const u8,
    flags: u32,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
) -> i32 {
    get_value_impl(
        root,
        &text_a(sub),
        &text_a(name),
        flags,
        true,
        kind,
        data,
        size,
    )
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn reg_get_value_w(
    root: usize,
    sub: *const u16,
    name: *const u16,
    flags: u32,
    kind: *mut u32,
    data: *mut u8,
    size: *mut u32,
) -> i32 {
    get_value_impl(
        root,
        &text_w(sub),
        &text_w(name),
        flags,
        false,
        kind,
        data,
        size,
    )
}

/// RegLoadMUIString : les chaînes « @fichier.dll,-id » ne sont pas encore
/// résolues ; une chaîne ordinaire est rendue telle quelle.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn reg_load_mui_string_w(
    handle: usize,
    name: *const u16,
    out: *mut u16,
    out_size: u32,
    needed: *mut u32,
    _flags: u32,
    _dir: *const u16,
) -> i32 {
    let path = match reg::handle_path(handle) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let value = match get_value(&path, &text_w(name)) {
        Ok(v) if reg::is_string(v.kind) => v,
        Ok(_) => return ERROR_INVALID_DATA_TYPE,
        Err(_) => return ERROR_FILE_NOT_FOUND,
    };
    let text = reg::string_of(&value.bytes);
    if text.starts_with('@') {
        return ERROR_FILE_NOT_FOUND;
    }
    let bytes = reg::utf16z(&text);
    if !needed.is_null() {
        *needed = bytes.len() as u32;
    }
    if out.is_null() || (out_size as usize) < bytes.len() {
        return ERROR_MORE_DATA;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), out.cast(), bytes.len());
    ERROR_SUCCESS
}
const ERROR_INVALID_DATA_TYPE: i32 = 1630;

fn delete_value_impl(path: &str, name: &str) -> i32 {
    status(reg::write(|d| {
        if !d.exists(path) {
            return Err(ERROR_FILE_NOT_FOUND);
        }
        if d.delete_value(path, name) {
            Ok(())
        } else {
            Err(ERROR_FILE_NOT_FOUND)
        }
    }))
}
pub unsafe extern "win64" fn reg_delete_value_a(handle: usize, name: *const u8) -> i32 {
    match reg::handle_path(handle) {
        Ok(p) => delete_value_impl(&p, &text_a(name)),
        Err(e) => e,
    }
}
pub unsafe extern "win64" fn reg_delete_value_w(handle: usize, name: *const u16) -> i32 {
    match reg::handle_path(handle) {
        Ok(p) => delete_value_impl(&p, &text_w(name)),
        Err(e) => e,
    }
}
pub unsafe extern "win64" fn reg_delete_key_value_a(
    root: usize,
    sub: *const u8,
    name: *const u8,
) -> i32 {
    match sub_path(root, &text_a(sub)) {
        Ok(p) => delete_value_impl(&p, &text_a(name)),
        Err(e) => e,
    }
}
pub unsafe extern "win64" fn reg_delete_key_value_w(
    root: usize,
    sub: *const u16,
    name: *const u16,
) -> i32 {
    match sub_path(root, &text_w(sub)) {
        Ok(p) => delete_value_impl(&p, &text_w(name)),
        Err(e) => e,
    }
}

// ── Suppression, copie, renommage de clés ──────────────────────────

/// RegDeleteKey : la clé doit exister et ne plus avoir de sous-clé.
fn delete_key_impl(root: usize, sub: &str) -> i32 {
    if sub.trim_matches('\\').is_empty() {
        return ERROR_ACCESS_DENIED;
    }
    let path = match sub_path(root, sub) {
        Ok(p) => p,
        Err(e) => return e,
    };
    status(reg::write(|d| {
        if !d.exists(&path) {
            return Err(ERROR_FILE_NOT_FOUND);
        }
        if d.has_subkeys(&path) {
            return Err(ERROR_ACCESS_DENIED);
        }
        if d.delete_tree(&path) {
            Ok(())
        } else {
            Err(ERROR_ACCESS_DENIED)
        }
    }))
}
pub unsafe extern "win64" fn reg_delete_key_a(root: usize, sub: *const u8) -> i32 {
    delete_key_impl(root, &text_a(sub))
}
pub unsafe extern "win64" fn reg_delete_key_w(root: usize, sub: *const u16) -> i32 {
    delete_key_impl(root, &text_w(sub))
}
pub unsafe extern "win64" fn reg_delete_key_ex_a(
    root: usize,
    sub: *const u8,
    _sam: u32,
    _reserved: u32,
) -> i32 {
    delete_key_impl(root, &text_a(sub))
}
pub unsafe extern "win64" fn reg_delete_key_ex_w(
    root: usize,
    sub: *const u16,
    _sam: u32,
    _reserved: u32,
) -> i32 {
    delete_key_impl(root, &text_w(sub))
}

/// RegDeleteTree : sans sous-clé, vide la clé (valeurs et sous-clés) sans
/// la supprimer ; avec sous-clé, supprime cette sous-clé et son contenu.
fn delete_tree_impl(root: usize, sub: &str) -> i32 {
    let path = match sub_path(root, sub) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let whole = !sub.trim_matches('\\').is_empty();
    status(reg::write(|d| {
        if !d.exists(&path) {
            return Err(ERROR_FILE_NOT_FOUND);
        }
        if whole {
            return if d.delete_tree(&path) {
                Ok(())
            } else {
                Err(ERROR_ACCESS_DENIED)
            };
        }
        for child in d.subkeys(&path) {
            d.delete_tree(&reg::join(&path, &child));
        }
        for v in d.values(&path) {
            d.delete_value(&path, &v.name);
        }
        Ok(())
    }))
}
pub unsafe extern "win64" fn reg_delete_tree_a(root: usize, sub: *const u8) -> i32 {
    delete_tree_impl(root, &text_a(sub))
}
pub unsafe extern "win64" fn reg_delete_tree_w(root: usize, sub: *const u16) -> i32 {
    delete_tree_impl(root, &text_w(sub))
}

fn copy_tree_impl(src: usize, sub: &str, dest: usize) -> i32 {
    let (from, to) = match (sub_path(src, sub), reg::handle_path(dest)) {
        (Ok(f), Ok(t)) => (f, t),
        (Err(e), _) | (_, Err(e)) => return e,
    };
    status(reg::write(|d| {
        if !d.exists(&from) || !d.exists(&to) {
            return Err(ERROR_FILE_NOT_FOUND);
        }
        d.copy_tree(&from, &to);
        Ok(())
    }))
}
pub unsafe extern "win64" fn reg_copy_tree_a(src: usize, sub: *const u8, dest: usize) -> i32 {
    copy_tree_impl(src, &text_a(sub), dest)
}
pub unsafe extern "win64" fn reg_copy_tree_w(src: usize, sub: *const u16, dest: usize) -> i32 {
    copy_tree_impl(src, &text_w(sub), dest)
}

/// RegRenameKey (utilisé par regedit) : renomme une clé et son contenu.
pub unsafe extern "win64" fn reg_rename_key(
    handle: usize,
    sub: *const u16,
    new_name: *const u16,
) -> i32 {
    let path = match sub_path(handle, &text_w(sub)) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let new_name = text_w(new_name);
    status(reg::write(|d| d.rename_key(&path, &new_name)))
}

pub unsafe extern "win64" fn reg_flush_key(handle: usize) -> i32 {
    match reg::handle_path(handle) {
        Ok(p) => status(reg::read(|d| {
            if d.exists(&p) {
                Ok(())
            } else {
                Err(ERROR_FILE_NOT_FOUND)
            }
        })),
        Err(e) => e,
    }
}

/// RegNotifyChangeKeyValue : prévient quand une appli modifie le registre.
/// Le suivi porte sur tout le fichier partagé (plus large que la clé
/// demandée, ce qui est permis : l'appli relit alors ses valeurs).
pub unsafe extern "win64" fn reg_notify_change_key_value(
    handle: usize,
    _subtree: i32,
    _filter: u32,
    event: usize,
    asynchronous: i32,
) -> i32 {
    if let Err(e) = reg::handle_path(handle) {
        return e;
    }
    if asynchronous != 0 {
        if event == 0 {
            return ERROR_INVALID_PARAMETER;
        }
        std::thread::spawn(move || {
            reg::wait_for_change(None);
            crate::dlls::kernel32::set_event(event);
        });
    } else {
        reg::wait_for_change(None);
    }
    ERROR_SUCCESS
}

// ── Énumération ────────────────────────────────────────────────────

fn subkey_at(handle: usize, index: u32) -> Result<(String, u64), i32> {
    let path = reg::handle_path(handle)?;
    reg::read(|d| {
        if !d.exists(&path) {
            return Err(ERROR_INVALID_HANDLE);
        }
        let name = d
            .subkeys(&path)
            .into_iter()
            .nth(index as usize)
            .ok_or(ERROR_NO_MORE_ITEMS)?;
        let stamp = d
            .key(&reg::join(&path, &name))
            .map(|k| k.last_write)
            .unwrap_or(0);
        Ok((name, stamp))
    })
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn reg_enum_key_ex_a(
    handle: usize,
    index: u32,
    name: *mut u8,
    name_size: *mut u32,
    _reserved: *mut u32,
    class: *mut u8,
    class_size: *mut u32,
    last_write: *mut u64,
) -> i32 {
    if name.is_null() || name_size.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let (child, stamp) = match subkey_at(handle, index) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let r = put_name_a(&child, name, name_size);
    if r == ERROR_SUCCESS {
        if !class.is_null() && !class_size.is_null() && *class_size > 0 {
            *class = 0;
        }
        if !class_size.is_null() {
            *class_size = 0;
        }
        if !last_write.is_null() {
            *last_write = stamp;
        }
    }
    r
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn reg_enum_key_ex_w(
    handle: usize,
    index: u32,
    name: *mut u16,
    name_size: *mut u32,
    _reserved: *mut u32,
    class: *mut u16,
    class_size: *mut u32,
    last_write: *mut u64,
) -> i32 {
    if name.is_null() || name_size.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let (child, stamp) = match subkey_at(handle, index) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let r = put_name_w(&child, name, name_size);
    if r == ERROR_SUCCESS {
        if !class.is_null() && !class_size.is_null() && *class_size > 0 {
            *class = 0;
        }
        if !class_size.is_null() {
            *class_size = 0;
        }
        if !last_write.is_null() {
            *last_write = stamp;
        }
    }
    r
}
pub unsafe extern "win64" fn reg_enum_key_a(
    handle: usize,
    index: u32,
    name: *mut u8,
    size: u32,
) -> i32 {
    let mut n = size;
    reg_enum_key_ex_a(
        handle,
        index,
        name,
        &mut n,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    )
}
pub unsafe extern "win64" fn reg_enum_key_w(
    handle: usize,
    index: u32,
    name: *mut u16,
    size: u32,
) -> i32 {
    let mut n = size;
    reg_enum_key_ex_w(
        handle,
        index,
        name,
        &mut n,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    )
}

fn value_at(handle: usize, index: u32) -> Result<reg::Value, i32> {
    let path = reg::handle_path(handle)?;
    reg::read(|d| {
        if !d.exists(&path) {
            return Err(ERROR_INVALID_HANDLE);
        }
        d.values(&path)
            .into_iter()
            .nth(index as usize)
            .ok_or(ERROR_NO_MORE_ITEMS)
    })
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn reg_enum_value_a(
    handle: usize,
    index: u32,
    name: *mut u8,
    name_size: *mut u32,
    _reserved: *mut u32,
    kind: *mut u32,
    data: *mut u8,
    data_size: *mut u32,
) -> i32 {
    if name.is_null() || name_size.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let value = match value_at(handle, index) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let r = put_name_a(&value.name, name, name_size);
    if r != ERROR_SUCCESS {
        return r;
    }
    if !kind.is_null() {
        *kind = value.kind;
    }
    put_data(&reg::to_ansi(value.kind, &value.bytes), data, data_size)
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn reg_enum_value_w(
    handle: usize,
    index: u32,
    name: *mut u16,
    name_size: *mut u32,
    _reserved: *mut u32,
    kind: *mut u32,
    data: *mut u8,
    data_size: *mut u32,
) -> i32 {
    if name.is_null() || name_size.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let value = match value_at(handle, index) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let r = put_name_w(&value.name, name, name_size);
    if r != ERROR_SUCCESS {
        return r;
    }
    if !kind.is_null() {
        *kind = value.kind;
    }
    put_data(&value.bytes, data, data_size)
}

#[allow(clippy::too_many_arguments)]
unsafe fn query_info_impl(
    handle: usize,
    class_size: *mut u32,
    sub_keys: *mut u32,
    max_sub_key_len: *mut u32,
    max_class_len: *mut u32,
    values: *mut u32,
    max_value_name_len: *mut u32,
    max_value_len: *mut u32,
    security: *mut u32,
    last_write: *mut u64,
) -> i32 {
    let path = match reg::handle_path(handle) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let info = reg::read(|d| {
        if !d.exists(&path) {
            return Err(ERROR_INVALID_HANDLE);
        }
        let children = d.subkeys(&path);
        let vals = d.values(&path);
        let len = |s: &str| s.encode_utf16().count() as u32;
        Ok((
            children.len() as u32,
            children.iter().map(|c| len(c)).max().unwrap_or(0),
            vals.len() as u32,
            vals.iter().map(|v| len(&v.name)).max().unwrap_or(0),
            vals.iter().map(|v| v.bytes.len() as u32).max().unwrap_or(0),
            d.key(&path).map(|k| k.last_write).unwrap_or(0),
        ))
    });
    let (n_sub, max_sub, n_val, max_name, max_data, stamp) = match info {
        Ok(i) => i,
        Err(e) => return e,
    };
    for (ptr, v) in [
        (class_size, 0),
        (sub_keys, n_sub),
        (max_sub_key_len, max_sub),
        (max_class_len, 0),
        (values, n_val),
        (max_value_name_len, max_name),
        (max_value_len, max_data),
        (security, 0),
    ] {
        if !ptr.is_null() {
            *ptr = v;
        }
    }
    if !last_write.is_null() {
        *last_write = stamp;
    }
    ERROR_SUCCESS
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn reg_query_info_key_a(
    handle: usize,
    class: *mut u8,
    class_size: *mut u32,
    _reserved: *mut u32,
    sub_keys: *mut u32,
    max_sub_key_len: *mut u32,
    max_class_len: *mut u32,
    values: *mut u32,
    max_value_name_len: *mut u32,
    max_value_len: *mut u32,
    security: *mut u32,
    last_write: *mut u64,
) -> i32 {
    if !class.is_null() && !class_size.is_null() && *class_size > 0 {
        *class = 0;
    }
    query_info_impl(
        handle,
        class_size,
        sub_keys,
        max_sub_key_len,
        max_class_len,
        values,
        max_value_name_len,
        max_value_len,
        security,
        last_write,
    )
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn reg_query_info_key_w(
    handle: usize,
    class: *mut u16,
    class_size: *mut u32,
    _reserved: *mut u32,
    sub_keys: *mut u32,
    max_sub_key_len: *mut u32,
    max_class_len: *mut u32,
    values: *mut u32,
    max_value_name_len: *mut u32,
    max_value_len: *mut u32,
    security: *mut u32,
    last_write: *mut u64,
) -> i32 {
    if !class.is_null() && !class_size.is_null() && *class_size > 0 {
        *class = 0;
    }
    query_info_impl(
        handle,
        class_size,
        sub_keys,
        max_sub_key_len,
        max_class_len,
        values,
        max_value_name_len,
        max_value_len,
        security,
        last_write,
    )
}

// ── Fonctions privées Winlux (regedit) ─────────────────────────────

/// Importe un fichier .reg (chemin Windows ou Linux). Renvoie le nombre
/// d'opérations appliquées, ou -1 si le fichier est illisible ou invalide.
pub unsafe extern "win64" fn winlux_reg_import_a(path: *const u8) -> i32 {
    let path = crate::paths::to_linux(&text_a(path));
    match reg::regfile::import_file(&path) {
        Ok(n) => n.min(i32::MAX as usize) as i32,
        Err(e) => {
            crate::log_warn!("registry", "import de {} impossible : {e}", path.display());
            -1
        }
    }
}

/// Exporte une clé (ou tout le registre si `key` est vide) en .reg
/// version 5 (UTF-16). Renvoie le nombre de clés écrites, ou -1.
pub unsafe extern "win64" fn winlux_reg_export_a(path: *const u8, key: *const u8) -> i32 {
    let path = crate::paths::to_linux(&text_a(path));
    let key = text_a(key);
    let key = if key.is_empty() {
        None
    } else {
        match reg::canonical(&key) {
            Some(k) => Some(k),
            None => return -1,
        }
    };
    match reg::regfile::export_file(&path, key.as_deref()) {
        Ok(n) => n.min(i32::MAX as usize) as i32,
        Err(e) => {
            crate::log_warn!(
                "registry",
                "export vers {} impossible : {e}",
                path.display()
            );
            -1
        }
    }
}

/// Renomme une valeur (regedit). 0 si réussi, sinon code d'erreur Win32.
pub unsafe extern "win64" fn winlux_reg_rename_value_a(
    handle: usize,
    old: *const u8,
    new: *const u8,
) -> i32 {
    let path = match reg::handle_path(handle) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let (old, new) = (text_a(old), text_a(new));
    status(reg::write(|d| d.rename_value(&path, &old, &new)))
}

/// Renomme une clé (regedit, version ANSI de RegRenameKey).
pub unsafe extern "win64" fn winlux_reg_rename_key_a(
    handle: usize,
    sub: *const u8,
    new_name: *const u8,
) -> i32 {
    let path = match sub_path(handle, &text_a(sub)) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let new_name = text_a(new_name);
    status(reg::write(|d| d.rename_key(&path, &new_name)))
}

// ── Accès internes (ntdll, msiexec, COM, services) ─────────────────

/// The local Linux identity owns HKCU; this is not Windows impersonation.
pub(crate) fn native_current_user_key_path() -> String {
    format!("\\Registry\\User\\S-1-22-1-{}", unsafe { libc::geteuid() })
}

fn native_key_path(root: usize, name: &str) -> Result<String, i32> {
    let (hive, path) = if name.starts_with('\\') {
        let lower = name.to_lowercase();
        let current = native_current_user_key_path().to_lowercase();
        if lower == current || lower.starts_with(&(current.clone() + "\\")) {
            (reg::HKEY_CURRENT_USER, &name[current.len()..])
        } else if lower == "\\registry\\machine" || lower.starts_with("\\registry\\machine\\") {
            (reg::HKEY_LOCAL_MACHINE, &name[17..])
        } else if lower == "\\registry\\user" || lower.starts_with("\\registry\\user\\") {
            (reg::HKEY_USERS, &name[14..])
        } else {
            return Err(ERROR_FILE_NOT_FOUND);
        }
    } else {
        (root, name)
    };
    sub_path(hive, path)
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
    match native_key_path(root, name).and_then(|p| reg::open(&p)) {
        Ok(h) => {
            *result = h;
            ERROR_SUCCESS
        }
        Err(e) => e,
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
    match native_key_path(root, name).and_then(|p| reg::create(&p)) {
        Ok((h, created)) => {
            *result = h;
            if !disposition.is_null() {
                *disposition = if created {
                    REG_CREATED_NEW_KEY
                } else {
                    REG_OPENED_EXISTING_KEY
                };
            }
            ERROR_SUCCESS
        }
        Err(e) => e,
    }
}

pub(crate) fn close_native_key(handle: usize) -> bool {
    reg::close_handle(handle)
}

pub(crate) unsafe fn query_native_value(handle: usize, name: &str) -> Result<(u32, Vec<u8>), i32> {
    let path = reg::handle_path(handle)?;
    let v = get_value(&path, name).map_err(|e| {
        if e == ERROR_INVALID_HANDLE {
            ERROR_FILE_NOT_FOUND
        } else {
            e
        }
    })?;
    Ok((v.kind, v.bytes))
}

/// Octets bruts d'une valeur (chemin `hklm\…`, `hkcr\…`, `HKEY_…\…`).
pub fn read_registry_value_internal(key_path: &str, value_name: &str) -> Option<Vec<u8>> {
    let path = reg::canonical(key_path)?;
    reg::read(|d| {
        d.get(&path, value_name)
            .map(|v| v.bytes.clone())
            .ok_or(ERROR_FILE_NOT_FOUND)
    })
    .ok()
}

/// Modification du registre faite par un outil interne (msiexec).
pub enum RegEdit {
    /// Crée la clé et ses parents.
    Key(String),
    Set {
        key: String,
        name: String,
        kind: u32,
        bytes: Vec<u8>,
    },
    DeleteValue {
        key: String,
        name: String,
    },
    /// Supprime la clé et toutes ses sous-clés.
    DeleteTree(String),
}

/// Applique une série de modifications en une seule écriture du registre.
/// Les chemins sont de la forme `hklm\\software\\...` (casse indifférente).
pub fn apply_registry_edits(edits: &[RegEdit]) -> bool {
    reg::write(|d| {
        for edit in edits {
            let path = |k: &str| reg::canonical(k).ok_or(ERROR_INVALID_PARAMETER);
            match edit {
                RegEdit::Key(key) => {
                    d.ensure(&path(key)?);
                }
                RegEdit::Set {
                    key,
                    name,
                    kind,
                    bytes,
                } => d.set(&path(key)?, name, *kind, bytes.clone()),
                RegEdit::DeleteValue { key, name } => {
                    d.delete_value(&path(key)?, name);
                }
                RegEdit::DeleteTree(key) => {
                    d.delete_tree(&path(key)?);
                }
            }
        }
        Ok(())
    })
    .is_ok()
}

/// Vrai si la clé existe et n'a ni valeur ni sous-clé.
pub fn registry_key_is_empty(key: &str) -> bool {
    let Some(key) = reg::canonical(key) else {
        return false;
    };
    reg::read(|d| Ok(d.exists(&key) && d.values(&key).is_empty() && !d.has_subkeys(&key)))
        .unwrap_or(false)
}

/// Vrai si la clé existe (chemin canonique : hklm\…, HKEY_CURRENT_USER\…).
pub fn registry_key_exists(key: &str) -> bool {
    let Some(key) = reg::canonical(key) else {
        return false;
    };
    reg::read(|d| Ok(d.exists(&key))).unwrap_or(false)
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
     "RegOpenKeyExA"=>fn reg_open_key_ex_a,"RegOpenKeyExW"=>fn reg_open_key_ex_w,
     "RegOpenKeyA"=>fn reg_open_key_a,"RegOpenKeyW"=>fn reg_open_key_w,
     "RegCreateKeyExA"=>fn reg_create_key_ex_a,"RegCreateKeyExW"=>fn reg_create_key_ex_w,
     "RegCreateKeyA"=>fn reg_create_key_a,"RegCreateKeyW"=>fn reg_create_key_w,
     "RegOpenCurrentUser"=>fn reg_open_current_user,"RegOpenUserClassesRoot"=>fn reg_open_user_classes_root,
     "RegCloseKey"=>fn reg_close_key,"RegFlushKey"=>fn reg_flush_key,
     "RegDisablePredefinedCache"=>fn reg_success_stub,"RegDisablePredefinedCacheEx"=>fn reg_success_stub,
     "RegSetValueExA"=>fn reg_set_value_ex_a,"RegSetValueExW"=>fn reg_set_value_ex_w,
     "RegSetValueA"=>fn reg_set_value_a,"RegSetValueW"=>fn reg_set_value_w,
     "RegSetKeyValueA"=>fn reg_set_key_value_a,"RegSetKeyValueW"=>fn reg_set_key_value_w,
     "RegQueryValueExA"=>fn reg_query_value_ex_a,"RegQueryValueExW"=>fn reg_query_value_ex_w,
     "RegQueryValueA"=>fn reg_query_value_a,"RegQueryValueW"=>fn reg_query_value_w,
     "RegGetValueA"=>fn reg_get_value_a,"RegGetValueW"=>fn reg_get_value_w,
     "RegLoadMUIStringW"=>fn reg_load_mui_string_w,
     "RegDeleteValueA"=>fn reg_delete_value_a,"RegDeleteValueW"=>fn reg_delete_value_w,
     "RegDeleteKeyValueA"=>fn reg_delete_key_value_a,"RegDeleteKeyValueW"=>fn reg_delete_key_value_w,
     "RegDeleteKeyA"=>fn reg_delete_key_a,"RegDeleteKeyW"=>fn reg_delete_key_w,
     "RegDeleteKeyExA"=>fn reg_delete_key_ex_a,"RegDeleteKeyExW"=>fn reg_delete_key_ex_w,
     "RegDeleteTreeA"=>fn reg_delete_tree_a,"RegDeleteTreeW"=>fn reg_delete_tree_w,
     "RegCopyTreeA"=>fn reg_copy_tree_a,"RegCopyTreeW"=>fn reg_copy_tree_w,
     "RegRenameKey"=>fn reg_rename_key,"RegNotifyChangeKeyValue"=>fn reg_notify_change_key_value,
     "RegEnumKeyExA"=>fn reg_enum_key_ex_a,"RegEnumKeyExW"=>fn reg_enum_key_ex_w,
     "RegEnumKeyA"=>fn reg_enum_key_a,"RegEnumKeyW"=>fn reg_enum_key_w,
     "RegEnumValueA"=>fn reg_enum_value_a,"RegEnumValueW"=>fn reg_enum_value_w,
     "RegQueryInfoKeyA"=>fn reg_query_info_key_a,"RegQueryInfoKeyW"=>fn reg_query_info_key_w,
     "WinluxRegImportA"=>fn winlux_reg_import_a,"WinluxRegExportA"=>fn winlux_reg_export_a,
     "WinluxRegRenameValueA"=>fn winlux_reg_rename_value_a,"WinluxRegRenameKeyA"=>fn winlux_reg_rename_key_a,
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
