//! Dernières fonctions « sécurisées » `*_s` de msvcrt, msvcr80…120 et
//! ucrtbase qui manquaient après #1980 (#2128).
//!
//! Chacune arrêtait le programme qui l'importe (ENTRYPOINT_NOT_FOUND,
//! 0xC0000139) : les douze copies multioctets `_mbs*_s` et `_mbccpy_s`,
//! leurs variantes `_l`, `_gcvt_s`, `_fcvt_s`, `_searchenv_s`,
//! `_wsearchenv_s` et `tmpfile_s`.
//!
//! Sources (salle blanche) : la liste des noms exportés vient des
//! bibliothèques d'import `libmsvcrt.a`, `libmsvcr100.a`, `libmsvcr120.a`
//! et `libucrtbase.a` de mingw-w64 11.0.1 ; le comportement, de la
//! documentation publique de la CRT (Microsoft Learn, dépôt
//! MicrosoftDocs/cpp-docs, pages `*-s.md` de `c-runtime-library/reference`) :
//! tables « Error conditions », troncature `_TRUNCATE` → `STRUNCATE`,
//! terminateur nul, gestionnaire de paramètre invalide.
//!
//! Paramètre invalide : la CRT appelle le gestionnaire installé par
//! `_set_invalid_parameter_handler` puis, s'il rend la main, pose `errno` et
//! rend le code d'erreur. Sans gestionnaire, Windows termine le programme ;
//! Winlux journalise et continue, comme pour le reste de sa CRT
//! (`_invalid_parameter_noinfo`).
//!
//! Page de code multioctet : celle de l'ANSI du système (`CP_ACP`), comme
//! `_mbsrchr` et `_mbscmp` ; les variantes `_l` ignorent leur locale. Les
//! majuscules et minuscules de `_mbslwr_s`/`_mbsupr_s` ne changent que les
//! lettres ASCII ; les caractères sur deux octets restent tels quels.

use std::ffi::c_void;

use super::crt_runtime::errno;

const ENOENT: i32 = 2;
const EINVAL: i32 = 22;
const ERANGE: i32 = 34;
const EILSEQ: i32 = 42;
const STRUNCATE: i32 = 80;
/// `_TRUNCATE` : `(size_t)-1`.
const TRUNCATE: usize = usize::MAX;
/// `_CVTBUFSIZE` (309 + 40) : tampon suffisant pour toute conversion.
const CVTBUFSIZE: usize = 349;

/// Paramètre invalide : gestionnaire de l'application, puis `errno`.
unsafe fn invalid(e: i32) -> i32 {
    super::crt_extra::invoke_invalid_parameter();
    *errno() = e;
    e
}

// ---------------------------------------------------------------------
// Caractères multioctets
// ---------------------------------------------------------------------

/// Page de code multioctet courante.
fn mbcp() -> u32 {
    crate::intl::codepage::resolve(crate::intl::codepage::CP_ACP)
}

fn is_lead(cp: u32, b: u8) -> bool {
    crate::intl::dbcs::is_lead_byte(cp, b)
}

/// Longueur de `s`, au plus `max` octets.
unsafe fn strnlen(s: *const u8, max: usize) -> usize {
    (0..max).position(|i| *s.add(i) == 0).unwrap_or(max)
}

/// Longueur en octets d'un caractère qui commence en `s[i]` (`s` de
/// longueur `n`) : 2 pour un octet de tête suivi d'un octet non nul.
unsafe fn char_len(cp: u32, s: *const u8, i: usize, n: usize) -> usize {
    if is_lead(cp, *s.add(i)) && i + 1 < n {
        2
    } else {
        1
    }
}

/// `s` (longueur `n`) finit-il par un octet de tête sans octet de queue ?
unsafe fn orphan_lead(cp: u32, s: *const u8, n: usize) -> bool {
    let mut i = 0;
    while i < n {
        if is_lead(cp, *s.add(i)) {
            if i + 1 >= n {
                return true;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    false
}

/// Octets des `count` premiers caractères de `s` (longueur `n`). Un octet
/// de tête sans octet de queue termine la chaîne.
unsafe fn bytes_of_chars(cp: u32, s: *const u8, n: usize, count: usize) -> usize {
    let (mut i, mut c) = (0, 0);
    while i < n && c < count {
        if is_lead(cp, *s.add(i)) && i + 1 >= n {
            break;
        }
        i += char_len(cp, s, i, n);
        c += 1;
    }
    i
}

/// Plus grande coupure `<= max` de `s` (longueur `n`) qui ne sépare pas
/// un caractère sur deux octets.
unsafe fn cut_at(cp: u32, s: *const u8, n: usize, max: usize) -> usize {
    let mut i = 0;
    while i < n {
        let l = char_len(cp, s, i, n);
        if i + l > max {
            break;
        }
        i += l;
    }
    i.min(max)
}

/// Copie bornée commune à `_mbsncpy_s` (`count` en caractères) et
/// `_mbsnbcpy_s` (`count` en octets), règles de `strncpy_s`.
unsafe fn ncpy(
    cp: u32,
    dst: *mut u8,
    size: usize,
    src: *const u8,
    count: usize,
    chars: bool,
) -> i32 {
    if dst.is_null() || size == 0 {
        return invalid(EINVAL);
    }
    if src.is_null() {
        *dst = 0;
        return invalid(EINVAL);
    }
    let n = crate::printf::narrow_len(src);
    let d = if count == TRUNCATE {
        n
    } else if chars {
        bytes_of_chars(cp, src, n, count)
    } else {
        count.min(n)
    };
    if d < size {
        std::ptr::copy(src, dst, d);
        *dst.add(d) = 0;
        return 0;
    }
    if count == TRUNCATE {
        let k = cut_at(cp, src, n, size - 1);
        std::ptr::copy(src, dst, k);
        *dst.add(k) = 0;
        return STRUNCATE;
    }
    *dst = 0;
    invalid(ERANGE)
}

/// Concaténation bornée commune à `_mbscat_s`, `_mbsncat_s` et
/// `_mbsnbcat_s`, règles de `strcat_s`/`strncat_s`.
unsafe fn ncat(
    cp: u32,
    dst: *mut u8,
    size: usize,
    src: *const u8,
    count: usize,
    chars: bool,
) -> i32 {
    if dst.is_null() || size == 0 {
        return invalid(EINVAL);
    }
    let mut used = strnlen(dst, size);
    if used == size {
        // Destination non terminée dans son tampon : laissée telle quelle.
        return invalid(EINVAL);
    }
    if src.is_null() {
        return invalid(EINVAL);
    }
    // Octet de tête orphelin en fin de destination : la source l'écrase.
    if !chars && orphan_lead(cp, dst, used) {
        used -= 1;
    }
    let n = crate::printf::narrow_len(src);
    let d = if count == TRUNCATE {
        n
    } else if chars {
        bytes_of_chars(cp, src, n, count)
    } else {
        count.min(n)
    };
    let room = size - used - 1;
    if d <= room {
        std::ptr::copy(src, dst.add(used), d);
        *dst.add(used + d) = 0;
        return 0;
    }
    if count == TRUNCATE {
        let k = cut_at(cp, src, n, room);
        std::ptr::copy(src, dst.add(used), k);
        *dst.add(used + k) = 0;
        return STRUNCATE;
    }
    *dst = 0;
    invalid(ERANGE)
}

unsafe extern "win64" fn mbscat_s(dst: *mut u8, size: usize, src: *const u8) -> i32 {
    // `_mbscat_s` : `strcat_s`, qui efface aussi la destination quand la
    // source est nulle.
    if !dst.is_null() && size > 0 && src.is_null() && strnlen(dst, size) < size {
        *dst = 0;
    }
    ncat(mbcp(), dst, size, src, TRUNCATE - 1, true)
}
unsafe extern "win64" fn mbsncat_s(dst: *mut u8, size: usize, src: *const u8, count: usize) -> i32 {
    ncat(mbcp(), dst, size, src, count, true)
}
unsafe extern "win64" fn mbsnbcat_s(
    dst: *mut u8,
    size: usize,
    src: *const u8,
    count: usize,
) -> i32 {
    ncat(mbcp(), dst, size, src, count, false)
}
unsafe extern "win64" fn mbsncpy_s(dst: *mut u8, size: usize, src: *const u8, count: usize) -> i32 {
    ncpy(mbcp(), dst, size, src, count, true)
}
unsafe extern "win64" fn mbsnbcpy_s(
    dst: *mut u8,
    size: usize,
    src: *const u8,
    count: usize,
) -> i32 {
    ncpy(mbcp(), dst, size, src, count, false)
}

/// `_mbscpy_s_l` : `_mbscpy_s` (servi par crt_releve) sans sa locale.
unsafe extern "win64" fn mbscpy_s_l(dst: *mut u8, size: usize, src: *const u8) -> i32 {
    if dst.is_null() || size == 0 {
        return invalid(EINVAL);
    }
    if src.is_null() {
        *dst = 0;
        return invalid(EINVAL);
    }
    let n = crate::printf::narrow_len(src);
    if n >= size {
        *dst = 0;
        return invalid(ERANGE);
    }
    std::ptr::copy(src, dst, n + 1);
    0
}

/// Chaîne à modifier sur place : non nulle et terminée dans `size`
/// octets. Rend sa longueur.
unsafe fn in_place(s: *const u8, size: usize) -> Option<usize> {
    if s.is_null() || size == 0 {
        return None;
    }
    let n = strnlen(s, size);
    (n < size).then_some(n)
}

/// Remplit `len` octets de `s` avec le caractère `c` : un octet, ou deux
/// pour un caractère multioctet (dernier octet impair : espace). Un `c`
/// invalide dans la page de code est remplacé par une espace (`EINVAL`
/// dans `errno`, sans gestionnaire).
unsafe fn fill(cp: u32, s: *mut u8, len: usize, c: u32) {
    let (hi, lo) = ((c >> 8) as u8, c as u8);
    let pair = c > 0xff;
    let valid = if pair {
        c <= 0xffff && is_lead(cp, hi) && lo != 0
    } else {
        !is_lead(cp, lo)
    };
    if !valid {
        *errno() = EINVAL;
        std::ptr::write_bytes(s, b' ', len);
        return;
    }
    if !pair {
        std::ptr::write_bytes(s, lo, len);
        return;
    }
    let mut i = 0;
    while i + 1 < len {
        *s.add(i) = hi;
        *s.add(i + 1) = lo;
        i += 2;
    }
    if i < len {
        *s.add(i) = b' ';
    }
}

/// `_mbsset_s` (`count` absent), `_mbsnset_s` (`count` en caractères) et
/// `_mbsnbset_s` (`count` en octets, nul refusé).
unsafe fn set(cp: u32, s: *mut u8, size: usize, c: u32, count: Option<(usize, bool)>) -> i32 {
    if matches!(count, Some((0, false))) {
        return invalid(EINVAL);
    }
    let Some(n) = in_place(s, size) else {
        return invalid(EINVAL);
    };
    let len = match count {
        None => n,
        Some((k, true)) => bytes_of_chars(cp, s, n, k),
        Some((k, false)) => k.min(n),
    };
    fill(cp, s, len, c);
    0
}
unsafe extern "win64" fn mbsset_s(s: *mut u8, size: usize, c: u32) -> i32 {
    set(mbcp(), s, size, c, None)
}
unsafe extern "win64" fn mbsnset_s(s: *mut u8, size: usize, c: u32, count: usize) -> i32 {
    set(mbcp(), s, size, c, Some((count, true)))
}
unsafe extern "win64" fn mbsnbset_s(s: *mut u8, size: usize, c: u32, count: usize) -> i32 {
    set(mbcp(), s, size, c, Some((count, false)))
}

/// Change la casse des lettres ASCII sur un octet, sans toucher aux
/// caractères sur deux octets.
unsafe fn map_case(cp: u32, s: *mut u8, size: usize, upper: bool) -> i32 {
    let Some(n) = in_place(s, size) else {
        return invalid(EINVAL);
    };
    let mut i = 0;
    while i < n {
        let l = char_len(cp, s, i, n);
        if l == 1 {
            let b = &mut *s.add(i);
            *b = if upper {
                b.to_ascii_uppercase()
            } else {
                b.to_ascii_lowercase()
            };
        }
        i += l;
    }
    0
}
unsafe extern "win64" fn mbslwr_s(s: *mut u8, size: usize) -> i32 {
    map_case(mbcp(), s, size, false)
}
unsafe extern "win64" fn mbsupr_s(s: *mut u8, size: usize) -> i32 {
    map_case(mbcp(), s, size, true)
}

/// Caractère (un ou deux octets) qui commence en `s[i]`.
unsafe fn char_at(cp: u32, s: *const u8, i: usize, n: usize) -> (u32, usize) {
    let l = char_len(cp, s, i, n);
    let c = if l == 2 {
        (*s.add(i) as u32) << 8 | *s.add(i + 1) as u32
    } else {
        *s.add(i) as u32
    };
    (c, l)
}

/// `_mbstok_s` : `strtok_s` sur des caractères multioctets.
unsafe extern "win64" fn mbstok_s(s: *mut u8, delims: *const u8, ctx: *mut *mut u8) -> *mut u8 {
    mbstok(mbcp(), s, delims, ctx)
}
unsafe fn mbstok(cp: u32, s: *mut u8, delims: *const u8, ctx: *mut *mut u8) -> *mut u8 {
    if delims.is_null() || ctx.is_null() || (s.is_null() && (*ctx).is_null()) {
        invalid(EINVAL);
        return std::ptr::null_mut();
    }
    let set: Vec<u32> = {
        let n = crate::printf::narrow_len(delims);
        let mut v = Vec::new();
        let mut i = 0;
        while i < n {
            let (c, l) = char_at(cp, delims, i, n);
            v.push(c);
            i += l;
        }
        v
    };
    let s = if s.is_null() { *ctx } else { s };
    let n = crate::printf::narrow_len(s);
    // Octet de tête sans octet de queue : fin de la chaîne.
    let end = if orphan_lead(cp, s, n) { n - 1 } else { n };
    let mut i = 0;
    while i < end {
        let (c, l) = char_at(cp, s, i, end);
        if !set.contains(&c) {
            break;
        }
        i += l;
    }
    if i >= end {
        *ctx = s.add(n);
        return std::ptr::null_mut();
    }
    let start = i;
    while i < end {
        let (c, l) = char_at(cp, s, i, end);
        if set.contains(&c) {
            for k in 0..l {
                *s.add(i + k) = 0;
            }
            *ctx = s.add(i + l);
            return s.add(start);
        }
        i += l;
    }
    *s.add(end) = 0;
    *ctx = s.add(end);
    s.add(start)
}

/// `_mbccpy_s` : copie un caractère (un ou deux octets), sans zéro final.
unsafe extern "win64" fn mbccpy_s(
    dst: *mut u8,
    size: usize,
    copied: *mut i32,
    src: *const u8,
) -> i32 {
    mbccpy(mbcp(), dst, size, copied, src)
}
unsafe fn mbccpy(cp: u32, dst: *mut u8, size: usize, copied: *mut i32, src: *const u8) -> i32 {
    if dst.is_null() || src.is_null() {
        return invalid(EINVAL);
    }
    let set_copied = |n: i32| {
        if !copied.is_null() {
            *copied = n;
        }
    };
    if is_lead(cp, *src) {
        if *src.add(1) == 0 {
            if size == 0 {
                return invalid(EINVAL);
            }
            *dst = 0;
            set_copied(1);
            *errno() = EILSEQ;
            return EILSEQ;
        }
        if size < 2 {
            if size > 0 {
                *dst = 0;
            }
            return invalid(EINVAL);
        }
        *dst = *src;
        *dst.add(1) = *src.add(1);
        set_copied(2);
        return 0;
    }
    if size == 0 {
        return invalid(EINVAL);
    }
    *dst = *src;
    set_copied(1);
    0
}

// ---------------------------------------------------------------------
// Conversions de nombres
// ---------------------------------------------------------------------

/// Texte de `_gcvt` pour une valeur finie : `digits` chiffres significatifs,
/// zéros finals supprimés, notation décimale tant que la virgule tombe entre
/// le premier chiffre (« 0.123… ») et le dernier, sinon exponentielle avec un
/// exposant sur trois chiffres. Les huit sorties de l'exemple de la
/// documentation de `_gcvt` fixent ces bornes : `-0.123456789012` reste
/// décimal, `-0.0123456789012` devient `-1.23456789012e-002`, et
/// `-123456789012` (12 chiffres) reste décimal quand `-1234567890123`
/// passe en exponentielle. Ce n'est donc pas le `%g` de printf.
fn gcvt_text(v: f64, digits: usize) -> String {
    let p = digits.clamp(1, CVTBUFSIZE - 1);
    let s = format!("{:.*e}", p - 1, v.abs());
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let d: String = mant.chars().filter(char::is_ascii_digit).collect();
    let x = exp.parse::<i32>().unwrap_or(0);
    let dec = if v == 0.0 { 1 } else { x + 1 };
    let mut out = String::new();
    if v.is_sign_negative() && v != 0.0 {
        out.push('-');
    }
    let frac = |f: &str| f.trim_end_matches('0').to_string();
    if dec < 0 || dec > p as i32 {
        out.push_str(&d[..1]);
        let f = frac(&d[1..]);
        if !f.is_empty() {
            out.push('.');
            out.push_str(&f);
        }
        out.push_str(&format!("e{}{:03}", if x < 0 { '-' } else { '+' }, x.abs()));
    } else if dec == 0 {
        out.push_str("0.");
        out.push_str(&frac(&d));
    } else {
        let (int, rest) = d.split_at(dec as usize);
        out.push_str(int);
        let f = frac(rest);
        if !f.is_empty() {
            out.push('.');
            out.push_str(&f);
        }
    }
    out
}

/// `_gcvt_s` : conversion de `_gcvt` dans un tampon borné.
unsafe extern "win64" fn gcvt_s(buf: *mut u8, size: usize, value: f64, digits: i32) -> i32 {
    if buf.is_null() || size == 0 || (digits.max(0) as usize) >= size {
        return invalid(EINVAL);
    }
    if !value.is_finite() {
        // Infini et NaN : le texte du printf de la CRT.
        let args: [u64; 2] = [digits.max(0) as u64, value.to_bits()];
        let n = super::crt_ucrt::stdio_common_vsprintf_s(
            0x10,
            buf,
            size,
            c"%.*g".as_ptr().cast(),
            0,
            args.as_ptr(),
        );
        if n < 0 {
            *buf = 0;
            return invalid(ERANGE);
        }
        return 0;
    }
    let t = gcvt_text(value, digits.max(0) as usize);
    if t.len() >= size {
        // La notation exponentielle ne tient pas dans le tampon.
        *buf = 0;
        return invalid(ERANGE);
    }
    std::ptr::copy_nonoverlapping(t.as_ptr(), buf, t.len());
    *buf.add(t.len()) = 0;
    0
}

/// Chiffres de `v` arrondi à `count` chiffres après la virgule (avant la
/// virgule si `count` est négatif), position de la virgule et signe.
fn fcvt_parts(v: f64, count: i32) -> (String, i32, bool) {
    let neg = v.is_sign_negative() && !v.is_nan();
    let a = v.abs();
    if a.is_nan() {
        return ("1#QNAN".into(), 1, neg);
    }
    if a.is_infinite() {
        return ("1#INF".into(), 1, neg);
    }
    let cap = (CVTBUFSIZE - 1) as i32;
    let count = count.min(cap);
    if count >= 0 {
        let s = format!("{a:.*}", count as usize);
        let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
        if int != "0" {
            let mut d = format!("{int}{frac}");
            d.truncate(CVTBUFSIZE - 1);
            return (d, int.len() as i32, neg);
        }
        let zeros = frac.bytes().take_while(|&b| b == b'0').count();
        if zeros == frac.len() {
            // Zéro, ou arrondi à zéro : des zéros, virgule en tête.
            return ("0".repeat(count as usize), 0, neg && a != 0.0);
        }
        return (frac[zeros..].to_string(), -(zeros as i32), neg);
    }
    // Arrondi à gauche de la virgule : `dec + count` chiffres significatifs.
    if a == 0.0 {
        return (String::new(), 0, false);
    }
    let dec = format!("{a:e}")
        .split_once('e')
        .and_then(|(_, e)| e.parse::<i32>().ok())
        .unwrap_or(0)
        + 1;
    let keep = dec + count;
    if keep <= 0 {
        return (String::new(), dec, neg);
    }
    let s = format!("{:.*e}", (keep - 1) as usize, a);
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let mut digits: String = mant.chars().filter(char::is_ascii_digit).collect();
    let dec2 = exp.parse::<i32>().unwrap_or(0) + 1;
    if dec2 > dec {
        // 9,99… arrondi à 10 : un chiffre de plus.
        digits.push('0');
    }
    (digits, dec2, neg)
}

unsafe extern "win64" fn fcvt_s(
    buf: *mut u8,
    size: usize,
    value: f64,
    count: i32,
    dec: *mut i32,
    sign: *mut i32,
) -> i32 {
    if buf.is_null() || size == 0 || dec.is_null() || sign.is_null() {
        return invalid(EINVAL);
    }
    let (d, e, neg) = fcvt_parts(value, count);
    if d.len() >= size {
        *buf = 0;
        return invalid(ERANGE);
    }
    std::ptr::copy_nonoverlapping(d.as_ptr(), buf, d.len());
    *buf.add(d.len()) = 0;
    *dec = e;
    *sign = neg as i32;
    0
}

// ---------------------------------------------------------------------
// Fichiers
// ---------------------------------------------------------------------

/// Unité de chaîne commune aux versions étroite et large.
trait Unit: Copy + Default + PartialEq + 'static {
    const SEP: &'static [Self];
    fn of(b: u8) -> Self;
}
impl Unit for u8 {
    const SEP: &'static [u8] = b";";
    fn of(b: u8) -> u8 {
        b
    }
}
impl Unit for u16 {
    const SEP: &'static [u16] = &[b';' as u16];
    fn of(b: u8) -> u16 {
        b as u16
    }
}

unsafe fn units<'a, T: Unit>(p: *const T) -> &'a [T] {
    let mut n = 0;
    while *p.add(n) != T::default() {
        n += 1;
    }
    std::slice::from_raw_parts(p, n)
}

/// Chemin complet de `name` s'il désigne un fichier ou un dossier existant.
trait Probe: Unit {
    unsafe fn existing(name: &[Self]) -> Option<Vec<Self>>;
    unsafe fn env(var: *const Self) -> Option<Vec<Self>>;
}

impl Probe for u8 {
    unsafe fn existing(name: &[u8]) -> Option<Vec<u8>> {
        let mut z = name.to_vec();
        z.push(0);
        let k = super::kernel32::get_full_path_name_a(
            z.as_ptr(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        if k == 0 {
            return None;
        }
        let mut full = vec![0u8; k as usize + 1];
        let n = super::kernel32::get_full_path_name_a(
            z.as_ptr(),
            full.len() as u32,
            full.as_mut_ptr(),
            std::ptr::null_mut(),
        ) as usize;
        if n == 0 || n >= full.len() {
            return None;
        }
        full.truncate(n);
        full.push(0);
        let found = super::kernel32::get_file_attributes_a(full.as_ptr()) != u32::MAX;
        full.pop();
        found.then_some(full)
    }
    unsafe fn env(var: *const u8) -> Option<Vec<u8>> {
        let v = super::crt_runtime::getenv(var);
        (!v.is_null()).then(|| units(v).to_vec())
    }
}

impl Probe for u16 {
    unsafe fn existing(name: &[u16]) -> Option<Vec<u16>> {
        let mut z = name.to_vec();
        z.push(0);
        let k = super::kernel32::get_full_path_name_w(
            z.as_ptr(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        if k == 0 {
            return None;
        }
        let mut full = vec![0u16; k as usize + 1];
        let n = super::kernel32::get_full_path_name_w(
            z.as_ptr(),
            full.len() as u32,
            full.as_mut_ptr(),
            std::ptr::null_mut(),
        ) as usize;
        if n == 0 || n >= full.len() {
            return None;
        }
        full.truncate(n);
        full.push(0);
        let found = super::kernel32::get_file_attributes_w(full.as_ptr()) != u32::MAX;
        full.pop();
        found.then_some(full)
    }
    unsafe fn env(var: *const u16) -> Option<Vec<u16>> {
        let v = super::crt_runtime::wgetenv(var);
        (!v.is_null()).then(|| units(v).to_vec())
    }
}

/// `_searchenv_s` et `_wsearchenv_s` : le dossier courant, puis chaque
/// dossier de la variable `var` (séparés par `;`, guillemets retirés).
unsafe fn searchenv<T: Probe>(file: *const T, var: *const T, out: *mut T, size: usize) -> i32 {
    if out.is_null() || file.is_null() || size == 0 {
        return invalid(EINVAL);
    }
    let name = units(file);
    if name.is_empty() {
        *out = T::default();
        *errno() = ENOENT;
        return ENOENT;
    }
    let put = |path: &[T]| -> i32 {
        if path.len() >= size {
            *out = T::default();
            return invalid(ERANGE);
        }
        std::ptr::copy_nonoverlapping(path.as_ptr(), out, path.len());
        *out.add(path.len()) = T::default();
        0
    };
    if let Some(p) = T::existing(name) {
        return put(&p);
    }
    // Variable absente : rien n'est écrit (documentation de _searchenv_s).
    let Some(list) = (if var.is_null() { None } else { T::env(var) }) else {
        return 0;
    };
    let quote = T::of(b'"');
    for dir in list.split(|c| T::SEP.contains(c)) {
        let dir: Vec<T> = dir.iter().copied().filter(|&c| c != quote).collect();
        if dir.is_empty() {
            continue;
        }
        let mut cand = dir.clone();
        let last = *cand.last().unwrap_or(&T::default());
        if last != T::of(b'\\') && last != T::of(b'/') {
            cand.push(T::of(b'\\'));
        }
        cand.extend_from_slice(name);
        if let Some(p) = T::existing(&cand) {
            return put(&p);
        }
    }
    *out = T::default();
    *errno() = ENOENT;
    ENOENT
}

unsafe extern "win64" fn searchenv_s(
    file: *const u8,
    var: *const u8,
    out: *mut u8,
    size: usize,
) -> i32 {
    searchenv(file, var, out, size)
}
unsafe extern "win64" fn wsearchenv_s(
    file: *const u16,
    var: *const u16,
    out: *mut u16,
    size: usize,
) -> i32 {
    searchenv(file, var, out, size)
}

/// `tmpfile_s` : fichier temporaire anonyme ouvert en « w+b », effacé à sa
/// fermeture ou à la fin du programme.
unsafe extern "win64" fn tmpfile_s(out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return invalid(EINVAL);
    }
    match super::crt_stdio::open_tmpfile() {
        Ok(f) => {
            *out = f;
            0
        }
        Err(e) => {
            *out = std::ptr::null_mut();
            *errno() = e;
            e
        }
    }
}

pub fn exports() -> std::collections::HashMap<&'static str, usize> {
    // Les variantes `_l` prennent une locale en dernier argument : la même
    // fonction l'ignore (les arguments en trop ne gênent ni win64 ni cdecl).
    crate::exports! {
        "_mbscat_s" => fn mbscat_s, "_mbscat_s_l" => fn mbscat_s,
        "_mbscpy_s_l" => fn mbscpy_s_l,
        "_mbsncat_s" => fn mbsncat_s, "_mbsncat_s_l" => fn mbsncat_s,
        "_mbsnbcat_s" => fn mbsnbcat_s, "_mbsnbcat_s_l" => fn mbsnbcat_s,
        "_mbsncpy_s" => fn mbsncpy_s, "_mbsncpy_s_l" => fn mbsncpy_s,
        "_mbsnbcpy_s" => fn mbsnbcpy_s, "_mbsnbcpy_s_l" => fn mbsnbcpy_s,
        "_mbsset_s" => fn mbsset_s, "_mbsset_s_l" => fn mbsset_s,
        "_mbsnset_s" => fn mbsnset_s, "_mbsnset_s_l" => fn mbsnset_s,
        "_mbsnbset_s" => fn mbsnbset_s, "_mbsnbset_s_l" => fn mbsnbset_s,
        "_mbslwr_s" => fn mbslwr_s, "_mbslwr_s_l" => fn mbslwr_s,
        "_mbsupr_s" => fn mbsupr_s, "_mbsupr_s_l" => fn mbsupr_s,
        "_mbstok_s" => fn mbstok_s, "_mbstok_s_l" => fn mbstok_s,
        "_mbccpy_s" => fn mbccpy_s, "_mbccpy_s_l" => fn mbccpy_s,
        "_gcvt_s" => fn gcvt_s, "_fcvt_s" => fn fcvt_s,
        "_searchenv_s" => fn searchenv_s, "_wsearchenv_s" => fn wsearchenv_s,
        "tmpfile_s" => fn tmpfile_s,
    }
}

/// Noms servis par ce module (tests et campagne).
#[cfg(test)]
const NAMES: &[&str] = &[
    "_mbscat_s",
    "_mbscat_s_l",
    "_mbscpy_s_l",
    "_mbsncat_s",
    "_mbsncat_s_l",
    "_mbsnbcat_s",
    "_mbsnbcat_s_l",
    "_mbsncpy_s",
    "_mbsncpy_s_l",
    "_mbsnbcpy_s",
    "_mbsnbcpy_s_l",
    "_mbsset_s",
    "_mbsset_s_l",
    "_mbsnset_s",
    "_mbsnset_s_l",
    "_mbsnbset_s",
    "_mbsnbset_s_l",
    "_mbslwr_s",
    "_mbslwr_s_l",
    "_mbsupr_s",
    "_mbsupr_s_l",
    "_mbstok_s",
    "_mbstok_s_l",
    "_mbccpy_s",
    "_mbccpy_s_l",
    "_gcvt_s",
    "_fcvt_s",
    "_searchenv_s",
    "_wsearchenv_s",
    "tmpfile_s",
];

#[cfg(test)]
mod tests;
