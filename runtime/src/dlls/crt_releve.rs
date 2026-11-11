//! Fonctions de la CRT relevées dans le corpus de #26 (lot A, #1508) :
//! ucrtbase et msvcrt, importées par numpy, Qt, panda3d, jdk4py, pywin32,
//! dotnetcore2, ffmpeg, sharp et les outils 32 bits d'electron-winstaller.
//!
//! Sous MSVC, `long double` est un `double` : les variantes en `l` sont
//! celles en `double`. Les nombres complexes de <complex.h> (`_Dcomplex`,
//! `_Fcomplex`) suivent l'ABI x64 de Windows : un `_Dcomplex` (16 octets)
//! passe par pointeur et revient par un pointeur caché en premier argument ;
//! un `_Fcomplex` (8 octets) tient dans un registre entier. Les passerelles
//! x86 sont dans `wow32::crt` (`complex32`).

use std::ffi::c_void;

use super::crt_runtime::errno;

const EINVAL: i32 = 22;
const ERANGE: i32 = 34;

unsafe fn fail(e: i32) -> i32 {
    *errno() = e;
    e
}

// ---------------------------------------------------------------------
// Mathématiques
// ---------------------------------------------------------------------

// Les fonctions de libm passent par la bibliothèque standard (comme
// crt_ucrt) : lier libm à part changerait la version de `cbrt` retenue.
mod m {
    extern "C" {
        pub fn nextafter(x: f64, y: f64) -> f64;
        pub fn nextafterf(x: f32, y: f32) -> f32;
        pub fn ilogb(x: f64) -> i32;
        pub fn erff(x: f32) -> f32;
        pub fn feclearexcept(e: i32) -> i32;
        pub fn fetestexcept(e: i32) -> i32;
    }
}

extern "win64" fn acoshl(x: f64) -> f64 {
    x.acosh()
}
extern "win64" fn asinhl(x: f64) -> f64 {
    x.asinh()
}
extern "win64" fn atanhl(x: f64) -> f64 {
    x.atanh()
}
extern "win64" fn cbrtl(x: f64) -> f64 {
    x.cbrt()
}
extern "win64" fn exp2l(x: f64) -> f64 {
    x.exp2()
}
extern "win64" fn expm1l(x: f64) -> f64 {
    x.exp_m1()
}
extern "win64" fn log1pl(x: f64) -> f64 {
    x.ln_1p()
}
extern "win64" fn nextafterl(x: f64, y: f64) -> f64 {
    unsafe { m::nextafter(x, y) }
}
extern "win64" fn nextafterf(x: f32, y: f32) -> f32 {
    unsafe { m::nextafterf(x, y) }
}
extern "win64" fn rintl(x: f64) -> f64 {
    x.round_ties_even()
}
extern "win64" fn truncl(x: f64) -> f64 {
    x.trunc()
}
extern "win64" fn copysignl(x: f64, y: f64) -> f64 {
    x.copysign(y)
}
extern "win64" fn copysignf(x: f32, y: f32) -> f32 {
    x.copysign(y)
}
extern "win64" fn ilogb(x: f64) -> i32 {
    unsafe { m::ilogb(x) }
}
extern "win64" fn erff(x: f32) -> f32 {
    unsafe { m::erff(x) }
}
extern "win64" fn isnanf(x: f32) -> i32 {
    x.is_nan() as i32
}

/// `nan("")` : NaN silencieux (la charge utile est ignorée, comme la CRT).
extern "win64" fn nan(_tag: *const u8) -> f64 {
    f64::NAN
}
extern "win64" fn nanf(_tag: *const u8) -> f32 {
    f32::NAN
}

// Exceptions flottantes : constantes de <fenv.h> de MSVC (FE_INEXACT 0x01,
// FE_UNDERFLOW 0x02, FE_OVERFLOW 0x04, FE_DIVBYZERO 0x08, FE_INVALID 0x10)
// face à celles de la glibc x86 (0x20, 0x10, 0x08, 0x04, 0x01).
const FE_MAP: [(i32, i32); 5] = [
    (0x01, 0x20),
    (0x02, 0x10),
    (0x04, 0x08),
    (0x08, 0x04),
    (0x10, 0x01),
];

fn fe_to_linux(e: i32) -> i32 {
    FE_MAP
        .iter()
        .filter(|(w, _)| e & w != 0)
        .fold(0, |a, (_, l)| a | l)
}

fn fe_from_linux(e: i32) -> i32 {
    FE_MAP
        .iter()
        .filter(|(_, l)| e & l != 0)
        .fold(0, |a, (w, _)| a | w)
}

extern "win64" fn feclearexcept(e: i32) -> i32 {
    if e & !0x1F != 0 {
        return 1;
    }
    unsafe { m::feclearexcept(fe_to_linux(e)) }
}

extern "win64" fn fetestexcept(e: i32) -> i32 {
    fe_from_linux(unsafe { m::fetestexcept(fe_to_linux(e & 0x1F)) }) & e
}

/// `_fpreset` : réinitialise l'unité flottante (exceptions effacées).
extern "win64" fn fpreset() {
    unsafe { m::feclearexcept(0x3F) };
}

/// `_dpcomp` : _FP_LT 1, _FP_EQ 2, _FP_GT 4 ; 0 si l'un est NaN.
extern "win64" fn dpcomp(x: f64, y: f64) -> i32 {
    match x.partial_cmp(&y) {
        Some(std::cmp::Ordering::Less) => 1,
        Some(std::cmp::Ordering::Equal) => 2,
        Some(std::cmp::Ordering::Greater) => 4,
        None => 0,
    }
}
extern "win64" fn fdpcomp(x: f32, y: f32) -> i32 {
    dpcomp(x as f64, y as f64)
}

/// Classe de <math.h> de MSVC : FP_INFINITE 1, FP_NAN 2, FP_NORMAL -1,
/// FP_SUBNORMAL -2, FP_ZERO 0.
fn class(x: f64) -> i16 {
    use std::num::FpCategory::*;
    match x.classify() {
        Infinite => 1,
        Nan => 2,
        Normal => -1,
        Subnormal => -2,
        Zero => 0,
    }
}
extern "win64" fn ldclass(x: f64) -> i16 {
    class(x)
}
unsafe extern "win64" fn dtest(p: *const f64) -> i16 {
    class(p.read_unaligned())
}

/// `_fpclass` : _FPCLASS_SNAN 1, QNAN 2, NINF 4, NN 8, ND 0x10, NZ 0x20,
/// PZ 0x40, PD 0x80, PN 0x100, PINF 0x200.
extern "win64" fn fpclass(x: f64) -> i32 {
    use std::num::FpCategory::*;
    let neg = x.is_sign_negative();
    match x.classify() {
        Nan => {
            if x.to_bits() & (1 << 51) != 0 {
                2
            } else {
                1
            }
        }
        Infinite => {
            if neg {
                4
            } else {
                0x200
            }
        }
        Normal => {
            if neg {
                8
            } else {
                0x100
            }
        }
        Subnormal => {
            if neg {
                0x10
            } else {
                0x80
            }
        }
        Zero => {
            if neg {
                0x20
            } else {
                0x40
            }
        }
    }
}

// ---------- Nombres complexes ----------

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct C64 {
    pub re: f64,
    pub im: f64,
}

impl C64 {
    fn new(re: f64, im: f64) -> Self {
        C64 { re, im }
    }
    fn mul(self, o: C64) -> C64 {
        C64::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
    fn div(self, o: C64) -> C64 {
        let d = o.re * o.re + o.im * o.im;
        C64::new(
            (self.re * o.re + self.im * o.im) / d,
            (self.im * o.re - self.re * o.im) / d,
        )
    }
    fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }
    fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }
    fn exp(self) -> C64 {
        let r = self.re.exp();
        C64::new(r * self.im.cos(), r * self.im.sin())
    }
    fn ln(self) -> C64 {
        C64::new(self.abs().ln(), self.arg())
    }
}

/// Opérations complexes, numérotées pour les passerelles x86.
#[derive(Clone, Copy)]
pub(crate) enum COp {
    Exp,
    Cos,
    Cosh,
    Sin,
    Sinh,
    Tan,
    Tanh,
}

pub(crate) fn complex_op(op: COp, z: C64) -> C64 {
    match op {
        COp::Exp => z.exp(),
        COp::Cosh => C64::new(z.re.cosh() * z.im.cos(), z.re.sinh() * z.im.sin()),
        COp::Sinh => C64::new(z.re.sinh() * z.im.cos(), z.re.cosh() * z.im.sin()),
        // cos z = cosh(iz), sin z = -i sinh(iz)
        COp::Cos => complex_op(COp::Cosh, C64::new(-z.im, z.re)),
        COp::Sin => {
            let s = complex_op(COp::Sinh, C64::new(-z.im, z.re));
            C64::new(s.im, -s.re)
        }
        COp::Tanh => {
            // Forme stable pour les grandes parties réelles.
            if z.re.abs() > 20.0 {
                return C64::new(1f64.copysign(z.re), 0f64.copysign((2.0 * z.im).sin()));
            }
            complex_op(COp::Sinh, z).div(complex_op(COp::Cosh, z))
        }
        COp::Tan => {
            let t = complex_op(COp::Tanh, C64::new(-z.im, z.re));
            C64::new(t.im, -t.re)
        }
    }
}

pub(crate) fn complex_pow(a: C64, b: C64) -> C64 {
    if a.re == 0.0 && a.im == 0.0 {
        return if b.re == 0.0 && b.im == 0.0 {
            C64::new(1.0, 0.0)
        } else {
            C64::new(0.0, 0.0)
        };
    }
    a.ln().mul(b).exp()
}

unsafe fn rd_c(p: *const f64) -> C64 {
    C64::new(p.read_unaligned(), p.add(1).read_unaligned())
}

unsafe fn wr_c(p: *mut f64, z: C64) -> *mut f64 {
    p.write_unaligned(z.re);
    p.add(1).write_unaligned(z.im);
    p
}

fn f_split(v: u64) -> C64 {
    C64::new(
        f32::from_bits(v as u32) as f64,
        f32::from_bits((v >> 32) as u32) as f64,
    )
}

fn f_join(z: C64) -> u64 {
    (z.re as f32).to_bits() as u64 | ((z.im as f32).to_bits() as u64) << 32
}

macro_rules! complex_fns {
    ($($d:ident, $f:ident, $op:ident;)*) => {$(
        unsafe extern "win64" fn $d(out: *mut f64, z: *const f64) -> *mut f64 {
            wr_c(out, complex_op(COp::$op, rd_c(z)))
        }
        extern "win64" fn $f(z: u64) -> u64 {
            f_join(complex_op(COp::$op, f_split(z)))
        }
    )*};
}

complex_fns! {
    cexp, cexpf, Exp;
    ccos, ccosf, Cos;
    ccosh, ccoshf, Cosh;
    csin, csinf, Sin;
    csinh, csinhf, Sinh;
    ctan, ctanf, Tan;
    ctanh, ctanhf, Tanh;
}

unsafe extern "win64" fn cpow(out: *mut f64, a: *const f64, b: *const f64) -> *mut f64 {
    wr_c(out, complex_pow(rd_c(a), rd_c(b)))
}
extern "win64" fn cpowf(a: u64, b: u64) -> u64 {
    f_join(complex_pow(f_split(a), f_split(b)))
}
unsafe extern "win64" fn cabs(z: *const f64) -> f64 {
    rd_c(z).abs()
}
extern "win64" fn cabsf(z: u64) -> f32 {
    f_split(z).abs() as f32
}
unsafe extern "win64" fn carg(z: *const f64) -> f64 {
    rd_c(z).arg()
}
extern "win64" fn cargf(z: u64) -> f32 {
    f_split(z).arg() as f32
}
unsafe extern "win64" fn creal(z: *const f64) -> f64 {
    z.read_unaligned()
}
extern "win64" fn crealf(z: u64) -> f32 {
    f32::from_bits(z as u32)
}
unsafe extern "win64" fn cimag(z: *const f64) -> f64 {
    z.add(1).read_unaligned()
}
extern "win64" fn cimagf(z: u64) -> f32 {
    f32::from_bits((z >> 32) as u32)
}

// ---------------------------------------------------------------------
// Conversions d'entiers « _s »
// ---------------------------------------------------------------------

fn digits(mut v: u64, neg: bool, radix: u32) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let d = (v % radix as u64) as u8;
        out.push(if d < 10 { b'0' + d } else { b'a' + d - 10 });
        v /= radix as u64;
        if v == 0 {
            break;
        }
    }
    if neg {
        out.push(b'-');
    }
    out.reverse();
    out
}

/// Écrit `text` + NUL dans `buf` de `size` éléments (u8 ou u16).
unsafe fn put<T: From<u8> + Copy>(buf: *mut T, size: usize, text: &[u8], radix: i32) -> i32 {
    if buf.is_null() || size == 0 {
        return fail(EINVAL);
    }
    if !(2..=36).contains(&radix) {
        *buf = T::from(0);
        return fail(EINVAL);
    }
    if text.len() + 1 > size {
        *buf = T::from(0);
        return fail(ERANGE);
    }
    for (i, &c) in text.iter().enumerate() {
        *buf.add(i) = T::from(c);
    }
    *buf.add(text.len()) = T::from(0);
    0
}

/// Valeur signée : le signe n'est écrit qu'en base 10, comme la CRT.
fn signed_digits(v: i64, bits: u32, radix: i32) -> Vec<u8> {
    let r = radix.clamp(2, 36) as u32;
    if r == 10 && v < 0 {
        digits(v.unsigned_abs(), true, 10)
    } else {
        let mask = if bits == 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        digits(v as u64 & mask, false, r)
    }
}

unsafe extern "win64" fn itoa_s(v: i32, buf: *mut u8, size: usize, radix: i32) -> i32 {
    put(buf, size, &signed_digits(v as i64, 32, radix), radix)
}
unsafe extern "win64" fn itow_s(v: i32, buf: *mut u16, size: usize, radix: i32) -> i32 {
    put(buf, size, &signed_digits(v as i64, 32, radix), radix)
}
unsafe extern "win64" fn i64toa_s(v: i64, buf: *mut u8, size: usize, radix: i32) -> i32 {
    put(buf, size, &signed_digits(v, 64, radix), radix)
}
unsafe extern "win64" fn i64tow_s(v: i64, buf: *mut u16, size: usize, radix: i32) -> i32 {
    put(buf, size, &signed_digits(v, 64, radix), radix)
}
unsafe extern "win64" fn ui64toa_s(v: u64, buf: *mut u8, size: usize, radix: i32) -> i32 {
    put(
        buf,
        size,
        &digits(v, false, radix.clamp(2, 36) as u32),
        radix,
    )
}
unsafe extern "win64" fn ui64tow_s(v: u64, buf: *mut u16, size: usize, radix: i32) -> i32 {
    put(
        buf,
        size,
        &digits(v, false, radix.clamp(2, 36) as u32),
        radix,
    )
}
unsafe extern "win64" fn ultow_s(v: u32, buf: *mut u16, size: usize, radix: i32) -> i32 {
    put(
        buf,
        size,
        &digits(v as u64, false, radix.clamp(2, 36) as u32),
        radix,
    )
}

/// `strtoimax` : intmax_t vaut 64 bits.
unsafe extern "win64" fn strtoimax(s: *const u8, end: *mut *const u8, base: i32) -> i64 {
    super::crt::strtoll(s, end as _, base)
}

// ---------------------------------------------------------------------
// Chaînes
// ---------------------------------------------------------------------

unsafe fn wlen(s: *const u16) -> usize {
    crate::printf::wide_len(s)
}

unsafe extern "win64" fn wcslwr(s: *mut u16) -> *mut u16 {
    if !s.is_null() {
        for i in 0..wlen(s) {
            *s.add(i) = super::crt_ucrt::towlower(*s.add(i));
        }
    }
    s
}
unsafe extern "win64" fn wcsupr(s: *mut u16) -> *mut u16 {
    if !s.is_null() {
        for i in 0..wlen(s) {
            *s.add(i) = super::crt_ucrt::towupper(*s.add(i));
        }
    }
    s
}
unsafe extern "win64" fn wcslwr_s(s: *mut u16, size: usize) -> i32 {
    if s.is_null() || wlen(s) >= size {
        return fail(EINVAL);
    }
    wcslwr(s);
    0
}
unsafe extern "win64" fn strupr_s(s: *mut u8, size: usize) -> i32 {
    if s.is_null() || crate::printf::narrow_len(s) >= size {
        return fail(EINVAL);
    }
    super::crt::strupr(s);
    0
}

unsafe extern "win64" fn wcscspn(s: *const u16, reject: *const u16) -> usize {
    let r = std::slice::from_raw_parts(reject, wlen(reject));
    let n = wlen(s);
    (0..n).find(|&i| r.contains(&*s.add(i))).unwrap_or(n)
}

/// `wcstok` d'ucrtbase : `wcstok_s` ; sans contexte, un contexte par thread.
unsafe extern "win64" fn wcstok(s: *mut u16, delim: *const u16, ctx: *mut usize) -> *mut u16 {
    thread_local! { static CTX: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
    if !ctx.is_null() {
        return super::crt_proc::wcstok_s(s, delim, ctx as usize);
    }
    let mut c = CTX.with(|c| c.get());
    let r = super::crt_proc::wcstok_s(s, delim, &mut c as *mut usize as usize);
    CTX.with(|x| x.set(c));
    r
}

/// Page de code ANSI à un octet (Winlux) : `_mbsrchr` et `_mbscmp` sont
/// `strrchr` et `strcmp`, sauf octet de tête DBCS.
unsafe extern "win64" fn mbsrchr(s: *const u8, c: u32) -> *const u8 {
    if s.is_null() {
        return std::ptr::null();
    }
    let cp = crate::intl::codepage::resolve(crate::intl::codepage::CP_ACP);
    let n = crate::printf::narrow_len(s);
    let mut last = std::ptr::null();
    let mut i = 0;
    while i <= n {
        let b = *s.add(i);
        if i < n && crate::intl::dbcs::is_lead_byte(cp, b) && i + 1 < n {
            if c == ((b as u32) << 8 | *s.add(i + 1) as u32) {
                last = s.add(i);
            }
            i += 2;
            continue;
        }
        if b as u32 == c {
            last = s.add(i);
        }
        i += 1;
    }
    last
}

unsafe extern "win64" fn mbscmp(a: *const u8, b: *const u8) -> i32 {
    if a.is_null() || b.is_null() {
        *errno() = EINVAL;
        return i32::MAX;
    }
    libc::strcmp(a as _, b as _).signum()
}

extern "win64" fn isleadbyte(c: i32) -> i32 {
    let cp = crate::intl::codepage::resolve(crate::intl::codepage::CP_ACP);
    crate::intl::dbcs::is_lead_byte(cp, c as u8) as i32
}

unsafe extern "win64" fn wcstombs_s(
    done: *mut usize,
    dst: *mut u8,
    size: usize,
    src: *const u16,
    count: usize,
) -> i32 {
    if src.is_null() || (dst.is_null() && size != 0) {
        return fail(EINVAL);
    }
    let n = wlen(src).min(count);
    let text = String::from_utf16_lossy(std::slice::from_raw_parts(src, n));
    let bytes = crate::dlls::encode_ansi(&text);
    if !done.is_null() {
        *done = bytes.len() + 1;
    }
    if dst.is_null() {
        return 0;
    }
    if bytes.len() + 1 > size {
        *dst = 0;
        return fail(ERANGE);
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
    *dst.add(bytes.len()) = 0;
    0
}

// ---------- Chemins ----------

/// Découpe `X:\dir\nom.ext` en (lecteur, dossier, nom, extension).
fn split_path(p: &[u16]) -> (Vec<u16>, Vec<u16>, Vec<u16>, Vec<u16>) {
    let colon = b':' as u16;
    let (drive, rest) = if p.len() >= 2 && p[1] == colon {
        (p[..2].to_vec(), &p[2..])
    } else {
        (Vec::new(), p)
    };
    let sep = rest
        .iter()
        .rposition(|&c| c == b'\\' as u16 || c == b'/' as u16);
    let (dir, file) = match sep {
        Some(i) => (rest[..=i].to_vec(), &rest[i + 1..]),
        None => (Vec::new(), rest),
    };
    let (name, ext) = match file.iter().rposition(|&c| c == b'.' as u16) {
        Some(i) => (file[..i].to_vec(), file[i..].to_vec()),
        None => (file.to_vec(), Vec::new()),
    };
    (drive, dir, name, ext)
}

unsafe fn put_part<T: Copy + TryFrom<u16> + From<u8>>(
    dst: *mut T,
    size: usize,
    part: &[u16],
) -> bool {
    if dst.is_null() {
        return true;
    }
    if part.len() + 1 > size {
        if size > 0 {
            *dst = T::from(0);
        }
        return false;
    }
    for (i, &c) in part.iter().enumerate() {
        *dst.add(i) = T::try_from(c).unwrap_or(T::from(b'?'));
    }
    *dst.add(part.len()) = T::from(0);
    true
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn wsplitpath_s(
    path: *const u16,
    drive: *mut u16,
    dsz: usize,
    dir: *mut u16,
    dirsz: usize,
    name: *mut u16,
    nsz: usize,
    ext: *mut u16,
    esz: usize,
) -> i32 {
    if path.is_null() {
        return fail(EINVAL);
    }
    let (d, di, n, e) = split_path(std::slice::from_raw_parts(path, wlen(path)));
    let ok = put_part(drive, dsz, &d)
        & put_part(dir, dirsz, &di)
        & put_part(name, nsz, &n)
        & put_part(ext, esz, &e);
    if ok {
        0
    } else {
        fail(ERANGE)
    }
}

unsafe extern "win64" fn wsplitpath(
    path: *const u16,
    drive: *mut u16,
    dir: *mut u16,
    name: *mut u16,
    ext: *mut u16,
) {
    wsplitpath_s(path, drive, 3, dir, 256, name, 256, ext, 256);
}

unsafe extern "win64" fn splitpath(
    path: *const u8,
    drive: *mut u8,
    dir: *mut u8,
    name: *mut u8,
    ext: *mut u8,
) {
    if path.is_null() {
        *errno() = EINVAL;
        return;
    }
    let raw = std::slice::from_raw_parts(path, crate::printf::narrow_len(path));
    let wide: Vec<u16> = raw.iter().map(|&b| b as u16).collect();
    let (d, di, n, e) = split_path(&wide);
    put_part(drive, 3, &d);
    put_part(dir, 256, &di);
    put_part(name, 256, &n);
    put_part(ext, 256, &e);
}

unsafe extern "win64" fn wmakepath_s(
    buf: *mut u16,
    size: usize,
    drive: *const u16,
    dir: *const u16,
    name: *const u16,
    ext: *const u16,
) -> i32 {
    if buf.is_null() || size == 0 {
        return fail(EINVAL);
    }
    let part = |p: *const u16| -> Vec<u16> {
        if p.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(p, wlen(p)).to_vec()
        }
    };
    let mut out = Vec::new();
    let d = part(drive);
    if let Some(&c) = d.first() {
        out.push(c);
        out.push(b':' as u16);
    }
    let di = part(dir);
    if !di.is_empty() {
        out.extend_from_slice(&di);
        if !matches!(di.last(), Some(&c) if c == b'\\' as u16 || c == b'/' as u16) {
            out.push(b'\\' as u16);
        }
    }
    out.extend(part(name));
    let e = part(ext);
    if !e.is_empty() {
        if e[0] != b'.' as u16 {
            out.push(b'.' as u16);
        }
        out.extend(e);
    }
    if out.len() + 1 > size {
        *buf = 0;
        return fail(ERANGE);
    }
    std::ptr::copy_nonoverlapping(out.as_ptr(), buf, out.len());
    *buf.add(out.len()) = 0;
    0
}

// ---------------------------------------------------------------------
// Fonctions « _s » voisines (Pythonwin, fiche 1393)
// ---------------------------------------------------------------------

/// `wmemcpy_s` : comme `memcpy_s`, tailles comptées en caractères larges.
unsafe extern "win64" fn wmemcpy_s(dst: *mut u16, size: usize, src: *const u16, n: usize) -> i32 {
    if n == 0 {
        return 0;
    }
    if dst.is_null() {
        return fail(EINVAL);
    }
    if src.is_null() || size < n {
        std::ptr::write_bytes(dst, 0, size);
        return fail(if src.is_null() { EINVAL } else { ERANGE });
    }
    std::ptr::copy(src, dst, n);
    0
}
/// `wmemmove_s` : la destination n'est pas effacée en cas d'erreur.
unsafe extern "win64" fn wmemmove_s(dst: *mut u16, size: usize, src: *const u16, n: usize) -> i32 {
    if n == 0 {
        return 0;
    }
    if dst.is_null() || src.is_null() {
        return fail(EINVAL);
    }
    if size < n {
        return fail(ERANGE);
    }
    std::ptr::copy(src, dst, n);
    0
}

/// Longueur d'une chaîne terminée dans `size` éléments, sinon `None`.
unsafe fn len_within<T: Copy + PartialEq + From<u8>>(s: *const T, size: usize) -> Option<usize> {
    (0..size).find(|&i| *s.add(i) == T::from(0))
}

/// `_strset_s`, `_wcsset_s`, `_strnset_s`, `_wcsnset_s` : chaîne non
/// terminée dans `size` → effacée, EINVAL.
unsafe fn set_s<T: Copy + PartialEq + From<u8>>(s: *mut T, size: usize, c: T, count: usize) -> i32 {
    if s.is_null() || size == 0 {
        return fail(EINVAL);
    }
    let Some(n) = len_within(s, size) else {
        *s = T::from(0);
        return fail(EINVAL);
    };
    for i in 0..n.min(count) {
        *s.add(i) = c;
    }
    0
}
unsafe extern "win64" fn strset_s(s: *mut u8, size: usize, c: i32) -> i32 {
    set_s(s, size, c as u8, usize::MAX)
}
unsafe extern "win64" fn wcsset_s(s: *mut u16, size: usize, c: u16) -> i32 {
    set_s(s, size, c, usize::MAX)
}
unsafe extern "win64" fn strnset_s(s: *mut u8, size: usize, c: i32, count: usize) -> i32 {
    set_s(s, size, c as u8, count)
}
unsafe extern "win64" fn wcsnset_s(s: *mut u16, size: usize, c: u16, count: usize) -> i32 {
    set_s(s, size, c, count)
}

unsafe extern "win64" fn wcsupr_s(s: *mut u16, size: usize) -> i32 {
    if s.is_null() || wlen(s) >= size {
        return fail(EINVAL);
    }
    wcsupr(s);
    0
}
unsafe extern "win64" fn wcslwr_s_l(s: *mut u16, size: usize, _locale: usize) -> i32 {
    wcslwr_s(s, size)
}
unsafe extern "win64" fn wcsupr_s_l(s: *mut u16, size: usize, _locale: usize) -> i32 {
    wcsupr_s(s, size)
}
unsafe extern "win64" fn strlwr_s(s: *mut u8, size: usize) -> i32 {
    if s.is_null() || crate::printf::narrow_len(s) >= size {
        return fail(EINVAL);
    }
    for i in 0..crate::printf::narrow_len(s) {
        *s.add(i) = (*s.add(i)).to_ascii_lowercase();
    }
    0
}

/// `_mbscpy_s` : copie d'octets, comme `strcpy_s`.
unsafe extern "win64" fn mbscpy_s(dst: *mut u8, size: usize, src: *const u8) -> i32 {
    if dst.is_null() || size == 0 {
        return fail(EINVAL);
    }
    if src.is_null() {
        *dst = 0;
        return fail(EINVAL);
    }
    let n = crate::printf::narrow_len(src);
    if n + 1 > size {
        *dst = 0;
        return fail(ERANGE);
    }
    std::ptr::copy(src, dst, n + 1);
    0
}

/// `_ltoa_s` et `_ultoa_s` : `long` vaut 32 bits sous Windows.
unsafe extern "win64" fn ltoa_s(v: i32, buf: *mut u8, size: usize, radix: i32) -> i32 {
    put(buf, size, &signed_digits(v as i64, 32, radix), radix)
}
unsafe extern "win64" fn ultoa_s(v: u32, buf: *mut u8, size: usize, radix: i32) -> i32 {
    put(
        buf,
        size,
        &digits(v as u64, false, radix.clamp(2, 36) as u32),
        radix,
    )
}

/// Octets vus comme des unités de 16 bits (et retour) : les séparateurs
/// et les octets de tête multioctets restent intacts.
unsafe fn widen(p: *const u8) -> Option<Vec<u16>> {
    (!p.is_null()).then(|| {
        std::slice::from_raw_parts(p, crate::printf::narrow_len(p))
            .iter()
            .map(|&b| b as u16)
            .collect()
    })
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn splitpath_s(
    path: *const u8,
    drive: *mut u8,
    dsz: usize,
    dir: *mut u8,
    dirsz: usize,
    name: *mut u8,
    nsz: usize,
    ext: *mut u8,
    esz: usize,
) -> i32 {
    let Some(wide) = widen(path) else {
        return fail(EINVAL);
    };
    let (d, di, n, e) = split_path(&wide);
    let ok = put_part(drive, dsz, &d)
        & put_part(dir, dirsz, &di)
        & put_part(name, nsz, &n)
        & put_part(ext, esz, &e);
    if ok {
        0
    } else {
        fail(ERANGE)
    }
}

unsafe extern "win64" fn makepath_s(
    buf: *mut u8,
    size: usize,
    drive: *const u8,
    dir: *const u8,
    name: *const u8,
    ext: *const u8,
) -> i32 {
    if buf.is_null() || size == 0 {
        return fail(EINVAL);
    }
    let z = |v: Option<Vec<u16>>| {
        v.map(|mut v| {
            v.push(0);
            v
        })
    };
    let parts = [widen(drive), widen(dir), widen(name), widen(ext)].map(z);
    let ptr = |i: usize| parts[i].as_ref().map_or(std::ptr::null(), |v| v.as_ptr());
    let mut out = vec![0u16; size];
    let r = wmakepath_s(out.as_mut_ptr(), size, ptr(0), ptr(1), ptr(2), ptr(3));
    if r != 0 {
        *buf = 0;
        return r;
    }
    for (i, &c) in out.iter().enumerate() {
        *buf.add(i) = c as u8;
        if c == 0 {
            break;
        }
    }
    0
}

// ---------------------------------------------------------------------
// Tas
// ---------------------------------------------------------------------

unsafe extern "win64" fn malloc_base(n: usize) -> *mut c_void {
    libc::malloc(n.max(1))
}
unsafe extern "win64" fn calloc_base(n: usize, size: usize) -> *mut c_void {
    libc::calloc(n.max(1), size.max(1))
}
unsafe extern "win64" fn realloc_base(p: *mut c_void, n: usize) -> *mut c_void {
    libc::realloc(p, n)
}
unsafe extern "win64" fn free_base(p: *mut c_void) {
    libc::free(p)
}

/// `_recalloc` : comme realloc, la partie ajoutée remise à zéro.
unsafe extern "win64" fn recalloc(p: *mut c_void, n: usize, size: usize) -> *mut c_void {
    let Some(total) = n.checked_mul(size) else {
        *errno() = 12;
        return std::ptr::null_mut();
    };
    let old = if p.is_null() {
        0
    } else {
        libc::malloc_usable_size(p)
    };
    let q = libc::realloc(p, total.max(1));
    if !q.is_null() && total > old {
        std::ptr::write_bytes((q as *mut u8).add(old), 0, total - old);
    }
    q
}

/// Blocs de `_aligned_offset_malloc` : adresse rendue → bloc de libc.
/// `_aligned_free` (crt_runtime) les reconnaît.
fn offset_blocks() -> &'static std::sync::Mutex<std::collections::HashMap<usize, usize>> {
    static M: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<usize, usize>>> =
        std::sync::OnceLock::new();
    M.get_or_init(Default::default)
}

/// Rend le bloc libc d'une adresse de `_aligned_offset_malloc`, si c'en est une.
pub(crate) fn take_offset_block(p: usize) -> Option<usize> {
    super::lock(offset_blocks()).remove(&p)
}

/// `_aligned_offset_malloc(size, align, offset)` : `p + offset` aligné.
/// `_aligned_free` libère le bloc d'origine (`take_offset_block`).
unsafe extern "win64" fn aligned_offset_malloc(
    size: usize,
    align: usize,
    offset: usize,
) -> *mut c_void {
    if align == 0 || !align.is_power_of_two() || (offset >= size && size != 0) {
        *errno() = EINVAL;
        return std::ptr::null_mut();
    }
    let base = libc::malloc(size + align + offset) as usize;
    if base == 0 {
        *errno() = 12;
        return std::ptr::null_mut();
    }
    let p = ((base + offset + align - 1) & !(align - 1)) - offset;
    let p = if p < base { p + align } else { p };
    super::lock(offset_blocks()).insert(p, base);
    p as *mut c_void
}

unsafe extern "win64" fn get_heap_handle() -> usize {
    match super::lookup("kernel32", "GetProcessHeap") {
        Some(a) => std::mem::transmute::<usize, extern "win64" fn() -> usize>(a)(),
        None => 0,
    }
}

// ---------------------------------------------------------------------
// Fichiers, processus, environnement
// ---------------------------------------------------------------------

unsafe extern "win64" fn wfopen_s(
    out: *mut *mut c_void,
    path: *const u16,
    mode: *const u16,
) -> i32 {
    if out.is_null() || path.is_null() || mode.is_null() {
        return fail(EINVAL);
    }
    let f = super::crt_stdio::wfopen(path, mode);
    *out = f;
    if f.is_null() {
        match *errno() {
            0 => fail(2),
            e => e,
        }
    } else {
        0
    }
}

unsafe extern "win64" fn chsize(fd: i32, size: i32) -> i32 {
    match super::crt_proc::chsize_s(fd, size as i64) {
        0 => 0,
        e => {
            *errno() = e;
            -1
        }
    }
}

/// `_wgetdcwd(lecteur, buf, taille)` : lecteur 0 = courant. Winlux garde
/// un seul dossier courant : celui du lecteur courant, sinon la racine.
unsafe extern "win64" fn wgetdcwd(drive: i32, buf: *mut u16, size: i32) -> *mut u16 {
    let cur = super::crt_extra::wgetcwd(std::ptr::null_mut(), 0);
    if cur.is_null() {
        return std::ptr::null_mut();
    }
    let mut text: Vec<u16> = std::slice::from_raw_parts(cur, wlen(cur)).to_vec();
    libc::free(cur as *mut c_void);
    if drive > 0 {
        let letter = (b'A' + (drive - 1) as u8) as u16;
        let same =
            text.first().map(|&c| c & !0x20) == Some(letter) && text.get(1) == Some(&(b':' as u16));
        if !same {
            text = vec![letter, b':' as u16, b'\\' as u16];
        }
    }
    if buf.is_null() {
        let n = (text.len() + 1).max(size.max(0) as usize);
        let p = libc::calloc(n, 2) as *mut u16;
        if !p.is_null() {
            std::ptr::copy_nonoverlapping(text.as_ptr(), p, text.len());
        }
        return p;
    }
    if text.len() + 1 > size.max(0) as usize {
        *errno() = ERANGE;
        return std::ptr::null_mut();
    }
    std::ptr::copy_nonoverlapping(text.as_ptr(), buf, text.len());
    *buf.add(text.len()) = 0;
    buf
}

unsafe extern "win64" fn wcreat(path: *const u16, mode: i32) -> i32 {
    // _O_RDWR | _O_CREAT | _O_TRUNC
    super::crt_runtime::wopen(path, 0x0002 | 0x0100 | 0x0200, mode as u32)
}

unsafe extern "win64" fn system(cmd: *const u8) -> i32 {
    if cmd.is_null() {
        return super::crt_proc::wsystem(std::ptr::null());
    }
    let mut w: Vec<u16> = super::cstr(cmd).encode_utf16().collect();
    w.push(0);
    super::crt_proc::wsystem(w.as_ptr())
}

unsafe extern "win64" fn wspawnvp(mode: i32, file: *const u16, argv: usize) -> isize {
    super::crt_proc::wspawnv(mode, file, argv)
}
unsafe extern "win64" fn wspawnvpe(mode: i32, file: *const u16, argv: usize, envp: usize) -> isize {
    super::crt_proc::wspawnve(mode, file, argv, envp)
}

unsafe fn wenv(name: *const u16) -> Option<Vec<u16>> {
    if name.is_null() {
        return None;
    }
    let v = super::crt_runtime::wgetenv(name);
    (!v.is_null()).then(|| std::slice::from_raw_parts(v, wlen(v)).to_vec())
}

unsafe extern "win64" fn wdupenv_s(out: *mut *mut u16, len: *mut usize, name: *const u16) -> i32 {
    if out.is_null() || name.is_null() {
        return fail(EINVAL);
    }
    *out = std::ptr::null_mut();
    if !len.is_null() {
        *len = 0;
    }
    if let Some(v) = wenv(name) {
        let p = libc::calloc(v.len() + 1, 2) as *mut u16;
        if p.is_null() {
            return fail(12);
        }
        std::ptr::copy_nonoverlapping(v.as_ptr(), p, v.len());
        *out = p;
        if !len.is_null() {
            *len = v.len() + 1;
        }
    }
    0
}

unsafe extern "win64" fn wgetenv_s(
    len: *mut usize,
    buf: *mut u16,
    size: usize,
    name: *const u16,
) -> i32 {
    if len.is_null() || name.is_null() || (buf.is_null() && size != 0) {
        return fail(EINVAL);
    }
    if !buf.is_null() && size > 0 {
        *buf = 0;
    }
    let Some(v) = wenv(name) else {
        *len = 0;
        return 0;
    };
    *len = v.len() + 1;
    if buf.is_null() {
        return 0;
    }
    if v.len() + 1 > size {
        return fail(ERANGE);
    }
    std::ptr::copy_nonoverlapping(v.as_ptr(), buf, v.len());
    *buf.add(v.len()) = 0;
    0
}

/// `_mktemp` : les six « X » de fin deviennent une lettre puis le numéro
/// du processus ; la lettre avance tant que le nom existe.
unsafe extern "win64" fn mktemp(t: *mut u8) -> *mut u8 {
    if t.is_null() {
        *errno() = EINVAL;
        return std::ptr::null_mut();
    }
    let n = crate::printf::narrow_len(t);
    if n < 6 || (0..6).any(|i| *t.add(n - 6 + i) != b'X') {
        *errno() = EINVAL;
        return std::ptr::null_mut();
    }
    let pid = format!("{:05}", std::process::id() % 100_000);
    for (i, &c) in pid.as_bytes().iter().enumerate() {
        *t.add(n - 5 + i) = c;
    }
    for letter in b'a'..=b'z' {
        *t.add(n - 6) = letter;
        let name = super::cstr(t);
        if !crate::paths::to_linux(&name).exists() {
            return t;
        }
    }
    *errno() = 17;
    std::ptr::null_mut()
}

/// `_tempnam(dossier, préfixe)` : chemin de fichier temporaire inexistant,
/// alloué par malloc. `TMP` passe avant `dossier`, comme la CRT.
unsafe extern "win64" fn tempnam(dir: *const u8, prefix: *const u8) -> *mut u8 {
    let tmp = {
        let mut w = [0u16; 512];
        let n =
            match super::lookup("kernel32", "GetTempPathW") {
                Some(a) => std::mem::transmute::<
                    usize,
                    unsafe extern "win64" fn(u32, *mut u16) -> u32,
                >(a)(512, w.as_mut_ptr()),
                None => 0,
            } as usize;
        String::from_utf16_lossy(&w[..n.min(512)])
    };
    let base = if !tmp.is_empty() {
        tmp
    } else if !dir.is_null() && *dir != 0 {
        super::cstr(dir)
    } else {
        "C:\\Windows\\Temp\\".to_string()
    };
    let base = if base.ends_with('\\') || base.ends_with('/') {
        base
    } else {
        format!("{base}\\")
    };
    let pre = if prefix.is_null() {
        String::new()
    } else {
        super::cstr(prefix)
    };
    static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    for _ in 0..10_000 {
        let k = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = format!("{base}{pre}{:x}{k:x}", std::process::id());
        if !crate::paths::to_linux(&name).exists() {
            let mut b = crate::dlls::encode_ansi(&name);
            b.push(0);
            let p = libc::malloc(b.len()) as *mut u8;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(b.as_ptr(), p, b.len());
            }
            return p;
        }
    }
    *errno() = 17;
    std::ptr::null_mut()
}

unsafe extern "win64" fn fflush_nolock(f: *mut c_void) -> i32 {
    super::crt_stdio::fflush(f)
}

// ---------------------------------------------------------------------
// Date et heure
// ---------------------------------------------------------------------

fn local_now() -> libc::tm {
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm
    }
}

unsafe fn put_ascii(buf: *mut u8, s: &str) -> *mut u8 {
    if !buf.is_null() {
        std::ptr::copy_nonoverlapping(s.as_ptr(), buf, s.len());
        *buf.add(s.len()) = 0;
    }
    buf
}

/// `_strdate` : « MM/JJ/AA ».
unsafe extern "win64" fn strdate(buf: *mut u8) -> *mut u8 {
    let t = local_now();
    put_ascii(
        buf,
        &format!(
            "{:02}/{:02}/{:02}",
            t.tm_mon + 1,
            t.tm_mday,
            t.tm_year % 100
        ),
    )
}

/// `_strtime` : « HH:MM:SS ».
unsafe extern "win64" fn strtime(buf: *mut u8) -> *mut u8 {
    let t = local_now();
    put_ascii(
        buf,
        &format!("{:02}:{:02}:{:02}", t.tm_hour, t.tm_min, t.tm_sec),
    )
}

/// Tampon par thread alloué sur le tas : un programme x86 doit pouvoir le
/// lire (la mémoire statique du runtime est au-dessus de 4 Go).
unsafe fn thread_buf(bytes: usize) -> *mut u8 {
    thread_local! { static BUF: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
    BUF.with(|b| {
        if b.get() == 0 {
            b.set(libc::calloc(1, bytes.max(64)) as usize);
        }
        b.get() as *mut u8
    })
}

unsafe extern "win64" fn wctime64(t: *const i64) -> *mut u16 {
    let s = super::crt_runtime::ctime64(t);
    if s.is_null() {
        return std::ptr::null_mut();
    }
    let n = crate::printf::narrow_len(s).min(31);
    let out = thread_buf(64) as *mut u16;
    for i in 0..n {
        *out.add(i) = *s.add(i) as u16;
    }
    *out.add(n) = 0;
    out
}

/// `_get_tzname(&taille, tampon, taille_tampon, index)` : 0 nom d'hiver,
/// 1 nom d'été.
unsafe extern "win64" fn get_tzname(len: *mut usize, buf: *mut u8, size: usize, index: i32) -> i32 {
    if len.is_null() || !(0..=1).contains(&index) || (buf.is_null() && size != 0) {
        return fail(EINVAL);
    }
    let mut tm = local_now();
    tm.tm_mon = if index == 0 { 0 } else { 6 };
    tm.tm_mday = 1;
    tm.tm_isdst = -1;
    let t = libc::mktime(&mut tm);
    let mut r: libc::tm = std::mem::zeroed();
    libc::localtime_r(&t, &mut r);
    let name = if r.tm_zone.is_null() {
        Vec::new()
    } else {
        std::ffi::CStr::from_ptr(r.tm_zone).to_bytes().to_vec()
    };
    *len = name.len() + 1;
    if buf.is_null() {
        return 0;
    }
    if name.len() + 1 > size {
        return fail(ERANGE);
    }
    std::ptr::copy_nonoverlapping(name.as_ptr(), buf, name.len());
    *buf.add(name.len()) = 0;
    0
}

// ---------------------------------------------------------------------
// _ecvt
// ---------------------------------------------------------------------

/// Chiffres de `v` sur `n` chiffres significatifs, position du point
/// décimal et signe.
fn ecvt_parts(v: f64, n: i32) -> (String, i32, bool) {
    let neg = v.is_sign_negative() && v != 0.0;
    let a = v.abs();
    let n = n.clamp(1, 17) as usize;
    if a == 0.0 {
        return ("0".repeat(n), 0, neg);
    }
    let s = format!("{:.*e}", n - 1, a);
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let digits: String = mant.chars().filter(|c| c.is_ascii_digit()).collect();
    (digits, exp.parse::<i32>().unwrap_or(0) + 1, neg)
}

unsafe extern "win64" fn ecvt(v: f64, n: i32, dec: *mut i32, sign: *mut i32) -> *mut u8 {
    let (d, e, neg) = ecvt_parts(v, n);
    if !dec.is_null() {
        *dec = e;
    }
    if !sign.is_null() {
        *sign = neg as i32;
    }
    put_ascii(thread_buf(64), &d)
}

unsafe extern "win64" fn ecvt_s(
    buf: *mut u8,
    size: usize,
    v: f64,
    n: i32,
    dec: *mut i32,
    sign: *mut i32,
) -> i32 {
    if buf.is_null() || size == 0 || dec.is_null() || sign.is_null() {
        return fail(EINVAL);
    }
    let (d, e, neg) = ecvt_parts(v, n);
    if d.len() + 1 > size {
        *buf = 0;
        return fail(ERANGE);
    }
    put_ascii(buf, &d);
    *dec = e;
    *sign = neg as i32;
    0
}

// ---------------------------------------------------------------------
// C++ et exceptions
// ---------------------------------------------------------------------

/// `__processing_throw` : compteur par thread des lancers en cours.
unsafe extern "win64" fn processing_throw() -> *mut i32 {
    thread_buf(64).add(32) as *mut i32
}

/// Nom de type lisible d'un nom décoré RTTI MSVC (`.?AVnom@espace@@`) :
/// « class espace::nom ». Les modèles gardent leurs arguments décorés.
pub(crate) fn undecorate_type(dec: &str) -> String {
    let body = dec.strip_prefix('.').unwrap_or(dec);
    let mut p = TypeParser {
        s: body.as_bytes(),
        i: 0,
        names: Vec::new(),
    };
    match p.ty() {
        Some(t) if p.i == p.s.len() => t,
        _ => dec.to_string(),
    }
}

/// Lecture des noms de type décorés de MSVC (sous-ensemble des RTTI :
/// classes, structures, unions, énumérations, modèles, types de base,
/// pointeurs), d'après la description publique du schéma de décoration.
struct TypeParser<'a> {
    s: &'a [u8],
    i: usize,
    /// Fragments de nom déjà lus, pour les renvois `0`…`9`.
    names: Vec<String>,
}

impl TypeParser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    /// Un type, avec son mot-clé pour les types composés.
    fn ty(&mut self) -> Option<String> {
        let c = self.peek()?;
        self.i += 1;
        let kw = match c {
            b'V' => "class ",
            b'U' => "struct ",
            b'T' => "union ",
            b'W' => {
                self.i += 1; // taille sous-jacente (4 = int)
                "enum "
            }
            b'?' => {
                // `?A` + type : forme des RTTI (`.?AV…`).
                self.eat(b'A');
                return self.ty();
            }
            b'P' | b'Q' | b'A' => {
                // Pointeur (P), pointeur constant (Q), référence (A) :
                // modificateur 64 bits facultatif (E), qualifiants (A à D).
                self.eat(b'E');
                let q = self.peek()?;
                self.i += 1;
                let inner = self.ty()?;
                let cv = match q {
                    b'B' => " const",
                    b'C' => " volatile",
                    b'D' => " const volatile",
                    _ => "",
                };
                let sym = if c == b'A' { "&" } else { "*" };
                return Some(format!("{inner}{cv} {sym}"));
            }
            b'_' => {
                let code = [b'_', self.peek()?];
                self.i += 1;
                return builtin_type(std::str::from_utf8(&code).ok()?).map(str::to_string);
            }
            _ => {
                return builtin_type(std::str::from_utf8(&[c]).ok()?).map(str::to_string);
            }
        };
        Some(format!("{kw}{}", self.qualified()?))
    }
    /// Nom qualifié : fragments du plus intérieur au plus extérieur, fin `@`.
    fn qualified(&mut self) -> Option<String> {
        let mut parts = Vec::new();
        loop {
            match self.peek()? {
                b'@' => {
                    self.i += 1;
                    break;
                }
                _ => parts.push(self.fragment()?),
            }
        }
        parts.reverse();
        Some(parts.join("::"))
    }
    fn fragment(&mut self) -> Option<String> {
        let c = self.peek()?;
        if c.is_ascii_digit() {
            self.i += 1;
            return self.names.get((c - b'0') as usize).cloned();
        }
        if self.s[self.i..].starts_with(b"?$") {
            self.i += 2;
            // Les arguments d'un modèle ont leur propre table de renvois.
            let saved = std::mem::take(&mut self.names);
            let name = self.simple()?;
            self.names.push(name.clone());
            let mut args = Vec::new();
            while !self.eat(b'@') {
                // Comme type_info::name : « class std::allocator<int> ».
                args.push(self.ty()?);
            }
            self.names = saved;
            let mut text = format!("{name}<{}", args.join(","));
            if text.ends_with('>') {
                text.push(' ');
            }
            text.push('>');
            if self.names.len() < 10 {
                self.names.push(text.clone());
            }
            return Some(text);
        }
        let name = self.simple()?;
        if self.names.len() < 10 {
            self.names.push(name.clone());
        }
        Some(name)
    }
    fn simple(&mut self) -> Option<String> {
        let rest = &self.s[self.i..];
        let end = rest.iter().position(|&b| b == b'@')?;
        self.i += end + 1;
        String::from_utf8(rest[..end].to_vec()).ok()
    }
}

fn builtin_type(c: &str) -> Option<&'static str> {
    Some(match c {
        "C" => "signed char",
        "D" => "char",
        "E" => "unsigned char",
        "F" => "short",
        "G" => "unsigned short",
        "H" => "int",
        "I" => "unsigned int",
        "J" => "long",
        "K" => "unsigned long",
        "M" => "float",
        "N" => "double",
        "O" => "long double",
        "X" => "void",
        "_J" => "__int64",
        "_K" => "unsigned __int64",
        "_N" => "bool",
        "_W" => "wchar_t",
        _ => return None,
    })
}

/// `__std_type_info_name(data, liste)` : `data` = { nom lisible mis en
/// cache (pointeur), nom décoré }. Le nom est alloué une fois, sur le tas.
unsafe extern "win64" fn std_type_info_name(data: *mut u8, _root: usize) -> *const u8 {
    let ps = crate::wow32::pointer_size();
    let cached = if ps == 8 {
        (data as *const usize).read_unaligned()
    } else {
        (data as *const u32).read_unaligned() as usize
    };
    if cached != 0 {
        return cached as *const u8;
    }
    let dec = super::cstr(data.add(ps));
    let mut name = undecorate_type(&dec).into_bytes();
    name.push(0);
    let p = libc::malloc(name.len()) as *mut u8;
    if p.is_null() {
        return std::ptr::null();
    }
    std::ptr::copy_nonoverlapping(name.as_ptr(), p, name.len());
    if ps == 8 {
        (data as *mut usize).write_unaligned(p as usize);
    } else {
        (data as *mut u32).write_unaligned(p as u32);
    }
    p
}

/// `__std_type_info_hash` : FNV-1a du nom décoré, sans le point initial
/// (64 bits en x64, 32 bits en x86).
unsafe extern "win64" fn std_type_info_hash(data: *const u8) -> usize {
    let ps = crate::wow32::pointer_size();
    let name = data.add(ps + 1);
    let bytes = std::slice::from_raw_parts(name, crate::printf::narrow_len(name));
    if ps == 8 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| {
            (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3)
        }) as usize
    } else {
        bytes.iter().fold(0x811c_9dc5u32, |h, &b| {
            (h ^ b as u32).wrapping_mul(0x0100_0193)
        }) as usize
    }
}

static PURECALL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

extern "win64" fn set_purecall_handler(h: usize) -> usize {
    PURECALL.swap(h, std::sync::atomic::Ordering::SeqCst)
}

/// Rappel WinRT de manque de mémoire : aucun projet WinRT sous Winlux.
extern "win64" fn set_winrt_oom_callback(_cb: usize) {}

/// `__GetPlatformExceptionInfo` : pas d'informations d'exception WinRT.
unsafe extern "win64" fn get_platform_exception_info(flag: *mut i32) -> usize {
    if !flag.is_null() {
        *flag = 0;
    }
    0
}

/// `__vcrt_InitializeCriticalSectionEx` : InitializeCriticalSectionEx.
unsafe extern "win64" fn vcrt_init_cs_ex(cs: usize, spin: u32, flags: u32) -> i32 {
    match super::lookup("kernel32", "InitializeCriticalSectionEx") {
        Some(a) => std::mem::transmute::<usize, unsafe extern "win64" fn(usize, u32, u32) -> i32>(
            a,
        )(cs, spin, flags),
        None => 0,
    }
}

// ---------------------------------------------------------------------
// Entrées-sorties formatées
// ---------------------------------------------------------------------

unsafe extern "win64" fn stdio_common_vfscanf(
    options: u64,
    stream: *mut c_void,
    fmt: *const u8,
    _locale: usize,
    va: *const u64,
) -> i32 {
    if stream.is_null() || fmt.is_null() {
        return -fail(EINVAL).signum();
    }
    let mut v = crate::thunks::VaList::new(va);
    super::crt_scan::secure_if(options & 1 != 0, || {
        super::crt_scan::scan_stream(
            stream,
            std::slice::from_raw_parts(fmt, crate::printf::narrow_len(fmt)),
            &mut v,
        )
    })
}

unsafe extern "win64" fn stdio_common_vfwscanf(
    options: u64,
    stream: *mut c_void,
    fmt: *const u16,
    _locale: usize,
    va: *const u64,
) -> i32 {
    if stream.is_null() || fmt.is_null() {
        return -fail(EINVAL).signum();
    }
    let f = super::crt_ucrt::wide_scan_format(std::slice::from_raw_parts(fmt, wlen(fmt)));
    let mut v = crate::thunks::VaList::new(va);
    super::crt_scan::secure_if(options & 1 != 0, || {
        super::crt_scan::scan_stream(stream, &f, &mut v)
    })
}

/// `__conio_common_vcprintf` : vers la console, c'est-à-dire la sortie
/// standard du terminal.
unsafe extern "win64" fn conio_common_vcprintf(
    options: u64,
    fmt: *const u8,
    _locale: usize,
    va: *const u64,
) -> i32 {
    if fmt.is_null() {
        return -fail(EINVAL).signum();
    }
    let out = super::crt_ucrt::format_narrow(options, fmt, va);
    libc::write(1, out.as_ptr() as *const c_void, out.len()) as i32
}

unsafe extern "win64" fn imp_swscanf(args: *const u64) -> i32 {
    super::crt_ucrt::stdio_common_vswscanf(
        0,
        *args as *const u16,
        usize::MAX,
        *args.add(1) as *const u16,
        0,
        args.add(2),
    )
}

/// `swscanf_s` : %s, %c et %[ prennent en plus la taille du tampon.
unsafe extern "win64" fn imp_swscanf_s(args: *const u64) -> i32 {
    super::crt_scan::with_secure(|| imp_swscanf(args))
}

unsafe extern "win64" fn imp_wprintf(args: *const u64) -> i32 {
    let out = super::crt_ucrt::format_wide(0, *args as *const u16, args.add(1));
    let text = String::from_utf16_lossy(&out);
    let stdout = super::crt_stdio::std_stream(1);
    match super::crt_stdio::write_bytes(stdout, text.as_bytes()) {
        n if n < 0 => n,
        _ => out.len() as i32,
    }
}

/// `_vscprintf(format, va)` : longueur du texte formaté.
unsafe extern "win64" fn vscprintf(fmt: *const u8, va: *const u64) -> i32 {
    if fmt.is_null() {
        return -fail(EINVAL).signum();
    }
    super::crt_ucrt::format_narrow(0, fmt, va).len() as i32
}

/// `fgetws(tampon, n, flux)` : une ligne de texte large (fgetwc).
unsafe extern "win64" fn fgetws(buf: *mut u16, n: i32, f: *mut c_void) -> *mut u16 {
    if buf.is_null() || n <= 0 || f.is_null() {
        *errno() = EINVAL;
        return std::ptr::null_mut();
    }
    let mut i = 0usize;
    while (i as i32) < n - 1 {
        let c = super::crt_stdio::fgetwc(f);
        if c == 0xFFFF {
            break;
        }
        *buf.add(i) = c as u16;
        i += 1;
        if c == b'\n' as u32 {
            break;
        }
    }
    if i == 0 {
        return std::ptr::null_mut();
    }
    *buf.add(i) = 0;
    buf
}

crate::varargs_thunk!(swscanf_thunk => imp_swscanf);
crate::varargs_thunk!(swscanf_s_thunk => imp_swscanf_s);
crate::varargs_thunk!(wprintf_thunk => imp_wprintf);

// ---------------------------------------------------------------------
// msvcrt : données internes, opérateurs, wrappers NLS
// ---------------------------------------------------------------------

/// `ioinfo` de msvcrt : 64 tableaux de 32 entrées, adressés par les CRT
/// MinGW qui lisent `__pioinfo[fd >> 5]`. Winlux ne remplit pas ces
/// tables : chaque entrée est vide (« fichier non ouvert »), taille large
/// pour couvrir les dispositions des versions de msvcrt.
const IOINFO_SIZE: usize = 0x60;

/// `__pioinfo` : 64 pointeurs, remplis au démarrage du processus.
static mut PIOINFO: [usize; 64] = [0; 64];
/// `__badioinfo` : handle -1, `osfile` FTEXT (0x80) et `pipech` LF à +8.
static mut BADIOINFO: [u64; IOINFO_SIZE / 8] = {
    let mut b = [0u64; IOINFO_SIZE / 8];
    b[0] = u64::MAX;
    b[1] = 0x0a80;
    b
};

/// Remplit `__pioinfo` de tableaux vides (« fichier non ouvert »).
pub fn process_started() {
    unsafe {
        let t = &mut *std::ptr::addr_of_mut!(PIOINFO);
        if t[0] != 0 {
            return;
        }
        for e in t.iter_mut() {
            *e = libc::calloc(32, IOINFO_SIZE) as usize;
        }
    }
}

unsafe extern "win64" fn op_new(n: usize) -> *mut c_void {
    libc::malloc(n.max(1))
}
unsafe extern "win64" fn op_delete(p: *mut c_void) {
    libc::free(p)
}

/// `__crtLCMapStringW(lcid, flags, src, n, dst, ndst)` (msvcrt).
unsafe extern "win64" fn crt_lcmap_string_w(
    lcid: u32,
    flags: u32,
    src: *const u16,
    n: i32,
    dst: *mut u16,
    nd: i32,
) -> i32 {
    match super::lookup("kernel32", "LCMapStringW") {
        Some(a) => std::mem::transmute::<
            usize,
            unsafe extern "win64" fn(u32, u32, *const u16, i32, *mut u16, i32) -> i32,
        >(a)(lcid, flags, src, n, dst, nd),
        None => 0,
    }
}

/// `__crtLCMapStringA(lcid, flags, src, n, dst, ndst, page, erreur)`.
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn crt_lcmap_string_a(
    lcid: u32,
    flags: u32,
    src: *const u8,
    n: i32,
    dst: *mut u8,
    nd: i32,
    _cp: i32,
    _err: i32,
) -> i32 {
    match super::lookup("kernel32", "LCMapStringA") {
        Some(a) => std::mem::transmute::<
            usize,
            unsafe extern "win64" fn(u32, u32, *const u8, i32, *mut u8, i32) -> i32,
        >(a)(lcid, flags, src, n, dst, nd),
        None => 0,
    }
}

/// `__crtGetStringTypeW(type, src, n, out, page, lcid)`.
unsafe extern "win64" fn crt_get_string_type_w(
    kind: u32,
    src: *const u16,
    n: i32,
    out: *mut u16,
    _cp: i32,
    _lcid: i32,
) -> i32 {
    match super::lookup("kernel32", "GetStringTypeW") {
        Some(a) => std::mem::transmute::<
            usize,
            unsafe extern "win64" fn(u32, *const u16, i32, *mut u16) -> i32,
        >(a)(kind, src, n, out),
        None => 0,
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        // Mathématiques (long double = double sous MSVC).
        "acoshl" => fn acoshl, "asinhl" => fn asinhl, "atanhl" => fn atanhl, "cbrtl" => fn cbrtl,
        "exp2l" => fn exp2l, "expm1l" => fn expm1l, "log1pl" => fn log1pl, "nextafterl" => fn nextafterl,
        "rintl" => fn rintl, "truncl" => fn truncl, "copysignl" => fn copysignl,
        "_copysignf" => fn copysignf, "_nextafterf" => fn nextafterf, "_isnanf" => fn isnanf,
        "ilogb" => fn ilogb, "erff" => fn erff, "nan" => fn nan, "nanf" => fn nanf,
        "feclearexcept" => fn feclearexcept, "fetestexcept" => fn fetestexcept, "_fpreset" => fn fpreset,
        "_dpcomp" => fn dpcomp, "_fdpcomp" => fn fdpcomp, "_ldclass" => fn ldclass, "_dtest" => fn dtest,
        "_fpclass" => fn fpclass,
        // Complexes.
        "cexp" => fn cexp, "cexpl" => fn cexp, "cexpf" => fn cexpf,
        "ccos" => fn ccos, "ccosl" => fn ccos, "ccosf" => fn ccosf,
        "ccosh" => fn ccosh, "ccoshl" => fn ccosh, "ccoshf" => fn ccoshf,
        "csin" => fn csin, "csinl" => fn csin, "csinf" => fn csinf,
        "csinh" => fn csinh, "csinhl" => fn csinh, "csinhf" => fn csinhf,
        "ctan" => fn ctan, "ctanl" => fn ctan, "ctanf" => fn ctanf,
        "ctanh" => fn ctanh, "ctanhl" => fn ctanh, "ctanhf" => fn ctanhf,
        "cpow" => fn cpow, "cpowl" => fn cpow, "cpowf" => fn cpowf,
        "cabs" => fn cabs, "cabsl" => fn cabs, "_cabs" => fn cabs, "cabsf" => fn cabsf,
        "carg" => fn carg, "cargl" => fn carg, "cargf" => fn cargf,
        "creal" => fn creal, "creall" => fn creal, "crealf" => fn crealf,
        "cimag" => fn cimag, "cimagl" => fn cimag, "cimagf" => fn cimagf,
        // Conversions et chaînes.
        "_itoa_s" => fn itoa_s, "_itow_s" => fn itow_s, "_i64toa_s" => fn i64toa_s, "_i64tow_s" => fn i64tow_s,
        "_ui64toa_s" => fn ui64toa_s, "_ui64tow_s" => fn ui64tow_s, "_ultow_s" => fn ultow_s,
        "strtoimax" => fn strtoimax,
        "_wcslwr" => fn wcslwr, "_wcsupr" => fn wcsupr, "_wcslwr_s" => fn wcslwr_s, "_strupr_s" => fn strupr_s,
        "wcscspn" => fn wcscspn, "wcstok" => fn wcstok, "wcstombs_s" => fn wcstombs_s,
        "_mbsrchr" => fn mbsrchr, "_mbscmp" => fn mbscmp, "isleadbyte" => fn isleadbyte,
        "_wsplitpath_s" => fn wsplitpath_s, "_wsplitpath" => fn wsplitpath, "_splitpath" => fn splitpath,
        "_wmakepath_s" => fn wmakepath_s,
        "wmemcpy_s" => fn wmemcpy_s, "wmemmove_s" => fn wmemmove_s,
        "_strset_s" => fn strset_s, "_wcsset_s" => fn wcsset_s,
        "_strnset_s" => fn strnset_s, "_wcsnset_s" => fn wcsnset_s,
        "_wcsupr_s" => fn wcsupr_s, "_strlwr_s" => fn strlwr_s,
        "_wcslwr_s_l" => fn wcslwr_s_l, "_wcsupr_s_l" => fn wcsupr_s_l,
        "_mbscpy_s" => fn mbscpy_s, "_ltoa_s" => fn ltoa_s, "_ultoa_s" => fn ultoa_s,
        "_splitpath_s" => fn splitpath_s, "_makepath_s" => fn makepath_s,
        // Tas.
        "_malloc_base" => fn malloc_base, "_calloc_base" => fn calloc_base,
        "_realloc_base" => fn realloc_base, "_free_base" => fn free_base,
        "_recalloc" => fn recalloc, "_aligned_offset_malloc" => fn aligned_offset_malloc,
        "_get_heap_handle" => fn get_heap_handle,
        // Fichiers, processus, environnement, temps.
        "_wfopen_s" => fn wfopen_s, "_chsize" => fn chsize, "_wgetdcwd" => fn wgetdcwd, "_wcreat" => fn wcreat,
        "system" => fn system, "_wspawnvp" => fn wspawnvp, "_wspawnvpe" => fn wspawnvpe,
        "_wdupenv_s" => fn wdupenv_s, "_wgetenv_s" => fn wgetenv_s,
        "_mktemp" => fn mktemp, "_tempnam" => fn tempnam, "_fflush_nolock" => fn fflush_nolock,
        "_strdate" => fn strdate, "_strtime" => fn strtime, "_wctime64" => fn wctime64, "_wctime" => fn wctime64,
        "_get_tzname" => fn get_tzname, "_ecvt" => fn ecvt, "_ecvt_s" => fn ecvt_s,
        // C++ et exceptions.
        "__processing_throw" => fn processing_throw,
        "__std_type_info_name" => fn std_type_info_name, "__std_type_info_hash" => fn std_type_info_hash,
        "_set_purecall_handler" => fn set_purecall_handler,
        "_SetWinRTOutOfMemoryExceptionCallback" => fn set_winrt_oom_callback,
        "__GetPlatformExceptionInfo" => fn get_platform_exception_info,
        "__vcrt_InitializeCriticalSectionEx" => fn vcrt_init_cs_ex,
        "__pioinfo" => data PIOINFO, "__badioinfo" => data BADIOINFO,
        "??2@YAPEAX_K@Z" => fn op_new, "??3@YAXPEAX@Z" => fn op_delete,
        // Entrées-sorties formatées.
        "__stdio_common_vfscanf" => fn stdio_common_vfscanf, "__stdio_common_vfwscanf" => fn stdio_common_vfwscanf,
        "__stdio_common_vfprintf_s" => fn super::crt_ucrt::stdio_common_vfprintf,
        "__stdio_common_vfwprintf_s" => fn super::crt_ucrt::stdio_common_vfwprintf,
        "__conio_common_vcprintf" => fn conio_common_vcprintf,
        "swscanf" => fn swscanf_thunk, "swscanf_s" => fn swscanf_s_thunk, "wprintf" => fn wprintf_thunk,
        "_vscprintf" => fn vscprintf, "fgetws" => fn fgetws,
        // msvcrt.
        "__crtLCMapStringW" => fn crt_lcmap_string_w, "__crtLCMapStringA" => fn crt_lcmap_string_a,
        "__crtGetStringTypeW" => fn crt_get_string_type_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonctions_s_de_pythonwin() {
        unsafe {
            let src: Vec<u16> = "abc".encode_utf16().collect();
            let mut dst = [9u16; 4];
            assert_eq!(wmemcpy_s(dst.as_mut_ptr(), 4, src.as_ptr(), 3), 0);
            assert_eq!(&dst[..3], &src[..]);
            assert_eq!(wmemcpy_s(dst.as_mut_ptr(), 2, src.as_ptr(), 3), ERANGE);
            assert_eq!(&dst[..2], &[0, 0]); // seule la taille annoncée est effacée
            assert_eq!(wmemmove_s(dst.as_mut_ptr(), 2, src.as_ptr(), 3), ERANGE);

            let mut s = *b"hello\0";
            assert_eq!(strnset_s(s.as_mut_ptr(), 6, b'x' as i32, 2), 0);
            assert_eq!(&s, b"xxllo\0");
            assert_eq!(strset_s(s.as_mut_ptr(), 6, b'y' as i32), 0);
            assert_eq!(&s, b"yyyyy\0");
            assert_eq!(strset_s(s.as_mut_ptr(), 3, b'z' as i32), EINVAL);
            assert_eq!(s[0], 0);

            let mut w: Vec<u16> = "Abc\0".encode_utf16().collect();
            assert_eq!(wcsupr_s(w.as_mut_ptr(), 4), 0);
            assert_eq!(String::from_utf16_lossy(&w[..3]), "ABC");
            assert_eq!(wcsupr_s(w.as_mut_ptr(), 3), EINVAL);

            let mut b = [0u8; 12];
            assert_eq!(ltoa_s(-42, b.as_mut_ptr(), 12, 10), 0);
            assert_eq!(&b[..4], b"-42\0");
            assert_eq!(ultoa_s(255, b.as_mut_ptr(), 12, 16), 0);
            assert_eq!(&b[..3], b"ff\0");
            assert_eq!(mbscpy_s(b.as_mut_ptr(), 3, b"abc\0".as_ptr()), ERANGE);

            let (mut d, mut di, mut n, mut e) = ([0u8; 3], [0u8; 32], [0u8; 32], [0u8; 8]);
            let path = b"C:\\Python\\pythonwin.exe\0";
            assert_eq!(
                splitpath_s(
                    path.as_ptr(),
                    d.as_mut_ptr(),
                    3,
                    di.as_mut_ptr(),
                    32,
                    n.as_mut_ptr(),
                    32,
                    e.as_mut_ptr(),
                    8
                ),
                0
            );
            assert_eq!(&d, b"C:\0");
            assert_eq!(&di[..9], b"\\Python\\\0");
            assert_eq!(&n[..10], b"pythonwin\0");
            assert_eq!(&e[..5], b".exe\0");
            let mut out = [0u8; 32];
            assert_eq!(
                makepath_s(
                    out.as_mut_ptr(),
                    32,
                    b"C\0".as_ptr(),
                    b"\\Python\0".as_ptr(),
                    b"python\0".as_ptr(),
                    b"exe\0".as_ptr()
                ),
                0
            );
            assert_eq!(&out[..21], b"C:\\Python\\python.exe\0");
            assert_eq!(
                makepath_s(
                    out.as_mut_ptr(),
                    4,
                    std::ptr::null(),
                    std::ptr::null(),
                    b"python\0".as_ptr(),
                    std::ptr::null()
                ),
                ERANGE
            );
        }
    }

    #[test]
    fn complexes() {
        let z = C64::new(1.0, 2.0);
        let close =
            |a: C64, b: (f64, f64)| (a.re - b.0).abs() < 1e-12 && (a.im - b.1).abs() < 1e-12;
        // Valeurs de référence : identités analytiques.
        assert!(close(
            complex_op(COp::Exp, z),
            (1f64.exp() * 2f64.cos(), 1f64.exp() * 2f64.sin())
        ));
        assert!(close(
            complex_op(COp::Cos, z),
            (1f64.cos() * 2f64.cosh(), -(1f64.sin() * 2f64.sinh()))
        ));
        assert!(close(
            complex_op(COp::Sin, z),
            (1f64.sin() * 2f64.cosh(), 1f64.cos() * 2f64.sinh())
        ));
        let t = complex_op(COp::Tan, z);
        let q = complex_op(COp::Sin, z).div(complex_op(COp::Cos, z));
        assert!(close(t, (q.re, q.im)));
        assert!(close(
            complex_pow(C64::new(0.0, 1.0), C64::new(2.0, 0.0)),
            (-1.0, 0.0)
        ));
        // ABI x64 : _Dcomplex par pointeur caché, _Fcomplex dans un registre.
        let mut out = [0f64; 2];
        let arg = [0.0, std::f64::consts::PI];
        unsafe { cexp(out.as_mut_ptr(), arg.as_ptr()) };
        assert!((out[0] + 1.0).abs() < 1e-12 && out[1].abs() < 1e-12);
        let f = f_join(C64::new(3.0, 4.0));
        assert_eq!(cabsf(f), 5.0);
        assert_eq!((crealf(f), cimagf(f)), (3.0, 4.0));
    }

    #[test]
    fn classes_et_comparaisons() {
        assert_eq!(dpcomp(1.0, 2.0), 1);
        assert_eq!(dpcomp(2.0, 2.0), 2);
        assert_eq!(dpcomp(f64::NAN, 2.0), 0);
        assert_eq!(fpclass(-0.0), 0x20);
        assert_eq!(fpclass(f64::INFINITY), 0x200);
        assert_eq!(fpclass(f64::NAN), 2);
        assert_eq!(ldclass(1.0), -1);
        assert_eq!(fe_from_linux(fe_to_linux(0x1F)), 0x1F);
    }

    #[test]
    fn conversions_s() {
        let mut b = [0u8; 12];
        unsafe {
            assert_eq!(itoa_s(-42, b.as_mut_ptr(), 8, 10), 0);
            assert_eq!(&b[..4], b"-42\0");
            assert_eq!(itoa_s(-1, b.as_mut_ptr(), 9, 16), 0);
            assert_eq!(&b[..8], b"ffffffff");
            assert_eq!(itoa_s(123456, b.as_mut_ptr(), 4, 10), ERANGE);
            assert_eq!(b[0], 0);
            let mut w = [0u16; 12];
            assert_eq!(ultow_s(255, w.as_mut_ptr(), 8, 2), ERANGE);
            assert_eq!(ultow_s(255, w.as_mut_ptr(), 9, 2), 0);
            assert_eq!(String::from_utf16_lossy(&w[..8]), "11111111");
        }
        let (d, e, neg) = ecvt_parts(-1234.5, 3);
        assert_eq!((d.as_str(), e, neg), ("123", 4, true));
    }

    #[test]
    fn chemins() {
        let w: Vec<u16> = "C:\\dossier\\sous\\nom.tar.gz".encode_utf16().collect();
        let (d, di, n, e) = split_path(&w);
        let s = |v: Vec<u16>| String::from_utf16_lossy(&v);
        assert_eq!(
            (s(d), s(di), s(n), s(e)),
            (
                "C:".into(),
                "\\dossier\\sous\\".into(),
                "nom.tar".into(),
                ".gz".into()
            )
        );
        let mut buf = [0u16; 64];
        let p = |t: &str| t.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let (dr, dir, nm, ex) = (p("D"), p("a\\b"), p("f"), p("txt"));
        unsafe {
            assert_eq!(
                wmakepath_s(
                    buf.as_mut_ptr(),
                    64,
                    dr.as_ptr(),
                    dir.as_ptr(),
                    nm.as_ptr(),
                    ex.as_ptr()
                ),
                0
            );
        }
        let n = buf.iter().position(|&c| c == 0).unwrap();
        assert_eq!(String::from_utf16_lossy(&buf[..n]), "D:a\\b\\f.txt");
    }

    #[test]
    fn noms_de_types() {
        assert_eq!(
            undecorate_type(".?AVbad_alloc@std@@"),
            "class std::bad_alloc"
        );
        assert_eq!(undecorate_type(".?AUPoint@@"), "struct Point");
        assert_eq!(undecorate_type(".H"), "int");
        assert_eq!(
            undecorate_type(".?AV?$vector@HV?$allocator@H@std@@@std@@"),
            "class std::vector<int,class std::allocator<int> >"
        );
        // __std_type_info_name : nom mis en cache dans data.
        let mut data = vec![0u8; 8];
        data.extend_from_slice(b".?AVexception@std@@\0");
        let p = unsafe { std_type_info_name(data.as_mut_ptr(), 0) };
        assert_eq!(unsafe { super::super::cstr(p) }, "class std::exception");
        assert_eq!(
            usize::from_le_bytes(data[..8].try_into().unwrap()),
            p as usize
        );
        unsafe { libc::free(p as *mut c_void) };
    }

    #[test]
    fn tas_aligne_avec_decalage() {
        unsafe {
            let p = aligned_offset_malloc(100, 64, 8) as usize;
            assert_eq!((p + 8) % 64, 0);
            assert!(take_offset_block(p).is_some());
        }
    }
}
