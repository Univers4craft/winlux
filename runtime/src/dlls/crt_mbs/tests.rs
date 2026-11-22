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

/// #2139 : longueur, pas en avant et en arrière, valeur d'un caractère.
#[test]
fn mbslen_mbsdec_en_page_932() {
    unsafe {
        let s = b"a\x95\x5Cb\0";
        let p = s.as_ptr();
        assert_eq!(mbslen_in(SJIS, p), 3);
        assert_eq!(mbslen_in(ANSI, p), 4);
        assert_eq!(mbsdec_in(SJIS, p, p.add(3)), p.add(1));
        assert_eq!(mbsdec_in(SJIS, p, p.add(1)), p);
        assert!(mbsdec_in(SJIS, p, p).is_null());
        assert_eq!(mbsdec_in(ANSI, p, p.add(3)), p.add(2));
    }
}

/// #2139 : comparaisons par caractère, avec ou sans casse.
#[test]
fn mbsncmp_et_mbsicmp() {
    unsafe {
        let a = b"Ab\x95\x5Cz\0".as_ptr();
        let b = b"aB\x95\x5Cy\0".as_ptr();
        assert_eq!(mbsncmp_in(SJIS, a, b, 3, true), 0);
        assert_eq!(mbsncmp_in(SJIS, a, b, 4, true), 1);
        assert_eq!(mbsncmp_in(SJIS, a, b, 1, false), -1);
        assert_eq!(mbsicmp(b"ABC\0".as_ptr(), b"abc\0".as_ptr()), 0);
        assert_eq!(mbsncmp_in(SJIS, a, std::ptr::null(), 1, false), NLSCMPERROR);
    }
}

/// #2139 : recherche alignée sur les caractères, empans, inversion.
#[test]
fn mbsstr_mbsspn_mbsrev() {
    unsafe {
        // En 932, le « \ » de « 表 » (0x95 0x5C) n'est pas une occurrence.
        let s = b"\x95\x5C\\x\0";
        let p = s.as_ptr();
        assert_eq!(mbsstr_in(SJIS, p, b"\\x\0".as_ptr()), p.add(2));
        assert_eq!(mbsstr_in(ANSI, p, b"\\\0".as_ptr()), p.add(1));
        assert!(mbsstr_in(SJIS, p, b"zz\0".as_ptr()).is_null());
        assert_eq!(mbsstr_in(SJIS, p, b"\0".as_ptr()), p);
        assert_eq!(span_in(SJIS, p, b"\\\0".as_ptr(), false), 2);
        assert_eq!(span_in(SJIS, b"aab\0".as_ptr(), b"a\0".as_ptr(), true), 2);
        let mut r = *b"a\x95\x5Cb\0";
        mbsrev_in(SJIS, r.as_mut_ptr());
        assert_eq!(&r, b"b\x95\x5Ca\0");
    }
}

/// #2139 : copies et ajouts bornés, sans couper un caractère double.
#[test]
fn mbsncpy_mbsncat_mbsnbcat() {
    unsafe {
        let mut d = [0xFFu8; 8];
        mbsncpy_in(SJIS, d.as_mut_ptr(), b"\x95\x5Cab\0".as_ptr(), 2);
        assert_eq!(&d[..4], b"\x95\x5Ca\xFF");
        mbsncpy_in(SJIS, d.as_mut_ptr(), b"z\0".as_ptr(), 3);
        assert_eq!(&d[..4], b"z\0\0\xFF");
        let mut c = [0u8; 8];
        c[0] = b'x';
        mbsncat_in(SJIS, c.as_mut_ptr(), b"\x95\x5Cab\0".as_ptr(), 2, false);
        assert_eq!(&c[..5], b"x\x95\x5Ca\0");
        let mut c = [0u8; 8];
        c[0] = b'x';
        mbsncat_in(SJIS, c.as_mut_ptr(), b"a\x95\x5C\0".as_ptr(), 2, true);
        assert_eq!(&c[..3], b"xa\0");
    }
}

/// #2139 : casse, remplissage et découpage passent par les `*_s`.
#[test]
fn mbslwr_mbsset_mbstok() {
    unsafe {
        let mut s = *b"AbC\0";
        assert_eq!(mbslwr(s.as_mut_ptr()), s.as_mut_ptr());
        assert_eq!(&s, b"abc\0");
        mbsupr(s.as_mut_ptr());
        assert_eq!(&s, b"ABC\0");
        mbsnset(s.as_mut_ptr(), b'z' as u32, 2);
        assert_eq!(&s, b"zzC\0");
        mbsset(s.as_mut_ptr(), b'q' as u32);
        assert_eq!(&s, b"qqq\0");
        let mut t = *b"a,b\0";
        let first = mbstok(t.as_mut_ptr(), b",\0".as_ptr());
        assert_eq!(*first, b'a');
        let second = mbstok(std::ptr::null_mut(), b",\0".as_ptr());
        assert_eq!(*second, b'b');
        assert!(mbstok(std::ptr::null_mut(), b",\0".as_ptr()).is_null());
        assert_eq!(mbctoupper(b'a' as u32), b'A' as u32);
        assert_eq!(mbctolower(0x955C), 0x955C);
    }
}

/// #2815 : bornes des plages de Learn, en page 932 et hors de la page 932.
#[test]
fn classification_ismbb_ismbc() {
    use Classe::*;
    let oui = |k, c| classe_in(SJIS, k, c);
    // Katakana d'un octet.
    assert!(oui(BKana, 0xA1) && oui(BKana, 0xDF) && !oui(BKana, 0xA0) && !oui(BKana, 0xE0));
    assert!(oui(BKPunct, 0xA5) && !oui(BKPunct, 0xA6));
    assert!(oui(BKAlnum, 0xA6) && oui(BAlpha, 0xDF) && oui(BAlnum, b'7' as u32));
    assert!(oui(BPrint, 0x20) && !oui(BGraph, 0x20) && oui(BGraph, 0xB1));
    assert!(oui(BPunct, b'!' as u32) && !oui(BPunct, b'a' as u32));
    // Octets de queue : 0x40–0x7E, 0x80–0xFC.
    assert!(oui(BTrail, 0x40) && oui(BTrail, 0x7E) && !oui(BTrail, 0x7F));
    assert!(oui(BTrail, 0x80) && oui(BTrail, 0xFC) && !oui(BTrail, 0xFD));
    // Caractères doubles.
    assert!(oui(CHira, 0x829F) && oui(CHira, 0x82F1) && !oui(CHira, 0x82F2));
    assert!(oui(CKata, 0x8340) && oui(CKata, 0x8396) && !oui(CKata, 0x8397));
    assert!(oui(CL0, 0x889E) && oui(CL1, 0x889F) && oui(CL1, 0x9872));
    assert!(oui(CL2, 0x989F) && oui(CL2, 0xEAA4) && !oui(CL2, 0xEAA5));
    assert!(oui(CSymbol, 0x8141) && oui(CSymbol, 0x81AC) && !oui(CSymbol, 0x81AD));
    assert!(oui(CLegal, 0x955C) && !oui(CLegal, 0x957F) && !oui(CLegal, 0xA05C));
    // Un octet : la routine _ismbb ; un caractère double : non.
    assert!(oui(CAlpha, 0xB1) && !oui(CAlpha, 0x8260));
    assert!(oui(CDigit, b'0' as u32) && !oui(CDigit, 0x824F));
    assert!(oui(CSpace, 0x0D) && oui(CUpper, b'Z' as u32) && oui(CLower, b'a' as u32));
    // Hors page 932 : rien de japonais.
    assert!(!classe_in(ANSI, BKana, 0xB1) && !classe_in(ANSI, CHira, 0x82A0));
    assert!(!classe_in(ANSI, BTrail, 0x5C) && !classe_in(ANSI, CLegal, 0x955C));
    assert!(classe_in(ANSI, BAlpha, b'q' as u32) && !classe_in(ANSI, BAlpha, 0xE9));
}
