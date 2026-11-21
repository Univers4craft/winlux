use super::*;

fn g(x: &Guid) -> &super::super::ole32::Guid {
    unsafe { &*(x as *const Guid as *const super::super::ole32::Guid) }
}

unsafe fn create(iid: &Guid) -> usize {
    let mut obj: *mut c_void = std::ptr::null_mut();
    assert_eq!(
        create_builtin(g(&CLSID_WEB_BROWSER), g(iid), &mut obj),
        Some(S_OK)
    );
    assert!(!obj.is_null());
    obj as usize
}

unsafe fn qi(obj: usize, iid: &Guid) -> (i32, usize) {
    let f: unsafe extern "win64" fn(usize, *const Guid, *mut usize) -> i32 =
        std::mem::transmute(method(obj, 0));
    let mut out = 0usize;
    let hr = f(obj, iid, &mut out);
    (hr, out)
}

unsafe fn release(obj: usize) -> u32 {
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(method(obj, 2));
    f(obj)
}

#[test]
fn controle_navigateur_cree_par_clsid_et_progid() {
    // #2228 : CLSID_WebBrowser se crée, et « Shell.Explorer » (ProgID
    // demandé par l'installeur WinRAR) comme « Shell.Explorer.2 » y mènent.
    unsafe {
        let wb = create(&IID_IWEB_BROWSER2);
        for iid in [
            IID_IUNKNOWN,
            IID_IDISPATCH,
            IID_IWEB_BROWSER,
            IID_IWEB_BROWSER_APP,
            IID_IOLE_OBJECT,
            IID_IOLE_IN_PLACE_OBJECT,
            IID_IOLE_WINDOW,
            IID_IOLE_IN_PLACE_ACTIVE_OBJECT,
            IID_IVIEW_OBJECT2,
            IID_IPERSIST_STREAM_INIT,
            IID_ICONNECTION_POINT_CONTAINER,
            IID_IOLE_CONTROL,
        ] {
            let (hr, p) = qi(wb, &iid);
            assert_eq!(hr, S_OK, "{}", guid_str(&iid));
            // Toute interface ramène au même IUnknown (règle de COM).
            let (_, unk) = qi(p, &IID_IUNKNOWN);
            assert_eq!(unk, wb);
            release(unk);
            release(p);
        }
        let (hr, _) = qi(wb, &ole_guid(0x0000_0099));
        assert_eq!(hr, E_NOINTERFACE);
        assert_eq!(release(wb), 0);
    }
    let expected: super::super::ole32::Guid = unsafe { std::mem::transmute(CLSID_WEB_BROWSER) };
    assert_eq!(clsid_for_prog_id("Shell.Explorer"), Some(expected));
    assert_eq!(clsid_for_prog_id("shell.explorer.2"), Some(expected));
    assert_eq!(clsid_for_prog_id("Shell.Application"), None);
    // Une autre classe n'est pas prise.
    let mut other: *mut c_void = std::ptr::null_mut();
    assert_eq!(
        unsafe { create_builtin(g(&IID_IWEB_BROWSER2), g(&IID_IUNKNOWN), &mut other) },
        None
    );
}

#[test]
fn navigation_notee_et_methode_absente_marquee() {
    unsafe {
        let wb = create(&IID_IWEB_BROWSER2);
        let url: Vec<u16> = "file:///C:/licence.html\0".encode_utf16().collect();
        let nav: unsafe extern "win64" fn(usize, *const u16) -> i32 =
            std::mem::transmute(method(wb, 11));
        assert_eq!(nav(wb, url.as_ptr()), S_OK);
        assert_eq!(state(from(wb, SLOT_BROWSER)).url, "file:///C:/licence.html");
        // Navigate2 : VARIANT VT_BSTR, la chaîne à +8.
        let url2: Vec<u16> = "about:blank\0".encode_utf16().collect();
        let mut v = super::super::comobj::Aligned::<24>::default();
        v.0[..2].copy_from_slice(&8u16.to_le_bytes());
        v.0[8..16].copy_from_slice(&(url2.as_ptr() as usize).to_le_bytes());
        let nav2: unsafe extern "win64" fn(usize, *const u8) -> i32 =
            std::mem::transmute(method(wb, 52));
        assert_eq!(nav2(wb, v.0.as_ptr()), S_OK);
        assert_eq!(state(from(wb, SLOT_BROWSER)).url, "about:blank");
        // GoBack n'est pas servi : E_NOTIMPL, pas un succès vide.
        let back: unsafe extern "win64" fn(usize) -> i32 = std::mem::transmute(method(wb, 7));
        assert_eq!(back(wb), E_NOTIMPL);
        assert_eq!(release(wb), 0);
    }
}

// Abonné IDispatch minimal pour DWebBrowserEvents2.
static SINK_REFS: AtomicU32 = AtomicU32::new(1);
#[repr(C)]
struct Sink {
    vtbl: *const (),
}
unsafe impl Sync for Sink {}
static SINK_VT: VTable<3> = VTable([
    sink_qi as *const (),
    sink_add_ref as *const (),
    sink_release as *const (),
]);
static SINK: Sink = Sink {
    vtbl: &SINK_VT as *const _ as *const (),
};
unsafe extern "win64" fn sink_qi(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    let g = *iid;
    if g == IID_IUNKNOWN || g == IID_IDISPATCH || g == DIID_DWEB_BROWSER_EVENTS2 {
        SINK_REFS.fetch_add(1, Ordering::SeqCst);
        *out = this;
        S_OK
    } else {
        *out = 0;
        E_NOINTERFACE
    }
}
extern "win64" fn sink_add_ref(_this: usize) -> u32 {
    SINK_REFS.fetch_add(1, Ordering::SeqCst) + 1
}
extern "win64" fn sink_release(_this: usize) -> u32 {
    SINK_REFS.fetch_sub(1, Ordering::SeqCst) - 1
}

#[test]
fn abonnement_aux_evenements_du_navigateur() {
    unsafe {
        let wb = create(&IID_IUNKNOWN);
        let (_, cpc) = qi(wb, &IID_ICONNECTION_POINT_CONTAINER);
        let find: unsafe extern "win64" fn(usize, *const Guid, *mut usize) -> i32 =
            std::mem::transmute(method(cpc, 4));
        let mut cp = 0usize;
        assert_eq!(find(cpc, &DIID_DWEB_BROWSER_EVENTS2, &mut cp), S_OK);
        let mut none = 0usize;
        assert_eq!(
            find(cpc, &IID_IWEB_BROWSER2, &mut none),
            CONNECT_E_NOCONNECTION
        );
        let advise: unsafe extern "win64" fn(usize, usize, *mut u32) -> i32 =
            std::mem::transmute(method(cp, 5));
        let unadvise: unsafe extern "win64" fn(usize, u32) -> i32 =
            std::mem::transmute(method(cp, 6));
        let sink = &SINK as *const Sink as usize;
        let before = SINK_REFS.load(Ordering::SeqCst);
        let mut cookie = 0u32;
        assert_eq!(advise(cp, sink, &mut cookie), S_OK);
        assert_ne!(cookie, 0);
        assert_eq!(SINK_REFS.load(Ordering::SeqCst), before + 1);
        assert_eq!(unadvise(cp, cookie), S_OK);
        assert_eq!(unadvise(cp, cookie), CONNECT_E_NOCONNECTION);
        assert_eq!(SINK_REFS.load(Ordering::SeqCst), before);
        release(cp);
        release(cpc);
        assert_eq!(release(wb), 0);
    }
}

#[test]
fn adresses_de_page_lues() {
    // file: (RFC 8089) : barres retournées et octets %XX décodés.
    assert_eq!(
        file_url_path("///C:/Program%20Files/Adobe/licence.html"),
        "C:\\Program Files\\Adobe\\licence.html"
    );
    assert_eq!(file_url_path("C:/a%C3%A9.htm"), "C:\\aé.htm");
    assert_eq!(strip_scheme("RES://x.dll/a", "res://"), Some("x.dll/a"));
    unsafe {
        assert_eq!(load("about:blank"), Ok(String::new()));
        assert!(load("https://example.com/").is_err());
        // Ressource absente : refus nommé, pas de plantage.
        assert!(load("res://module-absent.dll/licence.htm").is_err());
    }
}

// Abonné qui note les événements reçus par IDispatch::Invoke.
static EVENTS: Mutex<Vec<(i32, usize, String)>> = Mutex::new(Vec::new());
static EVENT_SINK_VT: VTable<7> = VTable([
    sink_qi as *const (),
    sink_add_ref as *const (),
    sink_release as *const (),
    sink_release as *const (), // GetTypeInfoCount (non appelé)
    sink_release as *const (), // GetTypeInfo (non appelé)
    sink_release as *const (), // GetIDsOfNames (non appelé)
    event_invoke as *const (),
]);
static EVENT_SINK: Sink = Sink {
    vtbl: &EVENT_SINK_VT as *const _ as *const (),
};
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn event_invoke(
    _this: usize,
    id: i32,
    _iid: *const Guid,
    _lcid: u32,
    flags: u16,
    params: *const [usize; 3],
    _result: usize,
    _exc: usize,
    _arg_err: usize,
) -> i32 {
    assert_eq!(flags, 1);
    let rgvarg = (*params)[0] as *const [u64; 3];
    let count = ((*params)[2] & 0xffff_ffff) as usize;
    let (mut disp, mut url) = (0usize, String::new());
    if count == 2 {
        // Ordre inverse : [0] = URL (VARIANT* par référence), [1] = pDisp.
        let a0 = &*rgvarg;
        assert_eq!(a0[0] as u16, 0x400c);
        let inner = a0[1] as *const [u64; 3];
        assert_eq!((*inner)[0] as u16, 8);
        url = wide_text((*inner)[1] as *const u16);
        let a1 = &*rgvarg.add(1);
        assert_eq!(a1[0] as u16, 9);
        disp = a1[1] as usize;
    }
    EVENTS.lock().unwrap().push((id, disp, url));
    S_OK
}

#[test]
fn fin_de_navigation_annoncee_aux_abonnes() {
    // #2228 : l'installeur WinRAR navigue vers about:blank puis attend la
    // fin du chargement avant d'écrire son contrat dans la page.
    unsafe {
        let wb = create(&IID_IWEB_BROWSER2);
        let (_, cpc) = qi(wb, &IID_ICONNECTION_POINT_CONTAINER);
        let find: unsafe extern "win64" fn(usize, *const Guid, *mut usize) -> i32 =
            std::mem::transmute(method(cpc, 4));
        let mut cp = 0usize;
        assert_eq!(find(cpc, &DIID_DWEB_BROWSER_EVENTS2, &mut cp), S_OK);
        let advise: unsafe extern "win64" fn(usize, usize, *mut u32) -> i32 =
            std::mem::transmute(method(cp, 5));
        let mut cookie = 0u32;
        let sink = &EVENT_SINK as *const Sink as usize;
        assert_eq!(advise(cp, sink, &mut cookie), S_OK);
        let url: Vec<u16> = "about:blank\0".encode_utf16().collect();
        let nav: unsafe extern "win64" fn(usize, *const u16) -> i32 =
            std::mem::transmute(method(wb, 11));
        assert_eq!(nav(wb, url.as_ptr()), S_OK);
        let got = std::mem::take(&mut *EVENTS.lock().unwrap());
        let ids: Vec<i32> = got.iter().map(|e| e.0).collect();
        assert_eq!(ids, [252, 104, 259]);
        for (id, disp, u) in got.iter().filter(|e| e.0 != 104) {
            assert_eq!(*disp, wb, "pDisp de {id}");
            assert_eq!(u, "about:blank");
        }
        let unadvise: unsafe extern "win64" fn(usize, u32) -> i32 =
            std::mem::transmute(method(cp, 6));
        assert_eq!(unadvise(cp, cookie), S_OK);
        release(cp);
        release(cpc);
        assert_eq!(release(wb), 0);
    }
}

#[test]
fn document_ecrit_par_write_montre_en_texte() {
    // #2228 : l'installeur WinRAR demande le document puis y écrit son
    // contrat en HTML.
    unsafe {
        let wb = create(&IID_IWEB_BROWSER2);
        let get_doc: unsafe extern "win64" fn(usize, *mut usize) -> i32 =
            std::mem::transmute(method(wb, 18));
        let mut doc = 0usize;
        assert_eq!(get_doc(wb, &mut doc), S_OK);
        assert_ne!(doc, 0);
        let mut again = 0usize;
        assert_eq!(get_doc(wb, &mut again), S_OK);
        assert_eq!(again, doc);
        release(again);
        let iid_doc2 = guid(
            0x332C_4425,
            0x26CB,
            0x11D0,
            [0xB4, 0x83, 0x00, 0xC0, 0x4F, 0xD9, 0x01, 0x19],
        );
        let (hr, d2) = qi(doc, &iid_doc2);
        assert_eq!(hr, S_OK);
        assert_eq!(d2, doc);
        release(d2);
        let (hr, psi) = qi(doc, &IID_IPERSIST_STREAM_INIT);
        assert_eq!(hr, S_OK);
        release(psi);
        // write(SAFEARRAY d'un VARIANT VT_BSTR).
        let html: Vec<u16> =
            "<p>Version de d&eacute;monstration</p><p>&laquo;&nbsp;logiciel&nbsp;&raquo;</p>\0"
                .encode_utf16()
                .collect();
        let bstr = super::super::oleaut32::sys_alloc_string(html.as_ptr());
        let mut var = [0u64; 3];
        var[0] = 8; // VT_BSTR
        var[1] = bstr as u64;
        let mut sa = [0u8; 32];
        sa[0..2].copy_from_slice(&1u16.to_le_bytes());
        sa[4..8].copy_from_slice(&24u32.to_le_bytes());
        sa[16..24].copy_from_slice(&(var.as_ptr() as u64).to_le_bytes());
        sa[24..28].copy_from_slice(&1u32.to_le_bytes());
        let write: unsafe extern "win64" fn(usize, *const u8) -> i32 =
            std::mem::transmute(method(doc, 59));
        assert_eq!(write(doc, sa.as_ptr()), S_OK);
        let b = from(wb, SLOT_BROWSER);
        let text = state(b).text.clone().unwrap();
        assert!(text.contains("Version de démonstration"), "{text:?}");
        assert!(text.contains("logiciel"), "{text:?}");
        // readyState « complete ».
        let ready: unsafe extern "win64" fn(usize, *mut *mut u16) -> i32 =
            std::mem::transmute(method(doc, 22));
        let mut rs: *mut u16 = std::ptr::null_mut();
        assert_eq!(ready(doc, &mut rs), S_OK);
        assert_eq!(wide_text(rs), "complete");
        super::super::oleaut32::sys_free_string(rs);
        super::super::oleaut32::sys_free_string(bstr);
        // Le document retient le contrôle : il est libéré en dernier.
        assert_eq!(release(wb), 1);
        assert_eq!(release(doc), 0);
    }
}

#[test]
fn flux_d_initialisation_d_atl_accepte() {
    unsafe {
        let ps = create(&IID_IPERSIST_STREAM_INIT);
        let load: unsafe extern "win64" fn(usize, usize) -> i32 =
            std::mem::transmute(method(ps, 5));
        assert_eq!(load(ps, 0), E_POINTER);
        // Le flux n'est pas lu : une adresse non nulle suffit.
        assert_eq!(load(ps, 0x1000), S_OK);
        assert_eq!(release(ps), 0);
    }
}
