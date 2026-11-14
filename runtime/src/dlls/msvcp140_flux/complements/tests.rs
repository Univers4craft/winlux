//! Tests des compléments de msvcp140 (#2309).

use super::*;

fn serie() -> std::sync::MutexGuard<'static, ()> {
    super::super::tests::serie()
}

/// Un basic_streambuf<char> dont la zone de lecture est `text`.
unsafe fn in_buf(text: &mut [u8]) -> P {
    let sb = alloc_zeroed(SB_SIZE);
    sb_ctor::<u8>(sb);
    let p = text.as_mut_ptr();
    sb_setg::<u8>(sb, p, p, p.add(text.len()));
    sb
}

/// Un basic_streambuf<char> avec une zone de sortie de 64 octets.
unsafe fn out_buf(store: &mut [u8; 64]) -> P {
    let sb = alloc_zeroed(SB_SIZE);
    sb_ctor::<u8>(sb);
    let p = store.as_mut_ptr();
    sb_setp2::<u8>(sb, p, p.add(store.len()));
    sb
}

unsafe fn written(sb: P, store: &[u8; 64]) -> String {
    let n = sb_pptr::<u8>(sb).offset_from(store.as_ptr()) as usize;
    String::from_utf8_lossy(&store[..n]).into_owned()
}

unsafe fn new_istream(sb: P) -> P {
    let is = alloc_zeroed(120);
    is_ctor::<u8>(is, sb, false, 1);
    is
}

unsafe fn new_ostream(sb: P) -> P {
    let os = alloc_zeroed(112);
    os_ctor::<u8>(os, sb, false, 1);
    os
}

unsafe fn state(s: P) -> i32 {
    rd(vbase(s), IOS_STATE)
}

#[test]
fn get_peek_ignore() {
    let _serie = serie();
    unsafe {
        let mut text = *b"ab\ncd;ef";
        let is = new_istream(in_buf(&mut text));
        assert_eq!(is_peek::<u8>(is), b'a' as i32);
        assert_eq!(is_gcount::<u8>(is), 0);
        assert_eq!(is_get::<u8>(is), b'a' as i32);
        assert_eq!(is_gcount::<u8>(is), 1);
        let mut buf = [0xFFu8; 8];
        is_get_str::<u8>(is, buf.as_mut_ptr(), 8);
        // Arrêt avant '\n', qui reste dans le flux.
        assert_eq!(&buf[..2], b"b\0");
        assert_eq!(is_gcount::<u8>(is), 1);
        assert_eq!(is_peek::<u8>(is), b'\n' as i32);
        is_ignore::<u8>(is, i64::MAX, b';' as i32);
        assert_eq!(is_gcount::<u8>(is), 4);
        assert_eq!(is_get::<u8>(is), b'e' as i32);
        is_ignore::<u8>(is, 5, -1);
        assert_eq!(is_gcount::<u8>(is), 1);
        assert_eq!(state(is), EOFBIT);
        assert_eq!(is_get::<u8>(is), -1);
        assert_eq!(state(is) & FAILBIT, FAILBIT);
    }
}

#[test]
fn get_chaine_vide_met_failbit() {
    let _serie = serie();
    unsafe {
        let mut text = *b"\nx";
        let is = new_istream(in_buf(&mut text));
        let mut buf = [0xFFu8; 4];
        is_get_str::<u8>(is, buf.as_mut_ptr(), 4);
        assert_eq!(buf[0], 0);
        assert_eq!(state(is), FAILBIT);
    }
}

#[test]
fn extraction_de_reels() {
    let _serie = serie();
    unsafe {
        let mut text = *b"  -1.5e2 0.25x .";
        let is = new_istream(in_buf(&mut text));
        let mut d = 0f64;
        let mut f = 0f32;
        is_double::<u8>(is, &mut d);
        assert_eq!(d, -150.0);
        is_float::<u8>(is, &mut f);
        assert_eq!(f, 0.25);
        assert_eq!(state(is), GOODBIT);
        assert_eq!(is_get::<u8>(is), b'x' as i32);
        is_double::<u8>(is, &mut d);
        assert_eq!(d, 0.0);
        assert_eq!(state(is) & FAILBIT, FAILBIT);
    }
}

#[test]
fn extraction_de_booleens() {
    let _serie = serie();
    unsafe {
        let mut text = *b"1 0 7";
        let is = new_istream(in_buf(&mut text));
        let mut v = false;
        is_bool::<u8>(is, &mut v);
        assert!(v);
        is_bool::<u8>(is, &mut v);
        assert!(!v);
        assert_eq!(state(is), GOODBIT);
        is_bool::<u8>(is, &mut v);
        assert!(v, "ni 0 ni 1 : true et failbit");
        assert_eq!(state(is) & FAILBIT, FAILBIT);

        let mut text = *b"true false tru";
        let is = new_istream(in_buf(&mut text));
        ios_setf(vbase(is), BOOLALPHA);
        is_bool::<u8>(is, &mut v);
        assert!(v);
        is_bool::<u8>(is, &mut v);
        assert!(!v);
        assert_eq!(state(is), GOODBIT);
        is_bool::<u8>(is, &mut v);
        assert!(!v);
        assert_eq!(state(is), EOFBIT | FAILBIT);
    }
}

#[test]
fn insertions_short_float_et_tampon() {
    let _serie = serie();
    unsafe {
        let mut store = [0u8; 64];
        let sb = out_buf(&mut store);
        let os = new_ostream(sb);
        let b = vbase(os);
        os_short::<u8>(os, -2);
        os_put::<u8>(os, b' ');
        ios_setf2(b, HEX, BASEFIELD);
        os_short::<u8>(os, -2);
        ios_setf2(b, DEC, BASEFIELD);
        os_put::<u8>(os, b' ');
        os_float::<u8>(os, 0.5);
        os_put::<u8>(os, b' ');
        let mut src = *b"copie";
        os_streambuf::<u8>(os, in_buf(&mut src));
        assert_eq!(state(os), GOODBIT);
        assert_eq!(written(sb, &store), "-2 fffe 0.5 copie");
        // Source vide : failbit ; source nulle : badbit.
        let mut vide = [0u8; 0];
        os_streambuf::<u8>(os, in_buf(&mut vide));
        assert_eq!(state(os), FAILBIT);
        bios_clear::<u8>(b, GOODBIT, false);
        os_streambuf::<u8>(os, std::ptr::null_mut());
        assert_eq!(state(os), BADBIT);
    }
}

#[test]
fn seekp_et_tellp_sans_position() {
    let _serie = serie();
    unsafe {
        let mut store = [0u8; 64];
        let os = new_ostream(out_buf(&mut store));
        let mut pos = Fpos::at(5);
        os_tellp::<u8>(os, &mut pos);
        assert_eq!(pos.offset(), -1);
        os_seekp_off::<u8>(os, 0, 0);
        assert_eq!(state(os), FAILBIT);
    }
}

#[test]
fn swap_et_affectation_de_tampons() {
    let _serie = serie();
    unsafe {
        let mut t1 = *b"un";
        let mut t2 = *b"deux";
        let (a, b) = (in_buf(&mut t1), in_buf(&mut t2));
        sb_swap::<u8>(a, b);
        assert_eq!(sb_eback::<u8>(a), t2.as_mut_ptr());
        assert_eq!(sb_egptr::<u8>(b), t1.as_mut_ptr().add(2));
        sb_assign::<u8>(b, a);
        assert_eq!(sb_gptr::<u8>(b), t2.as_mut_ptr());
    }
}

#[test]
fn swap_de_flux_et_copyfmt() {
    let _serie = serie();
    unsafe {
        let mut t1 = *b"a";
        let mut t2 = *b"b";
        let (s1, s2) = (in_buf(&mut t1), in_buf(&mut t2));
        let (i1, i2) = (new_istream(s1), new_istream(s2));
        ios_setf2(vbase(i1), HEX, BASEFIELD);
        is_get::<u8>(i1);
        is_swap::<u8>(i1, i2);
        // Le tampon reste ; formats et gcount passent.
        assert_eq!(bios_rdbuf::<u8>(vbase(i1)), s1);
        assert_eq!(ios_flags(vbase(i2)) & BASEFIELD, HEX);
        assert_eq!(is_gcount::<u8>(i2), 1);
        assert_eq!(is_gcount::<u8>(i1), 0);

        let mut store = [0u8; 64];
        let os = new_ostream(out_buf(&mut store));
        let b = vbase(os);
        bios_set_fill::<u8>(b, b'#');
        wr(b, IOS_PREC, 3i64);
        let n = findarr(b, 4);
        wr(n, ARR_LO, 9i32);
        bios_copyfmt::<u8>(vbase(i1), b);
        let c = vbase(i1);
        assert_eq!(bios_fill::<u8>(c), b'#');
        assert_eq!(rd::<i64>(c, IOS_PREC), 3);
        assert_eq!(rd::<i32>(findarr(c, 4), ARR_LO), 9);
        assert_ne!(findarr(c, 4), n, "nœud copié, pas partagé");

        let old = bios_set_rdbuf::<u8>(c, std::ptr::null_mut());
        assert_eq!(old, s1);
        assert_eq!(rd::<i32>(c, IOS_STATE), BADBIT);
    }
}

#[test]
fn ios_construit_sur_un_tampon() {
    let _serie = serie();
    unsafe {
        let mut t = *b"x";
        let sb = in_buf(&mut t);
        let ios = alloc_zeroed(BIOS_SIZE);
        bios_ctor_sb::<u8>(ios, sb);
        assert_eq!(bios_rdbuf::<u8>(ios), sb);
        assert_eq!(rd::<i32>(ios, IOS_STATE), GOODBIT);
        assert_eq!(bios_fill::<u8>(ios), b' ');
        ios_base_dtor(ios);
        free(ios);
    }
}

#[test]
fn identifiant_de_locale_call_once_et_dtest() {
    let id = AtomicUsize::new(0);
    let n = unsafe { id_to_size(&id) };
    assert!(n > 0);
    assert_eq!(unsafe { id_to_size(&id) }, n);

    static APPELS: AtomicUsize = AtomicUsize::new(0);
    unsafe extern "win64" fn rappel(_o: usize, pv: usize, _c: usize) -> i32 {
        APPELS.fetch_add(pv, Ordering::Relaxed);
        1
    }
    let flag = Box::new(0usize);
    let f = &*flag as *const usize as usize;
    for _ in 0..3 {
        assert_ne!(
            unsafe { execute_once(f, rappel as *const () as usize, 5) },
            0
        );
    }
    assert_eq!(APPELS.load(Ordering::Relaxed), 5);

    unsafe {
        assert_eq!(dtest(&f64::INFINITY), 1);
        assert_eq!(dtest(&f64::NAN), 2);
        assert_eq!(dtest(&1.0), -1);
        assert_eq!(dtest(&f64::MIN_POSITIVE.mul_add(0.5, 0.0)), -2);
        assert_eq!(dtest(&0.0), 0);
    }
    let e = exports();
    assert_eq!(e.len(), 33);
    assert!(e.contains_key("?_Raise_handler@std@@3P6AXAEBVexception@stdext@@@ZEA"));
}
