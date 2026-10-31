//! DXVA2 (`dxva2.dll`) : décodage vidéo matériel exposé aux .exe, traduit
//! vers VA-API (Intel, AMD, et NVIDIA via `nvidia-vaapi-driver`).
//!
//! Couvre `IDirect3DDeviceManager9`, `IDirectXVideoDecoderService`
//! (surfaces NV12 Direct3D 9, GUID des décodeurs, configurations) et
//! `IDirectXVideoDecoder` pour H.264 (format de tranches « long »).
//!
//! Garde-fous : la liste des décodeurs annoncés est exactement celle que la
//! machine décode en matériel (`gpu_caps::hardware_codecs`). Sans VA-API,
//! ou si l'utilisateur a choisi « logiciel » dans les Paramètres, la liste
//! est vide : l'application prend son décodeur logiciel, comme sous Windows
//! sans accélération. Tout échec VA-API à la création renvoie `E_FAIL`, que
//! les lecteurs traitent aussi par un repli logiciel.
//!
//! Les fonctions de configuration d'écran de `dxva2.dll` (luminosité…)
//! répondent « non disponible ».
#![allow(non_snake_case, dead_code, clippy::missing_safety_doc)]

use crate::dlls::d3d9;
use crate::dlls::gpu_caps::{self, Codec};
use crate::dlls::ole32::Guid;
use crate::dlls::va_bindings::{self as va, VABufferID, VAImage, VaSession};
use crate::{log_info, log_warn};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

const S_OK: i32 = 0;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000eu32 as i32;
const DXVA2_E_NOT_AVAILABLE: i32 = 0x8004_2603u32 as i32;
const E_HANDLE: i32 = 0x8007_0006u32 as i32;

/// Seuls les codecs dont le pont DXVA2 → VA-API est écrit sont annoncés.
pub(crate) fn bridge_supports(codec: Codec) -> bool {
    matches!(codec, Codec::H264)
}

const fn guid(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Guid {
    Guid {
        data1: d1,
        data2: d2,
        data3: d3,
        data4: d4,
    }
}

/// DXVA2_ModeH264_E (VLD, sans grain de film) et _F (avec).
const MODE_H264_E: Guid = guid(
    0x1b81_be68,
    0xa0c7,
    0x11d3,
    [0xb9, 0x84, 0x00, 0xc0, 0x4f, 0x2e, 0x73, 0xc5],
);
const MODE_H264_F: Guid = guid(
    0x1b81_be69,
    0xa0c7,
    0x11d3,
    [0xb9, 0x84, 0x00, 0xc0, 0x4f, 0x2e, 0x73, 0xc5],
);
const NO_ENCRYPT: Guid = guid(
    0x1b81_bed0,
    0xa0c7,
    0x11d3,
    [0xb9, 0x84, 0x00, 0xc0, 0x4f, 0x2e, 0x73, 0xc5],
);

fn same(a: &Guid, b: &Guid) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

fn codec_guids(c: Codec) -> &'static [Guid] {
    match c {
        Codec::H264 => &[MODE_H264_E, MODE_H264_F],
        _ => &[],
    }
}

/// GUID annoncés sur cette machine, dans l'ordre de préférence.
fn advertised() -> Vec<Guid> {
    let caps = gpu_caps::caps();
    let prefs = gpu_caps::prefs();
    gpu_caps::hardware_codecs(caps, &prefs)
        .into_iter()
        .flat_map(|c| codec_guids(c).iter().copied())
        .collect()
}

fn is_advertised(g: &Guid) -> bool {
    advertised().iter().any(|a| same(a, g))
}

// ── Objets COM ─────────────────────────────────────────────────────────────

#[repr(C)]
struct Obj {
    vtbl: *const usize,
    refs: AtomicU32,
    kind: Kind,
}

enum Kind {
    Manager(Mutex<Manager>),
    Service(*mut c_void),
    Decoder(Box<Mutex<Decoder>>),
}

struct Manager {
    device: *mut c_void,
    token: u32,
}

unsafe impl Send for Manager {}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

unsafe fn com_call(p: *mut c_void, slot: usize) -> u32 {
    let vt = *(p as *const *const usize);
    let f: unsafe extern "win64" fn(*mut c_void) -> u32 = std::mem::transmute(*vt.add(slot));
    f(p)
}
unsafe fn addref(p: *mut c_void) {
    if !p.is_null() {
        com_call(p, 1);
    }
}
unsafe fn release(p: *mut c_void) {
    if !p.is_null() {
        com_call(p, 2);
    }
}

unsafe extern "win64" fn stub_ok() -> i32 {
    S_OK
}
unsafe extern "win64" fn stub_notimpl() -> i32 {
    E_NOTIMPL
}

macro_rules! f {
    ($f:expr) => {
        $f as *const () as usize
    };
}

fn build(n: usize, default: usize, entries: &[(usize, usize)]) -> usize {
    let mut v = vec![default; n];
    for &(i, p) in entries {
        v[i] = p;
    }
    Box::leak(v.into_boxed_slice()).as_ptr() as usize
}

fn manager_vtable() -> *const usize {
    static V: OnceLock<usize> = OnceLock::new();
    *V.get_or_init(|| {
        build(
            10,
            f!(stub_notimpl),
            &[
                (0, f!(obj_qi)),
                (1, f!(obj_addref)),
                (2, f!(obj_release)),
                (3, f!(mgr_reset)),
                (4, f!(mgr_open)),
                (5, f!(stub_ok)),
                (6, f!(mgr_test)),
                (7, f!(mgr_lock)),
                (8, f!(stub_ok)),
                (9, f!(mgr_service)),
            ],
        )
    }) as *const usize
}

fn service_vtable() -> *const usize {
    static V: OnceLock<usize> = OnceLock::new();
    *V.get_or_init(|| {
        build(
            8,
            f!(stub_notimpl),
            &[
                (0, f!(obj_qi)),
                (1, f!(obj_addref)),
                (2, f!(obj_release)),
                (3, f!(svc_create_surface)),
                (4, f!(svc_guids)),
                (5, f!(svc_targets)),
                (6, f!(svc_configs)),
                (7, f!(svc_create_decoder)),
            ],
        )
    }) as *const usize
}

fn decoder_vtable() -> *const usize {
    static V: OnceLock<usize> = OnceLock::new();
    *V.get_or_init(|| {
        build(
            10,
            f!(stub_notimpl),
            &[
                (0, f!(obj_qi)),
                (1, f!(obj_addref)),
                (2, f!(obj_release)),
                (3, f!(dec_service)),
                (4, f!(dec_params)),
                (5, f!(dec_get_buffer)),
                (6, f!(stub_ok)),
                (7, f!(dec_begin)),
                (8, f!(dec_end)),
                (9, f!(dec_execute)),
            ],
        )
    }) as *const usize
}

fn new_obj(vtbl: *const usize, kind: Kind) -> *mut Obj {
    Box::into_raw(Box::new(Obj {
        vtbl,
        refs: AtomicU32::new(1),
        kind,
    }))
}

fn supports(kind: &Kind, d1: u32) -> bool {
    d1 == 0
        || match kind {
            Kind::Manager(_) => d1 == 0xa0ca_de0f,
            // IDirectXVideoAccelerationService / DecoderService.
            Kind::Service(_) => d1 == 0xfc51_a54f || d1 == 0xfc51_a551,
            Kind::Decoder(_) => d1 == 0xf2b0_810a,
        }
}

unsafe extern "win64" fn obj_qi(this: *mut Obj, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() || iid.is_null() {
        return E_INVALIDARG;
    }
    if supports(&(*this).kind, (*iid).data1) {
        obj_addref(this);
        *out = this.cast();
        return S_OK;
    }
    *out = std::ptr::null_mut();
    E_NOINTERFACE
}
unsafe extern "win64" fn obj_addref(this: *mut Obj) -> u32 {
    (*this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn obj_release(this: *mut Obj) -> u32 {
    let prev = (*this).refs.fetch_sub(1, Ordering::AcqRel);
    if prev != 1 {
        return prev.saturating_sub(1);
    }
    let obj = Box::from_raw(this);
    match obj.kind {
        Kind::Manager(m) => release(lock(&m).device),
        Kind::Service(dev) => release(dev),
        Kind::Decoder(d) => {
            let mut d = lock(&d);
            d.destroy();
        }
    }
    0
}

unsafe fn hand_out(obj: *mut Obj, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    let hr = obj_qi(obj, iid, out);
    obj_release(obj);
    hr
}

// ── IDirect3DDeviceManager9 ────────────────────────────────────────────────

const DEVICE_HANDLE: usize = 0x5157_0001;

unsafe extern "win64" fn mgr_reset(this: *mut Obj, device: *mut c_void, token: u32) -> i32 {
    let Kind::Manager(m) = &(*this).kind else {
        return E_INVALIDARG;
    };
    let mut m = lock(m);
    if token != m.token || !d3d9::is_device(device) {
        return E_INVALIDARG;
    }
    addref(device);
    release(std::mem::replace(&mut m.device, device));
    S_OK
}
unsafe extern "win64" fn mgr_open(this: *mut Obj, h: *mut usize) -> i32 {
    let Kind::Manager(m) = &(*this).kind else {
        return E_INVALIDARG;
    };
    if h.is_null() {
        return E_INVALIDARG;
    }
    if lock(m).device.is_null() {
        return DXVA2_E_NOT_AVAILABLE;
    }
    *h = DEVICE_HANDLE;
    S_OK
}
unsafe extern "win64" fn mgr_test(_: *mut Obj, h: usize) -> i32 {
    if h == DEVICE_HANDLE {
        S_OK
    } else {
        E_HANDLE
    }
}
unsafe extern "win64" fn mgr_lock(
    this: *mut Obj,
    h: usize,
    out: *mut *mut c_void,
    _block: i32,
) -> i32 {
    let Kind::Manager(m) = &(*this).kind else {
        return E_INVALIDARG;
    };
    if h != DEVICE_HANDLE || out.is_null() {
        return E_HANDLE;
    }
    let dev = lock(m).device;
    addref(dev);
    *out = dev;
    S_OK
}
unsafe extern "win64" fn mgr_service(
    this: *mut Obj,
    h: usize,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    let Kind::Manager(m) = &(*this).kind else {
        return E_INVALIDARG;
    };
    if h != DEVICE_HANDLE {
        return E_HANDLE;
    }
    let dev = lock(m).device;
    DXVA2CreateVideoService(dev, iid, out)
}

// ── IDirectXVideoDecoderService ────────────────────────────────────────────

unsafe fn service_device(this: *mut Obj) -> *mut c_void {
    match &(*this).kind {
        Kind::Service(d) => *d,
        _ => std::ptr::null_mut(),
    }
}

unsafe extern "win64" fn svc_create_surface(
    this: *mut Obj,
    w: u32,
    h: u32,
    back: u32,
    format: u32,
    _pool: u32,
    _usage: u32,
    _ty: u32,
    out: *mut *mut c_void,
    _shared: *mut usize,
) -> i32 {
    let dev = service_device(this);
    if out.is_null() || w == 0 || h == 0 || w > 8192 || h > 8192 {
        return E_INVALIDARG;
    }
    for i in 0..=back as usize {
        match d3d9::create_video_surface(dev.cast(), w, h, format) {
            Some(s) => *out.add(i) = s.cast(),
            None => {
                for k in 0..i {
                    release(*out.add(k));
                    *out.add(k) = std::ptr::null_mut();
                }
                return E_INVALIDARG;
            }
        }
    }
    S_OK
}

unsafe fn co_alloc<T: Copy>(items: &[T]) -> *mut T {
    let p = crate::dlls::ole32::co_task_mem_alloc(std::mem::size_of_val(items).max(1)) as *mut T;
    if !p.is_null() {
        std::ptr::copy_nonoverlapping(items.as_ptr(), p, items.len());
    }
    p
}

unsafe extern "win64" fn svc_guids(_: *mut Obj, count: *mut u32, out: *mut *mut Guid) -> i32 {
    if count.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let g = advertised();
    *count = g.len() as u32;
    *out = co_alloc(&g);
    if g.is_empty() {
        log_info!(
            "dxva2",
            "aucun décodeur matériel annoncé : l'application décodera en logiciel"
        );
    }
    if (*out).is_null() {
        *count = 0;
        return E_OUTOFMEMORY;
    }
    S_OK
}

unsafe extern "win64" fn svc_targets(
    _: *mut Obj,
    g: *const Guid,
    count: *mut u32,
    out: *mut *mut u32,
) -> i32 {
    if g.is_null() || count.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    if !is_advertised(&*g) {
        *count = 0;
        *out = std::ptr::null_mut();
        return E_INVALIDARG;
    }
    let formats = [d3d9::D3DFMT_NV12];
    *count = 1;
    *out = co_alloc(&formats);
    S_OK
}

/// DXVA2_ConfigPictureDecode (100 octets).
#[repr(C)]
#[derive(Clone, Copy)]
struct ConfigPictureDecode {
    bitstream_encryption: Guid,
    mb_control_encryption: Guid,
    resid_diff_encryption: Guid,
    bitstream_raw: u32,
    others: [u32; 11],
    min_render_targets: u16,
    decoder_specific: u16,
}

unsafe extern "win64" fn svc_configs(
    _: *mut Obj,
    g: *const Guid,
    _desc: *const c_void,
    _reserved: *mut c_void,
    count: *mut u32,
    out: *mut *mut ConfigPictureDecode,
) -> i32 {
    if g.is_null() || count.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    if !is_advertised(&*g) {
        *count = 0;
        *out = std::ptr::null_mut();
        return E_INVALIDARG;
    }
    let mut others = [0u32; 11];
    others[7] = 1; // ConfigResidDiffAccelerator
    let cfg = ConfigPictureDecode {
        bitstream_encryption: NO_ENCRYPT,
        mb_control_encryption: NO_ENCRYPT,
        resid_diff_encryption: NO_ENCRYPT,
        bitstream_raw: 1, // tranches au format long
        others,
        min_render_targets: 0,
        decoder_specific: 0,
    };
    *count = 1;
    *out = co_alloc(&[cfg]);
    S_OK
}

unsafe extern "win64" fn svc_create_decoder(
    this: *mut Obj,
    g: *const Guid,
    desc: *const u32,
    cfg: *const ConfigPictureDecode,
    targets: *const *mut c_void,
    n: u32,
    out: *mut *mut c_void,
) -> i32 {
    if g.is_null()
        || desc.is_null()
        || cfg.is_null()
        || targets.is_null()
        || out.is_null()
        || n == 0
        || n > 64
    {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    if !is_advertised(&*g) {
        return E_INVALIDARG;
    }
    if (*cfg).bitstream_raw != 1 {
        log_warn!(
            "dxva2",
            "configuration de tranches {} non prise en charge",
            (*cfg).bitstream_raw
        );
        return E_INVALIDARG;
    }
    let (w, h) = (*desc, *desc.add(1));
    let surfaces: Vec<*mut c_void> = (0..n as usize).map(|i| *targets.add(i)).collect();
    match Decoder::create(*g, w, h, surfaces, service_device(this)) {
        Some(d) => {
            let obj = new_obj(decoder_vtable(), Kind::Decoder(Box::new(Mutex::new(d))));
            hand_out(obj, &guid(0xf2b0_810a, 0, 0, [0; 8]), out)
        }
        None => E_FAIL,
    }
}

// ── Structures VA-API H.264 (va.h, alignement naturel) ─────────────────────

const VA_PICTURE_H264_INVALID: u32 = 0x01;
const VA_PICTURE_H264_TOP_FIELD: u32 = 0x02;
const VA_PICTURE_H264_BOTTOM_FIELD: u32 = 0x04;
const VA_PICTURE_H264_SHORT_TERM_REFERENCE: u32 = 0x08;
const VA_PICTURE_H264_LONG_TERM_REFERENCE: u32 = 0x10;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VAPictureH264 {
    pub picture_id: u32,
    pub frame_idx: u32,
    pub flags: u32,
    pub top_poc: i32,
    pub bottom_poc: i32,
    pub reserved: [u32; 4],
}

impl VAPictureH264 {
    const INVALID: VAPictureH264 = VAPictureH264 {
        picture_id: va::VA_INVALID_SURFACE,
        frame_idx: 0,
        flags: VA_PICTURE_H264_INVALID,
        top_poc: 0,
        bottom_poc: 0,
        reserved: [0; 4],
    };
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct VAPictureParameterBufferH264 {
    pub curr_pic: VAPictureH264,
    pub reference_frames: [VAPictureH264; 16],
    pub width_in_mbs_minus1: u16,
    pub height_in_mbs_minus1: u16,
    pub bit_depth_luma_minus8: u8,
    pub bit_depth_chroma_minus8: u8,
    pub num_ref_frames: u8,
    pub seq_fields: u32,
    pub num_slice_groups_minus1: u8,
    pub slice_group_map_type: u8,
    pub slice_group_change_rate_minus1: u16,
    pub pic_init_qp_minus26: i8,
    pub pic_init_qs_minus26: i8,
    pub chroma_qp_index_offset: i8,
    pub second_chroma_qp_index_offset: i8,
    pub pic_fields: u32,
    pub frame_num: u16,
    pub reserved: [u32; 8],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct VAIQMatrixBufferH264 {
    pub list4x4: [[u8; 16]; 6],
    pub list8x8: [[u8; 64]; 2],
    pub reserved: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct VASliceParameterBufferH264 {
    pub slice_data_size: u32,
    pub slice_data_offset: u32,
    pub slice_data_flag: u32,
    pub slice_data_bit_offset: u16,
    pub first_mb_in_slice: u16,
    pub slice_type: u8,
    pub direct_spatial_mv_pred_flag: u8,
    pub num_ref_idx_l0_active_minus1: u8,
    pub num_ref_idx_l1_active_minus1: u8,
    pub cabac_init_idc: u8,
    pub slice_qp_delta: i8,
    pub disable_deblocking_filter_idc: u8,
    pub slice_alpha_c0_offset_div2: i8,
    pub slice_beta_offset_div2: i8,
    pub ref_pic_list0: [VAPictureH264; 32],
    pub ref_pic_list1: [VAPictureH264; 32],
    pub luma_log2_weight_denom: u8,
    pub chroma_log2_weight_denom: u8,
    pub luma_weight_l0_flag: u8,
    pub luma_weight_l0: [i16; 32],
    pub luma_offset_l0: [i16; 32],
    pub chroma_weight_l0_flag: u8,
    pub chroma_weight_l0: [[i16; 2]; 32],
    pub chroma_offset_l0: [[i16; 2]; 32],
    pub luma_weight_l1_flag: u8,
    pub luma_weight_l1: [i16; 32],
    pub luma_offset_l1: [i16; 32],
    pub chroma_weight_l1_flag: u8,
    pub chroma_weight_l1: [[i16; 2]; 32],
    pub chroma_offset_l1: [[i16; 2]; 32],
    pub reserved: [u32; 4],
}

// ── Traduction DXVA (dxva.h, structures compactées) → VA-API ───────────────

const PP_SIZE: usize = 230; // jusqu'à SliceGroupMap, non utilisé
const SLICE_LONG_SIZE: usize = 864;
const QM_SIZE: usize = 224;

struct Rd<'a>(&'a [u8]);
impl Rd<'_> {
    fn u8(&self, o: usize) -> u8 {
        self.0.get(o).copied().unwrap_or(0)
    }
    fn i8(&self, o: usize) -> i8 {
        self.u8(o) as i8
    }
    fn u16(&self, o: usize) -> u16 {
        u16::from_le_bytes([self.u8(o), self.u8(o + 1)])
    }
    fn i16(&self, o: usize) -> i16 {
        self.u16(o) as i16
    }
    fn u32(&self, o: usize) -> u32 {
        u32::from_le_bytes([self.u8(o), self.u8(o + 1), self.u8(o + 2), self.u8(o + 3)])
    }
    fn i32(&self, o: usize) -> i32 {
        self.u32(o) as i32
    }
}

fn va_surface(surfaces: &[u32], entry: u8) -> Option<u32> {
    if entry == 0xff {
        return None;
    }
    surfaces.get((entry & 0x7f) as usize).copied()
}

/// Liste de référence de l'image (indexée comme RefFrameList).
fn reference_frames(pp: &Rd, surfaces: &[u32]) -> [VAPictureH264; 16] {
    let used = pp.u32(208);
    let mut refs = [VAPictureH264::INVALID; 16];
    for (i, r) in refs.iter_mut().enumerate() {
        let entry = pp.u8(16 + i);
        let Some(id) = va_surface(surfaces, entry) else {
            continue;
        };
        let bits = (used >> (2 * i)) & 3;
        if bits == 0 {
            continue;
        }
        let mut flags = if entry & 0x80 != 0 {
            VA_PICTURE_H264_LONG_TERM_REFERENCE
        } else {
            VA_PICTURE_H264_SHORT_TERM_REFERENCE
        };
        match bits {
            1 => flags |= VA_PICTURE_H264_TOP_FIELD,
            2 => flags |= VA_PICTURE_H264_BOTTOM_FIELD,
            _ => {}
        }
        *r = VAPictureH264 {
            picture_id: id,
            frame_idx: pp.u16(176 + 2 * i) as u32,
            flags,
            top_poc: pp.i32(40 + 8 * i),
            bottom_poc: pp.i32(44 + 8 * i),
            reserved: [0; 4],
        };
    }
    refs
}

pub(crate) fn convert_pic_params(
    raw: &[u8],
    surfaces: &[u32],
) -> Option<VAPictureParameterBufferH264> {
    if raw.len() < PP_SIZE {
        return None;
    }
    let pp = Rd(raw);
    let bits = pp.u16(6) as u32;
    let bit = |n: u32| (bits >> n) & 1;
    let field_pic = bit(0);
    let cur = pp.u8(4);
    let mut curr_pic = VAPictureH264 {
        picture_id: va_surface(surfaces, cur)?,
        frame_idx: pp.u16(214) as u32,
        flags: 0,
        top_poc: pp.i32(32),
        bottom_poc: pp.i32(36),
        reserved: [0; 4],
    };
    if field_pic != 0 {
        curr_pic.flags = if cur & 0x80 != 0 {
            VA_PICTURE_H264_BOTTOM_FIELD
        } else {
            VA_PICTURE_H264_TOP_FIELD
        };
    }
    let seq_fields = ((bits >> 4) & 3) // chroma_format_idc
        | (bit(2) << 2) // residual_colour_transform_flag
        | (bit(12) << 4) // frame_mbs_only_flag
        | (bit(1) << 5) // mb_adaptive_frame_field_flag (MbaffFrameFlag)
        | ((pp.u8(220) as u32 & 1) << 6) // direct_8x8_inference_flag
        | (bit(14) << 7) // MinLumaBiPredSize8x8
        | ((pp.u8(216) as u32 & 0xf) << 8) // log2_max_frame_num_minus4
        | ((pp.u8(217) as u32 & 3) << 12) // pic_order_cnt_type
        | ((pp.u8(218) as u32 & 0xf) << 14) // log2_max_pic_order_cnt_lsb_minus4
        | ((pp.u8(219) as u32 & 1) << 18); // delta_pic_order_always_zero_flag
    let pic_fields = (pp.u8(221) as u32 & 1) // entropy_coding_mode_flag
        | (bit(8) << 1) // weighted_pred_flag
        | (((bits >> 9) & 3) << 2) // weighted_bipred_idc
        | (bit(13) << 4) // transform_8x8_mode_flag
        | (field_pic << 5)
        | (bit(7) << 6) // constrained_intra_pred_flag
        | ((pp.u8(222) as u32 & 1) << 7) // pic_order_present_flag
        | ((pp.u8(225) as u32 & 1) << 8) // deblocking_filter_control_present_flag
        | ((pp.u8(226) as u32 & 1) << 9) // redundant_pic_cnt_present_flag
        | (bit(6) << 10); // reference_pic_flag
    Some(VAPictureParameterBufferH264 {
        curr_pic,
        reference_frames: reference_frames(&pp, surfaces),
        width_in_mbs_minus1: pp.u16(0),
        height_in_mbs_minus1: pp.u16(2),
        bit_depth_luma_minus8: pp.u8(8),
        bit_depth_chroma_minus8: pp.u8(9),
        num_ref_frames: pp.u8(5),
        seq_fields,
        num_slice_groups_minus1: pp.u8(223),
        slice_group_map_type: pp.u8(224),
        slice_group_change_rate_minus1: pp.u16(228),
        pic_init_qp_minus26: pp.i8(172),
        pic_init_qs_minus26: pp.i8(168),
        chroma_qp_index_offset: pp.i8(169),
        second_chroma_qp_index_offset: pp.i8(170),
        pic_fields,
        frame_num: pp.u16(214),
        reserved: [0; 8],
    })
}

const ZIGZAG4: [usize; 16] = [0, 1, 4, 8, 5, 2, 3, 6, 9, 12, 13, 10, 7, 11, 14, 15];
const ZIGZAG8: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// Les listes DXVA sont dans l'ordre du balayage zigzag, VA-API les veut
/// en ordre ligne par ligne.
pub(crate) fn convert_qmatrix(raw: &[u8]) -> Option<VAIQMatrixBufferH264> {
    if raw.len() < QM_SIZE {
        return None;
    }
    let mut q = VAIQMatrixBufferH264 {
        list4x4: [[16; 16]; 6],
        list8x8: [[16; 64]; 2],
        reserved: [0; 4],
    };
    for (l, list) in q.list4x4.iter_mut().enumerate() {
        for (j, &z) in ZIGZAG4.iter().enumerate() {
            list[z] = raw[l * 16 + j];
        }
    }
    for (l, list) in q.list8x8.iter_mut().enumerate() {
        for (j, &z) in ZIGZAG8.iter().enumerate() {
            list[z] = raw[96 + l * 64 + j];
        }
    }
    Some(q)
}

fn start_code_len(data: &[u8], at: usize) -> usize {
    match data.get(at..at + 4) {
        Some([0, 0, 0, 1]) => 4,
        _ => match data.get(at..at + 3) {
            Some([0, 0, 1]) => 3,
            _ => 0,
        },
    }
}

/// Tranche longue DXVA → paramètres de tranche VA (décalages relatifs au
/// tampon de données, code de départ retiré).
pub(crate) fn convert_slice(
    raw: &[u8],
    pp: &VAPictureParameterBufferH264,
    bitstream: &[u8],
) -> Option<VASliceParameterBufferH264> {
    if raw.len() < SLICE_LONG_SIZE {
        return None;
    }
    let s = Rd(raw);
    let location = s.u32(0) as usize;
    let bytes = s.u32(4) as usize;
    if location.checked_add(bytes)? > bitstream.len() {
        return None;
    }
    let sc = start_code_len(bitstream, location);
    let slice_type = s.u8(16) % 5;
    let refs = &pp.reference_frames;
    let list = |l: usize| {
        let mut out = [VAPictureH264::INVALID; 32];
        for (i, o) in out.iter_mut().enumerate() {
            let e = s.u8(24 + l * 32 + i);
            if e == 0xff {
                continue;
            }
            if let Some(r) = refs.get((e & 0x7f) as usize) {
                if r.flags & VA_PICTURE_H264_INVALID == 0 {
                    *o = *r;
                    if pp.pic_fields & (1 << 5) != 0 {
                        o.flags = (o.flags
                            & !(VA_PICTURE_H264_TOP_FIELD | VA_PICTURE_H264_BOTTOM_FIELD))
                            | if e & 0x80 != 0 {
                                VA_PICTURE_H264_BOTTOM_FIELD
                            } else {
                                VA_PICTURE_H264_TOP_FIELD
                            };
                    }
                }
            }
        }
        out
    };
    // Weights[liste][réf][Y, Cb, Cr][poids, décalage] à partir de 88.
    let w =
        |l: usize, r: usize, c: usize, k: usize| s.i16(88 + (((l * 32 + r) * 3 + c) * 2 + k) * 2);
    let weighted_p = pp.pic_fields & 0b10 != 0 && matches!(slice_type, 0 | 3);
    let weighted_b = (pp.pic_fields >> 2) & 3 == 1 && slice_type == 1;
    let explicit = weighted_p || weighted_b;
    let mut v = VASliceParameterBufferH264 {
        slice_data_size: (bytes - sc) as u32,
        slice_data_offset: (location + sc) as u32,
        slice_data_flag: 0, // VA_SLICE_DATA_FLAG_ALL
        slice_data_bit_offset: s.u16(14).wrapping_add(8),
        first_mb_in_slice: s.u16(10),
        slice_type,
        direct_spatial_mv_pred_flag: s.u8(859),
        num_ref_idx_l0_active_minus1: s.u8(19),
        num_ref_idx_l1_active_minus1: s.u8(20),
        cabac_init_idc: s.u8(860),
        slice_qp_delta: s.i8(857),
        disable_deblocking_filter_idc: s.u8(861),
        slice_alpha_c0_offset_div2: s.i8(21),
        slice_beta_offset_div2: s.i8(22),
        ref_pic_list0: list(0),
        ref_pic_list1: list(1),
        luma_log2_weight_denom: s.u8(17),
        chroma_log2_weight_denom: s.u8(18),
        luma_weight_l0_flag: explicit as u8,
        luma_weight_l0: [0; 32],
        luma_offset_l0: [0; 32],
        chroma_weight_l0_flag: explicit as u8,
        chroma_weight_l0: [[0; 2]; 32],
        chroma_offset_l0: [[0; 2]; 32],
        luma_weight_l1_flag: weighted_b as u8,
        luma_weight_l1: [0; 32],
        luma_offset_l1: [0; 32],
        chroma_weight_l1_flag: weighted_b as u8,
        chroma_weight_l1: [[0; 2]; 32],
        chroma_offset_l1: [[0; 2]; 32],
        reserved: [0; 4],
    };
    if explicit {
        for r in 0..32 {
            v.luma_weight_l0[r] = w(0, r, 0, 0);
            v.luma_offset_l0[r] = w(0, r, 0, 1);
            v.luma_weight_l1[r] = w(1, r, 0, 0);
            v.luma_offset_l1[r] = w(1, r, 0, 1);
            for c in 0..2 {
                v.chroma_weight_l0[r][c] = w(0, r, c + 1, 0);
                v.chroma_offset_l0[r][c] = w(0, r, c + 1, 1);
                v.chroma_weight_l1[r][c] = w(1, r, c + 1, 0);
                v.chroma_offset_l1[r][c] = w(1, r, c + 1, 1);
            }
        }
    }
    Some(v)
}

// ── IDirectXVideoDecoder ───────────────────────────────────────────────────

const BUF_PICTURE: u32 = 0;
const BUF_QMATRIX: u32 = 4;
const BUF_SLICE: u32 = 5;
const BUF_BITSTREAM: u32 = 6;
const BUF_KINDS: usize = 8;

struct Decoder {
    guid: Guid,
    width: u32,
    height: u32,
    targets: Vec<*mut c_void>,
    device: *mut c_void,
    session: VaSession,
    config: u32,
    context: u32,
    surfaces: Vec<u32>,
    buffers: Vec<Vec<u8>>,
    current: Option<usize>,
    failed: bool,
}

unsafe impl Send for Decoder {}

fn va_ok(st: i32, what: &str) -> bool {
    if st != va::VA_STATUS_SUCCESS {
        log_warn!("dxva2", "VA-API : échec de {} (code {})", what, st);
    }
    st == va::VA_STATUS_SUCCESS
}

impl Decoder {
    unsafe fn create(
        g: Guid,
        w: u32,
        h: u32,
        targets: Vec<*mut c_void>,
        device: *mut c_void,
    ) -> Option<Decoder> {
        let l = va::lib()?;
        let session = VaSession::open()?;
        let profiles = session.decode_profiles();
        let profile = [
            va::VA_PROFILE_H264_HIGH,
            va::VA_PROFILE_H264_MAIN,
            va::VA_PROFILE_H264_CONSTRAINED_BASELINE,
        ]
        .into_iter()
        .find(|p| profiles.contains(p))?;
        // VAConfigAttrib { type = VAConfigAttribRTFormat (0), value }
        let mut attrib = [0u32, va::VA_RT_FORMAT_YUV420];
        let mut config = 0;
        if !va_ok(
            (l.va.create_config)(
                session.display,
                profile,
                va::VA_ENTRYPOINT_VLD,
                attrib.as_mut_ptr().cast(),
                1,
                &mut config,
            ),
            "vaCreateConfig",
        ) {
            return None;
        }
        let mut surfaces = vec![0u32; targets.len()];
        if !va_ok(
            (l.va.create_surfaces)(
                session.display,
                va::VA_RT_FORMAT_YUV420,
                w,
                h,
                surfaces.as_mut_ptr(),
                surfaces.len() as u32,
                std::ptr::null_mut(),
                0,
            ),
            "vaCreateSurfaces",
        ) {
            (l.va.destroy_config)(session.display, config);
            return None;
        }
        let mut context = 0;
        if !va_ok(
            (l.va.create_context)(
                session.display,
                config,
                w as i32,
                h as i32,
                va::VA_PROGRESSIVE,
                surfaces.as_mut_ptr(),
                surfaces.len() as i32,
                &mut context,
            ),
            "vaCreateContext",
        ) {
            (l.va.destroy_surfaces)(
                session.display,
                surfaces.as_mut_ptr(),
                surfaces.len() as i32,
            );
            (l.va.destroy_config)(session.display, config);
            return None;
        }
        for &t in &targets {
            addref(t);
        }
        addref(device);
        log_info!(
            "dxva2",
            "décodeur H.264 matériel {}x{} ({} surfaces) via {}",
            w,
            h,
            targets.len(),
            session.vendor
        );
        let mut buffers = vec![Vec::new(); BUF_KINDS];
        for (i, b) in buffers.iter_mut().enumerate() {
            *b = vec![
                0u8;
                if i == BUF_BITSTREAM as usize {
                    4 << 20
                } else {
                    256 << 10
                }
            ];
        }
        Some(Decoder {
            guid: g,
            width: w,
            height: h,
            targets,
            device,
            session,
            config,
            context,
            surfaces,
            buffers,
            current: None,
            failed: false,
        })
    }

    unsafe fn destroy(&mut self) {
        if let Some(l) = va::lib() {
            let d = self.session.display;
            (l.va.destroy_context)(d, self.context);
            (l.va.destroy_surfaces)(d, self.surfaces.as_mut_ptr(), self.surfaces.len() as i32);
            (l.va.destroy_config)(d, self.config);
        }
        for t in std::mem::take(&mut self.targets) {
            release(t);
        }
        release(std::mem::replace(&mut self.device, std::ptr::null_mut()));
    }

    unsafe fn buffer(&self, l: &va::VaLib, ty: i32, data: &[u8], count: u32) -> Option<VABufferID> {
        let mut id = 0;
        let size = (data.len() / count.max(1) as usize) as u32;
        va_ok(
            (l.va.create_buffer)(
                self.session.display,
                self.context,
                ty,
                size,
                count,
                data.as_ptr() as *mut c_void,
                &mut id,
            ),
            "vaCreateBuffer",
        )
        .then_some(id)
    }

    /// Soumet l'image décrite par les tampons DXVA reçus.
    unsafe fn submit(&mut self, descs: &[(u32, usize, usize)]) -> bool {
        let Some(l) = va::lib() else { return false };
        let take = |ty: u32| -> Option<&[u8]> {
            let &(_, off, size) = descs.iter().find(|d| d.0 == ty)?;
            self.buffers
                .get(ty as usize)?
                .get(off..off.checked_add(size)?)
        };
        let Some(pp) = take(BUF_PICTURE).and_then(|b| convert_pic_params(b, &self.surfaces)) else {
            return false;
        };
        let Some(bits) = take(BUF_BITSTREAM) else {
            return false;
        };
        let Some(slices_raw) = take(BUF_SLICE) else {
            return false;
        };
        let slices: Vec<VASliceParameterBufferH264> = slices_raw
            .as_chunks::<SLICE_LONG_SIZE>()
            .0
            .iter()
            .filter_map(|c| convert_slice(c.as_slice(), &pp, bits))
            .collect();
        if slices.is_empty() {
            return false;
        }
        let qm = take(BUF_QMATRIX).and_then(convert_qmatrix);
        let as_bytes = |p: *const u8, n: usize| std::slice::from_raw_parts(p, n);
        let mut ids = Vec::new();
        let pp_bytes = as_bytes(&pp as *const _ as *const u8, std::mem::size_of_val(&pp));
        ids.extend(self.buffer(l, va::VA_PICTURE_PARAMETER_BUFFER, pp_bytes, 1));
        if let Some(q) = qm.as_ref() {
            ids.extend(self.buffer(
                l,
                va::VA_IQ_MATRIX_BUFFER,
                as_bytes(q as *const _ as *const u8, std::mem::size_of_val(q)),
                1,
            ));
        }
        let sl_bytes = as_bytes(
            slices.as_ptr() as *const u8,
            std::mem::size_of_val(&slices[..]),
        );
        ids.extend(self.buffer(
            l,
            va::VA_SLICE_PARAMETER_BUFFER,
            sl_bytes,
            slices.len() as u32,
        ));
        ids.extend(self.buffer(l, va::VA_SLICE_DATA_BUFFER, bits, 1));
        let expected = 3 + qm.is_some() as usize;
        let ok = ids.len() == expected
            && va_ok(
                (l.va.render_picture)(
                    self.session.display,
                    self.context,
                    ids.as_mut_ptr(),
                    ids.len() as i32,
                ),
                "vaRenderPicture",
            );
        for id in ids {
            (l.va.destroy_buffer)(self.session.display, id);
        }
        ok
    }

    /// Recopie la surface VA décodée (NV12) dans la surface Direct3D 9.
    unsafe fn read_back(&self, idx: usize) -> bool {
        let Some(l) = va::lib() else { return false };
        let d = self.session.display;
        let surf = self.surfaces[idx];
        if !va_ok((l.va.sync_surface)(d, surf), "vaSyncSurface") {
            return false;
        }
        let mut img = VAImage::default();
        if !va_ok((l.va.derive_image)(d, surf, &mut img), "vaDeriveImage") {
            return false;
        }
        let mut p: *mut c_void = std::ptr::null_mut();
        let mapped = va_ok((l.va.map_buffer)(d, img.buf, &mut p), "vaMapBuffer") && !p.is_null();
        let mut ok = false;
        if mapped && img.num_planes >= 2 {
            let src = std::slice::from_raw_parts(p as *const u8, img.data_size as usize);
            ok = d3d9::with_surface_bytes(self.targets[idx].cast(), |w, h, pitch, dst| {
                let (w, h, pitch) = (w as usize, h as usize, pitch as usize);
                for plane in 0..2 {
                    let rows = if plane == 0 { h } else { h / 2 };
                    let (so, sp) = (img.offsets[plane] as usize, img.pitches[plane] as usize);
                    let doff = if plane == 0 { 0 } else { pitch * h };
                    for y in 0..rows {
                        let (Some(s), Some(t)) = (
                            src.get(so + y * sp..so + y * sp + w),
                            dst.get_mut(doff + y * pitch..doff + y * pitch + w),
                        ) else {
                            return false;
                        };
                        t.copy_from_slice(s);
                    }
                }
                true
            })
            .unwrap_or(false);
        }
        if mapped {
            (l.va.unmap_buffer)(d, img.buf);
        }
        (l.va.destroy_image)(d, img.image_id);
        ok
    }
}

unsafe fn decoder_of<'a>(this: *mut Obj) -> Option<MutexGuard<'a, Decoder>> {
    match &(*this).kind {
        Kind::Decoder(m) => Some(lock(&*(m.as_ref() as *const Mutex<Decoder>))),
        _ => None,
    }
}

unsafe extern "win64" fn dec_service(this: *mut Obj, out: *mut *mut c_void) -> i32 {
    let Some(d) = decoder_of(this) else {
        return E_INVALIDARG;
    };
    let dev = d.device;
    drop(d);
    DXVA2CreateVideoService(dev, &guid(0xfc51_a551, 0, 0, [0; 8]), out)
}

unsafe extern "win64" fn dec_params(
    this: *mut Obj,
    g: *mut Guid,
    desc: *mut c_void,
    cfg: *mut c_void,
    targets: *mut *mut *mut c_void,
    count: *mut u32,
) -> i32 {
    let Some(d) = decoder_of(this) else {
        return E_INVALIDARG;
    };
    if !g.is_null() {
        *g = d.guid;
    }
    let _ = (desc, cfg);
    if !targets.is_null() && !count.is_null() {
        for &t in &d.targets {
            addref(t);
        }
        *targets = co_alloc(&d.targets);
        *count = d.targets.len() as u32;
    }
    S_OK
}

unsafe extern "win64" fn dec_get_buffer(
    this: *mut Obj,
    ty: u32,
    out: *mut *mut u8,
    size: *mut u32,
) -> i32 {
    let Some(mut d) = decoder_of(this) else {
        return E_INVALIDARG;
    };
    let Some(b) = d.buffers.get_mut(ty as usize) else {
        return E_INVALIDARG;
    };
    if out.is_null() || size.is_null() {
        return E_INVALIDARG;
    }
    *out = b.as_mut_ptr();
    *size = b.len() as u32;
    S_OK
}

unsafe extern "win64" fn dec_begin(
    this: *mut Obj,
    target: *mut c_void,
    _data: *const c_void,
) -> i32 {
    let Some(mut d) = decoder_of(this) else {
        return E_INVALIDARG;
    };
    let Some(idx) = d.targets.iter().position(|&t| t == target) else {
        return E_INVALIDARG;
    };
    let Some(l) = va::lib() else { return E_FAIL };
    if !va_ok(
        (l.va.begin_picture)(d.session.display, d.context, d.surfaces[idx]),
        "vaBeginPicture",
    ) {
        return E_FAIL;
    }
    d.current = Some(idx);
    d.failed = false;
    S_OK
}

/// DXVA2_DecodeBufferDesc (48 octets) : type, index, décalage, taille…
#[repr(C)]
struct BufferDesc {
    ty: u32,
    index: u32,
    offset: u32,
    size: u32,
    rest: [u32; 6],
    pvp: *mut c_void,
}

unsafe extern "win64" fn dec_execute(this: *mut Obj, params: *const u8) -> i32 {
    let Some(mut d) = decoder_of(this) else {
        return E_INVALIDARG;
    };
    if params.is_null() || d.current.is_none() {
        return E_INVALIDARG;
    }
    let n = std::ptr::read_unaligned(params as *const u32);
    let list = std::ptr::read_unaligned(params.add(8) as *const *const BufferDesc);
    if list.is_null() || n > 32 {
        return E_INVALIDARG;
    }
    let descs: Vec<(u32, usize, usize)> = (0..n as usize)
        .map(|i| {
            let b = &*list.add(i);
            (b.ty, b.offset as usize, b.size as usize)
        })
        .collect();
    if !d.submit(&descs) {
        d.failed = true;
        return E_FAIL;
    }
    S_OK
}

unsafe extern "win64" fn dec_end(this: *mut Obj, _handle: *mut usize) -> i32 {
    let Some(mut d) = decoder_of(this) else {
        return E_INVALIDARG;
    };
    let Some(idx) = d.current.take() else {
        return E_INVALIDARG;
    };
    let Some(l) = va::lib() else { return E_FAIL };
    if !va_ok(
        (l.va.end_picture)(d.session.display, d.context),
        "vaEndPicture",
    ) || d.failed
    {
        return E_FAIL;
    }
    if d.read_back(idx) {
        S_OK
    } else {
        E_FAIL
    }
}

// ── Exports ────────────────────────────────────────────────────────────────

static RESET_TOKEN: AtomicU32 = AtomicU32::new(0x7a11);

pub unsafe extern "win64" fn DXVA2CreateDirect3DDeviceManager9(
    token: *mut u32,
    out: *mut *mut c_void,
) -> i32 {
    if token.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let t = RESET_TOKEN.fetch_add(1, Ordering::Relaxed);
    *token = t;
    let m = Manager {
        device: std::ptr::null_mut(),
        token: t,
    };
    *out = new_obj(manager_vtable(), Kind::Manager(Mutex::new(m))).cast();
    S_OK
}

pub unsafe extern "win64" fn DXVA2CreateVideoService(
    device: *mut c_void,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    if !d3d9::is_device(device) {
        return E_INVALIDARG;
    }
    if iid.is_null() || !matches!((*iid).data1, 0xfc51_a54f | 0xfc51_a551) {
        // Le service de traitement vidéo (fc51a551/2) n'est pas fourni.
        return E_NOINTERFACE;
    }
    addref(device);
    hand_out(new_obj(service_vtable(), Kind::Service(device)), iid, out)
}

// Configuration d'écran (DDC/CI) : non disponible.
pub unsafe extern "win64" fn GetNumberOfPhysicalMonitorsFromHMONITOR(
    _m: usize,
    n: *mut u32,
) -> i32 {
    if !n.is_null() {
        *n = 0;
    }
    0
}
pub unsafe extern "win64" fn GetPhysicalMonitorsFromHMONITOR(
    _m: usize,
    _n: u32,
    _out: *mut c_void,
) -> i32 {
    0
}
pub unsafe extern "win64" fn DestroyPhysicalMonitors(_n: u32, _p: *mut c_void) -> i32 {
    1
}
pub unsafe extern "win64" fn GetMonitorBrightness(
    _h: usize,
    _a: *mut u32,
    _b: *mut u32,
    _c: *mut u32,
) -> i32 {
    0
}
pub unsafe extern "win64" fn SetMonitorBrightness(_h: usize, _v: u32) -> i32 {
    0
}
pub unsafe extern "win64" fn GetMonitorCapabilities(_h: usize, _a: *mut u32, _b: *mut u32) -> i32 {
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "DXVA2CreateDirect3DDeviceManager9" => fn DXVA2CreateDirect3DDeviceManager9,
        "DXVA2CreateVideoService" => fn DXVA2CreateVideoService,
        "GetNumberOfPhysicalMonitorsFromHMONITOR" => fn GetNumberOfPhysicalMonitorsFromHMONITOR,
        "GetPhysicalMonitorsFromHMONITOR" => fn GetPhysicalMonitorsFromHMONITOR,
        "DestroyPhysicalMonitors" => fn DestroyPhysicalMonitors,
        "GetMonitorBrightness" => fn GetMonitorBrightness,
        "SetMonitorBrightness" => fn SetMonitorBrightness,
        "GetMonitorCapabilities" => fn GetMonitorCapabilities,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn dispositions_va_api() {
        assert_eq!(size_of::<VAPictureH264>(), 36);
        assert_eq!(size_of::<VAPictureParameterBufferH264>(), 672);
        assert_eq!(size_of::<VAIQMatrixBufferH264>(), 240);
        assert_eq!(size_of::<VASliceParameterBufferH264>(), 3128);
        assert_eq!(size_of::<ConfigPictureDecode>(), 100);
        assert_eq!(size_of::<BufferDesc>(), 48);
    }

    fn pic_params() -> Vec<u8> {
        let mut pp = vec![0u8; 1040];
        pp[0..2].copy_from_slice(&79u16.to_le_bytes()); // 1280 px
        pp[2..4].copy_from_slice(&44u16.to_le_bytes()); // 720 px
        pp[4] = 2; // image courante : surface 2
        pp[5] = 1;
        // chroma 4:2:0, RefPicFlag, frame_mbs_only, transform_8x8
        let bits: u16 = (1 << 4) | (1 << 6) | (1 << 12) | (1 << 13);
        pp[6..8].copy_from_slice(&bits.to_le_bytes());
        pp[16..32].fill(0xff);
        pp[16] = 0; // référence 0 → surface 0, court terme
        pp[32..36].copy_from_slice(&4i32.to_le_bytes());
        pp[40..44].copy_from_slice(&2i32.to_le_bytes());
        pp[176..178].copy_from_slice(&7u16.to_le_bytes());
        pp[208..212].copy_from_slice(&3u32.to_le_bytes()); // deux champs utilisés
        pp[214..216].copy_from_slice(&8u16.to_le_bytes());
        pp[216] = 2;
        pp[221] = 1; // CABAC
        pp
    }

    #[test]
    fn parametres_image_h264() {
        let surfaces = [10, 11, 12, 13];
        let p = convert_pic_params(&pic_params(), &surfaces).unwrap();
        assert_eq!(p.curr_pic.picture_id, 12);
        assert_eq!(p.curr_pic.frame_idx, 8);
        assert_eq!(p.curr_pic.top_poc, 4);
        assert_eq!((p.width_in_mbs_minus1, p.height_in_mbs_minus1), (79, 44));
        assert_eq!(p.reference_frames[0].picture_id, 10);
        assert_eq!(
            p.reference_frames[0].flags,
            VA_PICTURE_H264_SHORT_TERM_REFERENCE
        );
        assert_eq!(p.reference_frames[0].frame_idx, 7);
        assert_eq!(p.reference_frames[1].flags, VA_PICTURE_H264_INVALID);
        assert_eq!(p.seq_fields & 3, 1);
        assert_eq!((p.seq_fields >> 4) & 1, 1);
        assert_eq!((p.seq_fields >> 8) & 0xf, 2);
        assert_eq!(p.pic_fields & 1, 1);
        assert_eq!((p.pic_fields >> 4) & 1, 1);
        assert_eq!((p.pic_fields >> 10) & 1, 1);
        // Surface hors limites : refus propre.
        assert!(convert_pic_params(&pic_params(), &surfaces[..2]).is_none());
        assert!(convert_pic_params(&[0; 10], &surfaces).is_none());
    }

    #[test]
    fn tranche_longue_h264() {
        let surfaces = [10, 11, 12];
        let pp = convert_pic_params(&pic_params(), &surfaces).unwrap();
        let mut bits = vec![0u8; 64];
        bits[0..4].copy_from_slice(&[0, 0, 1, 0x65]);
        let mut s = vec![0u8; SLICE_LONG_SIZE];
        s[4..8].copy_from_slice(&20u32.to_le_bytes());
        s[10..12].copy_from_slice(&5u16.to_le_bytes());
        s[14..16].copy_from_slice(&13u16.to_le_bytes());
        s[16] = 7; // I (7 % 5 = 2)
        s[24..88].fill(0xff);
        s[24] = 0;
        s[857] = (-3i8) as u8;
        let v = convert_slice(&s, &pp, &bits).unwrap();
        assert_eq!((v.slice_data_offset, v.slice_data_size), (3, 17));
        assert_eq!(v.slice_data_bit_offset, 21);
        assert_eq!(v.slice_type, 2);
        assert_eq!(v.first_mb_in_slice, 5);
        assert_eq!(v.slice_qp_delta, -3);
        assert_eq!(v.ref_pic_list0[0].picture_id, 10);
        assert_eq!(v.ref_pic_list0[1].flags, VA_PICTURE_H264_INVALID);
        assert_eq!(v.luma_weight_l0_flag, 0);
        // Tranche qui déborde du flux : refusée.
        s[4..8].copy_from_slice(&100u32.to_le_bytes());
        assert!(convert_slice(&s, &pp, &bits).is_none());
    }

    #[test]
    fn matrices_zigzag() {
        let mut raw = vec![0u8; QM_SIZE];
        for (i, b) in raw.iter_mut().enumerate() {
            *b = (i % 64) as u8;
        }
        let q = convert_qmatrix(&raw).unwrap();
        assert_eq!(q.list4x4[0][0], 0);
        assert_eq!(q.list4x4[0][4], 2); // 3e coefficient du zigzag → (1,0)
        assert_eq!(q.list8x8[0][8], (96 + 2) % 64);
        assert_eq!(q.list8x8[0][63], (96 + 63) % 64);
    }

    #[test]
    fn service_sans_materiel() {
        std::env::set_var("WINLUX_NO_VAAPI", "1");
        unsafe {
            let dev = crate::dlls::d3d9::test_device();
            let mut token = 0;
            let mut mgr: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                DXVA2CreateDirect3DDeviceManager9(&mut token, &mut mgr),
                S_OK
            );
            let mgr = mgr as *mut Obj;
            assert_eq!(mgr_reset(mgr, dev, token + 1), E_INVALIDARG);
            assert_eq!(mgr_reset(mgr, dev, token), S_OK);
            let mut h = 0;
            assert_eq!(mgr_open(mgr, &mut h), S_OK);
            let mut svc: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                mgr_service(mgr, h, &guid(0xfc51_a551, 0, 0, [0; 8]), &mut svc),
                S_OK
            );
            let svc = svc as *mut Obj;
            let mut n = 99;
            let mut list: *mut Guid = std::ptr::null_mut();
            assert_eq!(svc_guids(svc, &mut n, &mut list), S_OK);
            // Sans VA-API (ou sur une machine qui ne l'a pas) : aucun décodeur
            // annoncé, l'application décode en logiciel.
            if gpu_caps::caps().vaapi.is_none() {
                assert_eq!(n, 0);
            }
            crate::dlls::ole32::co_task_mem_free(list.cast());
            let mut surfs = [std::ptr::null_mut(); 3];
            assert_eq!(
                svc_create_surface(
                    svc,
                    64,
                    32,
                    2,
                    d3d9::D3DFMT_NV12,
                    0,
                    0,
                    0,
                    surfs.as_mut_ptr(),
                    std::ptr::null_mut()
                ),
                S_OK
            );
            let ok = d3d9::with_surface_bytes(surfs[0].cast(), |w, h, pitch, data| {
                (w, h, data.len() >= (pitch * h * 3 / 2) as usize)
            });
            assert_eq!(ok, Some((64, 32, true)));
            let desc = [64u32, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            let cfg = ConfigPictureDecode {
                bitstream_encryption: NO_ENCRYPT,
                mb_control_encryption: NO_ENCRYPT,
                resid_diff_encryption: NO_ENCRYPT,
                bitstream_raw: 1,
                others: [0; 11],
                min_render_targets: 0,
                decoder_specific: 0,
            };
            let mut dec = std::ptr::null_mut();
            let hr = svc_create_decoder(
                svc,
                &MODE_H264_E,
                desc.as_ptr(),
                &cfg,
                surfs.as_ptr(),
                3,
                &mut dec,
            );
            assert!(hr == E_INVALIDARG || hr == E_FAIL || hr == S_OK);
            if hr == S_OK {
                obj_release(dec as *mut Obj);
            }
            for s in surfs {
                release(s);
            }
            // Un « device » qui n'est pas Direct3D 9 est refusé.
            let mut bad = std::ptr::null_mut();
            let fake = [0usize; 4];
            assert_eq!(
                DXVA2CreateVideoService(
                    fake.as_ptr() as *mut c_void,
                    &guid(0xfc51_a551, 0, 0, [0; 8]),
                    &mut bad
                ),
                E_INVALIDARG
            );
            obj_release(svc);
            obj_release(mgr);
            release(dev);
        }
    }
}
