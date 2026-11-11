//! Compléments du CRT (UCRT et vcruntime) demandés par Audacity et les
//! programmes construits avec Visual Studio récent : descripteurs, chaînes
//! larges, classification des flottants, réglages du processus.

use super::crt_runtime::{errno, lseeki64};
use super::crt_stdio::{acrt_iob_func, fopen, write_bytes};
use super::wstr;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

// --- Descripteurs ---------------------------------------------------------------------

unsafe extern "win64" fn telli64(fd: i32) -> i64 {
    lseeki64(fd, 0, 1)
}

unsafe extern "win64" fn tell(fd: i32) -> i32 {
    lseeki64(fd, 0, 1) as i32
}

/// _eof : 1 en fin de fichier, 0 sinon, -1 en cas d'erreur.
unsafe extern "win64" fn eof(fd: i32) -> i32 {
    let cur = libc::lseek(fd, 0, libc::SEEK_CUR);
    if cur < 0 {
        *errno() = 9;
        return -1;
    }
    let end = libc::lseek(fd, 0, libc::SEEK_END);
    libc::lseek(fd, cur, libc::SEEK_SET);
    if end < 0 {
        *errno() = 9;
        return -1;
    }
    (cur >= end) as i32
}

unsafe extern "win64" fn commit(fd: i32) -> i32 {
    if libc::fsync(fd) == 0 {
        0
    } else {
        *errno() = 9;
        -1
    }
}

unsafe extern "win64" fn fsopen(path: *const u8, mode: *const u8, _share: i32) -> *mut c_void {
    fopen(path, mode)
}

// --- Chaînes larges -------------------------------------------------------------------

unsafe fn wlen(s: *const u16) -> usize {
    crate::printf::wide_len(s)
}

unsafe extern "win64" fn fputws(s: *const u16, stream: *mut c_void) -> i32 {
    if s.is_null() || stream.is_null() {
        *errno() = 22;
        return -1;
    }
    let text = wstr(s);
    if write_bytes(stream, text.as_bytes()) < 0 {
        -1
    } else {
        0
    }
}

unsafe extern "win64" fn putws(s: *const u16) -> i32 {
    let out = acrt_iob_func(1);
    if fputws(s, out) < 0 || write_bytes(out, b"\n") < 0 {
        return 0xFFFF; // WEOF
    }
    0
}

unsafe extern "win64" fn wcsncat(dst: *mut u16, src: *const u16, n: usize) -> *mut u16 {
    let d = wlen(dst);
    let mut i = 0;
    while i < n && *src.add(i) != 0 {
        *dst.add(d + i) = *src.add(i);
        i += 1;
    }
    *dst.add(d + i) = 0;
    dst
}

unsafe fn in_set(c: u16, set: *const u16) -> bool {
    let mut p = set;
    while *p != 0 {
        if *p == c {
            return true;
        }
        p = p.add(1);
    }
    false
}

unsafe extern "win64" fn wcspbrk(s: *const u16, set: *const u16) -> *const u16 {
    let mut p = s;
    while *p != 0 {
        if in_set(*p, set) {
            return p;
        }
        p = p.add(1);
    }
    std::ptr::null()
}

unsafe extern "win64" fn wcsspn(s: *const u16, set: *const u16) -> usize {
    let mut n = 0;
    while *s.add(n) != 0 && in_set(*s.add(n), set) {
        n += 1;
    }
    n
}

unsafe extern "win64" fn wcstol_l(
    s: *const u16,
    end: *mut *const u16,
    base: i32,
    _loc: usize,
) -> i32 {
    super::crt_ucrt::wcstol(s, end, base)
}

unsafe extern "win64" fn wcstoul_l(
    s: *const u16,
    end: *mut *const u16,
    base: i32,
    _loc: usize,
) -> u32 {
    super::crt_ucrt::wcstoul(s, end, base)
}

unsafe extern "win64" fn wperror(prefix: *const u16) {
    let p = if prefix.is_null() {
        None
    } else {
        std::ffi::CString::new(wstr(prefix)).ok()
    };
    super::crt_runtime::perror(p.as_ref().map_or(std::ptr::null(), |c| c.as_ptr().cast()));
}

// --- Fichiers -------------------------------------------------------------------------

/// _chmod / _wchmod : seul _S_IWRITE (0x80) compte, comme sous Windows.
fn chmod_path(path: &str, mode: i32) -> i32 {
    use std::os::unix::fs::PermissionsExt;
    let p = crate::paths::to_linux(path);
    let Ok(meta) = std::fs::metadata(&p) else {
        unsafe { *errno() = 2 };
        return -1;
    };
    let mut bits = meta.permissions().mode();
    if mode & 0x80 != 0 {
        bits |= 0o200;
    } else {
        bits &= !0o222;
    }
    match std::fs::set_permissions(&p, std::fs::Permissions::from_mode(bits)) {
        Ok(()) => 0,
        Err(_) => {
            unsafe { *errno() = 13 };
            -1
        }
    }
}

unsafe extern "win64" fn wchmod(path: *const u16, mode: i32) -> i32 {
    if path.is_null() {
        *errno() = 22;
        return -1;
    }
    chmod_path(&wstr(path), mode)
}

unsafe extern "win64" fn chmod(path: *const u8, mode: i32) -> i32 {
    if path.is_null() {
        *errno() = 22;
        return -1;
    }
    chmod_path(&super::cstr(path), mode)
}

/// _utime64 : struct __utimbuf64 { actime, modtime } ; NULL : maintenant.
unsafe fn utime_path(path: &str, times: *const i64) -> i32 {
    let p = crate::paths::to_linux(path);
    let Ok(c) = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()) else {
        *errno() = 22;
        return -1;
    };
    let r = if times.is_null() {
        libc::utimensat(libc::AT_FDCWD, c.as_ptr(), std::ptr::null(), 0)
    } else {
        let ts = [
            libc::timespec {
                tv_sec: *times as libc::time_t,
                tv_nsec: 0,
            },
            libc::timespec {
                tv_sec: *times.add(1) as libc::time_t,
                tv_nsec: 0,
            },
        ];
        libc::utimensat(libc::AT_FDCWD, c.as_ptr(), ts.as_ptr(), 0)
    };
    if r == 0 {
        0
    } else {
        *errno() = 2;
        -1
    }
}

unsafe extern "win64" fn wutime64(path: *const u16, times: *const i64) -> i32 {
    if path.is_null() {
        *errno() = 22;
        return -1;
    }
    utime_path(&wstr(path), times)
}

unsafe extern "win64" fn utime64(path: *const u8, times: *const i64) -> i32 {
    if path.is_null() {
        *errno() = 22;
        return -1;
    }
    utime_path(&super::cstr(path), times)
}

static TMP_N: AtomicUsize = AtomicUsize::new(0);
static mut TMPNAM_BUF: [u8; 260] = [0; 260];

/// tmpnam : nom unique « \sXXXX. » relatif au dossier courant, comme l'UCRT.
unsafe extern "win64" fn tmpnam(buf: *mut u8) -> *mut u8 {
    let n = TMP_N.fetch_add(1, Ordering::Relaxed);
    let name = format!("\\s{:x}{:x}.", std::process::id(), n);
    let dst = if buf.is_null() {
        std::ptr::addr_of_mut!(TMPNAM_BUF) as *mut u8
    } else {
        buf
    };
    std::ptr::copy_nonoverlapping(name.as_ptr(), dst, name.len());
    *dst.add(name.len()) = 0;
    dst
}

// --- Console --------------------------------------------------------------------------

/// _kbhit : sous Windows, seules les touches en attente dans la console
/// comptent ; sans console (entrée redirigée, lancement depuis le bureau),
/// la réponse est 0. Ici, seule une entrée standard qui est un terminal peut
/// avoir une touche en attente : /dev/null ou un fichier sont toujours
/// « lisibles » pour poll (fin de fichier), et Xonotic tournait sans fin dans
/// `while (_kbhit()) _getch();` sans plus jamais dessiner (#1421).
fn key_waiting(fd: i32) -> bool {
    unsafe {
        if libc::isatty(fd) != 1 {
            return false;
        }
        let mut fds = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        libc::poll(&mut fds, 1, 0) > 0 && fds.revents & libc::POLLIN != 0
    }
}

extern "win64" fn kbhit() -> i32 {
    key_waiting(0) as i32
}

unsafe extern "win64" fn getch() -> i32 {
    let mut b = 0u8;
    if libc::read(0, (&mut b as *mut u8).cast(), 1) == 1 {
        b as i32
    } else {
        -1
    }
}

// --- Flottants ------------------------------------------------------------------------

/// Classes de _dclass : FP_INFINITE 1, FP_NAN 2, FP_NORMAL -1,
/// FP_SUBNORMAL -2, FP_ZERO 0.
fn class_of(c: std::num::FpCategory) -> i16 {
    use std::num::FpCategory::*;
    match c {
        Infinite => 1,
        Nan => 2,
        Normal => -1,
        Subnormal => -2,
        Zero => 0,
    }
}

extern "win64" fn dclass(x: f64) -> i16 {
    class_of(x.classify())
}

extern "win64" fn fdclass(x: f32) -> i16 {
    class_of(x.classify())
}

extern "win64" fn dsign(x: f64) -> i32 {
    if x.is_sign_negative() {
        0x8000
    } else {
        0
    }
}

/// _ldsign : sous MSVC, `long double` a la taille d'un `double` ; même
/// résultat que `_dsign` (0x8000 si le bit de signe est levé). #1874.
extern "win64" fn ldsign(x: f64) -> i32 {
    dsign(x)
}

/// __initialize_lconv_for_unsigned_char : appelée au démarrage des
/// programmes compilés avec `/J` (char non signé). Winlux ne sert que la
/// locale « C », dont la `lconv` statique n'est pas réécrite ; l'appel
/// réussit (0) sans effet. L'ODT x86 s'arrêtait sans elle (#1874).
extern "win64" fn initialize_lconv_for_unsigned_char() -> i32 {
    0
}

extern "win64" fn fdsign(x: f32) -> i32 {
    if x.is_sign_negative() {
        0x8000
    } else {
        0
    }
}

extern "win64" fn log2l(x: f64) -> f64 {
    x.log2()
}

/// _clearfp / _statusfp : les drapeaux d'état x87/SSE ne sont pas suivis.
extern "win64" fn clearfp() -> u32 {
    0
}

// --- Réglages du processus ------------------------------------------------------------

static ABORT_BEHAVIOR: AtomicU32 = AtomicU32::new(1);
static NEW_HANDLER: AtomicUsize = AtomicUsize::new(0);

extern "win64" fn set_abort_behavior(flags: u32, mask: u32) -> u32 {
    let old = ABORT_BEHAVIOR.load(Ordering::Relaxed);
    ABORT_BEHAVIOR.store((old & !mask) | (flags & mask), Ordering::Relaxed);
    old
}

extern "win64" fn set_new_handler(h: usize) -> usize {
    NEW_HANDLER.swap(h, Ordering::Relaxed)
}

/// _crt_at_quick_exit : enregistrée ; quick_exit n'appelle pas encore la liste.
extern "win64" fn crt_at_quick_exit(_f: usize) -> i32 {
    0
}

// --- type_info ------------------------------------------------------------------------

/// __std_type_info_compare : type_info x64 = { vftable, nom non décoré,
/// nom décoré[] } ; comparaison des noms décorés.
unsafe extern "win64" fn type_info_compare(a: usize, b: usize) -> i32 {
    if a == b {
        return 0;
    }
    let na = std::ffi::CStr::from_ptr((a + 16) as *const std::ffi::c_char);
    let nb = std::ffi::CStr::from_ptr((b + 16) as *const std::ffi::c_char);
    match na.cmp(nb) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "_telli64" => fn telli64, "_tell" => fn tell, "_eof" => fn eof, "_commit" => fn commit,
        "_fsopen" => fn fsopen, "fputws" => fn fputws, "_putws" => fn putws,
        "wcsncat" => fn wcsncat, "wcspbrk" => fn wcspbrk, "wcsspn" => fn wcsspn,
        "_wcstol_l" => fn wcstol_l, "_wcstoul_l" => fn wcstoul_l, "_wperror" => fn wperror,
        "_wchmod" => fn wchmod, "_chmod" => fn chmod,
        "_wutime64" => fn wutime64, "_utime64" => fn utime64, "_wutime" => fn wutime64, "_utime" => fn utime64,
        "tmpnam" => fn tmpnam, "_kbhit" => fn kbhit, "_getch" => fn getch,
        "_dclass" => fn dclass, "_fdclass" => fn fdclass, "_dsign" => fn dsign, "_fdsign" => fn fdsign,
        "_ldsign" => fn ldsign, "__initialize_lconv_for_unsigned_char" => fn initialize_lconv_for_unsigned_char,
        "log2l" => fn log2l, "_clearfp" => fn clearfp, "_statusfp" => fn clearfp,
        "_set_abort_behavior" => fn set_abort_behavior, "_set_new_handler" => fn set_new_handler,
        "_crt_at_quick_exit" => fn crt_at_quick_exit,
        "__std_type_info_compare" => fn type_info_compare,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_crt_de_l_odt_servis() {
        // setup.exe de l'ODT (x86) importe ces deux fonctions (#1874).
        for (dll, f) in [
            (
                "api-ms-win-crt-locale-l1-1-0.dll",
                "__initialize_lconv_for_unsigned_char",
            ),
            ("api-ms-win-crt-math-l1-1-0.dll", "_ldsign"),
        ] {
            assert!(
                crate::dlls::lookup(&crate::dlls::normalize(dll), f).is_some(),
                "{f}"
            );
        }
        assert_eq!(ldsign(-2.5), 0x8000);
        assert_eq!(ldsign(-0.0), 0x8000);
        assert_eq!(ldsign(1.0), 0);
        assert_eq!(initialize_lconv_for_unsigned_char(), 0);
    }

    #[test]
    fn kbhit_sans_console_repond_zero() {
        // /dev/null et un tube fermé sont « lisibles » (fin de fichier) mais
        // ne portent aucune touche, comme une appli sans console (#1421).
        let null = std::fs::File::open("/dev/null").unwrap();
        assert!(!key_waiting(std::os::fd::AsRawFd::as_raw_fd(&null)));
        let mut p = [0i32; 2];
        assert_eq!(unsafe { libc::pipe(p.as_mut_ptr()) }, 0);
        unsafe { libc::close(p[1]) };
        assert!(!key_waiting(p[0]));
        unsafe { libc::close(p[0]) };
    }

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    #[test]
    fn chaines_larges() {
        unsafe {
            let mut buf = [0u16; 16];
            let a = w("ab");
            buf[..3].copy_from_slice(&a);
            let c = w("cdef");
            wcsncat(buf.as_mut_ptr(), c.as_ptr(), 2);
            assert_eq!(wstr(buf.as_ptr()), "abcd");
            let s = w("hello, world");
            let set = w(", ");
            assert_eq!(wcspbrk(s.as_ptr(), set.as_ptr()), s.as_ptr().add(5));
            let lead = w("lhe");
            assert_eq!(wcsspn(s.as_ptr(), lead.as_ptr()), 4);
        }
    }

    #[test]
    fn classes_des_flottants() {
        assert_eq!(dclass(1.0), -1);
        assert_eq!(dclass(0.0), 0);
        assert_eq!(dclass(f64::INFINITY), 1);
        assert_eq!(dclass(f64::NAN), 2);
        assert_eq!(fdclass(1e-40), -2);
        assert_eq!(dsign(-0.0), 0x8000);
        assert_eq!(fdsign(2.0), 0);
    }

    #[test]
    fn noms_de_type() {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        a[16..24].copy_from_slice(b".?AVA@@\0");
        b[16..24].copy_from_slice(b".?AVB@@\0");
        unsafe {
            assert_eq!(
                type_info_compare(a.as_ptr() as usize, a.as_ptr() as usize),
                0
            );
            assert_eq!(
                type_info_compare(a.as_ptr() as usize, b.as_ptr() as usize),
                -1
            );
        }
    }
}
