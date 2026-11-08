//! CreateURLMoniker (urlmon.dll) : un moniker d'URL minimal.
//!
//! Le moniker garde son URL ; il la rend par GetDisplayName, se présente
//! comme MKSYS_URLMONIKER et sait se lier à un IStream (BindToStorage,
//! BindToObject avec IID_IStream) en téléchargeant l'URL par le moteur de
//! WinINet de Winlux. Le reste de IMoniker (composition, comparaison,
//! persistance) rend E_NOTIMPL. WinSCP l'importe (#687).
//!
//! Référence : documentation publique d'urlmon (CreateURLMoniker, IMoniker).

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
const MK_E_SYNTAX: i32 = 0x8004_01E4u32 as i32;
const MKSYS_URLMONIKER: u32 = 6;

type Guid = [u8; 16];

const fn ole(d1: u32) -> Guid {
    let b = d1.to_le_bytes();
    [
        b[0], b[1], b[2], b[3], 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46,
    ]
}
const IID_IUNKNOWN: Guid = ole(0);
const IID_ISTREAM: Guid = ole(0xC);
const IID_IMONIKER: Guid = ole(0xF);
const IID_IPERSISTSTREAM: Guid = ole(0x109);
const IID_IPERSIST: Guid = ole(0x10C);
/// CLSID_StdURLMoniker {79EAC9E0-BAF9-11CE-8C82-00AA004BA90B}.
const CLSID_STD_URL_MONIKER: Guid = [
    0xE0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B,
];

#[repr(C)]
struct UrlMoniker {
    vtbl: *const [usize; 23],
    refs: AtomicU32,
    url: String,
}

type This = *mut UrlMoniker;

unsafe extern "win64" fn query_interface(
    this: This,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || iid.is_null() {
        return E_POINTER;
    }
    if [IID_IUNKNOWN, IID_IMONIKER, IID_IPERSISTSTREAM, IID_IPERSIST].contains(&*iid) {
        add_ref(this);
        *out = this.cast();
        S_OK
    } else {
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
}
unsafe extern "win64" fn add_ref(this: This) -> u32 {
    (*this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn release(this: This) -> u32 {
    let left = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if left == 0 {
        drop(Box::from_raw(this));
    }
    left
}
unsafe extern "win64" fn get_class_id(_this: This, out: *mut Guid) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = CLSID_STD_URL_MONIKER;
    S_OK
}
unsafe extern "win64" fn is_dirty(_this: This) -> i32 {
    S_FALSE
}
unsafe extern "win64" fn not_impl1(_this: This, _a: usize) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn not_impl2(_this: This, _a: usize, _b: usize) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn not_impl3(_this: This, _a: usize, _b: usize, _c: usize) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn not_impl4(_this: This, _a: usize, _b: usize, _c: usize, _d: usize) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn not_impl5(
    _this: This,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
    _e: usize,
) -> i32 {
    E_NOTIMPL
}

/// Télécharge l'URL dans un fichier temporaire et l'ouvre en IStream.
fn download_stream(url: &str) -> Result<*mut c_void, i32> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let target = format!("C:\\AppData\\Local\\Temp\\winlux-moniker-{stamp:x}");
    let linux = crate::paths::to_linux(&target);
    super::wininet::download(url, &linux).map_err(|e| e as i32)?;
    let stream = super::shlwapi_reg::file_stream(&target, 0, false);
    let _ = std::fs::remove_file(&linux);
    stream
}

unsafe extern "win64" fn bind_to_storage(
    this: This,
    _bind_ctx: usize,
    _left: usize,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || iid.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if *iid != IID_ISTREAM {
        return E_NOINTERFACE;
    }
    match download_stream(&(*this).url) {
        Ok(s) => {
            *out = s;
            S_OK
        }
        Err(hr) => hr,
    }
}

unsafe extern "win64" fn is_equal(this: This, other: *mut c_void) -> i32 {
    if other.is_null() {
        return E_INVALIDARG;
    }
    if std::ptr::eq(other.cast::<UrlMoniker>(), this) {
        S_OK
    } else {
        S_FALSE
    }
}

unsafe extern "win64" fn hash(this: This, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    // FNV-1a sur l'URL : stable d'un appel à l'autre.
    let mut h: u32 = 0x811C_9DC5;
    for b in (*this).url.bytes() {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    *out = h;
    S_OK
}

unsafe extern "win64" fn get_display_name(
    this: This,
    _bind_ctx: usize,
    _left: usize,
    out: *mut *mut u16,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let text: Vec<u16> = (*this)
        .url
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let p = super::ole32::co_task_mem_alloc(text.len() * 2) as *mut u16;
    if p.is_null() {
        *out = std::ptr::null_mut();
        return E_OUTOFMEMORY;
    }
    std::ptr::copy_nonoverlapping(text.as_ptr(), p, text.len());
    *out = p;
    S_OK
}

unsafe extern "win64" fn is_system_moniker(_this: This, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = MKSYS_URLMONIKER;
    S_OK
}

fn vtbl() -> &'static [usize; 23] {
    static VTBL: std::sync::OnceLock<[usize; 23]> = std::sync::OnceLock::new();
    VTBL.get_or_init(|| {
        [
            query_interface as *const () as usize,
            add_ref as *const () as usize,
            release as *const () as usize,
            get_class_id as *const () as usize,    // IPersist
            is_dirty as *const () as usize,        // IPersistStream
            not_impl1 as *const () as usize,       // Load
            not_impl2 as *const () as usize,       // Save
            not_impl1 as *const () as usize,       // GetSizeMax
            bind_to_storage as *const () as usize, // BindToObject : un IStream aussi
            bind_to_storage as *const () as usize, // BindToStorage
            not_impl4 as *const () as usize,       // Reduce
            not_impl3 as *const () as usize,       // ComposeWith
            not_impl2 as *const () as usize,       // Enum
            is_equal as *const () as usize,
            hash as *const () as usize,
            not_impl3 as *const () as usize, // IsRunning
            not_impl3 as *const () as usize, // GetTimeOfLastChange
            not_impl1 as *const () as usize, // Inverse
            not_impl2 as *const () as usize, // CommonPrefixWith
            not_impl2 as *const () as usize, // RelativePathTo
            get_display_name as *const () as usize,
            not_impl5 as *const () as usize, // ParseDisplayName
            is_system_moniker as *const () as usize,
        ]
    })
}

/// URL complète : `url` tel quel s'il a un schéma, sinon refusée (le
/// moniker de contexte n'est pas composé).
fn absolute(url: &str) -> Option<String> {
    let scheme = url.split_once(':')?.0;
    (scheme.len() > 1
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)))
    .then(|| url.to_string())
}

pub unsafe extern "win64" fn create_url_moniker(
    _context: usize,
    url: *const u16,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if url.is_null() {
        return E_INVALIDARG;
    }
    let Some(url) = absolute(&super::wstr(url)) else {
        return MK_E_SYNTAX;
    };
    *out = Box::into_raw(Box::new(UrlMoniker {
        vtbl: vtbl(),
        refs: AtomicU32::new(1),
        url,
    }))
    .cast();
    S_OK
}

pub unsafe extern "win64" fn create_url_moniker_ex(
    context: usize,
    url: *const u16,
    out: *mut *mut c_void,
    _flags: u32,
) -> i32 {
    create_url_moniker(context, url, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn call<F: Copy>(m: *mut c_void, slot: usize) -> F {
        let vtbl = *(m as *const *const usize);
        std::mem::transmute_copy(&*vtbl.add(slot))
    }

    #[test]
    fn moniker_rend_son_url() {
        unsafe {
            let url: Vec<u16> = "https://winscp.net/updates"
                .encode_utf16()
                .chain([0])
                .collect();
            let mut m = std::ptr::null_mut();
            assert_eq!(create_url_moniker(0, url.as_ptr(), &mut m), S_OK);
            let name: unsafe extern "win64" fn(*mut c_void, usize, usize, *mut *mut u16) -> i32 =
                call(m, 20);
            let mut text = std::ptr::null_mut();
            assert_eq!(name(m, 0, 0, &mut text), S_OK);
            assert_eq!(super::super::wstr(text), "https://winscp.net/updates");
            super::super::ole32::co_task_mem_free(text.cast());
            let sys: unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32 = call(m, 22);
            let mut kind = 0;
            assert_eq!(sys(m, &mut kind), S_OK);
            assert_eq!(kind, MKSYS_URLMONIKER);
            let rel: unsafe extern "win64" fn(*mut c_void) -> u32 = call(m, 2);
            assert_eq!(rel(m), 0);
            let bad: Vec<u16> = "pas une url".encode_utf16().chain([0]).collect();
            assert_eq!(create_url_moniker(0, bad.as_ptr(), &mut m), MK_E_SYNTAX);
            assert!(m.is_null());
        }
    }
}
