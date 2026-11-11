//! DLL de redistribuables et d'outils relevées dans le corpus de #26 (#1394).
//!
//! - `mfc42.dll` : MFC 4.2 de Visual C++ 6. signtool.exe (x86, livré par
//!   electron-winstaller) n'en importe que `operator new` (#823) et
//!   `operator delete` (#825), par numéro.
//! - `mssign32.dll` : signature Authenticode (SignerSign,
//!   SignerTimeStamp). Winlux ne signe pas de binaires : l'appel échoue
//!   proprement, avec un message dans le journal.
//! - `concrt140.dll` : runtime de concurrence de Visual C++.
//!   La msvcp140.dll de Microsoft livrée avec panda3d importe en différé
//!   `Concurrency::critical_section` et
//!   `Concurrency::details::_Condition_variable`.
//! - `wintypes.dll` (contrat `api-ms-win-ro-typeresolution-l1-1-0`) :
//!   résolution des types Windows Runtime. coreclr.dll de .NET Core 3.1
//!   l'importe en différé ; aucune métadonnée WinRT n'existe sous Winlux.

use super::msvcp140::{futex_wait, futex_wake};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

// ---------- mfc42.dll ----------

/// `operator new` de MFC 4.2 (cdecl). MFC lève CMemoryException en cas
/// d'échec ; ici, NULL, comme le `new` sans exception.
unsafe extern "win64" fn mfc_new(n: usize) -> *mut std::ffi::c_void {
    super::msvcrt::malloc(n.max(1))
}

unsafe extern "win64" fn mfc_delete(p: *mut std::ffi::c_void) {
    super::msvcrt::free(p)
}

pub fn mfc42_exports() -> super::Exports {
    crate::exports! {
        "#823" => fn mfc_new,
        "#825" => fn mfc_delete,
    }
}

// ---------- mssign32.dll ----------

const E_NOTIMPL: u32 = 0x8000_4001;

unsafe extern "win64" fn signer_sign(
    _subject: usize,
    _cert: usize,
    _sig: usize,
    _provider: usize,
    _http: usize,
    _request: usize,
    _sip: usize,
) -> u32 {
    crate::log_warn!(
        "mssign32",
        "SignerSign : Winlux ne signe pas de binaires Authenticode (#1394)"
    );
    E_NOTIMPL
}

unsafe extern "win64" fn signer_time_stamp(
    _subject: usize,
    _url: usize,
    _request: usize,
    _sip: usize,
) -> u32 {
    crate::log_warn!(
        "mssign32",
        "SignerTimeStamp : Winlux n'horodate pas de signature Authenticode (#1394)"
    );
    E_NOTIMPL
}

pub fn mssign32_exports() -> super::Exports {
    crate::exports! {
        "SignerSign" => fn signer_sign,
        "SignerSignEx" => fn signer_sign,
        "SignerTimeStamp" => fn signer_time_stamp,
        "SignerTimeStampEx" => fn signer_time_stamp,
    }
}

// ---------- concrt140.dll ----------
//
// L'objet C++ appartient au programme, et sa taille vient de concrt.h. Il
// fait au moins un pointeur : le premier mot pointe vers notre état,
// alloué par le constructeur et rendu par le destructeur.

struct Cs {
    /// 0 libre, 1 pris, 2 pris avec des threads en attente.
    word: AtomicU32,
}

struct Cv {
    seq: AtomicU32,
}

unsafe fn cs_of<'a>(this: *mut usize) -> &'a Cs {
    &*(this.read() as *const Cs)
}

unsafe extern "win64" fn cs_ctor(this: *mut usize) -> *mut usize {
    this.write(Box::into_raw(Box::new(Cs {
        word: AtomicU32::new(0),
    })) as usize);
    this
}

unsafe extern "win64" fn cs_dtor(this: *mut usize) {
    let p = this.read();
    if p != 0 {
        drop(Box::from_raw(p as *mut Cs));
        this.write(0);
    }
}

fn cs_lock_until(cs: &Cs, deadline: Option<Instant>) -> bool {
    if cs
        .word
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .is_ok()
    {
        return true;
    }
    loop {
        if cs.word.swap(2, Ordering::Acquire) == 0 {
            return true;
        }
        let left = match deadline {
            Some(d) => match d.checked_duration_since(Instant::now()) {
                Some(l) if !l.is_zero() => Some(l),
                _ => return false,
            },
            None => None,
        };
        unsafe { futex_wait(&cs.word, 2, left) };
    }
}

fn cs_release(cs: &Cs) {
    if cs.word.swap(0, Ordering::Release) == 2 {
        unsafe { futex_wake(&cs.word, 1) };
    }
}

unsafe extern "win64" fn cs_lock(this: *mut usize) {
    cs_lock_until(cs_of(this), None);
}

unsafe extern "win64" fn cs_try_lock(this: *mut usize) -> bool {
    cs_of(this)
        .word
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .is_ok()
}

unsafe extern "win64" fn cs_try_lock_for(this: *mut usize, ms: u32) -> bool {
    let deadline = Instant::now() + Duration::from_millis(ms as u64);
    cs_lock_until(cs_of(this), Some(deadline))
}

unsafe extern "win64" fn cs_unlock(this: *mut usize) {
    cs_release(cs_of(this));
}

unsafe extern "win64" fn cv_ctor(this: *mut usize) -> *mut usize {
    this.write(Box::into_raw(Box::new(Cv {
        seq: AtomicU32::new(0),
    })) as usize);
    this
}

unsafe extern "win64" fn cv_dtor(this: *mut usize) {
    let p = this.read();
    if p != 0 {
        drop(Box::from_raw(p as *mut Cv));
        this.write(0);
    }
}

/// Attend un signal, la section critique relâchée pendant l'attente et
/// reprise avant de rendre. Rend false si le délai a expiré.
unsafe fn cv_wait_impl(this: *mut usize, cs: *mut usize, timeout: Option<Duration>) -> bool {
    let cv = &*(this.read() as *const Cv);
    let seen = cv.seq.load(Ordering::Acquire);
    cs_release(cs_of(cs));
    let signaled = futex_wait(&cv.seq, seen, timeout);
    cs_lock_until(cs_of(cs), None);
    signaled || cv.seq.load(Ordering::Acquire) != seen
}

unsafe extern "win64" fn cv_wait(this: *mut usize, cs: *mut usize) {
    cv_wait_impl(this, cs, None);
}

/// INFINITE (0xFFFFFFFF) attend sans limite.
unsafe extern "win64" fn cv_wait_for(this: *mut usize, cs: *mut usize, ms: u32) -> bool {
    let timeout = (ms != u32::MAX).then(|| Duration::from_millis(ms as u64));
    cv_wait_impl(this, cs, timeout)
}

unsafe fn cv_notify(this: *mut usize, n: i32) {
    let cv = &*(this.read() as *const Cv);
    cv.seq.fetch_add(1, Ordering::Release);
    futex_wake(&cv.seq, n);
}

unsafe extern "win64" fn cv_notify_one(this: *mut usize) {
    cv_notify(this, 1);
}

unsafe extern "win64" fn cv_notify_all(this: *mut usize) {
    cv_notify(this, i32::MAX);
}

pub fn concrt140_exports() -> super::Exports {
    crate::exports! {
        "??0critical_section@Concurrency@@QEAA@XZ" => fn cs_ctor,
        "??1critical_section@Concurrency@@QEAA@XZ" => fn cs_dtor,
        "?lock@critical_section@Concurrency@@QEAAXXZ" => fn cs_lock,
        "?try_lock@critical_section@Concurrency@@QEAA_NXZ" => fn cs_try_lock,
        "?try_lock_for@critical_section@Concurrency@@QEAA_NI@Z" => fn cs_try_lock_for,
        "?unlock@critical_section@Concurrency@@QEAAXXZ" => fn cs_unlock,
        "??0_Condition_variable@details@Concurrency@@QEAA@XZ" => fn cv_ctor,
        "??1_Condition_variable@details@Concurrency@@QEAA@XZ" => fn cv_dtor,
        "?wait@_Condition_variable@details@Concurrency@@QEAAXAEAVcritical_section@3@@Z" => fn cv_wait,
        "?wait_for@_Condition_variable@details@Concurrency@@QEAA_NAEAVcritical_section@3@I@Z" => fn cv_wait_for,
        "?notify_one@_Condition_variable@details@Concurrency@@QEAAXXZ" => fn cv_notify_one,
        "?notify_all@_Condition_variable@details@Concurrency@@QEAAXXZ" => fn cv_notify_all,
    }
}

// ---------- wintypes.dll (api-ms-win-ro-typeresolution) ----------

const REGDB_E_CLASSNOTREG: u32 = 0x8004_0154;
const RO_E_METADATA_NAME_NOT_FOUND: u32 = 0x8000_000F;

/// RoParseTypeName(HSTRING, DWORD* parts, HSTRING** names) : aucune
/// métadonnée WinRT, comme une classe absente.
unsafe extern "win64" fn ro_parse_type_name(
    _name: usize,
    parts: *mut u32,
    names: *mut usize,
) -> u32 {
    if !parts.is_null() {
        *parts = 0;
    }
    if !names.is_null() {
        *names = 0;
    }
    crate::trace!("wintypes : RoParseTypeName sans métadonnées WinRT (#1394)");
    REGDB_E_CLASSNOTREG
}

unsafe extern "win64" fn ro_resolve_namespace(
    _name: usize,
    _dir: usize,
    _n: u32,
    _paths: usize,
    files_n: *mut u32,
    files: *mut usize,
    subs_n: *mut u32,
    subs: *mut usize,
) -> u32 {
    for (n, p) in [(files_n, files), (subs_n, subs)] {
        if !n.is_null() {
            *n = 0;
        }
        if !p.is_null() {
            *p = 0;
        }
    }
    RO_E_METADATA_NAME_NOT_FOUND
}

unsafe extern "win64" fn ro_get_meta_data_file(
    _name: usize,
    _dispenser: usize,
    path: *mut usize,
    import: *mut usize,
    token: *mut u32,
) -> u32 {
    if !path.is_null() {
        *path = 0;
    }
    if !import.is_null() {
        *import = 0;
    }
    if !token.is_null() {
        *token = 0;
    }
    RO_E_METADATA_NAME_NOT_FOUND
}

pub fn wintypes_exports() -> super::Exports {
    crate::exports! {
        "RoParseTypeName" => fn ro_parse_type_name,
        "RoResolveNamespace" => fn ro_resolve_namespace,
        "RoGetMetaDataFile" => fn ro_get_meta_data_file,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_critique_et_variable_de_condition() {
        unsafe {
            let mut cs = 0usize;
            let mut cv = 0usize;
            cs_ctor(&mut cs);
            cv_ctor(&mut cv);
            assert!(cs_try_lock(&mut cs));
            assert!(!cs_try_lock(&mut cs));
            assert!(!cs_try_lock_for(&mut cs, 20));
            // Délai expiré : false, section reprise.
            assert!(!cv_wait_for(&mut cv, &mut cs, 20));
            assert!(!cs_try_lock(&mut cs));
            cs_unlock(&mut cs);

            // Un autre thread signale pendant l'attente.
            let (pcs, pcv) = (
                &mut cs as *mut usize as usize,
                &mut cv as *mut usize as usize,
            );
            let flag = std::sync::Arc::new(AtomicU32::new(0));
            let f2 = flag.clone();
            cs_lock(pcs as *mut usize);
            let t = std::thread::spawn(move || {
                cs_lock(pcs as *mut usize);
                f2.store(1, Ordering::SeqCst);
                cv_notify_all(pcv as *mut usize);
                cs_unlock(pcs as *mut usize);
            });
            while flag.load(Ordering::SeqCst) == 0 {
                cv_wait_for(pcv as *mut usize, pcs as *mut usize, 5000);
            }
            cs_unlock(pcs as *mut usize);
            t.join().unwrap();
            cv_dtor(&mut cv);
            cs_dtor(&mut cs);
            assert_eq!((cs, cv), (0, 0));
        }
    }

    #[test]
    fn dll_du_corpus_1394() {
        for (dll, f) in [
            ("mfc42.dll", "#823"),
            ("mfc42.dll", "#825"),
            ("mssign32.dll", "SignerSign"),
            ("mssign32.dll", "SignerTimeStamp"),
            (
                "concrt140.dll",
                "?lock@critical_section@Concurrency@@QEAAXXZ",
            ),
            ("api-ms-win-ro-typeresolution-l1-1-0.dll", "RoParseTypeName"),
        ] {
            let n = crate::dlls::normalize(dll);
            assert!(crate::dlls::lookup(&n, f).is_some(), "{dll}!{f}");
        }
        let mut parts = 7u32;
        let mut names = 7usize;
        assert_eq!(
            unsafe { ro_parse_type_name(0, &mut parts, &mut names) },
            REGDB_E_CLASSNOTREG
        );
        assert_eq!((parts, names), (0, 0));
    }
}
