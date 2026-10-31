//! winmm waveOut* : lecture de tampons PCM (WAVEHDR) via le mixeur audio de
//! Winlux, avec rappels WOM_DONE (fonction, événement ou fenêtre), boucles,
//! pause, volume par flux et position de lecture.

use crate::audio::{self, dsp::Resampler, format::Format};
use crate::dlls::lock;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const MMSYSERR_NOERROR: u32 = 0;
const MMSYSERR_BADDEVICEID: u32 = 2;
const MMSYSERR_INVALHANDLE: u32 = 5;
const MMSYSERR_NOMEM: u32 = 7;
const MMSYSERR_NOTSUPPORTED: u32 = 8;
const MMSYSERR_INVALFLAG: u32 = 10;
const MMSYSERR_INVALPARAM: u32 = 11;
const WAVERR_BADFORMAT: u32 = 32;
const WAVERR_STILLPLAYING: u32 = 33;
const WAVERR_UNPREPARED: u32 = 34;

const WHDR_DONE: u32 = 0x01;
const WHDR_PREPARED: u32 = 0x02;
const WHDR_BEGINLOOP: u32 = 0x04;
const WHDR_ENDLOOP: u32 = 0x08;
const WHDR_INQUEUE: u32 = 0x10;

const WAVE_FORMAT_QUERY: u32 = 0x1;
const CALLBACK_TYPEMASK: u32 = 0x7_0000;
const CALLBACK_NULL: u32 = 0;
const CALLBACK_WINDOW: u32 = 0x1_0000;
const CALLBACK_THREAD: u32 = 0x2_0000;
const CALLBACK_FUNCTION: u32 = 0x3_0000;
const CALLBACK_EVENT: u32 = 0x5_0000;

const WOM_OPEN: u32 = 0x3bb;
const WOM_CLOSE: u32 = 0x3bc;
const WOM_DONE: u32 = 0x3bd;

const TIME_MS: u32 = 1;
const TIME_SAMPLES: u32 = 2;
const TIME_BYTES: u32 = 4;

const WAVE_MAPPER: u32 = u32::MAX;
const DEVICE_NAME: &str = "Haut-parleurs (Winlux)";

// WAVEHDR (x64) : lpData @0, dwBufferLength @8, dwBytesRecorded @12,
// dwUser @16, dwFlags @24, dwLoops @28, lpNext @32, reserved @40.
unsafe fn hdr_data(h: usize) -> *const u8 {
    *(h as *const *const u8)
}
unsafe fn hdr_len(h: usize) -> usize {
    *((h + 8) as *const u32) as usize
}
unsafe fn hdr_flags(h: usize) -> *mut u32 {
    (h + 24) as *mut u32
}
unsafe fn hdr_loops(h: usize) -> u32 {
    *((h + 28) as *const u32)
}

#[derive(Clone, Copy)]
struct Callback {
    kind: u32,
    target: usize,
    instance: usize,
}

struct State {
    queue: VecDeque<usize>,
    cur: usize,
    offset: usize,
    loop_begin: Option<usize>,
    loops_left: u32,
    paused: bool,
    bytes: u64,
    volume: [f32; 2],
    rate: f64,
    resampler: Resampler,
    done: Vec<usize>,
}

struct WaveOut {
    handle: usize,
    format: Format,
    callback: Callback,
    source: AtomicUsize,
    st: Mutex<State>,
}

impl WaveOut {
    fn notify(&self, msg: u32, p1: usize) {
        let cb = self.callback;
        match cb.kind {
            CALLBACK_FUNCTION if cb.target != 0 => unsafe {
                audio::ensure_windows_thread();
                let f: extern "win64" fn(usize, u32, usize, usize, usize) =
                    std::mem::transmute(cb.target);
                f(self.handle, msg, cb.instance, p1, 0);
            },
            CALLBACK_EVENT => super::audio_com::signal(cb.target),
            CALLBACK_WINDOW if cb.target != 0 => unsafe {
                super::user32::post_message_a(cb.target, msg, self.handle, p1 as isize);
            },
            _ => {}
        }
    }

    /// Avance d'un en-tête terminé : boucle ou le marque fini.
    fn finish_header(st: &mut State) {
        let h = st.queue[st.cur];
        let flags = unsafe { *hdr_flags(h) };
        st.offset = 0;
        if flags & WHDR_ENDLOOP != 0 {
            if let Some(b) = st.loop_begin {
                if st.loops_left > 0 {
                    st.loops_left -= 1;
                    st.cur = b;
                    return;
                }
                st.loop_begin = None;
                for _ in 0..=st.cur {
                    if let Some(x) = st.queue.pop_front() {
                        st.done.push(x);
                    }
                }
                st.cur = 0;
                return;
            }
        }
        if st.loop_begin.is_some() {
            st.cur += 1;
        } else if let Some(x) = st.queue.pop_front() {
            st.done.push(x);
        }
    }

    fn next_frame(&self, st: &mut State) -> Option<[f32; 2]> {
        let block = self.format.block as usize;
        loop {
            if st.paused || st.cur >= st.queue.len() {
                return None;
            }
            let h = st.queue[st.cur];
            if st.offset == 0
                && unsafe { *hdr_flags(h) } & WHDR_BEGINLOOP != 0
                && st.loop_begin != Some(st.cur)
            {
                st.loop_begin = Some(st.cur);
                st.loops_left = unsafe { hdr_loops(h) }.saturating_sub(1);
            }
            let len = unsafe { hdr_len(h) };
            if st.offset + block <= len {
                let data = unsafe { std::slice::from_raw_parts(hdr_data(h).add(st.offset), block) };
                st.offset += block;
                st.bytes += block as u64;
                return Some(self.format.stereo(data));
            }
            Self::finish_header(st);
        }
    }

    fn complete(&self, done: Vec<usize>) {
        for h in done {
            unsafe {
                let f = hdr_flags(h);
                *f = (*f & !WHDR_INQUEUE) | WHDR_DONE;
            }
            self.notify(WOM_DONE, h);
        }
    }
}

impl audio::Source for WaveOut {
    fn mix(&self, out: &mut [f32]) -> bool {
        let done = {
            let mut st = lock(&self.st);
            let mut rs = std::mem::take(&mut st.resampler);
            let step = self.format.rate as f64 * st.rate / audio::RATE as f64;
            let gain = st.volume;
            {
                let st_ref = &mut *st;
                let mut pull = || self.next_frame(st_ref);
                rs.mix_into(step, out, gain, &mut pull);
            }
            // Les en-têtes vidés en fin de période sont rendus tout de suite.
            if st.cur < st.queue.len() && !st.paused {
                let h = st.queue[st.cur];
                if st.offset >= unsafe { hdr_len(h) } {
                    Self::finish_header(&mut st);
                }
            }
            st.resampler = rs;
            std::mem::take(&mut st.done)
        };
        self.complete(done);
        true
    }
}

fn table() -> &'static Mutex<HashMap<usize, Arc<WaveOut>>> {
    static T: OnceLock<Mutex<HashMap<usize, Arc<WaveOut>>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get(h: usize) -> Option<Arc<WaveOut>> {
    lock(table()).get(&h).cloned()
}

fn valid_device(id: usize) -> bool {
    id == 0 || id as u32 == WAVE_MAPPER || id == usize::MAX || get(id).is_some()
}

pub extern "win64" fn wave_out_get_num_devs() -> u32 {
    1
}

unsafe fn fill_caps(caps: *mut u8, size: u32, wide: bool) -> u32 {
    let need = if wide { 84 } else { 52 };
    if caps.is_null() || size == 0 {
        return MMSYSERR_INVALPARAM;
    }
    let mut buf = vec![0u8; need];
    buf[0..2].copy_from_slice(&1u16.to_le_bytes()); // MM_MICROSOFT
    buf[2..4].copy_from_slice(&2u16.to_le_bytes()); // MM_WAVE_MAPPER
    buf[4..8].copy_from_slice(&0x0a00u32.to_le_bytes());
    let name_end = if wide {
        for (i, c) in DEVICE_NAME.encode_utf16().take(31).enumerate() {
            buf[8 + 2 * i..10 + 2 * i].copy_from_slice(&c.to_le_bytes());
        }
        8 + 64
    } else {
        for (i, c) in super::encode_ansi(DEVICE_NAME)
            .into_iter()
            .take(31)
            .enumerate()
        {
            buf[8 + i] = c;
        }
        8 + 32
    };
    buf[name_end..name_end + 4].copy_from_slice(&0x000f_ffffu32.to_le_bytes());
    buf[name_end + 4..name_end + 6].copy_from_slice(&2u16.to_le_bytes());
    // WAVECAPS_PLAYBACKRATE | VOLUME | LRVOLUME | SAMPLEACCURATE.
    buf[name_end + 8..name_end + 12].copy_from_slice(&0x2eu32.to_le_bytes());
    let n = (size as usize).min(need);
    std::ptr::copy_nonoverlapping(buf.as_ptr(), caps, n);
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn wave_out_get_dev_caps_a(id: usize, caps: *mut u8, size: u32) -> u32 {
    if !valid_device(id) {
        return MMSYSERR_BADDEVICEID;
    }
    fill_caps(caps, size, false)
}

pub unsafe extern "win64" fn wave_out_get_dev_caps_w(id: usize, caps: *mut u8, size: u32) -> u32 {
    if !valid_device(id) {
        return MMSYSERR_BADDEVICEID;
    }
    fill_caps(caps, size, true)
}

pub unsafe extern "win64" fn wave_out_open(
    phwo: *mut usize,
    device: u32,
    fmt: *const u8,
    callback: usize,
    instance: usize,
    flags: u32,
) -> u32 {
    if device != 0 && device != WAVE_MAPPER {
        return MMSYSERR_BADDEVICEID;
    }
    let Some(format) = Format::from_ptr(fmt) else {
        return WAVERR_BADFORMAT;
    };
    if flags & WAVE_FORMAT_QUERY != 0 {
        return MMSYSERR_NOERROR;
    }
    if phwo.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    let kind = flags & CALLBACK_TYPEMASK;
    if !matches!(
        kind,
        CALLBACK_NULL | CALLBACK_WINDOW | CALLBACK_THREAD | CALLBACK_FUNCTION | CALLBACK_EVENT
    ) {
        return MMSYSERR_INVALFLAG;
    }
    static NEXT: AtomicUsize = AtomicUsize::new(0x7731_0000);
    let handle = NEXT.fetch_add(4, Ordering::Relaxed);
    let wo = Arc::new(WaveOut {
        handle,
        format,
        callback: Callback {
            kind,
            target: callback,
            instance,
        },
        source: AtomicUsize::new(0),
        st: Mutex::new(State {
            queue: VecDeque::new(),
            cur: 0,
            offset: 0,
            loop_begin: None,
            loops_left: 0,
            paused: false,
            bytes: 0,
            volume: [1.0, 1.0],
            rate: 1.0,
            resampler: Resampler::new(),
            done: Vec::new(),
        }),
    });
    lock(table()).insert(handle, wo.clone());
    let id = audio::add_source(wo.clone());
    wo.source.store(id as usize, Ordering::Relaxed);
    *phwo = handle;
    if kind == CALLBACK_FUNCTION {
        // Le rappel WOM_OPEN arrive sur le thread appelant.
        let f: extern "win64" fn(usize, u32, usize, usize, usize) = std::mem::transmute(callback);
        if callback != 0 {
            f(handle, WOM_OPEN, instance, 0, 0);
        }
    } else {
        wo.notify(WOM_OPEN, 0);
    }
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn wave_out_close(h: usize) -> u32 {
    let Some(wo) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    if !lock(&wo.st).queue.is_empty() {
        return WAVERR_STILLPLAYING;
    }
    lock(table()).remove(&h);
    audio::remove_source(wo.source.load(Ordering::Relaxed) as u64);
    if wo.callback.kind == CALLBACK_FUNCTION && wo.callback.target != 0 {
        let f: extern "win64" fn(usize, u32, usize, usize, usize) =
            std::mem::transmute(wo.callback.target);
        f(h, WOM_CLOSE, wo.callback.instance, 0, 0);
    } else {
        wo.notify(WOM_CLOSE, 0);
    }
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn wave_out_prepare_header(h: usize, hdr: usize, size: u32) -> u32 {
    if get(h).is_none() {
        return MMSYSERR_INVALHANDLE;
    }
    if hdr == 0 || size < 32 || hdr_data(hdr).is_null() {
        return MMSYSERR_INVALPARAM;
    }
    let f = hdr_flags(hdr);
    if *f & WHDR_INQUEUE != 0 {
        return WAVERR_STILLPLAYING;
    }
    *f |= WHDR_PREPARED;
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn wave_out_unprepare_header(h: usize, hdr: usize, size: u32) -> u32 {
    if get(h).is_none() {
        return MMSYSERR_INVALHANDLE;
    }
    if hdr == 0 || size < 32 {
        return MMSYSERR_INVALPARAM;
    }
    let f = hdr_flags(hdr);
    if *f & WHDR_INQUEUE != 0 {
        return WAVERR_STILLPLAYING;
    }
    *f &= !WHDR_PREPARED;
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn wave_out_write(h: usize, hdr: usize, size: u32) -> u32 {
    let Some(wo) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    if hdr == 0 || size < 32 {
        return MMSYSERR_INVALPARAM;
    }
    let f = hdr_flags(hdr);
    if *f & WHDR_PREPARED == 0 {
        return WAVERR_UNPREPARED;
    }
    if *f & WHDR_INQUEUE != 0 {
        return WAVERR_STILLPLAYING;
    }
    *f = (*f & !WHDR_DONE) | WHDR_INQUEUE;
    let mut st = lock(&wo.st);
    // lpNext n'est pas utilisé par les programmes ; on le laisse intact.
    st.queue.push_back(hdr);
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn wave_out_pause(h: usize) -> u32 {
    let Some(wo) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    lock(&wo.st).paused = true;
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn wave_out_restart(h: usize) -> u32 {
    let Some(wo) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    lock(&wo.st).paused = false;
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn wave_out_reset(h: usize) -> u32 {
    let Some(wo) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    let done: Vec<usize> = {
        let mut st = lock(&wo.st);
        let mut d = std::mem::take(&mut st.done);
        d.extend(st.queue.drain(..));
        st.cur = 0;
        st.offset = 0;
        st.loop_begin = None;
        st.bytes = 0;
        st.paused = false;
        st.resampler.reset();
        d
    };
    wo.complete(done);
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn wave_out_break_loop(h: usize) -> u32 {
    let Some(wo) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    lock(&wo.st).loops_left = 0;
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn wave_out_get_position(h: usize, mmt: *mut u32, size: u32) -> u32 {
    let Some(wo) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    if mmt.is_null() || size < 8 {
        return MMSYSERR_INVALPARAM;
    }
    let bytes = lock(&wo.st).bytes;
    let fmt = wo.format;
    match *mmt {
        TIME_SAMPLES => *mmt.add(1) = (bytes / fmt.block as u64) as u32,
        TIME_MS => *mmt.add(1) = (bytes * 1000 / fmt.bytes_per_sec().max(1) as u64) as u32,
        _ => {
            *mmt = TIME_BYTES;
            *mmt.add(1) = bytes as u32;
        }
    }
    MMSYSERR_NOERROR
}

fn pack_volume(v: [f32; 2]) -> u32 {
    let l = (v[0].clamp(0.0, 1.0) * 65535.0).round() as u32;
    let r = (v[1].clamp(0.0, 1.0) * 65535.0).round() as u32;
    l | (r << 16)
}

pub unsafe extern "win64" fn wave_out_get_volume(h: usize, out: *mut u32) -> u32 {
    if out.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    match get(h) {
        Some(wo) => *out = pack_volume(lock(&wo.st).volume),
        None if h == 0 || h as u32 == WAVE_MAPPER => {
            let m = audio::master_volume();
            *out = pack_volume([m, m]);
        }
        None => return MMSYSERR_INVALHANDLE,
    }
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn wave_out_set_volume(h: usize, v: u32) -> u32 {
    let lr = [(v & 0xffff) as f32 / 65535.0, (v >> 16) as f32 / 65535.0];
    match get(h) {
        Some(wo) => lock(&wo.st).volume = lr,
        None if h == 0 || h as u32 == WAVE_MAPPER => {
            audio::set_master_volume(lr[0].max(lr[1]));
        }
        None => return MMSYSERR_INVALHANDLE,
    }
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn wave_out_get_playback_rate(h: usize, out: *mut u32) -> u32 {
    let Some(wo) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    if out.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    *out = (lock(&wo.st).rate * 65536.0) as u32;
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn wave_out_set_playback_rate(h: usize, rate: u32) -> u32 {
    let Some(wo) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    if rate == 0 {
        return MMSYSERR_INVALPARAM;
    }
    lock(&wo.st).rate = (rate as f64 / 65536.0).clamp(0.01, 16.0);
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn wave_out_pitch(h: usize, _v: usize) -> u32 {
    if get(h).is_none() {
        return MMSYSERR_INVALHANDLE;
    }
    MMSYSERR_NOTSUPPORTED
}

pub unsafe extern "win64" fn wave_out_get_id(h: usize, out: *mut u32) -> u32 {
    if get(h).is_none() {
        return MMSYSERR_INVALHANDLE;
    }
    if out.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    *out = 0;
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn wave_out_message(_h: usize, _msg: u32, _p1: usize, _p2: usize) -> u32 {
    MMSYSERR_NOTSUPPORTED
}

fn error_text(code: u32) -> &'static str {
    match code {
        MMSYSERR_NOERROR => "Aucune erreur.",
        MMSYSERR_BADDEVICEID => "Identificateur de périphérique hors limites.",
        MMSYSERR_INVALHANDLE => "Handle de périphérique non valide.",
        MMSYSERR_NOMEM => "Mémoire insuffisante.",
        MMSYSERR_NOTSUPPORTED => "Fonction non prise en charge.",
        MMSYSERR_INVALFLAG => "Indicateur non valide.",
        MMSYSERR_INVALPARAM => "Paramètre non valide.",
        WAVERR_BADFORMAT => "Format audio non pris en charge.",
        WAVERR_STILLPLAYING => "Des tampons sont encore en lecture.",
        WAVERR_UNPREPARED => "En-tête de tampon non préparé.",
        _ => "Erreur inconnue.",
    }
}

pub unsafe extern "win64" fn wave_out_get_error_text_a(code: u32, buf: *mut u8, cap: u32) -> u32 {
    if buf.is_null() || cap == 0 {
        return MMSYSERR_INVALPARAM;
    }
    let bytes = super::encode_ansi(error_text(code));
    let n = bytes.len().min(cap as usize - 1);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, n);
    *buf.add(n) = 0;
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn wave_out_get_error_text_w(code: u32, buf: *mut u16, cap: u32) -> u32 {
    if buf.is_null() || cap == 0 {
        return MMSYSERR_INVALPARAM;
    }
    let w: Vec<u16> = error_text(code).encode_utf16().collect();
    let n = w.len().min(cap as usize - 1);
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, n);
    *buf.add(n) = 0;
    MMSYSERR_NOERROR
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "waveOutGetNumDevs"=>fn wave_out_get_num_devs,
        "waveOutGetDevCapsA"=>fn wave_out_get_dev_caps_a,"waveOutGetDevCapsW"=>fn wave_out_get_dev_caps_w,
        "waveOutOpen"=>fn wave_out_open,"waveOutClose"=>fn wave_out_close,
        "waveOutPrepareHeader"=>fn wave_out_prepare_header,"waveOutUnprepareHeader"=>fn wave_out_unprepare_header,
        "waveOutWrite"=>fn wave_out_write,"waveOutPause"=>fn wave_out_pause,"waveOutRestart"=>fn wave_out_restart,
        "waveOutReset"=>fn wave_out_reset,"waveOutBreakLoop"=>fn wave_out_break_loop,
        "waveOutGetPosition"=>fn wave_out_get_position,
        "waveOutGetVolume"=>fn wave_out_get_volume,"waveOutSetVolume"=>fn wave_out_set_volume,
        "waveOutGetPlaybackRate"=>fn wave_out_get_playback_rate,"waveOutSetPlaybackRate"=>fn wave_out_set_playback_rate,
        "waveOutGetPitch"=>fn wave_out_pitch,"waveOutSetPitch"=>fn wave_out_pitch,
        "waveOutGetID"=>fn wave_out_get_id,"waveOutMessage"=>fn wave_out_message,
        "waveOutGetErrorTextA"=>fn wave_out_get_error_text_a,"waveOutGetErrorTextW"=>fn wave_out_get_error_text_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::format::Sample;

    #[repr(C)]
    struct WaveHdr {
        data: *const u8,
        len: u32,
        recorded: u32,
        user: usize,
        flags: u32,
        loops: u32,
        next: usize,
        reserved: usize,
    }

    fn hdr(data: &[u8], flags: u32, loops: u32) -> WaveHdr {
        WaveHdr {
            data: data.as_ptr(),
            len: data.len() as u32,
            recorded: 0,
            user: 0,
            flags,
            loops,
            next: 0,
            reserved: 0,
        }
    }

    #[test]
    fn lecture_boucle_et_position() {
        std::env::set_var("WINLUX_AUDIO", "null");
        let fmt = Format::new(Sample::S16, 1, 48000).unwrap().to_bytes();
        let pcm: Vec<u8> = (0..480u32).flat_map(|_| 1000i16.to_le_bytes()).collect();
        unsafe {
            let mut h = 0usize;
            assert_eq!(wave_out_open(&mut h, WAVE_MAPPER, fmt.as_ptr(), 0, 0, 0), 0);
            assert_eq!(
                wave_out_open(
                    std::ptr::null_mut(),
                    0,
                    fmt.as_ptr(),
                    0,
                    0,
                    WAVE_FORMAT_QUERY
                ),
                0
            );
            let mut a = hdr(&pcm, 0, 0);
            let mut b = hdr(&pcm, 0, 0);
            let pa = &mut a as *mut WaveHdr as usize;
            let pb = &mut b as *mut WaveHdr as usize;
            assert_eq!(wave_out_write(h, pa, 48), WAVERR_UNPREPARED);
            // Boucle de 3 lectures sur un seul en-tête, puis un second en-tête.
            a.flags = 0;
            a.loops = 3;
            assert_eq!(wave_out_prepare_header(h, pa, 48), 0);
            a.flags |= WHDR_BEGINLOOP | WHDR_ENDLOOP;
            assert_eq!(wave_out_prepare_header(h, pb, 48), 0);
            assert_eq!(wave_out_write(h, pa, 48), 0);
            assert_eq!(wave_out_write(h, pb, 48), 0);
            assert_eq!(wave_out_close(h), WAVERR_STILLPLAYING);
            let t0 = std::time::Instant::now();
            while std::ptr::read_volatile(std::ptr::addr_of!(b.flags)) & WHDR_DONE == 0 {
                assert!(t0.elapsed().as_secs() < 5, "WOM_DONE jamais atteint");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert!(a.flags & WHDR_DONE != 0 && a.flags & WHDR_INQUEUE == 0);
            let mut mmt = [TIME_SAMPLES, 0, 0];
            assert_eq!(wave_out_get_position(h, mmt.as_mut_ptr(), 12), 0);
            assert_eq!(mmt[1], 480 * 4, "3 tours de boucle + 1 en-tête");
            let mut v = 0;
            assert_eq!(wave_out_set_volume(h, 0x8000_ffff), 0);
            assert_eq!(wave_out_get_volume(h, &mut v), 0);
            assert_eq!(v & 0xffff, 0xffff);
            assert_eq!(wave_out_unprepare_header(h, pa, 48), 0);
            assert_eq!(wave_out_reset(h), 0);
            assert_eq!(wave_out_close(h), 0);
            assert_eq!(wave_out_close(h), MMSYSERR_INVALHANDLE);
        }
    }
}
