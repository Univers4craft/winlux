//! Profils ICC d'affichage sRGB et scRGB construits en mémoire (#1354).
//!
//! Sources publiques uniquement :
//! - la spécification ICC.1:2022 (v4.4) : en-tête de 128 octets, table des
//!   balises, types `XYZ `, `para`, `mluc` et `sf32` ;
//! - l'IEC 61966-2-1 (sRGB) : primaires et point blanc D65 et fonction de
//!   transfert. Les colorants sont adaptés à D50 par la transformation de
//!   Bradford (valeurs publiées par B. Lindbloom).
//!
//! Aucun profil n'est copié d'un système existant.

/// Nombre s15Fixed16Number (ICC.1 §4.6), gros-boutiste.
fn s15f16(v: f64) -> [u8; 4] {
    ((v * 65536.0).round() as i32).to_be_bytes()
}

fn xyz_tag(x: f64, y: f64, z: f64) -> Vec<u8> {
    let mut t = b"XYZ \0\0\0\0".to_vec();
    for v in [x, y, z] {
        t.extend_from_slice(&s15f16(v));
    }
    t
}

/// Courbe paramétrique (ICC.1 §10.18). Fonction 3 :
/// Y = (aX + b)^g si X >= d, sinon cX.
fn para_tag(linear: bool) -> Vec<u8> {
    let mut t = b"para\0\0\0\0".to_vec();
    if linear {
        t.extend_from_slice(&0u16.to_be_bytes());
        t.extend_from_slice(&[0, 0]);
        t.extend_from_slice(&s15f16(1.0));
    } else {
        t.extend_from_slice(&3u16.to_be_bytes());
        t.extend_from_slice(&[0, 0]);
        for v in [2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045] {
            t.extend_from_slice(&s15f16(v));
        }
    }
    t
}

/// Texte multilingue (ICC.1 §10.15), une seule entrée en-US.
fn mluc_tag(text: &str) -> Vec<u8> {
    let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
    let mut t = b"mluc\0\0\0\0".to_vec();
    t.extend_from_slice(&1u32.to_be_bytes()); // nombre d'entrées
    t.extend_from_slice(&12u32.to_be_bytes()); // taille d'une entrée
    t.extend_from_slice(b"enUS");
    t.extend_from_slice(&(utf16.len() as u32).to_be_bytes());
    t.extend_from_slice(&28u32.to_be_bytes()); // décalage depuis le début de la balise
    t.extend_from_slice(&utf16);
    t
}

/// Matrice d'adaptation chromatique (ICC.1 annexe G) : Bradford D65 → D50.
fn chad_tag() -> Vec<u8> {
    let mut t = b"sf32\0\0\0\0".to_vec();
    for v in [
        1.047_811_2,
        0.022_886_6,
        -0.050_127_0,
        0.029_542_4,
        0.990_484_4,
        -0.017_049_1,
        -0.009_234_5,
        0.015_043_6,
        0.752_131_6,
    ] {
        t.extend_from_slice(&s15f16(v));
    }
    t
}

/// Profil d'affichage RVB à matrice : sRGB, ou scRGB (mêmes primaires,
/// transfert linéaire) si `linear`.
fn display_profile(linear: bool, description: &str) -> Vec<u8> {
    // Colorants sRGB adaptés à D50 (Bradford).
    let trc = para_tag(linear);
    let tags: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"desc", mluc_tag(description)),
        (*b"cprt", mluc_tag("Domaine public")),
        (*b"wtpt", xyz_tag(0.9642, 1.0, 0.8249)),
        (*b"chad", chad_tag()),
        (*b"rXYZ", xyz_tag(0.436_074_7, 0.222_504_5, 0.013_932_2)),
        (*b"gXYZ", xyz_tag(0.385_064_9, 0.716_878_6, 0.097_104_5)),
        (*b"bXYZ", xyz_tag(0.143_080_4, 0.060_616_9, 0.714_173_3)),
        (*b"rTRC", trc.clone()),
        (*b"gTRC", trc.clone()),
        (*b"bTRC", trc),
    ];
    // Les trois courbes partagent les mêmes données (ICC.1 §7.3.5).
    let table_len = 4 + 12 * tags.len();
    let mut data: Vec<u8> = Vec::new();
    let mut entries: Vec<([u8; 4], u32, u32)> = Vec::new();
    let mut trc_at: Option<(u32, u32)> = None;
    for (sig, body) in &tags {
        let is_trc = sig.ends_with(b"TRC");
        if let (true, Some((off, len))) = (is_trc, trc_at) {
            entries.push((*sig, off, len));
            continue;
        }
        while !data.len().is_multiple_of(4) {
            data.push(0);
        }
        let off = (128 + table_len + data.len()) as u32;
        let len = body.len() as u32;
        data.extend_from_slice(body);
        if is_trc {
            trc_at = Some((off, len));
        }
        entries.push((*sig, off, len));
    }
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    let size = 128 + table_len + data.len();
    let mut p = Vec::with_capacity(size);
    p.extend_from_slice(&(size as u32).to_be_bytes()); // 0 taille
    p.extend_from_slice(&[0; 4]); // 4 CMM préféré
    p.extend_from_slice(&[4, 0x40, 0, 0]); // 8 version 4.4
    p.extend_from_slice(b"mntr"); // 12 classe : affichage
    p.extend_from_slice(b"RGB "); // 16 espace des données
    p.extend_from_slice(b"XYZ "); // 20 espace de connexion
    for v in [2026u16, 1, 1, 0, 0, 0] {
        p.extend_from_slice(&v.to_be_bytes()); // 24 date
    }
    p.extend_from_slice(b"acsp"); // 36 signature
    p.extend_from_slice(&[0; 4]); // 40 plate-forme
    p.extend_from_slice(&[0; 4]); // 44 drapeaux
    p.extend_from_slice(&[0; 8]); // 48 fabricant, modèle
    p.extend_from_slice(&[0; 8]); // 56 attributs
    p.extend_from_slice(&0u32.to_be_bytes()); // 64 intention perceptuelle
    for v in [0.9642, 1.0, 0.8249] {
        p.extend_from_slice(&s15f16(v)); // 68 illuminant D50
    }
    p.extend_from_slice(&[0; 4]); // 80 créateur
    p.extend_from_slice(&[0; 16]); // 84 identifiant (non calculé)
    p.extend_from_slice(&[0; 28]); // 100 réservé
    debug_assert_eq!(p.len(), 128);
    p.extend_from_slice(&(entries.len() as u32).to_be_bytes());
    for (sig, off, len) in entries {
        p.extend_from_slice(&sig);
        p.extend_from_slice(&off.to_be_bytes());
        p.extend_from_slice(&len.to_be_bytes());
    }
    p.extend_from_slice(&data);
    p
}

/// Profil sRGB (IEC 61966-2-1).
pub fn srgb() -> Vec<u8> {
    display_profile(false, "sRGB IEC61966-2.1")
}

/// Profil scRGB : primaires sRGB, transfert linéaire.
pub fn scrgb() -> Vec<u8> {
    display_profile(true, "scRGB (sRGB linéaire)")
}

// --- Lecture d'un profil matrice + courbes (ICC.1 §F.3) -------------------------------

/// Courbe de transfert d'un canal : encodé → linéaire.
#[derive(Clone, Debug)]
pub enum Curve {
    /// `curv` à 0 entrée (identité) ou à 1 entrée (gamma u8Fixed8).
    Gamma(f32),
    /// `curv` échantillonnée, valeurs 0..1 réparties uniformément.
    Table(Vec<f32>),
    /// `para` (ICC.1 §10.18) : fonction et paramètres g, a, b, c, d, e, f.
    Para(u16, [f32; 7]),
}

impl Curve {
    /// Valeur linéaire d'une valeur encodée (0..1).
    pub fn eval(&self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        match self {
            Curve::Gamma(g) => x.powf(*g),
            Curve::Table(t) => {
                if t.len() < 2 {
                    return x;
                }
                let pos = x * (t.len() - 1) as f32;
                let i = (pos.floor() as usize).min(t.len() - 2);
                let f = pos - i as f32;
                t[i] + (t[i + 1] - t[i]) * f
            }
            Curve::Para(kind, p) => {
                let [g, a, b, c, d, e, f] = *p;
                let pw = |v: f32| if v > 0.0 { v.powf(g) } else { 0.0 };
                match kind {
                    0 => pw(x),
                    1 => {
                        if x >= -b / a {
                            pw(a * x + b)
                        } else {
                            0.0
                        }
                    }
                    2 => {
                        if x >= -b / a {
                            pw(a * x + b) + c
                        } else {
                            c
                        }
                    }
                    3 => {
                        if x >= d {
                            pw(a * x + b)
                        } else {
                            c * x
                        }
                    }
                    _ => {
                        if x >= d {
                            pw(a * x + b) + e
                        } else {
                            c * x + f
                        }
                    }
                }
            }
        }
    }

    /// Valeur encodée d'une valeur linéaire (inverse, par dichotomie : les
    /// courbes d'un profil d'affichage sont croissantes).
    pub fn invert(&self, y: f32) -> f32 {
        if let Curve::Gamma(g) = self {
            return if *g > 0.0 {
                y.clamp(0.0, 1.0).powf(1.0 / g)
            } else {
                y
            };
        }
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if self.eval(mid) < y {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        (lo + hi) / 2.0
    }
}

/// Profil RVB matrice + courbes : colorants (colonnes R, G, B vers XYZ D50)
/// et courbes des trois canaux.
#[derive(Clone, Debug)]
pub struct Shaper {
    pub matrix: [[f32; 3]; 3],
    pub curves: [Curve; 3],
}

fn be_u32(p: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(p.get(at..at + 4)?.try_into().ok()?))
}

fn be_s15f16(p: &[u8], at: usize) -> Option<f32> {
    Some(i32::from_be_bytes(p.get(at..at + 4)?.try_into().ok()?) as f32 / 65536.0)
}

fn tag<'a>(p: &'a [u8], sig: &[u8; 4]) -> Option<&'a [u8]> {
    let count = be_u32(p, 128)? as usize;
    for i in 0..count.min(1024) {
        let e = 132 + 12 * i;
        if p.get(e..e + 4)? == sig {
            let off = be_u32(p, e + 4)? as usize;
            let len = be_u32(p, e + 8)? as usize;
            return p.get(off..off.checked_add(len)?);
        }
    }
    None
}

fn read_curve(t: &[u8]) -> Option<Curve> {
    match t.get(0..4)? {
        b"curv" => {
            let n = be_u32(t, 8)? as usize;
            match n {
                0 => Some(Curve::Gamma(1.0)),
                1 => Some(Curve::Gamma(
                    u16::from_be_bytes(t.get(12..14)?.try_into().ok()?) as f32 / 256.0,
                )),
                _ => {
                    let mut v = Vec::with_capacity(n.min(65536));
                    for i in 0..n.min(65536) {
                        let at = 12 + 2 * i;
                        let x = u16::from_be_bytes(t.get(at..at + 2)?.try_into().ok()?);
                        v.push(x as f32 / 65535.0);
                    }
                    Some(Curve::Table(v))
                }
            }
        }
        b"para" => {
            let kind = u16::from_be_bytes(t.get(8..10)?.try_into().ok()?);
            let count = match kind {
                0 => 1,
                1 => 3,
                2 => 4,
                3 => 5,
                4 => 7,
                _ => return None,
            };
            let mut p = [0f32; 7];
            for (i, v) in p.iter_mut().enumerate().take(count) {
                *v = be_s15f16(t, 12 + 4 * i)?;
            }
            Some(Curve::Para(kind, p))
        }
        _ => None,
    }
}

fn read_xyz(t: &[u8]) -> Option<[f32; 3]> {
    if t.get(0..4)? != b"XYZ " {
        return None;
    }
    Some([be_s15f16(t, 8)?, be_s15f16(t, 12)?, be_s15f16(t, 16)?])
}

/// Lit un profil RVB matrice + courbes ; `None` pour un autre profil
/// (tables LUT, gris, CMJN…).
pub fn shaper(p: &[u8]) -> Option<Shaper> {
    if p.len() < 132 || p.get(36..40)? != b"acsp" || p.get(16..20)? != b"RGB " {
        return None;
    }
    let r = read_xyz(tag(p, b"rXYZ")?)?;
    let g = read_xyz(tag(p, b"gXYZ")?)?;
    let b = read_xyz(tag(p, b"bXYZ")?)?;
    let curves = [
        read_curve(tag(p, b"rTRC")?)?,
        read_curve(tag(p, b"gTRC")?)?,
        read_curve(tag(p, b"bTRC")?)?,
    ];
    Some(Shaper {
        matrix: [[r[0], g[0], b[0]], [r[1], g[1], b[1]], [r[2], g[2], b[2]]],
        curves,
    })
}

/// Adobe RGB (1998) : colorants adaptés à D50 et gamma 563/256, d'après la
/// spécification publique d'Adobe (EXIF ColorSpace 2).
pub fn adobe_rgb_shaper() -> Shaper {
    Shaper {
        matrix: [
            [0.6097, 0.2053, 0.1492],
            [0.3111, 0.6257, 0.0632],
            [0.0195, 0.0609, 0.7446],
        ],
        curves: [
            Curve::Gamma(563.0 / 256.0),
            Curve::Gamma(563.0 / 256.0),
            Curve::Gamma(563.0 / 256.0),
        ],
    }
}

fn invert3(m: &[[f32; 3]; 3]) -> Option<[[f32; 3]; 3]> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-9 {
        return None;
    }
    let c = |a: usize, b: usize, c: usize, d: usize| m[a][b] * m[c][d] - m[a][d] * m[c][b];
    Some([
        [
            c(1, 1, 2, 2) / det,
            -c(0, 1, 2, 2) / det,
            c(0, 1, 1, 2) / det,
        ],
        [
            -c(1, 0, 2, 2) / det,
            c(0, 0, 2, 2) / det,
            -c(0, 0, 1, 2) / det,
        ],
        [
            c(1, 0, 2, 1) / det,
            -c(0, 0, 2, 1) / det,
            c(0, 0, 1, 1) / det,
        ],
    ])
}

fn mul3(a: &[[f32; 3]; 3], b: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut o = [[0f32; 3]; 3];
    for (i, row) in o.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    o
}

/// Transformation d'un profil matrice + courbes vers un autre.
pub struct Transform {
    src: Shaper,
    dst: Shaper,
    m: [[f32; 3]; 3],
}

impl Transform {
    pub fn new(src: Shaper, dst: Shaper) -> Option<Transform> {
        let m = mul3(&invert3(&dst.matrix)?, &src.matrix);
        Some(Transform { src, dst, m })
    }

    /// Convertit une couleur RVB encodée (0..1) du profil source au profil
    /// de destination.
    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let lin = [
            self.src.curves[0].eval(rgb[0]),
            self.src.curves[1].eval(rgb[1]),
            self.src.curves[2].eval(rgb[2]),
        ];
        let mut out = [0f32; 3];
        for (i, o) in out.iter_mut().enumerate() {
            let v: f32 = (0..3).map(|k| self.m[i][k] * lin[k]).sum();
            *o = self.dst.curves[i].invert(v.clamp(0.0, 1.0));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn be32(p: &[u8], at: usize) -> u32 {
        u32::from_be_bytes(p[at..at + 4].try_into().unwrap())
    }

    /// #1354 : Paint.NET refuse un profil de moins de 128 octets ; celui
    /// rendu doit être cohérent (taille, signature, balises dans le profil).
    #[test]
    fn profils_srgb_et_scrgb_complets() {
        for p in [srgb(), scrgb()] {
            assert!(p.len() > 128);
            assert_eq!(be32(&p, 0) as usize, p.len());
            assert_eq!(&p[36..40], b"acsp");
            assert_eq!(&p[12..20], b"mntrRGB ");
            let count = be32(&p, 128) as usize;
            assert_eq!(count, 10);
            for i in 0..count {
                let e = 132 + 12 * i;
                let (off, len) = (be32(&p, e + 4) as usize, be32(&p, e + 8) as usize);
                assert_eq!(off % 4, 0);
                assert!(off + len <= p.len());
            }
        }
        assert_ne!(srgb(), scrgb());
    }

    /// #1384 : les profils construits se relisent, et sRGB → scRGB linéarise.
    #[test]
    fn transformation_srgb_vers_scrgb() {
        let s = shaper(&srgb()).expect("sRGB matrice + courbes");
        let l = shaper(&scrgb()).expect("scRGB matrice + courbes");
        let t = Transform::new(s.clone(), l).unwrap();
        let o = t.apply([0.5, 0.5, 0.5]);
        for v in o {
            assert!((v - 0.214).abs() < 0.002, "{v}");
        }
        let id = Transform::new(s.clone(), s).unwrap();
        let o = id.apply([0.2, 0.6, 0.9]);
        for (a, b) in o.iter().zip([0.2, 0.6, 0.9]) {
            assert!((a - b).abs() < 0.002, "{a} {b}");
        }
        assert!(shaper(&[0u8; 64]).is_none());
    }
}
