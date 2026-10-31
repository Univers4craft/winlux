//! Points d'entrée propres à la CRT universelle (ucrtbase et ses contrats
//! `api-ms-win-crt-*`) qu'importent les programmes Visual C++ 2015 et plus :
//! la famille `__stdio_common_*` derrière printf/scanf, les fonctions de
//! chaînes « sécurisées » (`*_s`), les variantes larges et les fonctions
//! mathématiques en simple précision. Le lanceur de .NET (hostfxr,
//! hostpolicy) et coreclr en dépendent.

use std::ffi::c_void;

use crate::dlls::crt_runtime::errno;

const EINVAL: i32 = 22;
const ERANGE: i32 = 34;
const STRUNCATE: i32 = 80;
const TRUNCATE: usize = usize::MAX;

// Options passées en premier argument des fonctions `__stdio_common_*`.
const STANDARD_SNPRINTF_BEHAVIOR: u64 = 0x2;
const LEGACY_WIDE_SPECIFIERS: u64 = 0x4;
const LEGACY_THREE_DIGIT_EXPONENTS: u64 = 0x10;

fn set_errno(e: i32) -> i32 {
    unsafe { *errno() = e };
    e
}

fn opts(options: u64, wide: bool) -> crate::printf::Opts {
    crate::printf::Opts {
        // Sans LEGACY_WIDE_SPECIFIERS, %s désigne une chaîne étroite même
        // dans wprintf (mode ISO C).
        wide: wide && options & LEGACY_WIDE_SPECIFIERS != 0,
        three_digit_exp: options & LEGACY_THREE_DIGIT_EXPONENTS != 0,
    }
}

unsafe fn format_narrow(options: u64, fmt: *const u8, va: *const u64) -> Vec<u8> {
    let mut v = crate::thunks::VaList::new(va);
    crate::printf::format(
        std::slice::from_raw_parts(fmt, crate::printf::narrow_len(fmt)),
        &mut v,
        opts(options, false),
    )
}

unsafe fn format_wide(options: u64, fmt: *const u16, va: *const u64) -> Vec<u16> {
    let mut v = crate::thunks::VaList::new(va);
    let text = String::from_utf16_lossy(std::slice::from_raw_parts(
        fmt,
        crate::printf::wide_len(fmt),
    ));
    let out = crate::printf::format(text.as_bytes(), &mut v, opts(options, true));
    String::from_utf8_lossy(&out).encode_utf16().collect()
}

/// `vsnprintf` (comportement standard) ou `_vsnprintf` (historique).
unsafe fn store_snprintf<T: Copy + Default>(
    options: u64,
    buf: *mut T,
    count: usize,
    out: &[T],
) -> i32 {
    let n = out.len();
    if buf.is_null() {
        return if count == 0 { n as i32 } else { -1 };
    }
    if options & STANDARD_SNPRINTF_BEHAVIOR != 0 {
        if count > 0 {
            let k = n.min(count - 1);
            std::ptr::copy_nonoverlapping(out.as_ptr(), buf, k);
            *buf.add(k) = T::default();
        }
        return n as i32;
    }
    // Historique : pas de zéro final quand la sortie remplit tout, -1 si
    // elle déborde.
    let k = n.min(count);
    std::ptr::copy_nonoverlapping(out.as_ptr(), buf, k);
    if n < count {
        *buf.add(n) = T::default();
    }
    if n > count {
        -1
    } else {
        n as i32
    }
}

/// `vsprintf_s` : erreur et chaîne vide si la sortie ne tient pas.
unsafe fn store_sprintf_s<T: Copy + Default>(buf: *mut T, size: usize, out: &[T]) -> i32 {
    if buf.is_null() || size == 0 {
        set_errno(EINVAL);
        return -1;
    }
    if out.len() >= size {
        *buf = T::default();
        set_errno(ERANGE);
        return -1;
    }
    std::ptr::copy_nonoverlapping(out.as_ptr(), buf, out.len());
    *buf.add(out.len()) = T::default();
    out.len() as i32
}

/// `vsnprintf_s` : `max_count` limite la copie, `_TRUNCATE` tronque à la
/// taille du tampon ; -1 signale une troncature.
unsafe fn store_snprintf_s<T: Copy + Default>(
    buf: *mut T,
    size: usize,
    max_count: usize,
    out: &[T],
) -> i32 {
    if buf.is_null() || size == 0 {
        set_errno(EINVAL);
        return -1;
    }
    let n = out.len();
    let limit = if max_count == TRUNCATE {
        size - 1
    } else {
        max_count.min(size - 1)
    };
    if n <= limit {
        std::ptr::copy_nonoverlapping(out.as_ptr(), buf, n);
        *buf.add(n) = T::default();
        return n as i32;
    }
    if max_count == TRUNCATE || max_count < size {
        std::ptr::copy_nonoverlapping(out.as_ptr(), buf, limit);
        *buf.add(limit) = T::default();
        return -1;
    }
    *buf = T::default();
    set_errno(ERANGE);
    -1
}

pub unsafe extern "win64" fn stdio_common_vsprintf(
    options: u64,
    buf: *mut u8,
    count: usize,
    fmt: *const u8,
    _locale: usize,
    va: *const u64,
) -> i32 {
    if fmt.is_null() {
        set_errno(EINVAL);
        return -1;
    }
    store_snprintf(options, buf, count, &format_narrow(options, fmt, va))
}
pub unsafe extern "win64" fn stdio_common_vsprintf_s(
    options: u64,
    buf: *mut u8,
    size: usize,
    fmt: *const u8,
    _locale: usize,
    va: *const u64,
) -> i32 {
    if fmt.is_null() {
        set_errno(EINVAL);
        return -1;
    }
    store_sprintf_s(buf, size, &format_narrow(options, fmt, va))
}
pub unsafe extern "win64" fn stdio_common_vsnprintf_s(
    options: u64,
    buf: *mut u8,
    size: usize,
    max_count: usize,
    fmt: *const u8,
    _locale: usize,
    va: *const u64,
) -> i32 {
    if fmt.is_null() {
        set_errno(EINVAL);
        return -1;
    }
    store_snprintf_s(buf, size, max_count, &format_narrow(options, fmt, va))
}
pub unsafe extern "win64" fn stdio_common_vswprintf(
    options: u64,
    buf: *mut u16,
    count: usize,
    fmt: *const u16,
    _locale: usize,
    va: *const u64,
) -> i32 {
    if fmt.is_null() {
        set_errno(EINVAL);
        return -1;
    }
    store_snprintf(options, buf, count, &format_wide(options, fmt, va))
}
pub unsafe extern "win64" fn stdio_common_vswprintf_s(
    options: u64,
    buf: *mut u16,
    size: usize,
    fmt: *const u16,
    _locale: usize,
    va: *const u64,
) -> i32 {
    if fmt.is_null() {
        set_errno(EINVAL);
        return -1;
    }
    store_sprintf_s(buf, size, &format_wide(options, fmt, va))
}
pub unsafe extern "win64" fn stdio_common_vsnwprintf_s(
    options: u64,
    buf: *mut u16,
    size: usize,
    max_count: usize,
    fmt: *const u16,
    _locale: usize,
    va: *const u64,
) -> i32 {
    if fmt.is_null() {
        set_errno(EINVAL);
        return -1;
    }
    store_snprintf_s(buf, size, max_count, &format_wide(options, fmt, va))
}
pub unsafe extern "win64" fn stdio_common_vfprintf(
    options: u64,
    stream: *mut c_void,
    fmt: *const u8,
    _locale: usize,
    va: *const u64,
) -> i32 {
    if fmt.is_null() {
        set_errno(EINVAL);
        return -1;
    }
    let out = format_narrow(options, fmt, va);
    crate::dlls::crt_stdio::write_bytes(stream, &out)
}
pub unsafe extern "win64" fn stdio_common_vfwprintf(
    options: u64,
    stream: *mut c_void,
    fmt: *const u16,
    _locale: usize,
    va: *const u64,
) -> i32 {
    if fmt.is_null() {
        set_errno(EINVAL);
        return -1;
    }
    let out = String::from_utf16_lossy(&format_wide(options, fmt, va));
    match crate::dlls::crt_stdio::write_bytes(stream, out.as_bytes()) {
        n if n < 0 => n,
        _ => out.encode_utf16().count() as i32,
    }
}
pub unsafe extern "win64" fn stdio_common_vsscanf(
    _options: u64,
    buf: *const u8,
    len: usize,
    fmt: *const u8,
    _locale: usize,
    va: *const u64,
) -> i32 {
    if buf.is_null() || fmt.is_null() {
        set_errno(EINVAL);
        return -1;
    }
    let n = if len == usize::MAX {
        crate::printf::narrow_len(buf)
    } else {
        (0..len).position(|i| *buf.add(i) == 0).unwrap_or(len)
    };
    let mut v = crate::thunks::VaList::new(va);
    crate::dlls::crt_scan::scan(
        std::slice::from_raw_parts(buf, n),
        std::slice::from_raw_parts(fmt, crate::printf::narrow_len(fmt)),
        &mut v,
    )
}

// ---------- Flux ----------

pub unsafe extern "win64" fn wfsopen(
    path: *const u16,
    mode: *const u16,
    _share: i32,
) -> *mut c_void {
    crate::dlls::crt_stdio::wfopen(path, mode)
}
/// Les opérations sur un flux Winlux sont déjà atomiques (verrou interne).
pub extern "win64" fn lock_file(_stream: *mut c_void) {}
pub extern "win64" fn fputwc(c: u16, stream: *mut c_void) -> u16 {
    let text = String::from_utf16_lossy(&[c]);
    if unsafe { crate::dlls::crt_stdio::write_bytes(stream, text.as_bytes()) } < 0 {
        0xFFFF // WEOF
    } else {
        c
    }
}
pub extern "win64" fn flushall() -> i32 {
    unsafe { crate::dlls::crt_stdio::fflush(std::ptr::null_mut()) };
    0
}

/// `_get_stream_buffer_pointers` : la STL de MSVC lit et avance directement
/// les pointeurs du tampon d'un `FILE`. Les flux Winlux ne sont pas tamponnés
/// côté programme : chaque flux reçoit un tampon vide, si bien que la STL
/// repasse par fgetc/fputc à chaque caractère.
pub unsafe extern "win64" fn get_stream_buffer_pointers(
    stream: *mut c_void,
    base: *mut *mut *mut u8,
    ptr: *mut *mut *mut u8,
    count: *mut *mut i32,
) -> i32 {
    #[repr(C)]
    struct Empty {
        base: *mut u8,
        ptr: *mut u8,
        count: i32,
    }
    if stream.is_null() {
        return set_errno(EINVAL);
    }
    static SLOTS: std::sync::Mutex<Vec<(usize, usize)>> = std::sync::Mutex::new(Vec::new());
    let mut slots = crate::dlls::lock(&SLOTS);
    let addr = match slots.iter().find(|(s, _)| *s == stream as usize) {
        Some(&(_, a)) => a,
        None => {
            let a = Box::into_raw(Box::new(Empty {
                base: std::ptr::null_mut(),
                ptr: std::ptr::null_mut(),
                count: 0,
            })) as usize;
            slots.push((stream as usize, a));
            a
        }
    };
    let e = addr as *mut Empty;
    if !base.is_null() {
        *base = std::ptr::addr_of_mut!((*e).base);
    }
    if !ptr.is_null() {
        *ptr = std::ptr::addr_of_mut!((*e).ptr);
    }
    if !count.is_null() {
        *count = std::ptr::addr_of_mut!((*e).count);
    }
    0
}

// ---------- Chaînes ----------

fn lower16(c: u16) -> u16 {
    char::from_u32(c as u32)
        .and_then(|ch| {
            let mut l = ch.to_lowercase();
            match (l.next(), l.next()) {
                (Some(x), None) if (x as u32) < 0x10000 => Some(x as u16),
                _ => None,
            }
        })
        .unwrap_or(c)
}
fn upper16(c: u16) -> u16 {
    char::from_u32(c as u32)
        .and_then(|ch| {
            let mut u = ch.to_uppercase();
            match (u.next(), u.next()) {
                (Some(x), None) if (x as u32) < 0x10000 => Some(x as u16),
                _ => None,
            }
        })
        .unwrap_or(c)
}

pub extern "win64" fn towlower(c: u16) -> u16 {
    lower16(c)
}
pub extern "win64" fn towupper(c: u16) -> u16 {
    upper16(c)
}
pub extern "win64" fn iswascii(c: u16) -> i32 {
    (c < 0x80) as i32
}
pub extern "win64" fn iswspace(c: u16) -> i32 {
    char::from_u32(c as u32).is_some_and(char::is_whitespace) as i32
}
pub extern "win64" fn iswupper(c: u16) -> i32 {
    char::from_u32(c as u32).is_some_and(char::is_uppercase) as i32
}

pub unsafe extern "win64" fn wcsnicmp(a: *const u16, b: *const u16, n: usize) -> i32 {
    if a.is_null() || b.is_null() {
        set_errno(EINVAL);
        return i32::MAX;
    }
    for i in 0..n {
        let (x, y) = (lower16(*a.add(i)), lower16(*b.add(i)));
        if x != y {
            return x as i32 - y as i32;
        }
        if x == 0 {
            break;
        }
    }
    0
}
pub unsafe extern "win64" fn wcsicmp(a: *const u16, b: *const u16) -> i32 {
    wcsnicmp(a, b, usize::MAX)
}
pub unsafe extern "win64" fn wcsnlen(s: *const u16, max: usize) -> usize {
    (0..max).position(|i| *s.add(i) == 0).unwrap_or(max)
}
pub unsafe extern "win64" fn wcsdup(s: *const u16) -> *mut u16 {
    if s.is_null() {
        return std::ptr::null_mut();
    }
    let n = crate::printf::wide_len(s) + 1;
    let p = libc::malloc(n * 2) as *mut u16;
    if !p.is_null() {
        std::ptr::copy_nonoverlapping(s, p, n);
    }
    p
}
/// `__strncnt` : longueur d'une chaîne limitée à `n` octets.
pub unsafe extern "win64" fn strncnt(s: *const u8, n: usize) -> usize {
    (0..n).position(|i| *s.add(i) == 0).unwrap_or(n)
}

unsafe fn len_of<T: Copy + Default + PartialEq>(s: *const T, max: usize) -> usize {
    (0..max)
        .position(|i| *s.add(i) == T::default())
        .unwrap_or(max)
}

/// Copie sûre commune à strcpy_s, strncpy_s, wcscpy_s, wcsncpy_s.
unsafe fn copy_s<T: Copy + Default + PartialEq>(
    dst: *mut T,
    size: usize,
    src: *const T,
    count: usize,
) -> i32 {
    if dst.is_null() || size == 0 {
        return set_errno(EINVAL);
    }
    if src.is_null() {
        *dst = T::default();
        return set_errno(EINVAL);
    }
    let n = len_of(src, if count == TRUNCATE { size } else { count });
    if n >= size {
        if count == TRUNCATE {
            std::ptr::copy_nonoverlapping(src, dst, size - 1);
            *dst.add(size - 1) = T::default();
            return STRUNCATE;
        }
        *dst = T::default();
        return set_errno(ERANGE);
    }
    std::ptr::copy(src, dst, n);
    *dst.add(n) = T::default();
    0
}
/// Concaténation sûre commune à strcat_s, strncat_s, wcscat_s, wcsncat_s.
unsafe fn cat_s<T: Copy + Default + PartialEq>(
    dst: *mut T,
    size: usize,
    src: *const T,
    count: usize,
) -> i32 {
    if dst.is_null() || size == 0 {
        return set_errno(EINVAL);
    }
    let used = len_of(dst as *const T, size);
    if used == size {
        // Destination non terminée : erreur comme l'UCRT.
        *dst = T::default();
        return set_errno(EINVAL);
    }
    let r = copy_s(dst.add(used), size - used, src, count);
    if r != 0 && r != STRUNCATE {
        *dst = T::default();
    }
    r
}

pub unsafe extern "win64" fn strcpy_s(d: *mut u8, size: usize, s: *const u8) -> i32 {
    copy_s(d, size, s, usize::MAX - 1)
}
pub unsafe extern "win64" fn strncpy_s(d: *mut u8, size: usize, s: *const u8, n: usize) -> i32 {
    copy_s(d, size, s, n)
}
pub unsafe extern "win64" fn strcat_s(d: *mut u8, size: usize, s: *const u8) -> i32 {
    cat_s(d, size, s, usize::MAX - 1)
}
pub unsafe extern "win64" fn strncat_s(d: *mut u8, size: usize, s: *const u8, n: usize) -> i32 {
    cat_s(d, size, s, n)
}
pub unsafe extern "win64" fn wcscpy_s(d: *mut u16, size: usize, s: *const u16) -> i32 {
    copy_s(d, size, s, usize::MAX - 1)
}
pub unsafe extern "win64" fn wcsncpy_s(d: *mut u16, size: usize, s: *const u16, n: usize) -> i32 {
    copy_s(d, size, s, n)
}
pub unsafe extern "win64" fn wcscat_s(d: *mut u16, size: usize, s: *const u16) -> i32 {
    cat_s(d, size, s, usize::MAX - 1)
}
pub unsafe extern "win64" fn wcsncat_s(d: *mut u16, size: usize, s: *const u16, n: usize) -> i32 {
    cat_s(d, size, s, n)
}

// ---------- Conversions ----------

/// Analyse un entier à la manière de strtoul : blancs, signe, préfixe 0x.
unsafe fn parse_wide_int(s: *const u16, end: *mut *const u16, base: i32) -> (bool, u64, bool) {
    let mut p = s;
    while iswspace(*p) != 0 {
        p = p.add(1);
    }
    let mut neg = false;
    if *p == b'-' as u16 || *p == b'+' as u16 {
        neg = *p == b'-' as u16;
        p = p.add(1);
    }
    let mut base = base as u32;
    let hex_prefix = *p == b'0' as u16 && (*p.add(1) | 0x20) == b'x' as u16;
    if (base == 0 || base == 16) && hex_prefix && (*p.add(2) as u8 as char).is_ascii_hexdigit() {
        p = p.add(2);
        base = 16;
    } else if base == 0 {
        base = if *p == b'0' as u16 { 8 } else { 10 };
    }
    let start = p;
    let mut value: u64 = 0;
    let mut overflow = false;
    while let Some(d) = char::from_u32(*p as u32).and_then(|c| c.to_digit(base)) {
        match value
            .checked_mul(base as u64)
            .and_then(|v| v.checked_add(d as u64))
        {
            Some(v) => value = v,
            None => overflow = true,
        }
        p = p.add(1);
    }
    if !end.is_null() {
        *end = if p == start { s } else { p };
    }
    (neg, value, overflow)
}

pub unsafe extern "win64" fn wcstoul(s: *const u16, end: *mut *const u16, base: i32) -> u32 {
    let (neg, v, overflow) = parse_wide_int(s, end, base);
    if overflow || v > u32::MAX as u64 {
        set_errno(ERANGE);
        return u32::MAX;
    }
    if neg {
        (v as u32).wrapping_neg()
    } else {
        v as u32
    }
}
pub unsafe extern "win64" fn wcstoui64(s: *const u16, end: *mut *const u16, base: i32) -> u64 {
    let (neg, v, overflow) = parse_wide_int(s, end, base);
    if overflow {
        set_errno(ERANGE);
        return u64::MAX;
    }
    if neg {
        v.wrapping_neg()
    } else {
        v
    }
}
pub unsafe extern "win64" fn wcstol(s: *const u16, end: *mut *const u16, base: i32) -> i32 {
    let (neg, v, overflow) = parse_wide_int(s, end, base);
    let limit = if neg { 1u64 << 31 } else { (1u64 << 31) - 1 };
    if overflow || v > limit {
        set_errno(ERANGE);
        return if neg { i32::MIN } else { i32::MAX };
    }
    if neg {
        (v as i64).wrapping_neg() as i32
    } else {
        v as i32
    }
}
pub unsafe extern "win64" fn wtoi(s: *const u16) -> i32 {
    if s.is_null() {
        return 0;
    }
    let (neg, v, _) = parse_wide_int(s, std::ptr::null_mut(), 10);
    let v = v as i64;
    (if neg { v.wrapping_neg() } else { v }) as i32
}
pub unsafe extern "win64" fn ltow_s(value: i32, buf: *mut u16, size: usize, radix: i32) -> i32 {
    if buf.is_null() || size == 0 || !(2..=36).contains(&radix) {
        return set_errno(EINVAL);
    }
    let neg = radix == 10 && value < 0;
    let mut mag = if neg {
        (value as i64).unsigned_abs()
    } else {
        value as u32 as u64
    };
    let mut digits = Vec::new();
    loop {
        digits.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(mag % radix as u64) as usize] as u16);
        mag /= radix as u64;
        if mag == 0 {
            break;
        }
    }
    if neg {
        digits.push(b'-' as u16);
    }
    if digits.len() >= size {
        *buf = 0;
        return set_errno(ERANGE);
    }
    for (i, &d) in digits.iter().rev().enumerate() {
        *buf.add(i) = d;
    }
    *buf.add(digits.len()) = 0;
    0
}

pub unsafe extern "win64" fn wcserror_s(buf: *mut u16, size: usize, err: i32) -> i32 {
    let text = crate::dlls::cstr(crate::dlls::crt_runtime::strerror(err));
    let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
    match copy_s(buf, size, wide.as_ptr(), TRUNCATE) {
        STRUNCATE => 0,
        r => r,
    }
}

/// `wcsftime` : mêmes conversions que `strftime`, en caractères larges.
pub unsafe extern "win64" fn wcsftime(
    buf: *mut u16,
    max: usize,
    fmt: *const u16,
    t: *const crate::dlls::crt_runtime::Tm,
) -> usize {
    if buf.is_null() || fmt.is_null() || t.is_null() || max == 0 {
        set_errno(EINVAL);
        return 0;
    }
    let f = std::ffi::CString::new(crate::dlls::wstr(fmt)).unwrap_or_default();
    let mut tmp = vec![0u8; max * 4 + 1];
    let n = crate::dlls::crt_runtime::strftime(tmp.as_mut_ptr(), tmp.len(), f.as_ptr().cast(), t);
    let wide: Vec<u16> = String::from_utf8_lossy(&tmp[..n]).encode_utf16().collect();
    if n == 0 || wide.len() >= max {
        *buf = 0;
        return 0;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), buf, wide.len());
    *buf.add(wide.len()) = 0;
    wide.len()
}

// ---------- Exécution, exceptions, paramètres ----------

/// Filtre SEH du code de démarrage CRT : sans gestionnaire de signal
/// installé, l'exception continue sa recherche (EXCEPTION_CONTINUE_SEARCH).
pub extern "win64" fn seh_filter(_code: u32, _pointers: *mut c_void) -> i32 {
    0
}
pub extern "win64" fn terminate() -> ! {
    crate::log_error!("crt", "terminate() appelée : le programme s'arrête");
    eprintln!("winlux: le programme a appelé terminate() et s'arrête.");
    crate::dlls::kernel32::flush_and_exit(3)
}
pub extern "win64" fn invoke_watson(
    _expr: *const u16,
    _func: *const u16,
    _file: *const u16,
    _line: u32,
    _reserved: usize,
) -> ! {
    crate::log_error!("crt", "paramètre invalide fatal (_invoke_watson)");
    eprintln!("winlux: la CRT a détecté un paramètre invalide et arrête le programme.");
    crate::dlls::kernel32::flush_and_exit(0xC000_0409u32 as i32)
}
/// Sous Windows, le gestionnaire par défaut termine le processus ; ici on
/// journalise et l'appelant renvoie son code d'erreur (EINVAL).
pub extern "win64" fn invalid_parameter_noinfo() {
    crate::log_warn!("crt", "paramètre invalide signalé à la CRT");
}
pub extern "win64" fn invalid_parameter_noinfo_noreturn() -> ! {
    invoke_watson(std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, 0)
}
/// Aucun gestionnaire `new` n'est installé : l'allocation échoue.
pub extern "win64" fn callnewh(_size: usize) -> i32 {
    0
}
pub extern "win64" fn register_thread_local_exe_atexit_callback(_cb: usize) {}

static CONTROL_WORD: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x0009_001F);
/// `_controlfp_s` : mot de contrôle virtuel (masques d'exception, arrondi,
/// précision) ; le mot par défaut masque toutes les exceptions flottantes.
pub unsafe extern "win64" fn controlfp_s(current: *mut u32, new: u32, mask: u32) -> i32 {
    use std::sync::atomic::Ordering;
    let mask = mask & !0x0008_0000; // _EM_DENORMAL n'est pas modifiable
    let word = (CONTROL_WORD.load(Ordering::SeqCst) & !mask) | (new & mask);
    CONTROL_WORD.store(word, Ordering::SeqCst);
    if !current.is_null() {
        *current = word;
    }
    0
}

/// `___lc_locale_name_func` : noms de locale par catégorie ; la locale « C »
/// de démarrage n'a pas de nom (pointeurs nuls).
pub extern "win64" fn lc_locale_name_func() -> *const *const u16 {
    static NAMES: [usize; 6] = [0; 6];
    NAMES.as_ptr().cast()
}
pub extern "win64" fn lock_locales() {}

// ---------- Mathématiques en simple précision et C99 ----------

macro_rules! unary {
    ($($name:ident: $t:ty => $f:expr),* $(,)?) => {
        $(pub extern "win64" fn $name(x: $t) -> $t { let f: fn($t) -> $t = $f; f(x) })*
    };
}
macro_rules! binary {
    ($($name:ident: $t:ty => $f:expr),* $(,)?) => {
        $(pub extern "win64" fn $name(x: $t, y: $t) -> $t { let f: fn($t, $t) -> $t = $f; f(x, y) })*
    };
}
unary! {
    acosf: f32 => f32::acos, asinf: f32 => f32::asin, atanf: f32 => f32::atan,
    cosf: f32 => f32::cos, sinf: f32 => f32::sin, tanf: f32 => f32::tan,
    coshf: f32 => f32::cosh, sinhf: f32 => f32::sinh, tanhf: f32 => f32::tanh,
    acoshf: f32 => f32::acosh, asinhf: f32 => f32::asinh, atanhf: f32 => f32::atanh,
    expf: f32 => f32::exp, logf: f32 => f32::ln, log10f: f32 => f32::log10, log2f: f32 => f32::log2,
    sqrtf: f32 => f32::sqrt, ceilf: f32 => f32::ceil, floorf: f32 => f32::floor,
    roundf: f32 => f32::round, truncf: f32 => f32::trunc, cbrtf: f32 => f32::cbrt,
    acosh: f64 => f64::acosh, asinh: f64 => f64::asinh, atanh: f64 => f64::atanh,
    log2: f64 => f64::log2, cbrt: f64 => f64::cbrt, round: f64 => f64::round, trunc: f64 => f64::trunc,
}
binary! {
    atan2f: f32 => f32::atan2, powf: f32 => f32::powf, fmodf: f32 => |x, y| x % y,
    copysignf: f32 => f32::copysign, copysign: f64 => f64::copysign,
}
pub extern "win64" fn fma(x: f64, y: f64, z: f64) -> f64 {
    x.mul_add(y, z)
}
pub extern "win64" fn fmaf(x: f32, y: f32, z: f32) -> f32 {
    x.mul_add(y, z)
}
pub unsafe extern "win64" fn modff(x: f32, int_part: *mut f32) -> f32 {
    let t = x.trunc();
    if !int_part.is_null() {
        *int_part = t;
    }
    if x.is_infinite() {
        0.0f32.copysign(x)
    } else {
        x - t
    }
}
pub extern "win64" fn ilogbf(x: f32) -> i32 {
    if x == 0.0 {
        return i32::MIN; // FP_ILOGB0
    }
    if x.is_nan() {
        return i32::MIN;
    }
    if x.is_infinite() {
        return i32::MAX;
    }
    let exp = (x.abs().to_bits() >> 23) as i32;
    if exp == 0 {
        // Dénormal : l'exposant réel se lit après normalisation.
        ((x.abs() * 8_388_608.0).to_bits() >> 23) as i32 - 127 - 23
    } else {
        exp - 127
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "__stdio_common_vsprintf" => fn stdio_common_vsprintf,
        "__stdio_common_vsprintf_s" => fn stdio_common_vsprintf_s,
        "__stdio_common_vsnprintf_s" => fn stdio_common_vsnprintf_s,
        "__stdio_common_vswprintf" => fn stdio_common_vswprintf,
        "__stdio_common_vswprintf_s" => fn stdio_common_vswprintf_s,
        "__stdio_common_vsnwprintf_s" => fn stdio_common_vsnwprintf_s,
        "__stdio_common_vfprintf" => fn stdio_common_vfprintf,
        "__stdio_common_vfwprintf" => fn stdio_common_vfwprintf,
        "__stdio_common_vsscanf" => fn stdio_common_vsscanf,
        "_wfsopen" => fn wfsopen, "_lock_file" => fn lock_file, "_unlock_file" => fn lock_file,
        "fputwc" => fn fputwc, "_flushall" => fn flushall,
        "_get_stream_buffer_pointers" => fn get_stream_buffer_pointers,
        "towlower" => fn towlower, "towupper" => fn towupper,
        "iswascii" => fn iswascii, "iswspace" => fn iswspace, "iswupper" => fn iswupper,
        "_wcsicmp" => fn wcsicmp, "_wcsnicmp" => fn wcsnicmp, "wcsnlen" => fn wcsnlen,
        "_wcsdup" => fn wcsdup, "__strncnt" => fn strncnt,
        "strcpy_s" => fn strcpy_s, "strncpy_s" => fn strncpy_s, "strcat_s" => fn strcat_s, "strncat_s" => fn strncat_s,
        "wcscpy_s" => fn wcscpy_s, "wcsncpy_s" => fn wcsncpy_s, "wcscat_s" => fn wcscat_s, "wcsncat_s" => fn wcsncat_s,
        "wcstoul" => fn wcstoul, "_wcstoui64" => fn wcstoui64, "wcstol" => fn wcstol,
        "_wtoi" => fn wtoi, "_wtol" => fn wtoi, "_ltow_s" => fn ltow_s,
        "_wcserror_s" => fn wcserror_s, "wcsftime" => fn wcsftime,
        "_seh_filter_exe" => fn seh_filter, "_seh_filter_dll" => fn seh_filter,
        "terminate" => fn terminate, "_invoke_watson" => fn invoke_watson,
        "_invalid_parameter_noinfo" => fn invalid_parameter_noinfo,
        "_invalid_parameter_noinfo_noreturn" => fn invalid_parameter_noinfo_noreturn,
        "_callnewh" => fn callnewh,
        "_register_thread_local_exe_atexit_callback" => fn register_thread_local_exe_atexit_callback,
        "_controlfp_s" => fn controlfp_s,
        "___lc_locale_name_func" => fn lc_locale_name_func,
        "_lock_locales" => fn lock_locales, "_unlock_locales" => fn lock_locales,
        "acosf" => fn acosf, "asinf" => fn asinf, "atanf" => fn atanf, "atan2f" => fn atan2f,
        "cosf" => fn cosf, "sinf" => fn sinf, "tanf" => fn tanf,
        "coshf" => fn coshf, "sinhf" => fn sinhf, "tanhf" => fn tanhf,
        "acoshf" => fn acoshf, "asinhf" => fn asinhf, "atanhf" => fn atanhf,
        "acosh" => fn acosh, "asinh" => fn asinh, "atanh" => fn atanh,
        "expf" => fn expf, "logf" => fn logf, "log10f" => fn log10f, "log2f" => fn log2f, "log2" => fn log2,
        "powf" => fn powf, "fmodf" => fn fmodf, "sqrtf" => fn sqrtf,
        "ceilf" => fn ceilf, "floorf" => fn floorf, "roundf" => fn roundf, "round" => fn round,
        "truncf" => fn truncf, "trunc" => fn trunc, "cbrtf" => fn cbrtf, "cbrt" => fn cbrt,
        "copysignf" => fn copysignf, "copysign" => fn copysign, "_copysign" => fn copysign,
        "fma" => fn fma, "fmaf" => fn fmaf, "modff" => fn modff, "ilogbf" => fn ilogbf,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    #[test]
    fn vswprintf_suit_les_options_de_la_crt() {
        let fmt = w("%s=%d");
        let name = w("dotnet");
        let args = [name.as_ptr() as u64, 9];
        let mut buf = [0u16; 32];
        let n = unsafe {
            stdio_common_vswprintf(
                LEGACY_WIDE_SPECIFIERS | STANDARD_SNPRINTF_BEHAVIOR,
                buf.as_mut_ptr(),
                buf.len(),
                fmt.as_ptr(),
                0,
                args.as_ptr(),
            )
        };
        assert_eq!(n, 8);
        assert_eq!(String::from_utf16_lossy(&buf[..8]), "dotnet=9");
        assert_eq!(buf[8], 0);
        // Taille seule demandée (tampon nul).
        let need = unsafe {
            stdio_common_vswprintf(
                LEGACY_WIDE_SPECIFIERS,
                std::ptr::null_mut(),
                0,
                fmt.as_ptr(),
                0,
                args.as_ptr(),
            )
        };
        assert_eq!(need, 8);
    }

    #[test]
    fn snprintf_standard_tronque_et_historique_renvoie_moins_un() {
        let fmt = b"abcdef\0";
        let mut buf = [0xAAu8; 4];
        let n = unsafe {
            stdio_common_vsprintf(
                STANDARD_SNPRINTF_BEHAVIOR,
                buf.as_mut_ptr(),
                4,
                fmt.as_ptr(),
                0,
                std::ptr::null(),
            )
        };
        assert_eq!((n, &buf), (6, b"abc\0"));
        let mut buf = [0xAAu8; 4];
        let n = unsafe {
            stdio_common_vsprintf(0, buf.as_mut_ptr(), 4, fmt.as_ptr(), 0, std::ptr::null())
        };
        assert_eq!((n, &buf), (-1, b"abcd"));
    }

    #[test]
    fn sprintf_s_et_snprintf_s_signalent_le_debordement() {
        let fmt = b"abcdef\0";
        let mut buf = [0xAAu8; 4];
        let n = unsafe {
            stdio_common_vsprintf_s(0, buf.as_mut_ptr(), 4, fmt.as_ptr(), 0, std::ptr::null())
        };
        assert_eq!((n, buf[0]), (-1, 0));
        let mut buf = [0xAAu8; 4];
        let n = unsafe {
            stdio_common_vsnprintf_s(
                0,
                buf.as_mut_ptr(),
                4,
                TRUNCATE,
                fmt.as_ptr(),
                0,
                std::ptr::null(),
            )
        };
        assert_eq!((n, &buf), (-1, b"abc\0"));
    }

    #[test]
    fn copies_securisees() {
        let mut buf = [0u16; 6];
        let src = w("hostfxr");
        unsafe {
            assert_eq!(wcscpy_s(buf.as_mut_ptr(), 6, src.as_ptr()), ERANGE);
            assert_eq!(buf[0], 0);
            assert_eq!(
                wcsncpy_s(buf.as_mut_ptr(), 6, src.as_ptr(), TRUNCATE),
                STRUNCATE
            );
            assert_eq!(String::from_utf16_lossy(&buf[..5]), "hostf");
            let a = w("ab");
            let b = w("cd");
            let mut d = [0u16; 8];
            wcscpy_s(d.as_mut_ptr(), 8, a.as_ptr());
            assert_eq!(wcscat_s(d.as_mut_ptr(), 8, b.as_ptr()), 0);
            assert_eq!(String::from_utf16_lossy(&d[..4]), "abcd");
            let mut n = [0u8; 8];
            assert_eq!(strcpy_s(n.as_mut_ptr(), 8, b"net\0".as_ptr()), 0);
            assert_eq!(strncat_s(n.as_mut_ptr(), 8, b"9.0.13\0".as_ptr(), 2), 0);
            assert_eq!(&n[..6], b"net9.\0");
        }
    }

    #[test]
    fn comparaisons_et_conversions_larges() {
        let a = w("CoreCLR.DLL");
        let b = w("coreclr.dll");
        unsafe {
            assert_eq!(wcsicmp(a.as_ptr(), b.as_ptr()), 0);
            assert!(wcsnicmp(w("abc").as_ptr(), w("abd").as_ptr(), 3) < 0);
            assert_eq!(wcsnicmp(w("abc").as_ptr(), w("abd").as_ptr(), 2), 0);
            assert_eq!(wtoi(w("  -42x").as_ptr()), -42);
            let s = w("0x1F rest");
            let mut end = std::ptr::null();
            assert_eq!(wcstoul(s.as_ptr(), &mut end, 0), 31);
            assert_eq!(end, s.as_ptr().add(4));
            assert_eq!(
                wcstoui64(w("18446744073709551615").as_ptr(), std::ptr::null_mut(), 10),
                u64::MAX
            );
            let mut buf = [0u16; 8];
            assert_eq!(ltow_s(-255, buf.as_mut_ptr(), 8, 10), 0);
            assert_eq!(crate::dlls::wstr(buf.as_ptr()), "-255");
            assert_eq!(ltow_s(255, buf.as_mut_ptr(), 8, 16), 0);
            assert_eq!(crate::dlls::wstr(buf.as_ptr()), "ff");
        }
        assert_eq!(towupper('é' as u16), 'É' as u16);
        assert_eq!(towlower('A' as u16), 'a' as u16);
    }

    #[test]
    fn mathematiques_simple_precision() {
        assert_eq!(ceilf(1.2), 2.0);
        assert_eq!(roundf(-2.5), -3.0);
        assert_eq!(fmodf(7.5, 2.0), 1.5);
        assert_eq!(ilogbf(8.0), 3);
        assert_eq!(ilogbf(0.75), -1);
        let mut i = 0.0;
        assert_eq!(unsafe { modff(3.25, &mut i) }, 0.25);
        assert_eq!(i, 3.0);
        assert_eq!(fma(2.0, 3.0, 1.0), 7.0);
    }

    #[test]
    fn controlfp_s_garde_le_mot_virtuel() {
        let mut cw = 0;
        unsafe {
            controlfp_s(&mut cw, 0, 0);
            let initial = cw;
            controlfp_s(&mut cw, 0x100, 0x300);
            assert_eq!(cw & 0x300, 0x100);
            controlfp_s(&mut cw, initial, 0xFFFF_FFFF);
            assert_eq!(cw, initial);
        }
    }
}
