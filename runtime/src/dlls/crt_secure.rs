//! Fonctions « sécurisées » `*_s` de msvcr80…120 qui manquaient (#1980).
//!
//! gldriverquery64.exe de Steam s'arrêtait sur `msvcr120!sscanf_s`. La
//! liste des `*_s` exportées vient de la bibliothèque d'import
//! `libmsvcr120.a` de mingw-w64 ; leur comportement, de la documentation
//! publique de la CRT (Microsoft Learn) : taille du tampon de destination
//! passée en plus, chaîne vide et `ERANGE`/`EINVAL` quand la sortie ne tient
//! pas, et pour les `scanf_s` une taille après chaque `%s`, `%c` et `%[`.
//!
//! Les fonctions de mise en forme passent par le moteur des
//! `__stdio_common_*` de la CRT universelle (`crt_ucrt`), avec les
//! options de msvcrt (exposants à trois chiffres, `%s` large dans les
//! variantes larges). Les autres s'appuient sur la fonction sans `_s`
//! déjà servie.
//!
//! Les programmes x86 passent par `wow32::crt` (table `FORMATS`) pour les
//! fonctions variadiques et à `va_list`.

use std::ffi::c_void;

use super::crt_runtime::errno;
use super::crt_ucrt as ucrt;

const EINVAL: i32 = 22;
const ERANGE: i32 = 34;
const EILSEQ: i32 = 42;

/// Options de msvcrt pour le moteur de `__stdio_common_*` : exposants à
/// trois chiffres ; `%s` large dans wprintf et ses sœurs.
const NARROW: u64 = 0x10;
const WIDE: u64 = 0x14;
/// `_CRT_INTERNAL_SCANF_SECURECRT` : taille après %s, %c et %[.
const SCAN_S: u64 = 1;

unsafe fn fail(e: i32) -> i32 {
    *errno() = e;
    e
}

fn stdin() -> *mut c_void {
    super::crt_stdio::std_stream(0)
}
fn stdout() -> *mut c_void {
    super::crt_stdio::std_stream(1)
}

/// Fonction de msvcrt servie par Winlux, par son nom.
unsafe fn base<T: Copy>(name: &str) -> Option<T> {
    debug_assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<usize>());
    super::lookup("msvcrt", name).map(|a| std::mem::transmute_copy(&a))
}

// ---------------------------------------------------------------------
// Mise en forme : printf_s et ses sœurs
// ---------------------------------------------------------------------

unsafe extern "win64" fn vprintf_s(fmt: *const u8, va: *const u64) -> i32 {
    ucrt::stdio_common_vfprintf(NARROW, stdout(), fmt, 0, va)
}
unsafe extern "win64" fn vfprintf_s(f: *mut c_void, fmt: *const u8, va: *const u64) -> i32 {
    if f.is_null() {
        fail(EINVAL);
        return -1;
    }
    ucrt::stdio_common_vfprintf(NARROW, f, fmt, 0, va)
}
unsafe extern "win64" fn vwprintf_s(fmt: *const u16, va: *const u64) -> i32 {
    ucrt::stdio_common_vfwprintf(WIDE, stdout(), fmt, 0, va)
}
unsafe extern "win64" fn vfwprintf_s(f: *mut c_void, fmt: *const u16, va: *const u64) -> i32 {
    if f.is_null() {
        fail(EINVAL);
        return -1;
    }
    ucrt::stdio_common_vfwprintf(WIDE, f, fmt, 0, va)
}
unsafe extern "win64" fn vsprintf_s(
    buf: *mut u8,
    size: usize,
    fmt: *const u8,
    va: *const u64,
) -> i32 {
    ucrt::stdio_common_vsprintf_s(NARROW, buf, size, fmt, 0, va)
}
unsafe extern "win64" fn vswprintf_s(
    buf: *mut u16,
    size: usize,
    fmt: *const u16,
    va: *const u64,
) -> i32 {
    ucrt::stdio_common_vswprintf_s(WIDE, buf, size, fmt, 0, va)
}
/// `_vsnprintf_s` et `vsnprintf_s` (même signature).
unsafe extern "win64" fn vsnprintf_s(
    buf: *mut u8,
    size: usize,
    count: usize,
    fmt: *const u8,
    va: *const u64,
) -> i32 {
    ucrt::stdio_common_vsnprintf_s(NARROW, buf, size, count, fmt, 0, va)
}
unsafe extern "win64" fn vsnwprintf_s(
    buf: *mut u16,
    size: usize,
    count: usize,
    fmt: *const u16,
    va: *const u64,
) -> i32 {
    ucrt::stdio_common_vsnwprintf_s(WIDE, buf, size, count, fmt, 0, va)
}
unsafe extern "win64" fn vcprintf_s(fmt: *const u8, va: *const u64) -> i32 {
    super::crt_releve::conio_common_vcprintf(NARROW, fmt, 0, va)
}
unsafe extern "win64" fn vcwprintf_s(fmt: *const u16, va: *const u64) -> i32 {
    if fmt.is_null() {
        fail(EINVAL);
        return -1;
    }
    let out = ucrt::format_wide(WIDE, fmt, va);
    let text = String::from_utf16_lossy(&out);
    match libc::write(1, text.as_ptr() as *const c_void, text.len()) {
        n if n < 0 => -1,
        _ => out.len() as i32,
    }
}

unsafe extern "win64" fn imp_printf_s(a: *const u64) -> i32 {
    vprintf_s(*a as *const u8, a.add(1))
}
unsafe extern "win64" fn imp_fprintf_s(a: *const u64) -> i32 {
    vfprintf_s(*a as *mut c_void, *a.add(1) as *const u8, a.add(2))
}
unsafe extern "win64" fn imp_wprintf_s(a: *const u64) -> i32 {
    vwprintf_s(*a as *const u16, a.add(1))
}
unsafe extern "win64" fn imp_fwprintf_s(a: *const u64) -> i32 {
    vfwprintf_s(*a as *mut c_void, *a.add(1) as *const u16, a.add(2))
}
unsafe extern "win64" fn imp_sprintf_s(a: *const u64) -> i32 {
    vsprintf_s(
        *a as *mut u8,
        *a.add(1) as usize,
        *a.add(2) as *const u8,
        a.add(3),
    )
}
unsafe extern "win64" fn imp_cprintf_s(a: *const u64) -> i32 {
    vcprintf_s(*a as *const u8, a.add(1))
}
unsafe extern "win64" fn imp_cwprintf_s(a: *const u64) -> i32 {
    vcwprintf_s(*a as *const u16, a.add(1))
}

crate::varargs_thunk!(printf_s_thunk => imp_printf_s);
crate::varargs_thunk!(fprintf_s_thunk => imp_fprintf_s);
crate::varargs_thunk!(wprintf_s_thunk => imp_wprintf_s);
crate::varargs_thunk!(fwprintf_s_thunk => imp_fwprintf_s);
crate::varargs_thunk!(sprintf_s_thunk => imp_sprintf_s);
crate::varargs_thunk!(cprintf_s_thunk => imp_cprintf_s);
crate::varargs_thunk!(cwprintf_s_thunk => imp_cwprintf_s);

// ---------------------------------------------------------------------
// Lecture : scanf_s et ses sœurs
// ---------------------------------------------------------------------

unsafe extern "win64" fn vsscanf_s(buf: *const u8, fmt: *const u8, va: *const u64) -> i32 {
    ucrt::stdio_common_vsscanf(SCAN_S, buf, usize::MAX, fmt, 0, va)
}
unsafe extern "win64" fn vswscanf_s(buf: *const u16, fmt: *const u16, va: *const u64) -> i32 {
    ucrt::stdio_common_vswscanf(SCAN_S, buf, usize::MAX, fmt, 0, va)
}
unsafe extern "win64" fn vfscanf_s(f: *mut c_void, fmt: *const u8, va: *const u64) -> i32 {
    super::crt_releve::stdio_common_vfscanf(SCAN_S, f, fmt, 0, va)
}
unsafe extern "win64" fn vfwscanf_s(f: *mut c_void, fmt: *const u16, va: *const u64) -> i32 {
    super::crt_releve::stdio_common_vfwscanf(SCAN_S, f, fmt, 0, va)
}
unsafe extern "win64" fn vscanf_s(fmt: *const u8, va: *const u64) -> i32 {
    vfscanf_s(stdin(), fmt, va)
}
unsafe extern "win64" fn vwscanf_s(fmt: *const u16, va: *const u64) -> i32 {
    vfwscanf_s(stdin(), fmt, va)
}

unsafe extern "win64" fn imp_sscanf_s(a: *const u64) -> i32 {
    vsscanf_s(*a as *const u8, *a.add(1) as *const u8, a.add(2))
}
unsafe extern "win64" fn imp_snscanf_s(a: *const u64) -> i32 {
    ucrt::stdio_common_vsscanf(
        SCAN_S,
        *a as *const u8,
        *a.add(1) as usize,
        *a.add(2) as *const u8,
        0,
        a.add(3),
    )
}
unsafe extern "win64" fn imp_snwscanf_s(a: *const u64) -> i32 {
    ucrt::stdio_common_vswscanf(
        SCAN_S,
        *a as *const u16,
        *a.add(1) as usize,
        *a.add(2) as *const u16,
        0,
        a.add(3),
    )
}
unsafe extern "win64" fn imp_fscanf_s(a: *const u64) -> i32 {
    vfscanf_s(*a as *mut c_void, *a.add(1) as *const u8, a.add(2))
}
unsafe extern "win64" fn imp_fwscanf_s(a: *const u64) -> i32 {
    vfwscanf_s(*a as *mut c_void, *a.add(1) as *const u16, a.add(2))
}
/// `scanf_s` et `_cscanf_s` : l'entrée standard (la console).
unsafe extern "win64" fn imp_scanf_s(a: *const u64) -> i32 {
    vscanf_s(*a as *const u8, a.add(1))
}
unsafe extern "win64" fn imp_wscanf_s(a: *const u64) -> i32 {
    vwscanf_s(*a as *const u16, a.add(1))
}

crate::varargs_thunk!(sscanf_s_thunk => imp_sscanf_s);
crate::varargs_thunk!(snscanf_s_thunk => imp_snscanf_s);
crate::varargs_thunk!(snwscanf_s_thunk => imp_snwscanf_s);
crate::varargs_thunk!(fscanf_s_thunk => imp_fscanf_s);
crate::varargs_thunk!(fwscanf_s_thunk => imp_fwscanf_s);
crate::varargs_thunk!(scanf_s_thunk => imp_scanf_s);
crate::varargs_thunk!(wscanf_s_thunk => imp_wscanf_s);

// ---------------------------------------------------------------------
// Copies bornées
// ---------------------------------------------------------------------

/// Copie `src` et son zéro final dans `buf` de `size` cases : EINVAL sans
/// tampon, ERANGE (et chaîne vide) s'il est trop petit.
unsafe fn store<T: Copy + Default>(buf: *mut T, size: usize, src: &[T]) -> i32 {
    if buf.is_null() || size == 0 {
        return fail(EINVAL);
    }
    if src.len() >= size {
        *buf = T::default();
        return fail(ERANGE);
    }
    std::ptr::copy_nonoverlapping(src.as_ptr(), buf, src.len());
    *buf.add(src.len()) = T::default();
    0
}

unsafe fn c_str<'a>(p: *const u8) -> &'a [u8] {
    if p.is_null() {
        &[]
    } else {
        std::ffi::CStr::from_ptr(p.cast()).to_bytes()
    }
}

fn widen(b: &[u8]) -> Vec<u16> {
    String::from_utf8_lossy(b).encode_utf16().collect()
}

/// Texte d'une fonction de msvcrt qui rend une chaîne statique.
unsafe fn store_from(buf: *mut u8, size: usize, text: *const u8) -> i32 {
    if text.is_null() {
        if !buf.is_null() && size > 0 {
            *buf = 0;
        }
        return fail(EINVAL);
    }
    store(buf, size, c_str(text))
}
unsafe fn store_wide_from(buf: *mut u16, size: usize, text: *const u8) -> i32 {
    if text.is_null() {
        if !buf.is_null() && size > 0 {
            *buf = 0;
        }
        return fail(EINVAL);
    }
    store(buf, size, &widen(c_str(text)))
}

// ---------------------------------------------------------------------
// Dates et heures
// ---------------------------------------------------------------------

type TextOf = unsafe extern "win64" fn(*const c_void) -> *const u8;

unsafe extern "win64" fn asctime_s(buf: *mut u8, size: usize, tm: *const c_void) -> i32 {
    if tm.is_null() {
        return store_from(buf, size, std::ptr::null());
    }
    match base::<TextOf>("asctime") {
        Some(f) => store_from(buf, size, f(tm)),
        None => fail(EINVAL),
    }
}
unsafe extern "win64" fn wasctime_s(buf: *mut u16, size: usize, tm: *const c_void) -> i32 {
    if tm.is_null() {
        return store_wide_from(buf, size, std::ptr::null());
    }
    match base::<TextOf>("asctime") {
        Some(f) => store_wide_from(buf, size, f(tm)),
        None => fail(EINVAL),
    }
}
unsafe extern "win64" fn ctime32_s(buf: *mut u8, size: usize, t: *const i32) -> i32 {
    if t.is_null() {
        return store_from(buf, size, std::ptr::null());
    }
    match base::<TextOf>("_ctime32") {
        Some(f) => store_from(buf, size, f(t.cast())),
        None => fail(EINVAL),
    }
}
unsafe extern "win64" fn ctime64_s(buf: *mut u8, size: usize, t: *const i64) -> i32 {
    if t.is_null() {
        return store_from(buf, size, std::ptr::null());
    }
    match base::<TextOf>("_ctime64") {
        Some(f) => store_from(buf, size, f(t.cast())),
        None => fail(EINVAL),
    }
}
unsafe extern "win64" fn wctime64_s(buf: *mut u16, size: usize, t: *const i64) -> i32 {
    if t.is_null() {
        return store_wide_from(buf, size, std::ptr::null());
    }
    ctime_wide(buf, size, *t)
}
unsafe extern "win64" fn wctime32_s(buf: *mut u16, size: usize, t: *const i32) -> i32 {
    if t.is_null() {
        return store_wide_from(buf, size, std::ptr::null());
    }
    ctime_wide(buf, size, i64::from(*t))
}
unsafe fn ctime_wide(buf: *mut u16, size: usize, t: i64) -> i32 {
    match base::<TextOf>("_ctime64") {
        Some(f) => store_wide_from(buf, size, f((&t as *const i64).cast())),
        None => fail(EINVAL),
    }
}

/// `struct tm` : neuf `int`.
const TM_SIZE: usize = 36;

unsafe fn tm_from(out: *mut u8, t: *const i32, name: &str) -> i32 {
    if out.is_null() {
        return fail(EINVAL);
    }
    if t.is_null() {
        std::ptr::write_bytes(out, 0xFF, TM_SIZE);
        return fail(EINVAL);
    }
    let r = base::<TextOf>(name).map_or(std::ptr::null(), |f| f(t.cast()));
    if r.is_null() {
        std::ptr::write_bytes(out, 0xFF, TM_SIZE);
        return fail(EINVAL);
    }
    std::ptr::copy_nonoverlapping(r, out, TM_SIZE);
    0
}
unsafe extern "win64" fn gmtime32_s(out: *mut u8, t: *const i32) -> i32 {
    tm_from(out, t, "_gmtime32")
}
unsafe extern "win64" fn localtime32_s(out: *mut u8, t: *const i32) -> i32 {
    tm_from(out, t, "_localtime32")
}

type FillBuf = unsafe extern "win64" fn(*mut u8) -> *mut u8;

/// `_strdate_s` / `_strtime_s` : « MM/JJ/AA » ou « HH:MM:SS », 9 cases.
unsafe fn date_or_time(buf: *mut u8, size: usize, name: &str) -> i32 {
    if buf.is_null() || size == 0 {
        return fail(EINVAL);
    }
    if size < 9 {
        *buf = 0;
        return fail(ERANGE);
    }
    let mut tmp = [0u8; 16];
    match base::<FillBuf>(name) {
        Some(f) => {
            f(tmp.as_mut_ptr());
            store(buf, size, c_str(tmp.as_ptr()))
        }
        None => fail(EINVAL),
    }
}
unsafe fn date_or_time_wide(buf: *mut u16, size: usize, name: &str) -> i32 {
    if buf.is_null() || size == 0 {
        return fail(EINVAL);
    }
    if size < 9 {
        *buf = 0;
        return fail(ERANGE);
    }
    let mut tmp = [0u8; 16];
    match base::<FillBuf>(name) {
        Some(f) => {
            f(tmp.as_mut_ptr());
            store(buf, size, &widen(c_str(tmp.as_ptr())))
        }
        None => fail(EINVAL),
    }
}
unsafe extern "win64" fn strdate_s(buf: *mut u8, size: usize) -> i32 {
    date_or_time(buf, size, "_strdate")
}
unsafe extern "win64" fn strtime_s(buf: *mut u8, size: usize) -> i32 {
    date_or_time(buf, size, "_strtime")
}
unsafe extern "win64" fn wstrdate_s(buf: *mut u16, size: usize) -> i32 {
    date_or_time_wide(buf, size, "_strdate")
}
unsafe extern "win64" fn wstrtime_s(buf: *mut u16, size: usize) -> i32 {
    date_or_time_wide(buf, size, "_strtime")
}

// ---------------------------------------------------------------------
// Fichiers temporaires, environnement, divers
// ---------------------------------------------------------------------

unsafe extern "win64" fn tmpnam_s(buf: *mut u8, size: usize) -> i32 {
    match base::<FillBuf>("tmpnam") {
        Some(f) => store_from(buf, size, f(std::ptr::null_mut())),
        None => fail(EINVAL),
    }
}
unsafe extern "win64" fn wtmpnam_s(buf: *mut u16, size: usize) -> i32 {
    match base::<FillBuf>("tmpnam") {
        Some(f) => store_wide_from(buf, size, f(std::ptr::null_mut())),
        None => fail(EINVAL),
    }
}

/// `_mktemp_s(modèle, taille)` : les six « X » finaux remplacés sur place.
unsafe extern "win64" fn mktemp_s(tpl: *mut u8, size: usize) -> i32 {
    if tpl.is_null() || size == 0 || c_str(tpl).len() >= size {
        return fail(EINVAL);
    }
    match base::<FillBuf>("_mktemp") {
        Some(f) if !f(tpl).is_null() => 0,
        _ => fail(EINVAL),
    }
}
unsafe extern "win64" fn wmktemp_s(tpl: *mut u16, size: usize) -> i32 {
    if tpl.is_null() || size == 0 {
        return fail(EINVAL);
    }
    let n = (0..size).position(|i| *tpl.add(i) == 0);
    let Some(n) = n else {
        return fail(EINVAL);
    };
    let wide = std::slice::from_raw_parts(tpl, n);
    let mut narrow = String::from_utf16_lossy(wide).into_bytes();
    narrow.push(0);
    match base::<FillBuf>("_mktemp") {
        Some(f) if !f(narrow.as_mut_ptr()).is_null() => {
            let done = widen(c_str(narrow.as_ptr()));
            if done.len() != n {
                return fail(EINVAL);
            }
            std::ptr::copy_nonoverlapping(done.as_ptr(), tpl, n);
            0
        }
        _ => fail(EINVAL),
    }
}

unsafe extern "win64" fn umask_s(mode: i32, old: *mut i32) -> i32 {
    if old.is_null() || mode & !0x180 != 0 {
        return fail(EINVAL);
    }
    match base::<unsafe extern "win64" fn(i32) -> i32>("_umask") {
        Some(f) => {
            *old = f(mode);
            0
        }
        None => fail(EINVAL),
    }
}

unsafe extern "win64" fn clearerr_s(f: *mut c_void) -> i32 {
    if f.is_null() {
        return fail(EINVAL);
    }
    if let Some(c) = base::<unsafe extern "win64" fn(*mut c_void)>("clearerr") {
        c(f);
    }
    0
}

/// `fread_s(tampon, taille, élément, nombre, flux)` : jamais plus que la
/// place du tampon.
unsafe extern "win64" fn fread_s(
    buf: *mut c_void,
    size: usize,
    elem: usize,
    count: usize,
    f: *mut c_void,
) -> usize {
    if elem == 0 || count == 0 {
        return 0;
    }
    if buf.is_null() || f.is_null() {
        fail(EINVAL);
        return 0;
    }
    let fit = count.min(size / elem);
    if fit == 0 {
        fail(ERANGE);
        return 0;
    }
    type Fread = unsafe extern "win64" fn(*mut c_void, usize, usize, *mut c_void) -> usize;
    base::<Fread>("fread").map_or(0, |r| r(buf, elem, fit, f))
}

unsafe extern "win64" fn dupenv_s(out: *mut *mut u8, len: *mut usize, name: *const u8) -> i32 {
    if out.is_null() || name.is_null() {
        return fail(EINVAL);
    }
    *out = std::ptr::null_mut();
    if !len.is_null() {
        *len = 0;
    }
    let value = base::<unsafe extern "win64" fn(*const u8) -> *const u8>("getenv")
        .map_or(std::ptr::null(), |g| g(name));
    if value.is_null() {
        return 0;
    }
    let text = c_str(value);
    let malloc = base::<unsafe extern "win64" fn(usize) -> *mut u8>("malloc");
    let p = malloc.map_or(std::ptr::null_mut(), |m| m(text.len() + 1));
    if p.is_null() {
        return fail(12); // ENOMEM
    }
    std::ptr::copy_nonoverlapping(text.as_ptr(), p, text.len());
    *p.add(text.len()) = 0;
    *out = p;
    if !len.is_null() {
        *len = text.len() + 1;
    }
    0
}

unsafe extern "win64" fn freea_s(p: *mut c_void) {
    if let Some(f) = base::<unsafe extern "win64" fn(*mut c_void)>("free") {
        f(p);
    }
}

/// Texte de `errno`, précédé de « message: » s'il y en a un, et un saut de
/// ligne : celui de `_strerror`.
unsafe fn error_text(msg: &[u8]) -> Vec<u8> {
    let e = *errno();
    let text = base::<unsafe extern "win64" fn(i32) -> *const u8>("strerror")
        .map_or(&[][..], |f| c_str(f(e)));
    let mut out = Vec::new();
    if !msg.is_empty() {
        out.extend_from_slice(msg);
        out.extend_from_slice(b": ");
    }
    out.extend_from_slice(text);
    out.push(b'\n');
    out
}
unsafe extern "win64" fn strerror_s_legacy(buf: *mut u8, size: usize, msg: *const u8) -> i32 {
    store(buf, size, &error_text(c_str(msg)))
}
unsafe extern "win64" fn wcserror_s_legacy(buf: *mut u16, size: usize, msg: *const u16) -> i32 {
    let m = if msg.is_null() {
        Vec::new()
    } else {
        let n = crate::printf::wide_len(msg);
        String::from_utf16_lossy(std::slice::from_raw_parts(msg, n)).into_bytes()
    };
    store(buf, size, &widen(&error_text(&m)))
}

// ---------------------------------------------------------------------
// Lignes de la console
// ---------------------------------------------------------------------

/// Une ligne de l'entrée standard, sans son saut de ligne.
unsafe fn read_line(buf: *mut u8, size: usize) -> Option<usize> {
    let line = super::crt_stdio::fgets(buf, size.min(i32::MAX as usize) as i32, stdin());
    if line.is_null() {
        *buf = 0;
        return None;
    }
    let mut n = c_str(buf).len();
    if n > 0 && *buf.add(n - 1) == b'\n' {
        n -= 1;
        *buf.add(n) = 0;
    }
    Some(n)
}
unsafe fn read_wide_line(buf: *mut u16, size: usize) -> Option<usize> {
    let line = super::crt_releve::fgetws(buf, size.min(i32::MAX as usize) as i32, stdin().cast());
    if line.is_null() {
        *buf = 0;
        return None;
    }
    let mut n = crate::printf::wide_len(buf);
    if n > 0 && *buf.add(n - 1) == u16::from(b'\n') {
        n -= 1;
        *buf.add(n) = 0;
    }
    Some(n)
}

unsafe extern "win64" fn gets_s(buf: *mut u8, size: usize) -> *mut u8 {
    if buf.is_null() || size == 0 {
        fail(EINVAL);
        return std::ptr::null_mut();
    }
    match read_line(buf, size) {
        Some(_) => buf,
        None => std::ptr::null_mut(),
    }
}
unsafe extern "win64" fn getws_s(buf: *mut u16, size: usize) -> *mut u16 {
    if buf.is_null() || size == 0 {
        fail(EINVAL);
        return std::ptr::null_mut();
    }
    match read_wide_line(buf, size) {
        Some(_) => buf,
        None => std::ptr::null_mut(),
    }
}
unsafe extern "win64" fn cgets_s(buf: *mut u8, size: usize, read: *mut usize) -> i32 {
    if buf.is_null() || size == 0 || read.is_null() {
        return fail(EINVAL);
    }
    *read = read_line(buf, size).unwrap_or(0);
    0
}
unsafe extern "win64" fn cgetws_s(buf: *mut u16, size: usize, read: *mut usize) -> i32 {
    if buf.is_null() || size == 0 || read.is_null() {
        return fail(EINVAL);
    }
    *read = read_wide_line(buf, size).unwrap_or(0);
    0
}

// ---------------------------------------------------------------------
// Tri et recherche avec contexte
// ---------------------------------------------------------------------

type CmpCtx = extern "win64" fn(*mut c_void, *const c_void, *const c_void) -> i32;

unsafe extern "win64" fn qsort_s(
    base_ptr: *mut c_void,
    num: usize,
    width: usize,
    compare: usize,
    ctx: *mut c_void,
) {
    if base_ptr.is_null() || width == 0 || num < 2 || compare == 0 {
        return;
    }
    let cmp: CmpCtx = std::mem::transmute(compare);
    let bytes = std::slice::from_raw_parts_mut(base_ptr as *mut u8, num * width);
    let mut order: Vec<usize> = (0..num).collect();
    order.sort_by(|&a, &b| {
        let pa = bytes.as_ptr().add(a * width) as *const c_void;
        let pb = bytes.as_ptr().add(b * width) as *const c_void;
        cmp(ctx, pa, pb).cmp(&0)
    });
    let snapshot = bytes.to_vec();
    for (i, &src) in order.iter().enumerate() {
        std::ptr::copy_nonoverlapping(
            snapshot.as_ptr().add(src * width),
            bytes.as_mut_ptr().add(i * width),
            width,
        );
    }
}

unsafe extern "win64" fn bsearch_s(
    key: *const c_void,
    base_ptr: *const c_void,
    num: usize,
    width: usize,
    compare: usize,
    ctx: *mut c_void,
) -> *mut c_void {
    if key.is_null() || base_ptr.is_null() || width == 0 || compare == 0 {
        return std::ptr::null_mut();
    }
    let cmp: CmpCtx = std::mem::transmute(compare);
    let (mut lo, mut hi) = (0usize, num);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let p = (base_ptr as *const u8).add(mid * width) as *const c_void;
        match cmp(ctx, key, p) {
            0 => return p as *mut c_void,
            r if r < 0 => hi = mid,
            _ => lo = mid + 1,
        }
    }
    std::ptr::null_mut()
}

unsafe extern "win64" fn lfind_s(
    key: *const c_void,
    base_ptr: *const c_void,
    num: *const u32,
    width: usize,
    compare: usize,
    ctx: *mut c_void,
) -> *mut c_void {
    if key.is_null() || num.is_null() || width == 0 || compare == 0 {
        fail(EINVAL);
        return std::ptr::null_mut();
    }
    let cmp: CmpCtx = std::mem::transmute(compare);
    for i in 0..*num as usize {
        let p = (base_ptr as *const u8).add(i * width) as *const c_void;
        if cmp(ctx, key, p) == 0 {
            return p as *mut c_void;
        }
    }
    std::ptr::null_mut()
}

/// `_lsearch_s` : comme `_lfind_s`, et la clé ajoutée au bout si elle
/// manque (le tableau doit avoir la place).
unsafe extern "win64" fn lsearch_s(
    key: *const c_void,
    base_ptr: *mut c_void,
    num: *mut u32,
    width: usize,
    compare: usize,
    ctx: *mut c_void,
) -> *mut c_void {
    let found = lfind_s(key, base_ptr, num, width, compare, ctx);
    if !found.is_null() || num.is_null() || key.is_null() || width == 0 || compare == 0 {
        return found;
    }
    let end = (base_ptr as *mut u8).add(*num as usize * width);
    std::ptr::copy_nonoverlapping(key as *const u8, end, width);
    *num += 1;
    end as *mut c_void
}

// ---------------------------------------------------------------------
// Conversions multioctets
// ---------------------------------------------------------------------

const TRUNCATE: usize = usize::MAX;

type Mbsrtowcs = unsafe extern "win64" fn(*mut u16, *mut *const u8, usize, *mut c_void) -> usize;
type Wcsrtombs = unsafe extern "win64" fn(*mut u8, *mut *const u16, usize, *mut c_void) -> usize;

/// `mbsrtowcs_s(&n, dst, taille, &src, nombre, état)` : `n` compte le zéro
/// final.
unsafe extern "win64" fn mbsrtowcs_s(
    ret: *mut usize,
    dst: *mut u16,
    size: usize,
    src: *mut *const u8,
    count: usize,
    state: *mut c_void,
) -> i32 {
    if src.is_null() || (dst.is_null() && size != 0) {
        return fail(EINVAL);
    }
    let Some(conv) = base::<Mbsrtowcs>("mbsrtowcs") else {
        return fail(EINVAL);
    };
    if dst.is_null() {
        let n = conv(std::ptr::null_mut(), src, 0, state);
        if n == usize::MAX {
            return fail(EILSEQ);
        }
        if !ret.is_null() {
            *ret = n + 1;
        }
        return 0;
    }
    let room = size - 1;
    let want = if count == TRUNCATE { room } else { count };
    let n = conv(dst, src, want.min(room), state);
    if n == usize::MAX {
        *dst = 0;
        return fail(EILSEQ);
    }
    // La source ne tenait pas sans _TRUNCATE : erreur, chaîne vide.
    if count != TRUNCATE && count > room && !(*src).is_null() {
        *dst = 0;
        return fail(ERANGE);
    }
    *dst.add(n) = 0;
    if !ret.is_null() {
        *ret = n + 1;
    }
    0
}

unsafe extern "win64" fn wcsrtombs_s(
    ret: *mut usize,
    dst: *mut u8,
    size: usize,
    src: *mut *const u16,
    count: usize,
    state: *mut c_void,
) -> i32 {
    if src.is_null() || (dst.is_null() && size != 0) {
        return fail(EINVAL);
    }
    let Some(conv) = base::<Wcsrtombs>("wcsrtombs") else {
        return fail(EINVAL);
    };
    if dst.is_null() {
        let n = conv(std::ptr::null_mut(), src, 0, state);
        if n == usize::MAX {
            return fail(EILSEQ);
        }
        if !ret.is_null() {
            *ret = n + 1;
        }
        return 0;
    }
    let room = size - 1;
    let want = if count == TRUNCATE { room } else { count };
    let n = conv(dst, src, want.min(room), state);
    if n == usize::MAX {
        *dst = 0;
        return fail(EILSEQ);
    }
    if count != TRUNCATE && count > room && !(*src).is_null() {
        *dst = 0;
        return fail(ERANGE);
    }
    *dst.add(n) = 0;
    if !ret.is_null() {
        *ret = n + 1;
    }
    0
}

unsafe extern "win64" fn wctomb_s(ret: *mut i32, mb: *mut u8, size: usize, wc: u16) -> i32 {
    let mut tmp = [0u8; 8];
    let n = match base::<unsafe extern "win64" fn(*mut u8, u16) -> i32>("wctomb") {
        Some(f) => f(tmp.as_mut_ptr(), wc),
        None => -1,
    };
    if n < 0 {
        if !ret.is_null() {
            *ret = -1;
        }
        return fail(EILSEQ);
    }
    if !mb.is_null() {
        if n as usize > size {
            if size > 0 {
                *mb = 0;
            }
            return fail(ERANGE);
        }
        std::ptr::copy_nonoverlapping(tmp.as_ptr(), mb, n as usize);
    }
    if !ret.is_null() {
        *ret = n;
    }
    0
}

// ---------------------------------------------------------------------
// Variantes `_l` (#2137)
// ---------------------------------------------------------------------
//
// Learn (pages printf_s, sprintf_s, _snprintf_s, vprintf_s, scanf_s,
// sscanf_s, _strlwr_s, mbstowcs_s, wcstombs_s, wctomb_s) : chaque variante
// `_l` prend un `_locale_t` juste après le format (ou en dernier pour les
// conversions) et se comporte comme la fonction sans `_l` avec cette
// locale. Notre CRT ne sert que la locale « C » : la locale reçue est
// ignorée, comme dans `_tolower_l` ou les `_mbs*_s_l` (#2128). Liste des
// noms : en-têtes `sec_api` de mingw-w64 (domaine public).

// Mise en forme, liste d'arguments : la locale précède `va`.
unsafe extern "win64" fn vprintf_s_l(fmt: *const u8, _l: usize, va: *const u64) -> i32 {
    vprintf_s(fmt, va)
}
unsafe extern "win64" fn vwprintf_s_l(fmt: *const u16, _l: usize, va: *const u64) -> i32 {
    vwprintf_s(fmt, va)
}
unsafe extern "win64" fn vfprintf_s_l(
    f: *mut c_void,
    fmt: *const u8,
    _l: usize,
    va: *const u64,
) -> i32 {
    vfprintf_s(f, fmt, va)
}
unsafe extern "win64" fn vfwprintf_s_l(
    f: *mut c_void,
    fmt: *const u16,
    _l: usize,
    va: *const u64,
) -> i32 {
    vfwprintf_s(f, fmt, va)
}
unsafe extern "win64" fn vsprintf_s_l(
    buf: *mut u8,
    size: usize,
    fmt: *const u8,
    _l: usize,
    va: *const u64,
) -> i32 {
    vsprintf_s(buf, size, fmt, va)
}
unsafe extern "win64" fn vswprintf_s_l(
    buf: *mut u16,
    size: usize,
    fmt: *const u16,
    _l: usize,
    va: *const u64,
) -> i32 {
    vswprintf_s(buf, size, fmt, va)
}
unsafe extern "win64" fn vsnprintf_s_l(
    buf: *mut u8,
    size: usize,
    count: usize,
    fmt: *const u8,
    _l: usize,
    va: *const u64,
) -> i32 {
    vsnprintf_s(buf, size, count, fmt, va)
}
unsafe extern "win64" fn vsnwprintf_s_l(
    buf: *mut u16,
    size: usize,
    count: usize,
    fmt: *const u16,
    _l: usize,
    va: *const u64,
) -> i32 {
    vsnwprintf_s(buf, size, count, fmt, va)
}
unsafe extern "win64" fn vcprintf_s_l(fmt: *const u8, _l: usize, va: *const u64) -> i32 {
    vcprintf_s(fmt, va)
}
unsafe extern "win64" fn vcwprintf_s_l(fmt: *const u16, _l: usize, va: *const u64) -> i32 {
    vcwprintf_s(fmt, va)
}
/// `__conio_common_vcprintf_s(options, format, locale, va)` (ucrtbase).
unsafe extern "win64" fn conio_common_vcprintf_s(
    options: u64,
    fmt: *const u8,
    l: usize,
    va: *const u64,
) -> i32 {
    super::crt_releve::conio_common_vcprintf(options, fmt, l, va)
}
/// `__conio_common_vcwprintf_s(options, format, locale, va)` (ucrtbase).
unsafe extern "win64" fn conio_common_vcwprintf_s(
    _options: u64,
    fmt: *const u16,
    _l: usize,
    va: *const u64,
) -> i32 {
    vcwprintf_s(fmt, va)
}

// Mise en forme, arguments variables : `a` pointe sur les arguments, la
// locale est la case qui suit le format.
unsafe extern "win64" fn imp_printf_s_l(a: *const u64) -> i32 {
    vprintf_s(*a as *const u8, a.add(2))
}
unsafe extern "win64" fn imp_wprintf_s_l(a: *const u64) -> i32 {
    vwprintf_s(*a as *const u16, a.add(2))
}
unsafe extern "win64" fn imp_fprintf_s_l(a: *const u64) -> i32 {
    vfprintf_s(*a as *mut c_void, *a.add(1) as *const u8, a.add(3))
}
unsafe extern "win64" fn imp_fwprintf_s_l(a: *const u64) -> i32 {
    vfwprintf_s(*a as *mut c_void, *a.add(1) as *const u16, a.add(3))
}
unsafe extern "win64" fn imp_sprintf_s_l(a: *const u64) -> i32 {
    vsprintf_s(
        *a as *mut u8,
        *a.add(1) as usize,
        *a.add(2) as *const u8,
        a.add(4),
    )
}
unsafe extern "win64" fn imp_swprintf_s_l(a: *const u64) -> i32 {
    vswprintf_s(
        *a as *mut u16,
        *a.add(1) as usize,
        *a.add(2) as *const u16,
        a.add(4),
    )
}
unsafe extern "win64" fn imp_snprintf_s_l(a: *const u64) -> i32 {
    vsnprintf_s(
        *a as *mut u8,
        *a.add(1) as usize,
        *a.add(2) as usize,
        *a.add(3) as *const u8,
        a.add(5),
    )
}
unsafe extern "win64" fn imp_snwprintf_s_l(a: *const u64) -> i32 {
    vsnwprintf_s(
        *a as *mut u16,
        *a.add(1) as usize,
        *a.add(2) as usize,
        *a.add(3) as *const u16,
        a.add(5),
    )
}
unsafe extern "win64" fn imp_cprintf_s_l(a: *const u64) -> i32 {
    vcprintf_s(*a as *const u8, a.add(2))
}
unsafe extern "win64" fn imp_cwprintf_s_l(a: *const u64) -> i32 {
    vcwprintf_s(*a as *const u16, a.add(2))
}

crate::varargs_thunk!(printf_s_l_thunk => imp_printf_s_l);
crate::varargs_thunk!(wprintf_s_l_thunk => imp_wprintf_s_l);
crate::varargs_thunk!(fprintf_s_l_thunk => imp_fprintf_s_l);
crate::varargs_thunk!(fwprintf_s_l_thunk => imp_fwprintf_s_l);
crate::varargs_thunk!(sprintf_s_l_thunk => imp_sprintf_s_l);
crate::varargs_thunk!(swprintf_s_l_thunk => imp_swprintf_s_l);
crate::varargs_thunk!(snprintf_s_l_thunk => imp_snprintf_s_l);
crate::varargs_thunk!(snwprintf_s_l_thunk => imp_snwprintf_s_l);
crate::varargs_thunk!(cprintf_s_l_thunk => imp_cprintf_s_l);
crate::varargs_thunk!(cwprintf_s_l_thunk => imp_cwprintf_s_l);

// Lecture.
unsafe extern "win64" fn imp_sscanf_s_l(a: *const u64) -> i32 {
    vsscanf_s(*a as *const u8, *a.add(1) as *const u8, a.add(3))
}
unsafe extern "win64" fn imp_swscanf_s_l(a: *const u64) -> i32 {
    vswscanf_s(*a as *const u16, *a.add(1) as *const u16, a.add(3))
}
unsafe extern "win64" fn imp_snscanf_s_l(a: *const u64) -> i32 {
    ucrt::stdio_common_vsscanf(
        SCAN_S,
        *a as *const u8,
        *a.add(1) as usize,
        *a.add(2) as *const u8,
        0,
        a.add(4),
    )
}
unsafe extern "win64" fn imp_snwscanf_s_l(a: *const u64) -> i32 {
    ucrt::stdio_common_vswscanf(
        SCAN_S,
        *a as *const u16,
        *a.add(1) as usize,
        *a.add(2) as *const u16,
        0,
        a.add(4),
    )
}
unsafe extern "win64" fn imp_fscanf_s_l(a: *const u64) -> i32 {
    vfscanf_s(*a as *mut c_void, *a.add(1) as *const u8, a.add(3))
}
unsafe extern "win64" fn imp_fwscanf_s_l(a: *const u64) -> i32 {
    vfwscanf_s(*a as *mut c_void, *a.add(1) as *const u16, a.add(3))
}
/// `_scanf_s_l` et `_cscanf_s_l` : l'entrée standard, comme `scanf_s`.
unsafe extern "win64" fn imp_scanf_s_l(a: *const u64) -> i32 {
    vscanf_s(*a as *const u8, a.add(2))
}
unsafe extern "win64" fn imp_wscanf_s_l(a: *const u64) -> i32 {
    vwscanf_s(*a as *const u16, a.add(2))
}

crate::varargs_thunk!(sscanf_s_l_thunk => imp_sscanf_s_l);
crate::varargs_thunk!(swscanf_s_l_thunk => imp_swscanf_s_l);
crate::varargs_thunk!(snscanf_s_l_thunk => imp_snscanf_s_l);
crate::varargs_thunk!(snwscanf_s_l_thunk => imp_snwscanf_s_l);
crate::varargs_thunk!(fscanf_s_l_thunk => imp_fscanf_s_l);
crate::varargs_thunk!(fwscanf_s_l_thunk => imp_fwscanf_s_l);
crate::varargs_thunk!(scanf_s_l_thunk => imp_scanf_s_l);
crate::varargs_thunk!(wscanf_s_l_thunk => imp_wscanf_s_l);

// Chaînes et conversions : la locale vient en dernier.
unsafe extern "win64" fn strlwr_s_l(s: *mut u8, size: usize, _l: usize) -> i32 {
    match base::<unsafe extern "win64" fn(*mut u8, usize) -> i32>("_strlwr_s") {
        Some(f) => f(s, size),
        None => fail(EINVAL),
    }
}
unsafe extern "win64" fn strupr_s_l(s: *mut u8, size: usize, _l: usize) -> i32 {
    match base::<unsafe extern "win64" fn(*mut u8, usize) -> i32>("_strupr_s") {
        Some(f) => f(s, size),
        None => fail(EINVAL),
    }
}
type ConvS<D, S> = unsafe extern "win64" fn(*mut usize, *mut D, usize, *const S, usize) -> i32;
unsafe extern "win64" fn mbstowcs_s_l(
    done: *mut usize,
    dst: *mut u16,
    size: usize,
    src: *const u8,
    count: usize,
    _l: usize,
) -> i32 {
    match base::<ConvS<u16, u8>>("mbstowcs_s") {
        Some(f) => f(done, dst, size, src, count),
        None => fail(EINVAL),
    }
}
unsafe extern "win64" fn wcstombs_s_l(
    done: *mut usize,
    dst: *mut u8,
    size: usize,
    src: *const u16,
    count: usize,
    _l: usize,
) -> i32 {
    match base::<ConvS<u8, u16>>("wcstombs_s") {
        Some(f) => f(done, dst, size, src, count),
        None => fail(EINVAL),
    }
}
unsafe extern "win64" fn strncat_s_l(
    dst: *mut u8,
    size: usize,
    src: *const u8,
    count: usize,
    _l: usize,
) -> i32 {
    match base::<unsafe extern "win64" fn(*mut u8, usize, *const u8, usize) -> i32>("strncat_s") {
        Some(f) => f(dst, size, src, count),
        None => fail(EINVAL),
    }
}
unsafe extern "win64" fn strncpy_s_l(
    dst: *mut u8,
    size: usize,
    src: *const u8,
    count: usize,
    _l: usize,
) -> i32 {
    match base::<unsafe extern "win64" fn(*mut u8, usize, *const u8, usize) -> i32>("strncpy_s") {
        Some(f) => f(dst, size, src, count),
        None => fail(EINVAL),
    }
}
unsafe extern "win64" fn wcsncat_s_l(
    dst: *mut u16,
    size: usize,
    src: *const u16,
    count: usize,
    _l: usize,
) -> i32 {
    match base::<unsafe extern "win64" fn(*mut u16, usize, *const u16, usize) -> i32>("wcsncat_s") {
        Some(f) => f(dst, size, src, count),
        None => fail(EINVAL),
    }
}
unsafe extern "win64" fn wcsncpy_s_l(
    dst: *mut u16,
    size: usize,
    src: *const u16,
    count: usize,
    _l: usize,
) -> i32 {
    match base::<unsafe extern "win64" fn(*mut u16, usize, *const u16, usize) -> i32>("wcsncpy_s") {
        Some(f) => f(dst, size, src, count),
        None => fail(EINVAL),
    }
}
unsafe extern "win64" fn wcsnset_s_l(
    s: *mut u16,
    size: usize,
    c: u16,
    count: usize,
    _l: usize,
) -> i32 {
    match base::<unsafe extern "win64" fn(*mut u16, usize, u16, usize) -> i32>("_wcsnset_s") {
        Some(f) => f(s, size, c, count),
        None => fail(EINVAL),
    }
}
unsafe extern "win64" fn wcsset_s_l(s: *mut u16, size: usize, c: u16, _l: usize) -> i32 {
    match base::<unsafe extern "win64" fn(*mut u16, usize, u16) -> i32>("_wcsset_s") {
        Some(f) => f(s, size, c),
        None => fail(EINVAL),
    }
}
unsafe extern "win64" fn strtok_s_l(
    s: *mut u8,
    delim: *const u8,
    ctx: *mut *mut u8,
    _l: usize,
) -> *mut u8 {
    match base::<unsafe extern "win64" fn(*mut u8, *const u8, *mut *mut u8) -> *mut u8>("strtok_s")
    {
        Some(f) => f(s, delim, ctx),
        None => std::ptr::null_mut(),
    }
}
unsafe extern "win64" fn wcstok_s_l(
    s: *mut u16,
    delim: *const u16,
    ctx: *mut *mut u16,
    _l: usize,
) -> *mut u16 {
    match base::<unsafe extern "win64" fn(*mut u16, *const u16, *mut *mut u16) -> *mut u16>(
        "wcstok_s",
    ) {
        Some(f) => f(s, delim, ctx),
        None => std::ptr::null_mut(),
    }
}
unsafe extern "win64" fn wctomb_s_l(
    ret: *mut i32,
    mb: *mut u8,
    size: usize,
    wc: u16,
    _l: usize,
) -> i32 {
    wctomb_s(ret, mb, size, wc)
}

/// Noms servis par ce module (tests, campagne).
#[cfg(test)]
pub(crate) const NAMES: &[&str] = &[
    "printf_s",
    "fprintf_s",
    "wprintf_s",
    "fwprintf_s",
    "sprintf_s",
    "_cprintf_s",
    "_cwprintf_s",
    "vprintf_s",
    "vfprintf_s",
    "vwprintf_s",
    "vfwprintf_s",
    "vsprintf_s",
    "vswprintf_s",
    "vsnprintf_s",
    "_vsnprintf_s",
    "_vsnwprintf_s",
    "_vcprintf_s",
    "_vcwprintf_s",
    "sscanf_s",
    "_snscanf_s",
    "_snwscanf_s",
    "fscanf_s",
    "fwscanf_s",
    "scanf_s",
    "wscanf_s",
    "_cscanf_s",
    "_cwscanf_s",
    "vsscanf_s",
    "vswscanf_s",
    "vfscanf_s",
    "vfwscanf_s",
    "vscanf_s",
    "vwscanf_s",
    "asctime_s",
    "_wasctime_s",
    "_ctime32_s",
    "_ctime64_s",
    "_wctime32_s",
    "_wctime64_s",
    "_gmtime32_s",
    "_localtime32_s",
    "_strdate_s",
    "_strtime_s",
    "_wstrdate_s",
    "_wstrtime_s",
    "tmpnam_s",
    "_wtmpnam_s",
    "_mktemp_s",
    "_wmktemp_s",
    "_umask_s",
    "clearerr_s",
    "fread_s",
    "_fread_nolock_s",
    "_dupenv_s",
    "_freea_s",
    "_strerror_s",
    "__wcserror_s",
    "gets_s",
    "_getws_s",
    "_cgets_s",
    "_cgetws_s",
    "qsort_s",
    "bsearch_s",
    "_lfind_s",
    "_lsearch_s",
    "mbsrtowcs_s",
    "wcsrtombs_s",
    "wctomb_s",
    // Variantes _l (#2137).
    "_printf_s_l",
    "_wprintf_s_l",
    "_fprintf_s_l",
    "_fwprintf_s_l",
    "_sprintf_s_l",
    "_swprintf_s_l",
    "_snprintf_s_l",
    "_snwprintf_s_l",
    "_cprintf_s_l",
    "_cwprintf_s_l",
    "_vprintf_s_l",
    "_vwprintf_s_l",
    "_vfprintf_s_l",
    "_vfwprintf_s_l",
    "_vsprintf_s_l",
    "_vswprintf_s_l",
    "_vsnprintf_s_l",
    "_vsnwprintf_s_l",
    "_vcprintf_s_l",
    "_vcwprintf_s_l",
    "_sscanf_s_l",
    "_swscanf_s_l",
    "_snscanf_s_l",
    "_snwscanf_s_l",
    "_fscanf_s_l",
    "_fwscanf_s_l",
    "_scanf_s_l",
    "_wscanf_s_l",
    "_cscanf_s_l",
    "_cwscanf_s_l",
    "_strlwr_s_l",
    "_strupr_s_l",
    "_mbstowcs_s_l",
    "_wcstombs_s_l",
    "_wctomb_s_l",
    "__conio_common_vcprintf_s",
    "__conio_common_vcwprintf_s",
    "_strncat_s_l",
    "_strncpy_s_l",
    "_strtok_s_l",
    "_wcstok_s_l",
    "_wcsncat_s_l",
    "_wcsncpy_s_l",
    "_wcsnset_s_l",
    "_wcsset_s_l",
];

pub fn exports() -> std::collections::HashMap<&'static str, usize> {
    crate::exports! {
        // Mise en forme.
        "printf_s" => fn printf_s_thunk, "fprintf_s" => fn fprintf_s_thunk,
        "wprintf_s" => fn wprintf_s_thunk, "fwprintf_s" => fn fwprintf_s_thunk,
        "sprintf_s" => fn sprintf_s_thunk,
        "_cprintf_s" => fn cprintf_s_thunk, "_cwprintf_s" => fn cwprintf_s_thunk,
        "vprintf_s" => fn vprintf_s, "vfprintf_s" => fn vfprintf_s,
        "vwprintf_s" => fn vwprintf_s, "vfwprintf_s" => fn vfwprintf_s,
        "vsprintf_s" => fn vsprintf_s, "vswprintf_s" => fn vswprintf_s,
        "vsnprintf_s" => fn vsnprintf_s, "_vsnprintf_s" => fn vsnprintf_s,
        "_vsnwprintf_s" => fn vsnwprintf_s,
        "_vcprintf_s" => fn vcprintf_s, "_vcwprintf_s" => fn vcwprintf_s,
        // Lecture.
        "sscanf_s" => fn sscanf_s_thunk, "_snscanf_s" => fn snscanf_s_thunk,
        "_snwscanf_s" => fn snwscanf_s_thunk,
        "fscanf_s" => fn fscanf_s_thunk, "fwscanf_s" => fn fwscanf_s_thunk,
        "scanf_s" => fn scanf_s_thunk, "wscanf_s" => fn wscanf_s_thunk,
        "_cscanf_s" => fn scanf_s_thunk, "_cwscanf_s" => fn wscanf_s_thunk,
        "vsscanf_s" => fn vsscanf_s, "vswscanf_s" => fn vswscanf_s,
        "vfscanf_s" => fn vfscanf_s, "vfwscanf_s" => fn vfwscanf_s,
        "vscanf_s" => fn vscanf_s, "vwscanf_s" => fn vwscanf_s,
        // Dates et heures.
        "asctime_s" => fn asctime_s, "_wasctime_s" => fn wasctime_s,
        "_ctime32_s" => fn ctime32_s, "_ctime64_s" => fn ctime64_s,
        "_wctime32_s" => fn wctime32_s, "_wctime64_s" => fn wctime64_s,
        "_gmtime32_s" => fn gmtime32_s, "_localtime32_s" => fn localtime32_s,
        "_strdate_s" => fn strdate_s, "_strtime_s" => fn strtime_s,
        "_wstrdate_s" => fn wstrdate_s, "_wstrtime_s" => fn wstrtime_s,
        // Fichiers, environnement, erreurs.
        "tmpnam_s" => fn tmpnam_s, "_wtmpnam_s" => fn wtmpnam_s,
        "_mktemp_s" => fn mktemp_s, "_wmktemp_s" => fn wmktemp_s,
        "_umask_s" => fn umask_s, "clearerr_s" => fn clearerr_s,
        "fread_s" => fn fread_s, "_fread_nolock_s" => fn fread_s,
        "_dupenv_s" => fn dupenv_s, "_freea_s" => fn freea_s,
        "_strerror_s" => fn strerror_s_legacy, "__wcserror_s" => fn wcserror_s_legacy,
        // Console.
        "gets_s" => fn gets_s, "_getws_s" => fn getws_s,
        "_cgets_s" => fn cgets_s, "_cgetws_s" => fn cgetws_s,
        // Tri et recherche.
        "qsort_s" => fn qsort_s, "bsearch_s" => fn bsearch_s,
        "_lfind_s" => fn lfind_s, "_lsearch_s" => fn lsearch_s,
        // Multioctets.
        "mbsrtowcs_s" => fn mbsrtowcs_s, "wcsrtombs_s" => fn wcsrtombs_s, "wctomb_s" => fn wctomb_s,
        // Variantes _l (#2137).
        "_printf_s_l" => fn printf_s_l_thunk, "_wprintf_s_l" => fn wprintf_s_l_thunk,
        "_fprintf_s_l" => fn fprintf_s_l_thunk, "_fwprintf_s_l" => fn fwprintf_s_l_thunk,
        "_sprintf_s_l" => fn sprintf_s_l_thunk, "_swprintf_s_l" => fn swprintf_s_l_thunk,
        "_snprintf_s_l" => fn snprintf_s_l_thunk, "_snwprintf_s_l" => fn snwprintf_s_l_thunk,
        "_cprintf_s_l" => fn cprintf_s_l_thunk, "_cwprintf_s_l" => fn cwprintf_s_l_thunk,
        "_vprintf_s_l" => fn vprintf_s_l, "_vwprintf_s_l" => fn vwprintf_s_l,
        "_vfprintf_s_l" => fn vfprintf_s_l, "_vfwprintf_s_l" => fn vfwprintf_s_l,
        "_vsprintf_s_l" => fn vsprintf_s_l, "_vswprintf_s_l" => fn vswprintf_s_l,
        "_vsnprintf_s_l" => fn vsnprintf_s_l, "_vsnwprintf_s_l" => fn vsnwprintf_s_l,
        "_vcprintf_s_l" => fn vcprintf_s_l, "_vcwprintf_s_l" => fn vcwprintf_s_l,
        "__conio_common_vcprintf_s" => fn conio_common_vcprintf_s,
        "__conio_common_vcwprintf_s" => fn conio_common_vcwprintf_s,
        "_sscanf_s_l" => fn sscanf_s_l_thunk, "_swscanf_s_l" => fn swscanf_s_l_thunk,
        "_snscanf_s_l" => fn snscanf_s_l_thunk, "_snwscanf_s_l" => fn snwscanf_s_l_thunk,
        "_fscanf_s_l" => fn fscanf_s_l_thunk, "_fwscanf_s_l" => fn fwscanf_s_l_thunk,
        "_scanf_s_l" => fn scanf_s_l_thunk, "_wscanf_s_l" => fn wscanf_s_l_thunk,
        "_cscanf_s_l" => fn scanf_s_l_thunk, "_cwscanf_s_l" => fn wscanf_s_l_thunk,
        "_strlwr_s_l" => fn strlwr_s_l, "_strupr_s_l" => fn strupr_s_l,
        "_mbstowcs_s_l" => fn mbstowcs_s_l, "_wcstombs_s_l" => fn wcstombs_s_l,
        "_wctomb_s_l" => fn wctomb_s_l,
        "_strncat_s_l" => fn strncat_s_l, "_strncpy_s_l" => fn strncpy_s_l,
        "_strtok_s_l" => fn strtok_s_l, "_wcstok_s_l" => fn wcstok_s_l,
        "_wcsncat_s_l" => fn wcsncat_s_l, "_wcsncpy_s_l" => fn wcsncpy_s_l,
        "_wcsnset_s_l" => fn wcsnset_s_l, "_wcsset_s_l" => fn wcsset_s_l,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1980 : tous les noms sont servis par msvcrt (donc msvcr120).
    #[test]
    fn famille_s_servie_par_msvcrt() {
        for n in NAMES {
            assert!(super::super::lookup("msvcrt", n).is_some(), "msvcrt!{n}");
        }
    }

    /// #2137 : les variantes `_l` font comme leur sœur sans `_l`, la
    /// locale (ici nulle) étant la case qui suit le format.
    #[test]
    fn variantes_l_comme_sans_locale() {
        unsafe {
            let mut buf = [0u8; 16];
            let a: [u64; 5] = [
                buf.as_mut_ptr() as u64,
                buf.len() as u64,
                c"%d-%s".as_ptr() as u64,
                0,
                42,
            ];
            let mut a = a.to_vec();
            a.push(c"ok".as_ptr() as u64);
            assert_eq!(imp_sprintf_s_l(a.as_ptr()), 5);
            assert_eq!(&buf[..6], b"42-ok\0");
            // _snprintf_s_l avec _TRUNCATE : tronqué, -1.
            let mut petit = [0u8; 4];
            let a: [u64; 6] = [
                petit.as_mut_ptr() as u64,
                petit.len() as u64,
                usize::MAX as u64,
                c"%s".as_ptr() as u64,
                0,
                c"abcdef".as_ptr() as u64,
            ];
            assert_eq!(imp_snprintf_s_l(a.as_ptr()), -1);
            assert_eq!(&petit, b"abc\0");
            // _sscanf_s_l : %s suivi de la taille, après la locale.
            let mut n: i32 = 0;
            let mut mot = [0u8; 8];
            let a: [u64; 6] = [
                c"7 bonjour".as_ptr() as u64,
                c"%d %s".as_ptr() as u64,
                0,
                &mut n as *mut i32 as u64,
                mot.as_mut_ptr() as u64,
                mot.len() as u64,
            ];
            assert_eq!(imp_sscanf_s_l(a.as_ptr()), 2);
            assert_eq!(n, 7);
            assert_eq!(&mot[..8], b"bonjour\0");
            // _vsnprintf_s_l : la liste d'arguments suit la locale.
            let va: [u64; 1] = [12];
            let mut out = [0u8; 8];
            assert_eq!(
                vsnprintf_s_l(
                    out.as_mut_ptr(),
                    8,
                    7,
                    c"n=%d".as_ptr().cast(),
                    0,
                    va.as_ptr()
                ),
                4
            );
            assert_eq!(&out[..5], b"n=12\0");
            // _strupr_s_l et _wctomb_s_l.
            let mut s = *b"abc\0";
            assert_eq!(strupr_s_l(s.as_mut_ptr(), 4, 0), 0);
            assert_eq!(&s, b"ABC\0");
            let mut ret = 0;
            let mut mb = [0u8; 4];
            assert_eq!(wctomb_s_l(&mut ret, mb.as_mut_ptr(), 4, b'Z' as u16, 0), 0);
            assert_eq!((ret, mb[0]), (1, b'Z'));
            // _wcstombs_s_l.
            let w: Vec<u16> = "hé".encode_utf16().chain([0]).collect();
            let mut done = 0usize;
            let mut dst = [0u8; 8];
            assert_eq!(
                wcstombs_s_l(&mut done, dst.as_mut_ptr(), 8, w.as_ptr(), usize::MAX, 0),
                0
            );
            assert_eq!(dst[0], b'h');
            // _strncpy_s_l puis _strtok_s_l.
            let mut d = [0u8; 8];
            assert_eq!(
                strncpy_s_l(d.as_mut_ptr(), 8, c"a,b".as_ptr().cast(), 3, 0),
                0
            );
            let mut ctx = std::ptr::null_mut();
            let t = strtok_s_l(d.as_mut_ptr(), c",".as_ptr().cast(), &mut ctx, 0);
            assert_eq!(std::ffi::CStr::from_ptr(t.cast()).to_bytes(), b"a");
        }
    }

    /// sscanf_s : %d, et %s suivi de la taille du tampon.
    #[test]
    fn sscanf_s_lit_avec_la_taille() {
        let mut n: i32 = 0;
        let mut word = [0u8; 8];
        let args: [u64; 3] = [
            &mut n as *mut i32 as u64,
            word.as_mut_ptr() as u64,
            word.len() as u64,
        ];
        let got = unsafe {
            vsscanf_s(
                c"42 bonjour".as_ptr().cast(),
                c"%d %s".as_ptr().cast(),
                args.as_ptr(),
            )
        };
        assert_eq!(got, 2);
        assert_eq!(n, 42);
        assert_eq!(unsafe { c_str(word.as_ptr()) }, b"bonjour");
        // Tampon trop petit pour le mot : rien n'est écrit au-delà.
        let mut small = [0xAAu8; 4];
        let args: [u64; 3] = [
            &mut n as *mut i32 as u64,
            small.as_mut_ptr() as u64,
            small.len() as u64,
        ];
        let got = unsafe {
            vsscanf_s(
                c"7 bonjour".as_ptr().cast(),
                c"%d %s".as_ptr().cast(),
                args.as_ptr(),
            )
        };
        assert!(got < 2, "{got}");
    }

    /// sprintf_s : chaîne vide et -1 si la sortie ne tient pas.
    #[test]
    fn vsprintf_s_borne() {
        let mut buf = [0xAAu8; 8];
        let args: [u64; 1] = [1234];
        let n = unsafe {
            vsprintf_s(
                buf.as_mut_ptr(),
                buf.len(),
                c"n=%d".as_ptr().cast(),
                args.as_ptr(),
            )
        };
        assert_eq!(n, 6);
        assert_eq!(unsafe { c_str(buf.as_ptr()) }, b"n=1234");
        let args: [u64; 1] = [123456789];
        let n = unsafe {
            vsprintf_s(
                buf.as_mut_ptr(),
                buf.len(),
                c"n=%d".as_ptr().cast(),
                args.as_ptr(),
            )
        };
        assert_eq!(n, -1);
        assert_eq!(buf[0], 0);
    }

    extern "win64" fn by_value(ctx: *mut c_void, a: *const c_void, b: *const c_void) -> i32 {
        unsafe {
            *(ctx as *mut u32) += 1;
            (*(a as *const i32)).cmp(&*(b as *const i32)) as i32
        }
    }

    /// qsort_s et bsearch_s passent le contexte au comparateur.
    #[test]
    fn tri_et_recherche_avec_contexte() {
        let mut v = [5i32, 1, 4, 2, 3];
        let mut calls = 0u32;
        let ctx = &mut calls as *mut u32 as *mut c_void;
        unsafe {
            qsort_s(
                v.as_mut_ptr().cast(),
                v.len(),
                4,
                by_value as *const () as usize,
                ctx,
            )
        };
        assert_eq!(v, [1, 2, 3, 4, 5]);
        assert!(calls > 0);
        let key = 4i32;
        let p = unsafe {
            bsearch_s(
                (&key as *const i32).cast(),
                v.as_ptr().cast(),
                v.len(),
                4,
                by_value as *const () as usize,
                ctx,
            )
        };
        assert_eq!(p as usize, v.as_ptr() as usize + 12);
        let mut items = [7i32, 8, 0];
        let mut num = 2u32;
        let nine = 9i32;
        unsafe {
            lsearch_s(
                (&nine as *const i32).cast(),
                items.as_mut_ptr().cast(),
                &mut num,
                4,
                by_value as *const () as usize,
                ctx,
            )
        };
        assert_eq!((num, items), (3, [7, 8, 9]));
    }

    /// Copies bornées : ERANGE et chaîne vide quand le tampon est court.
    #[test]
    fn copies_bornees() {
        let mut buf = [0xAAu8; 4];
        assert_eq!(unsafe { store(buf.as_mut_ptr(), 4, b"abc") }, 0);
        assert_eq!(&buf, b"abc\0");
        assert_eq!(unsafe { store(buf.as_mut_ptr(), 4, b"abcd") }, ERANGE);
        assert_eq!(buf[0], 0);
        let mut d = [0u8; 9];
        assert_eq!(unsafe { strdate_s(d.as_mut_ptr(), 9) }, 0);
        assert_eq!(d[2], b'/');
        assert_eq!(unsafe { strdate_s(d.as_mut_ptr(), 8) }, ERANGE);
    }
}
