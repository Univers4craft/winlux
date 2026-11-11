use super::*;
use crate::dlls::comobj::VTable;

/// Objet de test : IUnknown, IPersistStream et IInitializeSpy sur une même
/// table (les emplacements 3 à 6 servent aux deux interfaces), IApartmentShutdown
/// (emplacement 3). Compteurs par emplacement ; jamais libéré.
#[repr(C)]
struct Probe {
    vtbl: *const (),
    refs: AtomicU32,
    calls: [AtomicU32; 8],
    last: AtomicU64,
}

const IID_PROBE: Guid = guid(0x1511_0001, 0x1511, 0x1511, [1, 2, 3, 4, 5, 6, 7, 8]);
const CLSID_PROBE: Guid = guid(0x1511_0002, 0x1511, 0x1511, [1, 2, 3, 4, 5, 6, 7, 8]);

static PROBE_VT: VTable<8> = VTable([
    p_qi as *const (),
    p_add_ref as *const (),
    p_release as *const (),
    p_slot3 as *const (),
    p_slot4 as *const (),
    p_slot5 as *const (),
    p_slot6 as *const (),
    p_slot7 as *const (),
]);

fn probe() -> usize {
    Box::into_raw(Box::new(Probe {
        vtbl: &PROBE_VT as *const _ as *const (),
        refs: AtomicU32::new(1),
        calls: Default::default(),
        last: AtomicU64::new(0),
    })) as usize
}

unsafe fn p(this: usize) -> &'static Probe {
    &*(this as *const Probe)
}

fn refs(this: usize) -> u32 {
    unsafe { p(this).refs.load(Ordering::SeqCst) }
}

fn calls(this: usize, slot: usize) -> u32 {
    unsafe { p(this).calls[slot].load(Ordering::SeqCst) }
}

unsafe extern "win64" fn p_qi(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    let g = *iid;
    if [
        IID_IUNKNOWN,
        IID_PROBE,
        IID_IPERSIST_STREAM,
        IID_IINITIALIZE_SPY,
    ]
    .contains(&g)
    {
        p_add_ref(this);
        *out = this;
        S_OK
    } else {
        *out = 0;
        E_NOINTERFACE
    }
}
unsafe extern "win64" fn p_add_ref(this: usize) -> u32 {
    p(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn p_release(this: usize) -> u32 {
    p(this).refs.fetch_sub(1, Ordering::SeqCst) - 1
}
/// GetClassID / PreInitialize / OnUninitialize.
unsafe extern "win64" fn p_slot3(this: usize, a: usize, _b: u32) -> i32 {
    p(this).calls[3].fetch_add(1, Ordering::SeqCst);
    p(this).last.store(a as u64, Ordering::SeqCst);
    if a > 0x10000 {
        // GetClassID : pointeur de sortie.
        std::ptr::write_unaligned(a as *mut Guid, CLSID_PROBE);
    }
    S_OK
}
/// IsDirty / PostInitialize : rend le HRESULT reçu.
unsafe extern "win64" fn p_slot4(this: usize, hr: i32, _c: u32, _r: u32) -> i32 {
    p(this).calls[4].fetch_add(1, Ordering::SeqCst);
    hr
}
/// Load / PreUninitialize.
unsafe extern "win64" fn p_slot5(this: usize, _a: usize) -> i32 {
    p(this).calls[5].fetch_add(1, Ordering::SeqCst);
    S_OK
}
/// Save / PostUninitialize.
unsafe extern "win64" fn p_slot6(this: usize, stm: usize, _clear: i32) -> i32 {
    p(this).calls[6].fetch_add(1, Ordering::SeqCst);
    if stm > 0x10000 {
        return stream_write(stm, b"donnees");
    }
    S_OK
}
unsafe extern "win64" fn p_slot7(_this: usize) -> i32 {
    E_NOTIMPL
}

unsafe fn new_stream() -> usize {
    let mut s: *mut c_void = std::ptr::null_mut();
    assert_eq!(
        super::super::ole32::create_stream_on_hglobal(0, 1, &mut s),
        S_OK
    );
    s as usize
}

unsafe fn rewind(stm: usize) {
    let seek: unsafe extern "win64" fn(usize, i64, u32, *mut u64) -> i32 =
        std::mem::transmute(slot(stm, 5));
    assert_eq!(seek(stm, 0, 0, std::ptr::null_mut()), S_OK);
}

#[test]
fn marshaling_dans_le_processus() {
    unsafe {
        let obj = probe();
        let stm = new_stream();
        let mut size = 0u32;
        assert_eq!(
            co_get_marshal_size_max(&mut size, &IID_PROBE, obj, 3, 0, 0),
            S_OK
        );
        assert_eq!(size as usize, MARSHAL_SIZE);

        // NORMAL : un seul CoUnmarshalInterface, la référence passe au client.
        assert_eq!(co_marshal_interface(stm, &IID_PROBE, obj, 3, 0, 0), S_OK);
        assert_eq!(refs(obj), 2);
        rewind(stm);
        let mut got = 0usize;
        assert_eq!(co_unmarshal_interface(stm, &IID_IUNKNOWN, &mut got), S_OK);
        assert_eq!(got, obj);
        assert_eq!(
            refs(obj),
            2,
            "référence du bloc rendue, celle du client prise"
        );
        rewind(stm);
        assert_eq!(
            co_unmarshal_interface(stm, &IID_PROBE, &mut got),
            CO_E_OBJNOTCONNECTED
        );
        assert_eq!(got, 0);
        release(obj);

        // TABLESTRONG : plusieurs lectures, puis CoReleaseMarshalData.
        let stm2 = new_stream();
        assert_eq!(co_marshal_interface(stm2, &IID_PROBE, obj, 3, 0, 1), S_OK);
        for _ in 0..2 {
            rewind(stm2);
            assert_eq!(co_unmarshal_interface(stm2, &GUID_NULL, &mut got), S_OK);
            release(got);
        }
        rewind(stm2);
        assert_eq!(co_release_marshal_data(stm2), S_OK);
        assert_eq!(refs(obj), 1);
        rewind(stm2);
        assert_eq!(co_release_marshal_data(stm2), CO_E_OBJNOTCONNECTED);
        release(stm);
        release(stm2);
    }
}

#[test]
fn sauvegarde_et_chargement_dans_un_flux() {
    unsafe {
        let obj = probe();
        let stm = new_stream();
        assert_eq!(ole_save_to_stream(obj, stm), S_OK);
        assert_eq!(calls(obj, 3), 1, "GetClassID");
        assert_eq!(calls(obj, 6), 1, "Save");
        rewind(stm);
        let mut clsid = super::super::ole32::Guid::default();
        assert_eq!(
            super::super::ole32_storage::read_class_stm(stm, &mut clsid),
            S_OK
        );
        let raw: Guid = std::mem::transmute(clsid);
        assert_eq!(raw, CLSID_PROBE);

        // Classe inconnue : échec propre, aucun objet.
        rewind(stm);
        let mut out = 1usize;
        let unknown = ole_load_from_stream(stm, &IID_IUNKNOWN, &mut out);
        assert!(unknown < 0);
        assert_eq!(out, 0);

        // Objet nul : CLSID_NULL seul, que le chargement refuse.
        let empty = new_stream();
        assert_eq!(ole_save_to_stream(0, empty), S_OK);
        rewind(empty);
        assert_eq!(
            ole_load_from_stream(empty, &IID_IUNKNOWN, &mut out),
            REGDB_E_CLASSNOTREG
        );
        release(stm);
        release(empty);
    }
}

#[test]
fn espions_d_initialisation_et_fin_d_appartement() {
    std::thread::spawn(|| unsafe {
        let spy = probe();
        let mut cookie = 0u64;
        assert_eq!(co_register_initialize_spy(spy, &mut cookie), S_OK);
        assert_ne!(cookie, 0);
        assert_eq!(super::super::ole32::co_initialize_ex(0, 2), S_OK);
        assert_eq!((calls(spy, 3), calls(spy, 4)), (1, 1));

        let (mut kind, mut qual) = (9, 9);
        assert_eq!(co_get_apartment_type(&mut kind, &mut qual), S_OK);
        assert_eq!((kind, qual), (APTTYPE_STA, APTTYPEQUALIFIER_NONE));

        let cb = probe();
        let (mut id, mut c) = (0u64, 0usize);
        assert_eq!(
            ro_register_for_apartment_shutdown(cb, &mut id, &mut c),
            S_OK
        );
        let mut id2 = 0u64;
        assert_eq!(ro_get_apartment_identifier(&mut id2), S_OK);
        assert_eq!(id, id2);

        super::super::ole32::co_uninitialize();
        assert_eq!((calls(spy, 5), calls(spy, 6)), (1, 1));
        assert_eq!(calls(cb, 3), 1, "OnUninitialize");
        assert_eq!(p(cb).last.load(Ordering::SeqCst), id);
        assert_eq!(refs(cb), 1);
        assert_eq!(ro_unregister_for_apartment_shutdown(c), E_INVALIDARG);

        assert_eq!(co_revoke_initialize_spy(cookie), S_OK);
        assert_eq!(co_revoke_initialize_spy(cookie), E_INVALIDARG);
        assert_eq!(refs(spy), 1);
        assert_eq!(super::super::ole32::co_initialize_ex(0, 0), S_OK);
        assert_eq!(calls(spy, 3), 1, "espion retiré");
        assert_eq!(co_get_apartment_type(&mut kind, &mut qual), S_OK);
        assert_eq!(kind, APTTYPE_MTA);
        super::super::ole32::co_uninitialize();
    })
    .join()
    .unwrap();
}

#[test]
fn noms_de_flux_de_proprietes() {
    assert_eq!(
        fmtid_to_name(&FMTID_SUMMARY_INFORMATION),
        "\u{5}SummaryInformation"
    );
    assert_eq!(
        fmtid_to_name(&FMTID_USER_DEFINED_PROPERTIES),
        "\u{5}DocumentSummaryInformation"
    );
    let name = fmtid_to_name(&IID_PROBE);
    assert_eq!(name.chars().count(), 27);
    assert_eq!(name_to_fmtid(&name), Some(IID_PROBE));
    assert_eq!(name_to_fmtid(&name.to_uppercase()[1..]), Some(IID_PROBE));
    assert_eq!(
        name_to_fmtid("\u{5}documentsummaryinformation"),
        Some(FMTID_DOC_SUMMARY_INFORMATION)
    );
    assert_eq!(name_to_fmtid("pas un nom"), None);
    unsafe {
        let mut buf = [0u16; 32];
        assert_eq!(fmt_id_to_prop_stg_name(&IID_PROBE, buf.as_mut_ptr()), S_OK);
        let mut back = GUID_NULL;
        assert_eq!(prop_stg_name_to_fmt_id(buf.as_ptr(), &mut back), S_OK);
        assert_eq!(back, IID_PROBE);
        let bad: Vec<u16> = "?".encode_utf16().chain([0]).collect();
        assert_eq!(
            prop_stg_name_to_fmt_id(bad.as_ptr(), &mut back),
            STG_E_INVALIDNAME
        );
    }
}

#[test]
fn windows_runtime_agile_erreur_et_fabriques() {
    unsafe {
        let obj = probe();
        let mut agile = 0usize;
        assert_eq!(ro_get_agile_reference(0, &IID_PROBE, obj, &mut agile), S_OK);
        let mut back = 0usize;
        let resolve: unsafe extern "win64" fn(usize, *const Guid, *mut usize) -> i32 =
            std::mem::transmute(slot(agile, 3));
        assert_eq!(resolve(agile, &IID_IUNKNOWN, &mut back), S_OK);
        assert_eq!(back, obj);
        release(back);
        release(agile);
        assert_eq!(refs(obj), 1);
        let other = guid(1, 2, 3, [0; 8]);
        assert_eq!(
            ro_get_agile_reference(0, &other, obj, &mut agile),
            E_NOINTERFACE
        );

        assert_eq!(set_restricted_error_info(obj), S_OK);
        assert_eq!(take_restricted_error_info(), obj);
        assert_eq!(take_restricted_error_info(), 0);
        release(obj);

        let mut iid = IID_PROBE;
        let mut extra = 5usize;
        assert_eq!(
            ro_get_parameterized_type_instance_iid(0, 0, 0, &mut iid, &mut extra),
            E_NOTIMPL
        );
        assert_eq!((iid, extra), (GUID_NULL, 0));

        unsafe extern "win64" fn getter(_class: usize, out: *mut usize) -> i32 {
            *out = FACTORY.load(Ordering::SeqCst) as usize;
            add_ref(*out);
            S_OK
        }
        static FACTORY: AtomicU64 = AtomicU64::new(0);
        let factory = probe();
        FACTORY.store(factory as u64, Ordering::SeqCst);
        let name: Vec<u16> = "Winlux.Essai.Lot1511".encode_utf16().collect();
        let mut h = 0usize;
        assert_eq!(
            super::super::electron_win32::WindowsCreateString(
                name.as_ptr(),
                name.len() as u32,
                &mut h
            ),
            S_OK
        );
        let mut cookie = 0usize;
        let getters = [getter as *const () as usize];
        assert_eq!(
            ro_register_activation_factories(&h, getters.as_ptr(), 1, &mut cookie),
            S_OK
        );
        let mut out = 0usize;
        let hr =
            registered_activation_factory(h, "Winlux.Essai.Lot1511", IID_PROBE.as_ptr(), &mut out);
        assert_eq!(hr, Some(S_OK));
        assert_eq!(out, factory);
        release(out);
        assert_eq!(refs(factory), 1);
        ro_revoke_activation_factories(cookie);
        assert_eq!(
            registered_activation_factory(h, "Winlux.Essai.Lot1511", IID_PROBE.as_ptr(), &mut out),
            None
        );
    }
}

#[test]
fn annulation_contexte_et_horloge() {
    std::thread::spawn(|| unsafe {
        assert_eq!(co_disable_call_cancellation(0), CO_E_CANCEL_DISABLED);
        assert_eq!(co_enable_call_cancellation(0), S_OK);
        assert_eq!(co_disable_call_cancellation(0), S_OK);
        let mut out = 1usize;
        assert_eq!(
            co_get_call_context(&IID_IUNKNOWN, &mut out),
            RPC_E_CALL_COMPLETE
        );
        assert_eq!(out, 0);
        out = 1;
        assert_eq!(
            co_get_cancel_object(0, &IID_IUNKNOWN, &mut out),
            RPC_E_CALL_COMPLETE
        );
        assert_eq!(out, 0);
        let obj = probe();
        assert_eq!(co_set_cancel_object(obj), S_OK);
        assert_eq!(refs(obj), 2);
        assert_eq!(co_set_cancel_object(0), S_OK);
        assert_eq!(refs(obj), 1);

        let mut t = 0u64;
        assert_eq!(co_file_time_now(&mut t), S_OK);
        // Après 2020 (132 223 104 000 000 000), en FILETIME.
        assert!(t > 132_223_104_000_000_000);
        assert_eq!(co_get_current_process(), std::process::id());
    })
    .join()
    .unwrap();
}
