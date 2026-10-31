//! Traitements du signal partagés par les API audio : rééchantillonnage,
//! gains en décibels, panoramique, filtres XAudio2, réverbération et 3D simple.

/// Rééchantillonneur linéaire stéréo en flux tiré (« pull »).
///
/// Il garde son état entre deux appels : un flux qui manque de données
/// reprend exactement là où il s'était arrêté quand de nouveaux tampons
/// arrivent (waveOut, XAudio2…).
#[derive(Clone, Debug)]
pub struct Resampler {
    frac: f64,
    prev: [f32; 2],
    cur: [f32; 2],
    need: u32,
}

impl Default for Resampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Resampler {
    pub fn new() -> Self {
        Resampler {
            frac: 0.0,
            prev: [0.0; 2],
            cur: [0.0; 2],
            need: 2,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Produit jusqu'à `out.len() / 2` trames stéréo en ajoutant `x * gain`
    /// dans `out`. `step` = fréquence source / fréquence de sortie. S'arrête
    /// quand `pull` ne fournit plus de trame et renvoie le nombre de trames
    /// produites.
    pub fn mix_into(
        &mut self,
        step: f64,
        out: &mut [f32],
        gain: [f32; 2],
        pull: &mut dyn FnMut() -> Option<[f32; 2]>,
    ) -> usize {
        let step = if step.is_finite() && step > 0.0 {
            step.min(1024.0)
        } else {
            1.0
        };
        let frames = out.len() / 2;
        for i in 0..frames {
            while self.need > 0 {
                match pull() {
                    Some(f) => {
                        self.prev = self.cur;
                        self.cur = f;
                        self.need -= 1;
                    }
                    None => return i,
                }
            }
            let t = self.frac as f32;
            let l = self.prev[0] + (self.cur[0] - self.prev[0]) * t;
            let r = self.prev[1] + (self.cur[1] - self.prev[1]) * t;
            out[2 * i] += l * gain[0];
            out[2 * i + 1] += r * gain[1];
            self.frac += step;
            let adv = self.frac.floor();
            self.frac -= adv;
            self.need = adv as u32;
        }
        frames
    }
}

/// Atténuation DirectSound (centièmes de dB, -10000..0) vers un gain linéaire.
pub fn db100_to_gain(v: i32) -> f32 {
    if v <= -10000 {
        0.0
    } else {
        10f32.powf(v.min(0) as f32 / 2000.0)
    }
}

/// Gain linéaire vers des centièmes de dB (inverse de [`db100_to_gain`]).
pub fn gain_to_db100(g: f32) -> i32 {
    if g <= 0.00001 {
        -10000
    } else {
        ((g.log10() * 2000.0).round() as i32).clamp(-10000, 0)
    }
}

/// Panoramique DirectSound (-10000 gauche .. 10000 droite) en gains (G, D).
pub fn pan_gains(pan: i32) -> [f32; 2] {
    let pan = pan.clamp(-10000, 10000);
    if pan < 0 {
        [1.0, db100_to_gain(pan)]
    } else {
        [db100_to_gain(-pan), 1.0]
    }
}

/// Type de filtre XAudio2 (XAUDIO2_FILTER_TYPE, 2.8+).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterKind {
    LowPass,
    BandPass,
    HighPass,
    Notch,
    LowPassOnePole,
    HighPassOnePole,
}

impl FilterKind {
    pub fn from_raw(v: u32) -> Option<Self> {
        Some(match v {
            0 => FilterKind::LowPass,
            1 => FilterKind::BandPass,
            2 => FilterKind::HighPass,
            3 => FilterKind::Notch,
            4 => FilterKind::LowPassOnePole,
            5 => FilterKind::HighPassOnePole,
            _ => return None,
        })
    }
    pub fn raw(self) -> u32 {
        self as u32
    }
}

/// Filtre à variables d'état (Chamberlin) tel que décrit par XAudio2 :
/// `frequency` vaut 2·sin(π·fc/fs) et `one_over_q` l'inverse du facteur Q.
#[derive(Clone, Copy, Debug)]
pub struct Filter {
    pub kind: FilterKind,
    pub frequency: f32,
    pub one_over_q: f32,
    low: [f32; 2],
    band: [f32; 2],
}

impl Default for Filter {
    fn default() -> Self {
        Filter {
            kind: FilterKind::LowPass,
            frequency: 1.0,
            one_over_q: 1.0,
            low: [0.0; 2],
            band: [0.0; 2],
        }
    }
}

impl Filter {
    pub fn set(&mut self, kind: FilterKind, frequency: f32, one_over_q: f32) {
        self.kind = kind;
        self.frequency = frequency.clamp(0.0, 1.0);
        self.one_over_q = one_over_q.clamp(0.0001, 1.5);
    }

    /// Filtre passe-tout : passe-bas grand ouvert.
    pub fn is_bypass(&self) -> bool {
        self.kind == FilterKind::LowPass && self.frequency >= 1.0 && self.one_over_q >= 1.0
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        if self.is_bypass() {
            return;
        }
        let f = self.frequency;
        let q = self.one_over_q;
        for fr in buf.chunks_exact_mut(2) {
            for (c, x) in fr.iter_mut().enumerate() {
                let input = *x;
                *x = match self.kind {
                    FilterKind::LowPassOnePole => {
                        self.low[c] += f * (input - self.low[c]);
                        self.low[c]
                    }
                    FilterKind::HighPassOnePole => {
                        self.low[c] += f * (input - self.low[c]);
                        input - self.low[c]
                    }
                    _ => {
                        self.low[c] += f * self.band[c];
                        let high = input - self.low[c] - q * self.band[c];
                        self.band[c] += f * high;
                        if !self.band[c].is_finite() || !self.low[c].is_finite() {
                            self.band[c] = 0.0;
                            self.low[c] = 0.0;
                        }
                        match self.kind {
                            FilterKind::LowPass => self.low[c],
                            FilterKind::HighPass => high,
                            FilterKind::BandPass => self.band[c],
                            _ => high + self.low[c],
                        }
                    }
                };
            }
        }
    }
}

/// Réverbération simple de type Schroeder (4 filtres en peigne + 2 passe-tout
/// par canal) pour l'effet XAudio2 « Reverb ».
#[derive(Clone, Debug)]
pub struct Reverb {
    pub wet_dry: f32,
    pub decay: f32,
    combs: Vec<(Vec<f32>, usize, f32)>,
    allpass: Vec<(Vec<f32>, usize)>,
    rate: u32,
}

const COMBS: [usize; 4] = [1116, 1188, 1277, 1356];
const ALLPASS: [usize; 2] = [556, 441];

impl Reverb {
    pub fn new(rate: u32) -> Self {
        let scale = rate as f32 / 44100.0;
        let mut combs = Vec::new();
        let mut allpass = Vec::new();
        for ch in 0..2usize {
            for &c in &COMBS {
                let n = ((c + ch * 23) as f32 * scale) as usize;
                combs.push((vec![0.0; n.max(1)], 0, 0.0));
            }
            for &a in &ALLPASS {
                let n = ((a + ch * 23) as f32 * scale) as usize;
                allpass.push((vec![0.0; n.max(1)], 0));
            }
        }
        let mut r = Reverb {
            wet_dry: 100.0,
            decay: 1.0,
            combs,
            allpass,
            rate,
        };
        r.set(100.0, 1.0);
        r
    }

    /// `wet_dry` en pourcentage (0..100), `decay` en secondes (temps RT60).
    pub fn set(&mut self, wet_dry: f32, decay: f32) {
        self.wet_dry = wet_dry.clamp(0.0, 100.0);
        self.decay = decay.clamp(0.1, 30.0);
        let rate = self.rate as f32;
        let decay = self.decay;
        for (buf, _, fb) in self.combs.iter_mut() {
            // Gain de rétroaction donnant -60 dB après `decay` secondes.
            let delay = buf.len() as f32 / rate;
            *fb = 10f32.powf(-3.0 * delay / decay).min(0.98);
        }
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        let wet = self.wet_dry / 100.0;
        let dry = 1.0 - wet;
        for fr in buf.chunks_exact_mut(2) {
            for (c, x) in fr.iter_mut().enumerate() {
                let input = *x * 0.25;
                let mut acc = 0.0;
                for (b, pos, fb) in self.combs[c * 4..c * 4 + 4].iter_mut() {
                    let y = b[*pos];
                    b[*pos] = input + y * *fb;
                    *pos = (*pos + 1) % b.len();
                    acc += y;
                }
                for (b, pos) in self.allpass[c * 2..c * 2 + 2].iter_mut() {
                    let y = b[*pos];
                    b[*pos] = acc + y * 0.5;
                    *pos = (*pos + 1) % b.len();
                    acc = y - acc;
                }
                *x = *x * dry + acc * wet;
            }
        }
    }
}

/// Vecteur 3D (repère main gauche de DirectSound : x à droite, y en haut,
/// z vers l'avant).
pub type Vec3 = [f32; 3];

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: Vec3) -> f32 {
    dot(a, a).sqrt()
}

/// Paramètres d'écoute DirectSound 3D (DS3DLISTENER).
#[derive(Clone, Copy, Debug)]
pub struct Listener {
    pub position: Vec3,
    pub velocity: Vec3,
    pub front: Vec3,
    pub top: Vec3,
    pub distance_factor: f32,
    pub rolloff: f32,
    pub doppler: f32,
}

impl Default for Listener {
    fn default() -> Self {
        Listener {
            position: [0.0; 3],
            velocity: [0.0; 3],
            front: [0.0, 0.0, 1.0],
            top: [0.0, 1.0, 0.0],
            distance_factor: 1.0,
            rolloff: 1.0,
            doppler: 1.0,
        }
    }
}

/// Paramètres d'une source DirectSound 3D (DS3DBUFFER).
#[derive(Clone, Copy, Debug)]
pub struct Emitter {
    pub position: Vec3,
    pub velocity: Vec3,
    pub inside_cone: u32,
    pub outside_cone: u32,
    pub cone_orientation: Vec3,
    pub cone_outside_volume: i32,
    pub min_distance: f32,
    pub max_distance: f32,
    /// 0 = normal, 1 = relatif à la tête, 2 = 3D désactivée.
    pub mode: u32,
}

impl Default for Emitter {
    fn default() -> Self {
        Emitter {
            position: [0.0; 3],
            velocity: [0.0; 3],
            inside_cone: 360,
            outside_cone: 360,
            cone_orientation: [0.0, 0.0, 1.0],
            cone_outside_volume: 0,
            min_distance: 1.0,
            max_distance: 1_000_000_000.0,
            mode: 0,
        }
    }
}

/// Gains (G, D) d'une source 3D : atténuation en 1/distance au-delà de la
/// distance minimale (bornée par la distance maximale) et panoramique à
/// puissance constante selon l'azimut par rapport à l'auditeur.
pub fn spatialize(listener: &Listener, e: &Emitter) -> [f32; 2] {
    if e.mode == 2 {
        return [1.0, 1.0];
    }
    let rel = if e.mode == 1 {
        e.position
    } else {
        sub(e.position, listener.position)
    };
    let dist = norm(rel);
    let min = e.min_distance.max(0.0001);
    let d = dist.clamp(min, e.max_distance.max(min));
    let rolloff = listener.rolloff.clamp(0.0, 10.0);
    let atten = if d <= min {
        1.0
    } else {
        (min / (min + rolloff * (d - min))).clamp(0.0, 1.0)
    };
    let (front, top) = if e.mode == 1 {
        ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0])
    } else {
        (listener.front, listener.top)
    };
    let right = cross(top, front);
    let rn = norm(right);
    let s = if dist > 1e-6 && rn > 1e-6 {
        (dot(rel, right) / (dist * rn)).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let l = ((1.0 - s) * 0.5).sqrt();
    let r = ((1.0 + s) * 0.5).sqrt();
    // Normalise pour qu'une source centrée garde un gain unitaire.
    let k = std::f32::consts::SQRT_2;
    [atten * (l * k).min(1.0), atten * (r * k).min(1.0)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constant(v: [f32; 2], n: usize) -> impl FnMut() -> Option<[f32; 2]> {
        let mut left = n;
        move || {
            if left == 0 {
                None
            } else {
                left -= 1;
                Some(v)
            }
        }
    }

    #[test]
    fn reechantillonnage_conserve_la_duree() {
        // 22050 -> 48000 : 2205 trames source donnent ~4800 trames.
        let mut r = Resampler::new();
        let mut out = vec![0.0f32; 2 * 6000];
        let mut src = constant([0.5, -0.5], 2205);
        let n = r.mix_into(22050.0 / 48000.0, &mut out, [1.0, 1.0], &mut src);
        assert!((4795..=4802).contains(&n), "{n}");
        assert!((out[100] - 0.5).abs() < 1e-6 && (out[101] + 0.5).abs() < 1e-6);
    }

    #[test]
    fn reechantillonnage_reprend_apres_manque() {
        let mut r = Resampler::new();
        let mut out = vec![0.0f32; 20];
        let mut a = constant([1.0, 1.0], 4);
        let n1 = r.mix_into(1.0, &mut out, [1.0, 1.0], &mut a);
        let mut b = constant([1.0, 1.0], 100);
        let n2 = r.mix_into(1.0, &mut out[2 * n1..], [1.0, 1.0], &mut b);
        assert_eq!(n1 + n2, 10);
        assert!(out.iter().all(|&x| (x - 1.0).abs() < 1e-6));
    }

    #[test]
    fn gains_en_decibels_et_panoramique() {
        assert_eq!(db100_to_gain(0), 1.0);
        assert_eq!(db100_to_gain(-10000), 0.0);
        assert!((db100_to_gain(-600) - 0.501).abs() < 0.01);
        assert_eq!(gain_to_db100(1.0), 0);
        assert_eq!(gain_to_db100(db100_to_gain(-1234)), -1234);
        assert_eq!(pan_gains(0), [1.0, 1.0]);
        let p = pan_gains(-10000);
        assert_eq!(p, [1.0, 0.0]);
    }

    #[test]
    fn filtre_passe_bas_attenue_les_aigus() {
        let mut f = Filter::default();
        assert!(f.is_bypass());
        // fc ≈ 500 Hz à 48 kHz.
        let coef = 2.0 * (std::f32::consts::PI * 500.0 / 48000.0).sin();
        f.set(FilterKind::LowPass, coef, 1.0);
        let mut buf: Vec<f32> = (0..9600)
            .flat_map(|i| {
                let v = if i % 2 == 0 { 1.0 } else { -1.0 }; // 24 kHz
                [v, v]
            })
            .collect();
        f.process(&mut buf);
        let tail = buf[9000..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(tail < 0.05, "{tail}");
    }

    #[test]
    fn reverb_produit_une_queue() {
        let mut r = Reverb::new(48000);
        r.set(50.0, 2.0);
        let mut buf = vec![0.0f32; 2 * 48000];
        buf[0] = 1.0;
        buf[1] = 1.0;
        r.process(&mut buf);
        let late = buf[2 * 10000..2 * 20000]
            .iter()
            .fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(late > 1e-4 && late < 1.0, "{late}");
    }

    #[test]
    fn spatialisation_gauche_droite_et_distance() {
        let l = Listener::default();
        let right = Emitter {
            position: [5.0, 0.0, 0.0],
            ..Emitter::default()
        };
        let g = spatialize(&l, &right);
        assert!(g[1] > g[0] * 5.0, "{g:?}");
        let near = Emitter {
            position: [0.0, 0.0, 1.0],
            ..Emitter::default()
        };
        let far = Emitter {
            position: [0.0, 0.0, 10.0],
            ..Emitter::default()
        };
        let gn = spatialize(&l, &near);
        let gf = spatialize(&l, &far);
        assert!((gn[0] - 1.0).abs() < 1e-3 && (gn[0] - gn[1]).abs() < 1e-6);
        assert!((gf[0] - 0.1).abs() < 1e-3, "{gf:?}");
    }
}
