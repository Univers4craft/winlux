//! Formats de pixels WIC à plus de 8 bits par canal (#1384) : entiers 16
//! bits, demi-flottants et flottants 32 bits, droits ou prémultipliés.
//!
//! GUID et disposition des canaux : `wincodec.h` des en-têtes MinGW-w64 et
//! la page publique « Native Pixel Formats » de WIC. Comme le décrit cette
//! page, les formats flottants portent des valeurs linéaires (scRGB) ; les
//! formats entiers portent des valeurs encodées sRGB.

use crate::dlls::ole32::Guid;

const fn wic(last: u8) -> Guid {
    crate::dlls::audio_com::guid(
        0x6fddc324,
        0x4e03,
        0x4bfe,
        [0xb1, 0x85, 0x3d, 0x77, 0x76, 0x8d, 0xc9, last],
    )
}
const fn g(a: u32, b: u16, c: u16, d: [u8; 8]) -> Guid {
    crate::dlls::audio_com::guid(a, b, c, d)
}

pub const FMT_GRAY16: Guid = wic(0x0b);
pub const FMT_GRAY_HALF16: Guid = wic(0x3e);
pub const FMT_GRAY_FLOAT32: Guid = wic(0x11);
pub const FMT_RGB32: Guid = g(
    0xd98c6b95,
    0x3efe,
    0x47d6,
    [0xbb, 0x25, 0xeb, 0x17, 0x48, 0xab, 0x0c, 0xf1],
);
pub const FMT_RGB48: Guid = wic(0x15);
pub const FMT_BGR48: Guid = g(
    0xe605a384,
    0xb468,
    0x46ce,
    [0xbb, 0x2e, 0x36, 0xf1, 0x80, 0xe6, 0x43, 0x13],
);
pub const FMT_RGB64: Guid = g(
    0xa1182111,
    0x186d,
    0x4d42,
    [0xbc, 0x6a, 0x9c, 0x83, 0x03, 0xa8, 0xdf, 0xf9],
);
pub const FMT_RGBA64: Guid = wic(0x16);
pub const FMT_BGRA64: Guid = g(
    0x1562ff7c,
    0xd352,
    0x46f9,
    [0x97, 0x9e, 0x42, 0x97, 0x6b, 0x79, 0x22, 0x46],
);
pub const FMT_PRGBA64: Guid = wic(0x17);
pub const FMT_PBGRA64: Guid = g(
    0x8c518e8e,
    0xa4ec,
    0x468b,
    [0xae, 0x70, 0xc9, 0xa3, 0x5a, 0x9c, 0x55, 0x30],
);
pub const FMT_RGB_HALF48: Guid = wic(0x3b);
pub const FMT_RGB_HALF64: Guid = wic(0x42);
pub const FMT_RGBA_HALF64: Guid = wic(0x3a);
pub const FMT_PRGBA_HALF64: Guid = g(
    0x58ad26c2,
    0xc623,
    0x4d9d,
    [0xb3, 0x20, 0x38, 0x7e, 0x49, 0xf8, 0xc4, 0x42],
);
pub const FMT_RGB_FLOAT96: Guid = g(
    0xe3fed78f,
    0xe8db,
    0x4acf,
    [0x84, 0xc1, 0xe9, 0x7f, 0x61, 0x36, 0xb3, 0x27],
);
pub const FMT_RGB_FLOAT128: Guid = wic(0x1b);
pub const FMT_RGBA_FLOAT128: Guid = wic(0x19);
pub const FMT_PRGBA_FLOAT128: Guid = wic(0x1a);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sample {
    U16,
    F16,
    F32,
}

/// Disposition d'un format : octets par pixel, type d'échantillon, indice
/// (en échantillons) de R, G, B et A (`None` : pas d'alpha ; gris : R = G = B).
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub bpp: u32,
    pub sample: Sample,
    pub rgb: [usize; 3],
    pub alpha: Option<usize>,
    pub premultiplied: bool,
}

impl Layout {
    /// Valeurs linéaires (formats flottants, scRGB).
    pub fn linear(&self) -> bool {
        self.sample != Sample::U16
    }
}

pub fn layout(fmt: &Guid) -> Option<Layout> {
    use Sample::*;
    let l = |bpp, sample, rgb, alpha, premultiplied| {
        Some(Layout {
            bpp,
            sample,
            rgb,
            alpha,
            premultiplied,
        })
    };
    const RGB: [usize; 3] = [0, 1, 2];
    const BGR: [usize; 3] = [2, 1, 0];
    const GRAY: [usize; 3] = [0, 0, 0];
    match *fmt {
        FMT_GRAY16 => l(2, U16, GRAY, None, false),
        FMT_GRAY_HALF16 => l(2, F16, GRAY, None, false),
        FMT_GRAY_FLOAT32 => l(4, F32, GRAY, None, false),
        FMT_RGB48 => l(6, U16, RGB, None, false),
        FMT_BGR48 => l(6, U16, BGR, None, false),
        FMT_RGB64 => l(8, U16, RGB, None, false),
        FMT_RGBA64 => l(8, U16, RGB, Some(3), false),
        FMT_BGRA64 => l(8, U16, BGR, Some(3), false),
        FMT_PRGBA64 => l(8, U16, RGB, Some(3), true),
        FMT_PBGRA64 => l(8, U16, BGR, Some(3), true),
        FMT_RGB_HALF48 => l(6, F16, RGB, None, false),
        FMT_RGB_HALF64 => l(8, F16, RGB, None, false),
        FMT_RGBA_HALF64 => l(8, F16, RGB, Some(3), false),
        FMT_PRGBA_HALF64 => l(8, F16, RGB, Some(3), true),
        FMT_RGB_FLOAT96 => l(12, F32, RGB, None, false),
        FMT_RGB_FLOAT128 => l(16, F32, RGB, None, false),
        FMT_RGBA_FLOAT128 => l(16, F32, RGB, Some(3), false),
        FMT_PRGBA_FLOAT128 => l(16, F32, RGB, Some(3), true),
        _ => None,
    }
}

/// Demi-flottant IEEE 754 binary16 → f32.
pub fn half_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1f) as i32;
    let man = (h & 0x3ff) as f32;
    sign * match exp {
        0 => man * 2f32.powi(-24),
        31 => {
            if man == 0.0 {
                f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => (1.0 + man / 1024.0) * 2f32.powi(exp - 15),
    }
}

/// f32 → demi-flottant (arrondi au plus proche, saturé à l'infini).
pub fn f32_to_half(v: f32) -> u16 {
    if v.is_nan() {
        return 0x7e00;
    }
    let sign = if v.is_sign_negative() { 0x8000 } else { 0 };
    let a = v.abs();
    if a >= 65520.0 {
        return sign | 0x7c00;
    }
    if a < 2f32.powi(-14) {
        return sign | (a * 2f32.powi(24)).round() as u16;
    }
    let exp = a.log2().floor() as i32;
    let mut man = ((a / 2f32.powi(exp) - 1.0) * 1024.0).round() as u32;
    let mut e = exp + 15;
    if man == 1024 {
        man = 0;
        e += 1;
    }
    sign | ((e as u16) << 10) | man as u16
}

/// sRGB encodé → linéaire (IEC 61966-2-1).
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linéaire → sRGB encodé.
pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn read_sample(p: &[u8], s: Sample, i: usize) -> f32 {
    match s {
        Sample::U16 => u16::from_le_bytes([p[2 * i], p[2 * i + 1]]) as f32 / 65535.0,
        Sample::F16 => half_to_f32(u16::from_le_bytes([p[2 * i], p[2 * i + 1]])),
        Sample::F32 => f32::from_le_bytes(p[4 * i..4 * i + 4].try_into().unwrap()),
    }
}

fn write_sample(p: &mut [u8], s: Sample, i: usize, v: f32) {
    match s {
        Sample::U16 => p[2 * i..2 * i + 2]
            .copy_from_slice(&((v.clamp(0.0, 1.0) * 65535.0).round() as u16).to_le_bytes()),
        Sample::F16 => p[2 * i..2 * i + 2].copy_from_slice(&f32_to_half(v).to_le_bytes()),
        Sample::F32 => p[4 * i..4 * i + 4].copy_from_slice(&v.to_le_bytes()),
    }
}

/// Pixel en RVBA droit, valeurs telles que stockées (sans changer de
/// courbe), alpha 0..1.
pub fn read(l: &Layout, p: &[u8]) -> [f32; 4] {
    let a = l.alpha.map_or(1.0, |i| read_sample(p, l.sample, i));
    let mut c = [
        read_sample(p, l.sample, l.rgb[0]),
        read_sample(p, l.sample, l.rgb[1]),
        read_sample(p, l.sample, l.rgb[2]),
        a,
    ];
    if l.premultiplied {
        for v in &mut c[..3] {
            *v = if a > 0.0 { *v / a } else { 0.0 };
        }
    }
    c
}

/// Écrit un pixel RVBA droit (valeurs dans la courbe du format).
pub fn write(l: &Layout, p: &mut [u8], c: [f32; 4]) {
    let a = c[3];
    let m = if l.premultiplied { a } else { 1.0 };
    if l.rgb == [0, 0, 0] {
        // Gris : luminance Rec. 709 (valeurs linéaires ou encodées).
        let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        write_sample(p, l.sample, 0, y * m);
    } else {
        for (&i, &v) in l.rgb.iter().zip(&c[..3]) {
            write_sample(p, l.sample, i, v * m);
        }
    }
    if let Some(i) = l.alpha {
        write_sample(p, l.sample, i, a);
    } else if l.bpp as usize > 3 * sample_size(l.sample) {
        // Canal de remplissage (64bppRGB, 128bppRGBFloat) : 1.
        write_sample(p, l.sample, 3, 1.0);
    }
}

fn sample_size(s: Sample) -> usize {
    match s {
        Sample::U16 | Sample::F16 => 2,
        Sample::F32 => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demi_flottants_aller_retour() {
        for v in [0.0f32, 1.0, 0.5, 0.214, 2.0, -1.0, 65504.0, 1e-5] {
            let back = half_to_f32(f32_to_half(v));
            assert!((back - v).abs() <= v.abs() * 1e-3 + 1e-7, "{v} {back}");
        }
        assert_eq!(f32_to_half(1.0), 0x3c00);
        assert_eq!(half_to_f32(0x3800), 0.5);
    }

    #[test]
    fn pixel_flottant_premultiplie() {
        let l = layout(&FMT_PRGBA_FLOAT128).unwrap();
        assert_eq!(l.bpp, 16);
        let mut p = [0u8; 16];
        write(&l, &mut p, [0.5, 0.25, 1.0, 0.5]);
        assert_eq!(f32::from_le_bytes(p[0..4].try_into().unwrap()), 0.25);
        let c = read(&l, &p);
        assert_eq!(c, [0.5, 0.25, 1.0, 0.5]);
    }
}
