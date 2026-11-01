//! Décodage WAVE PCM/IEEE-float, MP3/AAC et pont de sortie vers ALSA (Linux), sans
//! DLL Windows ni framework de compatibilité tiers.

use std::ffi::{c_char, c_int, c_void, CString};
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

#[link(name = "dl")]
unsafe extern "C" {
    fn dlopen(name: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
}

type PcmOpen = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
type PcmSetParams = unsafe extern "C" fn(*mut c_void, c_int, c_int, u32, u32, c_int, u32) -> c_int;
type PcmWrite = unsafe extern "C" fn(*mut c_void, *const c_void, usize) -> isize;
type PcmRecover = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;
type PcmDrain = unsafe extern "C" fn(*mut c_void) -> c_int;
type PcmDrop = unsafe extern "C" fn(*mut c_void) -> c_int;
type PcmClose = unsafe extern "C" fn(*mut c_void) -> c_int;

struct Alsa {
    lib: *mut c_void,
    open: PcmOpen,
    set_params: PcmSetParams,
    writei: PcmWrite,
    recover: PcmRecover,
    drain: PcmDrain,
    drop_pcm: PcmDrop,
    close: PcmClose,
}
unsafe impl Send for Alsa {}
unsafe impl Sync for Alsa {}
impl Drop for Alsa {
    fn drop(&mut self) {
        unsafe {
            dlclose(self.lib);
        }
    }
}
fn alsa() -> Result<Alsa, ()> {
    unsafe {
        let lib = dlopen(c"libasound.so.2".as_ptr(), 2);
        if lib.is_null() {
            return Err(());
        }
        macro_rules! sym {
            ($n:literal,$t:ty) => {{
                let p = dlsym(lib, concat!($n, "\0").as_ptr() as *const c_char);
                if p.is_null() {
                    dlclose(lib);
                    return Err(());
                }
                std::mem::transmute::<*mut c_void, $t>(p)
            }};
        }
        Ok(Alsa {
            lib,
            open: sym!("snd_pcm_open", PcmOpen),
            set_params: sym!("snd_pcm_set_params", PcmSetParams),
            writei: sym!("snd_pcm_writei", PcmWrite),
            recover: sym!("snd_pcm_recover", PcmRecover),
            drain: sym!("snd_pcm_drain", PcmDrain),
            drop_pcm: sym!("snd_pcm_drop", PcmDrop),
            close: sym!("snd_pcm_close", PcmClose),
        })
    }
}

struct Wave {
    channels: u16,
    rate: u32,
    samples: Vec<i16>,
}
#[path = "winmm_codecs.rs"]
mod codecs;
fn le16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
fn le32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn decode_wave(b: &[u8]) -> Option<Wave> {
    if b.len() < 12 || &b[..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return None;
    }
    let riff_end = 8usize.checked_add(le32(b, 4)? as usize)?;
    if riff_end > b.len() || riff_end < 12 {
        return None;
    }
    let mut off = 12usize;
    let (mut fmt, mut data) = (None, None);
    while off.checked_add(8)? <= riff_end {
        let id = &b[off..off + 4];
        let len = le32(b, off + 4)? as usize;
        let start = off + 8;
        let end = start.checked_add(len)?;
        if end > riff_end {
            return None;
        }
        if id == b"fmt " {
            if len < 16 {
                return None;
            }
            fmt = Some((
                le16(b, start)?,
                le16(b, start + 2)?,
                le32(b, start + 4)?,
                le16(b, start + 12)?,
                le16(b, start + 14)?,
            ));
        }
        if id == b"data" {
            data = Some(&b[start..end]);
        }
        off = end.checked_add(len & 1)?;
    }
    let (format, channels, rate, block, bits) = fmt?;
    let src = data?;
    if !(channels == 1 || channels == 2)
        || rate == 0
        || !(format == 1 || format == 3)
        || !(bits == 8 || bits == 16 || bits == 24 || bits == 32)
    {
        return None;
    }
    if format == 3 && bits != 32 {
        return None;
    }
    let bytes_per = (bits / 8) as usize;
    if block as usize != (channels as usize) * bytes_per || src.len() % (block as usize) != 0 {
        return None;
    }
    let mut samples = Vec::with_capacity(src.len() / bytes_per);
    for s in src.chunks_exact(bytes_per) {
        let v = match (format, bits) {
            (1, 8) => (s[0] as i32 - 128) << 8,
            (1, 16) => i16::from_le_bytes([s[0], s[1]]) as i32,
            (1, 24) => {
                let mut n = (s[0] as i32) | ((s[1] as i32) << 8) | ((s[2] as i32) << 16);
                if n & 0x800000 != 0 {
                    n |= !0xffffff;
                }
                n >> 8
            }
            (1, 32) => i32::from_le_bytes(s.try_into().ok()?) >> 16,
            (3, 32) => {
                let f = f32::from_bits(u32::from_le_bytes(s.try_into().ok()?));
                if !f.is_finite() {
                    0
                } else {
                    (f.clamp(-1.0, 1.0) * 32767.0) as i32
                }
            }
            _ => return None,
        };
        samples.push(v.clamp(i16::MIN as i32, i16::MAX as i32) as i16);
    }
    Some(Wave {
        channels,
        rate,
        samples,
    })
}

fn play_pcm(wave: Wave) -> Result<(), u32> {
    let a = alsa().map_err(|_| 50u32)?;
    let default = CString::new("default").unwrap();
    let mut pcm = std::ptr::null_mut();
    let rc = unsafe { (a.open)(&mut pcm, default.as_ptr(), 0, 0) };
    if rc < 0 {
        return Err(31);
    }
    // ALSA: S16_LE=2, RW_INTERLEAVED=3, playback=0.
    let rc = unsafe { (a.set_params)(pcm, 2, 3, wave.channels as u32, wave.rate, 1, 500_000) };
    if rc < 0 {
        unsafe {
            (a.close)(pcm);
        }
        return Err(50);
    }
    let frame_samples = wave.channels as usize;
    let mut offset = 0usize;
    while offset < wave.samples.len() {
        let frames = ((wave.samples.len() - offset) / frame_samples).min(4096);
        let rc = unsafe {
            (a.writei)(
                pcm,
                wave.samples[offset..].as_ptr() as *const c_void,
                frames,
            )
        };
        if rc > 0 {
            offset += rc as usize * frame_samples;
            continue;
        }
        if rc == -11 {
            continue;
        }
        if unsafe { (a.recover)(pcm, rc as c_int, 1) } < 0 {
            unsafe {
                (a.drop_pcm)(pcm);
                (a.close)(pcm);
            }
            return Err(31);
        }
    }
    let rc = unsafe { (a.drain)(pcm) };
    unsafe {
        (a.close)(pcm);
    }
    if rc < 0 {
        Err(31)
    } else {
        Ok(())
    }
}
fn probe_audio(wave: &Wave) -> Result<(), u32> {
    let a = alsa().map_err(|_| 50u32)?;
    let default = CString::new("default").unwrap();
    let mut pcm = std::ptr::null_mut();
    if unsafe { (a.open)(&mut pcm, default.as_ptr(), 0, 0) } < 0 {
        return Err(31);
    }
    let rc = unsafe { (a.set_params)(pcm, 2, 3, wave.channels as u32, wave.rate, 1, 500_000) };
    unsafe {
        (a.close)(pcm);
    }
    if rc < 0 {
        Err(50)
    } else {
        Ok(())
    }
}

const SND_ASYNC: u32 = 0x0001;
const SND_LOOP: u32 = 0x0008;
const SND_MEMORY: u32 = 0x0004;
const SND_PURGE: u32 = 0x0040;
const SND_NOSTOP: u32 = 0x0010;
struct Job {
    cancel: std::sync::Arc<AtomicBool>,
    thread: std::thread::JoinHandle<()>,
}
static JOBS: OnceLock<Mutex<Vec<Job>>> = OnceLock::new();
fn jobs() -> &'static Mutex<Vec<Job>> {
    JOBS.get_or_init(|| Mutex::new(Vec::new()))
}
use super::lock;
fn play_wave_cancellable(wave: Wave, cancel: std::sync::Arc<AtomicBool>) -> Result<(), u32> {
    let a = alsa().map_err(|_| 50u32)?;
    let default = CString::new("default").unwrap();
    let mut pcm = std::ptr::null_mut();
    if unsafe { (a.open)(&mut pcm, default.as_ptr(), 0, 0) } < 0 {
        return Err(31);
    }
    if unsafe { (a.set_params)(pcm, 2, 3, wave.channels as u32, wave.rate, 1, 500_000) } < 0 {
        unsafe {
            (a.close)(pcm);
        }
        return Err(50);
    }
    let frame_samples = wave.channels as usize;
    let mut offset = 0usize;
    while offset < wave.samples.len() && !cancel.load(Ordering::Relaxed) {
        let frames = ((wave.samples.len() - offset) / frame_samples).min(4096);
        let rc = unsafe {
            (a.writei)(
                pcm,
                wave.samples[offset..].as_ptr() as *const c_void,
                frames,
            )
        };
        if rc > 0 {
            offset += rc as usize * frame_samples;
        } else if rc == -11 {
            std::thread::sleep(std::time::Duration::from_millis(2));
        } else if unsafe { (a.recover)(pcm, rc as c_int, 1) } < 0 {
            unsafe {
                (a.drop_pcm)(pcm);
                (a.close)(pcm);
            }
            return Err(31);
        }
    }
    if cancel.load(Ordering::Relaxed) {
        unsafe {
            (a.drop_pcm)(pcm);
        }
    } else {
        unsafe {
            (a.drain)(pcm);
        }
    }
    unsafe {
        (a.close)(pcm);
    }
    Ok(())
}

pub unsafe extern "win64" fn play_sound_a(name: *const u8, _module: usize, flags: u32) -> i32 {
    if flags & SND_PURGE != 0 {
        for j in lock(jobs()).iter() {
            j.cancel.store(true, Ordering::Relaxed);
        }
        return 1;
    }
    if name.is_null()
        || flags & (SND_MEMORY | SND_LOOP) != 0
        || flags
            & !(SND_ASYNC | SND_LOOP | SND_MEMORY | SND_PURGE | SND_NOSTOP | 0x0002 | 0x0002_0000)
            != 0
    {
        crate::process::set_last_error(50);
        return 0;
    }
    play_sound_path(&crate::dlls::cstr(name), flags)
}
fn play_sound_path(name: &str, flags: u32) -> i32 {
    {
        let mut list = lock(jobs());
        list.retain(|j| !j.thread.is_finished());
        if flags & SND_NOSTOP != 0 && !list.is_empty() {
            return 0;
        }
        for j in list.iter() {
            j.cancel.store(true, Ordering::Relaxed);
        }
    }
    let filename = crate::paths::to_linux(name);
    let f = match std::fs::File::open(filename) {
        Ok(f) => f,
        Err(e) => {
            crate::process::set_last_error(if e.kind() == std::io::ErrorKind::NotFound {
                2
            } else {
                5
            });
            return 0;
        }
    };
    let mut raw = Vec::new();
    if f.take((codecs::MAX_INPUT + 1) as u64)
        .read_to_end(&mut raw)
        .is_err()
    {
        crate::process::set_last_error(31);
        return 0;
    }
    let Some(wave) = codecs::decode(&raw) else {
        crate::process::set_last_error(50);
        return 0;
    };
    if flags & SND_ASYNC != 0 {
        if let Err(e) = probe_audio(&wave) {
            crate::process::set_last_error(e);
            return 0;
        }
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let Ok(thread) = std::thread::Builder::new()
            .name("winlux-wave-playback".into())
            .spawn(move || {
                let _ = play_wave_cancellable(wave, worker_cancel);
            })
        else {
            crate::process::set_last_error(8);
            return 0;
        };
        lock(jobs()).push(Job { cancel, thread });
        1
    } else {
        match play_pcm(wave) {
            Ok(()) => 1,
            Err(e) => {
                crate::process::set_last_error(e);
                0
            }
        }
    }
}
pub unsafe extern "win64" fn play_sound_w(name: *const u16, _module: usize, flags: u32) -> i32 {
    if name.is_null() || flags & SND_PURGE != 0 {
        return play_sound_a(std::ptr::null(), 0, flags);
    }
    let n = crate::printf::wide_len(name);
    let s = match String::from_utf16(std::slice::from_raw_parts(name, n)) {
        Ok(s) => s,
        Err(_) => {
            crate::process::set_last_error(1113);
            return 0;
        }
    };
    if flags & (SND_MEMORY | SND_LOOP) != 0
        || flags
            & !(SND_ASYNC | SND_LOOP | SND_MEMORY | SND_PURGE | SND_NOSTOP | 0x0002 | 0x0002_0000)
            != 0
    {
        crate::process::set_last_error(50);
        return 0;
    }
    play_sound_path(&s, flags)
}
pub unsafe extern "win64" fn mci_send_string_a(
    _cmd: *const u8,
    _ret: *mut u8,
    _len: u32,
    _callback: usize,
) -> u32 {
    crate::process::set_last_error(50);
    277
}
pub extern "win64" fn time_get_time() -> u32 {
    crate::dlls::kernel32::get_tick_count()
}

// ---------------------------------------------------------------------------
// waveIn* – capture audio via ALSA (Linux), exposée comme l'API Win32
// ---------------------------------------------------------------------------

type PcmRead = unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> isize;
type PcmStart = unsafe extern "C" fn(*mut c_void) -> c_int;

struct AlsaCapture {
    lib: *mut c_void,
    open: PcmOpen,
    set_params: PcmSetParams,
    readi: PcmRead,
    start: PcmStart,
    recover: PcmRecover,
    drop_pcm: PcmDrop,
    close: PcmClose,
}
unsafe impl Send for AlsaCapture {}
unsafe impl Sync for AlsaCapture {}
impl Drop for AlsaCapture {
    fn drop(&mut self) {
        unsafe {
            dlclose(self.lib);
        }
    }
}
fn alsa_capture() -> Result<AlsaCapture, ()> {
    unsafe {
        let lib = dlopen(c"libasound.so.2".as_ptr(), 2);
        if lib.is_null() {
            return Err(());
        }
        macro_rules! sym {
            ($n:literal,$t:ty) => {{
                let p = dlsym(lib, concat!($n, "\0").as_ptr() as *const c_char);
                if p.is_null() {
                    dlclose(lib);
                    return Err(());
                }
                std::mem::transmute::<*mut c_void, $t>(p)
            }};
        }
        Ok(AlsaCapture {
            lib,
            open: sym!("snd_pcm_open", PcmOpen),
            set_params: sym!("snd_pcm_set_params", PcmSetParams),
            readi: sym!("snd_pcm_readi", PcmRead),
            start: sym!("snd_pcm_start", PcmStart),
            recover: sym!("snd_pcm_recover", PcmRecover),
            drop_pcm: sym!("snd_pcm_drop", PcmDrop),
            close: sym!("snd_pcm_close", PcmClose),
        })
    }
}

struct WaveHdrPtr(*mut u8);
unsafe impl Send for WaveHdrPtr {}

/// Format de capture demandé par le programme.
#[derive(Clone, Copy, Debug, PartialEq)]
struct RecordConfig {
    channels: u16,
    rate: u32,
    /// 8 (U8), 16 (S16_LE) ou 32 (flottant IEEE).
    bits: u16,
}

impl RecordConfig {
    fn alsa_format(&self) -> c_int {
        match self.bits {
            8 => 1,   // SND_PCM_FORMAT_U8
            32 => 14, // SND_PCM_FORMAT_FLOAT_LE
            _ => 2,   // SND_PCM_FORMAT_S16_LE
        }
    }
    fn block_align(&self) -> usize {
        self.channels as usize * (self.bits as usize / 8)
    }
}

/// Lit un WAVEFORMATEX (PCM, flottant ou WAVE_FORMAT_EXTENSIBLE).
unsafe fn parse_wave_format(pwfx: *const u8) -> Option<RecordConfig> {
    let tag = (pwfx as *const u16).read_unaligned();
    let channels = (pwfx.add(2) as *const u16).read_unaligned();
    let rate = (pwfx.add(4) as *const u32).read_unaligned();
    let bits = (pwfx.add(14) as *const u16).read_unaligned();
    let float = match tag {
        1 => false,
        3 => true,
        0xFFFE => {
            // Sous-format : GUID dont le premier mot vaut 1 (PCM) ou 3 (float).
            let extra = (pwfx.add(16) as *const u16).read_unaligned();
            if extra < 22 {
                return None;
            }
            match (pwfx.add(24) as *const u32).read_unaligned() {
                1 => false,
                3 => true,
                _ => return None,
            }
        }
        _ => return None,
    };
    let ok_bits = if float {
        bits == 32
    } else {
        bits == 8 || bits == 16
    };
    if !(1..=2).contains(&channels) || !(4000..=192_000).contains(&rate) || !ok_bits {
        return None;
    }
    Some(RecordConfig {
        channels,
        rate,
        bits,
    })
}

// Mécanismes de notification de waveInOpen (fdwOpen & CALLBACK_TYPEMASK).
const CALLBACK_TYPEMASK: u32 = 0x0007_0000;
const CALLBACK_WINDOW: u32 = 0x0001_0000;
const CALLBACK_FUNCTION: u32 = 0x0003_0000;
const CALLBACK_EVENT: u32 = 0x0005_0000;
const WAVE_FORMAT_QUERY: u32 = 0x0001;
const WIM_OPEN: u32 = 0x3BE;
const WIM_CLOSE: u32 = 0x3BF;
const WIM_DATA: u32 = 0x3C0;

/// Destinataire des messages WIM_* du programme.
#[derive(Clone, Copy)]
struct Notify {
    kind: u32,
    target: usize,
    instance: usize,
    handle: usize,
}

impl Notify {
    /// Envoie un message ; `param1` est le WAVEHDR pour WIM_DATA.
    unsafe fn send(&self, msg: u32, param1: usize) {
        match self.kind {
            CALLBACK_FUNCTION if self.target != 0 => {
                let f: unsafe extern "win64" fn(usize, u32, usize, usize, usize) =
                    std::mem::transmute(self.target);
                f(self.handle, msg, self.instance, param1, 0);
            }
            CALLBACK_EVENT if self.target != 0 => {
                crate::dlls::kernel32::set_event(self.target);
            }
            CALLBACK_WINDOW if self.target != 0 => {
                crate::dlls::user32::post_message_a(self.target, msg, self.handle, param1 as isize);
            }
            _ => {}
        }
    }
}

struct RecordState {
    config: RecordConfig,
    notify: Notify,
    buffers: std::sync::Arc<Mutex<Vec<WaveHdrPtr>>>,
    cancel: std::sync::Arc<AtomicBool>,
    /// Octets capturés depuis waveInOpen (waveInGetPosition).
    captured: std::sync::Arc<std::sync::atomic::AtomicU64>,
    thread: Option<std::thread::JoinHandle<()>>,
}

static RECORD: OnceLock<Mutex<Option<RecordState>>> = OnceLock::new();
fn record_lock() -> &'static Mutex<Option<RecordState>> {
    RECORD.get_or_init(|| Mutex::new(None))
}

/// HWAVEIN rendu au programme (un seul périphérique de capture ouvert).
const HWAVEIN: usize = 0x5749_4E00;

const WHDR_DONE: u32 = 0x01;
const WHDR_PREPARED: u32 = 0x02;
const WHDR_INQUEUE: u32 = 0x10;

unsafe fn wavehdr_data_ptr(pwh: *mut u8) -> *mut u8 {
    *(pwh as *const *mut u8)
}
unsafe fn wavehdr_buffer_length(pwh: *mut u8) -> u32 {
    *((pwh as *const u8).add(8) as *const u32)
}
unsafe fn wavehdr_bytes_recorded(pwh: *mut u8) -> *mut u32 {
    pwh.add(12) as *mut u32
}
unsafe fn wavehdr_flags(pwh: *mut u8) -> *mut u32 {
    pwh.add(24) as *mut u32
}

/// Marque un tampon terminé et prévient le programme.
unsafe fn finish_buffer(pwh: *mut u8, notify: &Notify) {
    *wavehdr_flags(pwh) = (*wavehdr_flags(pwh) & !WHDR_INQUEUE) | WHDR_DONE;
    notify.send(WIM_DATA, pwh as usize);
}

fn capture_thread(
    config: RecordConfig,
    notify: Notify,
    buffers: std::sync::Arc<Mutex<Vec<WaveHdrPtr>>>,
    cancel: std::sync::Arc<AtomicBool>,
    captured: std::sync::Arc<std::sync::atomic::AtomicU64>,
) {
    if notify.kind == CALLBACK_FUNCTION {
        // La fonction de rappel est du code Windows : il lui faut un TEB.
        crate::process::init_thread();
    }
    let Ok(a) = alsa_capture() else { return };
    let default = CString::new("default").unwrap();
    let mut pcm = std::ptr::null_mut();
    // mode 1 = SND_PCM_STREAM_CAPTURE
    if unsafe { (a.open)(&mut pcm, default.as_ptr(), 1, 0) } < 0 {
        return;
    }
    // RW_INTERLEAVED=3, rééchantillonnage autorisé, 0,5 s de latence max.
    let fmt = config.alsa_format();
    let ch = config.channels as u32;
    if unsafe { (a.set_params)(pcm, fmt, 3, ch, config.rate, 1, 500_000) } < 0 {
        unsafe {
            (a.close)(pcm);
        }
        return;
    }
    if unsafe { (a.start)(pcm) } < 0 {
        unsafe {
            (a.close)(pcm);
        }
        return;
    }

    let bytes_per_frame = config.block_align();
    let frames = (config.rate as usize / 50).max(64); // 20 ms
    let mut local_buf = vec![0u8; frames * bytes_per_frame];

    while !cancel.load(Ordering::Relaxed) {
        let rc = unsafe { (a.readi)(pcm, local_buf.as_mut_ptr() as *mut c_void, frames) };
        if rc > 0 {
            let captured_bytes = rc as usize * bytes_per_frame;
            captured.fetch_add(captured_bytes as u64, Ordering::Relaxed);
            let src = &local_buf[..captured_bytes];
            let mut src_off = 0usize;
            while src_off < captured_bytes {
                let mut list = lock(&buffers);
                if list.is_empty() {
                    break;
                }
                let pwh = list[0].0;
                let full = unsafe {
                    let buf_ptr = wavehdr_data_ptr(pwh);
                    let buf_len = wavehdr_buffer_length(pwh) as usize;
                    let recorded = *wavehdr_bytes_recorded(pwh) as usize;
                    let space = buf_len.saturating_sub(recorded);
                    let copy = (captured_bytes - src_off).min(space);
                    if copy > 0 {
                        std::ptr::copy_nonoverlapping(
                            src.as_ptr().add(src_off),
                            buf_ptr.add(recorded),
                            copy,
                        );
                        *wavehdr_bytes_recorded(pwh) = (recorded + copy) as u32;
                        src_off += copy;
                    }
                    *wavehdr_bytes_recorded(pwh) as usize >= buf_len
                };
                if full {
                    list.remove(0);
                    drop(list);
                    unsafe { finish_buffer(pwh, &notify) };
                }
            }
        } else if rc == -11 {
            // EAGAIN
            std::thread::sleep(std::time::Duration::from_millis(2));
        } else if rc < 0 && unsafe { (a.recover)(pcm, rc as c_int, 1) } < 0 {
            break;
        }
    }

    unsafe {
        (a.drop_pcm)(pcm);
        (a.close)(pcm);
    }
}

/// Paramètres > Confidentialité : le micro peut être refusé à tous les .exe
/// ou à cette application (question posée la première fois).
fn microphone_allowed() -> bool {
    crate::permissions::allowed(crate::permissions::Capability::Microphone)
}

fn microphone_available() -> bool {
    microphone_allowed() && alsa_capture().is_ok()
}

pub unsafe extern "win64" fn wave_in_get_num_devs() -> u32 {
    u32::from(microphone_available())
}

const MMSYSERR_BADDEVICEID: u32 = 2;
const MMSYSERR_ALLOCATED: u32 = 4;
const MMSYSERR_INVALHANDLE: u32 = 5;
const MMSYSERR_NODRIVER: u32 = 6;
const MMSYSERR_NOTSUPPORTED: u32 = 8;
const MMSYSERR_INVALFLAG: u32 = 10;
const MMSYSERR_INVALPARAM: u32 = 11;
const WAVERR_BADFORMAT: u32 = 32;
const WAVERR_STILLPLAYING: u32 = 33;
const WAVERR_UNPREPARED: u32 = 34;

fn valid_device(id: usize) -> bool {
    id == 0 || id as u32 == u32::MAX || id == usize::MAX
}

/// waveInGetDevCaps{A,W} : WAVEINCAPS (48 octets en A, 80 en W).
unsafe fn dev_caps(device: usize, out: *mut u8, size: u32, wide: bool) -> u32 {
    if out.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    if !valid_device(device) {
        return MMSYSERR_BADDEVICEID;
    }
    if !microphone_available() {
        return MMSYSERR_NODRIVER;
    }
    let full = if wide { 80 } else { 48 };
    let mut caps = vec![0u8; full];
    caps[0..2].copy_from_slice(&1u16.to_le_bytes()); // MM_MICROSOFT
    caps[2..4].copy_from_slice(&0x65u16.to_le_bytes()); // MM_PCM_WAVEIN générique
    caps[4..8].copy_from_slice(&0x0100u32.to_le_bytes());
    let name = "Microphone (Winlux)";
    let (name_len, tail) = if wide { (64, 72) } else { (32, 40) };
    super::comobj::write_fixed_str(
        caps[8..8 + name_len].as_mut_ptr(),
        name_len / if wide { 2 } else { 1 },
        name,
        wide,
    );
    caps[tail..tail + 4].copy_from_slice(&0x000F_FFFFu32.to_le_bytes()); // WAVE_FORMAT_* 11 à 96 kHz
    caps[tail + 4..tail + 6].copy_from_slice(&2u16.to_le_bytes());
    let n = (size as usize).min(full);
    std::ptr::copy_nonoverlapping(caps.as_ptr(), out, n);
    0
}

pub unsafe extern "win64" fn wave_in_get_dev_caps_a(device: usize, out: *mut u8, size: u32) -> u32 {
    dev_caps(device, out, size, false)
}
pub unsafe extern "win64" fn wave_in_get_dev_caps_w(device: usize, out: *mut u8, size: u32) -> u32 {
    dev_caps(device, out, size, true)
}

pub unsafe extern "win64" fn wave_in_open(
    phwi: *mut *mut c_void,
    device_id: u32,
    pwfx: *const u8,
    callback: usize,
    instance: usize,
    flags: u32,
) -> u32 {
    if !valid_device(device_id as usize) {
        return MMSYSERR_BADDEVICEID;
    }
    if !microphone_allowed() {
        return MMSYSERR_NODRIVER;
    }
    if pwfx.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    let Some(config) = parse_wave_format(pwfx) else {
        return WAVERR_BADFORMAT;
    };
    if flags & WAVE_FORMAT_QUERY != 0 {
        return 0;
    }
    let kind = flags & CALLBACK_TYPEMASK;
    if !matches!(
        kind,
        0 | CALLBACK_WINDOW | 0x0002_0000 | CALLBACK_FUNCTION | CALLBACK_EVENT
    ) {
        return MMSYSERR_INVALFLAG;
    }
    let notify = Notify {
        kind,
        target: callback,
        instance,
        handle: HWAVEIN,
    };
    {
        let mut state = lock(record_lock());
        if state.is_some() {
            return MMSYSERR_ALLOCATED;
        }
        *state = Some(RecordState {
            config,
            notify,
            buffers: std::sync::Arc::new(Mutex::new(Vec::new())),
            cancel: std::sync::Arc::new(AtomicBool::new(false)),
            captured: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            thread: None,
        });
    }
    if !phwi.is_null() {
        *phwi = HWAVEIN as *mut c_void;
    }
    notify.send(WIM_OPEN, 0);
    0
}

fn check_handle(hwi: *mut c_void) -> bool {
    // Anciennes versions : poignée factice non nulle acceptée aussi.
    !hwi.is_null() && lock(record_lock()).is_some()
}

pub unsafe extern "win64" fn wave_in_prepare_header(
    _hwi: *mut c_void,
    pwh: *mut u8,
    _cbwh: u32,
) -> u32 {
    if pwh.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    *wavehdr_flags(pwh) |= WHDR_PREPARED;
    0
}

pub unsafe extern "win64" fn wave_in_unprepare_header(
    _hwi: *mut c_void,
    pwh: *mut u8,
    _cbwh: u32,
) -> u32 {
    if pwh.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    if *wavehdr_flags(pwh) & WHDR_INQUEUE != 0 {
        return WAVERR_STILLPLAYING;
    }
    *wavehdr_flags(pwh) &= !WHDR_PREPARED;
    0
}

pub unsafe extern "win64" fn wave_in_add_buffer(hwi: *mut c_void, pwh: *mut u8, _cbwh: u32) -> u32 {
    if pwh.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    if !check_handle(hwi) {
        return MMSYSERR_INVALHANDLE;
    }
    if *wavehdr_flags(pwh) & WHDR_PREPARED == 0 {
        return WAVERR_UNPREPARED;
    }
    *wavehdr_bytes_recorded(pwh) = 0;
    *wavehdr_flags(pwh) = (*wavehdr_flags(pwh) & !WHDR_DONE) | WHDR_INQUEUE;
    let state = lock(record_lock());
    match *state {
        Some(ref s) => {
            lock(&s.buffers).push(WaveHdrPtr(pwh));
            0
        }
        None => MMSYSERR_INVALHANDLE,
    }
}

pub unsafe extern "win64" fn wave_in_start(hwi: *mut c_void) -> u32 {
    if hwi.is_null() {
        return MMSYSERR_INVALHANDLE;
    }
    let mut state = lock(record_lock());
    let Some(ref mut s) = *state else {
        return MMSYSERR_INVALHANDLE;
    };
    if s.thread.is_some() {
        return 0; // déjà en cours
    }
    s.cancel.store(false, Ordering::Relaxed);
    let (config, notify) = (s.config, s.notify);
    let (bufs, cancel, captured) = (s.buffers.clone(), s.cancel.clone(), s.captured.clone());
    match std::thread::Builder::new()
        .name("winlux-wave-capture".into())
        .spawn(move || capture_thread(config, notify, bufs, cancel, captured))
    {
        Ok(handle) => {
            s.thread = Some(handle);
            0
        }
        Err(_) => 7, // MMSYSERR_NOMEM
    }
}

/// Arrête la capture ; `flush` rend aussi les tampons en attente (Reset).
unsafe fn stop(flush: bool) -> u32 {
    let (thread, notify, pending) = {
        let mut state = lock(record_lock());
        let Some(ref mut s) = *state else {
            return MMSYSERR_INVALHANDLE;
        };
        s.cancel.store(true, Ordering::Relaxed);
        (s.thread.take(), s.notify, s.buffers.clone())
    };
    // Le fil peut être en train d'appeler le programme : ne pas tenir le verrou.
    if let Some(t) = thread {
        let _ = t.join();
    }
    let mut bufs = lock(&pending);
    // Stop : seul le tampon partiellement rempli est rendu ; Reset : tous.
    let n = if flush {
        bufs.len()
    } else {
        usize::from(
            bufs.first()
                .is_some_and(|b| *wavehdr_bytes_recorded(b.0) > 0),
        )
    };
    let done: Vec<WaveHdrPtr> = bufs.drain(..n).collect();
    drop(bufs);
    for b in done {
        finish_buffer(b.0, &notify);
    }
    0
}

pub unsafe extern "win64" fn wave_in_stop(_hwi: *mut c_void) -> u32 {
    stop(false)
}

pub unsafe extern "win64" fn wave_in_reset(_hwi: *mut c_void) -> u32 {
    let r = stop(true);
    if let Some(ref s) = *lock(record_lock()) {
        s.captured.store(0, Ordering::Relaxed);
    }
    r
}

pub unsafe extern "win64" fn wave_in_close(hwi: *mut c_void) -> u32 {
    if lock(record_lock()).is_none() {
        return MMSYSERR_INVALHANDLE;
    }
    wave_in_reset(hwi);
    let notify = lock(record_lock()).take().map(|s| s.notify);
    if let Some(n) = notify {
        n.send(WIM_CLOSE, 0);
    }
    0
}

/// waveInGetPosition : MMTIME en octets, échantillons ou millisecondes.
pub unsafe extern "win64" fn wave_in_get_position(
    _hwi: *mut c_void,
    mmt: *mut u32,
    size: u32,
) -> u32 {
    if mmt.is_null() || size < 8 {
        return MMSYSERR_INVALPARAM;
    }
    let state = lock(record_lock());
    let Some(ref s) = *state else {
        return MMSYSERR_INVALHANDLE;
    };
    let bytes = s.captured.load(Ordering::Relaxed);
    let frames = bytes / s.config.block_align().max(1) as u64;
    match *mmt {
        1 => *mmt.add(1) = (frames * 1000 / s.config.rate.max(1) as u64) as u32, // TIME_MS
        2 => *mmt.add(1) = frames as u32,                                        // TIME_SAMPLES
        _ => {
            *mmt = 4; // TIME_BYTES
            *mmt.add(1) = bytes as u32;
        }
    }
    0
}

pub unsafe extern "win64" fn wave_in_get_id(hwi: *mut c_void, id: *mut u32) -> u32 {
    if id.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    if !check_handle(hwi) {
        return MMSYSERR_INVALHANDLE;
    }
    *id = 0;
    0
}

pub extern "win64" fn wave_in_message(_hwi: usize, _msg: u32, _p1: usize, _p2: usize) -> u32 {
    MMSYSERR_NOTSUPPORTED
}

fn wave_in_error_text(err: u32) -> &'static str {
    match err {
        0 => "Aucune erreur.",
        MMSYSERR_BADDEVICEID => "Identifiant de p\u{e9}riph\u{e9}rique audio non valide.",
        MMSYSERR_ALLOCATED => "Le micro est d\u{e9}j\u{e0} utilis\u{e9} par un programme.",
        MMSYSERR_INVALHANDLE => "Poign\u{e9}e de p\u{e9}riph\u{e9}rique non valide.",
        MMSYSERR_NODRIVER => "Aucun micro disponible ou acc\u{e8}s refus\u{e9} (Param\u{e8}tres > Confidentialit\u{e9}).",
        WAVERR_BADFORMAT => "Format audio non pris en charge par le micro.",
        WAVERR_STILLPLAYING => "Le tampon est encore en cours d'utilisation.",
        WAVERR_UNPREPARED => "Le tampon n'a pas \u{e9}t\u{e9} pr\u{e9}par\u{e9}.",
        _ => "Erreur audio inconnue.",
    }
}

pub unsafe extern "win64" fn wave_in_get_error_text_a(err: u32, buf: *mut u8, cch: u32) -> u32 {
    if buf.is_null() || cch == 0 {
        return MMSYSERR_INVALPARAM;
    }
    super::comobj::write_fixed_str(buf, cch as usize, wave_in_error_text(err), false);
    0
}

pub unsafe extern "win64" fn wave_in_get_error_text_w(err: u32, buf: *mut u16, cch: u32) -> u32 {
    if buf.is_null() || cch == 0 {
        return MMSYSERR_INVALPARAM;
    }
    super::comobj::write_fixed_str(buf as *mut u8, cch as usize, wave_in_error_text(err), true);
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {"PlaySoundA"=>fn play_sound_a,"PlaySoundW"=>fn play_sound_w,"mciSendStringA"=>fn mci_send_string_a,"timeGetTime"=>fn time_get_time,"waveInGetNumDevs"=>fn wave_in_get_num_devs,"waveInOpen"=>fn wave_in_open,"waveInPrepareHeader"=>fn wave_in_prepare_header,"waveInUnprepareHeader"=>fn wave_in_unprepare_header,"waveInAddBuffer"=>fn wave_in_add_buffer,"waveInStart"=>fn wave_in_start,"waveInStop"=>fn wave_in_stop,"waveInReset"=>fn wave_in_reset,"waveInClose"=>fn wave_in_close,"waveInGetDevCapsA"=>fn wave_in_get_dev_caps_a,"waveInGetDevCapsW"=>fn wave_in_get_dev_caps_w,"waveInGetPosition"=>fn wave_in_get_position,"waveInGetID"=>fn wave_in_get_id,"waveInMessage"=>fn wave_in_message,"waveInGetErrorTextA"=>fn wave_in_get_error_text_a,"waveInGetErrorTextW"=>fn wave_in_get_error_text_w}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave_format(tag: u16, channels: u16, rate: u32, bits: u16) -> [u8; 18] {
        let mut f = [0u8; 18];
        f[0..2].copy_from_slice(&tag.to_le_bytes());
        f[2..4].copy_from_slice(&channels.to_le_bytes());
        f[4..8].copy_from_slice(&rate.to_le_bytes());
        let align = channels * bits / 8;
        f[8..12].copy_from_slice(&(rate * align as u32).to_le_bytes());
        f[12..14].copy_from_slice(&align.to_le_bytes());
        f[14..16].copy_from_slice(&bits.to_le_bytes());
        f
    }

    #[test]
    fn formats_de_capture_du_micro() {
        unsafe {
            let pcm = parse_wave_format(wave_format(1, 1, 44100, 16).as_ptr()).unwrap();
            assert_eq!((pcm.alsa_format(), pcm.block_align()), (2, 2));
            let u8s = parse_wave_format(wave_format(1, 2, 8000, 8).as_ptr()).unwrap();
            assert_eq!((u8s.alsa_format(), u8s.block_align()), (1, 2));
            let f = parse_wave_format(wave_format(3, 2, 48000, 32).as_ptr()).unwrap();
            assert_eq!((f.alsa_format(), f.block_align()), (14, 8));
            assert!(parse_wave_format(wave_format(1, 6, 48000, 16).as_ptr()).is_none());
            assert!(parse_wave_format(wave_format(3, 2, 48000, 16).as_ptr()).is_none());
            assert!(parse_wave_format(wave_format(2, 2, 48000, 16).as_ptr()).is_none());
        }
    }

    #[test]
    fn interrogation_de_format_sans_ouvrir_le_micro() {
        unsafe {
            let fmt = wave_format(1, 1, 16000, 16);
            if microphone_allowed() {
                let mut h = std::ptr::null_mut();
                assert_eq!(
                    wave_in_open(&mut h, u32::MAX, fmt.as_ptr(), 0, 0, WAVE_FORMAT_QUERY),
                    0
                );
                assert!(h.is_null(), "WAVE_FORMAT_QUERY n'ouvre rien");
                let bad = wave_format(1, 1, 16000, 24);
                assert_eq!(
                    wave_in_open(&mut h, 0, bad.as_ptr(), 0, 0, 0),
                    WAVERR_BADFORMAT
                );
            }
            let mut mmt = [4u32, 0, 0];
            assert_eq!(
                wave_in_get_position(std::ptr::null_mut(), mmt.as_mut_ptr(), 12),
                MMSYSERR_INVALHANDLE
            );
            let mut text = [0u16; 64];
            assert_eq!(
                wave_in_get_error_text_w(WAVERR_BADFORMAT, text.as_mut_ptr(), 64),
                0
            );
            assert_eq!(text[0], 'F' as u16);
        }
    }

    #[test]
    fn decode_wave_pcm_mono_16bits_and_rejette_un_fichier_tronque() {
        let mut w=Vec::from(&b"RIFF\x28\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x44\xac\0\0\x88\x58\x01\0\x02\0\x10\0data\x04\0\0\0\x00\x80\xff\x7f"[..]);
        let decoded = decode_wave(&w).unwrap();
        assert_eq!(decoded.channels, 1);
        assert_eq!(decoded.rate, 44100);
        assert_eq!(decoded.samples, [i16::MIN, i16::MAX]);
        w.pop();
        assert!(decode_wave(&w).is_none());
    }
}
