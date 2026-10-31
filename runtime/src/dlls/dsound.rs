//! dsound.dll : DirectSound et DirectSoundCapture écrits de zéro.
//!
//! Tampon primaire (format, volume global, auditeur 3D), tampons secondaires
//! mixés par le moteur audio de Winlux (lecture, boucle, volume, panoramique,
//! fréquence, notifications de position, duplication) et spatialisation 3D
//! simple (atténuation avec la distance et panoramique selon l'azimut).

use super::audio_com::*;
use super::ole32::Guid;
use crate::audio::{self, dsp, format::Format, CaptureConverter, CaptureQueue};
use crate::dlls::lock;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

const DSERR_CONTROLUNAVAIL: i32 = 0x8878_001Eu32 as i32;
const DSERR_INVALIDCALL: i32 = 0x8878_0032u32 as i32;
const DSERR_PRIOLEVELNEEDED: i32 = 0x8878_0046u32 as i32;
const DSERR_BADFORMAT: i32 = 0x8878_0064u32 as i32;
const DSERR_NODRIVER: i32 = 0x8878_0078u32 as i32;
const DSERR_ALREADYINITIALIZED: i32 = 0x8878_0082u32 as i32;
const DSERR_UNINITIALIZED: i32 = 0x8878_00AAu32 as i32;
const DSERR_FXUNAVAILABLE: i32 = 0x8878_00B0u32 as i32;
const DSERR_OBJECTNOTFOUND: i32 = 0x8878_1161u32 as i32;

const DSBCAPS_PRIMARYBUFFER: u32 = 0x1;
const DSBCAPS_LOCSOFTWARE: u32 = 0x8;
const DSBCAPS_CTRL3D: u32 = 0x10;
const DSBCAPS_CTRLFREQUENCY: u32 = 0x20;
const DSBCAPS_CTRLPAN: u32 = 0x40;
const DSBCAPS_CTRLVOLUME: u32 = 0x80;
const DSBCAPS_CTRLPOSITIONNOTIFY: u32 = 0x100;
const DSBCAPS_LOCHARDWARE: u32 = 0x4;

const DSBPLAY_LOOPING: u32 = 0x1;
const DSBSTATUS_PLAYING: u32 = 0x1;
const DSBSTATUS_LOOPING: u32 = 0x4;
const DSBSTATUS_LOCSOFTWARE: u32 = 0x10;
const DSBLOCK_FROMWRITECURSOR: u32 = 0x1;
const DSBLOCK_ENTIREBUFFER: u32 = 0x2;
const DSBPN_OFFSETSTOP: u32 = u32::MAX;
const DSSCL_PRIORITY: u32 = 2;
const DS3D_DEFERRED: u32 = 1;

const CLSID_DIRECTSOUND: Guid = guid(
    0x47d4d946,
    0x62e8,
    0x11cf,
    [0x93, 0xbc, 0x44, 0x45, 0x53, 0x54, 0, 0],
);
const CLSID_DIRECTSOUND8: Guid = guid(
    0x3901cc3f,
    0x84b5,
    0x4fa4,
    [0xba, 0x35, 0xaa, 0x81, 0x72, 0xb8, 0xa0, 0x9b],
);
const CLSID_DIRECTSOUNDCAPTURE: Guid = guid(
    0xb0210780,
    0x89cd,
    0x11d0,
    [0xaf, 0x08, 0x00, 0xa0, 0xc9, 0x25, 0xcd, 0x16],
);
const CLSID_DIRECTSOUNDCAPTURE8: Guid = guid(
    0xe4bcac13,
    0x7f99,
    0x4908,
    [0x9a, 0x8e, 0x74, 0xe3, 0xbf, 0x24, 0xb6, 0xe1],
);
const IID_IDIRECTSOUND: Guid = guid(
    0x279afa83,
    0x4981,
    0x11ce,
    [0xa5, 0x21, 0x00, 0x20, 0xaf, 0x0b, 0xe5, 0x60],
);
const IID_IDIRECTSOUND8: Guid = guid(
    0xc50a7e93,
    0xf395,
    0x4834,
    [0x9e, 0xf6, 0x7f, 0xa9, 0x9d, 0xe5, 0x09, 0x66],
);
const IID_IDIRECTSOUNDBUFFER: Guid = guid(
    0x279afa85,
    0x4981,
    0x11ce,
    [0xa5, 0x21, 0x00, 0x20, 0xaf, 0x0b, 0xe5, 0x60],
);
const IID_IDIRECTSOUNDBUFFER8: Guid = guid(
    0x6825a449,
    0x7524,
    0x4d82,
    [0x92, 0x0f, 0x50, 0xe3, 0x6a, 0xb3, 0xab, 0x1e],
);
const IID_IDIRECTSOUND3DLISTENER: Guid = guid(
    0x279afa84,
    0x4981,
    0x11ce,
    [0xa5, 0x21, 0x00, 0x20, 0xaf, 0x0b, 0xe5, 0x60],
);
const IID_IDIRECTSOUND3DBUFFER: Guid = guid(
    0x279afa86,
    0x4981,
    0x11ce,
    [0xa5, 0x21, 0x00, 0x20, 0xaf, 0x0b, 0xe5, 0x60],
);
const IID_IDIRECTSOUNDNOTIFY: Guid = guid(
    0xb0210783,
    0x89cd,
    0x11d0,
    [0xaf, 0x08, 0x00, 0xa0, 0xc9, 0x25, 0xcd, 0x16],
);
const IID_IDIRECTSOUNDCAPTURE: Guid = guid(
    0xb0210781,
    0x89cd,
    0x11d0,
    [0xaf, 0x08, 0x00, 0xa0, 0xc9, 0x25, 0xcd, 0x16],
);
const IID_IDIRECTSOUNDCAPTUREBUFFER: Guid = guid(
    0xb0210782,
    0x89cd,
    0x11d0,
    [0xaf, 0x08, 0x00, 0xa0, 0xc9, 0x25, 0xcd, 0x16],
);
const IID_IDIRECTSOUNDCAPTUREBUFFER8: Guid = guid(
    0x00990df4,
    0x0dbb,
    0x4872,
    [0x83, 0x3e, 0x6d, 0x30, 0x3e, 0x80, 0xae, 0xb6],
);
const DSDEVID_DEFAULTPLAYBACK: Guid = guid(
    0xdef00000,
    0x9c6d,
    0x47ed,
    [0xaa, 0xf1, 0x4d, 0xda, 0x8f, 0x2b, 0x5c, 0x03],
);
const DSDEVID_DEFAULTCAPTURE: Guid = guid(
    0xdef00001,
    0x9c6d,
    0x47ed,
    [0xaa, 0xf1, 0x4d, 0xda, 0x8f, 0x2b, 0x5c, 0x03],
);
const DSDEVID_DEFAULTVOICEPLAYBACK: Guid = guid(
    0xdef00002,
    0x9c6d,
    0x47ed,
    [0xaa, 0xf1, 0x4d, 0xda, 0x8f, 0x2b, 0x5c, 0x03],
);
const DSDEVID_DEFAULTVOICECAPTURE: Guid = guid(
    0xdef00003,
    0x9c6d,
    0x47ed,
    [0xaa, 0xf1, 0x4d, 0xda, 0x8f, 0x2b, 0x5c, 0x03],
);
/// Identifiant des périphériques Winlux (« WINLUX-AUDIO »).
pub const WINLUX_RENDER_GUID: Guid = guid(0x574e_4c58, 0x4155, 0x4449, [0x4f, 0, 0, 0, 0, 0, 0, 1]);
pub const WINLUX_CAPTURE_GUID: Guid =
    guid(0x574e_4c58, 0x4155, 0x4449, [0x4f, 0, 0, 0, 0, 0, 0, 2]);

const RENDER_NAME: &str = "Haut-parleurs (Winlux)";
const CAPTURE_NAME: &str = "Microphone (Winlux)";

// ---------------------------------------------------------------------------
// Mémoire partagée par un tampon et ses duplicata.
// ---------------------------------------------------------------------------

struct Memory {
    ptr: *mut u8,
    len: usize,
}
unsafe impl Send for Memory {}
unsafe impl Sync for Memory {}

impl Memory {
    fn new(len: usize, fill: u8) -> Option<Arc<Memory>> {
        let mut v: Vec<u8> = Vec::new();
        v.try_reserve_exact(len).ok()?;
        v.resize(len, fill);
        let b = v.into_boxed_slice();
        Some(Arc::new(Memory {
            len,
            ptr: Box::into_raw(b) as *mut u8,
        }))
    }
    fn slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl Drop for Memory {
    fn drop(&mut self) {
        unsafe {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                self.ptr, self.len,
            )));
        }
    }
}

// ---------------------------------------------------------------------------
// Périphérique (IDirectSound / IDirectSound8)
// ---------------------------------------------------------------------------

struct DevShared {
    coop: AtomicU32,
    speaker: AtomicU32,
    /// Auditeur 3D actif et réglages différés.
    listener: Mutex<(dsp::Listener, dsp::Listener)>,
    buffers: Mutex<Vec<Weak<BufShared>>>,
    primary_format: Mutex<Format>,
    primary: AtomicUsize,
}

struct Device {
    sh: Arc<DevShared>,
    initialized: AtomicBool,
}

fn new_device() -> *mut Com<Device> {
    Com::create(
        DS_VT.ptr(),
        Device {
            sh: Arc::new(DevShared {
                coop: AtomicU32::new(1),
                speaker: AtomicU32::new(0x0004_0004), // DSSPEAKER_STEREO, géométrie large
                listener: Mutex::new((dsp::Listener::default(), dsp::Listener::default())),
                buffers: Mutex::new(Vec::new()),
                primary_format: Mutex::new(
                    Format::new(audio::format::Sample::S16, 2, 44100).expect("format"),
                ),
                primary: AtomicUsize::new(0),
            }),
            initialized: AtomicBool::new(false),
        },
    )
}

fn known_render_guid(g: Option<Guid>) -> bool {
    match g {
        None => true,
        Some(g) => {
            g == DSDEVID_DEFAULTPLAYBACK
                || g == DSDEVID_DEFAULTVOICEPLAYBACK
                || g == WINLUX_RENDER_GUID
                || g == Guid::default()
        }
    }
}

fn known_capture_guid(g: Option<Guid>) -> bool {
    match g {
        None => true,
        Some(g) => {
            g == DSDEVID_DEFAULTCAPTURE
                || g == DSDEVID_DEFAULTVOICECAPTURE
                || g == WINLUX_CAPTURE_GUID
                || g == Guid::default()
        }
    }
}

unsafe extern "win64" fn ds_qi(
    this: *mut Com<Device>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    match read_guid(riid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IDIRECTSOUND || g == IID_IDIRECTSOUND8 => {
            Com::add_ref(this);
            put(out, this)
        }
        _ => {
            if !out.is_null() {
                *out = std::ptr::null_mut();
            }
            E_NOINTERFACE
        }
    }
}
unsafe extern "win64" fn ds_add_ref(this: *mut Com<Device>) -> u32 {
    Com::add_ref(this)
}
unsafe extern "win64" fn ds_release(this: *mut Com<Device>) -> u32 {
    Com::release(this)
}

unsafe fn ready(this: *mut Com<Device>) -> Result<&'static Device, i32> {
    let d: &Device = Com::get(this);
    if d.initialized.load(Ordering::Acquire) {
        Ok(d)
    } else {
        Err(DSERR_UNINITIALIZED)
    }
}

// DSBUFFERDESC : dwSize @0, dwFlags @4, dwBufferBytes @8, lpwfxFormat @16.
unsafe extern "win64" fn ds_create_sound_buffer(
    this: *mut Com<Device>,
    desc: *const u8,
    out: *mut *mut c_void,
    outer: *mut c_void,
) -> i32 {
    let d = match ready(this) {
        Ok(d) => d,
        Err(e) => return e,
    };
    if out.is_null() || desc.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    if !outer.is_null() {
        return CLASS_E_NOAGGREGATION;
    }
    let size = *(desc as *const u32);
    if size != 20 && size != 36 {
        return E_INVALIDARG;
    }
    let flags = *(desc.add(4) as *const u32);
    let bytes = *(desc.add(8) as *const u32) as usize;
    let wfx = *(desc.add(16) as *const *const u8);
    if flags & DSBCAPS_PRIMARYBUFFER != 0 {
        if bytes != 0 || !wfx.is_null() {
            return E_INVALIDARG;
        }
        let existing = d.sh.primary.load(Ordering::Acquire);
        if existing != 0 {
            Com::add_ref(existing as *mut Com<Buffer>);
            *out = existing as *mut c_void;
            return S_OK;
        }
        let b = new_buffer(d.sh.clone(), true, flags, None);
        d.sh.primary.store(b as usize, Ordering::Release);
        *out = b as *mut c_void;
        return S_OK;
    }
    let Some(fmt) = Format::from_ptr(wfx) else {
        return if wfx.is_null() {
            E_INVALIDARG
        } else {
            DSERR_BADFORMAT
        };
    };
    if !(4..=0x0fff_ffff).contains(&bytes) || bytes < fmt.block as usize {
        return E_INVALIDARG;
    }
    if flags & DSBCAPS_CTRL3D != 0 && fmt.channels != 1 {
        return E_INVALIDARG;
    }
    if flags & DSBCAPS_LOCHARDWARE != 0 && flags & DSBCAPS_LOCSOFTWARE != 0 {
        return E_INVALIDARG;
    }
    let len = bytes - bytes % fmt.block as usize;
    let Some(mem) = Memory::new(len, fmt.silence_byte()) else {
        return E_OUTOFMEMORY;
    };
    let b = new_buffer(d.sh.clone(), false, flags, Some((fmt, mem)));
    *out = b as *mut c_void;
    S_OK
}

// DSCAPS : 24 DWORD.
unsafe extern "win64" fn ds_get_caps(this: *mut Com<Device>, caps: *mut u32) -> i32 {
    if let Err(e) = ready(this) {
        return e;
    }
    if caps.is_null() || *caps != 96 {
        return E_INVALIDARG;
    }
    std::ptr::write_bytes(caps.add(1), 0, 23);
    // PRIMARYMONO|STEREO|8BIT|16BIT|CONTINUOUSRATE, SECONDARYMONO|STEREO|8BIT|16BIT.
    *caps.add(1) = 0x1 | 0x2 | 0x4 | 0x8 | 0x10 | 0x100 | 0x200 | 0x400 | 0x800;
    *caps.add(2) = 100;
    *caps.add(3) = 200_000;
    *caps.add(4) = 1;
    *caps.add(21) = 0; // dwPlayCpuOverheadSwBuffers
    S_OK
}

unsafe extern "win64" fn ds_duplicate(
    this: *mut Com<Device>,
    orig: *mut Com<Buffer>,
    out: *mut *mut c_void,
) -> i32 {
    let d = match ready(this) {
        Ok(d) => d,
        Err(e) => return e,
    };
    if orig.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    let src = Com::get(orig);
    if src.primary {
        return DSERR_INVALIDCALL;
    }
    let (fmt, mem) = {
        let st = lock(&src.sh.st);
        (st.format, src.sh.mem.clone())
    };
    let b = new_buffer(d.sh.clone(), false, src.flags, Some((fmt, mem)));
    {
        let s = lock(&src.sh.st);
        let mut t = lock(&Com::get(b).sh.st);
        t.volume = s.volume;
        t.pan = s.pan;
        t.freq = s.freq;
    }
    *out = b as *mut c_void;
    S_OK
}

unsafe extern "win64" fn ds_set_coop(this: *mut Com<Device>, _hwnd: usize, level: u32) -> i32 {
    let d = match ready(this) {
        Ok(d) => d,
        Err(e) => return e,
    };
    if !(1..=4).contains(&level) {
        return E_INVALIDARG;
    }
    d.sh.coop.store(level, Ordering::Relaxed);
    S_OK
}
unsafe extern "win64" fn ds_compact(this: *mut Com<Device>) -> i32 {
    match ready(this) {
        Ok(_) => S_OK,
        Err(e) => e,
    }
}
unsafe extern "win64" fn ds_get_speaker(this: *mut Com<Device>, out: *mut u32) -> i32 {
    let d = match ready(this) {
        Ok(d) => d,
        Err(e) => return e,
    };
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = d.sh.speaker.load(Ordering::Relaxed);
    S_OK
}
unsafe extern "win64" fn ds_set_speaker(this: *mut Com<Device>, v: u32) -> i32 {
    let d = match ready(this) {
        Ok(d) => d,
        Err(e) => return e,
    };
    d.sh.speaker.store(v, Ordering::Relaxed);
    S_OK
}
unsafe extern "win64" fn ds_initialize(this: *mut Com<Device>, dev: *const Guid) -> i32 {
    if !known_render_guid(read_guid(dev)) {
        return DSERR_NODRIVER;
    }
    if Com::get(this).initialized.swap(true, Ordering::AcqRel) {
        return DSERR_ALREADYINITIALIZED;
    }
    S_OK
}
unsafe extern "win64" fn ds_verify(this: *mut Com<Device>, out: *mut u32) -> i32 {
    if let Err(e) = ready(this) {
        return e;
    }
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 1; // DS_UNCERTIFIED
    S_OK
}

static DS_VT: VTable<12> = VTable([
    ds_qi as *const (),
    ds_add_ref as *const (),
    ds_release as *const (),
    ds_create_sound_buffer as *const (),
    ds_get_caps as *const (),
    ds_duplicate as *const (),
    ds_set_coop as *const (),
    ds_compact as *const (),
    ds_get_speaker as *const (),
    ds_set_speaker as *const (),
    ds_initialize as *const (),
    ds_verify as *const (),
]);

// ---------------------------------------------------------------------------
// Tampons (IDirectSoundBuffer8, IDirectSoundNotify, IDirectSound3DBuffer,
// IDirectSound3DListener)
// ---------------------------------------------------------------------------

struct BufState {
    format: Format,
    pos: usize,
    playing: bool,
    looping: bool,
    volume: i32,
    pan: i32,
    freq: u32,
    resampler: dsp::Resampler,
    notifies: Vec<(u32, usize)>,
    emitter: dsp::Emitter,
    pending: dsp::Emitter,
}

struct BufShared {
    st: Mutex<BufState>,
    mem: Arc<Memory>,
    dev: Arc<DevShared>,
    ctrl3d: bool,
}

impl audio::Source for BufShared {
    fn mix(&self, out: &mut [f32]) -> bool {
        let mut events = Vec::new();
        {
            let mut st = lock(&self.st);
            if !st.playing {
                return true;
            }
            let mut gain = dsp::pan_gains(st.pan);
            let v = dsp::db100_to_gain(st.volume);
            gain[0] *= v;
            gain[1] *= v;
            if self.ctrl3d {
                let l = lock(&self.dev.listener).0;
                let g = dsp::spatialize(&l, &st.emitter);
                gain[0] *= g[0];
                gain[1] *= g[1];
            }
            let rate = if st.freq == 0 {
                st.format.rate
            } else {
                st.freq
            };
            let step = rate as f64 / audio::RATE as f64;
            let mut rs = std::mem::take(&mut st.resampler);
            let mem = self.mem.slice();
            {
                let s = &mut *st;
                let block = s.format.block as usize;
                let len = mem.len();
                let mut pull = || {
                    if !s.playing || s.pos + block > len {
                        return None;
                    }
                    let frame = s.format.stereo(&mem[s.pos..s.pos + block]);
                    let old = s.pos;
                    let mut new = old + block;
                    for &(off, ev) in &s.notifies {
                        if off != DSBPN_OFFSETSTOP && (off as usize) >= old && (off as usize) < new
                        {
                            events.push(ev);
                        }
                    }
                    if new + block > len {
                        if s.looping {
                            new = 0;
                        } else {
                            s.playing = false;
                            for &(off, ev) in &s.notifies {
                                if off == DSBPN_OFFSETSTOP {
                                    events.push(ev);
                                }
                            }
                        }
                    }
                    s.pos = new;
                    Some(frame)
                };
                rs.mix_into(step, out, gain, &mut pull);
            }
            st.resampler = rs;
        }
        for e in events {
            signal(e);
        }
        true
    }
}

struct Buffer {
    sh: Arc<BufShared>,
    primary: bool,
    flags: u32,
    source: u64,
    notify: Iface,
    b3d: Iface,
    listener: Iface,
}

impl Drop for Buffer {
    fn drop(&mut self) {
        if self.primary {
            self.sh.dev.primary.store(0, Ordering::Release);
        } else {
            audio::remove_source(self.source);
        }
    }
}

fn new_buffer(
    dev: Arc<DevShared>,
    primary: bool,
    flags: u32,
    data: Option<(Format, Arc<Memory>)>,
) -> *mut Com<Buffer> {
    let (format, mem) = match data {
        Some(d) => d,
        None => {
            let f = *lock(&dev.primary_format);
            let len = (f.bytes_per_sec() as usize / 4).max(4096);
            let len = len - len % f.block as usize;
            (
                f,
                Memory::new(len, f.silence_byte()).expect("tampon primaire"),
            )
        }
    };
    let sh = Arc::new(BufShared {
        st: Mutex::new(BufState {
            format,
            pos: 0,
            playing: false,
            looping: false,
            volume: 0,
            pan: 0,
            freq: 0,
            resampler: dsp::Resampler::new(),
            notifies: Vec::new(),
            emitter: dsp::Emitter::default(),
            pending: dsp::Emitter::default(),
        }),
        mem,
        dev: dev.clone(),
        ctrl3d: flags & DSBCAPS_CTRL3D != 0 && !primary,
    });
    let source = if primary {
        0
    } else {
        lock(&dev.buffers).push(Arc::downgrade(&sh));
        audio::add_source(sh.clone())
    };
    let b = Com::create(
        BUF_VT.ptr(),
        Buffer {
            sh,
            primary,
            flags,
            source,
            notify: Iface::new(NOTIFY_VT.ptr()),
            b3d: Iface::new(B3D_VT.ptr()),
            listener: Iface::new(LISTENER_VT.ptr()),
        },
    );
    unsafe {
        (*b).inner.notify.owner = b as *mut c_void;
        (*b).inner.b3d.owner = b as *mut c_void;
        (*b).inner.listener.owner = b as *mut c_void;
    }
    b
}

unsafe extern "win64" fn buf_qi(
    this: *mut Com<Buffer>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let b = &mut (*this).inner;
    let obj: *mut c_void = match read_guid(riid) {
        Some(g)
            if g == IID_IUNKNOWN || g == IID_IDIRECTSOUNDBUFFER || g == IID_IDIRECTSOUNDBUFFER8 =>
        {
            this as *mut c_void
        }
        Some(g)
            if g == IID_IDIRECTSOUNDNOTIFY
                && !b.primary
                && b.flags & DSBCAPS_CTRLPOSITIONNOTIFY != 0 =>
        {
            &mut b.notify as *mut Iface as *mut c_void
        }
        Some(g) if g == IID_IDIRECTSOUND3DBUFFER && b.sh.ctrl3d => {
            &mut b.b3d as *mut Iface as *mut c_void
        }
        Some(g) if g == IID_IDIRECTSOUND3DLISTENER && b.primary => {
            &mut b.listener as *mut Iface as *mut c_void
        }
        _ => return E_NOINTERFACE,
    };
    Com::add_ref(this);
    *out = obj;
    S_OK
}
unsafe extern "win64" fn buf_add_ref(this: *mut Com<Buffer>) -> u32 {
    Com::add_ref(this)
}
unsafe extern "win64" fn buf_release(this: *mut Com<Buffer>) -> u32 {
    Com::release(this)
}

// DSBCAPS : dwSize, dwFlags, dwBufferBytes, dwUnlockTransferRate, dwPlayCpuOverhead.
unsafe extern "win64" fn buf_get_caps(this: *mut Com<Buffer>, caps: *mut u32) -> i32 {
    if caps.is_null() || *caps != 20 {
        return E_INVALIDARG;
    }
    let b = Com::get(this);
    *caps.add(1) = (b.flags & !DSBCAPS_LOCHARDWARE) | DSBCAPS_LOCSOFTWARE;
    *caps.add(2) = b.sh.mem.len as u32;
    *caps.add(3) = 0;
    *caps.add(4) = 0;
    S_OK
}

fn write_lead(fmt: &Format) -> usize {
    // Le curseur d'écriture devance la lecture d'une période de mixage.
    let bytes = fmt.bytes_per_sec() as usize / 100;
    bytes - bytes % fmt.block as usize + fmt.block as usize
}

unsafe extern "win64" fn buf_get_position(
    this: *mut Com<Buffer>,
    play: *mut u32,
    write: *mut u32,
) -> i32 {
    let b = Com::get(this);
    let len = b.sh.mem.len;
    let (pos, fmt) = if b.primary {
        let f = *lock(&b.sh.dev.primary_format);
        let frames = audio::frames_played() * f.rate as u64 / audio::RATE as u64;
        ((frames as usize * f.block as usize) % len, f)
    } else {
        let st = lock(&b.sh.st);
        (st.pos, st.format)
    };
    if !play.is_null() {
        *play = pos as u32;
    }
    if !write.is_null() {
        *write = ((pos + write_lead(&fmt)) % len) as u32;
    }
    S_OK
}

unsafe fn copy_format(f: &Format, wfx: *mut u8, size: u32, written: *mut u32) -> i32 {
    let bytes = f.to_bytes();
    if wfx.is_null() {
        if written.is_null() {
            return E_INVALIDARG;
        }
        *written = bytes.len() as u32;
        return S_OK;
    }
    let n = bytes.len().min(size as usize);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), wfx, n);
    if !written.is_null() {
        *written = n as u32;
    }
    S_OK
}

unsafe extern "win64" fn buf_get_format(
    this: *mut Com<Buffer>,
    wfx: *mut u8,
    size: u32,
    written: *mut u32,
) -> i32 {
    let b = Com::get(this);
    let f = if b.primary {
        *lock(&b.sh.dev.primary_format)
    } else {
        lock(&b.sh.st).format
    };
    copy_format(&f, wfx, size, written)
}

unsafe extern "win64" fn buf_get_volume(this: *mut Com<Buffer>, out: *mut i32) -> i32 {
    let b = Com::get(this);
    if out.is_null() {
        return E_INVALIDARG;
    }
    if b.flags & DSBCAPS_CTRLVOLUME == 0 {
        return DSERR_CONTROLUNAVAIL;
    }
    *out = if b.primary {
        dsp::gain_to_db100(audio::master_volume())
    } else {
        lock(&b.sh.st).volume
    };
    S_OK
}
unsafe extern "win64" fn buf_get_pan(this: *mut Com<Buffer>, out: *mut i32) -> i32 {
    let b = Com::get(this);
    if out.is_null() {
        return E_INVALIDARG;
    }
    if b.flags & DSBCAPS_CTRLPAN == 0 {
        return DSERR_CONTROLUNAVAIL;
    }
    *out = lock(&b.sh.st).pan;
    S_OK
}
unsafe extern "win64" fn buf_get_frequency(this: *mut Com<Buffer>, out: *mut u32) -> i32 {
    let b = Com::get(this);
    if out.is_null() {
        return E_INVALIDARG;
    }
    if b.flags & DSBCAPS_CTRLFREQUENCY == 0 {
        return DSERR_CONTROLUNAVAIL;
    }
    let st = lock(&b.sh.st);
    *out = if st.freq == 0 {
        st.format.rate
    } else {
        st.freq
    };
    S_OK
}
unsafe extern "win64" fn buf_get_status(this: *mut Com<Buffer>, out: *mut u32) -> i32 {
    let b = Com::get(this);
    if out.is_null() {
        return E_INVALIDARG;
    }
    let st = lock(&b.sh.st);
    let mut s = DSBSTATUS_LOCSOFTWARE;
    if st.playing || b.primary {
        s |= DSBSTATUS_PLAYING;
    }
    if st.looping || b.primary {
        s |= DSBSTATUS_LOOPING;
    }
    *out = s;
    S_OK
}
unsafe extern "win64" fn buf_initialize(
    _this: *mut Com<Buffer>,
    _ds: *mut c_void,
    _desc: *const u8,
) -> i32 {
    DSERR_ALREADYINITIALIZED
}

unsafe extern "win64" fn buf_lock(
    this: *mut Com<Buffer>,
    mut offset: u32,
    mut bytes: u32,
    p1: *mut *mut u8,
    n1: *mut u32,
    p2: *mut *mut u8,
    n2: *mut u32,
    flags: u32,
) -> i32 {
    let b = Com::get(this);
    if p1.is_null() || n1.is_null() {
        return E_INVALIDARG;
    }
    if b.primary {
        return DSERR_PRIOLEVELNEEDED;
    }
    let len = b.sh.mem.len as u32;
    if flags & DSBLOCK_FROMWRITECURSOR != 0 {
        let mut w = 0u32;
        buf_get_position(this, std::ptr::null_mut(), &mut w);
        offset = w;
    }
    if flags & DSBLOCK_ENTIREBUFFER != 0 {
        bytes = len;
    }
    if offset >= len || bytes == 0 || bytes > len {
        return E_INVALIDARG;
    }
    let first = bytes.min(len - offset);
    *p1 = b.sh.mem.ptr.add(offset as usize);
    *n1 = first;
    let rest = bytes - first;
    if !p2.is_null() {
        *p2 = if rest > 0 {
            b.sh.mem.ptr
        } else {
            std::ptr::null_mut()
        };
    }
    if !n2.is_null() {
        *n2 = if p2.is_null() { 0 } else { rest };
    }
    S_OK
}

unsafe extern "win64" fn buf_play(this: *mut Com<Buffer>, _r: u32, _prio: u32, flags: u32) -> i32 {
    let b = Com::get(this);
    if b.primary {
        return S_OK;
    }
    let mut st = lock(&b.sh.st);
    if !st.playing {
        st.resampler.reset();
    }
    st.playing = true;
    st.looping = flags & DSBPLAY_LOOPING != 0;
    S_OK
}

unsafe extern "win64" fn buf_set_position(this: *mut Com<Buffer>, pos: u32) -> i32 {
    let b = Com::get(this);
    if b.primary {
        return DSERR_INVALIDCALL;
    }
    let mut st = lock(&b.sh.st);
    let block = st.format.block as usize;
    if pos as usize >= b.sh.mem.len {
        return E_INVALIDARG;
    }
    st.pos = pos as usize - pos as usize % block;
    st.resampler.reset();
    S_OK
}

unsafe extern "win64" fn buf_set_format(this: *mut Com<Buffer>, wfx: *const u8) -> i32 {
    let b = Com::get(this);
    if !b.primary {
        return DSERR_INVALIDCALL;
    }
    if b.sh.dev.coop.load(Ordering::Relaxed) < DSSCL_PRIORITY {
        return DSERR_PRIOLEVELNEEDED;
    }
    match Format::from_ptr(wfx) {
        Some(f) => {
            *lock(&b.sh.dev.primary_format) = f;
            S_OK
        }
        None if wfx.is_null() => E_INVALIDARG,
        None => DSERR_BADFORMAT,
    }
}

unsafe extern "win64" fn buf_set_volume(this: *mut Com<Buffer>, v: i32) -> i32 {
    let b = Com::get(this);
    if b.flags & DSBCAPS_CTRLVOLUME == 0 {
        return DSERR_CONTROLUNAVAIL;
    }
    if !(-10000..=0).contains(&v) {
        return E_INVALIDARG;
    }
    if b.primary {
        audio::set_master_volume(dsp::db100_to_gain(v));
    } else {
        lock(&b.sh.st).volume = v;
    }
    S_OK
}
unsafe extern "win64" fn buf_set_pan(this: *mut Com<Buffer>, v: i32) -> i32 {
    let b = Com::get(this);
    if b.flags & DSBCAPS_CTRLPAN == 0 {
        return DSERR_CONTROLUNAVAIL;
    }
    if !(-10000..=10000).contains(&v) {
        return E_INVALIDARG;
    }
    lock(&b.sh.st).pan = v;
    S_OK
}
unsafe extern "win64" fn buf_set_frequency(this: *mut Com<Buffer>, v: u32) -> i32 {
    let b = Com::get(this);
    if b.flags & DSBCAPS_CTRLFREQUENCY == 0 || b.primary {
        return DSERR_CONTROLUNAVAIL;
    }
    if v != 0 && !(100..=200_000).contains(&v) {
        return E_INVALIDARG;
    }
    lock(&b.sh.st).freq = v;
    S_OK
}
unsafe extern "win64" fn buf_stop(this: *mut Com<Buffer>) -> i32 {
    let b = Com::get(this);
    let mut st = lock(&b.sh.st);
    let was = st.playing;
    st.playing = false;
    let stops: Vec<usize> = st
        .notifies
        .iter()
        .filter(|(o, _)| *o == DSBPN_OFFSETSTOP)
        .map(|(_, e)| *e)
        .collect();
    drop(st);
    if was {
        for e in stops {
            signal(e);
        }
    }
    S_OK
}
unsafe extern "win64" fn buf_unlock(
    this: *mut Com<Buffer>,
    p1: *mut u8,
    _n1: u32,
    _p2: *mut u8,
    _n2: u32,
) -> i32 {
    let b = Com::get(this);
    if p1.is_null() || b.primary {
        return if b.primary {
            DSERR_PRIOLEVELNEEDED
        } else {
            E_INVALIDARG
        };
    }
    S_OK
}
unsafe extern "win64" fn buf_restore(_this: *mut Com<Buffer>) -> i32 {
    S_OK
}
unsafe extern "win64" fn buf_set_fx(
    _this: *mut Com<Buffer>,
    count: u32,
    _desc: *const u8,
    results: *mut u32,
) -> i32 {
    if count == 0 {
        return S_OK;
    }
    if !results.is_null() {
        for i in 0..count as usize {
            *results.add(i) = 5; // DSFXR_UNKNOWN
        }
    }
    DSERR_FXUNAVAILABLE
}
unsafe extern "win64" fn buf_acquire(
    _this: *mut Com<Buffer>,
    _flags: u32,
    count: u32,
    results: *mut u32,
) -> i32 {
    if !results.is_null() {
        for i in 0..count as usize {
            *results.add(i) = 5;
        }
    }
    S_OK
}
unsafe extern "win64" fn buf_get_object(
    _this: *mut Com<Buffer>,
    _g: *const Guid,
    _i: u32,
    _iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    DSERR_OBJECTNOTFOUND
}

static BUF_VT: VTable<24> = VTable([
    buf_qi as *const (),
    buf_add_ref as *const (),
    buf_release as *const (),
    buf_get_caps as *const (),
    buf_get_position as *const (),
    buf_get_format as *const (),
    buf_get_volume as *const (),
    buf_get_pan as *const (),
    buf_get_frequency as *const (),
    buf_get_status as *const (),
    buf_initialize as *const (),
    buf_lock as *const (),
    buf_play as *const (),
    buf_set_position as *const (),
    buf_set_format as *const (),
    buf_set_volume as *const (),
    buf_set_pan as *const (),
    buf_set_frequency as *const (),
    buf_stop as *const (),
    buf_unlock as *const (),
    buf_restore as *const (),
    buf_set_fx as *const (),
    buf_acquire as *const (),
    buf_get_object as *const (),
]);

// ---------- interfaces secondaires : IUnknown délégué ----------

unsafe fn owner(this: *mut Iface) -> *mut Com<Buffer> {
    (*this).owner as *mut Com<Buffer>
}
unsafe extern "win64" fn tear_qi(
    this: *mut Iface,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    buf_qi(owner(this), riid, out)
}
unsafe extern "win64" fn tear_add_ref(this: *mut Iface) -> u32 {
    Com::add_ref(owner(this))
}
unsafe extern "win64" fn tear_release(this: *mut Iface) -> u32 {
    Com::release(owner(this))
}

// DSBPOSITIONNOTIFY : dwOffset @0, hEventNotify @8.
unsafe extern "win64" fn notify_set(this: *mut Iface, count: u32, list: *const u8) -> i32 {
    let b = Com::get(owner(this));
    if count > 0 && list.is_null() {
        return E_INVALIDARG;
    }
    let mut st = lock(&b.sh.st);
    if st.playing {
        return DSERR_INVALIDCALL;
    }
    let mut v = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        let off = *(list.add(i * 16) as *const u32);
        let ev = *(list.add(i * 16 + 8) as *const usize);
        if off != DSBPN_OFFSETSTOP && off as usize >= b.sh.mem.len {
            return E_INVALIDARG;
        }
        v.push((off, ev));
    }
    st.notifies = v;
    S_OK
}
static NOTIFY_VT: VTable<4> = VTable([
    tear_qi as *const (),
    tear_add_ref as *const (),
    tear_release as *const (),
    notify_set as *const (),
]);

// ---------- IDirectSound3DBuffer ----------

unsafe fn emitter_update(this: *mut Iface, apply: u32, f: impl FnOnce(&mut dsp::Emitter)) -> i32 {
    let b = Com::get(owner(this));
    let mut st = lock(&b.sh.st);
    f(&mut st.pending);
    if apply != DS3D_DEFERRED {
        st.emitter = st.pending;
    }
    S_OK
}
unsafe fn emitter(this: *mut Iface) -> dsp::Emitter {
    lock(&Com::get(owner(this)).sh.st).emitter
}
unsafe fn put_vec(p: *mut f32, v: dsp::Vec3) -> i32 {
    if p.is_null() {
        return E_INVALIDARG;
    }
    std::ptr::copy_nonoverlapping(v.as_ptr(), p, 3);
    S_OK
}
unsafe fn read_vec(p: *const f32) -> dsp::Vec3 {
    [*p, *p.add(1), *p.add(2)]
}

// DS3DBUFFER (64 octets) : dwSize, vPosition, vVelocity, dwInsideConeAngle,
// dwOutsideConeAngle, vConeOrientation, lConeOutsideVolume, flMinDistance,
// flMaxDistance, dwMode.
unsafe extern "win64" fn b3d_get_all(this: *mut Iface, p: *mut u8) -> i32 {
    if p.is_null() || *(p as *const u32) != 64 {
        return E_INVALIDARG;
    }
    let e = emitter(this);
    let f = p as *mut f32;
    put_vec(f.add(1), e.position);
    put_vec(f.add(4), e.velocity);
    *(p.add(28) as *mut u32) = e.inside_cone;
    *(p.add(32) as *mut u32) = e.outside_cone;
    put_vec(f.add(9), e.cone_orientation);
    *(p.add(48) as *mut i32) = e.cone_outside_volume;
    *(p.add(52) as *mut f32) = e.min_distance;
    *(p.add(56) as *mut f32) = e.max_distance;
    *(p.add(60) as *mut u32) = e.mode;
    S_OK
}
unsafe extern "win64" fn b3d_get_cone_angles(this: *mut Iface, i: *mut u32, o: *mut u32) -> i32 {
    let e = emitter(this);
    if !i.is_null() {
        *i = e.inside_cone;
    }
    if !o.is_null() {
        *o = e.outside_cone;
    }
    S_OK
}
unsafe extern "win64" fn b3d_get_cone_orientation(this: *mut Iface, v: *mut f32) -> i32 {
    put_vec(v, emitter(this).cone_orientation)
}
unsafe extern "win64" fn b3d_get_cone_volume(this: *mut Iface, v: *mut i32) -> i32 {
    if v.is_null() {
        return E_INVALIDARG;
    }
    *v = emitter(this).cone_outside_volume;
    S_OK
}
unsafe extern "win64" fn b3d_get_max(this: *mut Iface, v: *mut f32) -> i32 {
    if v.is_null() {
        return E_INVALIDARG;
    }
    *v = emitter(this).max_distance;
    S_OK
}
unsafe extern "win64" fn b3d_get_min(this: *mut Iface, v: *mut f32) -> i32 {
    if v.is_null() {
        return E_INVALIDARG;
    }
    *v = emitter(this).min_distance;
    S_OK
}
unsafe extern "win64" fn b3d_get_mode(this: *mut Iface, v: *mut u32) -> i32 {
    if v.is_null() {
        return E_INVALIDARG;
    }
    *v = emitter(this).mode;
    S_OK
}
unsafe extern "win64" fn b3d_get_position(this: *mut Iface, v: *mut f32) -> i32 {
    put_vec(v, emitter(this).position)
}
unsafe extern "win64" fn b3d_get_velocity(this: *mut Iface, v: *mut f32) -> i32 {
    put_vec(v, emitter(this).velocity)
}
unsafe extern "win64" fn b3d_set_all(this: *mut Iface, p: *const u8, apply: u32) -> i32 {
    if p.is_null() || *(p as *const u32) != 64 {
        return E_INVALIDARG;
    }
    let f = p as *const f32;
    let e = dsp::Emitter {
        position: read_vec(f.add(1)),
        velocity: read_vec(f.add(4)),
        inside_cone: *(p.add(28) as *const u32),
        outside_cone: *(p.add(32) as *const u32),
        cone_orientation: read_vec(f.add(9)),
        cone_outside_volume: *(p.add(48) as *const i32),
        min_distance: *(p.add(52) as *const f32),
        max_distance: *(p.add(56) as *const f32),
        mode: *(p.add(60) as *const u32),
    };
    if e.mode > 2 || e.min_distance <= 0.0 || e.max_distance <= 0.0 {
        return E_INVALIDARG;
    }
    emitter_update(this, apply, |x| *x = e)
}
unsafe extern "win64" fn b3d_set_cone_angles(this: *mut Iface, i: u32, o: u32, apply: u32) -> i32 {
    if i > 360 || o > 360 || i > o {
        return E_INVALIDARG;
    }
    emitter_update(this, apply, |e| {
        e.inside_cone = i;
        e.outside_cone = o;
    })
}
unsafe extern "win64" fn b3d_set_cone_orientation(
    this: *mut Iface,
    x: f32,
    y: f32,
    z: f32,
    apply: u32,
) -> i32 {
    emitter_update(this, apply, |e| e.cone_orientation = [x, y, z])
}
unsafe extern "win64" fn b3d_set_cone_volume(this: *mut Iface, v: i32, apply: u32) -> i32 {
    if !(-10000..=0).contains(&v) {
        return E_INVALIDARG;
    }
    emitter_update(this, apply, |e| e.cone_outside_volume = v)
}
unsafe extern "win64" fn b3d_set_max(this: *mut Iface, v: f32, apply: u32) -> i32 {
    if v <= 0.0 || !v.is_finite() {
        return E_INVALIDARG;
    }
    emitter_update(this, apply, |e| e.max_distance = v)
}
unsafe extern "win64" fn b3d_set_min(this: *mut Iface, v: f32, apply: u32) -> i32 {
    if v <= 0.0 || !v.is_finite() {
        return E_INVALIDARG;
    }
    emitter_update(this, apply, |e| e.min_distance = v)
}
unsafe extern "win64" fn b3d_set_mode(this: *mut Iface, v: u32, apply: u32) -> i32 {
    if v > 2 {
        return E_INVALIDARG;
    }
    emitter_update(this, apply, |e| e.mode = v)
}
unsafe extern "win64" fn b3d_set_position(
    this: *mut Iface,
    x: f32,
    y: f32,
    z: f32,
    apply: u32,
) -> i32 {
    emitter_update(this, apply, |e| e.position = [x, y, z])
}
unsafe extern "win64" fn b3d_set_velocity(
    this: *mut Iface,
    x: f32,
    y: f32,
    z: f32,
    apply: u32,
) -> i32 {
    emitter_update(this, apply, |e| e.velocity = [x, y, z])
}
static B3D_VT: VTable<21> = VTable([
    tear_qi as *const (),
    tear_add_ref as *const (),
    tear_release as *const (),
    b3d_get_all as *const (),
    b3d_get_cone_angles as *const (),
    b3d_get_cone_orientation as *const (),
    b3d_get_cone_volume as *const (),
    b3d_get_max as *const (),
    b3d_get_min as *const (),
    b3d_get_mode as *const (),
    b3d_get_position as *const (),
    b3d_get_velocity as *const (),
    b3d_set_all as *const (),
    b3d_set_cone_angles as *const (),
    b3d_set_cone_orientation as *const (),
    b3d_set_cone_volume as *const (),
    b3d_set_max as *const (),
    b3d_set_min as *const (),
    b3d_set_mode as *const (),
    b3d_set_position as *const (),
    b3d_set_velocity as *const (),
]);

// ---------- IDirectSound3DListener ----------

unsafe fn dev_of(this: *mut Iface) -> Arc<DevShared> {
    Com::get(owner(this)).sh.dev.clone()
}
unsafe fn listener_update(this: *mut Iface, apply: u32, f: impl FnOnce(&mut dsp::Listener)) -> i32 {
    let d = dev_of(this);
    let mut l = lock(&d.listener);
    f(&mut l.1);
    if apply != DS3D_DEFERRED {
        l.0 = l.1;
    }
    S_OK
}
unsafe fn listener(this: *mut Iface) -> dsp::Listener {
    lock(&dev_of(this).listener).0
}
unsafe fn put_f(p: *mut f32, v: f32) -> i32 {
    if p.is_null() {
        return E_INVALIDARG;
    }
    *p = v;
    S_OK
}

// DS3DLISTENER (64 octets) : dwSize, vPosition, vVelocity, vOrientFront,
// vOrientTop, flDistanceFactor, flRolloffFactor, flDopplerFactor.
unsafe extern "win64" fn lis_get_all(this: *mut Iface, p: *mut u8) -> i32 {
    if p.is_null() || *(p as *const u32) != 64 {
        return E_INVALIDARG;
    }
    let l = listener(this);
    let f = p as *mut f32;
    put_vec(f.add(1), l.position);
    put_vec(f.add(4), l.velocity);
    put_vec(f.add(7), l.front);
    put_vec(f.add(10), l.top);
    *f.add(13) = l.distance_factor;
    *f.add(14) = l.rolloff;
    *f.add(15) = l.doppler;
    S_OK
}
unsafe extern "win64" fn lis_get_distance(this: *mut Iface, v: *mut f32) -> i32 {
    put_f(v, listener(this).distance_factor)
}
unsafe extern "win64" fn lis_get_doppler(this: *mut Iface, v: *mut f32) -> i32 {
    put_f(v, listener(this).doppler)
}
unsafe extern "win64" fn lis_get_orientation(
    this: *mut Iface,
    front: *mut f32,
    top: *mut f32,
) -> i32 {
    let l = listener(this);
    if put_vec(front, l.front) != S_OK {
        return E_INVALIDARG;
    }
    put_vec(top, l.top)
}
unsafe extern "win64" fn lis_get_position(this: *mut Iface, v: *mut f32) -> i32 {
    put_vec(v, listener(this).position)
}
unsafe extern "win64" fn lis_get_rolloff(this: *mut Iface, v: *mut f32) -> i32 {
    put_f(v, listener(this).rolloff)
}
unsafe extern "win64" fn lis_get_velocity(this: *mut Iface, v: *mut f32) -> i32 {
    put_vec(v, listener(this).velocity)
}
unsafe extern "win64" fn lis_set_all(this: *mut Iface, p: *const u8, apply: u32) -> i32 {
    if p.is_null() || *(p as *const u32) != 64 {
        return E_INVALIDARG;
    }
    let f = p as *const f32;
    let l = dsp::Listener {
        position: read_vec(f.add(1)),
        velocity: read_vec(f.add(4)),
        front: read_vec(f.add(7)),
        top: read_vec(f.add(10)),
        distance_factor: *f.add(13),
        rolloff: *f.add(14),
        doppler: *f.add(15),
    };
    listener_update(this, apply, |x| *x = l)
}
unsafe extern "win64" fn lis_set_distance(this: *mut Iface, v: f32, apply: u32) -> i32 {
    if v <= 0.0 || !v.is_finite() {
        return E_INVALIDARG;
    }
    listener_update(this, apply, |l| l.distance_factor = v)
}
unsafe extern "win64" fn lis_set_doppler(this: *mut Iface, v: f32, apply: u32) -> i32 {
    if !(0.0..=10.0).contains(&v) {
        return E_INVALIDARG;
    }
    listener_update(this, apply, |l| l.doppler = v)
}
unsafe extern "win64" fn lis_set_orientation(
    this: *mut Iface,
    xf: f32,
    yf: f32,
    zf: f32,
    xt: f32,
    yt: f32,
    zt: f32,
    apply: u32,
) -> i32 {
    listener_update(this, apply, |l| {
        l.front = [xf, yf, zf];
        l.top = [xt, yt, zt];
    })
}
unsafe extern "win64" fn lis_set_position(
    this: *mut Iface,
    x: f32,
    y: f32,
    z: f32,
    apply: u32,
) -> i32 {
    listener_update(this, apply, |l| l.position = [x, y, z])
}
unsafe extern "win64" fn lis_set_rolloff(this: *mut Iface, v: f32, apply: u32) -> i32 {
    if !(0.0..=10.0).contains(&v) {
        return E_INVALIDARG;
    }
    listener_update(this, apply, |l| l.rolloff = v)
}
unsafe extern "win64" fn lis_set_velocity(
    this: *mut Iface,
    x: f32,
    y: f32,
    z: f32,
    apply: u32,
) -> i32 {
    listener_update(this, apply, |l| l.velocity = [x, y, z])
}
unsafe extern "win64" fn lis_commit(this: *mut Iface) -> i32 {
    let d = dev_of(this);
    {
        let mut l = lock(&d.listener);
        l.0 = l.1;
    }
    let bufs: Vec<Arc<BufShared>> = lock(&d.buffers).iter().filter_map(Weak::upgrade).collect();
    lock(&d.buffers).retain(|w| w.strong_count() > 0);
    for b in bufs {
        let mut st = lock(&b.st);
        st.emitter = st.pending;
    }
    S_OK
}
static LISTENER_VT: VTable<18> = VTable([
    tear_qi as *const (),
    tear_add_ref as *const (),
    tear_release as *const (),
    lis_get_all as *const (),
    lis_get_distance as *const (),
    lis_get_doppler as *const (),
    lis_get_orientation as *const (),
    lis_get_position as *const (),
    lis_get_rolloff as *const (),
    lis_get_velocity as *const (),
    lis_set_all as *const (),
    lis_set_distance as *const (),
    lis_set_doppler as *const (),
    lis_set_orientation as *const (),
    lis_set_position as *const (),
    lis_set_rolloff as *const (),
    lis_set_velocity as *const (),
    lis_commit as *const (),
]);

// ---------------------------------------------------------------------------
// DirectSoundCapture
// ---------------------------------------------------------------------------

struct Capture {
    initialized: AtomicBool,
}

struct CapState {
    pos: usize,
    capturing: bool,
    looping: bool,
    conv: CaptureConverter,
    queue: Option<Arc<CaptureQueue>>,
    notifies: Vec<(u32, usize)>,
}

struct CapShared {
    st: Mutex<CapState>,
    mem: Arc<Memory>,
}

impl audio::Source for CapShared {
    fn mix(&self, _out: &mut [f32]) -> bool {
        let mut events = Vec::new();
        {
            let mut st = lock(&self.st);
            if !st.capturing {
                return true;
            }
            let Some(q) = st.queue.clone() else {
                return true;
            };
            let input = q.take(q.available());
            let bytes = st.conv.convert(&input);
            let len = self.mem.len;
            for chunk in bytes.chunks(st.conv.format.block as usize) {
                if !st.capturing {
                    break;
                }
                let old = st.pos;
                let n = chunk.len().min(len - old);
                unsafe {
                    std::ptr::copy_nonoverlapping(chunk.as_ptr(), self.mem.ptr.add(old), n);
                }
                let mut new = old + n;
                for &(off, ev) in &st.notifies {
                    if off != DSBPN_OFFSETSTOP && (off as usize) >= old && (off as usize) < new {
                        events.push(ev);
                    }
                }
                if new >= len {
                    new = 0;
                    if !st.looping {
                        st.capturing = false;
                        st.queue = None;
                        events.extend(
                            st.notifies
                                .iter()
                                .filter(|(o, _)| *o == DSBPN_OFFSETSTOP)
                                .map(|(_, e)| *e),
                        );
                    }
                }
                st.pos = new;
            }
        }
        for e in events {
            signal(e);
        }
        true
    }
}

struct CapBuffer {
    sh: Arc<CapShared>,
    flags: u32,
    source: u64,
    notify: Iface,
}

impl Drop for CapBuffer {
    fn drop(&mut self) {
        audio::remove_source(self.source);
    }
}

fn new_capture() -> *mut Com<Capture> {
    Com::create(
        DSC_VT.ptr(),
        Capture {
            initialized: AtomicBool::new(false),
        },
    )
}

unsafe extern "win64" fn dsc_qi(
    this: *mut Com<Capture>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    match read_guid(riid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IDIRECTSOUNDCAPTURE => {
            Com::add_ref(this);
            put(out, this)
        }
        _ => {
            if !out.is_null() {
                *out = std::ptr::null_mut();
            }
            E_NOINTERFACE
        }
    }
}
unsafe extern "win64" fn dsc_add_ref(this: *mut Com<Capture>) -> u32 {
    Com::add_ref(this)
}
unsafe extern "win64" fn dsc_release(this: *mut Com<Capture>) -> u32 {
    Com::release(this)
}

// DSCBUFFERDESC : dwSize @0, dwFlags @4, dwBufferBytes @8, lpwfxFormat @16.
unsafe extern "win64" fn dsc_create_buffer(
    this: *mut Com<Capture>,
    desc: *const u8,
    out: *mut *mut c_void,
    outer: *mut c_void,
) -> i32 {
    if !Com::get(this).initialized.load(Ordering::Acquire) {
        return DSERR_UNINITIALIZED;
    }
    if desc.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    if !outer.is_null() {
        return CLASS_E_NOAGGREGATION;
    }
    let size = *(desc as *const u32);
    if size != 24 && size != 40 {
        return E_INVALIDARG;
    }
    let flags = *(desc.add(4) as *const u32);
    let bytes = *(desc.add(8) as *const u32) as usize;
    let Some(fmt) = Format::from_ptr(*(desc.add(16) as *const *const u8)) else {
        return DSERR_BADFORMAT;
    };
    if bytes < fmt.block as usize || bytes > 0x0fff_ffff {
        return E_INVALIDARG;
    }
    let len = bytes - bytes % fmt.block as usize;
    let Some(mem) = Memory::new(len, fmt.silence_byte()) else {
        return E_OUTOFMEMORY;
    };
    let sh = Arc::new(CapShared {
        st: Mutex::new(CapState {
            pos: 0,
            capturing: false,
            looping: false,
            conv: CaptureConverter::new(fmt),
            queue: None,
            notifies: Vec::new(),
        }),
        mem,
    });
    let source = audio::add_source(sh.clone());
    let b = Com::create(
        CAPBUF_VT.ptr(),
        CapBuffer {
            sh,
            flags,
            source,
            notify: Iface::new(CAPNOTIFY_VT.ptr()),
        },
    );
    (*b).inner.notify.owner = b as *mut c_void;
    *out = b as *mut c_void;
    S_OK
}

// DSCCAPS : dwSize, dwFlags, dwFormats, dwChannels.
unsafe extern "win64" fn dsc_get_caps(this: *mut Com<Capture>, caps: *mut u32) -> i32 {
    if !Com::get(this).initialized.load(Ordering::Acquire) {
        return DSERR_UNINITIALIZED;
    }
    if caps.is_null() || *caps != 16 {
        return E_INVALIDARG;
    }
    *caps.add(1) = 0;
    *caps.add(2) = 0x000f_ffff;
    *caps.add(3) = 2;
    S_OK
}
unsafe extern "win64" fn dsc_initialize(this: *mut Com<Capture>, dev: *const Guid) -> i32 {
    if !known_capture_guid(read_guid(dev)) {
        return DSERR_NODRIVER;
    }
    if Com::get(this).initialized.swap(true, Ordering::AcqRel) {
        return DSERR_ALREADYINITIALIZED;
    }
    S_OK
}
static DSC_VT: VTable<6> = VTable([
    dsc_qi as *const (),
    dsc_add_ref as *const (),
    dsc_release as *const (),
    dsc_create_buffer as *const (),
    dsc_get_caps as *const (),
    dsc_initialize as *const (),
]);

unsafe extern "win64" fn cb_qi(
    this: *mut Com<CapBuffer>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let obj: *mut c_void = match read_guid(riid) {
        Some(g)
            if g == IID_IUNKNOWN
                || g == IID_IDIRECTSOUNDCAPTUREBUFFER
                || g == IID_IDIRECTSOUNDCAPTUREBUFFER8 =>
        {
            this as *mut c_void
        }
        Some(g) if g == IID_IDIRECTSOUNDNOTIFY => {
            &mut (*this).inner.notify as *mut Iface as *mut c_void
        }
        _ => return E_NOINTERFACE,
    };
    Com::add_ref(this);
    *out = obj;
    S_OK
}
unsafe extern "win64" fn cb_add_ref(this: *mut Com<CapBuffer>) -> u32 {
    Com::add_ref(this)
}
unsafe extern "win64" fn cb_release(this: *mut Com<CapBuffer>) -> u32 {
    Com::release(this)
}
// DSCBCAPS : dwSize, dwFlags, dwBufferBytes, dwReserved.
unsafe extern "win64" fn cb_get_caps(this: *mut Com<CapBuffer>, caps: *mut u32) -> i32 {
    if caps.is_null() || *caps != 16 {
        return E_INVALIDARG;
    }
    let b = Com::get(this);
    *caps.add(1) = b.flags;
    *caps.add(2) = b.sh.mem.len as u32;
    *caps.add(3) = 0;
    S_OK
}
unsafe extern "win64" fn cb_get_position(
    this: *mut Com<CapBuffer>,
    cap: *mut u32,
    read: *mut u32,
) -> i32 {
    let pos = lock(&Com::get(this).sh.st).pos as u32;
    if !cap.is_null() {
        *cap = pos;
    }
    if !read.is_null() {
        *read = pos;
    }
    S_OK
}
unsafe extern "win64" fn cb_get_format(
    this: *mut Com<CapBuffer>,
    wfx: *mut u8,
    size: u32,
    written: *mut u32,
) -> i32 {
    let f = lock(&Com::get(this).sh.st).conv.format;
    copy_format(&f, wfx, size, written)
}
unsafe extern "win64" fn cb_get_status(this: *mut Com<CapBuffer>, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let st = lock(&Com::get(this).sh.st);
    *out = u32::from(st.capturing) | if st.capturing && st.looping { 2 } else { 0 };
    S_OK
}
unsafe extern "win64" fn cb_initialize(
    _this: *mut Com<CapBuffer>,
    _c: *mut c_void,
    _d: *const u8,
) -> i32 {
    DSERR_ALREADYINITIALIZED
}
unsafe extern "win64" fn cb_lock(
    this: *mut Com<CapBuffer>,
    offset: u32,
    mut bytes: u32,
    p1: *mut *mut u8,
    n1: *mut u32,
    p2: *mut *mut u8,
    n2: *mut u32,
    flags: u32,
) -> i32 {
    let b = Com::get(this);
    if p1.is_null() || n1.is_null() {
        return E_INVALIDARG;
    }
    let len = b.sh.mem.len as u32;
    if flags & 1 != 0 {
        bytes = len; // DSCBLOCK_ENTIREBUFFER
    }
    if offset >= len || bytes == 0 || bytes > len {
        return E_INVALIDARG;
    }
    let first = bytes.min(len - offset);
    *p1 = b.sh.mem.ptr.add(offset as usize);
    *n1 = first;
    let rest = bytes - first;
    if !p2.is_null() {
        *p2 = if rest > 0 {
            b.sh.mem.ptr
        } else {
            std::ptr::null_mut()
        };
    }
    if !n2.is_null() {
        *n2 = if p2.is_null() { 0 } else { rest };
    }
    S_OK
}
unsafe extern "win64" fn cb_start(this: *mut Com<CapBuffer>, flags: u32) -> i32 {
    let b = Com::get(this);
    let mut st = lock(&b.sh.st);
    st.looping = flags & 1 != 0;
    if !st.capturing {
        st.capturing = true;
        st.queue = Some(audio::subscribe_microphone());
    }
    S_OK
}
unsafe extern "win64" fn cb_stop(this: *mut Com<CapBuffer>) -> i32 {
    let b = Com::get(this);
    let mut st = lock(&b.sh.st);
    st.capturing = false;
    st.queue = None;
    S_OK
}
unsafe extern "win64" fn cb_unlock(
    _this: *mut Com<CapBuffer>,
    p1: *mut u8,
    _n1: u32,
    _p2: *mut u8,
    _n2: u32,
) -> i32 {
    if p1.is_null() {
        E_INVALIDARG
    } else {
        S_OK
    }
}
unsafe extern "win64" fn cb_get_object(
    _this: *mut Com<CapBuffer>,
    _g: *const Guid,
    _i: u32,
    _iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    DSERR_OBJECTNOTFOUND
}
unsafe extern "win64" fn cb_get_fx_status(
    _this: *mut Com<CapBuffer>,
    count: u32,
    results: *mut u32,
) -> i32 {
    if !results.is_null() {
        for i in 0..count as usize {
            *results.add(i) = 5;
        }
    }
    S_OK
}
static CAPBUF_VT: VTable<14> = VTable([
    cb_qi as *const (),
    cb_add_ref as *const (),
    cb_release as *const (),
    cb_get_caps as *const (),
    cb_get_position as *const (),
    cb_get_format as *const (),
    cb_get_status as *const (),
    cb_initialize as *const (),
    cb_lock as *const (),
    cb_start as *const (),
    cb_stop as *const (),
    cb_unlock as *const (),
    cb_get_object as *const (),
    cb_get_fx_status as *const (),
]);

unsafe fn cap_owner(this: *mut Iface) -> *mut Com<CapBuffer> {
    (*this).owner as *mut Com<CapBuffer>
}
unsafe extern "win64" fn capn_qi(
    this: *mut Iface,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    cb_qi(cap_owner(this), riid, out)
}
unsafe extern "win64" fn capn_add_ref(this: *mut Iface) -> u32 {
    Com::add_ref(cap_owner(this))
}
unsafe extern "win64" fn capn_release(this: *mut Iface) -> u32 {
    Com::release(cap_owner(this))
}
unsafe extern "win64" fn capn_set(this: *mut Iface, count: u32, list: *const u8) -> i32 {
    let b = Com::get(cap_owner(this));
    if count > 0 && list.is_null() {
        return E_INVALIDARG;
    }
    let mut st = lock(&b.sh.st);
    if st.capturing {
        return DSERR_INVALIDCALL;
    }
    let mut v = Vec::new();
    for i in 0..count as usize {
        let off = *(list.add(i * 16) as *const u32);
        if off != DSBPN_OFFSETSTOP && off as usize >= b.sh.mem.len {
            return E_INVALIDARG;
        }
        v.push((off, *(list.add(i * 16 + 8) as *const usize)));
    }
    st.notifies = v;
    S_OK
}
static CAPNOTIFY_VT: VTable<4> = VTable([
    capn_qi as *const (),
    capn_add_ref as *const (),
    capn_release as *const (),
    capn_set as *const (),
]);

// ---------------------------------------------------------------------------
// Exports et création COM
// ---------------------------------------------------------------------------

unsafe fn create_initialized<T>(
    obj: *mut Com<T>,
    init: unsafe extern "win64" fn(*mut Com<T>, *const Guid) -> i32,
    dev: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    let hr = init(obj, dev);
    if hr != S_OK {
        Com::release(obj);
        return hr;
    }
    *out = obj as *mut c_void;
    S_OK
}

pub unsafe extern "win64" fn direct_sound_create(
    dev: *const Guid,
    out: *mut *mut c_void,
    outer: *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    if !outer.is_null() {
        return DSERR_INVALIDCALL;
    }
    create_initialized(new_device(), ds_initialize, dev, out)
}

pub unsafe extern "win64" fn direct_sound_capture_create(
    dev: *const Guid,
    out: *mut *mut c_void,
    outer: *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    if !outer.is_null() {
        return DSERR_INVALIDCALL;
    }
    create_initialized(new_capture(), dsc_initialize, dev, out)
}

type EnumA = unsafe extern "win64" fn(*const Guid, *const u8, *const u8, usize) -> i32;
type EnumW = unsafe extern "win64" fn(*const Guid, *const u16, *const u16, usize) -> i32;

unsafe fn enumerate_a(cb: usize, ctx: usize, primary: &str, dev: &Guid, name: &str) -> i32 {
    if cb == 0 {
        return E_INVALIDARG;
    }
    audio::ensure_windows_thread();
    let f: EnumA = std::mem::transmute(cb);
    let mut p = super::encode_ansi(primary);
    p.push(0);
    let mut n = super::encode_ansi(name);
    n.push(0);
    let empty = [0u8];
    if f(std::ptr::null(), p.as_ptr(), empty.as_ptr(), ctx) != 0 {
        f(dev, n.as_ptr(), c"winlux".as_ptr() as *const u8, ctx);
    }
    S_OK
}
unsafe fn enumerate_w(cb: usize, ctx: usize, primary: &str, dev: &Guid, name: &str) -> i32 {
    if cb == 0 {
        return E_INVALIDARG;
    }
    let f: EnumW = std::mem::transmute(cb);
    let w = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (p, n, m, e) = (w(primary), w(name), w("winlux"), w(""));
    if f(std::ptr::null(), p.as_ptr(), e.as_ptr(), ctx) != 0 {
        f(dev, n.as_ptr(), m.as_ptr(), ctx);
    }
    S_OK
}
pub unsafe extern "win64" fn direct_sound_enumerate_a(cb: usize, ctx: usize) -> i32 {
    enumerate_a(
        cb,
        ctx,
        "Pilote audio principal",
        &WINLUX_RENDER_GUID,
        RENDER_NAME,
    )
}
pub unsafe extern "win64" fn direct_sound_enumerate_w(cb: usize, ctx: usize) -> i32 {
    enumerate_w(
        cb,
        ctx,
        "Pilote audio principal",
        &WINLUX_RENDER_GUID,
        RENDER_NAME,
    )
}
pub unsafe extern "win64" fn direct_sound_capture_enumerate_a(cb: usize, ctx: usize) -> i32 {
    enumerate_a(
        cb,
        ctx,
        "Pilote de capture principal",
        &WINLUX_CAPTURE_GUID,
        CAPTURE_NAME,
    )
}
pub unsafe extern "win64" fn direct_sound_capture_enumerate_w(cb: usize, ctx: usize) -> i32 {
    enumerate_w(
        cb,
        ctx,
        "Pilote de capture principal",
        &WINLUX_CAPTURE_GUID,
        CAPTURE_NAME,
    )
}

pub unsafe extern "win64" fn get_device_id(src: *const Guid, dst: *mut Guid) -> i32 {
    if dst.is_null() {
        return E_INVALIDARG;
    }
    let g = read_guid(src).unwrap_or(DSDEVID_DEFAULTPLAYBACK);
    *dst = if g == DSDEVID_DEFAULTPLAYBACK || g == DSDEVID_DEFAULTVOICEPLAYBACK {
        WINLUX_RENDER_GUID
    } else if g == DSDEVID_DEFAULTCAPTURE || g == DSDEVID_DEFAULTVOICECAPTURE {
        WINLUX_CAPTURE_GUID
    } else if g == WINLUX_RENDER_GUID || g == WINLUX_CAPTURE_GUID {
        g
    } else {
        return DSERR_NODRIVER;
    };
    S_OK
}

unsafe fn com_create<T>(
    obj: *mut Com<T>,
    qi: unsafe extern "win64" fn(*mut Com<T>, *const Guid, *mut *mut c_void) -> i32,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    let hr = qi(obj, riid, out);
    Com::release(obj);
    hr
}
unsafe fn create_ds(riid: *const Guid, out: *mut *mut c_void) -> i32 {
    com_create(new_device(), ds_qi, riid, out)
}
unsafe fn create_dsc(riid: *const Guid, out: *mut *mut c_void) -> i32 {
    com_create(new_capture(), dsc_qi, riid, out)
}

/// Classes DirectSound créables par CoCreateInstance (non initialisées).
pub fn creator(clsid: &Guid) -> Option<Creator> {
    if *clsid == CLSID_DIRECTSOUND || *clsid == CLSID_DIRECTSOUND8 {
        Some(create_ds)
    } else if *clsid == CLSID_DIRECTSOUNDCAPTURE || *clsid == CLSID_DIRECTSOUNDCAPTURE8 {
        Some(create_dsc)
    } else {
        None
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "DirectSoundCreate"=>fn direct_sound_create,"DirectSoundCreate8"=>fn direct_sound_create,
        "DirectSoundCaptureCreate"=>fn direct_sound_capture_create,"DirectSoundCaptureCreate8"=>fn direct_sound_capture_create,
        "DirectSoundEnumerateA"=>fn direct_sound_enumerate_a,"DirectSoundEnumerateW"=>fn direct_sound_enumerate_w,
        "DirectSoundCaptureEnumerateA"=>fn direct_sound_capture_enumerate_a,"DirectSoundCaptureEnumerateW"=>fn direct_sound_capture_enumerate_w,
        "GetDeviceID"=>fn get_device_id,
        "DllGetClassObject"=>fn dll_get_class_object,"DllCanUnloadNow"=>fn dll_can_unload_now,
    }
}

#[cfg(test)]
mod tests {
    #[repr(C, align(8))]
    struct Al<const N: usize>([u8; N]);
    impl<const N: usize> std::ops::Deref for Al<N> {
        type Target = [u8; N];
        fn deref(&self) -> &[u8; N] {
            &self.0
        }
    }
    impl<const N: usize> std::ops::DerefMut for Al<N> {
        fn deref_mut(&mut self) -> &mut [u8; N] {
            &mut self.0
        }
    }
    use super::*;
    use crate::audio::format::Sample;

    unsafe fn vcall<R>(obj: *mut c_void, idx: usize) -> R
    where
        R: Copy,
    {
        let vt = *(obj as *const *const usize);
        let f: unsafe extern "win64" fn(*mut c_void) -> R = std::mem::transmute(*vt.add(idx));
        f(obj)
    }

    #[test]
    fn tampon_secondaire_joue_boucle_et_notifie() {
        std::env::set_var("WINLUX_AUDIO", "null");
        unsafe {
            let mut ds: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                direct_sound_create(std::ptr::null(), &mut ds, std::ptr::null_mut()),
                S_OK
            );
            assert_eq!(ds_set_coop(ds as _, 0, DSSCL_PRIORITY), S_OK);
            // Tampon primaire avec auditeur 3D.
            let mut pdesc = Al([0u8; 36]);
            pdesc[0..4].copy_from_slice(&36u32.to_le_bytes());
            pdesc[4..8].copy_from_slice(
                &(DSBCAPS_PRIMARYBUFFER | DSBCAPS_CTRL3D | DSBCAPS_CTRLVOLUME).to_le_bytes(),
            );
            let mut prim: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                ds_create_sound_buffer(ds as _, pdesc.as_ptr(), &mut prim, std::ptr::null_mut()),
                S_OK
            );
            let mut lis: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                buf_qi(prim as _, &IID_IDIRECTSOUND3DLISTENER, &mut lis),
                S_OK
            );
            let fmt = Format::new(Sample::S16, 2, 22050).unwrap().to_bytes();
            assert_eq!(buf_set_format(prim as _, fmt.as_ptr()), S_OK);
            // Secondaire 0,1 s, mono 16 bits, 3D + notifications.
            let mono = Format::new(Sample::S16, 1, 22050).unwrap().to_bytes();
            let mut desc = Al([0u8; 36]);
            desc[0..4].copy_from_slice(&36u32.to_le_bytes());
            let flags = DSBCAPS_CTRL3D
                | DSBCAPS_CTRLPOSITIONNOTIFY
                | DSBCAPS_CTRLVOLUME
                | DSBCAPS_CTRLFREQUENCY;
            desc[4..8].copy_from_slice(&flags.to_le_bytes());
            desc[8..12].copy_from_slice(&4410u32.to_le_bytes());
            desc[16..24].copy_from_slice(&(mono.as_ptr() as usize).to_le_bytes());
            let mut buf: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                ds_create_sound_buffer(ds as _, desc.as_ptr(), &mut buf, std::ptr::null_mut()),
                S_OK
            );
            let (mut p1, mut n1, mut p2, mut n2) =
                (std::ptr::null_mut(), 0u32, std::ptr::null_mut(), 0u32);
            assert_eq!(
                buf_lock(buf as _, 4000, 1000, &mut p1, &mut n1, &mut p2, &mut n2, 0),
                S_OK
            );
            assert_eq!((n1, n2), (410, 590));
            assert_eq!(buf_unlock(buf as _, p1, n1, p2, n2), S_OK);
            let mut notify: *mut c_void = std::ptr::null_mut();
            assert_eq!(buf_qi(buf as _, &IID_IDIRECTSOUNDNOTIFY, &mut notify), S_OK);
            let ev = crate::dlls::kernel32::create_event_w(0, 1, 0, std::ptr::null());
            let mut list = Al([0u8; 16]);
            list[0..4].copy_from_slice(&2000u32.to_le_bytes());
            list[8..16].copy_from_slice(&ev.to_le_bytes());
            assert_eq!(notify_set(notify as _, 1, list.as_ptr()), S_OK);
            assert_eq!(buf_set_frequency(buf as _, 44100), S_OK);
            assert_eq!(buf_play(buf as _, 0, 0, DSBPLAY_LOOPING), S_OK);
            assert_eq!(notify_set(notify as _, 1, list.as_ptr()), DSERR_INVALIDCALL);
            assert_eq!(crate::dlls::kernel32::wait_for_single_object(ev, 3000), 0);
            let mut status = 0;
            buf_get_status(buf as _, &mut status);
            assert_eq!(status & 5, 5);
            assert_eq!(buf_stop(buf as _), S_OK);
            assert_eq!(buf_set_pan(buf as _, 0), DSERR_CONTROLUNAVAIL);
            let _: u32 = vcall(notify, 2);
            let _: u32 = vcall(lis, 2);
            let _: u32 = vcall(buf, 2);
            let _: u32 = vcall(prim, 2);
            assert_eq!(vcall::<u32>(ds, 2), 0);
            crate::dlls::kernel32::close_handle(ev);
        }
    }
}
