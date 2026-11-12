//! Tests de l'imitation des flux et des locales de msvcp140 (#2195).

use super::*;

/// Un basic_streambuf<char> avec une zone de sortie de 64 octets.
unsafe fn out_buf(store: &mut [u8; 64]) -> P {
    let sb = alloc_zeroed(SB_SIZE);
    sb_ctor::<u8>(sb);
    let p = store.as_mut_ptr();
    sb_setp2::<u8>(sb, p, p.add(store.len()));
    sb
}

/// Un basic_streambuf<char> dont la zone de lecture est `text`.
unsafe fn in_buf(text: &mut [u8]) -> P {
    let sb = alloc_zeroed(SB_SIZE);
    sb_ctor::<u8>(sb);
    let p = text.as_mut_ptr();
    sb_setg::<u8>(sb, p, p, p.add(text.len()));
    sb
}

unsafe fn written(sb: P, store: &[u8; 64]) -> String {
    let n = sb_pptr::<u8>(sb).offset_from(store.as_ptr()) as usize;
    String::from_utf8_lossy(&store[..n]).into_owned()
}

unsafe fn new_ostream(sb: P) -> P {
    let os = alloc_zeroed(112);
    os_ctor::<u8>(os, sb, false, 1);
    os
}

#[test]
fn ostream_construit_comme_msvc() {
    unsafe {
        let mut store = [0u8; 64];
        let sb = out_buf(&mut store);
        let os = new_ostream(sb);
        // vbptr en tête, base virtuelle à +16, vtordisp nulle à +12.
        assert_eq!(vbase(os), os.add(16));
        assert_eq!(rd::<i32>(os, 12), 0);
        let b = vbase(os);
        assert_eq!(bios_rdbuf::<u8>(b), sb);
        assert_eq!(ios_flags(b), SKIPWS | DEC);
        assert_eq!(rd::<i64>(b, IOS_PREC), 6);
        assert!(ios_good(b));
        // Le destructeur virtuel de la base rend l'objet complet.
        let deleting: unsafe extern "win64" fn(P, u32) -> P = std::mem::transmute(slot(b, 0));
        assert_eq!(deleting(b, 0), os);
        free(os);
    }
}

#[test]
fn ostream_entiers_et_remplissage() {
    unsafe {
        let mut store = [0u8; 64];
        let sb = out_buf(&mut store);
        let os = new_ostream(sb);
        let b = vbase(os);
        os_int::<u8>(os, -42);
        os_put::<u8>(os, b'|');
        ios_setf2(b, HEX, BASEFIELD);
        ios_setf(b, SHOWBASE);
        ios_setf2(b, INTERNAL, ADJUSTFIELD);
        ios_set_width(b, 8);
        bios_set_fill::<u8>(b, b'0');
        os_int::<u8>(os, 255);
        // La largeur ne vaut que pour une insertion.
        assert_eq!(ios_width(b), 0);
        os_put::<u8>(os, b'|');
        ios_setf2(b, DEC, BASEFIELD);
        ios_setf2(b, LEFT, ADJUSTFIELD);
        bios_set_fill::<u8>(b, b'*');
        ios_set_width(b, 5);
        os_uint::<u8>(os, 7);
        os_put::<u8>(os, b'|');
        ios_setf2(b, HEX, BASEFIELD);
        ios_unsetf(b, SHOWBASE);
        os_int::<u8>(os, -1);
        assert!(ios_good(b));
        assert_eq!(written(sb, &store), "-42|0x0000ff|7****|ffffffff");
    }
}

#[test]
fn ostream_reels_et_booleens() {
    unsafe {
        let mut store = [0u8; 64];
        let sb = out_buf(&mut store);
        let os = new_ostream(sb);
        let b = vbase(os);
        os_double::<u8>(os, 0.5);
        os_put::<u8>(os, b' ');
        ios_setf2(b, FIXED, FLOATFIELD);
        wr(b, IOS_PREC, 2i64);
        os_double::<u8>(os, 2.71828);
        os_put::<u8>(os, b' ');
        os_bool::<u8>(os, true);
        ios_setf(b, BOOLALPHA);
        os_put::<u8>(os, b' ');
        os_bool::<u8>(os, false);
        assert_eq!(written(sb, &store), "0.5 2.72 1 false");
    }
}

#[test]
fn ostream_plein_passe_en_badbit() {
    unsafe {
        let mut store = [0u8; 64];
        let sb = out_buf(&mut store);
        let p = store.as_mut_ptr();
        sb_setp2::<u8>(sb, p, p.add(2));
        let os = new_ostream(sb);
        os_int::<u8>(os, 12345);
        assert!(ios_bad(vbase(os)));
    }
}

#[test]
fn istream_lit_des_entiers() {
    unsafe {
        let mut text = *b"  -42\n17 0x1f zz";
        let sb = in_buf(&mut text);
        let is = alloc_zeroed(120);
        is_ctor::<u8>(is, sb, false, 1);
        assert_eq!(vbase(is), is.add(24));
        let b = vbase(is);
        let mut i = 0i32;
        let mut u = 0u32;
        is_int::<u8>(is, &mut i);
        assert_eq!(i, -42);
        is_uint::<u8>(is, &mut u);
        assert_eq!(u, 17);
        ios_setf2(b, 0, BASEFIELD);
        is_int::<u8>(is, &mut i);
        assert_eq!(i, 31);
        assert!(ios_good(b));
        is_int::<u8>(is, &mut i);
        assert!(ios_fail(b));
        assert_eq!(i, 0);
    }
}

#[test]
fn istream_getline_et_gcount() {
    unsafe {
        let mut text = *b"bonjour\nmonde";
        let sb = in_buf(&mut text);
        let is = alloc_zeroed(120);
        is_ctor::<u8>(is, sb, false, 1);
        let mut line = [0u8; 16];
        is_getline::<u8>(is, line.as_mut_ptr(), 16);
        assert_eq!(&line[..8], b"bonjour\0");
        // getline compte le délimiteur extrait.
        assert_eq!(is_gcount::<u8>(is), 8);
        assert!(ios_good(vbase(is)));
    }
}

#[test]
fn iostream_deux_vbptr_une_base() {
    unsafe {
        let mut store = [0u8; 64];
        let sb = out_buf(&mut store);
        let ios = alloc_zeroed(128);
        iostream_ctor::<u8>(ios, sb, 1);
        // Les sous-objets istream (+0) et ostream (+16) trouvent la même base.
        assert_eq!(vbase(ios), ios.add(32));
        assert_eq!(vbase(ios.add(16)), ios.add(32));
        os_int::<u8>(ios.add(16), 9);
        assert_eq!(written(sb, &store), "9");
    }
}

#[test]
fn locale_globale_et_classique() {
    unsafe {
        let g = locale_init(false);
        assert!(!g.is_null());
        assert_eq!(locale_init(false), g);
        assert_eq!(getgloballocale(), g);
        assert_eq!((*locale_classic()).load(Ordering::Acquire) as P, g);
        let before = refs(g).load(Ordering::Acquire);
        let l = new_locale();
        assert_eq!(refs(g).load(Ordering::Acquire), before + 1);
        delete_locale(l);
        assert_eq!(refs(g).load(Ordering::Acquire), before);
        // Les facettes standard sont créées à la demande, une seule fois.
        let f1 = use_own_facet(g, &ID_CTYPE_C, new_ctype_c);
        let f2 = use_own_facet(g, &ID_CTYPE_C, new_ctype_c);
        assert!(!f1.is_null());
        assert_eq!(f1, f2);
        assert_eq!(ctype_c_tolower(f1, b'Q'), b'q');
        assert_eq!(ctype_c_widen(f1, b'a'), b'a');
    }
}

#[test]
fn ids_de_facettes_distincts() {
    let ids = [
        id_index(&ID_CTYPE_C),
        id_index(&ID_CTYPE_W),
        id_index(&ID_CODECVT_C),
        id_index(&ID_CODECVT_W),
        id_index(&ID_COLLATE_C),
        id_index(&ID_TIME_PUT_C),
    ];
    for (i, a) in ids.iter().enumerate() {
        assert!(*a > 0);
        for b in &ids[i + 1..] {
            assert_ne!(a, b);
        }
    }
    assert_eq!(id_index(&ID_CTYPE_C), ids[0]);
}

#[test]
fn setw_et_setprecision() {
    unsafe {
        let mut store = [0u8; 64];
        let sb = out_buf(&mut store);
        let os = new_ostream(sb);
        let b = vbase(os);
        let mut m = [0usize; 2];
        setw(&mut m, 4);
        let f: unsafe extern "win64" fn(P, i64) = std::mem::transmute(m[0]);
        f(b, m[1] as i64);
        os_int::<u8>(os, 7);
        setprecision(&mut m, 3);
        let f: unsafe extern "win64" fn(P, i64) = std::mem::transmute(m[0]);
        f(b, m[1] as i64);
        os_put::<u8>(os, b' ');
        os_double::<u8>(os, 2.0 / 3.0);
        assert_eq!(written(sb, &store), "   7 0.667");
    }
}

#[test]
fn tables_virtuelles_completes() {
    let t = tables();
    let all: [&[usize]; 18] = [
        &t.facet,
        &t.locimp,
        &t.ctype_c,
        &t.ctype_w,
        &t.codecvt_c,
        &t.codecvt_w,
        &t.time_put,
        &t.bios_c,
        &t.bios_w,
        &t.sb_c,
        &t.sb_w,
        &t.console,
        &t.os_c,
        &t.os_w,
        &t.is_c,
        &t.is_w,
        &t.iostream_c,
        &t.iostream_w,
    ];
    for table in all {
        assert!(table.iter().all(|&a| a != 0));
    }
    assert_eq!(t.sb_c.len(), 15);
    assert_eq!(t.ctype_c.len(), 11);
    assert_eq!(t.ctype_w.len(), 15);
    assert_eq!(t.codecvt_w.len(), 10);
}

#[test]
fn exports_du_mur_observe() {
    let m = exports();
    // Le mur d'OBS Studio (#2195) et ses voisins immédiats.
    for name in [
        "??0?$basic_ios@DU?$char_traits@D@std@@@std@@IEAA@XZ",
        "?cout@std@@3V?$basic_ostream@DU?$char_traits@D@std@@@1@A",
        "?_Id_cnt@id@locale@std@@0HA",
    ] {
        assert!(m.get(name).is_some_and(|&a| a != 0), "{name}");
    }
    // Aucun nom ne double un export déjà servi par msvcp140.
    let base = super::super::msvcp140::exports();
    let doubles: Vec<_> = m.keys().filter(|k| base.contains_key(*k)).collect();
    assert!(doubles.is_empty(), "{doubles:?}");
}
