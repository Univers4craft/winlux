//! Condensat calculé au fil des appels (BCryptHashData, CryptHashData).
//!
//! Learn (BCryptHashData) : appeler la fonction plusieurs fois « is
//! functionally equivalent to concatenating these buffers into a single
//! buffer » ; CryptHashData « can be called multiple times to compute the
//! hash of long or discontinuous data streams ». L'état tient donc en
//! quelques octets, quelle que soit la quantité de données : garder tous les
//! octets reçus jusqu'à la fin faisait grossir un Vec par doublements, et
//! l'Office Deployment Tool, qui condense un fichier de 2,9 Go après l'avoir
//! téléchargé, mourait sur « memory allocation of 1073741824 bytes failed »
//! (#2738).
//!
//! SHA-1, SHA-256 et MD5 sont maison (comme les calculs d'un seul tenant de
//! bcrypt) ; SHA-384 et SHA-512 passent par l'OpenSSL de l'hôte. Le HMAC
//! (RFC 2104) est construit sur le condensat interne.

use crate::dlls::bcrypt::crypto::{sha1_compress, sha256_compress, SHA1_IV, SHA256_IV};
use crate::dlls::games_win32::{md5_compress, MD5_IV};

/// Algorithmes, numérotés comme dans bcrypt (2 SHA256, 3 SHA1, 4 MD5,
/// 5 SHA384, 6 SHA512).
const SHA256: usize = 2;
const SHA1: usize = 3;
const MD5: usize = 4;
const SHA384: usize = 5;
const SHA512: usize = 6;

/// Moteur par blocs de 64 octets (SHA-1, SHA-256, MD5).
#[derive(Clone)]
struct Blocs<const N: usize> {
    h: [u32; N],
    reste: Vec<u8>,
    total: u64,
    compresse: fn(&mut [u32; N], &[u8; 64]),
    gros_boutiste: bool,
}

impl<const N: usize> Blocs<N> {
    fn new(iv: [u32; N], compresse: fn(&mut [u32; N], &[u8; 64]), gros_boutiste: bool) -> Self {
        Blocs {
            h: iv,
            reste: Vec::with_capacity(64),
            total: 0,
            compresse,
            gros_boutiste,
        }
    }

    fn update(&mut self, mut data: &[u8]) {
        self.total = self.total.wrapping_add(data.len() as u64);
        if !self.reste.is_empty() {
            let n = (64 - self.reste.len()).min(data.len());
            self.reste.extend_from_slice(&data[..n]);
            data = &data[n..];
            if self.reste.len() < 64 {
                return;
            }
            let bloc: [u8; 64] = self.reste[..].try_into().unwrap();
            (self.compresse)(&mut self.h, &bloc);
            self.reste.clear();
        }
        let (blocs, fin) = data.as_chunks::<64>();
        for b in blocs {
            (self.compresse)(&mut self.h, b);
        }
        self.reste.extend_from_slice(fin);
    }

    fn finish(mut self) -> Vec<u8> {
        let bits = self.total.wrapping_mul(8);
        let mut pad = vec![0x80u8];
        while (self.reste.len() + pad.len()) % 64 != 56 {
            pad.push(0);
        }
        pad.extend_from_slice(&if self.gros_boutiste {
            bits.to_be_bytes()
        } else {
            bits.to_le_bytes()
        });
        let total = self.total;
        self.update(&pad);
        self.total = total;
        debug_assert!(self.reste.is_empty());
        self.h
            .iter()
            .flat_map(|v| {
                if self.gros_boutiste {
                    v.to_be_bytes()
                } else {
                    v.to_le_bytes()
                }
            })
            .collect()
    }
}

enum Moteur {
    Sha1(Blocs<5>),
    Sha256(Blocs<8>),
    Md5(Blocs<4>),
    Hote(crate::dlls::hostssl::Hasher),
}

impl Moteur {
    fn new(alg: usize) -> Option<Moteur> {
        Some(match alg {
            SHA1 => Moteur::Sha1(Blocs::new(SHA1_IV, sha1_compress, true)),
            SHA256 => Moteur::Sha256(Blocs::new(SHA256_IV, sha256_compress, true)),
            MD5 => Moteur::Md5(Blocs::new(MD5_IV, md5_compress, false)),
            SHA384 => Moteur::Hote(crate::dlls::hostssl::Hasher::new(c"SHA384")?),
            SHA512 => Moteur::Hote(crate::dlls::hostssl::Hasher::new(c"SHA512")?),
            _ => return None,
        })
    }

    fn try_clone(&self) -> Option<Moteur> {
        Some(match self {
            Moteur::Sha1(b) => Moteur::Sha1(b.clone()),
            Moteur::Sha256(b) => Moteur::Sha256(b.clone()),
            Moteur::Md5(b) => Moteur::Md5(b.clone()),
            Moteur::Hote(h) => Moteur::Hote(h.try_clone()?),
        })
    }

    fn update(&mut self, data: &[u8]) {
        match self {
            Moteur::Sha1(b) => b.update(data),
            Moteur::Sha256(b) => b.update(data),
            Moteur::Md5(b) => b.update(data),
            Moteur::Hote(h) => h.update(data),
        }
    }

    fn finish(self) -> Vec<u8> {
        match self {
            Moteur::Sha1(b) => b.finish(),
            Moteur::Sha256(b) => b.finish(),
            Moteur::Md5(b) => b.finish(),
            Moteur::Hote(h) => h.finish(),
        }
    }
}

/// Condensat en cours : algorithme `alg` (numérotation bcrypt), HMAC si une
/// clé est donnée.
pub struct Condensat {
    alg: usize,
    interne: Moteur,
    /// HMAC : clé complétée au bloc, combinée à 0x5c à la fin.
    externe: Option<Vec<u8>>,
}

impl Condensat {
    pub fn new(alg: usize, cle_hmac: Option<&[u8]>) -> Option<Condensat> {
        let mut interne = Moteur::new(alg)?;
        let externe = match cle_hmac {
            None => None,
            Some(cle) => {
                let bloc = if matches!(alg, SHA384 | SHA512) {
                    128
                } else {
                    64
                };
                let mut k = if cle.len() > bloc {
                    let mut m = Moteur::new(alg)?;
                    m.update(cle);
                    m.finish()
                } else {
                    cle.to_vec()
                };
                k.resize(bloc, 0);
                interne.update(&k.iter().map(|b| b ^ 0x36).collect::<Vec<u8>>());
                Some(k.iter().map(|b| b ^ 0x5c).collect())
            }
        };
        Some(Condensat {
            alg,
            interne,
            externe,
        })
    }

    pub fn update(&mut self, data: &[u8]) {
        self.interne.update(data);
    }

    /// Copie indépendante (BCryptDuplicateHash, CryptDuplicateHash, ou
    /// lecture de HP_HASHVAL sans clore le condensat).
    pub fn try_clone(&self) -> Option<Condensat> {
        Some(Condensat {
            alg: self.alg,
            interne: self.interne.try_clone()?,
            externe: self.externe.clone(),
        })
    }

    pub fn finish(self) -> Option<Vec<u8>> {
        let d = self.interne.finish();
        if d.is_empty() {
            return None;
        }
        match self.externe {
            None => Some(d),
            Some(k) => {
                let mut m = Moteur::new(self.alg)?;
                m.update(&k);
                m.update(&d);
                Some(m.finish()).filter(|v| !v.is_empty())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(v: &[u8]) -> String {
        v.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn en_morceaux(alg: usize, cle: Option<&[u8]>, data: &[u8], pas: usize) -> String {
        let mut c = Condensat::new(alg, cle).unwrap();
        for m in data.chunks(pas.max(1)) {
            c.update(m);
        }
        hex(&c.finish().unwrap())
    }

    /// Le résultat ne dépend pas du découpage, et égale le calcul d'un seul
    /// tenant (vecteurs FIPS 180 et RFC 1321).
    #[test]
    fn condensat_au_fil_de_l_eau_2738() {
        let abc = b"abc";
        assert_eq!(
            en_morceaux(SHA256, None, abc, 1),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            en_morceaux(SHA1, None, abc, 2),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            en_morceaux(MD5, None, abc, 3),
            "900150983cd24fb0d6963f7d28e17f72"
        );
        let long: Vec<u8> = (0..10_000u32).map(|i| (i * 7 + 3) as u8).collect();
        for pas in [1, 63, 64, 65, 1000, 10_000] {
            assert_eq!(
                en_morceaux(SHA256, None, &long, pas),
                hex(&crate::dlls::bcrypt::crypto::sha256(&[&long]))
            );
            assert_eq!(
                en_morceaux(SHA1, None, &long, pas),
                hex(&crate::dlls::bcrypt::crypto::sha1(&[&long]))
            );
            assert_eq!(
                en_morceaux(MD5, None, &long, pas),
                hex(&crate::dlls::games_win32::md5(&long))
            );
        }
        // HMAC-SHA256, RFC 4231 cas 2.
        assert_eq!(
            en_morceaux(SHA256, Some(b"Jefe"), b"what do ya want for nothing?", 5),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        // L'état reste de taille fixe : rien n'est gardé au-delà d'un bloc.
        let mut gros = Condensat::new(SHA256, None).unwrap();
        let mo = vec![0x5au8; 1 << 20];
        for _ in 0..8 {
            gros.update(&mo[..(1 << 20) - 3]);
        }
        match &gros.interne {
            Moteur::Sha256(b) => assert!(b.reste.len() < 64 && b.reste.capacity() <= 64),
            _ => unreachable!(),
        }
        // Copie : les deux suites continuent séparément.
        let mut a = Condensat::new(SHA256, None).unwrap();
        a.update(b"a");
        let mut b = a.try_clone().unwrap();
        a.update(b"bc");
        b.update(b"x");
        assert_eq!(
            hex(&a.finish().unwrap()),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_ne!(
            hex(&b.finish().unwrap()),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
