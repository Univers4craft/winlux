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

/// CRC-32 incrémental.
#[derive(Clone, Copy, Default)]
pub struct Crc32(u32);

impl Crc32 {
    pub fn new() -> Self {
        Crc32(0)
    }
    pub fn update(&mut self, data: &[u8]) {
        let mut c = !self.0;
        for &b in data {
            c = TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
        }
        self.0 = !c;
    }
    pub fn value(&self) -> u32 {
        self.0
    }
}

#[cfg(test)]
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
}
