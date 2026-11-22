//! URLOpenStreamW/A (urlmon.dll) : flux « poussé » depuis une URL (#2869).
//!
//! L'installeur de OneDrive l'appelle quand BITS lui manque. Les données
//! arrivent au client par IBindStatusCallback::OnDataAvailable, qui peut
//! annuler en rendant E_ABORT.
//!
//! Déroulé (le téléchargement passe par le moteur de WinINet de Winlux) :
//! OnStartBinding avec un IBinding, OnProgress(BINDSTATUS_BEGINDOWNLOADDATA),
//! téléchargement complet, OnProgress(BINDSTATUS_ENDDOWNLOADDATA), un seul
//! OnDataAvailable marqué BSCF_FIRSTDATANOTIFICATION |
//! BSCF_LASTDATANOTIFICATION | BSCF_DATAFULLYAVAILABLE avec un IStream
//! (TYMED_ISTREAM) sur toutes les données, puis OnStopBinding avec le
//! résultat, que la fonction rend aussi. L'appel ne rend la main qu'à la
//! fin.
//!
//! Provenance : documentation publique (URLOpenStream, IBindStatusCallback
//! et ses méthodes, énumération BSCF) ; ordre des méthodes des tables
//! virtuelles, valeurs BINDSTATUS, BSCF, TYMED et IID d'après windows-rs
//! (MIT). La page d'URLOpenStream ne nomme qu'OnDataAvailable : l'appel
//! d'OnStartBinding, OnProgress et OnStopBinding, et le retour une fois
//! le téléchargement fini, sont une inférence (comportement des liaisons
//! de monikers d'URL décrit par les pages d'IBindStatusCallback).

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

const S_OK: i32 = 0;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_ABORT: i32 = 0x8000_4004u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;

const BINDSTATUS_BEGINDOWNLOADDATA: u32 = 4;
const BINDSTATUS_ENDDOWNLOADDATA: u32 = 6;
const BSCF_FIRSTDATANOTIFICATION: u32 = 1;
const BSCF_LASTDATANOTIFICATION: u32 = 4;
const BSCF_DATAFULLYAVAILABLE: u32 = 8;
const TYMED_ISTREAM: u32 = 4;
const DVASPECT_CONTENT: u32 = 1;

// Emplacements des méthodes d'IBindStatusCallback.
const ON_START_BINDING: usize = 3;
const ON_PROGRESS: usize = 6;
const ON_STOP_BINDING: usize = 7;
const ON_DATA_AVAILABLE: usize = 9;

type Guid = [u8; 16];
/// IID_IUnknown.
const IID_IUNKNOWN: Guid = [0, 0, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
/// IID_IBinding {79EAC9C0-BAF9-11CE-8C82-00AA004BA90B}.
const IID_IBINDING: Guid = [
    0xC0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B,
];

#[repr(C)]
struct FormatEtc {
    cf_format: u16,
    ptd: usize,
    aspect: u32,
    lindex: i32,
    tymed: u32,
}

#[repr(C)]
struct StgMedium {
    tymed: u32,
    data: *mut c_void,
    unk_for_release: usize,
}

/// IBinding passé à OnStartBinding : Abort annule le téléchargement.
#[repr(C)]
struct Binding {
    vtbl: *const [usize; 9],
    refs: AtomicU32,
    aborted: AtomicBool,
}

unsafe extern "win64" fn binding_qi(
    this: *mut Binding,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || iid.is_null() {
        return E_POINTER;
    }
    if *iid == IID_IUNKNOWN || *iid == IID_IBINDING {
        binding_add_ref(this);
        *out = this.cast();
        return S_OK;
    }
    *out = std::ptr::null_mut();
    E_NOINTERFACE
}

unsafe extern "win64" fn binding_add_ref(this: *mut Binding) -> u32 {
    (*this).refs.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "win64" fn binding_release(this: *mut Binding) -> u32 {
    let n = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this));
    }
    n
}

unsafe extern "win64" fn binding_abort(this: *mut Binding) -> i32 {
    (*this).aborted.store(true, Ordering::Release);
    S_OK
}

/// Suspend et Resume : un téléchargement en cours ne se met pas en pause.
unsafe extern "win64" fn binding_not_impl(_this: *mut Binding) -> i32 {
    crate::trace!(
        "IBinding::Suspend/Resume : E_NOTIMPL ({}:{})",
        file!(),
        line!()
    );
    E_NOTIMPL
}

unsafe extern "win64" fn binding_set_priority(_this: *mut Binding, _p: i32) -> i32 {
    S_OK
}

unsafe extern "win64" fn binding_get_priority(_this: *mut Binding, p: *mut i32) -> i32 {
    if p.is_null() {
        return E_POINTER;
    }
    *p = 0; // THREAD_PRIORITY_NORMAL
    S_OK
}

/// GetBindResult : rien à rendre de plus que OnStopBinding.
unsafe extern "win64" fn binding_get_bind_result(
    _this: *mut Binding,
    _clsid: usize,
    _result: usize,
    _text: usize,
    _reserved: usize,
) -> i32 {
    crate::trace!(
        "IBinding::GetBindResult : E_NOTIMPL ({}:{})",
        file!(),
        line!()
    );
    E_NOTIMPL
}

fn binding_vtbl() -> &'static [usize; 9] {
    static V: std::sync::OnceLock<[usize; 9]> = std::sync::OnceLock::new();
    V.get_or_init(|| {
        [
            binding_qi as *const () as usize,
            binding_add_ref as *const () as usize,
            binding_release as *const () as usize,
            binding_abort as *const () as usize,
            binding_not_impl as *const () as usize,
            binding_not_impl as *const () as usize,
            binding_set_priority as *const () as usize,
            binding_get_priority as *const () as usize,
            binding_get_bind_result as *const () as usize,
        ]
    })
}

unsafe fn slot(obj: *mut c_void, n: usize) -> usize {
    *(*(obj as *const *const usize)).add(n)
}

unsafe fn com_release(obj: *mut c_void) {
    let f: unsafe extern "win64" fn(*mut c_void) -> u32 = std::mem::transmute(slot(obj, 2));
    f(obj);
}

unsafe fn on_progress(cb: *mut c_void, done: u32, total: u32, status: u32, url: &[u16]) -> i32 {
    let f: unsafe extern "win64" fn(*mut c_void, u32, u32, u32, *const u16) -> i32 =
        std::mem::transmute(slot(cb, ON_PROGRESS));
    f(cb, done, total, status, url.as_ptr())
}

/// Télécharge `url` dans un fichier temporaire, ouvert en IStream ; rend
/// aussi sa taille.
fn fetch(url: &str) -> Result<(*mut c_void, u64), i32> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let target = format!("C:\\AppData\\Local\\Temp\\winlux-flux-{stamp:x}");
    let linux = crate::paths::to_linux(&target);
    super::wininet::download(url, &linux).map_err(|e| e as i32)?;
    let size = std::fs::metadata(&linux).map(|m| m.len()).unwrap_or(0);
    let stream = super::shlwapi_reg::file_stream(&target, 0, false);
    let _ = std::fs::remove_file(&linux);
    stream.map(|s| (s, size))
}

unsafe fn open_stream(url: &str, cb: *mut c_void) -> i32 {
    if cb.is_null() {
        return match fetch(url) {
            Ok((s, _)) => {
                com_release(s);
                S_OK
            }
            Err(hr) => hr,
        };
    }
    let wide: Vec<u16> = url.encode_utf16().chain([0]).collect();
    let binding = Box::into_raw(Box::new(Binding {
        vtbl: binding_vtbl(),
        refs: AtomicU32::new(1),
        aborted: AtomicBool::new(false),
    }));
    let start: unsafe extern "win64" fn(*mut c_void, u32, *mut c_void) -> i32 =
        std::mem::transmute(slot(cb, ON_START_BINDING));
    let mut hr = if start(cb, 0, binding.cast()) < 0 {
        E_ABORT
    } else {
        S_OK
    };
    let aborted = |hr: i32| hr == E_ABORT || (*binding).aborted.load(Ordering::Acquire);
    if hr == S_OK && aborted(on_progress(cb, 0, 0, BINDSTATUS_BEGINDOWNLOADDATA, &wide)) {
        hr = E_ABORT;
    }
    if hr == S_OK {
        match fetch(url) {
            Ok((stream, size)) => {
                let size = size.min(u32::MAX as u64) as u32;
                if aborted(on_progress(
                    cb,
                    size,
                    size,
                    BINDSTATUS_ENDDOWNLOADDATA,
                    &wide,
                )) {
                    com_release(stream);
                    hr = E_ABORT;
                } else {
                    let format = FormatEtc {
                        cf_format: 0,
                        ptd: 0,
                        aspect: DVASPECT_CONTENT,
                        lindex: -1,
                        tymed: TYMED_ISTREAM,
                    };
                    // Le client libère le flux reçu (page d'OnDataAvailable).
                    let medium = StgMedium {
                        tymed: TYMED_ISTREAM,
                        data: stream,
                        unk_for_release: 0,
                    };
                    let data: unsafe extern "win64" fn(
                        *mut c_void,
                        u32,
                        u32,
                        *const FormatEtc,
                        *const StgMedium,
                    ) -> i32 = std::mem::transmute(slot(cb, ON_DATA_AVAILABLE));
                    let flags = BSCF_FIRSTDATANOTIFICATION
                        | BSCF_LASTDATANOTIFICATION
                        | BSCF_DATAFULLYAVAILABLE;
                    if aborted(data(cb, flags, size, &format, &medium)) {
                        hr = E_ABORT;
                    }
                }
            }
            Err(e) => hr = e,
        }
    }
    let stop: unsafe extern "win64" fn(*mut c_void, i32, *const u16) -> i32 =
        std::mem::transmute(slot(cb, ON_STOP_BINDING));
    let empty = [0u16];
    stop(cb, hr, empty.as_ptr());
    binding_release(binding);
    crate::trace!("URLOpenStream({url}) = {hr:#x}");
    hr
}

unsafe fn entry(url: Option<String>, cb: usize) -> i32 {
    let Some(url) = url else {
        return E_INVALIDARG;
    };
    if crate::wow32::active() && cb != 0 {
        // Un IBindStatusCallback x86 demande des appels vers le code 32
        // bits, que ce module ne fait pas encore.
        crate::trace!(
            "URLOpenStream({url}) x86 avec rappel : E_NOTIMPL ({}:{})",
            file!(),
            line!()
        );
        return E_NOTIMPL;
    }
    open_stream(&url, cb as *mut c_void)
}

pub unsafe extern "win64" fn url_open_stream_w(
    _caller: usize,
    url: *const u16,
    _reserved: u32,
    cb: usize,
) -> i32 {
    entry((!url.is_null()).then(|| super::wstr(url)), cb)
}

pub unsafe extern "win64" fn url_open_stream_a(
    _caller: usize,
    url: *const u8,
    _reserved: u32,
    cb: usize,
) -> i32 {
    entry((!url.is_null()).then(|| super::cstr(url)), cb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// IBindStatusCallback de test : note les appels et lit le flux.
    #[repr(C)]
    struct Rappel {
        vtbl: *const [usize; 11],
        appels: std::sync::Mutex<Vec<String>>,
        lu: std::sync::Mutex<Vec<u8>>,
        annule: bool,
    }

    unsafe extern "win64" fn r_qi(_t: *mut Rappel, _i: usize, o: *mut usize) -> i32 {
        *o = 0;
        E_NOINTERFACE
    }
    unsafe extern "win64" fn r_ref(_t: *mut Rappel) -> u32 {
        1
    }
    unsafe extern "win64" fn r_start(t: *mut Rappel, _r: u32, b: *mut c_void) -> i32 {
        (*t).appels
            .lock()
            .unwrap()
            .push(format!("start {}", !b.is_null()));
        S_OK
    }
    unsafe extern "win64" fn r_rien(_t: *mut Rappel, _a: usize) -> i32 {
        S_OK
    }
    unsafe extern "win64" fn r_progress(
        t: *mut Rappel,
        d: u32,
        _n: u32,
        s: u32,
        _u: *const u16,
    ) -> i32 {
        (*t).appels
            .lock()
            .unwrap()
            .push(format!("progress {s} {d}"));
        S_OK
    }
    unsafe extern "win64" fn r_stop(t: *mut Rappel, hr: i32, _e: *const u16) -> i32 {
        (*t).appels.lock().unwrap().push(format!("stop {hr:#x}"));
        S_OK
    }
    unsafe extern "win64" fn r_data(
        t: *mut Rappel,
        f: u32,
        n: u32,
        fmt: *const FormatEtc,
        m: *const StgMedium,
    ) -> i32 {
        (*t).appels
            .lock()
            .unwrap()
            .push(format!("data {f} {n} {} {}", (*fmt).tymed, (*m).tymed));
        let stream = (*m).data;
        let read: unsafe extern "win64" fn(*mut c_void, *mut u8, u32, *mut u32) -> i32 =
            std::mem::transmute(slot(stream, 3));
        let mut buf = vec![0u8; n as usize];
        let mut got = 0u32;
        read(stream, buf.as_mut_ptr(), n, &mut got);
        buf.truncate(got as usize);
        *(*t).lu.lock().unwrap() = buf;
        com_release(stream);
        if (*t).annule {
            E_ABORT
        } else {
            S_OK
        }
    }

    fn rappel(annule: bool) -> Rappel {
        static V: std::sync::OnceLock<[usize; 11]> = std::sync::OnceLock::new();
        Rappel {
            vtbl: V.get_or_init(|| {
                [
                    r_qi as *const () as usize,
                    r_ref as *const () as usize,
                    r_ref as *const () as usize,
                    r_start as *const () as usize,
                    r_rien as *const () as usize,
                    r_rien as *const () as usize,
                    r_progress as *const () as usize,
                    r_stop as *const () as usize,
                    r_rien as *const () as usize,
                    r_data as *const () as usize,
                    r_rien as *const () as usize,
                ]
            }),
            appels: std::sync::Mutex::new(Vec::new()),
            lu: std::sync::Mutex::new(Vec::new()),
            annule,
        }
    }

    /// Serveur HTTP local qui rend `corps` à chaque requête.
    fn serveur(corps: &'static [u8], requetes: usize) -> String {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in l.incoming().take(requetes) {
                let mut s = s.unwrap();
                let mut req = [0u8; 4096];
                let _ = s.read(&mut req);
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    corps.len()
                );
                let _ = s.write_all(corps);
            }
        });
        format!("http://127.0.0.1:{port}/manifeste.xml")
    }

    /// #2869 : les données arrivent par OnDataAvailable dans un IStream,
    /// encadrées par OnStartBinding, OnProgress et OnStopBinding(S_OK).
    #[test]
    fn flux_pousse_au_rappel_2869() {
        let _teb = crate::process::test_teb();
        let url: Vec<u16> = serveur(b"<manifeste/>", 1)
            .encode_utf16()
            .chain([0])
            .collect();
        let r = rappel(false);
        let hr = unsafe { url_open_stream_w(0, url.as_ptr(), 0, &r as *const Rappel as usize) };
        assert_eq!(hr, S_OK);
        assert_eq!(&*r.lu.lock().unwrap(), b"<manifeste/>");
        assert_eq!(
            *r.appels.lock().unwrap(),
            [
                "start true".to_string(),
                format!("progress {BINDSTATUS_BEGINDOWNLOADDATA} 0"),
                format!("progress {BINDSTATUS_ENDDOWNLOADDATA} 12"),
                format!("data {} 12 {TYMED_ISTREAM} {TYMED_ISTREAM}", 1 | 4 | 8),
                "stop 0x0".to_string(),
            ]
        );
    }

    /// #2869 : E_ABORT rendu par OnDataAvailable annule ; URL nulle :
    /// E_INVALIDARG.
    #[test]
    fn flux_annule_et_url_nulle_2869() {
        let _teb = crate::process::test_teb();
        let url: Vec<u16> = serveur(b"abc", 1).encode_utf16().chain([0]).collect();
        let r = rappel(true);
        let hr = unsafe { url_open_stream_w(0, url.as_ptr(), 0, &r as *const Rappel as usize) };
        assert_eq!(hr, E_ABORT);
        assert_eq!(
            r.appels.lock().unwrap().last().map(String::as_str),
            Some(format!("stop {:#x}", E_ABORT).as_str())
        );
        assert_eq!(
            unsafe { url_open_stream_w(0, std::ptr::null(), 0, 0) },
            E_INVALIDARG
        );
    }
}
