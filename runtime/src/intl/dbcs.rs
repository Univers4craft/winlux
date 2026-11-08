//! Pages de code asiatiques à deux octets (#183) : 932 (japonais), 936
//! (chinois simplifié), 949 (coréen) et 950 (chinois traditionnel).
//!
//! Les correspondances viennent des convertisseurs `iconv` de la glibc du
//! système (modules gconv CP932, CP936, CP949 et CP950 du paquet libc6),
//! pas de tables recopiées : Winlux ne décide que du découpage en octets de
//! tête, documenté publiquement pour chaque page. Si la glibc n'a pas le
//! module, la page est déclarée invalide (IsValidCodePage renvoie FALSE)
//! plutôt que de produire du texte faux.

use std::collections::HashMap;
use std::ffi::CString;
use std::sync::Mutex;

/// Pages à deux octets connues de Winlux.
pub fn is_dbcs(cp: u32) -> bool {
    matches!(cp, 932 | 936 | 949 | 950)
}

fn iconv_name(cp: u32) -> &'static str {
    match cp {
        932 => "CP932",
        936 => "CP936",
        949 => "CP949",
        _ => "CP950",
    }
}

/// Plages d'octets de tête (CPINFO.LeadByte), par paires bornes incluses.
pub fn lead_ranges(cp: u32) -> &'static [(u8, u8)] {
    match cp {
        932 => &[(0x81, 0x9f), (0xe0, 0xfc)],
        936 | 949 | 950 => &[(0x81, 0xfe)],
        _ => &[],
    }
}

/// IsDBCSLeadByteEx.
pub fn is_lead_byte(cp: u32, b: u8) -> bool {
    lead_ranges(cp)
        .iter()
        .any(|&(lo, hi)| (lo..=hi).contains(&b))
}

/// Caractère Unicode par défaut de la page (CPINFOEX.UnicodeDefaultChar) :
/// le point médian katakana pour le japonais, « ? » ailleurs.
pub fn unicode_default(cp: u32) -> u16 {
    if cp == 932 {
        0x30fb
    } else {
        0x3f
    }
}

struct Handle(libc::iconv_t);
// Chaque descripteur n'est utilisé que sous le verrou de CONVERTERS.
unsafe impl Send for Handle {}

static CONVERTERS: Mutex<Option<HashMap<(u32, bool), Handle>>> = Mutex::new(None);

/// Convertit `input` avec le descripteur (page, sens) ; `None` si la
/// séquence n'existe pas dans la page ou si le module gconv manque.
fn convert(cp: u32, to_unicode: bool, input: &[u8]) -> Option<Vec<u8>> {
    let mut guard = CONVERTERS.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    let handle = match map.get(&(cp, to_unicode)) {
        Some(h) => h.0,
        None => {
            let page = CString::new(iconv_name(cp)).ok()?;
            let utf32 = CString::new("UTF-32LE").ok()?;
            let (to, from) = if to_unicode {
                (&utf32, &page)
            } else {
                (&page, &utf32)
            };
            let cd = unsafe { libc::iconv_open(to.as_ptr(), from.as_ptr()) };
            if cd as isize == -1 {
                return None;
            }
            map.insert((cp, to_unicode), Handle(cd));
            cd
        }
    };
    let mut src = input.to_vec();
    let mut out = vec![0u8; input.len() * 4 + 8];
    let mut in_ptr = src.as_mut_ptr() as *mut libc::c_char;
    let mut in_left = src.len();
    let mut out_ptr = out.as_mut_ptr() as *mut libc::c_char;
    let mut out_left = out.len();
    unsafe {
        // Remet l'état à zéro (une conversion ratée peut en laisser un).
        libc::iconv(
            handle,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        let r = libc::iconv(
            handle,
            &mut in_ptr,
            &mut in_left,
            &mut out_ptr,
            &mut out_left,
        );
        if r == usize::MAX || in_left != 0 {
            return None;
        }
    }
    out.truncate(out.len() - out_left);
    Some(out)
}

/// La glibc sait-elle convertir cette page ? (IsValidCodePage)
pub fn available(cp: u32) -> bool {
    is_dbcs(cp) && convert(cp, true, b"A").is_some()
}

/// Caractère d'une séquence d'un ou deux octets.
pub fn decode_seq(cp: u32, seq: &[u8]) -> Option<char> {
    if seq.len() == 1 && seq[0] < 0x80 {
        return Some(seq[0] as char);
    }
    let out = convert(cp, true, seq)?;
    if out.len() != 4 {
        return None;
    }
    char::from_u32(u32::from_le_bytes([out[0], out[1], out[2], out[3]]))
}

/// Octets d'un caractère, s'il figure dans la page.
pub fn encode_char(cp: u32, c: char) -> Option<Vec<u8>> {
    if (c as u32) < 0x80 {
        return Some(vec![c as u8]);
    }
    convert(cp, false, &(c as u32).to_le_bytes()).filter(|b| !b.is_empty() && b.len() <= 2)
}

/// Décode des octets : un octet de tête suivi d'un octet forme un caractère,
/// les autres octets sont seuls. `strict` refuse toute séquence invalide
/// (MB_ERR_INVALID_CHARS), sinon elle devient le caractère par défaut.
pub fn decode(cp: u32, bytes: &[u8], strict: bool) -> Result<String, ()> {
    let fallback = char::from_u32(unicode_default(cp) as u32).unwrap_or('?');
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < 0x80 {
            out.push(b as char);
            i += 1;
            continue;
        }
        let len = if is_lead_byte(cp, b) && i + 1 < bytes.len() {
            2
        } else {
            1
        };
        match decode_seq(cp, &bytes[i..i + len]) {
            Some(c) => out.push(c),
            None if strict => return Err(()),
            None => out.push(fallback),
        }
        i += len;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(cp: u32, text: &str, bytes: &[u8]) {
        if !available(cp) {
            eprintln!("page {cp} : module gconv absent, test sauté");
            return;
        }
        assert_eq!(decode(cp, bytes, true).unwrap(), text, "page {cp}");
        let enc: Vec<u8> = text
            .chars()
            .flat_map(|c| encode_char(cp, c).unwrap())
            .collect();
        assert_eq!(enc, bytes, "page {cp}");
    }

    #[test]
    fn japonais_chinois_coreen() {
        roundtrip(
            932,
            "日本語 OK",
            &[0x93, 0xfa, 0x96, 0x7b, 0x8c, 0xea, b' ', b'O', b'K'],
        );
        // Katakana demi-chasse : un seul octet, pas un octet de tête.
        roundtrip(932, "ｱｲ", &[0xb1, 0xb2]);
        roundtrip(936, "中文", &[0xd6, 0xd0, 0xce, 0xc4]);
        roundtrip(949, "한국어", &[0xc7, 0xd1, 0xb1, 0xb9, 0xbe, 0xee]);
        roundtrip(950, "中文", &[0xa4, 0xa4, 0xa4, 0xe5]);
    }

    #[test]
    fn octets_de_tete_et_sequences_invalides() {
        assert!(is_lead_byte(932, 0x81) && is_lead_byte(932, 0xe0));
        assert!(!is_lead_byte(932, 0xb1) && !is_lead_byte(932, 0x41));
        assert!(is_lead_byte(936, 0xfe) && !is_lead_byte(936, 0x80));
        if !available(932) {
            return;
        }
        // Octet de tête orphelin en fin de texte.
        assert!(decode(932, &[b'a', 0x93], true).is_err());
        assert_eq!(decode(932, &[b'a', 0x93], false).unwrap(), "a\u{30fb}");
        assert_eq!(encode_char(932, 'é'), None);
    }
}
