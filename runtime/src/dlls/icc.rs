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
}
