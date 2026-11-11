//! Lot F de #1410 (#1615) : événements (IMFMediaEvent), descripteurs de flux
//! et de présentation (IMFStreamDescriptor, IMFPresentationDescriptor) et
//! leur IMFMediaTypeHandler. Ordre des méthodes et IID d'après mfobjects.h
//! et mfidl.h (en-têtes mingw-w64) ; comportement d'après Microsoft Learn.

use super::*;
use std::sync::atomic::AtomicBool;

pub(super) const IID_IMFMEDIAEVENT: Guid = guid(
    0xdf598932,
    0xf10c,
    0x4e39,
    [0xbb, 0xa2, 0xc3, 0x08, 0xf1, 0x01, 0xda, 0xa3],
);
pub(super) const IID_IMFSTREAMDESCRIPTOR: Guid = guid(
    0x56c03d9c,
    0x9dbb,
    0x45f5,
    [0xab, 0x4b, 0xd8, 0x0f, 0x47, 0xc0, 0x59, 0x38],
);
pub(super) const IID_IMFPRESENTATIONDESCRIPTOR: Guid = guid(
    0x03cb2711,
    0x24d7,
    0x4db6,
    [0xa1, 0x7f, 0xf3, 0xa7, 0xa4, 0x79, 0xa5, 0x36],
);
const IID_IMFMEDIATYPEHANDLER: Guid = guid(
    0xe93dcf6c,
    0x4b07,
    0x4e1e,
    [0x81, 0x23, 0xaa, 0x16, 0xed, 0x6e, 0xad, 0xf5],
);
const MF_E_INVALIDINDEX: i32 = 0xC00D_36BFu32 as i32;

// ---------- Données propres ----------

pub(crate) struct EventData {
    met: u32,
    ext: Guid,
    status: i32,
    value: Option<Val>,
}

impl Drop for EventData {
    fn drop(&mut self) {
        if let Some(Val::Unk(p)) = self.value {
            unsafe { com_release(p as *mut c_void) };
        }
    }
}

pub(crate) struct StreamData {
    id: u32,
    types: Vec<usize>,
    current: Mutex<usize>,
}

impl Drop for StreamData {
    fn drop(&mut self) {
        let cur = *lock(&self.current);
        for &p in self.types.iter().chain((cur != 0).then_some(&cur)) {
            unsafe { com_release(p as *mut c_void) };
        }
    }
}

pub(crate) struct PresData {
    streams: Vec<(usize, AtomicBool)>,
}

impl Drop for PresData {
    fn drop(&mut self) {
        for (p, _) in &self.streams {
            unsafe { com_release(*p as *mut c_void) };
        }
    }
}

// ---------- Création ----------

/// MFCreateMediaEvent : valeur copiée depuis le PROPVARIANT (types des
/// attributs MF ; les autres donnent un événement sans valeur).
pub unsafe fn create_event(met: u32, ext: *const u8, status: i32, pv: *const u8) -> *mut c_void {
    let value = if pv.is_null() {
        None
    } else {
        read_propvariant(pv)
    };
    if let Some(Val::Unk(p)) = value {
        com_addref(p as *mut c_void);
    }
    new_obj(Ext::Event(EventData {
        met,
        ext: comobj::read_guid(ext).unwrap_or([0; 16]),
        status,
        value,
    })) as *mut c_void
}

/// MFCreateStreamDescriptor : types de média gardés (référence prise).
pub unsafe fn create_stream_descriptor(id: u32, types: &[*mut c_void]) -> *mut c_void {
    for &t in types {
        com_addref(t);
    }
    new_obj(Ext::StreamDesc(StreamData {
        id,
        types: types.iter().map(|&t| t as usize).collect(),
        current: Mutex::new(0),
    })) as *mut c_void
}

/// MFCreatePresentationDescriptor : flux désélectionnés au départ.
pub unsafe fn create_presentation_descriptor(streams: &[*mut c_void]) -> *mut c_void {
    for &s in streams {
        com_addref(s);
    }
    new_obj(Ext::PresDesc(PresData {
        streams: streams
            .iter()
            .map(|&s| (s as usize, AtomicBool::new(false)))
            .collect(),
    })) as *mut c_void
}

// ---------- IMFMediaEvent ----------

unsafe fn event<'a>(this: *mut MfObj) -> Option<&'a EventData> {
    match &obj(this).ext {
        Ext::Event(e) => Some(e),
        _ => None,
    }
}
unsafe extern "win64" fn ev_get_type(this: *mut MfObj, out: *mut u32) -> i32 {
    match (event(this), out.is_null()) {
        (Some(e), false) => {
            *out = e.met;
            S_OK
        }
        _ => E_POINTER,
    }
}
unsafe extern "win64" fn ev_get_extended_type(this: *mut MfObj, out: *mut u8) -> i32 {
    match (event(this), out.is_null()) {
        (Some(e), false) => {
            std::ptr::copy_nonoverlapping(e.ext.as_ptr(), out, 16);
            S_OK
        }
        _ => E_POINTER,
    }
}
unsafe extern "win64" fn ev_get_status(this: *mut MfObj, out: *mut i32) -> i32 {
    match (event(this), out.is_null()) {
        (Some(e), false) => {
            *out = e.status;
            S_OK
        }
        _ => E_POINTER,
    }
}
unsafe extern "win64" fn ev_get_value(this: *mut MfObj, pv: *mut u8) -> i32 {
    let Some(e) = event(this) else {
        return E_POINTER;
    };
    if pv.is_null() {
        return E_POINTER;
    }
    match &e.value {
        Some(v) => write_propvariant(pv, v),
        None => std::ptr::write_bytes(pv, 0, 16),
    }
    S_OK
}

pub(super) static EVENT_VTBL: VTable<37> = VTable(concat(
    attr_vtbl_entries!(),
    [
        ev_get_type as *const (),
        ev_get_extended_type as *const (),
        ev_get_status as *const (),
        ev_get_value as *const (),
    ],
));

// ---------- IMFStreamDescriptor ----------

unsafe fn stream<'a>(this: *mut MfObj) -> Option<&'a StreamData> {
    match &obj(this).ext {
        Ext::StreamDesc(s) => Some(s),
        _ => None,
    }
}
unsafe extern "win64" fn sd_get_stream_identifier(this: *mut MfObj, out: *mut u32) -> i32 {
    match (stream(this), out.is_null()) {
        (Some(s), false) => {
            *out = s.id;
            S_OK
        }
        _ => E_POINTER,
    }
}

#[repr(C)]
struct Handler {
    vtbl: *const (),
    refs: AtomicU32,
    sd: *mut MfObj,
}

unsafe extern "win64" fn sd_get_media_type_handler(this: *mut MfObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    a_addref(this);
    *out = Box::into_raw(Box::new(Handler {
        vtbl: HANDLER_VTBL.0.as_ptr() as *const (),
        refs: AtomicU32::new(1),
        sd: this,
    })) as *mut c_void;
    S_OK
}

pub(super) static STREAMDESC_VTBL: VTable<35> = VTable(concat(
    attr_vtbl_entries!(),
    [
        sd_get_stream_identifier as *const (),
        sd_get_media_type_handler as *const (),
    ],
));

// ---------- IMFMediaTypeHandler ----------

unsafe fn handler_stream<'a>(h: *mut Handler) -> &'a StreamData {
    stream((*h).sd).expect("gestionnaire sans descripteur de flux")
}
unsafe extern "win64" fn h_qi(this: *mut Handler, riid: *const u8, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match comobj::read_guid(riid) {
        Some(iid) if iid == IID_IUNKNOWN || iid == IID_IMFMEDIATYPEHANDLER => {
            h_addref(this);
            *out = this as *mut c_void;
            S_OK
        }
        _ => {
            *out = std::ptr::null_mut();
            E_NOINTERFACE
        }
    }
}
unsafe extern "win64" fn h_addref(this: *mut Handler) -> u32 {
    (*this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn h_release(this: *mut Handler) -> u32 {
    let n = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let h = Box::from_raw(this);
        a_release(h.sd);
    }
    n
}
/// IsMediaTypeSupported : accepté si le type a le sous-type d'un des types
/// annoncés (ou si le flux n'en annonce aucun).
unsafe extern "win64" fn h_is_supported(
    this: *mut Handler,
    mt: *mut c_void,
    closest: *mut *mut c_void,
) -> i32 {
    if !closest.is_null() {
        *closest = std::ptr::null_mut();
    }
    if mt.is_null() {
        return E_POINTER;
    }
    let s = handler_stream(this);
    let sub = attr_get_guid(mt, &MF_MT_SUBTYPE);
    if s.types.is_empty()
        || s.types
            .iter()
            .any(|&t| attr_get_guid(t as *mut c_void, &MF_MT_SUBTYPE) == sub)
    {
        S_OK
    } else {
        MF_E_INVALIDMEDIATYPE
    }
}
unsafe extern "win64" fn h_get_count(this: *mut Handler, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = handler_stream(this).types.len() as u32;
    S_OK
}
unsafe extern "win64" fn h_get_by_index(this: *mut Handler, i: u32, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match handler_stream(this).types.get(i as usize) {
        Some(&t) => {
            com_addref(t as *mut c_void);
            *out = t as *mut c_void;
            S_OK
        }
        None => {
            *out = std::ptr::null_mut();
            MF_E_NO_MORE_TYPES
        }
    }
}
unsafe extern "win64" fn h_set_current(this: *mut Handler, mt: *mut c_void) -> i32 {
    if mt.is_null() {
        return E_POINTER;
    }
    com_addref(mt);
    let old = std::mem::replace(&mut *lock(&handler_stream(this).current), mt as usize);
    if old != 0 {
        com_release(old as *mut c_void);
    }
    S_OK
}
unsafe extern "win64" fn h_get_current(this: *mut Handler, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let cur = *lock(&handler_stream(this).current);
    if cur == 0 {
        *out = std::ptr::null_mut();
        return MF_E_NOT_INITIALIZED;
    }
    com_addref(cur as *mut c_void);
    *out = cur as *mut c_void;
    S_OK
}
/// GetMajorType : celui du type courant, sinon du premier type annoncé.
unsafe extern "win64" fn h_get_major_type(this: *mut Handler, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let s = handler_stream(this);
    let cur = *lock(&s.current);
    let t = if cur != 0 {
        cur
    } else {
        s.types.first().copied().unwrap_or(0)
    };
    match (t != 0)
        .then(|| attr_get_guid(t as *mut c_void, &MF_MT_MAJOR_TYPE))
        .flatten()
    {
        Some(g) => {
            std::ptr::copy_nonoverlapping(g.as_ptr(), out, 16);
            S_OK
        }
        None => MF_E_ATTRIBUTENOTFOUND,
    }
}

static HANDLER_VTBL: VTable<9> = VTable([
    h_qi as *const (),
    h_addref as *const (),
    h_release as *const (),
    h_is_supported as *const (),
    h_get_count as *const (),
    h_get_by_index as *const (),
    h_set_current as *const (),
    h_get_current as *const (),
    h_get_major_type as *const (),
]);

// ---------- IMFPresentationDescriptor ----------

unsafe fn pres<'a>(this: *mut MfObj) -> Option<&'a PresData> {
    match &obj(this).ext {
        Ext::PresDesc(p) => Some(p),
        _ => None,
    }
}
unsafe extern "win64" fn pd_get_count(this: *mut MfObj, out: *mut u32) -> i32 {
    match (pres(this), out.is_null()) {
        (Some(p), false) => {
            *out = p.streams.len() as u32;
            S_OK
        }
        _ => E_POINTER,
    }
}
unsafe extern "win64" fn pd_get_by_index(
    this: *mut MfObj,
    i: u32,
    selected: *mut i32,
    out: *mut *mut c_void,
) -> i32 {
    let Some(p) = pres(this) else {
        return E_POINTER;
    };
    if selected.is_null() || out.is_null() {
        return E_POINTER;
    }
    let Some((sd, sel)) = p.streams.get(i as usize) else {
        *out = std::ptr::null_mut();
        return E_INVALIDARG;
    };
    *selected = i32::from(sel.load(Ordering::Relaxed));
    com_addref(*sd as *mut c_void);
    *out = *sd as *mut c_void;
    S_OK
}
unsafe fn select(this: *mut MfObj, i: u32, on: bool) -> i32 {
    match pres(this).and_then(|p| p.streams.get(i as usize)) {
        Some((_, sel)) => {
            sel.store(on, Ordering::Relaxed);
            S_OK
        }
        None => MF_E_INVALIDINDEX,
    }
}
unsafe extern "win64" fn pd_select(this: *mut MfObj, i: u32) -> i32 {
    select(this, i, true)
}
unsafe extern "win64" fn pd_deselect(this: *mut MfObj, i: u32) -> i32 {
    select(this, i, false)
}
/// Clone : copie des attributs et de l'état de sélection ; les descripteurs
/// de flux sont partagés, comme sous Windows.
unsafe extern "win64" fn pd_clone(this: *mut MfObj, out: *mut *mut c_void) -> i32 {
    let Some(p) = pres(this) else {
        return E_POINTER;
    };
    if out.is_null() {
        return E_POINTER;
    }
    let streams: Vec<*mut c_void> = p.streams.iter().map(|(s, _)| *s as *mut c_void).collect();
    let copy = create_presentation_descriptor(&streams) as *mut MfObj;
    if let Some(c) = pres(copy) {
        for ((_, a), (_, b)) in p.streams.iter().zip(&c.streams) {
            b.store(a.load(Ordering::Relaxed), Ordering::Relaxed);
        }
    }
    let items = lock(&obj(this).attrs).0.clone_items();
    {
        let mut attrs = lock(&obj(copy).attrs);
        for (k, v) in items {
            attrs.set(k, v);
        }
    }
    *out = copy as *mut c_void;
    S_OK
}

pub(super) static PRESDESC_VTBL: VTable<38> = VTable(concat(
    attr_vtbl_entries!(),
    [
        pd_get_count as *const (),
        pd_get_by_index as *const (),
        pd_select as *const (),
        pd_deselect as *const (),
        pd_clone as *const (),
    ],
));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evenement_et_descripteurs() {
        unsafe {
            // Événement MEStreamStarted (66) avec une valeur VT_UI8.
            let pv = [21u64, 1234, 0];
            let ev = create_event(66, [7u8; 16].as_ptr(), 1, pv.as_ptr().cast());
            let vt = *(ev as *const *const *const ());
            let get_type: unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32 =
                std::mem::transmute(*vt.add(33));
            let mut met = 0;
            assert_eq!(get_type(ev, &mut met), 0);
            assert_eq!(met, 66);
            let get_value: unsafe extern "win64" fn(*mut c_void, *mut u8) -> i32 =
                std::mem::transmute(*vt.add(36));
            let mut out = [0u64; 3];
            assert_eq!(get_value(ev, out.as_mut_ptr().cast()), 0);
            assert_eq!((out[0] as u16, out[1]), (21, 1234));
            assert!(com_qi(ev, &IID_IMFMEDIAEVENT)
                .map(|p| com_release(p))
                .is_some());
            com_release(ev);

            // Descripteur de flux avec un type, présentation, clone.
            let mt = create_media_type();
            let set_guid: unsafe extern "win64" fn(*mut c_void, *const u8, *const u8) -> i32 =
                std::mem::transmute(slot(mt, 24));
            set_guid(mt, MF_MT_SUBTYPE.as_ptr(), MFVIDEOFORMAT_MJPG.as_ptr());
            let sd = create_stream_descriptor(3, &[mt]);
            let get_handler: unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32 =
                std::mem::transmute(slot(sd, 34));
            let mut h = std::ptr::null_mut();
            assert_eq!(get_handler(sd, &mut h), 0);
            let count: unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32 =
                std::mem::transmute(slot(h, 4));
            let mut n = 0;
            assert_eq!(count(h, &mut n), 0);
            assert_eq!(n, 1);
            let supported: unsafe extern "win64" fn(
                *mut c_void,
                *mut c_void,
                *mut *mut c_void,
            ) -> i32 = std::mem::transmute(slot(h, 3));
            assert_eq!(supported(h, mt, std::ptr::null_mut()), 0);
            com_release(h);

            let pd = create_presentation_descriptor(&[sd]);
            com_release(sd);
            let sel: unsafe extern "win64" fn(*mut c_void, u32) -> i32 =
                std::mem::transmute(slot(pd, 35));
            assert_eq!(sel(pd, 0), 0);
            assert_eq!(sel(pd, 1), MF_E_INVALIDINDEX);
            let clone: unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32 =
                std::mem::transmute(slot(pd, 37));
            let mut pd2 = std::ptr::null_mut();
            assert_eq!(clone(pd, &mut pd2), 0);
            let by_index: unsafe extern "win64" fn(
                *mut c_void,
                u32,
                *mut i32,
                *mut *mut c_void,
            ) -> i32 = std::mem::transmute(slot(pd2, 34));
            let mut selected = 0;
            let mut s = std::ptr::null_mut();
            assert_eq!(by_index(pd2, 0, &mut selected, &mut s), 0);
            assert_eq!((selected, s), (1, sd));
            let id: unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32 =
                std::mem::transmute(slot(s, 33));
            let mut ident = 0;
            assert_eq!(id(s, &mut ident), 0);
            assert_eq!(ident, 3);
            com_release(s);
            com_release(pd2);
            com_release(pd);
            com_release(mt);
        }
    }
}
