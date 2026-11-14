//! Tests de #2128 : succès et chaque cas d'erreur documenté.

use super::*;
use std::sync::atomic::{AtomicU32, Ordering};

const SJIS: u32 = 932;
const LATIN: u32 = 1252;

fn last_errno() -> i32 {
    unsafe { *errno() }
}
fn reset_errno() {
    unsafe { *errno() = 0 };
}

unsafe fn text<'a>(p: *const u8) -> &'a [u8] {
    std::ffi::CStr::from_ptr(p.cast()).to_bytes()
}

/// Tampon de `n` octets marqués, pour voir ce qui est écrit.
fn buf(n: usize) -> Vec<u8> {
    vec![0xAA; n]
}

/// Tous les noms sont servis par msvcrt (donc msvcr80…120) et ucrtbase.
#[test]
fn servis_par_toutes_les_crt() {
    for n in NAMES {
        assert!(super::super::lookup("msvcrt", n).is_some(), "msvcrt!{n}");
        assert!(
            super::super::lookup("ucrtbase", n).is_some(),
            "ucrtbase!{n}"
        );
    }
}

static CALLS: AtomicU32 = AtomicU32::new(0);
extern "win64" fn handler(_: *const u16, _: *const u16, _: *const u16, _: u32, _: usize) {
    CALLS.fetch_add(1, Ordering::SeqCst);
}

/// Le gestionnaire installé par `_set_invalid_parameter_handler` est
/// appelé, puis la fonction rend EINVAL.
#[test]
fn gestionnaire_de_parametre_invalide() {
    let set: extern "win64" fn(usize) -> usize = unsafe {
        std::mem::transmute(
            super::super::lookup("msvcrt", "_set_invalid_parameter_handler").unwrap(),
        )
    };
    let old = set(handler as *const () as usize);
    let before = CALLS.load(Ordering::SeqCst);
    let r = unsafe { mbscat_s(std::ptr::null_mut(), 4, c"a".as_ptr().cast()) };
    let after = CALLS.load(Ordering::SeqCst);
    set(old);
    assert_eq!(r, EINVAL);
    assert_eq!(last_errno(), EINVAL);
    assert!(after > before, "gestionnaire non appelé");
}

// ---------------------------------------------------------------------
// _mbscat_s
// ---------------------------------------------------------------------

#[test]
fn mbscat_s_cas_documentes() {
    unsafe {
        let mut b = buf(8);
        b[..3].copy_from_slice(b"ab\0");
        assert_eq!(mbscat_s(b.as_mut_ptr(), 8, c"cde".as_ptr().cast()), 0);
        assert_eq!(text(b.as_ptr()), b"abcde");
        // Destination nulle : EINVAL.
        assert_eq!(
            mbscat_s(std::ptr::null_mut(), 8, c"x".as_ptr().cast()),
            EINVAL
        );
        // Destination non terminée : EINVAL, laissée telle quelle.
        let mut full = [b'z'; 4];
        assert_eq!(mbscat_s(full.as_mut_ptr(), 4, c"x".as_ptr().cast()), EINVAL);
        assert_eq!(full, [b'z'; 4]);
        // Source nulle : EINVAL, destination vidée.
        assert_eq!(mbscat_s(b.as_mut_ptr(), 8, std::ptr::null()), EINVAL);
        assert_eq!(b[0], 0);
        // Trop petit : ERANGE, destination vidée.
        b[..3].copy_from_slice(b"ab\0");
        reset_errno();
        assert_eq!(
            mbscat_s(b.as_mut_ptr(), 8, c"cdefgh".as_ptr().cast()),
            ERANGE
        );
        assert_eq!((b[0], last_errno()), (0, ERANGE));
        // Taille nulle : EINVAL.
        assert_eq!(mbscat_s(b.as_mut_ptr(), 0, c"x".as_ptr().cast()), EINVAL);
    }
}

// ---------------------------------------------------------------------
// _mbsncpy_s et _mbsnbcpy_s
// ---------------------------------------------------------------------

#[test]
fn mbsncpy_s_cas_documentes() {
    unsafe {
        let mut b = buf(5);
        assert_eq!(
            ncpy(LATIN, b.as_mut_ptr(), 5, c"abcdef".as_ptr().cast(), 4, true),
            0
        );
        assert_eq!(text(b.as_ptr()), b"abcd");
        // count plus grand que la source : pas de remplissage par des zéros.
        let mut b = buf(8);
        assert_eq!(
            ncpy(LATIN, b.as_mut_ptr(), 8, c"ab".as_ptr().cast(), 6, true),
            0
        );
        assert_eq!(&b[..4], b"ab\0\xAA");
        // Exemple de la documentation : 5 caractères dans 5 octets → ERANGE.
        let mut b = buf(5);
        assert_eq!(
            ncpy(
                LATIN,
                b.as_mut_ptr(),
                5,
                c"a long string".as_ptr().cast(),
                5,
                true
            ),
            ERANGE
        );
        assert_eq!(b[0], 0);
        // _TRUNCATE : ce qui tient, STRUNCATE.
        let mut b = buf(5);
        assert_eq!(
            ncpy(
                LATIN,
                b.as_mut_ptr(),
                5,
                c"a long string".as_ptr().cast(),
                TRUNCATE,
                true
            ),
            STRUNCATE
        );
        assert_eq!(text(b.as_ptr()), b"a lo");
        // Destination nulle, taille nulle : EINVAL sans écriture.
        assert_eq!(
            ncpy(
                LATIN,
                std::ptr::null_mut(),
                5,
                c"a".as_ptr().cast(),
                1,
                true
            ),
            EINVAL
        );
        let mut b = buf(5);
        assert_eq!(
            ncpy(LATIN, b.as_mut_ptr(), 0, c"a".as_ptr().cast(), 1, true),
            EINVAL
        );
        assert_eq!(b[0], 0xAA);
        // Source nulle : EINVAL, destination vidée.
        assert_eq!(
            ncpy(LATIN, b.as_mut_ptr(), 5, std::ptr::null(), 1, true),
            EINVAL
        );
        assert_eq!(b[0], 0);
        // Fonctions exportées.
        let mut b = buf(5);
        assert_eq!(mbsncpy_s(b.as_mut_ptr(), 5, c"xyz".as_ptr().cast(), 2), 0);
        assert_eq!(text(b.as_ptr()), b"xy");
        assert_eq!(mbsnbcpy_s(b.as_mut_ptr(), 5, c"xyz".as_ptr().cast(), 3), 0);
        assert_eq!(text(b.as_ptr()), b"xyz");
    }
}

/// En page 932, `count` compte des caractères pour _mbsncpy_s et des octets
/// pour _mbsnbcpy_s ; _TRUNCATE ne coupe pas un caractère en deux.
#[test]
fn mbsncpy_s_double_octet() {
    // « a », un caractère sur deux octets (0x82 0xA0), « b ».
    let src = [b'a', 0x82, 0xA0, b'b', 0];
    unsafe {
        let mut b = buf(8);
        assert_eq!(ncpy(SJIS, b.as_mut_ptr(), 8, src.as_ptr(), 2, true), 0);
        assert_eq!(text(b.as_ptr()), &[b'a', 0x82, 0xA0]);
        assert_eq!(ncpy(SJIS, b.as_mut_ptr(), 8, src.as_ptr(), 2, false), 0);
        assert_eq!(text(b.as_ptr()), &[b'a', 0x82]);
        let mut b = buf(3);
        assert_eq!(
            ncpy(SJIS, b.as_mut_ptr(), 3, src.as_ptr(), TRUNCATE, true),
            STRUNCATE
        );
        assert_eq!(text(b.as_ptr()), b"a");
    }
}

// ---------------------------------------------------------------------
// _mbsncat_s et _mbsnbcat_s
// ---------------------------------------------------------------------

#[test]
fn mbsncat_s_cas_documentes() {
    unsafe {
        // Exemple de la documentation : "12" + 3 de "34567" dans 5 octets.
        let mut b = buf(5);
        b[..3].copy_from_slice(b"12\0");
        assert_eq!(
            mbsncat_s(b.as_mut_ptr(), 5, c"34567".as_ptr().cast(), 3),
            ERANGE
        );
        assert_eq!(b[0], 0);
        b[..3].copy_from_slice(b"12\0");
        assert_eq!(
            mbsncat_s(b.as_mut_ptr(), 5, c"34567".as_ptr().cast(), TRUNCATE),
            STRUNCATE
        );
        assert_eq!(text(b.as_ptr()), b"1234");
        b[..3].copy_from_slice(b"12\0");
        assert_eq!(mbsncat_s(b.as_mut_ptr(), 5, c"34567".as_ptr().cast(), 2), 0);
        assert_eq!(text(b.as_ptr()), b"1234");
        // Source nulle : EINVAL sans modification.
        b[..3].copy_from_slice(b"12\0");
        assert_eq!(mbsncat_s(b.as_mut_ptr(), 5, std::ptr::null(), 1), EINVAL);
        assert_eq!(text(b.as_ptr()), b"12");
        // Destination nulle, non terminée, taille nulle : EINVAL.
        assert_eq!(
            mbsncat_s(std::ptr::null_mut(), 5, c"a".as_ptr().cast(), 1),
            EINVAL
        );
        let mut full = [b'z'; 3];
        assert_eq!(
            mbsncat_s(full.as_mut_ptr(), 3, c"a".as_ptr().cast(), 1),
            EINVAL
        );
        assert_eq!(full, [b'z'; 3]);
        assert_eq!(
            mbsncat_s(b.as_mut_ptr(), 0, c"a".as_ptr().cast(), 1),
            EINVAL
        );
        // _mbsnbcat_s : count en octets.
        b[..2].copy_from_slice(b"x\0");
        assert_eq!(
            mbsnbcat_s(b.as_mut_ptr(), 5, c"abcdef".as_ptr().cast(), 2),
            0
        );
        assert_eq!(text(b.as_ptr()), b"xab");
    }
}

/// _mbsnbcat_s : un octet de tête orphelin en fin de destination est
/// écrasé par la source (documentation).
#[test]
fn mbsnbcat_s_ecrase_un_octet_de_tete_orphelin() {
    let mut b = [b'a', 0x82, 0, 0xAA, 0xAA, 0xAA];
    unsafe {
        assert_eq!(
            ncat(SJIS, b.as_mut_ptr(), 6, c"bc".as_ptr().cast(), 2, false),
            0
        );
        assert_eq!(text(b.as_ptr()), b"abc");
    }
}

// ---------------------------------------------------------------------
// _mbsset_s, _mbsnset_s, _mbsnbset_s
// ---------------------------------------------------------------------

#[test]
fn mbsset_s_cas_documentes() {
    unsafe {
        let mut b = *b"abcd\0\xAA";
        assert_eq!(mbsset_s(b.as_mut_ptr(), 6, b'*' as u32), 0);
        assert_eq!(&b, b"****\0\xAA");
        // Nul, taille nulle, non terminé : EINVAL.
        assert_eq!(mbsset_s(std::ptr::null_mut(), 6, b'*' as u32), EINVAL);
        assert_eq!(mbsset_s(b.as_mut_ptr(), 0, b'*' as u32), EINVAL);
        let mut full = *b"abcd";
        assert_eq!(mbsset_s(full.as_mut_ptr(), 4, b'*' as u32), EINVAL);
        assert_eq!(&full, b"abcd");

        let mut b = *b"abcd\0";
        assert_eq!(mbsnset_s(b.as_mut_ptr(), 5, b'-' as u32, 2), 0);
        assert_eq!(&b, b"--cd\0");
        // count plus grand que la chaîne : la longueur de la chaîne.
        assert_eq!(mbsnset_s(b.as_mut_ptr(), 5, b'+' as u32, 99), 0);
        assert_eq!(&b, b"++++\0");
        assert_eq!(mbsnset_s(std::ptr::null_mut(), 5, b'+' as u32, 1), EINVAL);

        let mut b = *b"abcd\0";
        assert_eq!(mbsnbset_s(b.as_mut_ptr(), 5, b'#' as u32, 3), 0);
        assert_eq!(&b, b"###d\0");
        // count nul : EINVAL.
        assert_eq!(mbsnbset_s(b.as_mut_ptr(), 5, b'#' as u32, 0), EINVAL);
        assert_eq!(mbsnbset_s(std::ptr::null_mut(), 5, b'#' as u32, 1), EINVAL);
    }
}

/// Caractère sur deux octets : octet impair final complété par une espace ;
/// caractère invalide remplacé par une espace avec errno EINVAL.
#[test]
fn mbsset_s_double_octet() {
    unsafe {
        let mut b = *b"abc\0";
        assert_eq!(set(SJIS, b.as_mut_ptr(), 4, 0x82A0, None), 0);
        assert_eq!(&b, &[0x82, 0xA0, b' ', 0]);
        // count en caractères : « a » et « b », soit deux octets.
        let mut b = *b"abcd\0";
        assert_eq!(set(SJIS, b.as_mut_ptr(), 5, 0x82A0, Some((2, true))), 0);
        assert_eq!(&b, &[0x82, 0xA0, b'c', b'd', 0]);
        // Un seul octet à remplir : espace.
        let mut b = *b"abcd\0";
        assert_eq!(set(SJIS, b.as_mut_ptr(), 5, 0x82A0, Some((1, false))), 0);
        assert_eq!(&b, b" bcd\0");
        reset_errno();
        let mut b = *b"ab\0";
        // 0x41 n'est pas un octet de tête : caractère invalide.
        assert_eq!(set(SJIS, b.as_mut_ptr(), 3, 0x4142, None), 0);
        assert_eq!((&b, last_errno()), (b"  \0", EINVAL));
    }
}

// ---------------------------------------------------------------------
// _mbslwr_s, _mbsupr_s
// ---------------------------------------------------------------------

#[test]
fn mbslwr_s_et_mbsupr_s() {
    unsafe {
        let mut b = *b"AbC1\0";
        assert_eq!(mbslwr_s(b.as_mut_ptr(), 5), 0);
        assert_eq!(&b, b"abc1\0");
        assert_eq!(mbsupr_s(b.as_mut_ptr(), 5), 0);
        assert_eq!(&b, b"ABC1\0");
        // Nul, non terminé dans la taille donnée : EINVAL.
        assert_eq!(mbslwr_s(std::ptr::null_mut(), 5), EINVAL);
        assert_eq!(mbsupr_s(b.as_mut_ptr(), 4), EINVAL);
        assert_eq!(&b, b"ABC1\0");
        // L'octet de queue d'un caractère double n'est pas une lettre.
        let mut b = [0x82, b'a', b'a', 0];
        assert_eq!(map_case(SJIS, b.as_mut_ptr(), 4, true), 0);
        assert_eq!(&b, &[0x82, b'a', b'A', 0]);
    }
}

// ---------------------------------------------------------------------
// _mbstok_s
// ---------------------------------------------------------------------

#[test]
fn mbstok_s_cas_documentes() {
    unsafe {
        let mut s = *b",,un, deux,,trois\0";
        let mut ctx: *mut u8 = std::ptr::null_mut();
        let d = c", ".as_ptr().cast();
        let t = mbstok_s(s.as_mut_ptr(), d, &mut ctx);
        assert_eq!(text(t), b"un");
        let t = mbstok_s(std::ptr::null_mut(), d, &mut ctx);
        assert_eq!(text(t), b"deux");
        let t = mbstok_s(std::ptr::null_mut(), d, &mut ctx);
        assert_eq!(text(t), b"trois");
        assert!(mbstok_s(std::ptr::null_mut(), d, &mut ctx).is_null());
        // Erreurs : délimiteurs nuls, contexte nul, str et *context nuls.
        reset_errno();
        let mut ctx: *mut u8 = std::ptr::null_mut();
        assert!(mbstok_s(std::ptr::null_mut(), d, &mut ctx).is_null());
        assert_eq!(last_errno(), EINVAL);
        reset_errno();
        assert!(mbstok_s(s.as_mut_ptr(), std::ptr::null(), &mut ctx).is_null());
        assert_eq!(last_errno(), EINVAL);
        reset_errno();
        assert!(mbstok_s(s.as_mut_ptr(), d, std::ptr::null_mut()).is_null());
        assert_eq!(last_errno(), EINVAL);
    }
}

/// En page 932, l'octet de queue d'un caractère double n'est jamais pris
/// pour un délimiteur.
#[test]
fn mbstok_s_double_octet() {
    // 0x83 0x5C : caractère double dont l'octet de queue vaut « \ ».
    let mut s = [b'a', 0x83, 0x5C, b'\\', b'b', 0];
    let mut ctx: *mut u8 = std::ptr::null_mut();
    unsafe {
        let t = mbstok(SJIS, s.as_mut_ptr(), c"\\".as_ptr().cast(), &mut ctx);
        assert_eq!(text(t), &[b'a', 0x83, 0x5C]);
        let t = mbstok(SJIS, std::ptr::null_mut(), c"\\".as_ptr().cast(), &mut ctx);
        assert_eq!(text(t), b"b");
    }
}

// ---------------------------------------------------------------------
// _mbccpy_s
// ---------------------------------------------------------------------

#[test]
fn mbccpy_s_table_de_la_documentation() {
    let mut d = [0xAAu8; 4];
    let mut n = -1;
    unsafe {
        // Octet simple.
        assert_eq!(
            mbccpy(SJIS, d.as_mut_ptr(), 4, &mut n, c"xy".as_ptr().cast()),
            0
        );
        assert_eq!((d[0], d[1], n), (b'x', 0xAA, 1));
        // Zéro : copié comme un octet simple.
        assert_eq!(
            mbccpy(SJIS, d.as_mut_ptr(), 4, &mut n, c"".as_ptr().cast()),
            0
        );
        assert_eq!((d[0], n), (0, 1));
        // Octet de tête suivi d'un octet non nul : deux octets.
        let src = [0x82u8, 0xA0, 0];
        assert_eq!(mbccpy(SJIS, d.as_mut_ptr(), 4, &mut n, src.as_ptr()), 0);
        assert_eq!((d[0], d[1], n), (0x82, 0xA0, 2));
        // Octet de tête suivi de zéro : 0 copié, EILSEQ.
        let src = [0x82u8, 0];
        assert_eq!(
            mbccpy(SJIS, d.as_mut_ptr(), 4, &mut n, src.as_ptr()),
            EILSEQ
        );
        assert_eq!((d[0], n, last_errno()), (0, 1, EILSEQ));
        // pCopied nul : ignoré.
        assert_eq!(
            mbccpy(
                SJIS,
                d.as_mut_ptr(),
                4,
                std::ptr::null_mut(),
                c"q".as_ptr().cast()
            ),
            0
        );
        // Pointeurs nuls, tampon trop petit : EINVAL.
        assert_eq!(
            mbccpy_s(std::ptr::null_mut(), 4, &mut n, c"q".as_ptr().cast()),
            EINVAL
        );
        assert_eq!(
            mbccpy_s(d.as_mut_ptr(), 4, &mut n, std::ptr::null()),
            EINVAL
        );
        let src = [0x82u8, 0xA0, 0];
        assert_eq!(
            mbccpy(SJIS, d.as_mut_ptr(), 1, &mut n, src.as_ptr()),
            EINVAL
        );
        assert_eq!(
            mbccpy(SJIS, d.as_mut_ptr(), 0, &mut n, c"q".as_ptr().cast()),
            EINVAL
        );
    }
}

// ---------------------------------------------------------------------
// _mbscpy_s_l
// ---------------------------------------------------------------------

#[test]
fn mbscpy_s_l_cas_documentes() {
    unsafe {
        let mut b = buf(4);
        assert_eq!(mbscpy_s_l(b.as_mut_ptr(), 4, c"abc".as_ptr().cast()), 0);
        assert_eq!(text(b.as_ptr()), b"abc");
        assert_eq!(
            mbscpy_s_l(b.as_mut_ptr(), 4, c"abcd".as_ptr().cast()),
            ERANGE
        );
        assert_eq!(b[0], 0);
        assert_eq!(
            mbscpy_s_l(std::ptr::null_mut(), 4, c"a".as_ptr().cast()),
            EINVAL
        );
        assert_eq!(mbscpy_s_l(b.as_mut_ptr(), 0, c"a".as_ptr().cast()), EINVAL);
        b[0] = b'z';
        assert_eq!(mbscpy_s_l(b.as_mut_ptr(), 4, std::ptr::null()), EINVAL);
        assert_eq!(b[0], 0);
    }
}

// ---------------------------------------------------------------------
// _gcvt_s, _fcvt_s
// ---------------------------------------------------------------------

/// Sorties de l'exemple de la documentation de `_gcvt` (12 chiffres) et
/// de `_gcvt_s` (1.2 sur 5 chiffres).
#[test]
fn gcvt_s_exemples_documentes() {
    let cases: [(f64, &[u8]); 8] = [
        (-1234567890.123, b"-1234567890.12"),
        (-12345678901.23, b"-12345678901.2"),
        (-123456789012.3, b"-123456789012"),
        (-1234567890123.0, b"-1.23456789012e+012"),
        (-12.34567890123, b"-12.3456789012"),
        (-1.234567890123, b"-1.23456789012"),
        (-0.1234567890123, b"-0.123456789012"),
        (-0.01234567890123, b"-1.23456789012e-002"),
    ];
    let mut b = [0u8; CVTBUFSIZE];
    for (v, want) in cases {
        assert_eq!(unsafe { gcvt_s(b.as_mut_ptr(), b.len(), v, 12) }, 0, "{v}");
        assert_eq!(unsafe { text(b.as_ptr()) }, want, "{v}");
    }
    assert_eq!(unsafe { gcvt_s(b.as_mut_ptr(), b.len(), 1.2, 5) }, 0);
    assert_eq!(unsafe { text(b.as_ptr()) }, b"1.2");
}

#[test]
fn gcvt_s_cas_d_erreur() {
    let mut b = [0xAAu8; 8];
    unsafe {
        // Tampon nul, taille nulle, digits >= taille : EINVAL sans écriture.
        assert_eq!(gcvt_s(std::ptr::null_mut(), 8, 1.0, 3), EINVAL);
        assert_eq!(gcvt_s(b.as_mut_ptr(), 0, 1.0, 3), EINVAL);
        assert_eq!(gcvt_s(b.as_mut_ptr(), 8, 1.0, 8), EINVAL);
        assert_eq!(b[0], 0xAA);
        // Notation exponentielle trop longue : ERANGE, chaîne vide.
        assert_eq!(gcvt_s(b.as_mut_ptr(), 8, -1.5e100, 6), ERANGE);
        assert_eq!(b[0], 0);
        // digits + 1 octets suffisent en notation décimale.
        assert_eq!(gcvt_s(b.as_mut_ptr(), 4, 12.0, 3), 0);
        assert_eq!(text(b.as_ptr()), b"12");
    }
}

/// Exemples de la documentation de `_fcvt_s` et `_fcvt`, et la position de
/// la virgule à gauche du premier chiffre.
#[test]
#[allow(clippy::approx_constant)] // 3.1415926535 : exemple de la documentation de _fcvt.
fn fcvt_s_exemples_documentes() {
    let mut b = [0u8; CVTBUFSIZE];
    let (mut dec, mut sign) = (0, 0);
    unsafe {
        assert_eq!(
            fcvt_s(b.as_mut_ptr(), b.len(), 1.2, 5, &mut dec, &mut sign),
            0
        );
        assert_eq!((text(b.as_ptr()), dec, sign), (&b"120000"[..], 1, 0));
        assert_eq!(
            fcvt_s(
                b.as_mut_ptr(),
                b.len(),
                3.1415926535,
                7,
                &mut dec,
                &mut sign
            ),
            0
        );
        assert_eq!((text(b.as_ptr()), dec, sign), (&b"31415927"[..], 1, 0));
        assert_eq!(
            fcvt_s(b.as_mut_ptr(), b.len(), -0.0123, 3, &mut dec, &mut sign),
            0
        );
        assert_eq!((text(b.as_ptr()), dec, sign), (&b"12"[..], -1, 1));
        assert_eq!(
            fcvt_s(b.as_mut_ptr(), b.len(), 9.996, 2, &mut dec, &mut sign),
            0
        );
        assert_eq!((text(b.as_ptr()), dec), (&b"1000"[..], 2));
        assert_eq!(
            fcvt_s(b.as_mut_ptr(), b.len(), 1234.5, -2, &mut dec, &mut sign),
            0
        );
        assert_eq!((text(b.as_ptr()), dec), (&b"12"[..], 4));
    }
}

#[test]
fn fcvt_s_cas_d_erreur() {
    let mut b = [0xAAu8; 4];
    let (mut dec, mut sign) = (7, 7);
    unsafe {
        assert_eq!(
            fcvt_s(std::ptr::null_mut(), 4, 1.0, 1, &mut dec, &mut sign),
            EINVAL
        );
        assert_eq!(
            fcvt_s(b.as_mut_ptr(), 0, 1.0, 1, &mut dec, &mut sign),
            EINVAL
        );
        assert_eq!(
            fcvt_s(b.as_mut_ptr(), 4, 1.0, 1, std::ptr::null_mut(), &mut sign),
            EINVAL
        );
        assert_eq!(
            fcvt_s(b.as_mut_ptr(), 4, 1.0, 1, &mut dec, std::ptr::null_mut()),
            EINVAL
        );
        assert_eq!((b[0], dec, sign), (0xAA, 7, 7));
        // Résultat trop long : ERANGE, chaîne vide.
        assert_eq!(
            fcvt_s(b.as_mut_ptr(), 4, 123.0, 2, &mut dec, &mut sign),
            ERANGE
        );
        assert_eq!(b[0], 0);
    }
}

// ---------------------------------------------------------------------
// _searchenv_s, _wsearchenv_s
// ---------------------------------------------------------------------

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

#[test]
fn searchenv_s_dossier_de_la_variable() {
    let dir = std::env::temp_dir().join(format!("winlux-2128-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("cible-2128.txt"), b"x").unwrap();
    let win = crate::paths::to_windows(&dir);
    let putenv_s: unsafe extern "win64" fn(*const u8, *const u8) -> i32 =
        unsafe { std::mem::transmute(super::super::lookup("msvcrt", "_putenv_s").unwrap()) };
    let var = c"WINLUX_TEST_2128";
    let value = std::ffi::CString::new(format!("C:\\nulle-part-2128;\"{win}\"")).unwrap();
    unsafe {
        assert_eq!(putenv_s(var.as_ptr().cast(), value.as_ptr().cast()), 0);
        let mut out = [0u8; 512];
        let file = c"cible-2128.txt";
        assert_eq!(
            searchenv_s(
                file.as_ptr().cast(),
                var.as_ptr().cast(),
                out.as_mut_ptr(),
                512
            ),
            0
        );
        let got = String::from_utf8_lossy(text(out.as_ptr())).to_string();
        assert!(got.ends_with("\\cible-2128.txt"), "{got}");
        assert!(
            got.eq_ignore_ascii_case(&format!("{win}\\cible-2128.txt")),
            "{got}"
        );
        // Large.
        let mut wout = [0u16; 512];
        let wfile = wide("cible-2128.txt");
        let wvar = wide("WINLUX_TEST_2128");
        assert_eq!(
            wsearchenv_s(wfile.as_ptr(), wvar.as_ptr(), wout.as_mut_ptr(), 512),
            0
        );
        let n = wout.iter().position(|&c| c == 0).unwrap();
        assert_eq!(String::from_utf16_lossy(&wout[..n]), got);
        // Tampon trop court : ERANGE, chaîne vide.
        let mut short = [0xAAu8; 8];
        assert_eq!(
            searchenv_s(
                file.as_ptr().cast(),
                var.as_ptr().cast(),
                short.as_mut_ptr(),
                8
            ),
            ERANGE
        );
        assert_eq!(short[0], 0);
        // Introuvable : ENOENT, chaîne vide.
        out[0] = b'z';
        let absent = c"absent-2128.txt";
        assert_eq!(
            searchenv_s(
                absent.as_ptr().cast(),
                var.as_ptr().cast(),
                out.as_mut_ptr(),
                512
            ),
            ENOENT
        );
        assert_eq!(out[0], 0);
        // Variable absente : 0, tampon inchangé.
        out[0] = b'z';
        let novar = c"WINLUX_TEST_2128_ABSENTE";
        assert_eq!(
            searchenv_s(
                absent.as_ptr().cast(),
                novar.as_ptr().cast(),
                out.as_mut_ptr(),
                512
            ),
            0
        );
        assert_eq!(out[0], b'z');
        // Nom vide : ENOENT.
        assert_eq!(
            searchenv_s(
                c"".as_ptr().cast(),
                var.as_ptr().cast(),
                out.as_mut_ptr(),
                512
            ),
            ENOENT
        );
        // Tampon nul, nom nul, taille nulle : EINVAL.
        assert_eq!(
            searchenv_s(
                file.as_ptr().cast(),
                var.as_ptr().cast(),
                std::ptr::null_mut(),
                512
            ),
            EINVAL
        );
        out[0] = b'z';
        assert_eq!(
            searchenv_s(std::ptr::null(), var.as_ptr().cast(), out.as_mut_ptr(), 512),
            EINVAL
        );
        assert_eq!(
            searchenv_s(
                file.as_ptr().cast(),
                var.as_ptr().cast(),
                out.as_mut_ptr(),
                0
            ),
            EINVAL
        );
        assert_eq!(out[0], b'z');
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------
// tmpfile_s
// ---------------------------------------------------------------------

#[test]
fn tmpfile_s_ouvre_un_fichier_lisible_et_inscriptible() {
    let f = |n: &str| super::super::lookup("msvcrt", n).unwrap();
    unsafe {
        let fwrite: unsafe extern "win64" fn(*const c_void, usize, usize, *mut c_void) -> usize =
            std::mem::transmute(f("fwrite"));
        let fread: unsafe extern "win64" fn(*mut c_void, usize, usize, *mut c_void) -> usize =
            std::mem::transmute(f("fread"));
        let rewind: unsafe extern "win64" fn(*mut c_void) = std::mem::transmute(f("rewind"));
        let fclose: unsafe extern "win64" fn(*mut c_void) -> i32 = std::mem::transmute(f("fclose"));
        let mut fp: *mut c_void = std::ptr::null_mut();
        assert_eq!(tmpfile_s(&mut fp), 0);
        assert!(!fp.is_null());
        // Binaire : « \n » reste un seul octet.
        assert_eq!(fwrite(b"a\nb".as_ptr().cast(), 1, 3, fp), 3);
        rewind(fp);
        let mut back = [0u8; 8];
        assert_eq!(fread(back.as_mut_ptr().cast(), 1, 8, fp), 3);
        assert_eq!(&back[..3], b"a\nb");
        assert_eq!(fclose(fp), 0);
        // Pointeur nul : EINVAL.
        assert_eq!(tmpfile_s(std::ptr::null_mut()), EINVAL);
    }
}
