//! Synthétiseur General MIDI maison (#237).
//!
//! Les anciens jeux jouent leur musique par `midiOut*` / `midiStream*` :
//! sous Windows, un synthétiseur logiciel livré avec le système fait le
//! son. Winlux n'embarque aucune banque d'échantillons : chaque famille
//! d'instruments General MIDI est produite par un modèle simple
//! (harmoniques, cordes pincées de Karplus-Strong, dents de scie filtrées,
//! orgue additif…) et les percussions du canal 10 par des bruits et des
//! glissements de hauteur. Le résultat n'imite aucun synthétiseur précis ;
//! il garde la mélodie, les accords, la dynamique et le rythme.
//!
//! Les messages suivis : note on/off, aftertouch ignoré, changement de
//! programme, pitch bend (étendue par RPN 0), contrôleurs 1 (modulation),
//! 7 (volume), 10 (panoramique), 11 (expression), 64 (sustain),
//! 120/123 (sons et notes coupés), 121 (contrôleurs remis), 100/101/6
//! (RPN), et les remises à zéro GM/GS/XG par message système exclusif.

use std::f32::consts::{PI, TAU};

/// Voix jouées en même temps au plus ; au-delà, la plus ancienne (de
/// préférence relâchée) est reprise.
const MAX_VOICES: usize = 64;
/// Gain global : beaucoup de voix s'additionnent, une limite douce suit.
const MASTER: f32 = 0.22;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Timbre {
    Piano,
    Bell,
    Organ,
    Pluck { bright: f32 },
    Strings,
    Brass,
    Reed,
    Pipe,
    Lead,
    Pad,
    Noise,
    Drum(Drum),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Drum {
    Kick,
    Snare,
    Stick,
    Clap,
    Tom(f32),
    HatClosed,
    HatOpen,
    Crash,
    Ride,
    Perc(f32),
}

/// Famille General MIDI (programme 0..127) → modèle de son.
fn timbre(program: u8) -> Timbre {
    match program / 8 {
        0 => Timbre::Piano,
        1 | 14 => Timbre::Bell,
        2 => Timbre::Organ,
        3 => Timbre::Pluck { bright: 0.5 },
        4 => Timbre::Pluck { bright: 0.2 },
        5 | 6 => Timbre::Strings,
        7 => Timbre::Brass,
        8 => Timbre::Reed,
        9 => Timbre::Pipe,
        10 => Timbre::Lead,
        11 | 12 => Timbre::Pad,
        13 => Timbre::Pluck { bright: 0.7 },
        _ => Timbre::Noise,
    }
}

/// Note du canal 10 (General MIDI) → percussion.
fn drum(note: u8) -> Drum {
    match note {
        35 | 36 => Drum::Kick,
        38 | 40 => Drum::Snare,
        37 => Drum::Stick,
        39 => Drum::Clap,
        41 | 43 | 45 | 47 | 48 | 50 => Drum::Tom(70.0 + (note as f32 - 41.0) * 14.0),
        42 | 44 => Drum::HatClosed,
        46 => Drum::HatOpen,
        49 | 52 | 55 | 57 => Drum::Crash,
        51 | 53 | 59 => Drum::Ride,
        _ => Drum::Perc(200.0 + (note as f32) * 12.0),
    }
}

#[derive(Clone, Copy)]
struct Channel {
    program: u8,
    volume: u8,
    expression: u8,
    pan: u8,
    sustain: bool,
    /// Pitch bend en demi-tons (déjà multiplié par l'étendue).
    bend: f32,
    bend_raw: i32,
    bend_range: f32,
    modulation: f32,
    rpn: (u8, u8),
}

impl Default for Channel {
    fn default() -> Self {
        Channel {
            program: 0,
            volume: 100,
            expression: 127,
            pan: 64,
            sustain: false,
            bend: 0.0,
            bend_raw: 0,
            bend_range: 2.0,
            modulation: 0.0,
            rpn: (127, 127),
        }
    }
}

impl Channel {
    fn gain(&self) -> [f32; 2] {
        let v = self.volume as f32 / 127.0;
        let e = self.expression as f32 / 127.0;
        let g = v * v * e * e;
        let p = (self.pan.min(127) as f32 / 127.0) * PI / 2.0;
        [g * p.cos() * 1.414, g * p.sin() * 1.414]
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Attack,
    Decay,
    Sustain,
    Release,
    Off,
}

/// Enveloppe ADSR linéaire en attaque, exponentielle ensuite. Les durées
/// de déclin et de relâchement sont celles d'une chute de 60 dB.
#[derive(Clone, Copy)]
struct Env {
    stage: Stage,
    level: f32,
    attack: f32,
    /// Coefficients par échantillon (multiplicatifs).
    decay: f32,
    sustain: f32,
    release: f32,
}

impl Env {
    fn new(rate: f32, a: f32, d: f32, s: f32, r: f32) -> Env {
        let coef = |t: f32| (-6.9 / (t.max(0.001) * rate)).exp();
        Env {
            stage: Stage::Attack,
            level: 0.0,
            attack: 1.0 / (a.max(0.001) * rate),
            decay: coef(d),
            sustain: s,
            release: coef(r),
        }
    }

    fn next(&mut self) -> f32 {
        match self.stage {
            Stage::Attack => {
                self.level += self.attack;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level = self.sustain + (self.level - self.sustain) * self.decay;
                if self.level - self.sustain < 0.001 {
                    self.stage = Stage::Sustain;
                }
                if self.sustain <= 0.0 && self.level < 0.001 {
                    self.stage = Stage::Off;
                }
            }
            Stage::Sustain => {
                if self.sustain <= 0.0 {
                    self.stage = Stage::Off;
                }
            }
            Stage::Release => {
                self.level *= self.release;
                if self.level < 0.001 {
                    self.stage = Stage::Off;
                }
            }
            Stage::Off => self.level = 0.0,
        }
        self.level
    }

    fn release(&mut self) {
        if self.stage != Stage::Off {
            self.stage = Stage::Release;
        }
    }
}

/// Correction polyBLEP : adoucit les sauts des dents de scie et des carrés
/// (moins de repliement de spectre que les formes naïves).
fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

fn saw(phase: f32, dt: f32) -> f32 {
    2.0 * phase - 1.0 - blep(phase, dt)
}

fn square(phase: f32, dt: f32) -> f32 {
    let mut v = if phase < 0.5 { 1.0 } else { -1.0 };
    v += blep(phase, dt);
    let p2 = (phase + 0.5).fract();
    v -= blep(p2, dt);
    v
}

fn triangle(phase: f32) -> f32 {
    4.0 * (phase - 0.5).abs() - 1.0
}

/// Filtre passe-bas d'état variable (Chamberlin), suffisant pour les
/// timbres soustractifs.
#[derive(Clone, Copy, Default)]
struct Svf {
    low: f32,
    band: f32,
}

impl Svf {
    fn low(&mut self, x: f32, cutoff: f32, rate: f32, q: f32) -> f32 {
        let f = (2.0 * (PI * cutoff.min(rate * 0.2) / rate).sin()).min(1.2);
        self.low += f * self.band;
        let high = x - self.low - q * self.band;
        self.band += f * high;
        if !self.low.is_finite() || !self.band.is_finite() {
            *self = Svf::default();
        }
        self.low
    }

    fn high(&mut self, x: f32, cutoff: f32, rate: f32) -> f32 {
        let l = self.low(x, cutoff, rate, 1.0);
        x - l
    }
}

struct Voice {
    channel: u8,
    note: u8,
    velocity: f32,
    timbre: Timbre,
    env: Env,
    /// Note relâchée pendant que la pédale de sustain est enfoncée.
    held: bool,
    phase: [f32; 3],
    /// Échantillons écoulés depuis le début de la note.
    age: u32,
    filter: Svf,
    filter2: Svf,
    /// Corde de Karplus-Strong.
    string: Vec<f32>,
    pos: usize,
    last: f32,
}

/// Générateur de bruit blanc (xorshift), déterministe.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

pub struct Synth {
    rate: f32,
    channels: [Channel; 16],
    voices: Vec<Voice>,
    noise: Noise,
    running: u8,
    /// Phase du vibrato commun (modulation).
    lfo: f32,
}

impl Synth {
    pub fn new(rate: u32) -> Synth {
        Synth {
            rate: rate as f32,
            channels: [Channel::default(); 16],
            voices: Vec::with_capacity(MAX_VOICES),
            noise: Noise(0x2545_F491),
            running: 0,
            lfo: 0.0,
        }
    }

    /// Voix en cours (tests, diagnostic).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn active_voices(&self) -> usize {
        self.voices.len()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn program(&self, channel: usize) -> u8 {
        self.channels[channel & 15].program
    }

    /// Remise à zéro complète (GM reset, midiOutReset).
    pub fn reset(&mut self) {
        self.voices.clear();
        self.channels = [Channel::default(); 16];
        self.running = 0;
    }

    /// Coupe toutes les notes avec leur relâchement.
    pub fn all_notes_off(&mut self) {
        for c in self.channels.iter_mut() {
            c.sustain = false;
        }
        for v in self.voices.iter_mut() {
            v.held = false;
            v.env.release();
        }
    }

    /// Message court packé comme pour midiOutShortMsg (octet de statut en
    /// bas). Le statut courant (« running status ») est accepté.
    pub fn short(&mut self, msg: u32) {
        let mut b = [msg as u8, (msg >> 8) as u8, (msg >> 16) as u8];
        if b[0] < 0x80 {
            if self.running == 0 {
                return;
            }
            b = [self.running, b[0], b[1]];
        } else if b[0] < 0xF0 {
            self.running = b[0];
        }
        let ch = (b[0] & 15) as usize;
        let (d1, d2) = (b[1] & 0x7F, b[2] & 0x7F);
        match b[0] & 0xF0 {
            0x80 => self.note_off(ch, d1),
            0x90 if d2 == 0 => self.note_off(ch, d1),
            0x90 => self.note_on(ch, d1, d2),
            0xB0 => self.control(ch, d1, d2),
            0xC0 => self.channels[ch].program = d1,
            0xE0 => {
                let c = &mut self.channels[ch];
                c.bend_raw = ((d2 as i32) << 7 | d1 as i32) - 8192;
                c.bend = c.bend_raw as f32 / 8192.0 * c.bend_range;
            }
            _ => {}
        }
    }

    /// Message système exclusif complet (F0 … F7).
    pub fn sysex(&mut self, data: &[u8]) {
        let gm_on = [0xF0, 0x7E, 0x7F, 0x09, 0x01, 0xF7];
        let gs_reset = [0x41, 0x10, 0x42, 0x12, 0x40, 0x00, 0x7F];
        let xg_on = [0x43, 0x10, 0x4C, 0x00, 0x00, 0x7E, 0x00];
        if data.starts_with(&gm_on[..5])
            || data.windows(gs_reset.len()).any(|w| w == gs_reset)
            || data.windows(xg_on.len()).any(|w| w == xg_on)
        {
            self.reset();
        }
    }

    fn control(&mut self, ch: usize, cc: u8, v: u8) {
        let c = &mut self.channels[ch];
        match cc {
            1 => c.modulation = v as f32 / 127.0,
            6 if c.rpn == (0, 0) => {
                c.bend_range = v as f32;
                c.bend = c.bend_raw as f32 / 8192.0 * c.bend_range;
            }
            7 => c.volume = v,
            10 => c.pan = v,
            11 => c.expression = v,
            64 => {
                c.sustain = v >= 64;
                if !c.sustain {
                    for voice in self.voices.iter_mut() {
                        if voice.channel as usize == ch && voice.held {
                            voice.held = false;
                            voice.env.release();
                        }
                    }
                }
            }
            100 => c.rpn.1 = v,
            101 => c.rpn.0 = v,
            120 => self.voices.retain(|x| x.channel as usize != ch),
            121 => {
                let program = c.program;
                *c = Channel {
                    program,
                    ..Channel::default()
                };
            }
            123..=127 => {
                for voice in self.voices.iter_mut() {
                    if voice.channel as usize == ch {
                        voice.held = false;
                        voice.env.release();
                    }
                }
            }
            _ => {}
        }
    }

    fn note_off(&mut self, ch: usize, note: u8) {
        let sustain = self.channels[ch].sustain;
        for v in self.voices.iter_mut() {
            if v.channel as usize == ch && v.note == note && !v.held && v.env.stage != Stage::Release
            {
                if sustain {
                    v.held = true;
                } else {
                    v.env.release();
                }
            }
        }
    }

    fn note_on(&mut self, ch: usize, note: u8, vel: u8) {
        let r = self.rate;
        let timbre = if ch == 9 {
            Timbre::Drum(drum(note))
        } else {
            timbre(self.channels[ch].program)
        };
        // Même note rejouée sur le même canal : l'ancienne s'éteint vite.
        for v in self.voices.iter_mut() {
            if v.channel as usize == ch && v.note == note {
                v.held = false;
                v.env.release = (-6.9 / (0.01 * r)).exp();
                v.env.release();
            }
        }
        // Charleston fermée : coupe la charleston ouverte.
        if timbre == Timbre::Drum(Drum::HatClosed) {
            for v in self.voices.iter_mut() {
                if v.timbre == Timbre::Drum(Drum::HatOpen) {
                    v.env.release = (-6.9 / (0.02 * r)).exp();
                    v.env.release();
                }
            }
        }
        if self.voices.len() >= MAX_VOICES {
            let victim = self
                .voices
                .iter()
                .enumerate()
                .max_by_key(|(_, v)| (v.env.stage == Stage::Release, v.age))
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.voices.swap_remove(victim);
        }
        let freq = 440.0 * 2f32.powf((note as f32 - 69.0) / 12.0);
        // Les notes graves sonnent plus longtemps (piano, cordes pincées).
        let low = (1.0 - (note as f32 - 21.0) / 88.0).clamp(0.2, 1.0);
        let env = match timbre {
            Timbre::Piano => Env::new(r, 0.002, 0.6 + 2.5 * low, 0.0, 0.25),
            Timbre::Bell => Env::new(r, 0.001, 0.5 + 1.0 * low, 0.0, 0.3),
            Timbre::Organ => Env::new(r, 0.006, 0.05, 1.0, 0.06),
            Timbre::Pluck { .. } => Env::new(r, 0.001, 30.0, 1.0, 0.08),
            Timbre::Strings => Env::new(r, 0.09, 0.2, 0.9, 0.35),
            Timbre::Brass => Env::new(r, 0.03, 0.2, 0.8, 0.12),
            Timbre::Reed => Env::new(r, 0.02, 0.1, 0.85, 0.08),
            Timbre::Pipe => Env::new(r, 0.03, 0.1, 0.9, 0.1),
            Timbre::Lead => Env::new(r, 0.005, 0.1, 0.8, 0.1),
            Timbre::Pad => Env::new(r, 0.35, 0.5, 0.8, 0.8),
            Timbre::Noise => Env::new(r, 0.01, 0.4, 0.0, 0.2),
            Timbre::Drum(d) => {
                let decay = match d {
                    Drum::Kick => 0.25,
                    Drum::Snare => 0.16,
                    Drum::Stick => 0.03,
                    Drum::Clap => 0.12,
                    Drum::Tom(_) => 0.3,
                    Drum::HatClosed => 0.04,
                    Drum::HatOpen => 0.35,
                    Drum::Crash => 1.2,
                    Drum::Ride => 0.7,
                    Drum::Perc(_) => 0.1,
                };
                Env::new(r, 0.0008, decay, 0.0, decay)
            }
        };
        let mut string = Vec::new();
        if let Timbre::Pluck { bright } = timbre {
            let n = (r / freq).round().max(2.0) as usize;
            string = (0..n).map(|_| self.noise.next()).collect();
            // Attaque moins brillante pour les basses : bruit lissé.
            let mut prev = 0.0;
            for s in string.iter_mut() {
                *s = *s * bright + prev * (1.0 - bright);
                prev = *s;
            }
        }
        let v = vel as f32 / 127.0;
        self.voices.push(Voice {
            channel: ch as u8,
            note,
            velocity: v * v,
            timbre,
            env,
            held: false,
            phase: [0.0, 0.33, 0.66],
            age: 0,
            filter: Svf::default(),
            filter2: Svf::default(),
            string,
            pos: 0,
            last: 0.0,
        });
    }

    /// Ajoute `out.len() / 2` trames stéréo produites par le synthétiseur.
    pub fn render(&mut self, out: &mut [f32]) {
        let frames = out.len() / 2;
        if self.voices.is_empty() {
            return;
        }
        let rate = self.rate;
        let lfo_step = 5.5 / rate;
        let gains: Vec<[f32; 2]> = self.channels.iter().map(|c| c.gain()).collect();
        for f in 0..frames {
            self.lfo = (self.lfo + lfo_step).fract();
            let vib = (self.lfo * TAU).sin();
            let mut l = 0.0;
            let mut rr = 0.0;
            for v in self.voices.iter_mut() {
                let c = &self.channels[v.channel as usize];
                let e = v.env.next();
                if e <= 0.0 && v.env.stage == Stage::Off {
                    continue;
                }
                let semis = v.note as f32 - 69.0 + c.bend + vib * c.modulation * 0.5;
                let freq = 440.0 * (semis / 12.0).exp2();
                let s = sample(v, freq, rate, &mut self.noise) * e * v.velocity;
                let g = gains[v.channel as usize];
                l += s * g[0];
                rr += s * g[1];
                v.age = v.age.saturating_add(1);
            }
            out[2 * f] += soft_clip(l * MASTER);
            out[2 * f + 1] += soft_clip(rr * MASTER);
        }
        self.voices.retain(|v| v.env.stage != Stage::Off);
    }
}

fn soft_clip(x: f32) -> f32 {
    if x.abs() < 0.6 {
        x
    } else {
        x.signum() * (0.6 + 0.4 * ((x.abs() - 0.6) / 0.4).tanh())
    }
}

/// Un échantillon (avant enveloppe) de la voix `v` à la fréquence `freq`.
fn sample(v: &mut Voice, freq: f32, rate: f32, noise: &mut Noise) -> f32 {
    let dt = (freq / rate).min(0.45);
    let mut adv = |i: usize, mul: f32| {
        let p = v.phase[i];
        v.phase[i] = (p + dt * mul).fract();
        p
    };
    let t = v.age as f32 / rate;
    match v.timbre {
        Timbre::Piano => {
            let p = adv(0, 1.0);
            let mut s = 0.0;
            // Harmoniques qui s'éteignent plus vite que la fondamentale.
            for (k, amp) in [(1.0, 1.0), (2.0, 0.5), (3.0, 0.3), (4.0, 0.18), (5.0, 0.1)] {
                if freq * k < rate * 0.45 {
                    s += amp * (TAU * p * k).sin() * (-t * k * 1.5).exp();
                }
            }
            s * 0.6
        }
        Timbre::Bell => {
            let p = adv(0, 1.0);
            let p2 = adv(1, 3.5);
            (TAU * p).sin() * 0.7 + (TAU * p2).sin() * 0.3 * (-t * 6.0).exp()
        }
        Timbre::Organ => {
            let p = adv(0, 1.0);
            let mut s = 0.0;
            for (k, amp) in [(0.5, 0.4), (1.0, 0.7), (2.0, 0.5), (3.0, 0.3), (4.0, 0.2)] {
                if freq * k < rate * 0.45 {
                    s += amp * (TAU * p * k).sin();
                }
            }
            s * 0.45
        }
        Timbre::Pluck { .. } => {
            let n = v.string.len();
            if n == 0 {
                return 0.0;
            }
            let cur = v.string[v.pos];
            let next = v.string[(v.pos + 1) % n];
            // Moyenne amortie : la corde perd ses aigus puis s'éteint.
            let damp = if v.env.stage == Stage::Release { 0.95 } else { 0.996 };
            v.string[v.pos] = (cur + next) * 0.5 * damp;
            v.pos = (v.pos + 1) % n;
            v.last = cur;
            cur * 0.9
        }
        Timbre::Strings | Timbre::Pad => {
            let a = saw(adv(0, 1.0), dt);
            let b = saw(adv(1, 1.004), dt);
            let c = saw(adv(2, 0.996), dt);
            let cut = if v.timbre == Timbre::Pad { 1800.0 } else { 3500.0 };
            v.filter.low((a + b + c) * 0.33, cut, rate, 0.9) * 0.8
        }
        Timbre::Brass => {
            let s = saw(adv(0, 1.0), dt);
            let open = 1.0 - (-t * 25.0).exp();
            v.filter.low(s, freq * (1.5 + 5.0 * open), rate, 0.7) * 0.7
        }
        Timbre::Reed => {
            let s = square(adv(0, 1.0), dt);
            v.filter.low(s, freq * 4.0, rate, 0.8) * 0.5
        }
        Timbre::Pipe => {
            let p = adv(0, 1.0);
            let breath = v.filter2.low(noise.next(), freq * 2.0, rate, 1.0) * 0.15;
            (TAU * p).sin() * 0.7 + triangle(adv(1, 2.0)) * 0.1 + breath
        }
        Timbre::Lead => {
            let a = square(adv(0, 1.0), dt);
            let b = saw(adv(1, 1.003), dt);
            v.filter.low((a + b) * 0.5, freq * 6.0, rate, 0.6) * 0.55
        }
        Timbre::Noise => v.filter.low(noise.next(), freq * 2.0, rate, 0.5) * 0.6,
        Timbre::Drum(d) => match d {
            Drum::Kick => {
                let f = 50.0 + 110.0 * (-t * 30.0).exp();
                v.phase[0] = (v.phase[0] + f / rate).fract();
                (TAU * v.phase[0]).sin() * 1.4
            }
            Drum::Tom(base) => {
                let f = base * (1.0 + 0.6 * (-t * 20.0).exp());
                v.phase[0] = (v.phase[0] + f / rate).fract();
                (TAU * v.phase[0]).sin()
            }
            Drum::Snare => {
                v.phase[0] = (v.phase[0] + 185.0 / rate).fract();
                let tone = (TAU * v.phase[0]).sin() * (-t * 30.0).exp();
                let n = v.filter.high(noise.next(), 1500.0, rate);
                tone * 0.5 + n * 0.8
            }
            Drum::Stick => v.filter.high(noise.next(), 3000.0, rate) * 0.8,
            Drum::Clap => {
                // Trois claquements rapprochés puis la queue.
                let burst = if t < 0.03 {
                    ((t * 1000.0) as u32 % 10 < 3) as u32 as f32
                } else {
                    1.0
                };
                v.filter.high(noise.next(), 1000.0, rate) * burst * 0.9
            }
            Drum::HatClosed | Drum::HatOpen => v.filter.high(noise.next(), 7000.0, rate) * 0.6,
            Drum::Crash => v.filter.high(noise.next(), 4000.0, rate) * 0.5,
            Drum::Ride => {
                let p = adv(0, 1.0);
                let ring = (TAU * p * 5.3).sin() * 0.2;
                v.filter.high(noise.next(), 5000.0, rate) * 0.35 + ring * (-t * 3.0).exp()
            }
            Drum::Perc(f) => {
                v.phase[0] = (v.phase[0] + f / rate).fract();
                (TAU * v.phase[0]).sin() * 0.7 + v.filter.high(noise.next(), 2000.0, rate) * 0.2
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn energy(s: &mut Synth, frames: usize) -> f32 {
        let mut out = vec![0.0f32; frames * 2];
        s.render(&mut out);
        out.iter().map(|x| x * x).sum::<f32>() / frames as f32
    }

    #[test]
    fn note_on_off_et_silence() {
        let mut s = Synth::new(48_000);
        assert_eq!(energy(&mut s, 480), 0.0);
        s.short(0x7F_3C_90); // note on, do 4, vélocité 127
        assert_eq!(s.active_voices(), 1);
        assert!(energy(&mut s, 4800) > 1e-4);
        s.short(0x00_3C_80);
        // Relâchement puis plus aucune voix.
        energy(&mut s, 48_000);
        assert_eq!(s.active_voices(), 0);
    }

    #[test]
    fn statut_courant_et_velocite_nulle() {
        let mut s = Synth::new(48_000);
        s.short(0x64_40_91); // note on canal 2
        s.short(0x00_64_43); // statut courant : autre note
        assert_eq!(s.active_voices(), 2);
        s.short(0x00_00_40); // vélocité 0 = note off
        energy(&mut s, 48_000);
        assert_eq!(s.active_voices(), 1);
    }

    #[test]
    fn sustain_garde_la_note() {
        let mut s = Synth::new(48_000);
        s.short(0x7F_40_B0); // sustain enfoncé
        s.short(0x7F_30_90 | 0x0000_0000);
        s.short(0x00_30_80);
        energy(&mut s, 9600);
        assert_eq!(s.active_voices(), 1, "note tenue par la pédale");
        s.short(0x00_40_B0);
        energy(&mut s, 48_000);
        assert_eq!(s.active_voices(), 0);
    }

    #[test]
    fn programmes_et_percussions_sonnent() {
        for program in (0..128).step_by(8) {
            let mut s = Synth::new(48_000);
            s.short(0xC0 | (program as u32) << 8);
            assert_eq!(s.program(0), program as u8);
            s.short(0x64_45_90);
            let e = energy(&mut s, 4800);
            assert!(e > 1e-5 && e.is_finite(), "programme {program} : {e}");
        }
        for note in 35..82u32 {
            let mut s = Synth::new(48_000);
            s.short(0x7F_00_99 | note << 8);
            let e = energy(&mut s, 2400);
            assert!(e > 1e-6 && e.is_finite(), "percussion {note} : {e}");
        }
    }

    #[test]
    fn pas_de_saturation() {
        let mut s = Synth::new(48_000);
        for ch in 0..16u32 {
            for n in 0..4u32 {
                s.short(0x7F_00_90 | ch | (40 + n * 7) << 8);
            }
        }
        let mut out = vec![0.0f32; 9600];
        s.render(&mut out);
        assert!(out.iter().all(|x| x.abs() <= 1.0));
    }

    #[test]
    fn remise_a_zero_gm() {
        let mut s = Synth::new(48_000);
        s.short(0x05_C3);
        s.short(0x7F_40_93);
        s.sysex(&[0xF0, 0x7E, 0x7F, 0x09, 0x01, 0xF7]);
        assert_eq!(s.active_voices(), 0);
        assert_eq!(s.program(3), 0);
    }

    #[test]
    fn etendue_du_pitch_bend_par_rpn() {
        let mut s = Synth::new(48_000);
        s.short(0x00_65_B0);
        s.short(0x00_64_B0);
        s.short(0x0C_06_B0); // 12 demi-tons
        s.short(0x7F_7F_E0); // bend maximal
        assert!((s.channels[0].bend - 12.0).abs() < 0.01);
    }
}
