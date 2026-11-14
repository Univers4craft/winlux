//! CRT : fonctions multioctets `_mbschr`, `_mbspbrk`, `_mbsnbcmp` et
//! `_mbsnbcpy` (et leurs variantes `_l`), servies par ucrtbase,
//! api-ms-win-crt-multibyte-l1-1-0, msvcrt et msvcr*.
//!
//! OBS Studio s'arrêtait sur `_mbspbrk` (#2218). Le reste de la famille
//! `_mbs*` est suivi par #2139.
//!
//! Sémantique : documentation publique de la CRT Microsoft (pages
//! « strchr, wcschr, _mbschr, _mbschr_l », « strpbrk, wcspbrk, _mbspbrk,
//! _mbspbrk_l », « _mbsnbcmp, _mbsnbcmp_l », « _mbsnbcpy, _mbsnbcpy_l »).
//! Les caractères doubles sont reconnus selon la page de code multioctet,
//! qui vaut par défaut la page ANSI du système (documentation de
//! `_setmbcp`) ; une valeur de caractère double est `(tête << 8) | queue`.
//! La locale passée aux variantes `_l` ne change pas cette page ici, comme
//! pour `_mbsrchr` dans crt_releve.rs.

use super::crt_runtime::errno;
use super::Exports;
use std::ffi::c_void;

const EINVAL: i32 = 22;
/// `_NLSCMPERROR` (string.h, mbstring.h).
const NLSCMPERROR: i32 = i32::MAX;

fn codepage() -> u32 {
    crate::intl::codepage::resolve(crate::intl::codepage::CP_ACP)
}

/// Longueur en octets du caractère qui commence en `s` : 2 pour un octet
/// de tête suivi d'un octet non nul, sinon 1.
unsafe fn char_len(cp: u32, s: *const u8) -> usize {
    if crate::intl::dbcs::is_lead_byte(cp, *s) && *s.add(1) != 0 {
        2
    } else {
        1
    }
}

/// Valeur du caractère qui commence en `s` (et sa longueur en octets).
unsafe fn char_at(cp: u32, s: *const u8) -> (u32, usize) {
    match char_len(cp, s) {
        2 => (((*s as u32) << 8) | *s.add(1) as u32, 2),
        _ => (*s as u32, 1),
    }
}

/// `_mbschr(str, c)` : premier caractère `c` de `str`, ou NULL ; `c` nul
/// désigne le zéro final.
unsafe extern "win64" fn mbschr(s: *const u8, c: u32) -> *const u8 {
    mbschr_in(codepage(), s, c)
}

unsafe fn mbschr_in(cp: u32, s: *const u8, c: u32) -> *const u8 {
    if s.is_null() {
        *errno() = EINVAL;
        return std::ptr::null();
    }
    let mut p = s;
    loop {
        if *p == 0 {
            return if c == 0 { p } else { std::ptr::null() };
        }
        let (v, n) = char_at(cp, p);
        if v == c {
            return p;
        }
        p = p.add(n);
    }
}

unsafe extern "win64" fn mbschr_l(s: *const u8, c: u32, _loc: *mut c_void) -> *const u8 {
    mbschr(s, c)
}

/// Vrai si le caractère `v` figure dans le jeu `set` (lu caractère par
/// caractère).
unsafe fn in_set(cp: u32, set: *const u8, v: u32) -> bool {
    let mut q = set;
    while *q != 0 {
        let (w, n) = char_at(cp, q);
        if w == v {
            return true;
        }
        q = q.add(n);
    }
    false
}

/// `_mbspbrk(str, set)` : premier caractère de `str` qui appartient à
/// `set`, ou NULL ; le zéro final n'est pas cherché.
unsafe extern "win64" fn mbspbrk(s: *const u8, set: *const u8) -> *const u8 {
    mbspbrk_in(codepage(), s, set)
}

unsafe fn mbspbrk_in(cp: u32, s: *const u8, set: *const u8) -> *const u8 {
    if s.is_null() || set.is_null() {
        *errno() = EINVAL;
        return std::ptr::null();
    }
    let mut p = s;
    while *p != 0 {
        let (v, n) = char_at(cp, p);
        if in_set(cp, set, v) {
            return p;
        }
        p = p.add(n);
    }
    std::ptr::null()
}

unsafe extern "win64" fn mbspbrk_l(s: *const u8, set: *const u8, _loc: *mut c_void) -> *const u8 {
    mbspbrk(s, set)
}

/// `_mbsnbcmp(a, b, count)` : compare au plus `count` octets, caractère par
/// caractère (valeurs non signées) ; un caractère double coupé par la
/// limite ne compte que par son octet de tête.
unsafe extern "win64" fn mbsnbcmp(a: *const u8, b: *const u8, count: usize) -> i32 {
    mbsnbcmp_in(codepage(), a, b, count)
}

unsafe fn mbsnbcmp_in(cp: u32, a: *const u8, b: *const u8, count: usize) -> i32 {
    if count == 0 {
        return 0;
    }
    if a.is_null() || b.is_null() {
        *errno() = EINVAL;
        return NLSCMPERROR;
    }
    let mut i = 0usize;
    while i < count {
        let (mut va, na) = char_at(cp, a.add(i));
        let (mut vb, nb) = char_at(cp, b.add(i));
        if i + 1 == count {
            // Il ne reste qu'un octet : seul l'octet de tête se compare.
            if na == 2 {
                va >>= 8;
            }
            if nb == 2 {
                vb >>= 8;
            }
        }
        if va != vb {
            return if va < vb { -1 } else { 1 };
        }
        if va == 0 {
            return 0;
        }
        i += na.max(nb);
    }
    0
}

unsafe extern "win64" fn mbsnbcmp_l(
    a: *const u8,
    b: *const u8,
    count: usize,
    _loc: *mut c_void,
) -> i32 {
    mbsnbcmp(a, b, count)
}

/// `_mbsnbcpy(dst, src, count)` : copie au plus `count` octets comme
/// `strncpy` (le reste est complété de zéros) ; un caractère double coupé
/// par la limite n'est pas copié à moitié, son octet de tête devient nul.
unsafe extern "win64" fn mbsnbcpy(dst: *mut u8, src: *const u8, count: usize) -> *mut u8 {
    mbsnbcpy_in(codepage(), dst, src, count)
}

unsafe fn mbsnbcpy_in(cp: u32, dst: *mut u8, src: *const u8, count: usize) -> *mut u8 {
    if count == 0 {
        return dst;
    }
    if dst.is_null() || src.is_null() {
        *errno() = EINVAL;
        return std::ptr::null_mut();
    }
    let mut i = 0usize;
    while i < count && *src.add(i) != 0 {
        let n = char_len(cp, src.add(i));
        if i + n > count {
            break;
        }
        for k in 0..n {
            *dst.add(i + k) = *src.add(i + k);
        }
        i += n;
    }
    while i < count {
        *dst.add(i) = 0;
        i += 1;
    }
    dst
}

unsafe extern "win64" fn mbsnbcpy_l(
    dst: *mut u8,
    src: *const u8,
    count: usize,
    _loc: *mut c_void,
) -> *mut u8 {
    mbsnbcpy(dst, src, count)
}

pub fn exports() -> Exports {
    crate::exports! {
        "_mbschr" => fn mbschr, "_mbschr_l" => fn mbschr_l,
        "_mbspbrk" => fn mbspbrk, "_mbspbrk_l" => fn mbspbrk_l,
        "_mbsnbcmp" => fn mbsnbcmp, "_mbsnbcmp_l" => fn mbsnbcmp_l,
        "_mbsnbcpy" => fn mbsnbcpy, "_mbsnbcpy_l" => fn mbsnbcpy_l,
    }
}

#[cfg(test)]
mod tests;
