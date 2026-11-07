//! Fonctions de la CRT (msvcrt et ucrtbase) demandées par de vrais jeux
//! compilés avec MinGW-w64 ou llvm-mingw (SuperTuxKart, Warzone 2100) et
//! par leurs bibliothèques (SDL, libpng, freetype, libcurl, libc++…).
//!
//! setjmp/longjmp, mathématiques C99, conversions multioctets, classes de
//! caractères avec locale, stat/findfirst 64 bits, chemins larges.

#![allow(clippy::missing_safety_doc)]

use crate::dlls::crt_runtime::{crt_errno, errno};
use std::ffi::c_void;
use std::sync::Mutex;

// ---------- setjmp / longjmp ----------
//
// Tampon (jmp_buf de 256 octets) : Frame, Rbx, Rsp, Rbp, Rsi, Rdi, R12-R15,
// Rip, MxCsr/FpCsr, puis Xmm6-Xmm15 — la disposition de _JUMP_BUFFER.
// Le second argument (cadre pour le déroulement SEH) est mémorisé mais
// longjmp restaure directement les registres, comme le mode non-SEH de MinGW.
core::arch::global_asm!(
    ".globl winlux_crt_setjmp",
    ".p2align 4",
    "winlux_crt_setjmp:",
    "mov [rcx], rdx",
    "mov [rcx+0x08], rbx",
    "lea rax, [rsp+8]",
    "mov [rcx+0x10], rax",
    "mov [rcx+0x18], rbp",
    "mov [rcx+0x20], rsi",
    "mov [rcx+0x28], rdi",
    "mov [rcx+0x30], r12",
    "mov [rcx+0x38], r13",
    "mov [rcx+0x40], r14",
    "mov [rcx+0x48], r15",
    "mov rax, [rsp]",
    "mov [rcx+0x50], rax",
    "stmxcsr [rcx+0x58]",
    "fnstcw [rcx+0x5c]",
    "movdqu [rcx+0x60], xmm6",
    "movdqu [rcx+0x70], xmm7",
    "movdqu [rcx+0x80], xmm8",
    "movdqu [rcx+0x90], xmm9",
    "movdqu [rcx+0xa0], xmm10",
    "movdqu [rcx+0xb0], xmm11",
    "movdqu [rcx+0xc0], xmm12",
    "movdqu [rcx+0xd0], xmm13",
    "movdqu [rcx+0xe0], xmm14",
    "movdqu [rcx+0xf0], xmm15",
    "xor eax, eax",
    "ret",
    ".globl winlux_crt_longjmp",
    ".p2align 4",
    "winlux_crt_longjmp:",
    "mov eax, edx",
    "test eax, eax",
    "jnz 1f",
    "inc eax",
    "1:",
    "mov rbx, [rcx+0x08]",
    "mov rsp, [rcx+0x10]",
    "mov rbp, [rcx+0x18]",
    "mov rsi, [rcx+0x20]",
    "mov rdi, [rcx+0x28]",
    "mov r12, [rcx+0x30]",
    "mov r13, [rcx+0x38]",
    "mov r14, [rcx+0x40]",
    "mov r15, [rcx+0x48]",
    "ldmxcsr [rcx+0x58]",
    "fldcw [rcx+0x5c]",
    "movdqu xmm6, [rcx+0x60]",
    "movdqu xmm7, [rcx+0x70]",
    "movdqu xmm8, [rcx+0x80]",
    "movdqu xmm9, [rcx+0x90]",
    "movdqu xmm10, [rcx+0xa0]",
    "movdqu xmm11, [rcx+0xb0]",
    "movdqu xmm12, [rcx+0xc0]",
    "movdqu xmm13, [rcx+0xd0]",
    "movdqu xmm14, [rcx+0xe0]",
    "movdqu xmm15, [rcx+0xf0]",
    "jmp qword ptr [rcx+0x50]",
);
extern "win64" {
    fn winlux_crt_setjmp(buf: *mut u8, frame: usize) -> i32;
    fn winlux_crt_longjmp(buf: *mut u8, value: i32) -> !;
}

// ---------- Mathématiques C99 (glibc libm) ----------

mod m {
    extern "C" {
        pub fn nextafter(x: f64, y: f64) -> f64;
        pub fn nextafterf(x: f32, y: f32) -> f32;
        pub fn remainder(x: f64, y: f64) -> f64;
        pub fn remainderf(x: f32, y: f32) -> f32;
        pub fn logb(x: f64) -> f64;
        pub fn logbf(x: f32) -> f32;
        pub fn nearbyint(x: f64) -> f64;
        pub fn nearbyintf(x: f32) -> f32;
        pub fn rint(x: f64) -> f64;
        pub fn rintf(x: f32) -> f32;
        pub fn remquo(x: f64, y: f64, q: *mut i32) -> f64;
        pub fn erf(x: f64) -> f64;
        pub fn erfc(x: f64) -> f64;
        pub fn tgamma(x: f64) -> f64;
        pub fn lgamma(x: f64) -> f64;
        pub fn scalbn(x: f64, n: i32) -> f64;
        pub fn scalbnf(x: f32, n: i32) -> f32;
        pub fn fdim(x: f64, y: f64) -> f64;
    }
}
extern "win64" fn hypot(x: f64, y: f64) -> f64 {
    x.hypot(y)
}
extern "win64" fn hypotf(x: f32, y: f32) -> f32 {
    x.hypot(y)
}
extern "win64" fn exp2(x: f64) -> f64 {
    x.exp2()
}
extern "win64" fn exp2f(x: f32) -> f32 {
    x.exp2()
}
extern "win64" fn expm1(x: f64) -> f64 {
    x.exp_m1()
}
extern "win64" fn expm1f(x: f32) -> f32 {
    x.exp_m1()
}
extern "win64" fn log1p(x: f64) -> f64 {
    x.ln_1p()
}
extern "win64" fn log1pf(x: f32) -> f32 {
    x.ln_1p()
}
extern "win64" fn rint(x: f64) -> f64 {
    unsafe { m::rint(x) }
}
extern "win64" fn rintf(x: f32) -> f32 {
    unsafe { m::rintf(x) }
}
extern "win64" fn nearbyint(x: f64) -> f64 {
    unsafe { m::nearbyint(x) }
}
extern "win64" fn nearbyintf(x: f32) -> f32 {
    unsafe { m::nearbyintf(x) }
}
/// `long` vaut 32 bits sous Windows : lrint/lround rendent un i32.
extern "win64" fn lrint(x: f64) -> i32 {
    unsafe { m::rint(x) as i32 }
}
extern "win64" fn lrintf(x: f32) -> i32 {
    unsafe { m::rintf(x) as i32 }
}
extern "win64" fn llrint(x: f64) -> i64 {
    unsafe { m::rint(x) as i64 }
}
extern "win64" fn llrintf(x: f32) -> i64 {
    unsafe { m::rintf(x) as i64 }
}
extern "win64" fn lround(x: f64) -> i32 {
    x.round() as i32
}
extern "win64" fn lroundf(x: f32) -> i32 {
    x.round() as i32
}
extern "win64" fn llround(x: f64) -> i64 {
    x.round() as i64
}
extern "win64" fn llroundf(x: f32) -> i64 {
    x.round() as i64
}
extern "win64" fn nextafter(x: f64, y: f64) -> f64 {
    unsafe { m::nextafter(x, y) }
}
extern "win64" fn nextafterf(x: f32, y: f32) -> f32 {
    unsafe { m::nextafterf(x, y) }
}
extern "win64" fn remainder(x: f64, y: f64) -> f64 {
    unsafe { m::remainder(x, y) }
}
extern "win64" fn remainderf(x: f32, y: f32) -> f32 {
    unsafe { m::remainderf(x, y) }
}
extern "win64" fn remquo(x: f64, y: f64, q: *mut i32) -> f64 {
    let mut tmp = 0;
    let r = unsafe { m::remquo(x, y, &mut tmp) };
    if !q.is_null() {
        unsafe { *q = tmp };
    }
    r
}
extern "win64" fn logb(x: f64) -> f64 {
    unsafe { m::logb(x) }
}
extern "win64" fn logbf(x: f32) -> f32 {
    unsafe { m::logbf(x) }
}
extern "win64" fn erf(x: f64) -> f64 {
    unsafe { m::erf(x) }
}
extern "win64" fn erfc(x: f64) -> f64 {
    unsafe { m::erfc(x) }
}
extern "win64" fn tgamma(x: f64) -> f64 {
    unsafe { m::tgamma(x) }
}
extern "win64" fn lgamma(x: f64) -> f64 {
    unsafe { m::lgamma(x) }
}
extern "win64" fn scalbn(x: f64, n: i32) -> f64 {
    unsafe { m::scalbn(x, n) }
}
extern "win64" fn scalbnf(x: f32, n: i32) -> f32 {
    unsafe { m::scalbnf(x, n) }
}
extern "win64" fn fdim(x: f64, y: f64) -> f64 {
    unsafe { m::fdim(x, y) }
}
extern "win64" fn fmax(x: f64, y: f64) -> f64 {
    x.max(y)
}
extern "win64" fn fmin(x: f64, y: f64) -> f64 {
    x.min(y)
}
extern "win64" fn fmaxf(x: f32, y: f32) -> f32 {
    x.max(y)
}
extern "win64" fn fminf(x: f32, y: f32) -> f32 {
    x.min(y)
}
extern "win64" fn exp10(x: f64) -> f64 {
    10f64.powf(x)
}

// ---------- Conversions de nombres ----------
//
// La CRT de Winlux tourne dans la locale « C » de glibc : le point décimal
// est « . », comme dans la locale C de Windows.

unsafe extern "win64" fn strtof(s: *const u8, end: *mut *mut u8) -> f32 {
    libc::strtof(s.cast(), end.cast())
}
unsafe extern "win64" fn strtod_l(s: *const u8, end: *mut *mut u8, _loc: usize) -> f64 {
    libc::strtod(s.cast(), end.cast())
}
unsafe extern "win64" fn strtof_l(s: *const u8, end: *mut *mut u8, _loc: usize) -> f32 {
    libc::strtof(s.cast(), end.cast())
}
unsafe extern "win64" fn strtoi64(s: *const u8, end: *mut *mut u8, base: i32) -> i64 {
    libc::strtoll(s.cast(), end.cast(), base)
}
unsafe extern "win64" fn strtoui64(s: *const u8, end: *mut *mut u8, base: i32) -> u64 {
    libc::strtoull(s.cast(), end.cast(), base)
}
unsafe extern "win64" fn strtoi64_l(s: *const u8, end: *mut *mut u8, base: i32, _l: usize) -> i64 {
    libc::strtoll(s.cast(), end.cast(), base)
}
unsafe extern "win64" fn strtoui64_l(s: *const u8, end: *mut *mut u8, base: i32, _l: usize) -> u64 {
    libc::strtoull(s.cast(), end.cast(), base)
}
unsafe extern "win64" fn strtol_l(s: *const u8, end: *mut *mut u8, base: i32, _l: usize) -> i32 {
    libc::strtol(s.cast(), end.cast(), base).clamp(i32::MIN as i64, i32::MAX as i64) as i32
}
unsafe extern "win64" fn strtoul_l(s: *const u8, end: *mut *mut u8, base: i32, _l: usize) -> u32 {
    let v = libc::strtoul(s.cast(), end.cast(), base);
    if v > u32::MAX as u64 && v < (u64::MAX - u32::MAX as u64) {
        u32::MAX
    } else {
        v as u32
    }
}

/// Conversion large → étroite des caractères d'un nombre (ASCII), puis
/// appel de la version étroite : `end` est replacé dans la chaîne large.
unsafe fn wide_number<T>(
    s: *const u16,
    end: *mut *mut u16,
    f: impl FnOnce(*const u8, *mut *mut u8) -> T,
) -> T {
    let mut narrow = Vec::new();
    if !s.is_null() {
        let mut i = 0;
        loop {
            let c = *s.add(i);
            if c == 0 || c >= 0x80 {
                break;
            }
            narrow.push(c as u8);
            i += 1;
        }
    }
    narrow.push(0);
    let mut e: *mut u8 = std::ptr::null_mut();
    let r = f(narrow.as_ptr(), &mut e);
    if !end.is_null() {
        let used = if e.is_null() {
            0
        } else {
            e.offset_from(narrow.as_ptr()) as usize
        };
        *end = s.add(used) as *mut u16;
    }
    r
}
unsafe extern "win64" fn wcstod(s: *const u16, end: *mut *mut u16) -> f64 {
    wide_number(s, end, |p, e| libc::strtod(p.cast(), e.cast()))
}
unsafe extern "win64" fn wcstof(s: *const u16, end: *mut *mut u16) -> f32 {
    wide_number(s, end, |p, e| libc::strtof(p.cast(), e.cast()))
}
unsafe extern "win64" fn wcstoll(s: *const u16, end: *mut *mut u16, base: i32) -> i64 {
    wide_number(s, end, |p, e| libc::strtoll(p.cast(), e.cast(), base))
}
unsafe extern "win64" fn wcstoull(s: *const u16, end: *mut *mut u16, base: i32) -> u64 {
    wide_number(s, end, |p, e| libc::strtoull(p.cast(), e.cast(), base))
}
unsafe extern "win64" fn wcstod_l(s: *const u16, end: *mut *mut u16, _l: usize) -> f64 {
    wcstod(s, end)
}
unsafe extern "win64" fn wcstoll_l(s: *const u16, end: *mut *mut u16, base: i32, _l: usize) -> i64 {
    wcstoll(s, end, base)
}
unsafe extern "win64" fn wcstoull_l(
    s: *const u16,
    end: *mut *mut u16,
    base: i32,
    _l: usize,
) -> u64 {
    wcstoull(s, end, base)
}
unsafe extern "win64" fn wtof(s: *const u16) -> f64 {
    wcstod(s, std::ptr::null_mut())
}
unsafe extern "win64" fn wtoi64(s: *const u16) -> i64 {
    wcstoll(s, std::ptr::null_mut(), 10)
}

// ---------- Multioctets (page ANSI de la locale) ----------

unsafe extern "win64" fn mbrtowc(
    dst: *mut u16,
    src: *const u8,
    n: usize,
    _state: *mut u32,
) -> isize {
    if src.is_null() {
        return 0;
    }
    if n == 0 {
        return -2;
    }
    let mut wc = 0u16;
    let r = crate::dlls::crt_runtime::mbtowc(&mut wc, src, n);
    if r < 0 {
        *errno() = 42; // EILSEQ
        return -1;
    }
    if !dst.is_null() {
        *dst = wc;
    }
    r as isize
}
unsafe extern "win64" fn mbrlen(src: *const u8, n: usize, state: *mut u32) -> isize {
    mbrtowc(std::ptr::null_mut(), src, n, state)
}
unsafe extern "win64" fn mbtowc_l(dst: *mut u16, src: *const u8, n: usize, _l: usize) -> i32 {
    crate::dlls::crt_runtime::mbtowc(dst, src, n)
}
unsafe extern "win64" fn wcrtomb(dst: *mut u8, wc: u16, _state: *mut u32) -> isize {
    let mut tmp = [0u8; 8];
    let target = if dst.is_null() { tmp.as_mut_ptr() } else { dst };
    let r = crate::dlls::crt_runtime::wctomb(target, wc);
    if r < 0 {
        *errno() = 42;
        return -1;
    }
    r as isize
}
unsafe extern "win64" fn wcrtomb_s(
    ret: *mut usize,
    dst: *mut u8,
    size: usize,
    wc: u16,
    state: *mut u32,
) -> i32 {
    let mut tmp = [0u8; 8];
    let n = wcrtomb(tmp.as_mut_ptr(), wc, state);
    if n < 0 {
        return 42;
    }
    if !dst.is_null() {
        if (n as usize) > size {
            return 34;
        }
        std::ptr::copy_nonoverlapping(tmp.as_ptr(), dst, n as usize);
    }
    if !ret.is_null() {
        *ret = n as usize;
    }
    0
}
unsafe extern "win64" fn mbsrtowcs(
    dst: *mut u16,
    src: *mut *const u8,
    len: usize,
    state: *mut u32,
) -> usize {
    if src.is_null() || (*src).is_null() {
        return 0;
    }
    let mut p = *src;
    let mut count = 0usize;
    while dst.is_null() || count < len {
        let mut wc = 0u16;
        let r = mbrtowc(&mut wc, p, 8, state);
        if r < 0 {
            *src = p;
            return usize::MAX;
        }
        if r == 0 {
            if !dst.is_null() {
                *dst.add(count) = 0;
                *src = std::ptr::null();
            }
            return count;
        }
        if !dst.is_null() {
            *dst.add(count) = wc;
        }
        p = p.add(r as usize);
        count += 1;
    }
    *src = p;
    count
}
unsafe extern "win64" fn wcsrtombs(
    dst: *mut u8,
    src: *mut *const u16,
    len: usize,
    state: *mut u32,
) -> usize {
    if src.is_null() || (*src).is_null() {
        return 0;
    }
    let mut p = *src;
    let mut count = 0usize;
    loop {
        let wc = *p;
        let mut tmp = [0u8; 8];
        let n = wcrtomb(tmp.as_mut_ptr(), wc, state);
        if n < 0 {
            *src = p;
            return usize::MAX;
        }
        if wc == 0 {
            if !dst.is_null() && count < len {
                *dst.add(count) = 0;
                *src = std::ptr::null();
            }
            return count;
        }
        if !dst.is_null() {
            if count + n as usize > len {
                *src = p;
                return count;
            }
            std::ptr::copy_nonoverlapping(tmp.as_ptr(), dst.add(count), n as usize);
        }
        count += n as usize;
        p = p.add(1);
    }
}
unsafe extern "win64" fn btowc(c: i32) -> u32 {
    if !(0..=255).contains(&c) {
        return 0xFFFF;
    }
    let b = [c as u8];
    let mut wc = 0u16;
    if crate::dlls::crt_runtime::mbtowc(&mut wc, b.as_ptr(), 1) < 0 {
        0xFFFF
    } else {
        wc as u32
    }
}
unsafe extern "win64" fn wctob(wc: u32) -> i32 {
    let mut tmp = [0u8; 8];
    if crate::dlls::crt_runtime::wctomb(tmp.as_mut_ptr(), wc as u16) == 1 {
        tmp[0] as i32
    } else {
        -1
    }
}
/// Pages ANSI à un octet : pas d'octet de tête de caractère double.
extern "win64" fn ismbblead(_c: u32) -> i32 {
    0
}

// ---------- Classes de caractères ----------

const UPPER: u16 = 0x1;
const LOWER: u16 = 0x2;
const DIGIT: u16 = 0x4;
const SPACE: u16 = 0x8;
const PUNCT: u16 = 0x10;
const CONTROL: u16 = 0x20;
const BLANK: u16 = 0x40;
const HEX: u16 = 0x80;
const ALPHA: u16 = 0x100;

fn class_of(c: char) -> u16 {
    let mut m = 0;
    if c.is_uppercase() {
        m |= UPPER | ALPHA;
    }
    if c.is_lowercase() {
        m |= LOWER | ALPHA;
    }
    if c.is_alphabetic() {
        m |= ALPHA;
    }
    if c.is_ascii_digit() {
        m |= DIGIT;
    }
    if c.is_whitespace() {
        m |= SPACE;
    }
    if c == ' ' || c == '\t' {
        m |= BLANK;
    }
    if c.is_control() {
        m |= CONTROL;
    }
    if c.is_ascii_hexdigit() {
        m |= HEX;
    }
    if !c.is_alphanumeric() && !c.is_whitespace() && !c.is_control() && c != '\0' {
        m |= PUNCT;
    }
    m
}
fn wide_char(c: u32) -> char {
    char::from_u32(c).unwrap_or('\u{FFFD}')
}
fn narrow_char(c: i32) -> char {
    if !(0..=255).contains(&c) {
        return '\0';
    }
    if c < 0x80 {
        return c as u8 as char;
    }
    crate::dlls::decode_ansi(&[c as u8])
        .chars()
        .next()
        .unwrap_or('\0')
}
extern "win64" fn isctype(c: i32, mask: i32) -> i32 {
    (class_of(narrow_char(c)) & mask as u16) as i32
}
extern "win64" fn isctype_l(c: i32, mask: i32, _l: usize) -> i32 {
    isctype(c, mask)
}
extern "win64" fn iswctype(c: u32, mask: u16) -> i32 {
    if c == 0xFFFF {
        return 0;
    }
    (class_of(wide_char(c)) & mask) as i32
}
extern "win64" fn iswctype_l(c: u32, mask: u16, _l: usize) -> i32 {
    iswctype(c, mask)
}
macro_rules! wclass_l {
    ($($name:ident => $mask:expr),*) => {$(
        extern "win64" fn $name(c: u32, _l: usize) -> i32 { iswctype(c, $mask) }
    )*};
}
wclass_l!(iswalpha_l => ALPHA, iswcntrl_l => CONTROL, iswdigit_l => DIGIT, iswlower_l => LOWER,
    iswpunct_l => PUNCT, iswspace_l => SPACE, iswupper_l => UPPER, iswxdigit_l => HEX,
    iswblank_l => BLANK);
macro_rules! wclass {
    ($($name:ident => $mask:expr),*) => {$(
        extern "win64" fn $name(c: u32) -> i32 { iswctype(c, $mask) }
    )*};
}
wclass!(iswalpha => ALPHA, iswcntrl => CONTROL, iswdigit => DIGIT, iswlower => LOWER,
    iswpunct => PUNCT, iswxdigit => HEX, iswblank => BLANK);
extern "win64" fn iswalnum(c: u32) -> i32 {
    iswctype(c, ALPHA | DIGIT)
}
extern "win64" fn iswprint(c: u32) -> i32 {
    (c != 0xFFFF && !wide_char(c).is_control()) as i32
}
extern "win64" fn iswprint_l(c: u32, _l: usize) -> i32 {
    iswprint(c)
}
extern "win64" fn iswgraph(c: u32) -> i32 {
    (iswprint(c) != 0 && c != 0x20) as i32
}
extern "win64" fn isblank(c: i32) -> i32 {
    (c == 0x20 || c == 0x09) as i32
}
extern "win64" fn tolower_l(c: i32, _l: usize) -> i32 {
    if (b'A' as i32..=b'Z' as i32).contains(&c) {
        c + 32
    } else {
        c
    }
}
extern "win64" fn toupper_l(c: i32, _l: usize) -> i32 {
    if (b'a' as i32..=b'z' as i32).contains(&c) {
        c - 32
    } else {
        c
    }
}
fn wlower(c: u32) -> u32 {
    let ch = wide_char(c);
    let mut it = ch.to_lowercase();
    match (it.next(), it.next()) {
        (Some(l), None) if (l as u32) < 0x10000 => l as u32,
        _ => c,
    }
}
fn wupper(c: u32) -> u32 {
    let ch = wide_char(c);
    let mut it = ch.to_uppercase();
    match (it.next(), it.next()) {
        (Some(u), None) if (u as u32) < 0x10000 => u as u32,
        _ => c,
    }
}
extern "win64" fn towlower_l(c: u32, _l: usize) -> u32 {
    wlower(c)
}
extern "win64" fn towupper_l(c: u32, _l: usize) -> u32 {
    wupper(c)
}

// ---------- Collation (locale « C ») ----------

unsafe fn wlen(p: *const u16) -> usize {
    crate::printf::wide_len(p)
}
unsafe extern "win64" fn strxfrm(dst: *mut u8, src: *const u8, n: usize) -> usize {
    let len = crate::printf::narrow_len(src);
    if !dst.is_null() && len < n {
        std::ptr::copy_nonoverlapping(src, dst, len + 1);
    }
    len
}
unsafe extern "win64" fn strxfrm_l(dst: *mut u8, src: *const u8, n: usize, _l: usize) -> usize {
    strxfrm(dst, src, n)
}
unsafe extern "win64" fn strcoll_l(a: *const u8, b: *const u8, _l: usize) -> i32 {
    crate::dlls::crt_runtime::strcoll(a, b)
}
unsafe extern "win64" fn wcscoll(a: *const u16, b: *const u16) -> i32 {
    let x = std::slice::from_raw_parts(a, wlen(a));
    let y = std::slice::from_raw_parts(b, wlen(b));
    x.cmp(y) as i32
}
unsafe extern "win64" fn wcscoll_l(a: *const u16, b: *const u16, _l: usize) -> i32 {
    wcscoll(a, b)
}
unsafe extern "win64" fn wcsxfrm(dst: *mut u16, src: *const u16, n: usize) -> usize {
    let len = wlen(src);
    if !dst.is_null() && len < n {
        std::ptr::copy_nonoverlapping(src, dst, len + 1);
    }
    len
}
unsafe extern "win64" fn wcsxfrm_l(dst: *mut u16, src: *const u16, n: usize, _l: usize) -> usize {
    wcsxfrm(dst, src, n)
}
unsafe extern "win64" fn strftime_l(
    buf: *mut u8,
    max: usize,
    fmt: *const u8,
    t: *const c_void,
    _l: usize,
) -> usize {
    crate::dlls::crt_runtime::strftime(buf, max, fmt, t.cast())
}

// ---------- Chaînes et mémoire ----------

unsafe extern "win64" fn strrev(s: *mut u8) -> *mut u8 {
    if !s.is_null() {
        let n = crate::printf::narrow_len(s);
        std::slice::from_raw_parts_mut(s, n).reverse();
    }
    s
}
unsafe extern "win64" fn wcsrev(s: *mut u16) -> *mut u16 {
    if !s.is_null() {
        std::slice::from_raw_parts_mut(s, wlen(s)).reverse();
    }
    s
}
unsafe extern "win64" fn memicmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    for i in 0..n {
        let x = (*a.add(i)).to_ascii_lowercase();
        let y = (*b.add(i)).to_ascii_lowercase();
        if x != y {
            return x as i32 - y as i32;
        }
    }
    0
}
unsafe extern "win64" fn memcpy_s(dst: *mut u8, size: usize, src: *const u8, n: usize) -> i32 {
    if n == 0 {
        return 0;
    }
    if dst.is_null() {
        return 22;
    }
    if src.is_null() || size < n {
        std::ptr::write_bytes(dst, 0, size);
        return if src.is_null() { 22 } else { 34 };
    }
    std::ptr::copy(src, dst, n);
    0
}
unsafe extern "win64" fn memmove_s(dst: *mut u8, size: usize, src: *const u8, n: usize) -> i32 {
    if n == 0 {
        return 0;
    }
    if dst.is_null() || src.is_null() {
        return 22;
    }
    if size < n {
        return 34;
    }
    std::ptr::copy(src, dst, n);
    0
}
unsafe extern "win64" fn getenv_s(
    len: *mut usize,
    buf: *mut u8,
    size: usize,
    name: *const u8,
) -> i32 {
    if len.is_null() || name.is_null() {
        return 22;
    }
    let v = crate::dlls::crt_runtime::getenv(name);
    if v.is_null() {
        *len = 0;
        if !buf.is_null() && size > 0 {
            *buf = 0;
        }
        return 0;
    }
    let n = crate::printf::narrow_len(v) + 1;
    *len = n;
    if buf.is_null() || size == 0 {
        return 0;
    }
    if size < n {
        return 34;
    }
    std::ptr::copy_nonoverlapping(v, buf, n);
    0
}
unsafe extern "win64" fn assert_fail(msg: *const u8, file: *const u8, line: u32) {
    eprintln!(
        "Assertion failed: {}, file {}, line {}",
        crate::dlls::cstr(msg),
        crate::dlls::cstr(file),
        line
    );
    libc::abort();
}
unsafe extern "win64" fn wassert_fail(msg: *const u16, file: *const u16, line: u32) {
    eprintln!(
        "Assertion failed: {}, file {}, line {}",
        crate::dlls::wstr(msg),
        crate::dlls::wstr(file),
        line
    );
    libc::abort();
}
extern "win64" fn endthreadex(code: u32) {
    crate::dlls::kernel32_misc::exit_thread(code)
}
extern "win64" fn endthread() {
    crate::dlls::kernel32_misc::exit_thread(0)
}
extern "win64" fn lconv_init() -> i32 {
    0
}
extern "win64" fn locking(_fd: i32, _mode: i32, _n: i32) -> i32 {
    0
}

type InvalidParameterHandler = usize;
static INVALID_PARAMETER: Mutex<InvalidParameterHandler> = Mutex::new(0);
extern "win64" fn set_invalid_parameter_handler(h: usize) -> usize {
    std::mem::replace(&mut *crate::dlls::lock(&INVALID_PARAMETER), h)
}
extern "win64" fn get_invalid_parameter_handler() -> usize {
    *crate::dlls::lock(&INVALID_PARAMETER)
}
/// Printf « positionnel » (%1$d) : les jeux s'en servent comme d'un printf
/// ordinaire ; le moteur de formatage commun suffit.
unsafe extern "win64" fn stdio_common_vsprintf_p(
    options: u64,
    buf: *mut u8,
    count: usize,
    fmt: *const u8,
    loc: usize,
    va: *const u64,
) -> i32 {
    crate::dlls::crt_ucrt::stdio_common_vsprintf(options, buf, count, fmt, loc, va)
}

// ---------- Messages d'erreur, ligne de commande, fuseau horaire ----------

const SYS_NERR: usize = 43;
static mut SYS_NERR_DATA: i32 = SYS_NERR as i32;
static mut SYS_ERRLIST: [usize; SYS_NERR] = [0; SYS_NERR];
static mut ACMDLN: usize = 0;
static mut WCMDLN: usize = 0;
static mut TZ_TIMEZONE: i32 = 0;
static mut TZ_DAYLIGHT: i32 = 0;
static mut TZ_DSTBIAS: i32 = -3600;
static mut TZ_NAMES: [usize; 2] = [0; 2];

fn init_tables() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        for (i, slot) in (*std::ptr::addr_of_mut!(SYS_ERRLIST))
            .iter_mut()
            .enumerate()
        {
            let text = std::ffi::CStr::from_ptr(libc::strerror(i as i32))
                .to_bytes()
                .to_vec();
            let mut b = text;
            b.push(0);
            *slot = Box::leak(b.into_boxed_slice()).as_ptr() as usize;
        }
        // Décalage de l'heure locale (heure d'hiver), en secondes à l'ouest d'UTC.
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        let mut jan = tm;
        jan.tm_mon = 0;
        jan.tm_mday = 1;
        jan.tm_isdst = -1;
        let t = libc::mktime(&mut jan);
        let mut local: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut local);
        TZ_TIMEZONE = -(local.tm_gmtoff as i32);
        let mut jul = tm;
        jul.tm_mon = 6;
        jul.tm_mday = 1;
        jul.tm_isdst = -1;
        let t2 = libc::mktime(&mut jul);
        let mut summer: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t2, &mut summer);
        TZ_DAYLIGHT = (summer.tm_gmtoff != local.tm_gmtoff) as i32;
        TZ_DSTBIAS = -((summer.tm_gmtoff - local.tm_gmtoff) as i32);
        let name = |p: *const libc::c_char| -> usize {
            let mut b = if p.is_null() {
                Vec::new()
            } else {
                std::ffi::CStr::from_ptr(p).to_bytes().to_vec()
            };
            b.push(0);
            Box::leak(b.into_boxed_slice()).as_ptr() as usize
        };
        TZ_NAMES = [name(local.tm_zone), name(summer.tm_zone)];
    });
}
/// Ligne de commande connue : renseigne les variables exportées `_acmdln`
/// et `_wcmdln` (importées comme données par les programmes MinGW).
pub fn process_started() {
    unsafe {
        ACMDLN = crate::dlls::kernel32::get_command_line_a();
        WCMDLN = crate::dlls::kernel32::get_command_line_w();
    }
}
fn cmdline() {
    if unsafe { ACMDLN } == 0 {
        process_started();
    }
}
extern "win64" fn p_sys_nerr() -> *mut i32 {
    init_tables();
    std::ptr::addr_of_mut!(SYS_NERR_DATA)
}
extern "win64" fn p_sys_errlist() -> *mut usize {
    init_tables();
    std::ptr::addr_of_mut!(SYS_ERRLIST) as *mut usize
}
pub(crate) extern "win64" fn p_acmdln() -> *mut usize {
    cmdline();
    std::ptr::addr_of_mut!(ACMDLN)
}
pub(crate) extern "win64" fn p_wcmdln() -> *mut usize {
    cmdline();
    std::ptr::addr_of_mut!(WCMDLN)
}
extern "win64" fn p_timezone() -> *mut i32 {
    init_tables();
    std::ptr::addr_of_mut!(TZ_TIMEZONE)
}
extern "win64" fn p_daylight() -> *mut i32 {
    init_tables();
    std::ptr::addr_of_mut!(TZ_DAYLIGHT)
}
extern "win64" fn p_dstbias() -> *mut i32 {
    init_tables();
    std::ptr::addr_of_mut!(TZ_DSTBIAS)
}
extern "win64" fn p_tzname() -> *mut usize {
    init_tables();
    std::ptr::addr_of_mut!(TZ_NAMES) as *mut usize
}

// ---------- Fichiers : descripteurs, stat, chemins larges ----------

fn os_err() -> i32 {
    let e = std::io::Error::last_os_error().raw_os_error().unwrap_or(22);
    unsafe { *errno() = crt_errno(e) };
    -1
}
fn io_err(e: std::io::Error) -> i32 {
    unsafe { *errno() = crt_errno(e.raw_os_error().unwrap_or(22)) };
    -1
}

unsafe extern "win64" fn filelengthi64(fd: i32) -> i64 {
    let mut st: libc::stat = std::mem::zeroed();
    if libc::fstat(fd, &mut st) < 0 {
        return os_err() as i64;
    }
    st.st_size
}
/// `_filelength` prend un descripteur (et non un FILE*).
unsafe extern "win64" fn filelength(fd: i32) -> i32 {
    let n = filelengthi64(fd);
    if n > i32::MAX as i64 {
        *errno() = 22;
        return -1;
    }
    n as i32
}
unsafe extern "win64" fn pipe(fds: *mut i32, _size: u32, _mode: i32) -> i32 {
    if fds.is_null() {
        *errno() = 22;
        return -1;
    }
    let mut p = [0i32; 2];
    if libc::pipe2(p.as_mut_ptr(), libc::O_CLOEXEC) < 0 {
        return os_err();
    }
    *fds = p[0];
    *fds.add(1) = p[1];
    0
}
unsafe extern "win64" fn wsopen_s(
    fd: *mut i32,
    path: *const u16,
    flags: i32,
    _share: i32,
    mode: u32,
) -> i32 {
    if fd.is_null() {
        return 22;
    }
    *fd = crate::dlls::crt_runtime::wopen(path, flags, mode);
    if *fd < 0 {
        *errno()
    } else {
        0
    }
}
/// `_wsopen_dispatch` / `_sopen_dispatch` (UCRT) : `_wsopen_s` avec le
/// drapeau « sécurisé » en plus.
unsafe extern "win64" fn wsopen_dispatch(
    path: *const u16,
    flags: i32,
    share: i32,
    mode: u32,
    fd: *mut i32,
    _secure: i32,
) -> i32 {
    wsopen_s(fd, path, flags, share, mode)
}
unsafe extern "win64" fn sopen_dispatch(
    path: *const u8,
    flags: i32,
    share: i32,
    mode: u32,
    fd: *mut i32,
    _secure: i32,
) -> i32 {
    if fd.is_null() {
        return 22;
    }
    *fd = crate::dlls::crt_runtime::sopen(path, flags, share, mode);
    if *fd < 0 {
        *errno()
    } else {
        0
    }
}

unsafe extern "win64" fn wsopen(path: *const u16, flags: i32, _share: i32, mode: u32) -> i32 {
    crate::dlls::crt_runtime::wopen(path, flags, mode)
}

/// Champs communs des variantes de `struct _stat`.
struct Stat {
    mode: u16,
    nlink: i16,
    size: i64,
    atime: i64,
    mtime: i64,
    ctime: i64,
}
fn stat_of(st: &libc::stat) -> Stat {
    let fmt = st.st_mode & libc::S_IFMT;
    let mut mode: u16 = match fmt {
        libc::S_IFDIR => 0x4000,
        libc::S_IFCHR => 0x2000,
        libc::S_IFIFO => 0x1000,
        _ => 0x8000,
    };
    // _S_IREAD, _S_IWRITE, _S_IEXEC, recopiés pour le groupe et les autres.
    let mut user = 0x100;
    if st.st_mode & 0o222 != 0 {
        user |= 0x80;
    }
    if fmt == libc::S_IFDIR || st.st_mode & 0o111 != 0 {
        user |= 0x40;
    }
    mode |= user | (user >> 3) | (user >> 6);
    Stat {
        mode,
        nlink: st.st_nlink.min(i16::MAX as u64) as i16,
        size: st.st_size,
        atime: st.st_atime,
        mtime: st.st_mtime,
        ctime: st.st_ctime,
    }
}
/// Écrit la structure au format demandé : (taille sur 64 bits ?, dates sur 64 bits ?).
unsafe fn store_stat(out: *mut u8, s: &Stat, size64: bool, time64: bool) {
    let w16 = |o: usize, v: u16| std::ptr::write_unaligned(out.add(o) as *mut u16, v);
    let w32 = |o: usize, v: u32| std::ptr::write_unaligned(out.add(o) as *mut u32, v);
    let w64 = |o: usize, v: u64| std::ptr::write_unaligned(out.add(o) as *mut u64, v);
    let total = match (size64, time64) {
        (true, true) => 56,
        (false, true) => 48,
        (true, false) => 48,
        (false, false) => 36,
    };
    std::ptr::write_bytes(out, 0, total);
    w32(0, 2); // st_dev : lecteur C:
    w16(6, s.mode);
    w16(8, s.nlink as u16);
    w32(16, 2); // st_rdev
    match (size64, time64) {
        (true, true) => {
            w64(24, s.size as u64);
            w64(32, s.atime as u64);
            w64(40, s.mtime as u64);
            w64(48, s.ctime as u64);
        }
        (false, true) => {
            w32(20, s.size.min(i32::MAX as i64) as u32);
            w64(24, s.atime as u64);
            w64(32, s.mtime as u64);
            w64(40, s.ctime as u64);
        }
        (true, false) => {
            w64(24, s.size as u64);
            w32(32, s.atime as u32);
            w32(36, s.mtime as u32);
            w32(40, s.ctime as u32);
        }
        (false, false) => {
            w32(20, s.size.min(i32::MAX as i64) as u32);
            w32(24, s.atime as u32);
            w32(28, s.mtime as u32);
            w32(32, s.ctime as u32);
        }
    }
}
fn host_path(path: &str) -> Option<std::ffi::CString> {
    // « C:\dossier\ » : stat refuse la barre finale sous Windows sauf à la racine,
    // mais les programmes s'attendent à ce que « dossier\ » fonctionne avec _wstat
    // pour un dossier : on la retire simplement.
    let trimmed = if path.len() > 3 {
        path.trim_end_matches(['\\', '/'])
    } else {
        path
    };
    let linux = crate::paths::to_linux(trimmed);
    std::ffi::CString::new(linux.to_string_lossy().as_bytes()).ok()
}
unsafe fn stat_path(path: &str, out: *mut u8, size64: bool, time64: bool) -> i32 {
    if out.is_null() {
        *errno() = 22;
        return -1;
    }
    let Some(c) = host_path(path) else {
        *errno() = 2;
        return -1;
    };
    let mut st: libc::stat = std::mem::zeroed();
    if libc::stat(c.as_ptr(), &mut st) < 0 {
        return os_err();
    }
    store_stat(out, &stat_of(&st), size64, time64);
    0
}
unsafe fn stat_fd(fd: i32, out: *mut u8, size64: bool, time64: bool) -> i32 {
    if out.is_null() {
        *errno() = 22;
        return -1;
    }
    let mut st: libc::stat = std::mem::zeroed();
    if libc::fstat(fd, &mut st) < 0 {
        *errno() = 9;
        return -1;
    }
    store_stat(out, &stat_of(&st), size64, time64);
    0
}
macro_rules! stat_variants {
    ($($n:ident, $w:ident, $f:ident: $s64:expr, $t64:expr;)*) => {$(
        unsafe extern "win64" fn $n(p: *const u8, out: *mut u8) -> i32 {
            if p.is_null() { *errno() = 22; return -1; }
            stat_path(&crate::dlls::cstr(p), out, $s64, $t64)
        }
        unsafe extern "win64" fn $w(p: *const u16, out: *mut u8) -> i32 {
            if p.is_null() { *errno() = 22; return -1; }
            stat_path(&crate::dlls::wstr(p), out, $s64, $t64)
        }
        unsafe extern "win64" fn $f(fd: i32, out: *mut u8) -> i32 { stat_fd(fd, out, $s64, $t64) }
    )*};
}
stat_variants! {
    stat64, wstat64, fstat64: true, true;
    stat64i32, wstat64i32, fstat64i32: false, true;
    stat32i64, wstat32i64, fstat32i64: true, false;
    stat32, wstat32, fstat32: false, false;
}

unsafe fn ok_or_errno(r: std::io::Result<()>) -> i32 {
    match r {
        Ok(()) => 0,
        Err(e) => io_err(e),
    }
}
unsafe extern "win64" fn waccess(path: *const u16, mode: i32) -> i32 {
    if path.is_null() {
        *errno() = 22;
        return -1;
    }
    let Some(c) = host_path(&crate::dlls::wstr(path)) else {
        *errno() = 2;
        return -1;
    };
    let m =
        (if mode & 2 != 0 { libc::W_OK } else { 0 }) | (if mode & 4 != 0 { libc::R_OK } else { 0 });
    if libc::access(c.as_ptr(), if m == 0 { libc::F_OK } else { m }) < 0 {
        return os_err();
    }
    0
}
unsafe extern "win64" fn waccess_s(path: *const u16, mode: i32) -> i32 {
    if waccess(path, mode) < 0 {
        *errno()
    } else {
        0
    }
}
unsafe extern "win64" fn access_s(path: *const u8, mode: i32) -> i32 {
    if crate::dlls::crt_runtime::access(path, mode) < 0 {
        *errno()
    } else {
        0
    }
}
unsafe fn wpath(p: *const u16) -> Option<std::path::PathBuf> {
    if p.is_null() {
        *errno() = 22;
        return None;
    }
    Some(crate::paths::to_linux(&crate::dlls::wstr(p)))
}
unsafe extern "win64" fn wchdir(p: *const u16) -> i32 {
    match wpath(p) {
        Some(l) => ok_or_errno(std::env::set_current_dir(l)),
        None => -1,
    }
}
unsafe extern "win64" fn wmkdir(p: *const u16) -> i32 {
    match wpath(p) {
        Some(l) => ok_or_errno(std::fs::create_dir(l)),
        None => -1,
    }
}
unsafe extern "win64" fn wrmdir(p: *const u16) -> i32 {
    match wpath(p) {
        Some(l) => ok_or_errno(std::fs::remove_dir(l)),
        None => -1,
    }
}
unsafe extern "win64" fn wremove(p: *const u16) -> i32 {
    match wpath(p) {
        Some(l) => ok_or_errno(std::fs::remove_file(l)),
        None => -1,
    }
}
unsafe extern "win64" fn wrename(a: *const u16, b: *const u16) -> i32 {
    match (wpath(a), wpath(b)) {
        (Some(x), Some(y)) => {
            // Windows refuse d'écraser une destination existante.
            if y.exists() {
                *errno() = 17;
                return -1;
            }
            ok_or_errno(std::fs::rename(x, y))
        }
        _ => -1,
    }
}

/// Copie `text` (avec NUL) dans `buf` ou dans un tampon alloué par malloc.
unsafe fn give_wide(text: &str, buf: *mut u16, size: usize) -> *mut u16 {
    let mut w: Vec<u16> = text.encode_utf16().collect();
    w.push(0);
    if buf.is_null() {
        let p = libc::malloc(w.len().max(size) * 2) as *mut u16;
        if !p.is_null() {
            std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
        }
        return p;
    }
    if size < w.len() {
        *errno() = 34;
        return std::ptr::null_mut();
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    buf
}
unsafe fn give_narrow(text: &str, buf: *mut u8, size: usize) -> *mut u8 {
    let mut b = crate::dlls::encode_ansi(text);
    b.push(0);
    if buf.is_null() {
        let p = libc::malloc(b.len().max(size)) as *mut u8;
        if !p.is_null() {
            std::ptr::copy_nonoverlapping(b.as_ptr(), p, b.len());
        }
        return p;
    }
    if size < b.len() {
        *errno() = 34;
        return std::ptr::null_mut();
    }
    std::ptr::copy_nonoverlapping(b.as_ptr(), buf, b.len());
    buf
}
unsafe extern "win64" fn wgetcwd(buf: *mut u16, size: i32) -> *mut u16 {
    match std::env::current_dir() {
        Ok(p) => give_wide(&crate::paths::to_windows(&p), buf, size.max(0) as usize),
        Err(_) => {
            *errno() = 2;
            std::ptr::null_mut()
        }
    }
}

/// Chemin Windows absolu, « . » et « .. » résolus sans suivre les liens.
fn full_path(rel: &str) -> String {
    let linux = crate::paths::to_linux(rel);
    let abs = if linux.is_absolute() {
        linux
    } else {
        std::env::current_dir().unwrap_or_default().join(linux)
    };
    let mut out = std::path::PathBuf::new();
    for c in abs.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    let mut w = crate::paths::to_windows(&out);
    if (rel.ends_with('\\') || rel.ends_with('/')) && !w.ends_with('\\') {
        w.push('\\');
    }
    w
}
unsafe extern "win64" fn wfullpath(buf: *mut u16, rel: *const u16, max: usize) -> *mut u16 {
    let r = if rel.is_null() {
        String::from(".")
    } else {
        crate::dlls::wstr(rel)
    };
    give_wide(
        &full_path(if r.is_empty() { "." } else { &r }),
        buf,
        if buf.is_null() { 0 } else { max },
    )
}
unsafe extern "win64" fn fullpath(buf: *mut u8, rel: *const u8, max: usize) -> *mut u8 {
    let r = if rel.is_null() {
        String::from(".")
    } else {
        crate::dlls::cstr(rel)
    };
    give_narrow(
        &full_path(if r.is_empty() { "." } else { &r }),
        buf,
        if buf.is_null() { 0 } else { max },
    )
}

// ---------- _findfirst / _findnext ----------
//
// Ils s'appuient sur FindFirstFileW de Winlux (motifs, casse, attributs).

fn filetime_to_unix(ft: u64) -> i64 {
    if ft == 0 {
        return 0;
    }
    (ft / 10_000_000) as i64 - 11_644_473_600
}
#[derive(Clone, Copy, PartialEq)]
enum FindLayout {
    /// _finddata64_t : dates 64 bits, taille 64 bits.
    T64,
    /// _finddata64i32_t (le _finddata_t du x64) : dates 64 bits, taille 32 bits.
    T64I32,
    /// _finddata32_t : dates et taille sur 32 bits.
    T32,
}
unsafe fn store_find(
    out: *mut u8,
    d: &crate::dlls::winfile::Win32FindDataW,
    layout: FindLayout,
    wide: bool,
) {
    let attrs = d.attributes;
    let size = ((d.size_high as u64) << 32) | d.size_low as u64;
    let (c, a, w) = (d.creation_time, d.access_time, d.write_time);
    let times = [
        filetime_to_unix(c),
        filetime_to_unix(a),
        filetime_to_unix(w),
    ];
    let name_utf16: Vec<u16> = {
        let n = d.file_name;
        let len = n.iter().position(|&x| x == 0).unwrap_or(n.len());
        n[..len].to_vec()
    };
    std::ptr::write_unaligned(out as *mut u32, attrs & 0x37);
    let name_off = match layout {
        FindLayout::T64 => {
            for (i, t) in times.iter().enumerate() {
                std::ptr::write_unaligned(out.add(8 + 8 * i) as *mut i64, *t);
            }
            std::ptr::write_unaligned(out.add(32) as *mut u64, size);
            40
        }
        FindLayout::T64I32 => {
            for (i, t) in times.iter().enumerate() {
                std::ptr::write_unaligned(out.add(8 + 8 * i) as *mut i64, *t);
            }
            std::ptr::write_unaligned(out.add(32) as *mut u32, size.min(u32::MAX as u64) as u32);
            36
        }
        FindLayout::T32 => {
            for (i, t) in times.iter().enumerate() {
                std::ptr::write_unaligned(out.add(4 + 4 * i) as *mut i32, *t as i32);
            }
            std::ptr::write_unaligned(out.add(16) as *mut u32, size.min(u32::MAX as u64) as u32);
            20
        }
    };
    if wide {
        let dst = out.add(name_off) as *mut u16;
        let n = name_utf16.len().min(259);
        for (i, ch) in name_utf16.iter().take(n).enumerate() {
            std::ptr::write_unaligned(dst.add(i), *ch);
        }
        std::ptr::write_unaligned(dst.add(n), 0);
    } else {
        let text = String::from_utf16_lossy(&name_utf16);
        let b = crate::dlls::encode_ansi(&text);
        let n = b.len().min(259);
        std::ptr::copy_nonoverlapping(b.as_ptr(), out.add(name_off), n);
        *out.add(name_off + n) = 0;
    }
}
unsafe fn find_first(pattern: &str, out: *mut u8, layout: FindLayout, wide: bool) -> isize {
    if out.is_null() {
        *errno() = 22;
        return -1;
    }
    let mut w: Vec<u16> = pattern.encode_utf16().collect();
    w.push(0);
    let mut d: crate::dlls::winfile::Win32FindDataW = std::mem::zeroed();
    let h = crate::dlls::winfile::find_first_file_w(w.as_ptr(), &mut d);
    if h == usize::MAX {
        *errno() = 2;
        return -1;
    }
    store_find(out, &d, layout, wide);
    h as isize
}
unsafe fn find_next(h: isize, out: *mut u8, layout: FindLayout, wide: bool) -> i32 {
    if out.is_null() {
        *errno() = 22;
        return -1;
    }
    let mut d: crate::dlls::winfile::Win32FindDataW = std::mem::zeroed();
    if crate::dlls::winfile::find_next_file_w(h as usize, &mut d) == 0 {
        *errno() = 2;
        return -1;
    }
    store_find(out, &d, layout, wide);
    0
}
macro_rules! find_variants {
    ($($first:ident, $next:ident, $wfirst:ident, $wnext:ident: $layout:expr;)*) => {$(
        unsafe extern "win64" fn $first(p: *const u8, out: *mut u8) -> isize {
            if p.is_null() { *errno() = 22; return -1; }
            find_first(&crate::dlls::cstr(p), out, $layout, false)
        }
        unsafe extern "win64" fn $next(h: isize, out: *mut u8) -> i32 { find_next(h, out, $layout, false) }
        unsafe extern "win64" fn $wfirst(p: *const u16, out: *mut u8) -> isize {
            if p.is_null() { *errno() = 22; return -1; }
            find_first(&crate::dlls::wstr(p), out, $layout, true)
        }
        unsafe extern "win64" fn $wnext(h: isize, out: *mut u8) -> i32 { find_next(h, out, $layout, true) }
    )*};
}
find_variants! {
    findfirst64, findnext64, wfindfirst64, wfindnext64: FindLayout::T64;
    findfirst64i32, findnext64i32, wfindfirst64i32, wfindnext64i32: FindLayout::T64I32;
    findfirst32, findnext32, wfindfirst32, wfindnext32: FindLayout::T32;
}
extern "win64" fn findclose(h: isize) -> i32 {
    if crate::dlls::winfile::find_close(h as usize) != 0 {
        0
    } else {
        unsafe { *errno() = 2 };
        -1
    }
}

pub fn exports() -> crate::dlls::Exports {
    // Les variables exportées comme données doivent être prêtes avant tout appel.
    init_tables();
    crate::exports! {
        "_setjmp" => fn winlux_crt_setjmp, "_setjmpex" => fn winlux_crt_setjmp, "setjmp" => fn winlux_crt_setjmp,
        "__intrinsic_setjmp" => fn winlux_crt_setjmp, "__intrinsic_setjmpex" => fn winlux_crt_setjmp,
        "longjmp" => fn winlux_crt_longjmp,
        "_hypot" => fn hypot, "hypot" => fn hypot, "_hypotf" => fn hypotf, "hypotf" => fn hypotf,
        "exp2" => fn exp2, "exp2f" => fn exp2f, "expm1" => fn expm1, "expm1f" => fn expm1f,
        "log1p" => fn log1p, "log1pf" => fn log1pf, "rint" => fn rint, "rintf" => fn rintf,
        "nearbyint" => fn nearbyint, "nearbyintf" => fn nearbyintf,
        "lrint" => fn lrint, "lrintf" => fn lrintf, "llrint" => fn llrint, "llrintf" => fn llrintf,
        "lround" => fn lround, "lroundf" => fn lroundf, "llround" => fn llround, "llroundf" => fn llroundf,
        "nextafter" => fn nextafter, "_nextafter" => fn nextafter, "nextafterf" => fn nextafterf,
        "remainder" => fn remainder, "remainderf" => fn remainderf, "remquo" => fn remquo,
        "logb" => fn logb, "_logb" => fn logb, "logbf" => fn logbf,
        "erf" => fn erf, "erfc" => fn erfc, "tgamma" => fn tgamma, "lgamma" => fn lgamma,
        "scalbn" => fn scalbn, "scalbnf" => fn scalbnf, "fdim" => fn fdim,
        "fmax" => fn fmax, "fmin" => fn fmin, "fmaxf" => fn fmaxf, "fminf" => fn fminf, "exp10" => fn exp10,
        "strtof" => fn strtof, "_strtod_l" => fn strtod_l, "_strtof_l" => fn strtof_l,
        "_strtoi64" => fn strtoi64, "_strtoui64" => fn strtoui64,
        "_strtoi64_l" => fn strtoi64_l, "_strtoui64_l" => fn strtoui64_l,
        "_strtoll_l" => fn strtoi64_l, "_strtoull_l" => fn strtoui64_l,
        "_strtol_l" => fn strtol_l, "_strtoul_l" => fn strtoul_l,
        "wcstod" => fn wcstod, "wcstof" => fn wcstof, "wcstoll" => fn wcstoll, "wcstoull" => fn wcstoull,
        "_wcstoi64" => fn wcstoll, "_wcstod_l" => fn wcstod_l, "_wcstoll_l" => fn wcstoll_l,
        "_wcstoull_l" => fn wcstoull_l, "_wcstoi64_l" => fn wcstoll_l, "_wcstoui64_l" => fn wcstoull_l,
        "_wtof" => fn wtof, "_wtoi64" => fn wtoi64,
        "mbrtowc" => fn mbrtowc, "mbrlen" => fn mbrlen, "_mbtowc_l" => fn mbtowc_l,
        "wcrtomb" => fn wcrtomb, "wcrtomb_s" => fn wcrtomb_s,
        "mbsrtowcs" => fn mbsrtowcs, "wcsrtombs" => fn wcsrtombs, "btowc" => fn btowc, "wctob" => fn wctob,
        "_ismbblead" => fn ismbblead,
        "_isctype" => fn isctype, "_isctype_l" => fn isctype_l, "iswctype" => fn iswctype, "_iswctype_l" => fn iswctype_l,
        "is_wctype" => fn iswctype,
        "_iswalpha_l" => fn iswalpha_l, "_iswcntrl_l" => fn iswcntrl_l, "_iswdigit_l" => fn iswdigit_l,
        "_iswlower_l" => fn iswlower_l, "_iswprint_l" => fn iswprint_l, "_iswpunct_l" => fn iswpunct_l,
        "_iswspace_l" => fn iswspace_l, "_iswupper_l" => fn iswupper_l, "_iswxdigit_l" => fn iswxdigit_l,
        "_iswblank_l" => fn iswblank_l,
        "iswalpha" => fn iswalpha, "iswcntrl" => fn iswcntrl, "iswdigit" => fn iswdigit, "iswlower" => fn iswlower,
        "iswpunct" => fn iswpunct, "iswxdigit" => fn iswxdigit, "iswblank" => fn iswblank,
        "iswalnum" => fn iswalnum, "iswprint" => fn iswprint, "iswgraph" => fn iswgraph, "isblank" => fn isblank,
        "_tolower_l" => fn tolower_l, "_toupper_l" => fn toupper_l,
        "_towlower_l" => fn towlower_l, "_towupper_l" => fn towupper_l,
        "strxfrm" => fn strxfrm, "_strxfrm_l" => fn strxfrm_l, "_strcoll_l" => fn strcoll_l,
        "wcscoll" => fn wcscoll, "_wcscoll_l" => fn wcscoll_l, "wcsxfrm" => fn wcsxfrm, "_wcsxfrm_l" => fn wcsxfrm_l,
        "_strftime_l" => fn strftime_l,
        "_strrev" => fn strrev, "_wcsrev" => fn wcsrev, "_memicmp" => fn memicmp,
        "memcpy_s" => fn memcpy_s, "memmove_s" => fn memmove_s, "getenv_s" => fn getenv_s,
        "_assert" => fn assert_fail, "_wassert" => fn wassert_fail,
        "_endthreadex" => fn endthreadex, "_endthread" => fn endthread,
        "__lconv_init" => fn lconv_init, "_locking" => fn locking,
        "_set_invalid_parameter_handler" => fn set_invalid_parameter_handler,
        "_set_thread_local_invalid_parameter_handler" => fn set_invalid_parameter_handler,
        "_get_invalid_parameter_handler" => fn get_invalid_parameter_handler,
        "__stdio_common_vsprintf_p" => fn stdio_common_vsprintf_p,
        // Formes « _p » (arguments positionnels %1$s) : le moteur les gère.
        "__stdio_common_vswprintf_p" => fn crate::dlls::crt_ucrt::stdio_common_vswprintf,
        "__stdio_common_vfprintf_p" => fn crate::dlls::crt_ucrt::stdio_common_vfprintf,
        "__stdio_common_vfwprintf_p" => fn crate::dlls::crt_ucrt::stdio_common_vfwprintf,
        "__sys_nerr" => fn p_sys_nerr, "__sys_errlist" => fn p_sys_errlist,
        "_sys_nerr" => data SYS_NERR_DATA, "_sys_errlist" => data SYS_ERRLIST,
        "_acmdln" => data ACMDLN, "_wcmdln" => data WCMDLN, "__p__acmdln" => fn p_acmdln, "__p__wcmdln" => fn p_wcmdln,
        "__timezone" => fn p_timezone, "__daylight" => fn p_daylight, "__dstbias" => fn p_dstbias, "__tzname" => fn p_tzname,
        "__p__timezone" => fn p_timezone, "__p__daylight" => fn p_daylight, "__p__dstbias" => fn p_dstbias, "__p__tzname" => fn p_tzname,
        "_timezone" => data TZ_TIMEZONE, "_daylight" => data TZ_DAYLIGHT, "_dstbias" => data TZ_DSTBIAS, "_tzname" => data TZ_NAMES,
        "_filelength" => fn filelength, "_filelengthi64" => fn filelengthi64, "_pipe" => fn pipe,
        "_wsopen_s" => fn wsopen_s, "_wsopen" => fn wsopen,
        "_wsopen_dispatch" => fn wsopen_dispatch, "_sopen_dispatch" => fn sopen_dispatch,
        "_stat64" => fn stat64, "_wstat64" => fn wstat64, "_fstat64" => fn fstat64,
        "_stat64i32" => fn stat64i32, "_wstat64i32" => fn wstat64i32, "_fstat64i32" => fn fstat64i32,
        "_stat32i64" => fn stat32i64, "_wstat32i64" => fn wstat32i64, "_fstat32i64" => fn fstat32i64,
        "_stat32" => fn stat32, "_wstat32" => fn wstat32, "_fstat32" => fn fstat32,
        "_stati64" => fn stat64, "_wstati64" => fn wstat64, "_fstati64" => fn fstat64,
        "_stat" => fn stat64i32, "_wstat" => fn wstat64i32, "_fstat" => fn fstat64i32,
        "_waccess" => fn waccess, "_waccess_s" => fn waccess_s, "_access_s" => fn access_s,
        "_wchdir" => fn wchdir, "_wmkdir" => fn wmkdir, "_wrmdir" => fn wrmdir,
        "_wremove" => fn wremove, "_wunlink" => fn wremove, "_wrename" => fn wrename,
        "_wgetcwd" => fn wgetcwd, "_wfullpath" => fn wfullpath, "_fullpath" => fn fullpath,
        "_findfirst64" => fn findfirst64, "_findnext64" => fn findnext64,
        "_wfindfirst64" => fn wfindfirst64, "_wfindnext64" => fn wfindnext64,
        "_findfirst64i32" => fn findfirst64i32, "_findnext64i32" => fn findnext64i32,
        "_wfindfirst64i32" => fn wfindfirst64i32, "_wfindnext64i32" => fn wfindnext64i32,
        "_findfirst" => fn findfirst64i32, "_findnext" => fn findnext64i32,
        "_wfindfirst" => fn wfindfirst64i32, "_wfindnext" => fn wfindnext64i32,
        "_findfirst32" => fn findfirst32, "_findnext32" => fn findnext32,
        "_wfindfirst32" => fn wfindfirst32, "_wfindnext32" => fn wfindnext32,
        "_findfirsti64" => fn findfirst64, "_findnexti64" => fn findnext64,
        "_wfindfirsti64" => fn wfindfirst64, "_wfindnexti64" => fn wfindnext64,
        "_findclose" => fn findclose,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat64_a_la_disposition_de_windows() {
        let dir = std::env::temp_dir().join(format!("winlux-stat-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("données.bin");
        std::fs::write(&f, [0u8; 1234]).unwrap();
        let win = crate::paths::to_windows(&f);
        let mut out = [0u8; 56];
        assert_eq!(unsafe { stat_path(&win, out.as_mut_ptr(), true, true) }, 0);
        let mode = u16::from_le_bytes([out[6], out[7]]);
        assert_eq!(mode & 0xF000, 0x8000);
        assert_eq!(u64::from_le_bytes(out[24..32].try_into().unwrap()), 1234);
        let mut out = [0u8; 48];
        assert_eq!(
            unsafe {
                stat_path(
                    &crate::paths::to_windows(&dir),
                    out.as_mut_ptr(),
                    false,
                    true,
                )
            },
            0
        );
        assert_eq!(u16::from_le_bytes([out[6], out[7]]) & 0xF000, 0x4000);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn classes_et_conversions() {
        assert_ne!(iswctype('É' as u32, UPPER), 0);
        assert_ne!(iswctype('7' as u32, DIGIT), 0);
        assert_eq!(iswctype('a' as u32, DIGIT), 0);
        assert_eq!(towlower_l('À' as u32, 0), 'à' as u32);
        let s: Vec<u16> = "  -12.5e1xyz".encode_utf16().chain([0]).collect();
        let mut end: *mut u16 = std::ptr::null_mut();
        let v = unsafe { wcstod(s.as_ptr(), &mut end) };
        assert_eq!(v, -125.0);
        assert_eq!(unsafe { end.offset_from(s.as_ptr()) }, 9);
        assert_eq!(lrint(2.5), 2);
        assert_eq!(lround(2.5), 3);
    }

    #[test]
    fn chemin_complet_resout_les_points() {
        let cwd = crate::paths::to_windows(&std::env::current_dir().unwrap());
        let p = full_path("a\\..\\b\\.\\c.txt");
        assert_eq!(p, format!("{}\\b\\c.txt", cwd.trim_end_matches('\\')));
    }
}
