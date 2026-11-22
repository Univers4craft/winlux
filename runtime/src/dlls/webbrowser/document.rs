//! Document du contrôle navigateur : IWebBrowser2::get_Document (#2228).
//!
//! L'installeur WinRAR demande le document après DocumentComplete, puis y
//! écrit son contrat. Interfaces servies : IHTMLDocument2 (avec
//! IHTMLDocument et IDispatch) et IPersistStreamInit. Ordre des méthodes
//! d'IHTMLDocument2 et IID : windows-rs (Win32::Web::MsHtml,
//! IHTMLDocument2_Vtbl, licence MIT). Le texte écrit (write, writeln,
//! IPersistStreamInit::Load) est du HTML, montré en texte par la zone de
//! lecture du contrôle, comme une page chargée par Navigate. Les autres
//! méthodes rendent E_NOTIMPL, notées avec leur nom, leur fichier et leur
//! ligne.

use super::*;

/// IHTMLDocument {626FC520-A41E-11CF-A731-00A0C9082637}.
const IID_IHTML_DOCUMENT: Guid = guid(
    0x626F_C520,
    0xA41E,
    0x11CF,
    [0xA7, 0x31, 0x00, 0xA0, 0xC9, 0x08, 0x26, 0x37],
);
/// IHTMLDocument2 {332C4425-26CB-11D0-B483-00C04FD90119}.
const IID_IHTML_DOCUMENT2: Guid = guid(
    0x332C_4425,
    0x26CB,
    0x11D0,
    [0xB4, 0x83, 0x00, 0xC0, 0x4F, 0xD9, 0x01, 0x19],
);
/// CLSID_HTMLDocument {25336920-03F9-11CF-8FD0-00AA00686F13} (mshtml.h).
const CLSID_HTML_DOCUMENT: Guid = guid(
    0x2533_6920,
    0x03F9,
    0x11CF,
    [0x8F, 0xD0, 0x00, 0xAA, 0x00, 0x68, 0x6F, 0x13],
);

const SLOT_DOC: usize = 0;
const SLOT_PSI: usize = 1;

#[repr(C)]
pub(super) struct Document {
    vtbls: [*const (); 2],
    refs: AtomicU32,
    /// Le contrôle (pointeur IWebBrowser2), retenu par le document.
    browser: usize,
    /// HTML écrit depuis le dernier open/clear/Load.
    html: Mutex<String>,
}

unsafe fn doc_from(this: usize, slot: usize) -> &'static Document {
    &*((this - slot * std::mem::size_of::<usize>()) as *const Document)
}

fn doc_iface(d: &Document, slot: usize) -> usize {
    (&d.vtbls[slot]) as *const *const () as usize
}

/// Document du contrôle `b`, créé au premier appel ; une référence de plus
/// pour l'appelant.
pub(super) unsafe fn document_of(b: &Browser) -> usize {
    let mut s = state(b);
    if s.document != 0 {
        let d = &*(s.document as *const Document);
        d.refs.fetch_add(1, Ordering::SeqCst);
        return doc_iface(d, SLOT_DOC);
    }
    b.refs.fetch_add(1, Ordering::SeqCst);
    let d = Box::into_raw(Box::new(Document {
        vtbls: [
            &DOC_VT as *const _ as *const (),
            &PSI_VT as *const _ as *const (),
        ],
        refs: AtomicU32::new(1),
        browser: iface(b, SLOT_BROWSER) as usize,
        html: Mutex::new(String::new()),
    }));
    s.document = d as usize;
    doc_iface(&*d, SLOT_DOC)
}

/// Le HTML du document devient la page montrée par le contrôle.
unsafe fn show(d: &Document) {
    let html = d.html.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let b = from(d.browser, SLOT_BROWSER);
    state(b).text = Some(texte::html_to_text(&html));
    show_text(b);
}

unsafe fn doc_query(d: &Document, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if iid.is_null() {
        return E_INVALIDARG;
    }
    let g = *iid;
    let slot = if g == IID_IUNKNOWN
        || g == IID_IDISPATCH
        || g == IID_IHTML_DOCUMENT
        || g == IID_IHTML_DOCUMENT2
    {
        SLOT_DOC
    } else if g == IID_IPERSIST_STREAM_INIT || g == IID_IPERSIST {
        SLOT_PSI
    } else {
        note(format_args!(
            "document : interface {} refusée",
            guid_str(&g)
        ));
        return E_NOINTERFACE;
    };
    d.refs.fetch_add(1, Ordering::SeqCst);
    *out = doc_iface(d, slot) as *mut c_void;
    S_OK
}

unsafe fn doc_release(d: &Document) -> u32 {
    let n = d.refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        let b = from(d.browser, SLOT_BROWSER);
        state(b).document = 0;
        drop(Box::from_raw(d as *const Document as *mut Document));
        release_obj(b);
    }
    n
}

macro_rules! doc_unknown {
    ($slot:expr, $qi:ident, $add:ident, $rel:ident) => {
        unsafe extern "win64" fn $qi(this: usize, iid: *const Guid, out: *mut *mut c_void) -> i32 {
            doc_query(doc_from(this, $slot), iid, out)
        }
        unsafe extern "win64" fn $add(this: usize) -> u32 {
            doc_from(this, $slot).refs.fetch_add(1, Ordering::SeqCst) + 1
        }
        unsafe extern "win64" fn $rel(this: usize) -> u32 {
            doc_release(doc_from(this, $slot))
        }
    };
}
doc_unknown!(SLOT_DOC, doc_qi, doc_add_ref, doc_release_fn);
doc_unknown!(SLOT_PSI, psi_qi, psi_add_ref, psi_release);

// ── IHTMLDocument2 ────────────────────────────────────────────────────

not_impl_fns!(
    GetTypeInfo,
    GetIDsOfNames,
    Invoke,
    get_Script,
    get_all,
    get_body,
    get_activeElement,
    get_images,
    get_applets,
    get_links,
    get_forms,
    get_anchors,
    get_scripts,
    get_selection,
    get_frames,
    get_embeds,
    get_plugins,
    get_alinkColor,
    get_bgColor,
    get_fgColor,
    get_linkColor,
    get_vlinkColor,
    get_referrer,
    get_location,
    get_lastModified,
    put_URL,
    put_domain,
    get_domain,
    put_cookie,
    get_cookie,
    put_expando,
    get_expando,
    get_fileSize,
    get_fileCreatedDate,
    get_fileModifiedDate,
    get_fileUpdatedDate,
    get_security,
    get_protocol,
    get_nameProp,
    queryCommandSupported,
    queryCommandEnabled,
    queryCommandState,
    queryCommandIndeterm,
    queryCommandText,
    queryCommandValue,
    execCommand,
    execCommandShowHelp,
    createElement,
    get_onhelp,
    get_onclick,
    get_ondblclick,
    get_onkeyup,
    get_onkeydown,
    get_onkeypress,
    get_onmouseup,
    get_onmousedown,
    get_onmousemove,
    get_onmouseout,
    get_onmouseover,
    get_onreadystatechange,
    get_onafterupdate,
    get_onrowexit,
    get_onrowenter,
    get_ondragstart,
    get_onselectstart,
    elementFromPoint,
    get_parentWindow,
    get_styleSheets,
    get_onbeforeupdate,
    get_onerrorupdate,
    toString,
    createStyleSheet,
    Save,
    GetSizeMax,
);

// Propriétés d'apparence et gestionnaires d'événements posés par le
// programme : acceptés, sans effet sur la page montrée en texte.
accepted_fns!(
    put_designMode,
    put_alinkColor,
    put_bgColor,
    put_fgColor,
    put_linkColor,
    put_vlinkColor,
    put_charset,
    put_defaultCharset,
    put_onhelp,
    put_onclick,
    put_ondblclick,
    put_onkeyup,
    put_onkeydown,
    put_onkeypress,
    put_onmouseup,
    put_onmousedown,
    put_onmousemove,
    put_onmouseout,
    put_onmouseover,
    put_onreadystatechange,
    put_onafterupdate,
    put_onrowexit,
    put_onrowenter,
    put_ondragstart,
    put_onselectstart,
    put_onbeforeupdate,
    put_onerrorupdate,
);

/// Rend une BSTR neuve de `s` dans `out`.
unsafe fn give_bstr(out: *mut *mut u16, s: &str) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    *out = super::super::oleaut32::sys_alloc_string(w.as_ptr());
    S_OK
}

unsafe extern "win64" fn doc_type_info_count(_this: usize, count: *mut u32) -> i32 {
    if count.is_null() {
        return E_POINTER;
    }
    *count = 0;
    S_OK
}

unsafe extern "win64" fn put_title(_this: usize, v: *const u16) -> i32 {
    note(format_args!("document : titre « {} »", wide_text(v)));
    S_OK
}

unsafe extern "win64" fn get_title(_this: usize, out: *mut *mut u16) -> i32 {
    give_bstr(out, "")
}

unsafe extern "win64" fn get_design_mode(_this: usize, out: *mut *mut u16) -> i32 {
    give_bstr(out, "Off")
}

unsafe extern "win64" fn get_ready_state(_this: usize, out: *mut *mut u16) -> i32 {
    give_bstr(out, "complete")
}

unsafe extern "win64" fn get_url(this: usize, out: *mut *mut u16) -> i32 {
    let d = doc_from(this, SLOT_DOC);
    let url = state(from(d.browser, SLOT_BROWSER)).url.clone();
    give_bstr(out, if url.is_empty() { "about:blank" } else { &url })
}

unsafe extern "win64" fn get_charset(_this: usize, out: *mut *mut u16) -> i32 {
    give_bstr(out, "utf-8")
}

unsafe extern "win64" fn get_mime_type(_this: usize, out: *mut *mut u16) -> i32 {
    give_bstr(out, "HTML Document")
}

/// Chaînes d'un SAFEARRAY de VARIANT (oaidl.h, x64 : cDims à 0,
/// cbElements à +4, pvData à +16, rgsabound[0].cElements à +24).
unsafe fn safearray_text(psa: *const u8) -> Option<String> {
    if psa.is_null() {
        return None;
    }
    let dims = std::ptr::read_unaligned(psa as *const u16);
    let elem = std::ptr::read_unaligned(psa.add(4) as *const u32) as usize;
    let data = std::ptr::read_unaligned(psa.add(16) as *const usize) as *const u8;
    let count = std::ptr::read_unaligned(psa.add(24) as *const u32) as usize;
    if dims != 1 || elem != 24 || data.is_null() {
        return None;
    }
    let mut s = String::new();
    for i in 0..count {
        if let Some(t) = variant_text(data.add(i * 24)) {
            s.push_str(&t);
        }
    }
    Some(s)
}

unsafe fn write_common(this: usize, psa: *const u8, newline: bool, what: &str) -> i32 {
    let d = doc_from(this, SLOT_DOC);
    let Some(text) = safearray_text(psa) else {
        note(format_args!(
            "document : {what} sans SAFEARRAY de VARIANT lisible"
        ));
        return E_INVALIDARG;
    };
    note(format_args!(
        "document : {what} de {} caractères",
        text.chars().count()
    ));
    {
        let mut html = d.html.lock().unwrap_or_else(|e| e.into_inner());
        html.push_str(&text);
        if newline {
            html.push('\n');
        }
    }
    show(d);
    S_OK
}

unsafe extern "win64" fn write(this: usize, psa: *const u8) -> i32 {
    write_common(this, psa, false, "write")
}

unsafe extern "win64" fn writeln(this: usize, psa: *const u8) -> i32 {
    write_common(this, psa, true, "writeln")
}

/// open(url, name, features, replace, ppWindow) : le document repart vide.
/// Les VARIANT passés par valeur arrivent en x64 par adresse.
unsafe extern "win64" fn open(
    this: usize,
    _url: *const u16,
    _name: *const u8,
    _features: *const u8,
    _replace: *const u8,
    out: *mut usize,
) -> i32 {
    let d = doc_from(this, SLOT_DOC);
    d.html.lock().unwrap_or_else(|e| e.into_inner()).clear();
    if !out.is_null() {
        *out = 0;
    }
    note(format_args!("document : open"));
    S_OK
}

unsafe extern "win64" fn close_doc(_this: usize) -> i32 {
    note(format_args!("document : close"));
    S_OK
}

unsafe extern "win64" fn clear(this: usize) -> i32 {
    let d = doc_from(this, SLOT_DOC);
    d.html.lock().unwrap_or_else(|e| e.into_inner()).clear();
    show(d);
    S_OK
}

static DOC_VT: VTable<116> = VTable([
    doc_qi as *const (),
    doc_add_ref as *const (),
    doc_release_fn as *const (),
    doc_type_info_count as *const (),
    GetTypeInfo as *const (),
    GetIDsOfNames as *const (),
    Invoke as *const (),
    // IHTMLDocument
    get_Script as *const (),
    // IHTMLDocument2
    get_all as *const (),
    get_body as *const (),
    get_activeElement as *const (),
    get_images as *const (),
    get_applets as *const (),
    get_links as *const (),
    get_forms as *const (),
    get_anchors as *const (),
    put_title as *const (),
    get_title as *const (),
    get_scripts as *const (),
    put_designMode as *const (),
    get_design_mode as *const (),
    get_selection as *const (),
    get_ready_state as *const (),
    get_frames as *const (),
    get_embeds as *const (),
    get_plugins as *const (),
    put_alinkColor as *const (),
    get_alinkColor as *const (),
    put_bgColor as *const (),
    get_bgColor as *const (),
    put_fgColor as *const (),
    get_fgColor as *const (),
    put_linkColor as *const (),
    get_linkColor as *const (),
    put_vlinkColor as *const (),
    get_vlinkColor as *const (),
    get_referrer as *const (),
    get_location as *const (),
    get_lastModified as *const (),
    put_URL as *const (),
    get_url as *const (),
    put_domain as *const (),
    get_domain as *const (),
    put_cookie as *const (),
    get_cookie as *const (),
    put_expando as *const (),
    get_expando as *const (),
    put_charset as *const (),
    get_charset as *const (),
    put_defaultCharset as *const (),
    get_charset as *const (), // defaultCharset
    get_mime_type as *const (),
    get_fileSize as *const (),
    get_fileCreatedDate as *const (),
    get_fileModifiedDate as *const (),
    get_fileUpdatedDate as *const (),
    get_security as *const (),
    get_protocol as *const (),
    get_nameProp as *const (),
    write as *const (),
    writeln as *const (),
    open as *const (),
    close_doc as *const (),
    clear as *const (),
    queryCommandSupported as *const (),
    queryCommandEnabled as *const (),
    queryCommandState as *const (),
    queryCommandIndeterm as *const (),
    queryCommandText as *const (),
    queryCommandValue as *const (),
    execCommand as *const (),
    execCommandShowHelp as *const (),
    createElement as *const (),
    put_onhelp as *const (),
    get_onhelp as *const (),
    put_onclick as *const (),
    get_onclick as *const (),
    put_ondblclick as *const (),
    get_ondblclick as *const (),
    put_onkeyup as *const (),
    get_onkeyup as *const (),
    put_onkeydown as *const (),
    get_onkeydown as *const (),
    put_onkeypress as *const (),
    get_onkeypress as *const (),
    put_onmouseup as *const (),
    get_onmouseup as *const (),
    put_onmousedown as *const (),
    get_onmousedown as *const (),
    put_onmousemove as *const (),
    get_onmousemove as *const (),
    put_onmouseout as *const (),
    get_onmouseout as *const (),
    put_onmouseover as *const (),
    get_onmouseover as *const (),
    put_onreadystatechange as *const (),
    get_onreadystatechange as *const (),
    put_onafterupdate as *const (),
    get_onafterupdate as *const (),
    put_onrowexit as *const (),
    get_onrowexit as *const (),
    put_onrowenter as *const (),
    get_onrowenter as *const (),
    put_ondragstart as *const (),
    get_ondragstart as *const (),
    put_onselectstart as *const (),
    get_onselectstart as *const (),
    elementFromPoint as *const (),
    get_parentWindow as *const (),
    get_styleSheets as *const (),
    put_onbeforeupdate as *const (),
    get_onbeforeupdate as *const (),
    put_onerrorupdate as *const (),
    get_onerrorupdate as *const (),
    toString as *const (),
    createStyleSheet as *const (),
]);

// ── IPersistStreamInit ────────────────────────────────────────────────

unsafe extern "win64" fn psi_class_id(_this: usize, out: *mut Guid) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = CLSID_HTML_DOCUMENT;
    S_OK
}

extern "win64" fn psi_is_dirty(_this: usize) -> i32 {
    S_FALSE
}

/// Load(IStream) : tout le flux (ISequentialStream::Read, 4e méthode).
unsafe extern "win64" fn psi_load(this: usize, stream: usize) -> i32 {
    if stream == 0 {
        return E_POINTER;
    }
    let d = doc_from(this, SLOT_PSI);
    let read: unsafe extern "win64" fn(usize, *mut u8, u32, *mut u32) -> i32 =
        std::mem::transmute(method(stream, 3));
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 16384];
    loop {
        let mut got = 0u32;
        let hr = read(stream, chunk.as_mut_ptr(), chunk.len() as u32, &mut got);
        bytes.extend_from_slice(&chunk[..got as usize]);
        if hr < 0 || got == 0 || bytes.len() > 64 << 20 {
            break;
        }
    }
    let html = texte::decode_page(&bytes);
    note(format_args!("document : Load de {} octets", bytes.len()));
    *d.html.lock().unwrap_or_else(|e| e.into_inner()) = html;
    show(d);
    S_OK
}

unsafe extern "win64" fn psi_init_new(this: usize) -> i32 {
    let d = doc_from(this, SLOT_PSI);
    d.html.lock().unwrap_or_else(|e| e.into_inner()).clear();
    S_OK
}

static PSI_VT: VTable<9> = VTable([
    psi_qi as *const (),
    psi_add_ref as *const (),
    psi_release as *const (),
    psi_class_id as *const (),
    psi_is_dirty as *const (),
    psi_load as *const (),
    Save as *const (),
    GetSizeMax as *const (),
    psi_init_new as *const (),
]);
