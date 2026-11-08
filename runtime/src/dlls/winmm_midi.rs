//! winmm MIDI (#237) : `midiOut*` et `midiStream*` jouent sur le
//! synthétiseur General MIDI maison (`audio::synth`), mixé par le moteur
//! audio commun comme waveOut et DirectSound.
//!
//! - Un seul périphérique de sortie, le synthétiseur (MOD_SWSYNTH), aussi
//!   joignable par MIDI_MAPPER.
//! - Flux (`midiStream*`) : tampons MIDIEVENT (delta, flux, événement,
//!   paramètres), division temporelle en PPQN ou SMPTE, tempo, événements
//!   longs (système exclusif), MEVT_F_CALLBACK → MOM_POSITIONCB, MOM_DONE à
//!   la fin de chaque tampon. Le flux s'ouvre en pause, comme sous Windows.
//! - Les rappels (fonction, événement, fenêtre, thread) suivent waveOut ;
//!   la fonction d'un programme 32 bits passe par une passerelle wow32.
//! - Entrée MIDI : aucun périphérique pour l'instant (midiInGetNumDevs = 0).

use crate::audio::{self, synth::Synth};
use crate::dlls::lock;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Appelle une fonction déjà exportée par Winlux. `None` si elle n'existe pas.
macro_rules! api {
    ($dll:literal, $name:literal, fn($($t:ty),*) -> $r:ty $(, $arg:expr)* $(,)?) => {{
        match crate::dlls::lookup($dll, $name) {
            Some(addr) => {
                let f: unsafe extern "win64" fn($($t),*) -> $r = std::mem::transmute(addr);
                Some(f($($arg),*))
            }
            None => None,
        }
    }};
}

const MMSYSERR_NOERROR: u32 = 0;
const MMSYSERR_BADDEVICEID: u32 = 2;
const MMSYSERR_INVALHANDLE: u32 = 5;
const MMSYSERR_NOTSUPPORTED: u32 = 8;
const MMSYSERR_INVALFLAG: u32 = 10;
const MMSYSERR_INVALPARAM: u32 = 11;
const MIDIERR_UNPREPARED: u32 = 64;
const MIDIERR_STILLPLAYING: u32 = 65;

const MHDR_DONE: u32 = 0x01;
const MHDR_PREPARED: u32 = 0x02;
const MHDR_INQUEUE: u32 = 0x04;
const MHDR_ISSTRM: u32 = 0x08;

const CALLBACK_TYPEMASK: u32 = 0x7_0000;
const CALLBACK_NULL: u32 = 0;
const CALLBACK_WINDOW: u32 = 0x1_0000;
const CALLBACK_THREAD: u32 = 0x2_0000;
const CALLBACK_FUNCTION: u32 = 0x3_0000;
const CALLBACK_EVENT: u32 = 0x5_0000;

const MOM_OPEN: u32 = 0x3C7;
const MOM_CLOSE: u32 = 0x3C8;
const MOM_DONE: u32 = 0x3C9;
const MOM_POSITIONCB: u32 = 0x3CA;

const MEVT_F_LONG: u32 = 0x8000_0000;
const MEVT_F_CALLBACK: u32 = 0x4000_0000;
const MEVT_SHORTMSG: u32 = 0x00;
const MEVT_TEMPO: u32 = 0x01;
const MEVT_LONGMSG: u32 = 0x80;

const MIDIPROP_SET: u32 = 0x8000_0000;
const MIDIPROP_GET: u32 = 0x4000_0000;
const MIDIPROP_TIMEDIV: u32 = 1;
const MIDIPROP_TEMPO: u32 = 2;

const TIME_MS: u32 = 0x01;
const TIME_TICKS: u32 = 0x20;

const MIDI_MAPPER: u32 = u32::MAX;

/// Nom du synthétiseur dans la langue de Paramètres.
fn device_name() -> &'static str {
    match crate::intl::locale::current().lang.iso639 {
        "fr" => "Synthétiseur Winlux",
        "de" => "Winlux-Synthesizer",
        "es" => "Sintetizador Winlux",
        _ => "Winlux Synthesizer",
    }
}

// MIDIHDR x64 : lpData @0, dwBufferLength @8, dwBytesRecorded @12,
// dwUser @16, dwFlags @24, lpNext @32, reserved @40, dwOffset @48.
// MIDIHDR x86 : lpData @0, dwBufferLength @4, dwBytesRecorded @8,
// dwUser @12, dwFlags @16, lpNext @20, reserved @24, dwOffset @28.
fn x86() -> bool {
    crate::wow32::active()
}
unsafe fn hdr_data(h: usize) -> *const u8 {
    if x86() {
        *(h as *const u32) as usize as *const u8
    } else {
        *(h as *const *const u8)
    }
}
unsafe fn hdr_len(h: usize) -> usize {
    *((h + if x86() { 4 } else { 8 }) as *const u32) as usize
}
unsafe fn hdr_recorded(h: usize) -> usize {
    *((h + if x86() { 8 } else { 12 }) as *const u32) as usize
}
unsafe fn hdr_flags(h: usize) -> *mut u32 {
    (h + if x86() { 16 } else { 24 }) as *mut u32
}
unsafe fn hdr_offset(h: usize) -> *mut u32 {
    (h + if x86() { 28 } else { 48 }) as *mut u32
}
/// Taille minimale acceptée pour cbMidiHdr (les anciens programmes
/// passent la MIDIHDR d'avant dwOffset : 8 champs).
fn min_hdr() -> u32 {
    if x86() {
        28
    } else {
        48
    }
}

#[derive(Clone, Copy)]
struct Callback {
    kind: u32,
    target: usize,
    instance: usize,
}

/// Lecture en cours d'un tampon de flux.
struct Stream {
    queue: VecDeque<usize>,
    /// Décalage dans le tampon de tête.
    offset: usize,
    /// Ticks restant avant l'événement suivant (`None` : delta pas encore lu).
    wait: Option<f64>,
    paused: bool,
    timediv: u32,
    /// Microsecondes par noire.
    tempo: u32,
    /// Ticks écoulés depuis le début (position).
    ticks: f64,
    /// Ticks joués depuis le dernier changement de tempo, et le temps
    /// correspondant, pour TIME_MS.
    ms: f64,
}

impl Stream {
    fn new() -> Stream {
        Stream {
            queue: VecDeque::new(),
            offset: 0,
            wait: None,
            paused: true,
            timediv: 96,
            tempo: 500_000,
            ticks: 0.0,
            ms: 0.0,
        }
    }

    /// Ticks par seconde selon la division et le tempo.
    fn ticks_per_second(&self) -> f64 {
        if self.timediv & 0x8000 != 0 {
            // SMPTE : -images par seconde dans l'octet haut, sous-images en bas.
            let fps = -((self.timediv >> 8) as u8 as i8) as f64;
            let fps = if fps == 29.0 { 29.97 } else { fps.max(1.0) };
            fps * (self.timediv & 0xFF).max(1) as f64
        } else {
            (self.timediv.max(1) as f64) * 1_000_000.0 / self.tempo.max(1) as f64
        }
    }
}

struct State {
    synth: Synth,
    stream: Option<Stream>,
    volume: [f32; 2],
    /// Tampons rendus et positions signalées, notifiés hors verrou.
    done: Vec<usize>,
    positions: Vec<usize>,
}

struct MidiOut {
    handle: usize,
    callback: Callback,
    source: AtomicUsize,
    st: Mutex<State>,
}

impl MidiOut {
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
            CALLBACK_THREAD if cb.target != 0 => unsafe {
                api!(
                    "user32",
                    "PostThreadMessageA",
                    fn(u32, u32, usize, isize) -> i32,
                    cb.target as u32,
                    msg,
                    self.handle,
                    p1 as isize
                );
            },
            _ => {}
        }
    }

    fn complete(&self, done: Vec<usize>, positions: Vec<usize>) {
        for h in positions {
            self.notify(MOM_POSITIONCB, h);
        }
        for h in done {
            unsafe {
                let f = hdr_flags(h);
                *f = (*f & !MHDR_INQUEUE) | MHDR_DONE;
            }
            self.notify(MOM_DONE, h);
        }
    }
}

/// Lit les événements dus du flux et les joue. Rend le nombre de trames
/// avant le prochain événement (au plus `left`).
fn step_stream(st: &mut State, left: usize) -> usize {
    let rate = audio::RATE as f64;
    loop {
        let Some(s) = st.stream.as_mut() else {
            return left;
        };
        if s.paused {
            return left;
        }
        let Some(&h) = s.queue.front() else {
            return left;
        };
        let used = unsafe { hdr_recorded(h).min(hdr_len(h)) };
        if s.offset + 12 > used {
            s.queue.pop_front();
            s.offset = 0;
            s.wait = None;
            st.done.push(h);
            continue;
        }
        let ev = unsafe { hdr_data(h).add(s.offset) as *const u32 };
        let wait = match s.wait {
            Some(w) => w,
            None => (unsafe { ev.read_unaligned() }) as f64,
        };
        if wait > 0.0 {
            let tps = s.ticks_per_second();
            let frames = (wait / tps * rate).ceil().max(1.0) as usize;
            if frames > left {
                let ticks = left as f64 * tps / rate;
                s.wait = Some(wait - ticks);
                s.ticks += ticks;
                s.ms += left as f64 * 1000.0 / rate;
                return left;
            }
            s.ticks += wait;
            s.ms += wait / tps * 1000.0;
            s.wait = Some(0.0);
            return frames;
        }
        // Événement dû.
        let event = unsafe { ev.add(2).read_unaligned() };
        let at = s.offset;
        let mut size = 12usize;
        if event & MEVT_F_LONG != 0 {
            let len = (event & 0xFF_FFFF) as usize;
            size += (len + 3) & !3;
            let data = unsafe { std::slice::from_raw_parts(ev.add(3) as *const u8, len) };
            if (event >> 24) & 0x3F == MEVT_LONGMSG & 0x3F && s.offset + 12 + len <= used {
                st.synth.sysex(data);
            }
        } else {
            match (event >> 24) & 0x3F {
                MEVT_SHORTMSG => st.synth.short(event & 0xFF_FFFF),
                MEVT_TEMPO => {
                    let t = event & 0xFF_FFFF;
                    if t > 0 {
                        s.tempo = t;
                    }
                }
                _ => {}
            }
        }
        s.offset += size;
        s.wait = None;
        if event & MEVT_F_CALLBACK != 0 {
            unsafe { *hdr_offset(h) = at as u32 };
            st.positions.push(h);
        }
    }
}

impl audio::Source for MidiOut {
    fn mix(&self, out: &mut [f32]) -> bool {
        let (done, positions) = {
            let mut st = lock(&self.st);
            let frames = out.len() / 2;
            let mut tmp = vec![0.0f32; out.len()];
            let mut pos = 0;
            while pos < frames {
                let n = step_stream(&mut st, frames - pos).clamp(1, frames - pos);
                st.synth.render(&mut tmp[2 * pos..2 * (pos + n)]);
                pos += n;
            }
            let g = st.volume;
            for (i, x) in tmp.iter().enumerate() {
                out[i] += x * g[i & 1];
            }
            (
                std::mem::take(&mut st.done),
                std::mem::take(&mut st.positions),
            )
        };
        self.complete(done, positions);
        true
    }
}

fn table() -> &'static Mutex<HashMap<usize, Arc<MidiOut>>> {
    static T: OnceLock<Mutex<HashMap<usize, Arc<MidiOut>>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get(h: usize) -> Option<Arc<MidiOut>> {
    lock(table()).get(&h).cloned()
}

fn valid_device(id: usize) -> bool {
    id == 0 || id as u32 == MIDI_MAPPER || id == usize::MAX || get(id).is_some()
}

pub extern "win64" fn midi_out_get_num_devs() -> u32 {
    1
}

pub extern "win64" fn midi_in_get_num_devs() -> u32 {
    0
}

/// MIDIOUTCAPS(A/W) : wMid, wPid, vDriverVersion, szPname[32],
/// wTechnology, wVoices, wNotes, wChannelMask, dwSupport.
unsafe fn fill_caps(caps: *mut u8, size: u32, wide: bool) -> u32 {
    let need = if wide { 84 } else { 52 };
    if caps.is_null() || size == 0 {
        return MMSYSERR_INVALPARAM;
    }
    let mut buf = vec![0u8; need];
    buf[0..2].copy_from_slice(&1u16.to_le_bytes()); // MM_MICROSOFT
    buf[2..4].copy_from_slice(&0x66u16.to_le_bytes()); // produit : synthé logiciel
    buf[4..8].copy_from_slice(&0x0100u32.to_le_bytes());
    let name = device_name();
    let end = if wide {
        for (i, c) in name.encode_utf16().take(31).enumerate() {
            buf[8 + 2 * i..10 + 2 * i].copy_from_slice(&c.to_le_bytes());
        }
        8 + 64
    } else {
        for (i, c) in super::encode_ansi(name).into_iter().take(31).enumerate() {
            buf[8 + i] = c;
        }
        8 + 32
    };
    buf[end..end + 2].copy_from_slice(&7u16.to_le_bytes()); // MOD_SWSYNTH
    buf[end + 2..end + 4].copy_from_slice(&64u16.to_le_bytes());
    buf[end + 4..end + 6].copy_from_slice(&64u16.to_le_bytes());
    buf[end + 6..end + 8].copy_from_slice(&0xFFFFu16.to_le_bytes());
    // MIDICAPS_VOLUME | MIDICAPS_LRVOLUME | MIDICAPS_STREAM.
    buf[end + 8..end + 12].copy_from_slice(&0x0Bu32.to_le_bytes());
    let n = (size as usize).min(need);
    std::ptr::copy_nonoverlapping(buf.as_ptr(), caps, n);
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn midi_out_get_dev_caps_a(id: usize, caps: *mut u8, size: u32) -> u32 {
    if !valid_device(id) {
        return MMSYSERR_BADDEVICEID;
    }
    fill_caps(caps, size, false)
}

pub unsafe extern "win64" fn midi_out_get_dev_caps_w(id: usize, caps: *mut u8, size: u32) -> u32 {
    if !valid_device(id) {
        return MMSYSERR_BADDEVICEID;
    }
    fill_caps(caps, size, true)
}

pub extern "win64" fn midi_in_bad_device() -> u32 {
    MMSYSERR_BADDEVICEID
}

pub extern "win64" fn midi_in_invalid_handle() -> u32 {
    MMSYSERR_INVALHANDLE
}

unsafe fn open(
    device: u32,
    callback: usize,
    instance: usize,
    flags: u32,
    stream: bool,
) -> Result<Arc<MidiOut>, u32> {
    if device != 0 && device != MIDI_MAPPER {
        return Err(MMSYSERR_BADDEVICEID);
    }
    let kind = flags & CALLBACK_TYPEMASK;
    if !matches!(
        kind,
        CALLBACK_NULL | CALLBACK_WINDOW | CALLBACK_THREAD | CALLBACK_FUNCTION | CALLBACK_EVENT
    ) {
        return Err(MMSYSERR_INVALFLAG);
    }
    let target = if kind == CALLBACK_FUNCTION && callback != 0 && x86() {
        crate::wow32::callback64(callback as u32)
    } else {
        callback
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0x7741_0000);
    let handle = NEXT.fetch_add(4, Ordering::Relaxed);
    let m = Arc::new(MidiOut {
        handle,
        callback: Callback {
            kind,
            target,
            instance,
        },
        source: AtomicUsize::new(0),
        st: Mutex::new(State {
            synth: Synth::new(audio::RATE),
            stream: stream.then(Stream::new),
            volume: [1.0, 1.0],
            done: Vec::new(),
            positions: Vec::new(),
        }),
    });
    lock(table()).insert(handle, m.clone());
    let id = audio::add_source(m.clone());
    m.source.store(id as usize, Ordering::Relaxed);
    m.notify(MOM_OPEN, 0);
    Ok(m)
}

pub unsafe extern "win64" fn midi_out_open(
    phmo: *mut usize,
    device: u32,
    callback: usize,
    instance: usize,
    flags: u32,
) -> u32 {
    if phmo.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    match open(device, callback, instance, flags, false) {
        Ok(m) => {
            *phmo = m.handle;
            MMSYSERR_NOERROR
        }
        Err(e) => e,
    }
}

/// midiStreamOpen(phms, puDeviceID, cMidi, dwCallback, dwInstance, fdwOpen).
pub unsafe extern "win64" fn midi_stream_open(
    phms: *mut usize,
    device: *mut u32,
    count: u32,
    callback: usize,
    instance: usize,
    flags: u32,
) -> u32 {
    if phms.is_null() || device.is_null() || count != 1 {
        return MMSYSERR_INVALPARAM;
    }
    match open(*device, callback, instance, flags, true) {
        Ok(m) => {
            *phms = m.handle;
            MMSYSERR_NOERROR
        }
        Err(e) => e,
    }
}

/// Rend tous les tampons de flux encore en file (reset, stop, close).
fn drain(m: &MidiOut) {
    let (done, positions) = {
        let mut st = lock(&m.st);
        st.synth.all_notes_off();
        let mut d = std::mem::take(&mut st.done);
        if let Some(s) = st.stream.as_mut() {
            d.extend(s.queue.drain(..));
            s.offset = 0;
            s.wait = None;
        }
        (d, std::mem::take(&mut st.positions))
    };
    m.complete(done, positions);
}

pub unsafe extern "win64" fn midi_out_close(h: usize) -> u32 {
    let Some(m) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    if lock(&m.st)
        .stream
        .as_ref()
        .is_some_and(|s| !s.queue.is_empty())
    {
        return MIDIERR_STILLPLAYING;
    }
    lock(table()).remove(&h);
    audio::remove_source(m.source.load(Ordering::Relaxed) as u64);
    m.notify(MOM_CLOSE, 0);
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn midi_stream_close(h: usize) -> u32 {
    if let Some(m) = get(h) {
        drain(&m);
    }
    midi_out_close(h)
}

pub unsafe extern "win64" fn midi_out_short_msg(h: usize, msg: u32) -> u32 {
    let Some(m) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    lock(&m.st).synth.short(msg);
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn midi_out_prepare_header(h: usize, hdr: usize, size: u32) -> u32 {
    if get(h).is_none() {
        return MMSYSERR_INVALHANDLE;
    }
    if hdr == 0 || size < min_hdr() || hdr_data(hdr).is_null() {
        return MMSYSERR_INVALPARAM;
    }
    let f = hdr_flags(hdr);
    if *f & MHDR_INQUEUE != 0 {
        return MIDIERR_STILLPLAYING;
    }
    *f = (*f & !MHDR_DONE) | MHDR_PREPARED;
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn midi_out_unprepare_header(h: usize, hdr: usize, size: u32) -> u32 {
    if get(h).is_none() {
        return MMSYSERR_INVALHANDLE;
    }
    if hdr == 0 || size < min_hdr() {
        return MMSYSERR_INVALPARAM;
    }
    let f = hdr_flags(hdr);
    if *f & MHDR_INQUEUE != 0 {
        return MIDIERR_STILLPLAYING;
    }
    *f &= !MHDR_PREPARED;
    MMSYSERR_NOERROR
}

/// midiOutLongMsg : message système exclusif, joué tout de suite.
pub unsafe extern "win64" fn midi_out_long_msg(h: usize, hdr: usize, size: u32) -> u32 {
    let Some(m) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    if hdr == 0 || size < min_hdr() {
        return MMSYSERR_INVALPARAM;
    }
    let f = hdr_flags(hdr);
    if *f & MHDR_PREPARED == 0 {
        return MIDIERR_UNPREPARED;
    }
    let data = std::slice::from_raw_parts(hdr_data(hdr), hdr_len(hdr));
    lock(&m.st).synth.sysex(data);
    *f = (*f & !MHDR_INQUEUE) | MHDR_DONE;
    m.notify(MOM_DONE, hdr);
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn midi_stream_out(h: usize, hdr: usize, size: u32) -> u32 {
    let Some(m) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    if hdr == 0 || size < min_hdr() {
        return MMSYSERR_INVALPARAM;
    }
    let f = hdr_flags(hdr);
    if *f & MHDR_PREPARED == 0 {
        return MIDIERR_UNPREPARED;
    }
    if *f & MHDR_INQUEUE != 0 {
        return MIDIERR_STILLPLAYING;
    }
    let mut st = lock(&m.st);
    let Some(s) = st.stream.as_mut() else {
        return MMSYSERR_NOTSUPPORTED;
    };
    *f = (*f & !MHDR_DONE) | MHDR_INQUEUE | MHDR_ISSTRM;
    s.queue.push_back(hdr);
    MMSYSERR_NOERROR
}

fn with_stream(h: usize, f: impl FnOnce(&mut Stream)) -> u32 {
    let Some(m) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    let mut st = lock(&m.st);
    match st.stream.as_mut() {
        Some(s) => {
            f(s);
            MMSYSERR_NOERROR
        }
        None => MMSYSERR_INVALHANDLE,
    }
}

pub extern "win64" fn midi_stream_restart(h: usize) -> u32 {
    with_stream(h, |s| s.paused = false)
}

pub extern "win64" fn midi_stream_pause(h: usize) -> u32 {
    let r = with_stream(h, |s| s.paused = true);
    if let Some(m) = get(h) {
        lock(&m.st).synth.all_notes_off();
    }
    r
}

pub extern "win64" fn midi_stream_stop(h: usize) -> u32 {
    let Some(m) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    drain(&m);
    with_stream(h, |s| {
        s.paused = true;
        s.ticks = 0.0;
        s.ms = 0.0;
    })
}

pub unsafe extern "win64" fn midi_out_reset(h: usize) -> u32 {
    let Some(m) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    drain(&m);
    MMSYSERR_NOERROR
}

/// midiStreamProperty(hms, lppropdata, dwProperty) : MIDIPROPTIMEDIV et
/// MIDIPROPTEMPO valent { cbStruct, valeur }.
pub unsafe extern "win64" fn midi_stream_property(h: usize, data: *mut u32, prop: u32) -> u32 {
    if data.is_null() || (prop & (MIDIPROP_SET | MIDIPROP_GET)) == 0 {
        return MMSYSERR_INVALPARAM;
    }
    let which = prop & !(MIDIPROP_SET | MIDIPROP_GET);
    if which != MIDIPROP_TIMEDIV && which != MIDIPROP_TEMPO {
        return MMSYSERR_INVALPARAM;
    }
    let value = data.add(1);
    with_stream(h, |s| {
        let slot = if which == MIDIPROP_TIMEDIV {
            &mut s.timediv
        } else {
            &mut s.tempo
        };
        if prop & MIDIPROP_SET != 0 {
            *slot = *value;
        } else {
            *value = *slot;
        }
    })
}

/// midiStreamPosition : MMTIME { wType; union u } (même disposition en x86).
pub unsafe extern "win64" fn midi_stream_position(h: usize, mmt: *mut u32, size: u32) -> u32 {
    if mmt.is_null() || size < 8 {
        return MMSYSERR_INVALPARAM;
    }
    with_stream(h, |s| match *mmt {
        TIME_TICKS => *mmt.add(1) = s.ticks as u32,
        _ => {
            *mmt = TIME_MS;
            *mmt.add(1) = s.ms as u32;
        }
    })
}

fn pack_volume(v: [f32; 2]) -> u32 {
    let l = (v[0].clamp(0.0, 1.0) * 65535.0).round() as u32;
    let r = (v[1].clamp(0.0, 1.0) * 65535.0).round() as u32;
    l | r << 16
}

pub unsafe extern "win64" fn midi_out_get_volume(h: usize, out: *mut u32) -> u32 {
    if out.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    match get(h) {
        Some(m) => {
            *out = pack_volume(lock(&m.st).volume);
            MMSYSERR_NOERROR
        }
        None if valid_device(h) => {
            *out = 0xFFFF_FFFF;
            MMSYSERR_NOERROR
        }
        None => MMSYSERR_INVALHANDLE,
    }
}

pub unsafe extern "win64" fn midi_out_set_volume(h: usize, v: u32) -> u32 {
    let Some(m) = get(h) else {
        return if valid_device(h) {
            MMSYSERR_NOERROR
        } else {
            MMSYSERR_INVALHANDLE
        };
    };
    lock(&m.st).volume = [(v & 0xFFFF) as f32 / 65535.0, (v >> 16) as f32 / 65535.0];
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn midi_out_get_id(h: usize, out: *mut u32) -> u32 {
    if out.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    if get(h).is_none() {
        return MMSYSERR_INVALHANDLE;
    }
    *out = 0;
    MMSYSERR_NOERROR
}

/// midiOutCachePatches / midiOutCacheDrumPatches : tous les instruments
/// sont toujours disponibles.
pub extern "win64" fn midi_out_cache_patches(
    h: usize,
    _bank: u32,
    _patches: usize,
    _flags: u32,
) -> u32 {
    if get(h).is_none() {
        return MMSYSERR_INVALHANDLE;
    }
    MMSYSERR_NOERROR
}

pub extern "win64" fn midi_out_message(_h: usize, _msg: u32, _p1: usize, _p2: usize) -> u32 {
    MMSYSERR_NOTSUPPORTED
}

fn error_text(code: u32) -> &'static str {
    match code {
        MMSYSERR_NOERROR => "No error.",
        MMSYSERR_BADDEVICEID => "The device ID is out of range.",
        MMSYSERR_INVALHANDLE => "The handle is invalid.",
        MMSYSERR_NOTSUPPORTED => "The function is not supported.",
        MMSYSERR_INVALFLAG => "An invalid flag was passed.",
        MMSYSERR_INVALPARAM => "An invalid parameter was passed.",
        MIDIERR_UNPREPARED => "The header has not been prepared.",
        MIDIERR_STILLPLAYING => "The buffer is still in use.",
        _ => "Unknown error.",
    }
}

pub unsafe extern "win64" fn midi_out_get_error_text_a(code: u32, buf: *mut u8, cap: u32) -> u32 {
    if buf.is_null() || cap == 0 {
        return MMSYSERR_INVALPARAM;
    }
    let t = error_text(code).as_bytes();
    let n = t.len().min(cap as usize - 1);
    std::ptr::copy_nonoverlapping(t.as_ptr(), buf, n);
    *buf.add(n) = 0;
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn midi_out_get_error_text_w(code: u32, buf: *mut u16, cap: u32) -> u32 {
    if buf.is_null() || cap == 0 {
        return MMSYSERR_INVALPARAM;
    }
    let t: Vec<u16> = error_text(code).encode_utf16().collect();
    let n = t.len().min(cap as usize - 1);
    std::ptr::copy_nonoverlapping(t.as_ptr(), buf, n);
    *buf.add(n) = 0;
    MMSYSERR_NOERROR
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "midiOutGetNumDevs" => fn midi_out_get_num_devs,
        "midiOutGetDevCapsA" => fn midi_out_get_dev_caps_a,
        "midiOutGetDevCapsW" => fn midi_out_get_dev_caps_w,
        "midiOutOpen" => fn midi_out_open,
        "midiOutClose" => fn midi_out_close,
        "midiOutShortMsg" => fn midi_out_short_msg,
        "midiOutLongMsg" => fn midi_out_long_msg,
        "midiOutPrepareHeader" => fn midi_out_prepare_header,
        "midiOutUnprepareHeader" => fn midi_out_unprepare_header,
        "midiOutReset" => fn midi_out_reset,
        "midiOutGetVolume" => fn midi_out_get_volume,
        "midiOutSetVolume" => fn midi_out_set_volume,
        "midiOutGetID" => fn midi_out_get_id,
        "midiOutCachePatches" => fn midi_out_cache_patches,
        "midiOutCacheDrumPatches" => fn midi_out_cache_patches,
        "midiOutMessage" => fn midi_out_message,
        "midiOutGetErrorTextA" => fn midi_out_get_error_text_a,
        "midiOutGetErrorTextW" => fn midi_out_get_error_text_w,
        "midiStreamOpen" => fn midi_stream_open,
        "midiStreamClose" => fn midi_stream_close,
        "midiStreamOut" => fn midi_stream_out,
        "midiStreamRestart" => fn midi_stream_restart,
        "midiStreamPause" => fn midi_stream_pause,
        "midiStreamStop" => fn midi_stream_stop,
        "midiStreamProperty" => fn midi_stream_property,
        "midiStreamPosition" => fn midi_stream_position,
        "midiInGetNumDevs" => fn midi_in_get_num_devs,
        "midiInOpen" => fn midi_in_bad_device,
        "midiInGetDevCapsA" => fn midi_in_bad_device,
        "midiInGetDevCapsW" => fn midi_in_bad_device,
        "midiInClose" => fn midi_in_invalid_handle,
        "midiInPrepareHeader" => fn midi_in_invalid_handle,
        "midiInUnprepareHeader" => fn midi_in_invalid_handle,
        "midiInAddBuffer" => fn midi_in_invalid_handle,
        "midiInStart" => fn midi_in_invalid_handle,
        "midiInStop" => fn midi_in_invalid_handle,
        "midiInReset" => fn midi_in_invalid_handle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(delta: u32, ev: u32) -> [u32; 3] {
        [delta, 0, ev]
    }

    #[test]
    fn flux_joue_les_evenements_au_bon_moment() {
        let mut st = State {
            synth: Synth::new(audio::RATE),
            stream: Some(Stream::new()),
            volume: [1.0, 1.0],
            done: Vec::new(),
            positions: Vec::new(),
        };
        // 96 PPQN, 500 000 µs par noire : 192 ticks par seconde.
        let events: Vec<u32> = [
            event(0, 0x7F_3C_90),
            event(96, MEVT_F_CALLBACK | 0x00_3C_80),
            event(0, MEVT_TEMPO << 24 | 250_000),
            event(96, 0x7F_40_90),
        ]
        .concat();
        let mut hdr = [0u8; 120];
        let p = events.as_ptr() as usize;
        hdr[0..8].copy_from_slice(&(p as u64).to_le_bytes());
        let len = (events.len() * 4) as u32;
        hdr[8..12].copy_from_slice(&len.to_le_bytes());
        hdr[12..16].copy_from_slice(&len.to_le_bytes());
        let h = hdr.as_mut_ptr() as usize;
        {
            let s = st.stream.as_mut().unwrap();
            s.paused = false;
            s.queue.push_back(h);
        }
        // La première note part tout de suite, la suivante à 0,5 s.
        let n = step_stream(&mut st, 48_000);
        assert_eq!(st.synth.active_voices(), 1);
        assert_eq!(n, 24_000);
        let n = step_stream(&mut st, 48_000 - n);
        // Note off + tempo doublé : la dernière note à 0,25 s de plus.
        assert_eq!(st.positions, vec![h]);
        assert_eq!(n, 12_000);
        step_stream(&mut st, 12_000);
        assert!(st.done.contains(&h));
        assert_eq!(st.stream.as_ref().unwrap().tempo, 250_000);
    }

    #[test]
    fn division_smpte() {
        let mut s = Stream::new();
        s.timediv = 0xE728; // -25 images/s, 40 sous-images
        assert_eq!(s.ticks_per_second(), 1000.0);
    }
}
