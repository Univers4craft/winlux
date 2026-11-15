//! Fonctions du shell exportées seulement par ordinal (#1750, WinRAR).
//!
//! Ordinaux tirés de la documentation publique de Microsoft, qui dit pour
//! chacune « not exported by name… exported by ordinal N » :
//! - shell32 #100 : `SHRestricted` (shlobj_core.h) ;
//! - shlwapi #176 : `IUnknown_QueryService` (shlwapi.h) ;
//! - shlwapi #219 : `QISearch` (shlwapi.h, structure `QITAB`).
//!
//! Aucun binaire Microsoft ni d'éditeur n'a été lu pour les écrire.

use super::comobj::{Guid, E_NOINTERFACE, E_POINTER, IID_IUNKNOWN};
use std::ffi::c_void;

const E_FAIL: i32 = 0x8000_4005u32 as i32;

/// IID_IServiceProvider {6D5140C1-7436-11CE-8034-00AA006009FA} (servprov.h).
const IID_ISERVICEPROVIDER: Guid = super::comobj::guid(
    0x6D51_40C1,
    0x7436,
    0x11CE,
    [0x80, 0x34, 0x00, 0xAA, 0x00, 0x60, 0x09, 0xFA],
);

type QueryInterface = unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32;
type AddRef = unsafe extern "win64" fn(*mut c_void) -> u32;
type Release = unsafe extern "win64" fn(*mut c_void) -> u32;
type QueryService =
    unsafe extern "win64" fn(*mut c_void, *const Guid, *const Guid, *mut *mut c_void) -> i32;

/// Entrée n° `slot` de la table virtuelle de l'objet COM `obj`.
unsafe fn slot(obj: *mut c_void, slot: usize) -> usize {
    let vtbl = *(obj as *const *const usize);
    *vtbl.add(slot)
}

/// SHRestricted : aucune stratégie de groupe ne restreint le shell chez
/// Winlux (pas de clé Policies\Explorer gérée), la réponse est donc 0
/// (« restriction absente ») pour toutes les valeurs de RESTRICTIONS.
pub extern "win64" fn sh_restricted(rest: u32) -> u32 {
    if std::env::var_os("WINLUX_TRACE").is_some() {
        eprintln!("winlux : SHRestricted({rest:#x}) = 0");
    }
    0
}

/// IUnknown_QueryService : demande IServiceProvider à `punk`, puis lui
/// demande le service `sid` sous l'interface `riid`.
pub unsafe extern "win64" fn iunknown_query_service(
    punk: *mut c_void,
    sid: *const Guid,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if punk.is_null() {
        return E_FAIL;
    }
    let qi: QueryInterface = std::mem::transmute(slot(punk, 0));
    let mut sp: *mut c_void = std::ptr::null_mut();
    let hr = qi(punk, &IID_ISERVICEPROVIDER, &mut sp);
    if hr < 0 || sp.is_null() {
        return if hr < 0 { hr } else { E_NOINTERFACE };
    }
    let qs: QueryService = std::mem::transmute(slot(sp, 3));
    let hr = qs(sp, sid, riid, out);
    let release: Release = std::mem::transmute(slot(sp, 2));
    release(sp);
    hr
}

/// Une entrée de QITAB à la disposition x64, pour les tests : lue par
/// `find_entry` d'après `qitab_layout`.
#[cfg(test)]
#[repr(C)]
struct QiTab {
    piid: *const Guid,
    offset: i32,
}

/// QISearch : QueryInterface générique d'après une table QITAB. IUnknown
/// répond par la première entrée de la table, comme le dit la
/// documentation (« the first entry in the table must be an interface
/// derived from IUnknown »).
pub unsafe extern "win64" fn qi_search(
    that: *mut c_void,
    table: *const c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if that.is_null() || table.is_null() || riid.is_null() {
        return E_POINTER;
    }
    let x86 = crate::wow32::active();
    let Some(offset) = find_entry(table as *const u8, &*riid, x86) else {
        return E_NOINTERFACE;
    };
    let p = (that as *mut u8).offset(offset as isize) as *mut c_void;
    if x86 {
        // Objet du programme x86 : AddRef est la 2e case de sa table x86,
        // en stdcall (#2476).
        let vt = crate::wow32::rd(p as usize as u32);
        crate::wow32::call32(crate::wow32::rd(vt + 4) as usize, &[p as usize as u32]);
    } else {
        let add_ref: AddRef = std::mem::transmute(slot(p, 1));
        add_ref(p);
    }
    *out = p;
    0
}

/// QITAB (shlwapi.h) : `{ const IID *piid; DWORD dwOffset; }`. Décalages
/// mesurés par `sizeof`/`offsetof` sur l'en-tête mingw-w64 (#2476) :
/// 8 octets et dwOffset à +4 pour un appelant x86, 16 octets et dwOffset
/// à +8 en x64.
fn qitab_layout(x86: bool) -> (usize, usize) {
    if x86 {
        (8, 4)
    } else {
        (16, 8)
    }
}

/// dwOffset de l'entrée qui répond à `wanted` ; IUnknown répond par la
/// première entrée. La table finit par une entrée dont piid est nul.
unsafe fn find_entry(table: *const u8, wanted: &Guid, x86: bool) -> Option<i32> {
    let (size, at) = qitab_layout(x86);
    let mut i = 0;
    loop {
        let e = table.add(i * size);
        let piid = if x86 {
            std::ptr::read_unaligned(e as *const u32) as usize as *const Guid
        } else {
            std::ptr::read_unaligned(e as *const *const Guid)
        };
        if piid.is_null() {
            return None;
        }
        if std::ptr::read_unaligned(piid) == *wanted || (i == 0 && *wanted == IID_IUNKNOWN) {
            return Some(std::ptr::read_unaligned(e.add(at) as *const i32));
        }
        i += 1;
    }
}

pub fn shell32() -> crate::dlls::Exports {
    crate::exports! {
        "SHRestricted" => fn sh_restricted,
        "#100" => fn sh_restricted,
    }
}

pub fn shlwapi() -> crate::dlls::Exports {
    crate::exports! {
        "IUnknown_QueryService" => fn iunknown_query_service,
        "#176" => fn iunknown_query_service,
        "QISearch" => fn qi_search,
        "#219" => fn qi_search,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Objet COM minimal à deux interfaces : la première (IUnknown et
    /// IServiceProvider) au début, la seconde 8 octets plus loin.
    #[repr(C)]
    struct Obj {
        vtbl1: *const usize,
        vtbl2: *const usize,
    }

    static REFS: AtomicU32 = AtomicU32::new(0);
    const IID_AUTRE: Guid = super::super::comobj::guid(1, 2, 3, [4, 5, 6, 7, 8, 9, 10, 11]);
    const SID: Guid = super::super::comobj::guid(9, 9, 9, [9; 8]);

    unsafe extern "win64" fn qi(this: *mut c_void, iid: *const Guid, out: *mut *mut c_void) -> i32 {
        let table = [
            QiTab {
                piid: &IID_ISERVICEPROVIDER,
                offset: 0,
            },
            QiTab {
                piid: &IID_AUTRE,
                offset: 8,
            },
            QiTab {
                piid: std::ptr::null(),
                offset: 0,
            },
        ];
        qi_search(this, table.as_ptr() as *const c_void, iid, out)
    }
    unsafe extern "win64" fn add_ref(_: *mut c_void) -> u32 {
        REFS.fetch_add(1, Ordering::SeqCst) + 1
    }
    unsafe extern "win64" fn release(_: *mut c_void) -> u32 {
        REFS.fetch_sub(1, Ordering::SeqCst) - 1
    }
    unsafe extern "win64" fn query_service(
        this: *mut c_void,
        sid: *const Guid,
        riid: *const Guid,
        out: *mut *mut c_void,
    ) -> i32 {
        if *sid != SID {
            return E_NOINTERFACE;
        }
        qi(this, riid, out)
    }

    /// #2476 : table QITAB d'un programme x86, entrées de 8 octets dans de
    /// la mémoire sous 4 Go. Lue à la disposition x64, l'IID de l'entrée 1
    /// était pris pour celui de l'entrée 0.
    #[test]
    fn table_qitab_x86() {
        let low = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                4096,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_32BIT,
                -1,
                0,
            )
        };
        assert_ne!(low, libc::MAP_FAILED);
        let base = low as *mut u8;
        unsafe {
            let g1 = base.add(256) as *mut Guid;
            let g2 = base.add(272) as *mut Guid;
            *g1 = IID_ISERVICEPROVIDER;
            *g2 = IID_AUTRE;
            let t = std::slice::from_raw_parts_mut(base as *mut u32, 6);
            // { &IID_IServiceProvider, 0 }, { &IID_AUTRE, 4 }, { NULL, 0 }
            t.copy_from_slice(&[g1 as usize as u32, 0, g2 as usize as u32, 4, 0, 0]);
            assert_eq!(find_entry(base, &IID_ISERVICEPROVIDER, true), Some(0));
            assert_eq!(find_entry(base, &IID_AUTRE, true), Some(4));
            assert_eq!(find_entry(base, &IID_IUNKNOWN, true), Some(0));
            let inconnu = super::super::comobj::guid(7, 7, 7, [7; 8]);
            assert_eq!(find_entry(base, &inconnu, true), None);
            libc::munmap(low, 4096);
        }
        assert_eq!(qitab_layout(true), (8, 4));
    }

    #[test]
    fn ordinaux_du_shell() {
        unsafe {
            let v1: [usize; 4] = [
                qi as *const () as usize,
                add_ref as *const () as usize,
                release as *const () as usize,
                query_service as *const () as usize,
            ];
            let v2: [usize; 3] = [
                qi as *const () as usize,
                add_ref as *const () as usize,
                release as *const () as usize,
            ];
            let mut obj = Obj {
                vtbl1: v1.as_ptr(),
                vtbl2: v2.as_ptr(),
            };
            let p = &mut obj as *mut Obj as *mut c_void;

            // QISearch : IUnknown = première entrée, l'autre interface à +8.
            let mut out = std::ptr::null_mut();
            assert_eq!(qi(p, &IID_IUNKNOWN, &mut out), 0);
            assert_eq!(out, p);
            assert_eq!(qi(p, &IID_AUTRE, &mut out), 0);
            assert_eq!(out as usize, p as usize + 8);
            let inconnu = super::super::comobj::guid(7, 7, 7, [7; 8]);
            assert_eq!(qi(p, &inconnu, &mut out), E_NOINTERFACE);
            assert!(out.is_null());
            assert_eq!(qitab_layout(false), (16, 8));

            // IUnknown_QueryService : passe par IServiceProvider et le relâche.
            let avant = REFS.load(Ordering::SeqCst);
            assert_eq!(iunknown_query_service(p, &SID, &IID_AUTRE, &mut out), 0);
            assert_eq!(out as usize, p as usize + 8);
            assert_eq!(
                REFS.load(Ordering::SeqCst),
                avant + 1,
                "seul le résultat garde une référence"
            );
            assert_eq!(
                iunknown_query_service(std::ptr::null_mut(), &SID, &IID_AUTRE, &mut out),
                E_FAIL
            );
            assert!(out.is_null());

            assert_eq!(sh_restricted(0x4000_0001), 0);
        }
        assert!(crate::dlls::lookup("shell32", "#100").is_some());
        assert!(crate::dlls::lookup("shlwapi", "#176").is_some());
        assert!(crate::dlls::lookup("shlwapi", "#219").is_some());
    }
}
