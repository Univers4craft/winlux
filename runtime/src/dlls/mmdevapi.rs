//! WASAPI (mmdevapi.dll / audioses.dll) écrit de zéro : énumération des
//! périphériques (IMMDeviceEnumerator, IMMDevice, IPropertyStore), flux
//! IAudioClient(2/3) en mode partagé ou exclusif, IAudioRenderClient,
//! IAudioCaptureClient (micro et « loopback »), horloge, volumes du flux et
//! de la session, IAudioEndpointVolume. Inclut aussi avrt.dll, que les
//! programmes WASAPI appellent pour leurs threads audio.

use super::audio_com::*;
use super::ole32::Guid;
use crate::audio::{self, dsp, format::Format, CaptureConverter, CaptureQueue};
use crate::dlls::lock;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const AUDCLNT_E_NOT_INITIALIZED: i32 = 0x8889_0001u32 as i32;
const AUDCLNT_E_ALREADY_INITIALIZED: i32 = 0x8889_0002u32 as i32;
const AUDCLNT_E_WRONG_ENDPOINT_TYPE: i32 = 0x8889_0003u32 as i32;
const AUDCLNT_E_NOT_STOPPED: i32 = 0x8889_0005u32 as i32;
const AUDCLNT_E_BUFFER_TOO_LARGE: i32 = 0x8889_0006u32 as i32;
const AUDCLNT_E_OUT_OF_ORDER: i32 = 0x8889_0007u32 as i32;
const AUDCLNT_E_UNSUPPORTED_FORMAT: i32 = 0x8889_0008u32 as i32;
const AUDCLNT_E_INVALID_SIZE: i32 = 0x8889_0009u32 as i32;
const AUDCLNT_E_DEVICE_IN_USE: i32 = 0x8889_000Au32 as i32;
const AUDCLNT_E_BUFFER_OPERATION_PENDING: i32 = 0x8889_000Bu32 as i32;
const AUDCLNT_E_EVENTHANDLE_NOT_EXPECTED: i32 = 0x8889_0011u32 as i32;
const AUDCLNT_E_EVENTHANDLE_NOT_SET: i32 = 0x8889_0014u32 as i32;
const AUDCLNT_E_INVALID_DEVICE_PERIOD: i32 = 0x8889_0020u32 as i32;
const AUDCLNT_S_BUFFER_EMPTY: i32 = 0x0889_0001;
const E_NOTFOUND: i32 = 0x8007_0490u32 as i32;
const STG_E_ACCESSDENIED: i32 = 0x8003_0005u32 as i32;

const AUDCLNT_SHAREMODE_EXCLUSIVE: u32 = 1;
const AUDCLNT_STREAMFLAGS_LOOPBACK: u32 = 0x0002_0000;
const AUDCLNT_STREAMFLAGS_EVENTCALLBACK: u32 = 0x0004_0000;
const AUDCLNT_BUFFERFLAGS_SILENT: u32 = 0x2;

/// Période du moteur partagé : 10 ms en unités de 100 ns.
const DEFAULT_PERIOD: i64 = 100_000;
const MIN_PERIOD: i64 = 30_000;

const CLSID_MMDEVICEENUMERATOR: Guid = guid(
    0xbcde0395,
    0xe52f,
    0x467c,
    [0x8e, 0x3d, 0xc4, 0x57, 0x92, 0x91, 0x69, 0x2e],
);
const IID_IMMDEVICEENUMERATOR: Guid = guid(
    0xa95664d2,
    0x9614,
    0x4f35,
    [0xa7, 0x46, 0xde, 0x8d, 0xb6, 0x36, 0x17, 0xe6],
);
const IID_IMMDEVICE: Guid = guid(
    0xd666063f,
    0x1587,
    0x4e43,
    [0x81, 0xf1, 0xb9, 0x48, 0xe8, 0x07, 0x36, 0x3f],
);
const IID_IMMDEVICECOLLECTION: Guid = guid(
    0x0bd7a1be,
    0x7a1a,
    0x44db,
    [0x83, 0x97, 0xcc, 0x53, 0x92, 0x38, 0x7b, 0x5e],
);
const IID_IMMENDPOINT: Guid = guid(
    0x1be09788,
    0x6894,
    0x4089,
    [0x85, 0x86, 0x9a, 0x2a, 0x6c, 0x26, 0x5a, 0xc5],
);
const IID_IPROPERTYSTORE: Guid = guid(
    0x886d8eeb,
    0x8cf2,
    0x4446,
    [0x8d, 0x02, 0xcd, 0xba, 0x1d, 0xbd, 0xcf, 0x99],
);
const IID_IAUDIOCLIENT: Guid = guid(
    0x1cb9ad4c,
    0xdbfa,
    0x4c32,
    [0xb1, 0x78, 0xc2, 0xf5, 0x68, 0xa7, 0x03, 0xb2],
);
const IID_IAUDIOCLIENT2: Guid = guid(
    0x726778cd,
    0xf60a,
    0x4eda,
    [0x82, 0xde, 0xe4, 0x76, 0x10, 0xcd, 0x78, 0xaa],
);
const IID_IAUDIOCLIENT3: Guid = guid(
    0x7ed4ee07,
    0x8e67,
    0x4cd4,
    [0x8c, 0x1a, 0x2b, 0x7a, 0x59, 0x87, 0xad, 0x42],
);
const IID_IAUDIORENDERCLIENT: Guid = guid(
    0xf294acfc,
    0x3146,
    0x4483,
    [0xa7, 0xbf, 0xad, 0xdc, 0xa7, 0xc2, 0x60, 0xe2],
);
const IID_IAUDIOCAPTURECLIENT: Guid = guid(
    0xc8adbd64,
    0xe71e,
    0x48a0,
    [0xa4, 0xde, 0x18, 0x5c, 0x39, 0x5c, 0xd3, 0x17],
);
const IID_IAUDIOCLOCK: Guid = guid(
    0xcd63314f,
    0x3fba,
    0x4a1b,
    [0x81, 0x2c, 0xef, 0x96, 0x35, 0x8d, 0x3a, 0xc9],
);
const IID_ISIMPLEAUDIOVOLUME: Guid = guid(
    0x87ce5498,
    0x68d6,
    0x44e5,
    [0x92, 0x15, 0x6d, 0xa4, 0x7e, 0xf8, 0x83, 0xd8],
);
const IID_IAUDIOSTREAMVOLUME: Guid = guid(
    0x93014887,
    0x242d,
    0x4068,
    [0x8a, 0x15, 0xcf, 0x5e, 0x93, 0xb9, 0x0f, 0xe3],
);
const IID_IAUDIOSESSIONCONTROL: Guid = guid(
    0xf4b1a599,
    0x7266,
    0x4319,
    [0xa8, 0xca, 0xe7, 0x0a, 0xcb, 0x11, 0xe8, 0xcd],
);
const IID_IAUDIOSESSIONCONTROL2: Guid = guid(
    0xbfb7ff88,
    0x7239,
    0x4fc9,
    [0x8f, 0xa2, 0x07, 0xc9, 0x50, 0xbe, 0x9c, 0x6d],
);
const IID_IAUDIOENDPOINTVOLUME: Guid = guid(
    0x5cdf2c82,
    0x841e,
    0x4546,
    [0x97, 0x22, 0x0c, 0xf7, 0x40, 0x78, 0x22, 0x9a],
);

const PKEY_DEVICE: Guid = guid(
    0xa45c254e,
    0xdf1c,
    0x4efd,
    [0x80, 0x20, 0x67, 0xd1, 0x46, 0xa8, 0x50, 0xe0],
);
const PKEY_INTERFACE: Guid = guid(
    0x026e516e,
    0xb814,
    0x414b,
    [0x83, 0xcd, 0x85, 0x6d, 0x6f, 0xef, 0x48, 0x22],
);
const PKEY_ENGINE_FORMAT: Guid = guid(
    0xf19f064d,
    0x082c,
    0x4e27,
    [0xbc, 0x73, 0x68, 0x82, 0xa1, 0xbb, 0x8e, 0x4c],
);
const PKEY_ENDPOINT: Guid = guid(
    0x1da5d803,
    0xd492,
    0x4edd,
    [0x8c, 0x23, 0xe0, 0xc0, 0xff, 0xee, 0x7f, 0x0e],
);

const RENDER_ID: &str = "{0.0.0.00000000}.{574e4c58-4155-4449-4f00-000000000001}";
const CAPTURE_ID: &str = "{0.0.1.00000000}.{574e4c58-4155-4449-4f00-000000000002}";

fn mix_format() -> Format {
    Format::mix(audio::RATE)
}

/// Copie une chaîne dans un bloc CoTaskMemAlloc (UTF-16, terminée par 0).
fn alloc_wide(s: &str) -> *mut u16 {
    let w: Vec<u16> = s.encode_utf16().chain([0]).collect();
    unsafe {
        let p = super::ole32::co_task_mem_alloc(w.len() * 2) as *mut u16;
        if !p.is_null() {
            std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
        }
        p
    }
}

unsafe fn qi_fail(out: *mut *mut c_void) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOINTERFACE
}

// ---------------------------------------------------------------------------
// IMMDeviceEnumerator
// ---------------------------------------------------------------------------

struct Enumerator;

unsafe extern "win64" fn en_qi(
    this: *mut Com<Enumerator>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    match read_guid(riid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IMMDEVICEENUMERATOR => {
            Com::add_ref(this);
            put(out, this)
        }
        _ => qi_fail(out),
    }
}
unsafe extern "win64" fn en_add_ref(this: *mut Com<Enumerator>) -> u32 {
    Com::add_ref(this)
}
unsafe extern "win64" fn en_release(this: *mut Com<Enumerator>) -> u32 {
    Com::release(this)
}
unsafe extern "win64" fn en_enum(
    _this: *mut Com<Enumerator>,
    flow: u32,
    mask: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if flow > 2 || mask == 0 || mask > 0xf {
        return E_INVALIDARG;
    }
    let mut flows = Vec::new();
    if mask & 1 != 0 {
        if flow == 0 || flow == 2 {
            flows.push(0);
        }
        if flow == 1 || flow == 2 {
            flows.push(1);
        }
    }
    put(out, Com::create(COLL_VT.ptr(), Collection { flows }))
}
unsafe extern "win64" fn en_default(
    _this: *mut Com<Enumerator>,
    flow: u32,
    role: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if flow > 1 || role > 2 {
        return E_INVALIDARG;
    }
    put(out, new_device(flow))
}
unsafe extern "win64" fn en_get_device(
    _this: *mut Com<Enumerator>,
    id: *const u16,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || id.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let s = super::wstr(id);
    if s.eq_ignore_ascii_case(RENDER_ID) {
        put(out, new_device(0))
    } else if s.eq_ignore_ascii_case(CAPTURE_ID) {
        put(out, new_device(1))
    } else {
        E_NOTFOUND
    }
}
unsafe extern "win64" fn en_notify(_this: *mut Com<Enumerator>, cb: *mut c_void) -> i32 {
    if cb.is_null() {
        E_POINTER
    } else {
        S_OK
    }
}
static EN_VT: VTable<8> = VTable([
    en_qi as *const (),
    en_add_ref as *const (),
    en_release as *const (),
    en_enum as *const (),
    en_default as *const (),
    en_get_device as *const (),
    en_notify as *const (),
    en_notify as *const (),
]);

// ---------------------------------------------------------------------------
// IMMDeviceCollection
// ---------------------------------------------------------------------------

struct Collection {
    flows: Vec<u32>,
}

unsafe extern "win64" fn coll_qi(
    this: *mut Com<Collection>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    match read_guid(riid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IMMDEVICECOLLECTION => {
            Com::add_ref(this);
            put(out, this)
        }
        _ => qi_fail(out),
    }
}
unsafe extern "win64" fn coll_add_ref(this: *mut Com<Collection>) -> u32 {
    Com::add_ref(this)
}
unsafe extern "win64" fn coll_release(this: *mut Com<Collection>) -> u32 {
    Com::release(this)
}
unsafe extern "win64" fn coll_count(this: *mut Com<Collection>, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = Com::get(this).flows.len() as u32;
    S_OK
}
unsafe extern "win64" fn coll_item(
    this: *mut Com<Collection>,
    i: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    match Com::get(this).flows.get(i as usize) {
        Some(&f) => put(out, new_device(f)),
        None => E_INVALIDARG,
    }
}
static COLL_VT: VTable<5> = VTable([
    coll_qi as *const (),
    coll_add_ref as *const (),
    coll_release as *const (),
    coll_count as *const (),
    coll_item as *const (),
]);

// ---------------------------------------------------------------------------
// IMMDevice + IMMEndpoint
// ---------------------------------------------------------------------------

struct Device {
    flow: u32,
    endpoint: Iface,
}

fn new_device(flow: u32) -> *mut Com<Device> {
    let d = Com::create(
        DEV_VT.ptr(),
        Device {
            flow,
            endpoint: Iface::new(ENDPOINT_VT.ptr()),
        },
    );
    unsafe { (*d).inner.endpoint.owner = d as *mut c_void };
    d
}

unsafe extern "win64" fn dev_qi(
    this: *mut Com<Device>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let obj = match read_guid(riid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IMMDEVICE => this as *mut c_void,
        Some(g) if g == IID_IMMENDPOINT => &mut (*this).inner.endpoint as *mut Iface as *mut c_void,
        _ => return qi_fail(out),
    };
    Com::add_ref(this);
    *out = obj;
    S_OK
}
unsafe extern "win64" fn dev_add_ref(this: *mut Com<Device>) -> u32 {
    Com::add_ref(this)
}
unsafe extern "win64" fn dev_release(this: *mut Com<Device>) -> u32 {
    Com::release(this)
}
unsafe extern "win64" fn dev_activate(
    this: *mut Com<Device>,
    riid: *const Guid,
    _ctx: u32,
    _params: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let flow = Com::get(this).flow;
    match read_guid(riid) {
        Some(g) if g == IID_IAUDIOCLIENT || g == IID_IAUDIOCLIENT2 || g == IID_IAUDIOCLIENT3 => {
            put(out, new_client(flow))
        }
        Some(g) if g == IID_IAUDIOENDPOINTVOLUME => {
            put(out, Com::create(EPV_VT.ptr(), EndpointVolume))
        }
        Some(g) if g == IID_IUNKNOWN => put(out, new_client(flow)),
        _ => E_NOINTERFACE,
    }
}
unsafe extern "win64" fn dev_props(
    this: *mut Com<Device>,
    _access: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(
        out,
        Com::create(
            PROPS_VT.ptr(),
            Props {
                flow: Com::get(this).flow,
            },
        ),
    )
}
unsafe extern "win64" fn dev_get_id(this: *mut Com<Device>, out: *mut *mut u16) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = alloc_wide(if Com::get(this).flow == 0 {
        RENDER_ID
    } else {
        CAPTURE_ID
    });
    S_OK
}
unsafe extern "win64" fn dev_get_state(_this: *mut Com<Device>, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 1; // DEVICE_STATE_ACTIVE
    S_OK
}
static DEV_VT: VTable<7> = VTable([
    dev_qi as *const (),
    dev_add_ref as *const (),
    dev_release as *const (),
    dev_activate as *const (),
    dev_props as *const (),
    dev_get_id as *const (),
    dev_get_state as *const (),
]);

unsafe fn ep_owner(this: *mut Iface) -> *mut Com<Device> {
    (*this).owner as *mut Com<Device>
}
unsafe extern "win64" fn ep_qi(this: *mut Iface, riid: *const Guid, out: *mut *mut c_void) -> i32 {
    dev_qi(ep_owner(this), riid, out)
}
unsafe extern "win64" fn ep_add_ref(this: *mut Iface) -> u32 {
    Com::add_ref(ep_owner(this))
}
unsafe extern "win64" fn ep_release(this: *mut Iface) -> u32 {
    Com::release(ep_owner(this))
}
unsafe extern "win64" fn ep_flow(this: *mut Iface, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = Com::get(ep_owner(this)).flow;
    S_OK
}
static ENDPOINT_VT: VTable<4> = VTable([
    ep_qi as *const (),
    ep_add_ref as *const (),
    ep_release as *const (),
    ep_flow as *const (),
]);

// ---------------------------------------------------------------------------
// IPropertyStore (lecture seule)
// ---------------------------------------------------------------------------

struct Props {
    flow: u32,
}

const KEYS: [(Guid, u32); 6] = [
    (PKEY_DEVICE, 14),       // PKEY_Device_FriendlyName
    (PKEY_DEVICE, 2),        // PKEY_Device_DeviceDesc
    (PKEY_INTERFACE, 2),     // PKEY_DeviceInterface_FriendlyName
    (PKEY_ENGINE_FORMAT, 0), // PKEY_AudioEngine_DeviceFormat
    (PKEY_ENDPOINT, 0),      // PKEY_AudioEndpoint_FormFactor
    (PKEY_ENDPOINT, 4),      // PKEY_AudioEndpoint_GUID
];

unsafe extern "win64" fn props_qi(
    this: *mut Com<Props>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    match read_guid(riid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IPROPERTYSTORE => {
            Com::add_ref(this);
            put(out, this)
        }
        _ => qi_fail(out),
    }
}
unsafe extern "win64" fn props_add_ref(this: *mut Com<Props>) -> u32 {
    Com::add_ref(this)
}
unsafe extern "win64" fn props_release(this: *mut Com<Props>) -> u32 {
    Com::release(this)
}
unsafe extern "win64" fn props_count(_this: *mut Com<Props>, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = KEYS.len() as u32;
    S_OK
}
unsafe extern "win64" fn props_get_at(_this: *mut Com<Props>, i: u32, key: *mut u8) -> i32 {
    if key.is_null() {
        return E_POINTER;
    }
    let Some((g, pid)) = KEYS.get(i as usize) else {
        return E_INVALIDARG;
    };
    std::ptr::write_unaligned(key as *mut Guid, *g);
    std::ptr::write_unaligned(key.add(16) as *mut u32, *pid);
    S_OK
}
unsafe extern "win64" fn props_get_value(
    this: *mut Com<Props>,
    key: *const u8,
    pv: *mut u8,
) -> i32 {
    if key.is_null() || pv.is_null() {
        return E_POINTER;
    }
    std::ptr::write_bytes(pv, 0, 24);
    let g: Guid = std::ptr::read_unaligned(key as *const Guid);
    let pid = std::ptr::read_unaligned(key.add(16) as *const u32);
    let render = Com::get(this).flow == 0;
    let text = |s: &str| {
        *(pv as *mut u16) = 31; // VT_LPWSTR
        *(pv.add(8) as *mut *mut u16) = alloc_wide(s);
    };
    match (g, pid) {
        (g, 14) if g == PKEY_DEVICE => text(if render {
            "Haut-parleurs (Winlux)"
        } else {
            "Microphone (Winlux)"
        }),
        (g, 2) if g == PKEY_DEVICE => text(if render {
            "Haut-parleurs"
        } else {
            "Microphone"
        }),
        (g, 2) if g == PKEY_INTERFACE => text("Winlux Audio"),
        (g, 0) if g == PKEY_ENGINE_FORMAT => {
            let f = mix_format().to_bytes();
            *(pv as *mut u16) = 65; // VT_BLOB
            *(pv.add(8) as *mut u32) = f.len() as u32;
            *(pv.add(16) as *mut *mut u8) = alloc_format(&mix_format());
        }
        (g, 0) if g == PKEY_ENDPOINT => {
            *(pv as *mut u16) = 19; // VT_UI4
            *(pv.add(8) as *mut u32) = if render { 1 } else { 4 }; // Speakers / Microphone
        }
        (g, 4) if g == PKEY_ENDPOINT => text(if render {
            "{574E4C58-4155-4449-4F00-000000000001}"
        } else {
            "{574E4C58-4155-4449-4F00-000000000002}"
        }),
        _ => {}
    }
    S_OK
}
unsafe extern "win64" fn props_set_value(
    _this: *mut Com<Props>,
    _k: *const u8,
    _v: *const u8,
) -> i32 {
    STG_E_ACCESSDENIED
}
unsafe extern "win64" fn props_commit(_this: *mut Com<Props>) -> i32 {
    STG_E_ACCESSDENIED
}
static PROPS_VT: VTable<8> = VTable([
    props_qi as *const (),
    props_add_ref as *const (),
    props_release as *const (),
    props_count as *const (),
    props_get_at as *const (),
    props_get_value as *const (),
    props_set_value as *const (),
    props_commit as *const (),
]);

// ---------------------------------------------------------------------------
// IAudioClient + services
// ---------------------------------------------------------------------------

static EXCLUSIVE_STREAMS: AtomicU32 = AtomicU32::new(0);

struct Stream {
    format: Format,
    exclusive: bool,
    flags: u32,
    buffer_frames: usize,
    period_frames: usize,
    event: usize,
    started: bool,
    /// Rendu : anneau de `buffer_frames` trames.
    ring: Vec<u8>,
    read: usize,
    filled: usize,
    /// Zone renvoyée par GetBuffer (rendu ou capture).
    staging: Box<[u8]>,
    pending: Option<usize>,
    resampler: dsp::Resampler,
    /// Trames consommées (rendu) ou capturées (capture) depuis Reset.
    position: u64,
    volumes: Vec<f32>,
    // Capture.
    queue: Option<Arc<CaptureQueue>>,
    conv: Option<CaptureConverter>,
    fifo: VecDeque<u8>,
}

struct ClientShared {
    flow: u32,
    st: Mutex<Option<Stream>>,
    session_volume: AtomicU32,
    session_mute: AtomicU32,
    source: AtomicU64,
}

impl audio::Source for ClientShared {
    fn mix(&self, out: &mut [f32]) -> bool {
        let mut event = 0;
        {
            let mut guard = lock(&self.st);
            let Some(s) = guard.as_mut() else {
                return true;
            };
            if !s.started {
                return true;
            }
            if s.flags & AUDCLNT_STREAMFLAGS_EVENTCALLBACK != 0 {
                event = s.event;
            }
            if let Some(q) = s.queue.clone() {
                // Capture : convertit ce qui est arrivé vers le format du client.
                let input = q.take(q.available());
                if let Some(conv) = s.conv.as_mut() {
                    let bytes = conv.convert(&input);
                    s.fifo.extend(bytes);
                }
                let cap = s.buffer_frames * s.format.block as usize;
                if s.fifo.len() > cap {
                    let excess = s.fifo.len() - cap;
                    s.fifo.drain(..excess);
                }
            } else {
                let mute = self.session_mute.load(Ordering::Relaxed) != 0;
                let sv = f32::from_bits(self.session_volume.load(Ordering::Relaxed));
                let v0 = s.volumes.first().copied().unwrap_or(1.0);
                let v1 = s.volumes.get(1).copied().unwrap_or(v0);
                let gain = if mute { [0.0, 0.0] } else { [sv * v0, sv * v1] };
                let step = s.format.rate as f64 / audio::RATE as f64;
                let mut rs = std::mem::take(&mut s.resampler);
                {
                    let block = s.format.block as usize;
                    let frames = s.buffer_frames;
                    let fmt = s.format;
                    let ring = &s.ring;
                    let (read, filled, pos) = (&mut s.read, &mut s.filled, &mut s.position);
                    let mut pull = || {
                        if *filled == 0 {
                            return None;
                        }
                        let o = *read * block;
                        let f = fmt.stereo(&ring[o..o + block]);
                        *read = (*read + 1) % frames;
                        *filled -= 1;
                        *pos += 1;
                        Some(f)
                    };
                    rs.mix_into(step, out, gain, &mut pull);
                }
                s.resampler = rs;
            }
        }
        signal(event);
        true
    }
}

struct Client {
    sh: Arc<ClientShared>,
    render: Iface,
    capture: Iface,
    clock: Iface,
    simple: Iface,
    stream_vol: Iface,
    session: Iface,
}

impl Drop for Client {
    fn drop(&mut self) {
        audio::remove_source(self.sh.source.load(Ordering::Relaxed));
        if let Some(s) = lock(&self.sh.st).as_ref() {
            if s.exclusive {
                EXCLUSIVE_STREAMS.fetch_sub(1, Ordering::AcqRel);
            }
        }
    }
}

fn new_client(flow: u32) -> *mut Com<Client> {
    let c = Com::create(
        AC_VT.ptr(),
        Client {
            sh: Arc::new(ClientShared {
                flow,
                st: Mutex::new(None),
                session_volume: AtomicU32::new(1.0f32.to_bits()),
                session_mute: AtomicU32::new(0),
                source: AtomicU64::new(0),
            }),
            render: Iface::new(RENDER_VT.ptr()),
            capture: Iface::new(CAPTURE_VT.ptr()),
            clock: Iface::new(CLOCK_VT.ptr()),
            simple: Iface::new(SIMPLE_VT.ptr()),
            stream_vol: Iface::new(STREAMVOL_VT.ptr()),
            session: Iface::new(SESSION_VT.ptr()),
        },
    );
    unsafe {
        let me = c as *mut c_void;
        let i = &mut (*c).inner;
        for f in [
            &mut i.render,
            &mut i.capture,
            &mut i.clock,
            &mut i.simple,
            &mut i.stream_vol,
            &mut i.session,
        ] {
            f.owner = me;
        }
    }
    c
}

unsafe extern "win64" fn ac_qi(
    this: *mut Com<Client>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    match read_guid(riid) {
        Some(g)
            if g == IID_IUNKNOWN
                || g == IID_IAUDIOCLIENT
                || g == IID_IAUDIOCLIENT2
                || g == IID_IAUDIOCLIENT3 =>
        {
            Com::add_ref(this);
            put(out, this)
        }
        _ => qi_fail(out),
    }
}
unsafe extern "win64" fn ac_add_ref(this: *mut Com<Client>) -> u32 {
    Com::add_ref(this)
}
unsafe extern "win64" fn ac_release(this: *mut Com<Client>) -> u32 {
    Com::release(this)
}

fn frames_for(hns: i64, rate: u32) -> usize {
    ((hns.max(0) as u128 * rate as u128).div_ceil(10_000_000)) as usize
}

unsafe fn initialize(
    this: *mut Com<Client>,
    share: u32,
    flags: u32,
    duration: i64,
    period: i64,
    wfx: *const u8,
) -> i32 {
    let c = Com::get(this);
    let mut guard = lock(&c.sh.st);
    if guard.is_some() {
        return AUDCLNT_E_ALREADY_INITIALIZED;
    }
    if wfx.is_null() {
        return E_POINTER;
    }
    let Some(format) = Format::from_ptr(wfx) else {
        return AUDCLNT_E_UNSUPPORTED_FORMAT;
    };
    let exclusive = share == AUDCLNT_SHAREMODE_EXCLUSIVE;
    if share > 1 {
        return E_INVALIDARG;
    }
    let loopback = flags & AUDCLNT_STREAMFLAGS_LOOPBACK != 0;
    if loopback && (c.sh.flow != 0 || exclusive) {
        return AUDCLNT_E_WRONG_ENDPOINT_TYPE;
    }
    let period = if exclusive {
        let p = if period == 0 { DEFAULT_PERIOD } else { period };
        if p < MIN_PERIOD {
            return AUDCLNT_E_INVALID_DEVICE_PERIOD;
        }
        p
    } else {
        DEFAULT_PERIOD
    };
    let mut duration = duration.max(0);
    if exclusive && flags & AUDCLNT_STREAMFLAGS_EVENTCALLBACK != 0 {
        duration = period;
    }
    let period_frames = frames_for(period, format.rate).max(1);
    let buffer_frames =
        frames_for(duration, format.rate).max(period_frames * if exclusive { 1 } else { 2 });
    if buffer_frames > format.rate as usize * 20 {
        return AUDCLNT_E_BUFFER_TOO_LARGE;
    }
    if exclusive && EXCLUSIVE_STREAMS.fetch_add(1, Ordering::AcqRel) != 0 {
        EXCLUSIVE_STREAMS.fetch_sub(1, Ordering::AcqRel);
        return AUDCLNT_E_DEVICE_IN_USE;
    }
    let block = format.block as usize;
    let capture = c.sh.flow == 1 || loopback;
    *guard = Some(Stream {
        format,
        exclusive,
        flags,
        buffer_frames,
        period_frames,
        event: 0,
        started: false,
        ring: if capture {
            Vec::new()
        } else {
            vec![format.silence_byte(); buffer_frames * block]
        },
        read: 0,
        filled: 0,
        staging: vec![0u8; buffer_frames * block].into_boxed_slice(),
        pending: None,
        resampler: dsp::Resampler::new(),
        position: 0,
        volumes: vec![1.0; format.channels as usize],
        queue: None,
        conv: capture.then(|| CaptureConverter::new(format)),
        fifo: VecDeque::new(),
    });
    drop(guard);
    let id = audio::add_source(c.sh.clone());
    c.sh.source.store(id, Ordering::Relaxed);
    S_OK
}

unsafe extern "win64" fn ac_initialize(
    this: *mut Com<Client>,
    share: u32,
    flags: u32,
    duration: i64,
    period: i64,
    wfx: *const u8,
    _session: *const Guid,
) -> i32 {
    initialize(this, share, flags, duration, period, wfx)
}

fn with_stream<R>(
    this: *mut Com<Client>,
    f: impl FnOnce(&mut Stream) -> Result<R, i32>,
) -> Result<R, i32> {
    let c = unsafe { Com::get(this) };
    let mut guard = lock(&c.sh.st);
    match guard.as_mut() {
        Some(s) => f(s),
        None => Err(AUDCLNT_E_NOT_INITIALIZED),
    }
}
fn hr<T>(r: Result<T, i32>) -> i32 {
    match r {
        Ok(_) => S_OK,
        Err(e) => e,
    }
}

unsafe extern "win64" fn ac_buffer_size(this: *mut Com<Client>, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    hr(with_stream(this, |s| {
        *out = s.buffer_frames as u32;
        Ok(())
    }))
}
unsafe extern "win64" fn ac_latency(this: *mut Com<Client>, out: *mut i64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    hr(with_stream(this, |s| {
        *out = s.period_frames as i64 * 10_000_000 / s.format.rate as i64;
        Ok(())
    }))
}
unsafe extern "win64" fn ac_padding(this: *mut Com<Client>, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    hr(with_stream(this, |s| {
        *out = if s.conv.is_some() {
            (s.fifo.len() / s.format.block as usize) as u32
        } else {
            s.filled as u32
        };
        Ok(())
    }))
}
unsafe extern "win64" fn ac_is_supported(
    _this: *mut Com<Client>,
    share: u32,
    wfx: *const u8,
    closest: *mut *mut u8,
) -> i32 {
    if wfx.is_null() {
        return E_POINTER;
    }
    if !closest.is_null() {
        *closest = std::ptr::null_mut();
    }
    if share > 1 {
        return E_INVALIDARG;
    }
    if Format::from_ptr(wfx).is_some() {
        return S_OK;
    }
    if share == 0 && !closest.is_null() {
        *closest = alloc_format(&mix_format());
        return S_FALSE;
    }
    AUDCLNT_E_UNSUPPORTED_FORMAT
}
unsafe extern "win64" fn ac_mix_format(_this: *mut Com<Client>, out: *mut *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = alloc_format(&mix_format());
    S_OK
}
unsafe extern "win64" fn ac_period(_this: *mut Com<Client>, def: *mut i64, min: *mut i64) -> i32 {
    if def.is_null() && min.is_null() {
        return E_POINTER;
    }
    if !def.is_null() {
        *def = DEFAULT_PERIOD;
    }
    if !min.is_null() {
        *min = MIN_PERIOD;
    }
    S_OK
}
unsafe extern "win64" fn ac_start(this: *mut Com<Client>) -> i32 {
    let flow = Com::get(this).sh.flow;
    hr(with_stream(this, |s| {
        if s.started {
            return Err(AUDCLNT_E_NOT_STOPPED);
        }
        if s.flags & AUDCLNT_STREAMFLAGS_EVENTCALLBACK != 0 && s.event == 0 {
            return Err(AUDCLNT_E_EVENTHANDLE_NOT_SET);
        }
        if s.conv.is_some() && s.queue.is_none() {
            s.queue = Some(if flow == 0 {
                audio::subscribe_loopback()
            } else {
                audio::subscribe_microphone()
            });
        }
        s.started = true;
        Ok(())
    }))
}
unsafe extern "win64" fn ac_stop(this: *mut Com<Client>) -> i32 {
    match with_stream(this, |s| {
        let was = s.started;
        s.started = false;
        s.queue = None;
        Ok(was)
    }) {
        Ok(true) => S_OK,
        Ok(false) => S_FALSE,
        Err(e) => e,
    }
}
unsafe extern "win64" fn ac_reset(this: *mut Com<Client>) -> i32 {
    hr(with_stream(this, |s| {
        if s.started {
            return Err(AUDCLNT_E_NOT_STOPPED);
        }
        if s.pending.is_some() {
            return Err(AUDCLNT_E_BUFFER_OPERATION_PENDING);
        }
        s.read = 0;
        s.filled = 0;
        s.position = 0;
        s.fifo.clear();
        s.resampler.reset();
        Ok(())
    }))
}
unsafe extern "win64" fn ac_set_event(this: *mut Com<Client>, h: usize) -> i32 {
    if h == 0 {
        return E_INVALIDARG;
    }
    hr(with_stream(this, |s| {
        if s.flags & AUDCLNT_STREAMFLAGS_EVENTCALLBACK == 0 {
            return Err(AUDCLNT_E_EVENTHANDLE_NOT_EXPECTED);
        }
        s.event = h;
        Ok(())
    }))
}
unsafe extern "win64" fn ac_get_service(
    this: *mut Com<Client>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let capture = match with_stream(this, |s| Ok(s.conv.is_some())) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let i = &mut (*this).inner;
    let obj: *mut Iface = match read_guid(riid) {
        Some(g) if g == IID_IAUDIORENDERCLIENT => {
            if capture {
                return AUDCLNT_E_WRONG_ENDPOINT_TYPE;
            }
            &mut i.render
        }
        Some(g) if g == IID_IAUDIOCAPTURECLIENT => {
            if !capture {
                return AUDCLNT_E_WRONG_ENDPOINT_TYPE;
            }
            &mut i.capture
        }
        Some(g) if g == IID_IAUDIOCLOCK => &mut i.clock,
        Some(g) if g == IID_ISIMPLEAUDIOVOLUME => &mut i.simple,
        Some(g) if g == IID_IAUDIOSTREAMVOLUME => &mut i.stream_vol,
        Some(g) if g == IID_IAUDIOSESSIONCONTROL || g == IID_IAUDIOSESSIONCONTROL2 => {
            &mut i.session
        }
        _ => return E_NOINTERFACE,
    };
    Com::add_ref(this);
    *out = obj as *mut c_void;
    S_OK
}
unsafe extern "win64" fn ac_offload(_this: *mut Com<Client>, _cat: u32, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}
unsafe extern "win64" fn ac_set_props(_this: *mut Com<Client>, p: *const u8) -> i32 {
    if p.is_null() {
        E_POINTER
    } else {
        S_OK
    }
}
unsafe extern "win64" fn ac_size_limits(
    _this: *mut Com<Client>,
    wfx: *const u8,
    _event: i32,
    min: *mut i64,
    max: *mut i64,
) -> i32 {
    if wfx.is_null() || min.is_null() || max.is_null() {
        return E_POINTER;
    }
    if Format::from_ptr(wfx).is_none() {
        return AUDCLNT_E_UNSUPPORTED_FORMAT;
    }
    *min = MIN_PERIOD;
    *max = 20_000_000;
    S_OK
}
unsafe extern "win64" fn ac_engine_period(
    _this: *mut Com<Client>,
    wfx: *const u8,
    def: *mut u32,
    fund: *mut u32,
    min: *mut u32,
    max: *mut u32,
) -> i32 {
    let Some(f) = Format::from_ptr(wfx) else {
        return if wfx.is_null() {
            E_POINTER
        } else {
            AUDCLNT_E_UNSUPPORTED_FORMAT
        };
    };
    let p = f.rate / 100;
    for o in [def, fund, min, max] {
        if o.is_null() {
            return E_POINTER;
        }
    }
    *def = p;
    *fund = p;
    *min = p;
    *max = p;
    S_OK
}
unsafe extern "win64" fn ac_current_period(
    _this: *mut Com<Client>,
    fmt: *mut *mut u8,
    cur: *mut u32,
) -> i32 {
    if fmt.is_null() || cur.is_null() {
        return E_POINTER;
    }
    *fmt = alloc_format(&mix_format());
    *cur = audio::RATE / 100;
    S_OK
}
unsafe extern "win64" fn ac_init_shared(
    this: *mut Com<Client>,
    flags: u32,
    period_frames: u32,
    wfx: *const u8,
    _session: *const Guid,
) -> i32 {
    let rate = Format::from_ptr(wfx).map(|f| f.rate).unwrap_or(audio::RATE);
    let hns = period_frames as i64 * 10_000_000 / rate as i64;
    initialize(this, 0, flags, hns, 0, wfx)
}
static AC_VT: VTable<21> = VTable([
    ac_qi as *const (),
    ac_add_ref as *const (),
    ac_release as *const (),
    ac_initialize as *const (),
    ac_buffer_size as *const (),
    ac_latency as *const (),
    ac_padding as *const (),
    ac_is_supported as *const (),
    ac_mix_format as *const (),
    ac_period as *const (),
    ac_start as *const (),
    ac_stop as *const (),
    ac_reset as *const (),
    ac_set_event as *const (),
    ac_get_service as *const (),
    ac_offload as *const (),
    ac_set_props as *const (),
    ac_size_limits as *const (),
    ac_engine_period as *const (),
    ac_current_period as *const (),
    ac_init_shared as *const (),
]);

// ---------- services : IUnknown délégué ----------

unsafe fn client(this: *mut Iface) -> *mut Com<Client> {
    (*this).owner as *mut Com<Client>
}
unsafe extern "win64" fn svc_qi(this: *mut Iface, riid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if read_guid(riid) == Some(IID_IUNKNOWN) {
        Com::add_ref(client(this));
        *out = this as *mut c_void;
        return S_OK;
    }
    ac_get_service(client(this), riid, out)
}
unsafe extern "win64" fn svc_add_ref(this: *mut Iface) -> u32 {
    Com::add_ref(client(this))
}
unsafe extern "win64" fn svc_release(this: *mut Iface) -> u32 {
    Com::release(client(this))
}

// IAudioRenderClient
unsafe extern "win64" fn rc_get_buffer(this: *mut Iface, frames: u32, out: *mut *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    hr(with_stream(client(this), |s| {
        if s.pending.is_some() {
            return Err(AUDCLNT_E_OUT_OF_ORDER);
        }
        let free = s.buffer_frames - s.filled;
        if frames as usize > free {
            return Err(AUDCLNT_E_BUFFER_TOO_LARGE);
        }
        if s.exclusive
            && s.flags & AUDCLNT_STREAMFLAGS_EVENTCALLBACK != 0
            && frames as usize != s.buffer_frames
            && frames != 0
        {
            return Err(AUDCLNT_E_INVALID_SIZE);
        }
        s.pending = Some(frames as usize);
        *out = s.staging.as_mut_ptr();
        Ok(())
    }))
}
unsafe extern "win64" fn rc_release_buffer(this: *mut Iface, frames: u32, flags: u32) -> i32 {
    hr(with_stream(client(this), |s| {
        let Some(req) = s.pending else {
            return if frames == 0 {
                Ok(())
            } else {
                Err(AUDCLNT_E_OUT_OF_ORDER)
            };
        };
        let frames = frames as usize;
        if frames > req {
            return Err(AUDCLNT_E_INVALID_SIZE);
        }
        s.pending = None;
        let block = s.format.block as usize;
        let silence = s.format.silence_byte();
        let mut w = (s.read + s.filled) % s.buffer_frames;
        for i in 0..frames {
            let dst = &mut s.ring[w * block..(w + 1) * block];
            if flags & AUDCLNT_BUFFERFLAGS_SILENT != 0 {
                dst.fill(silence);
            } else {
                dst.copy_from_slice(&s.staging[i * block..(i + 1) * block]);
            }
            w = (w + 1) % s.buffer_frames;
        }
        s.filled += frames;
        Ok(())
    }))
}
static RENDER_VT: VTable<5> = VTable([
    svc_qi as *const (),
    svc_add_ref as *const (),
    svc_release as *const (),
    rc_get_buffer as *const (),
    rc_release_buffer as *const (),
]);

// IAudioCaptureClient
unsafe extern "win64" fn cc_get_buffer(
    this: *mut Iface,
    data: *mut *mut u8,
    frames: *mut u32,
    flags: *mut u32,
    devpos: *mut u64,
    qpc: *mut u64,
) -> i32 {
    if data.is_null() || frames.is_null() || flags.is_null() {
        return E_POINTER;
    }
    *data = std::ptr::null_mut();
    *frames = 0;
    *flags = 0;
    match with_stream(client(this), |s| {
        if s.pending.is_some() {
            return Err(AUDCLNT_E_OUT_OF_ORDER);
        }
        let block = s.format.block as usize;
        let avail = s.fifo.len() / block;
        if avail < s.period_frames {
            return Ok(false);
        }
        let n = s.period_frames;
        for (d, b) in s.staging.iter_mut().zip(s.fifo.iter()).take(n * block) {
            *d = *b;
        }
        s.pending = Some(n);
        *data = s.staging.as_mut_ptr();
        *frames = n as u32;
        if !devpos.is_null() {
            *devpos = s.position;
        }
        if !qpc.is_null() {
            *qpc = qpc_now();
        }
        Ok(true)
    }) {
        Ok(true) => S_OK,
        Ok(false) => AUDCLNT_S_BUFFER_EMPTY,
        Err(e) => e,
    }
}
unsafe extern "win64" fn cc_release_buffer(this: *mut Iface, frames: u32) -> i32 {
    hr(with_stream(client(this), |s| {
        let Some(req) = s.pending else {
            return if frames == 0 {
                Ok(())
            } else {
                Err(AUDCLNT_E_OUT_OF_ORDER)
            };
        };
        if frames != 0 && frames as usize != req {
            return Err(AUDCLNT_E_INVALID_SIZE);
        }
        s.pending = None;
        let n = frames as usize * s.format.block as usize;
        s.fifo.drain(..n.min(s.fifo.len()));
        s.position += frames as u64;
        Ok(())
    }))
}
unsafe extern "win64" fn cc_next_packet(this: *mut Iface, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    hr(with_stream(client(this), |s| {
        let avail = s.fifo.len() / s.format.block as usize;
        *out = if avail >= s.period_frames {
            s.period_frames as u32
        } else {
            0
        };
        Ok(())
    }))
}
static CAPTURE_VT: VTable<6> = VTable([
    svc_qi as *const (),
    svc_add_ref as *const (),
    svc_release as *const (),
    cc_get_buffer as *const (),
    cc_release_buffer as *const (),
    cc_next_packet as *const (),
]);

/// Horloge QPC en unités de 100 ns.
fn qpc_now() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
    }
    ts.tv_sec as u64 * 10_000_000 + ts.tv_nsec as u64 / 100
}

// IAudioClock : fréquence en octets/s, position en octets.
unsafe extern "win64" fn clk_freq(this: *mut Iface, out: *mut u64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    hr(with_stream(client(this), |s| {
        *out = s.format.bytes_per_sec() as u64;
        Ok(())
    }))
}
unsafe extern "win64" fn clk_position(this: *mut Iface, pos: *mut u64, qpc: *mut u64) -> i32 {
    if pos.is_null() {
        return E_POINTER;
    }
    hr(with_stream(client(this), |s| {
        *pos = s.position * s.format.block as u64;
        if !qpc.is_null() {
            *qpc = qpc_now();
        }
        Ok(())
    }))
}
unsafe extern "win64" fn clk_chars(_this: *mut Iface, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}
static CLOCK_VT: VTable<6> = VTable([
    svc_qi as *const (),
    svc_add_ref as *const (),
    svc_release as *const (),
    clk_freq as *const (),
    clk_position as *const (),
    clk_chars as *const (),
]);

// ISimpleAudioVolume (volume de session de ce flux)
unsafe extern "win64" fn sv_set(this: *mut Iface, v: f32, _ctx: *const Guid) -> i32 {
    if !(0.0..=1.0).contains(&v) {
        return E_INVALIDARG;
    }
    Com::get(client(this))
        .sh
        .session_volume
        .store(v.to_bits(), Ordering::Relaxed);
    S_OK
}
unsafe extern "win64" fn sv_get(this: *mut Iface, out: *mut f32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = f32::from_bits(
        Com::get(client(this))
            .sh
            .session_volume
            .load(Ordering::Relaxed),
    );
    S_OK
}
unsafe extern "win64" fn sv_set_mute(this: *mut Iface, m: i32, _ctx: *const Guid) -> i32 {
    Com::get(client(this))
        .sh
        .session_mute
        .store(u32::from(m != 0), Ordering::Relaxed);
    S_OK
}
unsafe extern "win64" fn sv_get_mute(this: *mut Iface, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = Com::get(client(this))
        .sh
        .session_mute
        .load(Ordering::Relaxed) as i32;
    S_OK
}
static SIMPLE_VT: VTable<7> = VTable([
    svc_qi as *const (),
    svc_add_ref as *const (),
    svc_release as *const (),
    sv_set as *const (),
    sv_get as *const (),
    sv_set_mute as *const (),
    sv_get_mute as *const (),
]);

// IAudioStreamVolume
unsafe extern "win64" fn stv_count(this: *mut Iface, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    hr(with_stream(client(this), |s| {
        *out = s.volumes.len() as u32;
        Ok(())
    }))
}
unsafe extern "win64" fn stv_set(this: *mut Iface, i: u32, v: f32) -> i32 {
    hr(with_stream(client(this), |s| {
        if !(0.0..=1.0).contains(&v) {
            return Err(E_INVALIDARG);
        }
        let slot = s.volumes.get_mut(i as usize).ok_or(E_INVALIDARG)?;
        *slot = v;
        Ok(())
    }))
}
unsafe extern "win64" fn stv_get(this: *mut Iface, i: u32, out: *mut f32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    hr(with_stream(client(this), |s| {
        *out = *s.volumes.get(i as usize).ok_or(E_INVALIDARG)?;
        Ok(())
    }))
}
unsafe extern "win64" fn stv_set_all(this: *mut Iface, n: u32, v: *const f32) -> i32 {
    if v.is_null() {
        return E_POINTER;
    }
    hr(with_stream(client(this), |s| {
        if n as usize != s.volumes.len() {
            return Err(E_INVALIDARG);
        }
        let src = std::slice::from_raw_parts(v, n as usize);
        if src.iter().any(|x| !(0.0..=1.0).contains(x)) {
            return Err(E_INVALIDARG);
        }
        s.volumes.copy_from_slice(src);
        Ok(())
    }))
}
unsafe extern "win64" fn stv_get_all(this: *mut Iface, n: u32, v: *mut f32) -> i32 {
    if v.is_null() {
        return E_POINTER;
    }
    hr(with_stream(client(this), |s| {
        if n as usize != s.volumes.len() {
            return Err(E_INVALIDARG);
        }
        std::ptr::copy_nonoverlapping(s.volumes.as_ptr(), v, n as usize);
        Ok(())
    }))
}
static STREAMVOL_VT: VTable<8> = VTable([
    svc_qi as *const (),
    svc_add_ref as *const (),
    svc_release as *const (),
    stv_count as *const (),
    stv_set as *const (),
    stv_get as *const (),
    stv_set_all as *const (),
    stv_get_all as *const (),
]);

// IAudioSessionControl2
unsafe extern "win64" fn ses_state(this: *mut Iface, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let active = with_stream(client(this), |s| Ok(s.started)).unwrap_or(false);
    *out = u32::from(active); // AudioSessionStateInactive / Active
    S_OK
}
unsafe extern "win64" fn ses_get_str(_this: *mut Iface, out: *mut *mut u16) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = alloc_wide("");
    S_OK
}
unsafe extern "win64" fn ses_set_str(_this: *mut Iface, _s: *const u16, _ctx: *const Guid) -> i32 {
    S_OK
}
unsafe extern "win64" fn ses_grouping(_this: *mut Iface, out: *mut Guid) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = Guid::default();
    S_OK
}
unsafe extern "win64" fn ses_set_grouping(
    _this: *mut Iface,
    _g: *const Guid,
    _ctx: *const Guid,
) -> i32 {
    S_OK
}
unsafe extern "win64" fn ses_register(_this: *mut Iface, p: *mut c_void) -> i32 {
    if p.is_null() {
        E_POINTER
    } else {
        S_OK
    }
}
unsafe extern "win64" fn ses_identifier(_this: *mut Iface, out: *mut *mut u16) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = alloc_wide(&format!("{RENDER_ID}|{}", audio::backend::app_name()));
    S_OK
}
unsafe extern "win64" fn ses_pid(_this: *mut Iface, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::process::id();
    S_OK
}
unsafe extern "win64" fn ses_is_system(_this: *mut Iface) -> i32 {
    S_FALSE
}
unsafe extern "win64" fn ses_ducking(_this: *mut Iface, _opt: i32) -> i32 {
    S_OK
}
static SESSION_VT: VTable<17> = VTable([
    svc_qi as *const (),
    svc_add_ref as *const (),
    svc_release as *const (),
    ses_state as *const (),
    ses_get_str as *const (),
    ses_set_str as *const (),
    ses_get_str as *const (),
    ses_set_str as *const (),
    ses_grouping as *const (),
    ses_set_grouping as *const (),
    ses_register as *const (),
    ses_register as *const (),
    ses_identifier as *const (),
    ses_identifier as *const (),
    ses_pid as *const (),
    ses_is_system as *const (),
    ses_ducking as *const (),
]);

// ---------------------------------------------------------------------------
// IAudioEndpointVolume : pilote le volume maître du flux de ce programme.
// ---------------------------------------------------------------------------

struct EndpointVolume;

const MIN_DB: f32 = -65.25;

fn scalar_to_db(v: f32) -> f32 {
    if v <= 0.0005 {
        MIN_DB
    } else {
        (20.0 * v.log10()).max(MIN_DB)
    }
}
fn db_to_scalar(db: f32) -> f32 {
    if db <= MIN_DB {
        0.0
    } else {
        10f32.powf(db.min(0.0) / 20.0)
    }
}

unsafe extern "win64" fn epv_qi(
    this: *mut Com<EndpointVolume>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    match read_guid(riid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IAUDIOENDPOINTVOLUME => {
            Com::add_ref(this);
            put(out, this)
        }
        _ => qi_fail(out),
    }
}
unsafe extern "win64" fn epv_add_ref(this: *mut Com<EndpointVolume>) -> u32 {
    Com::add_ref(this)
}
unsafe extern "win64" fn epv_release(this: *mut Com<EndpointVolume>) -> u32 {
    Com::release(this)
}
unsafe extern "win64" fn epv_register(_this: *mut Com<EndpointVolume>, p: *mut c_void) -> i32 {
    if p.is_null() {
        E_POINTER
    } else {
        S_OK
    }
}
unsafe extern "win64" fn epv_count(_this: *mut Com<EndpointVolume>, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 2;
    S_OK
}
unsafe extern "win64" fn epv_set_db(
    _this: *mut Com<EndpointVolume>,
    db: f32,
    _ctx: *const Guid,
) -> i32 {
    if !(MIN_DB..=0.0).contains(&db) {
        return E_INVALIDARG;
    }
    audio::set_master_volume(db_to_scalar(db));
    S_OK
}
unsafe extern "win64" fn epv_set_scalar(
    _this: *mut Com<EndpointVolume>,
    v: f32,
    _ctx: *const Guid,
) -> i32 {
    if !(0.0..=1.0).contains(&v) {
        return E_INVALIDARG;
    }
    audio::set_master_volume(v);
    S_OK
}
unsafe extern "win64" fn epv_get_db(_this: *mut Com<EndpointVolume>, out: *mut f32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = scalar_to_db(audio::master_volume());
    S_OK
}
unsafe extern "win64" fn epv_get_scalar(_this: *mut Com<EndpointVolume>, out: *mut f32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = audio::master_volume();
    S_OK
}
unsafe extern "win64" fn epv_set_ch_db(
    this: *mut Com<EndpointVolume>,
    ch: u32,
    db: f32,
    ctx: *const Guid,
) -> i32 {
    if ch > 1 {
        return E_INVALIDARG;
    }
    epv_set_db(this, db, ctx)
}
unsafe extern "win64" fn epv_set_ch_scalar(
    this: *mut Com<EndpointVolume>,
    ch: u32,
    v: f32,
    ctx: *const Guid,
) -> i32 {
    if ch > 1 {
        return E_INVALIDARG;
    }
    epv_set_scalar(this, v, ctx)
}
unsafe extern "win64" fn epv_get_ch_db(
    this: *mut Com<EndpointVolume>,
    ch: u32,
    out: *mut f32,
) -> i32 {
    if ch > 1 {
        return E_INVALIDARG;
    }
    epv_get_db(this, out)
}
unsafe extern "win64" fn epv_get_ch_scalar(
    this: *mut Com<EndpointVolume>,
    ch: u32,
    out: *mut f32,
) -> i32 {
    if ch > 1 {
        return E_INVALIDARG;
    }
    epv_get_scalar(this, out)
}
unsafe extern "win64" fn epv_set_mute(
    _this: *mut Com<EndpointVolume>,
    m: i32,
    _ctx: *const Guid,
) -> i32 {
    audio::set_muted(m != 0);
    S_OK
}
unsafe extern "win64" fn epv_get_mute(_this: *mut Com<EndpointVolume>, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = i32::from(audio::muted());
    S_OK
}
unsafe extern "win64" fn epv_step_info(
    _this: *mut Com<EndpointVolume>,
    step: *mut u32,
    count: *mut u32,
) -> i32 {
    if step.is_null() || count.is_null() {
        return E_POINTER;
    }
    *count = 51;
    *step = (audio::master_volume() * 50.0).round() as u32;
    S_OK
}
unsafe extern "win64" fn epv_step_up(_this: *mut Com<EndpointVolume>, _ctx: *const Guid) -> i32 {
    audio::set_master_volume(audio::master_volume() + 0.02);
    S_OK
}
unsafe extern "win64" fn epv_step_down(_this: *mut Com<EndpointVolume>, _ctx: *const Guid) -> i32 {
    audio::set_master_volume(audio::master_volume() - 0.02);
    S_OK
}
unsafe extern "win64" fn epv_hw(_this: *mut Com<EndpointVolume>, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}
unsafe extern "win64" fn epv_range(
    _this: *mut Com<EndpointVolume>,
    min: *mut f32,
    max: *mut f32,
    inc: *mut f32,
) -> i32 {
    if min.is_null() || max.is_null() || inc.is_null() {
        return E_POINTER;
    }
    *min = MIN_DB;
    *max = 0.0;
    *inc = 0.75;
    S_OK
}
static EPV_VT: VTable<21> = VTable([
    epv_qi as *const (),
    epv_add_ref as *const (),
    epv_release as *const (),
    epv_register as *const (),
    epv_register as *const (),
    epv_count as *const (),
    epv_set_db as *const (),
    epv_set_scalar as *const (),
    epv_get_db as *const (),
    epv_get_scalar as *const (),
    epv_set_ch_db as *const (),
    epv_set_ch_scalar as *const (),
    epv_get_ch_db as *const (),
    epv_get_ch_scalar as *const (),
    epv_set_mute as *const (),
    epv_get_mute as *const (),
    epv_step_info as *const (),
    epv_step_up as *const (),
    epv_step_down as *const (),
    epv_hw as *const (),
    epv_range as *const (),
]);

// ---------------------------------------------------------------------------
// Création COM et exports
// ---------------------------------------------------------------------------

unsafe fn create_enumerator(riid: *const Guid, out: *mut *mut c_void) -> i32 {
    let e = Com::create(EN_VT.ptr(), Enumerator);
    let hr = en_qi(e, riid, out);
    Com::release(e);
    hr
}

pub fn creator(clsid: &Guid) -> Option<Creator> {
    (*clsid == CLSID_MMDEVICEENUMERATOR).then_some(create_enumerator as Creator)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "DllGetClassObject"=>fn dll_get_class_object,"DllCanUnloadNow"=>fn dll_can_unload_now,
    }
}

// avrt.dll : les priorités « Pro Audio » n'existent pas sous Linux ; on
// renvoie un jeton valide pour que le programme continue normalement.
pub unsafe extern "win64" fn av_set_mm_thread_characteristics(
    task: *const u8,
    index: *mut u32,
) -> usize {
    if task.is_null() || index.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    *index = 1;
    0x4156_5254
}
pub unsafe extern "win64" fn av_revert(_h: usize) -> i32 {
    1
}
pub unsafe extern "win64" fn av_set_priority(h: usize, _prio: i32) -> i32 {
    i32::from(h != 0)
}

pub fn avrt_exports() -> crate::dlls::Exports {
    crate::exports! {
        "AvSetMmThreadCharacteristicsA"=>fn av_set_mm_thread_characteristics,
        "AvSetMmThreadCharacteristicsW"=>fn av_set_mm_thread_characteristics,
        "AvRevertMmThreadCharacteristics"=>fn av_revert,
        "AvSetMmThreadPriority"=>fn av_set_priority,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn call<R: Copy>(obj: *mut c_void, idx: usize) -> R {
        let vt = *(obj as *const *const usize);
        let f: unsafe extern "win64" fn(*mut c_void) -> R = std::mem::transmute(*vt.add(idx));
        f(obj)
    }

    #[test]
    fn wasapi_partage_evenementiel_consomme_le_tampon() {
        std::env::set_var("WINLUX_AUDIO", "null");
        unsafe {
            let mut en: *mut c_void = std::ptr::null_mut();
            assert_eq!(create_enumerator(&IID_IMMDEVICEENUMERATOR, &mut en), S_OK);
            let mut dev: *mut c_void = std::ptr::null_mut();
            assert_eq!(en_default(en as _, 0, 0, &mut dev), S_OK);
            let mut id: *mut u16 = std::ptr::null_mut();
            assert_eq!(dev_get_id(dev as _, &mut id), S_OK);
            assert_eq!(super::super::wstr(id), RENDER_ID);
            super::super::ole32::co_task_mem_free(id as *mut c_void);
            let mut ac: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                dev_activate(dev as _, &IID_IAUDIOCLIENT, 0, std::ptr::null(), &mut ac),
                S_OK
            );
            let mut mix: *mut u8 = std::ptr::null_mut();
            assert_eq!(ac_mix_format(ac as _, &mut mix), S_OK);
            assert_eq!(Format::from_ptr(mix), Some(mix_format()));
            assert_eq!(
                initialize(ac as _, 0, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, 0, 0, mix),
                S_OK
            );
            assert_eq!(ac_start(ac as _), AUDCLNT_E_EVENTHANDLE_NOT_SET);
            let ev = crate::dlls::kernel32::create_event_w(0, 0, 0, std::ptr::null());
            assert_eq!(ac_set_event(ac as _, ev), S_OK);
            let mut frames = 0u32;
            assert_eq!(ac_buffer_size(ac as _, &mut frames), S_OK);
            assert_eq!(frames, 960);
            let mut rc: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                ac_get_service(ac as _, &IID_IAUDIORENDERCLIENT, &mut rc),
                S_OK
            );
            let mut cap: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                ac_get_service(ac as _, &IID_IAUDIOCAPTURECLIENT, &mut cap),
                AUDCLNT_E_WRONG_ENDPOINT_TYPE
            );
            let mut p: *mut u8 = std::ptr::null_mut();
            assert_eq!(
                rc_get_buffer(rc as _, 961, &mut p),
                AUDCLNT_E_BUFFER_TOO_LARGE
            );
            assert_eq!(rc_get_buffer(rc as _, 960, &mut p), S_OK);
            std::ptr::write_bytes(p, 0, 960 * 8);
            assert_eq!(rc_release_buffer(rc as _, 960, 0), S_OK);
            assert_eq!(ac_start(ac as _), S_OK);
            for _ in 0..3 {
                assert_eq!(crate::dlls::kernel32::wait_for_single_object(ev, 2000), 0);
            }
            let mut pad = 0;
            assert_eq!(ac_padding(ac as _, &mut pad), S_OK);
            assert!(pad < 960, "{pad}");
            let mut clock: *mut c_void = std::ptr::null_mut();
            assert_eq!(ac_get_service(ac as _, &IID_IAUDIOCLOCK, &mut clock), S_OK);
            let mut pos = 0u64;
            assert_eq!(
                clk_position(clock as _, &mut pos, std::ptr::null_mut()),
                S_OK
            );
            assert!(pos > 0);
            assert_eq!(ac_reset(ac as _), AUDCLNT_E_NOT_STOPPED);
            assert_eq!(ac_stop(ac as _), S_OK);
            assert_eq!(ac_stop(ac as _), S_FALSE);
            assert_eq!(ac_reset(ac as _), S_OK);
            for o in [clock, rc, ac, dev, en] {
                let _: u32 = call(o, 2);
            }
            super::super::ole32::co_task_mem_free(mix as *mut c_void);
            crate::dlls::kernel32::close_handle(ev);
        }
    }

    #[test]
    fn wasapi_exclusif_unique_et_loopback() {
        std::env::set_var("WINLUX_AUDIO", "null");
        unsafe {
            let fmt = Format::new(audio::format::Sample::S16, 2, 44100)
                .unwrap()
                .to_bytes();
            let a = new_client(0);
            let b = new_client(0);
            assert_eq!(initialize(a, 1, 0, 200_000, 100_000, fmt.as_ptr()), S_OK);
            assert_eq!(
                initialize(b, 1, 0, 200_000, 100_000, fmt.as_ptr()),
                AUDCLNT_E_DEVICE_IN_USE
            );
            Com::release(a);
            assert_eq!(
                initialize(b, 1, 0, 200_000, 10_000, fmt.as_ptr()),
                AUDCLNT_E_INVALID_DEVICE_PERIOD
            );
            Com::release(b);
            // Loopback : un client de capture sur le périphérique de rendu.
            let lb = new_client(0);
            assert_eq!(
                initialize(
                    lb,
                    0,
                    AUDCLNT_STREAMFLAGS_LOOPBACK,
                    1_000_000,
                    0,
                    fmt.as_ptr()
                ),
                S_OK
            );
            let mut cap: *mut c_void = std::ptr::null_mut();
            assert_eq!(ac_get_service(lb, &IID_IAUDIOCAPTURECLIENT, &mut cap), S_OK);
            assert_eq!(ac_start(lb), S_OK);
            let t0 = std::time::Instant::now();
            let mut packet = 0;
            while packet == 0 {
                assert!(t0.elapsed().as_secs() < 5);
                std::thread::sleep(std::time::Duration::from_millis(5));
                assert_eq!(cc_next_packet(cap as _, &mut packet), S_OK);
            }
            assert_eq!(packet, 441);
            let (mut data, mut n, mut flags) = (std::ptr::null_mut(), 0u32, 0u32);
            assert_eq!(
                cc_get_buffer(
                    cap as _,
                    &mut data,
                    &mut n,
                    &mut flags,
                    std::ptr::null_mut(),
                    std::ptr::null_mut()
                ),
                S_OK
            );
            assert_eq!(n, 441);
            assert_eq!(cc_release_buffer(cap as _, 440), AUDCLNT_E_INVALID_SIZE);
            assert_eq!(cc_release_buffer(cap as _, 441), S_OK);
            svc_release(cap as _);
            Com::release(lb);
        }
    }
}
