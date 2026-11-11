//! FoldStringW (#1585) : repli de chiffres, de la zone de compatibilité,
//! des ligatures, composition et décomposition.
//!
//! Source : documentation Microsoft Learn de FoldStringW et des drapeaux
//! MAP_* (winnls.h). Winlux n'embarque pas les tables de normalisation
//! Unicode : la composition et la décomposition couvrent les lettres
//! latines accentuées de Latin-1 (accents grave, aigu, circonflexe, tilde,
//! tréma, rond en chef et cédille), les autres caractères passent tels quels.

const MAP_FOLDCZONE: u32 = 0x10;
const MAP_PRECOMPOSED: u32 = 0x20;
const MAP_COMPOSITE: u32 = 0x40;
const MAP_FOLDDIGITS: u32 = 0x80;
const MAP_EXPAND_LIGATURES: u32 = 0x2000;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_INVALID_FLAGS: u32 = 1004;
const ERROR_INVALID_PARAMETER: u32 = 87;

/// Premiers chiffres (valeur 0) des suites de chiffres décimaux Unicode
/// du plan de base.
const DIGIT_ZEROS: &[u32] = &[
    0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66, 0x0CE6, 0x0D66,
    0x0DE6, 0x0E50, 0x0ED0, 0x0F20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x2080, 0xFF10,
];

fn fold_digit(c: u32) -> Option<u32> {
    match c {
        0xB2 => return Some(0x32),
        0xB3 => return Some(0x33),
        0xB9 => return Some(0x31),
        0x2070 => return Some(0x30),
        0x2074..=0x2079 => return Some(0x30 + c - 0x2070),
        _ => {}
    }
    DIGIT_ZEROS
        .iter()
        .find(|&&z| (z..z + 10).contains(&c))
        .map(|&z| 0x30 + c - z)
}

/// Zone de compatibilité : formes pleine chasse de l'ASCII et espace
/// idéographique.
fn fold_czone(c: u32) -> u32 {
    match c {
        0xFF01..=0xFF5E => c - 0xFF01 + 0x21,
        0x3000 => 0x20,
        _ => c,
    }
}

fn ligature(c: u32) -> Option<&'static str> {
    Some(match c {
        0xC6 => "AE",
        0xE6 => "ae",
        0x152 => "OE",
        0x153 => "oe",
        0x132 => "IJ",
        0x133 => "ij",
        0xFB00 => "ff",
        0xFB01 => "fi",
        0xFB02 => "fl",
        0xFB03 => "ffi",
        0xFB04 => "ffl",
        0xFB05 | 0xFB06 => "st",
        _ => return None,
    })
}

/// (lettre composée, base, accent combinant), majuscules de Latin-1 ; les
/// minuscules sont à +0x20 sur la lettre et la base.
const LATIN1: &[(u32, u32, u32)] = &[
    (0xC0, 0x41, 0x300),
    (0xC1, 0x41, 0x301),
    (0xC2, 0x41, 0x302),
    (0xC3, 0x41, 0x303),
    (0xC4, 0x41, 0x308),
    (0xC5, 0x41, 0x30A),
    (0xC7, 0x43, 0x327),
    (0xC8, 0x45, 0x300),
    (0xC9, 0x45, 0x301),
    (0xCA, 0x45, 0x302),
    (0xCB, 0x45, 0x308),
    (0xCC, 0x49, 0x300),
    (0xCD, 0x49, 0x301),
    (0xCE, 0x49, 0x302),
    (0xCF, 0x49, 0x308),
    (0xD1, 0x4E, 0x303),
    (0xD2, 0x4F, 0x300),
    (0xD3, 0x4F, 0x301),
    (0xD4, 0x4F, 0x302),
    (0xD5, 0x4F, 0x303),
    (0xD6, 0x4F, 0x308),
    (0xD9, 0x55, 0x300),
    (0xDA, 0x55, 0x301),
    (0xDB, 0x55, 0x302),
    (0xDC, 0x55, 0x308),
    (0xDD, 0x59, 0x301),
];

fn decompose(c: u32) -> Option<(u32, u32)> {
    if c == 0xFF {
        return Some((0x79, 0x308));
    }
    LATIN1.iter().find_map(|&(up, base, mark)| {
        if c == up {
            Some((base, mark))
        } else if c == up + 0x20 {
            Some((base + 0x20, mark))
        } else {
            None
        }
    })
}

fn compose(base: u32, mark: u32) -> Option<u32> {
    if (base, mark) == (0x79, 0x308) {
        return Some(0xFF);
    }
    LATIN1.iter().find_map(|&(up, b, m)| {
        if m != mark {
            None
        } else if b == base {
            Some(up)
        } else if b + 0x20 == base {
            Some(up + 0x20)
        } else {
            None
        }
    })
}

/// Repli d'une chaîne UTF-16 selon les drapeaux MAP_*.
pub(crate) fn fold(flags: u32, src: &[u16]) -> Vec<u16> {
    let mut chars: Vec<u32> = Vec::with_capacity(src.len());
    for c in char::decode_utf16(src.iter().copied()) {
        let mut c = c.map(u32::from).unwrap_or(0xFFFD);
        if flags & MAP_FOLDCZONE != 0 {
            c = fold_czone(c);
        }
        if flags & MAP_FOLDDIGITS != 0 {
            c = fold_digit(c).unwrap_or(c);
        }
        if flags & MAP_EXPAND_LIGATURES != 0 {
            if let Some(s) = ligature(c) {
                chars.extend(s.chars().map(u32::from));
                continue;
            }
        }
        if flags & MAP_COMPOSITE != 0 {
            if let Some((b, m)) = decompose(c) {
                chars.push(b);
                chars.push(m);
                continue;
            }
        }
        if flags & MAP_PRECOMPOSED != 0 {
            if let Some(&prev) = chars.last() {
                if let Some(p) = compose(prev, c) {
                    *chars.last_mut().unwrap() = p;
                    continue;
                }
            }
        }
        chars.push(c);
    }
    let mut out = Vec::with_capacity(chars.len());
    for c in chars {
        let mut buf = [0u16; 2];
        let ch = char::from_u32(c).unwrap_or('\u{FFFD}');
        out.extend_from_slice(ch.encode_utf16(&mut buf));
    }
    out
}

pub unsafe extern "win64" fn fold_string_w(
    flags: u32,
    src: *const u16,
    src_len: i32,
    dst: *mut u16,
    dst_len: i32,
) -> i32 {
    let known =
        MAP_FOLDCZONE | MAP_PRECOMPOSED | MAP_COMPOSITE | MAP_FOLDDIGITS | MAP_EXPAND_LIGATURES;
    if flags == 0
        || flags & !known != 0
        || flags & (MAP_PRECOMPOSED | MAP_COMPOSITE) == (MAP_PRECOMPOSED | MAP_COMPOSITE)
        || (flags & MAP_EXPAND_LIGATURES != 0 && flags & (MAP_PRECOMPOSED | MAP_COMPOSITE) != 0)
    {
        crate::process::set_last_error(ERROR_INVALID_FLAGS);
        return 0;
    }
    if src.is_null()
        || src_len == 0
        || src_len < -1
        || dst_len < 0
        || (dst_len > 0 && dst.is_null())
        || std::ptr::eq(src, dst)
    {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    // -1 : jusqu'au zéro final, compris.
    let n = if src_len == -1 {
        (0..).take_while(|&i| *src.add(i) != 0).count() + 1
    } else {
        src_len as usize
    };
    let out = fold(flags, std::slice::from_raw_parts(src, n));
    if dst_len == 0 {
        return out.len() as i32;
    }
    if out.len() > dst_len as usize {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(out.as_ptr(), dst, out.len());
    out.len() as i32
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "FoldStringW" => fn fold_string_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn repli_de_chaines() {
        assert_eq!(fold(MAP_FOLDDIGITS, &w("٣²₇５")), w("3275"));
        assert_eq!(fold(MAP_FOLDCZONE, &w("ＡＢＣ　ｚ")), w("ABC z"));
        assert_eq!(
            fold(MAP_EXPAND_LIGATURES, &w("ﬁnal œuvre Æ")),
            w("final oeuvre AE")
        );
        assert_eq!(
            fold(MAP_COMPOSITE, &w("Été ÿ")),
            w("E\u{301}te\u{301} y\u{308}")
        );
        assert_eq!(
            fold(MAP_PRECOMPOSED, &w("E\u{301}te\u{301} c\u{327}a")),
            w("Été ça")
        );
        let _teb = crate::process::test_teb();
        unsafe {
            let src = w("12");
            let mut dst = [0u16; 1];
            assert_eq!(
                fold_string_w(MAP_FOLDDIGITS, src.as_ptr(), 2, std::ptr::null_mut(), 0),
                2
            );
            assert_eq!(
                fold_string_w(MAP_FOLDDIGITS, src.as_ptr(), 2, dst.as_mut_ptr(), 1),
                0
            );
            assert_eq!(crate::process::last_error(), ERROR_INSUFFICIENT_BUFFER);
            assert_eq!(
                fold_string_w(
                    MAP_PRECOMPOSED | MAP_COMPOSITE,
                    src.as_ptr(),
                    2,
                    dst.as_mut_ptr(),
                    1
                ),
                0
            );
            assert_eq!(crate::process::last_error(), ERROR_INVALID_FLAGS);
            let z: Vec<u16> = w("٣").into_iter().chain([0]).collect();
            let mut out = [0u16; 4];
            assert_eq!(
                fold_string_w(MAP_FOLDDIGITS, z.as_ptr(), -1, out.as_mut_ptr(), 4),
                2
            );
            assert_eq!(&out[..2], &[0x33, 0]);
        }
    }
}
