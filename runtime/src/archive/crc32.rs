//! CRC-32 (polynôme IEEE 802.3, réfléchi), utilisé par ZIP et gzip.

const fn table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

pub static TABLE: [u32; 256] = table();

/// Tables du découpage par 16 octets : `SLICES[k][b]` est le CRC de l'octet
/// `b` suivi de `k` octets nuls. Méthode publique « slicing-by-N » (Kounavis
/// et Berry, Intel, 2005) ; mesuré à 304 Mo/s octet par octet (#3328).
const fn slices() -> [[u32; 256]; 16] {
    let mut t = [[0u32; 256]; 16];
    t[0] = table();
    let mut k = 1;
    while k < 16 {
        let mut i = 0;
        while i < 256 {
            let p = t[k - 1][i];
            t[k][i] = (p >> 8) ^ t[0][(p & 0xFF) as usize];
            i += 1;
        }
        k += 1;
    }
    t
}

static SLICES: [[u32; 256]; 16] = slices();

/// CRC-32 incrémental.
#[derive(Clone, Copy, Default)]
pub struct Crc32(u32);

impl Crc32 {
    pub fn new() -> Self {
        Crc32(0)
    }
    pub fn update(&mut self, data: &[u8]) {
        let t = &SLICES;
        let mut c = !self.0;
        let mut blocks = data.chunks_exact(16);
        for b in &mut blocks {
            let w = c ^ u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            c = t[15][(w & 0xFF) as usize]
                ^ t[14][((w >> 8) & 0xFF) as usize]
                ^ t[13][((w >> 16) & 0xFF) as usize]
                ^ t[12][(w >> 24) as usize]
                ^ t[11][b[4] as usize]
                ^ t[10][b[5] as usize]
                ^ t[9][b[6] as usize]
                ^ t[8][b[7] as usize]
                ^ t[7][b[8] as usize]
                ^ t[6][b[9] as usize]
                ^ t[5][b[10] as usize]
                ^ t[4][b[11] as usize]
                ^ t[3][b[12] as usize]
                ^ t[2][b[13] as usize]
                ^ t[1][b[14] as usize]
                ^ t[0][b[15] as usize];
        }
        for &b in blocks.remainder() {
            c = TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
        }
        self.0 = !c;
    }
    pub fn value(&self) -> u32 {
        self.0
    }
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut c = Crc32::new();
    c.update(data);
    c.value()
}

/// Mise à jour d'un octet, telle qu'utilisée par le chiffrement ZipCrypto.
pub fn crc32_byte(crc: u32, b: u8) -> u32 {
    TABLE[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        let mut c = Crc32::new();
        c.update(b"1234");
        c.update(b"56789");
        assert_eq!(c.value(), 0xCBF4_3926);
    }

    /// #3328 : le découpage par 16 donne le même CRC que l'octet par octet,
    /// pour toute longueur et tout découpage des appels.
    #[test]
    fn decoupage_identique_octet_par_octet() {
        let lent = |d: &[u8]| !d.iter().fold(!0u32, |c, &b| crc32_byte(c, b));
        let mut x = 0x9E37_79B9u32;
        let data: Vec<u8> = (0..4099)
            .map(|_| {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (x >> 24) as u8
            })
            .collect();
        for n in (0..64).chain([1000, 4096, 4099]) {
            assert_eq!(crc32(&data[..n]), lent(&data[..n]), "longueur {n}");
        }
        for cut in [1, 7, 15, 16, 17, 333] {
            let mut c = Crc32::new();
            for part in data.chunks(cut) {
                c.update(part);
            }
            assert_eq!(c.value(), lent(&data), "appels de {cut}");
        }
        let big = vec![0xA5u8; 64 << 20];
        let t = std::time::Instant::now();
        let v = crc32(std::hint::black_box(&big));
        let s = t.elapsed().as_secs_f64();
        eprintln!("crc32 : {:.0} Mo/s ({v:08x})", 64.0 / s);
    }
}
