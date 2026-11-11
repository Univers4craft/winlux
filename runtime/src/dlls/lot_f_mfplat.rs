//! mfplat, lot F de #1410 (#1615) : files d'événements, collections,
//! résultats asynchrones, descripteurs, sérialisation des attributs et
//! calculs de format.
//!
//! Ordre des méthodes, IID et structure MFASYNCRESULT d'après mfobjects.h,
//! mfapi.h et mferror.h (en-têtes mingw-w64) ; comportement d'après
//! Microsoft Learn. Les rappels sont appelés sur le fil de l'appelant
//! (aucune file de travail MF n'existe encore).

#![allow(non_snake_case)]

use super::comobj::{self, guid, Guid, VTable, IID_IUNKNOWN};
use super::lock;
use super::mfcapture::{self, com_addref, com_release, slot, Val};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Condvar, Mutex};

const S_OK: i32 = 0;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const MF_E_INVALIDMEDIATYPE: i32 = 0xC00D_36B4u32 as i32;
const MF_E_MULTIPLE_BEGIN: i32 = 0xC00D_36D9u32 as i32;
const MF_E_MULTIPLE_SUBSCRIBERS: i32 = 0xC00D_36DAu32 as i32;
const MF_E_NO_EVENTS_AVAILABLE: i32 = 0xC00D_3E80u32 as i32;
const MF_E_SHUTDOWN: i32 = 0xC00D_3E85u32 as i32;
/// MF_E_DXGI_DEVICE_NOT_INITIALIZED (mfapi.h).
const MF_E_DXGI_DEVICE_NOT_INITIALIZED: i32 = 0x8004_1000u32 as i32;
const MF_EVENT_FLAG_NO_WAIT: u32 = 1;
/// MF_ATTRIBUTE_SERIALIZE_UNKNOWN_BYREF.
const SERIALIZE_UNKNOWN_BYREF: u32 = 1;

const IID_IMFASYNCRESULT: Guid = guid(
    0xac6b7889,
    0x0740,
    0x4d51,
    [0x86, 0x19, 0x90, 0x59, 0x94, 0xa5, 0x5c, 0xc6],
);
const IID_IMFCOLLECTION: Guid = guid(
    0x5bc8a76b,
    0x869a,
    0x46a3,
    [0x9b, 0x03, 0xfa, 0x21, 0x8a, 0x66, 0xae, 0xbe],
);
const IID_IMFMEDIAEVENTQUEUE: Guid = guid(
    0x36f846fc,
    0x2256,
    0x48b6,
    [0xb5, 0x8e, 0xe2, 0xb6, 0x38, 0x31, 0x65, 0x81],
);

// ---------- Objet COM générique ----------

/// En-tête commun : table virtuelle, compteur, IID servi, données.
#[repr(C)]
struct Com<T> {
    vtbl: *const (),
    refs: AtomicU32,
    iid: Guid,
    data: T,
}

fn new_com<T>(vtbl: *const (), iid: Guid, data: T) -> *mut c_void {
    Box::into_raw(Box::new(Com {
        vtbl,
        refs: AtomicU32::new(1),
        iid,
        data,
    })) as *mut c_void
}

unsafe extern "win64" fn qi<T>(this: *mut Com<T>, riid: *const u8, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match comobj::read_guid(riid) {
        Some(iid) if iid == IID_IUNKNOWN || iid == (*this).iid => {
            addref(this);
            *out = this as *mut c_void;
            S_OK
        }
        _ => {
            *out = std::ptr::null_mut();
            E_NOINTERFACE
        }
    }
}
unsafe extern "win64" fn addref<T>(this: *mut Com<T>) -> u32 {
    (*this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn release<T>(this: *mut Com<T>) -> u32 {
    let n = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this));
    }
    n
}

/// Référence COM détenue, rendue à la destruction.
struct Held(usize);
impl Held {
    unsafe fn take(p: *mut c_void) -> Held {
        if !p.is_null() {
            com_addref(p);
        }
        Held(p as usize)
    }
    fn ptr(&self) -> *mut c_void {
        self.0 as *mut c_void
    }
    /// Nouvelle référence pour l'appelant.
    unsafe fn give(&self) -> *mut c_void {
        if self.0 != 0 {
            com_addref(self.ptr());
        }
        self.ptr()
    }
}
impl Drop for Held {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe { com_release(self.ptr()) };
        }
    }
}

// ---------- IMFAsyncResult (MFASYNCRESULT) ----------

/// MFASYNCRESULT de mfapi.h : la table virtuelle, OVERLAPPED (32 octets),
/// pCallback, hrStatusResult, dwBytesTransferred, hEvent ; puis nos
/// champs.
#[repr(C)]
struct AsyncResult {
    vtbl: *const (),
    overlapped: [u8; 32],
    callback: *mut c_void,
    status: i32,
    bytes: u32,
    event: usize,
    refs: AtomicU32,
    object: Held,
    state: Held,
}

/// Décalage de pCallback dans MFASYNCRESULT (x64).
const CALLBACK_OFFSET: usize = 40;

unsafe fn create_async_result(
    object: *mut c_void,
    callback: *mut c_void,
    state: *mut c_void,
) -> *mut c_void {
    if !callback.is_null() {
        com_addref(callback);
    }
    Box::into_raw(Box::new(AsyncResult {
        vtbl: RESULT_VTBL.0.as_ptr() as *const (),
        overlapped: [0; 32],
        callback,
        status: S_OK,
        bytes: 0,
        event: 0,
        refs: AtomicU32::new(1),
        object: Held::take(object),
        state: Held::take(state),
    })) as *mut c_void
}

fn is_result(p: *mut c_void) -> bool {
    !p.is_null() && unsafe { *(p as *const *const ()) } == RESULT_VTBL.0.as_ptr() as *const ()
}

unsafe extern "win64" fn r_qi(
    this: *mut AsyncResult,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match comobj::read_guid(riid) {
        Some(iid) if iid == IID_IUNKNOWN || iid == IID_IMFASYNCRESULT => {
            r_addref(this);
            *out = this as *mut c_void;
            S_OK
        }
        _ => {
            *out = std::ptr::null_mut();
            E_NOINTERFACE
        }
    }
}
unsafe extern "win64" fn r_addref(this: *mut AsyncResult) -> u32 {
    (*this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn r_release(this: *mut AsyncResult) -> u32 {
    let n = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let r = Box::from_raw(this);
        if !r.callback.is_null() {
            com_release(r.callback);
        }
    }
    n
}
unsafe extern "win64" fn r_get_state(this: *mut AsyncResult, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = (*this).state.give();
    S_OK
}
unsafe extern "win64" fn r_get_status(this: *mut AsyncResult) -> i32 {
    (*this).status
}
unsafe extern "win64" fn r_set_status(this: *mut AsyncResult, hr: i32) -> i32 {
    (*this).status = hr;
    S_OK
}
/// GetObject : E_POINTER quand aucun objet n'est associé (Microsoft Learn).
unsafe extern "win64" fn r_get_object(this: *mut AsyncResult, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = (*this).object.give();
    if (*out).is_null() {
        E_POINTER
    } else {
        S_OK
    }
}
unsafe extern "win64" fn r_get_state_no_addref(this: *mut AsyncResult) -> *mut c_void {
    (*this).state.ptr()
}

static RESULT_VTBL: VTable<8> = VTable([
    r_qi as *const (),
    r_addref as *const (),
    r_release as *const (),
    r_get_state as *const (),
    r_get_status as *const (),
    r_set_status as *const (),
    r_get_object as *const (),
    r_get_state_no_addref as *const (),
]);

/// MFCreateAsyncResult(punkObject, pCallback, punkState, ppAsyncResult).
pub unsafe extern "win64" fn MFCreateAsyncResult(
    object: *mut c_void,
    callback: *mut c_void,
    state: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = create_async_result(object, callback, state);
    S_OK
}

/// MFInvokeCallback : appelle IMFAsyncCallback::Invoke du pCallback de la
/// structure MFASYNCRESULT, ici sur le fil de l'appelant.
pub unsafe extern "win64" fn MFInvokeCallback(result: *mut c_void) -> i32 {
    if result.is_null() {
        return E_INVALIDARG;
    }
    let cb = std::ptr::read_unaligned((result as *const u8).add(CALLBACK_OFFSET) as *const usize)
        as *mut c_void;
    if cb.is_null() {
        return E_INVALIDARG;
    }
    let invoke: unsafe extern "win64" fn(*mut c_void, *mut c_void) -> i32 =
        std::mem::transmute(slot(cb, 4));
    invoke(cb, result);
    S_OK
}

// ---------- IMFCollection ----------

type Collection = Com<Mutex<Vec<Held>>>;

unsafe fn items<'a>(this: *mut Collection) -> std::sync::MutexGuard<'a, Vec<Held>> {
    lock(&(*this).data)
}
unsafe extern "win64" fn c_count(this: *mut Collection, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = items(this).len() as u32;
    S_OK
}
unsafe extern "win64" fn c_get(this: *mut Collection, i: u32, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match items(this).get(i as usize) {
        Some(h) => {
            *out = h.give();
            S_OK
        }
        None => {
            *out = std::ptr::null_mut();
            E_INVALIDARG
        }
    }
}
unsafe extern "win64" fn c_add(this: *mut Collection, unk: *mut c_void) -> i32 {
    items(this).push(Held::take(unk));
    S_OK
}
unsafe extern "win64" fn c_remove(this: *mut Collection, i: u32, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let mut v = items(this);
    if i as usize >= v.len() {
        *out = std::ptr::null_mut();
        return E_INVALIDARG;
    }
    let h = v.remove(i as usize);
    *out = h.ptr();
    std::mem::forget(h);
    S_OK
}
unsafe extern "win64" fn c_insert(this: *mut Collection, i: u32, unk: *mut c_void) -> i32 {
    let mut v = items(this);
    if i as usize > v.len() {
        return E_INVALIDARG;
    }
    v.insert(i as usize, Held::take(unk));
    S_OK
}
unsafe extern "win64" fn c_clear(this: *mut Collection) -> i32 {
    items(this).clear();
    S_OK
}

static COLLECTION_VTBL: VTable<9> = VTable([
    qi::<Mutex<Vec<Held>>> as *const (),
    addref::<Mutex<Vec<Held>>> as *const (),
    release::<Mutex<Vec<Held>>> as *const (),
    c_count as *const (),
    c_get as *const (),
    c_add as *const (),
    c_remove as *const (),
    c_insert as *const (),
    c_clear as *const (),
]);

pub unsafe extern "win64" fn MFCreateCollection(out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_com(
        COLLECTION_VTBL.0.as_ptr() as *const (),
        IID_IMFCOLLECTION,
        Mutex::new(Vec::<Held>::new()),
    );
    S_OK
}

// ---------- IMFMediaEventQueue ----------

#[derive(Default)]
struct QueueState {
    events: VecDeque<Held>,
    /// Rappel en attente de BeginGetEvent : (callback, état).
    pending: Option<(Held, Held)>,
    shut: bool,
}

struct Queue {
    state: Mutex<QueueState>,
    ready: Condvar,
}

type EventQueue = Com<Queue>;

/// Rend l'événement au rappel en attente, s'il y en a un et un événement.
unsafe fn dispatch(this: *mut EventQueue) {
    let (ev, cb, state) = {
        let mut s = lock(&(*this).data.state);
        if s.pending.is_none() || s.events.is_empty() {
            return;
        }
        let ev = s.events.pop_front().unwrap();
        let (cb, state) = s.pending.take().unwrap();
        (ev, cb, state)
    };
    let result = create_async_result(ev.ptr(), cb.ptr(), state.ptr());
    MFInvokeCallback(result);
    com_release(result);
}

unsafe extern "win64" fn q_get_event(
    this: *mut EventQueue,
    flags: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let q = &(*this).data;
    let mut s = lock(&q.state);
    loop {
        if s.shut {
            return MF_E_SHUTDOWN;
        }
        if s.pending.is_some() {
            return MF_E_MULTIPLE_SUBSCRIBERS;
        }
        if let Some(ev) = s.events.pop_front() {
            *out = ev.ptr();
            std::mem::forget(ev);
            return S_OK;
        }
        if flags & MF_EVENT_FLAG_NO_WAIT != 0 {
            return MF_E_NO_EVENTS_AVAILABLE;
        }
        s = q.ready.wait(s).unwrap_or_else(|e| e.into_inner());
    }
}
unsafe extern "win64" fn q_begin(
    this: *mut EventQueue,
    cb: *mut c_void,
    state: *mut c_void,
) -> i32 {
    if cb.is_null() {
        return E_INVALIDARG;
    }
    {
        let mut s = lock(&(*this).data.state);
        if s.shut {
            return MF_E_SHUTDOWN;
        }
        if s.pending.is_some() {
            return MF_E_MULTIPLE_BEGIN;
        }
        s.pending = Some((Held::take(cb), Held::take(state)));
    }
    dispatch(this);
    S_OK
}
/// EndGetEvent : l'événement est l'objet du résultat rendu au rappel.
unsafe extern "win64" fn q_end(
    this: *mut EventQueue,
    result: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if lock(&(*this).data.state).shut {
        return MF_E_SHUTDOWN;
    }
    if !is_result(result) {
        return E_INVALIDARG;
    }
    *out = (*(result as *mut AsyncResult)).object.give();
    if (*out).is_null() {
        E_FAIL
    } else {
        S_OK
    }
}
unsafe fn push(this: *mut EventQueue, ev: Held) -> i32 {
    {
        let mut s = lock(&(*this).data.state);
        if s.shut {
            return MF_E_SHUTDOWN;
        }
        s.events.push_back(ev);
    }
    (*this).data.ready.notify_all();
    dispatch(this);
    S_OK
}
unsafe extern "win64" fn q_queue(this: *mut EventQueue, ev: *mut c_void) -> i32 {
    if ev.is_null() {
        return E_POINTER;
    }
    push(this, Held::take(ev))
}
unsafe extern "win64" fn q_queue_var(
    this: *mut EventQueue,
    met: u32,
    ext: *const u8,
    hr: i32,
    pv: *const u8,
) -> i32 {
    push(
        this,
        Held(mfcapture::create_event(met, ext, hr, pv) as usize),
    )
}
unsafe extern "win64" fn q_queue_unk(
    this: *mut EventQueue,
    met: u32,
    ext: *const u8,
    hr: i32,
    unk: *mut c_void,
) -> i32 {
    // PROPVARIANT VT_UNKNOWN (13), ou VT_EMPTY sans objet.
    let mut pv = [0u64; 3];
    if !unk.is_null() {
        pv[0] = 13;
        pv[1] = unk as u64;
    }
    push(
        this,
        Held(mfcapture::create_event(met, ext, hr, pv.as_ptr().cast()) as usize),
    )
}
unsafe extern "win64" fn q_shutdown(this: *mut EventQueue) -> i32 {
    let mut s = lock(&(*this).data.state);
    s.shut = true;
    s.events.clear();
    s.pending = None;
    drop(s);
    (*this).data.ready.notify_all();
    S_OK
}

static QUEUE_VTBL: VTable<10> = VTable([
    qi::<Queue> as *const (),
    addref::<Queue> as *const (),
    release::<Queue> as *const (),
    q_get_event as *const (),
    q_begin as *const (),
    q_end as *const (),
    q_queue as *const (),
    q_queue_var as *const (),
    q_queue_unk as *const (),
    q_shutdown as *const (),
]);

pub unsafe extern "win64" fn MFCreateEventQueue(out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_com(
        QUEUE_VTBL.0.as_ptr() as *const (),
        IID_IMFMEDIAEVENTQUEUE,
        Queue {
            state: Mutex::new(QueueState::default()),
            ready: Condvar::new(),
        },
    );
    S_OK
}

// ---------- Événements et descripteurs ----------

pub unsafe extern "win64" fn MFCreateMediaEvent(
    met: u32,
    ext: *const u8,
    hr: i32,
    pv: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = mfcapture::create_event(met, ext, hr, pv);
    S_OK
}

unsafe fn ptr_array<'a>(count: u32, arr: *const *mut c_void) -> Option<&'a [*mut c_void]> {
    if count == 0 {
        return Some(&[]);
    }
    if arr.is_null() {
        return None;
    }
    let s = std::slice::from_raw_parts(arr, count as usize);
    (!s.iter().any(|p| p.is_null())).then_some(s)
}

pub unsafe extern "win64" fn MFCreateStreamDescriptor(
    id: u32,
    count: u32,
    types: *const *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(t) = ptr_array(count, types) else {
        return E_INVALIDARG;
    };
    *out = mfcapture::create_stream_descriptor(id, t);
    S_OK
}

pub unsafe extern "win64" fn MFCreatePresentationDescriptor(
    count: u32,
    streams: *const *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(s) = ptr_array(count, streams) else {
        return E_INVALIDARG;
    };
    *out = mfcapture::create_presentation_descriptor(s);
    S_OK
}

// ---------- Sérialisation des attributs ----------

/// Notre forme (celle de Windows n'est pas documentée) : nombre d'éléments,
/// puis pour chacun la clé, le VARTYPE et la valeur ; relue seulement par
/// MFDeserializeAttributesFromStream.
fn encode(items: &[(Guid, Val)], byref: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let kept: Vec<_> = items
        .iter()
        .filter(|(_, v)| byref || !matches!(v, Val::Unk(_)))
        .collect();
    out.extend_from_slice(&(kept.len() as u32).to_le_bytes());
    for (k, v) in kept {
        out.extend_from_slice(k);
        let (tag, data): (u32, Vec<u8>) = match v {
            Val::U32(x) => (19, x.to_le_bytes().to_vec()),
            Val::U64(x) => (21, x.to_le_bytes().to_vec()),
            Val::F64(x) => (5, x.to_le_bytes().to_vec()),
            Val::Guid(g) => (72, g.to_vec()),
            Val::Str(s) => (31, s.iter().flat_map(|c| c.to_le_bytes()).collect()),
            Val::Blob(b) => (0x1011, b.clone()),
            Val::Unk(p) => (13, (*p as u64).to_le_bytes().to_vec()),
        };
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&data);
    }
    out
}

fn decode(bytes: &[u8]) -> Option<Vec<(Guid, Val)>> {
    let u32_at = |o: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(o..o + 4)?.try_into().ok()?))
    };
    let n = u32_at(0)? as usize;
    let mut pos = 4;
    let mut out = Vec::with_capacity(n.min(1024));
    for _ in 0..n {
        let k: Guid = bytes.get(pos..pos + 16)?.try_into().ok()?;
        let tag = u32_at(pos + 16)?;
        let len = u32_at(pos + 20)? as usize;
        let d = bytes.get(pos + 24..pos + 24 + len)?;
        pos += 24 + len;
        let v = match tag {
            19 => Val::U32(u32::from_le_bytes(d.try_into().ok()?)),
            21 => Val::U64(u64::from_le_bytes(d.try_into().ok()?)),
            5 => Val::F64(f64::from_le_bytes(d.try_into().ok()?)),
            72 => Val::Guid(d.try_into().ok()?),
            31 => Val::Str(
                d.as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_le_bytes(*c))
                    .collect(),
            ),
            0x1011 => Val::Blob(d.to_vec()),
            13 => Val::Unk(u64::from_le_bytes(d.try_into().ok()?) as usize),
            _ => return None,
        };
        out.push((k, v));
    }
    Some(out)
}

/// Éléments d'un IMFAttributes quelconque (GetCount, GetItemByIndex).
unsafe fn attribute_items(attr: *mut c_void) -> Result<Vec<(Guid, Val)>, i32> {
    let count: unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32 =
        std::mem::transmute(slot(attr, 30));
    let by_index: unsafe extern "win64" fn(*mut c_void, u32, *mut u8, *mut u8) -> i32 =
        std::mem::transmute(slot(attr, 31));
    let mut n = 0;
    let hr = count(attr, &mut n);
    if hr < 0 {
        return Err(hr);
    }
    let mut out = Vec::new();
    for i in 0..n {
        let mut key = [0u8; 16];
        let mut pv = [0u64; 3];
        let hr = by_index(attr, i, key.as_mut_ptr(), pv.as_mut_ptr().cast());
        if hr < 0 {
            return Err(hr);
        }
        let v = mfcapture::read_propvariant(pv.as_ptr().cast());
        mfcapture::clear_propvariant(pv.as_mut_ptr().cast());
        out.push((key, v.ok_or(MF_E_INVALIDMEDIATYPE)?));
    }
    Ok(out)
}

/// MFSerializeAttributesToStream(pAttr, dwOptions, pStm) : IStream::Write.
pub unsafe extern "win64" fn MFSerializeAttributesToStream(
    attr: *mut c_void,
    options: u32,
    stream: *mut c_void,
) -> i32 {
    if attr.is_null() || stream.is_null() {
        return E_POINTER;
    }
    let items = match attribute_items(attr) {
        Ok(i) => i,
        Err(hr) => return hr,
    };
    let data = encode(&items, options & SERIALIZE_UNKNOWN_BYREF != 0);
    let write: unsafe extern "win64" fn(*mut c_void, *const u8, u32, *mut u32) -> i32 =
        std::mem::transmute(slot(stream, 4));
    let mut written = 0;
    let hr = write(stream, data.as_ptr(), data.len() as u32, &mut written);
    if hr < 0 {
        hr
    } else if written as usize != data.len() {
        E_FAIL
    } else {
        S_OK
    }
}

/// MFDeserializeAttributesFromStream : relit notre forme par IStream::Read.
pub unsafe extern "win64" fn MFDeserializeAttributesFromStream(
    attr: *mut c_void,
    options: u32,
    stream: *mut c_void,
) -> i32 {
    if attr.is_null() || stream.is_null() {
        return E_POINTER;
    }
    let read: unsafe extern "win64" fn(*mut c_void, *mut u8, u32, *mut u32) -> i32 =
        std::mem::transmute(slot(stream, 3));
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let mut got = 0;
        let hr = read(stream, chunk.as_mut_ptr(), chunk.len() as u32, &mut got);
        if hr < 0 {
            return hr;
        }
        bytes.extend_from_slice(&chunk[..got as usize]);
        if got == 0 || hr == 1 {
            break;
        }
    }
    let Some(items) = decode(&bytes) else {
        return E_INVALIDARG;
    };
    let set_item: unsafe extern "win64" fn(*mut c_void, *const u8, *const u8) -> i32 =
        std::mem::transmute(slot(attr, 18));
    for (k, v) in items {
        if matches!(v, Val::Unk(_)) && options & SERIALIZE_UNKNOWN_BYREF == 0 {
            continue;
        }
        let mut pv = [0u64; 3];
        mfcapture::write_propvariant(pv.as_mut_ptr().cast(), &v);
        let hr = set_item(attr, k.as_ptr(), pv.as_ptr().cast());
        mfcapture::clear_propvariant(pv.as_mut_ptr().cast());
        if hr < 0 {
            return hr;
        }
    }
    S_OK
}

// ---------- Calculs de format ----------

/// MFFrameRateToAverageTimePerFrame : durée d'une image en 100 ns,
/// arrondie (30000/1001 donne 333 667).
pub unsafe extern "win64" fn MFFrameRateToAverageTimePerFrame(
    num: u32,
    den: u32,
    out: *mut u64,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = if num == 0 {
        0
    } else {
        (den as u64 * 10_000_000 + num as u64 / 2) / num as u64
    };
    S_OK
}

const fn fourcc(s: &[u8; 4]) -> u32 {
    u32::from_le_bytes(*s)
}

/// Octets par pixel (en huitièmes) et alignement de la ligne, pour un
/// code D3DFORMAT ou FOURCC.
fn layout(format: u32) -> Option<(u32, u32)> {
    Some(match format {
        // D3DFMT_A8R8G8B8, X8R8G8B8, A2R10G10B10, A8B8G8R8, X8B8G8R8.
        21 | 22 | 31 | 32 | 33 => (32, 4),
        // D3DFMT_R8G8B8.
        20 => (24, 4),
        // D3DFMT_R5G6B5, X1R5G5B5, A1R5G5B5.
        23..=25 => (16, 4),
        // D3DFMT_P8, L8.
        41 | 50 => (8, 4),
        f if f == fourcc(b"AYUV") => (32, 1),
        f if [b"YUY2", b"UYVY", b"YVYU", b"Y422"]
            .iter()
            .any(|c| fourcc(c) == f) =>
        {
            (16, 1)
        }
        // Formats planaires : la ligne du plan de luminance.
        f if [b"NV12", b"YV12", b"I420", b"IYUV", b"NV11", b"Y800"]
            .iter()
            .any(|c| fourcc(c) == f) =>
        {
            (8, 1)
        }
        f if [b"P010", b"P016"].iter().any(|c| fourcc(c) == f) => (16, 1),
        _ => return None,
    })
}

/// MFGetStrideForBitmapInfoHeader : pas minimal d'une ligne.
pub unsafe extern "win64" fn MFGetStrideForBitmapInfoHeader(
    format: u32,
    width: u32,
    out: *mut i32,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some((bits, align)) = layout(format) else {
        *out = 0;
        return MF_E_INVALIDMEDIATYPE;
    };
    let bytes = (width as u64 * bits as u64).div_ceil(8);
    *out = bytes.next_multiple_of(align as u64) as i32;
    S_OK
}

// ---------- DXGI ----------

/// Aucun gestionnaire de périphérique DXGI n'est encore servi
/// (MFCreateDXGIDeviceManager rend E_NOTIMPL) : rien à verrouiller.
pub unsafe extern "win64" fn MFLockDXGIDeviceManager(
    token: *mut u32,
    out: *mut *mut c_void,
) -> i32 {
    if !token.is_null() {
        *token = 0;
    }
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    MF_E_DXGI_DEVICE_NOT_INITIALIZED
}
pub unsafe extern "win64" fn MFUnlockDXGIDeviceManager() -> i32 {
    S_OK
}
pub unsafe extern "win64" fn MFCreateDXGISurfaceBuffer(
    _riid: *const u8,
    _surface: *mut c_void,
    _sub: u32,
    _bottom_up: i32,
    out: *mut *mut c_void,
) -> i32 {
    crate::trace!(
        "mfplat!MFCreateDXGISurfaceBuffer : surfaces DXGI pas encore servies (E_NOTIMPL)"
    );
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "MFCreateCollection" => fn MFCreateCollection,
        "MFCreateEventQueue" => fn MFCreateEventQueue,
        "MFCreateMediaEvent" => fn MFCreateMediaEvent,
        "MFCreateStreamDescriptor" => fn MFCreateStreamDescriptor,
        "MFCreatePresentationDescriptor" => fn MFCreatePresentationDescriptor,
        "MFInvokeCallback" => fn MFInvokeCallback,
        "MFSerializeAttributesToStream" => fn MFSerializeAttributesToStream,
        "MFDeserializeAttributesFromStream" => fn MFDeserializeAttributesFromStream,
        "MFFrameRateToAverageTimePerFrame" => fn MFFrameRateToAverageTimePerFrame,
        "MFGetStrideForBitmapInfoHeader" => fn MFGetStrideForBitmapInfoHeader,
        "MFLockDXGIDeviceManager" => fn MFLockDXGIDeviceManager,
        "MFUnlockDXGIDeviceManager" => fn MFUnlockDXGIDeviceManager,
        "MFCreateDXGISurfaceBuffer" => fn MFCreateDXGISurfaceBuffer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// Rappel de test : IMFAsyncCallback dont Invoke appelle EndGetEvent.
    #[repr(C)]
    struct Cb {
        vtbl: *const (),
        queue: *mut c_void,
        got: AtomicUsize,
    }
    unsafe extern "win64" fn cb_unused(_: *mut Cb) -> u32 {
        1
    }
    unsafe extern "win64" fn cb_params(_: *mut Cb, _: *mut u32, _: *mut u32) -> i32 {
        E_NOTIMPL
    }
    unsafe extern "win64" fn cb_invoke(this: *mut Cb, result: *mut c_void) -> i32 {
        let end: unsafe extern "win64" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> i32 =
            std::mem::transmute(slot((*this).queue, 5));
        let mut ev = std::ptr::null_mut();
        assert_eq!(end((*this).queue, result, &mut ev), 0);
        (*this).got.store(ev as usize, Ordering::SeqCst);
        S_OK
    }
    static CB_VTBL: VTable<5> = VTable([
        std::ptr::null(),
        cb_unused as *const (),
        cb_unused as *const (),
        cb_params as *const (),
        cb_invoke as *const (),
    ]);

    #[test]
    fn file_d_evenements_collection_et_formats() {
        unsafe {
            let mut q = std::ptr::null_mut();
            assert_eq!(MFCreateEventQueue(&mut q), 0);
            let get: unsafe extern "win64" fn(*mut c_void, u32, *mut *mut c_void) -> i32 =
                std::mem::transmute(slot(q, 3));
            let mut ev = std::ptr::null_mut();
            assert_eq!(get(q, 1, &mut ev), MF_E_NO_EVENTS_AVAILABLE);

            // QueueEventParamVar puis GetEvent sans attente.
            let qvar: unsafe extern "win64" fn(*mut c_void, u32, *const u8, i32, *const u8) -> i32 =
                std::mem::transmute(slot(q, 7));
            let empty = [0u64; 3];
            assert_eq!(
                qvar(q, 103, [0u8; 16].as_ptr(), 0, empty.as_ptr().cast()),
                0
            );
            assert_eq!(get(q, 1, &mut ev), 0);
            let ty: unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32 =
                std::mem::transmute(slot(ev, 33));
            let mut met = 0;
            assert_eq!(ty(ev, &mut met), 0);
            assert_eq!(met, 103);
            com_release(ev);

            // BeginGetEvent : le rappel reçoit l'événement mis en file ensuite.
            let mut cb = Cb {
                vtbl: CB_VTBL.0.as_ptr() as *const (),
                queue: q,
                got: AtomicUsize::new(0),
            };
            let begin: unsafe extern "win64" fn(*mut c_void, *mut c_void, *mut c_void) -> i32 =
                std::mem::transmute(slot(q, 4));
            let cbp = &mut cb as *mut Cb as *mut c_void;
            assert_eq!(begin(q, cbp, std::ptr::null_mut()), 0);
            assert_eq!(begin(q, cbp, std::ptr::null_mut()), MF_E_MULTIPLE_BEGIN);
            assert_eq!(cb.got.load(Ordering::SeqCst), 0);
            assert_eq!(
                qvar(q, 104, [0u8; 16].as_ptr(), 0, empty.as_ptr().cast()),
                0
            );
            let got = cb.got.load(Ordering::SeqCst) as *mut c_void;
            assert!(!got.is_null());
            assert_eq!(ty(got, &mut met), 0);
            assert_eq!(met, 104);
            com_release(got);
            let shutdown: unsafe extern "win64" fn(*mut c_void) -> i32 =
                std::mem::transmute(slot(q, 9));
            assert_eq!(shutdown(q), 0);
            assert_eq!(get(q, 1, &mut ev), MF_E_SHUTDOWN);
            com_release(q);

            // Collection.
            let mut c = std::ptr::null_mut();
            assert_eq!(MFCreateCollection(&mut c), 0);
            let add: unsafe extern "win64" fn(*mut c_void, *mut c_void) -> i32 =
                std::mem::transmute(slot(c, 5));
            let a = mfcapture::create_attributes();
            assert_eq!(add(c, a), 0);
            assert_eq!(add(c, std::ptr::null_mut()), 0);
            let count: unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32 =
                std::mem::transmute(slot(c, 3));
            let mut n = 0;
            assert_eq!(count(c, &mut n), 0);
            assert_eq!(n, 2);
            let remove: unsafe extern "win64" fn(*mut c_void, u32, *mut *mut c_void) -> i32 =
                std::mem::transmute(slot(c, 6));
            let mut r = std::ptr::null_mut();
            assert_eq!(remove(c, 0, &mut r), 0);
            assert_eq!(r, a);
            com_release(r);
            com_release(c);

            // Sérialisation aller-retour par un IStream en mémoire.
            let set_u32: unsafe extern "win64" fn(*mut c_void, *const u8, u32) -> i32 =
                std::mem::transmute(slot(a, 21));
            let key = [9u8; 16];
            set_u32(a, key.as_ptr(), 77);
            let items = attribute_items(a).unwrap();
            let back = decode(&encode(&items, false)).unwrap();
            assert_eq!(back, vec![(key, Val::U32(77))]);
            com_release(a);

            // Formats.
            let mut t = 0u64;
            assert_eq!(MFFrameRateToAverageTimePerFrame(30000, 1001, &mut t), 0);
            assert_eq!(t, 333_667);
            let mut stride = 0;
            assert_eq!(MFGetStrideForBitmapInfoHeader(20, 3, &mut stride), 0);
            assert_eq!(stride, 12);
            assert_eq!(
                MFGetStrideForBitmapInfoHeader(fourcc(b"NV12"), 641, &mut stride),
                0
            );
            assert_eq!(stride, 641);
            assert_eq!(
                MFGetStrideForBitmapInfoHeader(0x1234, 1, &mut stride),
                MF_E_INVALIDMEDIATYPE
            );
        }
    }
}
