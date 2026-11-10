//! CRT des programmes x86 (#232, étape 3).
//!
//! La plupart des fonctions de msvcrt/ucrtbase passent telles quelles par
//! les passerelles génériques. Restent celles que la signature Rust ne
//! décrit pas assez :
//!
//! - **Données exportées** (`__argv`, `_iob`, `_environ`, `_pctype`…) : des
//!   tableaux de pointeurs 64 bits, au-dessus de 4 Go. Le programme x86
//!   reçoit une copie basse, en pointeurs de 4 octets.
//! - **Fonctions variadiques** (`printf`, `sscanf`…) et leurs variantes à
//!   `va_list` : les arguments x86 (4 octets, doubles de 8) sont recopiés en
//!   cases de 8 octets d'après le format, puis la version 64 bits est appelée.
//! - **Rappels** (`_initterm`, `atexit`, `qsort`, `signal`) : appelés par
//!   `call32`.
//! - **`setjmp`/`longjmp`** : tampon x86 et reprise par un CONTEXT 32 bits.
//! - **Pointeurs vers des données du runtime** (`localeconv`, `setlocale`,
//!   `localtime`…) : copiés sous 4 Go (`Fix`).
//! - **`time_t` de 32 bits** de msvcrt.dll en x86 : `time`, `localtime`,
//!   `mktime`… vont vers `_time32`, `_localtime32`…

use super::{call32, limit, rd, wr};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Correction du pointeur rendu par une fonction 64 bits.
#[derive(Clone, Copy)]
pub(super) enum Fix {
    None,
    /// Chaîne (copie basse, une par contenu distinct).
    Str {
        wide: bool,
    },
    /// Tampon par thread (struct tm, chaîne d'asctime) : recopié dans le
    /// TEB 32 bits au décalage `off`.
    Scratch {
        off: usize,
        len: usize,
    },
    /// Donnée immuable de taille fixe : copie basse, une par adresse.
    Static {
        len: usize,
    },
}

pub(super) enum Override {
    /// Adresse (basse) d'une donnée exportée.
    Data(usize),
    /// Fonction 64 bits ordinaire avec sa signature et sa correction.
    Native {
        target: usize,
        sig: &'static str,
        fix: Fix,
    },
    /// `target(args_x86, aux)` lit lui-même la pile x86 (convention cdecl,
    /// sauf `pop`).
    Raw {
        target: usize,
        aux: usize,
        pop: Option<u16>,
    },
}

// ---------------------------------------------------------------------
// Données basses
// ---------------------------------------------------------------------

const D_ARGC: usize = 0x00;
const D_ARGV: usize = 0x04;
const D_WARGV: usize = 0x08;
const D_ENVIRON: usize = 0x0c;
const D_WENVIRON: usize = 0x10;
const D_INITENV: usize = 0x14;
const D_WINITENV: usize = 0x18;
const D_FMODE: usize = 0x1c;
const D_COMMODE: usize = 0x20;
const D_MB_CUR_MAX: usize = 0x24;
const D_ACMDLN: usize = 0x28;
const D_WCMDLN: usize = 0x2c;
const D_PCTYPE_PTR: usize = 0x30;
const D_PWCTYPE_PTR: usize = 0x34;
/// `_adjust_fdiv` : 0, aucun Pentium à FDIV défectueux.
const D_ADJUST_FDIV: usize = 0x38;
/// Trois `FILE` de msvcrt x86 (32 octets chacun) : stdin, stdout, stderr.
const D_IOB: usize = 0x40;
const FILE32_SIZE: usize = 32;
/// Table des classes de caractères, précédée d'une case pour EOF (-1).
const D_PCTYPE: usize = 0x100;
const D_PWCTYPE: usize = 0x400;
/// `struct lconv` x86 : 10 pointeurs, 8 octets, 8 pointeurs larges.
const D_LCONV: usize = 0x700;
const D_STRINGS: usize = 0x780;
const DATA_SIZE: usize = 0x1000;

/// Champs du tampon par thread, après le TEB 32 bits (voir `TEB32_SCRATCH`).
pub(super) const S_ERRNO: usize = 0x00;
pub(super) const S_DOSERRNO: usize = 0x04;
const S_TM: usize = 0x10;
const S_ASCTIME: usize = 0x40;

fn data() -> usize {
    static D: OnceLock<usize> = OnceLock::new();
    *D.get_or_init(|| {
        let d = super::alloc_low(DATA_SIZE, false);
        unsafe {
            let w32 = |o: usize, v: u32| *((d + o) as *mut u32) = v;
            w32(D_FMODE, 0x4000);
            w32(D_MB_CUR_MAX, 1);
            // Flux standard : alias des trois flux du runtime.
            crate::dlls::crt_stdio::register_std_aliases(d + D_IOB, FILE32_SIZE);
            // Classes de caractères : table 64 bits recopiée, EOF à zéro.
            let t = crate::dlls::crt_runtime::pctype_func();
            std::ptr::copy_nonoverlapping(t, (d + D_PCTYPE + 2) as *mut u16, 256);
            w32(D_PCTYPE_PTR, (d + D_PCTYPE + 2) as u32);
            std::ptr::copy_nonoverlapping(t, (d + D_PWCTYPE + 2) as *mut u16, 256);
            w32(D_PWCTYPE_PTR, (d + D_PWCTYPE + 2) as u32);
            // localeconv : « . » et chaînes vides, comme la locale C.
            let s = d + D_STRINGS;
            *(s as *mut [u8; 4]) = *b".\0\0\0";
            *((s + 4) as *mut [u16; 2]) = [b'.' as u16, 0];
            *((s + 8) as *mut [u16; 2]) = [0, 0];
            for i in 0..10 {
                w32(D_LCONV + i * 4, if i == 0 { s } else { s + 2 } as u32);
            }
            for i in 0..8 {
                *((d + D_LCONV + 40 + i) as *mut u8) = 127;
                w32(
                    D_LCONV + 48 + i * 4,
                    if i == 0 { s + 4 } else { s + 8 } as u32,
                );
            }
        }
        d
    })
}

/// Copie basse d'une chaîne terminée par zéro (étroite ou large).
fn low_str(p: usize, wide: bool) -> u32 {
    if p == 0 {
        return 0;
    }
    let unit = if wide { 2 } else { 1 };
    let mut len = 0usize;
    unsafe {
        while if wide {
            *((p + len * 2) as *const u16) != 0
        } else {
            *((p + len) as *const u8) != 0
        } {
            len += 1;
        }
        let bytes = std::slice::from_raw_parts(p as *const u8, (len + 1) * unit).to_vec();
        if p + bytes.len() <= limit() {
            return p as u32;
        }
        static CACHE: Mutex<Option<HashMap<Vec<u8>, u32>>> = Mutex::new(None);
        let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        let c = c.get_or_insert_with(HashMap::new);
        if let Some(&a) = c.get(&bytes) {
            return a;
        }
        let at = libc::malloc(bytes.len()) as usize;
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), at as *mut u8, bytes.len());
        c.insert(bytes, at as u32);
        at as u32
    }
}

/// Tableau 32 bits (terminé par zéro) d'après un tableau de pointeurs 64 bits.
fn low_ptr_array(arr: *const usize, wide: bool) -> u32 {
    if arr.is_null() {
        return 0;
    }
    unsafe {
        let mut n = 0;
        while *arr.add(n) != 0 {
            n += 1;
        }
        let out = libc::calloc(n + 1, 4) as *mut u32;
        for i in 0..n {
            *out.add(i) = low_str(*arr.add(i), wide);
        }
        out as u32
    }
}

/// Remplit argc/argv/environ/ligne de commande, une fois le processus
/// décrit (appelé par `process::init_process` et `__getmainargs`).
pub fn fill_process_data() {
    use crate::dlls::{crt_extra, crt_runtime as rt};
    let d = data();
    unsafe {
        let w32 = |o: usize, v: u32| *((d + o) as *mut u32) = v;
        w32(D_ARGC, *rt::p_argc() as u32);
        w32(D_ARGV, low_ptr_array(*rt::p_argv() as *const usize, false));
        w32(D_WARGV, low_ptr_array(*rt::p_wargv() as *const usize, true));
        let env = low_ptr_array(*rt::p_environ() as *const usize, false);
        let wenv = low_ptr_array(*rt::p_wenviron() as *const usize, true);
        w32(D_ENVIRON, env);
        w32(D_WENVIRON, wenv);
        w32(D_INITENV, env);
        w32(D_WINITENV, wenv);
        w32(D_ACMDLN, low_str(*crt_extra::p_acmdln(), false));
        w32(D_WCMDLN, low_str(*crt_extra::p_wcmdln(), true));
    }
}

/// Après `_putenv` : `_environ` et `_wenviron` x86 suivent les tableaux
/// 64 bits reconstruits (#1474). `__initenv` garde l'instantané du début.
pub fn refresh_environ() {
    use crate::dlls::crt_runtime as rt;
    let d = data();
    unsafe {
        let env = low_ptr_array(*rt::p_environ() as *const usize, false);
        let wenv = low_ptr_array(*rt::p_wenviron() as *const usize, true);
        *((d + D_ENVIRON) as *mut u32) = env;
        *((d + D_WENVIRON) as *mut u32) = wenv;
    }
}

/// Copie basse d'une donnée scalaire exportée (instantané).
fn scalar_copy(dll: &str, name: &str, len: usize) -> Option<usize> {
    let a = crate::dlls::lookup(dll, name)?;
    static CACHE: Mutex<Option<HashMap<usize, usize>>> = Mutex::new(None);
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let c = c.get_or_insert_with(HashMap::new);
    Some(*c.entry(a).or_insert_with(|| unsafe {
        let at = libc::calloc(1, len.max(8)) as usize;
        std::ptr::copy_nonoverlapping(a as *const u8, at as *mut u8, len);
        at
    }))
}

/// Applique `fix` au résultat d'une fonction 64 bits.
pub(super) fn apply_fix(fix: Fix, r: u64) -> u64 {
    let p = r as usize;
    match fix {
        Fix::None => r,
        _ if p == 0 => 0,
        Fix::Str { wide } => low_str(p, wide) as u64,
        Fix::Scratch { off, len } => {
            let s = super::scratch() + off;
            unsafe { std::ptr::copy_nonoverlapping(p as *const u8, s as *mut u8, len) };
            s as u64
        }
        Fix::Static { len } => {
            if p + len <= limit() {
                return r;
            }
            static CACHE: Mutex<Option<HashMap<usize, usize>>> = Mutex::new(None);
            let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
            let c = c.get_or_insert_with(HashMap::new);
            *c.entry(p).or_insert_with(|| unsafe {
                let at = libc::malloc(len) as usize;
                std::ptr::copy_nonoverlapping(p as *const u8, at as *mut u8, len);
                at
            }) as u64
        }
    }
}

// ---------------------------------------------------------------------
// Formats printf/scanf
// ---------------------------------------------------------------------

struct Fmt {
    target: usize,
    /// Arguments fixes, format compris : `i` (4 octets) ou `q` (8 octets).
    fixed: &'static [u8],
    /// Rang du format dans `fixed`.
    fmt: usize,
    wide: bool,
    scan: bool,
    /// Un `va_list` suit les arguments fixes (sinon `...`).
    va: bool,
}

/// Valeur x86 de 4 octets en case 64 bits : signe gardé pour -1, -2…
fn ext(v: u32) -> u64 {
    if v >= 0xffff_0000 {
        v as i32 as i64 as u64
    } else {
        v as u64
    }
}

fn read_format(p: u32, wide: bool) -> Vec<u32> {
    let mut out = Vec::new();
    if p == 0 {
        return out;
    }
    let mut a = p as usize;
    unsafe {
        while out.len() < 1 << 16 {
            let c = if wide {
                *(a as *const u16) as u32
            } else {
                *(a as *const u8) as u32
            };
            if c == 0 {
                break;
            }
            out.push(c);
            a += if wide { 2 } else { 1 };
        }
    }
    out
}

/// Recopie les arguments x86 lus à `ap` en cases de 8 octets, d'après le
/// format (taille de chaque conversion). `secure` : scanf « _s », où %c,
/// %s et %[ sont suivis de la taille du tampon (#1508).
fn convert_args(fmt: &[u32], mut ap: u32, scan: bool, secure: bool) -> Vec<u64> {
    let mut out = Vec::new();
    let mut next = || {
        let v = rd(ap);
        ap += 4;
        v
    };
    let at = |i: usize| fmt.get(i).copied().and_then(char::from_u32).unwrap_or('\0');
    let mut i = 0;
    while i < fmt.len() {
        if at(i) != '%' {
            i += 1;
            continue;
        }
        i += 1;
        if at(i) == '%' {
            i += 1;
            continue;
        }
        if scan {
            let suppressed = at(i) == '*';
            while at(i) == '*' || at(i).is_ascii_digit() {
                i += 1;
            }
            while matches!(
                at(i),
                'h' | 'l' | 'L' | 'I' | '3' | '2' | '6' | '4' | 'j' | 'z' | 't' | 'w'
            ) {
                i += 1;
            }
            if at(i) == '[' {
                i += 1;
                if at(i) == '^' {
                    i += 1;
                }
                if at(i) == ']' {
                    i += 1;
                }
                while i < fmt.len() && at(i) != ']' {
                    i += 1;
                }
            }
            let conv = at(i);
            i += 1;
            if !suppressed {
                out.push(next() as u64);
                if secure && matches!(conv, 'c' | 'C' | 's' | 'S' | ']') {
                    out.push(next() as u64);
                }
            }
            continue;
        }
        while matches!(at(i), '-' | '+' | ' ' | '#' | '0' | '\'') {
            i += 1;
        }
        if at(i) == '*' {
            out.push(next() as i32 as i64 as u64);
            i += 1;
        }
        while at(i).is_ascii_digit() {
            i += 1;
        }
        if at(i) == '.' {
            i += 1;
            if at(i) == '*' {
                out.push(next() as i32 as i64 as u64);
                i += 1;
            }
            while at(i).is_ascii_digit() {
                i += 1;
            }
        }
        let mut big = false;
        loop {
            match at(i) {
                'l' if at(i + 1) == 'l' => {
                    big = true;
                    i += 2;
                }
                'I' if at(i + 1) == '6' && at(i + 2) == '4' => {
                    big = true;
                    i += 3;
                }
                'I' if at(i + 1) == '3' && at(i + 2) == '2' => i += 3,
                'j' | 'q' => {
                    big = true;
                    i += 1;
                }
                'h' | 'l' | 'L' | 'I' | 'z' | 't' | 'w' => i += 1,
                _ => break,
            }
        }
        let conv = at(i);
        i += 1;
        match conv {
            'd' | 'i' if !big => out.push(next() as i32 as i64 as u64),
            'd' | 'i' | 'u' | 'o' | 'x' | 'X' | 'b' if big => {
                let lo = next() as u64;
                out.push(lo | (next() as u64) << 32);
            }
            'u' | 'o' | 'x' | 'X' | 'b' | 'c' | 'C' | 'p' | 's' | 'S' | 'n' | 'Z' => {
                out.push(next() as u64)
            }
            'e' | 'E' | 'f' | 'F' | 'g' | 'G' | 'a' | 'A' => {
                let lo = next() as u64;
                out.push(lo | (next() as u64) << 32);
            }
            _ => {}
        }
    }
    out
}

extern "win64" fn fmt_call(args: u32, aux: usize) -> u64 {
    let s = unsafe { &*(aux as *const Fmt) };
    let mut slots: Vec<u64> = Vec::with_capacity(16);
    let mut p = args;
    let mut fmt_ptr = 0;
    for (i, k) in s.fixed.iter().enumerate() {
        if *k == b'q' {
            slots.push(rd(p) as u64 | (rd(p + 4) as u64) << 32);
            p += 8;
        } else {
            let v = rd(p);
            if i == s.fmt {
                fmt_ptr = v;
            }
            slots.push(ext(v));
            p += 4;
        }
    }
    let fmt = read_format(fmt_ptr, s.wide);
    let src = if s.va { rd(p) } else { p };
    // __stdio_common_v*scanf : bit 0 des options = variante « _s ».
    let secure = s.scan && s.fixed.first() == Some(&b'q') && slots[0] & 1 != 0;
    let mut conv = convert_args(&fmt, src, s.scan, secure);
    if s.va {
        // Quelques cases de plus : une lecture au-delà du format reste
        // dans le tableau.
        conv.extend_from_slice(&[0; 4]);
        slots.push(conv.as_ptr() as u64);
    } else {
        slots.extend_from_slice(&conv);
    }
    let n = slots.len();
    while slots.len() < 4 {
        slots.push(0);
    }
    let mut xmm0 = 0u64;
    let r = unsafe { super::winlux_wow_call64(s.target, slots.as_ptr(), n, &mut xmm0) };
    drop(conv);
    r
}

/// (nom, arguments fixes, rang du format, large, scanf, va_list, stdcall)
type FmtRow = (&'static str, &'static [u8], usize, bool, bool, bool, bool);

const FORMATS: &[FmtRow] = &[
    ("printf", b"i", 0, false, false, false, false),
    ("wprintf", b"i", 0, true, false, false, false),
    ("fprintf", b"ii", 1, false, false, false, false),
    ("fwprintf", b"ii", 1, true, false, false, false),
    ("sprintf", b"ii", 1, false, false, false, false),
    ("swprintf", b"ii", 1, true, false, false, false),
    ("_swprintf", b"ii", 1, true, false, false, false),
    ("_snprintf", b"iii", 2, false, false, false, false),
    ("snprintf", b"iii", 2, false, false, false, false),
    ("_snwprintf", b"iii", 2, true, false, false, false),
    ("_scprintf", b"i", 0, false, false, false, false),
    ("_scwprintf", b"i", 0, true, false, false, false),
    ("sprintf_s", b"iii", 2, false, false, false, false),
    ("swprintf_s", b"iii", 2, true, false, false, false),
    ("_snprintf_s", b"iiii", 3, false, false, false, false),
    ("_snwprintf_s", b"iiii", 3, true, false, false, false),
    ("wsprintfA", b"ii", 1, false, false, false, false),
    ("wsprintfW", b"ii", 1, true, false, false, false),
    ("vprintf", b"i", 0, false, false, true, false),
    ("vwprintf", b"i", 0, true, false, true, false),
    ("vfprintf", b"ii", 1, false, false, true, false),
    ("vfwprintf", b"ii", 1, true, false, true, false),
    ("vsprintf", b"ii", 1, false, false, true, false),
    ("vswprintf", b"ii", 1, true, false, true, false),
    ("_vswprintf", b"ii", 1, true, false, true, false),
    ("_vsnprintf", b"iii", 2, false, false, true, false),
    ("vsnprintf", b"iii", 2, false, false, true, false),
    ("_vsnwprintf", b"iii", 2, true, false, true, false),
    ("_vscprintf", b"i", 0, false, false, true, false),
    ("_vscwprintf", b"i", 0, true, false, true, false),
    ("vsprintf_s", b"iii", 2, false, false, true, false),
    ("vswprintf_s", b"iii", 2, true, false, true, false),
    ("_vsnprintf_s", b"iiii", 3, false, false, true, false),
    ("_vsnwprintf_s", b"iiii", 3, true, false, true, false),
    ("wvsprintfA", b"ii", 1, false, false, true, true),
    ("wvsprintfW", b"ii", 1, true, false, true, true),
    ("scanf", b"i", 0, false, true, false, false),
    ("wscanf", b"i", 0, true, true, false, false),
    ("fscanf", b"ii", 1, false, true, false, false),
    ("fwscanf", b"ii", 1, true, true, false, false),
    ("sscanf", b"ii", 1, false, true, false, false),
    ("swscanf", b"ii", 1, true, true, false, false),
    ("vsscanf", b"ii", 1, false, true, true, false),
    ("vswscanf", b"ii", 1, true, true, true, false),
    ("vfscanf", b"ii", 1, false, true, true, false),
    ("vscanf", b"i", 0, false, true, true, false),
    // CRT universelle : options (64 bits) d'abord, locale après le format.
    (
        "__stdio_common_vfprintf",
        b"qiii",
        2,
        false,
        false,
        true,
        false,
    ),
    (
        "__stdio_common_vfwprintf",
        b"qiii",
        2,
        true,
        false,
        true,
        false,
    ),
    (
        "__stdio_common_vfprintf_s",
        b"qiii",
        2,
        false,
        false,
        true,
        false,
    ),
    (
        "__stdio_common_vfwprintf_s",
        b"qiii",
        2,
        true,
        false,
        true,
        false,
    ),
    (
        "__stdio_common_vsprintf",
        b"qiiii",
        3,
        false,
        false,
        true,
        false,
    ),
    (
        "__stdio_common_vswprintf",
        b"qiiii",
        3,
        true,
        false,
        true,
        false,
    ),
    (
        "__stdio_common_vsprintf_s",
        b"qiiii",
        3,
        false,
        false,
        true,
        false,
    ),
    (
        "__stdio_common_vswprintf_s",
        b"qiiii",
        3,
        true,
        false,
        true,
        false,
    ),
    (
        "__stdio_common_vsnprintf_s",
        b"qiiiii",
        4,
        false,
        false,
        true,
        false,
    ),
    (
        "__stdio_common_vsnwprintf_s",
        b"qiiiii",
        4,
        true,
        false,
        true,
        false,
    ),
    (
        "__stdio_common_vsscanf",
        b"qiiii",
        3,
        false,
        true,
        true,
        false,
    ),
    (
        "__stdio_common_vswscanf",
        b"qiiii",
        3,
        true,
        true,
        true,
        false,
    ),
    (
        "__stdio_common_vfscanf",
        b"qiii",
        2,
        false,
        true,
        true,
        false,
    ),
    (
        "__stdio_common_vfwscanf",
        b"qiii",
        2,
        true,
        true,
        true,
        false,
    ),
];

fn fmt_override(dll: &str, func: &str) -> Option<Override> {
    let row = FORMATS.iter().find(|r| r.0 == func)?;
    let target = crate::dlls::lookup(dll, func)?;
    let (_, fixed, fmt, wide, scan, va, stdcall) = *row;
    let spec = Box::leak(Box::new(Fmt {
        target,
        fixed,
        fmt,
        wide,
        scan,
        va,
    }));
    let slots: usize = fixed
        .iter()
        .map(|k| if *k == b'q' { 2 } else { 1 })
        .sum::<usize>()
        + usize::from(va);
    Some(Override::Raw {
        target: fmt_call as *const () as usize,
        aux: spec as *const Fmt as usize,
        pop: stdcall.then_some((slots * 4) as u16),
    })
}

// ---------------------------------------------------------------------
// Démarrage et fin de programme
// ---------------------------------------------------------------------

extern "win64" fn ret_aux(_args: u32, aux: usize) -> u64 {
    aux as u64
}

/// Copie basse des variables de fuseau horaire (`_timezone`, `_daylight`,
/// `_dstbias`, `_tzname`), relue à chaque appel de `__timezone()`… : la
/// version 64 bits rend l'adresse d'une donnée de l'image, au-dessus de
/// 4 Go (python312.dll x86, #1407). `aux` : 0 à 2 pour les entiers,
/// 3 pour `_tzname` (deux pointeurs de chaîne).
fn tz_low(which: usize) -> usize {
    const NAMES: [&str; 4] = ["__timezone", "__daylight", "__dstbias", "__tzname"];
    static LOW: OnceLock<usize> = OnceLock::new();
    // 4 entiers de 4 octets, puis _tzname (deux pointeurs x86).
    let base = *LOW.get_or_init(|| unsafe { libc::calloc(1, 32) as usize });
    let Some(f) = crate::dlls::lookup("ucrtbase", NAMES[which]) else {
        return 0;
    };
    // SAFETY : fonction 64 bits sans argument qui rend un pointeur.
    let f: extern "win64" fn() -> usize = unsafe { std::mem::transmute(f) };
    let p = f();
    if p == 0 || base == 0 {
        return 0;
    }
    unsafe {
        if which < 3 {
            let at = base + which * 4;
            std::ptr::write_unaligned(at as *mut i32, std::ptr::read_unaligned(p as *const i32));
            at
        } else {
            let at = base + 16;
            for i in 0..2 {
                let s = std::ptr::read_unaligned((p + i * 8) as *const usize);
                std::ptr::write_unaligned((at + i * 4) as *mut u32, low_str(s, false));
            }
            at
        }
    }
}

/// Copie basse de `_sys_errlist` : tableau de pointeurs x86 vers des
/// copies basses des messages (#1437).
fn errlist_low(dll: &str) -> Option<usize> {
    static LOW: OnceLock<usize> = OnceLock::new();
    let nerr = crate::dlls::lookup(dll, "_sys_nerr")?;
    let list = crate::dlls::lookup(dll, "_sys_errlist")?;
    // `__sys_errlist()` remplit les tables au premier appel.
    if let Some(f) = crate::dlls::lookup(dll, "__sys_errlist") {
        // SAFETY : fonction 64 bits sans argument.
        let f: extern "win64" fn() -> usize = unsafe { std::mem::transmute(f) };
        f();
    }
    Some(*LOW.get_or_init(|| unsafe {
        let n = std::ptr::read_unaligned(nerr as *const i32).clamp(0, 1024) as usize;
        let at = libc::calloc(n.max(1), 4) as usize;
        for i in 0..n {
            let s = std::ptr::read_unaligned((list + i * 8) as *const usize);
            std::ptr::write_unaligned((at + i * 4) as *mut u32, low_str(s, false));
        }
        at
    }))
}

extern "win64" fn ret_tz(_args: u32, aux: usize) -> u64 {
    tz_low(aux) as u64
}

extern "win64" fn ret_deref(_args: u32, aux: usize) -> u64 {
    rd(aux as u32) as u64
}

extern "win64" fn iob_at(args: u32, _aux: usize) -> u64 {
    let i = rd(args) as usize;
    if i < 3 {
        (data() + D_IOB + i * FILE32_SIZE) as u64
    } else {
        0
    }
}

/// `__getmainargs(&argc, &argv, &env, glob, startinfo)` (et `__wgetmainargs`).
extern "win64" fn getmainargs(args: u32, wide: usize) -> u64 {
    fill_process_data();
    let d = data() as u32;
    let (argv, env) = if wide != 0 {
        (D_WARGV, D_WENVIRON)
    } else {
        (D_ARGV, D_ENVIRON)
    };
    let put = |slot: u32, v: u32| {
        if slot != 0 {
            wr(slot, v)
        }
    };
    put(rd(args), rd(d + D_ARGC as u32));
    put(rd(args + 4), rd(d + argv as u32));
    put(rd(args + 8), rd(d + env as u32));
    0
}

/// `_initterm(début, fin)` / `_initterm_e` : pointeurs x86 de 4 octets.
extern "win64" fn initterm(args: u32, stop_on_error: usize) -> u64 {
    let (mut p, end) = (rd(args), rd(args + 4));
    while p < end {
        let f = rd(p);
        if f != 0 {
            let r = call32(f as usize, &[]) as u32;
            if stop_on_error != 0 && r != 0 {
                return r as u64;
            }
        }
        p += 4;
    }
    0
}

extern "win64" fn atexit_trampoline(f: usize) {
    call32(f, &[]);
}

/// `__dllonexit(f, &début, &fin)` x86 : table propre à la DLL, qu'elle
/// parcourt à son DLL_PROCESS_DETACH, comme en 64 bits (#1479).
extern "win64" fn dllonexit32(args: u32, _aux: usize) -> u64 {
    let (f, pb, pe) = (rd(args), rd(args + 4), rd(args + 8));
    if f == 0 || pb == 0 || pe == 0 {
        return 0;
    }
    let (b, e) = (rd(pb), rd(pe));
    let len = e.wrapping_sub(b);
    if b == 0 || e < b || len % 4 != 0 || len > 1 << 22 {
        crate::log_warn!("crt", "__dllonexit (x86) : table {b:#x}..{e:#x} invalide");
        return atexit32(args, 1);
    }
    let count = len / 4;
    let cap = unsafe { libc::malloc_usable_size(b as usize as *mut libc::c_void) } as u32 / 4;
    let mut table = b;
    if count >= cap {
        let grown = unsafe {
            libc::realloc(
                b as usize as *mut libc::c_void,
                (count + count.max(16)) as usize * 4,
            )
        } as usize;
        if grown == 0 || grown > u32::MAX as usize {
            return 0;
        }
        table = grown as u32;
    }
    wr(table + count * 4, f);
    wr(pb, table);
    wr(pe, table + (count + 1) * 4);
    f as u64
}

/// `atexit(f)`, `_onexit(f)`, `_crt_atexit(f)` : la fonction x86 rejoint la
/// liste commune, exécutée par `exit`/`_cexit`.
extern "win64" fn atexit32(args: u32, returns_fn: usize) -> u64 {
    let f = rd(args);
    let r = crate::dlls::crt_runtime::register_atexit(
        atexit_trampoline as *const () as usize,
        f as usize,
    );
    match (returns_fn != 0, r == 0) {
        (true, true) => f as u64,
        (true, false) => 0,
        (false, _) => r as u32 as u64,
    }
}

extern "win64" fn ret_zero(_args: u32, _aux: usize) -> u64 {
    0
}

/// `_onexit_table_t` x86 : premier, dernier, fin (pointeurs de 4 octets).
extern "win64" fn onexit_table(args: u32, op: usize) -> u64 {
    let t = rd(args);
    if t == 0 {
        return u32::MAX as u64;
    }
    match op {
        // _initialize_onexit_table
        0 => {
            wr(t, 0);
            wr(t + 4, 0);
            wr(t + 8, 0);
        }
        // _register_onexit_function
        1 => {
            let f = rd(args + 4);
            let (first, last, end) = (rd(t), rd(t + 4), rd(t + 8));
            let (first, last) = if last == end {
                let len = if first == 0 { 0 } else { (last - first) / 4 };
                let cap = (len * 2).max(32);
                let p =
                    unsafe { libc::realloc(first as usize as *mut libc::c_void, cap as usize * 4) }
                        as usize as u32;
                if p == 0 {
                    return u32::MAX as u64;
                }
                wr(t + 8, p + cap * 4);
                (p, p + len * 4)
            } else {
                (first, last)
            };
            wr(last, f);
            wr(t, first);
            wr(t + 4, last + 4);
        }
        // _execute_onexit_table
        _ => {
            let (first, mut last) = (rd(t), rd(t + 4));
            wr(t, 0);
            wr(t + 4, 0);
            wr(t + 8, 0);
            while first != 0 && last > first {
                last -= 4;
                let f = rd(last);
                if f != 0 {
                    call32(f as usize, &[]);
                }
            }
            unsafe { libc::free(first as usize as *mut libc::c_void) };
        }
    }
    0
}

// ---------------------------------------------------------------------
// qsort, bsearch
// ---------------------------------------------------------------------

/// Tri fusion stable : un comparateur incohérent donne un ordre quelconque,
/// jamais une erreur.
fn merge_sort(idx: &mut [u32], tmp: &mut [u32], cmp: &mut impl FnMut(u32, u32) -> i32) {
    let n = idx.len();
    if n < 2 {
        return;
    }
    let mid = n / 2;
    {
        let (a, b) = idx.split_at_mut(mid);
        let (ta, tb) = tmp.split_at_mut(mid);
        merge_sort(a, ta, cmp);
        merge_sort(b, tb, cmp);
    }
    let (mut i, mut j, mut k) = (0, mid, 0);
    while i < mid && j < n {
        if cmp(idx[j], idx[i]) < 0 {
            tmp[k] = idx[j];
            j += 1;
        } else {
            tmp[k] = idx[i];
            i += 1;
        }
        k += 1;
    }
    tmp[k..k + mid - i].copy_from_slice(&idx[i..mid]);
    k += mid - i;
    tmp[k..k + n - j].copy_from_slice(&idx[j..n]);
    idx.copy_from_slice(&tmp[..n]);
}

/// `qsort(base, n, taille, cmp)`.
extern "win64" fn qsort32(args: u32, _aux: usize) -> u64 {
    let (base, n, size, cmp) = (rd(args), rd(args + 4), rd(args + 8), rd(args + 12));
    if n < 2 || size == 0 || cmp == 0 {
        return 0;
    }
    let mut idx: Vec<u32> = (0..n).collect();
    let mut tmp = vec![0u32; n as usize];
    let elem = |i: u32| base + i * size;
    merge_sort(&mut idx, &mut tmp, &mut |a, b| {
        call32(cmp as usize, &[elem(a), elem(b)]) as i32
    });
    let total = (n * size) as usize;
    let copy: Vec<u8> =
        unsafe { std::slice::from_raw_parts(base as usize as *const u8, total).to_vec() };
    for (dst, &src) in idx.iter().enumerate() {
        let s = src as usize * size as usize;
        let d = dst * size as usize;
        unsafe {
            std::ptr::copy_nonoverlapping(
                copy.as_ptr().add(s),
                (base as usize + d) as *mut u8,
                size as usize,
            );
        }
    }
    0
}

/// `bsearch(clé, base, n, taille, cmp)`.
extern "win64" fn bsearch32(args: u32, _aux: usize) -> u64 {
    let (key, base, n, size, cmp) = (
        rd(args),
        rd(args + 4),
        rd(args + 8),
        rd(args + 12),
        rd(args + 16),
    );
    let (mut lo, mut hi) = (0u32, n);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let e = base + mid * size;
        let r = call32(cmp as usize, &[key, e]) as i32;
        match r {
            0 => return e as u64,
            r if r < 0 => hi = mid,
            _ => lo = mid + 1,
        }
    }
    0
}

// ---------------------------------------------------------------------
// signal
// ---------------------------------------------------------------------

fn handlers32() -> &'static Mutex<HashMap<i32, u32>> {
    static H: OnceLock<Mutex<HashMap<i32, u32>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(HashMap::new()))
}

extern "win64" fn signal_trampoline(sig: i32) {
    let h = handlers32()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&sig)
        .copied();
    if let Some(h) = h {
        call32(h as usize, &[sig as u32]);
    }
}

/// `signal(sig, gestionnaire)` : SIG_DFL (0), SIG_IGN (1) ou fonction x86.
extern "win64" fn signal32(args: u32, _aux: usize) -> u64 {
    let (sig, h) = (rd(args) as i32, rd(args + 4));
    let h64 = if h <= 1 || h == u32::MAX {
        ext(h) as usize
    } else {
        signal_trampoline as *const () as usize
    };
    let prev = crate::dlls::crt_runtime::signal(sig, h64);
    if prev == usize::MAX {
        return u32::MAX as u64;
    }
    let sig = if sig == 6 { 22 } else { sig };
    let mut m = handlers32().lock().unwrap_or_else(|e| e.into_inner());
    let old = if h <= 1 {
        m.remove(&sig)
    } else {
        m.insert(sig, h)
    };
    if prev == signal_trampoline as *const () as usize {
        old.unwrap_or(0) as u64
    } else {
        prev as u32 as u64
    }
}

// ---------------------------------------------------------------------
// setjmp / longjmp
// ---------------------------------------------------------------------

// _JUMP_BUFFER x86 : Ebp, Ebx, Edi, Esi, Esp, Eip, Registration, TryLevel,
// Cookie, UnwindFunc, UnwindData[6]. Les deux dernières cases d'UnwindData
// gardent la chaîne des appels 64 → 32 (propre à Winlux).
const JB_COOKIE: u32 = 0x5643_3230; // « VC20 »

/// `_setjmp3(tampon, n, …)` et `_setjmp(tampon)`.
extern "win64" fn setjmp32(args: u32, extended: usize) -> u64 {
    let r = super::caller_regs();
    let jb = rd(args);
    let reg = rd(super::teb32_addr() as u32);
    for (i, v) in [r.ebp, r.ebx, r.edi, r.esi, r.esp, r.eip, reg]
        .into_iter()
        .enumerate()
    {
        wr(jb + i as u32 * 4, v);
    }
    let (mut try_level, mut unwind) = (u32::MAX, 0);
    if extended != 0 {
        let n = rd(args + 4);
        if n > 0 {
            unwind = rd(args + 8);
        }
        if n > 1 {
            try_level = rd(args + 12);
        }
        for i in 2..n.min(6) {
            wr(jb + 40 + (i - 2) * 4, rd(args + 8 + i * 4));
        }
    }
    wr(jb + 28, try_level);
    wr(jb + 32, JB_COOKIE);
    wr(jb + 36, unwind);
    let chain = super::seh::chain_now();
    wr(jb + 56, chain as u32);
    wr(jb + 60, (chain >> 32) as u32);
    0
}

/// `longjmp(tampon, valeur)` : déroule les cadres SEH posés depuis, puis
/// reprend après `setjmp` avec `valeur` (1 si 0).
extern "win64" fn longjmp32(args: u32, _aux: usize) -> u64 {
    let (jb, v) = (rd(args), rd(args + 4));
    let reg = rd(jb + 24);
    super::seh::unwind_to_frame(reg);
    if rd(jb + 32) == JB_COOKIE && rd(jb + 36) != 0 {
        call32(rd(jb + 36) as usize, &[jb]);
    }
    let chain = rd(jb + 56) as u64 | (rd(jb + 60) as u64) << 32;
    let regs = super::seh::Regs32 {
        eip: rd(jb + 20),
        esp: rd(jb + 16),
        ebp: rd(jb),
        ebx: rd(jb + 4),
        edi: rd(jb + 8),
        esi: rd(jb + 12),
        eax: if v == 0 { 1 } else { v },
    };
    super::seh::resume_regs(&regs, chain)
}

// ---------------------------------------------------------------------
// Table des remplacements
// ---------------------------------------------------------------------

fn raw(f: extern "win64" fn(u32, usize) -> u64, aux: usize) -> Override {
    Override::Raw {
        target: f as *const () as usize,
        aux,
        pop: None,
    }
}

fn native(dll: &str, name64: &str, sig: &'static str, fix: Fix) -> Option<Override> {
    Some(Override::Native {
        target: crate::dlls::lookup(dll, name64)?,
        sig,
        fix,
    })
}

// ---------------------------------------------------------------------
// Tableaux de HANDLE (4 octets en x86, 8 en 64 bits)
// ---------------------------------------------------------------------

struct HandleArray {
    target: usize,
    nargs: usize,
    count: usize,
    array: usize,
}

extern "win64" fn handle_array_call(args: u32, aux: usize) -> u64 {
    let s = unsafe { &*(aux as *const HandleArray) };
    let mut slots: Vec<u64> = (0..s.nargs as u32).map(|i| ext(rd(args + i * 4))).collect();
    let n = (slots[s.count] as u32).min(64);
    let base = slots[s.array] as u32;
    let wide: Vec<u64> = (0..n).map(|i| ext(rd(base + i * 4))).collect();
    if base != 0 {
        slots[s.array] = wide.as_ptr() as u64;
    }
    let nargs = slots.len();
    while slots.len() < 4 {
        slots.push(0);
    }
    let mut xmm0 = 0u64;
    let r = unsafe { super::winlux_wow_call64(s.target, slots.as_ptr(), nargs, &mut xmm0) };
    drop(wide);
    r
}

/// (nom, nombre d'arguments, rang du nombre, rang du tableau)
const HANDLE_ARRAYS: &[(&str, usize, usize, usize)] = &[
    ("WaitForMultipleObjects", 4, 0, 1),
    ("WaitForMultipleObjectsEx", 5, 0, 1),
    ("MsgWaitForMultipleObjects", 5, 0, 1),
    ("MsgWaitForMultipleObjectsEx", 5, 0, 1),
    // WSAEVENT est un HANDLE : tableau de cases de 4 octets en x86 (#779).
    ("WSAWaitForMultipleEvents", 5, 0, 1),
];

fn handle_array_override(n: &str, func: &str) -> Option<Override> {
    let &(_, nargs, count, array) = HANDLE_ARRAYS.iter().find(|r| r.0 == func)?;
    let target = crate::dlls::lookup(n, func)?;
    let spec = Box::leak(Box::new(HandleArray {
        target,
        nargs,
        count,
        array,
    }));
    Some(Override::Raw {
        target: handle_array_call as *const () as usize,
        aux: spec as *const HandleArray as usize,
        pop: Some((nargs * 4) as u16),
    })
}

/// `_ftol` : ST0 → entier 64 bits dans edx:eax, par troncature (arrondi
/// vers zéro le temps de `fistp`, puis mot de contrôle rétabli).
fn ftol_code() -> usize {
    static CODE: OnceLock<usize> = OnceLock::new();
    *CODE.get_or_init(|| {
        super::alloc_code(&[
            0x83, 0xEC, 0x0C, // sub esp, 12
            0xD9, 0x3C, 0x24, // fnstcw [esp]
            0x0F, 0xB7, 0x04, 0x24, // movzx eax, word [esp]
            0x80, 0xCC, 0x0C, // or ah, 0x0C (arrondi vers zéro)
            0x66, 0x89, 0x44, 0x24, 0x02, // mov [esp+2], ax
            0xD9, 0x6C, 0x24, 0x02, // fldcw [esp+2]
            0xDF, 0x7C, 0x24, 0x04, // fistp qword [esp+4]
            0xD9, 0x2C, 0x24, // fldcw [esp]
            0x8B, 0x44, 0x24, 0x04, // mov eax, [esp+4]
            0x8B, 0x54, 0x24, 0x08, // mov edx, [esp+8]
            0x83, 0xC4, 0x0C, // add esp, 12
            0xC3, // ret
        ]) as usize
    })
}

// ---------------------------------------------------------------------
// Aides mathématiques du CRT x86 (#1333)
// ---------------------------------------------------------------------
//
// `_CIfmod`, `_CIatan2` : opérandes sur la pile x87 (le premier poussé en
// ST1, le second en ST0), résultat dans ST0. `_libm_sse2_*_precise` :
// opérandes dans xmm0 (et xmm1), résultat dans xmm0. Les calculs vont à
// la bibliothèque mathématique du runtime par une passerelle « d>d ».

extern "win64" fn m_sin(x: f64) -> f64 {
    x.sin()
}
extern "win64" fn m_cos(x: f64) -> f64 {
    x.cos()
}
extern "win64" fn m_sqrt(x: f64) -> f64 {
    x.sqrt()
}
extern "win64" fn m_asin(x: f64) -> f64 {
    x.asin()
}
extern "win64" fn m_acos(x: f64) -> f64 {
    x.acos()
}
extern "win64" fn m_atan(x: f64) -> f64 {
    x.atan()
}
extern "win64" fn m_tan(x: f64) -> f64 {
    x.tan()
}
extern "win64" fn m_exp(x: f64) -> f64 {
    x.exp()
}
extern "win64" fn m_log(x: f64) -> f64 {
    x.ln()
}
extern "win64" fn m_log10(x: f64) -> f64 {
    x.log10()
}
extern "win64" fn m_sinh(x: f64) -> f64 {
    x.sinh()
}
extern "win64" fn m_cosh(x: f64) -> f64 {
    x.cosh()
}
extern "win64" fn m_tanh(x: f64) -> f64 {
    x.tanh()
}
extern "win64" fn m_pow(x: f64, y: f64) -> f64 {
    x.powf(y)
}
extern "win64" fn m_fmod(x: f64, y: f64) -> f64 {
    x % y
}
extern "win64" fn m_atan2(y: f64, x: f64) -> f64 {
    y.atan2(x)
}

/// Code x86 qui pose ses opérandes sur la pile, appelle la passerelle
/// `stub` (cdecl, résultat dans ST0) et rend le résultat dans xmm0.
fn sse2_code(stub: usize, binary: bool) -> usize {
    let size: u8 = if binary { 16 } else { 8 };
    let mut c = vec![0x83, 0xEC, size]; // sub esp, size
    c.extend_from_slice(&[0xF2, 0x0F, 0x11, 0x04, 0x24]); // movsd [esp], xmm0
    if binary {
        c.extend_from_slice(&[0xF2, 0x0F, 0x11, 0x4C, 0x24, 0x08]); // movsd [esp+8], xmm1
    }
    c.push(0xB8); // mov eax, stub
    c.extend_from_slice(&(stub as u32).to_le_bytes());
    c.extend_from_slice(&[0xFF, 0xD0]); // call eax
    c.extend_from_slice(&[0xDD, 0x1C, 0x24]); // fstp qword [esp]
    c.extend_from_slice(&[0xF2, 0x0F, 0x10, 0x04, 0x24]); // movsd xmm0, [esp]
    c.extend_from_slice(&[0x83, 0xC4, size, 0xC3]); // add esp, size ; ret
    super::alloc_code(&c) as usize
}

/// Code x86 de `_CIfmod` / `_CIatan2` : les deux opérandes x87 vont sur la
/// pile x86 dans l'ordre d'appel, puis la passerelle rend ST0.
fn ci_code(stub: usize) -> usize {
    let c = [
        0x83, 0xEC, 0x10, // sub esp, 16
        0xDD, 0x5C, 0x24, 0x08, // fstp qword [esp+8]  (second opérande, ST0)
        0xDD, 0x1C, 0x24, // fstp qword [esp]    (premier, ex-ST1)
        0xB8, 0, 0, 0, 0, // mov eax, stub
        0xFF, 0xD0, // call eax
        0x83, 0xC4, 0x10, // add esp, 16
        0xC3, // ret
    ];
    let mut c = c.to_vec();
    c[11..15].copy_from_slice(&(stub as u32).to_le_bytes());
    super::alloc_code(&c) as usize
}

/// Code x86 d'un `_CI*` à un opérande (`_CIsinh`, `_CIexp`…) : ST0 va sur
/// la pile x86, la passerelle rend le résultat dans ST0 (#1383).
fn ci1_code(stub: usize) -> usize {
    let mut c = vec![
        0x83, 0xEC, 0x08, // sub esp, 8
        0xDD, 0x1C, 0x24, // fstp qword [esp]
        0xB8, 0, 0, 0, 0, // mov eax, stub
        0xFF, 0xD0, // call eax
        0x83, 0xC4, 0x08, // add esp, 8
        0xC3, // ret
    ];
    c[7..11].copy_from_slice(&(stub as u32).to_le_bytes());
    super::alloc_code(&c) as usize
}

fn math_code(func: &str) -> Option<usize> {
    static CODES: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
    let codes = CODES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(&c) = codes.lock().unwrap_or_else(|e| e.into_inner()).get(func) {
        return Some(c);
    }
    let unary = |f: extern "win64" fn(f64) -> f64| {
        sse2_code(
            super::native_stub(f as *const () as usize, "d>d", func),
            false,
        )
    };
    let one =
        |f: extern "win64" fn(f64) -> f64| super::native_stub(f as *const () as usize, "d>d", func);
    let binary = |f: extern "win64" fn(f64, f64) -> f64| {
        super::native_stub(f as *const () as usize, "dd>d", func)
    };
    let code = match func {
        "_libm_sse2_sin_precise" | "_libm_sse2_sin" => unary(m_sin),
        "_libm_sse2_cos_precise" | "_libm_sse2_cos" => unary(m_cos),
        "_libm_sse2_sqrt_precise" | "_libm_sse2_sqrt" => unary(m_sqrt),
        "_libm_sse2_asin_precise" | "_libm_sse2_asin" => unary(m_asin),
        "_libm_sse2_acos_precise" | "_libm_sse2_acos" => unary(m_acos),
        "_libm_sse2_pow_precise" | "_libm_sse2_pow" => sse2_code(binary(m_pow), true),
        "_libm_sse2_atan_precise" | "_libm_sse2_atan" => unary(m_atan),
        "_libm_sse2_tan_precise" | "_libm_sse2_tan" => unary(m_tan),
        "_libm_sse2_exp_precise" | "_libm_sse2_exp" => unary(m_exp),
        "_libm_sse2_log_precise" | "_libm_sse2_log" => unary(m_log),
        "_libm_sse2_log10_precise" | "_libm_sse2_log10" => unary(m_log10),
        "_CIfmod" => ci_code(binary(m_fmod)),
        "_CIpow" => ci_code(binary(m_pow)),
        "_CIsin" => ci1_code(one(m_sin)),
        "_CIcos" => ci1_code(one(m_cos)),
        "_CItan" => ci1_code(one(m_tan)),
        "_CIasin" => ci1_code(one(m_asin)),
        "_CIacos" => ci1_code(one(m_acos)),
        "_CIatan" => ci1_code(one(m_atan)),
        "_CIsqrt" => ci1_code(one(m_sqrt)),
        "_CIexp" => ci1_code(one(m_exp)),
        "_CIlog" => ci1_code(one(m_log)),
        "_CIlog10" => ci1_code(one(m_log10)),
        "_CIsinh" => ci1_code(one(m_sinh)),
        "_CIcosh" => ci1_code(one(m_cosh)),
        "_CItanh" => ci1_code(one(m_tanh)),
        "_CIatan2" => ci_code(binary(m_atan2)),
        // _except1(flags, opcode, arg, résultat, cw, …) : appelée sur une
        // erreur de calcul ; rend le résultat déjà calculé (ST0).
        "_except1" => super::alloc_code(&[0xDD, 0x44, 0x24, 0x14, 0xC3]) as usize, // fld qword [esp+20] ; ret
        _ => return None,
    };
    codes
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(func.to_string(), code);
    Some(code)
}

/// Fonction x86 vide (`ret`).
fn ret_code() -> usize {
    static CODE: OnceLock<usize> = OnceLock::new();
    *CODE.get_or_init(|| super::alloc_code(&[0xC3]) as usize)
}

/// Remplacement x86 d'un export (`n` : nom normalisé de la DLL) : CRT,
/// fonctions à tableaux de HANDLE.
pub(super) fn override_for(n: &str, func: &str) -> Option<Override> {
    if let Some(o) = handle_array_override(n, func) {
        return Some(o);
    }
    let crt = n == "msvcrt" || n == "ucrtbase" || n.starts_with("msvcr");
    if crt {
        if let Some(c) = complex_code(func) {
            return Some(Override::Data(c));
        }
    }
    if n == "user32" && func.starts_with("w") {
        return fmt_override(n, func);
    }
    if n == "ntdll" && func.contains("printf") {
        return fmt_override("msvcrt", func);
    }
    if !crt {
        return None;
    }
    if let Some(o) = fmt_override(n, func) {
        return Some(o);
    }
    // msvcrt.dll x86 : time_t de 32 bits pour les noms sans suffixe.
    let msvcrt = n != "ucrtbase";
    let d = data();
    let tm = Fix::Scratch { off: S_TM, len: 36 };
    let text = Fix::Scratch {
        off: S_ASCTIME,
        len: 26,
    };
    Some(match func {
        "__argc" => Override::Data(d + D_ARGC),
        "__argv" => Override::Data(d + D_ARGV),
        "__wargv" => Override::Data(d + D_WARGV),
        "_environ" => Override::Data(d + D_ENVIRON),
        "_wenviron" => Override::Data(d + D_WENVIRON),
        "__initenv" => Override::Data(d + D_INITENV),
        "__winitenv" => Override::Data(d + D_WINITENV),
        "_fmode" => Override::Data(d + D_FMODE),
        "_commode" => Override::Data(d + D_COMMODE),
        "__mb_cur_max" => Override::Data(d + D_MB_CUR_MAX),
        "_acmdln" => Override::Data(d + D_ACMDLN),
        "_wcmdln" => Override::Data(d + D_WCMDLN),
        "_iob" => Override::Data(d + D_IOB),
        "_adjust_fdiv" => Override::Data(d + D_ADJUST_FDIV),
        "__pioinfo" => Override::Data(ioinfo32().0),
        "__badioinfo" => Override::Data(ioinfo32().1),
        f if f.starts_with("_CI") => Override::Data(math_code(func)?),
        "_except1" => Override::Data(math_code(func)?),
        f if f.starts_with("_libm_sse2_") => Override::Data(math_code(func)?),
        // Aides du compilateur MSVC x86 : code x86 direct (valeur dans ST0).
        "_ftol" | "_ftol2" | "_ftol2_sse" => Override::Data(ftol_code()),
        // type_info::~type_info (thiscall, sans argument sur la pile) : le
        // nom décoré mis en cache n'est jamais alloué ici, rien à libérer.
        "??1type_info@@UAE@XZ" => Override::Data(ret_code()),
        // std::exception et std::bad_cast de msvcrt x86 (thiscall).
        f if f.contains("exception@@") || f.contains("bad_cast@@") => {
            Override::Data(exc_code(func)?)
        }
        // FuncInfo dans eax : petit code d'entrée propre (voir `cxx`).
        "__CxxFrameHandler" | "__CxxFrameHandler2" | "__CxxFrameHandler3" => {
            Override::Data(super::cxx::frame_handler_code())
        }
        "_pctype" => Override::Data(d + D_PCTYPE_PTR),
        "_pwctype" => Override::Data(d + D_PWCTYPE_PTR),
        "_timezone" | "_daylight" | "_dstbias" => Override::Data(scalar_copy(n, func, 4)?),
        "_sys_nerr" => {
            // Tables remplies avant la copie basse (#1437).
            errlist_low(n)?;
            Override::Data(scalar_copy(n, func, 4)?)
        }
        "__sys_nerr" => {
            // Remplit les tables avant la copie basse.
            errlist_low(n)?;
            raw(ret_aux, scalar_copy(n, "_sys_nerr", 4)?)
        }
        "__sys_errlist" => raw(ret_aux, errlist_low(n)?),
        "_sys_errlist" => Override::Data(errlist_low(n)?),
        "__timezone" | "__p__timezone" => raw(ret_tz, 0),
        "__daylight" | "__p__daylight" => raw(ret_tz, 1),
        "__dstbias" | "__p__dstbias" => raw(ret_tz, 2),
        "__tzname" | "__p__tzname" => raw(ret_tz, 3),
        "_tzname" => Override::Data(tz_low(3)),
        "__p___argc" => raw(ret_aux, d + D_ARGC),
        "__p___argv" => raw(ret_aux, d + D_ARGV),
        "__p___wargv" => raw(ret_aux, d + D_WARGV),
        "__p__environ" => raw(ret_aux, d + D_ENVIRON),
        "__p__wenviron" => raw(ret_aux, d + D_WENVIRON),
        "__p___initenv" => raw(ret_aux, d + D_INITENV),
        "__p___winitenv" => raw(ret_aux, d + D_WINITENV),
        "__p__fmode" => raw(ret_aux, d + D_FMODE),
        "__p__commode" => raw(ret_aux, d + D_COMMODE),
        "__p__acmdln" => raw(ret_aux, d + D_ACMDLN),
        "__p__wcmdln" => raw(ret_aux, d + D_WCMDLN),
        "__p___mb_cur_max" => raw(ret_aux, d + D_MB_CUR_MAX),
        "__p__pctype" => raw(ret_aux, d + D_PCTYPE_PTR),
        "__p__pwctype" => raw(ret_aux, d + D_PWCTYPE_PTR),
        "__pctype_func" => raw(ret_deref, d + D_PCTYPE_PTR),
        "__pwctype_func" => raw(ret_deref, d + D_PWCTYPE_PTR),
        "_get_initial_narrow_environment" => raw(ret_deref, d + D_ENVIRON),
        "_get_initial_wide_environment" => raw(ret_deref, d + D_WENVIRON),
        "__iob_func" | "__p__iob" => raw(ret_aux, d + D_IOB),
        "__acrt_iob_func" => raw(iob_at, 0),
        "localeconv" => raw(ret_aux, d + D_LCONV),
        "__getmainargs" => raw(getmainargs, 0),
        "__wgetmainargs" => raw(getmainargs, 1),
        "_initterm" => raw(initterm, 0),
        "_initterm_e" => raw(initterm, 1),
        "atexit" | "_crt_atexit" => raw(atexit32, 0),
        "_onexit" => raw(atexit32, 1),
        "__dllonexit" => raw(dllonexit32, 0),
        "at_quick_exit" | "_crt_at_quick_exit" => raw(ret_zero, 0),
        "_initialize_onexit_table" => raw(onexit_table, 0),
        "_register_onexit_function" => raw(onexit_table, 1),
        "_execute_onexit_table" => raw(onexit_table, 2),
        "qsort" => raw(qsort32, 0),
        "bsearch" => raw(bsearch32, 0),
        "signal" => raw(signal32, 0),
        "_setjmp3" => raw(setjmp32, 1),
        "_setjmp" | "setjmp" => raw(setjmp32, 0),
        "longjmp" => raw(longjmp32, 0),
        "setlocale" => native(n, func, "ii>i", Fix::Str { wide: false })?,
        "_wsetlocale" => native(n, func, "ii>i", Fix::Str { wide: true })?,
        "strerror" | "_strerror" => native(n, func, "i>i", Fix::Str { wide: false })?,
        "_wcserror" | "__wcserror" => native(n, func, "i>i", Fix::Str { wide: true })?,
        "getenv" => native(n, func, "i>i", Fix::Str { wide: false })?,
        "_wgetenv" => native(n, func, "i>i", Fix::Str { wide: true })?,
        "___lc_handle_func" => native(n, func, ">i", Fix::Static { len: 24 })?,
        "time" if msvcrt => native(n, "_time32", "i>i", Fix::None)?,
        "localtime" if msvcrt => native(n, "_localtime32", "i>i", tm)?,
        "gmtime" if msvcrt => native(n, "_gmtime32", "i>i", tm)?,
        "mktime" if msvcrt => native(n, "_mktime32", "i>i", Fix::None)?,
        "ctime" if msvcrt => native(n, "_ctime32", "i>i", text)?,
        "difftime" if msvcrt => native(n, "_difftime32", "ii>d", Fix::None)?,
        "_localtime32" | "_localtime64" | "_gmtime32" | "_gmtime64" => native(n, func, "i>i", tm)?,
        "asctime" | "_ctime32" | "_ctime64" => native(n, func, "i>i", text)?,
        _ => return None,
    })
}

/// `__pioinfo` et `__badioinfo` de msvcrt x86 (#1508) : 64 pointeurs de
/// 32 bits vers des tableaux de 32 `ioinfo` vides (0x40 octets chacun),
/// et l'entrée « invalide » (handle -1, FTEXT, LF), sous 4 Go.
fn ioinfo32() -> (usize, usize) {
    static P: std::sync::OnceLock<(usize, usize)> = std::sync::OnceLock::new();
    *P.get_or_init(|| {
        const ENTRY: usize = 0x40;
        let table = super::alloc_low(64 * 4 + ENTRY + 64 * 32 * ENTRY, false);
        let bad = table + 64 * 4;
        let blocks = bad + ENTRY;
        unsafe {
            for i in 0..64 {
                *((table + i * 4) as *mut u32) = (blocks + i * 32 * ENTRY) as u32;
            }
            *(bad as *mut u32) = u32::MAX;
            *((bad + 4) as *mut u8) = 0x80;
            *((bad + 5) as *mut u8) = 10;
        }
        (table, bad)
    })
}

// ---------------------------------------------------------------------
// Nombres complexes de la CRT (#1508), convention x86 de MSVC
// ---------------------------------------------------------------------
//
// `_Dcomplex` (et `_Lcomplex`, long double = double) passe par valeur sur
// la pile et revient par un pointeur caché, premier argument, rendu dans
// eax ; `_Fcomplex` passe par valeur et revient dans edx:eax ; les
// résultats réels (creal, carg…) reviennent dans st0. Toutes cdecl.

use crate::dlls::crt_releve::{complex_op, complex_pow, COp, C64};

fn c_op(func: &str) -> Option<COp> {
    Some(match func {
        "cexp" => COp::Exp,
        "ccos" => COp::Cos,
        "ccosh" => COp::Cosh,
        "csin" => COp::Sin,
        "csinh" => COp::Sinh,
        "ctan" => COp::Tan,
        "ctanh" => COp::Tanh,
        _ => return None,
    })
}

const C_OPS: [COp; 7] = [
    COp::Exp,
    COp::Cos,
    COp::Cosh,
    COp::Sin,
    COp::Sinh,
    COp::Tan,
    COp::Tanh,
];

unsafe fn c_store(ret: u64, z: C64) -> u64 {
    let p = ret as u32 as usize as *mut f64;
    if !p.is_null() {
        p.write_unaligned(z.re);
        p.add(1).write_unaligned(z.im);
    }
    ret as u32 as u64
}

fn f_pack(z: C64) -> u64 {
    (z.re as f32).to_bits() as u64 | ((z.im as f32).to_bits() as u64) << 32
}

macro_rules! c32_ops {
    ($($i:literal => $d:ident, $f:ident;)*) => {
        $(
            extern "win64" fn $d(ret: u64, re: f64, im: f64) -> u64 {
                unsafe { c_store(ret, complex_op(C_OPS[$i], C64 { re, im })) }
            }
            extern "win64" fn $f(re: f32, im: f32) -> u64 {
                f_pack(complex_op(C_OPS[$i], C64 { re: re as f64, im: im as f64 }))
            }
        )*
        fn c32_targets(op: COp) -> (usize, usize) {
            match op as usize {
                $($i => ($d as *const () as usize, $f as *const () as usize),)*
                _ => unreachable!(),
            }
        }
    };
}

c32_ops! {
    0 => c32_exp, c32_expf;
    1 => c32_cos, c32_cosf;
    2 => c32_cosh, c32_coshf;
    3 => c32_sin, c32_sinf;
    4 => c32_sinh, c32_sinhf;
    5 => c32_tan, c32_tanf;
    6 => c32_tanh, c32_tanhf;
}

extern "win64" fn c32_pow(ret: u64, are: f64, aim: f64, bre: f64, bim: f64) -> u64 {
    let z = complex_pow(C64 { re: are, im: aim }, C64 { re: bre, im: bim });
    unsafe { c_store(ret, z) }
}
extern "win64" fn c32_powf(are: f32, aim: f32, bre: f32, bim: f32) -> u64 {
    f_pack(complex_pow(
        C64 {
            re: are as f64,
            im: aim as f64,
        },
        C64 {
            re: bre as f64,
            im: bim as f64,
        },
    ))
}
extern "win64" fn c32_real(re: f64, _im: f64) -> f64 {
    re
}
extern "win64" fn c32_imag(_re: f64, im: f64) -> f64 {
    im
}
extern "win64" fn c32_arg(re: f64, im: f64) -> f64 {
    im.atan2(re)
}
extern "win64" fn c32_abs(re: f64, im: f64) -> f64 {
    re.hypot(im)
}
extern "win64" fn c32_realf(re: f32, _im: f32) -> f32 {
    re
}
extern "win64" fn c32_imagf(_re: f32, im: f32) -> f32 {
    im
}
extern "win64" fn c32_argf(re: f32, im: f32) -> f32 {
    im.atan2(re)
}
extern "win64" fn c32_absf(re: f32, im: f32) -> f32 {
    re.hypot(im)
}

/// Passerelle x86 d'une fonction complexe de la CRT, si `func` en est une.
fn complex_code(func: &str) -> Option<usize> {
    let (base, float) = match func {
        "_cabs" => ("cabs", false),
        f if f.starts_with('c') && (f.ends_with('f') || f.ends_with('l')) => {
            (&f[..f.len() - 1], f.ends_with('f'))
        }
        f => (f, false),
    };
    let d2 = |f: extern "win64" fn(f64, f64) -> f64| (f as *const () as usize, "dd>d");
    let f2 = |f: extern "win64" fn(f32, f32) -> f32| (f as *const () as usize, "ff>f");
    let (target, sig) = match (base, float) {
        ("creal", false) => d2(c32_real),
        ("cimag", false) => d2(c32_imag),
        ("carg", false) => d2(c32_arg),
        ("cabs", false) => d2(c32_abs),
        ("creal", true) => f2(c32_realf),
        ("cimag", true) => f2(c32_imagf),
        ("carg", true) => f2(c32_argf),
        ("cabs", true) => f2(c32_absf),
        ("cpow", false) => (c32_pow as *const () as usize, "idddd>i"),
        ("cpow", true) => (c32_powf as *const () as usize, "ffff>q"),
        (b, false) => (c32_targets(c_op(b)?).0, "idd>i"),
        (b, true) => (c32_targets(c_op(b)?).1, "ff>q"),
    };
    static CODES: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
    let codes = CODES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut m = codes.lock().unwrap_or_else(|e| e.into_inner());
    Some(
        *m.entry(func.to_string())
            .or_insert_with(|| super::native_stub(target, sig, func)),
    )
}

// ---------------------------------------------------------------------
// std::exception et std::bad_cast de msvcrt x86 (#1508)
// ---------------------------------------------------------------------
//
// Disposition x86 : { vftable, const char *what, int doFree } (12 octets),
// table virtuelle { destructeur de suppression, what }. Méthodes
// thiscall (this dans ecx, l'appelé dépile ses arguments) : un petit code
// x86 passe ecx et l'argument à une fonction 64 bits cdecl.

const EXC_NAMES: [&str; 7] = [
    "??0exception@@QAE@XZ",
    "??0exception@@QAE@ABQBD@Z",
    "??0exception@@QAE@ABV0@@Z",
    "??1exception@@UAE@XZ",
    "?what@exception@@UBEPBDXZ",
    "??0bad_cast@@QAE@ABV0@@Z",
    "??1bad_cast@@UAE@XZ",
];

unsafe fn rd32(a: u64) -> u32 {
    (a as u32 as usize as *const u32).read_unaligned()
}
unsafe fn wr32(a: u64, v: u32) {
    (a as u32 as usize as *mut u32).write_unaligned(v)
}

/// Copie basse (tas, sous 4 Go) d'une chaîne x86.
unsafe fn exc_dup(s: u32) -> u32 {
    if s == 0 {
        return 0;
    }
    let len = libc::strlen(s as usize as *const libc::c_char);
    let p = libc::malloc(len + 1) as *mut u8;
    if p.is_null() || p as usize >= 1 << 32 {
        libc::free(p.cast());
        return 0;
    }
    std::ptr::copy_nonoverlapping(s as usize as *const u8, p, len + 1);
    p as usize as u32
}

/// Tables virtuelles de `exception` et `bad_cast`, et le texte par défaut.
fn exc_tables() -> (u32, u32, u32) {
    static T: OnceLock<(u32, u32, u32)> = OnceLock::new();
    *T.get_or_init(|| {
        let block = super::alloc_low(64, false);
        let text = b"Unknown exception\0";
        let vdtor = exc_thunk(
            exc_vdtor as *const () as usize,
            1,
            "exception::`vector deleting destructor'",
        );
        let what = exc_thunk(exc_what as *const () as usize, 0, "exception::what");
        unsafe {
            // [0] localisateur (absent), [1] vdtor, [2] what ; idem en +12.
            for base in [0usize, 12] {
                *((block + base + 4) as *mut u32) = vdtor as u32;
                *((block + base + 8) as *mut u32) = what as u32;
            }
            std::ptr::copy_nonoverlapping(text.as_ptr(), (block + 32) as *mut u8, text.len());
        }
        ((block + 4) as u32, (block + 16) as u32, (block + 32) as u32)
    })
}

extern "win64" fn exc_ctor(this: u64, _: u64) -> u64 {
    unsafe {
        wr32(this, exc_tables().0);
        wr32(this + 4, 0);
        wr32(this + 8, 0);
    }
    this as u32 as u64
}
extern "win64" fn exc_ctor_str(this: u64, pp: u64) -> u64 {
    unsafe {
        exc_ctor(this, 0);
        if pp as u32 != 0 {
            let d = exc_dup(rd32(pp));
            wr32(this + 4, d);
            wr32(this + 8, (d != 0) as u32);
        }
    }
    this as u32 as u64
}
extern "win64" fn exc_copy(this: u64, other: u64) -> u64 {
    unsafe {
        exc_ctor(this, 0);
        if other as u32 != 0 && other as u32 != this as u32 {
            let what = rd32(other + 4);
            if rd32(other + 8) != 0 {
                let d = exc_dup(what);
                wr32(this + 4, d);
                wr32(this + 8, (d != 0) as u32);
            } else {
                wr32(this + 4, what);
            }
        }
    }
    this as u32 as u64
}
extern "win64" fn bad_cast_copy(this: u64, other: u64) -> u64 {
    exc_copy(this, other);
    unsafe { wr32(this, exc_tables().1) };
    this as u32 as u64
}
extern "win64" fn exc_dtor(this: u64, _: u64) -> u64 {
    unsafe {
        wr32(this, exc_tables().0);
        if rd32(this + 8) != 0 {
            libc::free(rd32(this + 4) as usize as *mut libc::c_void);
        }
        wr32(this + 4, 0);
        wr32(this + 8, 0);
    }
    this as u32 as u64
}
extern "win64" fn exc_vdtor(this: u64, flags: u64) -> u64 {
    exc_dtor(this, 0);
    if flags & 1 != 0 {
        unsafe { libc::free(this as u32 as usize as *mut libc::c_void) };
    }
    this as u32 as u64
}
extern "win64" fn exc_what(this: u64, _: u64) -> u64 {
    match unsafe { rd32(this + 4) } {
        0 => exc_tables().2 as u64,
        w => w as u64,
    }
}

/// Code x86 thiscall : `push arg ; push ecx ; call cible ; add esp, 8 ;
/// ret 4*args`, la cible étant une passerelle cdecl « ii>i ».
fn exc_thunk(target: usize, args: u8, label: &str) -> usize {
    let native = super::native_stub(target, "ii>i", label) as u32;
    let mut c = Vec::new();
    if args == 1 {
        c.extend_from_slice(&[0xFF, 0x74, 0x24, 0x04]); // push [esp+4]
    } else {
        c.extend_from_slice(&[0x6A, 0x00]); // push 0
    }
    c.push(0x51); // push ecx
    c.push(0xB8); // mov eax, cible
    c.extend_from_slice(&native.to_le_bytes());
    c.extend_from_slice(&[0xFF, 0xD0, 0x83, 0xC4, 0x08]); // call eax ; add esp, 8
    if args == 1 {
        c.extend_from_slice(&[0xC2, 0x04, 0x00]);
    } else {
        c.push(0xC3);
    }
    super::alloc_code(&c) as usize
}

fn exc_code(func: &str) -> Option<usize> {
    let i = EXC_NAMES.iter().position(|n| *n == func)?;
    static CODES: OnceLock<Vec<usize>> = OnceLock::new();
    let codes = CODES.get_or_init(|| {
        let f = |t: extern "win64" fn(u64, u64) -> u64, args: u8, name: &str| {
            exc_thunk(t as *const () as usize, args, name)
        };
        vec![
            f(exc_ctor, 0, EXC_NAMES[0]),
            f(exc_ctor_str, 1, EXC_NAMES[1]),
            f(exc_copy, 1, EXC_NAMES[2]),
            f(exc_dtor, 0, EXC_NAMES[3]),
            f(exc_what, 0, EXC_NAMES[4]),
            f(bad_cast_copy, 1, EXC_NAMES[5]),
            f(exc_dtor, 0, EXC_NAMES[6]),
        ]
    });
    codes.get(i).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conv(fmt: &str, words: &[u32], scan: bool) -> Vec<u64> {
        let f: Vec<u32> = fmt.chars().map(|c| c as u32).collect();
        let buf: Vec<u32> = words.to_vec();
        // Pile x86 simulée : tampon dans le tas, sous 4 Go dans le test
        // seulement si l'allocateur l'y met ; on lit via `rd` qui prend une
        // adresse 32 bits, donc on recopie en mémoire basse.
        let low = crate::wow32::alloc_low(4096, false);
        unsafe {
            std::ptr::copy_nonoverlapping(buf.as_ptr(), low as *mut u32, buf.len());
        }
        convert_args(&f, low as u32, scan, false)
    }

    #[test]
    fn les_arguments_printf_x86_passent_en_cases_de_8_octets() {
        let d = 2.5f64.to_bits();
        let v = conv(
            "%d|%5.2f|%s|%lld|%c|%x|%*d|%%",
            &[
                (-7i32) as u32,
                d as u32,
                (d >> 32) as u32,
                0x1234,
                0x8765_4321,
                0x12,
                b'Z' as u32,
                0xbeef,
                4,
                9,
            ],
            false,
        );
        assert_eq!(
            v,
            vec![
                (-7i64) as u64,
                d,
                0x1234,
                0x12_8765_4321,
                b'Z' as u64,
                0xbeef,
                4,
                9
            ]
        );
    }

    #[test]
    fn scanf_s_x86_garde_la_taille_apres_chaque_tampon() {
        let f: Vec<u32> = "%d %s %*s %c %5[a-z]".chars().map(|c| c as u32).collect();
        let words = [0x100u32, 0x200, 16, 0x300, 1, 0x400, 6];
        let low = crate::wow32::alloc_low(4096, false);
        unsafe {
            std::ptr::copy_nonoverlapping(words.as_ptr(), low as *mut u32, words.len());
        }
        let v = convert_args(&f, low as u32, true, true);
        assert_eq!(v, vec![0x100, 0x200, 16, 0x300, 1, 0x400, 6]);
    }

    #[test]
    fn complexes_x86_par_pointeur_cache_et_edx_eax() {
        let low = crate::wow32::alloc_low(4096, false);
        let r = c32_exp(low as u64, 0.0, std::f64::consts::PI);
        assert_eq!(r, low as u64);
        let (re, im) = unsafe { (*(low as *const f64), *((low + 8) as *const f64)) };
        assert!((re + 1.0).abs() < 1e-12 && im.abs() < 1e-12);
        let packed = c32_powf(0.0, 1.0, 2.0, 0.0);
        let (re, im) = (
            f32::from_bits(packed as u32),
            f32::from_bits((packed >> 32) as u32),
        );
        assert!((re + 1.0).abs() < 1e-6 && im.abs() < 1e-6);
        assert_eq!(c32_arg(0.0, 1.0), std::f64::consts::FRAC_PI_2);
        for f in ["cexp", "ctanhf", "cpowl", "crealf", "_cabs"] {
            assert!(complex_code(f).is_some(), "{f}");
        }
        for f in ["ceil", "ceilf", "cosl", "cbrtl"] {
            assert!(complex_code(f).is_none(), "{f}");
        }
    }

    #[test]
    fn pioinfo_x86_sous_4_go() {
        let (table, bad) = ioinfo32();
        assert!(table < 1 << 32 && bad < 1 << 32);
        let first = unsafe { *(table as *const u32) } as usize;
        assert!(first != 0 && unsafe { *(first as *const u32) } == 0);
        assert_eq!(unsafe { *(bad as *const u32) }, u32::MAX);
    }

    #[test]
    fn scanf_compte_les_pointeurs_et_saute_les_conversions_ignorees() {
        let v = conv("%d %*s %lf %[^,], %%", &[0x100, 0x200, 0x300], true);
        assert_eq!(v, vec![0x100, 0x200, 0x300]);
    }

    #[test]
    fn le_tri_fusion_suit_le_comparateur() {
        let vals = [5, 3, 9, 1, 4, 3];
        let mut idx: Vec<u32> = (0..6).collect();
        let mut tmp = vec![0; 6];
        merge_sort(&mut idx, &mut tmp, &mut |a, b| {
            vals[a as usize] - vals[b as usize]
        });
        let sorted: Vec<i32> = idx.iter().map(|&i| vals[i as usize]).collect();
        assert_eq!(sorted, vec![1, 3, 3, 4, 5, 9]);
        // Stable : le premier 3 (rang 1) reste devant le second (rang 5).
        assert_eq!(&idx[1..3], &[1, 5]);
    }
}
