//! Cryptographie du bureau à distance, écrite sans bibliothèque externe :
//! SHA-1, SHA-256, AES-128/256, mode EAX, DES (authentification VNC
//! historique) et RSA (génération de clé, PKCS #1 v1.5).
//!
//! Ces primitives servent uniquement au protocole RFB « RSA-AES » (RA2 et
//! RA2_256, compatibles TigerVNC 1.13+) et à l'ancien défi VNC. Elles sont
//! vérifiées par les vecteurs officiels (FIPS 180, FIPS 197, article EAX,
//! FIPS 46) dans les tests ci-dessous.

use std::cmp::Ordering;
use std::io::Read;

// ---------------------------------------------------------------------------
// Aléa système
// ---------------------------------------------------------------------------

/// Octets aléatoires du noyau (getrandom), avec repli sur /dev/urandom.
pub fn random_bytes(out: &mut [u8]) {
    let mut filled = 0;
    while filled < out.len() {
        let n =
            unsafe { libc::getrandom(out[filled..].as_mut_ptr().cast(), out.len() - filled, 0) };
        if n <= 0 {
            break;
        }
        filled += n as usize;
    }
    if filled < out.len() {
        let mut f = std::fs::File::open("/dev/urandom").expect("/dev/urandom indisponible");
        f.read_exact(&mut out[filled..])
            .expect("lecture de /dev/urandom");
    }
}

// ---------------------------------------------------------------------------
// SHA-1 et SHA-256
// ---------------------------------------------------------------------------

fn md_pad(data_len: u64, buf: &mut Vec<u8>) {
    buf.push(0x80);
    while buf.len() % 64 != 56 {
        buf.push(0);
    }
    buf.extend_from_slice(&(data_len * 8).to_be_bytes());
}

pub fn sha1(parts: &[&[u8]]) -> [u8; 20] {
    let mut msg: Vec<u8> = parts.concat();
    let len = msg.len() as u64;
    md_pad(len, &mut msg);
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    for block in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

pub fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut msg: Vec<u8> = parts.concat();
    let len = msg.len() as u64;
    md_pad(len, &mut msg);
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    for block in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let [a, b, c, d, e, f, g, hh] = v;
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), a, b, c, d.wrapping_add(t1), e, f, g];
        }
        for (x, y) in h.iter_mut().zip(v) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 32];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

// ---------------------------------------------------------------------------
// AES (chiffrement seul : EAX n'utilise jamais le déchiffrement par bloc)
// ---------------------------------------------------------------------------

const fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0u8;
    while b != 0 {
        if b & 1 != 0 {
            p ^= a;
        }
        let hi = a & 0x80;
        a <<= 1;
        if hi != 0 {
            a ^= 0x1b;
        }
        b >>= 1;
    }
    p
}

const fn make_sbox() -> [u8; 256] {
    let mut s = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        // Inverse multiplicatif : a^254 dans GF(2^8).
        let a = i as u8;
        let mut inv = 1u8;
        let mut base = a;
        let mut e = 254u32;
        while e > 0 {
            if e & 1 != 0 {
                inv = gf_mul(inv, base);
            }
            base = gf_mul(base, base);
            e >>= 1;
        }
        if i == 0 {
            inv = 0;
        }
        let x = inv;
        s[i] = x ^ x.rotate_left(1) ^ x.rotate_left(2) ^ x.rotate_left(3) ^ x.rotate_left(4) ^ 0x63;
        i += 1;
    }
    s
}

const SBOX: [u8; 256] = make_sbox();

const fn make_te() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let s = SBOX[i];
        t[i] = u32::from_be_bytes([gf_mul(s, 2), s, s, gf_mul(s, 3)]);
        i += 1;
    }
    t
}

const TE0: [u32; 256] = make_te();

#[derive(Clone)]
pub struct Aes {
    round_keys: Vec<u32>,
    rounds: usize,
}

impl Aes {
    pub fn new(key: &[u8]) -> Aes {
        let nk = key.len() / 4;
        assert!(nk == 4 || nk == 8, "clé AES de 128 ou 256 bits");
        let rounds = nk + 6;
        let total = 4 * (rounds + 1);
        let mut w = vec![0u32; total];
        for i in 0..nk {
            w[i] = u32::from_be_bytes(key[i * 4..i * 4 + 4].try_into().unwrap());
        }
        let mut rcon = 1u8;
        for i in nk..total {
            let mut t = w[i - 1];
            if i % nk == 0 {
                t = sub_word(t.rotate_left(8)) ^ ((rcon as u32) << 24);
                rcon = gf_mul(rcon, 2);
            } else if nk > 6 && i % nk == 4 {
                t = sub_word(t);
            }
            w[i] = w[i - nk] ^ t;
        }
        Aes {
            round_keys: w,
            rounds,
        }
    }

    pub fn encrypt_block(&self, block: &mut [u8; 16]) {
        let k = &self.round_keys;
        let mut s = [0u32; 4];
        for (i, v) in s.iter_mut().enumerate() {
            *v = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap()) ^ k[i];
        }
        for round in 1..self.rounds {
            let mut t = [0u32; 4];
            for (i, v) in t.iter_mut().enumerate() {
                *v = TE0[(s[i] >> 24) as usize]
                    ^ TE0[((s[(i + 1) % 4] >> 16) & 0xff) as usize].rotate_right(8)
                    ^ TE0[((s[(i + 2) % 4] >> 8) & 0xff) as usize].rotate_right(16)
                    ^ TE0[(s[(i + 3) % 4] & 0xff) as usize].rotate_right(24)
                    ^ k[round * 4 + i];
            }
            s = t;
        }
        let last = self.rounds * 4;
        for i in 0..4 {
            let v = u32::from_be_bytes([
                SBOX[(s[i] >> 24) as usize],
                SBOX[((s[(i + 1) % 4] >> 16) & 0xff) as usize],
                SBOX[((s[(i + 2) % 4] >> 8) & 0xff) as usize],
                SBOX[(s[(i + 3) % 4] & 0xff) as usize],
            ]) ^ k[last + i];
            block[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
        }
    }
}

fn sub_word(w: u32) -> u32 {
    let b = w.to_be_bytes();
    u32::from_be_bytes([
        SBOX[b[0] as usize],
        SBOX[b[1] as usize],
        SBOX[b[2] as usize],
        SBOX[b[3] as usize],
    ])
}

// ---------------------------------------------------------------------------
// EAX (Bellare, Rogaway, Wagner) : CTR + OMAC, tag de 16 octets
// ---------------------------------------------------------------------------

fn dbl(block: &[u8; 16]) -> [u8; 16] {
    let mut out = [0u8; 16];
    let carry = block[0] >> 7;
    for i in 0..16 {
        out[i] = block[i] << 1 | if i < 15 { block[i + 1] >> 7 } else { 0 };
    }
    out[15] ^= 0x87 * carry;
    out
}

#[derive(Clone)]
pub struct Eax {
    aes: Aes,
    k1: [u8; 16],
    k2: [u8; 16],
}

impl Eax {
    pub fn new(key: &[u8]) -> Eax {
        let aes = Aes::new(key);
        let mut l = [0u8; 16];
        aes.encrypt_block(&mut l);
        let k1 = dbl(&l);
        let k2 = dbl(&k1);
        Eax { aes, k1, k2 }
    }

    /// OMAC^t(data) = CMAC([t]_16 || data).
    fn omac(&self, tag: u8, data: &[u8]) -> [u8; 16] {
        let mut state = [0u8; 16];
        state[15] = tag;
        if data.is_empty() {
            xor16(&mut state, &self.k1);
            self.aes.encrypt_block(&mut state);
            return state;
        }
        self.aes.encrypt_block(&mut state);
        let full = data.len().is_multiple_of(16);
        let blocks = data.len().div_ceil(16);
        for (n, chunk) in data.chunks(16).enumerate() {
            for (s, d) in state.iter_mut().zip(chunk) {
                *s ^= d;
            }
            if n + 1 == blocks {
                if full {
                    xor16(&mut state, &self.k1);
                } else {
                    state[chunk.len()] ^= 0x80;
                    xor16(&mut state, &self.k2);
                }
            }
            self.aes.encrypt_block(&mut state);
        }
        state
    }

    fn ctr(&self, iv: &[u8; 16], data: &mut [u8]) {
        let mut counter = *iv;
        for chunk in data.chunks_mut(16) {
            let mut ks = counter;
            self.aes.encrypt_block(&mut ks);
            for (d, k) in chunk.iter_mut().zip(ks) {
                *d ^= k;
            }
            for i in (0..16).rev() {
                counter[i] = counter[i].wrapping_add(1);
                if counter[i] != 0 {
                    break;
                }
            }
        }
    }

    /// Chiffre `data` en place et renvoie le tag.
    pub fn seal(&self, nonce: &[u8], header: &[u8], data: &mut [u8]) -> [u8; 16] {
        let n = self.omac(0, nonce);
        let h = self.omac(1, header);
        self.ctr(&n, data);
        let c = self.omac(2, data);
        let mut tag = [0u8; 16];
        for i in 0..16 {
            tag[i] = n[i] ^ h[i] ^ c[i];
        }
        tag
    }

    /// Vérifie le tag puis déchiffre `data` en place.
    pub fn open(&self, nonce: &[u8], header: &[u8], data: &mut [u8], tag: &[u8]) -> bool {
        let n = self.omac(0, nonce);
        let h = self.omac(1, header);
        let c = self.omac(2, data);
        let mut diff = 0u8;
        for i in 0..16 {
            diff |= (n[i] ^ h[i] ^ c[i]) ^ tag.get(i).copied().unwrap_or(0);
        }
        if diff != 0 || tag.len() != 16 {
            return false;
        }
        self.ctr(&n, data);
        true
    }
}

// ---------------------------------------------------------------------------
// DES (défi d'authentification VNC historique, chiffrement seul)
// ---------------------------------------------------------------------------

const DES_IP: [u8; 64] = [
    58, 50, 42, 34, 26, 18, 10, 2, 60, 52, 44, 36, 28, 20, 12, 4, 62, 54, 46, 38, 30, 22, 14, 6,
    64, 56, 48, 40, 32, 24, 16, 8, 57, 49, 41, 33, 25, 17, 9, 1, 59, 51, 43, 35, 27, 19, 11, 3, 61,
    53, 45, 37, 29, 21, 13, 5, 63, 55, 47, 39, 31, 23, 15, 7,
];
const DES_FP: [u8; 64] = [
    40, 8, 48, 16, 56, 24, 64, 32, 39, 7, 47, 15, 55, 23, 63, 31, 38, 6, 46, 14, 54, 22, 62, 30,
    37, 5, 45, 13, 53, 21, 61, 29, 36, 4, 44, 12, 52, 20, 60, 28, 35, 3, 43, 11, 51, 19, 59, 27,
    34, 2, 42, 10, 50, 18, 58, 26, 33, 1, 41, 9, 49, 17, 57, 25,
];
const DES_E: [u8; 48] = [
    32, 1, 2, 3, 4, 5, 4, 5, 6, 7, 8, 9, 8, 9, 10, 11, 12, 13, 12, 13, 14, 15, 16, 17, 16, 17, 18,
    19, 20, 21, 20, 21, 22, 23, 24, 25, 24, 25, 26, 27, 28, 29, 28, 29, 30, 31, 32, 1,
];
const DES_P: [u8; 32] = [
    16, 7, 20, 21, 29, 12, 28, 17, 1, 15, 23, 26, 5, 18, 31, 10, 2, 8, 24, 14, 32, 27, 3, 9, 19,
    13, 30, 6, 22, 11, 4, 25,
];
const DES_PC1: [u8; 56] = [
    57, 49, 41, 33, 25, 17, 9, 1, 58, 50, 42, 34, 26, 18, 10, 2, 59, 51, 43, 35, 27, 19, 11, 3, 60,
    52, 44, 36, 63, 55, 47, 39, 31, 23, 15, 7, 62, 54, 46, 38, 30, 22, 14, 6, 61, 53, 45, 37, 29,
    21, 13, 5, 28, 20, 12, 4,
];
const DES_PC2: [u8; 48] = [
    14, 17, 11, 24, 1, 5, 3, 28, 15, 6, 21, 10, 23, 19, 12, 4, 26, 8, 16, 7, 27, 20, 13, 2, 41, 52,
    31, 37, 47, 55, 30, 40, 51, 45, 33, 48, 44, 49, 39, 56, 34, 53, 46, 42, 50, 36, 29, 32,
];
const DES_SHIFTS: [u8; 16] = [1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 2, 2, 2, 1];
const DES_S: [[u8; 64]; 8] = [
    [
        14, 4, 13, 1, 2, 15, 11, 8, 3, 10, 6, 12, 5, 9, 0, 7, 0, 15, 7, 4, 14, 2, 13, 1, 10, 6, 12,
        11, 9, 5, 3, 8, 4, 1, 14, 8, 13, 6, 2, 11, 15, 12, 9, 7, 3, 10, 5, 0, 15, 12, 8, 2, 4, 9,
        1, 7, 5, 11, 3, 14, 10, 0, 6, 13,
    ],
    [
        15, 1, 8, 14, 6, 11, 3, 4, 9, 7, 2, 13, 12, 0, 5, 10, 3, 13, 4, 7, 15, 2, 8, 14, 12, 0, 1,
        10, 6, 9, 11, 5, 0, 14, 7, 11, 10, 4, 13, 1, 5, 8, 12, 6, 9, 3, 2, 15, 13, 8, 10, 1, 3, 15,
        4, 2, 11, 6, 7, 12, 0, 5, 14, 9,
    ],
    [
        10, 0, 9, 14, 6, 3, 15, 5, 1, 13, 12, 7, 11, 4, 2, 8, 13, 7, 0, 9, 3, 4, 6, 10, 2, 8, 5,
        14, 12, 11, 15, 1, 13, 6, 4, 9, 8, 15, 3, 0, 11, 1, 2, 12, 5, 10, 14, 7, 1, 10, 13, 0, 6,
        9, 8, 7, 4, 15, 14, 3, 11, 5, 2, 12,
    ],
    [
        7, 13, 14, 3, 0, 6, 9, 10, 1, 2, 8, 5, 11, 12, 4, 15, 13, 8, 11, 5, 6, 15, 0, 3, 4, 7, 2,
        12, 1, 10, 14, 9, 10, 6, 9, 0, 12, 11, 7, 13, 15, 1, 3, 14, 5, 2, 8, 4, 3, 15, 0, 6, 10, 1,
        13, 8, 9, 4, 5, 11, 12, 7, 2, 14,
    ],
    [
        2, 12, 4, 1, 7, 10, 11, 6, 8, 5, 3, 15, 13, 0, 14, 9, 14, 11, 2, 12, 4, 7, 13, 1, 5, 0, 15,
        10, 3, 9, 8, 6, 4, 2, 1, 11, 10, 13, 7, 8, 15, 9, 12, 5, 6, 3, 0, 14, 11, 8, 12, 7, 1, 14,
        2, 13, 6, 15, 0, 9, 10, 4, 5, 3,
    ],
    [
        12, 1, 10, 15, 9, 2, 6, 8, 0, 13, 3, 4, 14, 7, 5, 11, 10, 15, 4, 2, 7, 12, 9, 5, 6, 1, 13,
        14, 0, 11, 3, 8, 9, 14, 15, 5, 2, 8, 12, 3, 7, 0, 4, 10, 1, 13, 11, 6, 4, 3, 2, 12, 9, 5,
        15, 10, 11, 14, 1, 7, 6, 0, 8, 13,
    ],
    [
        4, 11, 2, 14, 15, 0, 8, 13, 3, 12, 9, 7, 5, 10, 6, 1, 13, 0, 11, 7, 4, 9, 1, 10, 14, 3, 5,
        12, 2, 15, 8, 6, 1, 4, 11, 13, 12, 3, 7, 14, 10, 15, 6, 8, 0, 5, 9, 2, 6, 11, 13, 8, 1, 4,
        10, 7, 9, 5, 0, 15, 14, 2, 3, 12,
    ],
    [
        13, 2, 8, 4, 6, 15, 11, 1, 10, 9, 3, 14, 5, 0, 12, 7, 1, 15, 13, 8, 10, 3, 7, 4, 12, 5, 6,
        11, 0, 14, 9, 2, 7, 11, 4, 1, 9, 12, 14, 2, 0, 6, 10, 13, 15, 3, 5, 8, 2, 1, 14, 7, 4, 10,
        8, 13, 15, 12, 9, 0, 3, 5, 6, 11,
    ],
];

fn permute(input: u64, in_bits: u32, table: &[u8]) -> u64 {
    let mut out = 0u64;
    for &pos in table {
        out = out << 1 | (input >> (in_bits - pos as u32)) & 1;
    }
    out
}

pub fn des_encrypt(key: &[u8; 8], block: &[u8; 8]) -> [u8; 8] {
    let k = permute(u64::from_be_bytes(*key), 64, &DES_PC1);
    let (mut c, mut d) = ((k >> 28) as u32 & 0x0fff_ffff, k as u32 & 0x0fff_ffff);
    let mut subkeys = [0u64; 16];
    for (i, sk) in subkeys.iter_mut().enumerate() {
        for _ in 0..DES_SHIFTS[i] {
            c = (c << 1 | c >> 27) & 0x0fff_ffff;
            d = (d << 1 | d >> 27) & 0x0fff_ffff;
        }
        *sk = permute((c as u64) << 28 | d as u64, 56, &DES_PC2);
    }
    let ip = permute(u64::from_be_bytes(*block), 64, &DES_IP);
    let (mut l, mut r) = ((ip >> 32) as u32, ip as u32);
    for sk in subkeys {
        let e = permute(r as u64, 32, &DES_E) ^ sk;
        let mut s_out = 0u32;
        for (i, sbox) in DES_S.iter().enumerate() {
            let six = (e >> (42 - 6 * i)) & 0x3f;
            let row = ((six >> 4) & 2) | (six & 1);
            let col = (six >> 1) & 0xf;
            s_out = s_out << 4 | sbox[(row * 16 + col) as usize] as u32;
        }
        let f = permute(s_out as u64, 32, &DES_P) as u32;
        (l, r) = (r, l ^ f);
    }
    permute((r as u64) << 32 | l as u64, 64, &DES_FP).to_be_bytes()
}

/// Réponse au défi VNC : DES avec le mot de passe (8 octets max) dont les
/// bits de chaque octet sont inversés, appliqué aux deux moitiés du défi.
pub fn vnc_response(password: &[u8], challenge: &[u8; 16]) -> [u8; 16] {
    let mut key = [0u8; 8];
    for (k, p) in key.iter_mut().zip(password.iter().take(8)) {
        *k = p.reverse_bits();
    }
    let mut out = [0u8; 16];
    for half in 0..2 {
        let block: [u8; 8] = challenge[half * 8..half * 8 + 8].try_into().unwrap();
        out[half * 8..half * 8 + 8].copy_from_slice(&des_encrypt(&key, &block));
    }
    out
}

// ---------------------------------------------------------------------------
// Grands entiers (limbes de 32 bits, petit-boutiste) et RSA
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big(Vec<u32>);

impl Big {
    pub fn from_u64(v: u64) -> Big {
        let mut b = Big(vec![v as u32, (v >> 32) as u32]);
        b.trim();
        b
    }
    pub fn from_be(bytes: &[u8]) -> Big {
        let mut limbs = vec![0u32; bytes.len().div_ceil(4)];
        for (i, &byte) in bytes.iter().rev().enumerate() {
            limbs[i / 4] |= (byte as u32) << (8 * (i % 4));
        }
        let mut b = Big(limbs);
        b.trim();
        b
    }
    /// Gros-boutiste complété à gauche sur `len` octets.
    pub fn to_be(&self, len: usize) -> Vec<u8> {
        let mut out = vec![0u8; len];
        for i in 0..len {
            let limb = self.0.get(i / 4).copied().unwrap_or(0);
            out[len - 1 - i] = (limb >> (8 * (i % 4))) as u8;
        }
        out
    }
    fn trim(&mut self) {
        while self.0.last() == Some(&0) {
            self.0.pop();
        }
    }
    pub fn bits(&self) -> usize {
        match self.0.last() {
            None => 0,
            Some(&top) => self.0.len() * 32 - top.leading_zeros() as usize,
        }
    }
    fn bit(&self, i: usize) -> bool {
        self.0.get(i / 32).is_some_and(|l| l >> (i % 32) & 1 == 1)
    }
    pub fn is_odd(&self) -> bool {
        self.0.first().is_some_and(|l| l & 1 == 1)
    }
    fn cmp_big(&self, other: &Big) -> Ordering {
        if self.0.len() != other.0.len() {
            return self.0.len().cmp(&other.0.len());
        }
        for i in (0..self.0.len()).rev() {
            match self.0[i].cmp(&other.0[i]) {
                Ordering::Equal => continue,
                o => return o,
            }
        }
        Ordering::Equal
    }
    pub fn add(&self, other: &Big) -> Big {
        let n = self.0.len().max(other.0.len());
        let mut out = Vec::with_capacity(n + 1);
        let mut carry = 0u64;
        for i in 0..n {
            let s = self.0.get(i).copied().unwrap_or(0) as u64
                + other.0.get(i).copied().unwrap_or(0) as u64
                + carry;
            out.push(s as u32);
            carry = s >> 32;
        }
        out.push(carry as u32);
        let mut b = Big(out);
        b.trim();
        b
    }
    /// self - other, avec self >= other.
    pub fn sub(&self, other: &Big) -> Big {
        let mut out = Vec::with_capacity(self.0.len());
        let mut borrow = 0i64;
        for i in 0..self.0.len() {
            let mut d = self.0[i] as i64 - other.0.get(i).copied().unwrap_or(0) as i64 - borrow;
            borrow = 0;
            if d < 0 {
                d += 1 << 32;
                borrow = 1;
            }
            out.push(d as u32);
        }
        let mut b = Big(out);
        b.trim();
        b
    }
    pub fn mul(&self, other: &Big) -> Big {
        let mut out = vec![0u32; self.0.len() + other.0.len()];
        for (i, &a) in self.0.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in other.0.iter().enumerate() {
                let t = out[i + j] as u64 + a as u64 * b as u64 + carry;
                out[i + j] = t as u32;
                carry = t >> 32;
            }
            out[i + other.0.len()] = carry as u32;
        }
        let mut b = Big(out);
        b.trim();
        b
    }
    /// Reste et quotient par un petit diviseur.
    pub fn divrem_small(&self, d: u32) -> (Big, u32) {
        let mut q = vec![0u32; self.0.len()];
        let mut rem = 0u64;
        for i in (0..self.0.len()).rev() {
            let cur = rem << 32 | self.0[i] as u64;
            q[i] = (cur / d as u64) as u32;
            rem = cur % d as u64;
        }
        let mut b = Big(q);
        b.trim();
        (b, rem as u32)
    }
    /// self mod m (division bit à bit : réservée aux calculs de préparation).
    pub fn rem(&self, m: &Big) -> Big {
        let mut r = Big(Vec::new());
        for i in (0..self.bits()).rev() {
            r = r.shl1(self.bit(i));
            if r.cmp_big(m) != Ordering::Less {
                r = r.sub(m);
            }
        }
        r
    }
    fn shl1(&self, low: bool) -> Big {
        let mut out = Vec::with_capacity(self.0.len() + 1);
        let mut carry = low as u32;
        for &l in &self.0 {
            out.push(l << 1 | carry);
            carry = l >> 31;
        }
        out.push(carry);
        let mut b = Big(out);
        b.trim();
        b
    }
    /// self^e mod m pour m impair (Montgomery).
    pub fn modpow(&self, e: &Big, m: &Big) -> Big {
        let mont = Montgomery::new(m);
        let base = mont.to_mont(&self.rem(m));
        let mut acc = mont.to_mont(&Big::from_u64(1));
        for i in (0..e.bits()).rev() {
            acc = mont.mul(&acc, &acc);
            if e.bit(i) {
                acc = mont.mul(&acc, &base);
            }
        }
        mont.leave_mont(&acc)
    }
}

struct Montgomery {
    m: Vec<u32>,
    n: usize,
    m_inv: u32,
    r2: Big,
}

impl Montgomery {
    fn new(m: &Big) -> Montgomery {
        assert!(m.is_odd(), "module de Montgomery impair");
        let n = m.0.len();
        // -m^-1 mod 2^32 par Newton.
        let m0 = m.0[0];
        let mut inv = 1u32;
        for _ in 0..5 {
            inv = inv.wrapping_mul(2u32.wrapping_sub(m0.wrapping_mul(inv)));
        }
        let mut r2 = vec![0u32; 2 * n + 1];
        r2[2 * n] = 1;
        let r2 = Big(r2).rem(m);
        Montgomery {
            m: m.0.clone(),
            n,
            m_inv: inv.wrapping_neg(),
            r2,
        }
    }
    fn mul(&self, a: &Big, b: &Big) -> Big {
        let n = self.n;
        let mut t = vec![0u32; 2 * n + 2];
        for i in 0..n {
            let ai = a.0.get(i).copied().unwrap_or(0) as u64;
            let mut carry = 0u64;
            for j in 0..n {
                let bj = b.0.get(j).copied().unwrap_or(0) as u64;
                let s = t[i + j] as u64 + ai * bj + carry;
                t[i + j] = s as u32;
                carry = s >> 32;
            }
            let mut k = i + n;
            while carry != 0 {
                let s = t[k] as u64 + carry;
                t[k] = s as u32;
                carry = s >> 32;
                k += 1;
            }
            let u = t[i].wrapping_mul(self.m_inv) as u64;
            let mut carry = 0u64;
            for j in 0..n {
                let s = t[i + j] as u64 + u * self.m[j] as u64 + carry;
                t[i + j] = s as u32;
                carry = s >> 32;
            }
            let mut k = i + n;
            while carry != 0 {
                let s = t[k] as u64 + carry;
                t[k] = s as u32;
                carry = s >> 32;
                k += 1;
            }
        }
        let mut r = Big(t[n..].to_vec());
        r.trim();
        let m = Big(self.m.clone());
        if r.cmp_big(&m) != Ordering::Less {
            r = r.sub(&m);
        }
        r
    }
    fn to_mont(&self, a: &Big) -> Big {
        self.mul(a, &self.r2)
    }
    fn leave_mont(&self, a: &Big) -> Big {
        self.mul(a, &Big::from_u64(1))
    }
}

fn xor16(state: &mut [u8; 16], key: &[u8; 16]) {
    for (s, k) in state.iter_mut().zip(key) {
        *s ^= k;
    }
}

const SMALL_PRIMES: [u32; 53] = [
    3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97,
    101, 103, 107, 109, 113, 127, 131, 137, 139, 149, 151, 157, 163, 167, 173, 179, 181, 191, 193,
    197, 199, 211, 223, 227, 229, 233, 239, 241, 251,
];

fn random_below_bits(bits: usize) -> Big {
    let mut bytes = vec![0u8; bits.div_ceil(8)];
    random_bytes(&mut bytes);
    let extra = bytes.len() * 8 - bits;
    bytes[0] &= 0xff >> extra;
    Big::from_be(&bytes)
}

fn probably_prime(n: &Big, rounds: usize) -> bool {
    for &p in &SMALL_PRIMES {
        if n.divrem_small(p).1 == 0 {
            return n == &Big::from_u64(p as u64);
        }
    }
    let one = Big::from_u64(1);
    let n1 = n.sub(&one);
    let mut s = 0;
    while !n1.bit(s) {
        s += 1;
    }
    let mut d = n1.clone();
    for _ in 0..s {
        d = d.divrem_small(2).0;
    }
    'witness: for _ in 0..rounds {
        let a = loop {
            let a = random_below_bits(n.bits() - 1);
            if a.bits() > 1 {
                break a;
            }
        };
        let mut x = a.modpow(&d, n);
        if x == one || x == n1 {
            continue;
        }
        for _ in 1..s {
            x = x.mul(&x).rem(n);
            if x == n1 {
                continue 'witness;
            }
        }
        return false;
    }
    true
}

fn random_prime(bits: usize) -> Big {
    loop {
        let mut bytes = vec![0u8; bits / 8];
        random_bytes(&mut bytes);
        bytes[0] |= 0xc0; // deux bits de poids fort : n a exactement 2*bits bits
        let last = bytes.len() - 1;
        bytes[last] |= 1;
        let candidate = Big::from_be(&bytes);
        // e = 65537 doit être inversible modulo p-1.
        if candidate.sub(&Big::from_u64(1)).divrem_small(65537).1 == 0 {
            continue;
        }
        if probably_prime(&candidate, 24) {
            return candidate;
        }
    }
}

pub struct RsaKey {
    pub n: Big,
    pub e: Big,
    d: Big,
    p: Big,
    q: Big,
    dp: Big,
    dq: Big,
    qinv: Big,
}

impl RsaKey {
    pub fn generate(bits: usize) -> RsaKey {
        loop {
            let p = random_prime(bits / 2);
            let q = random_prime(bits / 2);
            if p == q {
                continue;
            }
            if let Some(key) = RsaKey::from_primes(p, q) {
                if key.n.bits() == bits {
                    return key;
                }
            }
        }
    }

    fn from_primes(p: Big, q: Big) -> Option<RsaKey> {
        let one = Big::from_u64(1);
        let e = Big::from_u64(65537);
        let n = p.mul(&q);
        let phi = p.sub(&one).mul(&q.sub(&one));
        // d = (1 + k*phi) / e avec k = -phi^-1 mod e (e est petit).
        let r = phi.divrem_small(65537).1 as u64;
        let inv = modinv_u64(r, 65537)?;
        let k = (65537 - inv) % 65537;
        let (d, rest) = phi.mul(&Big::from_u64(k)).add(&one).divrem_small(65537);
        if rest != 0 {
            return None;
        }
        let dp = d.rem(&p.sub(&one));
        let dq = d.rem(&q.sub(&one));
        // q^-1 mod p par Fermat (p premier).
        let qinv = q.modpow(&p.sub(&Big::from_u64(2)), &p);
        Some(RsaKey {
            n,
            e,
            d,
            p,
            q,
            dp,
            dq,
            qinv,
        })
    }

    pub fn size(&self) -> usize {
        self.n.bits().div_ceil(8)
    }

    fn private(&self, c: &Big) -> Big {
        // Restes chinois : m = m2 + q * ((m1 - m2) * qinv mod p).
        let m1 = c.modpow(&self.dp, &self.p);
        let m2 = c.modpow(&self.dq, &self.q);
        let m2p = m2.rem(&self.p);
        let diff = if m1.cmp_big(&m2p) != Ordering::Less {
            m1.sub(&m2p)
        } else {
            m1.add(&self.p).sub(&m2p)
        };
        let h = diff.mul(&self.qinv).rem(&self.p);
        m2.add(&h.mul(&self.q))
    }

    /// Déchiffrement PKCS #1 v1.5 (type 2).
    pub fn decrypt(&self, cipher: &[u8]) -> Option<Vec<u8>> {
        let k = self.size();
        if cipher.len() != k {
            return None;
        }
        let c = Big::from_be(cipher);
        if c.cmp_big(&self.n) != Ordering::Less {
            return None;
        }
        let em = self.private(&c).to_be(k);
        if em[0] != 0 || em[1] != 2 {
            return None;
        }
        let sep = em[2..].iter().position(|&b| b == 0)? + 2;
        if sep < 10 {
            return None;
        }
        Some(em[sep + 1..].to_vec())
    }

    /// Sérialisation : n, e, d, p, q en hexadécimal, une valeur par ligne.
    pub fn to_text(&self) -> String {
        [&self.n, &self.e, &self.d, &self.p, &self.q]
            .iter()
            .map(|b| hex(&b.to_be(b.bits().div_ceil(8).max(1))))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    pub fn from_text(text: &str) -> Option<RsaKey> {
        let values: Vec<Big> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| unhex(l.trim()).map(|b| Big::from_be(&b)))
            .collect::<Option<_>>()?;
        if values.len() != 5 {
            return None;
        }
        let key = RsaKey::from_primes(values[3].clone(), values[4].clone())?;
        (key.n == values[0] && key.e == values[1] && key.d == values[2]).then_some(key)
    }
}

fn modinv_u64(a: u64, m: u64) -> Option<u64> {
    let (mut old_r, mut r) = (a as i64, m as i64);
    let (mut old_s, mut s) = (1i64, 0i64);
    while r != 0 {
        let q = old_r / r;
        (old_r, r) = (r, old_r - q * r);
        (old_s, s) = (s, old_s - q * s);
    }
    (old_r == 1).then(|| old_s.rem_euclid(m as i64) as u64)
}

/// Clé publique reçue d'un client RA2.
pub struct RsaPublic {
    pub n: Big,
    pub e: Big,
    pub size: usize,
}

impl RsaPublic {
    /// Chiffrement PKCS #1 v1.5 (type 2) avec bourrage aléatoire non nul.
    pub fn encrypt(&self, msg: &[u8]) -> Option<Vec<u8>> {
        let k = self.size;
        if msg.len() + 11 > k {
            return None;
        }
        let mut em = vec![0u8; k];
        em[1] = 2;
        let ps = k - msg.len() - 3;
        let mut pad = vec![0u8; ps];
        random_bytes(&mut pad);
        for b in pad.iter_mut() {
            while *b == 0 {
                let mut one = [0u8; 1];
                random_bytes(&mut one);
                *b = one[0];
            }
        }
        em[2..2 + ps].copy_from_slice(&pad);
        em[k - msg.len()..].copy_from_slice(msg);
        let m = Big::from_be(&em);
        if !self.n.is_odd() || m.cmp_big(&self.n) != Ordering::Less {
            return None;
        }
        Some(m.modpow(&self.e, &self.n).to_be(k))
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(s: &str) -> Vec<u8> {
        unhex(s).unwrap()
    }

    #[test]
    fn sha_vecteurs_fips180() {
        assert_eq!(
            hex(&sha1(&[b"abc"])),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex(&sha1(&[
                b"abcdbcdecdefdefgefghfghighijhi",
                b"jkijkljklmklmnlmnomnopnopq"
            ])),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
        assert_eq!(
            hex(&sha256(&[b"abc"])),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(&[b""])),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn aes_vecteurs_fips197() {
        let mut block: [u8; 16] = h("00112233445566778899aabbccddeeff").try_into().unwrap();
        Aes::new(&h("000102030405060708090a0b0c0d0e0f")).encrypt_block(&mut block);
        assert_eq!(hex(&block), "69c4e0d86a7b0430d8cdb78070b4c55a");
        let mut block: [u8; 16] = h("00112233445566778899aabbccddeeff").try_into().unwrap();
        Aes::new(&h(
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        ))
        .encrypt_block(&mut block);
        assert_eq!(hex(&block), "8ea2b7ca516745bfeafc49904b496089");
    }

    #[test]
    fn eax_vecteurs_officiels() {
        // Vecteurs de l'article EAX (Bellare, Rogaway, Wagner), AES-128.
        let cases = [
            (
                "233952DEE4D5ED5F9B9C6D6FF80FF478",
                "62EC67F9C3A4A407FCB2A8C49031A8B3",
                "6BFB914FD07EAE6B",
                "",
                "E037830E8389F27B025A2D6527E79D01",
            ),
            (
                "91945D3F4DCBEE0BF45EF52255F095A4",
                "BECAF043B0A23D843194BA972C66DEBD",
                "FA3BFD4806EB53FA",
                "F7FB",
                "19DD5C4C9331049D0BDAB0277408F67967E5",
            ),
            (
                "01F74AD64077F2E704C0F60ADA3DD523",
                "70C3DB4F0D26368400A10ED05D2BFF5E",
                "234A3463C1264AC6",
                "1A47CB4933",
                "D851D5BAE03A59F238A23E39199DC9266626C40F80",
            ),
        ];
        for (key, nonce, header, msg, expected) in cases {
            let eax = Eax::new(&h(&key.to_lowercase()));
            let mut data = h(&msg.to_lowercase());
            let tag = eax.seal(
                &h(&nonce.to_lowercase()),
                &h(&header.to_lowercase()),
                &mut data,
            );
            let mut out = data.clone();
            out.extend_from_slice(&tag);
            assert_eq!(hex(&out), expected.to_lowercase());
            assert!(eax.open(
                &h(&nonce.to_lowercase()),
                &h(&header.to_lowercase()),
                &mut data,
                &tag
            ));
            assert_eq!(hex(&data), msg.to_lowercase());
            let mut bad = tag;
            bad[0] ^= 1;
            let mut again = out[..out.len() - 16].to_vec();
            assert!(!eax.open(
                &h(&nonce.to_lowercase()),
                &h(&header.to_lowercase()),
                &mut again,
                &bad
            ));
        }
    }

    #[test]
    fn des_vecteur_classique_et_defi_vnc() {
        let key: [u8; 8] = h("133457799bbcdff1").try_into().unwrap();
        let block: [u8; 8] = h("0123456789abcdef").try_into().unwrap();
        assert_eq!(hex(&des_encrypt(&key, &block)), "85e813540f0ab405");
        // Réponse symétrique : deux moitiés identiques donnent deux blocs identiques.
        let r = vnc_response(b"secret", &[7u8; 16]);
        assert_eq!(r[..8], r[8..]);
    }

    #[test]
    fn grands_entiers_et_modpow() {
        let a = Big::from_be(&h("0123456789abcdef0123456789abcdef"));
        let b = Big::from_be(&h("fedcba9876543210"));
        assert_eq!(a.add(&b).sub(&b), a);
        assert_eq!(a.mul(&b).rem(&b), Big::from_u64(0));
        // 3^1000 mod (2^127 - 1) : Fermat, 2^127-1 premier => 3^(p-1) = 1.
        let p = Big::from_be(&h("7fffffffffffffffffffffffffffffff"));
        let pm1 = p.sub(&Big::from_u64(1));
        assert_eq!(Big::from_u64(3).modpow(&pm1, &p), Big::from_u64(1));
        assert!(probably_prime(&p, 8));
        assert!(!probably_prime(&p.add(&Big::from_u64(2)), 8));
        assert_eq!(
            Big::from_u64(5).modpow(&Big::from_u64(3), &Big::from_u64(13)),
            Big::from_u64(8)
        );
    }

    #[test]
    fn rsa_aller_retour_pkcs1() {
        let key = RsaKey::generate(768);
        assert_eq!(key.n.bits(), 768);
        let public = RsaPublic {
            n: key.n.clone(),
            e: key.e.clone(),
            size: key.size(),
        };
        let secret = [0x42u8; 32];
        let cipher = public.encrypt(&secret).unwrap();
        assert_eq!(key.decrypt(&cipher).unwrap(), secret);
        let mut tampered = cipher.clone();
        tampered[10] ^= 0x40;
        assert_ne!(key.decrypt(&tampered), Some(secret.to_vec()));
        let restored = RsaKey::from_text(&key.to_text()).unwrap();
        assert_eq!(restored.decrypt(&cipher).unwrap(), secret);
    }
}
