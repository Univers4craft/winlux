//! Tests des fonctions multioctets (#2218), en page 1252 et en page 932
//! (où « 表 » s'écrit 0x95 0x5C : son octet de queue est un « \ »).

// Chaînes d'octets avec zéro explicite : certaines ne sont pas de l'UTF-8.
#![allow(clippy::manual_c_str_literals)]

use super::*;

const SJIS: u32 = 932;
const ANSI: u32 = 1252;

#[test]
fn mbschr_saute_les_octets_de_queue() {
    unsafe {
        let s = b"a\x95\x5Cb\\c\0";
        let p = s.as_ptr();
        assert_eq!(mbschr_in(SJIS, p, b'\\' as u32), p.add(4));
        assert_eq!(mbschr_in(SJIS, p, 0x955C), p.add(1));
        assert_eq!(mbschr_in(SJIS, p, 0), p.add(6));
        assert!(mbschr_in(SJIS, p, b'z' as u32).is_null());
        // En page à un octet, l'octet 0x5C est un « \ » ordinaire.
        assert_eq!(mbschr_in(ANSI, p, b'\\' as u32), p.add(2));
        assert!(mbschr(std::ptr::null(), 1).is_null());
        assert_eq!(*errno(), EINVAL);
    }
}

#[test]
fn mbspbrk_cherche_un_caractere_du_jeu() {
    unsafe {
        let s = b"ab\x95\x5Cc/d\0";
        let p = s.as_ptr();
        assert_eq!(mbspbrk_in(SJIS, p, b"/\\\0".as_ptr()), p.add(5));
        assert_eq!(mbspbrk_in(ANSI, p, b"/\\\0".as_ptr()), p.add(3));
        assert_eq!(mbspbrk_in(SJIS, p, b"\x95\x5C\0".as_ptr()), p.add(2));
        assert_eq!(mbspbrk_in(SJIS, p, b"dc\0".as_ptr()), p.add(4));
        assert!(mbspbrk_in(SJIS, p, b"xyz\0".as_ptr()).is_null());
        assert!(mbspbrk_in(SJIS, p, b"\0".as_ptr()).is_null());
        assert!(mbspbrk(p, std::ptr::null()).is_null());
        assert_eq!(*errno(), EINVAL);
    }
}

#[test]
fn mbsnbcmp_compte_les_octets() {
    unsafe {
        let a = b"ab\x82\xA0x\0".as_ptr();
        let b = b"ab\x82\xA2x\0".as_ptr();
        assert_eq!(mbsnbcmp_in(SJIS, a, b, 2), 0);
        // La limite coupe le caractère double : seul l'octet de tête compte.
        assert_eq!(mbsnbcmp_in(SJIS, a, b, 3), 0);
        assert_eq!(mbsnbcmp_in(SJIS, a, b, 4), -1);
        assert_eq!(mbsnbcmp_in(SJIS, b, a, 10), 1);
        assert_eq!(mbsnbcmp_in(SJIS, a, a, 100), 0);
        assert_eq!(mbsnbcmp_in(ANSI, b"\xE9\0".as_ptr(), b"e\0".as_ptr(), 1), 1);
        assert_eq!(
            mbsnbcmp_in(SJIS, b"ab\0".as_ptr(), b"abc\0".as_ptr(), 3),
            -1
        );
        assert_eq!(mbsnbcmp(a, std::ptr::null(), 0), 0);
        assert_eq!(mbsnbcmp(a, std::ptr::null(), 1), NLSCMPERROR);
    }
}

#[test]
fn mbsnbcpy_complete_et_ne_coupe_pas() {
    unsafe {
        let src = b"a\x82\xA0b\0".as_ptr();
        let mut d = [0xAAu8; 8];
        assert_eq!(mbsnbcpy_in(SJIS, d.as_mut_ptr(), src, 6), d.as_mut_ptr());
        assert_eq!(&d, b"a\x82\xA0b\0\0\xAA\xAA");
        let mut d = [0xAAu8; 4];
        mbsnbcpy_in(SJIS, d.as_mut_ptr(), src, 2);
        assert_eq!(&d, b"a\0\xAA\xAA");
        let mut d = [0xAAu8; 4];
        mbsnbcpy_in(ANSI, d.as_mut_ptr(), src, 2);
        assert_eq!(&d, b"a\x82\xAA\xAA");
        assert!(mbsnbcpy(std::ptr::null_mut(), src, 1).is_null());
        assert_eq!(*errno(), EINVAL);
    }
}

#[test]
fn exports_de_la_famille() {
    let m = exports();
    for n in [
        "_mbschr",
        "_mbspbrk",
        "_mbsnbcmp",
        "_mbsnbcpy",
        "_mbspbrk_l",
    ] {
        assert!(m.contains_key(n), "{n}");
    }
}
