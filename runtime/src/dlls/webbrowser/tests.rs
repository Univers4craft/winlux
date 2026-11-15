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
        // get_Document n'est pas encore servi : E_NOTIMPL, pas un succès vide.
        let doc: unsafe extern "win64" fn(usize, *mut usize) -> i32 =
            std::mem::transmute(method(wb, 18));
        let mut d = 0usize;
        assert_eq!(doc(wb, &mut d), E_NOTIMPL);
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
