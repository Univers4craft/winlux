//! Monikers d'ole32 (lot C du relevé, #1511) : monikers de fichier,
//! d'élément et de pointeur, contexte de liaison (IBindCtx), table des
//! objets actifs (IRunningObjectTable), MkParseDisplayName et
//! CoGetObject.
//!
//! Sources : documentation Microsoft Learn d'IMoniker, IBindCtx,
//! IRunningObjectTable, IEnumMoniker et des fonctions Create*Moniker ;
//! ordre des méthodes d'objidl.h de MinGW-w64 (domaine public). Noms
//! exportés : bibliothèque d'importation Ole32.Lib du SDK Windows public.
//!
//! Winlux n'a ni moniker de classe ni moniker composite générique : un nom
//! comme « winmgmts: » ou « clsid:… » est refusé par MkParseDisplayName
//! (MK_E_SYNTAX), sans planter. Un moniker de fichier se lie à l'objet
//! inscrit sous son nom dans la table des objets actifs.

use super::comobj::{guid, Guid, VTable};
use super::ole32_lot_c::{add_ref, query, read_guid, release, slot, IID_IUNKNOWN};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
const MK_E_NEEDGENERIC: i32 = 0x8004_01E2u32 as i32;
const MK_E_UNAVAILABLE: i32 = 0x8004_01E3u32 as i32;
const MK_E_SYNTAX: i32 = 0x8004_01E4u32 as i32;
const MK_E_NOOBJECT: i32 = 0x8004_01E5u32 as i32;
const MK_E_NOTBOUND: i32 = 0x8004_01E9u32 as i32;
const MK_E_CANTOPENFILE: i32 = 0x8004_01EAu32 as i32;
const MK_E_NOINVERSE: i32 = 0x8004_01ECu32 as i32;
const MK_E_NOSTORAGE: i32 = 0x8004_01EDu32 as i32;
const MK_E_NOPREFIX: i32 = 0x8004_01EEu32 as i32;
const MK_S_REDUCED_TO_SELF: i32 = 0x0004_01E2;
const MK_S_US: i32 = 0x0004_01E6;
const MK_S_MONIKERALREADYREGISTERED: i32 = 0x0004_01E7;

const fn ole_guid(d1: u32) -> Guid {
    guid(d1, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46])
}

const IID_IBIND_CTX: Guid = ole_guid(0x0000_000E);
const IID_IMONIKER: Guid = ole_guid(0x0000_000F);
const IID_IRUNNING_OBJECT_TABLE: Guid = ole_guid(0x0000_0010);
const IID_IENUM_MONIKER: Guid = ole_guid(0x0000_0102);
const IID_IPERSIST_STREAM: Guid = ole_guid(0x0000_0109);
const IID_IPERSIST: Guid = ole_guid(0x0000_010C);
const CLSID_FILE_MONIKER: Guid = ole_guid(0x0000_0303);
const CLSID_ITEM_MONIKER: Guid = ole_guid(0x0000_0304);
const CLSID_POINTER_MONIKER: Guid = ole_guid(0x0000_0306);

const MKSYS_FILEMONIKER: u32 = 2;
const MKSYS_ITEMMONIKER: u32 = 4;
const MKSYS_POINTERMONIKER: u32 = 5;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

unsafe fn wide(p: *const u16) -> String {
    super::oleaut32_typelib::wide(p)
}

/// Chaîne CoTaskMem terminée par zéro.
unsafe fn task_string(s: &str) -> *mut u16 {
    let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    let p = super::ole32::co_task_mem_alloc(w.len() * 2) as *mut u16;
    if !p.is_null() {
        std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
    }
    p
}

fn filetime_now() -> u64 {
    let mut t = 0u64;
    unsafe { super::ole32_lot_c::co_file_time_now(&mut t) };
    t
}

// ---------- Monikers ----------

enum Kind {
    File(String),
    Item { delim: String, item: String },
    Pointer(usize),
}

#[repr(C)]
struct Moniker {
    vtbl: *const (),
    refs: AtomicU32,
    kind: Kind,
}

static MONIKER_VT: VTable<23> = VTable([
    mk_qi as *const (),
    mk_add_ref as *const (),
    mk_release as *const (),
    mk_get_class_id as *const (),
    mk_is_dirty as *const (),
    mk_load as *const (),
    mk_save as *const (),
    mk_get_size_max as *const (),
    mk_bind_to_object as *const (),
    mk_bind_to_storage as *const (),
    mk_reduce as *const (),
    mk_compose_with as *const (),
    mk_enum as *const (),
    mk_is_equal as *const (),
    mk_hash as *const (),
    mk_is_running as *const (),
    mk_get_time_of_last_change as *const (),
    mk_inverse as *const (),
    mk_common_prefix_with as *const (),
    mk_relative_path_to as *const (),
    mk_get_display_name as *const (),
    mk_parse_display_name as *const (),
    mk_is_system_moniker as *const (),
]);

fn new_moniker(kind: Kind) -> usize {
    Box::into_raw(Box::new(Moniker {
        vtbl: &MONIKER_VT as *const _ as *const (),
        refs: AtomicU32::new(1),
        kind,
    })) as usize
}

/// Moniker écrit ici (et non par le programme) ?
unsafe fn ours(p: usize) -> Option<&'static Moniker> {
    if p != 0 && *(p as *const *const ()) == &MONIKER_VT as *const _ as *const () {
        Some(&*(p as *const Moniker))
    } else {
        None
    }
}

unsafe fn this(p: usize) -> &'static Moniker {
    &*(p as *const Moniker)
}

impl Moniker {
    fn display(&self) -> Option<String> {
        match &self.kind {
            Kind::File(path) => Some(path.clone()),
            Kind::Item { delim, item } => Some(format!("{delim}{item}")),
            Kind::Pointer(_) => None,
        }
    }

    fn same(&self, other: &Moniker) -> bool {
        match (&self.kind, &other.kind) {
            (Kind::File(a), Kind::File(b)) => a.eq_ignore_ascii_case(b),
            (Kind::Item { item: a, .. }, Kind::Item { item: b, .. }) => a.eq_ignore_ascii_case(b),
            (Kind::Pointer(a), Kind::Pointer(b)) => a == b,
            _ => false,
        }
    }
}

unsafe extern "win64" fn mk_qi(p: usize, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    let Some(g) = read_guid(iid) else {
        return E_INVALIDARG;
    };
    if [
        IID_IUNKNOWN,
        IID_IMONIKER,
        IID_IPERSIST,
        IID_IPERSIST_STREAM,
    ]
    .contains(&g)
    {
        mk_add_ref(p);
        *out = p;
        return S_OK;
    }
    E_NOINTERFACE
}

unsafe extern "win64" fn mk_add_ref(p: usize) -> u32 {
    this(p).refs.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "win64" fn mk_release(p: usize) -> u32 {
    let left = this(p).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if left == 0 {
        let b = Box::from_raw(p as *mut Moniker);
        if let Kind::Pointer(punk) = b.kind {
            if punk != 0 {
                release(punk);
            }
        }
    }
    left
}

unsafe extern "win64" fn mk_get_class_id(p: usize, out: *mut Guid) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let g = match this(p).kind {
        Kind::File(_) => CLSID_FILE_MONIKER,
        Kind::Item { .. } => CLSID_ITEM_MONIKER,
        Kind::Pointer(_) => CLSID_POINTER_MONIKER,
    };
    std::ptr::write_unaligned(out, g);
    S_OK
}

extern "win64" fn mk_is_dirty(_p: usize) -> i32 {
    S_FALSE
}

/// Le format persistant des monikers de Windows n'est pas documenté :
/// ni lecture ni écriture.
extern "win64" fn mk_load(_p: usize, _stm: usize) -> i32 {
    E_NOTIMPL
}

extern "win64" fn mk_save(_p: usize, _stm: usize, _clear: i32) -> i32 {
    E_NOTIMPL
}

unsafe extern "win64" fn mk_get_size_max(_p: usize, out: *mut u64) -> i32 {
    if !out.is_null() {
        std::ptr::write_unaligned(out, 0);
    }
    E_NOTIMPL
}

unsafe extern "win64" fn mk_bind_to_object(
    p: usize,
    _bc: usize,
    _left: usize,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    let m = this(p);
    if let Kind::Pointer(punk) = m.kind {
        return if punk == 0 {
            E_NOINTERFACE
        } else {
            query(punk, iid, out)
        };
    }
    if let Some(obj) = rot_object(p) {
        let hr = query(obj, iid, out);
        release(obj);
        return hr;
    }
    match m.kind {
        Kind::File(_) => MK_E_CANTOPENFILE,
        _ => MK_E_NOOBJECT,
    }
}

unsafe extern "win64" fn mk_bind_to_storage(
    p: usize,
    bc: usize,
    left: usize,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    if let Kind::Pointer(_) = this(p).kind {
        return mk_bind_to_object(p, bc, left, iid, out);
    }
    if !out.is_null() {
        *out = 0;
    }
    MK_E_NOSTORAGE
}

unsafe extern "win64" fn mk_reduce(
    p: usize,
    _bc: usize,
    _how_far: u32,
    _left: *mut usize,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    mk_add_ref(p);
    *out = p;
    MK_S_REDUCED_TO_SELF
}

unsafe extern "win64" fn mk_compose_with(
    _p: usize,
    _right: usize,
    only_if_not_generic: i32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if only_if_not_generic != 0 {
        MK_E_NEEDGENERIC
    } else {
        E_NOTIMPL
    }
}

unsafe extern "win64" fn mk_enum(_p: usize, _forward: i32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}

unsafe extern "win64" fn mk_is_equal(p: usize, other: usize) -> i32 {
    match ours(other) {
        Some(o) if this(p).same(o) => S_OK,
        _ => S_FALSE,
    }
}

unsafe extern "win64" fn mk_hash(p: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let m = this(p);
    let text = match &m.kind {
        Kind::Pointer(punk) => format!("{punk:x}"),
        Kind::Item { item, .. } => item.to_lowercase(),
        Kind::File(path) => path.to_lowercase(),
    };
    // FNV-1a : seule l'égalité des hachages de deux monikers égaux compte.
    let mut h: u32 = 0x811C_9DC5;
    for b in text.bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    *out = h;
    S_OK
}

unsafe extern "win64" fn mk_is_running(p: usize, _bc: usize, _left: usize, _newly: usize) -> i32 {
    if let Kind::Pointer(_) = this(p).kind {
        return S_OK;
    }
    match rot_object(p) {
        Some(obj) => {
            release(obj);
            S_OK
        }
        None => S_FALSE,
    }
}

unsafe extern "win64" fn mk_get_time_of_last_change(
    p: usize,
    _bc: usize,
    _left: usize,
    out: *mut u64,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if let Some(t) = rot_time(p) {
        std::ptr::write_unaligned(out, t);
        return S_OK;
    }
    if let Kind::File(path) = &this(p).kind {
        let modified = std::fs::metadata(crate::paths::to_linux(path))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
        if let Some(d) = modified {
            let ticks = d.as_secs() * 10_000_000 + u64::from(d.subsec_nanos() / 100);
            std::ptr::write_unaligned(out, ticks + 116_444_736_000_000_000);
            return S_OK;
        }
    }
    MK_E_UNAVAILABLE
}

unsafe extern "win64" fn mk_inverse(_p: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    MK_E_NOINVERSE
}

unsafe extern "win64" fn mk_common_prefix_with(p: usize, other: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if mk_is_equal(p, other) == S_OK {
        mk_add_ref(p);
        *out = p;
        return MK_S_US;
    }
    MK_E_NOPREFIX
}

unsafe extern "win64" fn mk_relative_path_to(_p: usize, _other: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    E_NOTIMPL
}

unsafe extern "win64" fn mk_get_display_name(
    p: usize,
    _bc: usize,
    _left: usize,
    out: *mut *mut u16,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(text) = this(p).display() else {
        return E_NOTIMPL;
    };
    *out = task_string(&text);
    if (*out).is_null() {
        E_OUTOFMEMORY
    } else {
        S_OK
    }
}

unsafe extern "win64" fn mk_parse_display_name(
    _p: usize,
    _bc: usize,
    _left: usize,
    _name: *const u16,
    eaten: *mut u32,
    out: *mut usize,
) -> i32 {
    if !eaten.is_null() {
        *eaten = 0;
    }
    if !out.is_null() {
        *out = 0;
    }
    MK_E_SYNTAX
}

unsafe extern "win64" fn mk_is_system_moniker(p: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = match this(p).kind {
        Kind::File(_) => MKSYS_FILEMONIKER,
        Kind::Item { .. } => MKSYS_ITEMMONIKER,
        Kind::Pointer(_) => MKSYS_POINTERMONIKER,
    };
    S_OK
}

pub unsafe extern "win64" fn create_file_moniker(path: *const u16, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if path.is_null() {
        return E_INVALIDARG;
    }
    *out = new_moniker(Kind::File(wide(path)));
    S_OK
}

pub unsafe extern "win64" fn create_item_moniker(
    delim: *const u16,
    item: *const u16,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if item.is_null() {
        return E_INVALIDARG;
    }
    let delim = if delim.is_null() {
        String::new()
    } else {
        wide(delim)
    };
    *out = new_moniker(Kind::Item {
        delim,
        item: wide(item),
    });
    S_OK
}

pub unsafe extern "win64" fn create_pointer_moniker(punk: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if punk != 0 {
        add_ref(punk);
    }
    *out = new_moniker(Kind::Pointer(punk));
    S_OK
}

// ---------- Table des objets actifs ----------

struct RotEntry {
    cookie: u32,
    /// Nom d'affichage en minuscules, ou adresse du moniker sans nom.
    key: String,
    moniker: usize,
    object: usize,
    time: u64,
}

static ROT: Mutex<Vec<RotEntry>> = Mutex::new(Vec::new());
static NEXT_ROT_COOKIE: AtomicU32 = AtomicU32::new(1);

/// Clé d'un moniker : son nom d'affichage, comparé sans la casse.
unsafe fn moniker_key(mk: usize) -> String {
    if let Some(m) = ours(mk) {
        if let Some(text) = m.display() {
            return text.to_lowercase();
        }
        if let Kind::Pointer(punk) = m.kind {
            return format!("\u{1}pointer:{punk:x}");
        }
    }
    let get: unsafe extern "win64" fn(usize, usize, usize, *mut *mut u16) -> i32 =
        std::mem::transmute(slot(mk, 20));
    let mut name: *mut u16 = std::ptr::null_mut();
    if get(mk, 0, 0, &mut name) >= 0 && !name.is_null() {
        let text = wide(name).to_lowercase();
        super::ole32::co_task_mem_free(name.cast());
        return text;
    }
    format!("\u{1}moniker:{mk:x}")
}

/// Objet inscrit sous ce moniker, référence prise.
unsafe fn rot_object(mk: usize) -> Option<usize> {
    let key = moniker_key(mk);
    let obj = lock(&ROT).iter().rev().find(|e| e.key == key)?.object;
    add_ref(obj);
    Some(obj)
}

unsafe fn rot_time(mk: usize) -> Option<u64> {
    let key = moniker_key(mk);
    lock(&ROT)
        .iter()
        .rev()
        .find(|e| e.key == key)
        .map(|e| e.time)
}

#[repr(C)]
struct Rot {
    vtbl: *const (),
}
unsafe impl Sync for Rot {}

static ROT_OBJECT: Rot = Rot {
    vtbl: &ROT_VT as *const _ as *const (),
};

static ROT_VT: VTable<10> = VTable([
    rot_qi as *const (),
    static_add_ref as *const (),
    static_release as *const (),
    rot_register as *const (),
    rot_revoke as *const (),
    rot_is_running as *const (),
    rot_get_object as *const (),
    rot_note_change_time as *const (),
    rot_get_time_of_last_change as *const (),
    rot_enum_running as *const (),
]);

extern "win64" fn static_add_ref(_p: usize) -> u32 {
    2
}

extern "win64" fn static_release(_p: usize) -> u32 {
    1
}

unsafe extern "win64" fn rot_qi(p: usize, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    match read_guid(iid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IRUNNING_OBJECT_TABLE => {
            *out = p;
            S_OK
        }
        Some(_) => E_NOINTERFACE,
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn rot_register(
    _p: usize,
    _flags: u32,
    object: usize,
    mk: usize,
    cookie: *mut u32,
) -> i32 {
    if cookie.is_null() {
        return E_POINTER;
    }
    *cookie = 0;
    if object == 0 || mk == 0 {
        return E_INVALIDARG;
    }
    let key = moniker_key(mk);
    add_ref(object);
    add_ref(mk);
    let c = NEXT_ROT_COOKIE.fetch_add(1, Ordering::Relaxed);
    let mut rot = lock(&ROT);
    let already = rot.iter().any(|e| e.key == key);
    rot.push(RotEntry {
        cookie: c,
        key,
        moniker: mk,
        object,
        time: filetime_now(),
    });
    *cookie = c;
    if already {
        MK_S_MONIKERALREADYREGISTERED
    } else {
        S_OK
    }
}

unsafe extern "win64" fn rot_revoke(_p: usize, cookie: u32) -> i32 {
    let entry = {
        let mut rot = lock(&ROT);
        rot.iter()
            .position(|e| e.cookie == cookie)
            .map(|i| rot.remove(i))
    };
    match entry {
        Some(e) => {
            release(e.object);
            release(e.moniker);
            S_OK
        }
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn rot_is_running(_p: usize, mk: usize) -> i32 {
    if mk == 0 {
        return E_INVALIDARG;
    }
    match rot_object(mk) {
        Some(obj) => {
            release(obj);
            S_OK
        }
        None => S_FALSE,
    }
}

unsafe extern "win64" fn rot_get_object(_p: usize, mk: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if mk == 0 {
        return E_INVALIDARG;
    }
    match rot_object(mk) {
        Some(obj) => {
            *out = obj;
            S_OK
        }
        None => MK_E_UNAVAILABLE,
    }
}

unsafe extern "win64" fn rot_note_change_time(_p: usize, cookie: u32, time: *const u64) -> i32 {
    if time.is_null() {
        return E_INVALIDARG;
    }
    match lock(&ROT).iter_mut().find(|e| e.cookie == cookie) {
        Some(e) => {
            e.time = std::ptr::read_unaligned(time);
            S_OK
        }
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn rot_get_time_of_last_change(_p: usize, mk: usize, out: *mut u64) -> i32 {
    if out.is_null() || mk == 0 {
        return E_INVALIDARG;
    }
    match rot_time(mk) {
        Some(t) => {
            std::ptr::write_unaligned(out, t);
            S_OK
        }
        None => MK_E_UNAVAILABLE,
    }
}

unsafe extern "win64" fn rot_enum_running(_p: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let items: Vec<usize> = lock(&ROT).iter().map(|e| e.moniker).collect();
    for &m in &items {
        add_ref(m);
    }
    *out = new_enum(items, 0);
    S_OK
}

pub unsafe extern "win64" fn get_running_object_table(reserved: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if reserved != 0 {
        return E_INVALIDARG;
    }
    *out = &ROT_OBJECT as *const Rot as usize;
    S_OK
}

// ---------- IEnumMoniker ----------

#[repr(C)]
struct EnumMoniker {
    vtbl: *const (),
    refs: AtomicU32,
    state: Mutex<(Vec<usize>, usize)>,
}

static ENUM_VT: VTable<7> = VTable([
    en_qi as *const (),
    en_add_ref as *const (),
    en_release as *const (),
    en_next as *const (),
    en_skip as *const (),
    en_reset as *const (),
    en_clone as *const (),
]);

fn new_enum(items: Vec<usize>, pos: usize) -> usize {
    Box::into_raw(Box::new(EnumMoniker {
        vtbl: &ENUM_VT as *const _ as *const (),
        refs: AtomicU32::new(1),
        state: Mutex::new((items, pos)),
    })) as usize
}

unsafe fn en(p: usize) -> &'static EnumMoniker {
    &*(p as *const EnumMoniker)
}

unsafe extern "win64" fn en_qi(p: usize, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    match read_guid(iid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IENUM_MONIKER => {
            en_add_ref(p);
            *out = p;
            S_OK
        }
        Some(_) => E_NOINTERFACE,
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn en_add_ref(p: usize) -> u32 {
    en(p).refs.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "win64" fn en_release(p: usize) -> u32 {
    let left = en(p).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if left == 0 {
        let b = Box::from_raw(p as *mut EnumMoniker);
        let (items, _) = b.state.into_inner().unwrap_or_else(|e| e.into_inner());
        for m in items {
            release(m);
        }
    }
    left
}

unsafe extern "win64" fn en_next(p: usize, count: u32, out: *mut usize, fetched: *mut u32) -> i32 {
    if out.is_null() || (count > 1 && fetched.is_null()) {
        return E_INVALIDARG;
    }
    let mut st = lock(&en(p).state);
    let mut n = 0u32;
    while n < count && st.1 < st.0.len() {
        let m = st.0[st.1];
        add_ref(m);
        *out.add(n as usize) = m;
        st.1 += 1;
        n += 1;
    }
    if !fetched.is_null() {
        *fetched = n;
    }
    if n == count {
        S_OK
    } else {
        S_FALSE
    }
}

unsafe extern "win64" fn en_skip(p: usize, count: u32) -> i32 {
    let mut st = lock(&en(p).state);
    let len = st.0.len();
    let target = st.1 + count as usize;
    st.1 = target.min(len);
    if target <= len {
        S_OK
    } else {
        S_FALSE
    }
}

unsafe extern "win64" fn en_reset(p: usize) -> i32 {
    lock(&en(p).state).1 = 0;
    S_OK
}

unsafe extern "win64" fn en_clone(p: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let (items, pos) = lock(&en(p).state).clone();
    for &m in &items {
        add_ref(m);
    }
    *out = new_enum(items, pos);
    S_OK
}

// ---------- IBindCtx ----------

/// BIND_OPTS3 complet : cbStruct, grfFlags, grfMode, dwTickCountDeadline,
/// dwTrackFlags, dwClassContext, locale, pServerInfo, hwnd (x64).
const BIND_OPTS_MAX: usize = 48;

#[repr(C)]
struct BindCtx {
    vtbl: *const (),
    refs: AtomicU32,
    state: Mutex<BindState>,
}

struct BindState {
    bound: Vec<usize>,
    params: Vec<(String, usize)>,
    opts: [u8; BIND_OPTS_MAX],
}

static BIND_VT: VTable<13> = VTable([
    bc_qi as *const (),
    bc_add_ref as *const (),
    bc_release as *const (),
    bc_register_object_bound as *const (),
    bc_revoke_object_bound as *const (),
    bc_release_bound_objects as *const (),
    bc_set_bind_options as *const (),
    bc_get_bind_options as *const (),
    bc_get_running_object_table as *const (),
    bc_register_object_param as *const (),
    bc_get_object_param as *const (),
    bc_enum_object_param as *const (),
    bc_revoke_object_param as *const (),
]);

unsafe fn bc(p: usize) -> &'static BindCtx {
    &*(p as *const BindCtx)
}

pub unsafe extern "win64" fn create_bind_ctx(reserved: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if reserved != 0 {
        return E_INVALIDARG;
    }
    let mut opts = [0u8; BIND_OPTS_MAX];
    opts[0..4].copy_from_slice(&(BIND_OPTS_MAX as u32).to_le_bytes());
    // grfMode = STGM_READWRITE, dwClassContext = CLSCTX_SERVER.
    opts[8..12].copy_from_slice(&2u32.to_le_bytes());
    opts[20..24].copy_from_slice(&0x15u32.to_le_bytes());
    *out = Box::into_raw(Box::new(BindCtx {
        vtbl: &BIND_VT as *const _ as *const (),
        refs: AtomicU32::new(1),
        state: Mutex::new(BindState {
            bound: Vec::new(),
            params: Vec::new(),
            opts,
        }),
    })) as usize;
    S_OK
}

unsafe extern "win64" fn bc_qi(p: usize, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    match read_guid(iid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IBIND_CTX => {
            bc_add_ref(p);
            *out = p;
            S_OK
        }
        Some(_) => E_NOINTERFACE,
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn bc_add_ref(p: usize) -> u32 {
    bc(p).refs.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "win64" fn bc_release(p: usize) -> u32 {
    let left = bc(p).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if left == 0 {
        let b = Box::from_raw(p as *mut BindCtx);
        let st = b.state.into_inner().unwrap_or_else(|e| e.into_inner());
        for o in st.bound {
            release(o);
        }
        for (_, o) in st.params {
            release(o);
        }
    }
    left
}

unsafe extern "win64" fn bc_register_object_bound(p: usize, punk: usize) -> i32 {
    if punk == 0 {
        return E_INVALIDARG;
    }
    add_ref(punk);
    lock(&bc(p).state).bound.push(punk);
    S_OK
}

unsafe extern "win64" fn bc_revoke_object_bound(p: usize, punk: usize) -> i32 {
    let found = {
        let mut st = lock(&bc(p).state);
        st.bound
            .iter()
            .position(|&o| o == punk)
            .map(|i| st.bound.remove(i))
    };
    match found {
        Some(o) => {
            release(o);
            S_OK
        }
        None => MK_E_NOTBOUND,
    }
}

unsafe extern "win64" fn bc_release_bound_objects(p: usize) -> i32 {
    let bound = std::mem::take(&mut lock(&bc(p).state).bound);
    for o in bound {
        release(o);
    }
    S_OK
}

unsafe extern "win64" fn bc_set_bind_options(p: usize, opts: *const u8) -> i32 {
    if opts.is_null() {
        return E_INVALIDARG;
    }
    let size = std::ptr::read_unaligned(opts as *const u32) as usize;
    if size < 16 {
        return E_INVALIDARG;
    }
    let mut st = lock(&bc(p).state);
    let n = size.min(BIND_OPTS_MAX);
    std::ptr::copy_nonoverlapping(opts.add(4), st.opts.as_mut_ptr().add(4), n - 4);
    S_OK
}

unsafe extern "win64" fn bc_get_bind_options(p: usize, opts: *mut u8) -> i32 {
    if opts.is_null() {
        return E_INVALIDARG;
    }
    let size = std::ptr::read_unaligned(opts as *const u32) as usize;
    if size < 16 {
        return E_INVALIDARG;
    }
    let st = lock(&bc(p).state);
    let n = size.min(BIND_OPTS_MAX);
    std::ptr::copy_nonoverlapping(st.opts.as_ptr().add(4), opts.add(4), n - 4);
    S_OK
}

unsafe extern "win64" fn bc_get_running_object_table(_p: usize, out: *mut usize) -> i32 {
    get_running_object_table(0, out)
}

unsafe extern "win64" fn bc_register_object_param(p: usize, key: *const u16, punk: usize) -> i32 {
    if key.is_null() || punk == 0 {
        return E_INVALIDARG;
    }
    let key = wide(key);
    add_ref(punk);
    let old = {
        let mut st = lock(&bc(p).state);
        let old = st
            .params
            .iter()
            .position(|(k, _)| *k == key)
            .map(|i| st.params.remove(i).1);
        st.params.push((key, punk));
        old
    };
    if let Some(o) = old {
        release(o);
    }
    S_OK
}

unsafe extern "win64" fn bc_get_object_param(p: usize, key: *const u16, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if key.is_null() {
        return E_INVALIDARG;
    }
    let key = wide(key);
    let st = lock(&bc(p).state);
    match st.params.iter().find(|(k, _)| *k == key) {
        Some(&(_, o)) => {
            add_ref(o);
            *out = o;
            S_OK
        }
        None => E_FAIL,
    }
}

unsafe extern "win64" fn bc_enum_object_param(_p: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    E_NOTIMPL
}

unsafe extern "win64" fn bc_revoke_object_param(p: usize, key: *const u16) -> i32 {
    if key.is_null() {
        return E_INVALIDARG;
    }
    let key = wide(key);
    let found = {
        let mut st = lock(&bc(p).state);
        st.params
            .iter()
            .position(|(k, _)| *k == key)
            .map(|i| st.params.remove(i).1)
    };
    match found {
        Some(o) => {
            release(o);
            S_OK
        }
        None => E_FAIL,
    }
}

// ---------- MkParseDisplayName / CoGetObject ----------

/// Nom d'affichage → moniker : un nom inscrit dans la table des objets
/// actifs, ou un chemin de fichier existant. Rien d'autre (monikers de
/// classe, « winmgmts: », URL) : MK_E_SYNTAX.
unsafe fn parse_name(name: &str) -> Result<usize, i32> {
    if name.is_empty() {
        return Err(MK_E_SYNTAX);
    }
    let key = name.to_lowercase();
    let running = lock(&ROT).iter().any(|e| e.key == key);
    let is_path = !running
        && (name.contains('\\') || name.contains('/'))
        && std::fs::metadata(crate::paths::to_linux(name)).is_ok();
    if running || is_path {
        Ok(new_moniker(Kind::File(name.to_string())))
    } else {
        Err(MK_E_SYNTAX)
    }
}

pub unsafe extern "win64" fn mk_parse_display_name_api(
    bind: usize,
    name: *const u16,
    eaten: *mut u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() || eaten.is_null() {
        return E_POINTER;
    }
    *out = 0;
    *eaten = 0;
    if bind == 0 || name.is_null() {
        return E_INVALIDARG;
    }
    let text = wide(name);
    match parse_name(&text) {
        Ok(mk) => {
            *eaten = text.encode_utf16().count() as u32;
            *out = mk;
            S_OK
        }
        Err(hr) => hr,
    }
}

pub unsafe extern "win64" fn co_get_object(
    name: *const u16,
    opts: *const u8,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if name.is_null() || iid.is_null() {
        return E_INVALIDARG;
    }
    let mut bind = 0usize;
    let hr = create_bind_ctx(0, &mut bind);
    if hr < 0 {
        return hr;
    }
    if !opts.is_null() {
        let hr = bc_set_bind_options(bind, opts);
        if hr < 0 {
            bc_release(bind);
            return hr;
        }
    }
    let mut eaten = 0u32;
    let mut mk = 0usize;
    let mut hr = mk_parse_display_name_api(bind, name, &mut eaten, &mut mk);
    if hr >= 0 {
        hr = mk_bind_to_object(mk, bind, 0, iid, out);
        mk_release(mk);
    }
    bc_release(bind);
    hr
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CreateFileMoniker" => fn create_file_moniker,
        "CreateItemMoniker" => fn create_item_moniker,
        "CreatePointerMoniker" => fn create_pointer_moniker,
        "CreateBindCtx" => fn create_bind_ctx,
        "GetRunningObjectTable" => fn get_running_object_table,
        "MkParseDisplayName" => fn mk_parse_display_name_api,
        "CoGetObject" => fn co_get_object,
    }
}

#[cfg(test)]
#[path = "ole32_moniker/tests.rs"]
mod tests;
