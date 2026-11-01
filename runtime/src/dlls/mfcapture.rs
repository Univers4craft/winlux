//! Media Foundation minimal pour la capture webcam : attributs, types de
//! média, tampons, échantillons, activation des caméras (V4L2) et lecteur
//! de source (`IMFSourceReader`) synchrone ou asynchrone.

use super::comobj::{
    self, guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, E_POINTER, IID_IUNKNOWN,
    S_FALSE, S_OK,
};
use super::lock;
use super::v4l2::{self, Camera, Mode};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

// ---------- HRESULT ----------
pub const MF_E_INVALIDSTREAMNUMBER: i32 = 0xC00D_36B3u32 as i32;
pub const MF_E_INVALIDMEDIATYPE: i32 = 0xC00D_36B4u32 as i32;
pub const MF_E_NO_MORE_TYPES: i32 = 0xC00D_36B9u32 as i32;
pub const MF_E_UNSUPPORTED_SERVICE: i32 = 0xC00D_36BAu32 as i32;
pub const MF_E_INVALIDTYPE: i32 = 0xC00D_36BDu32 as i32;
pub const MF_E_INVALIDREQUEST: i32 = 0xC00D_36B2u32 as i32;
pub const MF_E_ATTRIBUTENOTFOUND: i32 = 0xC00D_36E6u32 as i32;
pub const MF_E_NOT_FOUND: i32 = 0xC00D_36D5u32 as i32;
const MF_E_BUFFERTOOSMALL: i32 = 0xC00D_36B1u32 as i32;
const MF_E_VIDEO_DEVICE_INVALIDATED: i32 = 0xC00D_3EA2u32 as i32;

// ---------- GUID ----------
const MF_D4: [u8; 8] = [0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71];
pub const fn media_subtype(code: u32) -> Guid {
    guid(code, 0x0000, 0x0010, MF_D4)
}
pub const MFMEDIATYPE_VIDEO: Guid = media_subtype(0x7364_6976);
pub const MFVIDEOFORMAT_YUY2: Guid = media_subtype(v4l2::fourcc(b"YUY2"));
pub const MFVIDEOFORMAT_NV12: Guid = media_subtype(v4l2::NV12);
pub const MFVIDEOFORMAT_MJPG: Guid = media_subtype(v4l2::MJPG);
pub const MFVIDEOFORMAT_RGB32: Guid = media_subtype(22);
pub const MFVIDEOFORMAT_ARGB32: Guid = media_subtype(21);

pub const MF_MT_MAJOR_TYPE: Guid = guid(
    0x48eba18e,
    0xf8c9,
    0x4687,
    [0xbf, 0x11, 0x0a, 0x74, 0xc9, 0xf9, 0x6a, 0x8f],
);
pub const MF_MT_SUBTYPE: Guid = guid(
    0xf7e34c9a,
    0x42e8,
    0x4714,
    [0xb7, 0x4b, 0xcb, 0x29, 0xd7, 0x2c, 0x35, 0xe5],
);
pub const MF_MT_FRAME_SIZE: Guid = guid(
    0x1652c33d,
    0xd6b2,
    0x4012,
    [0xb8, 0x34, 0x72, 0x03, 0x08, 0x49, 0xa3, 0x7d],
);
pub const MF_MT_FRAME_RATE: Guid = guid(
    0xc459a2e8,
    0x3d2c,
    0x4e44,
    [0xb1, 0x32, 0xfe, 0xe5, 0x15, 0x6c, 0x7b, 0xb0],
);
const MF_MT_PIXEL_ASPECT_RATIO: Guid = guid(
    0xc6376a1e,
    0x8d0a,
    0x4027,
    [0xbe, 0x45, 0x6d, 0x9a, 0x0a, 0xd3, 0x9b, 0xb6],
);
const MF_MT_INTERLACE_MODE: Guid = guid(
    0xe2724bb8,
    0xe676,
    0x4806,
    [0xb4, 0xb2, 0xa8, 0xd6, 0xef, 0xb4, 0x4c, 0xcd],
);
const MF_MT_ALL_SAMPLES_INDEPENDENT: Guid = guid(
    0xc9173739,
    0x5e56,
    0x461c,
    [0xb7, 0x13, 0x46, 0xfb, 0x99, 0x5c, 0xb9, 0x5f],
);
const MF_MT_FIXED_SIZE_SAMPLES: Guid = guid(
    0xb8ebefaf,
    0xb718,
    0x4e04,
    [0xb0, 0xa9, 0x11, 0x67, 0x75, 0xe3, 0x32, 0x1b],
);
const MF_MT_SAMPLE_SIZE: Guid = guid(
    0xdad3ab78,
    0x1990,
    0x408b,
    [0xbc, 0xe2, 0xeb, 0xa6, 0x73, 0xda, 0xcc, 0x10],
);
const MF_MT_DEFAULT_STRIDE: Guid = guid(
    0x644b4e48,
    0x1e02,
    0x4516,
    [0xb0, 0xeb, 0xc0, 0x1c, 0xa9, 0xd4, 0x9a, 0xc6],
);
pub const MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE: Guid = guid(
    0xc60ac5fe,
    0x252a,
    0x478f,
    [0xa0, 0xef, 0xbc, 0x8f, 0xa5, 0xf7, 0xca, 0xd3],
);
pub const MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID: Guid = guid(
    0x8ac3587a,
    0x4ae7,
    0x42d8,
    [0x99, 0xe0, 0x0a, 0x60, 0x13, 0xee, 0xf9, 0x0f],
);
pub const MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME: Guid = guid(
    0x60d0e559,
    0x52f8,
    0x4fa2,
    [0xbb, 0xce, 0xac, 0xdb, 0x34, 0xa8, 0xec, 0x01],
);
pub const MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK: Guid = guid(
    0x58f0aad8,
    0x22bf,
    0x4f8a,
    [0xbb, 0x3d, 0xd2, 0xc4, 0x97, 0x8c, 0x6e, 0x2f],
);
pub const MF_SOURCE_READER_ASYNC_CALLBACK: Guid = guid(
    0x1e3dbeac,
    0xbb43,
    0x4c35,
    [0xb5, 0x07, 0xcd, 0x64, 0x44, 0x64, 0xc9, 0x65],
);

const IID_IMFATTRIBUTES: Guid = guid(
    0x2cd2d921,
    0xc447,
    0x44a7,
    [0xa1, 0x3c, 0x4a, 0xda, 0xbf, 0xc2, 0x47, 0xe3],
);
const IID_IMFMEDIATYPE: Guid = guid(
    0x44ae0fa8,
    0xea31,
    0x4109,
    [0x8d, 0x2e, 0x4c, 0xae, 0x49, 0x97, 0xc5, 0x55],
);
const IID_IMFACTIVATE: Guid = guid(
    0x7fee9e9a,
    0x4a89,
    0x47a6,
    [0x89, 0x9c, 0xb6, 0xa5, 0x3a, 0x70, 0xfb, 0x67],
);
const IID_IMFSAMPLE: Guid = guid(
    0xc40a00f2,
    0xb93a,
    0x4d80,
    [0xae, 0x8c, 0x5a, 0x1c, 0x63, 0x4f, 0x58, 0xe4],
);
const IID_IMFMEDIABUFFER: Guid = guid(
    0x045fa593,
    0x8799,
    0x42b8,
    [0xbc, 0x8d, 0x89, 0x68, 0xc6, 0x45, 0x35, 0x07],
);
const IID_IMFMEDIASOURCE: Guid = guid(
    0x279a808d,
    0xaec7,
    0x40c8,
    [0x9c, 0x6b, 0xa6, 0xb4, 0x92, 0xc7, 0x8a, 0x66],
);
const IID_IMFMEDIAEVENTGENERATOR: Guid = guid(
    0x2cd0bd52,
    0xbcd5,
    0x4b89,
    [0xb6, 0x2c, 0xea, 0xdc, 0x0c, 0x03, 0x1e, 0x7d],
);
const IID_IMFSOURCEREADER: Guid = guid(
    0x70ae66f2,
    0xc809,
    0x4e4f,
    [0x89, 0x15, 0xbd, 0xcb, 0x40, 0x6b, 0x79, 0x93],
);
const IID_IMFSOURCEREADERCALLBACK: Guid = guid(
    0xdeec8d99,
    0xfa1d,
    0x4d82,
    [0x84, 0xc2, 0x2c, 0x89, 0x69, 0x94, 0x48, 0x67],
);

// ---------- Appels sur des objets COM quelconques ----------

/// Entrée `idx` de la table virtuelle de l'objet `obj`.
unsafe fn slot(obj: *mut c_void, idx: usize) -> *const () {
    *(*(obj as *const *const *const ())).add(idx)
}
unsafe fn com_addref(obj: *mut c_void) {
    if !obj.is_null() {
        let f: unsafe extern "win64" fn(*mut c_void) -> u32 = std::mem::transmute(slot(obj, 1));
        f(obj);
    }
}
unsafe fn com_release(obj: *mut c_void) {
    if !obj.is_null() {
        let f: unsafe extern "win64" fn(*mut c_void) -> u32 = std::mem::transmute(slot(obj, 2));
        f(obj);
    }
}
unsafe fn com_qi(obj: *mut c_void, iid: &Guid) -> Option<*mut c_void> {
    let f: unsafe extern "win64" fn(*mut c_void, *const u8, *mut *mut c_void) -> i32 =
        std::mem::transmute(slot(obj, 0));
    let mut out = std::ptr::null_mut();
    (f(obj, iid.as_ptr(), &mut out) >= 0 && !out.is_null()).then_some(out)
}
/// IMFAttributes::GetGUID (index 10) sur un objet étranger.
unsafe fn attr_get_guid(obj: *mut c_void, key: &Guid) -> Option<Guid> {
    if obj.is_null() {
        return None;
    }
    let f: unsafe extern "win64" fn(*mut c_void, *const u8, *mut Guid) -> i32 =
        std::mem::transmute(slot(obj, 10));
    let mut g = [0u8; 16];
    (f(obj, key.as_ptr(), &mut g) >= 0).then_some(g)
}
/// IMFAttributes::GetUINT64 (index 8).
unsafe fn attr_get_u64(obj: *mut c_void, key: &Guid) -> Option<u64> {
    if obj.is_null() {
        return None;
    }
    let f: unsafe extern "win64" fn(*mut c_void, *const u8, *mut u64) -> i32 =
        std::mem::transmute(slot(obj, 8));
    let mut v = 0u64;
    (f(obj, key.as_ptr(), &mut v) >= 0).then_some(v)
}
/// IMFAttributes::GetAllocatedString (index 13).
unsafe fn attr_get_string(obj: *mut c_void, key: &Guid) -> Option<String> {
    if obj.is_null() {
        return None;
    }
    let f: unsafe extern "win64" fn(*mut c_void, *const u8, *mut *mut u16, *mut u32) -> i32 =
        std::mem::transmute(slot(obj, 13));
    let mut p = std::ptr::null_mut();
    let mut n = 0u32;
    if f(obj, key.as_ptr(), &mut p, &mut n) < 0 || p.is_null() {
        return None;
    }
    let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n as usize));
    super::ole32::co_task_mem_free(p as *mut c_void);
    Some(s)
}
/// IMFAttributes::GetUnknown (index 17).
unsafe fn attr_get_unknown(obj: *mut c_void, key: &Guid, iid: &Guid) -> Option<*mut c_void> {
    if obj.is_null() {
        return None;
    }
    let f: unsafe extern "win64" fn(*mut c_void, *const u8, *const u8, *mut *mut c_void) -> i32 =
        std::mem::transmute(slot(obj, 17));
    let mut p = std::ptr::null_mut();
    (f(obj, key.as_ptr(), iid.as_ptr(), &mut p) >= 0 && !p.is_null()).then_some(p)
}

// ---------- Attributs ----------

/// Valeur d'attribut (MF_ATTRIBUTE_TYPE).
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    U32(u32),
    U64(u64),
    F64(f64),
    Guid(Guid),
    Str(Vec<u16>),
    Blob(Vec<u8>),
    /// Pointeur IUnknown avec une référence détenue.
    Unk(usize),
}

impl Val {
    fn vt(&self) -> u16 {
        match self {
            Val::U32(_) => 19,
            Val::U64(_) => 21,
            Val::F64(_) => 5,
            Val::Guid(_) => 72,
            Val::Str(_) => 31,
            Val::Blob(_) => 0x1011,
            Val::Unk(_) => 13,
        }
    }
}

#[derive(Default)]
pub struct Attrs(Vec<(Guid, Val)>);

impl Attrs {
    pub fn get(&self, k: &Guid) -> Option<&Val> {
        self.0.iter().find(|(g, _)| g == k).map(|(_, v)| v)
    }
    pub fn set(&mut self, k: Guid, v: Val) {
        if let Val::Unk(p) = v {
            unsafe { com_addref(p as *mut c_void) };
        }
        self.delete(&k);
        self.0.push((k, v));
    }
    fn delete(&mut self, k: &Guid) {
        if let Some(i) = self.0.iter().position(|(g, _)| g == k) {
            let (_, old) = self.0.remove(i);
            if let Val::Unk(p) = old {
                unsafe { com_release(p as *mut c_void) };
            }
        }
    }
    fn clear(&mut self) {
        let keys: Vec<Guid> = self.0.iter().map(|(g, _)| *g).collect();
        for k in keys {
            self.delete(&k);
        }
    }
    fn set_str(&mut self, k: Guid, s: &str) {
        self.set(k, Val::Str(s.encode_utf16().collect()));
    }
}

impl Drop for Attrs {
    fn drop(&mut self) {
        self.clear();
    }
}

/// Extension propre au type d'objet.
enum Ext {
    Plain,
    MediaType,
    Activate(Camera),
    Sample(Mutex<SampleData>),
}

#[derive(Default)]
struct SampleData {
    time: i64,
    duration: i64,
    flags: u32,
    buffers: Vec<usize>,
}

impl Drop for SampleData {
    fn drop(&mut self) {
        for b in self.buffers.drain(..) {
            unsafe { com_release(b as *mut c_void) };
        }
    }
}

/// Objet COM à attributs : IMFAttributes, IMFMediaType, IMFActivate, IMFSample.
#[repr(C)]
pub struct MfObj {
    vtbl: *const (),
    refs: AtomicU32,
    attrs: Mutex<Attrs>,
    ext: Ext,
}

fn new_obj(ext: Ext) -> *mut MfObj {
    let vtbl = match ext {
        Ext::Plain => ATTR_VTBL.0.as_ptr() as *const (),
        Ext::MediaType => MEDIATYPE_VTBL.0.as_ptr() as *const (),
        Ext::Activate(_) => ACTIVATE_VTBL.0.as_ptr() as *const (),
        Ext::Sample(_) => SAMPLE_VTBL.0.as_ptr() as *const (),
    };
    Box::into_raw(Box::new(MfObj {
        vtbl,
        refs: AtomicU32::new(1),
        attrs: Mutex::new(Attrs::default()),
        ext,
    }))
}

/// Vrai si `p` est un objet à attributs créé par Winlux.
fn is_ours(p: *mut c_void) -> bool {
    if p.is_null() {
        return false;
    }
    let v = unsafe { *(p as *const *const ()) };
    [
        ATTR_VTBL.0.as_ptr() as *const (),
        MEDIATYPE_VTBL.0.as_ptr() as *const (),
        ACTIVATE_VTBL.0.as_ptr() as *const (),
        SAMPLE_VTBL.0.as_ptr() as *const (),
    ]
    .contains(&v)
}

unsafe fn obj<'a>(this: *mut MfObj) -> &'a MfObj {
    &*this
}

unsafe extern "win64" fn a_qi(this: *mut MfObj, riid: *const u8, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = comobj::read_guid(riid) else {
        return E_POINTER;
    };
    let o = obj(this);
    let ok = iid == IID_IUNKNOWN
        || iid == IID_IMFATTRIBUTES
        || (iid == IID_IMFMEDIATYPE && matches!(o.ext, Ext::MediaType))
        || (iid == IID_IMFACTIVATE && matches!(o.ext, Ext::Activate(_)))
        || (iid == IID_IMFSAMPLE && matches!(o.ext, Ext::Sample(_)));
    if ok {
        o.refs.fetch_add(1, Ordering::Relaxed);
        *out = this as *mut c_void;
        S_OK
    } else {
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
}
unsafe extern "win64" fn a_addref(this: *mut MfObj) -> u32 {
    obj(this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn a_release(this: *mut MfObj) -> u32 {
    let n = obj(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this));
    }
    n
}

/// Écrit une valeur dans un PROPVARIANT (24 octets).
unsafe fn write_propvariant(pv: *mut u8, v: &Val) {
    std::ptr::write_bytes(pv, 0, 24);
    (pv as *mut u16).write_unaligned(v.vt());
    let data = pv.add(8);
    match v {
        Val::U32(x) => (data as *mut u32).write_unaligned(*x),
        Val::U64(x) => (data as *mut u64).write_unaligned(*x),
        Val::F64(x) => (data as *mut f64).write_unaligned(*x),
        Val::Guid(g) => {
            let p = super::ole32::co_task_mem_alloc(16) as *mut u8;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(g.as_ptr(), p, 16);
            }
            (data as *mut *mut u8).write_unaligned(p);
        }
        Val::Str(s) => (data as *mut *mut u16).write_unaligned(alloc_wide(s)),
        Val::Blob(b) => {
            let p = super::ole32::co_task_mem_alloc(b.len().max(1)) as *mut u8;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(b.as_ptr(), p, b.len());
            }
            (data as *mut u32).write_unaligned(b.len() as u32);
            (pv.add(16) as *mut *mut u8).write_unaligned(p);
        }
        Val::Unk(p) => {
            com_addref(*p as *mut c_void);
            (data as *mut usize).write_unaligned(*p);
        }
    }
}

/// Lit un PROPVARIANT ; `None` pour un type non pris en charge.
unsafe fn read_propvariant(pv: *const u8) -> Option<Val> {
    let vt = (pv as *const u16).read_unaligned();
    let data = pv.add(8);
    Some(match vt {
        19 => Val::U32((data as *const u32).read_unaligned()),
        21 => Val::U64((data as *const u64).read_unaligned()),
        5 => Val::F64((data as *const f64).read_unaligned()),
        72 => Val::Guid(comobj::read_guid(
            (data as *const *const u8).read_unaligned(),
        )?),
        31 => {
            let p = (data as *const *const u16).read_unaligned();
            let n = crate::printf::wide_len(p);
            Val::Str(std::slice::from_raw_parts(p, n).to_vec())
        }
        0x1011 => {
            let n = (data as *const u32).read_unaligned() as usize;
            let p = (pv.add(16) as *const *const u8).read_unaligned();
            Val::Blob(if p.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(p, n).to_vec()
            })
        }
        13 => Val::Unk((data as *const usize).read_unaligned()),
        _ => return None,
    })
}

unsafe fn alloc_wide(s: &[u16]) -> *mut u16 {
    let p = super::ole32::co_task_mem_alloc((s.len() + 1) * 2) as *mut u16;
    if !p.is_null() {
        std::ptr::copy_nonoverlapping(s.as_ptr(), p, s.len());
        *p.add(s.len()) = 0;
    }
    p
}

unsafe fn with_key<R>(
    this: *mut MfObj,
    key: *const u8,
    f: impl FnOnce(&Val) -> R,
) -> Result<R, i32> {
    let k = comobj::read_guid(key).ok_or(E_POINTER)?;
    let a = lock(&obj(this).attrs);
    a.get(&k).map(f).ok_or(MF_E_ATTRIBUTENOTFOUND)
}

unsafe extern "win64" fn a_get_item(this: *mut MfObj, key: *const u8, pv: *mut u8) -> i32 {
    match with_key(this, key, Clone::clone) {
        Ok(v) => {
            if !pv.is_null() {
                write_propvariant(pv, &v);
            }
            S_OK
        }
        Err(e) => e,
    }
}
unsafe extern "win64" fn a_get_item_type(this: *mut MfObj, key: *const u8, ty: *mut u32) -> i32 {
    match with_key(this, key, |v| v.vt() as u32) {
        Ok(t) => {
            if !ty.is_null() {
                *ty = t;
            }
            S_OK
        }
        Err(e) => e,
    }
}
unsafe extern "win64" fn a_compare_item(
    this: *mut MfObj,
    key: *const u8,
    pv: *const u8,
    result: *mut i32,
) -> i32 {
    if result.is_null() || pv.is_null() {
        return E_POINTER;
    }
    let other = read_propvariant(pv);
    *result = i32::from(with_key(this, key, |v| Some(v) == other.as_ref()).unwrap_or(false));
    S_OK
}
unsafe extern "win64" fn a_compare(
    this: *mut MfObj,
    other: *mut c_void,
    _match_type: u32,
    result: *mut i32,
) -> i32 {
    if result.is_null() || other.is_null() {
        return E_POINTER;
    }
    *result = if is_ours(other) {
        let a = lock(&obj(this).attrs);
        let b = lock(&(*(other as *mut MfObj)).attrs);
        i32::from(a.0.len() == b.0.len() && a.0.iter().all(|(k, v)| b.get(k) == Some(v)))
    } else {
        0
    };
    S_OK
}
macro_rules! getter {
    ($name:ident, $t:ty, $variant:ident) => {
        unsafe extern "win64" fn $name(this: *mut MfObj, key: *const u8, out: *mut $t) -> i32 {
            if out.is_null() {
                return E_POINTER;
            }
            match with_key(this, key, |v| match v {
                Val::$variant(x) => Some(x.clone()),
                _ => None,
            }) {
                Ok(Some(x)) => {
                    out.write_unaligned(x);
                    S_OK
                }
                Ok(None) => MF_E_INVALIDTYPE,
                Err(e) => e,
            }
        }
    };
}
getter!(a_get_u32, u32, U32);
getter!(a_get_u64, u64, U64);
getter!(a_get_double, f64, F64);
getter!(a_get_guid, Guid, Guid);

fn string_of(v: &Val) -> Option<Vec<u16>> {
    match v {
        Val::Str(s) => Some(s.clone()),
        _ => None,
    }
}
fn blob_of(v: &Val) -> Option<Vec<u8>> {
    match v {
        Val::Blob(b) => Some(b.clone()),
        _ => None,
    }
}
unsafe extern "win64" fn a_get_string_length(this: *mut MfObj, key: *const u8, n: *mut u32) -> i32 {
    match with_key(this, key, string_of) {
        Ok(Some(s)) => {
            if !n.is_null() {
                *n = s.len() as u32;
            }
            S_OK
        }
        Ok(None) => MF_E_INVALIDTYPE,
        Err(e) => e,
    }
}
unsafe extern "win64" fn a_get_string(
    this: *mut MfObj,
    key: *const u8,
    buf: *mut u16,
    cap: u32,
    len: *mut u32,
) -> i32 {
    match with_key(this, key, string_of) {
        Ok(Some(s)) => {
            if !len.is_null() {
                *len = s.len() as u32;
            }
            if buf.is_null() || (cap as usize) <= s.len() {
                return 0x8007_007Au32 as i32; // ERROR_INSUFFICIENT_BUFFER
            }
            std::ptr::copy_nonoverlapping(s.as_ptr(), buf, s.len());
            *buf.add(s.len()) = 0;
            S_OK
        }
        Ok(None) => MF_E_INVALIDTYPE,
        Err(e) => e,
    }
}
unsafe extern "win64" fn a_get_allocated_string(
    this: *mut MfObj,
    key: *const u8,
    out: *mut *mut u16,
    len: *mut u32,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match with_key(this, key, string_of) {
        Ok(Some(s)) => {
            *out = alloc_wide(&s);
            if !len.is_null() {
                *len = s.len() as u32;
            }
            S_OK
        }
        Ok(None) => MF_E_INVALIDTYPE,
        Err(e) => e,
    }
}
unsafe extern "win64" fn a_get_blob_size(this: *mut MfObj, key: *const u8, n: *mut u32) -> i32 {
    match with_key(this, key, blob_of) {
        Ok(Some(b)) => {
            if !n.is_null() {
                *n = b.len() as u32;
            }
            S_OK
        }
        Ok(None) => MF_E_INVALIDTYPE,
        Err(e) => e,
    }
}
unsafe extern "win64" fn a_get_blob(
    this: *mut MfObj,
    key: *const u8,
    buf: *mut u8,
    cap: u32,
    size: *mut u32,
) -> i32 {
    match with_key(this, key, blob_of) {
        Ok(Some(b)) => {
            if !size.is_null() {
                *size = b.len() as u32;
            }
            if buf.is_null() || (cap as usize) < b.len() {
                return 0x8007_007Au32 as i32;
            }
            std::ptr::copy_nonoverlapping(b.as_ptr(), buf, b.len());
            S_OK
        }
        Ok(None) => MF_E_INVALIDTYPE,
        Err(e) => e,
    }
}
unsafe extern "win64" fn a_get_allocated_blob(
    this: *mut MfObj,
    key: *const u8,
    out: *mut *mut u8,
    size: *mut u32,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match with_key(this, key, blob_of) {
        Ok(Some(b)) => {
            let p = super::ole32::co_task_mem_alloc(b.len().max(1)) as *mut u8;
            std::ptr::copy_nonoverlapping(b.as_ptr(), p, b.len());
            *out = p;
            if !size.is_null() {
                *size = b.len() as u32;
            }
            S_OK
        }
        Ok(None) => MF_E_INVALIDTYPE,
        Err(e) => e,
    }
}
unsafe extern "win64" fn a_get_unknown(
    this: *mut MfObj,
    key: *const u8,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    match with_key(this, key, |v| match v {
        Val::Unk(p) => Some(*p),
        _ => None,
    }) {
        Ok(Some(p)) => {
            let f: unsafe extern "win64" fn(*mut c_void, *const u8, *mut *mut c_void) -> i32 =
                std::mem::transmute(slot(p as *mut c_void, 0));
            f(p as *mut c_void, riid, out)
        }
        Ok(None) => MF_E_INVALIDTYPE,
        Err(e) => e,
    }
}
unsafe fn set_val(this: *mut MfObj, key: *const u8, v: Val) -> i32 {
    let Some(k) = comobj::read_guid(key) else {
        return E_POINTER;
    };
    lock(&obj(this).attrs).set(k, v);
    S_OK
}
unsafe extern "win64" fn a_set_item(this: *mut MfObj, key: *const u8, pv: *const u8) -> i32 {
    if pv.is_null() {
        return E_POINTER;
    }
    match read_propvariant(pv) {
        Some(v) => set_val(this, key, v),
        None => MF_E_INVALIDTYPE,
    }
}
unsafe extern "win64" fn a_delete_item(this: *mut MfObj, key: *const u8) -> i32 {
    if let Some(k) = comobj::read_guid(key) {
        lock(&obj(this).attrs).delete(&k);
    }
    S_OK
}
unsafe extern "win64" fn a_delete_all(this: *mut MfObj) -> i32 {
    lock(&obj(this).attrs).clear();
    S_OK
}
unsafe extern "win64" fn a_set_u32(this: *mut MfObj, key: *const u8, v: u32) -> i32 {
    set_val(this, key, Val::U32(v))
}
unsafe extern "win64" fn a_set_u64(this: *mut MfObj, key: *const u8, v: u64) -> i32 {
    set_val(this, key, Val::U64(v))
}
unsafe extern "win64" fn a_set_double(this: *mut MfObj, key: *const u8, v: f64) -> i32 {
    set_val(this, key, Val::F64(v))
}
unsafe extern "win64" fn a_set_guid(this: *mut MfObj, key: *const u8, v: *const u8) -> i32 {
    match comobj::read_guid(v) {
        Some(g) => set_val(this, key, Val::Guid(g)),
        None => E_POINTER,
    }
}
unsafe extern "win64" fn a_set_string(this: *mut MfObj, key: *const u8, s: *const u16) -> i32 {
    if s.is_null() {
        return E_POINTER;
    }
    let n = crate::printf::wide_len(s);
    set_val(
        this,
        key,
        Val::Str(std::slice::from_raw_parts(s, n).to_vec()),
    )
}
unsafe extern "win64" fn a_set_blob(this: *mut MfObj, key: *const u8, b: *const u8, n: u32) -> i32 {
    if b.is_null() && n > 0 {
        return E_POINTER;
    }
    let data = if n == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(b, n as usize).to_vec()
    };
    set_val(this, key, Val::Blob(data))
}
unsafe extern "win64" fn a_set_unknown(this: *mut MfObj, key: *const u8, u: *mut c_void) -> i32 {
    if u.is_null() {
        if let Some(k) = comobj::read_guid(key) {
            lock(&obj(this).attrs).delete(&k);
        }
        return S_OK;
    }
    set_val(this, key, Val::Unk(u as usize))
}
unsafe extern "win64" fn a_lock_store(_this: *mut MfObj) -> i32 {
    S_OK
}
unsafe extern "win64" fn a_get_count(this: *mut MfObj, n: *mut u32) -> i32 {
    if n.is_null() {
        return E_POINTER;
    }
    *n = lock(&obj(this).attrs).0.len() as u32;
    S_OK
}
unsafe extern "win64" fn a_get_item_by_index(
    this: *mut MfObj,
    index: u32,
    key: *mut Guid,
    pv: *mut u8,
) -> i32 {
    let a = lock(&obj(this).attrs);
    let Some((k, v)) = a.0.get(index as usize) else {
        return E_INVALIDARG;
    };
    if !key.is_null() {
        key.write_unaligned(*k);
    }
    if !pv.is_null() {
        write_propvariant(pv, v);
    }
    S_OK
}
unsafe extern "win64" fn a_copy_all_items(this: *mut MfObj, dest: *mut c_void) -> i32 {
    if dest.is_null() {
        return E_POINTER;
    }
    let items: Vec<(Guid, Val)> = lock(&obj(this).attrs).0.clone_items();
    let set: unsafe extern "win64" fn(*mut c_void, *const u8, *const u8) -> i32 =
        std::mem::transmute(slot(dest, 18));
    let mut pv = [0u8; 24];
    for (k, v) in items {
        write_propvariant(pv.as_mut_ptr(), &v);
        set(dest, k.as_ptr(), pv.as_ptr());
        clear_propvariant(pv.as_mut_ptr());
    }
    S_OK
}

/// Libère ce que `write_propvariant` a alloué.
unsafe fn clear_propvariant(pv: *mut u8) {
    let vt = (pv as *const u16).read_unaligned();
    let data = pv.add(8);
    match vt {
        72 | 31 => super::ole32::co_task_mem_free((data as *const *mut c_void).read_unaligned()),
        0x1011 => {
            super::ole32::co_task_mem_free((pv.add(16) as *const *mut c_void).read_unaligned())
        }
        13 => com_release((data as *const *mut c_void).read_unaligned()),
        _ => {}
    }
    std::ptr::write_bytes(pv, 0, 24);
}

trait CloneItems {
    fn clone_items(&self) -> Vec<(Guid, Val)>;
}
impl CloneItems for Vec<(Guid, Val)> {
    fn clone_items(&self) -> Vec<(Guid, Val)> {
        self.clone()
    }
}

macro_rules! attr_vtbl_entries {
    () => {
        [
            a_qi as *const (),
            a_addref as *const (),
            a_release as *const (),
            a_get_item as *const (),
            a_get_item_type as *const (),
            a_compare_item as *const (),
            a_compare as *const (),
            a_get_u32 as *const (),
            a_get_u64 as *const (),
            a_get_double as *const (),
            a_get_guid as *const (),
            a_get_string_length as *const (),
            a_get_string as *const (),
            a_get_allocated_string as *const (),
            a_get_blob_size as *const (),
            a_get_blob as *const (),
            a_get_allocated_blob as *const (),
            a_get_unknown as *const (),
            a_set_item as *const (),
            a_delete_item as *const (),
            a_delete_all as *const (),
            a_set_u32 as *const (),
            a_set_u64 as *const (),
            a_set_double as *const (),
            a_set_guid as *const (),
            a_set_string as *const (),
            a_set_blob as *const (),
            a_set_unknown as *const (),
            a_lock_store as *const (),
            a_lock_store as *const (),
            a_get_count as *const (),
            a_get_item_by_index as *const (),
            a_copy_all_items as *const (),
        ]
    };
}

const fn concat<const A: usize, const B: usize, const N: usize>(
    a: [*const (); A],
    b: [*const (); B],
) -> [*const (); N] {
    let mut out = [std::ptr::null::<()>(); N];
    let mut i = 0;
    while i < A {
        out[i] = a[i];
        i += 1;
    }
    while i < N {
        out[i] = b[i - A];
        i += 1;
    }
    out
}

static ATTR_VTBL: VTable<33> = VTable(attr_vtbl_entries!());

// ---------- IMFMediaType ----------

unsafe extern "win64" fn mt_get_major_type(this: *mut MfObj, out: *mut Guid) -> i32 {
    a_get_guid(this, MF_MT_MAJOR_TYPE.as_ptr(), out)
}
unsafe extern "win64" fn mt_is_compressed(this: *mut MfObj, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let sub = with_key(this, MF_MT_SUBTYPE.as_ptr(), |v| v.clone()).ok();
    *out = i32::from(sub == Some(Val::Guid(MFVIDEOFORMAT_MJPG)));
    S_OK
}
unsafe extern "win64" fn mt_is_equal(this: *mut MfObj, other: *mut c_void, flags: *mut u32) -> i32 {
    if other.is_null() || flags.is_null() {
        return E_POINTER;
    }
    let mine = |k: &Guid| lock(&obj(this).attrs).get(k).cloned();
    let theirs_guid = |k: &Guid| attr_get_guid(other, k);
    let mut f = 0;
    if mine(&MF_MT_MAJOR_TYPE) == theirs_guid(&MF_MT_MAJOR_TYPE).map(Val::Guid) {
        f |= 1;
    }
    if mine(&MF_MT_SUBTYPE) == theirs_guid(&MF_MT_SUBTYPE).map(Val::Guid) {
        f |= 2;
    }
    if mine(&MF_MT_FRAME_SIZE) == attr_get_u64(other, &MF_MT_FRAME_SIZE).map(Val::U64) {
        f |= 4;
    }
    f |= 8;
    *flags = f;
    if f == 15 {
        S_OK
    } else {
        S_FALSE
    }
}
unsafe extern "win64" fn mt_get_representation(
    _this: *mut MfObj,
    _g: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}
unsafe extern "win64" fn mt_free_representation(
    _this: *mut MfObj,
    _g: *const u8,
    _p: usize,
) -> i32 {
    S_OK
}

static MEDIATYPE_VTBL: VTable<38> = VTable(concat(
    attr_vtbl_entries!(),
    [
        mt_get_major_type as *const (),
        mt_is_compressed as *const (),
        mt_is_equal as *const (),
        mt_get_representation as *const (),
        mt_free_representation as *const (),
    ],
));

pub fn create_attributes() -> *mut c_void {
    new_obj(Ext::Plain) as *mut c_void
}
pub fn create_media_type() -> *mut c_void {
    new_obj(Ext::MediaType) as *mut c_void
}
pub fn create_sample() -> *mut c_void {
    new_obj(Ext::Sample(Mutex::new(SampleData::default()))) as *mut c_void
}

// ---------- Format de sortie ----------

/// Conversion appliquée aux images natives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Out {
    Native,
    Rgb32,
    Nv12,
}

pub fn native_subtype(fourcc: u32) -> Guid {
    match fourcc {
        v4l2::YUYV => MFVIDEOFORMAT_YUY2,
        v4l2::NV12 => MFVIDEOFORMAT_NV12,
        _ => MFVIDEOFORMAT_MJPG,
    }
}

fn out_subtype(mode: &Mode, out: Out) -> Guid {
    match out {
        Out::Native => native_subtype(mode.fourcc),
        Out::Rgb32 => MFVIDEOFORMAT_RGB32,
        Out::Nv12 => MFVIDEOFORMAT_NV12,
    }
}

fn out_size(mode: &Mode, out: Out) -> (usize, i32) {
    let (w, h) = (mode.width as usize, mode.height as usize);
    match (out, mode.fourcc) {
        (Out::Rgb32, _) => (w * h * 4, (w * 4) as i32),
        (Out::Nv12, _) | (Out::Native, v4l2::NV12) => (w * h * 3 / 2, w as i32),
        (Out::Native, v4l2::YUYV) => (w * h * 2, (w * 2) as i32),
        _ => (mode.frame_bytes(), 0),
    }
}

/// Construit un IMFMediaType décrivant `mode` converti en `out`.
fn media_type_for(mode: &Mode, out: Out) -> *mut c_void {
    let t = new_obj(Ext::MediaType);
    let (size, stride) = out_size(mode, out);
    {
        let mut a = lock(unsafe { &(*t).attrs });
        a.set(MF_MT_MAJOR_TYPE, Val::Guid(MFMEDIATYPE_VIDEO));
        a.set(MF_MT_SUBTYPE, Val::Guid(out_subtype(mode, out)));
        a.set(
            MF_MT_FRAME_SIZE,
            Val::U64(((mode.width as u64) << 32) | mode.height as u64),
        );
        a.set(
            MF_MT_FRAME_RATE,
            Val::U64(((mode.fps_num as u64) << 32) | mode.fps_den.max(1) as u64),
        );
        a.set(MF_MT_PIXEL_ASPECT_RATIO, Val::U64((1 << 32) | 1));
        a.set(MF_MT_INTERLACE_MODE, Val::U32(2)); // progressif
        a.set(MF_MT_ALL_SAMPLES_INDEPENDENT, Val::U32(1));
        let fixed = mode.fourcc != v4l2::MJPG || out != Out::Native;
        a.set(MF_MT_FIXED_SIZE_SAMPLES, Val::U32(u32::from(fixed)));
        if fixed {
            a.set(MF_MT_SAMPLE_SIZE, Val::U32(size as u32));
            a.set(MF_MT_DEFAULT_STRIDE, Val::U32(stride as u32));
        }
    }
    t as *mut c_void
}

/// Choisit le mode natif et la conversion pour un sous-type et une
/// définition demandés (`None` : garder la définition `current`).
pub fn choose_mode(
    modes: &[Mode],
    subtype: Guid,
    size: Option<(u32, u32)>,
    rate: Option<(u32, u32)>,
    current: &Mode,
) -> Option<(Mode, Out)> {
    let (w, h) = size.unwrap_or((current.width, current.height));
    let candidates: Vec<&Mode> = modes
        .iter()
        .filter(|m| m.width == w && m.height == h)
        .collect();
    let pick = |fourccs: &[u32], out: Out| -> Option<(Mode, Out)> {
        for &f in fourccs {
            let mut c: Vec<&&Mode> = candidates.iter().filter(|m| m.fourcc == f).collect();
            if let Some((n, d)) = rate {
                c.sort_by_key(|m| {
                    (m.fps_num as u64 * d as u64).abs_diff(n as u64 * m.fps_den as u64)
                });
            }
            if let Some(m) = c.first() {
                return Some((***m, out));
            }
        }
        None
    };
    if subtype == MFVIDEOFORMAT_YUY2 {
        pick(&[v4l2::YUYV], Out::Native)
    } else if subtype == MFVIDEOFORMAT_MJPG {
        pick(&[v4l2::MJPG], Out::Native)
    } else if subtype == MFVIDEOFORMAT_NV12 {
        pick(&[v4l2::NV12], Out::Native).or_else(|| pick(&[v4l2::YUYV], Out::Nv12))
    } else if subtype == MFVIDEOFORMAT_RGB32 || subtype == MFVIDEOFORMAT_ARGB32 {
        pick(&[v4l2::YUYV, v4l2::NV12], Out::Rgb32)
    } else {
        None
    }
}

/// Convertit une image native vers le format de sortie.
pub fn convert(frame: &[u8], mode: &Mode, stride: u32, out: Out) -> Vec<u8> {
    let (w, h) = (mode.width as usize, mode.height as usize);
    let natural = if mode.fourcc == v4l2::YUYV { w * 2 } else { w };
    let stride = if stride == 0 {
        natural
    } else {
        stride as usize
    };
    let mut buf = Vec::new();
    match (out, mode.fourcc) {
        (Out::Rgb32, v4l2::YUYV) => v4l2::yuyv_to_rgb32(frame, w, h, stride, &mut buf),
        (Out::Rgb32, v4l2::NV12) => v4l2::nv12_to_rgb32(frame, w, h, stride, &mut buf),
        (Out::Nv12, v4l2::YUYV) => v4l2::yuyv_to_nv12(frame, w, h, stride, &mut buf),
        _ => buf = v4l2::pack(frame, mode, stride),
    }
    buf
}

// ---------- IMFMediaBuffer ----------

#[repr(C)]
struct MemBuf {
    vtbl: *const (),
    refs: AtomicU32,
    data: Box<[u8]>,
    len: AtomicU32,
}

unsafe extern "win64" fn b_qi(this: *mut MemBuf, riid: *const u8, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let iid = comobj::read_guid(riid);
    if iid == Some(IID_IUNKNOWN) || iid == Some(IID_IMFMEDIABUFFER) {
        (*this).refs.fetch_add(1, Ordering::Relaxed);
        *out = this as *mut c_void;
        S_OK
    } else {
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
}
unsafe extern "win64" fn b_addref(this: *mut MemBuf) -> u32 {
    (*this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn b_release(this: *mut MemBuf) -> u32 {
    let n = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this));
    }
    n
}
unsafe extern "win64" fn b_lock(
    this: *mut MemBuf,
    ptr: *mut *mut u8,
    max: *mut u32,
    cur: *mut u32,
) -> i32 {
    if ptr.is_null() {
        return E_POINTER;
    }
    let b = &mut *this;
    *ptr = b.data.as_mut_ptr();
    if !max.is_null() {
        *max = b.data.len() as u32;
    }
    if !cur.is_null() {
        *cur = b.len.load(Ordering::Relaxed);
    }
    S_OK
}
unsafe extern "win64" fn b_unlock(_this: *mut MemBuf) -> i32 {
    S_OK
}
unsafe extern "win64" fn b_get_current_length(this: *mut MemBuf, n: *mut u32) -> i32 {
    if n.is_null() {
        return E_POINTER;
    }
    *n = (*this).len.load(Ordering::Relaxed);
    S_OK
}
unsafe extern "win64" fn b_set_current_length(this: *mut MemBuf, n: u32) -> i32 {
    if n as usize > (&(*this)).data.len() {
        return E_INVALIDARG;
    }
    (*this).len.store(n, Ordering::Relaxed);
    S_OK
}
unsafe extern "win64" fn b_get_max_length(this: *mut MemBuf, n: *mut u32) -> i32 {
    if n.is_null() {
        return E_POINTER;
    }
    *n = (&(*this)).data.len() as u32;
    S_OK
}

static BUFFER_VTBL: VTable<8> = VTable([
    b_qi as *const (),
    b_addref as *const (),
    b_release as *const (),
    b_lock as *const (),
    b_unlock as *const (),
    b_get_current_length as *const (),
    b_set_current_length as *const (),
    b_get_max_length as *const (),
]);

pub fn create_memory_buffer(max: usize) -> *mut c_void {
    Box::into_raw(Box::new(MemBuf {
        vtbl: BUFFER_VTBL.0.as_ptr() as *const (),
        refs: AtomicU32::new(1),
        data: vec![0u8; max].into_boxed_slice(),
        len: AtomicU32::new(0),
    })) as *mut c_void
}

fn buffer_from(bytes: &[u8]) -> *mut c_void {
    let b = create_memory_buffer(bytes.len());
    unsafe {
        let m = &mut *(b as *mut MemBuf);
        m.data.copy_from_slice(bytes);
        m.len.store(bytes.len() as u32, Ordering::Relaxed);
    }
    b
}

/// Lit le contenu d'un IMFMediaBuffer quelconque (Lock/Unlock).
unsafe fn buffer_bytes(b: *mut c_void) -> Vec<u8> {
    let lock_fn: unsafe extern "win64" fn(*mut c_void, *mut *mut u8, *mut u32, *mut u32) -> i32 =
        std::mem::transmute(slot(b, 3));
    let unlock_fn: unsafe extern "win64" fn(*mut c_void) -> i32 = std::mem::transmute(slot(b, 4));
    let mut p = std::ptr::null_mut();
    let mut cur = 0u32;
    if lock_fn(b, &mut p, std::ptr::null_mut(), &mut cur) < 0 || p.is_null() {
        return Vec::new();
    }
    let v = std::slice::from_raw_parts(p, cur as usize).to_vec();
    unlock_fn(b);
    v
}

// ---------- IMFSample ----------

unsafe fn sample<'a>(this: *mut MfObj) -> Option<std::sync::MutexGuard<'a, SampleData>> {
    match &obj(this).ext {
        Ext::Sample(s) => Some(lock(s)),
        _ => None,
    }
}
unsafe extern "win64" fn s_get_flags(this: *mut MfObj, f: *mut u32) -> i32 {
    if f.is_null() {
        return E_POINTER;
    }
    *f = sample(this).map_or(0, |s| s.flags);
    S_OK
}
unsafe extern "win64" fn s_set_flags(this: *mut MfObj, f: u32) -> i32 {
    if let Some(mut s) = sample(this) {
        s.flags = f;
    }
    S_OK
}
unsafe extern "win64" fn s_get_time(this: *mut MfObj, t: *mut i64) -> i32 {
    if t.is_null() {
        return E_POINTER;
    }
    *t = sample(this).map_or(0, |s| s.time);
    S_OK
}
unsafe extern "win64" fn s_set_time(this: *mut MfObj, t: i64) -> i32 {
    if let Some(mut s) = sample(this) {
        s.time = t;
    }
    S_OK
}
unsafe extern "win64" fn s_get_duration(this: *mut MfObj, t: *mut i64) -> i32 {
    if t.is_null() {
        return E_POINTER;
    }
    *t = sample(this).map_or(0, |s| s.duration);
    S_OK
}
unsafe extern "win64" fn s_set_duration(this: *mut MfObj, t: i64) -> i32 {
    if let Some(mut s) = sample(this) {
        s.duration = t;
    }
    S_OK
}
unsafe extern "win64" fn s_get_buffer_count(this: *mut MfObj, n: *mut u32) -> i32 {
    if n.is_null() {
        return E_POINTER;
    }
    *n = sample(this).map_or(0, |s| s.buffers.len() as u32);
    S_OK
}
unsafe extern "win64" fn s_get_buffer_by_index(
    this: *mut MfObj,
    i: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(b) = sample(this).and_then(|s| s.buffers.get(i as usize).copied()) else {
        return E_INVALIDARG;
    };
    com_addref(b as *mut c_void);
    *out = b as *mut c_void;
    S_OK
}
unsafe extern "win64" fn s_convert_to_contiguous(this: *mut MfObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(mut s) = sample(this) else {
        return E_INVALIDARG;
    };
    if s.buffers.len() == 1 {
        com_addref(s.buffers[0] as *mut c_void);
        *out = s.buffers[0] as *mut c_void;
        return S_OK;
    }
    let mut all = Vec::new();
    for &b in &s.buffers {
        all.extend(buffer_bytes(b as *mut c_void));
    }
    for b in s.buffers.drain(..) {
        com_release(b as *mut c_void);
    }
    let nb = buffer_from(&all);
    s.buffers.push(nb as usize);
    com_addref(nb);
    *out = nb;
    S_OK
}
unsafe extern "win64" fn s_add_buffer(this: *mut MfObj, b: *mut c_void) -> i32 {
    if b.is_null() {
        return E_POINTER;
    }
    if let Some(mut s) = sample(this) {
        com_addref(b);
        s.buffers.push(b as usize);
    }
    S_OK
}
unsafe extern "win64" fn s_remove_buffer_by_index(this: *mut MfObj, i: u32) -> i32 {
    let Some(mut s) = sample(this) else {
        return E_INVALIDARG;
    };
    if (i as usize) < s.buffers.len() {
        let b = s.buffers.remove(i as usize);
        com_release(b as *mut c_void);
        S_OK
    } else {
        E_INVALIDARG
    }
}
unsafe extern "win64" fn s_remove_all_buffers(this: *mut MfObj) -> i32 {
    if let Some(mut s) = sample(this) {
        for b in s.buffers.drain(..) {
            com_release(b as *mut c_void);
        }
    }
    S_OK
}
unsafe extern "win64" fn s_get_total_length(this: *mut MfObj, n: *mut u32) -> i32 {
    if n.is_null() {
        return E_POINTER;
    }
    let bufs = sample(this).map(|s| s.buffers.clone()).unwrap_or_default();
    let mut total = 0u32;
    for b in bufs {
        let f: unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32 =
            std::mem::transmute(slot(b as *mut c_void, 5));
        let mut l = 0;
        f(b as *mut c_void, &mut l);
        total += l;
    }
    *n = total;
    S_OK
}
unsafe extern "win64" fn s_copy_to_buffer(this: *mut MfObj, dest: *mut c_void) -> i32 {
    if dest.is_null() {
        return E_POINTER;
    }
    let bufs = sample(this).map(|s| s.buffers.clone()).unwrap_or_default();
    let mut all = Vec::new();
    for b in bufs {
        all.extend(buffer_bytes(b as *mut c_void));
    }
    let lock_fn: unsafe extern "win64" fn(*mut c_void, *mut *mut u8, *mut u32, *mut u32) -> i32 =
        std::mem::transmute(slot(dest, 3));
    let unlock_fn: unsafe extern "win64" fn(*mut c_void) -> i32 =
        std::mem::transmute(slot(dest, 4));
    let set_len: unsafe extern "win64" fn(*mut c_void, u32) -> i32 =
        std::mem::transmute(slot(dest, 6));
    let mut p = std::ptr::null_mut();
    let mut max = 0u32;
    if lock_fn(dest, &mut p, &mut max, std::ptr::null_mut()) < 0 {
        return E_INVALIDARG;
    }
    if (max as usize) < all.len() {
        unlock_fn(dest);
        return MF_E_BUFFERTOOSMALL;
    }
    std::ptr::copy_nonoverlapping(all.as_ptr(), p, all.len());
    unlock_fn(dest);
    set_len(dest, all.len() as u32);
    S_OK
}

static SAMPLE_VTBL: VTable<47> = VTable(concat(
    attr_vtbl_entries!(),
    [
        s_get_flags as *const (),
        s_set_flags as *const (),
        s_get_time as *const (),
        s_set_time as *const (),
        s_get_duration as *const (),
        s_set_duration as *const (),
        s_get_buffer_count as *const (),
        s_get_buffer_by_index as *const (),
        s_convert_to_contiguous as *const (),
        s_add_buffer as *const (),
        s_remove_buffer_by_index as *const (),
        s_remove_all_buffers as *const (),
        s_get_total_length as *const (),
        s_copy_to_buffer as *const (),
    ],
));

/// Échantillon contenant une copie de `bytes`.
fn sample_from(bytes: &[u8], time: i64, duration: i64) -> *mut c_void {
    let s = create_sample();
    unsafe {
        let b = buffer_from(bytes);
        if let Some(mut d) = sample(s as *mut MfObj) {
            d.time = time;
            d.duration = duration;
            d.buffers.push(b as usize);
        }
    }
    s
}

// ---------- IMFActivate (caméra) ----------

unsafe extern "win64" fn act_activate_object(
    this: *mut MfObj,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Ext::Activate(cam) = &obj(this).ext else {
        return E_INVALIDARG;
    };
    let src = create_media_source(cam.clone());
    let hr = source_qi(src as *mut MediaSource, riid, out);
    source_release(src as *mut MediaSource);
    hr
}
unsafe extern "win64" fn act_shutdown_object(_this: *mut MfObj) -> i32 {
    S_OK
}

static ACTIVATE_VTBL: VTable<36> = VTable(concat(
    attr_vtbl_entries!(),
    [
        act_activate_object as *const (),
        act_shutdown_object as *const (),
        act_shutdown_object as *const (),
    ],
));

/// IMFActivate décrivant une caméra.
pub fn create_camera_activate(cam: Camera) -> *mut c_void {
    let name = cam.name.clone();
    let link = cam.path.display().to_string();
    let a = new_obj(Ext::Activate(cam));
    {
        let mut at = lock(unsafe { &(*a).attrs });
        at.set_str(MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, &name);
        at.set(
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
            Val::Guid(MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID),
        );
        at.set_str(
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
            &link,
        );
    }
    a as *mut c_void
}

/// Lit le type de source demandé dans les attributs d'énumération.
pub unsafe fn requested_source_type(attrs: *mut c_void) -> Option<Guid> {
    attr_get_guid(attrs, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE)
}

/// Lien symbolique de caméra demandé à MFCreateDeviceSource.
pub unsafe fn requested_symbolic_link(attrs: *mut c_void) -> Option<String> {
    attr_get_string(
        attrs,
        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
    )
}

// ---------- IMFMediaSource ----------

#[repr(C)]
pub struct MediaSource {
    vtbl: *const (),
    refs: AtomicU32,
    camera: Camera,
}

unsafe extern "win64" fn source_qi(
    this: *mut MediaSource,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let iid = comobj::read_guid(riid);
    if [IID_IUNKNOWN, IID_IMFMEDIASOURCE, IID_IMFMEDIAEVENTGENERATOR]
        .iter()
        .any(|g| Some(*g) == iid)
    {
        (*this).refs.fetch_add(1, Ordering::Relaxed);
        *out = this as *mut c_void;
        S_OK
    } else {
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
}
unsafe extern "win64" fn source_addref(this: *mut MediaSource) -> u32 {
    (*this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn source_release(this: *mut MediaSource) -> u32 {
    let n = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this));
    }
    n
}
unsafe extern "win64" fn source_notimpl(_this: *mut MediaSource) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn source_characteristics(_this: *mut MediaSource, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 1; // MFMEDIASOURCE_IS_LIVE
    S_OK
}
unsafe extern "win64" fn source_ok(_this: *mut MediaSource) -> i32 {
    S_OK
}

static SOURCE_VTBL: VTable<13> = VTable([
    source_qi as *const (),
    source_addref as *const (),
    source_release as *const (),
    source_notimpl as *const (), // GetEvent
    source_notimpl as *const (), // BeginGetEvent
    source_notimpl as *const (), // EndGetEvent
    source_notimpl as *const (), // QueueEvent
    source_characteristics as *const (),
    source_notimpl as *const (), // CreatePresentationDescriptor
    source_notimpl as *const (), // Start
    source_ok as *const (),      // Stop
    source_ok as *const (),      // Pause
    source_ok as *const (),      // Shutdown
]);

pub fn create_media_source(camera: Camera) -> *mut c_void {
    Box::into_raw(Box::new(MediaSource {
        vtbl: SOURCE_VTBL.0.as_ptr() as *const (),
        refs: AtomicU32::new(1),
        camera,
    })) as *mut c_void
}

fn source_camera(src: *mut c_void) -> Option<Camera> {
    if src.is_null() || unsafe { *(src as *const *const ()) } != SOURCE_VTBL.0.as_ptr() as *const ()
    {
        return None;
    }
    Some(unsafe { (*(src as *mut MediaSource)).camera.clone() })
}

// ---------- IMFSourceReader ----------

const STREAM_FIRST_VIDEO: u32 = 0xFFFF_FFFC;
const STREAM_ANY: u32 = 0xFFFF_FFFE;
const STREAM_ALL: u32 = 0xFFFF_FFFE;
const READERF_ERROR: u32 = 0x1;
const READERF_STREAMTICK: u32 = 0x100;

struct ReaderState {
    mode: Mode,
    out: Out,
    stream: Option<v4l2::Stream>,
    selected: bool,
}

struct Core {
    camera: Camera,
    state: Mutex<ReaderState>,
    callback: usize,
}

impl Drop for Core {
    fn drop(&mut self) {
        unsafe { com_release(self.callback as *mut c_void) };
    }
}

enum Job {
    Read(u32),
    Flush(u32),
}

#[repr(C)]
struct Reader {
    vtbl: *const (),
    refs: AtomicU32,
    core: Arc<Core>,
    jobs: Mutex<Option<mpsc::Sender<Job>>>,
}

fn valid_stream(s: u32) -> bool {
    matches!(s, 0 | STREAM_FIRST_VIDEO | STREAM_ANY)
}

impl Core {
    /// Lit une image ; renvoie (hr, drapeaux, horodatage, échantillon).
    fn read(&self) -> (i32, u32, i64, *mut c_void) {
        let mut s = lock(&self.state);
        if !s.selected {
            return (MF_E_INVALIDREQUEST, READERF_ERROR, 0, std::ptr::null_mut());
        }
        if s.stream.is_none() {
            s.stream = v4l2::Stream::open(&self.camera.path, s.mode);
            if s.stream.is_none() {
                return (
                    MF_E_VIDEO_DEVICE_INVALIDATED,
                    READERF_ERROR,
                    0,
                    std::ptr::null_mut(),
                );
            }
        }
        let (mode, out) = (s.mode, s.out);
        let Some(stream) = s.stream.as_mut() else {
            return (
                MF_E_VIDEO_DEVICE_INVALIDATED,
                READERF_ERROR,
                0,
                std::ptr::null_mut(),
            );
        };
        let stride = stream.stride;
        let actual = stream.mode;
        match stream.read_frame(Duration::from_secs(3)) {
            Some((frame, ts)) => {
                let bytes = convert(&frame, &actual, stride, out);
                let duration =
                    10_000_000i64 * mode.fps_den.max(1) as i64 / mode.fps_num.max(1) as i64;
                (S_OK, 0, ts, sample_from(&bytes, ts, duration))
            }
            None => (S_OK, READERF_STREAMTICK, 0, std::ptr::null_mut()),
        }
    }
}

unsafe fn reader<'a>(this: *mut Reader) -> &'a Reader {
    &*this
}

unsafe extern "win64" fn r_qi(this: *mut Reader, riid: *const u8, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let iid = comobj::read_guid(riid);
    if iid == Some(IID_IUNKNOWN) || iid == Some(IID_IMFSOURCEREADER) {
        reader(this).refs.fetch_add(1, Ordering::Relaxed);
        *out = this as *mut c_void;
        S_OK
    } else {
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
}
unsafe extern "win64" fn r_addref(this: *mut Reader) -> u32 {
    reader(this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn r_release(this: *mut Reader) -> u32 {
    let n = reader(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this));
    }
    n
}
unsafe extern "win64" fn r_get_stream_selection(this: *mut Reader, s: u32, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if !valid_stream(s) {
        return MF_E_INVALIDSTREAMNUMBER;
    }
    *out = i32::from(lock(&reader(this).core.state).selected);
    S_OK
}
unsafe extern "win64" fn r_set_stream_selection(this: *mut Reader, s: u32, sel: i32) -> i32 {
    if !valid_stream(s) && s != STREAM_ALL && s != 0xFFFF_FFFF {
        return MF_E_INVALIDSTREAMNUMBER;
    }
    let mut st = lock(&reader(this).core.state);
    st.selected = sel != 0;
    if !st.selected {
        st.stream = None;
    }
    S_OK
}
unsafe extern "win64" fn r_get_native_media_type(
    this: *mut Reader,
    s: u32,
    index: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if !valid_stream(s) {
        return MF_E_INVALIDSTREAMNUMBER;
    }
    match reader(this).core.camera.modes.get(index as usize) {
        Some(m) => {
            *out = media_type_for(m, Out::Native);
            S_OK
        }
        None => MF_E_NO_MORE_TYPES,
    }
}
unsafe extern "win64" fn r_get_current_media_type(
    this: *mut Reader,
    s: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if !valid_stream(s) {
        return MF_E_INVALIDSTREAMNUMBER;
    }
    let st = lock(&reader(this).core.state);
    *out = media_type_for(&st.mode, st.out);
    S_OK
}
unsafe extern "win64" fn r_set_current_media_type(
    this: *mut Reader,
    s: u32,
    _reserved: *mut u32,
    ty: *mut c_void,
) -> i32 {
    if ty.is_null() {
        return E_POINTER;
    }
    if !valid_stream(s) {
        return MF_E_INVALIDSTREAMNUMBER;
    }
    let Some(sub) = attr_get_guid(ty, &MF_MT_SUBTYPE) else {
        return MF_E_INVALIDMEDIATYPE;
    };
    let size = attr_get_u64(ty, &MF_MT_FRAME_SIZE).map(|v| ((v >> 32) as u32, v as u32));
    let rate = attr_get_u64(ty, &MF_MT_FRAME_RATE).map(|v| ((v >> 32) as u32, v as u32));
    let core = &reader(this).core;
    let mut st = lock(&core.state);
    match choose_mode(&core.camera.modes, sub, size, rate, &st.mode) {
        Some((mode, out)) => {
            if mode != st.mode {
                st.stream = None;
            }
            st.mode = mode;
            st.out = out;
            S_OK
        }
        None => MF_E_INVALIDMEDIATYPE,
    }
}
unsafe extern "win64" fn r_set_current_position(
    _this: *mut Reader,
    _g: *const u8,
    _p: *const u8,
) -> i32 {
    MF_E_INVALIDREQUEST
}
unsafe extern "win64" fn r_read_sample(
    this: *mut Reader,
    s: u32,
    _control: u32,
    actual: *mut u32,
    flags: *mut u32,
    ts: *mut i64,
    out: *mut *mut c_void,
) -> i32 {
    if !valid_stream(s) {
        return MF_E_INVALIDSTREAMNUMBER;
    }
    let r = reader(this);
    if r.core.callback != 0 {
        if !out.is_null() || !flags.is_null() || !actual.is_null() || !ts.is_null() {
            return E_INVALIDARG;
        }
        return match lock(&r.jobs).as_ref().map(|tx| tx.send(Job::Read(s))) {
            Some(Ok(())) => S_OK,
            _ => MF_E_INVALIDREQUEST,
        };
    }
    if out.is_null() || flags.is_null() {
        return E_POINTER;
    }
    let (hr, f, t, smp) = r.core.read();
    if !actual.is_null() {
        *actual = 0;
    }
    *flags = f;
    if !ts.is_null() {
        *ts = t;
    }
    *out = smp;
    hr
}
unsafe extern "win64" fn r_flush(this: *mut Reader, s: u32) -> i32 {
    let r = reader(this);
    if r.core.callback != 0 {
        if let Some(tx) = lock(&r.jobs).as_ref() {
            let _ = tx.send(Job::Flush(s));
        }
    }
    S_OK
}
unsafe extern "win64" fn r_get_service_for_stream(
    _this: *mut Reader,
    _s: u32,
    _g: *const u8,
    _riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    MF_E_UNSUPPORTED_SERVICE
}
unsafe extern "win64" fn r_get_presentation_attribute(
    _this: *mut Reader,
    _s: u32,
    _g: *const u8,
    _pv: *mut u8,
) -> i32 {
    MF_E_ATTRIBUTENOTFOUND
}

static READER_VTBL: VTable<13> = VTable([
    r_qi as *const (),
    r_addref as *const (),
    r_release as *const (),
    r_get_stream_selection as *const (),
    r_set_stream_selection as *const (),
    r_get_native_media_type as *const (),
    r_get_current_media_type as *const (),
    r_set_current_media_type as *const (),
    r_set_current_position as *const (),
    r_read_sample as *const (),
    r_flush as *const (),
    r_get_service_for_stream as *const (),
    r_get_presentation_attribute as *const (),
]);

/// Fil de travail du mode asynchrone : appelle IMFSourceReaderCallback.
fn spawn_worker(core: Arc<Core>) -> Option<mpsc::Sender<Job>> {
    let (tx, rx) = mpsc::channel::<Job>();
    std::thread::Builder::new()
        .name("winlux-mf-reader".into())
        .spawn(move || {
            crate::process::init_thread();
            let cb = core.callback as *mut c_void;
            for job in rx {
                unsafe {
                    match job {
                        Job::Read(stream) => {
                            let (hr, flags, ts, smp) = core.read();
                            let f: unsafe extern "win64" fn(
                                *mut c_void,
                                i32,
                                u32,
                                u32,
                                i64,
                                *mut c_void,
                            ) -> i32 = std::mem::transmute(slot(cb, 3));
                            let index = if stream == 0 || valid_stream(stream) {
                                0
                            } else {
                                stream
                            };
                            f(cb, hr, index, flags, ts, smp);
                            com_release(smp);
                        }
                        Job::Flush(stream) => {
                            let f: unsafe extern "win64" fn(*mut c_void, u32) -> i32 =
                                std::mem::transmute(slot(cb, 4));
                            f(cb, stream);
                        }
                    }
                }
            }
        })
        .ok()?;
    Some(tx)
}

/// MFCreateSourceReaderFromMediaSource.
pub unsafe fn create_source_reader(
    source: *mut c_void,
    attrs: *mut c_void,
) -> Result<*mut c_void, i32> {
    let camera = source_camera(source).ok_or(E_INVALIDARG)?;
    let first = *camera.modes.first().ok_or(MF_E_INVALIDMEDIATYPE)?;
    let callback = attr_get_unknown(
        attrs,
        &MF_SOURCE_READER_ASYNC_CALLBACK,
        &IID_IMFSOURCEREADERCALLBACK,
    )
    .or_else(|| {
        attr_get_unknown(attrs, &MF_SOURCE_READER_ASYNC_CALLBACK, &IID_IUNKNOWN).and_then(|u| {
            let cb = com_qi(u, &IID_IMFSOURCEREADERCALLBACK);
            com_release(u);
            cb
        })
    })
    .map_or(0, |p| p as usize);
    let core = Arc::new(Core {
        camera,
        state: Mutex::new(ReaderState {
            mode: first,
            out: Out::Native,
            stream: None,
            selected: true,
        }),
        callback,
    });
    let jobs = if callback != 0 {
        spawn_worker(core.clone())
    } else {
        None
    };
    Ok(Box::into_raw(Box::new(Reader {
        vtbl: READER_VTBL.0.as_ptr() as *const (),
        refs: AtomicU32::new(1),
        core,
        jobs: Mutex::new(jobs),
    })) as *mut c_void)
}

/// Caméras proposées par MFEnumDeviceSources.
pub fn camera_activates() -> Vec<*mut c_void> {
    v4l2::cameras()
        .into_iter()
        .map(create_camera_activate)
        .collect()
}

/// Source pour MFCreateDeviceSource (lien symbolique, sinon 1re caméra).
pub fn camera_source(link: Option<&str>) -> Option<*mut c_void> {
    let cams = v4l2::cameras();
    let cam = match link {
        Some(l) => cams
            .into_iter()
            .find(|c| c.path.display().to_string() == l)?,
        None => cams.into_iter().next()?,
    };
    Some(create_media_source(cam))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(fourcc: u32, w: u32, h: u32) -> Mode {
        Mode {
            fourcc,
            width: w,
            height: h,
            fps_num: 30,
            fps_den: 1,
        }
    }

    #[test]
    fn attributs_aller_retour_par_la_table_virtuelle() {
        unsafe {
            let a = create_media_type() as *mut MfObj;
            assert_eq!(
                a_set_u64(a, MF_MT_FRAME_SIZE.as_ptr(), (640 << 32) | 480),
                S_OK
            );
            assert_eq!(
                a_set_guid(a, MF_MT_SUBTYPE.as_ptr(), MFVIDEOFORMAT_RGB32.as_ptr()),
                S_OK
            );
            let name: Vec<u16> = "Cam\u{e9}ra\0".encode_utf16().collect();
            a_set_string(
                a,
                MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME.as_ptr(),
                name.as_ptr(),
            );
            assert_eq!(
                attr_get_u64(a as *mut c_void, &MF_MT_FRAME_SIZE),
                Some((640 << 32) | 480)
            );
            assert_eq!(
                attr_get_guid(a as *mut c_void, &MF_MT_SUBTYPE),
                Some(MFVIDEOFORMAT_RGB32)
            );
            assert_eq!(
                attr_get_string(a as *mut c_void, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME).as_deref(),
                Some("Cam\u{e9}ra")
            );
            let mut v = 0u32;
            assert_eq!(
                a_get_u32(a, MF_MT_FRAME_SIZE.as_ptr(), &mut v),
                MF_E_INVALIDTYPE
            );
            assert_eq!(
                a_get_u32(a, MF_MT_SAMPLE_SIZE.as_ptr(), &mut v),
                MF_E_ATTRIBUTENOTFOUND
            );
            let mut n = 0;
            a_get_count(a, &mut n);
            assert_eq!(n, 3);
            // Copie vers un autre magasin, puis comparaison.
            let b = create_attributes() as *mut MfObj;
            assert_eq!(a_copy_all_items(a, b as *mut c_void), S_OK);
            let mut same = 0;
            a_compare(a, b as *mut c_void, 0, &mut same);
            assert_eq!(same, 1);
            let mut q: *mut c_void = std::ptr::null_mut();
            assert_eq!(a_qi(b, IID_IMFMEDIATYPE.as_ptr(), &mut q), E_NOINTERFACE);
            assert_eq!(a_qi(a, IID_IMFMEDIATYPE.as_ptr(), &mut q), S_OK);
            a_release(q as *mut MfObj);
            assert_eq!(a_release(b), 0);
            assert_eq!(a_release(a), 0);
        }
    }

    #[test]
    fn echantillon_et_tampon_contigu() {
        unsafe {
            let s = create_sample() as *mut MfObj;
            let b1 = buffer_from(&[1, 2, 3]);
            let b2 = buffer_from(&[4, 5]);
            s_add_buffer(s, b1);
            s_add_buffer(s, b2);
            com_release(b1);
            com_release(b2);
            let mut total = 0;
            s_get_total_length(s, &mut total);
            assert_eq!(total, 5);
            let mut c: *mut c_void = std::ptr::null_mut();
            assert_eq!(s_convert_to_contiguous(s, &mut c), S_OK);
            assert_eq!(buffer_bytes(c), vec![1, 2, 3, 4, 5]);
            com_release(c);
            let mut n = 0;
            s_get_buffer_count(s, &mut n);
            assert_eq!(n, 1);
            assert_eq!(a_release(s), 0);
        }
    }

    #[test]
    fn choix_du_mode_et_conversion() {
        let modes = [
            mode(v4l2::YUYV, 640, 480),
            mode(v4l2::YUYV, 1280, 720),
            mode(v4l2::MJPG, 1920, 1080),
        ];
        let cur = modes[0];
        assert_eq!(
            choose_mode(&modes, MFVIDEOFORMAT_RGB32, Some((1280, 720)), None, &cur),
            Some((modes[1], Out::Rgb32))
        );
        assert_eq!(
            choose_mode(&modes, MFVIDEOFORMAT_NV12, None, None, &cur),
            Some((modes[0], Out::Nv12))
        );
        assert_eq!(
            choose_mode(&modes, MFVIDEOFORMAT_MJPG, Some((1920, 1080)), None, &cur),
            Some((modes[2], Out::Native))
        );
        assert_eq!(
            choose_mode(&modes, MFVIDEOFORMAT_RGB32, Some((1920, 1080)), None, &cur),
            None
        );
        let rgb = convert(
            &[235, 128, 235, 128],
            &mode(v4l2::YUYV, 2, 1),
            4,
            Out::Rgb32,
        );
        assert_eq!(rgb, vec![255; 8]);
        let t = media_type_for(&modes[1], Out::Rgb32);
        unsafe {
            assert_eq!(attr_get_u64(t, &MF_MT_FRAME_SIZE), Some((1280 << 32) | 720));
            assert_eq!(attr_get_guid(t, &MF_MT_MAJOR_TYPE), Some(MFMEDIATYPE_VIDEO));
            com_release(t);
        }
    }

    #[test]
    fn lecteur_refuse_une_source_etrangere() {
        unsafe {
            let a = create_attributes();
            assert_eq!(
                create_source_reader(a, std::ptr::null_mut()).err(),
                Some(E_INVALIDARG)
            );
            com_release(a);
        }
    }
}
