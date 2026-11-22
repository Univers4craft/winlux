//! En-têtes HTTP d'une navigation WebView2 (#2964) :
//! `ICoreWebView2NavigationStartingEventArgs::get_RequestHeaders` rend un
//! `ICoreWebView2HttpRequestHeaders`, et `GetHeaders`/`GetIterator` un
//! `ICoreWebView2HttpHeadersCollectionIterator`.
//!
//! Ce que dit l'IDL public du SDK WebView2 (WebView2.idl 1.0.2849.39,
//! licence BSD) :
//! - les en-têtes de `NavigationStarting` servent à inspecter la requête ;
//!   « It is possible to modify the HTTP request from a
//!   `WebResourceRequested` event, but not from a `NavigationStarting`
//!   event » ;
//! - `GetCurrentHeader` échoue quand `MoveNext` a rendu FALSE ;
//!   `HasCurrentHeader` vaut FALSE sur une collection vide ou épuisée ;
//! - les chaînes rendues sont libérées par l'appelant (`CoTaskMemFree`).
//!
//! Choix de Winlux là où l'IDL se tait (inférences nommées) :
//! - les en-têtes sont ceux que WebKit porte sur la requête au moment de
//!   la décision de navigation, souvent aucun ; le moteur les transmet
//!   sous la forme `Nom: valeur`, une ligne par en-tête ;
//! - un nom se compare sans tenir compte de la casse (RFC 9110, §5.1) ;
//! - `GetHeader` d'un nom absent rend HRESULT_FROM_WIN32(ERROR_NOT_FOUND)
//!   et une chaîne nulle ; d'un nom répété, la première valeur ;
//! - `SetHeader` et `RemoveHeader` changent la copie que porte l'objet,
//!   relue ensuite par `GetHeader`, mais jamais la requête envoyée, comme
//!   le dit l'IDL pour `NavigationStarting` ;
//! - un itérateur parcourt une copie prise à sa création.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Mutex;

use super::comobj::{
    guid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, IID_IUNKNOWN, S_OK,
};

const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
/// HRESULT_FROM_WIN32(ERROR_NOT_FOUND).
const E_NOT_FOUND: i32 = 0x8007_0490u32 as i32;

/// ICoreWebView2HttpRequestHeaders.
pub(crate) const IID_REQUEST_HEADERS: Guid = guid(
    0xe86cac0e,
    0x5523,
    0x465c,
    [0xb5, 0x36, 0x8f, 0xb9, 0xfc, 0x8c, 0x8c, 0x60],
);
/// ICoreWebView2HttpHeadersCollectionIterator.
pub(crate) const IID_HEADERS_ITERATOR: Guid = guid(
    0x0702fc30,
    0xf43b,
    0x47bb,
    [0xab, 0x52, 0xa4, 0x2c, 0xb5, 0x52, 0xad, 0x9f],
);

type Headers = Vec<(String, String)>;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// En-têtes transmis par le moteur : `Nom: valeur`, une ligne chacun.
pub(crate) fn parse(text: &str) -> Headers {
    text.lines()
        .filter_map(|l| {
            let (n, v) = l.split_once(':')?;
            let n = n.trim();
            (!n.is_empty()).then(|| (n.to_string(), v.trim().to_string()))
        })
        .collect()
}

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

unsafe fn wide(p: *const u16) -> Option<String> {
    if p.is_null() {
        return None;
    }
    Some(super::wstr(p))
}

/// Chaîne allouée par CoTaskMemAlloc (l'application la libère).
unsafe fn co_string(s: &str, out: *mut *mut u16) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    let Some(alloc) = super::lookup("ole32", "CoTaskMemAlloc") else {
        *out = std::ptr::null_mut();
        return E_FAIL;
    };
    let alloc: unsafe extern "win64" fn(usize) -> *mut u16 = std::mem::transmute(alloc);
    let p = alloc(w.len() * 2);
    if p.is_null() {
        *out = std::ptr::null_mut();
        return 0x8007_000Eu32 as i32;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
    *out = p;
    S_OK
}

unsafe fn co_free(p: *mut u16) {
    if p.is_null() {
        return;
    }
    if let Some(free) = super::lookup("ole32", "CoTaskMemFree") {
        let free: unsafe extern "win64" fn(*mut u16) = std::mem::transmute(free);
        free(p);
    }
}

// ============================================================
//  ICoreWebView2HttpRequestHeaders
// ============================================================

#[repr(C)]
struct RequestHeaders {
    vt: &'static VTable<9>,
    refs: AtomicU32,
    items: Mutex<Headers>,
}

/// En-têtes d'une navigation (une référence).
pub(crate) fn new_request_headers(items: Headers) -> usize {
    Box::into_raw(Box::new(RequestHeaders {
        vt: &REQUEST_HEADERS_VTBL,
        refs: AtomicU32::new(1),
        items: Mutex::new(items),
    })) as usize
}

unsafe fn rh<'a>(this: usize) -> &'a RequestHeaders {
    &*(this as *const RequestHeaders)
}

unsafe extern "win64" fn rh_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    if iid != IID_REQUEST_HEADERS && iid != IID_IUNKNOWN {
        super::comobj::log_no_interface("ICoreWebView2HttpRequestHeaders", &iid as *const Guid);
        *out = 0;
        return E_NOINTERFACE;
    }
    rh(this).refs.fetch_add(1, Ordering::AcqRel);
    *out = this;
    S_OK
}
unsafe extern "win64" fn rh_add_ref(this: usize) -> u32 {
    rh(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn rh_release(this: usize) -> u32 {
    let n = rh(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this as *mut RequestHeaders));
    }
    n
}
unsafe extern "win64" fn rh_get_header(this: usize, name: *const u16, out: *mut *mut u16) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(name) = wide(name) else {
        *out = std::ptr::null_mut();
        return E_INVALIDARG;
    };
    let found = lock(&rh(this).items)
        .iter()
        .find(|(n, _)| same(n, &name))
        .map(|(_, v)| v.clone());
    match found {
        Some(v) => co_string(&v, out),
        None => {
            *out = std::ptr::null_mut();
            E_NOT_FOUND
        }
    }
}
unsafe extern "win64" fn rh_get_headers(this: usize, name: *const u16, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(name) = wide(name) else {
        *out = 0;
        return E_INVALIDARG;
    };
    let items: Headers = lock(&rh(this).items)
        .iter()
        .filter(|(n, _)| same(n, &name))
        .cloned()
        .collect();
    *out = new_iterator(items);
    S_OK
}
unsafe extern "win64" fn rh_contains(this: usize, name: *const u16, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(name) = wide(name) else {
        *out = 0;
        return E_INVALIDARG;
    };
    *out = lock(&rh(this).items).iter().any(|(n, _)| same(n, &name)) as i32;
    S_OK
}
unsafe extern "win64" fn rh_set_header(this: usize, name: *const u16, value: *const u16) -> i32 {
    let (Some(name), Some(value)) = (wide(name), wide(value)) else {
        return E_INVALIDARG;
    };
    if name.is_empty() {
        return E_INVALIDARG;
    }
    let mut items = lock(&rh(this).items);
    items.retain(|(n, _)| !same(n, &name));
    items.push((name, value));
    S_OK
}
unsafe extern "win64" fn rh_remove_header(this: usize, name: *const u16) -> i32 {
    let Some(name) = wide(name) else {
        return E_INVALIDARG;
    };
    lock(&rh(this).items).retain(|(n, _)| !same(n, &name));
    S_OK
}
unsafe extern "win64" fn rh_get_iterator(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_iterator(lock(&rh(this).items).clone());
    S_OK
}

static REQUEST_HEADERS_VTBL: VTable<9> = VTable([
    rh_qi as *const (),
    rh_add_ref as *const (),
    rh_release as *const (),
    rh_get_header as *const (),
    rh_get_headers as *const (),
    rh_contains as *const (),
    rh_set_header as *const (),
    rh_remove_header as *const (),
    rh_get_iterator as *const (),
]);

// ============================================================
//  ICoreWebView2HttpHeadersCollectionIterator
// ============================================================

#[repr(C)]
struct HeadersIter {
    vt: &'static VTable<6>,
    refs: AtomicU32,
    items: Headers,
    pos: AtomicUsize,
}

fn new_iterator(items: Headers) -> usize {
    Box::into_raw(Box::new(HeadersIter {
        vt: &ITERATOR_VTBL,
        refs: AtomicU32::new(1),
        items,
        pos: AtomicUsize::new(0),
    })) as usize
}

unsafe fn hi<'a>(this: usize) -> &'a HeadersIter {
    &*(this as *const HeadersIter)
}

unsafe extern "win64" fn hi_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    if iid != IID_HEADERS_ITERATOR && iid != IID_IUNKNOWN {
        super::comobj::log_no_interface(
            "ICoreWebView2HttpHeadersCollectionIterator",
            &iid as *const Guid,
        );
        *out = 0;
        return E_NOINTERFACE;
    }
    hi(this).refs.fetch_add(1, Ordering::AcqRel);
    *out = this;
    S_OK
}
unsafe extern "win64" fn hi_add_ref(this: usize) -> u32 {
    hi(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn hi_release(this: usize) -> u32 {
    let n = hi(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this as *mut HeadersIter));
    }
    n
}
unsafe extern "win64" fn hi_current(this: usize, name: *mut *mut u16, value: *mut *mut u16) -> i32 {
    if name.is_null() || value.is_null() {
        return E_POINTER;
    }
    let it = hi(this);
    let Some((n, v)) = it.items.get(it.pos.load(Ordering::Acquire)) else {
        *name = std::ptr::null_mut();
        *value = std::ptr::null_mut();
        return E_NOT_FOUND;
    };
    let hr = co_string(n, name);
    if hr != S_OK {
        *value = std::ptr::null_mut();
        return hr;
    }
    let hr = co_string(v, value);
    if hr != S_OK {
        co_free(*name);
        *name = std::ptr::null_mut();
    }
    hr
}
unsafe extern "win64" fn hi_has_current(this: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let it = hi(this);
    *out = (it.pos.load(Ordering::Acquire) < it.items.len()) as i32;
    S_OK
}
unsafe extern "win64" fn hi_move_next(this: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let it = hi(this);
    let len = it.items.len();
    let pos = (it.pos.load(Ordering::Acquire) + 1).min(len);
    it.pos.store(pos, Ordering::Release);
    *out = (pos < len) as i32;
    S_OK
}

static ITERATOR_VTBL: VTable<6> = VTable([
    hi_qi as *const (),
    hi_add_ref as *const (),
    hi_release as *const (),
    hi_current as *const (),
    hi_has_current as *const (),
    hi_move_next as *const (),
]);

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn take(p: *mut u16) -> String {
        let s = super::super::wstr(p);
        co_free(p);
        s
    }

    #[test]
    fn entetes_de_navigation_lus_2964() {
        assert_eq!(
            parse("Accept: text/html\nreferer: https://a/\nmauvaise ligne\n: vide\n"),
            vec![
                ("Accept".to_string(), "text/html".to_string()),
                ("referer".to_string(), "https://a/".to_string()),
            ]
        );
        unsafe {
            let h = new_request_headers(parse("Accept: text/html\nX-Deux: 1\nx-deux: 2"));
            let mut same_obj = 0usize;
            assert_eq!(rh_qi(h, IID_REQUEST_HEADERS.as_ptr(), &mut same_obj), S_OK);
            assert_eq!(same_obj, h);
            rh_release(same_obj);

            // GetHeader : casse ignorée, première valeur, absent refusé.
            let mut v = std::ptr::null_mut();
            assert_eq!(rh_get_header(h, w("accept").as_ptr(), &mut v), S_OK);
            assert_eq!(take(v), "text/html");
            assert_eq!(rh_get_header(h, w("X-DEUX").as_ptr(), &mut v), S_OK);
            assert_eq!(take(v), "1");
            assert_eq!(rh_get_header(h, w("Cookie").as_ptr(), &mut v), E_NOT_FOUND);
            assert!(v.is_null());
            let mut c = 9i32;
            assert_eq!(rh_contains(h, w("ACCEPT").as_ptr(), &mut c), S_OK);
            assert_eq!(c, 1);
            assert_eq!(rh_contains(h, w("Cookie").as_ptr(), &mut c), S_OK);
            assert_eq!(c, 0);

            // GetHeaders : les deux valeurs d'un nom répété.
            let mut it = 0usize;
            assert_eq!(rh_get_headers(h, w("x-deux").as_ptr(), &mut it), S_OK);
            let mut seen = Vec::new();
            loop {
                let mut has = 9i32;
                assert_eq!(hi_has_current(it, &mut has), S_OK);
                if has == 0 {
                    break;
                }
                let (mut n, mut val) = (std::ptr::null_mut(), std::ptr::null_mut());
                assert_eq!(hi_current(it, &mut n, &mut val), S_OK);
                seen.push((take(n), take(val)));
                let mut next = 9i32;
                assert_eq!(hi_move_next(it, &mut next), S_OK);
            }
            assert_eq!(
                seen,
                vec![
                    ("X-Deux".to_string(), "1".to_string()),
                    ("x-deux".to_string(), "2".to_string())
                ]
            );
            let (mut n, mut val) = (1 as *mut u16, 1 as *mut u16);
            assert_eq!(hi_current(it, &mut n, &mut val), E_NOT_FOUND);
            assert!(n.is_null() && val.is_null());
            let mut next = 9i32;
            assert_eq!(hi_move_next(it, &mut next), S_OK);
            assert_eq!(next, 0);
            assert_eq!(hi_release(it), 0);

            // SetHeader et RemoveHeader changent la copie de l'objet.
            assert_eq!(
                rh_set_header(h, w("X-DEUX").as_ptr(), w("3").as_ptr()),
                S_OK
            );
            assert_eq!(rh_get_header(h, w("x-deux").as_ptr(), &mut v), S_OK);
            assert_eq!(take(v), "3");
            assert_eq!(rh_remove_header(h, w("accept").as_ptr()), S_OK);
            let mut all = 0usize;
            assert_eq!(rh_get_iterator(h, &mut all), S_OK);
            let mut has = 0i32;
            assert_eq!(hi_has_current(all, &mut has), S_OK);
            assert_eq!(has, 1);
            let (mut n, mut val) = (std::ptr::null_mut(), std::ptr::null_mut());
            assert_eq!(hi_current(all, &mut n, &mut val), S_OK);
            assert_eq!(
                (take(n), take(val)),
                ("X-DEUX".to_string(), "3".to_string())
            );
            assert_eq!(hi_move_next(all, &mut has), S_OK);
            assert_eq!(has, 0);
            assert_eq!(hi_release(all), 0);

            // Collection vide : HasCurrentHeader FALSE d'emblée.
            let empty = new_request_headers(Vec::new());
            let mut e = 0usize;
            assert_eq!(rh_get_iterator(empty, &mut e), S_OK);
            assert_eq!(hi_has_current(e, &mut has), S_OK);
            assert_eq!(has, 0);
            hi_release(e);
            assert_eq!(rh_release(empty), 0);
            assert_eq!(rh_release(h), 0);
        }
    }
}
