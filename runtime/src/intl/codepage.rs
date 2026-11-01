//! Pages de code Windows écrites de zéro : ANSI (1252), OEM (437, 850, 858),
//! ISO 8859-1/15, ASCII et UTF-8. Elles servent aux API `A` des .exe, aux
//! noms de fichiers des archives ZIP créées sous Windows et à la conversion
//! des fichiers texte entre Windows et Linux.

pub const CP_ACP: u32 = 0;
pub const CP_OEMCP: u32 = 1;
pub const CP_MACCP: u32 = 2;
pub const CP_THREAD_ACP: u32 = 3;
pub const CP_UTF8: u32 = 65001;

/// Windows-1252, octets 0x80 à 0x9F (les autres valent leur code Latin-1).
/// Les cinq octets non définis gardent leur valeur C1, comme sous Windows.
const CP1252_C1: [u16; 32] = [
    0x20ac, 0x0081, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160, 0x2039,
    0x0152, 0x008d, 0x017d, 0x008f, 0x0090, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014,
    0x02dc, 0x2122, 0x0161, 0x203a, 0x0153, 0x009d, 0x017e, 0x0178,
];

/// Page OEM américaine (DOS, invite de commandes, ZIP historiques).
const CP437_HIGH: [u16; 128] = [
    0x00c7, 0x00fc, 0x00e9, 0x00e2, 0x00e4, 0x00e0, 0x00e5, 0x00e7, 0x00ea, 0x00eb, 0x00e8, 0x00ef,
    0x00ee, 0x00ec, 0x00c4, 0x00c5, 0x00c9, 0x00e6, 0x00c6, 0x00f4, 0x00f6, 0x00f2, 0x00fb, 0x00f9,
    0x00ff, 0x00d6, 0x00dc, 0x00a2, 0x00a3, 0x00a5, 0x20a7, 0x0192, 0x00e1, 0x00ed, 0x00f3, 0x00fa,
    0x00f1, 0x00d1, 0x00aa, 0x00ba, 0x00bf, 0x2310, 0x00ac, 0x00bd, 0x00bc, 0x00a1, 0x00ab, 0x00bb,
    0x2591, 0x2592, 0x2593, 0x2502, 0x2524, 0x2561, 0x2562, 0x2556, 0x2555, 0x2563, 0x2551, 0x2557,
    0x255d, 0x255c, 0x255b, 0x2510, 0x2514, 0x2534, 0x252c, 0x251c, 0x2500, 0x253c, 0x255e, 0x255f,
    0x255a, 0x2554, 0x2569, 0x2566, 0x2560, 0x2550, 0x256c, 0x2567, 0x2568, 0x2564, 0x2565, 0x2559,
    0x2558, 0x2552, 0x2553, 0x256b, 0x256a, 0x2518, 0x250c, 0x2588, 0x2584, 0x258c, 0x2590, 0x2580,
    0x03b1, 0x00df, 0x0393, 0x03c0, 0x03a3, 0x03c3, 0x00b5, 0x03c4, 0x03a6, 0x0398, 0x03a9, 0x03b4,
    0x221e, 0x03c6, 0x03b5, 0x2229, 0x2261, 0x00b1, 0x2265, 0x2264, 0x2320, 0x2321, 0x00f7, 0x2248,
    0x00b0, 0x2219, 0x00b7, 0x221a, 0x207f, 0x00b2, 0x25a0, 0x00a0,
];

/// Page OEM d'Europe de l'Ouest (français, allemand, espagnol…).
const CP850_HIGH: [u16; 128] = [
    0x00c7, 0x00fc, 0x00e9, 0x00e2, 0x00e4, 0x00e0, 0x00e5, 0x00e7, 0x00ea, 0x00eb, 0x00e8, 0x00ef,
    0x00ee, 0x00ec, 0x00c4, 0x00c5, 0x00c9, 0x00e6, 0x00c6, 0x00f4, 0x00f6, 0x00f2, 0x00fb, 0x00f9,
    0x00ff, 0x00d6, 0x00dc, 0x00f8, 0x00a3, 0x00d8, 0x00d7, 0x0192, 0x00e1, 0x00ed, 0x00f3, 0x00fa,
    0x00f1, 0x00d1, 0x00aa, 0x00ba, 0x00bf, 0x00ae, 0x00ac, 0x00bd, 0x00bc, 0x00a1, 0x00ab, 0x00bb,
    0x2591, 0x2592, 0x2593, 0x2502, 0x2524, 0x00c1, 0x00c2, 0x00c0, 0x00a9, 0x2563, 0x2551, 0x2557,
    0x255d, 0x00a2, 0x00a5, 0x2510, 0x2514, 0x2534, 0x252c, 0x251c, 0x2500, 0x253c, 0x00e3, 0x00c3,
    0x255a, 0x2554, 0x2569, 0x2566, 0x2560, 0x2550, 0x256c, 0x00a4, 0x00f0, 0x00d0, 0x00ca, 0x00cb,
    0x00c8, 0x0131, 0x00cd, 0x00ce, 0x00cf, 0x2518, 0x250c, 0x2588, 0x2584, 0x00a6, 0x00cc, 0x2580,
    0x00d3, 0x00df, 0x00d4, 0x00d2, 0x00f5, 0x00d5, 0x00b5, 0x00fe, 0x00de, 0x00da, 0x00db, 0x00d9,
    0x00fd, 0x00dd, 0x00af, 0x00b4, 0x00ad, 0x00b1, 0x2017, 0x00be, 0x00b6, 0x00a7, 0x00f7, 0x00b8,
    0x00b0, 0x00a8, 0x00b7, 0x00b9, 0x00b3, 0x00b2, 0x25a0, 0x00a0,
];

/// ISO 8859-15 : huit positions changent par rapport au Latin-1 (€, Š, Œ…).
const LATIN9: [(u8, u16); 8] = [
    (0xa4, 0x20ac),
    (0xa6, 0x0160),
    (0xa8, 0x0161),
    (0xb4, 0x017d),
    (0xb8, 0x017e),
    (0xbc, 0x0152),
    (0xbd, 0x0153),
    (0xbe, 0x0178),
];

/// Pages de code à un octet connues de Winlux.
pub fn is_single_byte(cp: u32) -> bool {
    matches!(cp, 437 | 850 | 858 | 1252 | 20127 | 28591 | 28605)
}

/// Pages acceptées par IsValidCodePage.
pub fn is_valid(cp: u32) -> bool {
    is_single_byte(cp) || cp == CP_UTF8
}

/// Remplace les pseudo-pages (CP_ACP, CP_OEMCP…) par la page réelle de la
/// locale du système.
pub fn resolve(cp: u32) -> u32 {
    match cp {
        CP_ACP | CP_THREAD_ACP | CP_MACCP => crate::intl::locale::current().ansi_cp,
        CP_OEMCP => crate::intl::locale::current().oem_cp,
        other => other,
    }
}

/// Caractère d'un octet dans une page à un octet ; `None` si l'octet n'y a
/// pas de sens (ASCII strict au-delà de 0x7F).
pub fn decode_byte(cp: u32, b: u8) -> Option<char> {
    if b < 0x80 {
        return Some(b as char);
    }
    let code = match cp {
        1252 if b < 0xa0 => CP1252_C1[(b - 0x80) as usize],
        1252 | 28591 => b as u16,
        28605 => LATIN9
            .iter()
            .find(|(k, _)| *k == b)
            .map_or(b as u16, |(_, v)| *v),
        437 => CP437_HIGH[(b - 0x80) as usize],
        850 => CP850_HIGH[(b - 0x80) as usize],
        858 if b == 0xd5 => 0x20ac,
        858 => CP850_HIGH[(b - 0x80) as usize],
        _ => return None,
    };
    char::from_u32(code as u32)
}

/// Octet d'un caractère dans une page à un octet, s'il y figure.
pub fn encode_char(cp: u32, c: char) -> Option<u8> {
    let n = c as u32;
    if n < 0x80 {
        return Some(n as u8);
    }
    if cp == 20127 || n > 0xffff {
        return None;
    }
    if matches!(cp, 1252 | 28591) && (0xa0..=0xff).contains(&n) {
        return Some(n as u8);
    }
    (0x80u8..=0xff).find(|&b| decode_byte(cp, b) == Some(c))
}

/// Substitution « meilleur ajustement » de Windows : lettre accentuée
/// absente de la page → lettre de base (Ā → A), guillemets typographiques →
/// guillemets droits. Désactivée par WC_NO_BEST_FIT_CHARS.
pub fn best_fit(c: char) -> Option<char> {
    let base = match c {
        'Ā' | 'Ă' | 'Ą' => 'A',
        'ā' | 'ă' | 'ą' => 'a',
        'Ć' | 'Ĉ' | 'Ċ' | 'Č' => 'C',
        'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'Ď' | 'Đ' => 'D',
        'ď' | 'đ' => 'd',
        'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' => 'E',
        'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' => 'G',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'İ' => 'I',
        'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => 'i',
        'Ł' | 'Ĺ' | 'Ľ' => 'L',
        'ł' | 'ĺ' | 'ľ' => 'l',
        'Ń' | 'Ņ' | 'Ň' => 'N',
        'ń' | 'ņ' | 'ň' => 'n',
        'Ō' | 'Ŏ' | 'Ő' => 'O',
        'ō' | 'ŏ' | 'ő' => 'o',
        'Ŕ' | 'Ř' => 'R',
        'ŕ' | 'ř' => 'r',
        'Ś' | 'Ş' | 'Ș' => 'S',
        'ś' | 'ş' | 'ș' => 's',
        'Ţ' | 'Ť' | 'Ț' => 'T',
        'ţ' | 'ť' | 'ț' => 't',
        'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' => 'U',
        'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'Ź' | 'Ż' => 'Z',
        'ź' | 'ż' => 'z',
        '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{2032}' => '\'',
        '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{2033}' => '"',
        '\u{2013}' | '\u{2014}' | '\u{2212}' => '-',
        '\u{2026}' => '.',
        '\u{202f}' | '\u{2009}' | '\u{2007}' => '\u{a0}',
        '\u{2022}' => '\u{b7}',
        '\u{20ac}' => 'E',
        _ => return None,
    };
    Some(base)
}

/// Décode des octets dans une page de code (réelle, déjà résolue).
/// `strict` refuse les octets invalides (MB_ERR_INVALID_CHARS).
pub fn decode(cp: u32, bytes: &[u8], strict: bool) -> Result<String, ()> {
    if cp == CP_UTF8 {
        return if strict {
            String::from_utf8(bytes.to_vec()).map_err(|_| ())
        } else {
            Ok(String::from_utf8_lossy(bytes).into_owned())
        };
    }
    if !is_single_byte(cp) {
        return Err(());
    }
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        match decode_byte(cp, b) {
            Some(c) => out.push(c),
            None if strict => return Err(()),
            None => out.push('\u{fffd}'),
        }
    }
    Ok(out)
}

/// Résultat d'un encodage : octets et indicateur « caractère par défaut
/// utilisé » (lpUsedDefaultChar).
pub struct Encoded {
    pub bytes: Vec<u8>,
    pub used_default: bool,
}

/// Encode un texte dans une page de code (réelle, déjà résolue).
pub fn encode(cp: u32, text: &str, default: u8, allow_best_fit: bool) -> Result<Encoded, ()> {
    if cp == CP_UTF8 {
        return Ok(Encoded {
            bytes: text.as_bytes().to_vec(),
            used_default: false,
        });
    }
    if !is_single_byte(cp) {
        return Err(());
    }
    let mut used_default = false;
    let bytes = text
        .chars()
        .map(|c| {
            encode_char(cp, c)
                .or_else(|| {
                    allow_best_fit
                        .then(|| best_fit(c))
                        .flatten()
                        .and_then(|b| encode_char(cp, b))
                })
                .unwrap_or_else(|| {
                    used_default = true;
                    default
                })
        })
        .collect();
    Ok(Encoded {
        bytes,
        used_default,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_accents_survivent_aux_pages_ansi_et_oem() {
        let texte = "Hélène, ça coûte 5 € à Noël ; Größe, año";
        let ansi = encode(1252, texte, b'?', true).unwrap();
        assert!(!ansi.used_default);
        assert_eq!(decode(1252, &ansi.bytes, true).unwrap(), texte);
        let sans_euro = texte.replace('€', "EUR");
        let oem = encode(850, &sans_euro, b'?', true).unwrap();
        assert!(!oem.used_default);
        assert_eq!(decode(850, &oem.bytes, true).unwrap(), sans_euro);
        // 858 = 850 avec l'euro.
        assert_eq!(decode(858, &[0xd5], true).unwrap(), "€");
        assert_eq!(
            decode(850, &[0x82, 0x85, 0x87, 0xb7], true).unwrap(),
            "éàçÀ"
        );
        assert_eq!(decode(437, &[0x82, 0xb7], true).unwrap(), "é╖");
        assert_eq!(decode(1252, &[0x80, 0x9c], true).unwrap(), "€œ");
        assert_eq!(decode(28605, &[0xa4, 0xbd], true).unwrap(), "€œ");
    }

    #[test]
    fn ascii_strict_et_caractere_par_defaut() {
        assert!(decode(20127, &[0xe9], true).is_err());
        let e = encode(20127, "é", b'#', false).unwrap();
        assert_eq!((e.bytes, e.used_default), (vec![b'#'], true));
        let fit = encode(1252, "Łódź – “ok”", b'?', true).unwrap();
        assert_eq!(fit.bytes, b"L\xf3dz \x96 \x93ok\x94");
        let oem = encode(850, "– “ok”", b'?', true).unwrap();
        assert_eq!(oem.bytes, b"- \"ok\"");
        let strict = encode(1252, "Ł", b'?', false).unwrap();
        assert_eq!((strict.bytes, strict.used_default), (vec![b'?'], true));
    }

    #[test]
    fn tables_bijectives() {
        for cp in [437u32, 850, 858, 1252, 28591, 28605] {
            for b in 0x80u8..=0xff {
                if let Some(c) = decode_byte(cp, b) {
                    assert_eq!(encode_char(cp, c), Some(b), "page {cp}, octet {b:#x}");
                }
            }
        }
    }
}
