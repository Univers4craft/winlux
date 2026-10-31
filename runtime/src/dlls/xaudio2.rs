//! XAudio2 (xaudio2_7 par COM, xaudio2_8, xaudio2_9) écrit de zéro.
//!
//! Graphe de voix : voix sources (tampons XAUDIO2_BUFFER, boucles, rapport
//! de fréquence, rappels IXAudio2VoiceCallback), voix de sous-mixage
//! triées par étage, voix de mastering. Matrices de sortie, volumes, filtres
//! d'état (passe-bas/haut/bande/réjection, un pôle) et deux effets intégrés :
//! réverbération et vumètre (CreateAudioReverb / CreateAudioVolumeMeter).
//!
//! Le moteur traite 10 ms à chaque passe, au rythme du mixeur Winlux. Les
//! « operation sets » (CommitChanges) sont appliqués immédiatement.

use super::audio_com::*;
use super::ole32::Guid;
use crate::audio::{self, dsp, format::Format};
use crate::dlls::lock;
use std::collections::{HashMap, VecDeque};
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const XAUDIO2_E_INVALID_CALL: i32 = 0x8896_0001u32 as i32;

const XAUDIO2_VOICE_NOSAMPLESPLAYED: u32 = 0x100;
const XAUDIO2_VOICE_USEFILTER: u32 = 0x8;
const XAUDIO2_SEND_USEFILTER: u32 = 0x80;
const XAUDIO2_END_OF_STREAM: u32 = 0x40;
const XAUDIO2_LOOP_INFINITE: u32 = 255;
const XAUDIO2_MAX_QUEUED_BUFFERS: usize = 64;
const XAUDIO2_MAX_FREQ_RATIO: f32 = 1024.0;

const PERIOD: usize = audio::PERIOD;

const CLSID_XAUDIO2_27: Guid = guid(
    0x5a508685,
    0xa254,
    0x4fba,
    [0x9b, 0x82, 0x9a, 0x24, 0xb0, 0x03, 0x06, 0xaf],
);
const CLSID_XAUDIO2_27_DEBUG: Guid = guid(
    0xdb05ea35,
    0x0329,
    0x4d4b,
    [0xa5, 0x3a, 0x6d, 0xea, 0xd0, 0x3d, 0x38, 0x52],
);
const CLSID_AUDIOREVERB_27: Guid = guid(
    0x6a93130e,
    0x1d53,
    0x41d1,
    [0xa9, 0xcf, 0xe7, 0x58, 0x80, 0x0b, 0xb1, 0x79],
);
const CLSID_AUDIOVOLUMEMETER_27: Guid = guid(
    0xcac1105f,
    0x619b,
    0x4d04,
    [0x83, 0x1a, 0x44, 0xe1, 0xcb, 0xf1, 0x2d, 0x57],
);
const IID_IXAUDIO2_27: Guid = guid(
    0x8bcf1f58,
    0x9fe7,
    0x4583,
    [0x8a, 0xc6, 0xe2, 0xad, 0xc4, 0x65, 0xc8, 0xbb],
);
const IID_IXAUDIO2_28: Guid = guid(
    0x60d8dac8,
    0x5aa1,
    0x4e8e,
    [0xb5, 0x97, 0x2f, 0x5e, 0x28, 0x83, 0xd4, 0x84],
);
const IID_IXAUDIO2_29: Guid = guid(
    0x2b02e3cf,
    0x2e0b,
    0x4ec3,
    [0xbe, 0x45, 0x1b, 0x2a, 0x3f, 0xe7, 0x21, 0x0d],
);

// ---------------------------------------------------------------------------
// Effets intégrés
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FxKind {
    Reverb,
    Meter,
}

/// Objet effet renvoyé par CreateAudioReverb / CreateAudioVolumeMeter.
struct FxObject {
    kind: FxKind,
}

unsafe extern "win64" fn fx_qi(
    this: *mut Com<FxObject>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if read_guid(riid) == Some(IID_IUNKNOWN) {
        Com::add_ref(this);
        return put(out, this);
    }
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOINTERFACE
}
unsafe extern "win64" fn fx_add_ref(this: *mut Com<FxObject>) -> u32 {
    Com::add_ref(this)
}
unsafe extern "win64" fn fx_release(this: *mut Com<FxObject>) -> u32 {
    Com::release(this)
}
static FX_VT: VTable<3> = VTable([
    fx_qi as *const (),
    fx_add_ref as *const (),
    fx_release as *const (),
]);

fn new_fx(kind: FxKind) -> *mut Com<FxObject> {
    Com::create(FX_VT.ptr(), FxObject { kind })
}

unsafe fn fx_kind(obj: *mut c_void) -> Option<FxKind> {
    if obj.is_null() || *(obj as *const *const ()) != FX_VT.ptr() {
        return None;
    }
    Some(Com::get(obj as *mut Com<FxObject>).kind)
}

enum FxState {
    Reverb(dsp::Reverb),
    Meter {
        peak: [f32; 2],
        rms: [f32; 2],
    },
    /// XAPO du programme : accepté mais non traité (son inchangé).
    External,
}

struct Effect {
    object: usize,
    enabled: bool,
    state: FxState,
}

impl Effect {
    fn process(&mut self, buf: &mut [f32]) {
        if !self.enabled {
            return;
        }
        match &mut self.state {
            FxState::Reverb(r) => r.process(buf),
            FxState::Meter { peak, rms } => {
                let mut sum = [0.0f32; 2];
                *peak = [0.0; 2];
                for fr in buf.chunks_exact(2) {
                    for c in 0..2 {
                        peak[c] = peak[c].max(fr[c].abs());
                        sum[c] += fr[c] * fr[c];
                    }
                }
                let n = (buf.len() / 2).max(1) as f32;
                *rms = [(sum[0] / n).sqrt(), (sum[1] / n).sqrt()];
            }
            FxState::External => {}
        }
    }
}

impl Drop for Effect {
    fn drop(&mut self) {
        unsafe { unknown_release(self.object as *mut c_void) }
    }
}

// ---------------------------------------------------------------------------
// Graphe de voix
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
struct Matrix2 {
    ll: f32,
    rl: f32,
    lr: f32,
    rr: f32,
}

struct Send {
    dest: usize,
    /// Matrice brute (destination × source) fixée par SetOutputMatrix.
    matrix: Option<(u32, u32, Vec<f32>)>,
    filter: Option<dsp::Filter>,
}

struct XBuf {
    flags: u32,
    data: *const u8,
    bytes: usize,
    end: usize,
    loop_begin: usize,
    loop_end: usize,
    loops_left: u32,
    ctx: usize,
    pos: usize,
    started: bool,
}
unsafe impl std::marker::Send for XBuf {}

struct SourceState {
    format: Format,
    queue: VecDeque<XBuf>,
    playing: bool,
    callback: usize,
    ratio: f32,
    max_ratio: f32,
    resampler: dsp::Resampler,
    samples_played: u64,
    /// Rappels OnBufferEnd en attente (FlushSourceBuffers).
    flushed: Vec<usize>,
}

enum Role {
    Source(Box<SourceState>),
    Submix { stage: u32 },
    Master,
}

struct VoiceState {
    role: Role,
    channels: u32,
    rate: u32,
    flags: u32,
    volume: f32,
    channel_volumes: Vec<f32>,
    sends: Vec<Send>,
    filter: dsp::Filter,
    effects: Vec<Effect>,
    mix: Vec<f32>,
}

impl VoiceState {
    fn gains(&self) -> [f32; 2] {
        let c0 = self.channel_volumes.first().copied().unwrap_or(1.0);
        let c1 = self.channel_volumes.get(1).copied().unwrap_or(c0);
        [self.volume * c0, self.volume * c1]
    }
}

#[derive(Default)]
struct Graph {
    voices: HashMap<usize, VoiceState>,
    master: Option<usize>,
    started: bool,
    engine_callbacks: Vec<usize>,
}

struct XaShared {
    graph: Mutex<Graph>,
    source: AtomicUsize,
}

enum Ev {
    BufferStart(usize, usize),
    BufferEnd(usize, usize),
    LoopEnd(usize, usize),
    StreamEnd(usize),
    PassEnd(usize),
}

/// Matrice 2×2 équivalente à une matrice XAudio2 (source × destination),
/// appliquée sur la représentation stéréo interne de la voix.
fn collapse(src: u32, dst: u32, m: &[f32]) -> Matrix2 {
    let at = |d: u32, s: u32| -> f32 {
        if d < dst && s < src {
            m.get((d * src + s) as usize).copied().unwrap_or(0.0)
        } else {
            0.0
        }
    };
    let center = if dst >= 3 {
        std::f32::consts::FRAC_1_SQRT_2
    } else {
        0.0
    };
    if dst == 1 {
        let (a, b) = if src == 1 {
            (at(0, 0) * 0.5, at(0, 0) * 0.5)
        } else {
            (at(0, 0), at(0, 1))
        };
        return Matrix2 {
            ll: a,
            rl: b,
            lr: a,
            rr: b,
        };
    }
    if src == 1 {
        // Mono : la voix porte x dans L et R.
        let l = at(0, 0) + at(2, 0) * center;
        let r = at(1, 0) + at(2, 0) * center;
        return Matrix2 {
            ll: l,
            rl: 0.0,
            lr: 0.0,
            rr: r,
        };
    }
    Matrix2 {
        ll: at(0, 0) + at(2, 0) * center,
        rl: at(0, 1) + at(2, 1) * center,
        lr: at(1, 0) + at(2, 0) * center,
        rr: at(1, 1) + at(2, 1) * center,
    }
}

fn default_matrix(dest_channels: u32) -> Matrix2 {
    if dest_channels == 1 {
        Matrix2 {
            ll: 0.5,
            rl: 0.5,
            lr: 0.5,
            rr: 0.5,
        }
    } else {
        Matrix2 {
            ll: 1.0,
            rl: 0.0,
            lr: 0.0,
            rr: 1.0,
        }
    }
}

impl XaShared {
    fn send_to(graph: &mut Graph, from: &VoiceState, buf: &mut [f32]) -> Vec<(usize, Vec<f32>)> {
        let mut outs = Vec::new();
        for s in &from.sends {
            let Some(dest) = graph.voices.get(&s.dest) else {
                continue;
            };
            let m = match &s.matrix {
                Some((src, dst, m)) => collapse(*src, *dst, m),
                None => default_matrix(dest.channels),
            };
            let mut out = vec![0.0f32; buf.len()];
            for (o, i) in out.chunks_exact_mut(2).zip(buf.chunks_exact(2)) {
                o[0] = i[0] * m.ll + i[1] * m.rl;
                o[1] = i[0] * m.lr + i[1] * m.rr;
            }
            if let Some(mut f) = s.filter {
                f.process(&mut out);
            }
            outs.push((s.dest, out));
        }
        let _ = buf;
        outs
    }

    /// Passe de traitement : renvoie la sortie du mastering (stéréo) et les
    /// événements de rappel, déjà ordonnés.
    fn process(&self, out: &mut [f32]) -> (Vec<Ev>, Vec<usize>) {
        let mut events = Vec::new();
        let mut cbs = Vec::new();
        let mut g = lock(&self.graph);
        let Some(master) = g.master else {
            return (events, cbs);
        };
        if !g.started {
            return (events, cbs);
        }
        cbs = g.engine_callbacks.clone();
        for v in g.voices.values_mut() {
            v.mix.iter_mut().for_each(|x| *x = 0.0);
            if v.mix.len() != out.len() {
                v.mix = vec![0.0; out.len()];
            }
        }
        // Voix sources.
        let ids: Vec<usize> = g
            .voices
            .iter()
            .filter(|(_, v)| matches!(v.role, Role::Source(_)))
            .map(|(k, _)| *k)
            .collect();
        let mut tmp = vec![0.0f32; out.len()];
        for id in ids {
            let Some(mut v) = g.voices.remove(&id) else {
                continue;
            };
            tmp.fill(0.0);
            let mut active = false;
            if let Role::Source(src) = &mut v.role {
                for ctx in src.flushed.drain(..) {
                    events.push(Ev::BufferEnd(src.callback, ctx));
                }
                if src.playing {
                    active = true;
                    render_source(src, &mut tmp, &mut events);
                    if src.callback != 0 {
                        events.push(Ev::PassEnd(src.callback));
                    }
                }
            }
            if active {
                if v.flags & XAUDIO2_VOICE_USEFILTER != 0 {
                    v.filter.process(&mut tmp);
                }
                let gn = v.gains();
                for fr in tmp.chunks_exact_mut(2) {
                    fr[0] *= gn[0];
                    fr[1] *= gn[1];
                }
                for e in v.effects.iter_mut() {
                    e.process(&mut tmp);
                }
                for (dest, data) in Self::send_to(&mut g, &v, &mut tmp) {
                    if let Some(d) = g.voices.get_mut(&dest) {
                        for (a, b) in d.mix.iter_mut().zip(data) {
                            *a += b;
                        }
                    }
                }
            }
            g.voices.insert(id, v);
        }
        // Sous-mixages par étage croissant.
        let mut subs: Vec<(u32, usize)> = g
            .voices
            .iter()
            .filter_map(|(k, v)| match v.role {
                Role::Submix { stage } => Some((stage, *k)),
                _ => None,
            })
            .collect();
        subs.sort();
        for (_, id) in subs {
            let Some(mut v) = g.voices.remove(&id) else {
                continue;
            };
            let mut buf = std::mem::take(&mut v.mix);
            if v.flags & XAUDIO2_VOICE_USEFILTER != 0 {
                v.filter.process(&mut buf);
            }
            let gn = v.gains();
            for fr in buf.chunks_exact_mut(2) {
                fr[0] *= gn[0];
                fr[1] *= gn[1];
            }
            for e in v.effects.iter_mut() {
                e.process(&mut buf);
            }
            for (dest, data) in Self::send_to(&mut g, &v, &mut buf) {
                if let Some(d) = g.voices.get_mut(&dest) {
                    for (a, b) in d.mix.iter_mut().zip(data) {
                        *a += b;
                    }
                }
            }
            v.mix = buf;
            g.voices.insert(id, v);
        }
        // Mastering.
        if let Some(m) = g.voices.get_mut(&master) {
            let gn = m.gains();
            let mut buf = std::mem::take(&mut m.mix);
            for fr in buf.chunks_exact_mut(2) {
                fr[0] *= gn[0];
                fr[1] *= gn[1];
            }
            for e in m.effects.iter_mut() {
                e.process(&mut buf);
            }
            for (o, b) in out.iter_mut().zip(buf.iter()) {
                *o += *b;
            }
            m.mix = buf;
        }
        (events, cbs)
    }
}

fn render_source(src: &mut SourceState, tmp: &mut [f32], events: &mut Vec<Ev>) {
    let step = src.format.rate as f64 * src.ratio as f64 / audio::RATE as f64;
    let mut rs = std::mem::take(&mut src.resampler);
    let fmt = src.format;
    let block = fmt.block as usize;
    let cb = src.callback;
    {
        let queue = &mut src.queue;
        let played = &mut src.samples_played;
        let mut pull = || loop {
            let b = queue.front_mut()?;
            if !b.started {
                b.started = true;
                events.push(Ev::BufferStart(cb, b.ctx));
            }
            let limit = if b.loops_left > 0 { b.loop_end } else { b.end };
            if b.pos < limit {
                let o = b.pos * block;
                if o + block > b.bytes {
                    b.pos = limit;
                    continue;
                }
                let frame = unsafe { std::slice::from_raw_parts(b.data.add(o), block) };
                b.pos += 1;
                *played += 1;
                return Some(fmt.stereo(frame));
            }
            if b.loops_left > 0 {
                if b.loops_left != XAUDIO2_LOOP_INFINITE {
                    b.loops_left -= 1;
                }
                b.pos = b.loop_begin;
                events.push(Ev::LoopEnd(cb, b.ctx));
                continue;
            }
            events.push(Ev::BufferEnd(cb, b.ctx));
            if b.flags & XAUDIO2_END_OF_STREAM != 0 {
                events.push(Ev::StreamEnd(cb));
            }
            queue.pop_front();
        };
        rs.mix_into(step, tmp, [1.0, 1.0], &mut pull);
    }
    src.resampler = rs;
}

impl audio::Source for XaShared {
    fn mix(&self, out: &mut [f32]) -> bool {
        // 1. Début de passe : rappels sans verrou (le programme y soumet
        //    souvent ses tampons).
        let (engine_cbs, voice_cbs) = {
            let g = lock(&self.graph);
            if !g.started || g.master.is_none() {
                return true;
            }
            let mut v = Vec::new();
            for vs in g.voices.values() {
                if let Role::Source(s) = &vs.role {
                    if s.playing && s.callback != 0 {
                        let need = (PERIOD as f64 * s.format.rate as f64 * s.ratio as f64
                            / audio::RATE as f64)
                            .ceil() as usize
                            + 2;
                        let queued: usize = s
                            .queue
                            .iter()
                            .map(|b| {
                                if b.loops_left > 0 {
                                    usize::MAX / 4
                                } else {
                                    b.end.saturating_sub(b.pos)
                                }
                            })
                            .fold(0usize, |a, b| a.saturating_add(b));
                        let bytes = need.saturating_sub(queued) * s.format.block as usize;
                        v.push((s.callback, bytes));
                    }
                }
            }
            (g.engine_callbacks.clone(), v)
        };
        if !engine_cbs.is_empty() || !voice_cbs.is_empty() {
            audio::ensure_windows_thread();
        }
        unsafe {
            for &cb in &engine_cbs {
                call0(cb as *mut c_void, 0); // OnProcessingPassStart
            }
            for &(cb, bytes) in &voice_cbs {
                call1(cb as *mut c_void, 0, bytes); // OnVoiceProcessingPassStart
            }
        }
        // 2. Traitement sous verrou.
        let (events, cbs) = self.process(out);
        // 3. Rappels de fin, sans verrou.
        if !events.is_empty() {
            audio::ensure_windows_thread();
        }
        unsafe {
            for e in events {
                match e {
                    Ev::BufferStart(cb, ctx) if cb != 0 => call1(cb as *mut c_void, 3, ctx),
                    Ev::BufferEnd(cb, ctx) if cb != 0 => call1(cb as *mut c_void, 4, ctx),
                    Ev::LoopEnd(cb, ctx) if cb != 0 => call1(cb as *mut c_void, 5, ctx),
                    Ev::StreamEnd(cb) if cb != 0 => call0(cb as *mut c_void, 2),
                    Ev::PassEnd(cb) => call0(cb as *mut c_void, 1),
                    _ => {}
                }
            }
            for &cb in &cbs {
                call0(cb as *mut c_void, 1); // OnProcessingPassEnd
            }
        }
        true
    }
}

// ---------------------------------------------------------------------------
// IXAudio2
// ---------------------------------------------------------------------------

struct Engine {
    sh: Arc<XaShared>,
    version: u8,
    source: u64,
}

impl Drop for Engine {
    fn drop(&mut self) {
        audio::remove_source(self.source);
        let mut g = lock(&self.sh.graph);
        g.started = false;
        g.engine_callbacks.clear();
    }
}

fn new_engine(version: u8) -> *mut Com<Engine> {
    let sh = Arc::new(XaShared {
        graph: Mutex::new(Graph::default()),
        source: AtomicUsize::new(0),
    });
    let source = audio::add_source(sh.clone());
    sh.source.store(source as usize, Ordering::Relaxed);
    let vt = if version == 7 {
        XA27_VT.ptr()
    } else {
        XA_VT.ptr()
    };
    Com::create(
        vt,
        Engine {
            sh,
            version,
            source,
        },
    )
}

unsafe extern "win64" fn xa_qi(
    this: *mut Com<Engine>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    match read_guid(riid) {
        Some(g)
            if g == IID_IUNKNOWN
                || g == IID_IXAUDIO2_27
                || g == IID_IXAUDIO2_28
                || g == IID_IXAUDIO2_29 =>
        {
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
unsafe extern "win64" fn xa_add_ref(this: *mut Com<Engine>) -> u32 {
    Com::add_ref(this)
}
unsafe extern "win64" fn xa_release(this: *mut Com<Engine>) -> u32 {
    Com::release(this)
}
unsafe extern "win64" fn xa_register(this: *mut Com<Engine>, cb: *mut c_void) -> i32 {
    if cb.is_null() {
        return E_INVALIDARG;
    }
    let mut g = lock(&Com::get(this).sh.graph);
    if !g.engine_callbacks.contains(&(cb as usize)) {
        g.engine_callbacks.push(cb as usize);
    }
    S_OK
}
unsafe extern "win64" fn xa_unregister(this: *mut Com<Engine>, cb: *mut c_void) {
    lock(&Com::get(this).sh.graph)
        .engine_callbacks
        .retain(|c| *c != cb as usize);
}

/// Voix renvoyée au programme : table virtuelle, moteur et identifiant.
#[repr(C)]
struct Voice {
    vtbl: *const (),
    sh: *const XaShared,
    version: u8,
}

unsafe fn voice_id(v: *mut Voice) -> usize {
    v as usize
}
unsafe fn shared<'a>(v: *mut Voice) -> &'a XaShared {
    &*(*v).sh
}

unsafe fn parse_sends(g: &Graph, list: *const u8) -> Result<Vec<Send>, i32> {
    if list.is_null() {
        return Ok(g
            .master
            .map(|m| Send {
                dest: m,
                matrix: None,
                filter: None,
            })
            .into_iter()
            .collect());
    }
    let count = *(list as *const u32) as usize;
    let sends = *(list.add(8) as *const *const u8);
    if count > 0 && sends.is_null() {
        return Err(E_INVALIDARG);
    }
    let mut v = Vec::with_capacity(count);
    for i in 0..count {
        let flags = *(sends.add(i * 16) as *const u32);
        let dest = *(sends.add(i * 16 + 8) as *const usize);
        if !g.voices.contains_key(&dest) || matches!(g.voices[&dest].role, Role::Source(_)) {
            return Err(E_INVALIDARG);
        }
        v.push(Send {
            dest,
            matrix: None,
            filter: (flags & XAUDIO2_SEND_USEFILTER != 0).then(dsp::Filter::default),
        });
    }
    Ok(v)
}

unsafe fn parse_effects(chain: *const u8) -> Result<Vec<Effect>, i32> {
    if chain.is_null() {
        return Ok(Vec::new());
    }
    let count = *(chain as *const u32) as usize;
    let desc = *(chain.add(8) as *const *const u8);
    if count > 0 && desc.is_null() {
        return Err(E_INVALIDARG);
    }
    let mut v = Vec::with_capacity(count);
    for i in 0..count {
        let obj = *(desc.add(i * 16) as *const *mut c_void);
        let enabled = *(desc.add(i * 16 + 8) as *const i32) != 0;
        if obj.is_null() {
            return Err(E_INVALIDARG);
        }
        unknown_add_ref(obj);
        let state = match fx_kind(obj) {
            Some(FxKind::Reverb) => FxState::Reverb(dsp::Reverb::new(audio::RATE)),
            Some(FxKind::Meter) => FxState::Meter {
                peak: [0.0; 2],
                rms: [0.0; 2],
            },
            None => {
                crate::log_warn!("xaudio2", "effet XAPO externe ignoré (son inchangé)");
                FxState::External
            }
        };
        v.push(Effect {
            object: obj as usize,
            enabled,
            state,
        });
    }
    Ok(v)
}

fn new_voice_state(
    role: Role,
    channels: u32,
    rate: u32,
    flags: u32,
    sends: Vec<Send>,
    effects: Vec<Effect>,
) -> VoiceState {
    VoiceState {
        role,
        channels,
        rate,
        flags,
        volume: 1.0,
        channel_volumes: vec![1.0; channels as usize],
        sends,
        filter: dsp::Filter::default(),
        effects,
        mix: vec![0.0; PERIOD * 2],
    }
}

unsafe fn alloc_voice(this: *mut Com<Engine>, vt: *const ()) -> *mut Voice {
    let e = Com::get(this);
    Box::into_raw(Box::new(Voice {
        vtbl: vt,
        sh: Arc::into_raw(e.sh.clone()),
        version: e.version,
    }))
}

unsafe extern "win64" fn xa_create_source(
    this: *mut Com<Engine>,
    out: *mut *mut Voice,
    wfx: *const u8,
    flags: u32,
    max_ratio: f32,
    callback: *mut c_void,
    sends: *const u8,
    effects: *const u8,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(format) = Format::from_ptr(wfx) else {
        return if wfx.is_null() {
            E_POINTER
        } else {
            XAUDIO2_E_INVALID_CALL
        };
    };
    let e = Com::get(this);
    let g = lock(&e.sh.graph);
    if g.master.is_none() {
        return XAUDIO2_E_INVALID_CALL;
    }
    let sends = match parse_sends(&g, sends) {
        Ok(s) => s,
        Err(e) => return e,
    };
    drop(g);
    let effects = match parse_effects(effects) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let vt = if e.version == 7 {
        SOURCE27_VT.ptr()
    } else {
        SOURCE_VT.ptr()
    };
    let v = alloc_voice(this, vt);
    let max_ratio = if max_ratio.is_finite() && max_ratio > 0.0 {
        max_ratio.min(XAUDIO2_MAX_FREQ_RATIO)
    } else {
        XAUDIO2_MAX_FREQ_RATIO
    };
    let state = new_voice_state(
        Role::Source(Box::new(SourceState {
            format,
            queue: VecDeque::new(),
            playing: false,
            callback: callback as usize,
            ratio: 1.0,
            max_ratio,
            resampler: dsp::Resampler::new(),
            samples_played: 0,
            flushed: Vec::new(),
        })),
        format.channels as u32,
        format.rate,
        flags,
        sends,
        effects,
    );
    lock(&e.sh.graph).voices.insert(voice_id(v), state);
    *out = v;
    S_OK
}

unsafe extern "win64" fn xa_create_submix(
    this: *mut Com<Engine>,
    out: *mut *mut Voice,
    channels: u32,
    rate: u32,
    flags: u32,
    stage: u32,
    sends: *const u8,
    effects: *const u8,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if !(1..=64).contains(&channels) || !(1000..=200_000).contains(&rate) {
        return E_INVALIDARG;
    }
    let e = Com::get(this);
    let g = lock(&e.sh.graph);
    if g.master.is_none() {
        return XAUDIO2_E_INVALID_CALL;
    }
    let sends = match parse_sends(&g, sends) {
        Ok(s) => s,
        Err(e) => return e,
    };
    drop(g);
    let effects = match parse_effects(effects) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let v = alloc_voice(this, SUBMIX_VT.ptr());
    let state = new_voice_state(
        Role::Submix { stage },
        channels,
        rate,
        flags,
        sends,
        effects,
    );
    lock(&e.sh.graph).voices.insert(voice_id(v), state);
    *out = v;
    S_OK
}

unsafe extern "win64" fn xa_create_master(
    this: *mut Com<Engine>,
    out: *mut *mut Voice,
    channels: u32,
    rate: u32,
    flags: u32,
    _device: usize,
    effects: *const u8,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let channels = if channels == 0 { 2 } else { channels };
    let rate = if rate == 0 { audio::RATE } else { rate };
    if channels > 64 || !(1000..=200_000).contains(&rate) {
        return E_INVALIDARG;
    }
    let e = Com::get(this);
    if lock(&e.sh.graph).master.is_some() {
        return XAUDIO2_E_INVALID_CALL;
    }
    let effects = match parse_effects(effects) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let v = alloc_voice(this, MASTER_VT.ptr());
    let state = new_voice_state(Role::Master, channels, rate, flags, Vec::new(), effects);
    let mut g = lock(&e.sh.graph);
    g.voices.insert(voice_id(v), state);
    g.master = Some(voice_id(v));
    g.started = true;
    *out = v;
    S_OK
}

unsafe extern "win64" fn xa_start(this: *mut Com<Engine>) -> i32 {
    lock(&Com::get(this).sh.graph).started = true;
    S_OK
}
unsafe extern "win64" fn xa_stop(this: *mut Com<Engine>) {
    lock(&Com::get(this).sh.graph).started = false;
}
unsafe extern "win64" fn xa_commit(_this: *mut Com<Engine>, _set: u32) -> i32 {
    S_OK
}
unsafe extern "win64" fn xa_perf(this: *mut Com<Engine>, p: *mut u8) {
    if p.is_null() {
        return;
    }
    std::ptr::write_bytes(p, 0, 64);
    let g = lock(&Com::get(this).sh.graph);
    let total = g
        .voices
        .values()
        .filter(|v| matches!(v.role, Role::Source(_)))
        .count() as u32;
    let active = g
        .voices
        .values()
        .filter(|v| matches!(&v.role, Role::Source(s) if s.playing))
        .count() as u32;
    let submix = g
        .voices
        .values()
        .filter(|v| matches!(v.role, Role::Submix { .. }))
        .count() as u32;
    *(p.add(28) as *mut u32) = PERIOD as u32; // CurrentLatencyInSamples
    *(p.add(36) as *mut u32) = active;
    *(p.add(40) as *mut u32) = total;
    *(p.add(44) as *mut u32) = submix;
    *(p.add(48) as *mut u32) = active;
}
unsafe extern "win64" fn xa_debug(_this: *mut Com<Engine>, _cfg: *const u8, _r: *mut c_void) {}

// 2.7 uniquement.
unsafe extern "win64" fn xa27_device_count(_this: *mut Com<Engine>, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 1;
    S_OK
}
unsafe extern "win64" fn xa27_device_details(
    _this: *mut Com<Engine>,
    index: u32,
    p: *mut u8,
) -> i32 {
    if p.is_null() || index != 0 {
        return E_INVALIDARG;
    }
    std::ptr::write_bytes(p, 0, 1068);
    let write = |off: usize, s: &str| {
        for (i, c) in s.encode_utf16().take(255).enumerate() {
            *(p.add(off + i * 2) as *mut u16) = c;
        }
    };
    write(0, "{0.0.0.00000000}.{574e4c58-4155-4449-4f00-000000000001}");
    write(512, "Haut-parleurs (Winlux)");
    *(p.add(1024) as *mut u32) = 0xf; // GlobalDefaultDevice
    let f = Format::mix(audio::RATE).to_bytes();
    std::ptr::copy_nonoverlapping(f.as_ptr(), p.add(1028), f.len());
    S_OK
}
unsafe extern "win64" fn xa27_initialize(_this: *mut Com<Engine>, _flags: u32, _proc: u32) -> i32 {
    S_OK
}

static XA_VT: VTable<13> = VTable([
    xa_qi as *const (),
    xa_add_ref as *const (),
    xa_release as *const (),
    xa_register as *const (),
    xa_unregister as *const (),
    xa_create_source as *const (),
    xa_create_submix as *const (),
    xa_create_master as *const (),
    xa_start as *const (),
    xa_stop as *const (),
    xa_commit as *const (),
    xa_perf as *const (),
    xa_debug as *const (),
]);
static XA27_VT: VTable<16> = VTable([
    xa_qi as *const (),
    xa_add_ref as *const (),
    xa_release as *const (),
    xa27_device_count as *const (),
    xa27_device_details as *const (),
    xa27_initialize as *const (),
    xa_register as *const (),
    xa_unregister as *const (),
    xa_create_source as *const (),
    xa_create_submix as *const (),
    xa_create_master as *const (),
    xa_start as *const (),
    xa_stop as *const (),
    xa_commit as *const (),
    xa_perf as *const (),
    xa_debug as *const (),
]);

// ---------------------------------------------------------------------------
// IXAudio2Voice (commun) et voix sources / mastering
// ---------------------------------------------------------------------------

unsafe fn with_voice<R>(v: *mut Voice, f: impl FnOnce(&mut Graph, usize) -> R) -> R {
    let sh = shared(v);
    let mut g = lock(&sh.graph);
    f(&mut g, voice_id(v))
}

unsafe extern "win64" fn v_details(v: *mut Voice, p: *mut u32) {
    if p.is_null() {
        return;
    }
    let version = (*v).version;
    with_voice(v, |g, id| {
        if let Some(s) = g.voices.get(&id) {
            if version == 7 {
                *p = s.flags;
                *p.add(1) = s.channels;
                *p.add(2) = s.rate;
            } else {
                *p = s.flags;
                *p.add(1) = 0;
                *p.add(2) = s.channels;
                *p.add(3) = s.rate;
            }
        }
    })
}
unsafe extern "win64" fn v_set_outputs(v: *mut Voice, list: *const u8) -> i32 {
    with_voice(v, |g, id| {
        let sends = match parse_sends(g, list) {
            Ok(s) => s,
            Err(e) => return e,
        };
        if sends.iter().any(|s| s.dest == id)
            || matches!(g.voices.get(&id).map(|x| &x.role), Some(Role::Master))
        {
            return XAUDIO2_E_INVALID_CALL;
        }
        match g.voices.get_mut(&id) {
            Some(s) => {
                s.sends = sends;
                S_OK
            }
            None => XAUDIO2_E_INVALID_CALL,
        }
    })
}
unsafe extern "win64" fn v_set_effect_chain(v: *mut Voice, chain: *const u8) -> i32 {
    let fx = match parse_effects(chain) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let old = with_voice(v, |g, id| {
        g.voices
            .get_mut(&id)
            .map(|s| std::mem::replace(&mut s.effects, fx))
    });
    // Libère les anciens effets hors verrou (Release peut rappeler le programme).
    drop(old);
    S_OK
}
unsafe fn set_effect_enabled(v: *mut Voice, index: u32, on: bool) -> i32 {
    with_voice(v, |g, id| {
        match g
            .voices
            .get_mut(&id)
            .and_then(|s| s.effects.get_mut(index as usize))
        {
            Some(e) => {
                e.enabled = on;
                S_OK
            }
            None => E_INVALIDARG,
        }
    })
}
unsafe extern "win64" fn v_enable_effect(v: *mut Voice, index: u32, _set: u32) -> i32 {
    set_effect_enabled(v, index, true)
}
unsafe extern "win64" fn v_disable_effect(v: *mut Voice, index: u32, _set: u32) -> i32 {
    set_effect_enabled(v, index, false)
}
unsafe extern "win64" fn v_effect_state(v: *mut Voice, index: u32, out: *mut i32) {
    if out.is_null() {
        return;
    }
    with_voice(v, |g, id| {
        if let Some(e) = g
            .voices
            .get(&id)
            .and_then(|s| s.effects.get(index as usize))
        {
            *out = i32::from(e.enabled);
        }
    })
}
unsafe extern "win64" fn v_set_effect_params(
    v: *mut Voice,
    index: u32,
    p: *const u8,
    size: u32,
    _set: u32,
) -> i32 {
    if p.is_null() {
        return E_INVALIDARG;
    }
    with_voice(v, |g, id| {
        let Some(e) = g
            .voices
            .get_mut(&id)
            .and_then(|s| s.effects.get_mut(index as usize))
        else {
            return E_INVALIDARG;
        };
        if let FxState::Reverb(r) = &mut e.state {
            // XAUDIO2FX_REVERB_PARAMETERS : 52 octets (2.7/2.8) ou 60 (2.9).
            let decay_at = match size {
                52 => 40,
                60 => 44,
                _ => return E_INVALIDARG,
            };
            let wet = *(p as *const f32);
            let decay = *(p.add(decay_at) as *const f32);
            r.set(wet, decay);
        }
        S_OK
    })
}
unsafe extern "win64" fn v_get_effect_params(
    v: *mut Voice,
    index: u32,
    p: *mut u8,
    size: u32,
) -> i32 {
    if p.is_null() {
        return E_INVALIDARG;
    }
    with_voice(v, |g, id| {
        let Some(e) = g
            .voices
            .get(&id)
            .and_then(|s| s.effects.get(index as usize))
        else {
            return E_INVALIDARG;
        };
        match &e.state {
            // XAUDIO2FX_VOLUMEMETER_LEVELS { float *pPeak; float *pRMS; UINT32 ChannelCount; }
            FxState::Meter { peak, rms } if size >= 20 => {
                let pk = *(p as *const *mut f32);
                let rm = *(p.add(8) as *const *mut f32);
                let n = *(p.add(16) as *const u32) as usize;
                for c in 0..n {
                    let (a, b) = if c < 2 { (peak[c], rms[c]) } else { (0.0, 0.0) };
                    if !pk.is_null() {
                        *pk.add(c) = a;
                    }
                    if !rm.is_null() {
                        *rm.add(c) = b;
                    }
                }
                S_OK
            }
            FxState::Reverb(r) if size >= 52 => {
                std::ptr::write_bytes(p, 0, size as usize);
                *(p as *mut f32) = r.wet_dry;
                let decay_at = if size >= 60 { 44 } else { 40 };
                *(p.add(decay_at) as *mut f32) = r.decay;
                S_OK
            }
            _ => E_INVALIDARG,
        }
    })
}

unsafe fn read_filter(p: *const u8) -> Option<(dsp::FilterKind, f32, f32)> {
    if p.is_null() {
        return None;
    }
    let kind = dsp::FilterKind::from_raw(*(p as *const u32))?;
    let f = *(p.add(4) as *const f32);
    let q = *(p.add(8) as *const f32);
    if !(0.0..=1.0).contains(&f) || !(q > 0.0 && q <= 1.5) {
        return None;
    }
    Some((kind, f, q))
}
unsafe fn write_filter(p: *mut u8, f: &dsp::Filter) {
    if p.is_null() {
        return;
    }
    *(p as *mut u32) = f.kind.raw();
    *(p.add(4) as *mut f32) = f.frequency;
    *(p.add(8) as *mut f32) = f.one_over_q;
}
unsafe extern "win64" fn v_set_filter(v: *mut Voice, p: *const u8, _set: u32) -> i32 {
    let Some((k, f, q)) = read_filter(p) else {
        return E_INVALIDARG;
    };
    with_voice(v, |g, id| match g.voices.get_mut(&id) {
        Some(s) if s.flags & XAUDIO2_VOICE_USEFILTER != 0 => {
            s.filter.set(k, f, q);
            S_OK
        }
        _ => XAUDIO2_E_INVALID_CALL,
    })
}
unsafe extern "win64" fn v_get_filter(v: *mut Voice, p: *mut u8) {
    with_voice(v, |g, id| {
        if let Some(s) = g.voices.get(&id) {
            write_filter(p, &s.filter);
        }
    })
}
unsafe extern "win64" fn v_set_output_filter(
    v: *mut Voice,
    dest: *mut Voice,
    p: *const u8,
    _set: u32,
) -> i32 {
    let Some((k, f, q)) = read_filter(p) else {
        return E_INVALIDARG;
    };
    with_voice(v, |g, id| {
        let Some(s) = g.voices.get_mut(&id) else {
            return XAUDIO2_E_INVALID_CALL;
        };
        let send = if dest.is_null() && s.sends.len() == 1 {
            s.sends.first_mut()
        } else {
            s.sends.iter_mut().find(|x| x.dest == dest as usize)
        };
        match send.and_then(|x| x.filter.as_mut()) {
            Some(filter) => {
                filter.set(k, f, q);
                S_OK
            }
            None => XAUDIO2_E_INVALID_CALL,
        }
    })
}
unsafe extern "win64" fn v_get_output_filter(v: *mut Voice, dest: *mut Voice, p: *mut u8) {
    with_voice(v, |g, id| {
        if let Some(f) = g
            .voices
            .get(&id)
            .and_then(|s| {
                s.sends
                    .iter()
                    .find(|x| x.dest == dest as usize || dest.is_null())
            })
            .and_then(|x| x.filter.as_ref())
        {
            write_filter(p, f);
        }
    })
}
unsafe extern "win64" fn v_set_volume(v: *mut Voice, vol: f32, _set: u32) -> i32 {
    if !vol.is_finite() || vol.abs() > 16_777_216.0 {
        return E_INVALIDARG;
    }
    with_voice(v, |g, id| {
        if let Some(s) = g.voices.get_mut(&id) {
            s.volume = vol;
        }
        S_OK
    })
}
unsafe extern "win64" fn v_get_volume(v: *mut Voice, out: *mut f32) {
    if out.is_null() {
        return;
    }
    with_voice(v, |g, id| {
        if let Some(s) = g.voices.get(&id) {
            *out = s.volume;
        }
    })
}
unsafe extern "win64" fn v_set_channel_volumes(
    v: *mut Voice,
    n: u32,
    p: *const f32,
    _set: u32,
) -> i32 {
    if p.is_null() {
        return E_INVALIDARG;
    }
    with_voice(v, |g, id| match g.voices.get_mut(&id) {
        Some(s) if s.channels == n => {
            s.channel_volumes = std::slice::from_raw_parts(p, n as usize).to_vec();
            S_OK
        }
        _ => E_INVALIDARG,
    })
}
unsafe extern "win64" fn v_get_channel_volumes(v: *mut Voice, n: u32, p: *mut f32) {
    if p.is_null() {
        return;
    }
    with_voice(v, |g, id| {
        if let Some(s) = g.voices.get(&id) {
            for i in 0..(n as usize).min(s.channel_volumes.len()) {
                *p.add(i) = s.channel_volumes[i];
            }
        }
    })
}
unsafe extern "win64" fn v_set_output_matrix(
    v: *mut Voice,
    dest: *mut Voice,
    src_ch: u32,
    dst_ch: u32,
    m: *const f32,
    _set: u32,
) -> i32 {
    if m.is_null() || src_ch == 0 || dst_ch == 0 {
        return E_INVALIDARG;
    }
    let data = std::slice::from_raw_parts(m, (src_ch * dst_ch) as usize).to_vec();
    with_voice(v, |g, id| {
        let Some(s) = g.voices.get_mut(&id) else {
            return XAUDIO2_E_INVALID_CALL;
        };
        if s.channels != src_ch {
            return E_INVALIDARG;
        }
        let send = if dest.is_null() {
            if s.sends.len() != 1 {
                return E_INVALIDARG;
            }
            s.sends.first_mut()
        } else {
            s.sends.iter_mut().find(|x| x.dest == dest as usize)
        };
        match send {
            Some(x) => {
                x.matrix = Some((src_ch, dst_ch, data));
                S_OK
            }
            None => E_INVALIDARG,
        }
    })
}
unsafe extern "win64" fn v_get_output_matrix(
    v: *mut Voice,
    dest: *mut Voice,
    src_ch: u32,
    dst_ch: u32,
    m: *mut f32,
) {
    if m.is_null() {
        return;
    }
    with_voice(v, |g, id| {
        let Some(s) = g.voices.get(&id) else {
            return;
        };
        let send = s
            .sends
            .iter()
            .find(|x| x.dest == dest as usize || dest.is_null());
        for d in 0..dst_ch {
            for c in 0..src_ch {
                let val = match send.and_then(|x| x.matrix.as_ref()) {
                    Some((sc, dc, mm)) if *sc == src_ch && *dc == dst_ch => {
                        mm[(d * src_ch + c) as usize]
                    }
                    _ => {
                        if src_ch == 1 || c == d {
                            1.0
                        } else {
                            0.0
                        }
                    }
                };
                *m.add((d * src_ch + c) as usize) = val;
            }
        }
    })
}
unsafe extern "win64" fn v_destroy(v: *mut Voice) {
    if v.is_null() {
        return;
    }
    let removed = with_voice(v, |g, id| {
        if g.master == Some(id) {
            g.master = None;
        }
        g.voices.remove(&id)
    });
    drop(removed);
    drop(Arc::from_raw((*v).sh));
    drop(Box::from_raw(v));
}

unsafe fn with_source<R>(v: *mut Voice, f: impl FnOnce(&mut SourceState) -> R) -> Option<R> {
    with_voice(v, |g, id| {
        match g.voices.get_mut(&id).map(|s| &mut s.role) {
            Some(Role::Source(src)) => Some(f(src)),
            _ => None,
        }
    })
}
unsafe extern "win64" fn s_start(v: *mut Voice, _flags: u32, _set: u32) -> i32 {
    with_source(v, |s| s.playing = true);
    S_OK
}
unsafe extern "win64" fn s_stop(v: *mut Voice, _flags: u32, _set: u32) -> i32 {
    with_source(v, |s| s.playing = false);
    S_OK
}
// XAUDIO2_BUFFER : Flags @0, AudioBytes @4, pAudioData @8, PlayBegin @16,
// PlayLength @20, LoopBegin @24, LoopLength @28, LoopCount @32, pContext @40.
unsafe extern "win64" fn s_submit(v: *mut Voice, b: *const u8, _wma: *const u8) -> i32 {
    if b.is_null() {
        return E_INVALIDARG;
    }
    let flags = *(b as *const u32);
    let bytes = *(b.add(4) as *const u32) as usize;
    let data = *(b.add(8) as *const *const u8);
    let play_begin = *(b.add(16) as *const u32) as usize;
    let play_len = *(b.add(20) as *const u32) as usize;
    let loop_begin = *(b.add(24) as *const u32) as usize;
    let loop_len = *(b.add(28) as *const u32) as usize;
    let loop_count = *(b.add(32) as *const u32);
    let ctx = *(b.add(40) as *const usize);
    with_source(v, |s| {
        let block = s.format.block as usize;
        if data.is_null()
            || bytes == 0
            || !bytes.is_multiple_of(block)
            || loop_count > XAUDIO2_LOOP_INFINITE
        {
            return E_INVALIDARG;
        }
        if s.queue.len() >= XAUDIO2_MAX_QUEUED_BUFFERS {
            return XAUDIO2_E_INVALID_CALL;
        }
        let total = bytes / block;
        let end = if play_len == 0 {
            total
        } else {
            play_begin + play_len
        };
        if play_begin >= total || end > total {
            return E_INVALIDARG;
        }
        let (lb, le) = if loop_count > 0 {
            let le = if loop_len == 0 {
                end
            } else {
                loop_begin + loop_len
            };
            if loop_begin >= le || le > end || loop_begin < play_begin {
                return E_INVALIDARG;
            }
            (loop_begin, le)
        } else {
            (0, 0)
        };
        s.queue.push_back(XBuf {
            flags,
            data,
            bytes,
            end,
            loop_begin: lb,
            loop_end: le,
            loops_left: loop_count,
            ctx,
            pos: play_begin,
            started: false,
        });
        S_OK
    })
    .unwrap_or(XAUDIO2_E_INVALID_CALL)
}
unsafe extern "win64" fn s_flush(v: *mut Voice) -> i32 {
    with_source(v, |s| {
        // Le tampon en cours continue s'il est commencé et que la voix joue.
        let keep = usize::from(s.playing && s.queue.front().is_some_and(|b| b.started));
        let removed: Vec<usize> = s.queue.drain(keep..).map(|b| b.ctx).collect();
        if let Some(b) = s.queue.front_mut() {
            b.loops_left = 0;
        }
        s.flushed.extend(removed);
    });
    S_OK
}
unsafe extern "win64" fn s_discontinuity(v: *mut Voice) -> i32 {
    with_source(v, |s| {
        if let Some(b) = s.queue.back_mut() {
            b.flags |= XAUDIO2_END_OF_STREAM;
        }
    });
    S_OK
}
unsafe extern "win64" fn s_exit_loop(v: *mut Voice, _set: u32) -> i32 {
    with_source(v, |s| {
        if let Some(b) = s.queue.front_mut() {
            b.loops_left = 0;
        }
    });
    S_OK
}
unsafe fn write_state(v: *mut Voice, p: *mut u8, flags: u32) {
    if p.is_null() {
        return;
    }
    with_source(v, |s| {
        let ctx = s.queue.front().map(|b| b.ctx).unwrap_or(0);
        *(p as *mut usize) = ctx;
        *(p.add(8) as *mut u32) = s.queue.len() as u32;
        if flags & XAUDIO2_VOICE_NOSAMPLESPLAYED == 0 {
            *(p.add(16) as *mut u64) = s.samples_played;
        }
    });
}
unsafe extern "win64" fn s_get_state(v: *mut Voice, p: *mut u8, flags: u32) {
    write_state(v, p, flags)
}
unsafe extern "win64" fn s27_get_state(v: *mut Voice, p: *mut u8) {
    write_state(v, p, 0)
}
unsafe extern "win64" fn s_set_ratio(v: *mut Voice, r: f32, _set: u32) -> i32 {
    if !r.is_finite() || r <= 0.0 {
        return E_INVALIDARG;
    }
    with_source(v, |s| s.ratio = r.clamp(1.0 / 1024.0, s.max_ratio));
    S_OK
}
unsafe extern "win64" fn s_get_ratio(v: *mut Voice, out: *mut f32) {
    if out.is_null() {
        return;
    }
    if let Some(r) = with_source(v, |s| s.ratio) {
        *out = r;
    }
}
unsafe extern "win64" fn s_set_rate(v: *mut Voice, rate: u32) -> i32 {
    if !(1000..=200_000).contains(&rate) {
        return E_INVALIDARG;
    }
    let r = with_voice(v, |g, id| {
        let Some(vs) = g.voices.get_mut(&id) else {
            return XAUDIO2_E_INVALID_CALL;
        };
        let Role::Source(s) = &mut vs.role else {
            return XAUDIO2_E_INVALID_CALL;
        };
        if !s.queue.is_empty() {
            return XAUDIO2_E_INVALID_CALL;
        }
        let Some(f) = Format::new(s.format.sample, s.format.channels, rate) else {
            return E_INVALIDARG;
        };
        s.format = f;
        vs.rate = rate;
        S_OK
    });
    r
}
unsafe extern "win64" fn m_channel_mask(v: *mut Voice, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let ch = with_voice(v, |g, id| {
        g.voices.get(&id).map(|s| s.channels).unwrap_or(2)
    });
    *out = audio::format::default_mask(ch.min(8) as u16);
    S_OK
}

macro_rules! voice_slots {
    () => {
        [
            v_details as *const (),
            v_set_outputs as *const (),
            v_set_effect_chain as *const (),
            v_enable_effect as *const (),
            v_disable_effect as *const (),
            v_effect_state as *const (),
            v_set_effect_params as *const (),
            v_get_effect_params as *const (),
            v_set_filter as *const (),
            v_get_filter as *const (),
            v_set_output_filter as *const (),
            v_get_output_filter as *const (),
            v_set_volume as *const (),
            v_get_volume as *const (),
            v_set_channel_volumes as *const (),
            v_get_channel_volumes as *const (),
            v_set_output_matrix as *const (),
            v_get_output_matrix as *const (),
            v_destroy as *const (),
        ]
    };
}

const fn concat<const A: usize, const B: usize, const C: usize>(
    a: [*const (); A],
    b: [*const (); B],
) -> [*const (); C] {
    let mut out = [std::ptr::null::<()>(); C];
    let mut i = 0;
    while i < A {
        out[i] = a[i];
        i += 1;
    }
    let mut j = 0;
    while j < B {
        out[A + j] = b[j];
        j += 1;
    }
    out
}

static SUBMIX_VT: VTable<19> = VTable(voice_slots!());
static MASTER_VT: VTable<20> = VTable(concat::<19, 1, 20>(
    voice_slots!(),
    [m_channel_mask as *const ()],
));
static SOURCE_VT: VTable<29> = VTable(concat::<19, 10, 29>(
    voice_slots!(),
    [
        s_start as *const (),
        s_stop as *const (),
        s_submit as *const (),
        s_flush as *const (),
        s_discontinuity as *const (),
        s_exit_loop as *const (),
        s_get_state as *const (),
        s_set_ratio as *const (),
        s_get_ratio as *const (),
        s_set_rate as *const (),
    ],
));
static SOURCE27_VT: VTable<29> = VTable(concat::<19, 10, 29>(
    voice_slots!(),
    [
        s_start as *const (),
        s_stop as *const (),
        s_submit as *const (),
        s_flush as *const (),
        s_discontinuity as *const (),
        s_exit_loop as *const (),
        s27_get_state as *const (),
        s_set_ratio as *const (),
        s_get_ratio as *const (),
        s_set_rate as *const (),
    ],
));

// ---------------------------------------------------------------------------
// Exports et création COM
// ---------------------------------------------------------------------------

pub unsafe extern "win64" fn xaudio2_create(
    out: *mut *mut c_void,
    _flags: u32,
    _processor: u32,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_engine(9) as *mut c_void;
    S_OK
}
pub unsafe extern "win64" fn xaudio2_create_with_version(
    out: *mut *mut c_void,
    flags: u32,
    processor: u32,
    _ntddi: u32,
) -> i32 {
    xaudio2_create(out, flags, processor)
}
pub unsafe extern "win64" fn create_audio_reverb(out: *mut *mut c_void) -> i32 {
    put(out, new_fx(FxKind::Reverb))
}
pub unsafe extern "win64" fn create_audio_volume_meter(out: *mut *mut c_void) -> i32 {
    put(out, new_fx(FxKind::Meter))
}
pub unsafe extern "win64" fn create_fx(
    _clsid: *const Guid,
    out: *mut *mut c_void,
    _init: *const c_void,
    _size: u32,
) -> i32 {
    // xapofx : effets avancés (égaliseur, écho…) non fournis.
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    0x8004_0111u32 as i32
}

unsafe fn create_engine27(riid: *const Guid, out: *mut *mut c_void) -> i32 {
    let e = new_engine(7);
    let hr = xa_qi(e, riid, out);
    Com::release(e);
    hr
}
unsafe fn create_reverb27(riid: *const Guid, out: *mut *mut c_void) -> i32 {
    let f = new_fx(FxKind::Reverb);
    let hr = fx_qi(f, riid, out);
    Com::release(f);
    hr
}
unsafe fn create_meter27(riid: *const Guid, out: *mut *mut c_void) -> i32 {
    let f = new_fx(FxKind::Meter);
    let hr = fx_qi(f, riid, out);
    Com::release(f);
    hr
}

/// Classes XAudio2 2.7 (créées par CoCreateInstance).
pub fn creator(clsid: &Guid) -> Option<Creator> {
    if *clsid == CLSID_XAUDIO2_27 || *clsid == CLSID_XAUDIO2_27_DEBUG {
        Some(create_engine27)
    } else if *clsid == CLSID_AUDIOREVERB_27 {
        Some(create_reverb27)
    } else if *clsid == CLSID_AUDIOVOLUMEMETER_27 {
        Some(create_meter27)
    } else {
        None
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "XAudio2Create"=>fn xaudio2_create,
        "XAudio2CreateWithVersionInfo"=>fn xaudio2_create_with_version,
        "CreateAudioReverb"=>fn create_audio_reverb,
        "CreateAudioVolumeMeter"=>fn create_audio_volume_meter,
        "CreateFX"=>fn create_fx,
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
    use std::sync::atomic::AtomicU32;

    #[test]
    fn matrice_de_sortie_ramenee_en_stereo() {
        let m = collapse(1, 2, &[0.25, 0.75]);
        assert_eq!((m.ll, m.rr, m.rl, m.lr), (0.25, 0.75, 0.0, 0.0));
        let s = collapse(2, 2, &[1.0, 0.0, 0.0, 0.5]);
        assert_eq!((s.ll, s.rl, s.lr, s.rr), (1.0, 0.0, 0.0, 0.5));
        let d = default_matrix(1);
        assert_eq!(d.ll, 0.5);
    }

    // Rappel de voix minimal : compte OnBufferEnd et OnStreamEnd.
    #[repr(C)]
    struct Cb {
        vt: *const usize,
        ends: AtomicU32,
        stream_end: AtomicU32,
        starts: AtomicU32,
    }
    unsafe extern "win64" fn nop1(_: *mut Cb, _: usize) {}
    unsafe extern "win64" fn nop0(_: *mut Cb) {}
    unsafe extern "win64" fn pass_start(c: *mut Cb, _bytes: u32) {
        (*c).starts.fetch_add(1, Ordering::SeqCst);
    }
    unsafe extern "win64" fn stream_end(c: *mut Cb) {
        (*c).stream_end.fetch_add(1, Ordering::SeqCst);
    }
    unsafe extern "win64" fn buffer_end(c: *mut Cb, _ctx: usize) {
        (*c).ends.fetch_add(1, Ordering::SeqCst);
    }
    unsafe extern "win64" fn err(_: *mut Cb, _: usize, _: i32) {}

    #[test]
    fn voix_source_vers_submix_vers_mastering_avec_rappels() {
        std::env::set_var("WINLUX_AUDIO", "null");
        unsafe {
            let vt = [
                pass_start as *const () as usize,
                nop0 as *const () as usize,
                stream_end as *const () as usize,
                nop1 as *const () as usize,
                buffer_end as *const () as usize,
                nop1 as *const () as usize,
                err as *const () as usize,
            ];
            let cb = Cb {
                vt: vt.as_ptr(),
                ends: AtomicU32::new(0),
                stream_end: AtomicU32::new(0),
                starts: AtomicU32::new(0),
            };
            let mut xa: *mut c_void = std::ptr::null_mut();
            assert_eq!(xaudio2_create(&mut xa, 0, 0), S_OK);
            let e = xa as *mut Com<Engine>;
            let fmt = Format::new(audio::format::Sample::F32, 1, 24000)
                .unwrap()
                .to_bytes();
            let mut src: *mut Voice = std::ptr::null_mut();
            assert_eq!(
                xa_create_source(
                    e,
                    &mut src,
                    fmt.as_ptr(),
                    0,
                    2.0,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null()
                ),
                XAUDIO2_E_INVALID_CALL,
                "il faut d'abord une voix de mastering"
            );
            let mut master: *mut Voice = std::ptr::null_mut();
            assert_eq!(
                xa_create_master(e, &mut master, 0, 0, 0, 0, std::ptr::null()),
                S_OK
            );
            let mut meter: *mut c_void = std::ptr::null_mut();
            assert_eq!(create_audio_volume_meter(&mut meter), S_OK);
            let mut desc = Al([0u8; 16]);
            desc[0..8].copy_from_slice(&(meter as usize).to_le_bytes());
            desc[8..12].copy_from_slice(&1i32.to_le_bytes());
            desc[12..16].copy_from_slice(&2u32.to_le_bytes());
            let mut chain = Al([0u8; 16]);
            chain[0..4].copy_from_slice(&1u32.to_le_bytes());
            chain[8..16].copy_from_slice(&(desc.as_ptr() as usize).to_le_bytes());
            let mut sub: *mut Voice = std::ptr::null_mut();
            assert_eq!(
                xa_create_submix(
                    e,
                    &mut sub,
                    2,
                    48000,
                    XAUDIO2_VOICE_USEFILTER,
                    0,
                    std::ptr::null(),
                    chain.as_ptr()
                ),
                S_OK
            );
            unknown_release(meter);
            let mut send = Al([0u8; 16]);
            send[8..16].copy_from_slice(&(sub as usize).to_le_bytes());
            let mut sends = Al([0u8; 16]);
            sends[0..4].copy_from_slice(&1u32.to_le_bytes());
            sends[8..16].copy_from_slice(&(send.as_ptr() as usize).to_le_bytes());
            assert_eq!(
                xa_create_source(
                    e,
                    &mut src,
                    fmt.as_ptr(),
                    0,
                    2.0,
                    &cb as *const Cb as *mut c_void,
                    sends.as_ptr(),
                    std::ptr::null()
                ),
                S_OK
            );
            // 0,1 s de signal constant à 0,5, joué deux fois (LoopCount = 1).
            let pcm: Vec<u8> = (0..2400).flat_map(|_| 0.5f32.to_le_bytes()).collect();
            let mut b = Al([0u8; 48]);
            b[0..4].copy_from_slice(&XAUDIO2_END_OF_STREAM.to_le_bytes());
            b[4..8].copy_from_slice(&(pcm.len() as u32).to_le_bytes());
            b[8..16].copy_from_slice(&(pcm.as_ptr() as usize).to_le_bytes());
            b[32..36].copy_from_slice(&1u32.to_le_bytes());
            b[40..48].copy_from_slice(&7usize.to_le_bytes());
            assert_eq!(s_submit(src, b.as_ptr(), std::ptr::null()), S_OK);
            let mut details = [0u32; 4];
            v_details(src, details.as_mut_ptr());
            assert_eq!((details[2], details[3]), (1, 24000));
            assert_eq!(s_start(src, 0, 0), S_OK);
            let t0 = std::time::Instant::now();
            while cb.stream_end.load(Ordering::SeqCst) == 0 {
                assert!(t0.elapsed().as_secs() < 5, "OnStreamEnd jamais reçu");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert_eq!(cb.ends.load(Ordering::SeqCst), 1);
            assert!(cb.starts.load(Ordering::SeqCst) >= 10);
            let mut state = Al([0u8; 24]);
            s_get_state(src, state.as_mut_ptr(), 0);
            assert_eq!(u64::from_le_bytes(state[16..24].try_into().unwrap()), 4800);
            // Le vumètre du sous-mixage a vu passer le signal.
            let (mut peak, mut rms) = ([0f32; 2], [0f32; 2]);
            let mut levels = Al([0u8; 24]);
            levels[0..8].copy_from_slice(&(peak.as_mut_ptr() as usize).to_le_bytes());
            levels[8..16].copy_from_slice(&(rms.as_mut_ptr() as usize).to_le_bytes());
            levels[16..20].copy_from_slice(&2u32.to_le_bytes());
            assert_eq!(v_get_effect_params(sub, 0, levels.as_mut_ptr(), 24), S_OK);
            let _ = rms;
            let mut f = Al([0u8; 12]);
            f[0..4].copy_from_slice(&2u32.to_le_bytes());
            f[4..8].copy_from_slice(&0.5f32.to_le_bytes());
            f[8..12].copy_from_slice(&1.0f32.to_le_bytes());
            assert_eq!(v_set_filter(sub, f.as_ptr(), 0), S_OK);
            assert_eq!(v_set_filter(src, f.as_ptr(), 0), XAUDIO2_E_INVALID_CALL);
            v_destroy(src);
            v_destroy(sub);
            v_destroy(master);
            xa_release(e);
        }
    }
}
