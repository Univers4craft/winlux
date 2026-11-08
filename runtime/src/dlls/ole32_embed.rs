//! Objets OLE incorporés (#1236) : OleCreate, OleLoad, OleSave, OleRun,
//! OleIsRunning, OleLockRunning, OleSetContainedObject, OleDraw, et les
//! porte-avis CreateOleAdviseHolder / CreateDataAdviseHolder.
//!
//! L'objet est créé directement par son serveur (CoCreateInstance) et
//! piloté par ses propres interfaces (IOleObject, IPersistStorage,
//! IRunnableObject, IViewObject), comme le décrit la documentation publique
//! d'OLE et les en-têtes mingw-w64 (ole2.h, oleidl.h, objidl.h). Le
//! stockage composé vient de ole32_storage.rs (#1235).

use super::ole32::Guid;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const OLE_E_NOCONNECTION: i32 = 0x8004_0004u32 as i32;
const OLE_E_BLANK: i32 = 0x8004_0007u32 as i32;
/// CLSCTX_INPROC_SERVER | CLSCTX_INPROC_HANDLER | CLSCTX_LOCAL_SERVER
const CLSCTX_SERVER_AND_HANDLER: u32 = 0x1 | 0x2 | 0x4;

const ADVF_NODATA: u32 = 1;
const ADVF_PRIMEFIRST: u32 = 2;
const ADVF_ONLYONCE: u32 = 4;

const fn ole_guid(data1: u32) -> Guid {
    Guid {
        data1,
        data2: 0,
        data3: 0,
        data4: [0xC0, 0, 0, 0, 0, 0, 0, 0x46],
    }
}
const IID_IUNKNOWN: Guid = ole_guid(0);
const IID_IPERSISTSTORAGE: Guid = ole_guid(0x10A);
const IID_IVIEWOBJECT: Guid = ole_guid(0x10D);
const IID_IENUMSTATDATA: Guid = ole_guid(0x105);
const IID_IDATAADVISEHOLDER: Guid = ole_guid(0x110);
const IID_IOLEADVISEHOLDER: Guid = ole_guid(0x111);
const IID_IOLEOBJECT: Guid = ole_guid(0x112);
const IID_IRUNNABLEOBJECT: Guid = ole_guid(0x126);

unsafe fn slot(obj: usize, n: usize) -> usize {
    *(*(obj as *const *const usize)).add(n)
}
unsafe fn add_ref(obj: usize) {
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(slot(obj, 1));
    f(obj);
}
unsafe fn release(obj: usize) {
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(slot(obj, 2));
    f(obj);
}
/// QueryInterface : l'interface demandée, ou 0.
unsafe fn query(obj: usize, iid: &Guid) -> usize {
    let f: unsafe extern "win64" fn(usize, *const Guid, *mut usize) -> i32 =
        std::mem::transmute(slot(obj, 0));
    let mut out = 0usize;
    if f(obj, iid, &mut out) < 0 {
        0
    } else {
        out
    }
}

// ── Création, chargement, enregistrement ────────────────────────────

/// Donne son site client à l'objet (IOleObject::SetClientSite).
unsafe fn set_client_site(unk: usize, site: usize) -> i32 {
    if site == 0 {
        return S_OK;
    }
    let ole = query(unk, &IID_IOLEOBJECT);
    if ole == 0 {
        return S_OK;
    }
    let f: unsafe extern "win64" fn(usize, usize) -> i32 = std::mem::transmute(slot(ole, 3));
    let hr = f(ole, site);
    release(ole);
    hr
}

/// Rend l'interface demandée de l'objet créé, qui est relâché ensuite.
unsafe fn hand_out(unk: usize, riid: *const Guid, out: *mut usize) -> i32 {
    let iid = std::ptr::read_unaligned(riid);
    let q = query(unk, &iid);
    release(unk);
    if q == 0 {
        return E_NOINTERFACE;
    }
    *out = q;
    S_OK
}

/// OleCreate : crée l'objet, lui donne son site client puis l'initialise
/// dans le stockage (IPersistStorage::InitNew).
pub unsafe extern "win64" fn ole_create(
    clsid: *const Guid,
    riid: *const Guid,
    _render: u32,
    _format: usize,
    site: usize,
    storage: usize,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if clsid.is_null() || riid.is_null() {
        return E_INVALIDARG;
    }
    if crate::wow32::active() {
        return E_NOTIMPL;
    }
    let mut unk = 0usize;
    let hr = super::ole32::co_create_instance(
        clsid,
        0,
        CLSCTX_SERVER_AND_HANDLER,
        &IID_IUNKNOWN,
        (&mut unk as *mut usize).cast(),
    );
    if hr < 0 {
        return hr;
    }
    let hr = set_client_site(unk, site);
    if hr < 0 {
        release(unk);
        return hr;
    }
    if storage != 0 {
        let ps = query(unk, &IID_IPERSISTSTORAGE);
        if ps != 0 {
            let init: unsafe extern "win64" fn(usize, usize) -> i32 =
                std::mem::transmute(slot(ps, 5));
            let hr = init(ps, storage);
            release(ps);
            if hr < 0 {
                release(unk);
                return hr;
            }
        }
    }
    hand_out(unk, riid, out)
}

/// OleLoad : lit le CLSID du stockage, crée l'objet, lui donne son site
/// client et le charge (IPersistStorage::Load).
pub unsafe extern "win64" fn ole_load(
    storage: usize,
    riid: *const Guid,
    site: usize,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if storage == 0 || riid.is_null() {
        return E_INVALIDARG;
    }
    if crate::wow32::active() {
        return E_NOTIMPL;
    }
    let mut clsid = Guid::default();
    let hr = super::ole32_storage::read_class_stg(storage, &mut clsid);
    if hr < 0 {
        return hr;
    }
    if clsid == Guid::default() {
        return OLE_E_BLANK;
    }
    let mut unk = 0usize;
    let hr = super::ole32::co_create_instance(
        &clsid,
        0,
        CLSCTX_SERVER_AND_HANDLER,
        &IID_IUNKNOWN,
        (&mut unk as *mut usize).cast(),
    );
    if hr < 0 {
        return hr;
    }
    let hr = set_client_site(unk, site);
    if hr < 0 {
        release(unk);
        return hr;
    }
    let ps = query(unk, &IID_IPERSISTSTORAGE);
    if ps == 0 {
        release(unk);
        return E_NOINTERFACE;
    }
    let load: unsafe extern "win64" fn(usize, usize) -> i32 = std::mem::transmute(slot(ps, 6));
    let hr = load(ps, storage);
    release(ps);
    if hr < 0 {
        release(unk);
        return hr;
    }
    hand_out(unk, riid, out)
}

/// OleSave : écrit le CLSID de l'objet dans le stockage puis l'objet
/// lui-même (IPersistStorage::Save). SaveCompleted reste à l'appelant.
pub unsafe extern "win64" fn ole_save(ps: usize, storage: usize, same_as_load: i32) -> i32 {
    if ps == 0 || storage == 0 {
        return E_INVALIDARG;
    }
    let get_class: unsafe extern "win64" fn(usize, *mut Guid) -> i32 =
        std::mem::transmute(slot(ps, 3));
    let mut clsid = Guid::default();
    let hr = get_class(ps, &mut clsid);
    if hr < 0 {
        return hr;
    }
    let hr = super::ole32_storage::write_class_stg(storage, &clsid);
    if hr < 0 {
        return hr;
    }
    let save: unsafe extern "win64" fn(usize, usize, i32) -> i32 = std::mem::transmute(slot(ps, 7));
    save(ps, storage, same_as_load)
}

// ── État d'exécution et dessin ──────────────────────────────────────

/// Appelle l'entrée `n` d'IRunnableObject, ou rend `fallback` sans elle.
unsafe fn with_runnable(unk: usize, fallback: i32, call: impl FnOnce(usize) -> i32) -> i32 {
    if unk == 0 {
        return E_INVALIDARG;
    }
    let r = query(unk, &IID_IRUNNABLEOBJECT);
    if r == 0 {
        return fallback;
    }
    let hr = call(r);
    release(r);
    hr
}

pub unsafe extern "win64" fn ole_run(unk: usize) -> i32 {
    with_runnable(unk, S_OK, |r| {
        let f: unsafe extern "win64" fn(usize, usize) -> i32 = std::mem::transmute(slot(r, 4));
        f(r, 0)
    })
}

/// OleIsRunning : TRUE (1) ou FALSE (0).
pub unsafe extern "win64" fn ole_is_running(obj: usize) -> i32 {
    if obj == 0 {
        return 0;
    }
    with_runnable(obj, 1, |r| {
        let f: unsafe extern "win64" fn(usize) -> i32 = std::mem::transmute(slot(r, 5));
        (f(r) != 0) as i32
    })
}

pub unsafe extern "win64" fn ole_lock_running(
    unk: usize,
    lock: i32,
    last_unlock_closes: i32,
) -> i32 {
    with_runnable(unk, S_OK, |r| {
        let f: unsafe extern "win64" fn(usize, i32, i32) -> i32 = std::mem::transmute(slot(r, 6));
        f(r, lock, last_unlock_closes)
    })
}

pub unsafe extern "win64" fn ole_set_contained_object(unk: usize, contained: i32) -> i32 {
    with_runnable(unk, S_OK, |r| {
        let f: unsafe extern "win64" fn(usize, i32) -> i32 = std::mem::transmute(slot(r, 7));
        f(r, contained)
    })
}

/// OleDraw : IViewObject::Draw dans le rectangle donné (RECT → RECTL).
pub unsafe extern "win64" fn ole_draw(
    unk: usize,
    aspect: u32,
    hdc: usize,
    rect: *const i32,
) -> i32 {
    if unk == 0 || rect.is_null() {
        return E_INVALIDARG;
    }
    let view = query(unk, &IID_IVIEWOBJECT);
    if view == 0 {
        return OLE_E_BLANK;
    }
    let bounds: [i32; 4] = std::ptr::read_unaligned(rect as *const [i32; 4]);
    type Draw = unsafe extern "win64" fn(
        usize,
        u32,
        i32,
        usize,
        usize,
        usize,
        usize,
        *const i32,
        usize,
        usize,
        usize,
    ) -> i32;
    let draw: Draw = std::mem::transmute(slot(view, 3));
    let hr = draw(view, aspect, -1, 0, 0, 0, hdc, bounds.as_ptr(), 0, 0, 0);
    release(view);
    hr
}

// ── Porte-avis ──────────────────────────────────────────────────────

struct Vtbl<const N: usize>([*const (); N]);
unsafe impl<const N: usize> Sync for Vtbl<N> {}

/// Une connexion : FORMATETC (32 octets), ADVF, puits, numéro.
#[derive(Clone, Copy)]
struct Advise {
    format: [u64; 4],
    advf: u32,
    sink: usize,
    conn: u32,
}

/// Porte-avis : IOleAdviseHolder ou IDataAdviseHolder selon la table.
#[repr(C)]
struct HolderObj {
    vtbl: *const usize,
    refs: AtomicU32,
    list: Mutex<Vec<Advise>>,
    next: AtomicU32,
}

unsafe fn holder(this: usize) -> &'static HolderObj {
    &*(this as *const HolderObj)
}

fn new_holder(vtbl: *const usize) -> usize {
    Box::into_raw(Box::new(HolderObj {
        vtbl,
        refs: AtomicU32::new(1),
        list: Mutex::new(Vec::new()),
        next: AtomicU32::new(1),
    })) as usize
}

unsafe extern "win64" fn holder_add_ref(this: usize) -> u32 {
    holder(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn holder_release(this: usize) -> u32 {
    let n = holder(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let h = Box::from_raw(this as *mut HolderObj);
        for a in h.list.into_inner().unwrap_or_else(|e| e.into_inner()) {
            release(a.sink);
        }
    }
    n
}
unsafe fn holder_qi(this: usize, iid: *const Guid, out: *mut usize, own: &Guid) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let g = std::ptr::read_unaligned(iid);
    if g == IID_IUNKNOWN || g == *own {
        holder_add_ref(this);
        *out = this;
        return S_OK;
    }
    *out = 0;
    E_NOINTERFACE
}
unsafe fn holder_add(this: usize, a: Advise) -> u32 {
    let h = holder(this);
    let conn = h.next.fetch_add(1, Ordering::AcqRel);
    add_ref(a.sink);
    h.list
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(Advise { conn, ..a });
    conn
}
unsafe extern "win64" fn holder_unadvise(this: usize, conn: u32) -> i32 {
    let mut l = holder(this).list.lock().unwrap_or_else(|e| e.into_inner());
    let Some(i) = l.iter().position(|a| a.conn == conn) else {
        return OLE_E_NOCONNECTION;
    };
    let a = l.remove(i);
    drop(l);
    release(a.sink);
    S_OK
}
unsafe extern "win64" fn holder_enum_advise(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let items = holder(this)
        .list
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    for a in &items {
        add_ref(a.sink);
    }
    *out = new_enum(items, 0);
    S_OK
}
fn sinks(this: usize) -> Vec<usize> {
    unsafe {
        let l = holder(this).list.lock().unwrap_or_else(|e| e.into_inner());
        l.iter()
            .map(|a| {
                add_ref(a.sink);
                a.sink
            })
            .collect()
    }
}
/// Appelle l'entrée `n` d'IAdviseSink de chaque puits (sans le verrou).
unsafe fn notify(this: usize, n: usize, arg: usize) {
    for s in sinks(this) {
        let f: unsafe extern "win64" fn(usize, usize) = std::mem::transmute(slot(s, n));
        f(s, arg);
        release(s);
    }
}

static OLE_HOLDER_VTBL: Vtbl<9> = Vtbl([
    ole_holder_qi as *const (),
    holder_add_ref as *const (),
    holder_release as *const (),
    ole_holder_advise as *const (),
    holder_unadvise as *const (),
    holder_enum_advise as *const (),
    ole_holder_send_on_rename as *const (),
    ole_holder_send_on_save as *const (),
    ole_holder_send_on_close as *const (),
]);

unsafe extern "win64" fn ole_holder_qi(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    holder_qi(this, iid, out, &IID_IOLEADVISEHOLDER)
}
unsafe extern "win64" fn ole_holder_advise(this: usize, sink: usize, conn: *mut u32) -> i32 {
    if conn.is_null() {
        return E_POINTER;
    }
    *conn = 0;
    if sink == 0 {
        return E_INVALIDARG;
    }
    *conn = holder_add(
        this,
        Advise {
            format: [0; 4],
            advf: 0,
            sink,
            conn: 0,
        },
    );
    S_OK
}
unsafe extern "win64" fn ole_holder_send_on_rename(this: usize, moniker: usize) -> i32 {
    notify(this, 5, moniker);
    S_OK
}
unsafe extern "win64" fn ole_holder_send_on_save(this: usize) -> i32 {
    notify(this, 6, 0);
    S_OK
}
unsafe extern "win64" fn ole_holder_send_on_close(this: usize) -> i32 {
    notify(this, 7, 0);
    S_OK
}

static DATA_HOLDER_VTBL: Vtbl<7> = Vtbl([
    data_holder_qi as *const (),
    holder_add_ref as *const (),
    holder_release as *const (),
    data_holder_advise as *const (),
    holder_unadvise as *const (),
    holder_enum_advise as *const (),
    data_holder_send_on_data_change as *const (),
]);

unsafe extern "win64" fn data_holder_qi(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    holder_qi(this, iid, out, &IID_IDATAADVISEHOLDER)
}

/// Prévient un puits d'un changement de données : GetData sur l'objet de
/// données, sauf ADVF_NODATA (médium vide).
unsafe fn send_data(data_obj: usize, a: &Advise) {
    let mut medium = [0u64; 3];
    let mut got = false;
    if a.advf & ADVF_NODATA == 0 && data_obj != 0 {
        let get: unsafe extern "win64" fn(usize, *const u64, *mut u64) -> i32 =
            std::mem::transmute(slot(data_obj, 3));
        got = get(data_obj, a.format.as_ptr(), medium.as_mut_ptr()) >= 0;
        if !got {
            medium = [0; 3];
        }
    }
    let f: unsafe extern "win64" fn(usize, *const u64, *mut u64) =
        std::mem::transmute(slot(a.sink, 3));
    f(a.sink, a.format.as_ptr(), medium.as_mut_ptr());
    if got {
        super::dataobject::release_stg_medium(medium.as_mut_ptr().cast());
    }
}

unsafe extern "win64" fn data_holder_advise(
    this: usize,
    data_obj: usize,
    format: *const u64,
    advf: u32,
    sink: usize,
    conn: *mut u32,
) -> i32 {
    if conn.is_null() {
        return E_POINTER;
    }
    *conn = 0;
    if sink == 0 || format.is_null() {
        return E_INVALIDARG;
    }
    let a = Advise {
        format: std::ptr::read_unaligned(format as *const [u64; 4]),
        advf,
        sink,
        conn: 0,
    };
    if advf & ADVF_PRIMEFIRST != 0 {
        send_data(data_obj, &a);
        if advf & ADVF_ONLYONCE != 0 {
            return S_OK;
        }
    }
    *conn = holder_add(this, a);
    S_OK
}

unsafe extern "win64" fn data_holder_send_on_data_change(
    this: usize,
    data_obj: usize,
    _reserved: u32,
    advf: u32,
) -> i32 {
    let list = holder(this)
        .list
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    for a in list {
        add_ref(a.sink);
        let mut b = a;
        b.advf |= advf & ADVF_NODATA;
        send_data(data_obj, &b);
        release(a.sink);
        if a.advf & ADVF_ONLYONCE != 0 {
            holder_unadvise(this, a.conn);
        }
    }
    S_OK
}

pub unsafe extern "win64" fn create_ole_advise_holder(out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_holder(OLE_HOLDER_VTBL.0.as_ptr().cast());
    S_OK
}

pub unsafe extern "win64" fn create_data_advise_holder(out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_holder(DATA_HOLDER_VTBL.0.as_ptr().cast());
    S_OK
}

// ── IEnumSTATDATA ───────────────────────────────────────────────────

/// Instantané des connexions ; chaque puits y est tenu (AddRef).
#[repr(C)]
struct EnumObj {
    vtbl: *const usize,
    refs: AtomicU32,
    items: Vec<Advise>,
    pos: Mutex<usize>,
}

static ENUM_VTBL: Vtbl<7> = Vtbl([
    enum_qi as *const (),
    enum_add_ref as *const (),
    enum_release as *const (),
    enum_next as *const (),
    enum_skip as *const (),
    enum_reset as *const (),
    enum_clone as *const (),
]);

fn new_enum(items: Vec<Advise>, pos: usize) -> usize {
    Box::into_raw(Box::new(EnumObj {
        vtbl: ENUM_VTBL.0.as_ptr().cast(),
        refs: AtomicU32::new(1),
        items,
        pos: Mutex::new(pos),
    })) as usize
}
unsafe fn en(this: usize) -> &'static EnumObj {
    &*(this as *const EnumObj)
}
unsafe extern "win64" fn enum_qi(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let g = std::ptr::read_unaligned(iid);
    if g == IID_IUNKNOWN || g == IID_IENUMSTATDATA {
        enum_add_ref(this);
        *out = this;
        return S_OK;
    }
    *out = 0;
    E_NOINTERFACE
}
unsafe extern "win64" fn enum_add_ref(this: usize) -> u32 {
    en(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn enum_release(this: usize) -> u32 {
    let n = en(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let e = Box::from_raw(this as *mut EnumObj);
        for a in &e.items {
            release(a.sink);
        }
    }
    n
}
/// Next : STATDATA x64 de 56 octets (FORMATETC 32, advf @32, puits @40,
/// connexion @48) ; le puits rendu est tenu pour l'appelant.
unsafe extern "win64" fn enum_next(this: usize, celt: u32, out: *mut u8, fetched: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let o = en(this);
    let mut pos = o.pos.lock().unwrap_or_else(|e| e.into_inner());
    let mut n = 0usize;
    while n < celt as usize && *pos < o.items.len() {
        let a = &o.items[*pos];
        let p = out.add(n * 56);
        std::ptr::write_bytes(p, 0, 56);
        std::ptr::write_unaligned(p as *mut [u64; 4], a.format);
        std::ptr::write_unaligned(p.add(32) as *mut u32, a.advf);
        add_ref(a.sink);
        std::ptr::write_unaligned(p.add(40) as *mut usize, a.sink);
        std::ptr::write_unaligned(p.add(48) as *mut u32, a.conn);
        n += 1;
        *pos += 1;
    }
    if !fetched.is_null() {
        *fetched = n as u32;
    }
    if n == celt as usize {
        S_OK
    } else {
        S_FALSE
    }
}
unsafe extern "win64" fn enum_skip(this: usize, celt: u32) -> i32 {
    let o = en(this);
    let mut pos = o.pos.lock().unwrap_or_else(|e| e.into_inner());
    *pos += celt as usize;
    if *pos <= o.items.len() {
        S_OK
    } else {
        S_FALSE
    }
}
unsafe extern "win64" fn enum_reset(this: usize) -> i32 {
    *en(this).pos.lock().unwrap_or_else(|e| e.into_inner()) = 0;
    S_OK
}
unsafe extern "win64" fn enum_clone(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let o = en(this);
    for a in &o.items {
        add_ref(a.sink);
    }
    let pos = *o.pos.lock().unwrap_or_else(|e| e.into_inner());
    *out = new_enum(o.items.clone(), pos);
    S_OK
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "OleCreate" => fn ole_create,
        "OleLoad" => fn ole_load,
        "OleSave" => fn ole_save,
        "OleRun" => fn ole_run,
        "OleIsRunning" => fn ole_is_running,
        "OleLockRunning" => fn ole_lock_running,
        "OleSetContainedObject" => fn ole_set_contained_object,
        "OleDraw" => fn ole_draw,
        "CreateOleAdviseHolder" => fn create_ole_advise_holder,
        "CreateDataAdviseHolder" => fn create_data_advise_holder
    }
}

#[cfg(test)]
mod tests;
