use super::*;
use crate::dlls::ole32_storage as stg;

const CLSID_ESSAI: Guid = Guid {
    data1: 0x1236_0001,
    data2: 0x0e1e,
    data3: 0x4e11,
    data4: [1, 2, 3, 4, 5, 6, 7, 9],
};

/// État observé de l'objet incorporé d'essai.
#[derive(Default, Debug)]
struct Etat {
    site: usize,
    init_new: bool,
    charge: Vec<u8>,
    run: bool,
    locked: bool,
    contained: bool,
    draw: Option<(u32, usize, [i32; 4])>,
}
static ETAT: Mutex<Option<Etat>> = Mutex::new(None);

fn etat<R>(f: impl FnOnce(&mut Etat) -> R) -> R {
    f(ETAT.lock().unwrap().get_or_insert_with(Etat::default))
}

/// Objet d'essai : IUnknown/IOleObject à +0, IPersistStorage à +8,
/// IRunnableObject à +16, IViewObject à +24.
#[repr(C)]
struct Objet {
    ole: *const usize,
    ps: *const usize,
    run: *const usize,
    view: *const usize,
    refs: AtomicU32,
}

unsafe fn base(this: usize, off: usize) -> usize {
    this - off
}
unsafe extern "win64" fn qi_at<const OFF: usize>(
    this: usize,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    let b = base(this, OFF);
    let g = std::ptr::read_unaligned(iid);
    let off = if g == IID_IUNKNOWN || g == IID_IOLEOBJECT {
        0
    } else if g == IID_IPERSISTSTORAGE {
        8
    } else if g == IID_IRUNNABLEOBJECT {
        16
    } else if g == IID_IVIEWOBJECT {
        24
    } else {
        *out = 0;
        return E_NOINTERFACE;
    };
    (*(b as *const Objet)).refs.fetch_add(1, Ordering::AcqRel);
    *out = b + off;
    S_OK
}
unsafe extern "win64" fn add_ref_at<const OFF: usize>(this: usize) -> u32 {
    (*(base(this, OFF) as *const Objet))
        .refs
        .fetch_add(1, Ordering::AcqRel)
        + 1
}
unsafe extern "win64" fn release_at<const OFF: usize>(this: usize) -> u32 {
    let b = base(this, OFF);
    let n = (*(b as *const Objet)).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(b as *mut Objet));
    }
    n
}
unsafe extern "win64" fn ok0(_: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn set_site(_: usize, site: usize) -> i32 {
    etat(|e| e.site = site);
    S_OK
}
unsafe extern "win64" fn get_class(_: usize, out: *mut Guid) -> i32 {
    *out = CLSID_ESSAI;
    S_OK
}
unsafe extern "win64" fn init_new(_: usize, _storage: usize) -> i32 {
    etat(|e| e.init_new = true);
    S_OK
}
unsafe extern "win64" fn load(_: usize, storage: usize) -> i32 {
    let name: Vec<u16> = "Donnees".encode_utf16().chain(Some(0)).collect();
    let open: unsafe extern "win64" fn(usize, *const u16, usize, u32, u32, *mut usize) -> i32 =
        std::mem::transmute(slot(storage, 4));
    let mut s = 0;
    let hr = open(storage, name.as_ptr(), 0, 0x10, 0, &mut s);
    if hr < 0 {
        return hr;
    }
    let read: unsafe extern "win64" fn(usize, *mut u8, u32, *mut u32) -> i32 =
        std::mem::transmute(slot(s, 3));
    let mut b = [0u8; 64];
    let mut n = 0;
    read(s, b.as_mut_ptr(), 64, &mut n);
    release(s);
    etat(|e| e.charge = b[..n as usize].to_vec());
    S_OK
}
unsafe extern "win64" fn save(_: usize, storage: usize, _same: i32) -> i32 {
    let name: Vec<u16> = "Donnees".encode_utf16().chain(Some(0)).collect();
    let create: unsafe extern "win64" fn(usize, *const u16, u32, u32, u32, *mut usize) -> i32 =
        std::mem::transmute(slot(storage, 3));
    let mut s = 0;
    let hr = create(storage, name.as_ptr(), 0x1012, 0, 0, &mut s);
    if hr < 0 {
        return hr;
    }
    let write: unsafe extern "win64" fn(usize, *const u8, u32, *mut u32) -> i32 =
        std::mem::transmute(slot(s, 4));
    let mut n = 0;
    write(s, b"objet incorpore".as_ptr(), 15, &mut n);
    release(s);
    S_OK
}
unsafe extern "win64" fn run(_: usize, _ctx: usize) -> i32 {
    etat(|e| e.run = true);
    S_OK
}
unsafe extern "win64" fn is_running(_: usize) -> i32 {
    etat(|e| e.run) as i32
}
unsafe extern "win64" fn lock_running(_: usize, lock: i32, _close: i32) -> i32 {
    etat(|e| e.locked = lock != 0);
    S_OK
}
unsafe extern "win64" fn set_contained(_: usize, c: i32) -> i32 {
    etat(|e| e.contained = c != 0);
    S_OK
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn draw(
    _: usize,
    aspect: u32,
    _lindex: i32,
    _pv: usize,
    _ptd: usize,
    _target: usize,
    hdc: usize,
    bounds: *const i32,
    _wb: usize,
    _cont: usize,
    _arg: usize,
) -> i32 {
    let r = std::ptr::read_unaligned(bounds as *const [i32; 4]);
    etat(|e| e.draw = Some((aspect, hdc, r)));
    S_OK
}

static OLE_V: Vtbl<24> = {
    let mut v = [ok0 as *const (); 24];
    v[0] = qi_at::<0> as *const ();
    v[1] = add_ref_at::<0> as *const ();
    v[2] = release_at::<0> as *const ();
    v[3] = set_site as *const ();
    Vtbl(v)
};
static PS_V: Vtbl<10> = Vtbl([
    qi_at::<8> as *const (),
    add_ref_at::<8> as *const (),
    release_at::<8> as *const (),
    get_class as *const (),
    ok0 as *const (),
    init_new as *const (),
    load as *const (),
    save as *const (),
    ok0 as *const (),
    ok0 as *const (),
]);
static RUN_V: Vtbl<8> = Vtbl([
    qi_at::<16> as *const (),
    add_ref_at::<16> as *const (),
    release_at::<16> as *const (),
    ok0 as *const (),
    run as *const (),
    is_running as *const (),
    lock_running as *const (),
    set_contained as *const (),
]);
static VIEW_V: Vtbl<9> = {
    let mut v = [ok0 as *const (); 9];
    v[0] = qi_at::<24> as *const ();
    v[1] = add_ref_at::<24> as *const ();
    v[2] = release_at::<24> as *const ();
    v[3] = draw as *const ();
    Vtbl(v)
};

// Fabrique de classe.
unsafe extern "win64" fn cf_qi(this: usize, _iid: *const Guid, out: *mut usize) -> i32 {
    *out = this;
    S_OK
}
unsafe extern "win64" fn cf_ref(_: usize) -> u32 {
    1
}
unsafe extern "win64" fn cf_create(
    _: usize,
    _outer: usize,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    let o = Box::into_raw(Box::new(Objet {
        ole: OLE_V.0.as_ptr().cast(),
        ps: PS_V.0.as_ptr().cast(),
        run: RUN_V.0.as_ptr().cast(),
        view: VIEW_V.0.as_ptr().cast(),
        refs: AtomicU32::new(1),
    })) as usize;
    let hr = qi_at::<0>(o, iid, out);
    release_at::<0>(o);
    hr
}
unsafe extern "win64" fn cf_lock(_: usize, _l: i32) -> i32 {
    S_OK
}
static CF_V: Vtbl<5> = Vtbl([
    cf_qi as *const (),
    cf_ref as *const (),
    cf_ref as *const (),
    cf_create as *const (),
    cf_lock as *const (),
]);

/// Conteneur : OleCreate, OleRun, OleDraw, OleSave puis OleLoad d'un objet
/// incorporé dans un stockage composé en mémoire.
#[test]
fn objet_incorpore_cree_enregistre_et_recharge() {
    unsafe {
        let factory = [CF_V.0.as_ptr() as usize];
        let mut cookie = 0u32;
        assert_eq!(
            super::super::ole32::co_register_class_object(
                &CLSID_ESSAI,
                factory.as_ptr() as usize,
                1,
                1,
                &mut cookie
            ),
            S_OK
        );
        let mut lb = 0;
        assert_eq!(stg::create_ilockbytes_on_hglobal(0, 1, &mut lb), S_OK);
        let mut storage = 0;
        assert_eq!(
            stg::stg_create_docfile_on_ilockbytes(lb, 0x1012, 0, &mut storage),
            S_OK
        );

        let site_factice = 0x5175usize;
        let mut obj = 0usize;
        assert_eq!(
            ole_create(
                &CLSID_ESSAI,
                &IID_IOLEOBJECT,
                1,
                0,
                site_factice,
                storage,
                &mut obj
            ),
            S_OK
        );
        assert_ne!(obj, 0);
        etat(|e| {
            assert_eq!(e.site, site_factice);
            assert!(e.init_new);
        });

        assert_eq!(ole_is_running(obj), 0);
        assert_eq!(ole_run(obj), S_OK);
        assert_eq!(ole_is_running(obj), 1);
        assert_eq!(ole_lock_running(obj, 1, 0), S_OK);
        assert_eq!(ole_set_contained_object(obj, 1), S_OK);
        let rect = [10i32, 20, 110, 70];
        assert_eq!(ole_draw(obj, 1, 0xdc, rect.as_ptr()), S_OK);
        etat(|e| {
            assert!(e.locked && e.contained);
            assert_eq!(e.draw, Some((1, 0xdc, rect)));
        });

        let ps = query(obj, &IID_IPERSISTSTORAGE);
        assert_ne!(ps, 0);
        assert_eq!(ole_save(ps, storage, 1), S_OK);
        release(ps);
        release(obj);
        let mut clsid = Guid::default();
        assert_eq!(stg::read_class_stg(storage, &mut clsid), S_OK);
        assert_eq!(clsid, CLSID_ESSAI);
        let commit: unsafe extern "win64" fn(usize, u32) -> i32 =
            std::mem::transmute(slot(storage, 9));
        assert_eq!(commit(storage, 0), S_OK);
        release(storage);

        // Rechargement depuis les octets du HGLOBAL.
        let mut relu = 0;
        assert_eq!(
            stg::stg_open_storage_on_ilockbytes(lb, 0, 0x10, 0, 0, &mut relu),
            S_OK
        );
        let mut obj2 = 0;
        assert_eq!(ole_load(relu, &IID_IOLEOBJECT, 0x5176, &mut obj2), S_OK);
        etat(|e| {
            assert_eq!(e.charge, b"objet incorpore");
            assert_eq!(e.site, 0x5176);
        });
        release(obj2);
        // Stockage sans CLSID : OLE_E_BLANK.
        let mut lb2 = 0;
        stg::create_ilockbytes_on_hglobal(0, 1, &mut lb2);
        let mut vide = 0;
        stg::stg_create_docfile_on_ilockbytes(lb2, 0x1012, 0, &mut vide);
        let mut x = 0;
        assert_eq!(ole_load(vide, &IID_IOLEOBJECT, 0, &mut x), OLE_E_BLANK);
        release(vide);
        release(lb2);
        release(relu);
        release(lb);
        super::super::ole32::co_revoke_class_object(cookie);
    }
}

// Puits d'avis d'essai : compte les appels.
static AVIS: Mutex<[u32; 8]> = Mutex::new([0; 8]);
unsafe extern "win64" fn sink_qi(this: usize, _iid: *const Guid, out: *mut usize) -> i32 {
    *out = this;
    S_OK
}
unsafe extern "win64" fn sink_ref(_: usize) -> u32 {
    1
}
unsafe extern "win64" fn on_data(_: usize, f: *const u16, m: *const u32) {
    assert_eq!(*f, 7); // cfFormat
    assert_eq!(*m, 0);
    AVIS.lock().unwrap()[3] += 1;
}
unsafe extern "win64" fn on_save(_: usize) {
    AVIS.lock().unwrap()[6] += 1;
}
unsafe extern "win64" fn on_close(_: usize) {
    AVIS.lock().unwrap()[7] += 1;
}
static SINK_V: Vtbl<8> = Vtbl([
    sink_qi as *const (),
    sink_ref as *const (),
    sink_ref as *const (),
    on_data as *const (),
    ok0 as *const (),
    ok0 as *const (),
    on_save as *const (),
    on_close as *const (),
]);
unsafe extern "win64" fn data_get(_: usize, _f: *const u64, m: *mut u32) -> i32 {
    *m = 0; // TYMED_NULL : rien à libérer
    AVIS.lock().unwrap()[5] += 1;
    S_OK
}
static DATA_V: Vtbl<4> = Vtbl([
    sink_qi as *const (),
    sink_ref as *const (),
    sink_ref as *const (),
    data_get as *const (),
]);

/// Porte-avis OLE et de données : Advise, envois, ONLYONCE, NODATA,
/// PRIMEFIRST, Unadvise et EnumAdvise.
#[test]
fn porte_avis_ole_et_donnees() {
    unsafe {
        let sink = [SINK_V.0.as_ptr() as usize];
        let sink = sink.as_ptr() as usize;
        let data = [DATA_V.0.as_ptr() as usize];
        let data = data.as_ptr() as usize;

        let mut h = 0;
        assert_eq!(create_ole_advise_holder(&mut h), S_OK);
        let mut c1 = 0u32;
        assert_eq!(ole_holder_advise(h, sink, &mut c1), S_OK);
        assert_ne!(c1, 0);
        ole_holder_send_on_save(h);
        ole_holder_send_on_close(h);
        let mut e = 0;
        assert_eq!(holder_enum_advise(h, &mut e), S_OK);
        let mut sd = [0u64; 7];
        let mut got = 0;
        assert_eq!(enum_next(e, 1, sd.as_mut_ptr().cast(), &mut got), S_OK);
        assert_eq!((got, sd[5] as usize, sd[6] as u32), (1, sink, c1));
        assert_eq!(enum_next(e, 1, sd.as_mut_ptr().cast(), &mut got), S_FALSE);
        enum_release(e);
        assert_eq!(holder_unadvise(h, c1), S_OK);
        assert_eq!(holder_unadvise(h, c1), OLE_E_NOCONNECTION);
        ole_holder_send_on_save(h);
        holder_release(h);
        {
            let a = AVIS.lock().unwrap();
            assert_eq!((a[6], a[7]), (1, 1));
        }

        let mut d = 0;
        assert_eq!(create_data_advise_holder(&mut d), S_OK);
        let fmt = [7u64, 0, 1 | (u64::MAX << 32), 1]; // cf 7, aspect 1, lindex -1, TYMED_HGLOBAL
        let mut c = 0u32;
        assert_eq!(
            data_holder_advise(d, data, fmt.as_ptr(), ADVF_PRIMEFIRST, sink, &mut c),
            S_OK
        );
        assert_eq!(AVIS.lock().unwrap()[3], 1);
        let mut once = 0u32;
        assert_eq!(
            data_holder_advise(
                d,
                data,
                fmt.as_ptr(),
                ADVF_ONLYONCE | ADVF_NODATA,
                sink,
                &mut once
            ),
            S_OK
        );
        data_holder_send_on_data_change(d, data, 0, 0);
        {
            let a = AVIS.lock().unwrap();
            assert_eq!(a[3], 3);
            assert_eq!(a[5], 2, "pas de GetData pour ADVF_NODATA");
        }
        data_holder_send_on_data_change(d, data, 0, 0);
        assert_eq!(AVIS.lock().unwrap()[3], 4, "ONLYONCE retiré après un envoi");
        assert_eq!(holder_unadvise(d, once), OLE_E_NOCONNECTION);
        assert_eq!(holder_unadvise(d, c), S_OK);
        holder_release(d);
    }
}
