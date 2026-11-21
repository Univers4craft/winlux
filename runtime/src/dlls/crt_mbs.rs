//! CRT : fonctions multioctets `_mbschr`, `_mbspbrk`, `_mbsnbcmp` et
//! `_mbsnbcpy` (et leurs variantes `_l`), servies par ucrtbase,
//! api-ms-win-crt-multibyte-l1-1-0, msvcrt et msvcr*.
//!
//! OBS Studio s'arrêtait sur `_mbspbrk` (#2218). Le reste des fonctions
//! de chaînes `_mbs*` et `_mbc*` vient avec #2139.
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

// ---------------------------------------------------------------------
// Reste de la famille `_mbs*` (#2139)
// ---------------------------------------------------------------------
//
// Learn, pages « _mbslen », « _mbsinc », « _mbsdec », « _mbsnextc »,
// « strstr/_mbsstr », « strtok/_mbstok », « _stricmp/_mbsicmp »,
// « strncmp/_mbsncmp », « _strnicmp/_mbsnicmp », « strcspn/_mbscspn »,
// « strspn/_mbsspn », « _strrev/_mbsrev », « _strlwr/_mbslwr »,
// « _strupr/_mbsupr », « _strset/_mbsset », « _strnset/_mbsnset »,
// « _mbsnbset », « strncpy/_mbsncpy », « strncat/_mbsncat », « _mbsnbcat »,
// « _mbclen », « _mbccpy », « _mbctolower/_mbctoupper ». Comme pour les
// `*_s` de #2128, seules les lettres ASCII changent de casse ; les
// caractères doubles restent tels quels.

/// Fonction de la CRT servie par Winlux, par son nom (les `*_s` de
/// crt_secure_reste.rs, dont les versions non sécurisées reprennent le
/// travail).
unsafe fn base<T: Copy>(name: &str) -> T {
    debug_assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<usize>());
    let a = super::lookup("msvcrt", name).unwrap_or_else(|| panic!("msvcrt!{name}"));
    std::mem::transmute_copy(&a)
}

unsafe fn c_len(s: *const u8) -> usize {
    std::ffi::CStr::from_ptr(s.cast()).to_bytes().len()
}

/// `_mbslen(str)` : nombre de caractères.
unsafe extern "win64" fn mbslen(s: *const u8) -> usize {
    mbslen_in(codepage(), s)
}
unsafe fn mbslen_in(cp: u32, s: *const u8) -> usize {
    if s.is_null() {
        *errno() = EINVAL;
        return 0;
    }
    let (mut p, mut n) = (s, 0);
    while *p != 0 {
        p = p.add(char_len(cp, p));
        n += 1;
    }
    n
}

/// `_mbsinc(current)` : caractère suivant.
unsafe extern "win64" fn mbsinc(p: *const u8) -> *const u8 {
    if p.is_null() {
        *errno() = EINVAL;
        return std::ptr::null();
    }
    p.add(char_len(codepage(), p))
}

/// `_mbsdec(start, current)` : début du caractère qui précède `current`,
/// NULL si `start >= current`.
unsafe extern "win64" fn mbsdec(start: *const u8, cur: *const u8) -> *const u8 {
    mbsdec_in(codepage(), start, cur)
}
unsafe fn mbsdec_in(cp: u32, start: *const u8, cur: *const u8) -> *const u8 {
    if start.is_null() || cur.is_null() {
        *errno() = EINVAL;
        return std::ptr::null();
    }
    if start >= cur {
        return std::ptr::null();
    }
    let mut p = start;
    loop {
        let q = p.add(char_len(cp, p).max(1));
        if q >= cur {
            return p;
        }
        p = q;
    }
}

/// `_mbsnextc(str)` : valeur du caractère qui commence en `str`.
unsafe extern "win64" fn mbsnextc(p: *const u8) -> u32 {
    if p.is_null() {
        *errno() = EINVAL;
        return 0;
    }
    char_at(codepage(), p).0
}

/// `_mbclen(c)` : 2 pour un caractère double, sinon 1.
unsafe extern "win64" fn mbclen(p: *const u8) -> usize {
    if p.is_null() {
        return 0;
    }
    if crate::intl::dbcs::is_lead_byte(codepage(), *p) {
        2
    } else {
        1
    }
}

/// `_mbccpy(dest, src)` : copie un caractère (un ou deux octets).
unsafe extern "win64" fn mbccpy(dst: *mut u8, src: *const u8) {
    if dst.is_null() || src.is_null() {
        *errno() = EINVAL;
        return;
    }
    let n = char_len(codepage(), src);
    std::ptr::copy_nonoverlapping(src, dst, n);
}

fn lower(v: u32) -> u32 {
    if (b'A' as u32..=b'Z' as u32).contains(&v) {
        v + 32
    } else {
        v
    }
}
fn upper(v: u32) -> u32 {
    if (b'a' as u32..=b'z' as u32).contains(&v) {
        v - 32
    } else {
        v
    }
}
extern "win64" fn mbctolower(c: u32) -> u32 {
    lower(c)
}
extern "win64" fn mbctoupper(c: u32) -> u32 {
    upper(c)
}

/// Compare au plus `count` caractères, en minuscules si `fold`.
unsafe fn mbsncmp_in(cp: u32, a: *const u8, b: *const u8, count: usize, fold: bool) -> i32 {
    if count == 0 {
        return 0;
    }
    if a.is_null() || b.is_null() {
        *errno() = EINVAL;
        return NLSCMPERROR;
    }
    let (mut pa, mut pb) = (a, b);
    for _ in 0..count {
        let (mut va, na) = char_at(cp, pa);
        let (mut vb, nb) = char_at(cp, pb);
        if fold {
            va = lower(va);
            vb = lower(vb);
        }
        if va != vb {
            return if va < vb { -1 } else { 1 };
        }
        if va == 0 {
            return 0;
        }
        pa = pa.add(na);
        pb = pb.add(nb);
    }
    0
}
unsafe extern "win64" fn mbsicmp(a: *const u8, b: *const u8) -> i32 {
    mbsncmp_in(codepage(), a, b, usize::MAX, true)
}
unsafe extern "win64" fn mbsncmp(a: *const u8, b: *const u8, count: usize) -> i32 {
    mbsncmp_in(codepage(), a, b, count, false)
}
unsafe extern "win64" fn mbsnicmp(a: *const u8, b: *const u8, count: usize) -> i32 {
    mbsncmp_in(codepage(), a, b, count, true)
}

/// `_mbsstr(str, sub)` : première occurrence de `sub` qui commence sur
/// une frontière de caractère.
unsafe extern "win64" fn mbsstr(s: *const u8, sub: *const u8) -> *const u8 {
    mbsstr_in(codepage(), s, sub)
}
unsafe fn mbsstr_in(cp: u32, s: *const u8, sub: *const u8) -> *const u8 {
    if s.is_null() || sub.is_null() {
        *errno() = EINVAL;
        return std::ptr::null();
    }
    let needle = std::ffi::CStr::from_ptr(sub.cast()).to_bytes();
    let mut p = s;
    loop {
        if std::ffi::CStr::from_ptr(p.cast())
            .to_bytes()
            .starts_with(needle)
        {
            return p;
        }
        if *p == 0 {
            return std::ptr::null();
        }
        p = p.add(char_len(cp, p));
    }
}

/// Longueur en octets du début de `s` dont les caractères sont (`inside`)
/// ou ne sont pas dans `set`.
unsafe fn span_in(cp: u32, s: *const u8, set: *const u8, inside: bool) -> usize {
    if s.is_null() || set.is_null() {
        *errno() = EINVAL;
        return 0;
    }
    let mut p = s;
    while *p != 0 {
        let (v, n) = char_at(cp, p);
        if in_set(cp, set, v) != inside {
            break;
        }
        p = p.add(n);
    }
    p.offset_from(s) as usize
}
unsafe extern "win64" fn mbscspn(s: *const u8, set: *const u8) -> usize {
    span_in(codepage(), s, set, false)
}
unsafe extern "win64" fn mbsspn(s: *const u8, set: *const u8) -> usize {
    span_in(codepage(), s, set, true)
}

/// `_mbsrev(str)` : caractères dans l'ordre inverse, chaque caractère
/// double gardant ses deux octets dans l'ordre.
unsafe extern "win64" fn mbsrev(s: *mut u8) -> *mut u8 {
    mbsrev_in(codepage(), s)
}
unsafe fn mbsrev_in(cp: u32, s: *mut u8) -> *mut u8 {
    if s.is_null() {
        *errno() = EINVAL;
        return s;
    }
    let len = c_len(s);
    let mut chars = Vec::new();
    let mut i = 0;
    while i < len {
        let n = char_len(cp, s.add(i));
        chars.push(std::slice::from_raw_parts(s.add(i), n).to_vec());
        i += n;
    }
    let out: Vec<u8> = chars.into_iter().rev().flatten().collect();
    std::ptr::copy_nonoverlapping(out.as_ptr(), s, out.len());
    s
}

type CaseS = unsafe extern "win64" fn(*mut u8, usize) -> i32;
unsafe extern "win64" fn mbslwr(s: *mut u8) -> *mut u8 {
    if s.is_null() {
        *errno() = EINVAL;
        return s;
    }
    base::<CaseS>("_mbslwr_s")(s, c_len(s) + 1);
    s
}
unsafe extern "win64" fn mbsupr(s: *mut u8) -> *mut u8 {
    if s.is_null() {
        *errno() = EINVAL;
        return s;
    }
    base::<CaseS>("_mbsupr_s")(s, c_len(s) + 1);
    s
}

unsafe extern "win64" fn mbsset(s: *mut u8, c: u32) -> *mut u8 {
    if s.is_null() {
        *errno() = EINVAL;
        return s;
    }
    base::<unsafe extern "win64" fn(*mut u8, usize, u32) -> i32>("_mbsset_s")(s, c_len(s) + 1, c);
    s
}
type NSetS = unsafe extern "win64" fn(*mut u8, usize, u32, usize) -> i32;
unsafe extern "win64" fn mbsnset(s: *mut u8, c: u32, count: usize) -> *mut u8 {
    if s.is_null() {
        *errno() = EINVAL;
        return s;
    }
    base::<NSetS>("_mbsnset_s")(s, c_len(s) + 1, c, count);
    s
}
unsafe extern "win64" fn mbsnbset(s: *mut u8, c: u32, count: usize) -> *mut u8 {
    if s.is_null() {
        *errno() = EINVAL;
        return s;
    }
    base::<NSetS>("_mbsnbset_s")(s, c_len(s) + 1, c, count);
    s
}

thread_local! {
    /// Position de `_mbstok` entre deux appels, propre au fil.
    static MBSTOK: std::cell::Cell<*mut u8> = const { std::cell::Cell::new(std::ptr::null_mut()) };
}
unsafe extern "win64" fn mbstok(s: *mut u8, delims: *const u8) -> *mut u8 {
    let tok =
        base::<unsafe extern "win64" fn(*mut u8, *const u8, *mut *mut u8) -> *mut u8>("_mbstok_s");
    MBSTOK.with(|ctx| {
        let mut c = ctx.get();
        let r = tok(s, delims, &mut c);
        ctx.set(c);
        r
    })
}

/// `_mbsncpy(dest, src, count)` : au plus `count` caractères ; si `src` est
/// plus court, la suite est complétée de zéros jusqu'à `count` octets nuls
/// (lecture de Learn, comme `strncpy`) ; pas de zéro ajouté sinon.
unsafe extern "win64" fn mbsncpy(dst: *mut u8, src: *const u8, count: usize) -> *mut u8 {
    mbsncpy_in(codepage(), dst, src, count)
}
unsafe fn mbsncpy_in(cp: u32, dst: *mut u8, src: *const u8, count: usize) -> *mut u8 {
    if count == 0 {
        return dst;
    }
    if dst.is_null() || src.is_null() {
        *errno() = EINVAL;
        return std::ptr::null_mut();
    }
    let (mut i, mut k) = (0usize, 0usize);
    while k < count && *src.add(i) != 0 {
        let n = char_len(cp, src.add(i));
        std::ptr::copy_nonoverlapping(src.add(i), dst.add(i), n);
        i += n;
        k += 1;
    }
    while k < count {
        *dst.add(i) = 0;
        i += 1;
        k += 1;
    }
    dst
}

/// `_mbsncat(dest, src, count)` : ajoute au plus `count` caractères, puis
/// un zéro.
unsafe extern "win64" fn mbsncat(dst: *mut u8, src: *const u8, count: usize) -> *mut u8 {
    mbsncat_in(codepage(), dst, src, count, false)
}
/// `_mbsnbcat(dest, src, count)` : ajoute au plus `count` octets sans
/// couper un caractère double, puis un zéro.
unsafe extern "win64" fn mbsnbcat(dst: *mut u8, src: *const u8, count: usize) -> *mut u8 {
    mbsncat_in(codepage(), dst, src, count, true)
}
unsafe fn mbsncat_in(cp: u32, dst: *mut u8, src: *const u8, count: usize, bytes: bool) -> *mut u8 {
    if dst.is_null() || src.is_null() {
        *errno() = EINVAL;
        return std::ptr::null_mut();
    }
    let d = dst.add(c_len(dst));
    let (mut i, mut k) = (0usize, 0usize);
    while *src.add(i) != 0 {
        let n = char_len(cp, src.add(i));
        let used = if bytes { i + n } else { k + 1 };
        if used > count {
            break;
        }
        std::ptr::copy_nonoverlapping(src.add(i), d.add(i), n);
        i += n;
        k += 1;
    }
    *d.add(i) = 0;
    dst
}

pub fn exports() -> Exports {
    crate::exports! {
        "_mbschr" => fn mbschr, "_mbschr_l" => fn mbschr_l,
        "_mbspbrk" => fn mbspbrk, "_mbspbrk_l" => fn mbspbrk_l,
        "_mbsnbcmp" => fn mbsnbcmp, "_mbsnbcmp_l" => fn mbsnbcmp_l,
        "_mbsnbcpy" => fn mbsnbcpy, "_mbsnbcpy_l" => fn mbsnbcpy_l,
        // #2139 ; les variantes `_l` ignorent leur locale, dernier argument
        // (en trop, il ne gêne ni win64 ni cdecl).
        "_mbslen" => fn mbslen, "_mbslen_l" => fn mbslen,
        "_mbsinc" => fn mbsinc, "_mbsinc_l" => fn mbsinc,
        "_mbsdec" => fn mbsdec, "_mbsdec_l" => fn mbsdec,
        "_mbsnextc" => fn mbsnextc, "_mbsnextc_l" => fn mbsnextc,
        "_mbclen" => fn mbclen, "_mbclen_l" => fn mbclen,
        "_mbccpy" => fn mbccpy, "_mbccpy_l" => fn mbccpy,
        "_mbctolower" => fn mbctolower, "_mbctolower_l" => fn mbctolower,
        "_mbctoupper" => fn mbctoupper, "_mbctoupper_l" => fn mbctoupper,
        "_mbsicmp" => fn mbsicmp, "_mbsicmp_l" => fn mbsicmp,
        "_mbsncmp" => fn mbsncmp, "_mbsncmp_l" => fn mbsncmp,
        "_mbsnicmp" => fn mbsnicmp, "_mbsnicmp_l" => fn mbsnicmp,
        "_mbsstr" => fn mbsstr, "_mbsstr_l" => fn mbsstr,
        "_mbscspn" => fn mbscspn, "_mbscspn_l" => fn mbscspn,
        "_mbsspn" => fn mbsspn, "_mbsspn_l" => fn mbsspn,
        "_mbsrev" => fn mbsrev, "_mbsrev_l" => fn mbsrev,
        "_mbslwr" => fn mbslwr, "_mbslwr_l" => fn mbslwr,
        "_mbsupr" => fn mbsupr, "_mbsupr_l" => fn mbsupr,
        "_mbsset" => fn mbsset, "_mbsset_l" => fn mbsset,
        "_mbsnset" => fn mbsnset, "_mbsnset_l" => fn mbsnset,
        "_mbsnbset" => fn mbsnbset, "_mbsnbset_l" => fn mbsnbset,
        "_mbstok" => fn mbstok, "_mbstok_l" => fn mbstok,
        "_mbsncpy" => fn mbsncpy, "_mbsncpy_l" => fn mbsncpy,
        "_mbsncat" => fn mbsncat, "_mbsncat_l" => fn mbsncat,
        "_mbsnbcat" => fn mbsnbcat, "_mbsnbcat_l" => fn mbsnbcat,
    }
}

#[cfg(test)]
mod tests;
