//! Moteur audio commun aux API Windows (waveOut, DirectSound, WASAPI,
//! XAudio2).
//!
//! Chaque API enregistre des « sources » ; un thread unique les mixe en
//! stéréo flottant 48 kHz par périodes de 10 ms puis écrit le résultat sur
//! le pilote Linux choisi à l'exécution (voir [`backend`]). Le processus
//! n'ouvre donc qu'un seul flux, nommé d'après le .exe : il apparaît comme
//! une application distincte dans le mixeur de volume du système.

pub mod backend;
pub mod dsp;
pub mod format;
mod pipewire;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};

use crate::dlls::lock;

/// Fréquence de mixage interne.
pub const RATE: u32 = 48_000;
/// Taille d'une période de mixage (10 ms).
pub const PERIOD: usize = 480;

/// Une source de son mixée par le moteur.
pub trait Source: Send + Sync {
    /// Ajoute `out.len() / 2` trames stéréo à `out`. Renvoie `false` quand
    /// la source est terminée et peut être retirée.
    ///
    /// La méthode est appelée depuis le thread de mixage sans aucun verrou du
    /// moteur : elle peut appeler du code du programme (rappels).
    fn mix(&self, out: &mut [f32]) -> bool;
}

struct Engine {
    sources: Mutex<Vec<(u64, Arc<dyn Source>)>>,
    wake: Condvar,
    next_id: AtomicU64,
    /// Trames écrites vers le périphérique depuis le démarrage.
    frames: AtomicU64,
    /// Volume maître du processus (f32 en bits) et sourdine.
    master: AtomicU32,
    muted: AtomicBool,
    backend: AtomicUsize,
    loopback: Mutex<Vec<Weak<CaptureQueue>>>,
    started: AtomicBool,
}

fn engine() -> &'static Engine {
    static E: OnceLock<Engine> = OnceLock::new();
    E.get_or_init(|| Engine {
        sources: Mutex::new(Vec::new()),
        wake: Condvar::new(),
        next_id: AtomicU64::new(1),
        frames: AtomicU64::new(0),
        master: AtomicU32::new(1.0f32.to_bits()),
        muted: AtomicBool::new(false),
        backend: AtomicUsize::new(usize::MAX),
        loopback: Mutex::new(Vec::new()),
        started: AtomicBool::new(false),
    })
}

/// Enregistre une source ; le thread de mixage démarre au premier appel.
pub fn add_source(src: Arc<dyn Source>) -> u64 {
    let e = engine();
    let id = e.next_id.fetch_add(1, Ordering::Relaxed);
    lock(&e.sources).push((id, src));
    if !e.started.swap(true, Ordering::AcqRel) {
        let spawned = std::thread::Builder::new()
            .name("winlux-audio-mix".into())
            .spawn(mix_thread);
        if spawned.is_err() {
            e.started.store(false, Ordering::Release);
        }
    }
    e.wake.notify_all();
    id
}

pub fn remove_source(id: u64) {
    lock(&engine().sources).retain(|(i, _)| *i != id);
}

/// Position du périphérique, en trames à [`RATE`] Hz.
pub fn frames_played() -> u64 {
    engine().frames.load(Ordering::Acquire)
}

/// Nom du pilote réellement ouvert (« pipewire », « pulseaudio », « alsa »,
/// « null ») ou, avant la première ouverture, du premier candidat.
pub fn backend_name() -> &'static str {
    match engine().backend.load(Ordering::Relaxed) {
        0 => "pipewire",
        1 => "pulseaudio",
        2 => "alsa",
        3 => "null",
        _ => backend::candidates()
            .first()
            .map(|k| k.name())
            .unwrap_or("null"),
    }
}

pub fn master_volume() -> f32 {
    f32::from_bits(engine().master.load(Ordering::Relaxed))
}
pub fn set_master_volume(v: f32) {
    let v = if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        1.0
    };
    engine().master.store(v.to_bits(), Ordering::Relaxed);
}
pub fn muted() -> bool {
    engine().muted.load(Ordering::Relaxed)
}
pub fn set_muted(m: bool) {
    engine().muted.store(m, Ordering::Relaxed);
}

/// Prépare le thread courant à appeler du code Windows (TEB, TLS des DLL).
/// À appeler avant un rappel vers le programme depuis un thread audio.
pub fn ensure_windows_thread() {
    thread_local! {
        static READY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    READY.with(|r| {
        if !r.get() {
            r.set(true);
            crate::process::thread_attach_current();
        }
    });
}

fn kind_index(k: backend::Kind) -> usize {
    match k {
        backend::Kind::PipeWire => 0,
        backend::Kind::Pulse => 1,
        backend::Kind::Alsa => 2,
        backend::Kind::Null => 3,
    }
}

fn mix_thread() {
    let e = engine();
    let mut out: Option<Box<dyn backend::Output>> = None;
    let mut buf = vec![0.0f32; PERIOD * 2];
    let mut idle_periods = 0u32;
    loop {
        let snapshot: Vec<(u64, Arc<dyn Source>)> = {
            let mut s = lock(&e.sources);
            if s.is_empty() {
                idle_periods += 1;
                // Après 2 s sans source, libère le périphérique et s'endort.
                if idle_periods > 200 {
                    out = None;
                    while s.is_empty() {
                        s = e
                            .wake
                            .wait(s)
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                    }
                    idle_periods = 0;
                }
            } else {
                idle_periods = 0;
            }
            s.clone()
        };
        buf.fill(0.0);
        let mut finished = Vec::new();
        for (id, src) in &snapshot {
            if !src.mix(&mut buf) {
                finished.push(*id);
            }
        }
        if !finished.is_empty() {
            lock(&e.sources).retain(|(i, _)| !finished.contains(i));
        }
        let gain = if muted() { 0.0 } else { master_volume() };
        for v in buf.iter_mut() {
            *v = (*v * gain).clamp(-1.0, 1.0);
        }
        publish_loopback(&buf);
        let o = out.get_or_insert_with(|| {
            let o = backend::open_output();
            e.backend.store(kind_index(o.kind()), Ordering::Relaxed);
            o
        });
        if !o.write(&buf) {
            crate::log_warn!("audio", "pilote {} perdu, bascule", o.kind().name());
            e.backend
                .store(kind_index(backend::Kind::Null), Ordering::Relaxed);
            out = Some(Box::new(backend::Null::new()));
        }
        e.frames.fetch_add(PERIOD as u64, Ordering::AcqRel);
    }
}

// ---------------------------------------------------------------------------
// Capture : micro (pilote d'entrée) et « loopback » (ce que le processus joue).
// ---------------------------------------------------------------------------

/// File de trames stéréo f32 à [`RATE`] Hz destinée à un client de capture.
pub struct CaptureQueue {
    data: Mutex<VecDeque<f32>>,
    /// Événement Win32 à signaler à chaque période reçue (0 = aucun).
    pub event: AtomicUsize,
}

impl CaptureQueue {
    fn new() -> Arc<CaptureQueue> {
        Arc::new(CaptureQueue {
            data: Mutex::new(VecDeque::new()),
            event: AtomicUsize::new(0),
        })
    }

    fn push(&self, stereo: &[f32]) {
        {
            let mut d = lock(&self.data);
            d.extend(stereo.iter().copied());
            // Garde au plus 2 s : un client qui ne lit pas perd le plus ancien.
            let cap = RATE as usize * 4;
            if d.len() > cap {
                let excess = d.len() - cap;
                d.drain(..excess);
            }
        }
        let ev = self.event.load(Ordering::Relaxed);
        if ev != 0 {
            crate::dlls::kernel32::set_event(ev);
        }
    }

    /// Trames stéréo disponibles.
    pub fn available(&self) -> usize {
        lock(&self.data).len() / 2
    }

    /// Retire au plus `frames` trames.
    pub fn take(&self, frames: usize) -> Vec<f32> {
        let mut d = lock(&self.data);
        let n = (frames * 2).min(d.len());
        d.drain(..n).collect()
    }
}

fn publish_loopback(buf: &[f32]) {
    let mut list = lock(&engine().loopback);
    list.retain(|w| match w.upgrade() {
        Some(q) => {
            q.push(buf);
            true
        }
        None => false,
    });
}

/// S'abonne au son produit par ce processus (capture « loopback » WASAPI).
pub fn subscribe_loopback() -> Arc<CaptureQueue> {
    let q = CaptureQueue::new();
    lock(&engine().loopback).push(Arc::downgrade(&q));
    q
}

struct Mic {
    subscribers: Mutex<Vec<Weak<CaptureQueue>>>,
    running: AtomicBool,
}

fn mic() -> &'static Mic {
    static M: OnceLock<Mic> = OnceLock::new();
    M.get_or_init(|| Mic {
        subscribers: Mutex::new(Vec::new()),
        running: AtomicBool::new(false),
    })
}

/// S'abonne au micro par défaut. Le pilote d'entrée est ouvert à la première
/// demande et refermé quand plus personne n'écoute.
pub fn subscribe_microphone() -> Arc<CaptureQueue> {
    let q = CaptureQueue::new();
    let m = mic();
    lock(&m.subscribers).push(Arc::downgrade(&q));
    if !m.running.swap(true, Ordering::AcqRel) {
        let ok = std::thread::Builder::new()
            .name("winlux-audio-capture".into())
            .spawn(capture_thread)
            .is_ok();
        if !ok {
            m.running.store(false, Ordering::Release);
        }
    }
    q
}

fn capture_thread() {
    let m = mic();
    let mut input = backend::open_input();
    let mut buf = vec![0.0f32; PERIOD * 2];
    loop {
        if !input.read(&mut buf) {
            input = Box::new(backend::Null::new());
            continue;
        }
        let mut subs = lock(&m.subscribers);
        subs.retain(|w| match w.upgrade() {
            Some(q) => {
                q.push(&buf);
                true
            }
            None => false,
        });
        if subs.is_empty() {
            m.running.store(false, Ordering::Release);
            return;
        }
    }
}

/// `winlux --audio-test` : joue un « la » de 0,8 s par le moteur et indique
/// le pilote utilisé. Sert à vérifier le son d'une installation.
pub fn self_test() -> i32 {
    struct Tone {
        n: AtomicU64,
    }
    impl Source for Tone {
        fn mix(&self, out: &mut [f32]) -> bool {
            let start = self.n.fetch_add((out.len() / 2) as u64, Ordering::Relaxed);
            for (i, fr) in out.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                let t = (start + i as u64) as f32 / RATE as f32;
                let v = (t * 440.0 * std::f32::consts::TAU).sin() * 0.2;
                fr[0] += v;
                fr[1] += v;
            }
            start < RATE as u64 * 8 / 10
        }
    }
    println!(
        "pilotes candidats : {}",
        backend::candidates()
            .iter()
            .map(|k| k.name())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let start = frames_played();
    add_source(Arc::new(Tone {
        n: AtomicU64::new(0),
    }));
    let t0 = std::time::Instant::now();
    while frames_played() < start + RATE as u64 && t0.elapsed().as_secs() < 5 {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let name = backend_name();
    println!("sortie audio : {name}");
    if frames_played() < start + RATE as u64 {
        eprintln!("le mixeur n'avance pas");
        return 1;
    }
    i32::from(name == "null" && std::env::var_os("WINLUX_AUDIO").is_none())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tone {
        left: AtomicU32,
    }
    impl Source for Tone {
        fn mix(&self, out: &mut [f32]) -> bool {
            for v in out.iter_mut() {
                *v += 0.25;
            }
            self.left.fetch_sub(1, Ordering::Relaxed) > 1
        }
    }

    #[test]
    fn moteur_mixe_et_retire_les_sources_terminees() {
        std::env::set_var("WINLUX_AUDIO", "null");
        let lb = subscribe_loopback();
        let start = frames_played();
        let id = add_source(Arc::new(Tone {
            left: AtomicU32::new(3),
        }));
        let t0 = std::time::Instant::now();
        while frames_played() < start + PERIOD as u64 * 6 {
            assert!(t0.elapsed().as_secs() < 5, "le mixeur n'avance pas");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(backend_name(), "null");
        let got = lb.take(PERIOD * 6);
        let loud = got.iter().filter(|v| (**v - 0.25).abs() < 1e-6).count();
        assert!(loud >= PERIOD * 2 * 3, "au moins trois périodes de son");
        assert!(!lock(&engine().sources).iter().any(|(i, _)| *i == id));
    }
}

/// Convertit des trames stéréo 48 kHz vers le format d'un client de capture.
pub struct CaptureConverter {
    pub format: format::Format,
    rs: dsp::Resampler,
}

impl CaptureConverter {
    pub fn new(format: format::Format) -> Self {
        CaptureConverter {
            format,
            rs: dsp::Resampler::new(),
        }
    }

    /// Rééchantillonne `input` (stéréo, [`RATE`] Hz) et renvoie les octets
    /// correspondants dans le format du client.
    pub fn convert(&mut self, input: &[f32]) -> Vec<u8> {
        let rate = self.format.rate as usize;
        let frames_in = input.len() / 2;
        let max_out = frames_in * rate / RATE as usize + 2;
        let mut tmp = vec![0.0f32; max_out * 2];
        let mut it = input.as_chunks::<2>().0.iter();
        let mut pull = || it.next().map(|c| [c[0], c[1]]);
        let n = self
            .rs
            .mix_into(RATE as f64 / rate as f64, &mut tmp, [1.0, 1.0], &mut pull);
        let block = self.format.block as usize;
        let mut bytes = vec![0u8; n * block];
        for (i, fr) in tmp[..n * 2].as_chunks::<2>().0.iter().enumerate() {
            self.format
                .write_stereo(&mut bytes[i * block..(i + 1) * block], [fr[0], fr[1]]);
        }
        bytes
    }
}

#[cfg(test)]
mod real_device_tests {
    /// Lit 0,2 s sur la source par défaut du serveur son réel.
    /// `WINLUX_AUDIO=pipewire cargo test -p winlux capture_reelle -- --ignored`
    #[test]
    #[ignore = "nécessite un serveur PipeWire ou PulseAudio actif"]
    fn capture_reelle() {
        let q = super::subscribe_microphone();
        let t0 = std::time::Instant::now();
        while q.available() < super::RATE as usize / 5 {
            assert!(t0.elapsed().as_secs() < 5, "aucune trame capturée");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
