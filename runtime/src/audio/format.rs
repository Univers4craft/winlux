//! Formats audio Windows (WAVEFORMATEX / WAVEFORMATEXTENSIBLE) et conversion
//! des échantillons vers le format interne du mixeur (stéréo f32).

/// Type d'échantillon accepté par les API audio Windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sample {
    U8,
    S16,
    S24,
    S32,
    F32,
}

pub const WAVE_FORMAT_PCM: u16 = 1;
pub const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
pub const WAVE_FORMAT_EXTENSIBLE: u16 = 0xfffe;
/// Fin commune des GUID KSDATAFORMAT_SUBTYPE_* (PCM = 1, IEEE_FLOAT = 3).
const SUBTYPE_TAIL: [u8; 12] = [
    0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71,
];

// Masques de canaux (ksmedia.h).
const FL: u32 = 0x1;
const FR: u32 = 0x2;
const FC: u32 = 0x4;
const LFE: u32 = 0x8;
const BL: u32 = 0x10;
const BR: u32 = 0x20;
const FLC: u32 = 0x40;
const FRC: u32 = 0x80;
const BC: u32 = 0x100;
const SL: u32 = 0x200;
const SR: u32 = 0x400;

/// Masque de canaux par défaut de Windows pour `n` canaux.
pub fn default_mask(channels: u16) -> u32 {
    match channels {
        1 => FC,
        2 => FL | FR,
        3 => FL | FR | FC,
        4 => FL | FR | BL | BR,
        5 => FL | FR | FC | BL | BR,
        6 => FL | FR | FC | LFE | BL | BR,
        7 => FL | FR | FC | LFE | BC | SL | SR,
        _ => FL | FR | FC | LFE | BL | BR | SL | SR,
    }
}

/// Description d'un flux PCM ou flottant entrelacé.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Format {
    pub sample: Sample,
    pub channels: u16,
    pub rate: u32,
    pub block: u16,
    pub bits: u16,
    pub valid_bits: u16,
    pub mask: u32,
    pub extensible: bool,
    /// Gains (gauche, droite) de chaque canal pour le repli en stéréo.
    downmix: [[f32; 2]; 8],
}

fn le16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn le32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

impl Format {
    /// Construit un format entier ou flottant ; `None` s'il n'est pas géré.
    pub fn new(sample: Sample, channels: u16, rate: u32) -> Option<Format> {
        let bits = match sample {
            Sample::U8 => 8,
            Sample::S16 => 16,
            Sample::S24 => 24,
            Sample::S32 | Sample::F32 => 32,
        };
        Self::build(
            sample,
            channels,
            rate,
            bits,
            bits,
            default_mask(channels),
            false,
        )
    }

    /// Format interne du mixeur : stéréo flottant 32 bits.
    pub fn mix(rate: u32) -> Format {
        let mut f = Self::new(Sample::F32, 2, rate).expect("format interne valide");
        f.extensible = true;
        f
    }

    fn build(
        sample: Sample,
        channels: u16,
        rate: u32,
        bits: u16,
        valid_bits: u16,
        mask: u32,
        extensible: bool,
    ) -> Option<Format> {
        if !(1..=8).contains(&channels) || !(1000..=384_000).contains(&rate) {
            return None;
        }
        let block = channels.checked_mul(bits / 8)?;
        let mask = if mask.count_ones() as u16 == channels {
            mask
        } else {
            default_mask(channels)
        };
        let mut downmix = [[0.0f32; 2]; 8];
        if channels == 1 {
            downmix[0] = [1.0, 1.0];
        } else {
            let mut ch = 0usize;
            for bit in 0..18u32 {
                let m = 1u32 << bit;
                if mask & m == 0 {
                    continue;
                }
                if ch >= 8 {
                    break;
                }
                const H: f32 = std::f32::consts::FRAC_1_SQRT_2;
                downmix[ch] = match m {
                    FL => [1.0, 0.0],
                    FR => [0.0, 1.0],
                    FC | BC => [H, H],
                    LFE => [0.0, 0.0],
                    BL | SL => [H, 0.0],
                    BR | SR => [0.0, H],
                    FLC => [0.85, 0.15],
                    FRC => [0.15, 0.85],
                    _ => [0.5, 0.5],
                };
                ch += 1;
            }
        }
        Some(Format {
            sample,
            channels,
            rate,
            block,
            bits,
            valid_bits,
            mask,
            extensible,
            downmix,
        })
    }

    /// Lit un WAVEFORMATEX (ou EXTENSIBLE) en mémoire du programme.
    ///
    /// # Safety
    /// `p` doit pointer sur une structure WAVEFORMATEX lisible.
    pub unsafe fn from_ptr(p: *const u8) -> Option<Format> {
        if p.is_null() {
            return None;
        }
        let head = std::slice::from_raw_parts(p, 18);
        let tag = le16(head, 0);
        let cb = le16(head, 16);
        if tag == WAVE_FORMAT_EXTENSIBLE {
            if cb < 22 {
                return None;
            }
            Self::parse(std::slice::from_raw_parts(p, 40))
        } else {
            Self::parse(head)
        }
    }

    /// Analyse les octets d'un WAVEFORMATEX (18 octets) ou EXTENSIBLE (40 octets).
    pub fn parse(b: &[u8]) -> Option<Format> {
        if b.len() < 16 {
            return None;
        }
        let tag = le16(b, 0);
        let channels = le16(b, 2);
        let rate = le32(b, 4);
        let block = le16(b, 12);
        let bits = le16(b, 14);
        let (is_float, valid, mask, ext) = match tag {
            WAVE_FORMAT_PCM => (false, bits, 0, false),
            WAVE_FORMAT_IEEE_FLOAT => (true, bits, 0, false),
            WAVE_FORMAT_EXTENSIBLE if b.len() >= 40 => {
                if b[28..40] != SUBTYPE_TAIL {
                    return None;
                }
                let sub = le32(b, 24);
                let valid = match le16(b, 18) {
                    0 => bits,
                    v => v,
                };
                match sub {
                    1 => (false, valid, le32(b, 20), true),
                    3 => (true, valid, le32(b, 20), true),
                    _ => return None,
                }
            }
            _ => return None,
        };
        let sample = match (is_float, bits) {
            (false, 8) => Sample::U8,
            (false, 16) => Sample::S16,
            (false, 24) => Sample::S24,
            (false, 32) => Sample::S32,
            (true, 32) => Sample::F32,
            _ => return None,
        };
        if valid == 0 || valid > bits {
            return None;
        }
        let f = Self::build(sample, channels, rate, bits, valid, mask, ext)?;
        if f.block != block {
            return None;
        }
        Some(f)
    }

    /// Octets d'un WAVEFORMATEXTENSIBLE (40) ou WAVEFORMATEX (18) décrivant ce format.
    pub fn to_bytes(self) -> Vec<u8> {
        let mut v = Vec::with_capacity(40);
        let tag = if self.extensible {
            WAVE_FORMAT_EXTENSIBLE
        } else if self.sample == Sample::F32 {
            WAVE_FORMAT_IEEE_FLOAT
        } else {
            WAVE_FORMAT_PCM
        };
        v.extend_from_slice(&tag.to_le_bytes());
        v.extend_from_slice(&self.channels.to_le_bytes());
        v.extend_from_slice(&self.rate.to_le_bytes());
        v.extend_from_slice(&(self.rate * self.block as u32).to_le_bytes());
        v.extend_from_slice(&self.block.to_le_bytes());
        v.extend_from_slice(&self.bits.to_le_bytes());
        if self.extensible {
            v.extend_from_slice(&22u16.to_le_bytes());
            v.extend_from_slice(&self.valid_bits.to_le_bytes());
            v.extend_from_slice(&self.mask.to_le_bytes());
            let sub: u32 = if self.sample == Sample::F32 { 3 } else { 1 };
            v.extend_from_slice(&sub.to_le_bytes());
            v.extend_from_slice(&SUBTYPE_TAIL);
        } else {
            v.extend_from_slice(&0u16.to_le_bytes());
        }
        v
    }

    pub fn bytes_per_sec(&self) -> u32 {
        self.rate * self.block as u32
    }

    /// Valeur du canal `ch` d'une trame, dans [-1, 1].
    #[inline]
    pub fn sample_at(&self, frame: &[u8], ch: usize) -> f32 {
        match self.sample {
            Sample::U8 => (frame[ch] as f32 - 128.0) / 128.0,
            Sample::S16 => {
                let o = ch * 2;
                i16::from_le_bytes([frame[o], frame[o + 1]]) as f32 / 32768.0
            }
            Sample::S24 => {
                let o = ch * 3;
                let v = i32::from_le_bytes([0, frame[o], frame[o + 1], frame[o + 2]]);
                v as f32 / 2_147_483_648.0
            }
            Sample::S32 => {
                let o = ch * 4;
                let v = i32::from_le_bytes([frame[o], frame[o + 1], frame[o + 2], frame[o + 3]]);
                v as f32 / 2_147_483_648.0
            }
            Sample::F32 => {
                let o = ch * 4;
                let v = f32::from_le_bytes([frame[o], frame[o + 1], frame[o + 2], frame[o + 3]]);
                if v.is_finite() {
                    v
                } else {
                    0.0
                }
            }
        }
    }

    /// Ramène une trame (de `block` octets) en stéréo.
    #[inline]
    pub fn stereo(&self, frame: &[u8]) -> [f32; 2] {
        if self.channels == 2 {
            return [self.sample_at(frame, 0), self.sample_at(frame, 1)];
        }
        let mut out = [0.0f32; 2];
        for ch in 0..self.channels as usize {
            let s = self.sample_at(frame, ch);
            out[0] += s * self.downmix[ch][0];
            out[1] += s * self.downmix[ch][1];
        }
        out
    }

    /// Écrit dans `out` (au moins `block` octets) une trame stéréo convertie.
    pub fn write_stereo(&self, out: &mut [u8], lr: [f32; 2]) {
        for ch in 0..self.channels as usize {
            let v = match (self.channels, ch) {
                (1, _) => (lr[0] + lr[1]) * 0.5,
                (_, 0) => lr[0],
                (_, 1) => lr[1],
                _ => 0.0,
            };
            self.write_sample(out, ch, v);
        }
    }

    fn write_sample(&self, out: &mut [u8], ch: usize, v: f32) {
        let v = v.clamp(-1.0, 1.0);
        match self.sample {
            Sample::U8 => out[ch] = (v * 127.0 + 128.0).round() as u8,
            Sample::S16 => {
                let s = (v * 32767.0).round() as i16;
                out[ch * 2..ch * 2 + 2].copy_from_slice(&s.to_le_bytes());
            }
            Sample::S24 => {
                let s = (v as f64 * 8_388_607.0).round() as i32;
                out[ch * 3..ch * 3 + 3].copy_from_slice(&s.to_le_bytes()[..3]);
            }
            Sample::S32 => {
                let s = (v as f64 * 2_147_483_647.0).round() as i32;
                out[ch * 4..ch * 4 + 4].copy_from_slice(&s.to_le_bytes());
            }
            Sample::F32 => out[ch * 4..ch * 4 + 4].copy_from_slice(&v.to_le_bytes()),
        }
    }

    /// Octet représentant le silence (0x80 en 8 bits non signé).
    pub fn silence_byte(&self) -> u8 {
        if self.sample == Sample::U8 {
            0x80
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyse_pcm_16_et_extensible_flottant() {
        let pcm = Format::new(Sample::S16, 2, 44100).unwrap();
        let bytes = pcm.to_bytes();
        assert_eq!(bytes.len(), 18);
        assert_eq!(Format::parse(&bytes), Some(pcm));
        let mix = Format::mix(48000);
        let ext = mix.to_bytes();
        assert_eq!(ext.len(), 40);
        let back = Format::parse(&ext).unwrap();
        assert_eq!(back.sample, Sample::F32);
        assert_eq!(back.channels, 2);
        assert_eq!(back.rate, 48000);
        assert!(back.extensible);
    }

    #[test]
    fn rejette_les_formats_incoherents() {
        let mut b = Format::new(Sample::S16, 2, 22050).unwrap().to_bytes();
        b[12] = 3; // nBlockAlign faux
        assert!(Format::parse(&b).is_none());
        let mut adpcm = Format::new(Sample::S16, 1, 22050).unwrap().to_bytes();
        adpcm[0] = 2; // MS-ADPCM
        assert!(Format::parse(&adpcm).is_none());
        assert!(Format::new(Sample::S16, 9, 44100).is_none());
    }

    #[test]
    fn conversion_des_echantillons_et_repli_stereo() {
        let u8m = Format::new(Sample::U8, 1, 8000).unwrap();
        assert_eq!(u8m.stereo(&[0x80]), [0.0, 0.0]);
        let s16 = Format::new(Sample::S16, 2, 8000).unwrap();
        let mut frame = [0u8; 4];
        s16.write_stereo(&mut frame, [1.0, -1.0]);
        let back = s16.stereo(&frame);
        assert!((back[0] - 1.0).abs() < 1e-3 && (back[1] + 1.0).abs() < 1e-3);
        let s24 = Format::new(Sample::S24, 1, 8000).unwrap();
        let mut f24 = [0u8; 3];
        s24.write_stereo(&mut f24, [0.5, 0.5]);
        assert!((s24.sample_at(&f24, 0) - 0.5).abs() < 1e-4);
        // 5.1 : le centre va des deux côtés, le LFE est ignoré.
        let six = Format::new(Sample::F32, 6, 48000).unwrap();
        let mut f = [0u8; 24];
        f[8..12].copy_from_slice(&1.0f32.to_le_bytes());
        f[12..16].copy_from_slice(&1.0f32.to_le_bytes());
        let lr = six.stereo(&f);
        assert!((lr[0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5);
        assert!((lr[0] - lr[1]).abs() < 1e-6);
    }
}
