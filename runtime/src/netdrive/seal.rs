//! Chiffrement authentifié ChaCha20-Poly1305 (RFC 8439), écrit de zéro.
//!
//! Sert à ne jamais garder en clair les mots de passe des lecteurs réseau :
//! chaque secret est scellé avec une clé aléatoire propre à l'utilisateur
//! (fichier 0600 séparé de la configuration) et un nonce neuf.

fn le32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn quarter(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(7);
}

/// Un bloc de 64 octets du flux ChaCha20.
pub fn chacha20_block(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> [u8; 64] {
    let mut init = [0u32; 16];
    init[0] = 0x6170_7865;
    init[1] = 0x3320_646e;
    init[2] = 0x7962_2d32;
    init[3] = 0x6b20_6574;
    for i in 0..8 {
        init[4 + i] = le32(&key[i * 4..]);
    }
    init[12] = counter;
    for i in 0..3 {
        init[13 + i] = le32(&nonce[i * 4..]);
    }
    let mut s = init;
    for _ in 0..10 {
        quarter(&mut s, 0, 4, 8, 12);
        quarter(&mut s, 1, 5, 9, 13);
        quarter(&mut s, 2, 6, 10, 14);
        quarter(&mut s, 3, 7, 11, 15);
        quarter(&mut s, 0, 5, 10, 15);
        quarter(&mut s, 1, 6, 11, 12);
        quarter(&mut s, 2, 7, 8, 13);
        quarter(&mut s, 3, 4, 9, 14);
    }
    let mut out = [0u8; 64];
    for i in 0..16 {
        out[i * 4..i * 4 + 4].copy_from_slice(&s[i].wrapping_add(init[i]).to_le_bytes());
    }
    out
}

fn chacha20_xor(key: &[u8; 32], counter: u32, nonce: &[u8; 12], data: &mut [u8]) {
    for (i, chunk) in data.chunks_mut(64).enumerate() {
        let block = chacha20_block(key, counter.wrapping_add(i as u32), nonce);
        for (b, k) in chunk.iter_mut().zip(block.iter()) {
            *b ^= k;
        }
    }
}

/// MAC Poly1305 (arithmétique en 5 limbes de 26 bits).
pub fn poly1305(key: &[u8; 32], msg: &[u8]) -> [u8; 16] {
    const M: u32 = 0x3ff_ffff;
    let r0 = le32(&key[0..]) & 0x3ff_ffff;
    let r1 = (le32(&key[3..]) >> 2) & 0x3ff_ff03;
    let r2 = (le32(&key[6..]) >> 4) & 0x3ff_c0ff;
    let r3 = (le32(&key[9..]) >> 6) & 0x3f0_3fff;
    let r4 = (le32(&key[12..]) >> 8) & 0x00f_ffff;
    let (s1, s2, s3, s4) = (r1 * 5, r2 * 5, r3 * 5, r4 * 5);
    let (mut h0, mut h1, mut h2, mut h3, mut h4) = (0u32, 0u32, 0u32, 0u32, 0u32);
    for chunk in msg.chunks(16) {
        let mut block = [0u8; 17];
        block[..chunk.len()].copy_from_slice(chunk);
        let hibit = if chunk.len() == 16 {
            1 << 24
        } else {
            block[chunk.len()] = 1;
            0
        };
        h0 += le32(&block[0..]) & M;
        h1 += (le32(&block[3..]) >> 2) & M;
        h2 += (le32(&block[6..]) >> 4) & M;
        h3 += (le32(&block[9..]) >> 6) & M;
        h4 += (le32(&block[12..]) >> 8) | hibit;
        let m = |a: u32, b: u32| a as u64 * b as u64;
        let d0 = m(h0, r0) + m(h1, s4) + m(h2, s3) + m(h3, s2) + m(h4, s1);
        let mut d1 = m(h0, r1) + m(h1, r0) + m(h2, s4) + m(h3, s3) + m(h4, s2);
        let mut d2 = m(h0, r2) + m(h1, r1) + m(h2, r0) + m(h3, s4) + m(h4, s3);
        let mut d3 = m(h0, r3) + m(h1, r2) + m(h2, r1) + m(h3, r0) + m(h4, s4);
        let mut d4 = m(h0, r4) + m(h1, r3) + m(h2, r2) + m(h3, r1) + m(h4, r0);
        h0 = d0 as u32 & M;
        d1 += d0 >> 26;
        h1 = d1 as u32 & M;
        d2 += d1 >> 26;
        h2 = d2 as u32 & M;
        d3 += d2 >> 26;
        h3 = d3 as u32 & M;
        d4 += d3 >> 26;
        h4 = d4 as u32 & M;
        h0 += (d4 >> 26) as u32 * 5;
        h1 += h0 >> 26;
        h0 &= M;
    }
    // Réduction complète modulo 2^130 - 5.
    let mut c = h1 >> 26;
    h1 &= M;
    h2 += c;
    c = h2 >> 26;
    h2 &= M;
    h3 += c;
    c = h3 >> 26;
    h3 &= M;
    h4 += c;
    c = h4 >> 26;
    h4 &= M;
    h0 += c * 5;
    c = h0 >> 26;
    h0 &= M;
    h1 += c;
    let mut g0 = h0.wrapping_add(5);
    c = g0 >> 26;
    g0 &= M;
    let mut g1 = h1.wrapping_add(c);
    c = g1 >> 26;
    g1 &= M;
    let mut g2 = h2.wrapping_add(c);
    c = g2 >> 26;
    g2 &= M;
    let mut g3 = h3.wrapping_add(c);
    c = g3 >> 26;
    g3 &= M;
    let g4 = h4.wrapping_add(c).wrapping_sub(1 << 26);
    // g4 négatif (bit de poids fort) : h < p, on garde h.
    let keep_g = (g4 >> 31).wrapping_sub(1);
    let pick = |h: u32, g: u32| (h & !keep_g) | (g & keep_g);
    let (h0, h1, h2, h3, h4) = (
        pick(h0, g0),
        pick(h1, g1),
        pick(h2, g2),
        pick(h3, g3),
        pick(h4, g4),
    );
    let w0 = h0 | (h1 << 26);
    let w1 = (h1 >> 6) | (h2 << 20);
    let w2 = (h2 >> 12) | (h3 << 14);
    let w3 = (h3 >> 18) | (h4 << 8);
    let mut out = [0u8; 16];
    let mut f = 0u64;
    for (i, w) in [w0, w1, w2, w3].into_iter().enumerate() {
        f = w as u64 + le32(&key[16 + i * 4..]) as u64 + (f >> 32);
        out[i * 4..i * 4 + 4].copy_from_slice(&(f as u32).to_le_bytes());
    }
    out
}

fn aead_tag(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], ct: &[u8]) -> [u8; 16] {
    let block = chacha20_block(key, 0, nonce);
    let mut otk = [0u8; 32];
    otk.copy_from_slice(&block[..32]);
    let mut mac = Vec::with_capacity(aad.len() + ct.len() + 48);
    for part in [aad, ct] {
        mac.extend_from_slice(part);
        mac.resize(mac.len().div_ceil(16) * 16, 0);
    }
    mac.extend_from_slice(&(aad.len() as u64).to_le_bytes());
    mac.extend_from_slice(&(ct.len() as u64).to_le_bytes());
    poly1305(&otk, &mac)
}

/// Chiffre `plain` : renvoie le chiffré suivi de l'étiquette de 16 octets.
pub fn seal(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], plain: &[u8]) -> Vec<u8> {
    let mut out = plain.to_vec();
    chacha20_xor(key, 1, nonce, &mut out);
    let tag = aead_tag(key, nonce, aad, &out);
    out.extend_from_slice(&tag);
    out
}

/// Déchiffre et vérifie ; `None` si la donnée a été altérée ou la clé est fausse.
pub fn open(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() < 16 {
        return None;
    }
    let (ct, tag) = sealed.split_at(sealed.len() - 16);
    let expected = aead_tag(key, nonce, aad, ct);
    // Comparaison en temps constant.
    if expected
        .iter()
        .zip(tag)
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        != 0
    {
        return None;
    }
    let mut out = ct.to_vec();
    chacha20_xor(key, 1, nonce, &mut out);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        let s: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn poly1305_rfc8439_2_5_2() {
        let key: [u8; 32] = hex("85d6be7857556d337f4452fe42d506a80103808afb0db2fd4abff6af4149f51b")
            .try_into()
            .unwrap();
        let tag = poly1305(&key, b"Cryptographic Forum Research Group");
        assert_eq!(tag.to_vec(), hex("a8061dc1305136c6c22b8baf0c0127a9"));
    }

    #[test]
    fn chacha20_block_rfc8439_2_3_2() {
        let key: [u8; 32] = (0u8..32).collect::<Vec<_>>().try_into().unwrap();
        let nonce: [u8; 12] = hex("000000090000004a00000000").try_into().unwrap();
        let block = chacha20_block(&key, 1, &nonce);
        assert_eq!(
            block[..16].to_vec(),
            hex("10f1e7e4d13b5915500fdd1fa32071c4")
        );
    }

    #[test]
    fn aead_rfc8439_2_8_2() {
        let key: [u8; 32] = (0x80u8..0xa0).collect::<Vec<_>>().try_into().unwrap();
        let nonce: [u8; 12] = hex("070000004041424344454647").try_into().unwrap();
        let aad = hex("50515253c0c1c2c3c4c5c6c7");
        let plain = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
        let sealed = seal(&key, &nonce, &aad, plain);
        assert_eq!(
            sealed[..16].to_vec(),
            hex("d31a8d34648e60db7b86afbc53ef7ec2")
        );
        assert_eq!(
            sealed[sealed.len() - 16..].to_vec(),
            hex("1ae10b594f09e26a7e902ecbd0600691")
        );
        assert_eq!(open(&key, &nonce, &aad, &sealed).unwrap(), plain.to_vec());
        let mut bad = sealed.clone();
        bad[3] ^= 1;
        assert!(open(&key, &nonce, &aad, &bad).is_none());
        assert!(open(&key, &nonce, b"autre", &sealed).is_none());
    }
}
