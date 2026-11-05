//! DDEML (user32 : fonctions `Dde*`, d'après Microsoft Learn).
//!
//! Winlux n'a pas encore d'échanges DDE entre processus. Une instance
//! s'initialise, crée ses chaînes et déclare ses services ; une conversation
//! ne s'établit jamais (DMLERR_NO_CONV_ESTABLISHED). C'est ce que voient les
//! programmes wxWidgets qui cherchent une instance déjà ouverte : ils
//! deviennent la première instance.

use super::{lock, wstr};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

const DMLERR_NO_ERROR: u32 = 0;
const DMLERR_INVALIDPARAMETER: u32 = 0x4006;
const DMLERR_NO_CONV_ESTABLISHED: u32 = 0x400A;

static NEXT_INST: AtomicU32 = AtomicU32::new(1);
static LAST_ERROR: AtomicU32 = AtomicU32::new(DMLERR_NO_ERROR);

/// Poignées de chaînes (HSZ) : texte et compte de références.
fn strings() -> &'static Mutex<(usize, HashMap<usize, (String, u32)>)> {
    static S: OnceLock<Mutex<(usize, HashMap<usize, (String, u32)>)>> = OnceLock::new();
    S.get_or_init(|| Mutex::new((0xd0e0_0000, HashMap::new())))
}

unsafe extern "win64" fn dde_initialize(inst: *mut u32, _cb: usize, _cmd: u32, _res: u32) -> u32 {
    if inst.is_null() {
        return DMLERR_INVALIDPARAMETER;
    }
    if *inst == 0 {
        *inst = NEXT_INST.fetch_add(1, Ordering::Relaxed);
    }
    DMLERR_NO_ERROR
}

unsafe extern "win64" fn dde_uninitialize(_inst: u32) -> i32 {
    1
}

fn new_string(s: String) -> usize {
    if s.is_empty() {
        return 0;
    }
    let mut t = lock(strings());
    // Même texte (sans casse) : même poignée, comme l'atome sous-jacent.
    if let Some((&h, v)) = t.1.iter_mut().find(|(_, v)| v.0.eq_ignore_ascii_case(&s)) {
        v.1 += 1;
        return h;
    }
    t.0 += 4;
    let h = t.0;
    t.1.insert(h, (s, 1));
    h
}

unsafe extern "win64" fn create_string_w(_inst: u32, s: *const u16, _cp: i32) -> usize {
    if s.is_null() {
        return 0;
    }
    new_string(wstr(s))
}

unsafe extern "win64" fn create_string_a(_inst: u32, s: *const u8, _cp: i32) -> usize {
    if s.is_null() {
        return 0;
    }
    new_string(super::cstr(s))
}

unsafe extern "win64" fn free_string(_inst: u32, h: usize) -> i32 {
    let mut t = lock(strings());
    match t.1.get_mut(&h) {
        Some(v) if v.1 > 1 => {
            v.1 -= 1;
            1
        }
        Some(_) => {
            t.1.remove(&h);
            1
        }
        None => 0,
    }
}

unsafe extern "win64" fn keep_string(_inst: u32, h: usize) -> i32 {
    match lock(strings()).1.get_mut(&h) {
        Some(v) => {
            v.1 += 1;
            1
        }
        None => 0,
    }
}

fn text_of(h: usize) -> Option<String> {
    lock(strings()).1.get(&h).map(|v| v.0.clone())
}

/// DdeQueryString : longueur sans le zéro final ; copie si un tampon est donné.
unsafe extern "win64" fn query_string_w(
    _inst: u32,
    h: usize,
    buf: *mut u16,
    cch: u32,
    _cp: i32,
) -> u32 {
    let Some(s) = text_of(h) else {
        return 0;
    };
    let w: Vec<u16> = s.encode_utf16().collect();
    if buf.is_null() {
        return w.len() as u32;
    }
    if cch == 0 {
        return 0;
    }
    let n = w.len().min(cch as usize - 1);
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, n);
    *buf.add(n) = 0;
    n as u32
}

unsafe extern "win64" fn query_string_a(
    _inst: u32,
    h: usize,
    buf: *mut u8,
    cch: u32,
    _cp: i32,
) -> u32 {
    let Some(s) = text_of(h) else {
        return 0;
    };
    let b = super::encode_ansi(&s);
    if buf.is_null() {
        return b.len() as u32;
    }
    if cch == 0 {
        return 0;
    }
    let n = b.len().min(cch as usize - 1);
    std::ptr::copy_nonoverlapping(b.as_ptr(), buf, n);
    *buf.add(n) = 0;
    n as u32
}

unsafe extern "win64" fn cmp_string(a: usize, b: usize) -> i32 {
    match (text_of(a), text_of(b)) {
        (Some(x), Some(y)) => match x.to_lowercase().cmp(&y.to_lowercase()) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        },
        _ => (a as isize - b as isize).signum() as i32,
    }
}

/// DdeNameService : le service est « déclaré » (rien ne s'y connectera).
unsafe extern "win64" fn name_service(_inst: u32, _hsz1: usize, _hsz2: usize, _cmd: u32) -> usize {
    1
}

unsafe extern "win64" fn connect(_inst: u32, _service: usize, _topic: usize, _ctx: usize) -> usize {
    LAST_ERROR.store(DMLERR_NO_CONV_ESTABLISHED, Ordering::Relaxed);
    0
}

unsafe extern "win64" fn get_last_error(_inst: u32) -> u32 {
    LAST_ERROR.swap(DMLERR_NO_ERROR, Ordering::Relaxed)
}

unsafe extern "win64" fn ok(_a: usize, _b: usize, _c: usize) -> i32 {
    1
}

unsafe extern "win64" fn zero(_a: usize, _b: usize, _c: usize, _d: usize) -> usize {
    LAST_ERROR.store(DMLERR_NO_CONV_ESTABLISHED, Ordering::Relaxed);
    0
}

unsafe extern "win64" fn get_data(_h: usize, _dst: *mut u8, _cb: u32, _off: u32) -> u32 {
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "DdeInitializeW" => fn dde_initialize,
        "DdeInitializeA" => fn dde_initialize,
        "DdeUninitialize" => fn dde_uninitialize,
        "DdeCreateStringHandleW" => fn create_string_w,
        "DdeCreateStringHandleA" => fn create_string_a,
        "DdeFreeStringHandle" => fn free_string,
        "DdeKeepStringHandle" => fn keep_string,
        "DdeQueryStringW" => fn query_string_w,
        "DdeQueryStringA" => fn query_string_a,
        "DdeCmpStringHandles" => fn cmp_string,
        "DdeNameService" => fn name_service,
        "DdeConnect" => fn connect,
        "DdeConnectList" => fn connect,
        "DdeGetLastError" => fn get_last_error,
        "DdeDisconnect" => fn ok,
        "DdeDisconnectList" => fn ok,
        "DdePostAdvise" => fn ok,
        "DdeFreeDataHandle" => fn ok,
        "DdeAbandonTransaction" => fn ok,
        "DdeEnableCallback" => fn ok,
        "DdeClientTransaction" => fn zero,
        "DdeCreateDataHandle" => fn zero,
        "DdeAddData" => fn zero,
        "DdeAccessData" => fn zero,
        "DdeUnaccessData" => fn ok,
        "DdeGetData" => fn get_data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaines_et_conversation_absente() {
        unsafe {
            let mut inst = 0u32;
            assert_eq!(dde_initialize(&mut inst, 0, 0, 0), DMLERR_NO_ERROR);
            assert_ne!(inst, 0);
            let s: Vec<u16> = "Audacity\0".encode_utf16().collect();
            let h = create_string_w(inst, s.as_ptr(), 1200);
            let s2: Vec<u16> = "AUDACITY\0".encode_utf16().collect();
            assert_eq!(create_string_w(inst, s2.as_ptr(), 1200), h);
            let mut buf = [0u16; 16];
            assert_eq!(query_string_w(inst, h, buf.as_mut_ptr(), 16, 1200), 8);
            assert_eq!(wstr(buf.as_ptr()), "Audacity");
            assert_eq!(connect(inst, h, h, 0), 0);
            assert_eq!(get_last_error(inst), DMLERR_NO_CONV_ESTABLISHED);
            assert_eq!(free_string(inst, h), 1);
            assert_eq!(free_string(inst, h), 1);
            assert_eq!(free_string(inst, h), 0);
        }
    }
}
